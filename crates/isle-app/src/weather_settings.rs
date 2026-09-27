//! Modeless Win32 controls: native edit/IME, list selection and dialog keyboard routing.
use isle_core::weather::City;
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::*,
        UI::{
            Controls::{
                InitCommonControlsEx, EM_LIMITTEXT, ICC_BAR_CLASSES, INITCOMMONCONTROLSEX,
                TBS_AUTOTICKS,
            },
            HiDpi::*,
            Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
};
pub const COMMAND: u32 = WM_APP + 74;
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
    if msg == WM_HSCROLL && lp.0 != 0 {
        let track = HWND(lp.0);
        let id = GetDlgCtrlID(track);
        if (SHAPE_CONTROL_BASE as i32..(SHAPE_CONTROL_BASE + 4) as i32).contains(&id) {
            let index = id as usize - SHAPE_CONTROL_BASE;
            let position = SendMessageW(track, WM_USER, WPARAM(0), LPARAM(0)).0;
            let text = HSTRING::from(format!("{}：{} px", SHAPE_NAMES[index], position));
            let _ = SetWindowTextW(GetDlgItem(hwnd, (SHAPE_LABEL_BASE + index) as i32), &text);
            return LRESULT(0);
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
        let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut rect = RECT {
            right: px(540.),
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
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class,
            w!("Isle 原生设置"),
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
            edge_position: HWND(0),
            font: HFONT(0),
            cities: vec![],
            time_zones: vec![],
            styles: vec![],
            edges: vec![],
            shape_controls: vec![],
            shape_labels: vec![],
            fill_color: HWND(0),
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
            let _ = DestroyWindow(self.hwnd);
            if self.font.0 != 0 {
                let _ = DeleteObject(self.font);
            }
        }
    }
}
