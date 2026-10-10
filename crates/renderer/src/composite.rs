use crate::types::Color;
use celesta_composition::{BlendMode, MaskMode};

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
    // A transparent source keeps the destination: the general formula below
    // divides `destination * alpha` by that same alpha. Over nothing, or
    // fully opaque, the source comes out as it is, for the same reason. Most
    // glyph and stroke pixels take one of these paths.
    if source_alpha == 0.0 {
        // Except that a transparent destination comes out all zeros.
        if destination[3] == 0 {
            destination.copy_from_slice(&[0, 0, 0, 0]);
        }
        return;
    }
    if source_alpha == 1.0 || destination[3] == 0 {
        destination.copy_from_slice(&[
            source.red,
            source.green,
            source.blue,
            (source_alpha * 255.0).round() as u8,
        ]);
        return;
    }
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

/// How much of a masked group shows through the straight-alpha mask pixel
/// `matte`, 0 to 1: its alpha, or the Rec. 709 luma of its sRGB-encoded
/// color times its alpha; `1 - m` when inverted.
pub(crate) fn mask_value(mode: MaskMode, invert: bool, matte: &[u8]) -> f64 {
    let alpha = f64::from(matte[3]) / 255.0;
    let value = match mode {
        MaskMode::Alpha => alpha,
        MaskMode::Luminance => {
            let [red, green, blue] = [matte[0], matte[1], matte[2]].map(|c| f64::from(c) / 255.0);
            (0.2126 * red + 0.7152 * green + 0.0722 * blue) * alpha
        }
    };
    if invert { 1.0 - value } else { value }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `blend` without its shortcuts.
    fn blend_exactly(destination: &mut [u8], source: Color, opacity: f64) {
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

    #[test]
    fn shortcuts_match_the_general_formula() {
        let channels = [0, 1, 37, 128, 200, 254, 255];
        for &red in &channels {
            for &alpha in &channels {
                for opacity in [0.0, 0.3, 1.0 / 255.0, 0.999, 1.0] {
                    for destination in [[0, 0, 0, 0], [9, 99, 199, 0], [40, 80, 120, 255]] {
                        let source = Color::rgba(red, 255 - red, red / 2, alpha);
                        let (mut fast, mut exact) = (destination, destination);
                        blend(&mut fast, source, opacity);
                        blend_exactly(&mut exact, source, opacity);
                        assert_eq!(fast, exact, "{source:?} at {opacity} over {destination:?}");
                    }
                }
            }
        }
    }
}
