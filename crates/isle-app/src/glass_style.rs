//! Pure color policy for Album Glass.
//!
//! The caller must composite [`BASE_RGB`], the matching 96×96 premultiplied
//! BGRA blur cache, an optional returned accent tint, and a black overlay at
//! `dark_overlay_alpha`, then present that material opaque. `blurred_bgra`
//! must belong to the same Cover identity. The style checks every cache pixel;
//! it never estimates the background from an average color.
//!
//! Call once when the Cover/cache changes, never from an animation frame. The
//! 96×96 cache is scanned at most three times (validation/bound, actual
//! contrast report); the overlay solve uses a conservative component-wise RGB
//! maximum and a fixed 24 iterations. Its channels may come from different
//! pixels, which can make the result darker than necessary but cannot weaken
//! the contrast guarantee. Intermediate opacity/fades must preserve the final
//! composed contrast themselves; this settled opaque-stack guarantee does not
//! claim that every transition state has the same contrast.

pub type Rgb = [f32; 3];

/// Existing Music text colors from `render.rs`: title #F0F5FF, artist #ADADAD,
/// and elapsed/remaining labels #8C8C8C. Values use exact 8-bit sRGB steps.
pub const PRIMARY_SMALL_TEXT: Rgb = [240.0 / 255.0, 245.0 / 255.0, 255.0 / 255.0];
pub const SECONDARY_SMALL_TEXT: Rgb = [173.0 / 255.0; 3];
pub const DIM_SMALL_TEXT: Rgb = [140.0 / 255.0; 3];
const SMALL_TEXT_COLORS: [Rgb; 3] = [PRIMARY_SMALL_TEXT, SECONDARY_SMALL_TEXT, DIM_SMALL_TEXT];

/// Opaque near-black backing for Cover transparency and the no-Cover fallback.
pub const BASE_RGB: Rgb = [0.025, 0.028, 0.034];
pub const BLUR_SIDE: usize = 96;
pub const BLUR_PIXELS: usize = BLUR_SIDE * BLUR_SIDE;
pub const BLUR_BYTES: usize = BLUR_SIDE * BLUR_SIDE * 4;
pub const MIN_SMALL_TEXT_CONTRAST: f32 = 4.5;
const CONTRAST_WORKING_TARGET: f32 = 4.75;
const ACCENT_ALPHA: f32 = 0.10;
const ACCENT_SATURATION_MAX: f32 = 0.40;
const ACCENT_LIGHTNESS_MAX: f32 = 0.34;
const DARK_OVERLAY_ALPHA_MAX: f32 = 0.96;
const OVERLAY_SOLVE_STEPS: usize = 24;
const MAX_COVER_SIDE: u32 = 512;
const MAX_COVER_PIXELS: usize = MAX_COVER_SIDE as usize * MAX_COVER_SIDE as usize;
/// One cover validation scan plus one bound scan and one actual contrast scan.
const MAX_STYLE_IMAGE_PIXEL_VISITS: usize = MAX_COVER_PIXELS + 2 * BLUR_PIXELS;

/// Borrowed, premultiplied BGRA cover pixels. Kept independent of the app
/// crate so this policy and its tests compile with `rustc --test` alone.
#[derive(Clone, Copy, Debug)]
pub struct CoverImage<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlbumGlassStyle {
    /// False means skip the Cover bitmap and paint only `BASE_RGB`.
    pub use_cover: bool,
    /// Bounded normalized sRGB accents. They are disabled when alpha is zero.
    pub primary_accent: Rgb,
    pub secondary_accent: Rgb,
    pub accent_alpha: f32,
    /// Alpha for a black layer drawn after the blurred Cover and accent.
    pub dark_overlay_alpha: f32,
    /// Minimum computed contrast across the cache and audited small text colors.
    pub worst_case_contrast: f32,
}

