//! Contact sheets: selected frames reduced to tiles and laid out on a grid in
//! one image, each labelled with its frame number and timecode.
//!
//! Composited on the CPU so the sheet needs no second render pass: tiles are
//! reduced with an area (box) filter over premultiplied linear-light pixels,
//! which averages every source pixel a tile pixel covers instead of skipping
//! rows and columns, and labels use a small built-in bitmap font.

use celesta_composition::Rational;

/// Space between tiles and around the sheet's edge, in sheet pixels.
pub(crate) const GAP: u32 = 8;
const BACKGROUND: [u8; 3] = [0x20, 0x20, 0x20];
const LABEL_COLOR: [u8; 3] = [0xf0, 0xf0, 0xf0];
/// Transparent frame pixels are shown over a checkerboard of these squares.
const CHECKER: u32 = 8;
const CHECKER_COLORS: [u8; 2] = [0x99, 0x66];

const GLYPH_WIDTH: u32 = 5;
const GLYPH_HEIGHT: u32 = 7;
/// Glyph cell advance in font pixels (one column of spacing).
const GLYPH_ADVANCE: u32 = GLYPH_WIDTH + 1;

/// A tile grid sized for `count` frames of `frame_width`×`frame_height`.
pub(crate) struct Sheet {
    frame_width: u32,
    frame_height: u32,
    columns: u32,
    tile_width: u32,
    tile_height: u32,
    /// Font pixel size in sheet pixels.
    label_scale: u32,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    frame_rate: Rational,
    horizontal: Vec<Taps>,
    vertical: Vec<Taps>,
}

/// One output pixel's source span: the first source index and the
/// normalized weight of each source pixel from there on.
struct Taps {
    first: usize,
    weights: Vec<f32>,
}

impl Sheet {
    /// Lays out the grid; fails if it would exceed `max_side` pixels on
    /// either axis.
    pub(crate) fn new(
        frame_width: u32,
        frame_height: u32,
        count: usize,
        columns: u32,
        tile_width: u32,
        max_side: u32,
        frame_rate: Rational,
    ) -> Result<Self, String> {
        if frame_width == 0 || frame_height == 0 {
            return Err(format!(
                "cannot lay out a contact sheet for {frame_width}x{frame_height} frames"
            ));
        }
        let count = u32::try_from(count).map_err(|_| "too many contact sheet frames")?;
        let columns = columns.min(count).max(1);
        let rows = count.div_ceil(columns);
        let tile_height = ((u64::from(tile_width) * u64::from(frame_height)
            + u64::from(frame_width) / 2)
            / u64::from(frame_width))
        .max(1);
        let label_scale = (tile_width / 160).clamp(1, 4);
        let label_height = u64::from((GLYPH_HEIGHT + 4) * label_scale);
        let width = u64::from(GAP) + u64::from(columns) * (u64::from(tile_width) + u64::from(GAP));
        let height =
            u64::from(GAP) + u64::from(rows) * (tile_height + label_height + u64::from(GAP));
        if width > u64::from(max_side) || height > u64::from(max_side) {
            return Err(format!(
                "a contact sheet of {count} frames in {columns} columns at tile width \
                 {tile_width} would be {width}x{height} pixels; the limit is \
                 {max_side} on each side (use fewer frames, more columns, or a smaller tile width)"
            ));
        }
        let (width, height, tile_height) = (width as u32, height as u32, tile_height as u32);
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width as usize * height as usize {
            pixels.extend_from_slice(&[BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 255]);
        }
        Ok(Self {
            frame_width,
            frame_height,
            columns,
            tile_width,
            tile_height,
            label_scale,
            width,
            height,
            pixels,
            frame_rate,
            horizontal: area_taps(frame_width, tile_width),
            vertical: area_taps(frame_height, tile_height),
        })
    }

    pub(crate) fn width(&self) -> u32 {
        self.width
    }

