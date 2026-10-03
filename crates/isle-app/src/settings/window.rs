use super::{
    controls::{ControlPaintDiagnostics, ControlPainter},
    model::{self, Page},
    render::ShellRender,
};
use isle_core::{
    configuration::{Appearance, Controls},
    settings::SettingsWindowPlacement,
    weather::City,
};
use std::{
    mem::size_of,
    ops::{Deref, DerefMut},
};
use windows::{
    core::{w, Error, Result, HSTRING, PCWSTR},
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{
                GetComboBoxInfo, InitCommonControlsEx, SetScrollInfo, ShowScrollBar, CDDS_PREPAINT,
                CDIS_DISABLED, CDIS_FOCUS, CDIS_HOT, CDIS_SELECTED, CDRF_DODEFAULT,
                CDRF_SKIPDEFAULT, COMBOBOXINFO, DRAWITEMSTRUCT, EM_LIMITTEXT, ICC_BAR_CLASSES,
                INITCOMMONCONTROLSEX, MEASUREITEMSTRUCT, NMCUSTOMDRAW,
                NMCUSTOMDRAW_DRAW_STATE_FLAGS, NMHDR, NM_CUSTOMDRAW, ODS_COMBOBOXEDIT,
                ODS_DISABLED, ODS_FOCUS, ODS_SELECTED, ODT_COMBOBOX, TBM_GETCHANNELRECT,
                TBM_GETNUMTICS, TBM_GETTHUMBRECT, TBM_GETTICPOS, TBS_AUTOTICKS, WM_MOUSELEAVE,
            },
            HiDpi::*,
            Input::KeyboardAndMouse::*,
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
};

pub const COMMAND: u32 = WM_APP + 74;
pub const SEARCH: usize = 102;
pub const APPLY: usize = 104;
pub const APPLY_CONTROLS: usize = 105;
pub const PLAYERS: usize = 107;
pub const APPLY_APPEARANCE: usize = 212;
pub const RETRY_SAVE: usize = 108;
pub const REVERT_SAVED: usize = 109;
pub const INSPECTION_LOCK: usize = 110;
pub const PLACEMENT_CHANGED: usize = 111;
pub const CLOSE: usize = 2;

const WEATHER_QUERY: i32 = 101;
const WEATHER_LIST: i32 = 103;
const WEATHER_STATUS: i32 = 106;
const PANEL: i32 = 200;
const ZONE: i32 = 210;
const STYLE: i32 = 211;
const EDGE: i32 = 213;
const EDGE_POSITION: i32 = 214;
const FILL_COLOR: i32 = 215;
const ALBUM_COLOR: i32 = 216;
const ISLAND_TOPMOST: i32 = 217;
const PLAYER_TOPMOST: i32 = 218;
const SHAPE_CONTROL_BASE: i32 = 220;
const INSPECTION_CHECK: i32 = 240;
const SPECTRUM: i32 = 251;
const SPECTRUM_REALTIME: i32 = 252;
const SPECTRUM_RANDOM: i32 = 253;
const NAV_BASE: i32 = 300;
const CONTROL_RECOVER_COMBO: u32 = WM_APP + 75;
const COMBO_SUBCLASS_ID: usize = 1;
const SHAPE_NAMES: [&str; 4] = ["收起长度", "收起凹肩", "展开凹肩", "展开圆角"];

const fn color_ref(red: u8, green: u8, blue: u8) -> COLORREF {
    COLORREF(((blue as u32) << 16) | ((green as u32) << 8) | red as u32)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveState {
    Saved,
    Saving,
    Dirty,
    Error(String),
}

impl SaveState {
    fn label(&self) -> &str {
        match self {
            Self::Saved => "已保存",
            Self::Saving => "正在保存…",
            Self::Dirty => "待保存",
            Self::Error(_) => "保存失败",
        }
    }
}

#[derive(Clone, Copy)]
struct Placement {
    hwnd: HWND,
    page: Option<Page>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Clone)]
struct PendingComboRecovery {
    old: HWND,
    id: i32,
}

struct Theme {
    shell: ShellRender,
    control_painter: Option<ControlPainter>,
    control_diagnostics: ControlPaintDiagnostics,
    allow_control_test_faults: bool,
    combo_labels: Vec<(i32, Vec<String>)>,
    combo_selection: Vec<(i32, i32)>,
    pending_combo_recoveries: Vec<PendingComboRecovery>,
    failed_combo_recoveries: Vec<i32>,
    page: Page,
    scale: f32,
    instance: HINSTANCE,
    font: HFONT,
    card_brush: HBRUSH,
    controls: Vec<Placement>,
    nav: [HWND; 6],
    query: HWND,
    list: HWND,
    weather_status: HWND,
    zone: HWND,
    style: HWND,
    edge: HWND,
    edge_position: HWND,
    shape_controls: Vec<HWND>,
    shape_labels: Vec<HWND>,
    fill_color: HWND,
    cities: Vec<City>,
    time_zones: Vec<String>,
    styles: Vec<String>,
    edges: Vec<String>,
    save_state: SaveState,
    message: String,
    suppress_changes: bool,
    scroll: f32,
    horizontal_scroll: i32,
    horizontal_scroll_max: i32,
    move_start: Option<SettingsWindowPlacement>,
    pending_placement_update: Option<SettingsWindowPlacement>,
    last_reported_placement: Option<SettingsWindowPlacement>,
}

#[derive(Clone, Copy)]
struct ThemeHandle(*mut Theme);

impl Deref for ThemeHandle {
    type Target = Theme;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0 }
    }
}

impl DerefMut for ThemeHandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.0 }
    }
}

impl Theme {
    fn save_label(&self) -> &str {
        if let SaveState::Error(error) = &self.save_state {
            error
        } else {
            self.save_state.label()
        }
    }
}

impl Drop for Theme {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.card_brush);
            if self.font.0 != 0 {
                let _ = DeleteObject(self.font);
            }
        }
    }
}

// Keep the Win32 creation arguments together here; callers need explicit
// page coordinates to maintain the legacy control map.
#[allow(clippy::too_many_arguments)]
unsafe fn add_control(
    hwnd: HWND,
    mut theme: ThemeHandle,
    class: PCWSTR,
    text: &str,
    id: i32,
    style: WINDOW_STYLE,
    page: Page,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Result<HWND> {
    let scale = theme.scale;
    let instance = theme.instance;
    let font = theme.font;
    let px = |n: f32| (n * scale).round() as i32;
    let control = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        class,
        &HSTRING::from(text),
        WS_CHILD | style,
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
    SendMessageW(control, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(0));
    theme.controls.push(Placement {
        hwnd: control,
        page: Some(page),
        x,
        y,
        width,
        height,
    });
    Ok(control)
}

unsafe fn add_nav(hwnd: HWND, mut theme: ThemeHandle, page: Page) -> Result<HWND> {
    let index = page.index();
    let instance = theme.instance;
    let font = theme.font;
    let control = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("BUTTON"),
        &HSTRING::from(page.title()),
        WS_CHILD | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        0,
        0,
        1,
        1,
        hwnd,
        HMENU((NAV_BASE + index as i32) as isize),
        instance,
        None,
    );
    if control.0 == 0 {
        return Err(Error::from_win32());
    }
    SendMessageW(control, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(0));
    theme.nav[index] = control;
    Ok(control)
}

unsafe fn child_text(
    hwnd: HWND,
    theme: ThemeHandle,
    page: Page,
    text: &str,
    x: f32,
    y: f32,
    w: f32,
) -> Result<HWND> {
    add_control(
        hwnd,
        theme,
        w!("STATIC"),
        text,
        0,
        WINDOW_STYLE(0),
        page,
        x,
        y,
        w,
        22.0,
    )
}

unsafe fn settings_font(scale: f32) -> HFONT {
    CreateFontW(
        -((14.0 * scale).round() as i32),
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
    )
}

#[allow(clippy::too_many_arguments)]
unsafe fn child_checkbox(
    hwnd: HWND,
    theme: ThemeHandle,
    page: Page,
    id: i32,
    text: &str,
    x: f32,
    y: f32,
    w: f32,
) -> Result<HWND> {
    add_control(
        hwnd,
        theme,
        w!("BUTTON"),
        text,
        id,
        WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        page,
        x,
        y,
        w,
        26.0,
    )
}

unsafe fn child_combo(
    hwnd: HWND,
    theme: ThemeHandle,
    page: Page,
    id: i32,
    x: f32,
    y: f32,
    w: f32,
) -> Result<HWND> {
    let owner_draw = theme.control_painter.is_some();
    let combo_style = if owner_draw {
        CBS_DROPDOWNLIST | CBS_OWNERDRAWFIXED | CBS_HASSTRINGS
    } else {
        CBS_DROPDOWNLIST
    };
    let combo = add_control(
        hwnd,
        theme,
        w!("COMBOBOX"),
        "",
        id,
        WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(combo_style as u32),
        page,
        x,
        y,
        w,
        180.0,
    )?;
    if owner_draw && !SetWindowSubclass(combo, Some(combo_subclass), COMBO_SUBCLASS_ID, 0).as_bool()
    {
        return Err(Error::from_win32());
    }
    Ok(combo)
}

unsafe fn set_combo_items(
    mut theme: ThemeHandle,
    combo: HWND,
    id: i32,
    values: impl IntoIterator<Item = String>,
) {
    let labels = values.into_iter().collect::<Vec<_>>();
    let owner_draw = theme.control_painter.is_some();
    let scale = theme.scale;
    SendMessageW(combo, WM_SETREDRAW, WPARAM(0), LPARAM(0));
    for label in &labels {
        let text = HSTRING::from(label);
        SendMessageW(
            combo,
            CB_ADDSTRING,
            WPARAM(0),
            LPARAM(text.as_ptr() as isize),
        );
    }
    SendMessageW(combo, WM_SETREDRAW, WPARAM(1), LPARAM(0));
    let _ = InvalidateRect(combo, None, false);
    if owner_draw {
        let item_height = (28.0 * scale).round().max(1.0) as isize;
        SendMessageW(combo, CB_SETITEMHEIGHT, WPARAM(0), LPARAM(item_height));
        SendMessageW(
            combo,
            CB_SETITEMHEIGHT,
            WPARAM(usize::MAX),
            LPARAM(item_height),
        );
    }
    theme.combo_labels.push((id, labels));
    theme.combo_selection.push((id, 0));
}

fn page_content_bottom(page: Page) -> f32 {
    match page {
        Page::General => 630.0,
        Page::Appearance => 650.0,
        Page::Modules => 500.0,
        Page::Media => 290.0,
        Page::Weather => 530.0,
        Page::Advanced => 280.0,
    }
}

fn horizontal_scroll_limit(content_right_dip: f32, client_width_dip: f32) -> i32 {
    let viewport_right = (client_width_dip - 16.0).max(super::model::CONTENT_LEFT + 1.0);
    (content_right_dip - viewport_right).ceil().max(0.0) as i32
}

fn min_track_px(requested: i32, work_area: i32) -> i32 {
    requested.max(1).min(work_area.max(1))
}

