//! GSMTC worker. No Tauri dependency and no WinRT waits on the window thread.
use isle_core::{Candidate, MediaSnapshot, Selection};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::{
    core::*,
    Foundation::{AsyncStatus, EventRegistrationToken, IAsyncOperation, TypedEventHandler},
    Media::Control::*,
    Win32::{Foundation::*, System::WinRT::*, UI::WindowsAndMessaging::*},
};
pub const UPDATED: u32 = WM_APP + 70;
#[derive(Clone, Copy)]
pub enum Action {
    Toggle,
    Previous,
    Next,
}
enum Command {
    Wake,
    Control(u64, Action, Instant),
}
#[derive(Default)]
pub struct Update {
    pub snapshot: MediaSnapshot,
    pub error: Option<String>,
}
struct Shared {
    polls: AtomicU64,
    updates: AtomicU64,
    manager_alive: AtomicBool,
    active: AtomicBool,
    stopping: AtomicBool,
    pending: AtomicBool,
    update: Mutex<Update>,
}
pub struct MediaService {
    shared: Arc<Shared>,
    sender: SyncSender<Command>,
    worker: Option<JoinHandle<()>>,
}
impl MediaService {
    pub fn new(hwnd: HWND, start: Instant) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            polls: AtomicU64::new(0),
            updates: AtomicU64::new(0),
            manager_alive: AtomicBool::new(false),
            active: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            pending: AtomicBool::new(false),
            update: Mutex::new(Update::default()),
        });
        let (sender, receiver) = mpsc::sync_channel(8);
        let state = shared.clone();
        let hwnd = hwnd.0;
        let worker = thread::Builder::new()
            .name("isle-media".into())
            .spawn(move || {
                unsafe {
                    if let Err(e) = RoInitialize(RO_INIT_MULTITHREADED) {
                        publish(
                            &state,
                            HWND(hwnd),
                            MediaSnapshot::default(),
                            Some(e.to_string()),
                        );
                        return;
                    }
                }
                run(&state, receiver, HWND(hwnd), start);
                unsafe {
                    RoUninitialize();
                }
            })?;
        Ok(Self {
            shared,
            sender,
            worker: Some(worker),
        })
    }
    pub fn set_active(&self, active: bool) {
        if self.shared.active.swap(active, Ordering::AcqRel) != active {
            let _ = self.sender.try_send(Command::Wake);
        }
    }
    pub fn diagnostics(&self) -> String {
        format!(
            "\"mediaActive\":{},\"mediaManagerAlive\":{},\"mediaPolls\":{},\"mediaUpdates\":{}",
            self.shared.active.load(Ordering::Acquire),
            self.shared.manager_alive.load(Ordering::Acquire),
            self.shared.polls.load(Ordering::Relaxed),
            self.shared.updates.load(Ordering::Relaxed)
        )
    }
    pub fn control(&self, session: u64, action: Action) {
        let _ = self
            .sender
            .try_send(Command::Control(session, action, Instant::now()));
    }
    pub fn take(&self) -> Update {
        // Clear while holding the slot so a concurrent publish cannot lose its notification.
        let state = self.shared.update.lock().unwrap_or_else(|e| e.into_inner());
        self.shared.pending.store(false, Ordering::Release);
        Update {
            snapshot: state.snapshot.clone(),
            error: state.error.clone(),
        }
    }
}
impl Drop for MediaService {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::Release);
        let _ = self.sender.try_send(Command::Wake);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn publish(shared: &Shared, hwnd: HWND, snapshot: MediaSnapshot, error: Option<String>) {
    let mut slot = shared.update.lock().unwrap_or_else(|e| e.into_inner());
    if slot.snapshot == snapshot && slot.error == error {
        return;
    }
    *slot = Update { snapshot, error };
    shared.updates.fetch_add(1, Ordering::Relaxed);
    if !shared.pending.swap(true, Ordering::AcqRel) {
        unsafe {
            let _ = PostMessageW(hwnd, UPDATED, WPARAM(0), LPARAM(0));
        }
    }
}
fn wait<T: RuntimeType>(operation: IAsyncOperation<T>, shared: &Shared) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if shared.stopping.load(Ordering::Acquire)
            || !shared.active.load(Ordering::Acquire)
            || Instant::now() >= deadline
        {
            let _ = operation.Cancel();
            return Err(Error::from(E_ABORT));
        }
        if operation.Status()? != AsyncStatus::Started {
            return operation.GetResults();
        }
        thread::sleep(Duration::from_millis(10));
    }
}
struct Session {
    value: GlobalSystemMediaTransportControlsSession,
    token: EventRegistrationToken,
    dirty: Arc<AtomicBool>,
    snapshot: MediaSnapshot,
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.value.RemoveMediaPropertiesChanged(self.token);
    }
}
fn poll(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
    current: &mut Option<Session>,
    sequence: &mut u64,
    shared: &Shared,
    start: Instant,
) -> Result<MediaSnapshot> {
    let sessions = manager.GetSessions()?;
    let mut candidates = Vec::new();
    for index in 0..sessions.Size()?.min(64) {
        if let Ok(session) = sessions.GetAt(index) {
            let Ok(id) = session.SourceAppUserModelId() else {
                continue;
            };
            let Ok(status) = session.GetPlaybackInfo().and_then(|p| p.PlaybackStatus()) else {
                continue;
            };
            let id = id.to_string();
            let playing =
                status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
            let updated = session
                .GetTimelineProperties()
                .and_then(|t| t.LastUpdatedTime())
                .map(|t| t.UniversalTime)
                .unwrap_or(0);
            candidates.push((session, id, playing, updated));
        }
    }
    let selection = Selection::default();
    let previous_source = shared
        .update
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .snapshot
        .source
        .clone();
    let index = selection.choose(
        &candidates
            .iter()
            .map(|(_, id, playing, updated)| Candidate {
                id,
                playing: *playing,
                updated: *updated,
            })
            .collect::<Vec<_>>(),
        current
            .as_ref()
            .map(|s| s.snapshot.source.as_str())
            .or(Some(previous_source.as_str())),
    );
    let Some(index) = index else {
        *current = None;
        return Ok(MediaSnapshot::default());
    };
    let (selected, id, _, _) = &candidates[index];
    if current.as_ref().is_none_or(|s| s.value != *selected) {
        *current = None;
        *sequence += 1;
        let dirty = Arc::new(AtomicBool::new(true));
        let flag = dirty.clone();
        let token = selected.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
            flag.store(true, Ordering::Release);
            Ok(())
        }))?;
        // Keep the last monotonic anchor across page release. Metadata below
        // resets it if the track changed while no page consumed media.
        let previous = shared
            .update
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot
            .clone();
        let previous = if previous.source == *id {
            previous
        } else {
            MediaSnapshot::default()
        };
        *current = Some(Session {
            value: selected.clone(),
            token,
            dirty,
            snapshot: MediaSnapshot {
                session: *sequence,
                source: id.clone(),
                ..previous
            },
        });
    }
    let current = current.as_mut().unwrap();
    let mut next = current.snapshot.clone();
    if current.dirty.swap(false, Ordering::AcqRel) {
        let info = match wait(selected.TryGetMediaPropertiesAsync()?, shared) {
            Ok(info) => info,
            Err(e) => {
                current.dirty.store(true, Ordering::Release);
                return Err(e);
            }
        };
        next.title = info.Title()?.to_string().chars().take(2048).collect();
        next.artist = info.Artist()?.to_string().chars().take(1024).collect();
    }
    let playback = selected.GetPlaybackInfo()?;
    next.playing = playback.PlaybackStatus()?
        == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
    let controls = playback.Controls()?;
    next.previous = controls.IsPreviousEnabled()?;
    next.play_pause = controls.IsPlayPauseToggleEnabled()?;
    next.next = controls.IsNextEnabled()?;
    let timeline = selected.GetTimelineProperties().ok().and_then(|t| {
        Some((
            (t.Position().ok()?.Duration / 10000).max(0) as u64,
            (t.EndTime().ok()?.Duration / 10000).max(0) as u64,
        ))
    });
    let new_track = next.title != current.snapshot.title || next.artist != current.snapshot.artist;
    next.timeline.accept(
        timeline,
        start.elapsed().as_secs_f64(),
        current.snapshot.playing,
        next.playing,
        new_track,
    );
    current.snapshot = next.clone();
    Ok(next)
}
fn run(shared: &Shared, receiver: Receiver<Command>, hwnd: HWND, start: Instant) {
    let mut manager = None;
    let mut current: Option<Session> = None;
    let mut sequence = 0;
    let mut next_poll = Instant::now();
    while !shared.stopping.load(Ordering::Acquire) {
        let active = shared.active.load(Ordering::Acquire);
        if !active {
            current = None;
            manager = None;
            shared.manager_alive.store(false, Ordering::Release);
        }
        let command = if active {
            receiver
                .recv_timeout(next_poll.saturating_duration_since(Instant::now()))
                .ok()
        } else {
            receiver.recv().ok()
        };
        if shared.stopping.load(Ordering::Acquire) {
            break;
        }
        if !shared.active.load(Ordering::Acquire) {
            continue;
        }
        let mut error = None;
        if let Some(Command::Control(target, action, when)) = command {
            if let Some(s) = current
                .as_ref()
                .filter(|s| s.snapshot.session == target && when.elapsed() < Duration::from_secs(2))
            {
                let op = match action {
                    Action::Toggle if s.snapshot.play_pause => s.value.TryTogglePlayPauseAsync(),
                    Action::Previous if s.snapshot.previous => s.value.TrySkipPreviousAsync(),
                    Action::Next if s.snapshot.next => s.value.TrySkipNextAsync(),
                    _ => {
                        continue;
                    }
                };
                if let Err(e) = op.and_then(|op| wait(op, shared)).and_then(|accepted| {
                    if accepted {
                        Ok(())
                    } else {
                        Err(Error::from(E_FAIL))
                    }
                }) {
                    error = Some(e.to_string());
                }
            }
        }
        if manager.is_none() {
            match GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
                .and_then(|op| wait(op, shared))
            {
                Ok(m) => {
                    manager = Some(m);
                    shared.manager_alive.store(true, Ordering::Release);
                }
                Err(e) => error = Some(e.to_string()),
            }
        }
        if let Some(m) = &manager {
            shared.polls.fetch_add(1, Ordering::Relaxed);
            match poll(m, &mut current, &mut sequence, shared, start) {
                Ok(snapshot) if shared.active.load(Ordering::Acquire) => {
                    publish(shared, hwnd, snapshot, error)
                }
                Ok(_) => {}
                Err(e) => {
                    // Stale controls must never target a disappearing/replaced session.
                    current = None;
                    manager = None;
                    shared.manager_alive.store(false, Ordering::Release);
                    if shared.active.load(Ordering::Acquire) {
                        publish(shared, hwnd, MediaSnapshot::default(), Some(e.to_string()));
                    }
                }
            }
        } else if shared.active.load(Ordering::Acquire) {
            publish(shared, hwnd, MediaSnapshot::default(), error);
        }
        next_poll = Instant::now() + Duration::from_secs(1);
    }
    drop(current);
    drop(manager);
}
