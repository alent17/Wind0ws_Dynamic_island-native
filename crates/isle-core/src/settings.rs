//! Runtime settings state and coalesced persistence bookkeeping.
//!
//! Patches are applied to the in-memory snapshot immediately. Persistence is
//! represented by one latest pending snapshot and revision, so controls can
//! update freely while an older snapshot is being written.
use crate::{
    configuration::{Appearance, Controls, Document},
    weather::City,
    widgets::WidgetConfig,
    Selection,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub type Revision = u64;

/// A manually chosen native Settings window position and outer size.
/// Coordinates use physical screen pixels; size is DPI-independent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsWindowPlacement {
    pub x: i32,
    pub y: i32,
    pub width_dip: u32,
    pub height_dip: u32,
    pub dpi: u32,
}

impl SettingsWindowPlacement {
    pub const MAX_SIZE_DIP: u32 = 8192;
    pub const MIN_DPI: u32 = 48;
    pub const MAX_DPI: u32 = 960;

    pub fn validate(&self) -> Result<(), String> {
        if self.width_dip == 0
            || self.height_dip == 0
            || self.width_dip > Self::MAX_SIZE_DIP
            || self.height_dip > Self::MAX_SIZE_DIP
        {
            return Err("设置窗口尺寸无效".into());
        }
        if !(Self::MIN_DPI..=Self::MAX_DPI).contains(&self.dpi) {
            return Err("设置窗口 DPI 无效".into());
        }
        Ok(())
    }

    /// Convert the stored DIP outer size to physical pixels for a target
    /// monitor. Position remains in physical screen coordinates.
    pub fn size_px_for_dpi(&self, dpi: u32) -> Result<(i32, i32), String> {
        self.validate()?;
        if !(Self::MIN_DPI..=Self::MAX_DPI).contains(&dpi) {
            return Err("目标显示器 DPI 无效".into());
        }
        let to_px = |dip: u32| -> Result<i32, String> {
            let numerator = u64::from(dip)
                .checked_mul(u64::from(dpi))
                .ok_or_else(|| "设置窗口尺寸超出范围".to_string())?;
            let rounded = numerator
                .checked_add(48)
                .ok_or_else(|| "设置窗口尺寸超出范围".to_string())?
                / 96;
            i32::try_from(rounded).map_err(|_| "设置窗口尺寸超出范围".into())
        };
        Ok((to_px(self.width_dip)?, to_px(self.height_dip)?))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsSnapshot {
    pub city: Option<City>,
    pub controls: Controls,
    pub appearance: Appearance,
    pub selection: Selection,
    pub widgets: Vec<WidgetConfig>,
    pub window_placement: Option<SettingsWindowPlacement>,
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            city: None,
            controls: Controls::default(),
            appearance: Appearance::default(),
            selection: Selection::default(),
            widgets: crate::widgets::defaults(),
            window_placement: None,
        }
    }
}

