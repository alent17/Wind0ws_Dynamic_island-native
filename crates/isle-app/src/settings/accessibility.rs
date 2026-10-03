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
            CO_E_OBJNOTCONNECTED, E_FAIL, E_NOTIMPL, E_OUTOFMEMORY, HWND, LPARAM, WPARAM,
        },
        System::{
            Com::SAFEARRAY,
            Ole::{SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement},
            Variant::*,
        },
        UI::{
            Accessibility::*,
            Controls::{
                BST_CHECKED, BST_INDETERMINATE, DLG_BUTTON_CHECK_STATE, TBM_GETLINESIZE,
                TBM_GETPAGESIZE, TBM_GETRANGEMAX, TBM_GETRANGEMIN, TBM_SETPOSNOTIFY,
            },
            Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled},
            WindowsAndMessaging::{
                EnumChildWindows, GetClassNameW, GetDlgCtrlID, GetParent, GetWindowLongW,
                GetWindowTextLengthW, GetWindowTextW, IsWindow, PostMessageW, SendMessageW,
                SetWindowTextW, BM_CLICK, BM_GETCHECK, BS_AUTO3STATE, BS_AUTOCHECKBOX,
                BS_AUTORADIOBUTTON, BS_CHECKBOX, BS_RADIOBUTTON, CBS_DROPDOWN, CBS_DROPDOWNLIST,
                CB_GETDROPPEDSTATE, CB_SHOWDROPDOWN, ES_READONLY, GWL_STYLE, WM_USER,
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
    ISelectionItemProvider
)]
struct NativeControlProvider {
    hwnd: HWND,
}

#[implement(IRawElementProviderSimple, ISelectionProvider)]
struct SettingsProvider {
    hwnd: HWND,
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
