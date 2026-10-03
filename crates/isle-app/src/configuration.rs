//! Independent native persistence. No registry or legacy-file writes.
use isle_core::{
    configuration::{Appearance, Controls, Document, MAX_BYTES},
    settings::{
        Revision, RuntimeSettings, SaveRequest, SettingsPatch, SettingsSnapshot,
        SettingsWindowPlacement,
    },
    weather::City,
};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{sync_channel, SyncSender},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows::{
    core::HSTRING,
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH},
        UI::WindowsAndMessaging::{PostMessageW, WM_APP},
    },
};

pub const UPDATED: u32 = WM_APP + 75;
pub fn config_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("IsleNative/settings.json")
}
pub fn legacy_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("com.isle-app.isle/settings.json"))
}
fn read(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("无法读取配置，原文件已保留".into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "读取配置失败")?;
    if bytes.len() > MAX_BYTES {
        return Err("配置超过 1 MiB，原文件已保留".into());
    }
    Ok(Some(bytes))
}
struct Pending(PathBuf);
impl Drop for Pending {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
static SERIAL: AtomicU64 = AtomicU64::new(0);
fn atomic_write(path: &Path, bytes: &[u8], replace: bool) -> Result<(), String> {
    let pending_path = path.with_extension(format!(
        "{}.{}.pending",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending_path)
        .map_err(|_| "无法创建配置临时文件")?;
    let pending = Pending(pending_path);
    let written = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "无法写入配置临时文件");
    drop(file);
    written?;
    let flags = if replace {
        MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH
    } else {
        MOVEFILE_WRITE_THROUGH
    };
    unsafe {
        MoveFileExW(
            &HSTRING::from(pending.0.as_os_str()),
            &HSTRING::from(path.as_os_str()),
            flags,
        )
    }
    .map_err(|_| "无法替换配置，原文件未改变".into())
}

struct Store {
    path: PathBuf,
    baseline: Option<Vec<u8>>,
    legacy: Option<(PathBuf, Vec<u8>)>,
    document: Result<Document, String>,
}
impl Store {
    fn open(path: PathBuf, legacy_path: Option<PathBuf>) -> Self {
        let mut store = Self {
            path,
            baseline: None,
            legacy: None,
            document: Ok(Document::default()),
        };
        store.document = (|| {
            store.baseline = read(&store.path)?;
            if let Some(bytes) = &store.baseline {
                return Document::parse(bytes);
            }
            if let Some(path) = legacy_path {
                if path == store.path {
                    return Err("原生配置与旧版配置路径不能相同".into());
                }
                if let Some(bytes) = read(&path)? {
                    let document = Document::parse(&bytes)?;
                    store.legacy = Some((path, bytes));
                    return Ok(document);
                }
            }
            Ok(Document::default())
        })();
        store
    }
    #[cfg(test)]
    fn save_city(&mut self, city: &City) -> Result<(), String> {
        self.save(&Edit::City(city.clone()))
    }
    #[cfg(test)]
    fn save(&mut self, edit: &Edit) -> Result<(), String> {
        let mut document = self.document.clone()?;
        match edit {
            Edit::City(city) => document.set_city(city)?,
            Edit::Controls(controls) => document.set_controls(controls),
            Edit::Players(selection) => document.set_selection(selection)?,
            Edit::Appearance(appearance) => document.set_appearance(appearance)?,
            Edit::WindowPlacement(placement) => {
                document.set_window_placement(placement.as_ref())?
            }
        }
        self.commit(document)
    }
    fn save_snapshot(&mut self, snapshot: &SettingsSnapshot) -> Result<(), String> {
        let mut document = self.document.clone()?;
        if let Some(city) = &snapshot.city {
            document.set_city(city)?;
        } else {
            document.clear_city();
        }
        document.set_controls(&snapshot.controls);
        document.set_appearance(&snapshot.appearance)?;
        document.set_selection(&snapshot.selection)?;
        document.set_widgets(&snapshot.widgets)?;
        document.set_window_placement(snapshot.window_placement.as_ref())?;
        self.commit(document)
    }
    fn commit(&mut self, document: Document) -> Result<(), String> {
        let bytes = document.bytes()?;
        let parent = self
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|_| "无法创建原生配置目录")?;
        // The OS releases this exclusive handle even after a crash; the empty file may remain.
        let _lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(self.path.with_extension("native.lock"))
            .map_err(|_| "另一原生实例正在保存，请稍后重试")?;
        if read(&self.path)? != self.baseline {
            return Err("配置已被其他程序修改，请重新打开应用后保存".into());
        }
        if let Some((path, original)) = &self.legacy {
            if read(path)?.as_ref() != Some(original) {
                return Err("旧版配置已改变，请重新打开应用后迁移".into());
            }
            let backup = self.path.with_extension("legacy-backup.json");
            match read(&backup)? {
                Some(existing) if existing != *original => {
                    return Err("迁移备份已存在且内容不同，已停止保存".into())
                }
                None => atomic_write(&backup, original, false)?,
                _ => {}
            }
        }
        if let Some(previous) = &self.baseline {
            atomic_write(&self.path.with_extension("previous.json"), previous, true)?;
        }
        atomic_write(&self.path, &bytes, true)?;
        self.baseline = Some(bytes);
        self.document = Ok(document);
        self.legacy = None;
        Ok(())
    }
}