impl SettingsSnapshot {
    fn validate(&self) -> Result<(), String> {
        let mut document = Document::default();
        if let Some(city) = &self.city {
            document.set_city(city)?;
        }
        document.set_appearance(&self.appearance)?;
        document.set_selection(&self.selection)?;
        document.set_widgets(&self.widgets)?;
        document.set_controls(&self.controls);
        document.set_window_placement(self.window_placement.as_ref())?;
        document.preferences()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppearancePatch {
    pub style: Option<String>,
    pub edge: Option<String>,
    pub edge_position: Option<u8>,
    pub compact_length: Option<u16>,
    pub collapsed_shoulder_radius: Option<u8>,
    pub expanded_shoulder_radius: Option<u8>,
    pub expanded_corner_radius: Option<u32>,
    pub floating_fill_color: Option<String>,
    pub floating_use_album_color: Option<bool>,
    pub show_spectrum: Option<bool>,
    pub spectrum_mode: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControlsPatch {
    pub panel: Option<bool>,
    pub tools: [Option<bool>; 7],
    pub animations: Option<bool>,
    pub reduced: Option<bool>,
    pub always_on_top: Option<bool>,
    pub floating_always_on_top: Option<bool>,
    pub time_zone: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModulesPatch {
    pub panel: Option<bool>,
    pub tools: [Option<bool>; 7],
    pub widgets: Option<Vec<WidgetConfig>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaPatch {
    pub selection: Option<Selection>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeatherPatch {
    /// `None` leaves the city unchanged.
    pub city: Option<City>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingsPatch {
    Appearance(AppearancePatch),
    Controls(ControlsPatch),
    Modules(ModulesPatch),
    Media(MediaPatch),
    Weather(WeatherPatch),
    WindowPlacement(Option<SettingsWindowPlacement>),
}

impl SettingsSnapshot {
    pub fn apply_patch(&mut self, patch: SettingsPatch) -> Result<bool, String> {
        let mut next = self.clone();
        match patch {
            SettingsPatch::Appearance(p) => {
                let a = &mut next.appearance;
                if let Some(v) = p.style {
                    a.style = v;
                }
                if let Some(v) = p.edge {
                    a.edge = v;
                }
                if let Some(v) = p.edge_position {
                    a.edge_position = v;
                }
                if let Some(v) = p.compact_length {
                    a.compact_length = v;
                }
                if let Some(v) = p.collapsed_shoulder_radius {
                    a.collapsed_shoulder_radius = v;
                }
                if let Some(v) = p.expanded_shoulder_radius {
                    a.expanded_shoulder_radius = v;
                }
                if let Some(v) = p.expanded_corner_radius {
                    a.expanded_corner_radius = v;
                }
                if let Some(v) = p.floating_fill_color {
                    a.floating_fill_color = v;
                }
                if let Some(v) = p.floating_use_album_color {
                    a.floating_use_album_color = v;
                }
                if let Some(v) = p.show_spectrum {
                    a.show_spectrum = v;
                }
                if let Some(v) = p.spectrum_mode {
                    a.spectrum_mode = v;
                }
            }
            SettingsPatch::Controls(p) => {
                let c = &mut next.controls;
                if let Some(v) = p.panel {
                    c.panel = v;
                }
                for (current, update) in c.tools.iter_mut().zip(p.tools) {
                    if let Some(v) = update {
                        *current = v;
                    }
                }
                if let Some(v) = p.animations {
                    c.animations = v;
                }
                if let Some(v) = p.reduced {
                    c.reduced = v;
                }
                if let Some(v) = p.always_on_top {
                    c.always_on_top = v;
                }
                if let Some(v) = p.floating_always_on_top {
                    c.floating_always_on_top = v;
                }
                if let Some(v) = p.time_zone {
                    c.time_zone = v;
                }
            }
            SettingsPatch::Modules(p) => {
                if let Some(v) = p.panel {
                    next.controls.panel = v;
                }
                for (current, update) in next.controls.tools.iter_mut().zip(p.tools) {
                    if let Some(v) = update {
                        *current = v;
                    }
                }
                if let Some(widgets) = p.widgets {
                    next.widgets = widgets;
                }
            }
            SettingsPatch::Media(p) => {
                if let Some(v) = p.selection {
                    next.selection = v;
                }
            }
            SettingsPatch::Weather(p) => {
                if let Some(v) = p.city {
                    next.city = Some(v);
                }
            }
            SettingsPatch::WindowPlacement(placement) => {
                if let Some(placement) = &placement {
                    placement.validate()?;
                }
                next.window_placement = placement;
            }
        }
        next.validate()?;
        let changed = *self != next;
        if changed {
            *self = next;
        }
        Ok(changed)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaveRequest {
    pub revision: Revision,
    pub snapshot: SettingsSnapshot,
}

/// Main-thread-owned revision and debounce state for settings persistence.
pub struct RuntimeSettings {
    runtime: SettingsSnapshot,
    persisted: SettingsSnapshot,
    pending: Option<SaveRequest>,
    saving_revision: Option<Revision>,
    runtime_revision: Revision,
    persisted_revision: Revision,
    deadline: Option<Duration>,
    last_error: Option<String>,
}

impl RuntimeSettings {
    pub fn new(initial: SettingsSnapshot) -> Self {
        Self {
            runtime: initial.clone(),
            persisted: initial,
            pending: None,
            saving_revision: None,
            runtime_revision: 0,
            persisted_revision: 0,
            deadline: None,
            last_error: None,
        }
    }

    pub fn runtime(&self) -> &SettingsSnapshot {
        &self.runtime
    }
    pub fn persisted(&self) -> &SettingsSnapshot {
        &self.persisted
    }
    pub fn pending(&self) -> Option<&SaveRequest> {
        self.pending.as_ref()
    }
    pub fn runtime_revision(&self) -> Revision {
        self.runtime_revision
    }
    pub fn persisted_revision(&self) -> Revision {
        self.persisted_revision
    }
    pub fn saving_revision(&self) -> Option<Revision> {
        self.saving_revision
    }
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
    pub fn is_dirty(&self) -> bool {
        self.runtime != self.persisted || self.pending.is_some() || self.saving_revision.is_some()
    }

    pub fn apply_patch(
        &mut self,
        patch: SettingsPatch,
        now: Duration,
        debounce: Duration,
    ) -> Result<Revision, String> {
        if !self.runtime.apply_patch(patch)? {
            return Ok(self.runtime_revision);
        }
        self.schedule_latest(now, debounce);
        Ok(self.runtime_revision)
    }

    /// Replace the runtime snapshot after a typed compatibility edit.
    pub fn replace_runtime(
        &mut self,
        snapshot: SettingsSnapshot,
        now: Duration,
        debounce: Duration,
    ) -> Result<Revision, String> {
        snapshot.validate()?;
        if self.runtime == snapshot {
            return Ok(self.runtime_revision);
        }
        self.runtime = snapshot;
        self.schedule_latest(now, debounce);
        Ok(self.runtime_revision)
    }

    fn schedule_latest(&mut self, now: Duration, debounce: Duration) {
        self.runtime_revision = self.runtime_revision.saturating_add(1);
        self.pending = Some(SaveRequest {
            revision: self.runtime_revision,
            snapshot: self.runtime.clone(),
        });
        self.deadline = Some(now.saturating_add(debounce));
        self.last_error = None;
    }

    /// Queue the current snapshot even when its values already match Runtime.
    /// This supports an explicit Apply/Save action and retrying a baseline.
    pub fn queue_current(&mut self, now: Duration, debounce: Duration) -> Revision {
        self.runtime_revision = self.runtime_revision.saturating_add(1);
        self.pending = Some(SaveRequest {
            revision: self.runtime_revision,
            snapshot: self.runtime.clone(),
        });
        self.deadline = Some(now.saturating_add(debounce));
        self.last_error = None;
        self.runtime_revision
    }

    pub fn next_save_delay(&self, now: Duration) -> Option<Duration> {
        if self.last_error.is_some() || self.pending.is_none() || self.saving_revision.is_some() {
            return None;
        }
        self.deadline.map(|deadline| deadline.saturating_sub(now))
    }

    pub fn force_flush(&mut self, now: Duration) {
        if self.pending.is_some() && self.last_error.is_none() {
            self.deadline = Some(now);
        }
    }

    pub fn begin_save(&mut self, now: Duration) -> Option<SaveRequest> {
        if self.saving_revision.is_some()
            || self.last_error.is_some()
            || self.deadline.is_none_or(|d| d > now)
        {
            return None;
        }
        let request = self.pending.clone()?;
        self.saving_revision = Some(request.revision);
        Some(request)
    }

    pub fn cancel_begin_save(&mut self, revision: Revision) {
        if self.saving_revision == Some(revision) {
            self.saving_revision = None;
        }
    }

    pub fn finish_save(&mut self, request: &SaveRequest, result: Result<(), String>) {
        if self.saving_revision == Some(request.revision) {
            self.saving_revision = None;
        }
        match result {
            Ok(()) => {
                if request.revision >= self.persisted_revision {
                    self.persisted = request.snapshot.clone();
                    self.persisted_revision = request.revision;
                }
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.revision <= request.revision)
                {
                    self.pending = None;
                    self.deadline = None;
                }
                self.last_error = None;
            }
            Err(error) => {
                self.last_error = Some(error);
                self.deadline = None;
            }
        }
    }

    pub fn retry(&mut self, now: Duration) -> bool {
        let had_error = self.last_error.take().is_some();
        if self.pending.is_none() && self.runtime != self.persisted {
            self.pending = Some(SaveRequest {
                revision: self.runtime_revision,
                snapshot: self.runtime.clone(),
            });
        }
        if self.pending.is_some() {
            self.deadline = Some(now);
            true
        } else {
            had_error
        }
    }

    /// Restore the last persisted values. If a write is already in flight, queue
    /// the restored snapshot behind it so its eventual completion cannot win.
    pub fn revert(&mut self, now: Duration) -> bool {
        let had_changes = self.is_dirty() || self.last_error.is_some();
        if !had_changes {
            return false;
        }
        self.last_error = None;
        self.runtime = self.persisted.clone();
        self.runtime_revision = self.runtime_revision.saturating_add(1);
        if self.saving_revision.is_some() {
            self.pending = Some(SaveRequest {
                revision: self.runtime_revision,
                snapshot: self.runtime.clone(),
            });
            self.deadline = Some(now);
        } else {
            self.pending = None;
            self.deadline = None;
            self.persisted_revision = self.runtime_revision;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(x: i32) -> SettingsWindowPlacement {
        SettingsWindowPlacement {
            x,
            y: 120,
            width_dip: 820,
            height_dip: 760,
            dpi: 144,
        }
    }

    #[test]
    fn window_placement_patch_validates_and_participates_in_revert() {
        let mut state = RuntimeSettings::new(SettingsSnapshot::default());
        let first = placement(-1920);
        let revision = state
            .apply_patch(
                SettingsPatch::WindowPlacement(Some(first.clone())),
                Duration::ZERO,
                Duration::from_millis(200),
            )
            .unwrap();
        assert_eq!(state.runtime().window_placement, Some(first.clone()));
        assert_eq!(state.pending().unwrap().revision, revision);

        let same = state
            .apply_patch(
                SettingsPatch::WindowPlacement(Some(first)),
                Duration::from_millis(50),
                Duration::from_millis(200),
            )
            .unwrap();
        assert_eq!(same, revision);
        assert_eq!(
            state.next_save_delay(Duration::from_millis(50)),
            Some(Duration::from_millis(150))
        );

        assert!(state.revert(Duration::from_millis(50)));
        assert_eq!(state.runtime().window_placement, None);
        assert!(!state.is_dirty());
    }

    #[test]
    fn invalid_window_placement_does_not_advance_runtime_or_revision() {
        let mut state = RuntimeSettings::new(SettingsSnapshot::default());
        for invalid in [
            SettingsWindowPlacement {
                width_dip: 0,
                ..placement(0)
            },
            SettingsWindowPlacement {
                height_dip: u32::MAX,
                ..placement(0)
            },
            SettingsWindowPlacement {
                dpi: 0,
                ..placement(0)
            },
            SettingsWindowPlacement {
                dpi: 1200,
                ..placement(0)
            },
        ] {
            assert!(state
                .apply_patch(
                    SettingsPatch::WindowPlacement(Some(invalid)),
                    Duration::ZERO,
                    Duration::ZERO,
                )
                .is_err());
            assert_eq!(state.runtime_revision(), 0);
            assert_eq!(state.runtime().window_placement, None);
        }
    }

    #[test]
    fn placement_size_uses_target_dpi_with_checked_physical_bounds() {
        let saved = placement(i32::MIN);
        assert_eq!(saved.size_px_for_dpi(96).unwrap(), (820, 760));
        assert_eq!(saved.size_px_for_dpi(192).unwrap(), (1640, 1520));
        assert!(saved.size_px_for_dpi(u32::MAX).is_err());
    }

    #[test]
    fn patches_apply_immediately_and_debounce_latest_snapshot() {
        let mut state = RuntimeSettings::new(SettingsSnapshot::default());
        let delay = Duration::from_millis(350);
        let t0 = Duration::ZERO;
        state
            .apply_patch(
                SettingsPatch::Appearance(AppearancePatch {
                    compact_length: Some(120),
                    ..Default::default()
                }),
                t0,
                delay,
            )
            .unwrap();
        assert_eq!(state.runtime().appearance.compact_length, 120);
        assert!(state.begin_save(Duration::from_millis(349)).is_none());
        state
            .apply_patch(
                SettingsPatch::Appearance(AppearancePatch {
                    compact_length: Some(144),
                    ..Default::default()
                }),
                Duration::from_millis(100),
                delay,
            )
            .unwrap();
        assert!(state.begin_save(Duration::from_millis(449)).is_none());
        let request = state.begin_save(Duration::from_millis(450)).unwrap();
        assert_eq!(request.snapshot.appearance.compact_length, 144);
        state.finish_save(&request, Ok(()));
        assert_eq!(state.persisted().appearance.compact_length, 144);
        assert!(!state.is_dirty());
    }

    #[test]
    fn newer_patch_survives_an_older_in_flight_save() {
        let mut state = RuntimeSettings::new(SettingsSnapshot::default());
        let first = state
            .apply_patch(
                SettingsPatch::Appearance(AppearancePatch {
                    compact_length: Some(120),
                    ..Default::default()
                }),
                Duration::ZERO,
                Duration::ZERO,
            )
            .unwrap();
        let old = state.begin_save(Duration::ZERO).unwrap();
        assert_eq!(old.revision, first);
        assert_eq!(state.next_save_delay(Duration::from_secs(5)), None);
        state
            .apply_patch(
                SettingsPatch::Appearance(AppearancePatch {
                    compact_length: Some(180),
                    ..Default::default()
                }),
                Duration::from_millis(10),
                Duration::from_millis(300),
            )
            .unwrap();
        assert_eq!(state.runtime().appearance.compact_length, 180);
        assert_eq!(state.next_save_delay(Duration::from_millis(100)), None);
        state.finish_save(&old, Ok(()));
        assert_eq!(state.persisted().appearance.compact_length, 120);
        assert_eq!(
            state.next_save_delay(Duration::from_millis(100)),
            Some(Duration::from_millis(210))
        );
        let latest = state.begin_save(Duration::from_millis(310)).unwrap();
        assert_eq!(latest.snapshot.appearance.compact_length, 180);
    }

    #[test]
    fn failed_save_waits_for_retry_and_revert_restores_persisted_values() {
        let mut state = RuntimeSettings::new(SettingsSnapshot::default());
        state
            .apply_patch(
                SettingsPatch::Appearance(AppearancePatch {
                    compact_length: Some(130),
                    ..Default::default()
                }),
                Duration::ZERO,
                Duration::ZERO,
            )
            .unwrap();
        let request = state.begin_save(Duration::ZERO).unwrap();
        state.finish_save(&request, Err("stale file".into()));
        assert_eq!(state.last_error(), Some("stale file"));
        assert_eq!(state.next_save_delay(Duration::from_secs(5)), None);
        assert!(state.retry(Duration::from_secs(5)));
        assert_eq!(
            state.next_save_delay(Duration::from_secs(5)),
            Some(Duration::ZERO)
        );
        assert!(state.revert(Duration::from_secs(5)));
        assert_eq!(
            state.runtime().appearance.compact_length,
            Appearance::default().compact_length
        );
        assert!(!state.is_dirty());
    }

    #[test]
    fn invalid_typed_patch_does_not_change_runtime_or_revision() {
        let mut state = RuntimeSettings::new(SettingsSnapshot::default());
        let result = state.apply_patch(
            SettingsPatch::Appearance(AppearancePatch {
                compact_length: Some(40),
                ..Default::default()
            }),
            Duration::ZERO,
            Duration::ZERO,
        );
        assert!(result.is_err());
        assert_eq!(state.runtime_revision(), 0);
        assert_eq!(
            state.runtime().appearance.compact_length,
            Appearance::default().compact_length
        );
    }

    #[test]
    fn identical_patch_does_not_advance_revision_or_debounce_again() {
        let mut state = RuntimeSettings::new(SettingsSnapshot::default());
        let debounce = Duration::from_millis(350);
        let patch = SettingsPatch::Appearance(AppearancePatch {
            compact_length: Some(120),
            ..Default::default()
        });
        let first = state
            .apply_patch(patch.clone(), Duration::ZERO, debounce)
            .unwrap();
        let same = state
            .apply_patch(patch, Duration::from_millis(200), debounce)
            .unwrap();
        assert_eq!(same, first);
        assert_eq!(
            state.next_save_delay(Duration::from_millis(200)),
            Some(Duration::from_millis(150))
        );
    }
}
