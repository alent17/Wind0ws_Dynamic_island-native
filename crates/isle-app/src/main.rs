#![windows_subsystem = "windows"]
mod accessibility;
mod artwork;
mod clock;
mod configuration;
mod frame_timer;
mod media;
mod players;
mod render;
mod spectrum;
mod system_audio;
mod weather;
mod weather_settings;
const WM_MOUSELEAVE: u32 = 0x02A3;
use isle_ui::{geometry::*, model::*};
use render::Renderer;
use std::{
    cell::RefCell,
    collections::VecDeque,
    time::{Duration, Instant},
};
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{Com::*, LibraryLoader::*},
        UI::{Accessibility::*, HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
};
#[derive(Debug)]
enum Event {
    Players(usize, isize),
    PlayersUpdated,
    ConfigSaved,
    Weather,
    City(usize, isize),
    Spectrum,
    Audio,
    Media,
    Paint,
    Tick,
    Move(i32, i32),
    Down(i32, i32),
    Up(i32, i32),
    Wheel(i16),
    Key(u32),
    Leave,
    Cancel,
    Resize,
    Dpi(u32, RECT),
    Visibility(bool),
    Preferences,
    Access(u32, usize, u32, u16),
    Diagnostic,
    Close,
}
thread_local! {static EVENTS:RefCell<VecDeque<Event>>=const{RefCell::new(VecDeque::new())};}
thread_local! {static ACCESSIBLE:RefCell<Option<IAccessible>>=const{RefCell::new(None)};}
fn enqueue(e: Event) {
    EVENTS.with(|q| q.borrow_mut().push_back(e));
}
fn coordinates(lp: LPARAM) -> (i32, i32) {
    (
        (lp.0 as u16 as i16) as i32,
        ((lp.0 >> 16) as u16 as i16) as i32,
    )
}
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        players::COMMAND => {
            enqueue(Event::Players(wp.0, lp.0));
            LRESULT(0)
        }
        players::UPDATED => {
            enqueue(Event::PlayersUpdated);
            LRESULT(0)
        }
        configuration::UPDATED => {
            enqueue(Event::ConfigSaved);
            LRESULT(0)
        }
        weather::UPDATED => {
            enqueue(Event::Weather);
            LRESULT(0)
        }
        weather_settings::COMMAND => {
            enqueue(Event::City(wp.0, lp.0));
            LRESULT(0)
        }
        spectrum::UPDATED => {
            enqueue(Event::Spectrum);
            LRESULT(0)
        }
        system_audio::UPDATED => {
            enqueue(Event::Audio);
            LRESULT(0)
        }
        media::UPDATED => {
            enqueue(Event::Media);
            LRESULT(0)
        }
        WM_GETOBJECT if lp.0 as i32 == UiaRootObjectId => ACCESSIBLE.with(|a| {
            a.borrow()
                .as_ref()
                .and_then(|a| UiaProviderFromIAccessible(a, 0, 0).ok())
                .map(|provider| UiaReturnRawElementProvider(hwnd, wp, lp, &provider))
                .unwrap_or(LRESULT(0))
        }),
        0x803c => {
            enqueue(Event::Diagnostic);
            LRESULT(0)
        }
        WM_GETOBJECT if lp.0 as i32 == OBJID_CLIENT.0 => ACCESSIBLE.with(|a| {
            a.borrow()
                .as_ref()
                .map(|a| LresultFromObject(&IAccessible::IID, wp, a))
                .unwrap_or(LRESULT(0))
        }),
        accessibility::INVOKE | accessibility::FOCUS | accessibility::VALUE => {
            enqueue(Event::Access(
                msg,
                wp.0,
                ((lp.0 as u64) >> 16) as u32,
                lp.0 as u16,
            ));
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(hwnd, &mut ps);
            EndPaint(hwnd, &ps);
            enqueue(Event::Paint);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_TIMER => {
            enqueue(Event::Tick);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = coordinates(lp);
            enqueue(Event::Move(x, y));
            let mut track = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut track);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            enqueue(Event::Leave);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = coordinates(lp);
            SetCapture(hwnd);
            enqueue(Event::Down(x, y));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = coordinates(lp);
            enqueue(Event::Up(x, y));
            let _ = ReleaseCapture();
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            if lp.0 != hwnd.0 {
                enqueue(Event::Cancel);
            }
            LRESULT(0)
        }
        WM_CANCELMODE => {
            enqueue(Event::Cancel);
            let _ = ReleaseCapture();
            LRESULT(0)
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            enqueue(Event::Wheel((wp.0 >> 16) as u16 as i16));
            LRESULT(0)
        }
        WM_KEYDOWN => {
            enqueue(Event::Key(wp.0 as u32));
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if lp.0 != 0 {
                enqueue(Event::Dpi(wp.0 as u16 as u32, *(lp.0 as *const RECT)));
            }
            LRESULT(0)
        }
        WM_SHOWWINDOW => {
            enqueue(Event::Visibility(wp.0 != 0));
            LRESULT(0)
        }
        WM_SIZE => {
            if wp.0 == SIZE_MINIMIZED as usize {
                enqueue(Event::Visibility(false));
            } else if IsWindowVisible(hwnd).as_bool() {
                enqueue(Event::Visibility(true));
            }
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            enqueue(Event::Resize);
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            enqueue(Event::Preferences);
            LRESULT(0)
        }
        WM_TIMECHANGE => {
            enqueue(Event::Paint);
            LRESULT(0)
        }
        WM_CLOSE => {
            enqueue(Event::Close);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
struct App {
    weather: weather::Service,
    settings: Option<weather_settings::Settings>,
    player_dialog: Option<players::Dialog>,
    configuration: configuration::Service,
    saving_window: Option<isize>,
    audio: Option<system_audio::AudioService>,
    spectrum: Option<spectrum::SpectrumService>,
    media: Option<media::MediaService>,
    media_error: Option<String>,
    window: HWND,
    always_on_top: bool,
    accessible: accessibility::Shared,
    renderer: Option<Renderer>,
    suspended: bool,
    pending_completion: bool,
    total_frames: u64,
    font_family: &'static str,
    test_dpi: Option<u32>,
    test_work: Option<(u32, u32)>,
    test_monitor: Option<String>,
    model: Model,
    edge_position: u8,
    scale: f32,
    last: Instant,
    start: Instant,
    hover: Option<Hit>,
    down: Option<(Point, Hit, f32)>,
    dragged: bool,
    interval: u32,
    region: Vec<POINT>,
    exit_after: Option<f64>,
    log: Option<String>,
    scripted: bool,
    last_script: u64,
    frames_ms: Vec<f64>,
    intervals_ms: Vec<f64>,
    last_present: Option<Instant>,
    frame_timer: frame_timer::FrameTimer,
    reduced_override: Option<bool>,
}
impl App {
    unsafe fn position(&mut self) -> Result<()> {
        self.position_for(None, None)
    }
    unsafe fn position_for(&mut self, dpi: Option<u32>, suggested: Option<RECT>) -> Result<()> {
        let monitor = if let Some(name) = &self.test_monitor {
            unsafe extern "system" fn select(
                monitor: HMONITOR,
                _: HDC,
                _: *mut RECT,
                data: LPARAM,
            ) -> BOOL {
                let selected = &mut *(data.0 as *mut (&str, HMONITOR));
                let mut info = MONITORINFOEXW::default();
                info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
                if GetMonitorInfoW(monitor, &mut info.monitorInfo).as_bool() {
                    let end = info
                        .szDevice
                        .iter()
                        .position(|v| *v == 0)
                        .unwrap_or(info.szDevice.len());
                    if String::from_utf16_lossy(&info.szDevice[..end]) == selected.0 {
                        selected.1 = monitor;
                    }
                }
                BOOL(1)
            }
            let mut selected = (name.as_str(), HMONITOR(0));
            EnumDisplayMonitors(
                None,
                None,
                Some(select),
                LPARAM((&mut selected as *mut (&str, HMONITOR)) as isize),
            )
            .ok()?;
            if selected.1 .0 == 0 {
                return Err(E_INVALIDARG.into());
            }
            selected.1
        } else if let Some(r) = suggested {
            MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST)
        } else {
            MonitorFromWindow(self.window, MONITOR_DEFAULTTONEAREST)
        };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(monitor, &mut info).ok()?;
        let dpi = self
            .test_dpi
            .or(dpi)
            .unwrap_or_else(|| GetDpiForWindow(self.window))
            .max(96);
        let r = info.rcWork;
        let mut work = Rect {
            x: r.left as f32,
            y: r.top as f32,
            w: (r.right - r.left) as f32,
            h: (r.bottom - r.top) as f32,
        };
        if let Some((w, h)) = self.test_work {
            work.w = work.w.min(w as f32);
            work.h = work.h.min(h as f32);
        }
        let (mut bounds, scale) = window_placement(work, dpi, HOST, self.model.edge);
        let along = f32::from(self.edge_position.min(100)) / 100.;
        match self.model.edge {
            Edge::Top | Edge::Bottom => bounds.x = work.x + (work.w - bounds.w) * along,
            Edge::Left | Edge::Right => bounds.y = work.y + (work.h - bounds.h) * along,
        }
        self.scale = scale;
        SetWindowPos(
            self.window,
            if self.always_on_top {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            },
            bounds.x as i32,
            bounds.y as i32,
            bounds.w as i32,
            bounds.h as i32,
            SWP_NOACTIVATE,
        )?;
        Ok(())
    }
    unsafe fn apply_topmost(&self) -> Result<()> {
        SetWindowPos(
            self.window,
            if self.always_on_top {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            },
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )?;
        Ok(())
    }
    unsafe fn region(&mut self) -> Result<()> {
        let points: Vec<POINT> = self
            .model
            .outline()
            .iter()
            .map(|p| POINT {
                x: (p.x * self.scale).round() as i32,
                y: (p.y * self.scale).round() as i32,
            })
            .collect();
        if self.region.len() == points.len()
            && self
                .region
                .iter()
                .zip(&points)
                .all(|(a, b)| a.x == b.x && a.y == b.y)
        {
            return Ok(());
        }
        let region = CreatePolygonRgn(&points, WINDING);
        if region.0 == 0 {
            return Err(Error::from_win32());
        }
        if SetWindowRgn(self.window, region, false) == 0 {
            DeleteObject(region);
            return Err(Error::from_win32());
        }
        self.region = points;
        Ok(())
    }
    unsafe fn update_accessibility(&self) -> Result<()> {
        let mut window = RECT::default();
        GetWindowRect(self.window, &mut window)?;
        let transform = |r: Rect| Rect {
            x: window.left as f32 + r.x * self.scale,
            y: window.top as f32 + r.y * self.scale,
            w: r.w * self.scale,
            h: r.h * self.scale,
        };
        let origin = self.model.origin();
        let bounds = transform(Rect {
            x: origin.x,
            y: origin.y,
            w: self.model.width.value,
            h: self.model.height.value,
        });
        let mut nodes = vec![];
        if !self.suspended
            && self.model.expanded
            && self.model.width.value > 250.
            && (self.model.height.value - self.model.height.target).abs() < 35.
        {
            for (hit, mut rect) in self.model.controls() {
                if matches!(hit, Hit::Tool(_)) {
                    let bar = self.model.bar();
                    let right = (rect.x + rect.w).min(bar.x + bar.w);
                    rect.x = rect.x.max(bar.x);
                    rect.w = (right - rect.x).max(0.);
                }
                nodes.push(accessibility::Node {
                    key: if let Hit::Device(index) = hit {
                        self.model
                            .audio
                            .as_ref()
                            .and_then(|a| a.devices.get(index))
                            .map(|d| d.id.clone())
                            .unwrap_or_default()
                    } else {
                        format!("{hit:?}")
                    },
                    enabled: self.model.enabled(hit),
                    hit,
                    name: match hit {
                        Hit::Device(index) => self
                            .model
                            .audio
                            .as_ref()
                            .and_then(|a| a.devices.get(index))
                            .map(|d| d.name.clone())
                            .unwrap_or_default(),
                        Hit::Back if self.model.device_menu => "返回音量".into(),
                        Hit::Mute if self.model.audio.as_ref().is_some_and(|a| a.muted) => {
                            "取消静音".into()
                        }
                        _ => accessibility::label(hit, self.model.playing).to_string(),
                    },
                    rect: transform(rect),
                });
            }
        }
        let mut state = self.accessible.lock().map_err(|_| Error::from(E_FAIL))?;
        let reordered = state
            .nodes
            .iter()
            .map(|n| &n.key)
            .ne(nodes.iter().map(|n| &n.key));
        let focus_changed = state.focus != self.model.focus;
        let volume = self.model.volume.round() as u32;
        let volume_changed = state.volume != volume;
        if reordered {
            state.revision = state.revision.wrapping_add(1);
        }
        state.visible = !self.suspended;
        state.nodes = nodes;
        state.bounds = bounds;
        state.outline = self
            .model
            .outline()
            .into_iter()
            .map(|p| Point {
                x: window.left as f32 + p.x * self.scale,
                y: window.top as f32 + p.y * self.scale,
            })
            .collect();
        state.focus = self.model.focus;
        state.volume = volume;
        let volume_id = state
            .nodes
            .iter()
            .position(|node| node.hit == Hit::Volume)
            .map(|index| index as i32 + 1);
        let focus_id = state
            .nodes
            .iter()
            .position(|n| Some(n.hit) == state.focus)
            .map(|i| i as i32 + 1)
            .unwrap_or(0);
        drop(state);
        if reordered {
            NotifyWinEvent(EVENT_OBJECT_REORDER, self.window, OBJID_CLIENT.0, 0);
        }
        if focus_changed {
            NotifyWinEvent(EVENT_OBJECT_FOCUS, self.window, OBJID_CLIENT.0, focus_id);
        }
        if volume_changed {
            if let Some(volume_id) = volume_id {
                NotifyWinEvent(
                    EVENT_OBJECT_VALUECHANGE,
                    self.window,
                    OBJID_CLIENT.0,
                    volume_id,
                );
            }
        }
        Ok(())
    }
    unsafe fn redraw(&mut self) -> Result<()> {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;
        let timer_was_running = self.model.timer_deadline.is_some();
        if self.spectrum.is_some() && !self.spectrum_visible() {
            self.model.spectrum = Some(SpectrumVisual::default());
        }
        self.model.step(dt, (now - self.start).as_secs_f64());
        if self.suspended {
            if timer_was_running && self.model.timer_deadline.is_none() {
                self.pending_completion = true;
            }
            if self.model.expanded {
                self.model.toggle();
            }
        }
        if self.suspended {
            self.sync_timer()?;
            return Ok(());
        }
        self.region()?;
        if self.renderer.is_none() {
            self.renderer = Some(Renderer::new(self.window, self.scale)?);
        }
        let start = Instant::now();
        if let Err(error) = self.renderer.as_mut().unwrap().draw(
            &self.model,
            self.hover,
            self.down.filter(|_| !self.dragged).map(|(_, h, _)| h),
        ) {
            eprintln!("Rendering failed, rebuilding device: {error}");
            // Release the previous composition target before binding a new one
            // to the same HWND, including the device-loss recovery path.
            self.renderer = None;
            self.renderer = Some(Renderer::new(self.window, self.scale)?);
            self.renderer.as_mut().unwrap().draw(
                &self.model,
                self.hover,
                self.down.filter(|_| !self.dragged).map(|(_, h, _)| h),
            )?;
        }
        self.total_frames += 1;
        self.font_family = self.renderer.as_ref().unwrap().font_family;
        self.model.title_overflow = self.renderer.as_ref().unwrap().title_overflow;
        let presented = Instant::now();
        if self.log.is_some() && self.model.continuous() && self.start.elapsed().as_secs() >= 5 {
            if let Some(last) = self.last_present {
                if self.intervals_ms.len() < 36000 {
                    self.intervals_ms
                        .push((presented - last).as_secs_f64() * 1000.);
                }
            }
            self.last_present = Some(presented);
        } else {
            self.last_present = None;
        }
        if self.log.is_some() && self.frames_ms.len() < 36000 {
            self.frames_ms.push(start.elapsed().as_secs_f64() * 1000.);
        }
        self.update_accessibility()?;
        self.sync_timer()
    }
    fn continuous(&self) -> bool {
        !self.suspended && self.model.continuous()
    }
    unsafe fn sync_timer(&mut self) -> Result<()> {
        if self.settings.is_none() {
            let job =
                if !self.suspended && self.model.expanded && self.model.page() == Page::Weather {
                    self.model.weather.city.clone().map(weather::Job::Forecast)
                } else {
                    None
                };
            self.weather.request(job);
        }
        if let Some(service) = &self.spectrum {
            let active = self.spectrum_visible();
            service.active(active);
            if !active {
                self.model.spectrum = Some(SpectrumVisual::default());
            }
        }
        if let Some(audio) = &self.audio {
            audio.active(
                !self.suspended && self.model.expanded && self.model.page() == Page::Volume,
            );
        }
        if let Some(service) = &self.media {
            service.set_active(
                !self.suspended && (!self.model.expanded || self.model.page() == Page::Music),
            );
        }
        let desired = if self.suspended {
            if self.model.timer_deadline.is_some() || self.exit_after.is_some() {
                1000
            } else {
                0
            }
        } else if self.model.continuous() {
            0
        } else if self.model.timer_deadline.is_some()
            || self.scripted
            || self.exit_after.is_some()
            || self.model.progress_tick()
        {
            1000
        } else if self.model.expanded && matches!(self.model.page(), Page::Clock | Page::Weather) {
            60000
                - (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
                    % 60000) as u32
        } else {
            0
        };
        if desired != self.interval {
            if self.interval > 0 {
                let _ = KillTimer(self.window, 1);
            }
            if desired > 0 && SetTimer(self.window, 1, desired, None) == 0 {
                return Err(Error::from_win32());
            }
            self.interval = desired;
        }
        Ok(())
    }
    fn point(&self, x: i32, y: i32) -> Point {
        Point {
            x: x as f32 / self.scale,
            y: y as f32 / self.scale,
        }
    }
    fn spectrum_visible(&self) -> bool {
        !self.suspended
            && (!self.model.expanded || self.model.page() == Page::Music)
            && self.model.media.as_ref().is_none_or(|media| media.playing)
    }
    unsafe fn action(&mut self, hit: Hit) {
        if matches!(hit, Hit::WeatherSettings | Hit::Tool(3)) {
            if let Some(settings) = &self.settings {
                if IsWindowEnabled(self.window).as_bool() {
                    SetForegroundWindow(settings.hwnd);
                }
            } else {
                match weather_settings::Settings::new(
                    self.window,
                    &self.configuration.controls,
                    &self.configuration.appearance,
                ) {
                    Ok(settings) => {
                        self.weather.request(None);
                        if let Some(error) = &self.configuration.load_error {
                            settings.message(error);
                        }
                        settings.saving(self.configuration.busy());
                        self.settings = Some(settings);
                    }
                    Err(_) => {
                        MessageBoxW(self.window, w!("无法打开天气设置"), w!("Isle"), MB_OK);
                    }
                }
            }
            return;
        }
        if !self.model.enabled(hit) {
            return;
        }
        if let (Some(service), Some(audio)) = (&self.audio, &self.model.audio) {
            match hit {
                Hit::Mute => {
                    service.command(
                        audio.device.id.clone(),
                        system_audio::Action::Mute(!audio.muted),
                    );
                    return;
                }
                Hit::Device(index) => {
                    if let Some(device) = audio.devices.get(index) {
                        service.command(
                            audio.device.id.clone(),
                            system_audio::Action::Device(device.id.clone()),
                        );
                    }
                    self.model.back();
                    return;
                }
                _ => {}
            }
        }
        if let Some(service) = &self.media {
            let action = match hit {
                Hit::Play => Some(media::Action::Toggle),
                Hit::Previous => Some(media::Action::Previous),
                Hit::Next => Some(media::Action::Next),
                _ => None,
            };
            if let Some(action) = action {
                if let Some(snapshot) = &self.model.media {
                    service.control(snapshot.session, action);
                }
                return;
            }
        }
        match hit {
            Hit::Tool(2) => {
                MessageBoxW(
                    self.window,
                    w!("此阶段验证原生绘制。独立悬浮播放器将在业务接入阶段迁移。"),
                    w!("Isle 原生原型"),
                    MB_OK,
                );
            }
            Hit::Tool(4) => {
                self.model.toggle();
            }
            _ => self.model.activate(hit),
        }
    }
    unsafe fn handle(&mut self, event: Event) -> Result<bool> {
        if self.suspended
            && !matches!(
                event,
                Event::Tick
                    | Event::Media
                    | Event::Audio
                    | Event::Spectrum
                    | Event::Weather
                    | Event::Players(_, _)
                    | Event::PlayersUpdated
                    | Event::ConfigSaved
                    | Event::City(_, _)
                    | Event::Diagnostic
                    | Event::Close
                    | Event::Visibility(_)
                    | Event::Dpi(_, _)
                    | Event::Resize
                    | Event::Preferences
            )
        {
            return Ok(true);
        }
        let mut changed = true;
        let previous_volume = self.model.volume;
        let audio_update = matches!(event, Event::Audio);
        match event {
            Event::PlayersUpdated => {
                if let Some(dialog) = &mut self.player_dialog {
                    dialog.take_update();
                }
                changed = false;
            }
            Event::Players(action, hwnd) => {
                if self
                    .player_dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.hwnd.0 == hwnd)
                {
                    match action {
                        players::CLOSE => self.player_dialog = None,
                        players::SAVE
                            if !self.configuration.busy()
                                && self.player_dialog.as_ref().is_some_and(|d| d.ready()) =>
                        {
                            let selection = self.player_dialog.as_ref().unwrap().selection();
                            if self
                                .configuration
                                .save(configuration::Edit::Players(selection))
                            {
                                self.player_dialog.as_mut().unwrap().saving(true);
                                if let Some(settings) = &self.settings {
                                    settings.saving(true);
                                }
                            }
                        }
                        _ => {
                            if let Some(dialog) = &mut self.player_dialog {
                                dialog.action(action);
                            }
                        }
                    }
                }
                changed = false;
            }
            Event::City(action, hwnd) => {
                if self.settings.as_ref().is_some_and(|s| s.hwnd.0 == hwnd) {
                    match action {
                        weather_settings::SEARCH if !self.configuration.busy() => {
                            let settings = self.settings.as_mut().unwrap();
                            let query = settings.query();
                            self.weather.request(None);
                            if (2..=80).contains(&query.chars().count()) {
                                settings.loading();
                                self.weather.request(Some(weather::Job::Search(query)));
                            } else {
                                settings.message("请输入 2–80 个字符");
                            }
                        }
                        weather_settings::APPLY if !self.configuration.busy() => {
                            if let Some(city) = self.settings.as_ref().and_then(|s| s.selected()) {
                                if self.configuration.save(configuration::Edit::City(city)) {
                                    self.saving_window = Some(hwnd);
                                    self.settings.as_ref().unwrap().saving(true);
                                }
                            }
                        }
                        weather_settings::APPLY_CONTROLS if !self.configuration.busy() => {
                            let controls = self.settings.as_ref().unwrap().controls();
                            if self
                                .configuration
                                .save(configuration::Edit::Controls(controls))
                            {
                                self.saving_window = Some(hwnd);
                                self.settings.as_ref().unwrap().saving(true);
                            }
                        }
                        weather_settings::APPLY_APPEARANCE if !self.configuration.busy() => {
                            if let Some(appearance) = self.settings.as_ref().unwrap().appearance() {
                                if self
                                    .configuration
                                    .save(configuration::Edit::Appearance(appearance))
                                {
                                    self.saving_window = Some(hwnd);
                                    self.settings.as_ref().unwrap().saving(true);
                                }
                            } else if let Some(settings) = &self.settings {
                                settings.message("沿边位置需为 0–100，颜色需为 #RRGGBB");
                            }
                        }
                        weather_settings::PLAYERS if !self.configuration.busy() => {
                            if let Some(dialog) = &self.player_dialog {
                                if IsWindowEnabled(self.window).as_bool() {
                                    SetForegroundWindow(dialog.hwnd);
                                }
                            } else {
                                match players::Dialog::new(
                                    self.window,
                                    &self.configuration.selection,
                                ) {
                                    Ok(dialog) => self.player_dialog = Some(dialog),
                                    Err(_) => self
                                        .settings
                                        .as_ref()
                                        .unwrap()
                                        .message("无法打开播放器选择"),
                                }
                            }
                        }
                        weather_settings::CLOSE => {
                            self.player_dialog = None;
                            self.settings = None;
                            self.weather.request(None);
                        }
                        _ => {}
                    }
                }
            }
            Event::ConfigSaved => {
                if let Some(outcome) = self.configuration.take() {
                    let same_window = self
                        .settings
                        .as_ref()
                        .is_some_and(|s| Some(s.hwnd.0) == self.saving_window);
                    self.saving_window = None;
                    match outcome.result {
                        Ok(()) => match outcome.edit {
                            configuration::Edit::City(city) => {
                                self.model.weather = isle_core::weather::View {
                                    city: Some(city),
                                    ..Default::default()
                                };
                                if same_window {
                                    self.player_dialog = None;
                                    self.settings = None;
                                    self.weather.request(None);
                                    if !self.suspended {
                                        self.model.switch(Page::Weather);
                                    }
                                } else if let Some(settings) = &self.settings {
                                    settings.saving(false);
                                    settings.message("城市已保存");
                                }
                            }
                            configuration::Edit::Players(selection) => {
                                if let Some(media) = &self.media {
                                    media.set_selection(selection);
                                }
                                if self.model.media.is_some() {
                                    self.model.media = Some(isle_core::MediaSnapshot::default());
                                }
                                self.down = None;
                                let _ = ReleaseCapture();
                                if let Some(dialog) = &mut self.player_dialog {
                                    dialog.saving(false);
                                    dialog.message("播放器设置已保存");
                                }
                                if let Some(settings) = &self.settings {
                                    settings.saving(false);
                                    settings.message("播放器设置已保存");
                                }
                            }
                            configuration::Edit::Controls(controls) => {
                                self.model.time_zone = controls.time_zone.clone();
                                self.always_on_top = controls.always_on_top;
                                unsafe {
                                    self.apply_topmost()?;
                                }
                                self.model.reduced = self
                                    .reduced_override
                                    .unwrap_or_else(|| system_reduced_motion())
                                    || !controls.animations
                                    || controls.reduced;
                                self.model.set_tool_mask(controls.mask());
                                if let Some(settings) = &self.settings {
                                    settings.saving(false);
                                    settings.message("设置已应用");
                                }
                            }
                            configuration::Edit::Appearance(appearance) => {
                                self.edge_position = appearance.edge_position;
                                self.model.attached = appearance.style == "edge";
                                self.model.edge = match appearance.edge.as_str() {
                                    "right" => Edge::Right,
                                    "bottom" => Edge::Bottom,
                                    "left" => Edge::Left,
                                    _ => Edge::Top,
                                };
                                self.model.compact_length = appearance.compact_length;
                                self.model.collapsed_shoulder_radius =
                                    appearance.collapsed_shoulder_radius;
                                self.model.expanded_shoulder_radius =
                                    appearance.expanded_shoulder_radius;
                                self.model.expanded_corner_radius =
                                    appearance.expanded_corner_radius;
                                self.model.background_color =
                                    rgb_color(&appearance.floating_fill_color);
                                self.model.use_album_color = appearance.floating_use_album_color;
                                self.model.retarget();
                                let _ = self.position();
                                if let Some(settings) = &self.settings {
                                    settings.saving(false);
                                    settings.message("外观已应用");
                                }
                            }
                        },
                        Err(error) => {
                            if let Some(dialog) = &mut self.player_dialog {
                                dialog.saving(false);
                                dialog.message(&error);
                            }
                            if let Some(settings) = &self.settings {
                                settings.saving(false);
                                settings.message(&error);
                            }
                        }
                    }
                }
                changed = !self.suspended;
            }
            Event::Weather => {
                if let Some(output) = self.weather.take() {
                    match output {
                        weather::Output::Forecast(city, data, failed) => {
                            if self.model.weather.city.as_ref() == Some(&city) {
                                self.model.weather.data = data;
                                self.model.weather.failed = failed;
                            }
                        }
                        weather::Output::Search(cities, failed) => {
                            if let Some(settings) = &mut self.settings {
                                settings.results(cities, failed);
                                settings.saving(self.configuration.busy());
                            }
                        }
                    }
                }
                changed =
                    !self.suspended && self.model.expanded && self.model.page() == Page::Weather;
            }
            Event::Spectrum => {
                if let Some(service) = &self.spectrum {
                    let frame = service.take();
                    if self.spectrum_visible() {
                        if let Some(spectrum) = &mut self.model.spectrum {
                            spectrum.target = frame.bars;
                            spectrum.failed = frame.failed;
                        }
                    }
                }
                // Interpolated bars use the existing frame timer; incoming data must
                // not add a second stream of presents between those frames.
                changed = self.spectrum_visible() && self.model.reduced;
            }
            Event::Audio => {
                if let Some(service) = &self.audio {
                    let audio = service.take();
                    if self
                        .model
                        .audio
                        .as_ref()
                        .is_some_and(|old| old.device.id != audio.device.id)
                        && self.down.take().is_some()
                    {
                        let _ = ReleaseCapture();
                        self.dragged = false;
                    }
                    if self.down.is_none_or(|(_, hit, _)| hit != Hit::Volume) {
                        self.model.volume = audio.volume as f32;
                    }
                    if self
                        .model
                        .audio
                        .as_ref()
                        .is_some_and(|old| old.devices != audio.devices)
                    {
                        self.model.device_offset = 0;
                        self.model.focus = None;
                    }
                    self.model.audio = Some(audio);
                }
                changed =
                    !self.suspended && self.model.expanded && self.model.page() == Page::Volume;
            }
            Event::Media => {
                if let Some(service) = &self.media {
                    let update = service.take();
                    if self.model.media.as_ref().is_none_or(|old| {
                        old.title != update.snapshot.title || old.artist != update.snapshot.artist
                    }) {
                        self.model.title_started = self.start.elapsed().as_secs_f64();
                    }
                    self.model.playing = update.snapshot.playing;
                    self.model.media = Some(update.snapshot);
                    self.model.media_failed = update.error.is_some();
                    self.media_error = update.error;
                }
                changed =
                    !self.suspended && (!self.model.expanded || self.model.page() == Page::Music);
            }
            Event::Close => return Ok(false),
            Event::Diagnostic => {
                self.report();
                return Ok(true);
            }
            Event::Access(message, child, revision, value) => {
                let hit = {
                    let state = self.accessible.lock().map_err(|_| Error::from(E_FAIL))?;
                    if revision != state.revision || !state.visible {
                        return Ok(true);
                    }
                    if child == 0 {
                        Some(Hit::Blank)
                    } else {
                        state.nodes.get(child - 1).map(|n| n.hit)
                    }
                };
                if let Some(hit) = hit {
                    match message {
                        accessibility::INVOKE => self.action(hit),
                        accessibility::FOCUS => {
                            SetFocus(self.window);
                            self.model.focus = if hit == Hit::Blank { None } else { Some(hit) };
                        }
                        accessibility::VALUE
                            if hit == Hit::Volume && value <= 100 && self.model.enabled(hit) =>
                        {
                            self.model.volume = value as f32
                        }
                        _ => {}
                    }
                }
            }
            Event::Paint => {}
            Event::Cancel => {
                self.down = None;
                self.dragged = false;
            }
            Event::Tick => {
                let elapsed = self.start.elapsed().as_secs_f64();
                if self.exit_after.is_some_and(|limit| elapsed >= limit) {
                    return Ok(false);
                }
                changed = self.model.continuous()
                    || self.model.progress_tick()
                    || self.model.timer_deadline.is_some()
                    || self.scripted
                    || (self.model.expanded
                        && matches!(self.model.page(), Page::Clock | Page::Weather));
                if self.scripted && !self.suspended {
                    let step = (elapsed / 0.35) as u64;
                    if step != self.last_script {
                        self.last_script = step;
                        match step % 6 {
                            0 => self.model.switch(Page::Music),
                            1 => self.model.switch(Page::Volume),
                            2 => self.model.switch(Page::Timer),
                            3 => self.model.switch(Page::Weather),
                            4 => self.model.toggle(),
                            _ => self.model.switch(Page::Clock),
                        }
                    }
                }
            }
            Event::Preferences => {
                let old_scale = self.scale;
                self.position()?;
                if old_scale != self.scale {
                    self.renderer = None;
                    self.region.clear();
                }
                self.model.reduced = self
                    .reduced_override
                    .unwrap_or_else(|| system_reduced_motion())
                    || !self.configuration.controls.animations
                    || self.configuration.controls.reduced;
                self.model.retarget();
            }
            Event::Visibility(visible) => {
                let suspended = !visible;
                if suspended == self.suspended {
                    return Ok(true);
                }
                self.suspended = suspended;
                self.last_present = None;
                self.down = None;
                self.hover = None;
                if suspended {
                    self.frame_timer.cancel();
                    if self.model.expanded {
                        self.model.toggle();
                    }
                    self.renderer = None;
                    self.update_accessibility()?;
                    self.sync_timer()?;
                    return Ok(true);
                }
                self.position()?;
                self.region.clear();
                self.last = Instant::now();
                if self.pending_completion {
                    self.pending_completion = false;
                    self.model.switch(Page::Timer);
                }
            }
            Event::Dpi(dpi, rect) => {
                self.position_for(Some(dpi), Some(rect))?;
                self.renderer = None;
                self.region.clear();
                self.model.retarget();
            }
            Event::Resize => {
                self.position()?;
                self.renderer = None;
                self.region.clear();
            }
            Event::Leave => {
                self.hover = None;
                if !self.model.expanded && self.model.hovered {
                    self.model.hovered = false;
                    self.model.retarget();
                }
            }
            Event::Move(x, y) => {
                let p = self.point(x, y);
                let next = self.model.hit(p);
                changed = self.hover != next;
                self.hover = next;
                if !self.model.expanded && !self.model.hovered && next.is_some() {
                    self.model.hovered = true;
                    self.model.retarget();
                    changed = true;
                }
                if let Some((start, hit, initial_scroll)) = self.down {
                    let dx = p.x - start.x;
                    if dx.abs() > 5. || (p.y - start.y).abs() > 5. {
                        self.dragged = true;
                    }
                    if self.dragged {
                        if self.model.bar().contains(start) {
                            self.model.scroll = initial_scroll;
                            self.model.scroll_by(-dx);
                        } else if hit == Hit::Volume && self.model.enabled(hit) {
                            let r = self.model.body();
                            self.model.volume = ((p.x - r.x) / r.w * 100.).clamp(0., 100.);
                        }
                        changed = true;
                    }
                }
            }
            Event::Down(x, y) => {
                let p = self.point(x, y);
                if let Some(hit) = self.model.hit(p) {
                    self.down = Some((p, hit, self.model.scroll));
                    self.dragged = false;
                    self.model.focus = None;
                }
            }
            Event::Up(x, y) => {
                let p = self.point(x, y);
                if let Some((_, hit, _)) = self.down.take() {
                    if !self.dragged && self.model.hit(p) == Some(hit) {
                        if hit == Hit::Volume && self.model.enabled(hit) {
                            let r = self.model.body();
                            self.model.volume = ((p.x - r.x) / r.w * 100.).clamp(0., 100.);
                        } else {
                            self.action(hit);
                        }
                    }
                }
            }
            Event::Wheel(delta) => {
                if self.model.device_menu {
                    self.model.activate(if delta < 0 {
                        Hit::DeviceNext
                    } else {
                        Hit::DevicePrev
                    });
                } else if self.model.expanded {
                    self.model.scroll_by(-delta as f32 / 120. * 32.);
                }
            }
            Event::Key(key) => match key {
                0x77 => self.action(Hit::Tool(3)),
                0x1b => self.model.back(),
                0x20 => {
                    if let Some(hit) = self.model.focus {
                        self.action(hit);
                    } else {
                        self.action(Hit::Play);
                    }
                }
                0x70..=0x73 => {
                    self.model.edge =
                        [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left][(key - 0x70) as usize];
                    self.model.retarget();
                    self.position()?;
                }
                0x74 => {
                    self.model.attached = !self.model.attached;
                    self.model.retarget();
                }
                0x75 => {
                    self.model.reduced = !self.model.reduced
                        || !self.configuration.controls.animations
                        || self.configuration.controls.reduced;
                    self.reduced_override = Some(self.model.reduced);
                    self.model.retarget();
                }
                0x76 if self.media.is_none() => self.model.change_track(),
                0x09 => self
                    .model
                    .move_focus(GetKeyState(VK_SHIFT.0 as i32) < 0, false),
                0x25 | 0x27 => {
                    if self.model.device_menu {
                        self.model.activate(if key == 0x25 {
                            Hit::DevicePrev
                        } else {
                            Hit::DeviceNext
                        });
                    } else if self.model.focus == Some(Hit::Volume)
                        && self.model.enabled(Hit::Volume)
                    {
                        self.model.volume = (self.model.volume
                            + if key == 0x25 { -1. } else { 1. })
                        .clamp(0., 100.);
                    } else {
                        self.model.move_focus(key == 0x25, true);
                    }
                }
                0x0d => {
                    if let Some(hit) = self.model.focus {
                        self.action(hit)
                    } else if !self.model.expanded {
                        self.model.toggle();
                    }
                }
                _ => changed = false,
            },
        }
        if changed {
            if !audio_update && previous_volume != self.model.volume {
                if let (Some(service), Some(audio)) = (&self.audio, &self.model.audio) {
                    service.command(
                        audio.device.id.clone(),
                        system_audio::Action::Volume(self.model.volume.round() as u8),
                    );
                }
            }
            self.redraw()?;
        }
        Ok(true)
    }
    fn report(&self) {
        if let Some(path) = &self.log {
            let mut frames = self.frames_ms.clone();
            frames.sort_by(f64::total_cmp);
            let p95 = frames
                .get(frames.len().saturating_sub(1) * 95 / 100)
                .copied()
                .unwrap_or(0.);
            let mut intervals = self.intervals_ms.clone();
            intervals.sort_by(f64::total_cmp);
            let interval_p95 = intervals
                .get(intervals.len().saturating_sub(1) * 95 / 100)
                .copied()
                .unwrap_or(0.);
            let interval_mean = if intervals.is_empty() {
                0.
            } else {
                intervals.iter().sum::<f64>() / intervals.len() as f64
            };
            let text=format!("{{\"prototype\":true,\"renderer\":\"Direct2D/DirectComposition\",\"elapsedSeconds\":{},\"frames\":{},\"drawAndPresentP95Ms\":{},\"livePages\":{},\"pageGenerations\":{},\"regionPoints\":{},\"scale\":{},\"timerIntervalMs\":{},\"fontFamily\":\"{}\",\"highResolutionTimer\":{},\"presentCallIntervalMeanMs\":{},\"presentCallIntervalP95Ms\":{},\"intervalSamples\":{},\"suspended\":{},\"rendererAlive\":{},\"timerRunning\":{},\"timerLeft\":{},\"pendingCompletion\":{}}}",self.start.elapsed().as_secs_f64(),self.total_frames,p95,usize::from(self.model.current.is_some()),self.model.generation,self.region.len(),self.scale,self.interval,self.font_family,self.frame_timer.high_resolution,interval_mean,interval_p95,intervals.len(),self.suspended,self.renderer.is_some(),self.model.timer_deadline.is_some(),self.model.timer_left,self.pending_completion);
            let mut text = text;
            text.pop();
            text.push_str(&format!(",\"playerDialogAlive\":{},\"playerListReady\":{},\"playerListRows\":{},\"playerSelectionAutomatic\":{},\"playerAllowedCount\":{},\"playerOrderCount\":{}",self.player_dialog.is_some(),self.player_dialog.as_ref().is_some_and(|d|d.ready()),self.player_dialog.as_ref().map(|d|d.row_count()).unwrap_or(0),self.configuration.selection.allowed.is_none(),self.configuration.selection.allowed.as_ref().map(|ids|ids.len()).unwrap_or(0),self.configuration.selection.order.len()));
            text.push_str(&format!(
                ",\"clockZoneIndex\":{},\"clockZoneSupported\":{}",
                clock::ZONES
                    .iter()
                    .position(|(id, _)| *id == self.model.time_zone)
                    .map(|i| i as i32)
                    .unwrap_or(-1),
                clock::now(&self.model.time_zone).is_some()
            ));

            text.push_str(&format!(
                ",\"configurationValid\":{},\"configurationSaving\":{}",
                self.configuration.load_error.is_none(),
                self.configuration.busy()
            ));
            text.push_str(&format!(
                ",\"visibleTools\":{:?},\"reducedMotion\":{},\"islandAlwaysOnTop\":{},\"islandTopmostStyle\":{},\"islandAttached\":{},\"islandEdge\":\"{:?}\",\"islandEdgePosition\":{},\"compactLength\":{},\"collapsedShoulderRadius\":{},\"expandedShoulderRadius\":{},\"expandedCornerRadius\":{}",
                self.model.visible_tools().collect::<Vec<_>>(),
                self.model.reduced,
                self.always_on_top,
                unsafe { GetWindowLongPtrW(self.window, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize != 0 },
                self.model.attached,
                self.model.edge,
                self.edge_position,
                self.model.compact_length,
                self.model.collapsed_shoulder_radius,
                self.model.expanded_shoulder_radius,
                self.model.expanded_corner_radius,
            ));
            let background = self
                .model
                .background_color
                .map(|channel| (channel * 255.).round() as u8);
            text.push_str(&format!(
                ",\"backgroundColor\":\"#{:02X}{:02X}{:02X}\",\"albumColorEnabled\":{}",
                background[0], background[1], background[2], self.model.use_album_color
            ));
            text.push_str(&format!(",{},\"weatherConfigured\":{},\"weatherData\":{},\"weatherError\":{},\"weatherDays\":{},\"settingsWindowAlive\":{}",self.weather.diagnostics(),self.model.weather.city.is_some(),self.model.weather.data.is_some(),self.model.weather.failed,self.model.weather.data.as_ref().map(|d|d.days.len()).unwrap_or(0),self.settings.is_some()));
            if let Some(service) = &self.audio {
                text.push(',');
                text.push_str(&service.diagnostics());
                if let Some(audio) = &self.model.audio {
                    text.push_str(&format!(",\"audioVolume\":{},\"audioMuted\":{},\"audioDevices\":{},\"audioError\":{},\"audioDeviceMenu\":{}", audio.volume, audio.muted, audio.devices.len(), audio.failed, self.model.device_menu));
                }
            }
            if let Some(service) = &self.spectrum {
                let visual = self.model.spectrum.as_ref().unwrap();
                text.push_str(&format!(
                    ",{},\"spectrumError\":{},\"spectrumPeak\":{}",
                    service.diagnostics(),
                    visual.failed,
                    visual.values.iter().copied().fold(0., f32::max)
                ));
            }
            if let Some(service) = &self.media {
                let media = self.model.media.as_ref().unwrap();
                text.push_str(&format!(
                    ",\"coverTextureAlive\":{}",
                    self.renderer.as_ref().is_some_and(|r| r.cover_alive())
                ));
                text.push_str(&format!(
                    ",\"mediaCoverBytes\":{}",
                    media.cover.as_ref().map(|c| c.pixels.len()).unwrap_or(0)
                ));
                text.push_str(&format!(",{},\"mediaSession\":{},\"mediaTitleChars\":{},\"mediaPlaying\":{},\"mediaPositionKnown\":{},\"mediaPositionMs\":{},\"mediaDurationMs\":{},\"mediaError\":{}", service.diagnostics(), media.session, media.title.chars().count(), media.playing, media.timeline.position_known, media.timeline.position(self.start.elapsed().as_secs_f64(), media.playing), media.timeline.duration_ms, self.media_error.is_some()));
            }
            if let Some(settings) = &self.settings {
                if let Some(draft) = unsafe { settings.appearance() } {
                    text.push_str(&format!(
                        ",\"settingsDraftPosition\":{},\"settingsDraftShape\":[{},{},{},{}],\"settingsDraftFillColor\":\"{}\",\"settingsDraftAlbumColor\":{}",
                        draft.edge_position,
                        draft.compact_length,
                        draft.collapsed_shoulder_radius,
                        draft.expanded_shoulder_radius,
                        draft.expanded_corner_radius,
                        draft.floating_fill_color,
                        draft.floating_use_album_color
                    ));
                }
            }
            text.push('}');
            let _ = std::fs::write(path, text);
        }
    }
}
fn value(args: &[String], key: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == key).map(|w| w[1].clone())
}
fn rgb_color(value: &str) -> [f32; 3] {
    if value.len() != 7 || !value.starts_with('#') {
        return [40. / 255., 50. / 255., 60. / 255.];
    }
    let mut rgb = [0.; 3];
    for (index, channel) in rgb.iter_mut().enumerate() {
        *channel = u8::from_str_radix(&value[1 + index * 2..3 + index * 2], 16)
            .unwrap_or([40, 50, 60][index]) as f32
            / 255.;
    }
    rgb
}
unsafe fn system_reduced_motion() -> bool {
    let mut animate = BOOL(1);
    if SystemParametersInfoW(
        SPI_GETCLIENTAREAANIMATION,
        0,
        Some((&mut animate as *mut BOOL).cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    )
    .is_ok()
    {
        !animate.as_bool()
    } else {
        false
    }
}
unsafe fn run() -> Result<()> {
    CoInitializeEx(None, COINIT_APARTMENTTHREADED)?;
    let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let args: Vec<String> = std::env::args().collect();
    let instance = GetModuleHandleW(None)?;
    let class = w!("IsleNativePrototype");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(procedure),
        hInstance: instance.into(),
        lpszClassName: class,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        ..Default::default()
    };
    if RegisterClassW(&wc) == 0 {
        return Err(Error::from_win32());
    }
    let window = CreateWindowExW(
        WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        class,
        w!("Isle Native Prototype"),
        WS_POPUP,
        0,
        0,
        HOST as i32,
        HOST as i32,
        None,
        None,
        instance,
        None,
    );
    if window.0 == 0 {
        return Err(Error::from_win32());
    }
    let accessible = std::sync::Arc::new(std::sync::Mutex::new(accessibility::Snapshot {
        hwnd: window.0,
        ..Default::default()
    }));
    ACCESSIBLE.with(|a| *a.borrow_mut() = Some(accessibility::root(&accessible)));
    let config_override = value(&args, "--settings-path");
    let config_path = config_override
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(configuration::config_path);
    let configuration = configuration::Service::new(
        window,
        config_path,
        if config_override.is_none() {
            configuration::legacy_path()
        } else {
            None
        },
    )
    .map_err(|_| Error::from(E_FAIL))?;
    clock::warm_up();
    let appearance = configuration.appearance.clone();
    let configured_edge = match appearance.edge.as_str() {
        "right" => Edge::Right,
        "bottom" => Edge::Bottom,
        "left" => Edge::Left,
        _ => Edge::Top,
    };
    let mut model = Model {
        time_zone: configuration.controls.time_zone.clone(),
        weather: isle_core::weather::View {
            city: configuration.city.clone(),
            ..Default::default()
        },
        spectrum: args
            .iter()
            .any(|a| a == "--live-spectrum" || a == "--live-media")
            .then(SpectrumVisual::default),
        audio: args
            .iter()
            .any(|a| a == "--live-audio" || a == "--live-media")
            .then(isle_core::AudioSnapshot::default),
        media: args
            .iter()
            .any(|a| a == "--live-media")
            .then(isle_core::MediaSnapshot::default),
        reduced: args.iter().any(|a| a == "--reduced-motion")
            || system_reduced_motion()
            || !configuration.controls.animations
            || configuration.controls.reduced,
        tool_mask: configuration.controls.mask(),
        compact_length: appearance.compact_length,
        collapsed_shoulder_radius: appearance.collapsed_shoulder_radius,
        expanded_shoulder_radius: appearance.expanded_shoulder_radius,
        expanded_corner_radius: appearance.expanded_corner_radius,
        background_color: rgb_color(&appearance.floating_fill_color),
        use_album_color: appearance.floating_use_album_color,
        playing: !args.iter().any(|a| a == "--paused"),
        attached: args.iter().any(|a| a == "--attached") || appearance.style == "edge",
        edge: match value(&args, "--edge").as_deref() {
            Some("right") => Edge::Right,
            Some("bottom") => Edge::Bottom,
            Some("left") => Edge::Left,
            Some("top") => Edge::Top,
            _ => configured_edge,
        },
        tool_count: value(&args, "--tools")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(7)
            .min(7),
        track: usize::from(args.iter().any(|a| a == "--long-title")),
        ..Model::default()
    };
    if let Some(page) = value(&args, "--page") {
        model.switch(match page.as_str() {
            "volume" => Page::Volume,
            "timer" => Page::Timer,
            "clock" => Page::Clock,
            "weather" => Page::Weather,
            _ => Page::Music,
        });
    }
    model.retarget();
    if model.media.is_some() {
        model.playing = false;
    }
    if let Some(ms) = value(&args, "--test-countdown-ms").and_then(|v| v.parse::<u32>().ok()) {
        model.timer_left = ms as f64 / 1000.;
        model.timer_deadline = Some(model.timer_left);
    }
    let scale = GetDpiForWindow(window) as f32 / 96.;
    let start = Instant::now();
    let media_selection = configuration.selection.clone();
    let always_on_top = configuration.controls.always_on_top;
    let mut app = App {
        player_dialog: None,
        weather: weather::Service::new(window).map_err(|_| Error::from(E_FAIL))?,
        settings: None,
        configuration,
        saving_window: None,
        spectrum: if model.spectrum.is_some() {
            Some(spectrum::SpectrumService::new(window)?)
        } else {
            None
        },
        audio: if model.audio.is_some() {
            Some(system_audio::AudioService::new(window).map_err(|_| Error::from(E_FAIL))?)
        } else {
            None
        },
        media: if model.media.is_some() {
            Some(
                media::MediaService::new(window, start, media_selection)
                    .map_err(|_| Error::from(E_FAIL))?,
            )
        } else {
            None
        },
        media_error: None,
        window,
        accessible,
        renderer: None,
        suspended: false,
        pending_completion: false,
        total_frames: 0,
        font_family: "uninitialized",
        test_dpi: value(&args, "--test-dpi").and_then(|v| v.parse().ok()),
        test_work: value(&args, "--test-work-area").and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse::<u32>().ok()?.max(1), h.parse::<u32>().ok()?.max(1)))
        }),
        test_monitor: value(&args, "--test-monitor")
            .or_else(|| std::env::var("ISLE_TEST_MONITOR").ok()),
        model,
        always_on_top,
        edge_position: appearance.edge_position,
        scale,
        last: start,
        start,
        hover: None,
        down: None,
        dragged: false,
        interval: 0,
        region: vec![],
        exit_after: value(&args, "--exit-after").and_then(|v| v.parse().ok()),
        log: value(&args, "--log"),
        scripted: args.iter().any(|a| a == "--scripted"),
        last_script: 0,
        frames_ms: vec![],
        intervals_ms: vec![],
        last_present: None,
        frame_timer: frame_timer::FrameTimer::new()?,
        reduced_override: args.iter().any(|a| a == "--reduced-motion").then_some(true),
    };
    app.position()?;
    app.redraw()?;
    if args.iter().any(|a| a == "--benchmark") {
        // Keep the same rendering path while preventing input from changing a
        // measured scenario. Only the sampling harness opts into this mode.
        EnableWindow(window, false);
    }
    ShowWindow(window, SW_SHOWNOACTIVATE);
    app.apply_topmost()?;
    let mut msg = MSG::default();
    'running: loop {
        let continuous = app.continuous();
        if continuous {
            let deadline = app.last + Duration::from_nanos(16_666_667);
            app.frame_timer
                .arm(deadline.saturating_duration_since(Instant::now()))?;
        } else {
            app.frame_timer.cancel();
        }
        let handles = if continuous {
            vec![app.frame_timer.handle]
        } else {
            vec![]
        };
        let result =
            MsgWaitForMultipleObjectsEx(Some(&handles), u32::MAX, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
        if result == WAIT_FAILED {
            return Err(Error::from_win32());
        }
        if continuous && result == WAIT_OBJECT_0 {
            enqueue(Event::Tick);
        }
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                break 'running;
            }
            if app
                .player_dialog
                .as_ref()
                .is_some_and(|dialog| dialog.route(&msg))
            {
                continue;
            }
            if app
                .settings
                .as_ref()
                .is_some_and(|settings| settings.route(&msg))
            {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        loop {
            let event = EVENTS.with(|q| q.borrow_mut().pop_front());
            if let Some(event) = event {
                if !app.handle(event)? {
                    break 'running;
                }
            } else {
                break;
            }
        }
    }
    app.report();
    if app.interval > 0 {
        let _ = KillTimer(window, 1);
    }
    app.accessible
        .lock()
        .map_err(|_| Error::from(E_FAIL))?
        .closed = true;
    UiaReturnRawElementProvider(
        window,
        WPARAM(0),
        LPARAM(0),
        None::<&IRawElementProviderSimple>,
    );
    ACCESSIBLE.with(|a| *a.borrow_mut() = None);
    app.media = None;
    app.audio = None;
    app.player_dialog = None;
    app.settings = None;
    app.weather.request(None);
    app.spectrum = None;
    DestroyWindow(window)?;
    drop(app);
    CoUninitialize();
    Ok(())
}
fn main() {
    if let Err(error) = unsafe { run() } {
        let message = format!("Isle native prototype: {error}");
        let _ = std::fs::write(std::env::temp_dir().join("isle-native-error.log"), &message);
        unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(message),
                w!("Isle Native — error"),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}