    pub(crate) fn height(&self) -> u32 {
        self.height
    }

    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Reduces straight-alpha RGBA `frame` of `size` into tile `index` and
    /// labels it with composition frame `frame_number`.
    pub(crate) fn place(
        &mut self,
        index: usize,
        frame_number: u64,
        frame: &[u8],
        size: (u32, u32),
    ) -> Result<(), String> {
        let expected = (self.frame_width, self.frame_height);
        if size != expected || frame.len() != size.0 as usize * size.1 as usize * 4 {
            return Err(format!(
                "frame {frame_number} rendered at {}x{}, but the contact sheet was laid out for {}x{}",
                size.0, size.1, expected.0, expected.1
            ));
        }
        let index = index as u32;
        let (column, row) = (index % self.columns, index / self.columns);
        let label_height = (GLYPH_HEIGHT + 4) * self.label_scale;
        let left = GAP + column * (self.tile_width + GAP);
        let top = GAP + row * (self.tile_height + label_height + GAP);
        self.draw_tile(left, top, frame);

        let timecode = timecode(frame_number, self.frame_rate);
        let full = format!("#{frame_number} {timecode}");
        let short = format!("#{frame_number}");
        let label_top = top + self.tile_height + 2 * self.label_scale;
        // Prefer the full label at a smaller size over only the frame number.
        let fitting = [&full, &short].into_iter().find_map(|text| {
            (1..=self.label_scale)
                .rev()
                .find(|&scale| text_width(text, scale) <= self.tile_width)
                .map(|scale| (text, scale))
        });
        if let Some((text, scale)) = fitting {
            self.draw_text(left, label_top, text, scale);
        }
        Ok(())
    }

    fn draw_tile(&mut self, left: u32, top: u32, frame: &[u8]) {
        let linear = srgb_to_linear_table();
        let frame_width = self.frame_width as usize;
        let frame_height = self.frame_height as usize;
        let tile_width = self.tile_width as usize;
        // Horizontal pass: every source row reduced to the tile's width.
        let mut rows = vec![0f32; frame_height * tile_width * 4];
        for y in 0..frame_height {
            let source = &frame[y * frame_width * 4..(y + 1) * frame_width * 4];
            for (x, taps) in self.horizontal.iter().enumerate() {
                let mut sum = [0f32; 4];
                for (offset, weight) in taps.weights.iter().enumerate() {
                    let pixel = &source[(taps.first + offset) * 4..][..4];
                    let alpha = f32::from(pixel[3]) / 255.0;
                    sum[0] += linear[pixel[0] as usize] * alpha * weight;
                    sum[1] += linear[pixel[1] as usize] * alpha * weight;
                    sum[2] += linear[pixel[2] as usize] * alpha * weight;
                    sum[3] += alpha * weight;
                }
                rows[(y * tile_width + x) * 4..][..4].copy_from_slice(&sum);
            }
        }
        // Vertical pass, then over the checkerboard and back to sRGB.
        for (y, taps) in self.vertical.iter().enumerate() {
            for x in 0..tile_width {
                let mut sum = [0f32; 4];
                for (offset, weight) in taps.weights.iter().enumerate() {
                    let pixel = &rows[((taps.first + offset) * tile_width + x) * 4..][..4];
                    for (total, value) in sum.iter_mut().zip(pixel) {
                        *total += value * weight;
                    }
                }
                let checker =
                    CHECKER_COLORS[((x as u32 / CHECKER + y as u32 / CHECKER) % 2) as usize];
                let behind = linear[checker as usize] * (1.0 - sum[3].clamp(0.0, 1.0));
                let target = ((top as usize + y) * self.width as usize + left as usize + x) * 4;
                let output = &mut self.pixels[target..target + 4];
                for (value, total) in output.iter_mut().zip(&sum[..3]) {
                    *value = linear_to_srgb(total + behind);
                }
                output[3] = 255;
            }
        }
    }

    fn draw_text(&mut self, left: u32, top: u32, text: &str, scale: u32) {
        for (index, character) in text.chars().enumerate() {
            let glyph = glyph(character);
            let glyph_left = left + index as u32 * GLYPH_ADVANCE * scale;
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..GLYPH_WIDTH {
                    if bits & (1 << (GLYPH_WIDTH - 1 - column)) == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let x = glyph_left + column * scale + dx;
                            let y = top + row as u32 * scale + dy;
                            let target = (y as usize * self.width as usize + x as usize) * 4;
                            self.pixels[target..target + 3].copy_from_slice(&LABEL_COLOR);
                        }
                    }
                }
            }
        }
    }
}

/// `HH:MM:SS:FF` for zero-based `frame`: the wall-clock second the frame
/// starts in, and the frame's index within that second (non-drop-frame, so
/// fractional rates such as 30000/1001 count real seconds).
pub(crate) fn timecode(frame: u64, frame_rate: Rational) -> String {
    let numerator = u128::from(frame_rate.numerator.max(1));
    let denominator = u128::from(frame_rate.denominator.max(1));
    let seconds = u128::from(frame) * denominator / numerator;
    let first_of_second = (seconds * numerator).div_ceil(denominator);
    let frames = u128::from(frame) - first_of_second;
    format!(
        "{:02}:{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        frames
    )
}

