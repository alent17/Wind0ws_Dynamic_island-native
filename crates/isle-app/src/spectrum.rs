//! Event-driven default-output WASAPI loopback. Never opens an input/microphone.
use isle_core::spectrum::{Analyzer, BANDS};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Media::Audio::*,
        System::{Com::*, Threading::*},
        UI::WindowsAndMessaging::*,
    },
};
pub const UPDATED: u32 = WM_APP + 72;
struct Signal(isize);
impl Signal {
    unsafe fn new() -> Result<Self> {
        Ok(Self(CreateEventW(None, false, false, None)?.0))
    }
    fn handle(&self) -> HANDLE {
        HANDLE(self.0)
    }
    fn wake(&self) {
        unsafe {
            let _ = SetEvent(self.handle());
        }
    }
}
impl Drop for Signal {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle());
        }
    }
}
#[derive(Clone, Copy, Default)]
pub struct Frame {
    pub bars: [f32; BANDS],
    pub failed: bool,
    epoch: u64,
}
struct Shared {
    signal: Signal,
    active: AtomicBool,
    quit: AtomicBool,
    epoch: AtomicU64,
    frame: Mutex<Frame>,
    pending: AtomicBool,
    alive: AtomicBool,
    packets: AtomicU64,
    analyses: AtomicU64,
    restarts: AtomicU64,
}
pub struct SpectrumService {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl SpectrumService {
    pub unsafe fn new(hwnd: HWND) -> Result<Self> {
        let shared = Arc::new(Shared {
            signal: Signal::new()?,
            active: AtomicBool::new(false),
            quit: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
            frame: Mutex::default(),
            pending: AtomicBool::new(false),
            alive: AtomicBool::new(false),
            packets: AtomicU64::new(0),
            analyses: AtomicU64::new(0),
            restarts: AtomicU64::new(0),
        });
        let state = shared.clone();
        let hwnd = hwnd.0;
        let worker = thread::Builder::new()
            .name("isle-spectrum".into())
            .spawn(move || {
                if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                    while !state.quit.load(Ordering::Acquire) {
                        publish(
                            &state,
                            HWND(hwnd),
                            state.epoch.load(Ordering::Acquire),
                            [0.; BANDS],
                            true,
                        );
                        WaitForSingleObject(state.signal.handle(), INFINITE);
                    }
                    return;
                }
                run(&state, HWND(hwnd));
                CoUninitialize();
            })
            .map_err(|_| Error::from(E_FAIL))?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }
    pub fn active(&self, active: bool) {
        if self.shared.active.swap(active, Ordering::AcqRel) != active {
            self.shared.epoch.fetch_add(1, Ordering::AcqRel);
            self.shared.signal.wake();
        }
    }
    pub fn take(&self) -> Frame {
        let frame = self.shared.frame.lock().unwrap_or_else(|e| e.into_inner());
        self.shared.pending.store(false, Ordering::Release);
        if frame.epoch == self.shared.epoch.load(Ordering::Acquire) {
            *frame
        } else {
            Frame::default()
        }
    }
    pub fn diagnostics(&self) -> String {
        format!("\"spectrumActive\":{},\"spectrumCaptureAlive\":{},\"spectrumPackets\":{},\"spectrumAnalyses\":{},\"spectrumStarts\":{}",self.shared.active.load(Ordering::Acquire),self.shared.alive.load(Ordering::Acquire),self.shared.packets.load(Ordering::Relaxed),self.shared.analyses.load(Ordering::Relaxed),self.shared.restarts.load(Ordering::Relaxed))
    }
}
impl Drop for SpectrumService {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Release);
        self.shared.signal.wake();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
