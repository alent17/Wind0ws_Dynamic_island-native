use super::model::{self, Page};
use std::path::PathBuf;
use windows::{
    core::{w, ComInterface, Result, HSTRING},
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::{
            Direct2D::Common::{
                D2D1_ALPHA_MODE_UNKNOWN, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
            },
            Direct2D::*,
            DirectWrite::*,
            Dxgi::Common::DXGI_FORMAT_UNKNOWN,
        },
        UI::WindowsAndMessaging::GetClientRect,
    },
};

fn color(rgb: (u8, u8, u8), alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: rgb.0 as f32 / 255.0,
        g: rgb.1 as f32 / 255.0,
        b: rgb.2 as f32 / 255.0,
        a: alpha,
    }
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}

pub struct ShellRender {
    target: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
    _write: IDWriteFactory,
    _fonts: Option<IDWriteFontCollection>,
    _family: &'static str,
    regular: IDWriteTextFormat,
    medium: IDWriteTextFormat,
    title: IDWriteTextFormat,
    eyebrow: IDWriteTextFormat,
    scale: f32,
}

impl ShellRender {
    pub unsafe fn new(hwnd: HWND, scale: f32) -> Result<Self> {
        let mut bounds = RECT::default();
        GetClientRect(hwnd, &mut bounds)?;
        let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let target = factory.CreateHwndRenderTarget(
            &D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_UNKNOWN,
                    alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
                },
                dpiX: 96.0 * scale,
                dpiY: 96.0 * scale,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            },
            &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U {
                    width: (bounds.right - bounds.left).max(1) as u32,
                    height: (bounds.bottom - bounds.top).max(1) as u32,
                },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            },
        )?;
        let brush = target.CreateSolidColorBrush(&color(model::PRIMARY_TEXT, 1.0), None)?;
        let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let fonts = load_fonts(&write);
        let family = if fonts.is_some() {
            "MiSans"
        } else {
            "Segoe UI"
        };
        let regular = text_format(
            &write,
            fonts.as_ref(),
            family,
            14.0,
            DWRITE_FONT_WEIGHT_NORMAL,
        )?;
        let medium = text_format(
            &write,
            fonts.as_ref(),
            family,
            14.0,
            DWRITE_FONT_WEIGHT_MEDIUM,
        )?;
        let title = text_format(
            &write,
            fonts.as_ref(),
            family,
            28.0,
            DWRITE_FONT_WEIGHT_BOLD,
        )?;
        let eyebrow = text_format(
            &write,
            fonts.as_ref(),
            family,
            11.0,
            DWRITE_FONT_WEIGHT_NORMAL,
        )?;
        Ok(Self {
            target,
            brush,
            _write: write,
            _fonts: fonts,
            _family: family,
            regular,
            medium,
            title,
            eyebrow,
            scale,
        })
    }

    pub unsafe fn set_dpi(&mut self, scale: f32) {
        self.scale = scale;
        self.target.SetDpi(96.0 * scale, 96.0 * scale);
    }

    pub(super) unsafe fn create_control_painter(
        &self,
        scale: f32,
    ) -> Result<super::controls::ControlPainter> {
        super::controls::ControlPainter::new(
            &self._write,
            self._fonts.as_ref(),
            self._family,
            scale,
        )
    }

    pub unsafe fn resize(&self, width: i32, height: i32) -> Result<()> {
        self.target.Resize(&D2D_SIZE_U {
            width: width.max(1) as u32,
            height: height.max(1) as u32,
        })
    }

    unsafe fn fill(&self, bounds: D2D_RECT_F, radius: f32, rgb: (u8, u8, u8), alpha: f32) {
        self.brush.SetColor(&color(rgb, alpha));
        self.target.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: bounds,
                radiusX: radius,
                radiusY: radius,
            },
            &self.brush,
        );
    }

    unsafe fn text(
        &self,
        text: &str,
        bounds: D2D_RECT_F,
        format: &IDWriteTextFormat,
        rgb: (u8, u8, u8),
    ) {
        let wide = text.encode_utf16().collect::<Vec<_>>();
        self.brush.SetColor(&color(rgb, 1.0));
        self.target.DrawText(
            &wide,
            format,
            &bounds,
            &self.brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    unsafe fn card(&self, scroll_x: f32, y: f32, height: f32, width: f32, title: &str, hint: &str) {
        let left = model::CONTENT_LEFT - scroll_x;
        self.fill(rect(left, y, width, height), 16.0, model::CARD, 1.0);
        self.text(
            title,
            rect(left + 24.0, y + 18.0, width - 48.0, 22.0),
            &self.medium,
            model::PRIMARY_TEXT,
        );
        self.text(
            hint,
            rect(left + 24.0, y + 42.0, width - 48.0, 20.0),
            &self.regular,
            model::SECONDARY_TEXT,
        );
    }

    pub unsafe fn draw(
        &mut self,
        hwnd: HWND,
        page: Page,
        save_status: &str,
        notice: &str,
        scroll: f32,
        horizontal_scroll: f32,
    ) -> Result<()> {
        let mut client = RECT::default();
        GetClientRect(hwnd, &mut client)?;
        let width = (client.right - client.left) as f32 / self.scale;
        let height = (client.bottom - client.top) as f32 / self.scale;
        let card_width = (width - model::CONTENT_LEFT - 28.0)
            .max(model::WINDOW_WIDTH - model::CONTENT_LEFT - 28.0)
            .max(280.0);
        let content_left = model::CONTENT_LEFT - horizontal_scroll;
        let navigation = model::navigation_layout(height);

        self.target.BeginDraw();
        self.target.Clear(Some(&color(model::WINDOW_BG, 1.0)));
        self.fill(
            rect(0.0, 0.0, model::NAV_WIDTH, height),
            0.0,
            model::SURFACE,
            1.0,
        );
        if !model::navigation_is_compact(height) {
            self.text(
                "ISLE",
                rect(24.0, 28.0, 120.0, 25.0),
                &self.medium,
                model::PRIMARY_TEXT,
            );
            self.text(
                "设置中心",
                rect(24.0, 57.0, 135.0, 20.0),
                &self.regular,
                model::SECONDARY_TEXT,
            );
        }
        let active_nav = navigation[page.index()];
        self.fill(
            rect(active_nav.x, active_nav.y, active_nav.w, active_nav.h),
            9.0,
            model::CARD_HOVER,
            1.0,
        );
        self.target.PushAxisAlignedClip(
            &rect(
                model::CONTENT_LEFT,
                0.0,
                (width - model::CONTENT_LEFT).max(0.0),
                height,
            ),
            D2D1_ANTIALIAS_MODE_ALIASED,
        );
        self.text(
            "ISLE  /  SETTINGS",
            rect(content_left, 29.0, card_width, 18.0),
            &self.eyebrow,
            model::MUTED_TEXT,
        );
        self.text(
            page.title(),
            rect(content_left, 51.0, card_width, 38.0),
            &self.title,
            model::PRIMARY_TEXT,
        );
        self.text(
            page.description(),
            rect(content_left, 94.0, card_width, 23.0),
            &self.regular,
            model::SECONDARY_TEXT,
        );

        match page {
            Page::General => {
                self.card(
                    horizontal_scroll,
                    144.0 - scroll,
                    224.0,
                    card_width,
                    "系统行为",
                    "设置即时作用于正在运行的 Isle。",
                );
                self.card(
                    horizontal_scroll,
                    384.0 - scroll,
                    246.0,
                    card_width,
                    "时钟与窗口",
                    "桌面集成选项。更改会自动保存。",
                );
            }
            Page::Appearance => {
                self.card(
                    horizontal_scroll,
                    144.0 - scroll,
                    232.0,
                    card_width,
                    "外观与位置",
                    "调整真实灵动岛，不使用单独的预览状态。",
                );
                self.card(
                    horizontal_scroll,
                    392.0 - scroll,
                    280.0,
                    card_width,
                    "岛体几何",
                    "拖动滑块时，灵动岛会实时更新。",
                );
            }
            Page::Modules => {
                self.card(
                    horizontal_scroll,
                    144.0 - scroll,
                    356.0,
                    card_width,
                    "工具栏模块",
                    "选择需要出现在 Isle 工具栏中的功能。",
                );
            }
            Page::Media => {
                self.card(
                    horizontal_scroll,
                    144.0 - scroll,
                    230.0,
                    card_width,
                    "播放器",
                    "使用当前系统媒体会话和已接入的播放器设置。",
                );
            }
            Page::Weather => {
                self.card(
                    horizontal_scroll,
                    144.0 - scroll,
                    386.0,
                    card_width,
                    "天气城市",
                    "搜索并保存城市后，天气模块会使用该位置。",
                );
            }
            Page::Advanced => {
                self.card(
                    horizontal_scroll,
                    144.0 - scroll,
                    280.0,
                    card_width,
                    "配置保存",
                    "运行时更改保持生效；写入失败时可重试或恢复。 ",
                );
            }
        }
        self.target.PopAxisAlignedClip();
        let footer_width = (width - model::CONTENT_LEFT - 28.0).max(80.0);
        self.text(
            &format!("保存状态：{save_status}"),
            rect(
                model::CONTENT_LEFT,
                height - 34.0,
                footer_width * 0.42,
                18.0,
            ),
            &self.regular,
            model::SECONDARY_TEXT,
        );
        if !notice.is_empty() {
            self.text(
                notice,
                rect(
                    model::CONTENT_LEFT + footer_width * 0.44,
                    height - 34.0,
                    footer_width * 0.56,
                    18.0,
                ),
                &self.regular,
                model::SECONDARY_TEXT,
            );
        }
        self.target.EndDraw(None, None)?;
        Ok(())
    }
}

unsafe fn text_format(
    write: &IDWriteFactory,
    fonts: Option<&IDWriteFontCollection>,
    family: &str,
    size: f32,
    weight: DWRITE_FONT_WEIGHT,
) -> Result<IDWriteTextFormat> {
    let format = write.CreateTextFormat(
        &HSTRING::from(family),
        fonts,
        weight,
        DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        size,
        w!("zh-CN"),
    )?;
    format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    Ok(format)
}

/// Match the main renderer's private MiSans collection without installing the font system-wide.
unsafe fn load_fonts(write: &IDWriteFactory) -> Option<IDWriteFontCollection> {
    let factory: IDWriteFactory5 = write.cast().ok()?;
    let builder = factory.CreateFontSetBuilder2().ok()?;
    let folder = std::env::current_exe().ok()?.parent()?.join("fonts");
    for weight in ["Regular", "Medium", "Bold"] {
        let path: PathBuf = folder.join(format!("MiSans-{weight}.ttf"));
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