unsafe fn position_navigation(hwnd: HWND, scale: f32, nav: [HWND; 6]) {
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let height = (client.bottom - client.top).max(0) as f32 / scale;
    for (nav, bounds) in nav.iter().zip(model::navigation_layout(height)) {
        if nav.0 == 0 {
            continue;
        }
        let _ = SetWindowPos(
            *nav,
            None,
            (bounds.x * scale).round() as i32,
            (bounds.y * scale).round() as i32,
            (bounds.w * scale).round().max(1.0) as i32,
            (bounds.h * scale).round().max(1.0) as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
        ShowWindow(*nav, SW_SHOWNOACTIVATE);
        let _ = EnableWindow(*nav, true);
    }
}

unsafe fn position_controls(hwnd: HWND, theme_ptr: *mut Theme) {
    if theme_ptr.is_null() {
        return;
    }
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let (scale, page, scroll, horizontal_scroll, horizontal_scroll_max, controls, nav, scroll_info) = {
        let theme = &mut *theme_ptr;
        let client_width = (client.right - client.left).max(0) as f32 / theme.scale;
        let content_right = theme
            .controls
            .iter()
            .filter(|placement| placement.page == Some(theme.page))
            .map(|placement| placement.x + placement.width)
            .fold(super::model::WINDOW_WIDTH - 28.0, f32::max);
        let extent = (content_right - super::model::CONTENT_LEFT).ceil().max(0.0) as i32;
        let viewport = (client_width - super::model::CONTENT_LEFT - 16.0)
            .floor()
            .max(1.0) as i32;
        theme.horizontal_scroll_max = horizontal_scroll_limit(content_right, client_width);
        theme.horizontal_scroll = theme
            .horizontal_scroll
            .clamp(0, theme.horizontal_scroll_max);
        let height = (client.bottom - client.top) as f32 / theme.scale;
        let viewport_bottom = (height - 58.0).max(super::model::PAGE_TOP + 1.0);
        theme.scroll = theme.scroll.clamp(
            0.0,
            (page_content_bottom(theme.page) - viewport_bottom).max(0.0),
        );
        let scroll_info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMin: 0,
            nMax: extent,
            nPage: viewport as u32,
            nPos: theme.horizontal_scroll,
            ..Default::default()
        };
        (
            theme.scale,
            theme.page,
            theme.scroll,
            theme.horizontal_scroll,
            theme.horizontal_scroll_max,
            theme.controls.clone(),
            theme.nav,
            scroll_info,
        )
    };
    let _ = SetScrollInfo(hwnd, SB_HORZ, &scroll_info, true);
    let _ = ShowScrollBar(hwnd, SB_HORZ, horizontal_scroll_max > 0);
    let viewport_right = (client.right - client.left) as f32 / scale;
    for placement in &controls {
        let is_content = placement.x >= super::model::CONTENT_LEFT;
        let logical_x = if is_content {
            placement.x - horizontal_scroll as f32
        } else {
            placement.x
        };
        let x = (logical_x * scale).round() as i32;
        let y = ((placement.y - scroll) * scale).round() as i32;
        let w = (placement.width * scale).round() as i32;
        let h = (placement.height * scale).round() as i32;
        let _ = SetWindowPos(
            placement.hwnd,
            None,
            x,
            y,
            w,
            h,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
        let inside_content = !is_content
            || (logical_x < viewport_right
                && logical_x + placement.width > super::model::CONTENT_LEFT);
        if is_content {
            let child_left = x;
            let clip_left =
                ((super::model::CONTENT_LEFT * scale).round() as i32 - child_left).max(0);
            let clip_right = ((client.right - client.left) - child_left).min(w);
            if inside_content && clip_left < clip_right {
                let region = CreateRectRgn(clip_left, 0, clip_right, h.max(1));
                if region.0 != 0 && SetWindowRgn(placement.hwnd, region, true) == 0 {
                    let _ = DeleteObject(HGDIOBJ(region.0));
                }
            } else {
                let _ = SetWindowRgn(placement.hwnd, HRGN(0), true);
            }
        }
        if placement.page == Some(page) && inside_content {
            ShowWindow(placement.hwnd, SW_SHOWNOACTIVATE);
        } else {
            ShowWindow(placement.hwnd, SW_HIDE);
        }
    }
    position_navigation(hwnd, scale, nav);
    let _ = InvalidateRect(hwnd, None, false);
}

unsafe fn set_horizontal_scroll(hwnd: HWND, theme_ptr: *mut Theme, position: i32) {
    if theme_ptr.is_null() {
        return;
    }
    {
        let theme = &mut *theme_ptr;
        theme.horizontal_scroll = position.clamp(0, theme.horizontal_scroll_max);
    }
    position_controls(hwnd, theme_ptr);
}

unsafe fn set_page(hwnd: HWND, theme_ptr: *mut Theme, page: Page) {
    if theme_ptr.is_null() || (*theme_ptr).page == page {
        return;
    }
    if page != Page::Appearance && get_check(hwnd, INSPECTION_CHECK) {
        set_check(hwnd, INSPECTION_CHECK, false);
        post_command(hwnd, INSPECTION_LOCK);
    }
    {
        let theme = &mut *theme_ptr;
        theme.page = page;
        theme.scroll = 0.0;
        theme.horizontal_scroll = 0;
    }
    position_controls(hwnd, theme_ptr);
}

unsafe fn post_command(hwnd: HWND, command: usize) {
    let owner = GetWindow(hwnd, GW_OWNER);
    let _ = PostMessageW(owner, COMMAND, WPARAM(command), LPARAM(hwnd.0));
}

fn combo_slot(theme: &Theme, id: i32) -> HWND {
    match id {
        ZONE => theme.zone,
        STYLE => theme.style,
        EDGE => theme.edge,
        EDGE_POSITION => theme.edge_position,
        _ => HWND(0),
    }
}

fn combo_labels(theme: &Theme, id: i32) -> Option<&[String]> {
    theme
        .combo_labels
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .map(|(_, labels)| labels.as_slice())
}

fn combo_selection(theme: &Theme, id: i32) -> i32 {
    theme
        .combo_selection
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .map(|(_, selection)| *selection)
        .unwrap_or(0)
}

unsafe fn update_combo_selection(theme: &mut Theme, id: i32, selection: i32) {
    if let Some((_, current)) = theme
        .combo_selection
        .iter_mut()
        .find(|(candidate, _)| *candidate == id)
    {
        *current = selection.max(0);
    }
}

unsafe fn schedule_all_combo_recoveries(hwnd: HWND, theme: &mut Theme) {
    let before = theme.pending_combo_recoveries.len();
    for id in [ZONE, STYLE, EDGE, EDGE_POSITION] {
        if theme.control_painter.is_some() && combo_slot(theme, id).0 != 0 {
            queue_combo_recovery(hwnd, theme, id);
        }
    }
    theme.control_diagnostics.native_fallbacks +=
        (theme.pending_combo_recoveries.len() - before) as u64;
}

unsafe fn count_control_paint(theme: &mut Theme, role: super::controls::ControlRole) {
    match role {
        super::controls::ControlRole::Button => theme.control_diagnostics.button_paints += 1,
        super::controls::ControlRole::Navigation => {
            theme.control_diagnostics.nav_button_paints += 1
        }
        super::controls::ControlRole::Toggle => theme.control_diagnostics.toggle_paints += 1,
        super::controls::ControlRole::Segmented => theme.control_diagnostics.segment_paints += 1,
        super::controls::ControlRole::Slider => theme.control_diagnostics.slider_paints += 1,
        super::controls::ControlRole::Dropdown => theme.control_diagnostics.dropdown_paints += 1,
    }
    sync_control_diagnostics(theme);
}

fn sync_control_diagnostics(theme: &mut Theme) {
    if let Some(painter) = &theme.control_painter {
        let generation = painter.generation();
        if generation > theme.control_diagnostics.generation {
            theme.control_diagnostics.target_recreates +=
                generation - theme.control_diagnostics.generation;
        }
        theme.control_diagnostics.renderer_enabled = painter.is_enabled();
        theme.control_diagnostics.generation = generation;
        theme.control_diagnostics.high_contrast = painter.high_contrast();
    } else {
        theme.control_diagnostics.renderer_enabled = false;
    }
}

unsafe fn queue_combo_recovery(parent: HWND, theme: &mut Theme, id: i32) {
    let Some(placement) = theme
        .controls
        .iter()
        .find(|placement| GetDlgCtrlID(placement.hwnd) == id)
        .copied()
    else {
        return;
    };
    if theme.failed_combo_recoveries.contains(&id)
        || theme
            .pending_combo_recoveries
            .iter()
            .any(|pending| pending.id == id)
    {
        return;
    }
    let old = placement.hwnd;
    theme
        .pending_combo_recoveries
        .push(PendingComboRecovery { old, id });
    let _ = PostMessageW(parent, CONTROL_RECOVER_COMBO, WPARAM(0), LPARAM(0));
}

unsafe fn recover_next_combo(parent: HWND, theme_ptr: *mut Theme) -> bool {
    if theme_ptr.is_null() {
        return false;
    }
    let Some((
        pending,
        placement,
        scale,
        vertical_scroll,
        horizontal_scroll,
        font,
        instance,
        labels,
    )) = ({
        let theme = &mut *theme_ptr;
        if theme.pending_combo_recoveries.is_empty() {
            None
        } else {
            let pending = theme.pending_combo_recoveries.remove(0);
            let placement = theme
                .controls
                .iter()
                .find(|placement| placement.hwnd == pending.old)
                .copied();
            placement.map(|placement| {
                let labels = combo_labels(theme, pending.id).unwrap_or_default().to_vec();
                (
                    pending,
                    placement,
                    theme.scale,
                    theme.scroll,
                    theme.horizontal_scroll,
                    theme.font,
                    theme.instance,
                    labels,
                )
            })
        }
    })
    else {
        return false;
    };

    // Read the live HWND state when the deferred recovery actually runs so a
    // user action after the paint failure is retained.
    let selection = SendMessageW(pending.old, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32;
    let enabled = IsWindowEnabled(pending.old).as_bool();
    let visible = IsWindowVisible(pending.old).as_bool();
    let focused = GetFocus() == pending.old;
    let old_style = GetWindowLongW(pending.old, GWL_STYLE) as u32;
    let old_ex_style = GetWindowLongW(pending.old, GWL_EXSTYLE) as u32;

    // Native calls are deliberately outside the Theme borrow: creating and populating a
    // combobox may synchronously request owner drawing from its parent.
    let content_x = if placement.x >= super::model::CONTENT_LEFT {
        placement.x - horizontal_scroll as f32
    } else {
        placement.x
    };
    let x = (content_x * scale).round() as i32;
    let y = ((placement.y - vertical_scroll) * scale).round() as i32;
    let width = (placement.width * scale).round() as i32;
    let height = (placement.height * scale).round() as i32;
    let replacement_style = old_style & !(CBS_OWNERDRAWFIXED as u32) & !(CBS_HASSTRINGS as u32);
    let replacement = CreateWindowExW(
        WINDOW_EX_STYLE(old_ex_style),
        w!("COMBOBOX"),
        w!(""),
        WINDOW_STYLE(replacement_style),
        x,
        y,
        width,
        height,
        parent,
        HMENU(pending.id as isize),
        instance,
        None,
    );
    if replacement.0 == 0 {
        let theme = &mut *theme_ptr;
        theme.failed_combo_recoveries.push(pending.id);
        theme.control_diagnostics.native_fallbacks += 1;
        return true;
    }
    SendMessageW(replacement, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(0));
    for label in &labels {
        let value = HSTRING::from(label);
        SendMessageW(
            replacement,
            CB_ADDSTRING,
            WPARAM(0),
            LPARAM(value.as_ptr() as isize),
        );
    }
    if !labels.is_empty() && selection >= 0 {
        SendMessageW(
            replacement,
            CB_SETCURSEL,
            WPARAM(selection.clamp(0, labels.len() as i32 - 1) as usize),
            LPARAM(0),
        );
    }
    EnableWindow(replacement, enabled);
    let _ = SetWindowPos(
        replacement,
        pending.old,
        x,
        y,
        width,
        height,
        SWP_NOACTIVATE,
    );
    let _ = DestroyWindow(pending.old);

    {
        let theme = &mut *theme_ptr;
        if let Some(placement) = theme
            .controls
            .iter_mut()
            .find(|placement| placement.hwnd == pending.old)
        {
            placement.hwnd = replacement;
        }
        match pending.id {
            ZONE => theme.zone = replacement,
            STYLE => theme.style = replacement,
            EDGE => theme.edge = replacement,
            EDGE_POSITION => theme.edge_position = replacement,
            _ => {}
        }
        update_combo_selection(theme, pending.id, selection);
        theme.control_diagnostics.combo_recoveries += 1;
    }
    if focused {
        let _ = SetFocus(replacement);
    }
    if placement.x >= super::model::CONTENT_LEFT {
        let mut client = RECT::default();
        if GetClientRect(parent, &mut client).is_ok() {
            let clip_left = ((super::model::CONTENT_LEFT * scale).round() as i32 - x).max(0);
            let clip_right = ((client.right - client.left) - x).min(width);
            if clip_left < clip_right {
                let region = CreateRectRgn(clip_left, 0, clip_right, height.max(1));
                if region.0 != 0 && SetWindowRgn(replacement, region, true) == 0 {
                    let _ = DeleteObject(HGDIOBJ(region.0));
                }
            }
        }
    }
    if visible {
        ShowWindow(replacement, SW_SHOWNOACTIVATE);
    }
    true
}

unsafe extern "system" fn combo_subclass(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _subclass_id: usize,
    _reference_data: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(combo_subclass), COMBO_SUBCLASS_ID);
        return DefSubclassProc(hwnd, msg, wp, lp);
    }

    let result = DefSubclassProc(hwnd, msg, wp, lp);
    if msg == WM_PAINT {
        let parent = GetParent(hwnd);
        let id = GetDlgCtrlID(hwnd);
        if matches!(id, ZONE | STYLE | EDGE | EDGE_POSITION) {
            let theme_ptr = GetWindowLongPtrW(parent, GWLP_USERDATA) as *mut Theme;
            if !theme_ptr.is_null() {
                let mut combo_info = COMBOBOXINFO {
                    cbSize: size_of::<COMBOBOXINFO>() as u32,
                    ..Default::default()
                };
                if GetComboBoxInfo(hwnd, &mut combo_info).is_ok() {
                    let mut client = RECT::default();
                    let _ = GetClientRect(hwnd, &mut client);
                    let item_bounds = combo_info.rcItem;
                    let button_bounds = combo_info.rcButton;
                    if rect_inside(item_bounds, client) && rect_inside(button_bounds, client) {
                        let dc = GetDC(hwnd);
                        if dc.0 == 0 {
                            queue_combo_recovery(parent, &mut *theme_ptr, id);
                            return result;
                        }
                        let focused = GetFocus() == hwnd;
                        let dropped =
                            SendMessageW(hwnd, CB_GETDROPPEDSTATE, WPARAM(0), LPARAM(0)).0 != 0;
                        let disabled = !IsWindowEnabled(hwnd).as_bool();
                        let mut hot = dropped || focused;
                        let mut cursor = POINT::default();
                        if GetCursorPos(&mut cursor).is_ok()
                            && ScreenToClient(hwnd, &mut cursor).as_bool()
                        {
                            hot |= cursor.x >= client.left
                                && cursor.x < client.right
                                && cursor.y >= client.top
                                && cursor.y < client.bottom;
                        }
                        let state = super::controls::ControlState {
                            checked: false,
                            indeterminate: false,
                            hot,
                            pressed: dropped,
                            focused,
                            disabled,
                            active: dropped,
                        };
                        let selection =
                            SendMessageW(hwnd, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32;
                        let label = {
                            let theme = &*theme_ptr;
                            combo_labels(theme, id)
                                .and_then(|labels| {
                                    usize::try_from(selection)
                                        .ok()
                                        .and_then(|index| labels.get(index))
                                })
                                .cloned()
                                .unwrap_or_default()
                        };
                        let failed = {
                            let theme = &mut *theme_ptr;
                            let painted = theme.control_painter.as_mut().is_some_and(|painter| {
                                painter
                                    .draw_dropdown_control(
                                        dc,
                                        client,
                                        item_bounds,
                                        button_bounds,
                                        &label,
                                        state,
                                    )
                                    .is_ok()
                            });
                            if painted {
                                theme.control_diagnostics.dropdown_chrome_paints += 1;
                                sync_control_diagnostics(theme);
                                false
                            } else {
                                sync_control_diagnostics(theme);
                                theme.control_diagnostics.native_fallbacks += 1;
                                true
                            }
                        };
                        if failed {
                            queue_combo_recovery(parent, &mut *theme_ptr, id);
                        }
                        let _ = ReleaseDC(hwnd, dc);
                    } else {
                        queue_combo_recovery(parent, &mut *theme_ptr, id);
                    }
                } else {
                    queue_combo_recovery(parent, &mut *theme_ptr, id);
                }
            }
        }
    } else if msg == WM_MOUSEMOVE {
        let mut track = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        let _ = TrackMouseEvent(&mut track);
        let _ = InvalidateRect(hwnd, None, false);
    } else if matches!(msg, WM_MOUSELEAVE | WM_ENABLE | WM_SETFOCUS | WM_KILLFOCUS) {
        let _ = InvalidateRect(hwnd, None, false);
    }
    result
}

fn rect_inside(inner: RECT, outer: RECT) -> bool {
    inner.left >= outer.left
        && inner.top >= outer.top
        && inner.right <= outer.right
        && inner.bottom <= outer.bottom
        && inner.right > inner.left
        && inner.bottom > inner.top
}

unsafe fn draw_combo_item(parent: HWND, theme_ptr: *mut Theme, item: &DRAWITEMSTRUCT) -> bool {
    if item.CtlType != ODT_COMBOBOX || theme_ptr.is_null() {
        return false;
    }
    let id = item.CtlID as i32;
    if !matches!(id, ZONE | STYLE | EDGE | EDGE_POSITION) {
        return false;
    }
    let theme = &mut *theme_ptr;
    let labels = combo_labels(theme, id).unwrap_or_default();
    let selection_field = item.itemState.0 & ODS_COMBOBOXEDIT.0 != 0 || item.itemID == u32::MAX;
    let index = if selection_field {
        combo_selection(theme, id)
    } else {
        item.itemID as i32
    };
    let label = labels
        .get(index.max(0) as usize)
        .cloned()
        .unwrap_or_default();
    let scale = theme.scale;
    let font = theme.font;
    let state = super::controls::ControlState {
        checked: false,
        indeterminate: false,
        hot: item.itemState.0 & ODS_SELECTED.0 != 0,
        pressed: item.itemState.0 & ODS_SELECTED.0 != 0,
        focused: item.itemState.0 & ODS_FOCUS.0 != 0,
        disabled: item.itemState.0 & ODS_DISABLED.0 != 0,
        active: selection_field && (item.itemState.0 & ODS_FOCUS.0 != 0),
    };
    let result = theme.control_painter.as_mut().map(|painter| {
        painter.draw_dropdown(item.hDC, item.rcItem, &label, state, selection_field)
    });
    match result {
        Some(Ok(())) => {
            theme.control_diagnostics.dropdown_paints += 1;
        }
        Some(Err(_)) | None => {
            sync_control_diagnostics(theme);
            theme.control_diagnostics.native_fallbacks += 1;
            queue_combo_recovery(parent, theme, id);
            draw_native_combo_fallback(item, &label, scale, font);
        }
    }
    true
}

unsafe fn draw_native_combo_fallback(item: &DRAWITEMSTRUCT, label: &str, scale: f32, font: HFONT) {
    let dc = item.hDC;
    let rect = item.rcItem;
    let brush = GetSysColorBrush(COLOR_WINDOW);
    let _ = FillRect(dc, &rect, brush);
    let old_font = SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, COLORREF(GetSysColor(COLOR_WINDOWTEXT)));
    let mut wide = label.encode_utf16().collect::<Vec<_>>();
    let mut text_rect = rect;
    text_rect.left += (8.0 * scale).round() as i32;
    if item.itemState.0 & ODS_COMBOBOXEDIT.0 != 0 || item.itemID == u32::MAX {
        text_rect.right -= (32.0 * scale).round() as i32;
    }
    let _ = DrawTextW(
        dc,
        &mut wide,
        &mut text_rect,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    let _ = SelectObject(dc, old_font);
}

unsafe fn custom_draw(theme_ptr: *mut Theme, hdr: &NMHDR) -> Option<LRESULT> {
    if hdr.code != NM_CUSTOMDRAW || theme_ptr.is_null() {
        return None;
    }
    let custom = &*(hdr as *const NMHDR as *const NMCUSTOMDRAW);
    if custom.dwDrawStage != CDDS_PREPAINT {
        return None;
    }
    let id = hdr.idFrom as i32;
    if (SHAPE_CONTROL_BASE..SHAPE_CONTROL_BASE + 4).contains(&id) {
        let mut channel = RECT::default();
        let mut thumb = RECT::default();
        SendMessageW(
            hdr.hwndFrom,
            TBM_GETCHANNELRECT,
            WPARAM(0),
            LPARAM((&mut channel as *mut RECT) as isize),
        );
        SendMessageW(
            hdr.hwndFrom,
            TBM_GETTHUMBRECT,
            WPARAM(0),
            LPARAM((&mut thumb as *mut RECT) as isize),
        );
        let num_ticks = SendMessageW(hdr.hwndFrom, TBM_GETNUMTICS, WPARAM(0), LPARAM(0))
            .0
            .clamp(0, 128);
        let ticks = (0..num_ticks)
            .map(|index| {
                SendMessageW(
                    hdr.hwndFrom,
                    TBM_GETTICPOS,
                    WPARAM(index as usize),
                    LPARAM(0),
                )
                .0 as i32
            })
            .filter(|position| *position >= 0)
            .collect::<Vec<_>>();
        let state = control_state(custom.uItemState, false, false);
        let mut bounds = RECT::default();
        let _ = GetClientRect(hdr.hwndFrom, &mut bounds);
        let theme = &mut *theme_ptr;
        let result = theme
            .control_painter
            .as_mut()
            .map(|painter| painter.draw_slider(custom.hdc, bounds, channel, thumb, &ticks, state));
        if result.is_some_and(|result| result.is_ok()) {
            count_control_paint(theme, super::controls::ControlRole::Slider);
            Some(LRESULT(CDRF_SKIPDEFAULT as isize))
        } else {
            sync_control_diagnostics(theme);
            theme.control_diagnostics.native_fallbacks += 1;
            Some(LRESULT(CDRF_DODEFAULT as isize))
        }
    } else {
        let button_style = GetWindowLongW(hdr.hwndFrom, GWL_STYLE) as u32;
        let button_type = (button_style & BS_TYPEMASK as u32) as i32;
        let role = if (NAV_BASE..NAV_BASE + 6).contains(&id) {
            super::controls::ControlRole::Navigation
        } else if matches!(
            button_type,
            BS_AUTOCHECKBOX | BS_CHECKBOX | BS_AUTO3STATE | BS_3STATE
        ) {
            super::controls::ControlRole::Toggle
        } else if matches!(button_type, BS_AUTORADIOBUTTON | BS_RADIOBUTTON) {
            super::controls::ControlRole::Segmented
        } else {
            super::controls::ControlRole::Button
        };
        let check = SendMessageW(hdr.hwndFrom, BM_GETCHECK, WPARAM(0), LPARAM(0)).0;
        let label = read_text(hdr.hwndFrom, 256);
        let mut bounds = RECT::default();
        let _ = GetClientRect(hdr.hwndFrom, &mut bounds);
        let mut state = control_state(
            custom.uItemState,
            check == 1,
            role == super::controls::ControlRole::Navigation
                && (NAV_BASE..NAV_BASE + 6).contains(&id),
        );
        state.checked = check == 1;
        state.indeterminate = check == 2;
        let theme = &mut *theme_ptr;
        if role == super::controls::ControlRole::Navigation {
            state.active = id - NAV_BASE == theme.page.index() as i32;
        }
        let result = theme
            .control_painter
            .as_mut()
            .map(|painter| painter.draw_button(custom.hdc, bounds, &label, role, state));
        if result.is_some_and(|result| result.is_ok()) {
            count_control_paint(theme, role);
            Some(LRESULT(CDRF_SKIPDEFAULT as isize))
        } else {
            sync_control_diagnostics(theme);
            theme.control_diagnostics.native_fallbacks += 1;
            Some(LRESULT(CDRF_DODEFAULT as isize))
        }
    }
}

fn control_state(
    flags: NMCUSTOMDRAW_DRAW_STATE_FLAGS,
    checked: bool,
    active: bool,
) -> super::controls::ControlState {
    super::controls::ControlState {
        checked,
        indeterminate: false,
        hot: flags.0 & CDIS_HOT.0 != 0,
        pressed: flags.0 & CDIS_SELECTED.0 != 0,
        focused: flags.0 & CDIS_FOCUS.0 != 0,
        disabled: flags.0 & CDIS_DISABLED.0 != 0,
        active,
    }
}

unsafe fn handle_reentrant_message(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    theme_ptr: *mut Theme,
) -> Option<LRESULT> {
    match msg {
        WM_SIZE => {
            {
                let theme = &mut *theme_ptr;
                let _ = theme
                    .shell
                    .resize((lp.0 & 0xffff) as i32, ((lp.0 >> 16) & 0xffff) as i32);
            }
            position_controls(hwnd, theme_ptr);
            Some(LRESULT(0))
        }
        WM_DPICHANGED if lp.0 != 0 => {
            let new_scale = ((wp.0 & 0xffff) as f32 / 96.0).max(0.5);
            let suggested = *(lp.0 as *const RECT);
            let suggested_rect = PixelRect::from_win32(suggested);
            let mut work_info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let adjusted = if GetMonitorInfoW(
                MonitorFromRect(&suggested, MONITOR_DEFAULTTONEAREST),
                &mut work_info,
            )
            .as_bool()
            {
                suggested_rect
                    .and_then(|rect| {
                        PixelRect::from_win32(work_info.rcWork)
                            .and_then(|work| rect.clamp_inside(work))
                    })
                    .unwrap_or_else(|| PixelRect::from_win32(suggested).unwrap_or_default())
            } else {
                suggested_rect.unwrap_or_default()
            };
            let _ = SetWindowPos(
                hwnd,
                None,
                adjusted.left,
                adjusted.top,
                adjusted.width().max(1),
                adjusted.height().max(1),
                SWP_NOACTIVATE | SWP_NOZORDER,
            );

            let font = settings_font(new_scale);
            let (old_font, controls, nav) = {
                let theme = &mut *theme_ptr;
                theme.scale = new_scale;
                theme.shell.set_dpi(new_scale);
                if let Some(painter) = theme.control_painter.as_mut() {
                    painter.set_dpi(new_scale);
                }
                let old_font = std::mem::replace(&mut theme.font, font);
                (
                    old_font,
                    theme.controls.iter().map(|p| p.hwnd).collect::<Vec<_>>(),
                    theme.nav,
                )
            };
            for control in controls.iter().copied().chain(nav) {
                SendMessageW(control, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(0));
                let _ = InvalidateRect(control, None, false);
            }
            if old_font.0 != 0 {
                let _ = DeleteObject(old_font);
            }
            position_controls(hwnd, theme_ptr);
            Some(LRESULT(0))
        }
        WM_HSCROLL if lp.0 == 0 => {
            let action = (wp.0 & 0xffff) as u32;
            let mut info = SCROLLINFO {
                cbSize: size_of::<SCROLLINFO>() as u32,
                fMask: SIF_TRACKPOS,
                ..Default::default()
            };
            let _ = GetScrollInfo(hwnd, SB_HORZ, &mut info);
            let mut client = RECT::default();
            let _ = GetClientRect(hwnd, &mut client);
            let (scale, current, max) = {
                let theme = &*theme_ptr;
                (
                    theme.scale,
                    theme.horizontal_scroll,
                    theme.horizontal_scroll_max,
                )
            };
            let page =
                (((client.right - client.left) as f32 / scale) - super::model::CONTENT_LEFT - 16.0)
                    .round()
                    .max(1.0) as i32;
            let line = 48;
            let next = match action {
                value if value == SB_LINELEFT.0 as u32 => current.saturating_sub(line),
                value if value == SB_LINERIGHT.0 as u32 => current.saturating_add(line),
                value if value == SB_PAGELEFT.0 as u32 => current.saturating_sub(page),
                value if value == SB_PAGERIGHT.0 as u32 => current.saturating_add(page),
                value if value == SB_THUMBPOSITION.0 as u32 || value == SB_THUMBTRACK.0 as u32 => {
                    info.nTrackPos
                }
                value if value == SB_LEFT.0 as u32 => 0,
                value if value == SB_RIGHT.0 as u32 => max,
                _ => current,
            };
            set_horizontal_scroll(hwnd, theme_ptr, next);
            Some(LRESULT(0))
        }
        WM_HSCROLL if lp.0 != 0 => {
            let track = HWND(lp.0);
            let id = GetDlgCtrlID(track);
            let index = (id - SHAPE_CONTROL_BASE) as usize;
            let (suppressed, label) = {
                let theme = &*theme_ptr;
                (
                    theme.suppress_changes,
                    theme.shape_labels.get(index).copied(),
                )
            };
            if (SHAPE_CONTROL_BASE..SHAPE_CONTROL_BASE + 4).contains(&id) && !suppressed {
                // TBM_GETPOS is WM_USER in the Win32 trackbar contract.
                let position = SendMessageW(track, WM_USER, WPARAM(0), LPARAM(0)).0;
                if let Some(label) = label {
                    let text = HSTRING::from(format!("{}：{}", SHAPE_NAMES[index], position));
                    let _ = SetWindowTextW(label, &text);
                }
                post_command(hwnd, APPLY_APPEARANCE);
            }
            Some(LRESULT(0))
        }
        WM_THEMECHANGED | WM_SYSCOLORCHANGE | WM_SETTINGCHANGE => {
            let controls = {
                let theme = &mut *theme_ptr;
                if let Some(painter) = theme.control_painter.as_mut() {
                    painter.refresh_high_contrast();
                    theme.control_diagnostics.high_contrast = painter.high_contrast();
                    theme.control_diagnostics.generation = painter.generation();
                    theme.control_diagnostics.renderer_enabled = painter.is_enabled();
                }
                theme
                    .controls
                    .iter()
                    .map(|p| p.hwnd)
                    .chain(theme.nav)
                    .chain([theme.zone, theme.style, theme.edge, theme.edge_position])
                    .collect::<Vec<_>>()
            };
            for control in controls {
                let _ = InvalidateRect(control, None, false);
            }
            let _ = InvalidateRect(hwnd, None, false);
            Some(LRESULT(0))
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN | WM_CTLCOLORLISTBOX => {
            let dc = HDC(wp.0 as isize);
            let (high_contrast, card_brush) = {
                let theme = &*theme_ptr;
                (
                    theme
                        .control_painter
                        .as_ref()
                        .map(ControlPainter::high_contrast)
                        .unwrap_or_else(super::controls::high_contrast_enabled),
                    theme.card_brush,
                )
            };
            if high_contrast {
                SetTextColor(dc, COLORREF(GetSysColor(COLOR_WINDOWTEXT)));
                SetBkColor(dc, COLORREF(GetSysColor(COLOR_WINDOW)));
                if msg == WM_CTLCOLORSTATIC {
                    SetBkMode(dc, TRANSPARENT);
                }
                Some(LRESULT(GetSysColorBrush(COLOR_WINDOW).0))
            } else {
                SetTextColor(dc, color_ref(245, 247, 250));
                SetBkColor(dc, color_ref(23, 28, 35));
                if msg == WM_CTLCOLORSTATIC {
                    SetBkMode(dc, TRANSPARENT);
                }
                Some(LRESULT(card_brush.0))
            }
        }
        WM_COMMAND => {
            let id = (wp.0 & 0xffff) as i32;
            let notify = wp.0 >> 16;
            if matches!(id, ZONE | STYLE | EDGE | EDGE_POSITION) && notify == CBN_SELCHANGE as usize
            {
                let selection =
                    SendMessageW(HWND(lp.0), CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32;
                update_combo_selection(&mut *theme_ptr, id, selection);
            }
            if (NAV_BASE..NAV_BASE + 6).contains(&id) && notify == BN_CLICKED as usize {
                if let Some(page) = Page::ALL.get((id - NAV_BASE) as usize).copied() {
                    set_page(hwnd, theme_ptr, page);
                }
                return Some(LRESULT(0));
            }
            if id == SEARCH as i32 && notify == BN_CLICKED as usize {
                post_command(hwnd, SEARCH);
                return Some(LRESULT(0));
            }
            if id == WEATHER_LIST
                && (notify == LBN_SELCHANGE as usize || notify == LBN_DBLCLK as usize)
            {
                post_command(hwnd, APPLY);
                return Some(LRESULT(0));
            }
            if id == PLAYERS as i32 && notify == BN_CLICKED as usize {
                post_command(hwnd, PLAYERS);
                return Some(LRESULT(0));
            }
            if id == RETRY_SAVE as i32 && notify == BN_CLICKED as usize {
                post_command(hwnd, RETRY_SAVE);
                return Some(LRESULT(0));
            }
            if id == REVERT_SAVED as i32 && notify == BN_CLICKED as usize {
                post_command(hwnd, REVERT_SAVED);
                return Some(LRESULT(0));
            }
            if id == INSPECTION_CHECK && notify == BN_CLICKED as usize {
                post_command(hwnd, INSPECTION_LOCK);
                return Some(LRESULT(0));
            }
            let suppressed = (*theme_ptr).suppress_changes;
            if suppressed {
                return Some(DefWindowProcW(hwnd, msg, wp, lp));
            }
            if (((PANEL..=209).contains(&id) || (ISLAND_TOPMOST..=PLAYER_TOPMOST).contains(&id))
                && notify == BN_CLICKED as usize)
                || (id == ZONE && notify == CBN_SELCHANGE as usize)
            {
                post_command(hwnd, APPLY_CONTROLS);
                return Some(LRESULT(0));
            }
            if ((id == STYLE || id == EDGE || id == EDGE_POSITION)
                && notify == CBN_SELCHANGE as usize)
                || ((id == ALBUM_COLOR
                    || id == SPECTRUM
                    || id == SPECTRUM_REALTIME
                    || id == SPECTRUM_RANDOM)
                    && notify == BN_CLICKED as usize)
            {
                post_command(hwnd, APPLY_APPEARANCE);
                return Some(LRESULT(0));
            }
            if id == FILL_COLOR && notify == EN_CHANGE as usize {
                let fill_color = (*theme_ptr).fill_color;
                if valid_color(&fill_color) {
                    post_command(hwnd, APPLY_APPEARANCE);
                    return Some(LRESULT(0));
                }
            }
            Some(DefWindowProcW(hwnd, msg, wp, lp))
        }
        WM_CLOSE => {
            post_command(hwnd, CLOSE);
            Some(LRESULT(0))
        }
        WM_SHOWWINDOW if wp.0 == 0 => {
            post_command(hwnd, CLOSE);
            Some(LRESULT(0))
        }
        _ => None,
    }
}

unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let theme_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Theme;
    if theme_ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    if msg == WM_NOTIFY && lp.0 != 0 {
        let hdr = &*(lp.0 as *const NMHDR);
        if let Some(result) = custom_draw(theme_ptr, hdr) {
            return result;
        }
    }
    if msg == WM_DRAWITEM && lp.0 != 0 {
        let item = &*(lp.0 as *const DRAWITEMSTRUCT);
        if draw_combo_item(hwnd, theme_ptr, item) {
            return LRESULT(1);
        }
    }
    if msg == WM_MEASUREITEM && lp.0 != 0 {
        let item = &mut *(lp.0 as *mut MEASUREITEMSTRUCT);
        if matches!(item.CtlID as i32, ZONE | STYLE | EDGE | EDGE_POSITION) {
            let scale = (*theme_ptr).scale;
            item.itemHeight = (28.0 * scale).round().max(1.0) as u32;
            return LRESULT(1);
        }
    }
    if msg == CONTROL_RECOVER_COMBO {
        let _ = recover_next_combo(hwnd, theme_ptr);
        return LRESULT(0);
    }
    if let Some(result) = handle_reentrant_message(hwnd, msg, wp, lp, theme_ptr) {
        return result;
    }
    match msg {
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut paint);
            let (page, save_status, notice, scroll, horizontal_scroll) = {
                let theme = &*theme_ptr;
                (
                    theme.page,
                    theme.save_label().to_owned(),
                    theme.message.clone(),
                    theme.scroll,
                    theme.horizontal_scroll as f32,
                )
            };
            let _ = (*theme_ptr).shell.draw(
                hwnd,
                page,
                &save_status,
                &notice,
                scroll,
                horizontal_scroll,
            );
            let _ = EndPaint(hwnd, &paint);
            return LRESULT(0);
        }
        WM_ENTERSIZEMOVE => {
            let placement = capture_window_placement(hwnd);
            (*theme_ptr).move_start = placement;
            return LRESULT(0);
        }
        WM_EXITSIZEMOVE => {
            finish_manual_move(hwnd, theme_ptr, true);
            return LRESULT(0);
        }
        WM_GETMINMAXINFO if lp.0 != 0 => {
            let limits = &mut *(lp.0 as *mut MINMAXINFO);
            let scale = (*theme_ptr).scale;
            let px = |n: f32| (n * scale).round() as i32;
            let mut minimum = RECT {
                left: 0,
                top: 0,
                right: px(super::model::MIN_WIDTH),
                bottom: px(super::model::MIN_HEIGHT),
            };
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            let _ = AdjustWindowRectExForDpi(
                &mut minimum,
                WINDOW_STYLE(style),
                false,
                WINDOW_EX_STYLE(ex_style),
                (96.0 * scale).round() as u32,
            );
            let mut monitor_info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(
                MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                &mut monitor_info,
            )
            .as_bool()
            {
                let work_width = (i64::from(monitor_info.rcWork.right)
                    - i64::from(monitor_info.rcWork.left))
                .clamp(1, i64::from(i32::MAX)) as i32;
                let work_height = (i64::from(monitor_info.rcWork.bottom)
                    - i64::from(monitor_info.rcWork.top))
                .clamp(1, i64::from(i32::MAX)) as i32;
                limits.ptMinTrackSize.x = min_track_px(minimum.right - minimum.left, work_width);
                limits.ptMinTrackSize.y = min_track_px(minimum.bottom - minimum.top, work_height);
            } else {
                limits.ptMinTrackSize.x = minimum.right - minimum.left;
                limits.ptMinTrackSize.y = minimum.bottom - minimum.top;
            }
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe fn valid_color(edit: &HWND) -> bool {
    let mut text = [0u16; 16];
    let len = GetWindowTextW(*edit, &mut text);
    if len != 7 || text[0] != b'#' as u16 {
        return false;
    }
    text[1..7].iter().all(|c| (*c as u8).is_ascii_hexdigit())
}

unsafe fn set_check(hwnd: HWND, id: i32, checked: bool) {
    SendMessageW(
        GetDlgItem(hwnd, id),
        BM_SETCHECK,
        WPARAM(usize::from(checked)),
        LPARAM(0),
    );
}

unsafe fn get_check(hwnd: HWND, id: i32) -> bool {
    SendMessageW(GetDlgItem(hwnd, id), BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PixelRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl PixelRect {
    fn from_xywh(x: i32, y: i32, width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        Some(Self {
            left: x,
            top: y,
            right: x.checked_add(width)?,
            bottom: y.checked_add(height)?,
        })
    }

    fn from_win32(rect: RECT) -> Option<Self> {
        (rect.right > rect.left && rect.bottom > rect.top).then_some(Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        })
    }

    fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }

    fn intersection_area(self, other: Self) -> i64 {
        let width =
            (i64::from(self.right.min(other.right)) - i64::from(self.left.max(other.left))).max(0);
        let height =
            (i64::from(self.bottom.min(other.bottom)) - i64::from(self.top.max(other.top))).max(0);
        width.saturating_mul(height)
    }

    fn clamp_inside(self, work: Self) -> Option<Self> {
        let width = self.width().min(work.width());
        let height = self.height().min(work.height());
        if width <= 0 || height <= 0 {
            return None;
        }
        let right_limit = work.right.checked_sub(width)?;
        let bottom_limit = work.bottom.checked_sub(height)?;
        let left = self.left.clamp(work.left, right_limit);
        let top = self.top.clamp(work.top, bottom_limit);
        Self::from_xywh(left, top, width, height)
    }
}

#[derive(Clone, Copy, Debug)]
struct MonitorWorkArea {
    rect: PixelRect,
    dpi: u32,
}

fn resolve_saved_placement(
    saved: &SettingsWindowPlacement,
    work_areas: &[MonitorWorkArea],
) -> Option<(PixelRect, u32)> {
    saved.validate().ok()?;
    let (old_width, old_height) = saved.size_px_for_dpi(saved.dpi).ok()?;
    let old_rect = PixelRect::from_xywh(saved.x, saved.y, old_width, old_height)?;
    let target = work_areas
        .iter()
        .filter_map(|area| {
            let overlap = old_rect.intersection_area(area.rect);
            (overlap > 0).then_some((overlap, *area))
        })
        .max_by_key(|(overlap, _)| *overlap)?
        .1;
    let (width, height) = saved.size_px_for_dpi(target.dpi).ok()?;
    let requested = PixelRect::from_xywh(saved.x, saved.y, width, height)?;
    Some((requested.clamp_inside(target.rect)?, target.dpi))
}

fn default_position(owner: PixelRect, work: PixelRect, width: i32, height: i32) -> PixelRect {
    let width = width.min(work.width().saturating_sub(32).max(1));
    let height = height.min(work.height().saturating_sub(32).max(1));
    let width = width.max(1);
    let height = height.max(1);
    let top = ((i64::from(work.top) + i64::from(work.bottom) - i64::from(height)) / 2).clamp(
        i64::from(work.top),
        i64::from(work.bottom) - i64::from(height),
    ) as i32;
    let candidates = [
        owner.right.saturating_add(20),
        owner.left.saturating_sub(width).saturating_sub(20),
        work.left.saturating_add(16),
        work.right.saturating_sub(width).saturating_sub(16),
    ];
    for left in candidates {
        if let Some(candidate) = PixelRect::from_xywh(left, top, width, height) {
            let inside = candidate.left >= work.left
                && candidate.right <= work.right
                && candidate.top >= work.top
                && candidate.bottom <= work.bottom;
            let overlaps_owner = candidate.intersection_area(owner) > 0;
            if inside && !overlaps_owner {
                return candidate;
            }
        }
    }
    let left = (i64::from(work.left) + (i64::from(work.width()) - i64::from(width)) / 2).clamp(
        i64::from(work.left),
        i64::from(work.right) - i64::from(width),
    ) as i32;
    PixelRect::from_xywh(left, top, width, height).unwrap_or(work)
}

unsafe extern "system" fn collect_work_area(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let areas = &mut *(data.0 as *mut Vec<MonitorWorkArea>);
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(monitor, &mut info).as_bool() {
        if let Some(rect) = PixelRect::from_win32(info.rcWork) {
            let mut dpi_x = 0;
            let mut dpi_y = 0;
            let dpi = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y)
                .ok()
                .map(|_| dpi_x)
                .filter(|dpi| {
                    (SettingsWindowPlacement::MIN_DPI..=SettingsWindowPlacement::MAX_DPI)
                        .contains(dpi)
                })
                .unwrap_or(96);
            areas.push(MonitorWorkArea { rect, dpi });
        }
    }
    BOOL(1)
}

unsafe fn enumerate_work_areas() -> Vec<MonitorWorkArea> {
    let mut areas = Vec::new();
    let _ = EnumDisplayMonitors(
        None,
        None,
        Some(collect_work_area),
        LPARAM((&mut areas as *mut Vec<MonitorWorkArea>) as isize),
    );
    areas
}

unsafe fn monitor_position(owner: HWND, width: i32, height: i32) -> (i32, i32, i32, i32) {
    let monitor = MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut info).as_bool() {
        return (CW_USEDEFAULT, CW_USEDEFAULT, width, height);
    }
    let Some(work) = PixelRect::from_win32(info.rcWork) else {
        return (CW_USEDEFAULT, CW_USEDEFAULT, width, height);
    };
    let mut owner_rect = RECT::default();
    let _ = GetWindowRect(owner, &mut owner_rect);
    let owner_rect = PixelRect::from_win32(owner_rect).unwrap_or(work);
    let result = default_position(owner_rect, work, width, height);
    (result.left, result.top, result.width(), result.height())
}

fn pixels_to_dip(pixels: i32, dpi: u32) -> Option<u32> {
    if pixels <= 0
        || !(SettingsWindowPlacement::MIN_DPI..=SettingsWindowPlacement::MAX_DPI).contains(&dpi)
    {
        return None;
    }
    let numerator = u64::try_from(pixels).ok()?.checked_mul(96)?;
    let rounded = numerator.checked_add(u64::from(dpi / 2))? / u64::from(dpi);
    u32::try_from(rounded).ok()
}

unsafe fn capture_window_placement(hwnd: HWND) -> Option<SettingsWindowPlacement> {
    let mut rect = RECT::default();
    GetWindowRect(hwnd, &mut rect).ok()?;
    let rect = PixelRect::from_win32(rect)?;
    let dpi = GetDpiForWindow(hwnd);
    let placement = SettingsWindowPlacement {
        x: rect.left,
        y: rect.top,
        width_dip: pixels_to_dip(rect.width(), dpi)?,
        height_dip: pixels_to_dip(rect.height(), dpi)?,
        dpi,
    };
    placement.validate().ok()?;
    Some(placement)
}

unsafe fn finish_manual_move(hwnd: HWND, theme_ptr: *mut Theme, notify_owner: bool) {
    if theme_ptr.is_null() {
        return;
    }
    let Some(start) = ({
        let theme = &mut *theme_ptr;
        theme.move_start.take()
    }) else {
        return;
    };
    let Some(current) = capture_window_placement(hwnd) else {
        return;
    };
    let should_publish = should_publish_manual_placement(
        &start,
        &current,
        (*theme_ptr).last_reported_placement.as_ref(),
    );
    if should_publish {
        {
            let theme = &mut *theme_ptr;
            theme.last_reported_placement = Some(current.clone());
            theme.pending_placement_update = Some(current);
        }
        if notify_owner {
            post_command(hwnd, PLACEMENT_CHANGED);
        }
    }
}

fn should_publish_manual_placement(
    start: &SettingsWindowPlacement,
    current: &SettingsWindowPlacement,
    last_reported: Option<&SettingsWindowPlacement>,
) -> bool {
    current != start && last_reported != Some(current)
}

pub struct Settings {
    pub hwnd: HWND,
    // The facade owns Theme for the entire HWND lifetime, including partial
    // construction failure. GWLP_USERDATA only borrows this allocation.
    theme: *mut Theme,
}

impl Settings {
    // Compatibility constructor retained for existing native Settings callers.
    #[allow(dead_code)]
    pub unsafe fn new_v2(
        owner: HWND,
        controls: &Controls,
        appearance: &Appearance,
    ) -> Result<Self> {
        Self::new_v2_with_placement(owner, controls, appearance, None)
    }

    // Preserve the pre-fixture placement entry point as a stable API.
    #[allow(dead_code)]
    pub unsafe fn new_v2_with_placement(
        owner: HWND,
        controls: &Controls,
        appearance: &Appearance,
        saved_placement: Option<SettingsWindowPlacement>,
    ) -> Result<Self> {
        Self::new_v2_with_placement_and_control_fixture(
            owner,
            controls,
            appearance,
            saved_placement,
            false,
        )
    }

    pub unsafe fn new_v2_with_placement_and_control_fixture(
        owner: HWND,
        controls: &Controls,
        appearance: &Appearance,
        saved_placement: Option<SettingsWindowPlacement>,
        allow_control_test_faults: bool,
    ) -> Result<Self> {
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        })
        .ok()?;
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = w!("IsleNativeSettingsV2");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
        let owner_dpi = GetDpiForWindow(owner).max(96);
        let owner_scale = owner_dpi as f32 / 96.0;
        let style =
            WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_THICKFRAME | WS_CLIPCHILDREN | WS_HSCROLL;
        let mut outer = RECT {
            left: 0,
            top: 0,
            right: (super::model::WINDOW_WIDTH * owner_scale).round() as i32,
            bottom: (super::model::WINDOW_HEIGHT * owner_scale).round() as i32,
        };
        AdjustWindowRectExForDpi(&mut outer, style, false, WS_EX_CONTROLPARENT, owner_dpi)?;
        let default_width = outer.right - outer.left;
        let default_height = outer.bottom - outer.top;
        let work_areas = enumerate_work_areas();
        let restored = saved_placement
            .as_ref()
            .and_then(|saved| resolve_saved_placement(saved, &work_areas));
        let (x, y, width, height, scale) = if let Some((rect, dpi)) = restored {
            (
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                dpi as f32 / 96.0,
            )
        } else {
            let (x, y, width, height) = monitor_position(owner, default_width, default_height);
            (x, y, width, height, owner_scale)
        };
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class,
            w!("Isle 设置"),
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
        let mut settings = Self {
            hwnd,
            theme: std::ptr::null_mut(),
        };
        let shell = ShellRender::new(hwnd, scale)?;
        let font = settings_font(scale);
        let fixture_create_failure = allow_control_test_faults
            && std::env::var("ISLE_TEST_CONTROL_FAIL_CREATE").as_deref() == Ok("1");
        let control_painter = if fixture_create_failure {
            None
        } else {
            shell.create_control_painter(scale).ok()
        };
        let mut control_diagnostics = ControlPaintDiagnostics {
            renderer_enabled: control_painter.is_some(),
            ..ControlPaintDiagnostics::default()
        };
        if control_painter.is_none() {
            control_diagnostics.native_fallbacks = 1;
        }
        if let Some(painter) = &control_painter {
            control_diagnostics.high_contrast = painter.high_contrast();
            control_diagnostics.generation = painter.generation();
        }
        let theme = Box::new(Theme {
            shell,
            control_painter,
            control_diagnostics,
            allow_control_test_faults,
            combo_labels: Vec::new(),
            combo_selection: Vec::new(),
            pending_combo_recoveries: Vec::new(),
            failed_combo_recoveries: Vec::new(),
            page: Page::General,
            scale,
            instance,
            font,
            card_brush: CreateSolidBrush(COLORREF(0x00171c23)),
            controls: Vec::new(),
            nav: [HWND(0); 6],
            query: HWND(0),
            list: HWND(0),
            weather_status: HWND(0),
            zone: HWND(0),
            style: HWND(0),
            edge: HWND(0),
            edge_position: HWND(0),
            shape_controls: Vec::new(),
            shape_labels: Vec::new(),
            fill_color: HWND(0),
            cities: Vec::new(),
            time_zones: Vec::new(),
            styles: Vec::new(),
            edges: Vec::new(),
            save_state: SaveState::Saved,
            message: String::new(),
            suppress_changes: false,
            scroll: 0.0,
            horizontal_scroll: 0,
            horizontal_scroll_max: 0,
            move_start: None,
            pending_placement_update: None,
            last_reported_placement: None,
        });
        settings.theme = Box::into_raw(theme);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, settings.theme as isize);
        let mut theme = ThemeHandle(settings.theme);
        for page in Page::ALL {
            add_nav(hwnd, theme, page)?;
        }

        // General: stable behavior switches and clock integration.
        child_checkbox(
            hwnd,
            theme,
            Page::General,
            208,
            "启用 Isle 动画",
            232.0,
            208.0,
            245.0,
        )?;
        child_checkbox(
            hwnd,
            theme,
            Page::General,
            209,
            "减少动画效果",
            232.0,
            246.0,
            245.0,
        )?;
        child_checkbox(
            hwnd,
            theme,
            Page::General,
            ISLAND_TOPMOST,
            "灵动岛始终置顶",
            492.0,
            208.0,
            255.0,
        )?;
        child_text(hwnd, theme, Page::General, "时钟时区", 232.0, 299.0, 120.0)?;
        theme.zone = child_combo(hwnd, theme, Page::General, ZONE, 338.0, 292.0, 292.0)?;
        let mut zones = crate::clock::ZONES
            .iter()
            .map(|(id, label)| (id.to_string(), format!("{label} · {id}")))
            .collect::<Vec<_>>();
        if !zones.iter().any(|(id, _)| id == &controls.time_zone) {
            zones.push((
                controls.time_zone.clone(),
                format!("未支持：{}", controls.time_zone),
            ));
        }
        theme.time_zones = zones.iter().map(|(id, _)| id.clone()).collect();
        let zone_combo = theme.zone;
        set_combo_items(
            theme,
            zone_combo,
            ZONE,
            zones.into_iter().map(|(_, label)| label),
        );

        // Appearance: values map directly to the live app's current model.
        child_text(
            hwnd,
            theme,
            Page::Appearance,
            "显示方式",
            232.0,
            198.0,
            98.0,
        )?;
        theme.style = child_combo(hwnd, theme, Page::Appearance, STYLE, 330.0, 190.0, 170.0)?;
        theme.styles = vec!["floating".to_string(), "edge".to_string()];
        if !theme.styles.contains(&appearance.style) {
            theme.styles.push(appearance.style.clone());
        }
        let style_combo = theme.style;
        let style_labels = theme
            .styles
            .iter()
            .map(|value| match value.as_str() {
                "floating" => "悬浮".to_string(),
                "edge" => "贴边".to_string(),
                other => format!("其他：{other}"),
            })
            .collect::<Vec<_>>();
        set_combo_items(theme, style_combo, STYLE, style_labels);
        child_text(
            hwnd,
            theme,
            Page::Appearance,
            "贴边方向",
            520.0,
            198.0,
            96.0,
        )?;
        theme.edge = child_combo(hwnd, theme, Page::Appearance, EDGE, 617.0, 190.0, 145.0)?;
        theme.edges = vec!["top".into(), "right".into(), "bottom".into(), "left".into()];
        let edge_labels = ["上", "右", "下", "左"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let edge_combo = theme.edge;
        set_combo_items(theme, edge_combo, EDGE, edge_labels);
        child_text(
            hwnd,
            theme,
            Page::Appearance,
            "沿边位置",
            232.0,
            246.0,
            98.0,
        )?;
        theme.edge_position = child_combo(
            hwnd,
            theme,
            Page::Appearance,
            EDGE_POSITION,
            330.0,
            238.0,
            170.0,
        )?;
        let edge_position_combo = theme.edge_position;
        set_combo_items(
            theme,
            edge_position_combo,
            EDGE_POSITION,
            (0..=100).map(|position| format!("{position}%")),
        );
        child_checkbox(
            hwnd,
            theme,
            Page::Appearance,
            ALBUM_COLOR,
            "跟随专辑主色",
            520.0,
            236.0,
            202.0,
        )?;
        child_text(
            hwnd,
            theme,
            Page::Appearance,
            "自定义颜色",
            232.0,
            298.0,
            98.0,
        )?;
        theme.fill_color = add_control(
            hwnd,
            theme,
            w!("EDIT"),
            &appearance.floating_fill_color,
            FILL_COLOR,
            WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            Page::Appearance,
            330.0,
            290.0,
            170.0,
            28.0,
        )?;
        SendMessageW(theme.fill_color, EM_LIMITTEXT, WPARAM(7), LPARAM(0));
        child_checkbox(
            hwnd,
            theme,
            Page::Appearance,
            SPECTRUM,
            "显示频谱",
            520.0,
            280.0,
            202.0,
        )?;
        let radio_style = WS_TABSTOP | WINDOW_STYLE(BS_AUTORADIOBUTTON as u32);
        add_control(
            hwnd,
            theme,
            w!("BUTTON"),
            "实时声音",
            SPECTRUM_REALTIME,
            radio_style | WS_GROUP,
            Page::Appearance,
            520.0,
            312.0,
            108.0,
            25.0,
        )?;
        add_control(
            hwnd,
            theme,
            w!("BUTTON"),
            "随机跳动",
            SPECTRUM_RANDOM,
            radio_style,
            Page::Appearance,
            632.0,
            312.0,
            118.0,
            25.0,
        )?;
        child_checkbox(
            hwnd,
            theme,
            Page::Appearance,
            INSPECTION_CHECK,
            "保持当前展开状态",
            232.0,
            350.0,
            230.0,
        )?;
        for (index, (initial, min, max, y)) in [
            (appearance.compact_length as i32, 80, 300, 454.0),
            (appearance.collapsed_shoulder_radius as i32, 0, 16, 508.0),
            (appearance.expanded_shoulder_radius as i32, 0, 64, 562.0),
            (appearance.expanded_corner_radius as i32, 0, 80, 616.0),
        ]
        .into_iter()
        .enumerate()
        {
            let label = child_text(
                hwnd,
                theme,
                Page::Appearance,
                &format!("{}：{}", SHAPE_NAMES[index], initial),
                232.0,
                y,
                154.0,
            )?;
            theme.shape_labels.push(label);
            let track = add_control(
                hwnd,
                theme,
                w!("msctls_trackbar32"),
                "",
                SHAPE_CONTROL_BASE + index as i32,
                WS_TABSTOP | WINDOW_STYLE(TBS_AUTOTICKS),
                Page::Appearance,
                382.0,
                y - 4.0,
                340.0,
                34.0,
            )?;
            SendMessageW(
                track,
                WM_USER + 6,
                WPARAM(1),
                LPARAM(((max as u32) << 16 | min as u32) as isize),
            );
            SendMessageW(
                track,
                WM_USER + 5,
                WPARAM(1),
                LPARAM(initial.clamp(min, max) as isize),
            );
            SendMessageW(track, WM_USER + 20, WPARAM(10), LPARAM(0));
            theme.shape_controls.push(track);
        }

        // Modules: all entries correspond to existing Controls::tools.
        child_checkbox(
            hwnd,
            theme,
            Page::Modules,
            PANEL,
            "启用工具栏",
            232.0,
            210.0,
            226.0,
        )?;
        let tool_labels = [
            "倒计时",
            "音量",
            "悬浮播放器",
            "设置",
            "隐藏",
            "时间",
            "天气",
        ];
        for (index, label) in tool_labels.iter().enumerate() {
            let col = index % 2;
            let row = index / 2;
            child_checkbox(
                hwnd,
                theme,
                Page::Modules,
                201 + index as i32,
                label,
                232.0 + col as f32 * 240.0,
                250.0 + row as f32 * 42.0,
                220.0,
            )?;
        }

        // Media: expose only the existing secondary player settings.
        child_checkbox(
            hwnd,
            theme,
            Page::Media,
            PLAYER_TOPMOST,
            "悬浮播放器始终置顶",
            232.0,
            210.0,
            270.0,
        )?;
        add_control(
            hwnd,
            theme,
            w!("BUTTON"),
            "播放器设置…",
            PLAYERS as i32,
            WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            Page::Media,
            232.0,
            256.0,
            176.0,
            34.0,
        )?;

        // Weather: preserve the native edit (IME), result list, and existing search/apply route.
        child_text(
            hwnd,
            theme,
            Page::Weather,
            "城市或拼音",
            232.0,
            202.0,
            140.0,
        )?;
        theme.query = add_control(
            hwnd,
            theme,
            w!("EDIT"),
            "",
            WEATHER_QUERY,
            WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            Page::Weather,
            232.0,
            228.0,
            310.0,
            30.0,
        )?;
        SendMessageW(theme.query, EM_LIMITTEXT, WPARAM(80), LPARAM(0));
        add_control(
            hwnd,
            theme,
            w!("BUTTON"),
            "搜索",
            SEARCH as i32,
            WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            Page::Weather,
            556.0,
            228.0,
            98.0,
            30.0,
        )?;
        theme.weather_status = add_control(
            hwnd,
            theme,
            w!("STATIC"),
            "输入至少两个字或拼音，按 Enter 搜索",
            WEATHER_STATUS,
            WINDOW_STYLE(0),
            Page::Weather,
            232.0,
            268.0,
            490.0,
            22.0,
        )?;
        theme.list = add_control(
            hwnd,
            theme,
            w!("LISTBOX"),
            "",
            WEATHER_LIST,
            WS_BORDER
                | WS_TABSTOP
                | WS_VSCROLL
                | WINDOW_STYLE(LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32),
            Page::Weather,
            232.0,
            302.0,
            490.0,
            174.0,
        )?;
        SendMessageW(
            theme.list,
            LB_SETHORIZONTALEXTENT,
            WPARAM((850.0 * scale) as usize),
            LPARAM(0),
        );
        // Advanced: recovery actions route to the runtime settings service.
        child_text(
            hwnd,
            theme,
            Page::Advanced,
            "保存状态和错误会显示在窗口底部。",
            232.0,
            204.0,
            480.0,
        )?;
        add_control(
            hwnd,
            theme,
            w!("BUTTON"),
            "重试保存",
            RETRY_SAVE as i32,
            WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            Page::Advanced,
            232.0,
            246.0,
            132.0,
            34.0,
        )?;
        add_control(
            hwnd,
            theme,
            w!("BUTTON"),
            "恢复已保存设置",
            REVERT_SAVED as i32,
            WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            Page::Advanced,
            378.0,
            246.0,
            176.0,
            34.0,
        )?;

        sync_theme(hwnd, theme, controls, appearance);
        let _ = InvalidateRect(hwnd, None, true);
        position_controls(hwnd, theme.0);
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
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(theme.nav[Page::General.index()]);
        }
        Ok(settings)
    }

    pub unsafe fn controls(&self) -> Controls {
        let (zone, time_zones) = {
            let theme = get_theme(self.hwnd);
            (theme.zone, theme.time_zones.clone())
        };
        Controls {
            panel: get_check(self.hwnd, PANEL),
            tools: std::array::from_fn(|index| get_check(self.hwnd, 201 + index as i32)),
            animations: get_check(self.hwnd, 208),
            reduced: get_check(self.hwnd, 209),
            always_on_top: get_check(self.hwnd, ISLAND_TOPMOST),
            floating_always_on_top: get_check(self.hwnd, PLAYER_TOPMOST),
            time_zone: time_zones
                .get(SendMessageW(zone, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as usize)
                .cloned()
                .unwrap_or_else(|| "system".into()),
        }
    }

    pub unsafe fn appearance(&self) -> Option<Appearance> {
        let (style, styles, edge, edges, edge_position_combo, shape_controls, fill_color) = {
            let theme = get_theme(self.hwnd);
            (
                theme.style,
                theme.styles.clone(),
                theme.edge,
                theme.edges.clone(),
                theme.edge_position,
                theme.shape_controls.clone(),
                theme.fill_color,
            )
        };
        let read_combo = |combo: HWND, values: &[String]| {
            values
                .get(SendMessageW(combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as usize)
                .cloned()
        };
        let edge_position =
            u8::try_from(SendMessageW(edge_position_combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0)
                .ok()
                .filter(|value| *value <= 100)?;
        let shape_values = shape_controls
            .iter()
            .map(|control| SendMessageW(*control, WM_USER, WPARAM(0), LPARAM(0)).0)
            .collect::<Vec<_>>();
        let color_text = read_text(fill_color, 16);
        if color_text.len() != 7
            || !color_text.starts_with('#')
            || !color_text.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return None;
        }
        Some(Appearance {
            style: read_combo(style, &styles)?,
            edge: read_combo(edge, &edges)?,
            edge_position,
            compact_length: u16::try_from(*shape_values.first()?).ok()?,
            collapsed_shoulder_radius: u8::try_from(*shape_values.get(1)?).ok()?,
            expanded_shoulder_radius: u8::try_from(*shape_values.get(2)?).ok()?,
            expanded_corner_radius: u32::try_from(*shape_values.get(3)?).ok()?,
            floating_fill_color: color_text,
            floating_use_album_color: get_check(self.hwnd, ALBUM_COLOR),
            show_spectrum: get_check(self.hwnd, SPECTRUM),
            spectrum_mode: if get_check(self.hwnd, SPECTRUM_RANDOM) {
                "random"
            } else {
                "realtime"
            }
            .into(),
        })
    }

    pub unsafe fn query(&self) -> String {
        let query = get_theme(self.hwnd).query;
        read_text(query, 81).trim().to_string()
    }

    pub unsafe fn take_placement_update(&mut self) -> Option<SettingsWindowPlacement> {
        if self.theme.is_null() {
            return None;
        }
        if (*self.theme).move_start.is_some() {
            finish_manual_move(self.hwnd, self.theme, false);
        }
        (*self.theme).pending_placement_update.take()
    }

    pub unsafe fn control_diagnostics(&self) -> Option<ControlPaintDiagnostics> {
        if self.theme.is_null() {
            return None;
        }
        let theme = &*self.theme;
        let mut diagnostics = theme.control_diagnostics;
        if let Some(painter) = &theme.control_painter {
            diagnostics.renderer_enabled = painter.is_enabled();
            diagnostics.generation = painter.generation();
            diagnostics.high_contrast = painter.high_contrast();
        } else {
            diagnostics.renderer_enabled = false;
        }
        Some(diagnostics)
    }

    pub unsafe fn test_control_fault(&self, kind: u32) -> bool {
        if self.theme.is_null() {
            return false;
        }
        let controls = {
            let theme = &mut *self.theme;
            if !theme.allow_control_test_faults {
                return false;
            }
            let Some(painter) = theme.control_painter.as_mut() else {
                return false;
            };
            if !painter.inject_fault(kind) {
                return false;
            }
            if kind == 4 {
                schedule_all_combo_recoveries(self.hwnd, theme);
            }
            if let Some(painter) = &theme.control_painter {
                theme.control_diagnostics.renderer_enabled = painter.is_enabled();
                theme.control_diagnostics.generation = painter.generation();
                theme.control_diagnostics.high_contrast = painter.high_contrast();
            }
            theme
                .controls
                .iter()
                .map(|placement| placement.hwnd)
                .collect::<Vec<_>>()
        };
        for control in controls {
            let _ = InvalidateRect(control, None, false);
        }
        let _ = InvalidateRect(self.hwnd, None, false);
        true
    }

    pub unsafe fn selected(&self) -> Option<City> {
        let list = get_theme(self.hwnd).list;
        let index = SendMessageW(list, LB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
        usize::try_from(index)
            .ok()
            .and_then(|i| get_theme(self.hwnd).cities.get(i).cloned())
    }

    pub unsafe fn loading(&mut self) {
        let (list, status) = {
            let mut theme = get_theme_mut(self.hwnd);
            theme.cities.clear();
            (theme.list, theme.weather_status)
        };
        SendMessageW(list, LB_RESETCONTENT, WPARAM(0), LPARAM(0));
        let _ = SetWindowTextW(status, &HSTRING::from("正在搜索…"));
    }

    pub unsafe fn results(&mut self, cities: Vec<City>, failed: bool) {
        self.loading();
        let labels = cities
            .iter()
            .map(|city| {
                HSTRING::from(format!(
                    "{} ({:.2}, {:.2})",
                    city.name, city.latitude, city.longitude
                ))
            })
            .collect::<Vec<_>>();
        let has_results = !cities.is_empty();
        let (list, status_hwnd) = {
            let mut theme = get_theme_mut(self.hwnd);
            theme.cities = cities;
            (theme.list, theme.weather_status)
        };
        for text in &labels {
            SendMessageW(
                list,
                LB_ADDSTRING,
                WPARAM(0),
                LPARAM(text.as_ptr() as isize),
            );
        }
        if has_results {
            SendMessageW(list, LB_SETCURSEL, WPARAM(0), LPARAM(0));
        }
        let status = if failed {
            "搜索失败，请重试"
        } else if !has_results {
            "未找到城市，请尝试完整名称或拼音"
        } else {
            "选择城市后保存；双击结果也可保存"
        };
        let _ = SetWindowTextW(status_hwnd, &HSTRING::from(status));
    }

    pub unsafe fn saving(&self, saving: bool) {
        self.set_save_state(if saving {
            SaveState::Saving
        } else {
            SaveState::Saved
        });
    }

    pub unsafe fn message(&self, text: &str) {
        let status = {
            let mut theme = get_theme_mut(self.hwnd);
            theme.message = text.to_string();
            theme.weather_status
        };
        let _ = SetWindowTextW(status, &HSTRING::from(text));
        let _ = InvalidateRect(self.hwnd, None, false);
    }

    pub unsafe fn set_save_state(&self, state: SaveState) {
        {
            let mut theme = get_theme_mut(self.hwnd);
            theme.save_state = state;
        }
        let _ = InvalidateRect(self.hwnd, None, false);
    }

    pub unsafe fn sync_settings(&mut self, controls: &Controls, appearance: &Appearance) {
        let theme = get_theme_mut(self.hwnd);
        sync_theme(self.hwnd, theme, controls, appearance);
        position_controls(self.hwnd, theme.0);
    }

    pub unsafe fn inspection_locked(&self) -> bool {
        get_check(self.hwnd, INSPECTION_CHECK)
    }

    pub unsafe fn route(&self, msg: &MSG) -> bool {
        let belongs = msg.hwnd == self.hwnd || IsChild(self.hwnd, msg.hwnd).as_bool();
        if !belongs {
            return false;
        }
        if msg.message == WM_KEYDOWN
            && (GetKeyState(VK_CONTROL.0 as i32) < 0)
            && (GetKeyState(VK_SHIFT.0 as i32) < 0)
            && [VK_LEFT.0 as usize, VK_RIGHT.0 as usize].contains(&msg.wParam.0)
        {
            let (query, fill_color, next_scroll) = {
                let theme = get_theme_mut(self.hwnd);
                (
                    theme.query,
                    theme.fill_color,
                    theme
                        .horizontal_scroll
                        .saturating_add(if msg.wParam.0 == VK_LEFT.0 as usize {
                            -48
                        } else {
                            48
                        }),
                )
            };
            if GetFocus() != query && GetFocus() != fill_color {
                set_horizontal_scroll(self.hwnd, self.theme, next_scroll);
                return true;
            }
        }
        if msg.message == WM_MOUSEWHEEL {
            let id = GetDlgCtrlID(msg.hwnd);
            let native_scroll_target = id == WEATHER_LIST
                || [ZONE, STYLE, EDGE, EDGE_POSITION].contains(&id)
                || (SHAPE_CONTROL_BASE..SHAPE_CONTROL_BASE + 4).contains(&id);
            if !native_scroll_target {
                let mut theme = get_theme_mut(self.hwnd);
                let delta = ((msg.wParam.0 >> 16) as u16 as i16) as f32;
                theme.scroll -= delta / 120.0 * 54.0;
                position_controls(self.hwnd, theme.0);
                let _ = InvalidateRect(self.hwnd, None, false);
                return true;
            }
        }
        if msg.message == WM_KEYDOWN
            && msg.wParam.0 == VK_RETURN.0 as usize
            && GetFocus() == get_theme(self.hwnd).query
            && get_theme(self.hwnd).page == Page::Weather
        {
            let command = ((BN_CLICKED as usize) << 16) | SEARCH;
            let _ = PostMessageW(self.hwnd, WM_COMMAND, WPARAM(command), LPARAM(0));
            return true;
        }
        IsDialogMessageW(self.hwnd, msg).as_bool()
    }
}

unsafe fn read_text(hwnd: HWND, capacity: usize) -> String {
    let mut text = vec![0u16; capacity];
    let len = GetWindowTextW(hwnd, &mut text);
    String::from_utf16_lossy(&text[..(len.max(0) as usize).min(text.len())])
}

unsafe fn get_theme(hwnd: HWND) -> &'static Theme {
    &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Theme)
}

