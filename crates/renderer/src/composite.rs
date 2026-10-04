use crate::types::{Color, RgbaFrame};
use celesta_composition::BlendMode;

/// Composites a same-size coverage `mask` onto `frame`, coloring each pixel
/// with `color_at(x, y)`.
pub(crate) fn composite_mask_with(
    frame: &mut RgbaFrame,
    mask: &[u8],
    width: u32,
    color_at: impl Fn(u32, u32) -> Color,
) {
    for (index, &alpha) in mask.iter().enumerate() {
        if alpha == 0 {
            continue;
        }
        let (x, y) = (index as u32 % width, index as u32 / width);
        let offset = index * 4;
        blend(
            &mut frame.pixels[offset..offset + 4],
            color_at(x, y),
            f64::from(alpha) / 255.0,
        );
    }
}

/// Like `composite_mask`, but `source` already carries its own per-pixel RGBA
/// (used for glyph rendering, where a color emoji glyph's pixels vary in hue
/// across the glyph, not just coverage).
pub(crate) fn composite_rgba(
    frame: &mut RgbaFrame,
    source: &[u8],
    width: u32,
    height: u32,
    left: i32,
    top: i32,
) {
    for y in 0..height {
        for x in 0..width {
            let destination_x = left + x as i32;
            let destination_y = top + y as i32;
            if destination_x < 0
                || destination_y < 0
                || destination_x >= frame.width as i32
                || destination_y >= frame.height as i32
            {
                continue;
            }
            let source_offset = ((y * width + x) * 4) as usize;
            let alpha = source[source_offset + 3];
            if alpha == 0 {
                continue;
            }
            let destination_offset =
                ((destination_y as u32 * frame.width + destination_x as u32) * 4) as usize;
            let color = Color::rgba(
                source[source_offset],
                source[source_offset + 1],
                source[source_offset + 2],
                alpha,
            );
            blend(
                &mut frame.pixels[destination_offset..destination_offset + 4],
                color,
                1.0,
            );
        }
    }
}

/// Composites `source` onto `destination` (both non-premultiplied) through
/// `mode`: the source color becomes `(1 - ab) * Cs + ab * B(Cb, Cs)`, which
/// then composites source-over, as in the W3C Compositing and Blending spec.
pub(crate) fn blend_with_mode(
    destination: &mut [u8],
    source: Color,
    opacity: f64,
    mode: BlendMode,
) {
    if mode.is_normal() {
        return blend(destination, source, opacity);
    }
    blend_mixed(destination, source, opacity, |backdrop, source| {
        mode.blend_channel(backdrop, source)
    });
}

