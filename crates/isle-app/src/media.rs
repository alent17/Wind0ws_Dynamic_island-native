//! GSMTC worker. No Tauri dependency and no WinRT waits on the window thread.
use isle_core::{player::PlayerKind, Candidate, MediaSnapshot, Selection};
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
    Media::{Control::*, MediaPlaybackAutoRepeatMode},
    Win32::{Foundation::*, System::WinRT::*, UI::WindowsAndMessaging::*},
};
pub const UPDATED: u32 = WM_APP + 70;
#[derive(Clone, Copy)]
pub enum Action {
    Toggle,
    Previous,
    Next,
    Seek(u64),
    Shuffle(bool),
    Repeat(u8),
}

/// Standard playback controls are always gated by the capabilities reported
/// by the active GSMTC session. Player-specific extensions do not intercept
/// play/pause, previous, next, or seek.
fn gsmtc_action_enabled(snapshot: &MediaSnapshot, action: Action) -> bool {
    match action {
        Action::Toggle => snapshot.play_pause,
        Action::Previous => snapshot.previous,
        Action::Next => snapshot.next,
        Action::Seek(_) => snapshot.seek && snapshot.timeline.duration_ms > 0,
        Action::Shuffle(_) => snapshot.shuffle_enabled,
        Action::Repeat(mode) => snapshot.repeat_enabled && mode <= 2,
    }
}

type ControllerResult = std::result::Result<(), String>;

trait GsmTcController {
    fn toggle(&self) -> ControllerResult;
    fn previous(&self) -> ControllerResult;
    fn next(&self) -> ControllerResult;
    fn seek(&self, position_ms: u64) -> ControllerResult;
}

