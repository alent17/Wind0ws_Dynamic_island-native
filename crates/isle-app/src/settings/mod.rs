pub mod controls;
pub mod legacy;
pub mod model;
pub mod render;
pub mod window;

use isle_core::{
    configuration::{Appearance, Controls},
    settings::SettingsWindowPlacement,
    weather::City,
};
use windows::{core::Result, Win32::Foundation::HWND, Win32::UI::WindowsAndMessaging::MSG};

pub use controls::ControlPaintDiagnostics;
pub use window::SaveState;

enum Inner {
    Legacy(legacy::Settings),
    V2(window::Settings),
}

/// Compatibility facade for the app message loop. `new` retains the legacy
/// window and `new_v2` opts into the native Direct2D shell.
pub struct Settings {
    pub hwnd: HWND,
    inner: Inner,
}

impl Settings {
    pub unsafe fn new(owner: HWND, controls: &Controls, appearance: &Appearance) -> Result<Self> {
        let inner = legacy::Settings::new(owner, controls, appearance)?;
        Ok(Self {
            hwnd: inner.hwnd,
            inner: Inner::Legacy(inner),
        })
    }

    // Kept for downstream callers that adopted the original V2 facade before
    // placement-aware construction was added.
    #[allow(dead_code)]
    pub unsafe fn new_v2(
        owner: HWND,
        controls: &Controls,
        appearance: &Appearance,
    ) -> Result<Self> {
        let inner = window::Settings::new_v2(owner, controls, appearance)?;
        Ok(Self {
            hwnd: inner.hwnd,
            inner: Inner::V2(inner),
        })
    }

    // Preserve the placement API independently of the test-fixture constructor.
    #[allow(dead_code)]
    pub unsafe fn new_v2_with_placement(
        owner: HWND,
        controls: &Controls,
        appearance: &Appearance,
        placement: Option<SettingsWindowPlacement>,
    ) -> Result<Self> {
        let inner =
            window::Settings::new_v2_with_placement(owner, controls, appearance, placement)?;
        Ok(Self {
            hwnd: inner.hwnd,
            inner: Inner::V2(inner),
        })
    }

    /// Creates the V2 window with the fixture-only control renderer fault gate.
    pub unsafe fn new_v2_with_placement_and_control_fixture(
        owner: HWND,
        controls: &Controls,
        appearance: &Appearance,
        placement: Option<SettingsWindowPlacement>,
        allow_test_faults: bool,
    ) -> Result<Self> {
        let inner = window::Settings::new_v2_with_placement_and_control_fixture(
            owner,
            controls,
            appearance,
            placement,
            allow_test_faults,
        )?;
        Ok(Self {
            hwnd: inner.hwnd,
            inner: Inner::V2(inner),
        })
    }

    pub unsafe fn controls(&self) -> Controls {
        match &self.inner {
            Inner::Legacy(value) => value.controls(),
            Inner::V2(value) => value.controls(),
        }
    }

    pub unsafe fn appearance(&self) -> Option<Appearance> {
        match &self.inner {
            Inner::Legacy(value) => value.appearance(),
            Inner::V2(value) => value.appearance(),
        }
    }

    pub unsafe fn query(&self) -> String {
        match &self.inner {
            Inner::Legacy(value) => value.query(),
            Inner::V2(value) => value.query(),
        }
    }

    pub unsafe fn selected(&self) -> Option<City> {
        match &self.inner {
            Inner::Legacy(value) => value.selected(),
            Inner::V2(value) => value.selected(),
        }
    }

    pub unsafe fn loading(&mut self) {
        match &mut self.inner {
            Inner::Legacy(value) => value.loading(),
            Inner::V2(value) => value.loading(),
        }
    }

    pub unsafe fn results(&mut self, cities: Vec<City>, failed: bool) {
        match &mut self.inner {
            Inner::Legacy(value) => value.results(cities, failed),
            Inner::V2(value) => value.results(cities, failed),
        }
    }

    pub unsafe fn saving(&self, saving: bool) {
        match &self.inner {
            Inner::Legacy(value) => value.saving(saving),
            Inner::V2(value) => value.saving(saving),
        }
    }

    pub unsafe fn message(&self, text: &str) {
        match &self.inner {
            Inner::Legacy(value) => value.message(text),
            Inner::V2(value) => value.message(text),
        }
    }

    pub unsafe fn route(&self, msg: &MSG) -> bool {
        match &self.inner {
            Inner::Legacy(value) => value.route(msg),
            Inner::V2(value) => value.route(msg),
        }
    }

    pub unsafe fn set_save_state(&self, state: SaveState) {
        if let Inner::V2(value) = &self.inner {
            value.set_save_state(state);
        }
    }

    pub unsafe fn sync_settings(&mut self, controls: &Controls, appearance: &Appearance) {
        if let Inner::V2(value) = &mut self.inner {
            value.sync_settings(controls, appearance);
        }
    }

    pub unsafe fn inspection_locked(&self) -> bool {
        match &self.inner {
            Inner::Legacy(_) => false,
            Inner::V2(value) => value.inspection_locked(),
        }
    }

    pub unsafe fn take_placement_update(&mut self) -> Option<SettingsWindowPlacement> {
        match &mut self.inner {
            Inner::Legacy(_) => None,
            Inner::V2(value) => value.take_placement_update(),
        }
    }

    pub unsafe fn control_diagnostics(&self) -> Option<ControlPaintDiagnostics> {
        match &self.inner {
            Inner::Legacy(_) => None,
            Inner::V2(value) => value.control_diagnostics(),
        }
    }

    pub unsafe fn test_control_fault(&self, kind: u32) -> bool {
        match &self.inner {
            Inner::Legacy(_) => false,
            Inner::V2(value) => value.test_control_fault(kind),
        }
    }
}
