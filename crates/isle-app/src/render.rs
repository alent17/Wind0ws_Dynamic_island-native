use crate::icons::{Icon, Icons};
use isle_ui::{geometry::*, model::*};
#[path = "panels.rs"]
mod panels;
use std::collections::HashMap;
use windows::{
    core::*,
    Foundation::Numerics::Matrix3x2,
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            Direct3D::*,
            Direct3D11::*,
            DirectComposition::*,
            DirectWrite::*,
            Dxgi::{Common::*, *},
        },
    },
};
pub struct Renderer {
    digits: Vec<Option<isle_ui::rolling::Digit>>,
    digits_page: (u64, bool),
    number_used: bool,
    pub content_animating: bool,
    page_enter: Option<(u64, f64)>,
    ruler_motion: Option<isle_ui::rolling::Tween>,
    ruler_dragging: bool,
    icons: Icons,
    cover: Option<(std::sync::Arc<isle_core::Cover>, ID2D1Bitmap, [[f32; 3]; 2])>,
    disc_brush: Option<ID2D1BitmapBrush>,
    pub ctx: ID2D1DeviceContext,
    factory: ID2D1Factory1,
    write: IDWriteFactory,
    fonts: Option<IDWriteFontCollection>,
    pub font_family: &'static str,
    swap: IDXGISwapChain1,
    _composition: IDCompositionDevice,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    brush: ID2D1SolidColorBrush,
    formats: HashMap<(u32, i32), IDWriteTextFormat>,
    layouts: HashMap<String, (IDWriteTextLayout, f32)>,
    pub frames: u64,
    pub title_overflow: bool,
}
fn color(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}
fn point(x: f32, y: f32) -> D2D_POINT_2F {
    D2D_POINT_2F { x, y }
}
fn rect(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.x,
        top: r.y,
        right: r.x + r.w,
        bottom: r.y + r.h,
    }
}
fn write_media_time(milliseconds: u64, output: &mut [u16]) -> usize {
    let seconds = milliseconds / 1_000;
    let mut minutes = seconds / 60;
    let mut reversed = [0_u16; 20];
    let mut minute_count = 0;
    loop {
        reversed[minute_count] = u16::from(b'0') + (minutes % 10) as u16;
        minute_count += 1;
        minutes /= 10;
        if minutes == 0 {
            break;
        }
    }
    if output.len() < minute_count + 3 {
        return 0;
    }
    for index in 0..minute_count {
        output[index] = reversed[minute_count - index - 1];
    }
    output[minute_count] = u16::from(b':');
    output[minute_count + 1] = u16::from(b'0') + (seconds % 60 / 10) as u16;
    output[minute_count + 2] = u16::from(b'0') + (seconds % 10) as u16;
    minute_count + 3
}

#[cfg(test)]
fn format_media_time(milliseconds: u64) -> String {
    let mut output = [0_u16; 24];
    let length = write_media_time(milliseconds, &mut output);
    String::from_utf16(&output[..length]).unwrap()
}