unsafe fn get_theme_mut(hwnd: HWND) -> ThemeHandle {
    ThemeHandle(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Theme)
}

unsafe fn select_combo(theme: ThemeHandle, id: i32, value: &str) {
    let (combo, index) = match id {
        STYLE => (
            theme.style,
            theme
                .styles
                .iter()
                .position(|item| item == value)
                .unwrap_or(0),
        ),
        EDGE => (
            theme.edge,
            theme
                .edges
                .iter()
                .position(|item| item == value)
                .unwrap_or(0),
        ),
        _ => return,
    };
    SendMessageW(combo, CB_SETCURSEL, WPARAM(index), LPARAM(0));
    update_combo_selection(&mut *theme.0, id, index as i32);
}

unsafe fn sync_theme(
    hwnd: HWND,
    mut theme: ThemeHandle,
    controls: &Controls,
    appearance: &Appearance,
) {
    let (
        zone,
        style,
        edge,
        edge_position_combo,
        time_zones,
        shape_controls,
        shape_labels,
        fill_color,
    ) = {
        theme.suppress_changes = true;
        (
            theme.zone,
            theme.style,
            theme.edge,
            theme.edge_position,
            theme.time_zones.clone(),
            theme.shape_controls.clone(),
            theme.shape_labels.clone(),
            theme.fill_color,
        )
    };
    let combos = [zone, style, edge, edge_position_combo];
    for combo in combos {
        SendMessageW(combo, WM_SETREDRAW, WPARAM(0), LPARAM(0));
    }
    for (id, checked) in [
        (PANEL, controls.panel),
        (208, controls.animations),
        (209, controls.reduced),
        (ISLAND_TOPMOST, controls.always_on_top),
        (PLAYER_TOPMOST, controls.floating_always_on_top),
        (ALBUM_COLOR, appearance.floating_use_album_color),
        (SPECTRUM, appearance.show_spectrum),
    ] {
        set_check(hwnd, id, checked);
    }
    for index in 0..7 {
        set_check(hwnd, 201 + index, controls.tools[index as usize]);
    }
    let zone_index = time_zones
        .iter()
        .position(|value| value == &controls.time_zone)
        .unwrap_or(0);
    SendMessageW(zone, CB_SETCURSEL, WPARAM(zone_index), LPARAM(0));
    update_combo_selection(&mut *theme.0, ZONE, zone_index as i32);
    select_combo(theme, STYLE, &appearance.style);
    select_combo(theme, EDGE, &appearance.edge);
    let edge_position_index = appearance.edge_position as usize;
    SendMessageW(
        edge_position_combo,
        CB_SETCURSEL,
        WPARAM(edge_position_index),
        LPARAM(0),
    );
    update_combo_selection(&mut *theme.0, EDGE_POSITION, edge_position_index as i32);
    let values = [
        appearance.compact_length as i32,
        appearance.collapsed_shoulder_radius as i32,
        appearance.expanded_shoulder_radius as i32,
        appearance.expanded_corner_radius as i32,
    ];
    for (index, track) in shape_controls.iter().enumerate() {
        SendMessageW(
            *track,
            WM_USER + 5,
            WPARAM(1),
            LPARAM(values[index] as isize),
        );
        let label = HSTRING::from(format!("{}：{}", SHAPE_NAMES[index], values[index]));
        let _ = SetWindowTextW(shape_labels[index], &label);
    }
    let _ = SetWindowTextW(fill_color, &HSTRING::from(&appearance.floating_fill_color));
    set_check(
        hwnd,
        SPECTRUM_REALTIME,
        appearance.spectrum_mode != "random",
    );
    set_check(hwnd, SPECTRUM_RANDOM, appearance.spectrum_mode == "random");
    theme.suppress_changes = false;
    for combo in combos {
        SendMessageW(combo, WM_SETREDRAW, WPARAM(1), LPARAM(0));
        let _ = InvalidateRect(combo, None, false);
    }
    let _ = InvalidateRect(hwnd, None, false);
}

