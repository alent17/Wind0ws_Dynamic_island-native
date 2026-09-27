//! Native MSAA provider. Windows can expose this through its legacy UIA bridge.
//! Provider callbacks read snapshots; all mutations are posted to the UI thread.
#![allow(non_snake_case)]
use isle_ui::{geometry::*, model::*};
use std::sync::{Arc, Mutex};
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        System::{Com::*, Variant::*},
        UI::{
            Accessibility::*,
            Controls::{STATE_SYSTEM_FOCUSABLE, STATE_SYSTEM_INVISIBLE, STATE_SYSTEM_UNAVAILABLE},
            WindowsAndMessaging::*,
        },
    },
};

pub const INVOKE: u32 = WM_APP + 50;
pub const FOCUS: u32 = WM_APP + 51;
pub const VALUE: u32 = WM_APP + 52;
#[derive(Clone)]
pub struct Node {
    pub key: String,
    pub enabled: bool,
    pub hit: Hit,
    pub name: String,
    pub rect: Rect,
}
#[derive(Default)]
pub struct Snapshot {
    pub hwnd: isize,
    pub revision: u32,
    pub visible: bool,
    pub closed: bool,
    pub nodes: Vec<Node>,
    pub bounds: Rect,
    pub outline: Vec<Point>,
    pub focus: Option<Hit>,
    pub volume: u32,
}
pub type Shared = Arc<Mutex<Snapshot>>;
pub fn label(hit: Hit, playing: bool) -> &'static str {
    match hit {
        Hit::Tool(i) => [
            "倒计时",
            "音量",
            "悬浮播放器（原型说明）",
            "设置",
            "收起",
            "时间",
            "天气",
        ]
        .get(i)
        .copied()
        .unwrap_or("功能"),
        Hit::Blank => "展开音乐",
        Hit::Back => "返回音乐",
        Hit::Play if playing => "暂停",
        Hit::Play => "播放",
        Hit::Previous => "上一首",
        Hit::Next => "下一首",
        Hit::Volume => "音量",
        Hit::Timer => "开始或暂停倒计时",
        Hit::Reset => "重置倒计时",
        Hit::Devices => "选择输出设备",
        Hit::Device(_) => "输出设备",
        Hit::DevicePrev => "上一页设备",
        Hit::DeviceNext => "下一页设备",
        Hit::Mute => "切换静音",
        Hit::WeatherSettings => "设置天气城市",
    }
}
pub fn root(shared: &Shared) -> IAccessible {
    Provider {
        shared: shared.clone(),
        child: 0,
        revision: 0,
    }
    .into()
}
#[implement(IAccessible)]
struct Provider {
    shared: Shared,
    child: usize,
    revision: u32,
}
fn number(value: i32) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        (*v.Anonymous.Anonymous).vt = VT_I4;
        (*v.Anonymous.Anonymous).Anonymous.lVal = value;
    }
    v
}
fn dispatch(value: IDispatch) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        (*v.Anonymous.Anonymous).vt = VT_DISPATCH;
        (*v.Anonymous.Anonymous).Anonymous.pdispVal = std::mem::ManuallyDrop::new(Some(value));
    }
    v
}
fn id(v: &VARIANT) -> Result<usize> {
    unsafe {
        if (*v.Anonymous.Anonymous).vt == VT_I4 && v.Anonymous.Anonymous.Anonymous.lVal >= 0 {
            Ok(v.Anonymous.Anonymous.Anonymous.lVal as usize)
        } else {
            Err(E_INVALIDARG.into())
        }
    }
}
impl Provider {
    fn snapshot(&self) -> Result<std::sync::MutexGuard<'_, Snapshot>> {
        let s = self.shared.lock().map_err(|_| Error::from(E_FAIL))?;
        if s.closed || (self.child > 0 && s.revision != self.revision) {
            return Err(CO_E_OBJNOTCONNECTED.into());
        }
        Ok(s)
    }
    fn resolve(&self, v: &VARIANT, s: &Snapshot) -> Result<usize> {
        let requested = id(v)?;
        let n = if self.child == 0 {
            requested
        } else if requested == 0 {
            self.child
        } else {
            return Err(E_INVALIDARG.into());
        };
        if n <= s.nodes.len() {
            Ok(n)
        } else {
            Err(E_INVALIDARG.into())
        }
    }
    fn object(&self, child: usize, revision: u32) -> IDispatch {
        let a: IAccessible = Provider {
            shared: self.shared.clone(),
            child,
            revision,
        }
        .into();
        a.cast().unwrap()
    }
    fn post(&self, v: &VARIANT, msg: u32, value: u16) -> Result<()> {
        let s = self.snapshot()?;
        let n = self.resolve(v, &s)?;
        if !s.visible {
            return Err(E_ACCESSDENIED.into());
        }
        unsafe {
            PostMessageW(
                HWND(s.hwnd),
                msg,
                WPARAM(n),
                LPARAM(((s.revision as u64) << 16 | value as u64) as isize),
            )
        }
    }
}
impl IDispatch_Impl for Provider {
    fn GetTypeInfoCount(&self) -> Result<u32> {
        Ok(0)
    }
    fn GetTypeInfo(&self, _: u32, _: u32) -> Result<ITypeInfo> {
        Err(E_NOTIMPL.into())
    }
    fn GetIDsOfNames(
        &self,
        _: *const GUID,
        _: *const PCWSTR,
        _: u32,
        _: u32,
        _: *mut i32,
    ) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn Invoke(
        &self,
        _: i32,
        _: *const GUID,
        _: u32,
        _: DISPATCH_FLAGS,
        _: *const DISPPARAMS,
        _: *mut VARIANT,
        _: *mut EXCEPINFO,
        _: *mut u32,
    ) -> Result<()> {
        Err(DISP_E_MEMBERNOTFOUND.into())
    }
}
impl IAccessible_Impl for Provider {
    fn accParent(&self) -> Result<IDispatch> {
        let s = self.snapshot()?;
        if self.child > 0 {
            return Ok(self.object(0, 0));
        }
        unsafe {
            let mut p = std::ptr::null_mut();
            CreateStdAccessibleObject(HWND(s.hwnd), OBJID_WINDOW.0, &IDispatch::IID, &mut p)?;
            IDispatch::from_abi(p)
        }
    }
    fn accChildCount(&self) -> Result<i32> {
        let s = self.snapshot()?;
        Ok(if self.child == 0 {
            s.nodes.len() as i32
        } else {
            0
        })
    }
    fn get_accChild(&self, v: &VARIANT) -> Result<IDispatch> {
        let s = self.snapshot()?;
        let n = self.resolve(v, &s)?;
        if self.child != 0 || n == 0 {
            return Err(E_INVALIDARG.into());
        }
        Ok(self.object(n, s.revision))
    }
    fn get_accName(&self, v: &VARIANT) -> Result<BSTR> {
        let s = self.snapshot()?;
        let n = self.resolve(v, &s)?;
        Ok(BSTR::from(if n == 0 {
            "Isle 原生原型"
        } else {
            &s.nodes[n - 1].name
        }))
    }
    fn get_accValue(&self, v: &VARIANT) -> Result<BSTR> {
        let s = self.snapshot()?;
        let n = self.resolve(v, &s)?;
        Ok(if n > 0 && s.nodes[n - 1].hit == Hit::Volume {
            BSTR::from(s.volume.to_string())
        } else {
            BSTR::default()
        })
    }
    fn get_accDescription(&self, v: &VARIANT) -> Result<BSTR> {
        self.get_accName(v)
    }
    fn get_accRole(&self, v: &VARIANT) -> Result<VARIANT> {
        let s = self.snapshot()?;
        let n = self.resolve(v, &s)?;
        Ok(number(if n == 0 {
            ROLE_SYSTEM_CLIENT
        } else if s.nodes[n - 1].hit == Hit::Volume {
            ROLE_SYSTEM_SLIDER
        } else {
            ROLE_SYSTEM_PUSHBUTTON
        } as i32))
    }
    fn get_accState(&self, v: &VARIANT) -> Result<VARIANT> {
        let s = self.snapshot()?;
        let n = self.resolve(v, &s)?;
        let mut flags = STATE_SYSTEM_FOCUSABLE.0;
        if n > 0 && !s.nodes[n - 1].enabled {
            flags |= STATE_SYSTEM_UNAVAILABLE.0;
        }
        if !s.visible {
            flags |= STATE_SYSTEM_INVISIBLE.0 | STATE_SYSTEM_UNAVAILABLE.0;
        }
        if n > 0 && s.focus == Some(s.nodes[n - 1].hit) {
            flags |= STATE_SYSTEM_FOCUSED;
        }
        Ok(number(flags as i32))
    }
    fn get_accHelp(&self, _: &VARIANT) -> Result<BSTR> {
        Ok(BSTR::from(
            "Tab 切换控件，Enter 激活，Escape 返回。音量可用方向键调节。",
        ))
    }
    fn get_accHelpTopic(&self, file: *mut BSTR, _: &VARIANT, topic: *mut i32) -> Result<()> {
        unsafe {
            if !file.is_null() {
                file.write(BSTR::default());
            }
            if !topic.is_null() {
                topic.write(0);
            }
        }
        Err(E_NOTIMPL.into())
    }
    fn get_accKeyboardShortcut(&self, _: &VARIANT) -> Result<BSTR> {
        Ok(BSTR::default())
    }
    fn accFocus(&self) -> Result<VARIANT> {
        let s = self.snapshot()?;
        if self.child > 0 {
            return Ok(if s.focus == Some(s.nodes[self.child - 1].hit) {
                number(0)
            } else {
                VARIANT::default()
            });
        }
        Ok(s.nodes
            .iter()
            .position(|n| Some(n.hit) == s.focus)
            .map(|i| dispatch(self.object(i + 1, s.revision)))
            .unwrap_or_default())
    }
    fn accSelection(&self) -> Result<VARIANT> {
        Ok(VARIANT::default())
    }
    fn get_accDefaultAction(&self, v: &VARIANT) -> Result<BSTR> {
        let s = self.snapshot()?;
        let n = self.resolve(v, &s)?;
        Ok(BSTR::from(if n > 0 && s.nodes[n - 1].hit == Hit::Volume {
            "调整"
        } else {
            "按下"
        }))
    }
    fn accSelect(&self, flags: i32, v: &VARIANT) -> Result<()> {
        if flags & SELFLAG_TAKEFOCUS as i32 == 0 {
            return Err(E_INVALIDARG.into());
        }
        self.post(v, FOCUS, 0)
    }
    fn accLocation(
        &self,
        x: *mut i32,
        y: *mut i32,
        w: *mut i32,
        h: *mut i32,
        v: &VARIANT,
    ) -> Result<()> {
        if x.is_null() || y.is_null() || w.is_null() || h.is_null() {
            return Err(E_POINTER.into());
        }
        let s = self.snapshot()?;
        let n = self.resolve(v, &s)?;
        let r = if n == 0 {
            s.bounds
        } else {
            s.nodes[n - 1].rect
        };
        unsafe {
            x.write(r.x.round() as i32);
            y.write(r.y.round() as i32);
            w.write(r.w.round() as i32);
            h.write(r.h.round() as i32);
        }
        Ok(())
    }
    fn accNavigate(&self, dir: i32, v: &VARIANT) -> Result<VARIANT> {
        let s = self.snapshot()?;
        let n = self.resolve(v, &s)?;
        let target = match dir as u32 {
            NAVDIR_FIRSTCHILD if n == 0 => Some(1),
            NAVDIR_LASTCHILD if n == 0 => Some(s.nodes.len()),
            NAVDIR_NEXT if n > 0 => Some(n + 1),
            NAVDIR_PREVIOUS if n > 1 => Some(n - 1),
            _ => None,
        };
        Ok(target
            .filter(|n| *n > 0 && *n <= s.nodes.len())
            .map(|n| dispatch(self.object(n, s.revision)))
            .unwrap_or_default())
    }
    fn accHitTest(&self, x: i32, y: i32) -> Result<VARIANT> {
        let s = self.snapshot()?;
        let p = Point {
            x: x as f32,
            y: y as f32,
        };
        if !s.visible || !inside(p, &s.outline) {
            return Ok(VARIANT::default());
        }
        Ok(s.nodes
            .iter()
            .position(|n| n.rect.contains(p))
            .map(|i| dispatch(self.object(i + 1, s.revision)))
            .unwrap_or_else(|| number(0)))
    }
    fn accDoDefaultAction(&self, v: &VARIANT) -> Result<()> {
        self.post(v, INVOKE, 0)
    }
    fn put_accName(&self, _: &VARIANT, _: &BSTR) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn put_accValue(&self, v: &VARIANT, value: &BSTR) -> Result<()> {
        {
            let s = self.snapshot()?;
            let n = self.resolve(v, &s)?;
            if n == 0 || s.nodes[n - 1].hit != Hit::Volume {
                return Err(E_INVALIDARG.into());
            }
        }
        let value = value
            .to_string()
            .parse::<u16>()
            .map_err(|_| Error::from(E_INVALIDARG))?;
        if value > 100 {
            return Err(E_INVALIDARG.into());
        }
        self.post(v, VALUE, value)
    }
}
