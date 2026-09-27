use isle_ui::{geometry::*, model::*};
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
    cover: Option<(std::sync::Arc<isle_core::Cover>, ID2D1Bitmap, [f32; 3])>,
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
    formats: HashMap<u32, IDWriteTextFormat>,
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
fn cover_accent(cover: &isle_core::Cover) -> [f32; 3] {
    let mut counts = HashMap::<(u8, u8, u8), usize>::new();
    for pixel in cover
        .pixels
        .chunks_exact(4)
        .take((cover.width as usize).saturating_mul(cover.height as usize))
    {
        let alpha = pixel[3] as u32;
        if alpha < 128 {
            continue;
        }
        let unpremultiply = |channel: u8| ((channel as u32 * 255 / alpha).min(255)) as u8;
        let b = unpremultiply(pixel[0]);
        let g = unpremultiply(pixel[1]);
        let r = unpremultiply(pixel[2]);
        if (r == 0 && g == 0 && b == 0) || (r == 255 && g == 255 && b == 255) {
            continue;
        }
        *counts
            .entry((r / 12 * 12, g / 12 * 12, b / 12 * 12))
            .or_default() += 1;
    }
    let saturation = |(r, g, b): (u8, u8, u8)| {
        let (r, g, b) = (r as f32 / 255., g as f32 / 255., b as f32 / 255.);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let lightness = (max + min) / 2.;
        if max == min {
            0.
        } else if lightness > 0.5 {
            (max - min) / (2. - max - min)
        } else {
            (max - min) / (max + min)
        }
    };
    counts
        .into_iter()
        .max_by(|(color_a, count_a), (color_b, count_b)| {
            let score_a = saturation(*color_a) * (*count_a as f32).ln();
            let score_b = saturation(*color_b) * (*count_b as f32).ln();
            score_a
                .total_cmp(&score_b)
                .then_with(|| count_a.cmp(count_b))
        })
        .map(|((r, g, b), _)| {
            [
                r.saturating_add(12),
                g.saturating_add(12),
                b.saturating_add(12),
            ]
            .map(|channel| channel as f32 / 255.)
        })
        .unwrap_or([60. / 255., 80. / 255., 100. / 255.])
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
        Ok(Self {
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
    unsafe fn line(&self, a: Point, b: Point, width: f32, c: D2D1_COLOR_F) {
        self.ink(c);
        self.ctx
            .DrawLine(point(a.x, a.y), point(b.x, b.y), &self.brush, width, None);
    }
    unsafe fn format(&mut self, size: u32) -> Result<IDWriteTextFormat> {
        if let Some(f) = self.formats.get(&size) {
            return Ok(f.clone());
        }
        let f = self.write.CreateTextFormat(
            &HSTRING::from(self.font_family),
            self.fonts.as_ref(),
            if size >= 26 {
                DWRITE_FONT_WEIGHT_BOLD
            } else if size == 13 {
                DWRITE_FONT_WEIGHT_MEDIUM
            } else {
                DWRITE_FONT_WEIGHT_NORMAL
            },
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size as f32,
            w!("zh-CN"),
        )?;
        f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        self.formats.insert(size, f.clone());
        Ok(f)
    }
    unsafe fn text(&mut self, text: &str, r: Rect, size: u32, c: D2D1_COLOR_F) -> Result<()> {
        let f = self.format(size)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        self.ink(c);
        self.ctx.DrawText(
            &wide,
            &f,
            &rect(r),
            &self.brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
        Ok(())
    }
    unsafe fn glyph(&self, id: usize, r: Rect, c: D2D1_COLOR_F) {
        let x = r.x + r.w / 2.;
        let y = r.y + r.h / 2.;
        self.ink(c);
        let line = |a: (f32, f32), b: (f32, f32)| {
            self.line(
                Point {
                    x: x + a.0,
                    y: y + a.1,
                },
                Point {
                    x: x + b.0,
                    y: y + b.1,
                },
                1.5,
                c,
            )
        };
        match id {
            0 | 5 => {
                self.ctx.DrawEllipse(
                    &D2D1_ELLIPSE {
                        point: point(x, y),
                        radiusX: 6.,
                        radiusY: 6.,
                    },
                    &self.brush,
                    1.4,
                    None,
                );
                line((0., -3.), (0., 0.));
                line((0., 0.), (3., 1.));
                if id == 0 {
                    line((-2., -9.), (2., -9.));
                }
            }
            1 => {
                line((-6., -2.), (-3., -2.));
                line((-3., -2.), (1., -5.));
                line((1., -5.), (1., 5.));
                line((1., 5.), (-3., 2.));
                line((-3., 2.), (-6., 2.));
                line((-6., 2.), (-6., -2.));
                line((5., -4.), (6., 0.));
                line((6., 0.), (5., 4.));
            }
            2 => {
                self.ctx.DrawRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(Rect {
                            x: x - 5.,
                            y: y - 6.,
                            w: 11.,
                            h: 12.,
                        }),
                        radiusX: 2.,
                        radiusY: 2.,
                    },
                    &self.brush,
                    1.4,
                    None,
                );
                line((-8., -4.), (-8., 4.));
            }
            3 => {
                self.ctx.DrawEllipse(
                    &D2D1_ELLIPSE {
                        point: point(x, y),
                        radiusX: 4.,
                        radiusY: 4.,
                    },
                    &self.brush,
                    1.5,
                    None,
                );
                for i in 0..8 {
                    let t = i as f32 * std::f32::consts::PI / 4.;
                    line((t.cos() * 5., t.sin() * 5.), (t.cos() * 7., t.sin() * 7.));
                }
            }
            4 => {
                line((-7., -5.), (7., 5.));
                line((-7., 0.), (-3., -4.));
                line((-3., -4.), (3., -4.));
                line((3., -4.), (7., 0.));
                line((7., 0.), (3., 4.));
                line((3., 4.), (-3., 4.));
                line((-3., 4.), (-7., 0.));
            }
            _ => {
                self.ctx.DrawEllipse(
                    &D2D1_ELLIPSE {
                        point: point(x, y),
                        radiusX: 3.,
                        radiusY: 3.,
                    },
                    &self.brush,
                    1.5,
                    None,
                );
                for i in 0..8 {
                    let t = i as f32 * std::f32::consts::PI / 4.;
                    line((t.cos() * 5., t.sin() * 5.), (t.cos() * 7., t.sin() * 7.));
                }
            }
        }
    }
    pub unsafe fn draw(
        &mut self,
        m: &Model,
        hover: Option<Hit>,
        pressed: Option<Hit>,
    ) -> Result<()> {
        if !m.expanded || m.page() != Page::Music {
            self.layouts.clear();
            self.cover = None;
        }
        let white = color(0.94, 0.96, 1., 1.);
        let gray = color(0.5, 0.55, 0.6, 1.);
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
        let media_cover = if m.expanded && m.page() == Page::Music {
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
                self.cover = Some((cover.clone(), bitmap, cover_accent(cover)));
            }
        } else {
            self.cover = None;
        }
        let background = if m.use_album_color {
            self.cover
                .as_ref()
                .map(|(_, _, accent)| *accent)
                .unwrap_or(m.background_color)
        } else {
            m.background_color
        };
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
        let c = m.body();
        if m.expanded && m.width.value > 250. && (m.height.value - m.height.target).abs() < 35. {
            // Focus and press feedback share the same hit rectangles as input.
            for (hit, r) in m.controls() {
                if !matches!(hit, Hit::Tool(_))
                    && m.page() != Page::Music
                    && hover == Some(hit)
                    && hit != Hit::Volume
                {
                    self.fill(r, 12., color(1., 1., 1., 0.1));
                }
                if pressed == Some(hit) {
                    self.fill(r, 12., color(1., 1., 1., 0.16));
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
                        if m.focus == Some(Hit::Tool(i)) {
                            blue
                        } else {
                            white
                        },
                    );
                }
                self.ctx.PopAxisAlignedClip();
            }
            if m.page() != Page::Music {
                self.line(
                    Point {
                        x: c.x + 18.,
                        y: c.y + 8.,
                    },
                    Point {
                        x: c.x + 10.,
                        y: c.y + 14.,
                    },
                    1.5,
                    blue,
                );
                self.line(
                    Point {
                        x: c.x + 10.,
                        y: c.y + 14.,
                    },
                    Point {
                        x: c.x + 18.,
                        y: c.y + 20.,
                    },
                    1.5,
                    blue,
                );
                self.text(
                    match m.page() {
                        Page::Timer => "倒计时",
                        Page::Volume if m.device_menu => "输出设备",
                        Page::Volume => "音量",
                        Page::Clock => "时间",
                        Page::Weather => "天气",
                        _ => "",
                    },
                    Rect {
                        x: c.x + 38.,
                        y: c.y + 4.,
                        w: c.w - 38.,
                        h: 24.,
                    },
                    12,
                    gray,
                )?;
            }
            match m.page() {
                Page::Music => {
                    self.fill(
                        Rect {
                            x: c.x,
                            y: c.y + 4.,
                            w: 54.,
                            h: 54.,
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
                    );
                    if m.media.as_ref().is_some_and(|media| media.cover.is_some()) {
                        // Clear the placeholder beneath translucent rounded corners.
                        self.fill(
                            Rect {
                                x: c.x,
                                y: c.y + 4.,
                                w: 54.,
                                h: 54.,
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
                                w: 54.,
                                h: 54.,
                            })),
                            1.,
                            D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                            None,
                        );
                    }
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
                    self.text(
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
                        10,
                        gray,
                    )?;
                    self.spectrum(c.x + c.w - 28., c.y + 30., m);
                    self.fill(
                        Rect {
                            x: c.x,
                            y: c.y + 68.,
                            w: c.w,
                            h: 3.,
                        },
                        1.5,
                        color(1., 1., 1., 0.15),
                    );
                    self.fill(
                        Rect {
                            x: c.x,
                            y: c.y + 68.,
                            w: c.w
                                * m.media
                                    .as_ref()
                                    .map(|media| {
                                        if media.timeline.duration_ms == 0 {
                                            0.
                                        } else {
                                            media.timeline.position(m.now, media.playing) as f32
                                                / media.timeline.duration_ms as f32
                                        }
                                    })
                                    .unwrap_or(0.46),
                            h: 3.,
                        },
                        1.5,
                        white,
                    );
                    for (hit, r) in m.controls() {
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
                            if hit == Hit::Play && m.playing {
                                self.fill(
                                    Rect {
                                        x: x - 6.,
                                        y: y - 7.,
                                        w: 4.,
                                        h: 14.,
                                    },
                                    1.,
                                    white,
                                );
                                self.fill(
                                    Rect {
                                        x: x + 2.,
                                        y: y - 7.,
                                        w: 4.,
                                        h: 14.,
                                    },
                                    1.,
                                    white,
                                );
                            } else {
                                let direction = if hit == Hit::Previous { -1. } else { 1. };
                                let triangle = self.factory.CreatePathGeometry()?;
                                let sink = triangle.Open()?;
                                sink.BeginFigure(
                                    point(x - 5. * direction, y - 7.),
                                    D2D1_FIGURE_BEGIN_FILLED,
                                );
                                sink.AddLines(&[
                                    point(x + 6. * direction, y),
                                    point(x - 5. * direction, y + 7.),
                                ]);
                                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                                sink.Close()?;
                                self.ink(white);
                                self.ctx.FillGeometry(&triangle, &self.brush, None);
                                if hit != Hit::Play {
                                    self.fill(
                                        Rect {
                                            x: x + 7. * direction - 1.,
                                            y: y - 7.,
                                            w: 2.,
                                            h: 14.,
                                        },
                                        1.,
                                        white,
                                    );
                                }
                            }
                        }
                    }
                }
                Page::Volume if m.device_menu => {
                    if let Some(audio) = &m.audio {
                        if audio.devices.is_empty() {
                            self.text(
                                "没有可用输出设备",
                                Rect {
                                    x: c.x,
                                    y: c.y + 44.,
                                    w: c.w,
                                    h: 28.,
                                },
                                12,
                                gray,
                            )?;
                        }
                        for (hit, r) in m.controls() {
                            match hit {
                                Hit::Device(index) => {
                                    let device = &audio.devices[index];
                                    if hover == Some(hit) || m.focus == Some(hit) {
                                        self.fill(r, 12., color(1., 1., 1., 0.1));
                                    }
                                    if device.id == audio.device.id {
                                        self.fill(
                                            Rect {
                                                x: r.x + 8.,
                                                y: r.y + 11.,
                                                w: 5.,
                                                h: 5.,
                                            },
                                            2.,
                                            blue,
                                        );
                                    }
                                    self.text(
                                        &device.name,
                                        Rect {
                                            x: r.x + 22.,
                                            y: r.y + 5.,
                                            w: r.w - 28.,
                                            h: 20.,
                                        },
                                        11,
                                        white,
                                    )?;
                                }
                                Hit::DevicePrev | Hit::DeviceNext => {
                                    if hover == Some(hit) || m.focus == Some(hit) {
                                        self.fill(r, 12., color(1., 1., 1., 0.1));
                                    }
                                    self.text(
                                        if hit == Hit::DevicePrev { "‹" } else { "›" },
                                        Rect {
                                            x: r.x + 9.,
                                            y: r.y + 2.,
                                            w: 20.,
                                            h: 24.,
                                        },
                                        18,
                                        white,
                                    )?;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Page::Volume => {
                    for (hit, r) in m.controls() {
                        if matches!(hit, Hit::Devices | Hit::Mute)
                            && (hover == Some(hit) || m.focus == Some(hit))
                        {
                            self.fill(r, 12., color(1., 1., 1., 0.1));
                        }
                    }
                    for i in 0..23 {
                        let x = c.x + i as f32 * c.w / 22.;
                        let h = if i % 5 == 0 { 25. } else { 14. };
                        self.line(
                            Point { x, y: c.y + 55. },
                            Point {
                                x,
                                y: c.y + 55. + h,
                            },
                            1.,
                            if i as f32 / 22. * 100. <= m.volume {
                                blue
                            } else {
                                gray
                            },
                        );
                    }
                    self.text(
                        &if m.audio.as_ref().is_some_and(|a| a.muted) {
                            "静音".into()
                        } else {
                            format!("{}%", m.volume as u32)
                        },
                        Rect {
                            x: c.x,
                            y: c.y + 95.,
                            w: 80.,
                            h: 38.,
                        },
                        26,
                        blue,
                    )?;
                    self.text(
                        m.audio
                            .as_ref()
                            .map(|a| {
                                if a.failed {
                                    "音频操作暂不可用"
                                } else if a.device.id.is_empty() {
                                    "正在读取设备"
                                } else {
                                    &a.device.name
                                }
                            })
                            .unwrap_or("输出设备 · 原型数据"),
                        Rect {
                            x: c.x + 90.,
                            y: c.y + 107.,
                            w: c.w - 90.,
                            h: 22.,
                        },
                        11,
                        gray,
                    )?;
                }
                Page::Timer => {
                    let remaining = m.timer_left.ceil() as u64;
                    self.text(
                        &format!("{:02}:{:02}", remaining / 60, remaining % 60),
                        Rect {
                            x: c.x,
                            y: c.y + 40.,
                            w: c.w,
                            h: 54.,
                        },
                        42,
                        white,
                    )?;
                    self.text(
                        if m.timer_deadline.is_some() {
                            "暂停"
                        } else if m.timer_left == 0. {
                            "完成 · 重新开始"
                        } else {
                            "开始"
                        },
                        Rect {
                            x: c.x,
                            y: c.y + 109.,
                            w: c.w - 40.,
                            h: 24.,
                        },
                        14,
                        blue,
                    )?;
                    self.glyph(
                        0,
                        Rect {
                            x: c.x + c.w - 32.,
                            y: c.y + 102.,
                            w: 32.,
                            h: 32.,
                        },
                        gray,
                    );
                }
                Page::Clock => {
                    let time = crate::clock::now(&m.time_zone);
                    let st = time.unwrap_or_default();
                    self.text(
                        &if time.is_some() {
                            format!("{:02}:{:02}", st.wHour, st.wMinute)
                        } else {
                            "--:--".into()
                        },
                        Rect {
                            x: c.x,
                            y: c.y + 42.,
                            w: c.w,
                            h: 58.,
                        },
                        44,
                        white,
                    )?;
                    self.text(
                        &if time.is_some() {
                            format!("{} 年 {} 月 {} 日", st.wYear, st.wMonth, st.wDay)
                        } else {
                            "无法读取所选时区".into()
                        },
                        Rect {
                            x: c.x,
                            y: c.y + 108.,
                            w: c.w,
                            h: 22.,
                        },
                        13,
                        gray,
                    )?;
                    self.text(
                        crate::clock::label(&m.time_zone),
                        Rect {
                            x: c.x,
                            y: c.y + 140.,
                            w: c.w,
                            h: 20.,
                        },
                        11,
                        gray,
                    )?;
                }
                Page::Weather => {
                    use isle_core::weather::description;
                    for (hit, r) in m.controls() {
                        if hit == Hit::WeatherSettings {
                            if hover == Some(hit) || m.focus == Some(hit) {
                                self.fill(r, 12., color(1., 1., 1., 0.1));
                            }
                            self.glyph(3, r, white);
                        }
                    }
                    if let Some(city) = &m.weather.city {
                        self.text(
                            &city.name,
                            Rect {
                                x: c.x,
                                y: c.y + 38.,
                                w: c.w,
                                h: 24.,
                            },
                            16,
                            white,
                        )?;
                        if let Some(data) = &m.weather.data {
                            self.text(
                                &format!("{:.0}°  {}", data.temperature, description(data.code)),
                                Rect {
                                    x: c.x,
                                    y: c.y + 66.,
                                    w: c.w,
                                    h: 40.,
                                },
                                26,
                                white,
                            )?;
                            for (i, day) in data.days.iter().take(3).enumerate() {
                                let x = c.x + i as f32 * c.w / 3.;
                                let width = c.w / 3. - 4.;
                                self.text(
                                    &day.date[5..],
                                    Rect {
                                        x,
                                        y: c.y + 121.,
                                        w: width,
                                        h: 20.,
                                    },
                                    12,
                                    gray,
                                )?;
                                self.text(
                                    description(day.code),
                                    Rect {
                                        x,
                                        y: c.y + 149.,
                                        w: width,
                                        h: 20.,
                                    },
                                    12,
                                    blue,
                                )?;
                                self.text(
                                    &format!("{:.0}° / {:.0}°", day.high, day.low),
                                    Rect {
                                        x,
                                        y: c.y + 178.,
                                        w: width,
                                        h: 20.,
                                    },
                                    12,
                                    white,
                                )?;
                            }
                            self.text(
                                &format!(
                                    "{} {} · Open-Meteo",
                                    if m.weather.failed {
                                        "更新失败，缓存"
                                    } else {
                                        "更新"
                                    },
                                    &data.observed[11..]
                                ),
                                Rect {
                                    x: c.x,
                                    y: c.y + 207.,
                                    w: c.w,
                                    h: 16.,
                                },
                                10,
                                gray,
                            )?;
                        } else {
                            self.text(
                                if m.weather.failed {
                                    "天气读取失败，将自动重试"
                                } else {
                                    "正在获取天气…"
                                },
                                Rect {
                                    x: c.x,
                                    y: c.y + 85.,
                                    w: c.w,
                                    h: 28.,
                                },
                                14,
                                gray,
                            )?;
                        }
                    } else {
                        self.text(
                            "还没有设置城市",
                            Rect {
                                x: c.x,
                                y: c.y + 55.,
                                w: c.w,
                                h: 32.,
                            },
                            20,
                            white,
                        )?;
                        self.text(
                            "点击右上角设置，搜索天气城市",
                            Rect {
                                x: c.x,
                                y: c.y + 97.,
                                w: c.w,
                                h: 26.,
                            },
                            12,
                            gray,
                        )?;
                    }
                }
            }
        } else if !m.expanded {
            let o = m.origin();
            if vertical(m.edge) {
                self.glyph(
                    5,
                    Rect {
                        x: o.x,
                        y: o.y + 10.,
                        w: m.width.value,
                        h: 28.,
                    },
                    blue,
                );
                self.spectrum(o.x + 4., o.y + m.height.value - 25., m);
            } else {
                self.glyph(
                    5,
                    Rect {
                        x: o.x + 8.,
                        y: o.y,
                        w: 28.,
                        h: m.height.value,
                    },
                    blue,
                );
                self.spectrum(o.x + m.width.value - 34., o.y + m.height.value / 2., m);
            }
        }
        self.ctx.EndDraw(None, None)?;
        self.swap.Present(1, 0).ok()?;
        self.frames += 1;
        Ok(())
    }
    unsafe fn spectrum(&self, x: f32, y: f32, m: &Model) {
        for i in 0..6 {
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
                color(0.85, 0.95, 1., 1.),
            );
        }
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
    use super::cover_accent;

    #[test]
    fn album_accent_prefers_visible_saturated_cover_colors() {
        let cover = isle_core::Cover {
            width: 4,
            height: 1,
            pixels: vec![
                0, 0, 0, 0, // transparent pixel is ignored
                0, 0, 255, 255, // red
                0, 0, 255, 255, // repeated red has stronger frequency
                255, 255, 255, 255, // white is ignored
            ],
        };
        let accent = cover_accent(&cover);
        assert!(accent[0] > 0.95);
        assert!(accent[1] < 0.06 && accent[2] < 0.06);
    }
}