impl Drop for Settings {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(self.hwnd).as_bool() {
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                let _ = DestroyWindow(self.hwnd);
            }
            if !self.theme.is_null() {
                drop(Box::from_raw(self.theme));
                self.theme = std::ptr::null_mut();
            }
        }
    }
}

#[cfg(test)]
mod placement_tests {
    use super::*;

    fn area(rect: (i32, i32, i32, i32), dpi: u32) -> MonitorWorkArea {
        MonitorWorkArea {
            rect: PixelRect {
                left: rect.0,
                top: rect.1,
                right: rect.2,
                bottom: rect.3,
            },
            dpi,
        }
    }

    fn saved(x: i32, y: i32, width_dip: u32, height_dip: u32, dpi: u32) -> SettingsWindowPlacement {
        SettingsWindowPlacement {
            x,
            y,
            width_dip,
            height_dip,
            dpi,
        }
    }

    #[test]
    fn saved_position_restores_on_primary_monitor_at_its_current_dpi() {
        let monitors = [area((0, 0, 1920, 1080), 96), area((-1280, 0, 0, 1024), 144)];
        let (rect, dpi) = resolve_saved_placement(&saved(120, 160, 820, 760, 96), &monitors)
            .expect("saved position intersects primary monitor");
        assert_eq!(dpi, 96);
        assert_eq!(rect, PixelRect::from_xywh(120, 160, 820, 760).unwrap());
    }

