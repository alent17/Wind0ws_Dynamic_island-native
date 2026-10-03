//! UI Automation support for the native controls hosted by the Settings window.
//!
//! Each child HWND supplies its own provider. This preserves the existing
//! Settings layout and input path while exposing standard UIA control semantics.
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use std::mem::ManuallyDrop;
use windows::{
    core::{implement, ComInterface, IUnknown, Interface, Result, BSTR, PCWSTR},
    Win32::{
        Foundation::{
            CO_E_OBJNOTCONNECTED, E_FAIL, E_NOTIMPL, E_OUTOFMEMORY, HWND, LPARAM, RECT, WPARAM,
        },
        Graphics::Gdi::ClientToScreen,
        System::{
            Com::SAFEARRAY,
            Ole::{SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement},
            Variant::*,
        },
        UI::{
            Accessibility::*,
            Controls::{
                GetComboBoxInfo, BST_CHECKED, BST_INDETERMINATE, COMBOBOXINFO,
                DLG_BUTTON_CHECK_STATE, TBM_GETLINESIZE, TBM_GETPAGESIZE, TBM_GETRANGEMAX,
                TBM_GETRANGEMIN, TBM_SETPOSNOTIFY,
            },
            Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled},
            WindowsAndMessaging::{
                EnumChildWindows, GetClassNameW, GetDlgCtrlID, GetParent, GetWindowLongW,
                GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsWindow, PostMessageW,
                SendMessageW, SetWindowTextW, BM_CLICK, BM_GETCHECK, BS_AUTO3STATE,
                BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_CHECKBOX, BS_RADIOBUTTON, CBN_SELCHANGE,
                CBS_DROPDOWN, CBS_DROPDOWNLIST, CB_GETCOUNT, CB_GETCURSEL, CB_GETDROPPEDSTATE,
                CB_GETLBTEXT, CB_GETLBTEXTLEN, CB_SETCURSEL, CB_SHOWDROPDOWN, ES_READONLY,
                GWL_STYLE, LB_GETITEMRECT, WM_COMMAND, WM_USER,
            },
        },
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlKind {
    Button,
    Toggle,
    Radio,
    ComboBox,
    Edit,
    Slider,
    Text,
    Other,
}

fn check_state(hwnd: HWND) -> Result<DLG_BUTTON_CHECK_STATE> {
    ensure_window(hwnd)?;
    Ok(DLG_BUTTON_CHECK_STATE(unsafe {
        SendMessageW(hwnd, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 as u32
    }))
}

fn ensure_window(hwnd: HWND) -> Result<()> {
    if unsafe { IsWindow(hwnd).as_bool() } {
        Ok(())
    } else {
        Err(CO_E_OBJNOTCONNECTED.into())
    }
}

unsafe extern "system" fn find_selected_radio(
    hwnd: HWND,
    selected: LPARAM,
) -> windows::Win32::Foundation::BOOL {
    if let Ok(class) = class_name(hwnd) {
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let button_type = (style & 0x0f) as i32;
        if class.eq_ignore_ascii_case("Button")
            && matches!(button_type, BS_AUTORADIOBUTTON | BS_RADIOBUTTON)
            && SendMessageW(hwnd, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 as u32 == BST_CHECKED.0
        {
            (selected.0 as *mut HWND).write(hwnd);
            return windows::Win32::Foundation::BOOL(0);
        }
    }
    windows::Win32::Foundation::BOOL(1)
}

fn class_name(hwnd: HWND) -> Result<String> {
    ensure_window(hwnd)?;
    let mut buffer = [0u16; 64];
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
    Ok(String::from_utf16_lossy(&buffer[..length.max(0) as usize]))
}

fn selected_radio(parent: HWND) -> Result<HWND> {
    ensure_window(parent)?;
    let mut selected = HWND(0);
    unsafe {
        EnumChildWindows(
            parent,
            Some(find_selected_radio),
            LPARAM((&mut selected as *mut HWND) as isize),
        );
    }
    if selected.0 == 0 {
        Err(E_FAIL.into())
    } else {
        Ok(selected)
    }
}

fn combo_list_hwnd(combo: HWND) -> Result<HWND> {
    ensure_window(combo)?;
    let mut info = COMBOBOXINFO {
        cbSize: std::mem::size_of::<COMBOBOXINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetComboBoxInfo(combo, &mut info)? };
    if info.hwndList.0 == 0 {
        Err(E_FAIL.into())
    } else {
        Ok(info.hwndList)
    }
}

fn combo_item_count(combo: HWND) -> Result<i32> {
    ensure_window(combo)?;
    Ok(unsafe { SendMessageW(combo, CB_GETCOUNT, WPARAM(0), LPARAM(0)).0 as i32 }.max(0))
}

fn combo_item_text(combo: HWND, index: i32) -> Result<String> {
    let count = combo_item_count(combo)?;
    if index < 0 || index >= count {
        return Err(windows::core::HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32).into());
    }
    let length =
        unsafe { SendMessageW(combo, CB_GETLBTEXTLEN, WPARAM(index as usize), LPARAM(0)).0 };
    if !(0..=4096).contains(&length) {
        return Err(E_FAIL.into());
    }
    let mut text = vec![0u16; length as usize + 1];
    let copied = unsafe {
        SendMessageW(
            combo,
            CB_GETLBTEXT,
            WPARAM(index as usize),
            LPARAM(text.as_mut_ptr() as isize),
        )
        .0
    };
    if copied < 0 || copied as usize >= text.len() {
        return Err(E_FAIL.into());
    }
    Ok(String::from_utf16_lossy(&text[..copied as usize]))
}

fn combo_item_provider(combo: HWND, index: i32) -> Result<IRawElementProviderSimple> {
    let list = combo_list_hwnd(combo)?;
    Ok(ComboListItemProvider {
        hwnd: list,
        combo,
        index,
    }
    .into())
}

fn combo_selection_array(combo: HWND) -> Result<*mut SAFEARRAY> {
    let selected = unsafe { SendMessageW(combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32 };
    let count = u32::from(selected >= 0);
    let array = unsafe { SafeArrayCreateVector(VT_UNKNOWN, 0, count) };
    if array.is_null() {
        return Err(E_OUTOFMEMORY.into());
    }
    if selected >= 0 {
        let item = match combo_item_provider(combo, selected) {
            Ok(item) => item,
            Err(error) => {
                unsafe { SafeArrayDestroy(array) }?;
                return Err(error);
            }
        };
        if let Err(error) = unsafe { SafeArrayPutElement(array, &0, item.as_raw()) } {
            unsafe { SafeArrayDestroy(array) }?;
            return Err(error);
        }
    }
    Ok(array)
}

fn item_runtime_id(hwnd: HWND, index: i32) -> Result<*mut SAFEARRAY> {
    let array = unsafe { SafeArrayCreateVector(VT_I4, 0, 3) };
    if array.is_null() {
        return Err(E_OUTOFMEMORY.into());
    }
    let values = [UiaAppendRuntimeId as i32, hwnd.0 as i32, index];
    for (position, value) in values.iter().enumerate() {
        if let Err(error) =
            unsafe { SafeArrayPutElement(array, &(position as i32), (value as *const i32).cast()) }
        {
            unsafe { SafeArrayDestroy(array) }?;
            return Err(error);
        }
    }
    Ok(array)
}

fn null_fragment() -> IRawElementProviderFragment {
    unsafe { Interface::from_raw(std::ptr::null_mut()) }
}

fn combo_list_rect(list: HWND) -> Result<RECT> {
    ensure_window(list)?;
    let mut rect = RECT::default();
    unsafe { GetWindowRect(list, &mut rect)? };
    Ok(rect)
}

fn combo_item_rect(list: HWND, _combo: HWND, index: i32) -> Result<UiaRect> {
    let mut rect = windows::Win32::Foundation::RECT::default();
    if unsafe {
        SendMessageW(
            list,
            LB_GETITEMRECT,
            WPARAM(index as usize),
            LPARAM((&mut rect as *mut windows::Win32::Foundation::RECT) as isize),
        )
        .0
    } == 0
    {
        return Ok(UiaRect::default());
    }
    let mut top_left = windows::Win32::Foundation::POINT {
        x: rect.left,
        y: rect.top,
    };
    let mut bottom_right = windows::Win32::Foundation::POINT {
        x: rect.right,
        y: rect.bottom,
    };
    unsafe {
        ClientToScreen(list, &mut top_left);
        ClientToScreen(list, &mut bottom_right);
    }
    if bottom_right.y <= top_left.y || bottom_right.x <= top_left.x {
        return Ok(UiaRect::default());
    }
    Ok(UiaRect {
        left: top_left.x as f64,
        top: top_left.y as f64,
        width: (bottom_right.x - top_left.x) as f64,
        height: (bottom_right.y - top_left.y) as f64,
    })
}

fn set_i32(value: i32) -> VARIANT {
    let mut variant = VARIANT::default();
    unsafe {
        (*variant.Anonymous.Anonymous).vt = VT_I4;
        (*variant.Anonymous.Anonymous).Anonymous.lVal = value;
    }
    variant
}

fn set_bool(value: bool) -> VARIANT {
    let mut variant = VARIANT::default();
    unsafe {
        (*variant.Anonymous.Anonymous).vt = VT_BOOL;
        (*variant.Anonymous.Anonymous).Anonymous.boolVal =
            windows::Win32::Foundation::VARIANT_BOOL(if value { -1 } else { 0 });
    }
    variant
}

fn set_string(value: &str) -> VARIANT {
    let mut variant = VARIANT::default();
    unsafe {
        (*variant.Anonymous.Anonymous).vt = VT_BSTR;
        (*variant.Anonymous.Anonymous).Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(value));
    }
    variant
}

#[implement(
    IRawElementProviderSimple,
    IInvokeProvider,
    IToggleProvider,
    IExpandCollapseProvider,
    IValueProvider,
    IRangeValueProvider,
    ISelectionItemProvider,
    ISelectionProvider
)]
struct NativeControlProvider {
    hwnd: HWND,
}

