//! Independent native persistence. No registry or legacy-file writes.
use isle_core::{
    configuration::{Controls, Document, MAX_BYTES},
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
    fn save(&mut self, edit: &Edit) -> Result<(), String> {
        let mut document = self.document.clone()?;
        match edit {
            Edit::City(city) => document.set_city(city)?,
            Edit::Controls(controls) => document.set_controls(controls),
        }
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
}
pub struct Outcome {
    pub edit: Edit,
    pub result: Result<(), String>,
}
pub struct Service {
    pub city: Option<City>,
    pub controls: Controls,
    pub load_error: Option<String>,
    busy: bool,
    sender: SyncSender<Option<Edit>>,
    outcome: Arc<Mutex<Option<Outcome>>>,
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
        let load_error = store.document.as_ref().err().cloned();
        let (sender, receiver) = sync_channel(1);
        let outcome = Arc::new(Mutex::new(None));
        let result_slot = outcome.clone();
        let hwnd = hwnd.0;
        let worker = std::thread::Builder::new()
            .name("isle-config".into())
            .spawn(move || {
                while let Ok(Some(edit)) = receiver.recv() {
                    let result = store.save(&edit);
                    *result_slot.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(Outcome { edit, result });
                    unsafe {
                        let _ = PostMessageW(HWND(hwnd), UPDATED, WPARAM(0), LPARAM(0));
                    }
                }
            })?;
        Ok(Self {
            city,
            controls,
            load_error,
            busy: false,
            sender,
            outcome,
            worker: Some(worker),
        })
    }
    pub fn save(&mut self, edit: Edit) -> bool {
        if self.busy {
            return false;
        }
        self.busy = self.sender.try_send(Some(edit)).is_ok();
        self.busy
    }
    pub fn take(&mut self) -> Option<Outcome> {
        let out = self
            .outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()?;
        self.busy = false;
        if out.result.is_ok() {
            match &out.edit {
                Edit::City(city) => self.city = Some(city.clone()),
                Edit::Controls(controls) => self.controls = controls.clone(),
            }
        }
        Some(out)
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.sender.send(None);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