    #[test]
    fn negative_secondary_position_uses_current_dpi_and_clamps_to_work_area() {
        let monitors = [area((-1280, 0, 0, 1440), 192)];
        let (rect, dpi) = resolve_saved_placement(&saved(-1200, 100, 820, 760, 120), &monitors)
            .expect("saved negative coordinates map to the secondary display");
        assert_eq!(dpi, 192);
        assert_eq!(rect, PixelRect::from_xywh(-1280, 0, 1280, 1440).unwrap());
        assert!(rect.left >= monitors[0].rect.left && rect.right <= monitors[0].rect.right);
        assert!(rect.top >= monitors[0].rect.top && rect.bottom <= monitors[0].rect.bottom);
    }

    #[test]
    fn partially_offscreen_saved_window_is_clamped_without_losing_reachability() {
        let monitors = [area((-1920, 0, 0, 1080), 96)];
        let (rect, _) = resolve_saved_placement(&saved(-1900, 900, 820, 760, 96), &monitors)
            .expect("saved window still intersects the display");
        assert_eq!(rect, PixelRect::from_xywh(-1900, 320, 820, 760).unwrap());
        assert!(rect.left >= monitors[0].rect.left && rect.right <= monitors[0].rect.right);
        assert!(rect.top >= monitors[0].rect.top && rect.bottom <= monitors[0].rect.bottom);
    }

