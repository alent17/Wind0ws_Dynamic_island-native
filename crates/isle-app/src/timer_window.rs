//! Native independent countdown window backed by the island's shared timer.
use isle_ui::model::Model;
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::*,
        UI::{HiDpi::*, Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
    },
};

pub const COMMAND: u32 = WM_APP + 82;
pub const CLOSE: usize = 0;
pub const TOGGLE: usize = 1;
pub const RESET: usize = 2;
const PRESETS: [u16; 4] = [5, 10, 25, 60];
const WIDTH: i32 = 350;
const HEIGHT: i32 = 350;

struct State {
    owner: HWND,
    selected: u16,
    custom: bool,
    custom_valid: bool,
    edit: HWND,
    edit_brush: HBRUSH,
    remaining: u64,
    running: bool,
    active: bool,
    finished: bool,
    large: HFONT,
    medium: HFONT,
    small: HFONT,
}

fn format_time(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = seconds / 60 % 60;
    let seconds = seconds % 60;
    format!("{hours}:{minutes:02}:{seconds:02}")
}

unsafe fn round_window(hwnd: HWND, width: i32, height: i32) {
    let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, 56, 56);
    if SetWindowRgn(hwnd, region, true) == 0 {
        let _ = DeleteObject(region);
    }
}

unsafe fn fill(dc: HDC, rect: RECT, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    FillRect(dc, &rect, brush);
    let _ = DeleteObject(brush);
}

unsafe fn round_fill(dc: HDC, rect: RECT, color: COLORREF, radius: i32) {
    let brush = CreateSolidBrush(color);
    let old_brush = SelectObject(dc, brush);
    let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
    let _ = RoundRect(
        dc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius,
        radius,
    );
    let _ = SelectObject(dc, old_pen);
    let _ = SelectObject(dc, old_brush);
    let _ = DeleteObject(brush);
}

unsafe fn text(
    dc: HDC,
    content: &str,
    rect: RECT,
    font: HFONT,
    color: COLORREF,
    align: DRAW_TEXT_FORMAT,
) {
    if content.is_empty() {
        return;
    }
    let old = SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, color);
    let mut wide = content.encode_utf16().collect::<Vec<_>>();
    let mut rect = rect;
    let _ = DrawTextW(dc, &mut wide, &mut rect, align);
    let _ = SelectObject(dc, old);
}