#[implement(IRawElementProviderSimple, ISelectionProvider)]
struct SettingsProvider {
    hwnd: HWND,
}

#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot,
    ISelectionProvider
)]
struct ComboListProvider {
    hwnd: HWND,
    combo: HWND,
}

#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    ISelectionItemProvider
)]
struct ComboListItemProvider {
    hwnd: HWND,
    combo: HWND,
    index: i32,
}

impl NativeControlProvider {
    fn kind(&self) -> Result<ControlKind> {
        ensure_window(self.hwnd)?;
        let mut buffer = [0u16; 64];
        let length = unsafe { GetClassNameW(self.hwnd, &mut buffer) };
        let class = String::from_utf16_lossy(&buffer[..length.max(0) as usize]);
        let style = unsafe { GetWindowLongW(self.hwnd, GWL_STYLE) } as u32;
        Ok(if class.eq_ignore_ascii_case("Button") {
            match (style & 0x0f) as i32 {
                BS_AUTOCHECKBOX | BS_AUTO3STATE | BS_CHECKBOX => ControlKind::Toggle,
                BS_AUTORADIOBUTTON | BS_RADIOBUTTON => ControlKind::Radio,
                _ => ControlKind::Button,
            }
        } else if class.eq_ignore_ascii_case("ComboBox")
            && matches!((style & 0x0f) as i32, CBS_DROPDOWN | CBS_DROPDOWNLIST)
        {
            ControlKind::ComboBox
        } else if class.eq_ignore_ascii_case("Edit") {
            ControlKind::Edit
        } else if class.eq_ignore_ascii_case("msctls_trackbar32") {
            ControlKind::Slider
        } else if class.eq_ignore_ascii_case("Static") {
            ControlKind::Text
        } else {
            ControlKind::Other
        })
    }