    #[test]
    fn missing_saved_monitor_falls_back_and_default_placement_avoids_owner() {
        let monitors = [area((0, 0, 1920, 1080), 96)];
        assert!(resolve_saved_placement(&saved(-5000, 200, 820, 760, 96), &monitors).is_none());
        let owner = PixelRect::from_xywh(900, 0, 200, 64).unwrap();
        let fallback = default_position(owner, monitors[0].rect, 820, 760);
        assert_eq!(fallback.left, owner.left - fallback.width() - 20);
        assert!(fallback.left >= monitors[0].rect.left && fallback.right <= monitors[0].rect.right);
        assert_eq!(fallback.intersection_area(owner), 0);
    }

    #[test]
    fn extremely_small_work_area_shrinks_window_and_caps_minimum_tracking_size() {
        let work = PixelRect::from_xywh(0, 0, 500, 350).unwrap();
        let owner = PixelRect::from_xywh(100, 0, 100, 40).unwrap();
        let fitted = default_position(owner, work, 1600, 1500);
        assert!(fitted.width() <= work.width());
        assert!(fitted.height() <= work.height());
        assert!(fitted.left >= work.left && fitted.right <= work.right);
        assert!(fitted.top >= work.top && fitted.bottom <= work.bottom);
        assert_eq!(min_track_px(1600, 500), 500);
        assert_eq!(min_track_px(700, 0), 1);
    }

