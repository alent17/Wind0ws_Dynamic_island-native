//! A small, independently owned Win32 player. Its media snapshot shares the
//! decoded cover with the island; closing the window drops that reference.
use isle_core::MediaSnapshot;
use std::time::Instant;
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::*,
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
};

pub const COMMAND: u32 = WM_APP + 81;
pub const CLOSE: usize = 0;
pub const PREVIOUS: usize = 1;
pub const PLAY_PAUSE: usize = 2;
pub const NEXT: usize = 3;
const WIDTH: i32 = 360;
const HEIGHT: i32 = 430;

struct State {
    owner: HWND,
    media: Option<MediaSnapshot>,
    now: f64,
    updated: Instant,
    hover: bool,
    down: Option<usize>,
    font: HFONT,
    small_font: HFONT,
    placeholder_font: HFONT,
}

fn control_at(x: i32, y: i32, width: i32, height: i32) -> Option<usize> {
    if x >= width - 46 && y < 44 {
        return Some(CLOSE);
    }
    if y >= height - 143 && y < height - 67 {
        let center = width / 2;
        if (x - center).abs() < 34 {
            return Some(PLAY_PAUSE);
        }
        if (x - (center - 70)).abs() < 27 {
            return Some(PREVIOUS);
        }
        if (x - (center + 70)).abs() < 27 {
            return Some(NEXT);
        }
    }
    None
}

unsafe fn sync_progress_timer(hwnd: HWND, media: Option<&MediaSnapshot>) {
    let advancing = media.is_some_and(|media| media.playing && media.timeline.duration_ms > 0);
    if advancing {
        let _ = SetTimer(hwnd, 1, 1000, None);
    } else {
        let _ = KillTimer(hwnd, 1);
    }
}

unsafe fn round_window(hwnd: HWND, width: i32, height: i32) {
    let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, 20, 20);
    if SetWindowRgn(hwnd, region, true) == 0 {
        let _ = DeleteObject(region);
    }
}

unsafe fn label(
    dc: HDC,
    text: &str,
    rect: RECT,
    color: COLORREF,
    font: HFONT,
    align: DRAW_TEXT_FORMAT,
) {
    if text.is_empty() {
        return;
    }
    let previous = SelectObject(dc, font);
    SetTextColor(dc, color);
    SetBkMode(dc, TRANSPARENT);
    let mut wide = text.encode_utf16().collect::<Vec<_>>();
    let mut rect = rect;
    let _ = DrawTextW(dc, &mut wide, &mut rect, align);
    let _ = SelectObject(dc, previous);
}

unsafe fn media_symbol(dc: HDC, action: usize, center_x: i32, center_y: i32, playing: bool) {
    let white = CreateSolidBrush(COLORREF(0x00ffffff));
    let old_brush = SelectObject(dc, white);
    let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
    match action {
        PLAY_PAUSE if playing => {
            let _ = Rectangle(dc, center_x - 8, center_y - 11, center_x - 3, center_y + 11);
            let _ = Rectangle(dc, center_x + 3, center_y - 11, center_x + 8, center_y + 11);
        }
        PLAY_PAUSE => {
            let _ = Polygon(
                dc,
                &[
                    POINT {
                        x: center_x - 8,
                        y: center_y - 12,
                    },
                    POINT {
                        x: center_x - 8,
                        y: center_y + 12,
                    },
                    POINT {
                        x: center_x + 11,
                        y: center_y,
                    },
                ],
            );
        }
        PREVIOUS => {
            let _ = Rectangle(
                dc,
                center_x - 12,
                center_y - 10,
                center_x - 9,
                center_y + 10,
            );
            let _ = Polygon(
                dc,
                &[
                    POINT {
                        x: center_x + 9,
                        y: center_y - 10,
                    },
                    POINT {
                        x: center_x + 9,
                        y: center_y + 10,
                    },
                    POINT {
                        x: center_x - 8,
                        y: center_y,
                    },
                ],
            );
        }
        NEXT => {
            let _ = Rectangle(
                dc,
                center_x + 9,
                center_y - 10,
                center_x + 12,
                center_y + 10,
            );
            let _ = Polygon(
                dc,
                &[
                    POINT {
                        x: center_x - 9,
                        y: center_y - 10,
                    },
                    POINT {
                        x: center_x - 9,
                        y: center_y + 10,
                    },
                    POINT {
                        x: center_x + 8,
                        y: center_y,
                    },
                ],
            );
        }
        _ => {}
    }
    let _ = SelectObject(dc, old_pen);
    let _ = SelectObject(dc, old_brush);
    let _ = DeleteObject(white);
}