    fn text(&self) -> Result<String> {
        ensure_window(self.hwnd)?;
        let length = unsafe { GetWindowTextLengthW(self.hwnd) }.clamp(0, 4096) as usize;
        let mut buffer = vec![0u16; length + 1];
        let copied = unsafe { GetWindowTextW(self.hwnd, &mut buffer) };
        Ok(String::from_utf16_lossy(&buffer[..copied.max(0) as usize]))
    }

    fn click(&self) -> Result<()> {
        ensure_window(self.hwnd)?;
        if !unsafe { IsWindowEnabled(self.hwnd).as_bool() } {
            return Err(windows::core::HRESULT(UIA_E_ELEMENTNOTENABLED as i32).into());
        }
        unsafe { PostMessageW(self.hwnd, BM_CLICK, WPARAM(0), LPARAM(0)) }
    }
}

impl IRawElementProviderSimple_Impl for NativeControlProvider {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, patternid: UIA_PATTERN_ID) -> Result<IUnknown> {
        let kind = self.kind()?;
        if patternid == UIA_InvokePatternId && kind == ControlKind::Button {
            let provider: IInvokeProvider = NativeControlProvider { hwnd: self.hwnd }.into();
            return provider.cast();
        }
        if patternid == UIA_SelectionItemPatternId && kind == ControlKind::Radio {
            let provider: ISelectionItemProvider = NativeControlProvider { hwnd: self.hwnd }.into();
            return provider.cast();
        }
        if patternid == UIA_TogglePatternId && kind == ControlKind::Toggle {
            let provider: IToggleProvider = NativeControlProvider { hwnd: self.hwnd }.into();
            return provider.cast();
        }
        if patternid == UIA_ExpandCollapsePatternId && kind == ControlKind::ComboBox {
            let provider: IExpandCollapseProvider =
                NativeControlProvider { hwnd: self.hwnd }.into();
            return provider.cast();
        }
        if patternid == UIA_SelectionPatternId && kind == ControlKind::ComboBox {
            let provider: ISelectionProvider = NativeControlProvider { hwnd: self.hwnd }.into();
            return provider.cast();
        }
        if patternid == UIA_ValuePatternId && kind == ControlKind::Edit {
            let provider: IValueProvider = NativeControlProvider { hwnd: self.hwnd }.into();
            return provider.cast();
        }
        if patternid == UIA_RangeValuePatternId && kind == ControlKind::Slider {
            let provider: IRangeValueProvider = NativeControlProvider { hwnd: self.hwnd }.into();
            return provider.cast();
        }
        Err(E_NOTIMPL.into())
    }

    fn GetPropertyValue(&self, propertyid: UIA_PROPERTY_ID) -> Result<VARIANT> {
        ensure_window(self.hwnd)?;
        let kind = self.kind()?;
        let control_type = match kind {
            ControlKind::Button => UIA_ButtonControlTypeId.0,
            ControlKind::Toggle => UIA_CheckBoxControlTypeId.0,
            ControlKind::Radio => UIA_RadioButtonControlTypeId.0,
            ControlKind::ComboBox => UIA_ComboBoxControlTypeId.0,
            ControlKind::Edit => UIA_EditControlTypeId.0,
            ControlKind::Slider => UIA_SliderControlTypeId.0,
            ControlKind::Text => UIA_TextControlTypeId.0,
            ControlKind::Other => UIA_CustomControlTypeId.0,
        };
        Ok(match propertyid {
            UIA_ControlTypePropertyId => set_i32(control_type as i32),
            UIA_NamePropertyId => set_string(&self.text()?),
            UIA_AutomationIdPropertyId => {
                let id = unsafe { GetDlgCtrlID(self.hwnd) };
                if id > 0 {
                    set_string(&id.to_string())
                } else {
                    set_string("")
                }
            }
            UIA_ClassNamePropertyId => {
                let mut buffer = [0u16; 64];
                let length = unsafe { GetClassNameW(self.hwnd, &mut buffer) };
                set_string(&String::from_utf16_lossy(&buffer[..length.max(0) as usize]))
            }
            UIA_FrameworkIdPropertyId => set_string("Win32"),
            UIA_IsEnabledPropertyId => set_bool(unsafe { IsWindowEnabled(self.hwnd).as_bool() }),
            UIA_IsControlElementPropertyId | UIA_IsContentElementPropertyId => set_bool(true),
            UIA_NativeWindowHandlePropertyId => set_i32(self.hwnd.0 as i32),
            UIA_HasKeyboardFocusPropertyId => set_bool(unsafe { GetFocus() == self.hwnd }),
            _ => VARIANT::default(),
        })
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(self.hwnd) }
    }
}

