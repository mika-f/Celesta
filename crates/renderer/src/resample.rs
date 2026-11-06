//! CPU resampling of straight-alpha RGBA shared by the renderers and tools.
//!
//! Two kernels live here, each kept because its callers need exactly its
//! output:
//!
//! - [`resize_rgba`], a triangle filter with alpha-weighted color in `f32`,
//!   for any size change (image and PSD fitting, GPU texture limits).
//! - [`box_downscale`], an integer box average over the even partition
//!   [`box_span`] gives, with [`average_block`] as its per-pixel kernel (GPU
//!   mip levels, MCP inline previews). PSD layer reduction reuses
//!   [`average_block`] over its own canvas-aligned blocks.
//!
//! `image::imageops::resize` does not weight color by alpha, so opaque white
//! beside transparent black shrinks to grey there; it replaces neither.
//! The exporter's contact sheet keeps its own area filter, which averages in
//! linear light over a checkerboard (see `contact_sheet::area_taps`).

use image::RgbaImage;
use std::{collections::VecDeque, ops::Range};

/// How [`average_block`] weighs each pixel's color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coverage {
    /// Color weighted by alpha, so transparent pixels do not darken the
    /// edges they are averaged into. A fully transparent block is
    /// transparent black.
    Alpha,
    /// Every channel averaged on its own, as straight values. Gives the same
    /// bytes as [`Coverage::Alpha`] wherever the block is opaque.
    Straight,
}

/// The source pixels output pixel `index` of `to` covers out of `from`
/// (`to` at most `from`): an even integer partition of `0..from`, where a
/// block that would be empty takes the one pixel at its start.
pub fn box_span(index: u32, from: u32, to: u32) -> Range<u32> {
    let edge = |index: u32| (u64::from(index) * u64::from(from) / u64::from(to)) as u32;
    let start = edge(index);
    start..edge(index + 1).max(start + 1).min(from)
}

/// The rounded average of the straight RGBA `pixels` (rows `width` pixels
/// long) in `rows` x `columns`, weighting color by `coverage`. Alpha is
/// always the plain average.
#[inline]
pub fn average_block(
    pixels: &[u8],
    width: u32,
    rows: Range<u32>,
    columns: Range<u32>,
    coverage: Coverage,
) -> [u8; 4] {
    let mut color = [0_u64; 3];
    let mut alpha = 0_u64;
    let mut weights = 0_u64;
    for y in rows.clone() {
        let row = y as usize * width as usize;
        let start = (row + columns.start as usize) * 4;
        let end = (row + columns.end as usize) * 4;
        for pixel in pixels[start..end].as_chunks::<4>().0 {
            let opacity = u64::from(pixel[3]);
            let weight = match coverage {
                Coverage::Alpha => opacity,
                Coverage::Straight => 1,
            };
            for channel in 0..3 {
                color[channel] += u64::from(pixel[channel]) * weight;
            }
            alpha += opacity;
            weights += weight;
        }
    }
    let count = u64::from(rows.end - rows.start) * u64::from(columns.end - columns.start);
    let average = |sum: u64, count: u64| (sum + count / 2).checked_div(count).unwrap_or(0) as u8;
    [
        average(color[0], weights),
        average(color[1], weights),
        average(color[2], weights),
        average(alpha, count),
    ]
}

/// Shrinks straight RGBA `pixels` (`width` x `height`, which they must
/// match) to `to_width` x `to_height`, neither larger than its source side:
/// each output pixel is the [`average_block`] of its [`box_span`] on both
/// axes.
pub fn box_downscale(
    width: u32,
    height: u32,
    pixels: &[u8],
    to_width: u32,
    to_height: u32,
    coverage: Coverage,
) -> Vec<u8> {
    assert_eq!(
        pixels.len(),
        width as usize * height as usize * 4,
        "pixels match their size"
    );
    assert!(
        (1..=width).contains(&to_width) && (1..=height).contains(&to_height),
        "box_downscale only shrinks"
    );
    let columns: Vec<_> = (0..to_width)
        .map(|x| box_span(x, width, to_width))
        .collect();
    let mut output = Vec::with_capacity(to_width as usize * to_height as usize * 4);
    for y in 0..to_height {
        let rows = box_span(y, height, to_height);
        for columns in &columns {
            output.extend(average_block(
                pixels,
                width,
                rows.clone(),
                columns.clone(),
                coverage,
            ));
        }
    }
    output
}

