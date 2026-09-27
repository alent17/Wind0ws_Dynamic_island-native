//! One cancellable enrichment job; native HTTP and image decoding, no WebView.
use isle_core::Cover;
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
    Data::Json::*,
    Foundation::*,
    Graphics::Imaging::*,
    Storage::Streams::*,
    Web::Http::*,
    Win32::{Foundation::*, System::WinRT::*},
};

#[derive(Clone, Default)]
pub struct Extra {
    pub duration: u64,
    pub cover: Option<Arc<Cover>>,
}
pub struct Request {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub netease: bool,
    pub thumbnail: Option<AgileReference<IRandomAccessStreamReference>>,
}
#[derive(Default)]
struct Slot {
    job: Option<(u64, Request)>,
    output: Option<(String, Extra)>,
    quit: bool,
}
struct State {
    http_times: Mutex<VecDeque<Instant>>,
    stats: Arc<Statistics>,
    epoch: AtomicU64,
    slot: Mutex<Slot>,
    ready: Condvar,
}
pub struct Enricher {
    state: Arc<State>,
    worker: Option<JoinHandle<()>>,
    requested: Option<(String, Instant)>,
}
#[derive(Default)]
pub struct Statistics {
    pub busy: AtomicBool,
    pub jobs: AtomicU64,
    pub requests: AtomicU64,
    pub entries: AtomicU64,
}
impl Enricher {
    pub fn new(stats: Arc<Statistics>) -> std::io::Result<Self> {
        let state = Arc::new(State {
            http_times: Mutex::default(),
            stats,
            epoch: AtomicU64::new(0),
            slot: Mutex::new(Slot::default()),
            ready: Condvar::new(),
        });
        let shared = state.clone();
        let worker = thread::Builder::new()
            .name("isle-artwork".into())
            .spawn(move || {
                if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_err() {
                    return;
                }
                let mut cache: VecDeque<(String, Instant, Extra)> = VecDeque::new();
                loop {
                    let mut slot = shared.slot.lock().unwrap_or_else(|e| e.into_inner());
                    while slot.job.is_none() && !slot.quit {
                        slot = shared.ready.wait(slot).unwrap_or_else(|e| e.into_inner());
                    }
                    if slot.quit {
                        break;
                    }
                    let (epoch, request) = slot.job.take().unwrap();
                    drop(slot);
                    shared.stats.busy.store(true, Ordering::Release);
                    shared.stats.jobs.fetch_add(1, Ordering::Relaxed);
                    let context = Context {
                        state: &shared,
                        epoch,
                        deadline: Instant::now() + Duration::from_secs(8),
                    };
                    let extra = if let Some(index) = cache.iter().position(|(key, time, extra)| {
                        key == &request.key
                            && time.elapsed()
                                < Duration::from_secs(
                                    if extra.cover.is_some()
                                        && (!request.netease || extra.duration > 0)
                                    {
                                        86400
                                    } else {
                                        30
                                    },
                                )
                    }) {
                        let item = cache.remove(index).unwrap();
                        let extra = item.2.clone();
                        cache.push_back(item);
                        extra
                    } else {
                        let extra = resolve(&request, &context);
                        if context.current() {
                            cache.retain(|(key, _, _)| key != &request.key);
                            if cache.len() >= 8 {
                                cache.pop_front();
                            }
                            cache.push_back((request.key.clone(), Instant::now(), extra.clone()));
                        }
                        extra
                    };
                    let mut slot = shared.slot.lock().unwrap_or_else(|e| e.into_inner());
                    if context.current() {
                        slot.output = Some((request.key, extra));
                    }
                    shared
                        .stats
                        .entries
                        .store(cache.len() as u64, Ordering::Relaxed);
                    shared.stats.busy.store(false, Ordering::Release);
                }
                unsafe {
                    RoUninitialize();
                }
            })?;
        Ok(Self {
            state,
            worker: Some(worker),
            requested: None,
        })
    }
    pub fn request(&mut self, request: Request) -> Option<Extra> {
        let mut slot = self.state.slot.lock().unwrap_or_else(|e| e.into_inner());
        let result = slot
            .output
            .as_ref()
            .filter(|(key, _)| key == &request.key)
            .map(|(_, extra)| extra.clone());
        if self.requested.as_ref().is_none_or(|(key, time)| {
            key != &request.key
                || (time.elapsed() > Duration::from_secs(30)
                    && result
                        .as_ref()
                        .is_none_or(|e| e.cover.is_none() || (request.netease && e.duration == 0)))
        }) {
            self.requested = Some((request.key.clone(), Instant::now()));
            let epoch = self.state.epoch.fetch_add(1, Ordering::AcqRel) + 1;
            slot.job = Some((epoch, request));
            self.state.ready.notify_one();
        }
        result
    }
    pub fn cancel(&mut self) {
        if self.requested.take().is_some() {
            self.state.epoch.fetch_add(1, Ordering::AcqRel);
            let mut slot = self.state.slot.lock().unwrap_or_else(|e| e.into_inner());
            slot.job = None;
            slot.output = None;
        }
    }
}
impl Drop for Enricher {
    fn drop(&mut self) {
        self.cancel();
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
    state: &'a State,
    epoch: u64,
    deadline: Instant,
}
impl Context<'_> {
    fn current(&self) -> bool {
        self.state.epoch.load(Ordering::Acquire) == self.epoch
    }
    fn ready<T: ComInterface>(&self, operation: &T) -> Result<()> {
        let info: IAsyncInfo = operation.cast()?;
        loop {
            if !self.current() || Instant::now() >= self.deadline {
                let _ = info.Cancel();
                return Err(Error::from(E_ABORT));
            }
            if info.Status()? != AsyncStatus::Started {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
macro_rules! receive {
    ($ctx:expr, $op:expr) => {{
        let op = $op?;
        $ctx.ready(&op)?;
        op.GetResults()?
    }};
}

struct CloseOnDrop(Option<IClosable>);
impl CloseOnDrop {
    fn new<T: ComInterface>(object: &T) -> Self {
        Self(object.cast().ok())
    }
}
impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        if let Some(object) = &self.0 {
            let _ = object.Close();
        }
    }
}

fn read(stream: &IInputStream, context: &Context<'_>) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let buffer = Buffer::Create(16384)?;
        let buffer = receive!(
            context,
            stream.ReadAsync(&buffer, 16384, InputStreamOptions::Partial)
        );
        let size = buffer.Length()? as usize;
        if size == 0 {
            return Ok(bytes);
        }
        if bytes.len() + size > 2 * 1024 * 1024 {
            return Err(Error::from(E_OUTOFMEMORY));
        }
        let offset = bytes.len();
        bytes.resize(offset + size, 0);
        DataReader::FromBuffer(&buffer)?.ReadBytes(&mut bytes[offset..])?;
    }
}
fn download(url: &str, context: &Context<'_>) -> Result<Vec<u8>> {
    if !context.current() || Instant::now() >= context.deadline {
        return Err(Error::from(E_ABORT));
    }
    {
        let mut times = context
            .state
            .http_times
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        while times
            .front()
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(60))
        {
            times.pop_front();
        }
        if times.len() >= 18 {
            return Err(Error::from(E_ABORT));
        }
        times.push_back(Instant::now());
    }
    context.state.stats.requests.fetch_add(1, Ordering::Relaxed);
    let client = HttpClient::new()?;
    let _client_lifetime = CloseOnDrop::new(&client);
    let uri = Uri::CreateUri(&HSTRING::from(url))?;
    let response = receive!(
        context,
        client.GetWithOptionAsync(&uri, HttpCompletionOption::ResponseHeadersRead)
    );
    let _response_lifetime = CloseOnDrop::new(&response);
    if response.StatusCode()? != HttpStatusCode::Ok {
        return Err(Error::from(E_FAIL));
    }
    let stream = receive!(context, response.Content()?.ReadAsInputStreamAsync());
    let _stream_lifetime = CloseOnDrop::new(&stream);
    read(&stream, context)
}
fn object(url: &str, context: &Context<'_>) -> Result<JsonObject> {
    let bytes = download(url, context)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| Error::from(E_FAIL))?;
    JsonObject::Parse(&HSTRING::from(text))
}
fn string(value: &JsonObject, key: &str) -> String {
    value
        .GetNamedString(&HSTRING::from(key))
        .unwrap_or_default()
        .to_string()
}
fn number(value: &JsonObject, key: &str) -> u64 {
    value
        .GetNamedNumber(&HSTRING::from(key))
        .unwrap_or(0.)
        .max(0.) as u64
}
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn song(title: &str, artist: &str, context: &Context<'_>) -> Result<Option<JsonObject>> {
    for query in [format!("{title} {artist}"), title.to_string()] {
        let json = object(
            &format!(
                "https://music.163.com/api/search/get/?s={}&type=1&limit=30&offset=0",
                encode(&query)
            ),
            context,
        )?;
        let Ok(songs) = json
            .GetNamedObject(&HSTRING::from("result"))
            .and_then(|r| r.GetNamedArray(&HSTRING::from("songs")))
        else {
            continue;
        };
        for index in 0..songs.Size()?.min(30) {
            let candidate = songs.GetObjectAt(index)?;
            let Ok(artists) = candidate
                .GetNamedArray(&HSTRING::from("artists"))
                .or_else(|_| candidate.GetNamedArray(&HSTRING::from("ar")))
            else {
                continue;
            };
            let names = (0..artists.Size()?)
                .filter_map(|i| artists.GetObjectAt(i).ok())
                .map(|a| string(&a, "name"))
                .collect::<Vec<_>>()
                .join(" ");
            if isle_core::matching_song(title, artist, &string(&candidate, "name"), &names) {
                let id = number(&candidate, "id");
                let detail = object(
                    &format!("https://music.163.com/api/song/detail/?id={id}&ids=%5B{id}%5D"),
                    context,
                )
                .and_then(|d| d.GetNamedArray(&HSTRING::from("songs")))
                .and_then(|a| a.GetObjectAt(0));
                let detail = detail
                    .ok()
                    .filter(|d| number(d, "id") == id)
                    .unwrap_or_else(|| candidate.clone());
                if number(&detail, "duration").max(number(&detail, "dt")) == 0 {
                    detail.Insert(
                        &HSTRING::from("duration"),
                        &JsonValue::CreateNumberValue(
                            number(&candidate, "duration").max(number(&candidate, "dt")) as f64,
                        )?,
                    )?;
                }
                return Ok(Some(detail));
            }
        }
    }
    Ok(None)
}
fn decode(bytes: &[u8], context: &Context<'_>) -> Result<Arc<Cover>> {
    let stream = InMemoryRandomAccessStream::new()?;
    let _stream_lifetime = CloseOnDrop::new(&stream);
    let writer = DataWriter::CreateDataWriter(&stream)?;
    let _writer_lifetime = CloseOnDrop::new(&writer);
    writer.WriteBytes(bytes)?;
    receive!(context, writer.StoreAsync());
    writer.DetachStream()?;
    stream.Seek(0)?;
    let decoder = receive!(context, BitmapDecoder::CreateAsync(&stream));
    let (width, height) = (decoder.PixelWidth()?, decoder.PixelHeight()?);
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || width as u64 * height as u64 > 16_000_000
    {
        return Err(Error::from(E_INVALIDARG));
    }
    let transform = BitmapTransform::new()?;
    // Album art occupies a square; crop the centre before downsampling.
    let side = width.min(height);
    let scaled_width = (width as u64 * 128 / side as u64) as u32;
    let scaled_height = (height as u64 * 128 / side as u64) as u32;
    if scaled_width > 2048 || scaled_height > 2048 {
        return Err(Error::from(E_INVALIDARG));
    }
    transform.SetScaledWidth(scaled_width)?;
    transform.SetScaledHeight(scaled_height)?;
    transform.SetBounds(BitmapBounds {
        X: (scaled_width - 128) / 2,
        Y: (scaled_height - 128) / 2,
        Width: 128,
        Height: 128,
    })?;
    let provider = receive!(
        context,
        decoder.GetPixelDataTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Premultiplied,
            &transform,
            ExifOrientationMode::IgnoreExifOrientation,
            ColorManagementMode::DoNotColorManage
        )
    );
    let mut pixels = provider.DetachPixelData()?.to_vec();
    if pixels.len() != 128 * 128 * 4 {
        return Err(Error::from(E_FAIL));
    }
    // Pre-mask the rounded cover once instead of creating a D2D layer per frame.
    for y in 0..128 {
        for x in 0..128 {
            let dx = (28. - (x as f32 + 0.5).min(127.5 - x as f32)).max(0.);
            let dy = (28. - (y as f32 + 0.5).min(127.5 - y as f32)).max(0.);
            let alpha = (28.5 - (dx * dx + dy * dy).sqrt()).clamp(0., 1.);
            for channel in &mut pixels[(y * 128 + x) * 4..(y * 128 + x) * 4 + 4] {
                *channel = (*channel as f32 * alpha).round() as u8;
            }
        }
    }
    Ok(Arc::new(Cover {
        width: 128,
        height: 128,
        pixels,
    }))
}
fn thumbnail(
    reference: &IRandomAccessStreamReference,
    context: &Context<'_>,
) -> Result<Arc<Cover>> {
    let stream = receive!(context, reference.OpenReadAsync());
    let _stream_lifetime = CloseOnDrop::new(&stream);
    let bytes = read(&stream.cast()?, context)?;
    decode(&bytes, context)
}
fn resolve(request: &Request, context: &Context<'_>) -> Extra {
    let mut extra = Extra::default();
    if let Some(reference) = &request.thumbnail {
        extra.cover = reference
            .resolve()
            .and_then(|r| thumbnail(&r, context))
            .ok();
    }
    if extra.cover.is_some() && context.current() {
        let mut slot = context.state.slot.lock().unwrap_or_else(|e| e.into_inner());
        if context.current() {
            slot.output = Some((request.key.clone(), extra.clone()));
        }
    }
    if request.netease && !request.title.is_empty() && !request.artist.is_empty() {
        // Let quick page/track transitions settle before sending search terms.
        for _ in 0..30 {
            if !context.current() {
                return extra;
            }
            thread::sleep(Duration::from_millis(10));
        }
        if let Ok(Some(song)) = song(&request.title, &request.artist, context) {
            extra.duration = number(&song, "duration").max(number(&song, "dt"));
            if extra.duration > 360_000_000 {
                extra.duration = 0;
            }
            if extra.cover.is_none() {
                if let Ok(album) = song
                    .GetNamedObject(&HSTRING::from("album"))
                    .or_else(|_| song.GetNamedObject(&HSTRING::from("al")))
                {
                    let url = string(&album, "picUrl");
                    // Only the provider's image CDN is an accepted network fallback.
                    if let Ok(uri) =
                        Uri::CreateUri(&HSTRING::from(url.replace("http://", "https://")))
                    {
                        let host = uri.Host().unwrap_or_default().to_string().to_lowercase();
                        if host.ends_with(".music.126.net") {
                            let url = format!(
                                "{}?param=256y256",
                                uri.AbsoluteUri()
                                    .unwrap_or_default()
                                    .to_string()
                                    .split('?')
                                    .next()
                                    .unwrap_or("")
                            );
                            extra.cover = download(&url, context)
                                .and_then(|b| decode(&b, context))
                                .ok();
                        }
                    }
                }
            }
        }
    }
    extra
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_decode_masks_corners_and_cancellation_rejects_stale_work() {
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED).unwrap();
        }
        let state = State {
            http_times: Mutex::default(),
            stats: Arc::default(),
            epoch: AtomicU64::new(1),
            slot: Mutex::new(Slot::default()),
            ready: Condvar::new(),
        };
        let context = Context {
            state: &state,
            epoch: 1,
            deadline: Instant::now() + Duration::from_secs(3),
        };
        let cover = decode(include_bytes!("../tests/cover.png"), &context).unwrap();
        assert_eq!(
            (cover.width, cover.height, cover.pixels.len()),
            (128, 128, 65536)
        );
        assert_eq!(cover.pixels[3], 0);
        assert_eq!(
            &cover.pixels[(64 * 128 + 64) * 4..(64 * 128 + 64) * 4 + 4],
            &[80, 40, 220, 255]
        );
        assert!(decode(b"invalid image", &context).is_err());
        state.epoch.store(2, Ordering::Release);
        assert!(decode(include_bytes!("../tests/cover.png"), &context).is_err());
        unsafe {
            RoUninitialize();
        }
    }
    #[test]
    fn job_result_is_keyed_and_cancel_releases_slot() {
        let mut enrichment = Enricher::new(Arc::default()).unwrap();
        let request = |key: &str| Request {
            key: key.into(),
            title: String::new(),
            artist: String::new(),
            netease: false,
            thumbnail: None,
        };
        enrichment.request(request("old"));
        enrichment.cancel();
        enrichment.request(request("new"));
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if enrichment.request(request("new")).is_some() {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let slot = enrichment.state.slot.lock().unwrap();
        assert_eq!(slot.output.as_ref().unwrap().0, "new");
        drop(slot);
        for i in 0..12 {
            let key = format!("cache-{i}");
            let deadline = Instant::now() + Duration::from_secs(3);
            while enrichment.request(request(&key)).is_none() {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
        }
        assert_eq!(enrichment.state.stats.entries.load(Ordering::Relaxed), 8);
        enrichment.cancel();
        let slot = enrichment.state.slot.lock().unwrap();
        assert!(slot.output.is_none() && slot.job.is_none());
    }

    #[test]
    #[ignore = "Opt-in network check against NetEase, without player thumbnail"]
    fn live_provider_downloads_fallback_cover_and_duration() {
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED).unwrap();
        }
        let state = State {
            http_times: Mutex::default(),
            stats: Arc::default(),
            epoch: AtomicU64::new(1),
            slot: Mutex::new(Slot::default()),
            ready: Condvar::new(),
        };
        let context = Context {
            state: &state,
            epoch: 1,
            deadline: Instant::now() + Duration::from_secs(8),
        };
        let extra = resolve(
            &Request {
                key: "provider-fixture".into(),
                title: "大鱼 (唱片版)".into(),
                artist: "周深".into(),
                netease: true,
                thumbnail: None,
            },
            &context,
        );
        assert!(extra.duration > 0);
        assert_eq!(extra.cover.as_ref().map(|c| c.pixels.len()), Some(65536));
        println!(
            "NetEase fallback: duration={}ms, cover=128x128, HTTP requests={}",
            extra.duration,
            state.stats.requests.load(Ordering::Relaxed)
        );
        unsafe {
            RoUninitialize();
        }
    }
}
