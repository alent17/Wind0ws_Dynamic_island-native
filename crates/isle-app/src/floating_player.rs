//! A small, independently owned Win32 player. Its media snapshot shares the
//! decoded cover with the island; closing the window drops that reference.
use isle_core::{AudioSnapshot, MediaSnapshot};
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
pub const FAVORITE: usize = 4;
pub const MODE: usize = 5;
pub const REATTACH: usize = 6;
pub const SET_VOLUME: usize = 7;
pub const MUTE: usize = 8;
pub const LYRICS: usize = 9;
const WIDTH: i32 = 300;
const HEIGHT: i32 = 540;
const ART_BOTTOM: i32 = WIDTH + 23;

fn ui_scale(height: i32) -> f32 {
    height.max(1) as f32 / HEIGHT as f32
}

fn ui_px(value: i32, scale: f32) -> i32 {
    (value as f32 * scale).round() as i32
}

struct State {
    owner: HWND,
    media: Option<MediaSnapshot>,
    audio: Option<AudioSnapshot>,
    now: f64,
    updated: Instant,
    liked: bool,
    hover: bool,
    down: Option<usize>,
    font: HFONT,
    small_font: HFONT,
    placeholder_font: HFONT,
}

fn control_at(x: i32, y: i32, width: i32, height: i32) -> Option<usize> {
    let scale = ui_scale(height);
    let px = |value| ui_px(value, scale);
    let art_bottom = px(ART_BOTTOM).min(height);
    if x >= width - px(46) && y < px(44) {
        return Some(CLOSE);
    }
    if y >= art_bottom - px(23) && y < art_bottom + px(19) && x >= width - px(68) {
        return Some(FAVORITE);
    }
    if y >= art_bottom + px(70) && y < art_bottom + px(112) {
        let center = width / 2;
        if (x - center).abs() < px(28) {
            return Some(PLAY_PAUSE);
        }
        if (x - (center - px(44))).abs() < px(24) {
            return Some(PREVIOUS);
        }
        if (x - (center + px(44))).abs() < px(24) {
            return Some(NEXT);
        }
    }
    if (height - px(67)..height - px(31)).contains(&y) && x >= width - px(42) {
        return Some(MODE);
    }
    if (height - px(55)..height - px(25)).contains(&y) && x < px(42) {
        return Some(LYRICS);
    }
    if (height - px(99)..height - px(69)).contains(&y) && (px(24)..width - px(20)).contains(&x) {
        return Some(SET_VOLUME);
    }
    if (height - px(99)..height - px(69)).contains(&y) && x < px(24) {
        return Some(MUTE);
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

fn media_time(milliseconds: u64) -> String {
    let seconds = milliseconds / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

unsafe fn rounded_bar(dc: HDC, left: i32, top: i32, right: i32, bottom: i32, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    let old_brush = SelectObject(dc, brush);
    let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
    let _ = RoundRect(dc, left, top, right, bottom, bottom - top, bottom - top);
    let _ = SelectObject(dc, old_pen);
    let _ = SelectObject(dc, old_brush);
    let _ = DeleteObject(brush);
}

unsafe fn speaker_symbol(dc: HDC, center_x: i32, center_y: i32, waves: bool) {
    let color = COLORREF(0x00b8b8c0);
    let brush = CreateSolidBrush(color);
    let old_brush = SelectObject(dc, brush);
    let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
    let _ = Polygon(
        dc,
        &[
            POINT {
                x: center_x - 6,
                y: center_y - 3,
            },
            POINT {
                x: center_x - 2,
                y: center_y - 3,
            },
            POINT {
                x: center_x + 3,
                y: center_y - 7,
            },
            POINT {
                x: center_x + 3,
                y: center_y + 7,
            },
            POINT {
                x: center_x - 2,
                y: center_y + 3,
            },
            POINT {
                x: center_x - 6,
                y: center_y + 3,
            },
        ],
    );
    let _ = SelectObject(dc, old_pen);
    let _ = SelectObject(dc, old_brush);
    let _ = DeleteObject(brush);
    if waves {
        let pen = CreatePen(PS_SOLID, 1, color);
        let old_pen = SelectObject(dc, pen);
        let _ = Polyline(
            dc,
            &[
                POINT {
                    x: center_x + 5,
                    y: center_y - 4,
                },
                POINT {
                    x: center_x + 8,
                    y: center_y,
                },
                POINT {
                    x: center_x + 5,
                    y: center_y + 4,
                },
            ],
        );
        let _ = Polyline(
            dc,
            &[
                POINT {
                    x: center_x + 8,
                    y: center_y - 7,
                },
                POINT {
                    x: center_x + 12,
                    y: center_y,
                },
                POINT {
                    x: center_x + 8,
                    y: center_y + 7,
                },
            ],
        );
        let _ = SelectObject(dc, old_pen);
        let _ = DeleteObject(pen);
    }
}

unsafe fn monitor_symbol(dc: HDC, center_x: i32, center_y: i32) {
    let pen = CreatePen(PS_SOLID, 1, COLORREF(0x00a0a0a8));
    let old_pen = SelectObject(dc, pen);
    let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
    let _ = Rectangle(dc, center_x - 8, center_y - 7, center_x + 8, center_y + 4);
    let _ = Rectangle(dc, center_x - 2, center_y + 4, center_x + 2, center_y + 7);
    let _ = Rectangle(dc, center_x - 7, center_y + 7, center_x + 7, center_y + 9);
    let _ = SelectObject(dc, old_brush);
    let _ = SelectObject(dc, old_pen);
    let _ = DeleteObject(pen);
}

unsafe fn lyrics_symbol(dc: HDC, center_x: i32, center_y: i32) {
    let fill = CreateSolidBrush(COLORREF(0x00b8b8c0));
    let old_brush = SelectObject(dc, fill);
    let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
    let _ = RoundRect(
        dc,
        center_x - 9,
        center_y - 7,
        center_x + 9,
        center_y + 5,
        5,
        5,
    );
    let _ = Polygon(
        dc,
        &[
            POINT {
                x: center_x - 2,
                y: center_y + 3,
            },
            POINT {
                x: center_x - 2,
                y: center_y + 8,
            },
            POINT {
                x: center_x + 4,
                y: center_y + 3,
            },
        ],
    );
    let _ = SelectObject(dc, old_pen);
    let _ = SelectObject(dc, old_brush);
    let _ = DeleteObject(fill);
    let dots = CreateSolidBrush(COLORREF(0x00303038));
    let old_brush = SelectObject(dc, dots);
    let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
    let _ = Ellipse(dc, center_x - 5, center_y - 2, center_x - 2, center_y + 1);
    let _ = Ellipse(dc, center_x + 1, center_y - 2, center_x + 4, center_y + 1);
    let _ = SelectObject(dc, old_pen);
    let _ = SelectObject(dc, old_brush);
    let _ = DeleteObject(dots);
}

unsafe fn media_symbol(dc: HDC, action: usize, center_x: i32, center_y: i32, playing: bool) {
    let white = CreateSolidBrush(COLORREF(0x00ffffff));
    let old_brush = SelectObject(dc, white);
    let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
    match action {
        PLAY_PAUSE if playing => {
            let _ = Rectangle(dc, center_x - 9, center_y - 13, center_x - 3, center_y + 13);
            let _ = Rectangle(dc, center_x + 3, center_y - 13, center_x + 9, center_y + 13);
        }
        PLAY_PAUSE => {
            let _ = Polygon(
                dc,
                &[
                    POINT {
                        x: center_x - 9,
                        y: center_y - 13,
                    },
                    POINT {
                        x: center_x - 9,
                        y: center_y + 13,
                    },
                    POINT {
                        x: center_x + 12,
                        y: center_y,
                    },
                ],
            );
        }
        PREVIOUS | NEXT => {
            let backward = action == PREVIOUS;
            let wedges = if backward {
                [
                    [
                        POINT {
                            x: center_x + 20,
                            y: center_y - 12,
                        },
                        POINT {
                            x: center_x + 1,
                            y: center_y,
                        },
                        POINT {
                            x: center_x + 20,
                            y: center_y + 12,
                        },
                    ],
                    [
                        POINT {
                            x: center_x - 1,
                            y: center_y - 12,
                        },
                        POINT {
                            x: center_x - 20,
                            y: center_y,
                        },
                        POINT {
                            x: center_x - 1,
                            y: center_y + 12,
                        },
                    ],
                ]
            } else {
                [
                    [
                        POINT {
                            x: center_x - 20,
                            y: center_y - 12,
                        },
                        POINT {
                            x: center_x - 1,
                            y: center_y,
                        },
                        POINT {
                            x: center_x - 20,
                            y: center_y + 12,
                        },
                    ],
                    [
                        POINT {
                            x: center_x + 1,
                            y: center_y - 12,
                        },
                        POINT {
                            x: center_x + 20,
                            y: center_y,
                        },
                        POINT {
                            x: center_x + 1,
                            y: center_y + 12,
                        },
                    ],
                ]
            };
            for wedge in wedges {
                let _ = Polygon(dc, &wedge);
            }
        }
        _ => {}
    }
    let _ = SelectObject(dc, old_pen);
    let _ = SelectObject(dc, old_brush);
    let _ = DeleteObject(white);
}

unsafe fn paint(hwnd: HWND, state: &State) {
    let mut ps = PAINTSTRUCT::default();
    let window_dc = BeginPaint(hwnd, &mut ps);
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let width = rect.right;
    let height = rect.bottom;
    let scale = ui_scale(height);
    let px = |value| ui_px(value, scale);
    let dc = CreateCompatibleDC(window_dc);
    let bitmap = CreateCompatibleBitmap(window_dc, width, height);
    let old_bitmap = SelectObject(dc, bitmap);
    let background = CreateSolidBrush(COLORREF(0x00000000));
    FillRect(dc, &rect, background);
    let _ = DeleteObject(background);

    let has_track = state
        .media
        .as_ref()
        .is_some_and(|media| !media.title.is_empty());
    let art_bottom = if has_track {
        px(ART_BOTTOM).min(height)
    } else {
        height
    };
    if let Some(cover) = state.media.as_ref().and_then(|media| media.cover.as_ref()) {
        let mut cover_pixels = cover.pixels.clone();
        let fade_start = cover.height as usize * 2 / 3;
        let fade_span = (cover.height as usize - fade_start).max(1);
        for y in fade_start..cover.height as usize {
            let progress = (y - fade_start) as f32 / fade_span as f32;
            let brightness = 1. - progress * 0.55;
            for x in 0..cover.width as usize {
                let at = (y * cover.width as usize + x) * 4;
                for channel in &mut cover_pixels[at..at + 3] {
                    *channel = (*channel as f32 * brightness).round() as u8;
                }
            }
        }
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
        let source_width = cover.width as f32;
        let source_height = cover.height as f32;
        let scale = (width as f32 / source_width).max(art_bottom as f32 / source_height);
        let crop_width = (width as f32 / scale).min(source_width).round() as i32;
        let crop_height = (art_bottom as f32 / scale).min(source_height).round() as i32;
        let _ = StretchDIBits(
            dc,
            0,
            0,
            width,
            art_bottom,
            (cover.width as i32 - crop_width) / 2,
            (cover.height as i32 - crop_height) / 2,
            crop_width,
            crop_height,
            Some(cover_pixels.as_ptr().cast()),
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
                top: art_bottom / 2 - px(34),
                right: width,
                bottom: art_bottom / 2 + px(34),
            },
            COLORREF(0x009090a0),
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
                left: px(18),
                top: art_bottom - px(20),
                right: width - px(62),
                bottom: art_bottom + px(5),
            },
            COLORREF(0x00ffffff),
            state.font,
            DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        label(
            dc,
            artist,
            RECT {
                left: px(18),
                top: art_bottom + px(1),
                right: width - px(62),
                bottom: art_bottom + px(22),
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
            let elapsed = media_time(position);
            let remaining = format!("-{}", media_time(duration.saturating_sub(position)));
            let time_y = art_bottom + px(46);
            label(
                dc,
                &elapsed,
                RECT {
                    left: px(18),
                    top: time_y,
                    right: px(78),
                    bottom: time_y + px(20),
                },
                COLORREF(0x00aaaaaa),
                state.small_font,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            label(
                dc,
                &remaining,
                RECT {
                    left: width - px(78),
                    top: time_y,
                    right: width - px(18),
                    bottom: time_y + px(20),
                },
                COLORREF(0x00aaaaaa),
                state.small_font,
                DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
            );
            let x = px(18);
            let y = art_bottom + px(34);
            let right = width - px(18);
            rounded_bar(dc, x, y - px(2), right, y + px(2), COLORREF(0x00404040));
            let position_x =
                x + ((right - x) as f64 * position as f64 / duration as f64).round() as i32;
            rounded_bar(
                dc,
                x,
                y - px(2),
                position_x.max(x + px(2)),
                y + px(2),
                COLORREF(0x00ffffff),
            );
            let progress = CreateSolidBrush(COLORREF(0x00ffffff));
            let previous = SelectObject(dc, progress);
            let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
            let _ = Ellipse(
                dc,
                position_x - px(4),
                y - px(4),
                position_x + px(4),
                y + px(4),
            );
            let _ = SelectObject(dc, old_pen);
            let _ = SelectObject(dc, previous);
            let _ = DeleteObject(progress);
        }
    }
    if let Some(media) = state.media.as_ref().filter(|_| has_track) {
        let favorite = if state.liked { "♥" } else { "♡" };
        label(
            dc,
            favorite,
            RECT {
                left: width - px(54),
                top: art_bottom - px(23),
                right: width - px(14),
                bottom: art_bottom + px(19),
            },
            if state.liked {
                COLORREF(0x00734fff)
            } else {
                COLORREF(0x00bdbdbd)
            },
            state.font,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        let center_y = art_bottom + px(91);
        for (index, action) in [PREVIOUS, PLAY_PAUSE, NEXT].into_iter().enumerate() {
            let center_x = width / 2 + (index as i32 - 1) * px(44);
            media_symbol(dc, action, center_x, center_y, media.playing);
        }
        let audio = state.audio.as_ref();
        let volume = audio.map_or(0, |snapshot| snapshot.volume) as i32;
        let volume_y = height - px(84);
        speaker_symbol(dc, px(15), volume_y, false);
        speaker_symbol(dc, width - px(17), volume_y, true);
        let volume_left = px(32);
        let volume_right = width - px(34);
        rounded_bar(
            dc,
            volume_left,
            volume_y - px(2),
            volume_right,
            volume_y + px(2),
            COLORREF(0x00404040),
        );
        let volume_x = volume_left + (volume_right - volume_left) * volume / 100;
        rounded_bar(
            dc,
            volume_left,
            volume_y - px(2),
            volume_x.max(volume_left + px(2)),
            volume_y + px(2),
            COLORREF(0x00ffffff),
        );
        let progress = CreateSolidBrush(COLORREF(0x00ffffff));
        let previous = SelectObject(dc, progress);
        let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
        let _ = Ellipse(
            dc,
            volume_x - px(4),
            volume_y - px(4),
            volume_x + px(4),
            volume_y + px(4),
        );
        let _ = SelectObject(dc, old_pen);
        let _ = SelectObject(dc, previous);
        let _ = DeleteObject(progress);
        let output = audio.map_or("默认输出", |snapshot| snapshot.device.name.as_str());
        let output_y = height - px(40);
        lyrics_symbol(dc, px(20), output_y);
        monitor_symbol(dc, px(66), output_y);
        label(
            dc,
            output,
            RECT {
                left: px(78),
                top: height - px(59),
                right: width - px(28),
                bottom: height - px(31),
            },
            COLORREF(0x00a0a0a8),
            state.small_font,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        let mode = if media.shuffle {
            "⇄"
        } else if media.repeat_mode == 1 {
            "↻¹"
        } else {
            "↻"
        };
        label(
            dc,
            mode,
            RECT {
                left: width - px(30),
                top: height - px(64),
                right: width - px(2),
                bottom: height - px(32),
            },
            if media.shuffle || media.repeat_mode > 0 {
                COLORREF(0x00e6e6e6)
            } else {
                COLORREF(0x007f7f86)
            },
            state.font,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
    if state.hover {
        label(
            dc,
            "×",
            RECT {
                left: width - px(46),
                top: px(8),
                right: width - px(8),
                bottom: px(44),
            },
            COLORREF(0x00ffffff),
            state.font,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
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
                        let packed = if action == SET_VOLUME {
                            let scale = ui_scale(rect.bottom);
                            let left = ui_px(32, scale);
                            let right = rect.right - ui_px(34, scale);
                            let value =
                                ((x - left) * 100 / (right - left).max(1)).clamp(0, 100) as usize;
                            action | (value << 8)
                        } else {
                            action
                        };
                        let _ = PostMessageW(state.owner, COMMAND, WPARAM(packed), LPARAM(hwnd.0));
                    }
                }
                return LRESULT(0);
            }
            WM_EXITSIZEMOVE => {
                let _ = PostMessageW(state.owner, COMMAND, WPARAM(REATTACH), LPARAM(hwnd.0));
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
        audio: Option<&AudioSnapshot>,
        liked: bool,
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
            audio: audio.cloned(),
            now,
            updated: Instant::now(),
            liked,
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

    pub unsafe fn update(
        &self,
        media: Option<&MediaSnapshot>,
        audio: Option<&AudioSnapshot>,
        liked: bool,
        now: f64,
    ) {
        let pointer = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut State;
        if let Some(state) = pointer.as_mut() {
            state.media = media.cloned();
            state.audio = audio.cloned();
            state.liked = liked;
            state.now = now;
            state.updated = Instant::now();
            sync_progress_timer(self.hwnd, media);
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    pub unsafe fn move_to_center(&self, x: i32, y: i32) {
        let mut rect = RECT::default();
        let _ = GetWindowRect(self.hwnd, &mut rect);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let _ = SetWindowPos(
            self.hwnd,
            HWND_TOPMOST,
            x - width / 2,
            y - height / 2,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE,
        );
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
        assert_eq!(control_at(278, 20, WIDTH, HEIGHT), Some(CLOSE));
        assert_eq!(control_at(150, 414, WIDTH, HEIGHT), Some(PLAY_PAUSE));
        assert_eq!(control_at(106, 414, WIDTH, HEIGHT), Some(PREVIOUS));
        assert_eq!(control_at(194, 414, WIDTH, HEIGHT), Some(NEXT));
        assert_eq!(control_at(80, 80, WIDTH, HEIGHT), None);
    }
}
