use super::model;
use windows::{
    core::{w, Result, HSTRING},
    Win32::{
        Foundation::RECT,
        Graphics::{
            Direct2D::Common::{
                D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F,
            },
            Direct2D::*,
            DirectWrite::*,
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Gdi::{
                GetSysColor, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW,
                COLOR_WINDOWTEXT,
            },
        },
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            WindowsAndMessaging::{SystemParametersInfoW, SPI_GETHIGHCONTRAST},
        },
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ControlPaintDiagnostics {
    pub button_paints: u64,
    pub nav_button_paints: u64,
    pub toggle_paints: u64,
    pub segment_paints: u64,
    pub slider_paints: u64,
    pub dropdown_paints: u64,
    pub dropdown_chrome_paints: u64,
    pub target_recreates: u64,
    pub native_fallbacks: u64,
    pub combo_recoveries: u64,
    pub high_contrast: bool,
    pub generation: u64,
    pub renderer_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ControlRole {
    Button,
    Navigation,
    Toggle,
    Segmented,
    Slider,
    Dropdown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ControlState {
    pub checked: bool,
    pub indeterminate: bool,
    pub hot: bool,
    pub pressed: bool,
    pub focused: bool,
    pub disabled: bool,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PaintFailure {
    Bind,
    Draw,
    RendererDisabled,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TargetRecoveryPolicy {
    recreation_already_attempted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetRecoveryDecision {
    Recreate,
    Disable,
}

impl TargetRecoveryPolicy {
    fn target_lost(&mut self) -> TargetRecoveryDecision {
        if self.recreation_already_attempted {
            TargetRecoveryDecision::Disable
        } else {
            self.recreation_already_attempted = true;
            TargetRecoveryDecision::Recreate
        }
    }

    fn draw_succeeded(&mut self) {
        self.recreation_already_attempted = false;
    }
}

pub(super) struct ControlPainter {
    factory: ID2D1Factory,
    target: ID2D1DCRenderTarget,
    background: ID2D1SolidColorBrush,
    surface: ID2D1SolidColorBrush,
    selected: ID2D1SolidColorBrush,
    text: ID2D1SolidColorBrush,
    muted: ID2D1SolidColorBrush,
    border: ID2D1SolidColorBrush,
    format: IDWriteTextFormat,
    scale: f32,
    enabled: bool,
    synthetic_high_contrast: Option<bool>,
    real_high_contrast: bool,
    fault_target: Option<ControlRole>,
    fail_next_bind: bool,
    fail_next_end_draw: bool,
    recreate_next_target: bool,
    recovery: TargetRecoveryPolicy,
    generation: u64,
}

fn color(rgb: (u8, u8, u8)) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: rgb.0 as f32 / 255.0,
        g: rgb.1 as f32 / 255.0,
        b: rgb.2 as f32 / 255.0,
        a: 1.0,
    }
}

fn system_color(raw: u32) -> (u8, u8, u8) {
    (raw as u8, (raw >> 8) as u8, (raw >> 16) as u8)
}

fn rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left,
        top,
        right,
        bottom,
    }
}

fn render_target_properties(scale: f32) -> D2D1_RENDER_TARGET_PROPERTIES {
    D2D1_RENDER_TARGET_PROPERTIES {
        r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_IGNORE,
        },
        dpiX: 96.0 * scale,
        dpiY: 96.0 * scale,
        usage: D2D1_RENDER_TARGET_USAGE_NONE,
        minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
    }
}

pub(super) fn high_contrast_enabled() -> bool {
    let mut info = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            info.cbSize,
            Some((&mut info as *mut HIGHCONTRASTW).cast()),
            Default::default(),
        )
        .is_ok()
            && info.dwFlags.0 & HCF_HIGHCONTRASTON.0 != 0
    }
}