fn rgb_to_hsl([r, g, b]: [f32; 3]) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = (max + min) / 2.;
    if max == min {
        return (0., 0., lightness);
    }
    let delta = max - min;
    let saturation = if lightness > 0.5 {
        delta / (2. - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if max == r {
        (g - b) / delta + if g < b { 6. } else { 0. }
    } else if max == g {
        (b - r) / delta + 2.
    } else {
        (r - g) / delta + 4.
    } * 60.;
    (hue, saturation, lightness)
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    if saturation == 0. {
        return [lightness; 3];
    }
    let hue = hue.rem_euclid(360.) / 360.;
    let q = if lightness < 0.5 {
        lightness * (1. + saturation)
    } else {
        lightness + saturation - lightness * saturation
    };
    let p = 2. * lightness - q;
    let hue_to_channel = |mut channel: f32| {
        if channel < 0. {
            channel += 1.;
        }
        if channel > 1. {
            channel -= 1.;
        }
        if channel < 1. / 6. {
            p + (q - p) * 6. * channel
        } else if channel < 0.5 {
            q
        } else if channel < 2. / 3. {
            p + (q - p) * (2. / 3. - channel) * 6.
        } else {
            p
        }
    };
    [
        hue_to_channel(hue + 1. / 3.),
        hue_to_channel(hue),
        hue_to_channel(hue - 1. / 3.),
    ]
}

fn cover_spectrum_palette(cover: &isle_core::Cover) -> [[f32; 3]; 2] {
    const CHANNEL_BINS: usize = 12;
    let mut bins = [0_f32; CHANNEL_BINS * CHANNEL_BINS * CHANNEL_BINS];
    let step_x = (cover.width / 24).max(1) as usize;
    let step_y = (cover.height / 24).max(1) as usize;
    let width = cover.width as usize;
    let height = cover.height as usize;
    for y in (0..height).step_by(step_y) {
        for x in (0..width).step_by(step_x) {
            let offset = (y * width + x) * 4;
            let Some(pixel) = cover.pixels.get(offset..offset + 4) else {
                continue;
            };
            let alpha = pixel[3] as u32;
            if alpha < 128 {
                continue;
            }
            let channel = |value: u8| {
                ((value as u32 * 255 / alpha).min(255) as f32 / 24.)
                    .round()
                    .mul_add(24., 0.)
                    .clamp(0., 255.)
                    / 24.
            };
            let blue = channel(pixel[0]) as usize;
            let green = channel(pixel[1]) as usize;
            let red = channel(pixel[2]) as usize;
            let bin = (red * CHANNEL_BINS + green) * CHANNEL_BINS + blue;
            let nx = (x as f32 + 0.5) / width.max(1) as f32 - 0.5;
            let ny = (y as f32 + 0.5) / height.max(1) as f32 - 0.5;
            let weight = 1. + (0.3 - nx.hypot(ny) * 0.42).max(0.);
            bins[bin] += weight;
        }
    }

    let candidate = |index: usize| {
        let bucket = [
            index / (CHANNEL_BINS * CHANNEL_BINS),
            (index / CHANNEL_BINS) % CHANNEL_BINS,
            index % CHANNEL_BINS,
        ];
        let rgb = bucket.map(|value| {
            if value == 10 {
                1.
            } else {
                value as f32 * 24. / 255.
            }
        });
        let (hue, saturation, lightness) = rgb_to_hsl(rgb);
        let midtone = (1. - (lightness - 0.5).abs() * 1.35).clamp(0.22, 1.);
        (
            rgb,
            hue,
            saturation,
            lightness,
            bins[index] * (0.32 + saturation * 1.18) * midtone,
        )
    };

    let mut primary = None;
    for (index, weight) in bins.iter().enumerate() {
        if *weight == 0. {
            continue;
        }
        let option = candidate(index);
        if primary.is_none_or(|best: ([f32; 3], f32, f32, f32, f32)| option.4 > best.4) {
            primary = Some(option);
        }
    }
    let Some(primary) = primary else {
        return [[0.53; 3], [0.9; 3]];
    };

    let hue_distance = |a: f32, b: f32| ((b - a + 540.).rem_euclid(360.) - 180.).abs() / 180.;
    let mut secondary: Option<([f32; 3], f32, f32, f32, f32)> = None;
    let mut secondary_score = 0.;
    for (index, weight) in bins.iter().enumerate() {
        if *weight == 0. {
            continue;
        }
        let option = candidate(index);
        if option.0 == primary.0 {
            continue;
        }
        let distance =
            hue_distance(primary.1, option.1) * 0.58 + (primary.3 - option.3).abs() * 0.42;
        let score = option.4 * (0.42 + distance);
        if secondary.is_none() || score > secondary_score {
            secondary = Some(option);
            secondary_score = score;
        }
    }

    let mut secondary = secondary.unwrap_or(primary);
    let separation =
        hue_distance(primary.1, secondary.1) * 0.55 + (primary.3 - secondary.3).abs() * 0.45;
    if separation < 0.1 {
        let companion_lightness = if primary.3 > 0.58 {
            primary.3 - 0.2
        } else {
            primary.3 + 0.2
        }
        .clamp(0.18, 0.82);
        secondary.0 = hsl_to_rgb(primary.1, primary.2, companion_lightness);
        secondary.3 = companion_lightness;
    }

    let (top, bottom) = if primary.3 >= secondary.3 {
        (primary, secondary)
    } else {
        (secondary, primary)
    };
    let has_color = top.2.max(bottom.2) >= 0.08;
    let hue = if top.2 >= bottom.2 { top.1 } else { bottom.1 };
    let top_hue = if top.2 >= 0.08 { top.1 } else { hue };
    let bottom_hue = if bottom.2 >= 0.08 { bottom.1 } else { hue };
    let minimum_saturation = if has_color {
        bottom.2.min(top.2).max(0.16)
    } else {
        0.
    };
    let lightness_gap = (top.3 - bottom.3).abs();
    let gap_boost = if lightness_gap < 0.18 { 0.1 } else { 0. };
    let readable_bottom =
        (bottom.3.min(top.3) - gap_boost * 0.35).clamp(if has_color { 0.3 } else { 0.48 }, 0.72);
    let readable_top =
        (bottom.3.max(top.3) + gap_boost).clamp(if has_color { 0.48 } else { 0.58 }, 0.9);
    let palette_color = |amount: f32| {
        let saturation = bottom.2 + (top.2 - bottom.2) * amount;
        let saturation = if has_color {
            (saturation + amount * 0.08)
                .min(0.92)
                .max(minimum_saturation)
        } else {
            0.
        };
        let color_hue = if has_color {
            let delta = (top_hue - bottom_hue + 540.).rem_euclid(360.) - 180.;
            bottom_hue + delta * amount
        } else {
            0.
        };
        hsl_to_rgb(
            color_hue,
            saturation,
            readable_bottom + (readable_top - readable_bottom) * amount,
        )
    };
    [palette_color(0.), palette_color(1.)]
}

impl Renderer {
    pub fn cover_alive(&self) -> bool {
        self.cover.is_some()
    }
    pub unsafe fn new(hwnd: HWND, scale: f32) -> Result<Self> {
        let mut device = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE(0),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )?;
        let device = device.ok_or_else(Error::from_win32)?;
        let dxgi: IDXGIDevice = device.cast()?;
        let adapter = dxgi.GetAdapter()?;
        let dxgi_factory: IDXGIFactory2 = adapter.GetParent()?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: (HOST * scale).round() as u32,
            Height: (HOST * scale).round() as u32,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            Scaling: DXGI_SCALING_STRETCH,
            ..Default::default()
        };
        let swap = dxgi_factory.CreateSwapChainForComposition(&device, &desc, None)?;
        let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let ctx = factory
            .CreateDevice(&dxgi)?
            .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
        let surface: IDXGISurface = swap.GetBuffer(0)?;
        let bitmap = ctx.CreateBitmapFromDxgiSurface(
            &surface,
            Some(&D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96. * scale,
                dpiY: 96. * scale,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                colorContext: std::mem::ManuallyDrop::new(None),
            }),
        )?;
        ctx.SetTarget(&bitmap);
        ctx.SetDpi(96. * scale, 96. * scale);
        ctx.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        let composition: IDCompositionDevice = DCompositionCreateDevice(&dxgi)?;
        let target = composition.CreateTargetForHwnd(hwnd, true)?;
        let visual = composition.CreateVisual()?;
        visual.SetContent(&swap)?;
        target.SetRoot(&visual)?;
        composition.Commit()?;
        let brush = ctx.CreateSolidColorBrush(&color(1., 1., 1., 1.), None)?;
        let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let fonts = load_fonts(&write);
        let font_family = if fonts.is_some() {
            "MiSans"
        } else {
            "Segoe UI"
        };
        let icons = Icons::new(&ctx)?;
        Ok(Self {
            icons,
            fonts,
            font_family,
            ctx,
            factory,
            write,
            swap,
            _composition: composition,
            _target: target,
            _visual: visual,
            brush,
            formats: HashMap::new(),
            cover: None,
            disc_brush: None,
            digits: Vec::new(),
            digits_page: (0, false),
            number_used: false,
            content_animating: false,
            page_enter: None,
            ruler_motion: None,
            ruler_dragging: false,
            layouts: HashMap::new(),
            frames: 0,
            title_overflow: false,
        })
    }
    unsafe fn ink(&self, c: D2D1_COLOR_F) {
        self.brush.SetColor(&c);
    }
    unsafe fn fill(&self, r: Rect, radius: f32, c: D2D1_COLOR_F) {
        self.ink(c);
        self.ctx.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: rect(r),
                radiusX: radius,
                radiusY: radius,
            },
            &self.brush,
        );
    }
    unsafe fn format_with_weight(
        &mut self,
        size: u32,
        weight: DWRITE_FONT_WEIGHT,
    ) -> Result<IDWriteTextFormat> {
        if let Some(f) = self.formats.get(&(size, weight.0)) {
            return Ok(f.clone());
        }
        let f = self.write.CreateTextFormat(
            &HSTRING::from(self.font_family),
            self.fonts.as_ref(),
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size as f32,
            w!("zh-CN"),
        )?;
        f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        self.formats.insert((size, weight.0), f.clone());
        Ok(f)
    }
    unsafe fn format(&mut self, size: u32) -> Result<IDWriteTextFormat> {
        let weight = if size >= 26 || size == 13 {
            DWRITE_FONT_WEIGHT_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        };
        self.format_with_weight(size, weight)
    }
    unsafe fn text_with_weight(
        &mut self,
        text: &str,
        r: Rect,
        size: u32,
        weight: DWRITE_FONT_WEIGHT,
        c: D2D1_COLOR_F,
    ) -> Result<()> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        self.text_utf16(&wide, r, size, weight, c)
    }
    unsafe fn text_utf16(
        &mut self,
        wide: &[u16],
        r: Rect,
        size: u32,
        weight: DWRITE_FONT_WEIGHT,
        c: D2D1_COLOR_F,
    ) -> Result<()> {
        let f = self.format_with_weight(size, weight)?;
        self.ink(c);
        self.ctx.DrawText(
            wide,
            &f,
            &rect(r),
            &self.brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
        Ok(())
    }
    unsafe fn glyph(&self, id: usize, r: Rect, c: D2D1_COLOR_F, size: f32) -> Result<()> {
        let icon = [
            Icon::Timer,
            Icon::Volume,
            Icon::Floating,
            Icon::Settings,
            Icon::Hide,
            Icon::Clock,
            Icon::CloudSun,
        ][id.min(6)];
        self.icons.draw(
            icon,
            r.x + (r.w - size) / 2.,
            r.y + (r.h - size) / 2.,
            size,
            1.8,
            c,
        )
    }
    pub unsafe fn draw(
        &mut self,
        m: &Model,
        hover: Option<Hit>,
        pressed: Option<Hit>,
        dragging: bool,
    ) -> Result<()> {
        if self.digits_page != (m.generation, m.expanded) {
            self.digits.clear();
            self.ruler_motion = None;
            self.digits_page = (m.generation, m.expanded);
        }
        self.number_used = false;
        self.ruler_dragging = dragging;
        self.content_animating = false;
        if !m.expanded {
            self.page_enter = None;
        }
        if !m.expanded || m.page() != Page::Music {
            self.layouts.clear();
        }
        let white = color(0.94, 0.96, 1., 1.);
        let blue = color(0.45, 0.76, 1., 1.);
        self.title_overflow = false;
        self.ctx.BeginDraw();
        self.ctx.SetTransform(&Matrix3x2 {
            M11: 1.,
            M12: 0.,
            M21: 0.,
            M22: 1.,
            M31: 0.,
            M32: 0.,
        });
        self.ctx.Clear(Some(&color(0., 0., 0., 0.)));
        let media_cover = if m.cover_visible() {
            m.media.as_ref().and_then(|media| media.cover.as_ref())
        } else {
            None
        };
        if let Some(cover) = media_cover {
            if self
                .cover
                .as_ref()
                .is_none_or(|(old, _, _)| !std::sync::Arc::ptr_eq(old, cover))
            {
                let bitmap = self.ctx.CreateBitmap(
                    D2D_SIZE_U {
                        width: cover.width,
                        height: cover.height,
                    },
                    Some(cover.pixels.as_ptr().cast()),
                    cover.width * 4,
                    &D2D1_BITMAP_PROPERTIES {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        dpiX: 96.,
                        dpiY: 96.,
                    },
                )?;
                self.disc_brush = Some(self.ctx.CreateBitmapBrush(&bitmap, None, None)?);
                self.cover = Some((cover.clone(), bitmap, cover_spectrum_palette(cover)));
            }
        } else {
            self.cover = None;
            self.disc_brush = None;
        }
        // The existing Isle island keeps a black surface. Cover color belongs
        // to the separate floating player, so artwork stays in the cover only.
        let background = [0., 0., 0.];
        let background_color = color(background[0], background[1], background[2], 1.);
        let p = m.outline();
        let shape = self.factory.CreatePathGeometry()?;
        let sink = shape.Open()?;
        sink.BeginFigure(point(p[0].x, p[0].y), D2D1_FIGURE_BEGIN_FILLED);
        let points: Vec<_> = p.iter().skip(1).map(|p| point(p.x, p.y)).collect();
        sink.AddLines(&points);
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
        self.ink(background_color);
        self.ctx.FillGeometry(&shape, &self.brush, None);
        // Clip every content frame to the same animated outline used for input.
        // A spring interrupted mid-flight must never expose rectangular page edges.
        let mut layer = D2D1_LAYER_PARAMETERS {
            contentBounds: rect(Rect {
                x: 0.,
                y: 0.,
                w: HOST,
                h: HOST,
            }),
            geometricMask: std::mem::ManuallyDrop::new(Some(shape.cast()?)),
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2 {
                M11: 1.,
                M22: 1.,
                ..Default::default()
            },
            opacity: 1.,
            ..Default::default()
        };
        self.ctx.PushLayer(&layer, None);
        std::mem::ManuallyDrop::drop(&mut layer.geometricMask);
        if m.expanded {
            self.ink(color(1., 1., 1., 0.1));
            self.ctx.DrawGeometry(&shape, &self.brush, 1., None);
        }
        let c = m.body();
        if m.expanded && m.width.value > 250. && (m.height.value - m.height.target).abs() < 35. {
            // Focus and press feedback share the same hit rectangles as input.
            for (hit, r) in m.visual_controls() {
                if pressed == Some(hit) {
                    let inset = r.h
                        * if matches!(hit, Hit::Tool(_)) {
                            0.025
                        } else {
                            0.03
                        };
                    self.fill(
                        Rect {
                            x: r.x + inset,
                            y: r.y + inset,
                            w: r.w - inset * 2.,
                            h: r.h - inset * 2.,
                        },
                        12.,
                        color(1., 1., 1., 0.16),
                    );
                }
                if m.focus == Some(hit) {
                    self.ink(blue);
                    self.ctx.DrawRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: rect(r),
                            radiusX: 12.,
                            radiusY: 12.,
                        },
                        &self.brush,
                        1.,
                        None,
                    );
                }
            }
            if m.visible_tools().next().is_some() {
                self.ctx
                    .PushAxisAlignedClip(&rect(m.bar()), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                for i in m.visible_tools() {
                    let r = m.tool(i);
                    if hover == Some(Hit::Tool(i)) || m.focus == Some(Hit::Tool(i)) {
                        self.fill(r, 12., color(1., 1., 1., 0.1));
                    }
                    self.glyph(
                        i,
                        r,
                        if matches!(
                            (m.page(), i),
                            (Page::Timer, 0)
                                | (Page::Volume, 1)
                                | (Page::Clock, 5)
                                | (Page::Weather, 6)
                        ) {
                            color(145. / 255., 202. / 255., 1., 1.)
                        } else {
                            color(217. / 255., 234. / 255., 1., 1.)
                        },
                        if pressed == Some(Hit::Tool(i)) {
                            15.2
                        } else {
                            16.
                        },
                    )?;
                }
                self.ctx.PopAxisAlignedClip();
            }
            let exit_opacity = m.pending_page.map_or(1., |(_, started)| {
                1. - isle_ui::rolling::page_ease(((m.now - started) / 0.1).clamp(0., 1.) as f32)
            });
            let exit_layer = D2D1_LAYER_PARAMETERS {
                contentBounds: rect(Rect {
                    x: 0.,
                    y: 0.,
                    w: HOST,
                    h: HOST,
                }),
                maskTransform: Matrix3x2 {
                    M11: 1.,
                    M22: 1.,
                    ..Default::default()
                },
                opacity: exit_opacity,
                ..Default::default()
            };
            self.ctx.PushLayer(&exit_layer, None);
            if m.page() != Page::Music {
                self.fill(
                    Rect {
                        x: c.x,
                        y: c.y,
                        w: 28.,
                        h: 28.,
                    },
                    8.,
                    color(
                        65. / 255.,
                        145. / 255.,
                        235. / 255.,
                        if hover == Some(Hit::Back) { 0.25 } else { 0.14 },
                    ),
                );
                self.icons.draw(
                    Icon::Back,
                    c.x + 6.,
                    c.y + 6.,
                    16.,
                    2.,
                    color(145. / 255., 202. / 255., 1., 1.),
                )?;
                self.text_with_weight(
                    match m.page() {
                        Page::Timer => "倒计时",
                        Page::Volume => "系统音量",
                        Page::Clock => "时间",
                        Page::Weather => "天气",
                        _ => "",
                    },
                    Rect {
                        x: c.x + 37.,
                        y: c.y + 7.,
                        w: c.w - 37.,
                        h: 18.,
                    },
                    11,
                    DWRITE_FONT_WEIGHT_MEDIUM,
                    color(235. / 255., 244. / 255., 1., 0.78),
                )?;
            }
            let started = match self.page_enter {
                Some((generation, started)) if generation == m.generation => started,
                _ => {
                    self.page_enter = Some((m.generation, m.now));
                    m.now
                }
            };
            let progress = if m.reduced || m.page() == Page::Music {
                1.
            } else {
                ((m.now - started) / 0.18).clamp(0., 1.) as f32
            };
            // CSS `ease` (.25,.1,.25,1), matching page-enter in the Svelte surface.
            let eased = isle_ui::rolling::page_ease(progress);
            self.content_animating |= progress < 1.;
            let page_layer = D2D1_LAYER_PARAMETERS {
                contentBounds: rect(Rect {
                    x: 0.,
                    y: 0.,
                    w: HOST,
                    h: HOST,
                }),
                maskTransform: Matrix3x2 {
                    M11: 1.,
                    M22: 1.,
                    ..Default::default()
                },
                opacity: eased,
                ..Default::default()
            };
            self.ctx.PushLayer(&page_layer, None);
            self.ctx.SetTransform(&Matrix3x2 {
                M11: 1.,
                M22: 1.,
                M32: (1. - eased) * 4.,
                ..Default::default()
            });
            match m.page() {
                Page::Music => {
                    self.fill(
                        Rect {
                            x: c.x,
                            y: c.y + 4.,
                            w: 52.,
                            h: 52.,
                        },
                        12.,
                        color(0.14, 0.22, 0.3, 1.),
                    );
                    self.ink(blue);
                    self.ctx.DrawEllipse(
                        &D2D1_ELLIPSE {
                            point: point(c.x + 27., c.y + 31.),
                            radiusX: 17.,
                            radiusY: 17.,
                        },
                        &self.brush,
                        1.5,
                        None,
                    );
                    self.glyph(
                        5,
                        Rect {
                            x: c.x + 13.,
                            y: c.y + 17.,
                            w: 28.,
                            h: 28.,
                        },
                        white,
                        16.,
                    )?;
                    if m.media.as_ref().is_some_and(|media| media.cover.is_some()) {
                        // Clear the placeholder beneath translucent rounded corners.
                        self.fill(
                            Rect {
                                x: c.x,
                                y: c.y + 4.,
                                w: 52.,
                                h: 52.,
                            },
                            12.,
                            background_color,
                        );
                        let bitmap = &self.cover.as_ref().unwrap().1;
                        self.ctx.DrawBitmap(
                            bitmap,
                            Some(&rect(Rect {
                                x: c.x,
                                y: c.y + 4.,
                                w: 52.,
                                h: 52.,
                            })),
                            1.,
                            D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                            None,
                        );
                    }
                    self.ink(color(1., 1., 1., 0.1));
                    self.ctx.DrawRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: rect(Rect {
                                x: c.x - 0.5,
                                y: c.y + 3.5,
                                w: 53.,
                                h: 53.,
                            }),
                            radiusX: 12.,
                            radiusY: 12.,
                        },
                        &self.brush,
                        1.,
                        None,
                    );
                    let title = if let Some(media) = &m.media {
                        if media.title.is_empty() {
                            "暂无媒体"
                        } else {
                            &media.title
                        }
                    } else if m.track == 0 {
                        "Midnight City"
                    } else {
                        "宇宙尽头的浪漫主义与一场不会结束的午夜公路旅行"
                    };
                    let title_rect = Rect {
                        x: c.x + 64.,
                        y: c.y + 12.,
                        w: (c.w - 100.).max(24.),
                        h: 22.,
                    };
                    let (layout, title_width) = if let Some(l) = self.layouts.get(title) {
                        l.clone()
                    } else {
                        self.layouts.clear(); // Only the currently displayed title owns a layout.
                        let f = self.format(13)?;
                        let wide: Vec<u16> = title.encode_utf16().collect();
                        let l = self.write.CreateTextLayout(&wide, &f, 2000., 24.)?;
                        let mut metrics = DWRITE_TEXT_METRICS::default();
                        l.GetMetrics(&mut metrics)?;
                        let cached = (l, metrics.width);
                        self.layouts.insert(title.to_string(), cached.clone());
                        cached
                    };
                    let distance = (title_width - title_rect.w).max(0.);
                    self.title_overflow = distance > 0.;
                    let shift = if m.reduced {
                        0.
                    } else {
                        marquee(distance, m.now - m.title_started)
                    };
                    self.ctx
                        .PushAxisAlignedClip(&rect(title_rect), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                    self.ink(white);
                    self.ctx.DrawTextLayout(
                        point(title_rect.x - shift, title_rect.y),
                        &layout,
                        &self.brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                    );
                    self.ctx.PopAxisAlignedClip();
                    if distance > 0. {
                        for i in 0..12 {
                            let a = (12 - i) as f32 / 12.;
                            self.fill(
                                Rect {
                                    x: title_rect.x + i as f32,
                                    y: title_rect.y,
                                    w: 1.,
                                    h: 22.,
                                },
                                0.,
                                color(background[0], background[1], background[2], a),
                            );
                            self.fill(
                                Rect {
                                    x: title_rect.x + title_rect.w - i as f32 - 1.,
                                    y: title_rect.y,
                                    w: 1.,
                                    h: 22.,
                                },
                                0.,
                                color(background[0], background[1], background[2], a),
                            );
                        }
                    }
                    self.text_with_weight(
                        m.media
                            .as_ref()
                            .map(|media| {
                                if m.media_failed {
                                    "媒体暂不可用"
                                } else if media.session == 0 {
                                    "等待播放器"
                                } else {
                                    media.artist.as_str()
                                }
                            })
                            .unwrap_or("M83 · 原生渲染预览"),
                        Rect {
                            x: c.x + 64.,
                            y: c.y + 36.,
                            w: c.w - 100.,
                            h: 18.,
                        },
                        11,
                        DWRITE_FONT_WEIGHT_MEDIUM,
                        color(0.68, 0.68, 0.68, 1.),
                    )?;
                    self.spectrum(c.x + c.w - 28., c.y + 30., m);
                    let (elapsed_ms, duration_ms, position_known) = m
                        .media
                        .as_ref()
                        .map(|media| {
                            (
                                media.timeline.position(m.now, media.playing),
                                media.timeline.duration_ms,
                                media.timeline.position_known,
                            )
                        })
                        .unwrap_or((0, 0, false));
                    let progress = if duration_ms == 0 {
                        if m.media.is_none() {
                            0.46
                        } else {
                            0.
                        }
                    } else if !position_known {
                        0.
                    } else {
                        (elapsed_ms as f32 / duration_ms as f32).clamp(0., 1.)
                    };
                    let progress_y = c.y + 68.;
                    let progress_x = c.x + 34.;
                    let progress_width = (c.w - 76.).max(24.);
                    let progress_color = color(1., 1., 1., 0.19);
                    let time_color = color(0.55, 0.55, 0.55, 1.);
                    let mut elapsed_label = [0_u16; 24];
                    let elapsed_label_len =
                        if (position_known && duration_ms > 0) || m.media.is_none() {
                            write_media_time(elapsed_ms, &mut elapsed_label)
                        } else {
                            elapsed_label[..5].copy_from_slice(&[
                                b'-' as u16,
                                b'-' as u16,
                                b':' as u16,
                                b'-' as u16,
                                b'-' as u16,
                            ]);
                            5
                        };
                    self.text_utf16(
                        &elapsed_label[..elapsed_label_len],
                        Rect {
                            x: c.x,
                            y: progress_y - 5.,
                            w: 26.,
                            h: 16.,
                        },
                        11,
                        DWRITE_FONT_WEIGHT_SEMI_BOLD,
                        time_color,
                    )?;
                    let mut remaining_label = [0_u16; 25];
                    let remaining_label_len =
                        if (position_known && duration_ms > 0) || m.media.is_none() {
                            remaining_label[0] = u16::from(b'-');
                            1 + write_media_time(
                                duration_ms.saturating_sub(elapsed_ms),
                                &mut remaining_label[1..],
                            )
                        } else {
                            remaining_label[..5].copy_from_slice(&[
                                b'-' as u16,
                                b'-' as u16,
                                b':' as u16,
                                b'-' as u16,
                                b'-' as u16,
                            ]);
                            5
                        };
                    self.text_utf16(
                        &remaining_label[..remaining_label_len],
                        Rect {
                            x: c.x + c.w - 34.,
                            y: progress_y - 5.,
                            w: 34.,
                            h: 16.,
                        },
                        11,
                        DWRITE_FONT_WEIGHT_SEMI_BOLD,
                        time_color,
                    )?;
                    self.fill(
                        Rect {
                            x: progress_x,
                            y: progress_y - 1.5,
                            w: progress_width,
                            h: 6.,
                        },
                        3.,
                        progress_color,
                    );
                    self.fill(
                        Rect {
                            x: progress_x,
                            y: progress_y - 1.5,
                            w: progress_width * progress,
                            h: 6.,
                        },
                        3.,
                        white,
                    );
                    for (hit, r) in m.visual_controls() {
                        if matches!(hit, Hit::Previous | Hit::Play | Hit::Next) {
                            let white = if m.enabled(hit) {
                                white
                            } else {
                                color(0.3, 0.32, 0.35, 1.)
                            };
                            if m.enabled(hit) && (hover == Some(hit) || m.focus == Some(hit)) {
                                self.fill(r, 12., color(1., 1., 1., 0.1));
                            }
                            let x = r.x + 24.;
                            let y = r.y + 24.;
                            let icon_scale = if pressed == Some(hit) && m.enabled(hit) {
                                0.94
                            } else {
                                1.
                            };
                            if hit == Hit::Play && m.playing {
                                self.fill(
                                    Rect {
                                        x: x - 6. * icon_scale,
                                        y: y - 7. * icon_scale,
                                        w: 4. * icon_scale,
                                        h: 14. * icon_scale,
                                    },
                                    1.,
                                    white,
                                );
                                self.fill(
                                    Rect {
                                        x: x + 2. * icon_scale,
                                        y: y - 7. * icon_scale,
                                        w: 4. * icon_scale,
                                        h: 14. * icon_scale,
                                    },
                                    1.,
                                    white,
                                );
                            } else {
                                let direction = if hit == Hit::Previous { -1. } else { 1. };
                                let triangle = self.factory.CreatePathGeometry()?;
                                let sink = triangle.Open()?;
                                sink.BeginFigure(
                                    point(x - 5. * direction * icon_scale, y - 7. * icon_scale),
                                    D2D1_FIGURE_BEGIN_FILLED,
                                );
                                sink.AddLines(&[
                                    point(x + 6. * direction * icon_scale, y),
                                    point(x - 5. * direction * icon_scale, y + 7. * icon_scale),
                                ]);
                                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                                sink.Close()?;
                                self.ink(white);
                                self.ctx.FillGeometry(&triangle, &self.brush, None);
                                if hit != Hit::Play {
                                    self.fill(
                                        Rect {
                                            x: x + 7. * direction * icon_scale - icon_scale,
                                            y: y - 7. * icon_scale,
                                            w: 2. * icon_scale,
                                            h: 14. * icon_scale,
                                        },
                                        1.,
                                        white,
                                    );
                                }
                            }
                        }
                    }
                }
                Page::Volume => self.volume_panel(m, hover)?,
                Page::Timer => self.timer_panel(m, hover)?,
                Page::Clock => self.clock_panel(m)?,
                Page::Weather => self.weather_panel(m, hover)?,
            }
            self.ctx.SetTransform(&Matrix3x2 {
                M11: 1.,
                M22: 1.,
                ..Default::default()
            });
            self.ctx.PopLayer();
            self.ctx.PopLayer();
        } else if !m.expanded {
            let o = m.origin();
            if m.timer_active || m.timer_finished {
                self.compact_timer(m)?;
            } else if vertical(m.edge) {
                self.compact_artwork(m)?;
                let inset = if m.attached { m.shoulder.value } else { 0. };
                self.spectrum(
                    o.x + m.width.value / 2. - 11.,
                    o.y + m.height.value - inset - 18.,
                    m,
                );
            } else {
                self.compact_artwork(m)?;
                let inset = if m.attached { m.shoulder.value } else { 0. };
                self.spectrum(
                    o.x + m.width.value - inset - 30.,
                    o.y + m.height.value / 2.,
                    m,
                );
            }
        }
        self.ctx.PopLayer();
        if !self.number_used {
            self.digits.clear();
        }
        self.ctx.EndDraw(None, None)?;
        self.swap.Present(1, 0).ok()?;
        self.frames += 1;
        Ok(())
    }
    unsafe fn spectrum(&self, x: f32, y: f32, m: &Model) {
        let palette = self
            .cover
            .as_ref()
            .map(|(_, _, palette)| *palette)
            .unwrap_or([[0.53; 3], [0.9; 3]]);
        for i in 0..6 {
            let sample = palette[if i >= 3 { 1 } else { 0 }];
            let h = if let Some(spectrum) = &m.spectrum {
                2. + 15. * spectrum.values[i].clamp(0., 1.)
            } else if !m.playing || m.media.is_some() {
                2.
            } else if m.reduced {
                8.
            } else {
                3. + ((m.now as f32 * 4. + i as f32 * 1.3).sin() + 1.) * 7.
            };
            self.fill(
                Rect {
                    x: x + i as f32 * 4.,
                    y: y - h / 2.,
                    w: 2.,
                    h,
                },
                1.,
                color(sample[0], sample[1], sample[2], 1.),
            );
        }
    }
    unsafe fn compact_artwork(&self, m: &Model) -> Result<()> {
        let r = m.compact_cover();
        self.fill(r, 10., color(1., 1., 1., 0.06));
        if let (Some((cover, _, _)), Some(brush)) = (&self.cover, &self.disc_brush) {
            let angle = m.disc_angle;
            let (sin, cos) = angle.sin_cos();
            let sx = r.w / cover.width as f32;
            let sy = r.h / cover.height as f32;
            brush.SetTransform(&Matrix3x2 {
                M11: sx * cos,
                M12: sx * sin,
                M21: -sy * sin,
                M22: sy * cos,
                M31: r.x + 10. - 10. * cos + 10. * sin,
                M32: r.y + 10. - 10. * sin - 10. * cos,
            });
            self.ctx.FillEllipse(
                &D2D1_ELLIPSE {
                    point: point(r.x + 10., r.y + 10.),
                    radiusX: 10.,
                    radiusY: 10.,
                },
                brush,
            );
        } else {
            self.icons.draw(
                Icon::Music,
                r.x + 4.,
                r.y + 4.,
                12.,
                2.,
                color(1., 1., 1., 0.3),
            )?;
        }
        Ok(())
    }
}

