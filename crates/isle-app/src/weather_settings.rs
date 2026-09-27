//! Modeless Win32 controls: native edit/IME, list selection and dialog keyboard routing.
use isle_core::weather::City;
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::*,
        UI::{
            Controls::EM_LIMITTEXT, HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*,
        },
    },
};
pub const COMMAND: u32 = WM_APP + 74;
pub const SEARCH: usize = 102;
pub const APPLY: usize = 104;
pub const CLOSE: usize = 2;
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let action = match msg {
        WM_CLOSE => Some(CLOSE),
        WM_SHOWWINDOW if wp.0 == 0 => Some(CLOSE),
        WM_SIZE if wp.0 == SIZE_MINIMIZED as usize => Some(CLOSE),
        WM_COMMAND => {
            let id = wp.0 & 0xffff;
            let notify = wp.0 >> 16;
            if id == 103 && notify == LBN_DBLCLK as usize {
                Some(APPLY)
            } else if matches!(id, SEARCH | APPLY | CLOSE) && notify == BN_CLICKED as usize {
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
    font: HFONT,
    pub cities: Vec<City>,
}
impl Settings {
    pub unsafe fn new(owner: HWND) -> Result<Self> {
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
        let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut rect = RECT {
            right: px(540.),
            bottom: px(348.),
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
            w!("Isle 原生设置 · 天气"),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
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
        let mut value = Self {
            hwnd,
            query: HWND(0),
            list: HWND(0),
            status: HWND(0),
            apply: HWND(0),
            font: HFONT(0),
            cities: vec![],
        };
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
            w!("Segoe UI"),
        );
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
                px(x),
                px(y),
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
            Ok(control)
        };
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
    pub unsafe fn query(&self) -> String {
        let mut text = [0u16; 81];
        let len = GetWindowTextW(self.query, &mut text);
        String::from_utf16_lossy(&text[..len.max(0) as usize])
            .trim()
            .to_string()
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
            let _ = DestroyWindow(self.hwnd);
            if self.font.0 != 0 {
                let _ = DeleteObject(self.font);
            }
        }
    }
}