impl IInvokeProvider_Impl for NativeControlProvider {
    fn Invoke(&self) -> Result<()> {
        if self.kind()? != ControlKind::Button {
            return Err(E_NOTIMPL.into());
        }
        self.click()
    }
}

impl ISelectionItemProvider_Impl for NativeControlProvider {
    fn Select(&self) -> Result<()> {
        if self.kind()? != ControlKind::Radio {
            return Err(E_NOTIMPL.into());
        }
        self.click()
    }

    fn AddToSelection(&self) -> Result<()> {
        self.Select()
    }

    fn RemoveFromSelection(&self) -> Result<()> {
        if self.kind()? != ControlKind::Radio {
            return Err(E_NOTIMPL.into());
        }
        Err(windows::core::HRESULT(UIA_E_INVALIDOPERATION as i32).into())
    }

    fn IsSelected(&self) -> Result<windows::Win32::Foundation::BOOL> {
        if self.kind()? != ControlKind::Radio {
            return Err(E_NOTIMPL.into());
        }
        Ok(windows::Win32::Foundation::BOOL(i32::from(
            check_state(self.hwnd)?.0 == BST_CHECKED.0,
        )))
    }

    fn SelectionContainer(&self) -> Result<IRawElementProviderSimple> {
        if self.kind()? != ControlKind::Radio {
            return Err(E_NOTIMPL.into());
        }
        let parent = unsafe { GetParent(self.hwnd) };
        unsafe { settings_provider(parent) }
    }
}

impl IRawElementProviderSimple_Impl for SettingsProvider {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, patternid: UIA_PATTERN_ID) -> Result<IUnknown> {
        ensure_window(self.hwnd)?;
        if patternid == UIA_SelectionPatternId {
            let provider: ISelectionProvider = SettingsProvider { hwnd: self.hwnd }.into();
            return provider.cast();
        }
        Err(E_NOTIMPL.into())
    }

    fn GetPropertyValue(&self, propertyid: UIA_PROPERTY_ID) -> Result<VARIANT> {
        ensure_window(self.hwnd)?;
        Ok(match propertyid {
            UIA_ControlTypePropertyId => set_i32(UIA_WindowControlTypeId.0 as i32),
            UIA_NamePropertyId => set_string("Isle 设置"),
            UIA_AutomationIdPropertyId => set_string("IsleSettings"),
            UIA_ClassNamePropertyId => set_string("IsleNativeSettingsV2"),
            UIA_FrameworkIdPropertyId => set_string("Win32"),
            UIA_IsEnabledPropertyId => set_bool(unsafe { IsWindowEnabled(self.hwnd).as_bool() }),
            UIA_IsControlElementPropertyId | UIA_IsContentElementPropertyId => set_bool(true),
            UIA_NativeWindowHandlePropertyId => set_i32(self.hwnd.0 as i32),
            UIA_HasKeyboardFocusPropertyId => set_bool(unsafe { GetFocus() == self.hwnd }),
            _ => VARIANT::default(),
        })
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(self.hwnd) }
    }
}