/// Produce bounded tints and a conservative black overlay that keeps every
/// audited small Music text color at 4.5:1 or above for every pixel in the
/// exact blur cache that will be drawn. A component-wise RGB upper bound is
/// used for the fixed overlay solve; the reported contrast is measured once
/// against the actual premultiplied BGRA pixels after all returned layers.
///
/// `palette` is the pair from the renderer's cached `cover_spectrum_palette`.
/// Invalid palettes disable tint but do not invalidate a valid blur cache.
/// Invalid Cover data or a malformed/missing blur cache returns the neutral
/// fallback and requires the renderer to skip drawing that Cover.
///
/// Paint the returned stack as an opaque material with fully visible text to
/// use this guarantee. Any intermediate opacity/fade must preserve contrast in
/// its final composite; a strong black overlay can also constrain an unknown
/// desktop backdrop, but an uncontrolled transition is outside this result.
pub fn album_glass_style(
    cover: Option<CoverImage<'_>>,
    blurred_bgra: Option<&[u8]>,
    palette: Option<[Rgb; 2]>,
    album_color_enabled: bool,
) -> AlbumGlassStyle {
    let Some(_cover) = cover.filter(|cover| valid_cover(*cover)) else {
        return neutral_with_measured_contrast();
    };
    let Some(blurred_bgra) = blurred_bgra.filter(|bytes| bytes.len() == BLUR_BYTES) else {
        return neutral_with_measured_contrast();
    };

    let (primary_accent, secondary_accent, accent_alpha) = if album_color_enabled {
        palette
            .and_then(|[primary, secondary]| {
                Some((
                    bound_accent(primary)?,
                    bound_accent(secondary)?,
                    ACCENT_ALPHA,
                ))
            })
            .unwrap_or((BASE_RGB, BASE_RGB, 0.0))
    } else {
        (BASE_RGB, BASE_RGB, 0.0)
    };
    let accents = [primary_accent, secondary_accent];

    let Some(componentwise_max) = maximum_tinted_background(blurred_bgra, accents, accent_alpha)
    else {
        return neutral_with_measured_contrast();
    };
    let text_luminances = SMALL_TEXT_COLORS.map(relative_luminance);
    let Some(dark_overlay_alpha) = overlay_for_contrast(componentwise_max, text_luminances[2])
    else {
        return neutral_with_measured_contrast();
    };
    let worst_case_contrast = minimum_cache_contrast(
        blurred_bgra,
        accents,
        accent_alpha,
        dark_overlay_alpha,
        text_luminances,
    );
    if !worst_case_contrast.is_finite() || worst_case_contrast + 1e-4 < MIN_SMALL_TEXT_CONTRAST {
        return neutral_with_measured_contrast();
    }

    AlbumGlassStyle {
        use_cover: true,
        primary_accent,
        secondary_accent,
        accent_alpha,
        dark_overlay_alpha,
        worst_case_contrast,
    }
}

