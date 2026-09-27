//! Read-only, on-demand GSMTC discovery and a native selection dialog.
use isle_core::Selection;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Receiver},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows::{
    core::*,
    Foundation::AsyncStatus,
    Media::Control::*,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{LibraryLoader::*, WinRT::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
};
pub const COMMAND: u32 = WM_APP + 76;
pub const UPDATED: u32 = WM_APP + 77;
pub const SAVE: usize = 307;
pub const CLOSE: usize = 2;
#[derive(Clone)]
struct Row {
    id: String,
    enabled: bool,
    online: bool,
    playing: bool,
}
fn display_name(id: &str) -> &str {
    let id = id.to_ascii_lowercase();
    if id.contains("cloudmusic") || id.contains("netease") {
        "网易云音乐"
    } else if id.contains("spotify") {
        "Spotify"
    } else if id.contains("qqmusic") {
        "QQ 音乐"
    } else if id.contains("msedge") {
        "Microsoft Edge"
    } else if id.contains("chrome") {
        "Chrome"
    } else if id.contains("firefox") {
        "Firefox"
    } else if id.contains("applemusic") {
        "Apple Music"
    } else {
        "媒体播放器"
    }
}
fn rows(selection: &Selection, live: &[(String, bool)]) -> std::result::Result<Vec<Row>, String> {
    let mut rows: Vec<Row> = vec![];
    for id in selection
        .order
        .iter()
        .chain(selection.allowed.iter().flatten())
        .chain(live.iter().map(|(id, _)| id))
    {
        if rows.iter().any(|row| row.id == *id) {
            continue;
        }
        if rows.len() >= 256
            || id.chars().count() > 512
            || id.is_empty()
            || id.chars().any(char::is_control)
        {
            return Err("播放器列表过大或标识无效，未截断原配置".into());
        }
        let active = live.iter().find(|(key, _)| key == id);
        rows.push(Row {
            id: id.clone(),
            enabled: selection
                .allowed
                .as_ref()
                .is_none_or(|ids| ids.contains(id)),
            online: active.is_some(),
            playing: active.is_some_and(|(_, playing)| *playing),
        });
    }
    Ok(rows)
}
type DiscoveryResult = std::result::Result<Vec<(String, bool)>, String>;
struct Discovery {
    cancel: Arc<AtomicBool>,
    receiver: Receiver<DiscoveryResult>,
    worker: Option<JoinHandle<()>>,
}
impl Discovery {
    fn start(hwnd: HWND) -> std::io::Result<Self> {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (sender, receiver) = channel();
        let hwnd = hwnd.0;
        let worker=std::thread::Builder::new().name("isle-player-list".into()).spawn(move||{
            let result=unsafe{RoInitialize(RO_INIT_MULTITHREADED)}.and_then(|_|{
                let result=(||{
                    let operation=GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?;
                    let deadline=Instant::now()+Duration::from_secs(2);
                    while operation.Status()?==AsyncStatus::Started {
                        if flag.load(Ordering::Acquire)||Instant::now()>=deadline {let _=operation.Cancel();return Err(Error::from(E_ABORT));}
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    let manager=operation.GetResults()?;let sessions=manager.GetSessions()?;let mut values:Vec<(String,bool)>=vec![];
                    for i in 0..sessions.Size()?.min(64) {
                        if flag.load(Ordering::Acquire) {return Err(Error::from(E_ABORT));}
                        let session=sessions.GetAt(i)?;let id=session.SourceAppUserModelId()?.to_string();
                        let playing=session.GetPlaybackInfo().and_then(|p|p.PlaybackStatus()).is_ok_and(|s|s==GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing);
                        if let Some((_,prior))=values.iter_mut().find(|(key,_)|*key==id) {*prior|=playing;} else {values.push((id,playing));}
                    }
                    Ok(values)
                })();
                unsafe{RoUninitialize();}result
            }).map_err(|_|"无法读取播放器，已保留现有列表；请重试".to_string());
            if !flag.load(Ordering::Acquire) {let _=sender.send(result);unsafe{let _=PostMessageW(HWND(hwnd),UPDATED,WPARAM(0),LPARAM(0));}}
        })?;
        Ok(Self {
            cancel,
            receiver,
            worker: Some(worker),
        })
    }
}
impl Drop for Discovery {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let action = match msg {
        WM_CLOSE => Some(CLOSE),
        WM_SHOWWINDOW if wp.0 == 0 => Some(CLOSE),
        WM_SIZE if wp.0 == SIZE_MINIMIZED as usize => Some(CLOSE),
        WM_COMMAND => {
            let id = wp.0 & 0xffff;
            let notify = wp.0 >> 16;
            if ((301..=307).contains(&id) || id == CLOSE)
                && (notify == BN_CLICKED as usize
                    || (id == 302 && notify == LBN_SELCHANGE as usize))
            {
                Some(id)
            } else {
                None
            }
        }
        _ => None,
    };
    if let Some(action) = action {
        let _ = PostMessageW(
            GetWindow(hwnd, GW_OWNER),
            COMMAND,
            WPARAM(action),
            LPARAM(hwnd.0),
        );
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
pub struct Dialog {
    pub hwnd: HWND,
    font: HFONT,
    list: HWND,
    status: HWND,
    rows: Vec<Row>,
    automatic: bool,
    valid: bool,
    saving: bool,
    discovery: Option<Discovery>,
}
impl Dialog {
    pub unsafe fn new(owner: HWND, selection: &Selection) -> Result<Self> {
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = w!("IsleNativePlayers");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as isize),
            ..Default::default()
        });
        let scale = GetDpiForWindow(owner) as f32 / 96.;
        let px = |v: f32| (v * scale).round() as i32;
        let style = WS_CAPTION | WS_SYSMENU;
        let mut rect = RECT {
            right: px(560.),
            bottom: px(360.),
            ..Default::default()
        };
        AdjustWindowRectExForDpi(
            &mut rect,
            style,
            false,
            WS_EX_CONTROLPARENT,
            GetDpiForWindow(owner),
        )?;
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class,
            w!("Isle · 播放器选择"),
            style,
            0,
            0,
            rect.right - rect.left,
            rect.bottom - rect.top,
            owner,
            None,
            instance,
            None,
        );
        if hwnd.0 == 0 {
            return Err(Error::from_win32());
        }
        let mut dialog = Self {
            hwnd,
            font: HFONT(0),
            list: HWND(0),
            status: HWND(0),
            rows: vec![],
            automatic: selection.allowed.is_none(),
            valid: true,
            saving: false,
            discovery: None,
        };
        dialog.font = CreateFontW(
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
            w!("Segoe UI"),
        );
        let child = |class: PCWSTR,
                     text: &str,
                     id: usize,
                     style: WINDOW_STYLE,
                     x: f32,
                     y: f32,
                     width: f32,
                     height: f32|
         -> Result<HWND> {
            let control = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                &HSTRING::from(text),
                WS_CHILD | WS_VISIBLE | style,
                px(x),
                px(y),
                px(width),
                px(height),
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
                WPARAM(dialog.font.0 as usize),
                LPARAM(1),
            );
            Ok(control)
        };
        let all = child(
            w!("BUTTON"),
            "允许所有播放器",
            301,
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            20.,
            16.,
            230.,
            26.,
        )?;
        SendMessageW(
            all,
            BM_SETCHECK,
            WPARAM(usize::from(dialog.automatic)),
            LPARAM(0),
        );
        child(
            w!("STATIC"),
            "优先使用排在上方的播放器；未运行项目也会保留",
            0,
            WINDOW_STYLE(0),
            20.,
            48.,
            520.,
            24.,
        )?;
        dialog.list = child(
            w!("LISTBOX"),
            "播放器优先顺序",
            302,
            WS_TABSTOP
                | WS_BORDER
                | WS_VSCROLL
                | WS_HSCROLL
                | WINDOW_STYLE(LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32),
            20.,
            80.,
            405.,
            178.,
        )?;
        SendMessageW(
            dialog.list,
            LB_SETHORIZONTALEXTENT,
            WPARAM(px(1600.) as usize),
            LPARAM(0),
        );
        for (id, label, y) in [
            (304, "上移", 80.),
            (305, "下移", 120.),
            (306, "刷新列表", 176.),
        ] {
            child(w!("BUTTON"), label, id, WS_TABSTOP, 438., y, 102., 30.)?;
        }
        child(
            w!("BUTTON"),
            "启用所选播放器",
            303,
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            20.,
            266.,
            400.,
            26.,
        )?;
        dialog.status = child(
            w!("STATIC"),
            "正在读取播放器…",
            308,
            WINDOW_STYLE(0),
            20.,
            298.,
            520.,
            22.,
        )?;
        child(
            w!("BUTTON"),
            "保存",
            SAVE,
            WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
            320.,
            326.,
            102.,
            28.,
        )?;
        child(
            w!("BUTTON"),
            "取消",
            CLOSE,
            WS_TABSTOP,
            438.,
            326.,
            102.,
            28.,
        )?;
        match rows(selection, &[]) {
            Ok(rows) => dialog.rows = rows,
            Err(error) => {
                dialog.valid = false;
                dialog.message(&error);
            }
        }
        dialog.paint_list(0);
        if dialog.valid {
            dialog.refresh();
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(
            MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
            &mut info,
        )
        .ok()?;
        SetWindowPos(
            hwnd,
            None,
            info.rcWork.left
                + ((info.rcWork.right - info.rcWork.left - (rect.right - rect.left)) / 2).max(0),
            info.rcWork.top
                + ((info.rcWork.bottom - info.rcWork.top - (rect.bottom - rect.top)) / 2).max(0),
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        )?;
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
            SetFocus(all);
        }
        Ok(dialog)
    }
    fn selected(&self) -> Option<usize> {
        let index = unsafe { SendMessageW(self.list, LB_GETCURSEL, WPARAM(0), LPARAM(0)) }.0;
        usize::try_from(index).ok().filter(|i| *i < self.rows.len())
    }
    pub fn selection(&self) -> Selection {
        Selection {
            order: self.rows.iter().map(|r| r.id.clone()).collect(),
            allowed: if self.automatic {
                None
            } else {
                Some(
                    self.rows
                        .iter()
                        .filter(|r| r.enabled)
                        .map(|r| r.id.clone())
                        .collect(),
                )
            },
        }
    }
    pub unsafe fn message(&self, text: &str) {
        let _ = SetWindowTextW(self.status, &HSTRING::from(text));
    }
    unsafe fn paint_list(&self, index: usize) {
        SendMessageW(self.list, LB_RESETCONTENT, WPARAM(0), LPARAM(0));
        for row in &self.rows {
            let text = HSTRING::from(format!(
                "{} {} · {} · {}",
                if self.automatic || row.enabled {
                    "✓"
                } else {
                    "○"
                },
                display_name(&row.id),
                row.id,
                if row.playing {
                    "播放中"
                } else if row.online {
                    "已连接"
                } else {
                    "未运行"
                }
            ));
            SendMessageW(
                self.list,
                LB_ADDSTRING,
                WPARAM(0),
                LPARAM(text.as_ptr() as isize),
            );
        }
        if !self.rows.is_empty() {
            SendMessageW(
                self.list,
                LB_SETCURSEL,
                WPARAM(index.min(self.rows.len() - 1)),
                LPARAM(0),
            );
        }
        self.update_controls();
    }
    unsafe fn update_controls(&self) {
        let selected = self.selected();
        for id in [301, 302, 303, 304, 305, 306, SAVE] {
            let enabled = !self.saving
                && self.valid
                && match id {
                    303 => !self.automatic && selected.is_some(),
                    304 => selected.is_some_and(|i| i > 0),
                    305 => selected.is_some_and(|i| i + 1 < self.rows.len()),
                    306 | SAVE => self.discovery.is_none(),
                    _ => true,
                };
            EnableWindow(GetDlgItem(self.hwnd, id as i32), enabled);
        }
        SendMessageW(
            GetDlgItem(self.hwnd, 303),
            BM_SETCHECK,
            WPARAM(usize::from(
                selected.is_some_and(|i| self.automatic || self.rows[i].enabled),
            )),
            LPARAM(0),
        );
    }
    pub unsafe fn saving(&mut self, saving: bool) {
        self.saving = saving;
        self.update_controls();
        if saving {
            self.message("正在保存…");
        }
    }
    pub unsafe fn action(&mut self, action: usize) {
        if self.saving || !self.valid {
            return;
        }
        let index = self.selected().unwrap_or(0);
        match action {
            301 => {
                self.automatic = SendMessageW(
                    GetDlgItem(self.hwnd, 301),
                    BM_GETCHECK,
                    WPARAM(0),
                    LPARAM(0),
                )
                .0 == 1;
                self.paint_list(index);
            }
            302 => self.update_controls(),
            303 => {
                if let Some(row) = self.rows.get_mut(index) {
                    row.enabled = !row.enabled;
                }
                self.paint_list(index);
            }
            304 if index > 0 => {
                self.rows.swap(index, index - 1);
                self.paint_list(index - 1);
            }
            305 if index + 1 < self.rows.len() => {
                self.rows.swap(index, index + 1);
                self.paint_list(index + 1);
            }
            306 => self.refresh(),
            _ => {}
        }
    }
    unsafe fn refresh(&mut self) {
        if self.discovery.is_some() {
            return;
        }
        match Discovery::start(GetWindow(self.hwnd, GW_OWNER)) {
            Ok(job) => {
                self.discovery = Some(job);
                self.message("正在读取播放器…");
            }
            Err(_) => self.message("无法创建读取任务，请重试"),
        };
        self.update_controls();
    }
    pub unsafe fn take_update(&mut self) {
        let Some(result) = self
            .discovery
            .as_ref()
            .and_then(|job| job.receiver.try_recv().ok())
        else {
            return;
        };
        self.discovery = None;
        match result {
            Ok(live) => match rows(&self.selection(), &live) {
                Ok(rows) => {
                    self.rows = rows;
                    self.paint_list(0);
                    self.message(if self.rows.is_empty() {
                        "未发现播放器；请先打开播放器，再刷新"
                    } else {
                        "选中项目后勾选或调整顺序，再保存"
                    });
                }
                Err(error) => {
                    self.valid = false;
                    self.message(&error);
                }
            },
            Err(error) => self.message(&error),
        }
        self.update_controls();
    }
    pub fn ready(&self) -> bool {
        self.valid && self.discovery.is_none() && !self.saving
    }
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
    pub unsafe fn route(&self, msg: &MSG) -> bool {
        (msg.hwnd == self.hwnd || IsChild(self.hwnd, msg.hwnd).as_bool())
            && IsDialogMessageW(self.hwnd, msg).as_bool()
    }
}
impl Drop for Dialog {
    fn drop(&mut self) {
        self.discovery = None;
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            if self.font.0 != 0 {
                let _ = DeleteObject(self.font);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_merge_preserves_offline_priority_and_explicit_empty_selection() {
        let selection = Selection {
            order: vec!["offline".into(), "b".into()],
            allowed: Some(vec![]),
        };
        let merged = rows(&selection, &[("b".into(), true), ("a".into(), false)]).unwrap();
        assert_eq!(
            merged.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["offline", "b", "a"]
        );
        assert!(merged.iter().all(|r| !r.enabled));
        assert!(!merged[0].online && merged[1].playing);
        assert!(rows(
            &Selection {
                order: (0..257).map(|i| i.to_string()).collect(),
                allowed: None
            },
            &[]
        )
        .is_err());
    }
}
