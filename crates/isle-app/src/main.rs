#![windows_subsystem = "windows"]
mod frame_timer;
mod render;
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
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
};
#[derive(Debug)]
enum Event {
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
    Preferences,
    Close,
}
thread_local! {static EVENTS:RefCell<VecDeque<Event>>=const{RefCell::new(VecDeque::new())};}
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
        WM_DPICHANGED | WM_DISPLAYCHANGE => {
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
    window: HWND,
    renderer: Renderer,
    model: Model,
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
        let monitor = MonitorFromWindow(self.window, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(monitor, &mut info).ok()?;
        self.scale = GetDpiForWindow(self.window) as f32 / 96.;
        if self.scale == 0. {
            self.scale = 1.;
        }
        let r = info.rcWork;
        let size = (HOST * self.scale).ceil() as i32;
        let (x, y) = match self.model.edge {
            Edge::Top => ((r.left + r.right - size) / 2, r.top),
            Edge::Bottom => ((r.left + r.right - size) / 2, r.bottom - size),
            Edge::Left => (r.left, (r.top + r.bottom - size) / 2),
            Edge::Right => (r.right - size, (r.top + r.bottom - size) / 2),
        };
        SetWindowPos(self.window, HWND_TOPMOST, x, y, size, size, SWP_NOACTIVATE)?;
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
    unsafe fn redraw(&mut self) -> Result<()> {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;
        self.model.step(dt, (now - self.start).as_secs_f64());
        self.region()?;
        let start = Instant::now();
        if let Err(error) = self.renderer.draw(
            &self.model,
            self.hover,
            self.down.filter(|_| !self.dragged).map(|(_, h, _)| h),
        ) {
            eprintln!("Rendering failed, rebuilding device: {error}");
            self.renderer = Renderer::new(self.window, self.scale)?;
            self.renderer.draw(
                &self.model,
                self.hover,
                self.down.filter(|_| !self.dragged).map(|(_, h, _)| h),
            )?;
        }
        self.model.title_overflow = self.renderer.title_overflow;
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
        let desired = if self.model.continuous() {
            0
        } else if self.model.timer_deadline.is_some() || self.scripted || self.exit_after.is_some()
        {
            1000
        } else if self.model.expanded && self.model.page() == Page::Clock {
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
    unsafe fn action(&mut self, hit: Hit) {
        match hit {
            Hit::Tool(2) => {
                MessageBoxW(
                    self.window,
                    w!("此阶段验证原生绘制。独立悬浮播放器将在业务接入阶段迁移。"),
                    w!("Isle 原生原型"),
                    MB_OK,
                );
            }
            Hit::Tool(3) => {
                MessageBoxW(self.window,w!("F1–F4：四边贴靠\nF5：悬浮/贴边\nF6：减少动画\nF7：切换长歌名\n空白：展开/收起\n方向键：功能选择；Enter：打开\nEscape：返回\nAlt+F4：退出\n\n这是独立原型，未连接真实媒体或旧版设置。"),w!("原型操作"),MB_OK);
            }
            Hit::Tool(4) => {
                self.model.toggle();
            }
            _ => self.model.activate(hit),
        }
    }
    unsafe fn handle(&mut self, event: Event) -> Result<bool> {
        let mut changed = true;
        match event {
            Event::Close => return Ok(false),
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
                    || self.model.timer_deadline.is_some()
                    || self.scripted
                    || (self.model.expanded && self.model.page() == Page::Clock);
                if self.scripted {
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
                self.model.reduced = self
                    .reduced_override
                    .unwrap_or_else(|| system_reduced_motion());
                self.model.retarget();
            }
            Event::Resize => {
                self.position()?;
                self.renderer = Renderer::new(self.window, self.scale)?;
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
                        } else if hit == Hit::Volume {
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
                        if hit == Hit::Volume {
                            let r = self.model.body();
                            self.model.volume = ((p.x - r.x) / r.w * 100.).clamp(0., 100.);
                        } else {
                            self.action(hit);
                        }
                    }
                }
            }
            Event::Wheel(delta) => {
                if self.model.expanded {
                    self.model.scroll_by(-delta as f32 / 120. * 32.);
                }
            }
            Event::Key(key) => match key {
                0x1b => self.model.back(),
                0x20 => {
                    if let Some(hit) = self.model.focus {
                        self.action(hit);
                    } else {
                        self.model.playing = !self.model.playing;
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
                    self.model.reduced = !self.model.reduced;
                    self.reduced_override = Some(self.model.reduced);
                    self.model.retarget();
                }
                0x76 => self.model.change_track(),
                0x09 => self
                    .model
                    .move_focus(GetKeyState(VK_SHIFT.0 as i32) < 0, false),
                0x25 | 0x27 => {
                    if self.model.focus == Some(Hit::Volume) {
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
            let text=format!("{{\"prototype\":true,\"renderer\":\"Direct2D/DirectComposition\",\"elapsedSeconds\":{},\"frames\":{},\"drawAndPresentP95Ms\":{},\"livePages\":{},\"pageGenerations\":{},\"regionPoints\":{},\"scale\":{},\"timerIntervalMs\":{},\"fontFamily\":\"{}\",\"highResolutionTimer\":{},\"presentCallIntervalMeanMs\":{},\"presentCallIntervalP95Ms\":{},\"intervalSamples\":{}}}",self.start.elapsed().as_secs_f64(),self.renderer.frames,p95,usize::from(self.model.current.is_some()),self.model.generation,self.region.len(),self.scale,self.interval,self.renderer.font_family,self.frame_timer.high_resolution,interval_mean,interval_p95,intervals.len());
            let _ = std::fs::write(path, text);
        }
    }
}
fn value(args: &[String], key: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == key).map(|w| w[1].clone())
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
    let mut model = Model {
        reduced: args.iter().any(|a| a == "--reduced-motion") || system_reduced_motion(),
        playing: !args.iter().any(|a| a == "--paused"),
        attached: args.iter().any(|a| a == "--attached"),
        edge: match value(&args, "--edge").as_deref() {
            Some("right") => Edge::Right,
            Some("bottom") => Edge::Bottom,
            Some("left") => Edge::Left,
            _ => Edge::Top,
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
    let scale = GetDpiForWindow(window) as f32 / 96.;
    let start = Instant::now();
    let mut app = App {
        window,
        renderer: Renderer::new(window, scale)?,
        model,
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
    if app.scale != app.renderer.scale {
        app.renderer = Renderer::new(window, app.scale)?;
    }
    app.redraw()?;
    if args.iter().any(|a| a == "--benchmark") {
        // Keep the same rendering path while preventing input from changing a
        // measured scenario. Only the sampling harness opts into this mode.
        EnableWindow(window, false);
    }
    ShowWindow(window, SW_SHOWNOACTIVATE);
    let mut msg = MSG::default();
    'running: loop {
        let continuous = app.model.continuous();
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