fn valid_cover(cover: CoverImage<'_>) -> bool {
    if cover.width == 0
        || cover.height == 0
        || cover.width > MAX_COVER_SIDE
        || cover.height > MAX_COVER_SIDE
    {
        return false;
    }
    let Some(expected) = usize::try_from(cover.width)
        .ok()
        .and_then(|width| {
            usize::try_from(cover.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
    else {
        return false;
    };
    if cover.pixels.len() != expected {
        return false;
    }
    cover.pixels.chunks_exact(4).all(|bgra| {
        let alpha = bgra[3];
        bgra[0] <= alpha && bgra[1] <= alpha && bgra[2] <= alpha
    })
}

fn neutral_with_measured_contrast() -> AlbumGlassStyle {
    AlbumGlassStyle {
        use_cover: false,
        primary_accent: BASE_RGB,
        secondary_accent: BASE_RGB,
        accent_alpha: 0.0,
        dark_overlay_alpha: 0.0,
        worst_case_contrast: minimum_text_contrast(BASE_RGB),
    }
}

fn bound_accent(rgb: Rgb) -> Option<Rgb> {
    if !rgb.iter().all(|channel| channel.is_finite()) {
        return None;
    }
    let rgb = rgb.map(|channel| channel.clamp(0.0, 1.0));
    let (hue, saturation, lightness) = rgb_to_hsl(rgb);
    Some(hsl_to_rgb(
        hue,
        saturation.min(ACCENT_SATURATION_MAX),
        lightness.min(ACCENT_LIGHTNESS_MAX),
    ))
}

fn maximum_tinted_background(cache: &[u8], accents: [Rgb; 2], accent_alpha: f32) -> Option<Rgb> {
    if cache.len() != BLUR_BYTES {
        return None;
    }
    let mut maximum = [0.0_f32; 3];
    for bgra in cache.chunks_exact(4) {
        let alpha = bgra[3];
        if bgra[0] > alpha || bgra[1] > alpha || bgra[2] > alpha {
            return None;
        }
        let background = composite_premultiplied_bgra(bgra, BASE_RGB);
        for accent in accents {
            let tinted = composite_rgb(accent, accent_alpha, background);
            for channel in 0..3 {
                maximum[channel] = maximum[channel].max(tinted[channel]);
            }
        }
    }
    Some(maximum)
}

/// Solve against a component-wise upper bound. Relative luminance is
/// monotone per channel, so this bound safely includes colors assembled from
/// separate pixels. The target uses the darkest small-text foreground and
/// extra margin; correctness does not depend on finding the least overlay.
fn overlay_for_contrast(componentwise_max: Rgb, dim_text_luminance: f32) -> Option<f32> {
    let max_background_luminance =
        |dark_alpha: f32| relative_luminance(after_black_overlay(componentwise_max, dark_alpha));
    let max_allowed_luminance = (dim_text_luminance + 0.05) / CONTRAST_WORKING_TARGET - 0.05;
    if max_allowed_luminance <= 0.0 {
        return None;
    }
    if max_background_luminance(0.0) <= max_allowed_luminance {
        return Some(0.0);
    }
    if max_background_luminance(DARK_OVERLAY_ALPHA_MAX) > max_allowed_luminance {
        return None;
    }

    let (mut low, mut high) = (0.0, DARK_OVERLAY_ALPHA_MAX);
    for _ in 0..OVERLAY_SOLVE_STEPS {
        let middle = (low + high) * 0.5;
        if max_background_luminance(middle) <= max_allowed_luminance {
            high = middle;
        } else {
            low = middle;
        }
    }
    // Headroom covers 8-bit swap-chain quantization and compositor rounding.
    Some((high + 0.01).min(DARK_OVERLAY_ALPHA_MAX))
}

fn minimum_cache_contrast(
    cache: &[u8],
    accents: [Rgb; 2],
    accent_alpha: f32,
    dark_alpha: f32,
    text_luminances: [f32; 3],
) -> f32 {
    let mut minimum = f32::INFINITY;
    for bgra in cache.chunks_exact(4) {
        let background = composite_premultiplied_bgra(bgra, BASE_RGB);
        for accent in accents {
            let tinted = composite_rgb(accent, accent_alpha, background);
            let final_background = after_black_overlay(tinted, dark_alpha);
            minimum = minimum.min(minimum_text_contrast_with_luminances(
                final_background,
                text_luminances,
            ));
        }
    }
    minimum
}

/// Composite premultiplied BGRA over the known opaque neutral backing. This
/// accounts for transparent cover texels instead of treating them as black.
fn composite_premultiplied_bgra(pixel: &[u8], base: Rgb) -> Rgb {
    let alpha = f32::from(pixel[3]) / 255.0;
    let premultiplied = [
        f32::from(pixel[2]) / 255.0,
        f32::from(pixel[1]) / 255.0,
        f32::from(pixel[0]) / 255.0,
    ];
    std::array::from_fn(|index| premultiplied[index] + base[index] * (1.0 - alpha))
}

fn composite_rgb(foreground: Rgb, alpha: f32, background: Rgb) -> Rgb {
    std::array::from_fn(|index| foreground[index] * alpha + background[index] * (1.0 - alpha))
}

fn after_black_overlay(background: Rgb, alpha: f32) -> Rgb {
    background.map(|channel| channel * (1.0 - alpha))
}

fn minimum_text_contrast(background: Rgb) -> f32 {
    minimum_text_contrast_with_luminances(background, SMALL_TEXT_COLORS.map(relative_luminance))
}

fn minimum_text_contrast_with_luminances(background: Rgb, foregrounds: [f32; 3]) -> f32 {
    let background_luminance = relative_luminance(background);
    foregrounds
        .into_iter()
        .map(|foreground_luminance| {
            (foreground_luminance.max(background_luminance) + 0.05)
                / (foreground_luminance.min(background_luminance) + 0.05)
        })
        .fold(f32::INFINITY, f32::min)
}

fn relative_luminance(rgb: Rgb) -> f32 {
    let linear = rgb.map(|channel| {
        let channel = channel.clamp(0.0, 1.0);
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
}

fn rgb_to_hsl([red, green, blue]: Rgb) -> (f32, f32, f32) {
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let lightness = (max + min) * 0.5;
    if max == min {
        return (0.0, 0.0, lightness);
    }
    let delta = max - min;
    let saturation = if lightness > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if max == red {
        (green - blue) / delta + if green < blue { 6.0 } else { 0.0 }
    } else if max == green {
        (blue - red) / delta + 2.0
    } else {
        (red - green) / delta + 4.0
    } * 60.0;
    (hue, saturation, lightness)
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> Rgb {
    if saturation == 0.0 {
        return [lightness; 3];
    }
    let hue = hue.rem_euclid(360.0) / 360.0;
    let q = if lightness < 0.5 {
        lightness * (1.0 + saturation)
    } else {
        lightness + saturation - lightness * saturation
    };
    let p = 2.0 * lightness - q;
    let hue_to_channel = |mut channel: f32| {
        if channel < 0.0 {
            channel += 1.0;
        }
        if channel > 1.0 {
            channel -= 1.0;
        }
        if channel < 1.0 / 6.0 {
            p + (q - p) * 6.0 * channel
        } else if channel < 0.5 {
            q
        } else if channel < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - channel) * 6.0
        } else {
            p
        }
    };
    [
        hue_to_channel(hue + 1.0 / 3.0),
        hue_to_channel(hue),
        hue_to_channel(hue - 1.0 / 3.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        album_glass_style, maximum_tinted_background, relative_luminance, AlbumGlassStyle,
        CoverImage, Rgb, ACCENT_ALPHA, ACCENT_LIGHTNESS_MAX, ACCENT_SATURATION_MAX, BASE_RGB,
        BLUR_BYTES, BLUR_PIXELS, DARK_OVERLAY_ALPHA_MAX, DIM_SMALL_TEXT,
        MAX_STYLE_IMAGE_PIXEL_VISITS, MIN_SMALL_TEXT_CONTRAST, OVERLAY_SOLVE_STEPS,
        PRIMARY_SMALL_TEXT, SECONDARY_SMALL_TEXT,
    };

    struct TestCover {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    }

    impl TestCover {
        fn image(&self) -> CoverImage<'_> {
            CoverImage {
                width: self.width,
                height: self.height,
                pixels: &self.pixels,
            }
        }
    }

    fn opaque_cover(color: [u8; 4], width: u32, height: u32) -> TestCover {
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width * height {
            pixels.extend_from_slice(&color);
        }
        TestCover {
            width,
            height,
            pixels,
        }
    }

    fn cache(color: [u8; 4]) -> Vec<u8> {
        let mut pixels = Vec::with_capacity(BLUR_BYTES);
        for _ in 0..BLUR_BYTES / 4 {
            pixels.extend_from_slice(&color);
        }
        pixels
    }

    fn style_for_cover(color: [u8; 4]) -> (TestCover, Vec<u8>, AlbumGlassStyle) {
        let cover = opaque_cover(color, 2, 2);
        let blur = cache(color);
        let style = album_glass_style(
            Some(cover.image()),
            Some(&blur),
            Some([[0.92, 0.08, 0.04], [0.02, 0.88, 0.12]]),
            true,
        );
        (cover, blur, style)
    }

    /// Independent end-to-end oracle: examine each byte pixel in the exact
    /// cache after the returned layers. This catches a bright isolated texel
    /// that a palette average would conceal.
    fn assert_cache_pixels_meet_contrast(cache: &[u8], style: AlbumGlassStyle) {
        assert!(style.use_cover);
        assert!((0.0..=ACCENT_ALPHA).contains(&style.accent_alpha));
        assert!((0.0..=DARK_OVERLAY_ALPHA_MAX).contains(&style.dark_overlay_alpha));
        let foregrounds: [Rgb; 3] = [PRIMARY_SMALL_TEXT, SECONDARY_SMALL_TEXT, DIM_SMALL_TEXT];
        for bgra in cache.chunks_exact(4) {
            let alpha = f32::from(bgra[3]) / 255.0;
            let background = [
                f32::from(bgra[2]) / 255.0 + BASE_RGB[0] * (1.0 - alpha),
                f32::from(bgra[1]) / 255.0 + BASE_RGB[1] * (1.0 - alpha),
                f32::from(bgra[0]) / 255.0 + BASE_RGB[2] * (1.0 - alpha),
            ];
            for accent in [style.primary_accent, style.secondary_accent] {
                let tinted = [
                    accent[0] * style.accent_alpha + background[0] * (1.0 - style.accent_alpha),
                    accent[1] * style.accent_alpha + background[1] * (1.0 - style.accent_alpha),
                    accent[2] * style.accent_alpha + background[2] * (1.0 - style.accent_alpha),
                ];
                let final_bg = tinted.map(|channel| channel * (1.0 - style.dark_overlay_alpha));
                for foreground in foregrounds {
                    let fg = relative_luminance(foreground);
                    let bg = relative_luminance(final_bg);
                    let contrast = (fg.max(bg) + 0.05) / (fg.min(bg) + 0.05);
                    assert!(
                        contrast + 1e-4 >= MIN_SMALL_TEXT_CONTRAST,
                        "contrast {contrast:.4} below target for cache pixel {bgra:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn black_white_gray_and_saturated_covers_keep_small_text_readable() {
        for color in [
            [0, 0, 0, 255],
            [255, 255, 255, 255],
            [128, 128, 128, 255],
            [16, 4, 240, 255],
        ] {
            let (cover, blur, style) = style_for_cover(color);
            assert!(style.worst_case_contrast >= MIN_SMALL_TEXT_CONTRAST);
            assert_cache_pixels_meet_contrast(&blur, style);
            assert_eq!(cover.width, 2);
        }
    }

    #[test]
    fn one_bright_cache_pixel_is_not_hidden_by_a_dark_cover_average() {
        let cover = opaque_cover([0, 0, 0, 255], 16, 16);
        let mut blur = cache([0, 0, 0, 255]);
        let last = BLUR_BYTES - 4;
        blur[last..].copy_from_slice(&[255, 255, 255, 255]);
        let style = album_glass_style(
            Some(cover.image()),
            Some(&blur),
            Some([[0.03, 0.03, 0.03], [0.10, 0.08, 0.12]]),
            true,
        );
        assert_cache_pixels_meet_contrast(&blur, style);
        assert!(style.dark_overlay_alpha > 0.80);
    }

    #[test]
    fn saturated_palette_is_bounded_and_cannot_break_white_cover_contrast() {
        let (cover, blur, _) = style_for_cover([255, 255, 255, 255]);
        let style = album_glass_style(
            Some(cover.image()),
            Some(&blur),
            Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
            true,
        );
        assert!(style.accent_alpha <= ACCENT_ALPHA);
        for accent in [style.primary_accent, style.secondary_accent] {
            let (saturation, lightness) = independent_hsl_metrics(accent);
            assert!(saturation <= ACCENT_SATURATION_MAX + 1e-4);
            assert!(lightness <= ACCENT_LIGHTNESS_MAX + 1e-4);
        }
        assert_cache_pixels_meet_contrast(&blur, style);
    }

    #[test]
    fn premultiplied_half_transparency_is_composited_over_the_opaque_base() {
        let (cover, blur, style) = style_for_cover([128, 128, 128, 128]);
        assert_cache_pixels_meet_contrast(&blur, style);
        assert!(style.use_cover);
        assert_eq!(cover.pixels[0], 128);
    }

    #[test]
    fn invalid_palette_disables_tint_without_poisoning_valid_blur() {
        let cover = opaque_cover([180, 120, 210, 255], 2, 2);
        let blur = cache([180, 120, 210, 255]);
        let style = album_glass_style(
            Some(cover.image()),
            Some(&blur),
            Some([[f32::NAN, 0.2, 0.3], [0.4, f32::INFINITY, 0.2]]),
            true,
        );
        assert!(style.use_cover);
        assert_eq!(style.accent_alpha, 0.0);
        assert!(style.worst_case_contrast.is_finite());
        assert_cache_pixels_meet_contrast(&blur, style);
    }

    #[test]
    fn disabled_tint_and_missing_cover_use_a_readable_neutral_fallback() {
        let cover = opaque_cover([255, 240, 225, 255], 2, 2);
        let blur = cache([255, 240, 225, 255]);
        let disabled = album_glass_style(
            Some(cover.image()),
            Some(&blur),
            Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
            false,
        );
        assert!(disabled.use_cover);
        assert_eq!(disabled.accent_alpha, 0.0);
        assert!(disabled.worst_case_contrast >= MIN_SMALL_TEXT_CONTRAST);
        assert_cache_pixels_meet_contrast(&blur, disabled);

        let absent = album_glass_style(None, None, None, true);
        assert!(!absent.use_cover);
        assert_eq!(absent.accent_alpha, 0.0);
        assert!(absent.worst_case_contrast >= MIN_SMALL_TEXT_CONTRAST);
    }

    #[test]
    fn malformed_cover_or_blur_fails_closed() {
        let invalid_covers = [
            TestCover {
                width: 0,
                height: 1,
                pixels: vec![],
            },
            TestCover {
                width: 513,
                height: 1,
                pixels: vec![0; 513 * 4],
            },
            TestCover {
                width: 1,
                height: 1,
                pixels: vec![0; 3],
            },
            TestCover {
                width: 1,
                height: 1,
                pixels: vec![0, 0, 200, 100],
            },
        ];
        for invalid in invalid_covers {
            let style = album_glass_style(
                Some(invalid.image()),
                Some(&cache([0, 0, 0, 255])),
                None,
                true,
            );
            assert!(!style.use_cover);
            assert_eq!(style.dark_overlay_alpha, 0.0);
            assert!(style.worst_case_contrast >= MIN_SMALL_TEXT_CONTRAST);
        }

        let cover = opaque_cover([0, 0, 0, 255], 1, 1);
        let mut invalid_premultiplied_cache = cache([0, 0, 0, 255]);
        invalid_premultiplied_cache[..4].copy_from_slice(&[0, 0, 200, 100]);
        for malformed_blur in [vec![], vec![0; BLUR_BYTES - 4], invalid_premultiplied_cache] {
            let style = album_glass_style(Some(cover.image()), Some(&malformed_blur), None, true);
            assert!(!style.use_cover);
            assert!(style.worst_case_contrast >= MIN_SMALL_TEXT_CONTRAST);
        }
    }

    #[test]
    fn componentwise_bound_covers_rgb_peaks_from_different_pixels() {
        // The exact cache has red, green, and blue peaks at separate pixels;
        // no actual texel is white, while their conservative bound is.
        let cover = opaque_cover([0, 0, 0, 255], 2, 2);
        let mut blur = cache([0, 0, 0, 255]);
        blur[0..4].copy_from_slice(&[0, 0, 255, 255]);
        blur[4..8].copy_from_slice(&[0, 255, 0, 255]);
        blur[8..12].copy_from_slice(&[255, 0, 0, 255]);
        let upper = maximum_tinted_background(&blur, [BASE_RGB; 2], 0.0).unwrap();
        assert_eq!(upper, [1.0; 3]);
        assert!(!blur
            .chunks_exact(4)
            .any(|pixel| pixel[..3] == [255, 255, 255]));

        let style = album_glass_style(Some(cover.image()), Some(&blur), None, false);
        assert_cache_pixels_meet_contrast(&blur, style);
        assert!(style.dark_overlay_alpha > 0.80);
    }

    #[test]
    fn per_change_image_work_has_a_fixed_budget() {
        assert_eq!(BLUR_PIXELS, 96 * 96);
        assert_eq!(OVERLAY_SOLVE_STEPS, 24);
        assert_eq!(MAX_STYLE_IMAGE_PIXEL_VISITS, 512 * 512 + 2 * 96 * 96);
    }

    fn independent_hsl_metrics(rgb: Rgb) -> (f32, f32) {
        let max = rgb[0].max(rgb[1]).max(rgb[2]);
        let min = rgb[0].min(rgb[1]).min(rgb[2]);
        let lightness = (max + min) * 0.5;
        let delta = max - min;
        let saturation = if delta == 0.0 {
            0.0
        } else if lightness > 0.5 {
            delta / (2.0 - max - min)
        } else {
            delta / (max + min)
        };
        (saturation, lightness)
    }
}
