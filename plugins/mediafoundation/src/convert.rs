//! RGB to the YUV the encoders take: BT.709 limited range, 4:2:0, with
//! straight alpha ignored.
//!
//! Per pixel, with R, G, B in [0, 1]: `Y' = 0.2126 R + 0.7152 G + 0.0722 B`,
//! `Y = 16 + 219 Y'`, `Cb = 128 + 224 (B - Y') / 1.8556`, and
//! `Cr = 128 + 224 (R - Y') / 1.5748`, all four times as large for 10
//! bits. Chroma is the mean of its 2x2 block's Cb and Cr, rounded once.

/// Y, Cb, and Cr of one pixel, in 8-bit codes (as floats).
fn yuv([r, g, b]: [f64; 3]) -> [f64; 3] {
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    [
        16.0 + 219.0 * y,
        128.0 + 224.0 * (b - y) / 1.8556,
        128.0 + 224.0 * (r - y) / 1.5748,
    ]
}

/// The planes of a frame as codes `scale` times the 8-bit ones: `width` ×
/// `height` luma, then each 2x2 block's Cb and Cr interleaved. A block at
/// an odd edge has the pixels it has.
fn planes(
    width: u32,
    height: u32,
    scale: f64,
    pixel: impl Fn(usize, usize) -> [f64; 3],
) -> (Vec<u16>, Vec<u16>) {
    let (width, height) = (width as usize, height as usize);
    let code = |value: f64| (value * scale).round().clamp(0.0, 65535.0) as u16;
    let mut luma = Vec::with_capacity(width * height);
    let mut chroma = Vec::with_capacity(width.div_ceil(2) * height.div_ceil(2) * 2);
    for y in 0..height {
        for x in 0..width {
            luma.push(code(yuv(pixel(x, y))[0]));
        }
    }
    for block_y in (0..height).step_by(2) {
        for block_x in (0..width).step_by(2) {
            let (mut cb, mut cr, mut count) = (0.0, 0.0, 0.0);
            for y in block_y..(block_y + 2).min(height) {
                for x in block_x..(block_x + 2).min(width) {
                    let [_, pixel_cb, pixel_cr] = yuv(pixel(x, y));
                    cb += pixel_cb;
                    cr += pixel_cr;
                    count += 1.0;
                }
            }
            chroma.push(code(cb / count));
            chroma.push(code(cr / count));
        }
    }
    (luma, chroma)
}

/// NV12 (8-bit Y, then interleaved Cb and Cr) from RGBA8. `frame` must be
/// `width × height × 4` bytes.
pub(crate) fn rgba8_to_nv12(frame: &[u8], width: u32, height: u32) -> Vec<u8> {
    assert_eq!(
        frame.len(),
        width as usize * height as usize * 4,
        "an RGBA8 frame of {width}x{height}"
    );
    let (luma, chroma) = planes(width, height, 1.0, |x, y| {
        let at = (y * width as usize + x) * 4;
        [0, 1, 2].map(|c| f64::from(frame[at + c]) / 255.0)
    });
    luma.into_iter().chain(chroma).map(|c| c as u8).collect()
}