impl ISelectionProvider_Impl for SettingsProvider {
    fn GetSelection(&self) -> Result<*mut SAFEARRAY> {
        let selected = selected_radio(self.hwnd)?;
        let array = unsafe { SafeArrayCreateVector(VT_UNKNOWN, 0, 1) };
        if array.is_null() {
            return Err(E_OUTOFMEMORY.into());
        }
        let item: IRawElementProviderSimple = NativeControlProvider { hwnd: selected }.into();
        let index = 0i32;
        if let Err(error) = unsafe { SafeArrayPutElement(array, &index, item.as_raw()) } {
            unsafe { SafeArrayDestroy(array) }?;
            return Err(error);
        }
        Ok(array)
    }

    fn CanSelectMultiple(&self) -> Result<windows::Win32::Foundation::BOOL> {
        ensure_window(self.hwnd)?;
        Ok(windows::Win32::Foundation::BOOL(0))
    }

    fn IsSelectionRequired(&self) -> Result<windows::Win32::Foundation::BOOL> {
        ensure_window(self.hwnd)?;
        Ok(windows::Win32::Foundation::BOOL(1))
    }
}

impl ISelectionProvider_Impl for NativeControlProvider {
    fn GetSelection(&self) -> Result<*mut SAFEARRAY> {
        if self.kind()? != ControlKind::ComboBox {
            return Err(E_NOTIMPL.into());
        }
        combo_selection_array(self.hwnd)
    }

    fn CanSelectMultiple(&self) -> Result<windows::Win32::Foundation::BOOL> {
        if self.kind()? != ControlKind::ComboBox {
            return Err(E_NOTIMPL.into());
        }
        Ok(windows::Win32::Foundation::BOOL(0))
    }

    fn IsSelectionRequired(&self) -> Result<windows::Win32::Foundation::BOOL> {
        if self.kind()? != ControlKind::ComboBox {
            return Err(E_NOTIMPL.into());
        }
        Ok(windows::Win32::Foundation::BOOL(1))
    }
}

impl IRawElementProviderSimple_Impl for ComboListProvider {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, patternid: UIA_PATTERN_ID) -> Result<IUnknown> {
        ensure_window(self.hwnd)?;
        if patternid == UIA_SelectionPatternId {
            let provider: ISelectionProvider = ComboListProvider {
                hwnd: self.hwnd,
                combo: self.combo,
            }
            .into();
            return provider.cast();
        }
        Err(E_NOTIMPL.into())
    }

    fn GetPropertyValue(&self, propertyid: UIA_PROPERTY_ID) -> Result<VARIANT> {
        ensure_window(self.hwnd)?;
        Ok(match propertyid {
            UIA_ControlTypePropertyId => set_i32(UIA_ListControlTypeId.0 as i32),
            UIA_NamePropertyId => set_string("选项"),
            UIA_AutomationIdPropertyId => {
                set_string(&format!("ComboList{}", unsafe { GetDlgCtrlID(self.combo) }))
            }
            UIA_ClassNamePropertyId => set_string("ComboLBox"),
            UIA_FrameworkIdPropertyId => set_string("Win32"),
            UIA_IsEnabledPropertyId => set_bool(unsafe { IsWindowEnabled(self.combo).as_bool() }),
            UIA_IsControlElementPropertyId | UIA_IsContentElementPropertyId => set_bool(true),
            UIA_NativeWindowHandlePropertyId => set_i32(self.hwnd.0 as i32),
            _ => VARIANT::default(),
        })
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(self.hwnd) }
    }
}

impl ISelectionProvider_Impl for ComboListProvider {
    fn GetSelection(&self) -> Result<*mut SAFEARRAY> {
        combo_selection_array(self.combo)
    }

    fn CanSelectMultiple(&self) -> Result<windows::Win32::Foundation::BOOL> {
        ensure_window(self.combo)?;
        Ok(windows::Win32::Foundation::BOOL(0))
    }

    fn IsSelectionRequired(&self) -> Result<windows::Win32::Foundation::BOOL> {
        ensure_window(self.combo)?;
        Ok(windows::Win32::Foundation::BOOL(1))
    }
}

impl IRawElementProviderFragment_Impl for ComboListProvider {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let count = combo_item_count(self.combo)?;
        match direction {
            NavigateDirection_Parent => Ok(null_fragment()),
            NavigateDirection_FirstChild if count > 0 => Ok(ComboListItemProvider {
                hwnd: self.hwnd,
                combo: self.combo,
                index: 0,
            }
            .into()),
            NavigateDirection_LastChild if count > 0 => Ok(ComboListItemProvider {
                hwnd: self.hwnd,
                combo: self.combo,
                index: count - 1,
            }
            .into()),
            _ => Ok(null_fragment()),
        }
    }

    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        item_runtime_id(self.hwnd, -1)
    }

    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let rect = combo_list_rect(self.hwnd)?;
        Ok(UiaRect {
            left: rect.left as f64,
            top: rect.top as f64,
            width: (rect.right - rect.left) as f64,
            height: (rect.bottom - rect.top) as f64,
        })
    }

    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn SetFocus(&self) -> Result<()> {
        unsafe { windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(self.hwnd) };
        Ok(())
    }

    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        let provider: IRawElementProviderFragmentRoot = ComboListProvider {
            hwnd: self.hwnd,
            combo: self.combo,
        }
        .into();
        Ok(provider)
    }
}