impl ControlPainter {
    pub(super) unsafe fn new(
        write: &IDWriteFactory,
        fonts: Option<&IDWriteFontCollection>,
        family: &str,
        scale: f32,
    ) -> Result<Self> {
        let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let properties = render_target_properties(scale);
        let target = factory.CreateDCRenderTarget(&properties)?;
        let fonts = fonts.cloned();
        let format = write.CreateTextFormat(
            &HSTRING::from(family),
            fonts.as_ref(),
            DWRITE_FONT_WEIGHT_MEDIUM,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            14.0,
            w!("zh-CN"),
        )?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        let brushes = Self::create_brushes(&target)?;
        Ok(Self {
            factory,
            target,
            background: brushes.0,
            surface: brushes.1,
            selected: brushes.2,
            text: brushes.3,
            muted: brushes.4,
            border: brushes.5,
            format,
            scale,
            enabled: true,
            synthetic_high_contrast: None,
            real_high_contrast: high_contrast_enabled(),
            fault_target: None,
            fail_next_bind: false,
            fail_next_end_draw: false,
            recreate_next_target: false,
            recovery: TargetRecoveryPolicy::default(),
            generation: 1,
        })
    }

    unsafe fn create_brushes(
        target: &ID2D1DCRenderTarget,
    ) -> Result<(
        ID2D1SolidColorBrush,
        ID2D1SolidColorBrush,
        ID2D1SolidColorBrush,
        ID2D1SolidColorBrush,
        ID2D1SolidColorBrush,
        ID2D1SolidColorBrush,
    )> {
        Ok((
            target.CreateSolidColorBrush(&color(model::CARD), None)?,
            target.CreateSolidColorBrush(&color(model::CARD), None)?,
            target.CreateSolidColorBrush(&color((35, 77, 137)), None)?,
            target.CreateSolidColorBrush(&color(model::PRIMARY_TEXT), None)?,
            target.CreateSolidColorBrush(&color(model::SECONDARY_TEXT), None)?,
            target.CreateSolidColorBrush(&color(model::CARD_HOVER), None)?,
        ))
    }

    pub(super) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn high_contrast(&self) -> bool {
        self.synthetic_high_contrast
            .unwrap_or(self.real_high_contrast)
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) unsafe fn set_dpi(&mut self, scale: f32) {
        self.scale = scale.max(0.5);
        self.target.SetDpi(96.0 * self.scale, 96.0 * self.scale);
    }

    pub(super) fn refresh_high_contrast(&mut self) {
        self.real_high_contrast = high_contrast_enabled();
    }

    pub(super) fn inject_fault(&mut self, kind: u32) -> bool {
        match kind {
            1 => {
                self.fail_next_bind = true;
                self.fault_target = Some(ControlRole::Dropdown);
            }
            2 => {
                self.fail_next_end_draw = true;
                self.fault_target = Some(ControlRole::Dropdown);
            }
            3 => {
                self.recreate_next_target = true;
                self.fault_target = Some(ControlRole::Dropdown);
            }
            4 => self.enabled = false,
            5 => self.synthetic_high_contrast = Some(true),
            6 => self.synthetic_high_contrast = None,
            _ => return false,
        }
        true
    }

    unsafe fn brush_palette(&mut self) {
        self.background.SetOpacity(1.0);
        self.surface.SetOpacity(1.0);
        self.selected.SetOpacity(1.0);
        self.text.SetOpacity(1.0);
        self.muted.SetOpacity(1.0);
        self.border.SetOpacity(1.0);
        if self.high_contrast() {
            self.background
                .SetColor(&color(system_color(GetSysColor(COLOR_WINDOW))));
            self.surface
                .SetColor(&color(system_color(GetSysColor(COLOR_WINDOW))));
            self.selected
                .SetColor(&color(system_color(GetSysColor(COLOR_HIGHLIGHT))));
            self.text
                .SetColor(&color(system_color(GetSysColor(COLOR_WINDOWTEXT))));
            self.muted
                .SetColor(&color(system_color(GetSysColor(COLOR_GRAYTEXT))));
            self.border
                .SetColor(&color(system_color(GetSysColor(COLOR_HIGHLIGHT))));
        } else {
            self.background.SetColor(&color(model::CARD));
            self.surface.SetColor(&color(model::CARD));
            self.selected.SetColor(&color((35, 77, 137)));
            self.text.SetColor(&color(model::PRIMARY_TEXT));
            self.muted.SetColor(&color(model::SECONDARY_TEXT));
            self.border.SetColor(&color(model::CARD_HOVER));
        }
    }