/// A private DirectWrite collection; never installs fonts into Windows.
unsafe fn load_fonts(write: &IDWriteFactory) -> Option<IDWriteFontCollection> {
    let factory: IDWriteFactory5 = write.cast().ok()?;
    let builder = factory.CreateFontSetBuilder2().ok()?;
    let folder = std::env::current_exe().ok()?.parent()?.join("fonts");
    for weight in ["Regular", "Medium", "Bold"] {
        let path = folder.join(format!("MiSans-{weight}.ttf"));
        let file = factory
            .CreateFontFileReference(&HSTRING::from(path.as_os_str()), None)
            .ok()?;
        builder.AddFontFile(&file).ok()?;
    }
    factory
        .CreateFontCollectionFromFontSet(&builder.CreateFontSet().ok()?)
        .ok()?
        .cast()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::{cover_spectrum_palette, format_media_time};

    #[test]
    fn progress_time_labels_use_the_legacy_minute_second_format() {
        assert_eq!(format_media_time(0), "0:00");
        assert_eq!(format_media_time(129_999), "2:09");
        assert_eq!(format_media_time(3_600_000), "60:00");
    }

    #[test]
    fn artwork_spectrum_palette_preserves_the_cover_hue_without_heap_caches() {
        let cover = isle_core::Cover {
            width: 2,
            height: 1,
            pixels: vec![0, 0, 255, 255, 0, 0, 255, 255],
        };
        let palette = cover_spectrum_palette(&cover);
        for color in palette {
            assert!(color[0] > 0.7);
            assert!(color[0] > color[1] * 1.5 && color[0] > color[2] * 1.5);
        }
    }
}