unsafe fn publish(shared: &Shared, hwnd: HWND, epoch: u64, bars: [f32; BANDS], failed: bool) {
    if !shared.active.load(Ordering::Acquire) || shared.epoch.load(Ordering::Acquire) != epoch {
        return;
    }
    let mut frame = shared.frame.lock().unwrap_or_else(|e| e.into_inner());
    // Subpixel noise does not keep the UI message queue or renderer busy.
    let bars = bars.map(|v| (v.clamp(0., 1.) * 256.).round() / 256.);
    if frame.epoch == epoch && frame.bars == bars && frame.failed == failed {
        return;
    }
    *frame = Frame {
        bars,
        failed,
        epoch,
    };
    if !shared.pending.swap(true, Ordering::AcqRel) {
        let _ = PostMessageW(hwnd, UPDATED, WPARAM(0), LPARAM(0));
    }
}
#[derive(Clone, Copy)]
struct Format {
    channels: usize,
    bytes: usize,
    stride: usize,
    float: bool,
    rate: u32,
}
impl Format {
    unsafe fn read(raw: *const WAVEFORMATEX) -> Result<Self> {
        let f = raw.read_unaligned();
        let tag = if f.wFormatTag == 0xfffe && f.cbSize >= 22 {
            let ext = raw.cast::<WAVEFORMATEXTENSIBLE>().read_unaligned();
            let sub = ext.SubFormat;
            if sub == GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71) {
                3
            } else if sub == GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71) {
                1
            } else {
                0
            }
        } else {
            f.wFormatTag
        };
        let valid = (tag == 3 && f.wBitsPerSample == 32)
            || (tag == 1 && matches!(f.wBitsPerSample, 16 | 24 | 32));
        let channels = f.nChannels as usize;
        let bytes = f.wBitsPerSample as usize / 8;
        let stride = f.nBlockAlign as usize;
        let rate = f.nSamplesPerSec;
        if !valid
            || !(1..=32).contains(&channels)
            || !(8000..=384000).contains(&rate)
            || stride != channels * bytes
        {
            return Err(Error::from(E_INVALIDARG));
        }
        Ok(Self {
            channels,
            bytes,
            stride,
            float: tag == 3,
            rate: f.nSamplesPerSec,
        })
    }
    fn mono(self, frame: &[u8]) -> f32 {
        let sum = frame
            .chunks_exact(self.bytes)
            .take(self.channels)
            .map(|v| {
                let sample = if self.float {
                    f32::from_le_bytes(v.try_into().unwrap())
                } else {
                    match self.bytes {
                        2 => i16::from_le_bytes(v.try_into().unwrap()) as f32 / 32768.,
                        3 => {
                            ((i32::from_le_bytes([v[0], v[1], v[2], 0]) << 8) >> 8) as f32
                                / 8388608.
                        }
                        _ => i32::from_le_bytes(v.try_into().unwrap()) as f32 / 2147483648.,
                    }
                };
                if sample.is_finite() {
                    sample.clamp(-1., 1.)
                } else {
                    0.
                }
            })
            .sum::<f32>();
        sum / self.channels as f32
    }
}
unsafe fn device_id(device: &IMMDevice) -> Result<String> {
    let raw = device.GetId()?;
    let id = raw.to_string();
    CoTaskMemFree(Some(raw.0.cast()));
    Ok(id?)
}
struct Capture {
    capture: IAudioCaptureClient,
    client: IAudioClient,
    event: Signal,
    format: Format,
    analyzer: Analyzer,
    id: String,
    last_packet: Instant,
}
impl Capture {
    unsafe fn new(enumerator: &IMMDeviceEnumerator) -> Result<Self> {
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        let id = device_id(&device)?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let raw = client.GetMixFormat()?;
        let format = Format::read(raw);
        let initialized = if format.is_ok() {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                0,
                0,
                raw,
                None,
            )
        } else {
            Err(Error::from(E_INVALIDARG))
        };
        CoTaskMemFree(Some(raw.cast()));
        let format = format?;
        initialized?;
        let event = Signal::new()?;
        client.SetEventHandle(event.handle())?;
        let capture = client.GetService::<IAudioCaptureClient>()?;
        let mut value = Self {
            capture,
            client,
            event,
            format,
            analyzer: Analyzer::new(format.rate),
            id,
            last_packet: Instant::now(),
        };
        value.analyzer.clear();
        value.client.Start()?;
        Ok(value)
    }
    unsafe fn drain(&mut self, shared: &Shared, epoch: u64) -> Result<()> {
        // Bounded drain lets visibility/quit win even with an overloaded endpoint.
        for _ in 0..128 {
            if !shared.active.load(Ordering::Acquire)
                || shared.quit.load(Ordering::Acquire)
                || shared.epoch.load(Ordering::Acquire) != epoch
                || self.capture.GetNextPacketSize()? == 0
            {
                break;
            }
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0, 0);
            self.capture
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            let result = (|| -> Result<()> {
                if frames > self.format.rate {
                    return Err(Error::from(E_INVALIDARG));
                }
                if flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                    self.analyzer.clear();
                }
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                    for _ in 0..frames {
                        self.analyzer.push(0.);
                    }
                } else if frames > 0 {
                    if data.is_null() {
                        return Err(Error::from(E_POINTER));
                    }
                    let bytes =
                        std::slice::from_raw_parts(data, frames as usize * self.format.stride);
                    for frame in bytes.chunks_exact(self.format.stride) {
                        self.analyzer.push(self.format.mono(frame));
                    }
                }
                Ok(())
            })();
            let release = self.capture.ReleaseBuffer(frames);
            result?;
            release?;
            self.last_packet = Instant::now();
            shared.packets.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}