    #[test]
    fn horizontal_range_keeps_the_rightmost_control_reachable_by_keyboard() {
        let max_scroll = horizontal_scroll_limit(792.0, 600.0);
        let viewport_right = 600.0 - 16.0;
        assert!(max_scroll > 0);
        assert!((792.0 - max_scroll as f32 - viewport_right).abs() <= 1.0);
        assert_eq!(horizontal_scroll_limit(792.0, 900.0), 0);
    }

    #[test]
    fn only_a_changed_manual_move_publishes_one_final_placement() {
        let start = saved(100, 100, 820, 760, 96);
        let moved = saved(120, 110, 820, 760, 96);
        assert!(!should_publish_manual_placement(&start, &start, None));
        assert!(should_publish_manual_placement(&start, &moved, None));
        assert!(!should_publish_manual_placement(
            &start,
            &moved,
            Some(&moved)
        ));
    }

    #[test]
    fn extreme_signed_rectangles_do_not_overflow_intersection_or_restore() {
        let huge = PixelRect {
            left: i32::MIN,
            top: i32::MIN,
            right: i32::MAX,
            bottom: i32::MAX,
        };
        assert_eq!(huge.intersection_area(huge), i64::MAX);
        let monitors = [area((0, 0, 1920, 1080), 96)];
        assert!(
            resolve_saved_placement(&saved(i32::MAX - 2, 0, 820, 760, 96), &monitors).is_none()
        );
        assert!(
            resolve_saved_placement(&saved(i32::MIN, i32::MIN, 820, 760, 96), &monitors).is_none()
        );
    }
}