/// Resamples straight RGBA `pixels` (`width` x `height`, which they must
/// match) to `to_width` x `to_height` with a triangle filter. Colors are
/// weighted by alpha so transparent texels do not darken the edges they are
/// averaged into, and are kept in `f32` until the end so translucent texels
/// keep their hue.
///
/// Rows are resampled across first and kept only while the output rows
/// being filled read them, so a large source needs no full-size buffer
/// besides the result.
pub fn resize_rgba(
    width: u32,
    height: u32,
    pixels: &[u8],
    to_width: u32,
    to_height: u32,
) -> RgbaImage {
    assert_eq!(
        pixels.len(),
        width as usize * height as usize * 4,
        "pixels match their size"
    );
    let columns = triangle_taps(width, to_width);
    let mut rows: VecDeque<(usize, Vec<[f32; 4]>)> = VecDeque::new();
    let mut next_row = 0;
    let mut output = Vec::with_capacity(to_width as usize * to_height as usize * 4);
    for (first, weights) in triangle_taps(height, to_height) {
        while rows.front().is_some_and(|(row, _)| *row < first) {
            rows.pop_front();
        }
        next_row = next_row.max(first);
        while next_row < first + weights.len() {
            let source = &pixels[next_row * width as usize * 4..][..width as usize * 4];
            let row = columns
                .iter()
                .map(|(first, weights)| {
                    let mut sum = [0.0f32; 4];
                    for (pixel, weight) in
                        source[first * 4..].as_chunks::<4>().0.iter().zip(weights)
                    {
                        let alpha = f32::from(pixel[3]) * weight;
                        for channel in 0..3 {
                            sum[channel] += f32::from(pixel[channel]) * alpha;
                        }
                        sum[3] += alpha;
                    }
                    sum
                })
                .collect();
            rows.push_back((next_row, row));
            next_row += 1;
        }
        let offset = first - rows.front().expect("the rows read are kept").0;
        for column in 0..to_width as usize {
            let mut sum = [0.0f32; 4];
            for ((_, row), weight) in rows.iter().skip(offset).zip(&weights) {
                for channel in 0..4 {
                    sum[channel] += row[column][channel] * weight;
                }
            }
            let alpha = sum[3];
            if alpha <= 0.0 {
                output.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            for channel in &sum[..3] {
                output.push((channel / alpha).round().clamp(0.0, 255.0) as u8);
            }
            output.push(alpha.round().clamp(0.0, 255.0) as u8);
        }
    }
    RgbaImage::from_raw(to_width, to_height, output).expect("output matches its size")
}

/// For each of `to` output pixels along an axis of `from` source pixels,
/// the first source pixel it reads and the normalized triangle weights of
/// it and the ones after it. Shrinking widens the triangle to cover every
/// source pixel; enlarging interpolates between neighbors.
fn triangle_taps(from: u32, to: u32) -> Vec<(usize, Vec<f32>)> {
    let scale = f64::from(from) / f64::from(to);
    let support = scale.max(1.0);
    (0..to)
        .map(|index| {
            let center = (f64::from(index) + 0.5) * scale - 0.5;
            let first = ((center - support).floor() + 1.0).max(0.0) as usize;
            let last = ((center + support).ceil() - 1.0).min(f64::from(from) - 1.0) as usize;
            let mut weights: Vec<f32> = (first..=last.max(first))
                .map(|source| (1.0 - (source as f64 - center).abs() / support).max(0.0) as f32)
                .collect();
            let total: f32 = weights.iter().sum();
            for weight in &mut weights {
                *weight /= total;
            }
            (first, weights)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_spans_partition_every_source_pixel_once() {
        for from in 1..40 {
            for to in 1..=from {
                let spans: Vec<_> = (0..to).map(|index| box_span(index, from, to)).collect();
                assert_eq!(spans[0].start, 0, "{from} -> {to}");
                assert_eq!(spans[to as usize - 1].end, from, "{from} -> {to}");
                for pair in spans.windows(2) {
                    assert_eq!(pair[0].end, pair[1].start, "{from} -> {to}");
                }
                assert!(spans.iter().all(|span| !span.is_empty()));
            }
        }
        // A halved odd size folds its last pixel into the block before it.
        assert_eq!(box_span(1, 5, 2), 2..5);
    }

    #[test]
    fn alpha_coverage_keeps_the_color_next_to_transparency() {
        let pixels = [255, 255, 255, 255, 0, 0, 0, 0];
        assert_eq!(
            average_block(&pixels, 2, 0..1, 0..2, Coverage::Alpha),
            [255, 255, 255, 128]
        );
        assert_eq!(
            average_block(&pixels, 2, 0..1, 0..2, Coverage::Straight),
            [128, 128, 128, 128]
        );
        // Fully transparent blocks are transparent black either way.
        let clear = [9, 9, 9, 0].repeat(4);
        for coverage in [Coverage::Alpha, Coverage::Straight] {
            let block = average_block(&clear, 2, 0..2, 0..2, coverage);
            assert_eq!(block[3], 0);
        }
        assert_eq!(
            average_block(&clear, 2, 0..2, 0..2, Coverage::Alpha),
            [0, 0, 0, 0]
        );
    }

    #[test]
    fn coverages_agree_on_opaque_blocks() {
        // Every channel sum of 1 to 12 opaque pixels rounds the same way.
        for count in 1..=12_u32 {
            for sum in 0..=255 * count {
                let pixels: Vec<u8> = (0..count)
                    .flat_map(|index| {
                        let value = (sum / count + u32::from(index < sum % count)) as u8;
                        [value, value, value, 255]
                    })
                    .collect();
                let alpha = average_block(&pixels, count, 0..1, 0..count, Coverage::Alpha);
                let straight = average_block(&pixels, count, 0..1, 0..count, Coverage::Straight);
                assert_eq!(alpha, straight, "{count} pixels summing to {sum}");
            }
        }
    }

    #[test]
    fn box_downscale_averages_in_integers() {
        // Opaque black beside white averages to 128 here; the contact sheet,
        // which averages in linear light, gives 188 instead.
        let pixels = [0, 0, 0, 255, 255, 255, 255, 255];
        assert_eq!(
            box_downscale(2, 1, &pixels, 1, 1, Coverage::Alpha),
            [128, 128, 128, 255]
        );
        // 5 x 3 to 2 x 1: blocks of 2 x 3 and 3 x 3 pixels.
        let pixels: Vec<u8> = (0..15_u8).flat_map(|i| [i * 10, 0, 0, 255]).collect();
        // Columns 0..2 of the reds 0..=140 sum to 330 over 6 pixels, columns
        // 2..5 to 720 over 9.
        assert_eq!(
            box_downscale(5, 3, &pixels, 2, 1, Coverage::Straight),
            [55, 0, 0, 255, 80, 0, 0, 255]
        );
    }

    #[test]
    fn resizing_does_not_darken_edges_next_to_transparency() {
        // Opaque white beside fully transparent black.
        let pixels = [[255, 255, 255, 255], [0, 0, 0, 0]].repeat(2).concat();
        let resized = resize_rgba(2, 2, &pixels, 1, 1);
        let pixel = resized.get_pixel(0, 0).0;
        assert_eq!(&pixel[..3], &[255, 255, 255]);
        assert!((100..=155).contains(&pixel[3]));
    }

    #[test]
    fn resizing_keeps_the_hue_of_translucent_pixels() {
        let pixels = [64, 128, 192, 1].repeat(16);
        let resized = resize_rgba(4, 4, &pixels, 3, 2);
        assert!(resized.pixels().all(|pixel| pixel.0 == [64, 128, 192, 1]));
    }

    #[test]
    fn resizing_averages_what_each_output_pixel_covers() {
        // A 4-pixel ramp halved: each output pixel weighs its two source
        // pixels 0.75 and the next one inward 0.25; the one past the edge
        // is missing, so the weights are renormalized over 1.75.
        let pixels = [0u8, 60, 120, 180]
            .iter()
            .flat_map(|&value| [value, value, value, 255])
            .collect::<Vec<_>>();
        let resized = resize_rgba(4, 1, &pixels, 2, 1);
        assert_eq!(resized.get_pixel(0, 0).0, [43, 43, 43, 255]);
        assert_eq!(resized.get_pixel(1, 0).0, [137, 137, 137, 255]);
    }
}