fn text_width(text: &str, scale: u32) -> u32 {
    let characters = text.chars().count() as u32;
    (characters * GLYPH_ADVANCE).saturating_sub(1) * scale
}

/// Area-filter taps reducing (or enlarging) `source` pixels to `target`:
/// each output pixel covers `source / target` input pixels and weights each
/// by how much of it falls inside that span.
fn area_taps(source: u32, target: u32) -> Vec<Taps> {
    let scale = f64::from(source) / f64::from(target);
    (0..target)
        .map(|index| {
            let start = f64::from(index) * scale;
            let end = (f64::from(index + 1) * scale).min(f64::from(source));
            let first = start.floor() as usize;
            let last = (end.ceil() as usize).clamp(first + 1, source as usize);
            let mut weights: Vec<f32> = (first..last)
                .map(|pixel| {
                    let covered = end.min(pixel as f64 + 1.0) - start.max(pixel as f64);
                    covered.max(0.0) as f32
                })
                .collect();
            let total: f32 = weights.iter().sum();
            if total > 0.0 {
                for weight in &mut weights {
                    *weight /= total;
                }
            } else {
                let even = 1.0 / weights.len() as f32;
                weights.fill(even);
            }
            Taps { first, weights }
        })
        .collect()
}

fn srgb_to_linear_table() -> [f32; 256] {
    std::array::from_fn(|value| {
        let value = value as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    })
}

fn linear_to_srgb(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0 + 0.5) as u8
}

/// 5×7 bitmaps for the characters labels use; each row's low five bits are
/// its pixels, left to right.
fn glyph(character: char) -> [u8; GLYPH_HEIGHT as usize] {
    match character {
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e],
        '6' => [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
        ':' => [0x00, 0x0c, 0x0c, 0x00, 0x0c, 0x0c, 0x00],
        '#' => [0x0a, 0x0a, 0x1f, 0x0a, 0x1f, 0x0a, 0x0a],
        _ => [0; GLYPH_HEIGHT as usize],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timecodes_count_frames_within_each_second() {
        assert_eq!(timecode(0, Rational::new(30, 1)), "00:00:00:00");
        assert_eq!(timecode(29, Rational::new(30, 1)), "00:00:00:29");
        assert_eq!(
            timecode(3_600 * 30 + 61 * 30 + 5, Rational::new(30, 1)),
            "01:01:01:05"
        );
        // 29.97 fps: frame 30 still starts inside the first second.
        assert_eq!(timecode(30, Rational::new(30_000, 1_001)), "00:00:01:00");
        assert_eq!(timecode(29, Rational::new(30_000, 1_001)), "00:00:00:29");
        assert_eq!(timecode(60, Rational::new(30_000, 1_001)), "00:00:02:00");
    }

    #[test]
    fn area_taps_average_every_covered_pixel() {
        let taps = area_taps(4, 2);
        assert_eq!(
            (taps[0].first, taps[0].weights.as_slice()),
            (0, &[0.5, 0.5][..])
        );
        assert_eq!(
            (taps[1].first, taps[1].weights.as_slice()),
            (2, &[0.5, 0.5][..])
        );
        let taps = area_taps(3, 2);
        assert_eq!(taps[0].first, 0);
        assert!((taps[0].weights[0] - 2.0 / 3.0).abs() < 1e-6);
        assert!((taps[0].weights[1] - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(
            area_taps(2, 4).iter().map(|t| t.first).collect::<Vec<_>>(),
            [0, 0, 1, 1]
        );
    }

    #[test]
    fn reduces_with_area_average_and_rejects_oversized_sheets() {
        // A 2x1 black/white frame reduced to one pixel is mid-gray in linear
        // light, not whichever source pixel a decimation would pick.
        let mut sheet = Sheet::new(2, 1, 1, 1, 1, 1_000, Rational::new(1, 1)).unwrap();
        sheet
            .place(0, 0, &[0, 0, 0, 255, 255, 255, 255, 255], (2, 1))
            .unwrap();
        assert!(sheet.place(0, 0, &[0; 4], (1, 1)).is_err());
        let pixel = ((GAP * sheet.width() + GAP) * 4) as usize;
        assert_eq!(&sheet.pixels()[pixel..pixel + 4], &[188, 188, 188, 255]);
        assert!(Sheet::new(16, 9, 400, 5, 4_000, 16_384, Rational::new(30, 1)).is_err());
    }
}