    unsafe fn bind_impl(
        &mut self,
        dc: windows::Win32::Graphics::Gdi::HDC,
        bounds: RECT,
        role: ControlRole,
        clear: bool,
    ) -> std::result::Result<(), PaintFailure> {
        if !self.enabled {
            return Err(PaintFailure::RendererDisabled);
        }
        if self.fail_next_bind && self.fault_target == Some(role) {
            self.fail_next_bind = false;
            self.fault_target = None;
            return Err(PaintFailure::Bind);
        }
        self.target
            .BindDC(dc, &bounds)
            .map_err(|_| PaintFailure::Bind)?;
        self.brush_palette();
        self.target.BeginDraw();
        if clear {
            let clear_color = if self.high_contrast() {
                color(system_color(GetSysColor(COLOR_WINDOW)))
            } else if role == ControlRole::Navigation {
                color(model::SURFACE)
            } else {
                color(model::CARD)
            };
            self.target.Clear(Some(&clear_color));
        }
        Ok(())
    }

    unsafe fn bind(
        &mut self,
        dc: windows::Win32::Graphics::Gdi::HDC,
        bounds: RECT,
        role: ControlRole,
    ) -> std::result::Result<(), PaintFailure> {
        self.bind_impl(dc, bounds, role, true)
    }

    unsafe fn finish(&mut self, role: ControlRole) -> std::result::Result<(), PaintFailure> {
        let result = self.target.EndDraw(None, None);
        if self.recreate_next_target && self.fault_target == Some(role) {
            self.recreate_next_target = false;
            self.fault_target = None;
            self.recover_lost_target()?;
            return Err(PaintFailure::Draw);
        }
        if self.fail_next_end_draw && self.fault_target == Some(role) {
            self.fail_next_end_draw = false;
            self.fault_target = None;
            return Err(PaintFailure::Draw);
        }
        match result {
            Ok(()) => {
                self.recovery.draw_succeeded();
                Ok(())
            }
            Err(error) => {
                if error.code() == windows::Win32::Foundation::D2DERR_RECREATE_TARGET {
                    self.recover_lost_target()?;
                }
                Err(PaintFailure::Draw)
            }
        }
    }

    unsafe fn recover_lost_target(&mut self) -> std::result::Result<(), PaintFailure> {
        match self.recovery.target_lost() {
            TargetRecoveryDecision::Recreate => self.recreate_target(),
            TargetRecoveryDecision::Disable => {
                self.enabled = false;
                Err(PaintFailure::Draw)
            }
        }
    }

    unsafe fn recreate_target(&mut self) -> std::result::Result<(), PaintFailure> {
        let target = match self
            .factory
            .CreateDCRenderTarget(&render_target_properties(self.scale))
        {
            Ok(target) => target,
            Err(_) => {
                self.enabled = false;
                return Err(PaintFailure::Draw);
            }
        };
        let brushes = match Self::create_brushes(&target) {
            Ok(brushes) => brushes,
            Err(_) => {
                self.enabled = false;
                return Err(PaintFailure::Draw);
            }
        };
        self.target = target;
        self.background = brushes.0;
        self.surface = brushes.1;
        self.selected = brushes.2;
        self.text = brushes.3;
        self.muted = brushes.4;
        self.border = brushes.5;
        self.generation = self.generation.saturating_add(1);
        Ok(())
    }