#[derive(Clone)]
pub enum Edit {
    City(City),
    Controls(Controls),
    Players(isle_core::Selection),
    Appearance(Appearance),
    WindowPlacement(Option<SettingsWindowPlacement>),
}
pub struct Outcome {
    pub edit: Edit,
    pub result: Result<(), String>,
    pub revision: Revision,
}
struct Job {
    request: SaveRequest,
    edit: Edit,
}
struct Completion {
    outcome: Outcome,
    request: SaveRequest,
}
pub struct Service {
    pub city: Option<City>,
    pub controls: Controls,
    pub appearance: Appearance,
    pub selection: isle_core::Selection,
    pub widgets: Vec<isle_core::widgets::WidgetConfig>,
    pub load_error: Option<String>,
    busy: bool,
    state: RuntimeSettings,
    pending_edit: Option<(Revision, Edit)>,
    clock: Instant,
    sender: SyncSender<Option<Job>>,
    outcome: Arc<Mutex<Option<Completion>>>,
    worker: Option<JoinHandle<()>>,
}
impl Service {
    pub fn new(hwnd: HWND, path: PathBuf, legacy: Option<PathBuf>) -> std::io::Result<Self> {
        let mut store = Store::open(path, legacy);
        let city = store.document.as_ref().ok().and_then(Document::city);
        let controls = store
            .document
            .as_ref()
            .ok()
            .map(Document::controls)
            .unwrap_or_default();
        let appearance = store
            .document
            .as_ref()
            .ok()
            .map(Document::appearance)
            .unwrap_or_default();
        let selection = store
            .document
            .as_ref()
            .ok()
            .map(Document::selection)
            .unwrap_or_default();
        let widgets = store
            .document
            .as_ref()
            .ok()
            .and_then(|d| d.widgets().ok())
            .unwrap_or_else(isle_core::widgets::defaults);
        let window_placement = store
            .document
            .as_ref()
            .ok()
            .and_then(|d| d.window_placement().ok().flatten());
        let load_error = store.document.as_ref().err().cloned();
        let initial = SettingsSnapshot {
            city: city.clone(),
            controls: controls.clone(),
            appearance: appearance.clone(),
            selection: selection.clone(),
            widgets,
            window_placement,
        };
        let state = RuntimeSettings::new(initial);
        let (sender, receiver) = sync_channel::<Option<Job>>(1);
        let outcome = Arc::new(Mutex::new(None));
        let result_slot = outcome.clone();
        let hwnd = hwnd.0;
        let worker = std::thread::Builder::new()
            .name("isle-config".into())
            .spawn(move || {
                while let Ok(Some(job)) = receiver.recv() {
                    let result = store.save_snapshot(&job.request.snapshot);
                    *result_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(Completion {
                        outcome: Outcome {
                            edit: job.edit,
                            result,
                            revision: job.request.revision,
                        },
                        request: job.request,
                    });
                    unsafe {
                        let _ = PostMessageW(HWND(hwnd), UPDATED, WPARAM(0), LPARAM(0));
                    }
                }
            })?;
        Ok(Self {
            city,
            controls,
            appearance,
            selection,
            widgets: state.runtime().widgets.clone(),
            load_error,
            busy: false,
            state,
            pending_edit: None,
            clock: Instant::now(),
            sender,
            outcome,
            worker: Some(worker),
        })
    }