impl IRawElementProviderFragmentRoot_Impl for ComboListProvider {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        for index in 0..combo_item_count(self.combo)? {
            let rect = combo_item_rect(self.hwnd, self.combo, index)?;
            if rect.width > 0.0
                && x >= rect.left
                && x < rect.left + rect.width
                && y >= rect.top
                && y < rect.top + rect.height
            {
                return Ok(ComboListItemProvider {
                    hwnd: self.hwnd,
                    combo: self.combo,
                    index,
                }
                .into());
            }
        }
        Ok(null_fragment())
    }

    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        if unsafe { GetFocus() } == self.hwnd {
            let index =
                unsafe { SendMessageW(self.combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32 };
            if index >= 0 {
                return Ok(ComboListItemProvider {
                    hwnd: self.hwnd,
                    combo: self.combo,
                    index,
                }
                .into());
            }
        }
        Ok(null_fragment())
    }
}

impl IRawElementProviderSimple_Impl for ComboListItemProvider {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, patternid: UIA_PATTERN_ID) -> Result<IUnknown> {
        combo_item_text(self.combo, self.index)?;
        if patternid == UIA_SelectionItemPatternId {
            let provider: ISelectionItemProvider = ComboListItemProvider {
                hwnd: self.hwnd,
                combo: self.combo,
                index: self.index,
            }
            .into();
            return provider.cast();
        }
        Err(E_NOTIMPL.into())
    }

    fn GetPropertyValue(&self, propertyid: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let name = combo_item_text(self.combo, self.index)?;
        let rect = combo_item_rect(self.hwnd, self.combo, self.index)?;
        Ok(match propertyid {
            UIA_ControlTypePropertyId => set_i32(UIA_ListItemControlTypeId.0 as i32),
            UIA_NamePropertyId => set_string(&name),
            UIA_AutomationIdPropertyId => set_string(&format!(
                "{}:{}",
                unsafe { GetDlgCtrlID(self.combo) },
                self.index
            )),
            UIA_ClassNamePropertyId => set_string("ListItem"),
            UIA_FrameworkIdPropertyId => set_string("Win32"),
            UIA_IsEnabledPropertyId => set_bool(unsafe { IsWindowEnabled(self.combo).as_bool() }),
            UIA_IsControlElementPropertyId | UIA_IsContentElementPropertyId => set_bool(true),
            UIA_IsOffscreenPropertyId => set_bool(rect.width <= 0.0 || rect.height <= 0.0),
            UIA_NativeWindowHandlePropertyId => set_i32(0),
            UIA_HasKeyboardFocusPropertyId => set_bool(
                unsafe { GetFocus() == self.hwnd }
                    && unsafe {
                        SendMessageW(self.combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32
                    } == self.index,
            ),
            _ => VARIANT::default(),
        })
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(self.hwnd) }
    }
}

impl ISelectionItemProvider_Impl for ComboListItemProvider {
    fn Select(&self) -> Result<()> {
        combo_item_text(self.combo, self.index)?;
        if !unsafe { IsWindowEnabled(self.combo).as_bool() } {
            return Err(windows::core::HRESULT(UIA_E_ELEMENTNOTENABLED as i32).into());
        }
        unsafe {
            SendMessageW(
                self.combo,
                CB_SETCURSEL,
                WPARAM(self.index as usize),
                LPARAM(0),
            );
            let parent = GetParent(self.combo);
            let command = ((CBN_SELCHANGE as usize) << 16) | GetDlgCtrlID(self.combo) as usize;
            SendMessageW(parent, WM_COMMAND, WPARAM(command), LPARAM(self.combo.0));
            SendMessageW(self.combo, CB_SHOWDROPDOWN, WPARAM(0), LPARAM(0));
        }
        Ok(())
    }

    fn AddToSelection(&self) -> Result<()> {
        self.Select()
    }

    fn RemoveFromSelection(&self) -> Result<()> {
        combo_item_text(self.combo, self.index)?;
        Err(windows::core::HRESULT(UIA_E_INVALIDOPERATION as i32).into())
    }