/// Source-over compositing with a separable mixing function `mix(backdrop,
/// source)` per channel, all in 0–1 (W3C Compositing, "simple alpha
/// compositing" with blending).
pub(crate) fn blend_mixed(
    destination: &mut [u8],
    source: Color,
    opacity: f64,
    mix: impl Fn(f64, f64) -> f64,
) {
    let source_alpha = (f64::from(source.alpha) / 255.0) * opacity.clamp(0.0, 1.0);
    if source_alpha == 0.0 {
        return;
    }
    let backdrop_alpha = f64::from(destination[3]) / 255.0;
    let output_alpha = source_alpha + backdrop_alpha * (1.0 - source_alpha);
    for channel in 0..3 {
        let source_value = f64::from([source.red, source.green, source.blue][channel]) / 255.0;
        let backdrop_value = f64::from(destination[channel]) / 255.0;
        let mixed = (1.0 - backdrop_alpha) * source_value
            + backdrop_alpha * mix(backdrop_value, source_value);
        let output = (source_alpha * mixed
            + backdrop_alpha * backdrop_value * (1.0 - source_alpha))
            / output_alpha;
        destination[channel] = (output * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    destination[3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

pub(crate) fn blend(destination: &mut [u8], source: Color, opacity: f64) {
    let source_alpha = (f64::from(source.alpha) / 255.0) * opacity.clamp(0.0, 1.0);
    let destination_alpha = f64::from(destination[3]) / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if output_alpha == 0.0 {
        destination.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    for channel in 0..3 {
        let source_value = f64::from([source.red, source.green, source.blue][channel]);
        let destination_value = f64::from(destination[channel]);
        let output = (source_value * source_alpha
            + destination_value * destination_alpha * (1.0 - source_alpha))
            / output_alpha;
        destination[channel] = output.round().clamp(0.0, 255.0) as u8;
    }
    destination[3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// The per-channel mixing function `B(backdrop, source)` of a PSD layer's
/// blend mode, all in 0–1, for the modes that blend each channel on its own.
/// `None` for normal, and for the modes drawn as normal because they mix
/// whole colors (hue, saturation, color, luminosity, darker and lighter
/// color) or noise (dissolve).
pub(crate) fn psd_blend_channel(mode: &str) -> Option<fn(f64, f64) -> f64> {
    fn screen(b: f64, s: f64) -> f64 {
        b + s - b * s
    }
    fn color_burn(b: f64, s: f64) -> f64 {
        if b >= 1.0 {
            1.0
        } else if s <= 0.0 {
            0.0
        } else {
            1.0 - ((1.0 - b) / s).min(1.0)
        }
    }
    fn color_dodge(b: f64, s: f64) -> f64 {
        if b <= 0.0 {
            0.0
        } else if s >= 1.0 {
            1.0
        } else {
            (b / (1.0 - s)).min(1.0)
        }
    }
    fn hard_light(b: f64, s: f64) -> f64 {
        if s <= 0.5 {
            b * 2.0 * s
        } else {
            screen(b, 2.0 * s - 1.0)
        }
    }
    fn soft_light(b: f64, s: f64) -> f64 {
        if s <= 0.5 {
            b - (1.0 - 2.0 * s) * b * (1.0 - b)
        } else {
            let d = if b <= 0.25 {
                ((16.0 * b - 12.0) * b + 4.0) * b
            } else {
                b.sqrt()
            };
            b + (2.0 * s - 1.0) * (d - b)
        }
    }
    fn vivid_light(b: f64, s: f64) -> f64 {
        if s <= 0.5 {
            color_burn(b, 2.0 * s)
        } else {
            color_dodge(b, 2.0 * s - 1.0)
        }
    }
    fn pin_light(b: f64, s: f64) -> f64 {
        if s <= 0.5 {
            b.min(2.0 * s)
        } else {
            b.max(2.0 * s - 1.0)
        }
    }

    Some(match mode {
        "Darken" => f64::min,
        "Multiply" => |b, s| b * s,
        "ColorBurn" => color_burn,
        "LinearBurn" => |b, s| (b + s - 1.0).max(0.0),
        "Lighten" => f64::max,
        "Screen" => screen,
        "ColorDodge" => color_dodge,
        "LinearDodge" => |b, s| (b + s).min(1.0),
        "Overlay" => |b, s| hard_light(s, b),
        "SoftLight" => soft_light,
        "HardLight" => hard_light,
        "VividLight" => vivid_light,
        "LinearLight" => |b, s| (b + 2.0 * s - 1.0).clamp(0.0, 1.0),
        "PinLight" => pin_light,
        "HardMix" => |b, s| if b + s >= 1.0 { 1.0 } else { 0.0 },
        "Difference" => |b, s| (b - s).abs(),
        "Exclusion" => |b, s| b + s - 2.0 * b * s,
        "Subtract" => |b, s| (b - s).max(0.0),
        "Divide" => |b, s| {
            if s <= 0.0 {
                if b <= 0.0 { 0.0 } else { 1.0 }
            } else {
                (b / s).min(1.0)
            }
        },
        // PassThrough, Normal, Dissolve, DarkerColor, LighterColor, Hue,
        // Saturation, Color, Luminosity.
        _ => return None,
    })
}