unsafe fn run(shared: &Shared, hwnd: HWND) {
    while !shared.quit.load(Ordering::Acquire) {
        if !shared.active.load(Ordering::Acquire) {
            WaitForSingleObject(shared.signal.handle(), INFINITE);
            continue;
        }
        let epoch = shared.epoch.load(Ordering::Acquire);
        let result = (|| -> Result<()> {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let mut capture = Capture::new(&enumerator)?;
            shared.restarts.fetch_add(1, Ordering::Relaxed);
            shared.alive.store(true, Ordering::Release);
            let mut analysis = Instant::now();
            let mut check = Instant::now();
            while shared.active.load(Ordering::Acquire)
                && !shared.quit.load(Ordering::Acquire)
                && shared.epoch.load(Ordering::Acquire) == epoch
            {
                let wait = WaitForMultipleObjects(
                    &[shared.signal.handle(), capture.event.handle()],
                    false,
                    100,
                );
                if wait == WAIT_FAILED {
                    return Err(Error::from_win32());
                }
                if !shared.active.load(Ordering::Acquire)
                    || shared.quit.load(Ordering::Acquire)
                    || shared.epoch.load(Ordering::Acquire) != epoch
                {
                    break;
                }
                capture.drain(shared, epoch)?;
                if analysis.elapsed() >= Duration::from_millis(50) {
                    let seconds = analysis.elapsed().as_secs_f32();
                    analysis = Instant::now();
                    let bars = if capture.last_packet.elapsed() > Duration::from_millis(200) {
                        capture.analyzer.clear();
                        [0.; BANDS]
                    } else {
                        shared.analyses.fetch_add(1, Ordering::Relaxed);
                        capture.analyzer.analyze(seconds)
                    };
                    publish(shared, hwnd, epoch, bars, false);
                }
                if check.elapsed() >= Duration::from_secs(1) {
                    check = Instant::now();
                    let current = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
                    if device_id(&current)? != capture.id {
                        break;
                    }
                }
            }
            Ok(())
        })();
        shared.alive.store(false, Ordering::Release);
        if shared.active.load(Ordering::Acquire) && shared.epoch.load(Ordering::Acquire) == epoch {
            publish(shared, hwnd, epoch, [0.; BANDS], result.is_err());
            if result.is_err() {
                WaitForSingleObject(shared.signal.handle(), 1000);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pcm_and_float_decode_with_no_channel_or_sign_confusion() {
        for (bytes, data) in [
            (2, vec![0, 64, 0, 192]),
            (3, vec![0, 0, 64, 0, 0, 192]),
            (4, vec![0, 0, 0, 64, 0, 0, 0, 192]),
        ] {
            let format = Format {
                channels: 2,
                bytes,
                stride: 2 * bytes,
                float: false,
                rate: 48000,
            };
            assert_eq!(format.mono(&data), 0.);
            assert!(
                (Format {
                    channels: 1,
                    ..format
                }
                .mono(&data[..bytes])
                    - 0.5)
                    .abs()
                    < 0.00001
            );
        }
        let format = Format {
            channels: 1,
            bytes: 4,
            stride: 4,
            float: true,
            rate: 48000,
        };
        assert_eq!(format.mono(&0.25f32.to_le_bytes()), 0.25);
        assert_eq!(format.mono(&f32::NAN.to_le_bytes()), 0.);
    }
    #[test]
    fn unsupported_or_malformed_formats_are_rejected() {
        let mut f = WAVEFORMATEX {
            wFormatTag: 3,
            nChannels: 2,
            nSamplesPerSec: 48000,
            wBitsPerSample: 32,
            nBlockAlign: 8,
            ..Default::default()
        };
        unsafe {
            assert!(Format::read(&f).is_ok());
            f.nBlockAlign = 2;
            assert!(Format::read(&f).is_err());
            f.nBlockAlign = 8;
            f.wFormatTag = 0xfffe;
            assert!(Format::read(&f).is_err());
        }
    }
}