fn dispatch_gsmtc_action(
    snapshot: &MediaSnapshot,
    action: Action,
    controller: &impl GsmTcController,
) -> Option<ControllerResult> {
    if !gsmtc_action_enabled(snapshot, action) {
        return None;
    }
    Some(match action {
        Action::Toggle => controller.toggle(),
        Action::Previous => controller.previous(),
        Action::Next => controller.next(),
        Action::Seek(position) => controller.seek(position.min(snapshot.timeline.duration_ms)),
        Action::Shuffle(_) | Action::Repeat(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netease_standard_transport_controls_remain_gsmtc_capability_gated() {
        let netease = PlayerKind::from_session_identity("CloudMusic.exe");
        assert!(netease.uses_netease_extension());
        let snapshot = MediaSnapshot {
            source: "CloudMusic.exe".into(),
            play_pause: true,
            previous: false,
            next: true,
            seek: true,
            timeline: isle_core::Timeline {
                duration_ms: 180_000,
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(gsmtc_action_enabled(&snapshot, Action::Toggle));
        assert!(!gsmtc_action_enabled(&snapshot, Action::Previous));
        assert!(gsmtc_action_enabled(&snapshot, Action::Next));
        assert!(gsmtc_action_enabled(&snapshot, Action::Seek(60_000)));
        assert!(gsmtc_action_enabled(&snapshot, Action::Seek(0)));
        let empty_timeline = MediaSnapshot {
            timeline: Default::default(),
            ..snapshot.clone()
        };
        assert!(!gsmtc_action_enabled(&empty_timeline, Action::Seek(0)));

        let unsupported = MediaSnapshot {
            play_pause: false,
            next: false,
            ..snapshot
        };
        assert!(!gsmtc_action_enabled(&unsupported, Action::Toggle));
        assert!(!gsmtc_action_enabled(&unsupported, Action::Next));
    }

    #[derive(Default)]
    struct RecordingController(std::sync::Mutex<Vec<&'static str>>);

    impl GsmTcController for RecordingController {
        fn toggle(&self) -> ControllerResult {
            self.0.lock().unwrap().push("toggle");
            Ok(())
        }

        fn previous(&self) -> ControllerResult {
            self.0.lock().unwrap().push("previous");
            Ok(())
        }

        fn next(&self) -> ControllerResult {
            self.0.lock().unwrap().push("next");
            Ok(())
        }

        fn seek(&self, _position_ms: u64) -> ControllerResult {
            self.0.lock().unwrap().push("seek");
            Ok(())
        }
    }

    #[test]
    fn netease_next_and_previous_dispatch_only_to_the_gsmtc_controller() {
        let snapshot = MediaSnapshot {
            source: "CloudMusic.exe".into(),
            previous: true,
            next: true,
            ..Default::default()
        };
        let controller = RecordingController::default();

        assert!(dispatch_gsmtc_action(&snapshot, Action::Next, &controller)
            .unwrap()
            .is_ok());
        assert!(
            dispatch_gsmtc_action(&snapshot, Action::Previous, &controller)
                .unwrap()
                .is_ok()
        );
        assert_eq!(*controller.0.lock().unwrap(), ["next", "previous"]);

        let unsupported = MediaSnapshot {
            next: false,
            ..snapshot
        };
        assert!(dispatch_gsmtc_action(&unsupported, Action::Next, &controller).is_none());
        assert_eq!(*controller.0.lock().unwrap(), ["next", "previous"]);
    }
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
    artwork: Arc<crate::artwork::Statistics>,
    selection: Mutex<Selection>,
    revision: AtomicU64,
    polls: AtomicU64,
    updates: AtomicU64,
    manager_alive: AtomicBool,
    active: AtomicBool,
    display_artwork: AtomicBool,
    stopping: AtomicBool,
    pending: AtomicBool,
    update: Mutex<Update>,
    seek: Mutex<Option<(u64, u64, Instant)>>,
}
pub struct MediaService {
    shared: Arc<Shared>,
    sender: SyncSender<Command>,
    worker: Option<JoinHandle<()>>,
}
impl MediaService {
    pub fn new(hwnd: HWND, start: Instant, selection: Selection) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            artwork: Arc::default(),
            selection: Mutex::new(selection),
            revision: AtomicU64::new(0),
            polls: AtomicU64::new(0),
            updates: AtomicU64::new(0),
            manager_alive: AtomicBool::new(false),
            active: AtomicBool::new(false),
            display_artwork: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            pending: AtomicBool::new(false),
            update: Mutex::new(Update::default()),
            seek: Mutex::new(None),
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
    pub fn set_selection(&self, selection: Selection) {
        self.shared
            .seek
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        *self
            .shared
            .selection
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = selection;
        self.shared.revision.fetch_add(1, Ordering::AcqRel);
        *self.shared.update.lock().unwrap_or_else(|e| e.into_inner()) = Update::default();
        let _ = self.sender.try_send(Command::Wake);
    }
    pub fn set_active(&self, active: bool) {
        if self.shared.active.swap(active, Ordering::AcqRel) != active {
            let _ = self.sender.try_send(Command::Wake);
        }
    }
    pub fn set_artwork_display(&self, display: bool) {
        if self.shared.display_artwork.swap(display, Ordering::AcqRel) != display {
            let _ = self.sender.try_send(Command::Wake);
        }
    }
    pub fn diagnostics(&self) -> String {
        format!(
            "\"mediaActive\":{},\"mediaManagerAlive\":{},\"mediaPolls\":{},\"mediaUpdates\":{},\"artworkBusy\":{},\"artworkJobs\":{},\"artworkHttpRequests\":{},\"artworkCacheEntries\":{},\"artworkDisplayCacheEntries\":{}",
            self.shared.active.load(Ordering::Acquire),
            self.shared.manager_alive.load(Ordering::Acquire),
            self.shared.polls.load(Ordering::Relaxed),
            self.shared.updates.load(Ordering::Relaxed),
            self.shared.artwork.busy.load(Ordering::Acquire),
            self.shared.artwork.jobs.load(Ordering::Relaxed),
            self.shared.artwork.requests.load(Ordering::Relaxed),
            self.shared.artwork.entries.load(Ordering::Relaxed),
            self.shared.artwork.display_entries.load(Ordering::Relaxed)
        )
    }
    pub fn control(&self, session: u64, action: Action) {
        if let Action::Seek(position) = action {
            *self.shared.seek.lock().unwrap_or_else(|e| e.into_inner()) =
                Some((session, position, Instant::now()));
            let _ = self.sender.try_send(Command::Wake);
            return;
        }
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

struct LiveGsmTcController<'a> {
    session: &'a GlobalSystemMediaTransportControlsSession,
    shared: &'a Shared,
}

impl LiveGsmTcController<'_> {
    fn finish(&self, operation: Result<IAsyncOperation<bool>>) -> ControllerResult {
        operation
            .and_then(|operation| wait(operation, self.shared))
            .and_then(|accepted| {
                if accepted {
                    Ok(())
                } else {
                    Err(Error::from(E_FAIL))
                }
            })
            .map_err(|error| error.to_string())
    }
}

impl GsmTcController for LiveGsmTcController<'_> {
    fn toggle(&self) -> ControllerResult {
        self.finish(self.session.TryTogglePlayPauseAsync())
    }

    fn previous(&self) -> ControllerResult {
        self.finish(self.session.TrySkipPreviousAsync())
    }

    fn next(&self) -> ControllerResult {
        self.finish(self.session.TrySkipNextAsync())
    }

    fn seek(&self, position_ms: u64) -> ControllerResult {
        self.finish(
            self.session
                .TryChangePlaybackPositionAsync((position_ms * 10_000) as i64),
        )
    }
}
struct Session {
    thumbnail: Option<AgileReference<windows::Storage::Streams::IRandomAccessStreamReference>>,
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
    enrichment: &mut crate::artwork::Enricher,
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
    let selection = shared
        .selection
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
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
        enrichment.cancel();
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
            thumbnail: None,
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
        current.thumbnail = info
            .Thumbnail()
            .ok()
            .and_then(|r| AgileReference::new(&r).ok());
    }
    let playback = selected.GetPlaybackInfo()?;
    next.playing = playback.PlaybackStatus()?
        == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
    let controls = playback.Controls()?;
    next.previous = controls.IsPreviousEnabled()?;
    next.play_pause = controls.IsPlayPauseToggleEnabled()?;
    next.next = controls.IsNextEnabled()?;
    next.seek = controls.IsPlaybackPositionEnabled()?;
    next.shuffle_enabled = controls.IsShuffleEnabled().unwrap_or(false);
    next.repeat_enabled = controls.IsRepeatEnabled().unwrap_or(false);
    next.shuffle = playback
        .IsShuffleActive()
        .ok()
        .and_then(|value| value.Value().ok())
        .unwrap_or(false);
    next.repeat_mode = playback
        .AutoRepeatMode()
        .ok()
        .and_then(|value| value.Value().ok())
        .map_or(0, |mode| mode.0.clamp(0, 2) as u8);
    let timeline = selected.GetTimelineProperties().ok().and_then(|t| {
        Some((
            (t.Position().ok()?.Duration / 10000).max(0) as u64,
            (t.EndTime().ok()?.Duration / 10000).max(0) as u64,
        ))
    });
    let new_track = next.title != current.snapshot.title || next.artist != current.snapshot.artist;
    if new_track {
        next.cover = None;
    }
    let netease = PlayerKind::from_session_identity(&next.source).uses_netease_extension();
    let empty_timeline = timeline.is_none_or(|(position, duration)| position == 0 && duration == 0);
    let observed_at = start.elapsed().as_secs_f64();
    next.timeline.accept(
        timeline,
        observed_at,
        current.snapshot.playing,
        next.playing,
        new_track,
    );
    if netease && empty_timeline && !next.title.is_empty() {
        next.timeline.start_missing_position_estimate(observed_at);
    }
    let key = format!("{}\0{}\0{}", next.source, next.title, next.artist);
    if let Some(extra) = enrichment.request(crate::artwork::Request {
        key,
        title: next.title.clone(),
        artist: next.artist.clone(),
        netease,
        thumbnail: current.thumbnail.clone(),
        tier: if shared.display_artwork.load(Ordering::Acquire) {
            crate::artwork::ArtworkTier::Display512
        } else {
            crate::artwork::ArtworkTier::Thumbnail128
        },
    }) {
        if next.timeline.duration_ms == 0 {
            next.timeline.duration_ms = extra.duration;
        }
        next.cover = extra.cover;
    }
    current.snapshot = next.clone();
    Ok(next)
}
fn run(shared: &Shared, receiver: Receiver<Command>, hwnd: HWND, start: Instant) {
    let Ok(mut enrichment) = crate::artwork::Enricher::new(shared.artwork.clone()) else {
        publish(
            shared,
            hwnd,
            MediaSnapshot::default(),
            Some("Artwork worker unavailable".into()),
        );
        return;
    };
    let mut manager = None;
    let mut current: Option<Session> = None;
    let mut sequence = 0;
    let mut next_poll = Instant::now();
    let mut revision = shared.revision.load(Ordering::Acquire);
    while !shared.stopping.load(Ordering::Acquire) {
        let active = shared.active.load(Ordering::Acquire);
        if !active {
            shared.seek.lock().unwrap_or_else(|e| e.into_inner()).take();
            enrichment.cancel();
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
        let latest = shared.revision.load(Ordering::Acquire);
        if revision != latest {
            current = None;
            enrichment.cancel();
            revision = latest;
        }
        let command = if matches!(command, Some(Command::Control(..))) {
            command
        } else {
            shared
                .seek
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
                .map(|(session, position, when)| {
                    Command::Control(session, Action::Seek(position), when)
                })
                .or(command)
        };
        let mut error = None;
        if let Some(Command::Control(target, action, when)) = command {
            if let Some(s) = current
                .as_ref()
                .filter(|s| s.snapshot.session == target && when.elapsed() < Duration::from_secs(2))
            {
                let controller = LiveGsmTcController {
                    session: &s.value,
                    shared,
                };
                let result = match action {
                    Action::Shuffle(enabled) if s.snapshot.shuffle_enabled => Some(controller.finish(
                        s.value.TryChangeShuffleActiveAsync(enabled),
                    )),
                    Action::Repeat(mode) if s.snapshot.repeat_enabled && mode <= 2 => {
                        Some(controller.finish(s.value.TryChangeAutoRepeatModeAsync(match mode {
                            1 => MediaPlaybackAutoRepeatMode::Track,
                            2 => MediaPlaybackAutoRepeatMode::List,
                            _ => MediaPlaybackAutoRepeatMode::None,
                        })))
                    }
                    _ => dispatch_gsmtc_action(&s.snapshot, action, &controller),
                };
                if let Some(result) = result {
                    error = result.err();
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
            match poll(
                m,
                &mut current,
                &mut sequence,
                shared,
                start,
                &mut enrichment,
            ) {
                Ok(snapshot)
                    if shared.active.load(Ordering::Acquire)
                        && revision == shared.revision.load(Ordering::Acquire) =>
                {
                    publish(shared, hwnd, snapshot, error)
                }
                Ok(_) => {}
                Err(e) => {
                    // Stale controls must never target a disappearing/replaced session.
                    current = None;
                    enrichment.cancel();
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