    fn IsSelected(&self) -> Result<windows::Win32::Foundation::BOOL> {
        combo_item_text(self.combo, self.index)?;
        Ok(windows::Win32::Foundation::BOOL(i32::from(
            unsafe { SendMessageW(self.combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32 }
                == self.index,
        )))
    }

    fn SelectionContainer(&self) -> Result<IRawElementProviderSimple> {
        combo_item_text(self.combo, self.index)?;
        Ok(NativeControlProvider { hwnd: self.combo }.into())
    }
}

impl IRawElementProviderFragment_Impl for ComboListItemProvider {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let count = combo_item_count(self.combo)?;
        match direction {
            NavigateDirection_Parent => Ok(ComboListProvider {
                hwnd: self.hwnd,
                combo: self.combo,
            }
            .into()),
            NavigateDirection_NextSibling if self.index + 1 < count => Ok(ComboListItemProvider {
                hwnd: self.hwnd,
                combo: self.combo,
                index: self.index + 1,
            }
            .into()),
            NavigateDirection_PreviousSibling if self.index > 0 => Ok(ComboListItemProvider {
                hwnd: self.hwnd,
                combo: self.combo,
                index: self.index - 1,
            }
            .into()),
            _ => Ok(null_fragment()),
        }
    }

    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        item_runtime_id(self.hwnd, self.index)
    }

    fn BoundingRectangle(&self) -> Result<UiaRect> {
        combo_item_rect(self.hwnd, self.combo, self.index)
    }

    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn SetFocus(&self) -> Result<()> {
        unsafe {
            windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(self.combo);
            SendMessageW(self.combo, CB_SHOWDROPDOWN, WPARAM(1), LPARAM(0));
        }
        Ok(())
    }

    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        let provider: IRawElementProviderFragmentRoot = ComboListProvider {
            hwnd: self.hwnd,
            combo: self.combo,
        }
        .into();
        Ok(provider)
    }
}

impl IToggleProvider_Impl for NativeControlProvider {
    fn Toggle(&self) -> Result<()> {
        if self.kind()? != ControlKind::Toggle {
            return Err(E_NOTIMPL.into());
        }
        self.click()
    }

    fn ToggleState(&self) -> Result<ToggleState> {
        if self.kind()? != ControlKind::Toggle {
            return Err(E_NOTIMPL.into());
        }
        Ok(match check_state(self.hwnd)?.0 {
            value if value == BST_CHECKED.0 => ToggleState_On,
            value if value == BST_INDETERMINATE.0 => ToggleState_Indeterminate,
            _ => ToggleState_Off,
        })
    }
}

impl IExpandCollapseProvider_Impl for NativeControlProvider {
    fn Expand(&self) -> Result<()> {
        self.set_expanded(true)
    }

    fn Collapse(&self) -> Result<()> {
        self.set_expanded(false)
    }

    fn ExpandCollapseState(&self) -> Result<ExpandCollapseState> {
        if self.kind()? != ControlKind::ComboBox {
            return Err(E_NOTIMPL.into());
        }
        Ok(
            if unsafe { SendMessageW(self.hwnd, CB_GETDROPPEDSTATE, WPARAM(0), LPARAM(0)).0 != 0 } {
                ExpandCollapseState_Expanded
            } else {
                ExpandCollapseState_Collapsed
            },
        )
    }
}

impl IValueProvider_Impl for NativeControlProvider {
    fn SetValue(&self, value: &PCWSTR) -> Result<()> {
        if self.kind()? != ControlKind::Edit {
            return Err(E_NOTIMPL.into());
        }
        ensure_window(self.hwnd)?;
        let style = unsafe { GetWindowLongW(self.hwnd, GWL_STYLE) } as u32;
        if style & ES_READONLY as u32 != 0 {
            return Err(windows::core::HRESULT(UIA_E_INVALIDOPERATION as i32).into());
        }
        if !unsafe { IsWindowEnabled(self.hwnd).as_bool() } {
            return Err(windows::core::HRESULT(UIA_E_ELEMENTNOTENABLED as i32).into());
        }
        if value.is_null() {
            return Err(windows::Win32::Foundation::E_INVALIDARG.into());
        }
        let value = unsafe { value.to_string()? };
        if value.chars().count() > 4096 {
            return Err(windows::core::HRESULT(UIA_E_INVALIDOPERATION as i32).into());
        }
        let wide = value
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        unsafe { SetWindowTextW(self.hwnd, PCWSTR(wide.as_ptr())) }
    }

    fn Value(&self) -> Result<BSTR> {
        if self.kind()? != ControlKind::Edit {
            return Err(E_NOTIMPL.into());
        }
        Ok(BSTR::from(self.text()?))
    }

    fn IsReadOnly(&self) -> Result<windows::Win32::Foundation::BOOL> {
        if self.kind()? != ControlKind::Edit {
            return Err(E_NOTIMPL.into());
        }
        ensure_window(self.hwnd)?;
        let style = unsafe { GetWindowLongW(self.hwnd, GWL_STYLE) } as u32;
        Ok(windows::Win32::Foundation::BOOL(i32::from(
            style & ES_READONLY as u32 != 0 || !unsafe { IsWindowEnabled(self.hwnd).as_bool() },
        )))
    }
}