    unsafe fn draw_text(&self, value: &str, bounds: D2D_RECT_F, brush: &ID2D1SolidColorBrush) {
        let wide = value.encode_utf16().collect::<Vec<_>>();
        self.target.DrawText(
            &wide,
            &self.format,
            &bounds,
            brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    pub(super) unsafe fn draw_button(
        &mut self,
        dc: windows::Win32::Graphics::Gdi::HDC,
        bounds: RECT,
        label: &str,
        role: ControlRole,
        state: ControlState,
    ) -> std::result::Result<(), PaintFailure> {
        self.bind(dc, bounds, role)?;
        let width = (bounds.right - bounds.left).max(1) as f32 / self.scale;
        let height = (bounds.bottom - bounds.top).max(1) as f32 / self.scale;
        let emphasized = match role {
            ControlRole::Navigation => state.active,
            ControlRole::Segmented => state.checked,
            ControlRole::Toggle | ControlRole::Slider | ControlRole::Dropdown => false,
            ControlRole::Button => state.pressed,
        };
        let background = if !state.disabled && emphasized {
            &self.selected
        } else if self.high_contrast() && (state.hot || state.focused) {
            &self.surface
        } else if state.hot || state.focused {
            &self.border
        } else {
            &self.surface
        };
        if self.high_contrast() && (emphasized || state.pressed) {
            self.text
                .SetColor(&color(system_color(GetSysColor(COLOR_HIGHLIGHTTEXT))));
        }
        let radius = match role {
            ControlRole::Navigation | ControlRole::Segmented => 9.0,
            ControlRole::Toggle => 8.0,
            _ => 8.0,
        };
        self.target.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: rect(0.0, 0.0, width, height),
                radiusX: radius,
                radiusY: radius,
            },
            background,
        );
        if state.disabled {
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect(0.5, 0.5, (width - 0.5).max(0.5), (height - 0.5).max(0.5)),
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.muted,
                1.0,
                None::<&ID2D1StrokeStyle>,
            );
        }
        match role {
            ControlRole::Toggle => {
                self.draw_text(
                    label,
                    rect(8.0, 0.0, (width - 54.0).max(16.0), height),
                    if state.disabled {
                        &self.muted
                    } else {
                        &self.text
                    },
                );
                let track_w = 36.0f32.min((width * 0.36).max(24.0));
                let track_h = 20.0f32.min((height - 4.0).max(14.0));
                let left = (width - track_w - 8.0).max(4.0);
                let top = (height - track_h) * 0.5;
                let track = if !state.disabled && (state.checked || state.indeterminate) {
                    &self.selected
                } else {
                    &self.border
                };
                self.target.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(left, top, left + track_w, top + track_h),
                        radiusX: track_h * 0.5,
                        radiusY: track_h * 0.5,
                    },
                    track,
                );
                let knob_r = (track_h - 4.0).max(6.0) * 0.5;
                let knob_x = if state.indeterminate {
                    left + track_w * 0.5
                } else if state.checked {
                    left + track_w - knob_r - 2.0
                } else {
                    left + knob_r + 2.0
                };
                self.target.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                            x: knob_x,
                            y: height * 0.5,
                        },
                        radiusX: knob_r,
                        radiusY: knob_r,
                    },
                    &self.text,
                );
                if state.indeterminate {
                    self.target.FillRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: rect(
                                knob_x - 4.0,
                                height * 0.5 - 1.2,
                                knob_x + 4.0,
                                height * 0.5 + 1.2,
                            ),
                            radiusX: 1.2,
                            radiusY: 1.2,
                        },
                        &self.selected,
                    );
                }
            }
            ControlRole::Segmented => {
                let selected = if state.checked {
                    &self.text
                } else {
                    &self.muted
                };
                self.draw_text(label, rect(5.0, 0.0, width - 5.0, height), selected);
            }
            _ => {
                let inset = if role == ControlRole::Navigation {
                    14.0
                } else {
                    8.0
                };
                self.draw_text(
                    label,
                    rect(inset, 0.0, (width - inset - 8.0).max(inset), height),
                    if state.disabled {
                        &self.muted
                    } else {
                        &self.text
                    },
                );
            }
        }
        if state.focused {
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect(2.0, 2.0, (width - 2.0).max(2.0), (height - 2.0).max(2.0)),
                    radiusX: (radius - 2.0).max(2.0),
                    radiusY: (radius - 2.0).max(2.0),
                },
                if self.high_contrast() {
                    &self.border
                } else {
                    &self.text
                },
                1.2,
                None::<&ID2D1StrokeStyle>,
            );
        }
        self.finish(role)
    }

    pub(super) unsafe fn draw_slider(
        &mut self,
        dc: windows::Win32::Graphics::Gdi::HDC,
        bounds: RECT,
        channel: RECT,
        thumb: RECT,
        ticks: &[i32],
        state: ControlState,
    ) -> std::result::Result<(), PaintFailure> {
        self.bind(dc, bounds, ControlRole::Slider)?;
        let scale = self.scale;
        let track_left = channel.left as f32 / scale;
        let track_right = (channel.right.max(channel.left + 1)) as f32 / scale;
        let center_y = ((channel.top + channel.bottom) as f32 * 0.5) / scale;
        let thumb_x = ((thumb.left + thumb.right) as f32 * 0.5) / scale;
        let thumb_y = ((thumb.top + thumb.bottom) as f32 * 0.5) / scale;
        let thumb_radius_x = ((thumb.right - thumb.left).max(1) as f32 / scale * 0.5).max(5.0);
        let thumb_radius_y = ((thumb.bottom - thumb.top).max(1) as f32 / scale * 0.5).max(5.0);
        let track_color = if state.disabled {
            &self.border
        } else {
            &self.selected
        };
        self.target.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: rect(track_left, center_y - 2.5, track_right, center_y + 2.5),
                radiusX: 2.5,
                radiusY: 2.5,
            },
            &self.border,
        );
        self.target.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: rect(
                    track_left,
                    center_y - 2.5,
                    thumb_x.max(track_left + 1.0),
                    center_y + 2.5,
                ),
                radiusX: 2.5,
                radiusY: 2.5,
            },
            track_color,
        );
        let knob = if state.disabled {
            &self.muted
        } else {
            &self.selected
        };
        self.target.FillEllipse(
            &D2D1_ELLIPSE {
                point: windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                    x: thumb_x,
                    y: thumb_y,
                },
                radiusX: thumb_radius_x,
                radiusY: thumb_radius_y,
            },
            knob,
        );
        let tick_y =
            (channel.bottom as f32 / scale + 3.0).min((bounds.bottom - bounds.top) as f32 / scale);
        for tick in ticks.iter().copied() {
            let x = tick as f32 / scale;
            self.target.DrawLine(
                windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F { x, y: tick_y },
                windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                    x,
                    y: (tick_y + 5.0).min((bounds.bottom - bounds.top) as f32 / scale),
                },
                &self.muted,
                1.0,
                None::<&ID2D1StrokeStyle>,
            );
        }
        if state.focused || state.hot {
            self.target.DrawEllipse(
                &D2D1_ELLIPSE {
                    point: windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                        x: thumb_x,
                        y: thumb_y,
                    },
                    radiusX: thumb_radius_x + 3.0,
                    radiusY: thumb_radius_y + 3.0,
                },
                &self.text,
                1.2,
                None::<&ID2D1StrokeStyle>,
            );
        }
        self.finish(ControlRole::Slider)
    }

    pub(super) unsafe fn draw_dropdown(
        &mut self,
        dc: windows::Win32::Graphics::Gdi::HDC,
        bounds: RECT,
        label: &str,
        state: ControlState,
        selection_field: bool,
    ) -> std::result::Result<(), PaintFailure> {
        self.bind(dc, bounds, ControlRole::Dropdown)?;
        let width = (bounds.right - bounds.left).max(1) as f32 / self.scale;
        let height = (bounds.bottom - bounds.top).max(1) as f32 / self.scale;
        let selected_state = !state.disabled && (state.pressed || state.active);
        let fill = if selected_state {
            &self.selected
        } else if (state.hot || state.focused) && !self.high_contrast() {
            &self.border
        } else {
            &self.surface
        };
        if self.high_contrast() && selected_state {
            self.text
                .SetColor(&color(system_color(GetSysColor(COLOR_HIGHLIGHTTEXT))));
        }
        self.target
            .FillRectangle(&rect(0.0, 0.0, width, height), fill);
        if self.high_contrast() && state.active {
            self.text
                .SetColor(&color(system_color(GetSysColor(COLOR_HIGHLIGHTTEXT))));
        }
        self.draw_text(
            label,
            rect(
                10.0,
                0.0,
                (width - if selection_field { 34.0 } else { 10.0 }).max(10.0),
                height,
            ),
            if state.disabled {
                &self.muted
            } else {
                &self.text
            },
        );
        self.finish(ControlRole::Dropdown)
    }

    /// Repaint the closed ComboBox after ComCtl32's default procedure. The
    /// real rcItem and rcButton bounds define where the string and arrow go;
    /// the HWND, list, and input/automation behavior remain native.
    pub(super) unsafe fn draw_dropdown_control(
        &mut self,
        dc: windows::Win32::Graphics::Gdi::HDC,
        bounds: RECT,
        item_bounds: RECT,
        button_bounds: RECT,
        label: &str,
        state: ControlState,
    ) -> std::result::Result<(), PaintFailure> {
        self.bind(dc, bounds, ControlRole::Dropdown)?;
        let width = (bounds.right - bounds.left).max(1) as f32 / self.scale;
        let height = (bounds.bottom - bounds.top).max(1) as f32 / self.scale;
        let to_dip = |value: i32| value as f32 / self.scale;
        let item = rect(
            to_dip(item_bounds.left - bounds.left),
            to_dip(item_bounds.top - bounds.top),
            to_dip(item_bounds.right - bounds.left),
            to_dip(item_bounds.bottom - bounds.top),
        );
        let button = rect(
            to_dip(button_bounds.left - bounds.left),
            to_dip(button_bounds.top - bounds.top),
            to_dip(button_bounds.right - bounds.left),
            to_dip(button_bounds.bottom - bounds.top),
        );
        let selected_state = !state.disabled && (state.pressed || state.active);
        let fill = if state.disabled {
            &self.surface
        } else if selected_state {
            &self.selected
        } else if (state.hot || state.focused) && !self.high_contrast() {
            &self.border
        } else {
            &self.surface
        };
        if self.high_contrast() && selected_state {
            self.text
                .SetColor(&color(system_color(GetSysColor(COLOR_HIGHLIGHTTEXT))));
        }
        self.target
            .FillRectangle(&rect(0.0, 0.0, width, height), &self.surface);
        self.target.FillRectangle(&item, fill);
        self.draw_text(
            label,
            rect(
                item.left + 8.0,
                item.top,
                (item.right - 8.0).max(item.left + 8.0),
                item.bottom,
            ),
            if state.disabled {
                &self.muted
            } else {
                &self.text
            },
        );
        self.target.FillRectangle(&button, fill);
        if button.left > item.left {
            self.target.DrawLine(
                windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                    x: button.left,
                    y: (button.top + 4.0).min(button.bottom),
                },
                windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                    x: button.left,
                    y: (button.bottom - 4.0).max(button.top),
                },
                &self.border,
                1.0,
                None::<&ID2D1StrokeStyle>,
            );
        }
        self.target.DrawRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: rect(0.5, 0.5, (width - 0.5).max(0.5), (height - 0.5).max(0.5)),
                radiusX: 4.0,
                radiusY: 4.0,
            },
            &self.border,
            if state.focused { 1.5 } else { 1.0 },
            None::<&ID2D1StrokeStyle>,
        );
        let center_x = (button.left + button.right) * 0.5;
        let center_y = (button.top + button.bottom) * 0.5;
        let arrow_color = if state.disabled {
            &self.muted
        } else {
            &self.text
        };
        self.target.DrawLine(
            windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                x: center_x - 4.0,
                y: center_y - 2.0,
            },
            windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                x: center_x,
                y: center_y + 2.0,
            },
            arrow_color,
            1.5,
            None::<&ID2D1StrokeStyle>,
        );
        self.target.DrawLine(
            windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                x: center_x,
                y: center_y + 2.0,
            },
            windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                x: center_x + 4.0,
                y: center_y - 2.0,
            },
            arrow_color,
            1.5,
            None::<&ID2D1StrokeStyle>,
        );
        self.finish(ControlRole::Dropdown)
    }
}

