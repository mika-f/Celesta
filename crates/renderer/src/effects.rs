use crate::composite::blend;
use crate::error::RenderError;
use crate::types::{Color, RgbaFrame, parse_color};

use std::ops::RangeInclusive;

pub(crate) fn premultiply_pixels(pixels: &[u8]) -> Vec<u8> {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| {
            let alpha = f64::from(pixel[3]) / 255.0;
            [
                (f64::from(pixel[0]) * alpha).round() as u8,
                (f64::from(pixel[1]) * alpha).round() as u8,
                (f64::from(pixel[2]) * alpha).round() as u8,
                pixel[3],
            ]
        })
        .collect()
}

pub(crate) fn unpremultiply_color(pixel: &[u8]) -> Color {
    if pixel[3] == 0 {
        return Color::TRANSPARENT;
    }
    let alpha = f64::from(pixel[3]);
    Color::rgba(
        (f64::from(pixel[0]) * 255.0 / alpha)
            .round()
            .clamp(0.0, 255.0) as u8,
        (f64::from(pixel[1]) * 255.0 / alpha)
            .round()
            .clamp(0.0, 255.0) as u8,
        (f64::from(pixel[2]) * 255.0 / alpha)
            .round()
            .clamp(0.0, 255.0) as u8,
        pixel[3],
    )
}