    fn now(&self) -> Duration {
        self.clock.elapsed()
    }
    fn sync_public_runtime(&mut self) {
        let snapshot = self.state.runtime();
        self.city = snapshot.city.clone();
        self.controls = snapshot.controls.clone();
        self.appearance = snapshot.appearance.clone();
        self.selection = snapshot.selection.clone();
        self.widgets = snapshot.widgets.clone();
    }
    fn edit_for_patch(&self, patch: &SettingsPatch) -> Edit {
        match patch {
            SettingsPatch::Appearance(_) => {
                Edit::Appearance(self.state.runtime().appearance.clone())
            }
            SettingsPatch::Controls(_) | SettingsPatch::Modules(_) => {
                Edit::Controls(self.state.runtime().controls.clone())
            }
            SettingsPatch::Media(_) => Edit::Players(self.state.runtime().selection.clone()),
            SettingsPatch::Weather(_) => self
                .state
                .runtime()
                .city
                .clone()
                .map(Edit::City)
                .unwrap_or_else(|| Edit::Controls(self.state.runtime().controls.clone())),
            SettingsPatch::WindowPlacement(placement) => Edit::WindowPlacement(placement.clone()),
        }
    }
    fn patch_for_edit(edit: &Edit) -> SettingsPatch {
        match edit {
            Edit::City(city) => SettingsPatch::Weather(isle_core::settings::WeatherPatch {
                city: Some(city.clone()),
            }),
            Edit::Controls(c) => SettingsPatch::Controls(isle_core::settings::ControlsPatch {
                panel: Some(c.panel),
                tools: c.tools.map(Some),
                animations: Some(c.animations),
                reduced: Some(c.reduced),
                always_on_top: Some(c.always_on_top),
                floating_always_on_top: Some(c.floating_always_on_top),
                time_zone: Some(c.time_zone.clone()),
            }),
            Edit::Players(selection) => SettingsPatch::Media(isle_core::settings::MediaPatch {
                selection: Some(selection.clone()),
            }),
            Edit::Appearance(a) => {
                SettingsPatch::Appearance(isle_core::settings::AppearancePatch {
                    style: Some(a.style.clone()),
                    edge: Some(a.edge.clone()),
                    edge_position: Some(a.edge_position),
                    compact_length: Some(a.compact_length),
                    collapsed_shoulder_radius: Some(a.collapsed_shoulder_radius),
                    expanded_shoulder_radius: Some(a.expanded_shoulder_radius),
                    expanded_corner_radius: Some(a.expanded_corner_radius),
                    floating_fill_color: Some(a.floating_fill_color.clone()),
                    floating_use_album_color: Some(a.floating_use_album_color),
                    show_spectrum: Some(a.show_spectrum),
                    spectrum_mode: Some(a.spectrum_mode.clone()),
                })
            }
            Edit::WindowPlacement(placement) => SettingsPatch::WindowPlacement(placement.clone()),
        }
    }
    pub fn apply_patch(
        &mut self,
        patch: SettingsPatch,
        debounce: Duration,
    ) -> Result<Revision, String> {
        let edit_patch = patch.clone();
        let revision = self.state.apply_patch(patch, self.now(), debounce)?;
        self.sync_public_runtime();
        if self.state.pending().is_some_and(|p| p.revision == revision) {
            let edit = self.edit_for_patch(&edit_patch);
            self.pending_edit = Some((revision, edit));
        }
        Ok(revision)
    }
    pub fn apply_edit(&mut self, edit: Edit, debounce: Duration) -> Result<Revision, String> {
        let revision = self.apply_patch(Self::patch_for_edit(&edit), debounce)?;
        if self
            .state
            .pending()
            .is_some_and(|pending| pending.revision == revision)
        {
            self.pending_edit = Some((revision, edit));
        }
        Ok(revision)
    }
    /// Compatibility with the previous Apply button: apply immediately and
    /// start persistence without a debounce interval.
    pub fn save(&mut self, edit: Edit) -> bool {
        let before = self.state.runtime_revision();
        let accepted = self.save_debounced(edit.clone(), Duration::ZERO);
        if accepted {
            let revision = if self.state.runtime_revision() == before {
                if self.state.pending().is_some() {
                    self.state.force_flush(self.now());
                    self.state.runtime_revision()
                } else {
                    self.state.queue_current(self.now(), Duration::ZERO)
                }
            } else {
                self.state.runtime_revision()
            };
            self.pending_edit = Some((revision, edit));
            self.tick();
        }
        accepted
    }
    /// Apply the edit to Runtime now, then coalesce it into the latest full
    /// pending snapshot for persistence after `debounce`.
    pub fn save_debounced(&mut self, edit: Edit, debounce: Duration) -> bool {
        self.apply_edit(edit, debounce).is_ok()
    }
    pub fn tick(&mut self) -> bool {
        if self.busy {
            return false;
        }
        let Some(request) = self.state.begin_save(self.now()) else {
            return false;
        };
        let edit = self
            .pending_edit
            .as_ref()
            .filter(|(revision, _)| *revision == request.revision)
            .map(|(_, edit)| edit.clone())
            .unwrap_or_else(|| Edit::Controls(request.snapshot.controls.clone()));
        match self.sender.try_send(Some(Job {
            request: request.clone(),
            edit,
        })) {
            Ok(()) => {
                self.busy = true;
                true
            }
            Err(_) => {
                self.state.cancel_begin_save(request.revision);
                false
            }
        }
    }
    pub fn take(&mut self) -> Option<Outcome> {
        let completion = self
            .outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()?;
        self.busy = false;
        self.state
            .finish_save(&completion.request, completion.outcome.result.clone());
        if self
            .pending_edit
            .as_ref()
            .is_some_and(|(revision, _)| *revision <= completion.outcome.revision)
        {
            self.pending_edit = None;
        }
        let outcome = completion.outcome;
        self.tick();
        Some(outcome)
    }
    pub fn next_save_delay(&self) -> Option<Duration> {
        self.state.next_save_delay(self.now())
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    pub fn dirty(&self) -> bool {
        self.state.is_dirty()
    }
    pub fn last_error(&self) -> Option<&str> {
        self.state.last_error()
    }
    pub fn runtime_revision(&self) -> Revision {
        self.state.runtime_revision()
    }
    pub fn persisted_revision(&self) -> Revision {
        self.state.persisted_revision()
    }
    pub fn window_placement(&self) -> Option<&SettingsWindowPlacement> {
        self.state.runtime().window_placement.as_ref()
    }
    pub fn saving_revision(&self) -> Option<Revision> {
        self.state.saving_revision()
    }
    pub fn retry(&mut self) -> bool {
        let changed = self.state.retry(self.now());
        self.tick();
        changed
    }
    pub fn revert(&mut self) -> bool {
        let changed = self.state.revert(self.now());
        if changed {
            self.sync_public_runtime();
            if self.state.pending().is_some() {
                let revision = self.state.runtime_revision();
                self.pending_edit = Some((revision, Edit::Controls(self.controls.clone())));
            } else {
                self.pending_edit = None;
            }
        }
        self.tick();
        changed
    }
    pub fn flush_timeout(&mut self, timeout: Duration) -> Result<(), String> {
        self.state.force_flush(self.now());
        self.tick();
        let deadline = Instant::now() + timeout;
        loop {
            let has_completion = self
                .outcome
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some();
            if has_completion {
                if let Some(outcome) = self.take() {
                    outcome.result?;
                    self.state.force_flush(self.now());
                    self.tick();
                }
            }
            if let Some(error) = self.state.last_error() {
                return Err(error.to_string());
            }
            if !self.busy && !self.state.is_dirty() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("配置保存超时，仍有未写入的修改".into());
            }
            std::thread::sleep(
                Duration::from_millis(5).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.sender.try_send(None);
        // A timed-out filesystem operation must never hold process shutdown.
        // Dropping JoinHandle detaches the worker; channel closure ends it once
        // any current atomic write has returned.
        self.worker.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn appearance_length(service: &Service, length: u16) -> Edit {
        let mut appearance = service.appearance.clone();
        appearance.compact_length = length;
        Edit::Appearance(appearance)
    }
    #[test]
    fn service_flush_preserves_latest_edit_behind_an_in_flight_write() {
        let fixture = Fixture::new();
        let path = fixture.path("settings.json");
        std::fs::write(
            &path,
            br#"{"compactLength":100,"future":{"keep":[1,null,3]}}"#,
        )
        .unwrap();
        let mut service = Service::new(HWND(0), path.clone(), None).unwrap();
        assert!(service.save(appearance_length(&service, 120)));
        assert!(service.busy());
        service
            .apply_edit(appearance_length(&service, 180), Duration::from_secs(30))
            .unwrap();
        assert_eq!(service.appearance.compact_length, 180);
        assert_eq!(service.next_save_delay(), None);
        service.flush_timeout(Duration::from_secs(2)).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert_eq!(
            Document::parse(&saved).unwrap().appearance().compact_length,
            180
        );
        let compact: String = String::from_utf8(saved)
            .unwrap()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        assert!(compact.contains(r#""future":{"keep":[1,null,3]}"#));
        assert!(!service.dirty());
        assert_eq!(service.runtime_revision(), service.persisted_revision());
    }
    #[test]
    fn service_revert_wins_over_an_in_flight_write_and_keeps_conflicts_safe() {
        let fixture = Fixture::new();
        let path = fixture.path("settings.json");
        std::fs::write(&path, br#"{"compactLength":100}"#).unwrap();
        let mut service = Service::new(HWND(0), path.clone(), None).unwrap();
        assert!(service.save(appearance_length(&service, 160)));
        assert!(service.revert());
        service.flush_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(service.appearance.compact_length, 100);
        let saved = std::fs::read(&path).unwrap();
        assert_eq!(
            Document::parse(&saved).unwrap().appearance().compact_length,
            100
        );
        let external = br#"{"external":true,"compactLength":92}"#;
        std::fs::write(&path, external).unwrap();
        service
            .apply_edit(appearance_length(&service, 190), Duration::ZERO)
            .unwrap();
        assert!(service.flush_timeout(Duration::from_secs(2)).is_err());
        assert_eq!(service.appearance.compact_length, 190);
        assert_eq!(std::fs::read(&path).unwrap(), external);
        service.retry();
        assert!(service.flush_timeout(Duration::from_secs(2)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), external);
        service.revert();
        assert_eq!(service.appearance.compact_length, 100);
        assert!(!service.dirty());
    }

    #[test]
    fn placement_and_other_edits_share_latest_snapshot_and_survive_restart() {
        let fixture = Fixture::new();
        let path = fixture.path("settings.json");
        std::fs::write(
            &path,
            br#"{"compactLength":100,"futureRoot":{"keep":[1,null,3]},"settingsWindowPlacement":{"x":-1920,"y":120,"widthDip":820,"heightDip":760,"dpi":144,"future":{"keep":true}}}"#,
        )
        .unwrap();
        {
            let mut service = Service::new(HWND(0), path.clone(), None).unwrap();
            assert_eq!(service.window_placement().unwrap().x, -1920);
            let placement = SettingsWindowPlacement {
                x: -1280,
                y: 88,
                width_dip: 900,
                height_dip: 700,
                dpi: 192,
            };
            service
                .apply_patch(
                    SettingsPatch::WindowPlacement(Some(placement.clone())),
                    Duration::ZERO,
                )
                .unwrap();
            service
                .apply_patch(
                    SettingsPatch::Appearance(isle_core::settings::AppearancePatch {
                        compact_length: Some(180),
                        ..Default::default()
                    }),
                    Duration::ZERO,
                )
                .unwrap();
            service.flush_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(service.window_placement(), Some(&placement));
            let saved: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(saved["compactLength"], 180);
            assert_eq!(saved["settingsWindowPlacement"]["x"], -1280);
            assert_eq!(saved["settingsWindowPlacement"]["future"]["keep"], true);
            assert_eq!(saved["futureRoot"]["keep"], serde_json::json!([1, null, 3]));
        }
        let restarted = Service::new(HWND(0), path, None).unwrap();
        assert_eq!(restarted.window_placement().unwrap().x, -1280);
        assert_eq!(restarted.appearance.compact_length, 180);
    }

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "isle-config-test-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn city() -> City {
        City {
            name: "Test".into(),
            latitude: 31.,
            longitude: 121.,
        }
    }
    #[test]
    fn legacy_migration_is_read_only_backed_up_and_restartable() {
        let f = Fixture::new();
        let old = f.path("old.json");
        let new = f.path("native.json");
        let original = br#"{"future":{"a":[1,2]},"autoHide":false,"playerOrderIds":["two","one"]}"#;
        std::fs::write(&old, original).unwrap();
        let mut store = Store::open(new.clone(), Some(old.clone()));
        assert!(!new.exists());
        store.save_city(&city()).unwrap();
        assert_eq!(read(&old).unwrap().unwrap(), original);
        assert_eq!(
            read(&new.with_extension("legacy-backup.json"))
                .unwrap()
                .unwrap(),
            original
        );
        let restarted = Store::open(new.clone(), Some(old));
        assert_eq!(restarted.document.as_ref().unwrap().city(), Some(city()));
        let bytes = read(&new).unwrap().unwrap();
        let mut second = city();
        second.name = "Second".into();
        store.save_city(&second).unwrap();
        assert_eq!(
            read(&new.with_extension("previous.json")).unwrap().unwrap(),
            bytes
        );
        let persisted = String::from_utf8(read(&new).unwrap().unwrap()).unwrap();
        assert!(persisted.contains("future") && persisted.contains("two"));
    }
    #[test]
    fn corrupt_native_blocks_legacy_fallback_and_overwrite() {
        let f = Fixture::new();
        let p = f.path("native.json");
        std::fs::write(&p, b"{broken").unwrap();
        let mut store = Store::open(p.clone(), Some(f.path("legacy.json")));
        assert!(store.document.is_err() && store.save_city(&city()).is_err());
        assert_eq!(read(&p).unwrap().unwrap(), b"{broken");
    }
    #[test]
    fn stale_instances_and_exclusive_save_locks_do_not_overwrite() {
        let f = Fixture::new();
        let p = f.path("native.json");
        std::fs::write(&p, b"{}").unwrap();
        let mut first = Store::open(p.clone(), None);
        let mut second = Store::open(p.clone(), None);
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(p.with_extension("native.lock"))
            .unwrap();
        assert!(first.save_city(&city()).is_err());
        drop(lock);
        first.save_city(&city()).unwrap();
        let saved = read(&p).unwrap();
        assert!(second.save_city(&city()).is_err());
        assert_eq!(read(&p).unwrap(), saved);
    }
    #[test]
    fn failed_replace_keeps_original_and_cleans_own_pending_file() {
        let f = Fixture::new();
        let p = f.path("native.json");
        std::fs::write(&p, b"{}").unwrap();
        let mut store = Store::open(p.clone(), None);
        let held = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&p)
            .unwrap();
        assert!(store.save_city(&city()).is_err());
        assert_eq!(read(&p).unwrap().unwrap(), b"{}");
        drop(held);
        assert!(std::fs::read_dir(&f.0).unwrap().all(|entry| entry
            .unwrap()
            .path()
            .extension()
            .is_none_or(|e| e != "pending")));
        store.save_city(&city()).unwrap();
    }
}