impl IRangeValueProvider_Impl for NativeControlProvider {
    fn SetValue(&self, value: f64) -> Result<()> {
        if self.kind()? != ControlKind::Slider {
            return Err(E_NOTIMPL.into());
        }
        ensure_window(self.hwnd)?;
        if !unsafe { IsWindowEnabled(self.hwnd).as_bool() } {
            return Err(windows::core::HRESULT(UIA_E_ELEMENTNOTENABLED as i32).into());
        }
        let minimum = unsafe { SendMessageW(self.hwnd, TBM_GETRANGEMIN, WPARAM(0), LPARAM(0)).0 };
        let maximum = unsafe { SendMessageW(self.hwnd, TBM_GETRANGEMAX, WPARAM(0), LPARAM(0)).0 };
        if !value.is_finite() || value < minimum as f64 || value > maximum as f64 {
            return Err(windows::core::HRESULT(UIA_E_INVALIDOPERATION as i32).into());
        }
        unsafe {
            SendMessageW(
                self.hwnd,
                TBM_SETPOSNOTIFY,
                WPARAM(1),
                LPARAM(value.round() as isize),
            );
        }
        Ok(())
    }

    fn Value(&self) -> Result<f64> {
        if self.kind()? != ControlKind::Slider {
            return Err(E_NOTIMPL.into());
        }
        ensure_window(self.hwnd)?;
        Ok(unsafe { SendMessageW(self.hwnd, WM_USER, WPARAM(0), LPARAM(0)).0 as f64 })
    }

    fn IsReadOnly(&self) -> Result<windows::Win32::Foundation::BOOL> {
        if self.kind()? != ControlKind::Slider {
            return Err(E_NOTIMPL.into());
        }
        ensure_window(self.hwnd)?;
        Ok(windows::Win32::Foundation::BOOL(i32::from(!unsafe {
            IsWindowEnabled(self.hwnd).as_bool()
        })))
    }

    fn Maximum(&self) -> Result<f64> {
        if self.kind()? != ControlKind::Slider {
            return Err(E_NOTIMPL.into());
        }
        ensure_window(self.hwnd)?;
        Ok(unsafe { SendMessageW(self.hwnd, TBM_GETRANGEMAX, WPARAM(0), LPARAM(0)).0 as f64 })
    }

    fn Minimum(&self) -> Result<f64> {
        if self.kind()? != ControlKind::Slider {
            return Err(E_NOTIMPL.into());
        }
        ensure_window(self.hwnd)?;
        Ok(unsafe { SendMessageW(self.hwnd, TBM_GETRANGEMIN, WPARAM(0), LPARAM(0)).0 as f64 })
    }

    fn LargeChange(&self) -> Result<f64> {
        if self.kind()? != ControlKind::Slider {
            return Err(E_NOTIMPL.into());
        }
        ensure_window(self.hwnd)?;
        Ok(unsafe { SendMessageW(self.hwnd, TBM_GETPAGESIZE, WPARAM(0), LPARAM(0)).0 as f64 })
    }

    fn SmallChange(&self) -> Result<f64> {
        if self.kind()? != ControlKind::Slider {
            return Err(E_NOTIMPL.into());
        }
        ensure_window(self.hwnd)?;
        Ok(unsafe { SendMessageW(self.hwnd, TBM_GETLINESIZE, WPARAM(0), LPARAM(0)).0 as f64 })
    }
}

impl NativeControlProvider {
    fn set_expanded(&self, expanded: bool) -> Result<()> {
        if self.kind()? != ControlKind::ComboBox {
            return Err(E_NOTIMPL.into());
        }
        ensure_window(self.hwnd)?;
        if !unsafe { IsWindowEnabled(self.hwnd).as_bool() } {
            return Err(windows::core::HRESULT(UIA_E_ELEMENTNOTENABLED as i32).into());
        }
        unsafe {
            PostMessageW(
                self.hwnd,
                CB_SHOWDROPDOWN,
                WPARAM(usize::from(expanded)),
                LPARAM(0),
            )
        }
    }
}

/// Returns the provider used for the child HWND's UIA root object request.
pub unsafe fn provider(hwnd: HWND) -> Result<IRawElementProviderSimple> {
    ensure_window(hwnd)?;
    Ok(NativeControlProvider { hwnd }.into())
}

/// Returns the root provider for the Settings window's radio-button selection.
pub unsafe fn settings_provider(hwnd: HWND) -> Result<IRawElementProviderSimple> {
    ensure_window(hwnd)?;
    Ok(SettingsProvider { hwnd }.into())
}

/// Returns the virtual list provider for an open Win32 ComboBox drop-down.
pub unsafe fn combo_list_provider(hwnd: HWND, combo: HWND) -> Result<IRawElementProviderSimple> {
    ensure_window(hwnd)?;
    ensure_window(combo)?;
    Ok(ComboListProvider { hwnd, combo }.into())
}