pub(crate) fn blur_pixels(source: &RgbaFrame, radius: f64) -> Vec<u8> {
    let sigma = radius.clamp(0.0, 64.0);
    let input = premultiply_pixels(&source.pixels);
    if sigma == 0.0 {
        return input;
    }
    // Premultiplied, a transparent pixel is all zeros and adds nothing to the
    // pixels around it, so only the box around the visible pixels, widened by
    // the kernel, can come out non-zero. Effect layers are drawn alone into a
    // frame-sized buffer, where that box is usually a small part.
    let Some((left, top, right, bottom)) = visible_bounds(source) else {
        return input;
    };
    let extent = (sigma * 3.0).ceil() as i32;
    let weights: Vec<f64> = (-extent..=extent)
        .map(|i| (-f64::from(i * i) / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    let columns = (left - extent).max(0)..=(right + extent).min(source.width as i32 - 1);
    let rows = (top - extent).max(0)..=(bottom + extent).min(source.height as i32 - 1);
    let pass = |input: &[u8], horizontal: bool, columns: RangeInclusive<i32>, rows| {
        let mut output = vec![0; input.len()];
        for y in rows {
            for x in columns.clone() {
                let mut value = [0.0; 4];
                for (index, weight) in weights.iter().enumerate() {
                    let shift = index as i32 - extent;
                    let (sx, sy) = if horizontal {
                        (x + shift, y)
                    } else {
                        (x, y + shift)
                    };
                    if sx < 0 || sy < 0 || sx >= source.width as i32 || sy >= source.height as i32 {
                        continue;
                    }
                    let offset = ((sy as u32 * source.width + sx as u32) * 4) as usize;
                    for channel in 0..4 {
                        value[channel] += f64::from(input[offset + channel]) * weight / total;
                    }
                }
                let offset = ((y as u32 * source.width + x as u32) * 4) as usize;
                for channel in 0..4 {
                    output[offset + channel] = value[channel].round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        output
    };
    // Across, only the visible rows have anything to spread.
    let across = pass(&input, true, columns.clone(), top..=bottom);
    pass(&across, false, columns, rows)
}

/// The left, top, right, and bottom (inclusive) of the pixels that are not
/// fully transparent, or None if every pixel is.
fn visible_bounds(frame: &RgbaFrame) -> Option<(i32, i32, i32, i32)> {
    let width = frame.width as usize;
    let mut bounds: Option<(i32, i32, i32, i32)> = None;
    for (index, pixel) in frame.pixels.as_chunks::<4>().0.iter().enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let (x, y) = ((index % width) as i32, (index / width) as i32);
        bounds = Some(match bounds {
            None => (x, y, x, y),
            Some((left, top, right, bottom)) => {
                (left.min(x), top.min(y), right.max(x), bottom.max(y))
            }
        });
    }
    bounds
}

pub(crate) fn render_effect_shadow(
    destination: &mut RgbaFrame,
    source: &RgbaFrame,
    color: &str,
    radius: f64,
    offset_x: f64,
    offset_y: f64,
) -> Result<(), RenderError> {
    let tint = parse_color(color)?;
    let blurred = blur_pixels(source, radius);
    for y in 0..source.height as i32 {
        for x in 0..source.width as i32 {
            let destination_offset = ((y as u32 * source.width + x as u32) * 4) as usize;
            let alpha = sample_effect_alpha(
                &blurred,
                source.width,
                source.height,
                f64::from(x) - offset_x,
                f64::from(y) - offset_y,
            );
            blend(
                &mut destination.pixels[destination_offset..destination_offset + 4],
                tint,
                alpha / 255.0,
            );
        }
    }
    Ok(())
}

pub(crate) fn sample_effect_alpha(pixels: &[u8], width: u32, height: u32, x: f64, y: f64) -> f64 {
    if x <= -1.0 || y <= -1.0 || x >= f64::from(width) || y >= f64::from(height) {
        return 0.0;
    }
    let left = x.floor() as i32;
    let top = y.floor() as i32;
    let fx = x - f64::from(left);
    let fy = y - f64::from(top);
    let alpha = |x: i32, y: i32| {
        if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
            0.0
        } else {
            f64::from(pixels[((y as u32 * width + x as u32) * 4 + 3) as usize])
        }
    };
    let upper = alpha(left, top) * (1.0 - fx) + alpha(left + 1, top) * fx;
    let lower = alpha(left, top + 1) * (1.0 - fx) + alpha(left + 1, top + 1) * fx;
    upper * (1.0 - fy) + lower * fy
}

#[cfg(test)]
mod blur_tests {
    use super::{blur_pixels, premultiply_pixels};
    use crate::types::RgbaFrame;

    /// Blurs every pixel of the frame, transparent or not.
    fn blur_every_pixel(source: &RgbaFrame, radius: f64) -> Vec<u8> {
        let sigma = radius.clamp(0.0, 64.0);
        let input = premultiply_pixels(&source.pixels);
        if sigma == 0.0 {
            return input;
        }
        let extent = (sigma * 3.0).ceil() as i32;
        let weights: Vec<f64> = (-extent..=extent)
            .map(|i| (-f64::from(i * i) / (2.0 * sigma * sigma)).exp())
            .collect();
        let total: f64 = weights.iter().sum();
        let (width, height) = (source.width as i32, source.height as i32);
        let pass = |input: &[u8], horizontal: bool| {
            let mut output = vec![0; input.len()];
            for y in 0..height {
                for x in 0..width {
                    let mut value = [0.0; 4];
                    for (index, weight) in weights.iter().enumerate() {
                        let shift = index as i32 - extent;
                        let (sx, sy) = if horizontal {
                            (x + shift, y)
                        } else {
                            (x, y + shift)
                        };
                        if sx < 0 || sy < 0 || sx >= width || sy >= height {
                            continue;
                        }
                        let offset = ((sy * width + sx) * 4) as usize;
                        for channel in 0..4 {
                            value[channel] += f64::from(input[offset + channel]) * weight / total;
                        }
                    }
                    let offset = ((y * width + x) * 4) as usize;
                    for channel in 0..4 {
                        output[offset + channel] = value[channel].round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
            output
        };
        pass(&pass(&input, true), false)
    }

    #[test]
    fn matches_blurring_every_pixel() {
        let mut seed = 12345_u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            seed
        };
        for (width, height) in [(1, 1), (1, 9), (9, 1), (17, 11), (40, 23), (64, 48)] {
            // Mostly empty, like a layer drawn alone into a frame-sized
            // buffer, with color left in some fully transparent pixels.
            let scattered: Vec<u8> = (0..width * height)
                .flat_map(|_| {
                    let opaque = next() >> 29 == 0;
                    let [red, green, blue, alpha] = next().to_le_bytes();
                    [red, green, blue, if opaque { alpha.max(1) } else { 0 }]
                })
                .collect();
            let mut corner = vec![0; (width * height * 4) as usize];
            corner[..4].copy_from_slice(&[255, 128, 0, 255]);
            let mut far_corner = vec![0; (width * height * 4) as usize];
            let last = far_corner.len() - 4;
            far_corner[last..].copy_from_slice(&[10, 200, 90, 77]);
            let empty = vec![0; (width * height * 4) as usize];
            let full = vec![200; (width * height * 4) as usize];
            for pixels in [scattered, corner, far_corner, empty, full] {
                let source = RgbaFrame {
                    width,
                    height,
                    pixels,
                };
                for radius in [0.0, 0.3, 1.0, 2.5, 7.0, 30.0, 100.0] {
                    assert_eq!(
                        blur_pixels(&source, radius),
                        blur_every_pixel(&source, radius),
                        "{width}x{height} by {radius}"
                    );
                }
            }
        }
    }
}
