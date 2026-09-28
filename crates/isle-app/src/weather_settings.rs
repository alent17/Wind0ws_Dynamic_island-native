//! Modeless Win32 controls: native edit/IME, list selection and dialog keyboard routing.
use crate::render::Renderer;
use isle_core::weather::City;
use isle_ui::{
    geometry::Edge,
    model::{Model, Page, HOST},
};
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::*,
        UI::{
            Controls::{
                InitCommonControlsEx, DRAWITEMSTRUCT, EM_LIMITTEXT, ICC_BAR_CLASSES,
                INITCOMMONCONTROLSEX, ODS_FOCUS, ODS_SELECTED, TBS_AUTOTICKS,
            },
            HiDpi::*,
            Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
};
pub const COMMAND: u32 = WM_APP + 74;
const STUDIO_BG: COLORREF = COLORREF(0x00120f0e);
const STUDIO_SURFACE: COLORREF = COLORREF(0x002a2522);
const STUDIO_TEXT: COLORREF = COLORREF(0x00f7f6f5);
fn studio_control_y(y: f32, content_y: f32) -> f32 {
    if y >= 560. {
        y - 460. + content_y
    } else if y >= 356. {
        y + 120. + content_y
    } else {
        y + 670. + content_y
    }
}
fn control_clip(y: i32, height: i32, top: i32, bottom: i32) -> Option<(i32, i32)> {
    let clip_top = (top - y).clamp(0, height);
    let clip_bottom = (bottom - y).clamp(0, height);
    (clip_bottom > clip_top).then_some((clip_top, clip_bottom))
}
unsafe fn draw_preview_choice(
    item: &DRAWITEMSTRUCT,
    active: bool,
    font: HFONT,
    scale: f32,
    surface: HBRUSH,
) {
    let dc = item.hDC;
    let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
    let background = if active {
        COLORREF(0x00f7f6f5)
    } else if selected {
        COLORREF(0x003a3330)
    } else {
        COLORREF(0x00120f0e)
    };
    let brush = CreateSolidBrush(background);
    let old_brush = SelectObject(dc, brush);
    let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
    let r = item.rcItem;
    FillRect(dc, &r, surface);
    let radius = (8. * scale).round() as i32;
    let _ = RoundRect(dc, r.left, r.top, r.right, r.bottom, radius, radius);
    let _ = SelectObject(dc, old_pen);
    let _ = SelectObject(dc, old_brush);
    let _ = DeleteObject(brush);
    let old_font = SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(
        dc,
        if active {
            COLORREF(0x00121611)
        } else {
            STUDIO_TEXT
        },
    );
    let labels = ["收起", "悬停", "展开", "隐藏"];
    let mut text = labels[item.CtlID as usize - 240]
        .encode_utf16()
        .collect::<Vec<_>>();
    let mut text_rect = r;
    let _ = DrawTextW(
        dc,
        &mut text,
        &mut text_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    let _ = SelectObject(dc, old_font);
    if item.itemState.0 & ODS_FOCUS.0 != 0 {
        let mut focus = r;
        focus.left += 2;
        focus.top += 2;
        focus.right -= 2;
        focus.bottom -= 2;
        let _ = DrawFocusRect(dc, &focus);
    }
}
struct Theme {
    background: HBRUSH,
    surface: HBRUSH,
    stage: HBRUSH,
    island: HBRUSH,
    font: HFONT,
    scale: f32,
    panel_x: i32,
    scroll: i32,
    max_scroll: i32,
    controls: Vec<(HWND, i32, i32, i32, i32)>,
    preview: Option<Preview>,
    preview_mode: usize,
}
struct Preview {
    hwnd: HWND,
    renderer: Option<Renderer>,
    model: Model,
}
unsafe fn redraw_preview(hwnd: HWND, theme: &mut Theme) {
    let Some(preview) = theme.preview.as_mut() else {
        return;
    };
    let checked = |id| SendMessageW(GetDlgItem(hwnd, id), BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1;
    if theme.preview_mode == 3 {
        preview.renderer = None;
        ShowWindow(preview.hwnd, SW_HIDE);
        return;
    }
    if preview.renderer.is_none() {
        if let Ok(mut renderer) = Renderer::new(preview.hwnd, theme.scale) {
            renderer.opaque_preview = true;
            preview.renderer = Some(renderer);
        }
    }
    ShowWindow(preview.hwnd, SW_SHOWNOACTIVATE);
    match theme.preview_mode {
        0 | 1 if preview.model.expanded => preview.model.toggle(),
        2 if !preview.model.expanded => preview.model.toggle(),
        _ => {}
    }
    preview.model.hovered = theme.preview_mode == 1;
    preview.model.tool_mask = std::array::from_fn(|i| checked(201 + i as i32));
    if !checked(200) {
        preview.model.tool_mask = [false; 7];
    }
    preview.model.attached =
        SendMessageW(GetDlgItem(hwnd, 211), CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 == 1;
    preview.model.edge =
        match SendMessageW(GetDlgItem(hwnd, 213), CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 {
            1 => Edge::Right,
            2 => Edge::Bottom,
            3 => Edge::Left,
            _ => Edge::Top,
        };
    let read = |id| {
        SendMessageW(GetDlgItem(hwnd, id), WM_USER, WPARAM(0), LPARAM(0))
            .0
            .max(0) as u32
    };
    preview.model.compact_length = read(SHAPE_CONTROL_BASE as i32) as u16;
    preview.model.collapsed_shoulder_radius = read(SHAPE_CONTROL_BASE as i32 + 1) as u8;
    preview.model.expanded_shoulder_radius = read(SHAPE_CONTROL_BASE as i32 + 2) as u8;
    preview.model.expanded_corner_radius = read(SHAPE_CONTROL_BASE as i32 + 3);
    preview.model.retarget();
    if let Some(renderer) = preview.renderer.as_mut() {
        let _ = renderer.draw(&preview.model, None, None, false);
    }
}
unsafe fn position_controls(hwnd: HWND, theme: &Theme) {
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let top = (130. * theme.scale).round() as i32;
    let bottom = client.bottom - (20. * theme.scale).round() as i32;
    for &(control, x, base_y, width, height) in &theme.controls {
        let y = base_y - theme.scroll;
        let _ = SetWindowPos(
            control,
            None,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
        if let Some((clip_top, clip_bottom)) = control_clip(y, height, top, bottom) {
            let region = CreateRectRgn(0, clip_top, width, clip_bottom);
            if SetWindowRgn(control, region, true) == 0 {
                let _ = DeleteObject(region);
            }
            ShowWindow(control, SW_SHOWNOACTIVATE);
        } else {
            ShowWindow(control, SW_HIDE);
        }
    }
    let _ = InvalidateRect(hwnd, None, false);
}
unsafe fn studio_text(dc: HDC, x: i32, y: i32, label: &str, color: COLORREF) {
    SetTextColor(dc, color);
    let text = label.encode_utf16().collect::<Vec<_>>();
    let _ = TextOutW(dc, x, y, &text);
}
unsafe fn paint_studio(hwnd: HWND, theme: &Theme) {
    let mut paint = PAINTSTRUCT::default();
    let dc = BeginPaint(hwnd, &mut paint);
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    FillRect(dc, &client, theme.background);
    let px = |value: f32| (value * theme.scale).round() as i32;
    let old_font = SelectObject(dc, theme.font);
    SetBkMode(dc, TRANSPARENT);
    studio_text(
        dc,
        px(32.),
        px(22.),
        "I S L E  /  S T U D I O",
        COLORREF(0x00aaa49b),
    );
    studio_text(dc, px(32.), px(44.), "Isle Studio", STUDIO_TEXT);
    studio_text(
        dc,
        px(32.),
        px(75.),
        "集中设置灵动岛的外观、播放和系统行为。",
        COLORREF(0x00aaa49b),
    );
    let left = px(32.);
    let top = px(130.);
    let right = theme.panel_x - px(24.);
    let bottom = top + px(468.);
    let old_brush = SelectObject(dc, theme.stage);
    let _ = RoundRect(dc, left, top, right, bottom, px(24.), px(24.));
    studio_text(
        dc,
        left + px(20.),
        top + px(18.),
        "●  灵动岛预览",
        COLORREF(0x00938f89),
    );
    let preview_width = px(300.).min(right - left - px(50.));
    let preview_left = left + (right - left - preview_width) / 2;
    let preview_top = top + px(76.);
    if theme.preview.is_none() {
        SelectObject(dc, theme.island);
        let _ = RoundRect(
            dc,
            preview_left,
            preview_top,
            preview_left + preview_width,
            preview_top + px(155.),
            px(45.),
            px(45.),
        );
        studio_text(
            dc,
            preview_left + px(26.),
            preview_top + px(62.),
            "♪    Midnight City",
            COLORREF(0x00f7f6f5),
        );
    }
    studio_text(
        dc,
        left + px(90.),
        bottom - px(26.),
        if theme.preview.is_some() {
            "预览舞台 · 原生形状预览"
        } else {
            "预览舞台 · 暂不可用"
        },
        COLORREF(0x00938f89),
    );
    // The settings groups scroll as separate cards while the preview stage stays fixed.
    let saved_dc = SaveDC(dc);
    let panel_right = client.right - px(32.);
    let panel_bottom = client.bottom - px(20.);
    let _ = IntersectClipRect(dc, theme.panel_x, top, panel_right, panel_bottom);
    SelectObject(dc, theme.surface);
    for (card_top, card_bottom) in [(130., 580.), (588., 790.), (800., 1130.)] {
        let card_top = px(card_top) - theme.scroll;
        let card_bottom = px(card_bottom) - theme.scroll;
        if card_bottom > top && card_top < panel_bottom {
            let _ = RoundRect(
                dc,
                theme.panel_x,
                card_top,
                panel_right,
                card_bottom,
                px(16.),
                px(16.),
            );
        }
    }
    let _ = RestoreDC(dc, saved_dc);
    let _ = SelectObject(dc, old_brush);
    let _ = SelectObject(dc, old_font);
    let _ = EndPaint(hwnd, &paint);
}
pub const SEARCH: usize = 102;
pub const APPLY: usize = 104;
pub const APPLY_CONTROLS: usize = 105;
pub const CLOSE: usize = 2;
pub const PLAYERS: usize = 107;
pub const APPLY_APPEARANCE: usize = 212;
const SHAPE_CONTROL_BASE: usize = 220;
const SHAPE_LABEL_BASE: usize = 230;
const SHAPE_NAMES: [&str; 4] = ["收起长度", "收起凹肩", "展开凹肩", "展开圆角"];
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let theme = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Theme;
    if !theme.is_null() {
        match msg {
            WM_ERASEBKGND => {
                let mut r = RECT::default();
                let _ = GetClientRect(hwnd, &mut r);
                FillRect(HDC(wp.0 as isize), &r, (*theme).background);
                return LRESULT(1);
            }
            WM_PAINT => {
                paint_studio(hwnd, &*theme);
                return LRESULT(0);
            }
            WM_MOUSEWHEEL => {
                let wheel = (wp.0 >> 16) as i16 as i32;
                let steps = wheel / WHEEL_DELTA as i32;
                if steps != 0 {
                    let next = ((*theme).scroll - steps * ((54. * (*theme).scale) as i32))
                        .clamp(0, (*theme).max_scroll);
                    if next != (*theme).scroll {
                        (*(theme as *mut Theme)).scroll = next;
                        position_controls(hwnd, &*theme);
                    }
                    return LRESULT(0);
                }
            }
            WM_DRAWITEM if lp.0 != 0 => {
                let item = &*(lp.0 as *const DRAWITEMSTRUCT);
                if (240..244).contains(&(item.CtlID as usize)) {
                    draw_preview_choice(
                        item,
                        item.CtlID as usize - 240 == (*theme).preview_mode,
                        (*theme).font,
                        (*theme).scale,
                        (*theme).surface,
                    );
                    return LRESULT(1);
                }
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                let dc = HDC(wp.0 as isize);
                SetTextColor(dc, STUDIO_TEXT);
                SetBkColor(dc, STUDIO_SURFACE);
                return LRESULT((*theme).surface.0);
            }
            _ => {}
        }
    }
    if msg == WM_HSCROLL && lp.0 != 0 {
        let track = HWND(lp.0);
        let id = GetDlgCtrlID(track);
        if (SHAPE_CONTROL_BASE as i32..(SHAPE_CONTROL_BASE + 4) as i32).contains(&id) {
            let index = id as usize - SHAPE_CONTROL_BASE;
            let position = SendMessageW(track, WM_USER, WPARAM(0), LPARAM(0)).0;
            let text = HSTRING::from(format!("{}：{} px", SHAPE_NAMES[index], position));
            let _ = SetWindowTextW(GetDlgItem(hwnd, (SHAPE_LABEL_BASE + index) as i32), &text);
            if !theme.is_null() {
                redraw_preview(hwnd, &mut *(theme as *mut Theme));
            }
            return LRESULT(0);
        }
    }
    if msg == WM_COMMAND && !theme.is_null() {
        let id = wp.0 & 0xffff;
        let notify = wp.0 >> 16;
        if (240..244).contains(&id) && notify == BN_CLICKED as usize {
            let theme = &mut *(theme as *mut Theme);
            theme.preview_mode = id - 240;
            for index in 0..4 {
                let _ = InvalidateRect(GetDlgItem(hwnd, 240 + index), None, true);
            }
            redraw_preview(hwnd, theme);
            return LRESULT(0);
        }
        if (id == 211 || id == 213) && notify == CBN_SELCHANGE as usize
            || (200..=207).contains(&id) && notify == BN_CLICKED as usize
        {
            redraw_preview(hwnd, &mut *(theme as *mut Theme));
        }
    }
    let action = match msg {
        WM_CLOSE => Some(CLOSE),
        WM_SHOWWINDOW if wp.0 == 0 => Some(CLOSE),
        WM_SIZE if wp.0 == SIZE_MINIMIZED as usize => Some(CLOSE),
        WM_COMMAND => {
            let id = wp.0 & 0xffff;
            let notify = wp.0 >> 16;
            if id == 103 && notify == LBN_DBLCLK as usize {
                Some(APPLY)
            } else if matches!(
                id,
                SEARCH | APPLY | APPLY_CONTROLS | APPLY_APPEARANCE | PLAYERS | CLOSE
            ) && notify == BN_CLICKED as usize
            {
                Some(id)
            } else {
                None
            }
        }
        _ => None,
    };
    if let Some(action) = action {
        let owner = GetWindow(hwnd, GW_OWNER);
        let _ = PostMessageW(owner, COMMAND, WPARAM(action), LPARAM(hwnd.0));
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
pub struct Settings {
    pub hwnd: HWND,
    query: HWND,
    list: HWND,
    status: HWND,
    apply: HWND,
    edge_position: HWND,
    font: HFONT,
    private_fonts: Vec<HSTRING>,
    pub cities: Vec<City>,
    time_zones: Vec<String>,
    styles: Vec<String>,
    edges: Vec<String>,
    shape_controls: Vec<HWND>,
    shape_labels: Vec<HWND>,
    fill_color: HWND,
}
impl Settings {
    pub unsafe fn new(
        owner: HWND,
        controls: &isle_core::configuration::Controls,
        appearance: &isle_core::configuration::Appearance,
    ) -> Result<Self> {
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        })
        .ok()?;
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = w!("IsleNativeWeatherSettings");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let scale = GetDpiForWindow(owner) as f32 / 96.;
        let px = |v: f32| (v * scale).round() as i32;
        let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        let mut rect = RECT {
            right: px(1240.),
            bottom: px(930.),
            ..Default::default()
        };
        AdjustWindowRectExForDpi(
            &mut rect,
            style,
            false,
            WS_EX_CONTROLPARENT,
            GetDpiForWindow(owner),
        )?;
        // Open beside the island on its monitor, so desktop testing and daily
        // use do not steal the user's other display.
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let found = GetMonitorInfoW(
            MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        )
        .as_bool();
        let (x, y, width, height) = if found {
            let work = monitor.rcWork;
            let width = (rect.right - rect.left).min(work.right - work.left);
            let height = (rect.bottom - rect.top).min(work.bottom - work.top);
            (
                work.left + ((work.right - work.left - width) / 2),
                work.top + ((work.bottom - work.top - height) / 2),
                width,
                height,
            )
        } else {
            (
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                rect.right - rect.left,
                rect.bottom - rect.top,
            )
        };
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class,
            w!("Isle Studio"),
            style,
            x,
            y,
            width,
            height,
            owner,
            None,
            instance,
            None,
        );
        if hwnd.0 == 0 {
            return Err(Error::from_win32());
        }
        let mut value = Self {
            hwnd,
            query: HWND(0),
            list: HWND(0),
            status: HWND(0),
            apply: HWND(0),
            edge_position: HWND(0),
            font: HFONT(0),
            private_fonts: vec![],
            cities: vec![],
            time_zones: vec![],
            styles: vec![],
            edges: vec![],
            shape_controls: vec![],
            shape_labels: vec![],
            fill_color: HWND(0),
        };
        let panel_x = px(548.);
        let content_y = 120.;
        let mut client = RECT::default();
        GetClientRect(hwnd, &mut client)?;
        let theme = Box::new(Theme {
            background: CreateSolidBrush(STUDIO_BG),
            surface: CreateSolidBrush(STUDIO_SURFACE),
            stage: CreateSolidBrush(COLORREF(0x00ffffff)),
            island: CreateSolidBrush(COLORREF(0)),
            font: HFONT(0),
            scale,
            panel_x,
            scroll: 0,
            max_scroll: (px(1010. + content_y) - client.bottom).max(0),
            controls: Vec::new(),
            preview: None,
            preview_mode: 2,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(theme) as isize);
        if let Some(folder) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("fonts")))
        {
            for weight in ["Regular", "Medium", "Bold"] {
                let path = HSTRING::from(folder.join(format!("MiSans-{weight}.ttf")).as_os_str());
                if AddFontResourceExW(&path, FR_PRIVATE, None) > 0 {
                    value.private_fonts.push(path);
                }
            }
        }
        value.font = CreateFontW(
            -px(15.),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            if value.private_fonts.is_empty() {
                w!("Segoe UI")
            } else {
                w!("MiSans")
            },
        );
        let theme = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Theme;
        (*theme).font = value.font;
        let preview_window = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP,
            w!("STATIC"),
            w!(""),
            WS_CHILD | WS_VISIBLE,
            px(37.),
            px(180.),
            px(HOST),
            px(395.),
            hwnd,
            None,
            instance,
            None,
        );
        if preview_window.0 != 0 {
            if let Ok(mut renderer) = Renderer::new(preview_window, scale) {
                renderer.opaque_preview = true;
                let mut model = Model {
                    reduced: true,
                    media: Some(isle_core::MediaSnapshot {
                        session: 1,
                        title: "Midnight City".into(),
                        artist: "M83 · Hurry Up, We're Dreaming".into(),
                        playing: true,
                        previous: true,
                        play_pause: true,
                        next: true,
                        timeline: isle_core::Timeline {
                            position_ms: 122_000,
                            duration_ms: 244_000,
                            received_at: 0.,
                            position_known: true,
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                model.switch(Page::Music);
                (*theme).preview = Some(Preview {
                    hwnd: preview_window,
                    renderer: Some(renderer),
                    model,
                });
            } else {
                let _ = DestroyWindow(preview_window);
            }
        }
        let child = |class: PCWSTR,
                     text: &str,
                     id: usize,
                     style: WINDOW_STYLE,
                     x: f32,
                     y: f32,
                     w: f32,
                     h: f32|
         -> Result<HWND> {
            let control = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                &HSTRING::from(text),
                WS_CHILD | WS_VISIBLE | style,
                px(x) + panel_x,
                px(if (239..244).contains(&id) {
                    y + content_y
                } else {
                    studio_control_y(y, content_y)
                }),
                px(w),
                px(h),
                hwnd,
                HMENU(id as isize),
                instance,
                None,
            );
            if control.0 == 0 {
                return Err(Error::from_win32());
            }
            SendMessageW(
                control,
                WM_SETFONT,
                WPARAM(value.font.0 as usize),
                LPARAM(1),
            );
            let theme = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Theme;
            let mut rect = RECT::default();
            GetWindowRect(control, &mut rect)?;
            let mut top_left = POINT {
                x: rect.left,
                y: rect.top,
            };
            ScreenToClient(hwnd, &mut top_left);
            (*theme).controls.push((
                control,
                top_left.x,
                top_left.y,
                rect.right - rect.left,
                rect.bottom - rect.top,
            ));
            Ok(control)
        };
        child(
            w!("STATIC"),
            "预览状态",
            239,
            WINDOW_STYLE(0),
            20.,
            18.,
            500.,
            20.,
        )?;
        for (index, label) in ["收起", "悬停", "展开", "隐藏"].iter().enumerate() {
            let button = child(
                w!("BUTTON"),
                label,
                240 + index,
                WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                20. + index as f32 * 125.,
                48.,
                120.,
                28.,
            )?;
            let _ = button;
        }
        child(
            w!("STATIC"),
            "天气城市",
            0,
            WINDOW_STYLE(0),
            20.,
            18.,
            480.,
            22.,
        )?;
        value.query = child(
            w!("EDIT"),
            "",
            101,
            WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            20.,
            48.,
            394.,
            30.,
        )?;
        SendMessageW(value.query, EM_LIMITTEXT, WPARAM(80), LPARAM(0));
        child(
            w!("BUTTON"),
            "搜索",
            SEARCH,
            WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
            424.,
            48.,
            96.,
            30.,
        )?;
        value.status = child(
            w!("STATIC"),
            "输入至少两个字或拼音，按 Enter 搜索",
            106,
            WINDOW_STYLE(0),
            20.,
            88.,
            500.,
            22.,
        )?;
        value.list = child(
            w!("LISTBOX"),
            "搜索结果",
            103,
            WS_BORDER
                | WS_TABSTOP
                | WS_VSCROLL
                | WS_HSCROLL
                | WINDOW_STYLE(LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32),
            20.,
            120.,
            500.,
            152.,
        )?;
        SendMessageW(
            value.list,
            LB_SETHORIZONTALEXTENT,
            WPARAM(px(1000.) as usize),
            LPARAM(0),
        );
        child(
            w!("STATIC"),
            "天气数据：Open-Meteo  ·  仅保存到原生版配置",
            0,
            WINDOW_STYLE(0),
            20.,
            284.,
            500.,
            20.,
        )?;
        value.apply = child(
            w!("BUTTON"),
            "保存城市",
            APPLY,
            WS_TABSTOP,
            310.,
            310.,
            100.,
            28.,
        )?;
        child(
            w!("BUTTON"),
            "取消",
            CLOSE,
            WS_TABSTOP,
            420.,
            310.,
            100.,
            28.,
        )?;
        EnableWindow(value.apply, false);
        child(
            w!("STATIC"),
            "顶部工具栏与动画",
            0,
            WINDOW_STYLE(0),
            20.,
            356.,
            480.,
            22.,
        )?;
        let labels = [
            "启用功能栏",
            "倒计时",
            "音量",
            "悬浮播放器",
            "设置",
            "隐藏",
            "时间",
            "天气",
            "启用动画",
            "减少动画",
        ];
        let states = [
            controls.panel,
            controls.tools[0],
            controls.tools[1],
            controls.tools[2],
            controls.tools[3],
            controls.tools[4],
            controls.tools[5],
            controls.tools[6],
            controls.animations,
            controls.reduced,
        ];
        for (i, label) in labels.iter().enumerate() {
            let checkbox = child(
                w!("BUTTON"),
                label,
                200 + i,
                WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                20. + (i % 4) as f32 * 125.,
                382. + (i / 4) as f32 * 28.,
                124.,
                24.,
            )?;
            SendMessageW(
                checkbox,
                BM_SETCHECK,
                WPARAM(usize::from(states[i])),
                LPARAM(0),
            );
        }
        let floating_topmost = child(
            w!("BUTTON"),
            "悬浮播放器始终置顶",
            218,
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            20.,
            438.,
            245.,
            24.,
        )?;
        SendMessageW(
            floating_topmost,
            BM_SETCHECK,
            WPARAM(usize::from(controls.floating_always_on_top)),
            LPARAM(0),
        );
        let topmost = child(
            w!("BUTTON"),
            "灵动岛始终置顶",
            217,
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            270.,
            438.,
            210.,
            24.,
        )?;
        SendMessageW(
            topmost,
            BM_SETCHECK,
            WPARAM(usize::from(controls.always_on_top)),
            LPARAM(0),
        );
        child(
            w!("BUTTON"),
            "应用设置",
            APPLY_CONTROLS,
            WS_TABSTOP,
            410.,
            516.,
            110.,
            28.,
        )?;
        child(
            w!("STATIC"),
            "关闭功能栏后，可在岛上按 F8 打开设置",
            0,
            WINDOW_STYLE(0),
            20.,
            520.,
            375.,
            22.,
        )?;
        child(
            w!("STATIC"),
            "时钟时区",
            0,
            WINDOW_STYLE(0),
            20.,
            477.,
            80.,
            22.,
        )?;
        let zone = child(
            w!("COMBOBOX"),
            "时钟时区",
            210,
            WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
            105.,
            472.,
            300.,
            180.,
        )?;
        let mut names = crate::clock::ZONES
            .iter()
            .map(|(id, label)| (id.to_string(), format!("{label} · {id}")))
            .collect::<Vec<_>>();
        if !names.iter().any(|(id, _)| id == &controls.time_zone) {
            names.push((
                controls.time_zone.clone(),
                format!("未支持：{}", controls.time_zone),
            ));
        }
        for (id, label) in &names {
            let text = HSTRING::from(label);
            SendMessageW(
                zone,
                CB_ADDSTRING,
                WPARAM(0),
                LPARAM(text.as_ptr() as isize),
            );
            value.time_zones.push(id.clone());
        }
        let selected = value
            .time_zones
            .iter()
            .position(|id| id == &controls.time_zone)
            .unwrap_or(0);
        SendMessageW(zone, CB_SETCURSEL, WPARAM(selected), LPARAM(0));
        child(
            w!("BUTTON"),
            "播放器…",
            PLAYERS,
            WS_TABSTOP,
            414.,
            472.,
            106.,
            28.,
        )?;
        child(
            w!("STATIC"),
            "灵动岛外观与位置",
            0,
            WINDOW_STYLE(0),
            20.,
            560.,
            480.,
            22.,
        )?;
        child(
            w!("STATIC"),
            "样式",
            0,
            WINDOW_STYLE(0),
            20.,
            592.,
            42.,
            22.,
        )?;
        let style = child(
            w!("COMBOBOX"),
            "灵动岛样式",
            211,
            WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
            64.,
            586.,
            130.,
            150.,
        )?;
        for (id, label) in [("floating", "悬浮"), ("edge", "贴边")] {
            let text = HSTRING::from(label);
            SendMessageW(
                style,
                CB_ADDSTRING,
                WPARAM(0),
                LPARAM(text.as_ptr() as isize),
            );
            value.styles.push(id.into());
        }
        if !value.styles.iter().any(|id| id == &appearance.style) {
            let text = HSTRING::from(format!("其他：{}", appearance.style));
            SendMessageW(
                style,
                CB_ADDSTRING,
                WPARAM(0),
                LPARAM(text.as_ptr() as isize),
            );
            value.styles.push(appearance.style.clone());
        }
        SendMessageW(
            style,
            CB_SETCURSEL,
            WPARAM(
                value
                    .styles
                    .iter()
                    .position(|id| id == &appearance.style)
                    .unwrap_or(0),
            ),
            LPARAM(0),
        );
        child(
            w!("STATIC"),
            "贴边方向",
            0,
            WINDOW_STYLE(0),
            210.,
            592.,
            58.,
            22.,
        )?;
        let edge = child(
            w!("COMBOBOX"),
            "贴边方向",
            213,
            WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
            270.,
            586.,
            120.,
            150.,
        )?;
        for (id, label) in [
            ("top", "上"),
            ("right", "右"),
            ("bottom", "下"),
            ("left", "左"),
        ] {
            let text = HSTRING::from(label);
            SendMessageW(
                edge,
                CB_ADDSTRING,
                WPARAM(0),
                LPARAM(text.as_ptr() as isize),
            );
            value.edges.push(id.into());
        }
        if !value.edges.iter().any(|id| id == &appearance.edge) {
            let text = HSTRING::from(format!("其他：{}", appearance.edge));
            SendMessageW(
                edge,
                CB_ADDSTRING,
                WPARAM(0),
                LPARAM(text.as_ptr() as isize),
            );
            value.edges.push(appearance.edge.clone());
        }
        SendMessageW(
            edge,
            CB_SETCURSEL,
            WPARAM(
                value
                    .edges
                    .iter()
                    .position(|id| id == &appearance.edge)
                    .unwrap_or(0),
            ),
            LPARAM(0),
        );
        child(
            w!("STATIC"),
            "沿边位置",
            0,
            WINDOW_STYLE(0),
            408.,
            592.,
            50.,
            22.,
        )?;
        value.edge_position = child(
            w!("COMBOBOX"),
            "沿边位置",
            214,
            WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
            460.,
            586.,
            78.,
            180.,
        )?;
        for position in 0..=100 {
            let text = HSTRING::from(format!("{position}%"));
            SendMessageW(
                value.edge_position,
                CB_ADDSTRING,
                WPARAM(0),
                LPARAM(text.as_ptr() as isize),
            );
        }
        SendMessageW(
            value.edge_position,
            CB_SETCURSEL,
            WPARAM(appearance.edge_position as usize),
            LPARAM(0),
        );
        child(
            w!("STATIC"),
            "岛体形状",
            0,
            WINDOW_STYLE(0),
            20.,
            652.,
            480.,
            22.,
        )?;
        let shape_values = [
            (appearance.compact_length as i32, 80, 300, 20., 680.),
            (
                appearance.collapsed_shoulder_radius as i32,
                0,
                16,
                280.,
                680.,
            ),
            (appearance.expanded_shoulder_radius as i32, 0, 64, 20., 742.),
            (appearance.expanded_corner_radius as i32, 0, 80, 280., 742.),
        ];
        for (index, (initial, min, max, x, y)) in shape_values.into_iter().enumerate() {
            let label = child(
                w!("STATIC"),
                &format!("{}：{} px", SHAPE_NAMES[index], initial),
                SHAPE_LABEL_BASE + index,
                WINDOW_STYLE(0),
                x,
                y,
                230.,
                20.,
            )?;
            value.shape_labels.push(label);
            let track = child(
                w!("msctls_trackbar32"),
                "",
                SHAPE_CONTROL_BASE + index,
                WS_TABSTOP | WINDOW_STYLE(TBS_AUTOTICKS),
                x,
                y + 22.,
                230.,
                30.,
            )?;
            SendMessageW(
                track,
                WM_USER + 6,
                WPARAM(1),
                LPARAM(((max as u32) << 16 | min as u32) as isize),
            );
            SendMessageW(track, WM_USER + 5, WPARAM(1), LPARAM(initial as isize));
            SendMessageW(track, WM_USER + 20, WPARAM(10), LPARAM(0));
            value.shape_controls.push(track);
        }
        child(
            w!("STATIC"),
            "灵动岛背景色",
            0,
            WINDOW_STYLE(0),
            20.,
            808.,
            480.,
            20.,
        )?;
        let album_color = child(
            w!("BUTTON"),
            "跟随专辑主色",
            216,
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            20.,
            836.,
            150.,
            24.,
        )?;
        SendMessageW(
            album_color,
            BM_SETCHECK,
            WPARAM(usize::from(appearance.floating_use_album_color)),
            LPARAM(0),
        );
        child(
            w!("STATIC"),
            "自定义颜色",
            0,
            WINDOW_STYLE(0),
            240.,
            837.,
            92.,
            22.,
        )?;
        value.fill_color = child(
            w!("EDIT"),
            &appearance.floating_fill_color,
            215,
            WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            338.,
            832.,
            112.,
            28.,
        )?;
        SendMessageW(value.fill_color, EM_LIMITTEXT, WPARAM(7), LPARAM(0));
        child(
            w!("BUTTON"),
            "应用外观",
            APPLY_APPEARANCE,
            WS_TABSTOP,
            410.,
            878.,
            110.,
            28.,
        )?;
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(
            MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
            &mut info,
        )
        .ok()?;
        let mut bounds = RECT::default();
        GetWindowRect(hwnd, &mut bounds)?;
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        SetWindowPos(
            hwnd,
            None,
            info.rcWork.left + ((info.rcWork.right - info.rcWork.left - width) / 2).max(0),
            info.rcWork.top + ((info.rcWork.bottom - info.rcWork.top - height) / 2).max(0),
            width,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )?;
        let theme = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Theme;
        position_controls(hwnd, &*theme);
        redraw_preview(hwnd, &mut *(theme as *mut Theme));
        let activate = IsWindowEnabled(owner).as_bool();
        ShowWindow(
            hwnd,
            if activate {
                SW_SHOWNORMAL
            } else {
                SW_SHOWNOACTIVATE
            },
        );
        if activate {
            SetForegroundWindow(hwnd);
            SetFocus(value.query);
        }
        Ok(value)
    }
    pub unsafe fn controls(&self) -> isle_core::configuration::Controls {
        let checked =
            |i| SendMessageW(GetDlgItem(self.hwnd, i), BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1;
        isle_core::configuration::Controls {
            panel: checked(200),
            tools: std::array::from_fn(|i| checked(201 + i as i32)),
            animations: checked(208),
            reduced: checked(209),
            always_on_top: checked(217),
            floating_always_on_top: checked(218),
            time_zone: self
                .time_zones
                .get(
                    SendMessageW(
                        GetDlgItem(self.hwnd, 210),
                        CB_GETCURSEL,
                        WPARAM(0),
                        LPARAM(0),
                    )
                    .0 as usize,
                )
                .cloned()
                .unwrap_or_else(|| "system".into()),
        }
    }
    pub unsafe fn appearance(&self) -> Option<isle_core::configuration::Appearance> {
        let read_combo = |id, values: &[String]| {
            values
                .get(
                    SendMessageW(
                        GetDlgItem(self.hwnd, id),
                        CB_GETCURSEL,
                        WPARAM(0),
                        LPARAM(0),
                    )
                    .0 as usize,
                )
                .cloned()
        };
        let edge_position =
            u8::try_from(SendMessageW(self.edge_position, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0)
                .ok()
                .filter(|value| *value <= 100)?;
        let shape_values = self
            .shape_controls
            .iter()
            .map(|track| SendMessageW(*track, WM_USER, WPARAM(0), LPARAM(0)).0)
            .collect::<Vec<_>>();
        let fill_color = self.fill_color_text();
        if fill_color.len() != 7
            || !fill_color.starts_with('#')
            || !fill_color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return None;
        }
        let album_color = SendMessageW(
            GetDlgItem(self.hwnd, 216),
            BM_GETCHECK,
            WPARAM(0),
            LPARAM(0),
        )
        .0 == 1;
        Some(isle_core::configuration::Appearance {
            style: read_combo(211, &self.styles)?,
            edge: read_combo(213, &self.edges)?,
            edge_position,
            compact_length: u16::try_from(*shape_values.first()?).ok()?,
            collapsed_shoulder_radius: u8::try_from(*shape_values.get(1)?).ok()?,
            expanded_shoulder_radius: u8::try_from(*shape_values.get(2)?).ok()?,
            expanded_corner_radius: u32::try_from(*shape_values.get(3)?).ok()?,
            floating_fill_color: fill_color,
            floating_use_album_color: album_color,
        })
    }
    unsafe fn fill_color_text(&self) -> String {
        let mut text = [0u16; 16];
        let len = GetWindowTextW(self.fill_color, &mut text);
        String::from_utf16_lossy(&text[..len.max(0) as usize])
    }
    pub unsafe fn query(&self) -> String {
        let mut text = [0u16; 81];
        let len = GetWindowTextW(self.query, &mut text);
        String::from_utf16_lossy(&text[..len.max(0) as usize])
            .trim()
            .to_string()
    }
    pub unsafe fn saving(&self, saving: bool) {
        for control in [self.query, self.list, GetDlgItem(self.hwnd, SEARCH as i32)] {
            EnableWindow(control, !saving);
        }
        EnableWindow(self.apply, !saving && !self.cities.is_empty());
        for id in (200..211).chain([211, 213, APPLY_CONTROLS, APPLY_APPEARANCE, PLAYERS]) {
            EnableWindow(GetDlgItem(self.hwnd, id as i32), !saving);
        }
        EnableWindow(GetDlgItem(self.hwnd, 217), !saving);
        EnableWindow(self.edge_position, !saving);
        EnableWindow(GetDlgItem(self.hwnd, 215), !saving);
        EnableWindow(GetDlgItem(self.hwnd, 216), !saving);
        for track in &self.shape_controls {
            EnableWindow(*track, !saving);
        }
        if saving {
            self.message("正在保存…");
        }
    }
    pub unsafe fn message(&self, text: &str) {
        let _ = SetWindowTextW(self.status, &HSTRING::from(text));
    }
    pub unsafe fn loading(&mut self) {
        self.cities.clear();
        SendMessageW(self.list, LB_RESETCONTENT, WPARAM(0), LPARAM(0));
        EnableWindow(self.apply, false);
        self.message("正在搜索…");
    }
    pub unsafe fn results(&mut self, cities: Vec<City>, failed: bool) {
        self.loading();
        self.cities = cities;
        for city in &self.cities {
            let text = HSTRING::from(format!(
                "{} ({:.2}, {:.2})",
                city.name, city.latitude, city.longitude
            ));
            SendMessageW(
                self.list,
                LB_ADDSTRING,
                WPARAM(0),
                LPARAM(text.as_ptr() as isize),
            );
        }
        if !self.cities.is_empty() {
            SendMessageW(self.list, LB_SETCURSEL, WPARAM(0), LPARAM(0));
            EnableWindow(self.apply, true);
        }
        self.message(if failed {
            "搜索失败，请重试"
        } else if self.cities.is_empty() {
            "未找到城市，请尝试完整名称或拼音"
        } else {
            "选择城市后保存；双击也可保存"
        });
    }
    pub unsafe fn selected(&self) -> Option<City> {
        let index = SendMessageW(self.list, LB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
        usize::try_from(index)
            .ok()
            .and_then(|i| self.cities.get(i).cloned())
    }
    pub unsafe fn route(&self, msg: &MSG) -> bool {
        (msg.hwnd == self.hwnd || IsChild(self.hwnd, msg.hwnd).as_bool())
            && IsDialogMessageW(self.hwnd, msg).as_bool()
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        unsafe {
            let theme = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut Theme;
            if !theme.is_null() {
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            }
            if !theme.is_null() {
                (*theme).preview.take();
            }
            let _ = DestroyWindow(self.hwnd);
            if !theme.is_null() {
                let theme = Box::from_raw(theme);
                let _ = DeleteObject(theme.background);
                let _ = DeleteObject(theme.surface);
                let _ = DeleteObject(theme.stage);
                let _ = DeleteObject(theme.island);
            }
            for font in &self.private_fonts {
                let _ = RemoveFontResourceExW(font, FR_PRIVATE.0, None);
            }
            if self.font.0 != 0 {
                let _ = DeleteObject(self.font);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::{control_clip, studio_control_y};
    #[test]
    fn studio_sections_follow_reference_order_and_clip_at_panel_edges() {
        assert_eq!(studio_control_y(560., 120.), 220.);
        assert_eq!(studio_control_y(356., 120.), 596.);
        assert_eq!(studio_control_y(18., 120.), 808.);
        assert_eq!(control_clip(125, 30, 130, 900), Some((5, 30)));
        assert_eq!(control_clip(890, 40, 130, 900), Some((0, 10)));
        assert_eq!(control_clip(900, 40, 130, 900), None);
    }
}