/// P010 (10-bit codes in the high bits of little-endian 16-bit samples: Y,
/// then interleaved Cb and Cr) from RGBA16. `frame` must be
/// `width × height × 8` bytes.
pub(crate) fn rgba16_to_p010(frame: &[u8], width: u32, height: u32) -> Vec<u8> {
    assert_eq!(
        frame.len(),
        width as usize * height as usize * 8,
        "an RGBA16 frame of {width}x{height}"
    );
    let (luma, chroma) = planes(width, height, 4.0, |x, y| {
        let at = (y * width as usize + x) * 8;
        [0, 1, 2].map(|c| {
            f64::from(u16::from_le_bytes([
                frame[at + 2 * c],
                frame[at + 2 * c + 1],
            ])) / 65535.0
        })
    });
    luma.into_iter()
        .chain(chroma)
        .flat_map(|code| (code << 6).to_le_bytes())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_media::PixelFormat;

    fn rgba8(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .flat_map(|(x, y)| pixel(x, y))
            .collect()
    }

    fn rgba16(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u16; 4]) -> Vec<u8> {
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .flat_map(|(x, y)| pixel(x, y))
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    fn samples(bytes: &[u8]) -> Vec<u16> {
        bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect()
    }

    #[test]
    fn red_is_bt709_limited() {
        let red = rgba8(2, 2, |_, _| [255, 0, 0, 255]);
        assert_eq!(rgba8_to_nv12(&red, 2, 2), [63, 63, 63, 63, 102, 240]);

        let red = rgba16(2, 2, |_, _| [0xFFFF, 0, 0, 0xFFFF]);
        assert_eq!(
            samples(&rgba16_to_p010(&red, 2, 2)),
            [250 << 6, 250 << 6, 250 << 6, 250 << 6, 409 << 6, 960 << 6]
        );
    }

    #[test]
    fn black_and_white_are_the_limits() {
        let black = rgba8(2, 2, |_, _| [0, 0, 0, 255]);
        assert_eq!(rgba8_to_nv12(&black, 2, 2), [16, 16, 16, 16, 128, 128]);
        let white = rgba8(2, 2, |_, _| [255, 255, 255, 255]);
        assert_eq!(rgba8_to_nv12(&white, 2, 2), [235, 235, 235, 235, 128, 128]);
    }

    #[test]
    fn chroma_averages_the_block() {
        // Two red and two blue: the mean of red's (102.34, 240) and blue's
        // (240, 114.3) before rounding, not of their rounded codes.
        let mixed = rgba8(2, 2, |x, _| {
            if x == 0 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        });
        let nv12 = rgba8_to_nv12(&mixed, 2, 2);
        assert_eq!(nv12[..4], [63, 32, 63, 32]);
        let blue_cb: f64 = 128.0 + 224.0 * (1.0 - 0.0722) / 1.8556;
        let red_cb: f64 = 128.0 + 224.0 * (0.0 - 0.2126) / 1.8556;
        let blue_cr: f64 = 128.0 + 224.0 * (0.0 - 0.0722) / 1.5748;
        let red_cr: f64 = 128.0 + 224.0 * (1.0 - 0.2126) / 1.5748;
        assert_eq!(f64::from(nv12[4]), ((blue_cb + red_cb) / 2.0).round());
        assert_eq!(f64::from(nv12[5]), ((blue_cr + red_cr) / 2.0).round());
    }

    #[test]
    fn alpha_is_ignored() {
        let opaque = rgba8(4, 2, |x, y| [x as u8 * 60, y as u8 * 100, 90, 255]);
        let clear = rgba8(4, 2, |x, y| [x as u8 * 60, y as u8 * 100, 90, 0]);
        assert_eq!(rgba8_to_nv12(&opaque, 4, 2), rgba8_to_nv12(&clear, 4, 2));
        let opaque = rgba16(4, 2, |x, y| {
            [x as u16 * 9000, y as u16 * 20000, 900, 0xFFFF]
        });
        let clear = rgba16(4, 2, |x, y| {
            [x as u16 * 9000, y as u16 * 20000, 900, 0x1234]
        });
        assert_eq!(rgba16_to_p010(&opaque, 4, 2), rgba16_to_p010(&clear, 4, 2));
    }

    #[test]
    fn sizes() {
        let frame = rgba8(64, 36, |x, y| [x as u8, y as u8, 7, 255]);
        assert_eq!(
            rgba8_to_nv12(&frame, 64, 36).len(),
            PixelFormat::Nv12.frame_bytes(64, 36)
        );
        let frame = rgba16(64, 36, |x, y| [x as u16, y as u16, 7, 0xFFFF]);
        assert_eq!(
            rgba16_to_p010(&frame, 64, 36).len(),
            PixelFormat::P010.frame_bytes(64, 36)
        );
    }
}
