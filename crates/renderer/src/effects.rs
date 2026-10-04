use crate::composite::blend;
use crate::error::RenderError;
use crate::types::{Color, RgbaFrame};

pub(crate) fn premultiply_pixels(pixels: &[u8]) -> Vec<u8> {
    pixels
        .chunks_exact(4)
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
    let extent = (sigma * 3.0).ceil() as i32;
    let weights: Vec<f64> = (-extent..=extent)
        .map(|i| (-f64::from(i * i) / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    let pass = |input: &[u8], horizontal: bool| {
        let mut output = vec![0; input.len()];
        for y in 0..source.height as i32 {
            for x in 0..source.width as i32 {
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
    pass(&pass(&input, true), false)
}

pub(crate) fn render_effect_shadow(
    destination: &mut RgbaFrame,
    source: &RgbaFrame,
    color: &str,
    radius: f64,
    offset_x: f64,
    offset_y: f64,
) -> Result<(), RenderError> {
    let tint = Color::from_hex(color)?;
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