unsafe fn dim_artwork(dc: HDC, width: i32, art_bottom: i32) {
    let source = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, 1, 1);
    let previous = SelectObject(source, bitmap);
    let _ = SetPixel(source, 0, 0, COLORREF(0));
    let _ = AlphaBlend(
        dc,
        0,
        0,
        width,
        art_bottom,
        source,
        0,
        0,
        1,
        1,
        BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 90,
            AlphaFormat: 0,
        },
    );
    let _ = SelectObject(source, previous);
    let _ = DeleteObject(bitmap);
    let _ = DeleteDC(source);
}

unsafe fn paint(hwnd: HWND, state: &State) {
    let mut ps = PAINTSTRUCT::default();
    let window_dc = BeginPaint(hwnd, &mut ps);
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let width = rect.right;
    let height = rect.bottom;
    let dc = CreateCompatibleDC(window_dc);
    let bitmap = CreateCompatibleBitmap(window_dc, width, height);
    let old_bitmap = SelectObject(dc, bitmap);
    let background = CreateSolidBrush(COLORREF(0x001c1c1c));
    FillRect(dc, &rect, background);
    let _ = DeleteObject(background);

    let has_track = state
        .media
        .as_ref()
        .is_some_and(|media| !media.title.is_empty());
    let art_bottom = if has_track { height - 64 } else { height };
    if let Some(cover) = state.media.as_ref().and_then(|media| media.cover.as_ref()) {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: cover.width as i32,
                biHeight: -(cover.height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        SetStretchBltMode(dc, HALFTONE);
        let _ = StretchDIBits(
            dc,
            0,
            0,
            width,
            art_bottom,
            0,
            0,
            cover.width as i32,
            cover.height as i32,
            Some(cover.pixels.as_ptr().cast()),
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
    } else {
        label(
            dc,
            "♪",
            RECT {
                left: 0,
                top: art_bottom / 2 - 34,
                right: width,
                bottom: art_bottom / 2 + 34,
            },
            COLORREF(0x00444444),
            state.placeholder_font,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
    if has_track {
        let band = CreateSolidBrush(COLORREF(0x00101010));
        FillRect(
            dc,
            &RECT {
                left: 0,
                top: art_bottom,
                right: width,
                bottom: height,
            },
            band,
        );
        let _ = DeleteObject(band);
        let title = state
            .media
            .as_ref()
            .filter(|media| !media.title.is_empty())
            .map_or("等待播放", |media| media.title.as_str());
        let artist = state
            .media
            .as_ref()
            .map_or("", |media| media.artist.as_str());
        label(
            dc,
            title,
            RECT {
                left: 16,
                top: art_bottom + 9,
                right: width - 16,
                bottom: art_bottom + 33,
            },
            COLORREF(0x00ffffff),
            state.font,
            DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        label(
            dc,
            artist,
            RECT {
                left: 16,
                top: art_bottom + 34,
                right: width - 16,
                bottom: art_bottom + 52,
            },
            COLORREF(0x00aaaaaa),
            state.small_font,
            DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    }
    if let Some(media) = state.media.as_ref().filter(|_| has_track) {
        let duration = media.timeline.duration_ms;
        if duration > 0 {
            let now = state.now + state.updated.elapsed().as_secs_f64();
            let position = media.timeline.position(now, media.playing).min(duration);
            let track = CreateSolidBrush(COLORREF(0x00525252));
            let progress = CreateSolidBrush(COLORREF(0x00ffffff));
            let y = height - 3;
            FillRect(
                dc,
                &RECT {
                    left: 0,
                    top: y,
                    right: width,
                    bottom: height,
                },
                track,
            );
            FillRect(
                dc,
                &RECT {
                    left: 0,
                    top: y,
                    right: (position as f64 / duration as f64 * width as f64).round() as i32,
                    bottom: height,
                },
                progress,
            );
            let _ = DeleteObject(track);
            let _ = DeleteObject(progress);
        }
    }
    if state.hover {
        if has_track {
            dim_artwork(dc, width, art_bottom);
        }
        label(
            dc,
            "×",
            RECT {
                left: width - 46,
                top: 8,
                right: width - 8,
                bottom: 44,
            },
            COLORREF(0x00ffffff),
            state.font,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        if has_track {
            for (index, action) in [PREVIOUS, PLAY_PAUSE, NEXT].into_iter().enumerate() {
                media_symbol(
                    dc,
                    action,
                    width / 2 + (index as i32 - 1) * 70,
                    height - 106,
                    state.media.as_ref().is_some_and(|media| media.playing),
                );
            }
        }
    }
    let _ = BitBlt(window_dc, 0, 0, width, height, dc, 0, 0, SRCCOPY);
    let _ = SelectObject(dc, old_bitmap);
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
            WM_PAINT => {
                paint(hwnd, state);
                return LRESULT(0);
            }
            WM_TIMER => {
                let _ = InvalidateRect(hwnd, None, false);
                return LRESULT(0);
            }
            WM_NCHITTEST => {
                let original = DefWindowProcW(hwnd, message, wp, lp);
                if original.0 != HTCLIENT as isize {
                    return original;
                }
                let mut point = POINT {
                    x: lp.0 as u16 as i16 as i32,
                    y: (lp.0 >> 16) as u16 as i16 as i32,
                };
                ScreenToClient(hwnd, &mut point);
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let left = point.x < 8;
                let right = point.x >= rect.right - 8;
                let top = point.y < 8;
                let bottom = point.y >= rect.bottom - 8;
                let resize = match (left, right, top, bottom) {
                    (true, _, true, _) => Some(HTTOPLEFT),
                    (_, true, true, _) => Some(HTTOPRIGHT),
                    (true, _, _, true) => Some(HTBOTTOMLEFT),
                    (_, true, _, true) => Some(HTBOTTOMRIGHT),
                    (true, _, _, _) => Some(HTLEFT),
                    (_, true, _, _) => Some(HTRIGHT),
                    (_, _, true, _) => Some(HTTOP),
                    (_, _, _, true) => Some(HTBOTTOM),
                    _ => None,
                };
                if let Some(hit) = resize {
                    return LRESULT(hit as isize);
                }
                return if control_at(point.x, point.y, rect.right, rect.bottom).is_some() {
                    LRESULT(HTCLIENT as isize)
                } else {
                    LRESULT(HTCAPTION as isize)
                };
            }
            WM_MOUSEMOVE => {
                if !state.hover {
                    state.hover = true;
                    let _ = InvalidateRect(hwnd, None, false);
                }
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    ..Default::default()
                };
                let _ = TrackMouseEvent(&mut track);
                return LRESULT(0);
            }
            0x02A3 => {
                state.hover = false;
                state.down = None;
                let _ = InvalidateRect(hwnd, None, false);
                return LRESULT(0);
            }
            WM_LBUTTONDOWN => {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let x = lp.0 as u16 as i16 as i32;
                let y = (lp.0 >> 16) as u16 as i16 as i32;
                state.down = control_at(x, y, rect.right, rect.bottom);
                if state.down.is_some() {
                    SetCapture(hwnd);
                }
                return LRESULT(0);
            }
            WM_LBUTTONUP => {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let x = lp.0 as u16 as i16 as i32;
                let y = (lp.0 >> 16) as u16 as i16 as i32;
                if let Some(action) = state.down.take() {
                    let _ = ReleaseCapture();
                    if control_at(x, y, rect.right, rect.bottom) == Some(action) {
                        let _ = PostMessageW(state.owner, COMMAND, WPARAM(action), LPARAM(hwnd.0));
                    }
                }
                return LRESULT(0);
            }
            WM_KEYDOWN => {
                let action = match wp.0 as u32 {
                    0x1b => Some(CLOSE),
                    0x20 => Some(PLAY_PAUSE),
                    0x25 => Some(PREVIOUS),
                    0x27 => Some(NEXT),
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
            WM_SIZE if wp.0 != SIZE_MINIMIZED as usize => {
                let width = lp.0 as u16 as i32;
                let height = (lp.0 >> 16) as u16 as i32;
                round_window(hwnd, width, height);
                return LRESULT(0);
            }
            WM_NCDESTROY => {
                let _ = KillTimer(hwnd, 1);
                let _ = DeleteObject(state.font);
                let _ = DeleteObject(state.small_font);
                let _ = DeleteObject(state.placeholder_font);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(pointer));
            }
            _ => {}
        }
    }
    DefWindowProcW(hwnd, message, wp, lp)
}

pub struct FloatingPlayer {
    pub hwnd: HWND,
}

impl FloatingPlayer {
    pub unsafe fn new(
        owner: HWND,
        media: Option<&MediaSnapshot>,
        now: f64,
        topmost: bool,
        show_in_taskbar: bool,
    ) -> Result<Self> {
        let module = GetModuleHandleW(None)?;
        let class = w!("IsleNativeFloatingPlayer");
        let _ = RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: module.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class,
            ..Default::default()
        });
        let dpi = GetDpiForWindow(owner);
        let scale = dpi as f32 / 96.;
        let px = |v: i32| (v as f32 * scale).round() as i32;
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(
            MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
            &mut info,
        )
        .ok()?;
        let width = px(WIDTH).min(info.rcWork.right - info.rcWork.left);
        let height = px(HEIGHT).min(info.rcWork.bottom - info.rcWork.top);
        let hwnd = CreateWindowExW(
            (if topmost {
                WS_EX_TOPMOST
            } else {
                WINDOW_EX_STYLE(0)
            }) | if show_in_taskbar {
                WS_EX_APPWINDOW
            } else {
                WS_EX_TOOLWINDOW
            },
            class,
            w!("Isle Floating Player"),
            WS_POPUP,
            info.rcWork.right - width - px(24),
            info.rcWork.top + px(64),
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
        let make_font = |height, weight| {
            CreateFontW(
                -px(height),
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
            media: media.cloned(),
            now,
            updated: Instant::now(),
            hover: false,
            down: None,
            font: make_font(18, 600),
            small_font: make_font(12, 400),
            placeholder_font: make_font(52, 400),
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
        round_window(hwnd, width, height);
        sync_progress_timer(hwnd, media);
        ShowWindow(hwnd, SW_SHOWNORMAL);
        Ok(Self { hwnd })
    }

    pub unsafe fn update(&self, media: Option<&MediaSnapshot>, now: f64) {
        let pointer = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut State;
        if let Some(state) = pointer.as_mut() {
            state.media = media.cloned();
            state.now = now;
            state.updated = Instant::now();
            sync_progress_timer(self.hwnd, media);
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    pub unsafe fn set_topmost(&self, enabled: bool) -> Result<()> {
        SetWindowPos(
            self.hwnd,
            if enabled {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            },
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    }
}

impl Drop for FloatingPlayer {
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
    fn control_hit_targets_do_not_capture_the_drag_surface() {
        assert_eq!(control_at(338, 20, 360, 430), Some(CLOSE));
        assert_eq!(control_at(180, 325, 360, 430), Some(PLAY_PAUSE));
        assert_eq!(control_at(110, 325, 360, 430), Some(PREVIOUS));
        assert_eq!(control_at(250, 325, 360, 430), Some(NEXT));
        assert_eq!(control_at(80, 80, 360, 430), None);
    }
}
