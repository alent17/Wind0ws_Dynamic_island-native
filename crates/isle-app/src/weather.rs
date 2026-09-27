use isle_core::weather::{self, Cache, City, Forecast};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::{
    core::*,
    Win32::{Foundation::*, Networking::WinHttp::*, UI::WindowsAndMessaging::*},
};
pub const UPDATED: u32 = WM_APP + 73;
#[derive(Clone, PartialEq)]
pub enum Job {
    Forecast(City),
    Search(String),
}
pub enum Output {
    Forecast(City, Option<Forecast>, bool),
    Search(Vec<City>, bool),
}
#[derive(Default)]
struct Slot {
    job: Option<(u64, Job)>,
    output: Option<(u64, Output)>,
    quit: bool,
}
struct Shared {
    epoch: AtomicU64,
    slot: Mutex<Slot>,
    ready: Condvar,
    busy: AtomicBool,
    requests: AtomicU64,
    entries: AtomicU64,
    error: AtomicU64,
    status: AtomicU64,
}
pub struct Service {
    state: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    requested: Option<(Job, Instant)>,
}
impl Service {
    pub fn new(hwnd: HWND) -> std::io::Result<Self> {
        let state = Arc::new(Shared {
            epoch: AtomicU64::new(0),
            slot: Mutex::default(),
            ready: Condvar::new(),
            busy: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            entries: AtomicU64::new(0),
            error: AtomicU64::new(0),
            status: AtomicU64::new(0),
        });
        let s = state.clone();
        let hwnd = hwnd.0;
        let worker = thread::Builder::new()
            .name("isle-weather".into())
            .spawn(move || {
                let start = Instant::now();
                let mut cache = Cache::default();
                loop {
                    let mut slot = s.slot.lock().unwrap_or_else(|e| e.into_inner());
                    while slot.job.is_none() && !slot.quit {
                        slot = s.ready.wait(slot).unwrap_or_else(|e| e.into_inner());
                    }
                    if slot.quit {
                        break;
                    }
                    let (epoch, job) = slot.job.take().unwrap();
                    drop(slot);
                    s.busy.store(true, Ordering::Release);
                    let ctx = Context {
                        state: &s,
                        epoch,
                        deadline: Instant::now() + Duration::from_secs(20),
                    };
                    let output = match job {
                        Job::Forecast(city) => {
                            let (mut data, needed, mut failed) =
                                cache.lookup(&city, start.elapsed().as_secs());
                            if needed {
                                let result = record(&ctx, forecast(&city, &ctx));
                                if ctx.current() {
                                    cache.record(city.clone(), result, start.elapsed().as_secs());
                                    (data, _, failed) =
                                        cache.lookup(&city, start.elapsed().as_secs());
                                }
                            }
                            Output::Forecast(city, data, failed)
                        }
                        Job::Search(query) => {
                            let result = record(&ctx, search(&query, &ctx));
                            let failed = result.is_none();
                            Output::Search(result.unwrap_or_default(), failed)
                        }
                    };
                    s.entries.store(cache.len() as u64, Ordering::Relaxed);
                    s.busy.store(false, Ordering::Release);
                    if ctx.current() {
                        let mut slot = s.slot.lock().unwrap_or_else(|e| e.into_inner());
                        slot.output = Some((epoch, output));
                        unsafe {
                            let _ = PostMessageW(HWND(hwnd), UPDATED, WPARAM(0), LPARAM(0));
                        }
                    }
                }
            })?;
        Ok(Self {
            state,
            worker: Some(worker),
            requested: None,
        })
    }
    pub fn request(&mut self, job: Option<Job>) {
        if job.as_ref().is_some_and(|job| {
            self.requested.as_ref().is_some_and(|(old, time)| {
                old == job
                    && (matches!(job, Job::Search(_)) || time.elapsed() < Duration::from_secs(60))
            })
        }) {
            return;
        }
        if job.is_none() && self.requested.is_none() {
            return;
        }
        let epoch = self.state.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        self.requested = job.clone().map(|j| (j, Instant::now()));
        let mut slot = self.state.slot.lock().unwrap_or_else(|e| e.into_inner());
        slot.job = job.map(|j| (epoch, j));
        slot.output = None;
        self.state.ready.notify_one();
    }
    pub fn take(&self) -> Option<Output> {
        self.state
            .slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .output
            .take()
            .filter(|(epoch, _)| *epoch == self.state.epoch.load(Ordering::Acquire))
            .map(|(_, out)| out)
    }
    pub fn diagnostics(&self) -> String {
        format!(
            "\"weatherBusy\":{},\"weatherRequests\":{},\"weatherCacheEntries\":{},\"weatherHresult\":{},\"weatherHttpStatus\":{}",
            self.state.busy.load(Ordering::Acquire),
            self.state.requests.load(Ordering::Relaxed),
            self.state.entries.load(Ordering::Relaxed),
            self.state.error.load(Ordering::Relaxed),
            self.state.status.load(Ordering::Relaxed)
        )
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.request(None);
        self.state
            .slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .quit = true;
        self.state.ready.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
struct Context<'a> {
    state: &'a Shared,
    epoch: u64,
    deadline: Instant,
}
fn record<T>(ctx: &Context<'_>, result: Result<T>) -> Option<T> {
    if ctx.current() {
        ctx.state.error.store(
            result
                .as_ref()
                .err()
                .map(|e| e.code().0 as u32 as u64)
                .unwrap_or(0),
            Ordering::Relaxed,
        );
    }
    result.ok()
}
impl Context<'_> {
    fn current(&self) -> bool {
        self.state.epoch.load(Ordering::Acquire) == self.epoch
    }
    fn check(&self) -> Result<()> {
        if self.current() && Instant::now() < self.deadline {
            Ok(())
        } else {
            Err(E_ABORT.into())
        }
    }
}
struct Internet(*mut core::ffi::c_void);
impl Internet {
    fn new(value: *mut core::ffi::c_void) -> Result<Self> {
        if value.is_null() {
            Err(Error::from_win32())
        } else {
            Ok(Self(value))
        }
    }
}
impl Drop for Internet {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}
struct Proxy(WINHTTP_CURRENT_USER_IE_PROXY_CONFIG);
impl Drop for Proxy {
    fn drop(&mut self) {
        unsafe {
            for value in [
                self.0.lpszProxy,
                self.0.lpszProxyBypass,
                self.0.lpszAutoConfigUrl,
            ] {
                if !value.is_null() {
                    let _ = GlobalFree(HGLOBAL(value.0.cast()));
                }
            }
        }
    }
}
fn download(url: &str, ctx: &Context<'_>) -> Result<Vec<u8>> {
    ctx.check()?;
    let (host, path) = url
        .strip_prefix("https://")
        .and_then(|s| s.split_once('/'))
        .ok_or_else(|| Error::from(E_INVALIDARG))?;
    if !matches!(host, "api.open-meteo.com" | "geocoding-api.open-meteo.com") {
        return Err(E_INVALIDARG.into());
    }
    unsafe {
        let mut proxy = Proxy(WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default());
        let _ = WinHttpGetIEProxyConfigForCurrentUser(&mut proxy.0);
        let explicit = !proxy.0.lpszProxy.is_null();
        let client = Internet::new(WinHttpOpen(
            w!("IsleNative/0.1"),
            if explicit {
                WINHTTP_ACCESS_TYPE_NAMED_PROXY
            } else {
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY
            },
            PCWSTR(proxy.0.lpszProxy.0),
            PCWSTR(proxy.0.lpszProxyBypass.0),
            0,
        ))?;
        WinHttpSetTimeouts(client.0, 2000, 4000, 4000, 8000)?;
        let connection = Internet::new(WinHttpConnect(client.0, &HSTRING::from(host), 443, 0))?;
        let request = Internet::new(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            &HSTRING::from(format!("/{path}")),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        let redirect = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&redirect.to_ne_bytes()),
        )?;
        ctx.state.requests.fetch_add(1, Ordering::Relaxed);
        ctx.check()?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0)?;
        ctx.check()?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())?;
        let mut status = 0u32;
        let mut length = 4;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut length,
            std::ptr::null_mut(),
        )?;
        ctx.state.status.store(status as u64, Ordering::Relaxed);
        if status != 200 {
            return Err(E_FAIL.into());
        }
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            ctx.check()?;
            let mut read = 0;
            WinHttpReadData(request.0, buffer.as_mut_ptr().cast(), 8192, &mut read)?;
            ctx.check()?;
            if read == 0 {
                break;
            }
            if bytes.len() + read as usize > 262144 {
                return Err(E_OUTOFMEMORY.into());
            }
            bytes.extend_from_slice(&buffer[..read as usize]);
        }
        Ok(bytes)
    }
}
fn forecast(city: &City, ctx: &Context<'_>) -> Result<Forecast> {
    if !city.valid() {
        return Err(E_INVALIDARG.into());
    }
    let url=format!("https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code&daily=weather_code,temperature_2m_max,temperature_2m_min&forecast_days=4&timezone=auto",city.latitude,city.longitude);
    weather::parse_forecast(&download(&url, ctx)?).ok_or_else(|| E_FAIL.into())
}
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn search(query: &str, ctx: &Context<'_>) -> Result<Vec<City>> {
    if !(2..=80).contains(&query.trim().chars().count()) {
        return Err(E_INVALIDARG.into());
    }
    for _ in 0..25 {
        if !ctx.current() {
            return Err(E_ABORT.into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    let mut queries = vec![query.trim().to_string()];
    if query
        .chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
        && !query.ends_with('市')
    {
        queries.insert(0, format!("{}市", query.trim()));
    }
    let mut cities = vec![];
    for query in queries {
        let url=format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=8&language=zh&format=json",encode(&query));
        let result = download(&url, ctx)
            .and_then(|bytes| weather::parse_cities(&bytes).ok_or_else(|| Error::from(E_FAIL)));
        let values = match result {
            Ok(values) => values,
            Err(_) if !cities.is_empty() => break,
            Err(error) => return Err(error),
        };
        for city in values {
            if !cities
                .iter()
                .any(|old: &weather::Candidate| old.city.same(&city.city))
            {
                cities.push(city);
            }
        }
    }
    cities.truncate(16);
    cities.sort_by_key(|city| std::cmp::Reverse(city.rank));
    Ok(cities.into_iter().map(|candidate| candidate.city).collect())
}
pub fn config_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("IsleNative")
        .join("settings.json")
}
pub fn load(path: &std::path::Path, legacy: bool) -> Option<City> {
    fn read(path: &std::path::Path) -> Option<City> {
        if std::fs::metadata(path).ok()?.len() > 65536 {
            return None;
        }
        weather::settings(&std::fs::read(path).ok()?)
    }
    if path.exists() {
        return read(path);
    }
    if legacy {
        read(
            &std::env::var_os("APPDATA")
                .map(PathBuf::from)?
                .join("com.isle-app.isle/settings.json"),
        )
    } else {
        None
    }
}
pub fn save(path: &std::path::Path, city: &City) -> Result<()> {
    if !city.valid() {
        return Err(E_INVALIDARG.into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| Error::from(E_FAIL))?;
    }
    let pending = path.with_extension(format!("{}.pending", std::process::id()));
    std::fs::write(&pending, weather::settings_bytes(city)).map_err(|_| Error::from(E_FAIL))?;
    use windows::Win32::Storage::FileSystem::*;
    let result = unsafe {
        MoveFileExW(
            &HSTRING::from(pending.as_os_str()),
            &HSTRING::from(path.as_os_str()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result.is_err() {
        let _ = std::fs::remove_file(pending);
    }
    result
}