unsafe fn paint(hwnd: HWND, state: &State) {
    let mut ps = PAINTSTRUCT::default();
    let target = BeginPaint(hwnd, &mut ps);
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let dc = CreateCompatibleDC(target);
    let bitmap = CreateCompatibleBitmap(target, client.right, client.bottom);
    let old = SelectObject(dc, bitmap);
    fill(dc, client, COLORREF(0x00151515));
    text(
        dc,
        "×",
        RECT {
            left: client.right - 38,
            top: 8,
            right: client.right - 8,
            bottom: 38,
        },
        state.medium,
        COLORREF(0x00bbbbbb),
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    let shown = if state.finished {
        0
    } else if state.active {
        state.remaining
    } else {
        state.selected as u64 * 60
    };
    text(
        dc,
        &format_time(shown),
        RECT {
            left: 28,
            top: 47,
            right: client.right - 78,
            bottom: 124,
        },
        state.large,
        COLORREF(0x00ffffff),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    round_fill(
        dc,
        RECT {
            left: client.right - 72,
            top: 60,
            right: client.right - 24,
            bottom: 108,
        },
        COLORREF(0x00f78824),
        48,
    );
    text(
        dc,
        if state.running { "Ⅱ" } else { "✓" },
        RECT {
            left: client.right - 72,
            top: 60,
            right: client.right - 24,
            bottom: 108,
        },
        state.medium,
        COLORREF(0x00ffffff),
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    if state.active || state.finished {
        text(
            dc,
            "重置",
            RECT {
                left: client.right - 82,
                top: 166,
                right: client.right - 27,
                bottom: 192,
            },
            state.small,
            COLORREF(0x00dddddd),
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
    text(
        dc,
        "时长  ·  选择预设或自定义时长",
        RECT {
            left: 27,
            top: 120,
            right: client.right - 22,
            bottom: 148,
        },
        state.small,
        COLORREF(0x00aaaaaa),
        DT_SINGLELINE,
    );
    text(
        dc,
        "时长",
        RECT {
            left: 27,
            top: 169,
            right: 120,
            bottom: 191,
        },
        state.medium,
        COLORREF(0x00ffffff),
        DT_SINGLELINE,
    );
    text(
        dc,
        "选择一个预设，输入自定义时长",
        RECT {
            left: 27,
            top: 192,
            right: client.right - 22,
            bottom: 211,
        },
        state.small,
        COLORREF(0x008e8e8e),
        DT_SINGLELINE,
    );
    round_fill(
        dc,
        RECT {
            left: 26,
            top: 220,
            right: client.right - 26,
            bottom: 286,
        },
        COLORREF(0x002c1b0d),
        16,
    );
    for (index, minutes) in PRESETS.into_iter().enumerate() {
        let x = 30 + index as i32 * 73;
        round_fill(
            dc,
            RECT {
                left: x,
                top: 224,
                right: x + 69,
                bottom: 282,
            },
            if !state.custom && state.selected == minutes {
                COLORREF(0x00a4671b)
            } else {
                COLORREF(0x00583818)
            },
            12,
        );
        text(
            dc,
            &minutes.to_string(),
            RECT {
                left: x,
                top: 231,
                right: x + 69,
                bottom: 258,
            },
            state.medium,
            COLORREF(0x00ffffff),
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        text(
            dc,
            "分钟",
            RECT {
                left: x,
                top: 258,
                right: x + 69,
                bottom: 278,
            },
            state.small,
            COLORREF(0x00aaaaaa),
            DT_CENTER | DT_SINGLELINE,
        );
    }
    round_fill(
        dc,
        RECT {
            left: 26,
            top: 294,
            right: client.right - 26,
            bottom: 340,
        },
        if state.custom {
            COLORREF(0x003a2819)
        } else {
            COLORREF(0x00232323)
        },
        10,
    );
    text(
        dc,
        "+",
        RECT {
            left: 38,
            top: 302,
            right: 64,
            bottom: 332,
        },
        state.medium,
        COLORREF(0x00f78824),
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    text(
        dc,
        "自定义时长",
        RECT {
            left: 66,
            top: 300,
            right: 206,
            bottom: 322,
        },
        state.small,
        COLORREF(0x00ffffff),
        DT_SINGLELINE,
    );
    text(
        dc,
        "输入 1–1440 分钟",
        RECT {
            left: 66,
            top: 318,
            right: client.right - 22,
            bottom: 338,
        },
        state.small,
        COLORREF(0x008e8e8e),
        DT_SINGLELINE,
    );
    if state.active || state.finished {
        text(
            dc,
            if state.finished {
                "已完成"
            } else if state.running {
                "进行中"
            } else {
                "已暂停"
            },
            RECT {
                left: client.right - 80,
                top: 120,
                right: client.right - 25,
                bottom: 144,
            },
            state.small,
            COLORREF(0x00f78824),
            DT_RIGHT | DT_SINGLELINE,
        );
    }
    let _ = BitBlt(target, 0, 0, client.right, client.bottom, dc, 0, 0, SRCCOPY);
    let _ = SelectObject(dc, old);
    let _ = DeleteObject(bitmap);
    let _ = DeleteDC(dc);
    let _ = EndPaint(hwnd, &ps);
}

unsafe extern "system" fn procedure(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    if !pointer.is_null() {
        let state = &mut *pointer;
        match message {
            WM_ERASEBKGND => return LRESULT(1),
            WM_CTLCOLOREDIT => {
                let dc = HDC(wp.0 as isize);
                SetTextColor(dc, COLORREF(0x00ffffff));
                SetBkColor(dc, COLORREF(0x003a2819));
                return LRESULT(state.edit_brush.0);
            }
            WM_COMMAND if wp.0 & 0xffff == 300 && wp.0 >> 16 == EN_CHANGE as usize => {
                if state.edit.0 == 0 {
                    return LRESULT(0);
                }
                let mut chars = [0u16; 8];
                let length = GetWindowTextW(state.edit, &mut chars).max(0) as usize;
                let minutes = String::from_utf16_lossy(&chars[..length])
                    .parse::<u16>()
                    .ok();
                state.custom_valid = minutes.is_some_and(|value| (1..=1440).contains(&value));
                if let Some(minutes) = minutes.filter(|value| (1..=1440).contains(value)) {
                    state.selected = minutes;
                }
                let _ = InvalidateRect(hwnd, None, false);
                return LRESULT(0);
            }
            WM_PAINT => {
                paint(hwnd, state);
                return LRESULT(0);
            }
            WM_SIZE if wp.0 != SIZE_MINIMIZED as usize => {
                round_window(hwnd, lp.0 as u16 as i32, (lp.0 >> 16) as u16 as i32);
                return LRESULT(0);
            }
            WM_NCHITTEST => {
                let mut point = POINT {
                    x: lp.0 as u16 as i16 as i32,
                    y: (lp.0 >> 16) as u16 as i16 as i32,
                };
                ScreenToClient(hwnd, &mut point);
                return if point.y < 43 && point.x > WIDTH - 50
                    || point.x > WIDTH - 82 && (55..115).contains(&point.y)
                    || (state.active || state.finished)
                        && (WIDTH - 82..WIDTH - 27).contains(&point.x)
                        && (166..198).contains(&point.y)
                    || (215..342).contains(&point.y)
                {
                    LRESULT(HTCLIENT as isize)
                } else {
                    LRESULT(HTCAPTION as isize)
                };
            }
            WM_LBUTTONUP => {
                let x = lp.0 as u16 as i16 as i32;
                let y = (lp.0 >> 16) as u16 as i16 as i32;
                let action = if y < 43 && x > WIDTH - 50 {
                    Some(CLOSE)
                } else if (state.active || state.finished)
                    && (WIDTH - 82..WIDTH - 27).contains(&x)
                    && (166..198).contains(&y)
                {
                    Some(RESET)
                } else if x > WIDTH - 82 && (55..115).contains(&y) {
                    Some(TOGGLE)
                } else if (220..286).contains(&y) && (28..WIDTH - 27).contains(&x) && !state.active
                {
                    let index = ((x - 28) / 73).clamp(0, 3) as usize;
                    state.selected = PRESETS[index];
                    state.custom = false;
                    ShowWindow(state.edit, SW_HIDE);
                    let _ = InvalidateRect(hwnd, None, false);
                    None
                } else if (294..340).contains(&y) && !state.active {
                    state.custom = true;
                    state.selected = 30;
                    ShowWindow(state.edit, SW_SHOW);
                    SetFocus(state.edit);
                    let _ = InvalidateRect(hwnd, None, false);
                    None
                } else {
                    None
                };
                if let Some(action) = action {
                    let _ = PostMessageW(state.owner, COMMAND, WPARAM(action), LPARAM(hwnd.0));
                }
                return LRESULT(0);
            }
            WM_KEYDOWN => {
                let action = match wp.0 as u32 {
                    0x1b => Some(CLOSE),
                    0x20 | 0x0d => Some(TOGGLE),
                    0x52 => Some(RESET),
                    _ => None,
                };
                if let Some(action) = action {
                    let _ = PostMessageW(state.owner, COMMAND, WPARAM(action), LPARAM(hwnd.0));
                    return LRESULT(0);
                }
            }
            WM_CLOSE => {
                let _ = PostMessageW(state.owner, COMMAND, WPARAM(CLOSE), LPARAM(hwnd.0));
                return LRESULT(0);
            }
            WM_NCDESTROY => {
                let _ = DeleteObject(state.large);
                let _ = DeleteObject(state.medium);
                let _ = DeleteObject(state.small);
                let _ = DeleteObject(state.edit_brush);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(pointer));
            }
            _ => {}
        }
    }
    DefWindowProcW(hwnd, message, wp, lp)
}

pub struct TimerWindow {
    pub hwnd: HWND,
}
impl TimerWindow {
    pub unsafe fn new(owner: HWND, model: &Model, show_in_taskbar: bool) -> Result<Self> {
        let module = GetModuleHandleW(None)?;
        let class = w!("IsleNativeTimerWindow");
        let _ = RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: module.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class,
            ..Default::default()
        });
        let scale = GetDpiForWindow(owner) as f32 / 96.;
        let px = |v: i32| (v as f32 * scale).round() as i32;
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(
            MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        )
        .ok()?;
        let width = px(WIDTH);
        let height = px(HEIGHT);
        let hwnd = CreateWindowExW(
            if show_in_taskbar {
                WS_EX_APPWINDOW
            } else {
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST
            },
            class,
            w!("Isle Countdown"),
            WS_POPUP,
            monitor.rcWork.left + (monitor.rcWork.right - monitor.rcWork.left - width) / 2,
            monitor.rcWork.top + px(120),
            width,
            height,
            None,
            None,
            module,
            None,
        );
        if hwnd.0 == 0 {
            return Err(Error::from_win32());
        }
        let font = |size, weight| {
            CreateFontW(
                -px(size),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                0,
                w!("MiSans"),
            )
        };
        let state = Box::new(State {
            owner,
            selected: if model.timer_active {
                model.timer_minutes
            } else {
                60
            },
            custom: model.timer_active && !PRESETS.contains(&model.timer_minutes),
            custom_valid: true,
            edit: HWND(0),
            edit_brush: CreateSolidBrush(COLORREF(0x003a2819)),
            remaining: model.timer_left.ceil() as u64,
            running: model.timer_deadline.is_some(),
            active: model.timer_active,
            finished: model.timer_finished,
            large: font(58, 700),
            medium: font(20, 600),
            small: font(12, 400),
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
        let custom_text = if model.timer_active && !PRESETS.contains(&model.timer_minutes) {
            model.timer_minutes.to_string()
        } else {
            "30".into()
        };
        let edit = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            &HSTRING::from(custom_text),
            WS_CHILD | WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_NUMBER as u32),
            px(203),
            px(303),
            px(96),
            px(27),
            hwnd,
            HMENU(300),
            module,
            None,
        );
        if edit.0 != 0 {
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
            (*pointer).edit = edit;
            SendMessageW(
                edit,
                WM_SETFONT,
                WPARAM((*pointer).small.0 as usize),
                LPARAM(1),
            );
        }
        round_window(hwnd, width, height);
        ShowWindow(hwnd, SW_SHOWNORMAL);
        Ok(Self { hwnd })
    }

    pub unsafe fn selected(&self) -> Option<u16> {
        let pointer = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *const State;
        pointer
            .as_ref()
            .and_then(|state| (!state.custom || state.custom_valid).then_some(state.selected))
    }

    pub unsafe fn update(&self, model: &Model) {
        let pointer = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut State;
        if let Some(state) = pointer.as_mut() {
            let remaining = model.timer_left.ceil() as u64;
            let running = model.timer_deadline.is_some();
            let active = model.timer_active;
            let finished = model.timer_finished;
            if (state.remaining, state.running, state.active, state.finished)
                != (remaining, running, active, finished)
            {
                if state.active != active || state.finished != finished {
                    ShowWindow(
                        state.edit,
                        if active || finished {
                            SW_HIDE
                        } else if state.custom {
                            SW_SHOW
                        } else {
                            SW_HIDE
                        },
                    );
                }
                state.remaining = remaining;
                state.running = running;
                state.active = active;
                state.finished = finished;
                let _ = InvalidateRect(self.hwnd, None, false);
            }
        }
    }
}
impl Drop for TimerWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn time_and_presets_use_hours_minutes_seconds() {
        assert_eq!(format_time(0), "0:00:00");
        assert_eq!(format_time(3600), "1:00:00");
        assert_eq!(PRESETS, [5, 10, 25, 60]);
    }
}