#[cfg(test)]
mod palette_tests {
    use super::*;

    fn relative_luminance(rgb: (u8, u8, u8)) -> f64 {
        let channel = |value: u8| {
            let value = f64::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(rgb.0) + 0.7152 * channel(rgb.1) + 0.0722 * channel(rgb.2)
    }

    fn contrast_ratio(foreground: (u8, u8, u8), background: (u8, u8, u8)) -> f64 {
        let a = relative_luminance(foreground);
        let b = relative_luminance(background);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn selected_button_palette_keeps_small_text_above_wcag_aa_contrast() {
        assert!(contrast_ratio(model::PRIMARY_TEXT, (35, 77, 137)) >= 4.5);
    }

    #[test]
    fn disabled_text_stays_legible_on_dark_surface_without_alpha_fade() {
        assert!(contrast_ratio(model::SECONDARY_TEXT, model::CARD) >= 4.5);
    }

    #[test]
    fn target_loss_recreates_once_then_disables_until_a_successful_draw() {
        let mut policy = TargetRecoveryPolicy::default();
        assert_eq!(policy.target_lost(), TargetRecoveryDecision::Recreate);
        assert_eq!(policy.target_lost(), TargetRecoveryDecision::Disable);
        policy.draw_succeeded();
        assert_eq!(policy.target_lost(), TargetRecoveryDecision::Recreate);
    }
}
