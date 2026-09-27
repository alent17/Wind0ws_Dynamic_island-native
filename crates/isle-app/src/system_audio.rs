//! Endpoint handles live only on a dedicated COM thread while the volume page is visible.
use isle_core::{AudioDevice, AudioSnapshot};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::{
    core::*,
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::*,
        Media::Audio::{Endpoints::*, *},
        System::Com::{StructuredStorage::*, *},
        UI::WindowsAndMessaging::*,
    },
};
pub const UPDATED: u32 = WM_APP + 71;
pub enum Action {
    Volume(u8),
    Mute(bool),
    Device(String),
}
struct Command {
    target: String,
    action: Action,
    created: Instant,
}
#[derive(Default)]
struct Slot {
    active: bool,
    quit: bool,
    commands: VecDeque<Command>,
    state: AudioSnapshot,
}
impl Slot {
    fn enqueue(&mut self, command: Command) {
        if !self.active {
            return;
        }
        if matches!(command.action, Action::Volume(_))
            && self.commands.back().is_some_and(|old| {
                old.target == command.target && matches!(old.action, Action::Volume(_))
            })
        {
            self.commands.pop_back();
        }
        if self.commands.len() < 8 {
            self.commands.push_back(command);
        }
    }
}
struct Shared {
    slot: Mutex<Slot>,
    ready: Condvar,
    pending: AtomicBool,
    pub alive: AtomicBool,
    pub polls: AtomicU64,
    pub writes: AtomicU64,
}
pub struct AudioService {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl AudioService {
    pub fn new(hwnd: HWND) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            slot: Mutex::default(),
            ready: Condvar::new(),
            pending: AtomicBool::new(false),
            alive: AtomicBool::new(false),
            polls: AtomicU64::new(0),
            writes: AtomicU64::new(0),
        });
        let state = shared.clone();
        let hwnd = hwnd.0;
        let worker = thread::Builder::new()
            .name("isle-system-audio".into())
            .spawn(move || unsafe {
                if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                    state
                        .slot
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .state
                        .failed = true;
                    state.pending.store(true, Ordering::Release);
                    let _ = PostMessageW(HWND(hwnd), UPDATED, WPARAM(0), LPARAM(0));
                    return;
                }
                run(&state, HWND(hwnd));
                CoUninitialize();
            })?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }
    pub fn active(&self, active: bool) {
        let mut slot = self.shared.slot.lock().unwrap_or_else(|e| e.into_inner());
        if slot.active != active {
            slot.active = active;
            if !active {
                slot.commands.clear();
            }
            self.shared.ready.notify_one();
        }
    }
    pub fn command(&self, target: String, action: Action) {
        let mut slot = self.shared.slot.lock().unwrap_or_else(|e| e.into_inner());
        if slot.active {
            slot.enqueue(Command {
                target,
                action,
                created: Instant::now(),
            });
            self.shared.ready.notify_one();
        }
    }
    pub fn take(&self) -> AudioSnapshot {
        let slot = self.shared.slot.lock().unwrap_or_else(|e| e.into_inner());
        self.shared.pending.store(false, Ordering::Release);
        slot.state.clone()
    }
    pub fn diagnostics(&self) -> String {
        format!(
            "\"audioEndpointAlive\":{},\"audioPolls\":{},\"audioWrites\":{}",
            self.shared.alive.load(Ordering::Acquire),
            self.shared.polls.load(Ordering::Relaxed),
            self.shared.writes.load(Ordering::Relaxed)
        )
    }
}
impl Drop for AudioService {
    fn drop(&mut self) {
        self.shared
            .slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .quit = true;
        self.shared.ready.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
unsafe fn id(device: &IMMDevice) -> Result<String> {
    let raw = device.GetId()?;
    let value = raw.to_string();
    CoTaskMemFree(Some(raw.0.cast()));
    Ok(value?)
}
unsafe fn name(device: &IMMDevice) -> String {
    let read = || -> Result<String> {
        let store = device.OpenPropertyStore(STGM_READ)?;
        let mut value = store.GetValue(&PKEY_Device_FriendlyName)?;
        let mut text = [0u16; 512];
        let result = PropVariantToString(&value, &mut text);
        let _ = PropVariantClear(&mut value);
        result?;
        Ok(String::from_utf16_lossy(
            &text[..text.iter().position(|v| *v == 0).unwrap_or(512)],
        ))
    };
    read().unwrap_or_else(|_| "音频输出设备".into())
}
unsafe fn snapshot(
    enumerator: &IMMDeviceEnumerator,
) -> Result<(AudioSnapshot, IAudioEndpointVolume)> {
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
    let endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
    let selected = AudioDevice {
        id: id(&device)?,
        name: name(&device),
    };
    let collection = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
    let mut devices = Vec::new();
    for index in 0..collection.GetCount()?.min(128) {
        if let Ok(device) = collection.Item(index) {
            if let Ok(id) = id(&device) {
                devices.push(AudioDevice {
                    id,
                    name: name(&device),
                });
            }
        }
    }
    devices.sort_by_key(|d| (d.id != selected.id, d.name.to_lowercase()));
    Ok((
        AudioSnapshot {
            device: selected,
            devices,
            volume: (endpoint.GetMasterVolumeLevelScalar()?.clamp(0., 1.) * 100.).round() as u8,
            muted: endpoint.GetMute()?.as_bool(),
            failed: false,
        },
        endpoint,
    ))
}
unsafe fn run(shared: &Shared, hwnd: HWND) {
    let mut enumerator: Option<IMMDeviceEnumerator> = None;
    let mut endpoint: Option<IAudioEndpointVolume> = None;
    let mut next = Instant::now();
    loop {
        let mut slot = shared.slot.lock().unwrap_or_else(|e| e.into_inner());
        if !slot.active {
            endpoint = None;
            enumerator = None;
            shared.alive.store(false, Ordering::Release);
        }
        while !slot.quit && (!slot.active || (slot.commands.is_empty() && Instant::now() < next)) {
            slot = if slot.active {
                shared
                    .ready
                    .wait_timeout(slot, next.saturating_duration_since(Instant::now()))
                    .unwrap_or_else(|e| e.into_inner())
                    .0
            } else {
                shared.ready.wait(slot).unwrap_or_else(|e| e.into_inner())
            };
            if !slot.active {
                endpoint = None;
                enumerator = None;
                shared.alive.store(false, Ordering::Release);
            }
        }
        if slot.quit {
            break;
        }
        let command = slot.commands.pop_front();
        drop(slot);
        if enumerator.is_none() {
            enumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok();
        }
        let result = (|| -> Result<AudioSnapshot> {
            let enumerator = enumerator.as_ref().ok_or_else(|| Error::from(E_FAIL))?;
            let (mut current, active_endpoint) = snapshot(enumerator)?;
            endpoint = Some(active_endpoint);
            shared.alive.store(true, Ordering::Release);
            if let Some(command) = command.filter(|c| c.created.elapsed() < Duration::from_secs(2))
            {
                if !shared.slot.lock().unwrap_or_else(|e| e.into_inner()).active {
                    return Ok(current);
                }
                if current.device.id != command.target {
                    return Err(Error::from(E_ABORT));
                }
                let endpoint = endpoint.as_ref().unwrap();
                let result = match command.action {
                    Action::Volume(value) => endpoint
                        .SetMasterVolumeLevelScalar(value.min(100) as f32 / 100., std::ptr::null())
                        .and_then(|_| {
                            if value > 0 {
                                endpoint.SetMute(false, std::ptr::null())
                            } else {
                                Ok(())
                            }
                        }),
                    Action::Mute(value) => endpoint.SetMute(value, std::ptr::null()),
                    Action::Device(id) => {
                        if !current.devices.iter().any(|d| d.id == id) {
                            Err(Error::from(E_INVALIDARG))
                        } else {
                            set_default(&id)
                        }
                    }
                };
                if result.is_ok() {
                    shared.writes.fetch_add(1, Ordering::Relaxed);
                }
                current = snapshot(enumerator)?.0;
                current.failed = result.is_err();
            }
            Ok(current)
        })();
        shared.polls.fetch_add(1, Ordering::Relaxed);
        let current = result.unwrap_or_else(|_| {
            endpoint = None;
            shared.alive.store(false, Ordering::Release);
            AudioSnapshot {
                failed: true,
                ..AudioSnapshot::default()
            }
        });
        let mut slot = shared.slot.lock().unwrap_or_else(|e| e.into_inner());
        if slot.active && slot.state != current {
            slot.state = current;
            if !shared.pending.swap(true, Ordering::AcqRel) {
                let _ = PostMessageW(hwnd, UPDATED, WPARAM(0), LPARAM(0));
            }
        }
        // Limit coalesced drag commands; keep only the latest queued value.
        if !slot.commands.is_empty() {
            next = Instant::now() + Duration::from_millis(20);
        } else {
            next = Instant::now() + Duration::from_secs(1);
        }
        drop(slot);
        thread::sleep(Duration::from_millis(20));
    }
    drop(endpoint);
    drop(enumerator);
}
#[repr(transparent)]
#[derive(Clone, PartialEq, Eq)]
struct IPolicyConfig(IUnknown);

unsafe impl Interface for IPolicyConfig {
    type Vtable = IPolicyConfig_Vtbl;
}

unsafe impl ComInterface for IPolicyConfig {
    const IID: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
}

#[repr(C)]
#[allow(non_snake_case)]
struct IPolicyConfig_Vtbl {
    base__: IUnknown_Vtbl,
    GetMixFormat: usize,
    GetDeviceFormat: usize,
    ResetDeviceFormat: usize,
    SetDeviceFormat: usize,
    GetProcessingPeriod: usize,
    SetProcessingPeriod: usize,
    GetShareMode: usize,
    SetShareMode: usize,
    GetPropertyValue: usize,
    SetPropertyValue: usize,
    SetDefaultEndpoint:
        unsafe extern "system" fn(*mut core::ffi::c_void, PCWSTR, i32) -> windows::core::HRESULT,
    SetEndpointVisibility: usize,
}

fn set_default(id: &str) -> Result<()> {
    let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
    unsafe {
        const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
        let policy: IPolicyConfig = CoCreateInstance(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)?;
        let set_default = Interface::vtable(&policy).SetDefaultEndpoint;
        for role in [eConsole, eMultimedia, eCommunications] {
            set_default(Interface::as_raw(&policy), PCWSTR(wide.as_ptr()), role.0).ok()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drag_coalesces_without_discarding_device_or_mute_commands() {
        let mut slot = Slot {
            active: true,
            ..Default::default()
        };
        let cmd = |action| Command {
            target: "device".into(),
            action,
            created: Instant::now(),
        };
        for volume in 0..100 {
            slot.enqueue(cmd(Action::Volume(volume)));
        }
        assert_eq!(slot.commands.len(), 1);
        assert!(matches!(slot.commands[0].action, Action::Volume(99)));
        slot.enqueue(cmd(Action::Device("second".into())));
        slot.enqueue(cmd(Action::Mute(true)));
        slot.enqueue(cmd(Action::Volume(20)));
        slot.enqueue(cmd(Action::Volume(30)));
        assert_eq!(slot.commands.len(), 4);
        assert!(matches!(slot.commands[1].action, Action::Device(_)));
        assert!(matches!(slot.commands[2].action, Action::Mute(true)));
        for _ in 0..20 {
            slot.enqueue(cmd(Action::Mute(false)));
        }
        assert_eq!(slot.commands.len(), 8);
        slot.active = false;
        slot.commands.clear();
        slot.enqueue(cmd(Action::Volume(90)));
        assert!(slot.commands.is_empty());
    }
}
