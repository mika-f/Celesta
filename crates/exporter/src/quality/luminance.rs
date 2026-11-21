//! Relative luminance and contrast ratio, as WCAG defines them.

use std::sync::OnceLock;

/// The linear value of each 8-bit sRGB code.
pub(super) fn linear_table() -> &'static [f32; 256] {
    static TABLE: OnceLock<[f32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0; 256];
        for (code, value) in table.iter_mut().enumerate() {
            *value = linear(code as f64 / 255.0) as f32;
        }
        table
    })
}

/// sRGB's transfer function, inverted: an encoded 0–1 value to linear light.
pub(super) fn linear(encoded: f64) -> f64 {
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear RGB to relative luminance (BT.709 primaries, as sRGB's).
pub(super) fn luminance_of_linear([red, green, blue]: [f64; 3]) -> f64 {
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

/// The relative luminance of an 8-bit sRGB color, 0 for black to 1 for
/// white.
pub fn relative_luminance([red, green, blue]: [u8; 3]) -> f64 {
    let table = linear_table();
    luminance_of_linear([
        f64::from(table[usize::from(red)]),
        f64::from(table[usize::from(green)]),
        f64::from(table[usize::from(blue)]),
    ])
}

/// WCAG's contrast ratio of two relative luminances, from 1 (none) to 21
/// (black on white), whichever is lighter.
pub fn contrast_ratio(a: f64, b: f64) -> f64 {
    let (lighter, darker) = if a > b { (a, b) } else { (b, a) };
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_on_white_is_21_and_a_color_on_itself_is_1() {
        let white = relative_luminance([255, 255, 255]);
        let black = relative_luminance([0, 0, 0]);
        assert!((white - 1.0).abs() < 1e-6);
        assert_eq!(black, 0.0);
        assert!((contrast_ratio(white, black) - 21.0).abs() < 1e-4);
        assert!((contrast_ratio(black, white) - 21.0).abs() < 1e-4);
        assert_eq!(contrast_ratio(0.3, 0.3), 1.0);
    }

    #[test]
    fn matches_published_ratios() {
        // #777777 on white is 4.48:1, just below AA.
        let grey = relative_luminance([0x77, 0x77, 0x77]);
        let ratio = contrast_ratio(grey, 1.0);
        assert!((ratio - 4.48).abs() < 0.01, "{ratio}");
        // #767676 is the lightest grey that passes on white.
        let grey = relative_luminance([0x76, 0x76, 0x76]);
        assert!(contrast_ratio(grey, 1.0) >= 4.5);
    }
}
