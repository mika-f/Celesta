use celesta_media::{DecodedMatrix, DecodedPixelFormat, DecodedRange, MediaError, VideoFrame};

pub fn rgba(frame: &VideoFrame) -> Result<VideoFrame, MediaError> {
    frame.validate()?;
    if frame.pixel_format == DecodedPixelFormat::Rgba8 {
        return Ok(frame.clone());
    }
    let color = frame.color.unwrap();
    let width = frame.width as usize;
    let height = frame.height as usize;
    let chroma_width = width.div_ceil(2);
    let chroma_height = height.div_ceil(2);
    let luma_bytes = width * height;
    let (kr, kb) = match color.matrix {
        DecodedMatrix::Bt601 => (0.299_f64, 0.114_f64),
        DecodedMatrix::Bt709 => (0.2126_f64, 0.0722_f64),
    };
    let mut pixels = Vec::with_capacity(luma_bytes * 4);
    for row in 0..height {
        for column in 0..width {
            let (cb, cr) = match frame.pixel_format {
                DecodedPixelFormat::Yuv420p => {
                    let index = row / 2 * chroma_width + column / 2;
                    (
                        f64::from(frame.pixels[luma_bytes + index]),
                        f64::from(frame.pixels[luma_bytes + chroma_width * chroma_height + index]),
                    )
                }
                DecodedPixelFormat::Nv12 => {
                    let position = row as f64 / 2.0 - 0.25;
                    let count = chroma_height.saturating_sub(2).clamp(1, 5);
                    let start = (position - (count as f64 - 2.0) / 2.0).trunc() as isize;
                    let weights: Vec<_> = (start..start + count as isize)
                        .map(|sample| cubic((position - sample as f64).abs()))
                        .collect();
                    let normalization: f64 = weights.iter().sum();
                    let mut chroma = [0.0; 2];
                    for (tap, weight) in weights.into_iter().enumerate() {
                        let sample =
                            (start + tap as isize).clamp(0, chroma_height as isize - 1) as usize;
                        let index = luma_bytes + 2 * (sample * chroma_width + column / 2);
                        for (channel, value) in chroma.iter_mut().enumerate() {
                            *value +=
                                weight / normalization * f64::from(frame.pixels[index + channel]);
                        }
                    }
                    (chroma[0], chroma[1])
                }
                DecodedPixelFormat::Rgba8 => unreachable!(),
            };
            let mut y = f64::from(frame.pixels[row * width + column]);
            let mut u = cb - 128.0;
            let mut v = cr - 128.0;
            if color.range == DecodedRange::Limited {
                y = (y - 16.0) * 255.0 / 219.0;
                u *= 255.0 / 224.0;
                v *= 255.0 / 224.0;
            }
            let rgb = [
                y + 2.0 * (1.0 - kr) * v,
                y - 2.0 * kb * (1.0 - kb) / (1.0 - kr - kb) * u
                    - 2.0 * kr * (1.0 - kr) / (1.0 - kr - kb) * v,
                y + 2.0 * (1.0 - kb) * u,
            ];
            pixels.extend(rgb.map(|value| value.round().clamp(0.0, 255.0) as u8));
            pixels.push(255);
        }
    }
    VideoFrame::rgba(frame.width, frame.height, pixels)
}

fn cubic(distance: f64) -> f64 {
    if distance < 1.0 {
        1.4 * distance.powi(3) - 2.4 * distance.powi(2) + 1.0
    } else if distance < 2.0 {
        -0.6 * distance.powi(3) + 3.0 * distance.powi(2) - 4.8 * distance + 2.4
    } else {
        0.0
    }
}

pub fn exact(label: &str, actual: &[u8], reference: &[u8]) -> bool {
    assert_eq!(actual.len(), reference.len());
    assert!(!actual.is_empty());
    let (mut maximum, mut total, mut differing) = (0, 0_u64, 0);
    let (mut below, mut above) = (0, 0);
    for (actual, reference) in actual
        .as_chunks::<4>()
        .0
        .iter()
        .zip(reference.as_chunks::<4>().0)
    {
        assert_eq!(actual[3], reference[3], "{label}: alpha");
        differing += usize::from(actual != reference);
        for channel in 0..3 {
            let difference = actual[channel].abs_diff(reference[channel]);
            maximum = maximum.max(difference);
            total += u64::from(difference);
            below += usize::from(actual[channel] < reference[channel]);
            above += usize::from(actual[channel] > reference[channel]);
        }
    }
    println!(
        "REFERENCE {label} max={maximum} mean={:.9} differing={differing} below={below} above={above}",
        total as f64 / (actual.len() / 4 * 3) as f64
    );
    if maximum != 0 {
        for (index, (actual, reference)) in actual
            .iter()
            .zip(reference)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .take(8)
        {
            println!("REFERENCE {label} byte={index} actual={actual} expected={reference}");
        }
    }
    maximum == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_media::DecodedColor;

    #[test]
    fn neutral_chroma_rounds_to_nearest_and_clamps() {
        for format in [DecodedPixelFormat::Yuv420p, DecodedPixelFormat::Nv12] {
            for matrix in [DecodedMatrix::Bt601, DecodedMatrix::Bt709] {
                for range in [DecodedRange::Limited, DecodedRange::Full] {
                    let frame = VideoFrame::new(
                        2,
                        2,
                        format,
                        Some(DecodedColor {
                            matrix,
                            range,
                            chroma_location: Default::default(),
                        }),
                        vec![0, 32, 235, 255, 128, 128],
                    )
                    .unwrap();
                    let expected = if range == DecodedRange::Limited {
                        [0, 19, 255, 255]
                    } else {
                        [0, 32, 235, 255]
                    };
                    for (pixel, value) in rgba(&frame)
                        .unwrap()
                        .pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .zip(expected)
                    {
                        assert_eq!(*pixel, [value, value, value, 255]);
                    }
                }
            }
        }
    }

    #[test]
    fn colored_samples_follow_each_matrix_and_range() {
        for format in [DecodedPixelFormat::Yuv420p, DecodedPixelFormat::Nv12] {
            for (matrix, range, expected) in [
                (DecodedMatrix::Bt601, DecodedRange::Full, [201, 62, 33, 255]),
                (
                    DecodedMatrix::Bt601,
                    DecodedRange::Limited,
                    [213, 54, 21, 255],
                ),
                (DecodedMatrix::Bt709, DecodedRange::Full, [213, 73, 29, 255]),
                (
                    DecodedMatrix::Bt709,
                    DecodedRange::Limited,
                    [227, 68, 18, 255],
                ),
            ] {
                let frame = VideoFrame::new(
                    2,
                    2,
                    format,
                    Some(DecodedColor {
                        matrix,
                        range,
                        chroma_location: Default::default(),
                    }),
                    vec![100, 100, 100, 100, 90, 200],
                )
                .unwrap();
                assert_eq!(*rgba(&frame).unwrap().pixels, expected.repeat(4));
            }
        }
    }

    #[test]
    fn rgba_preserves_alpha_and_empty_or_malformed_frames_are_errors() {
        let frame = VideoFrame::rgba(1, 1, vec![1, 2, 3, 4]).unwrap();
        assert_eq!(rgba(&frame).unwrap(), frame);
        let mut invalid = frame.clone();
        invalid.pixels = Vec::new().into();
        assert!(rgba(&invalid).is_err());
        invalid = frame;
        invalid.width = 0;
        assert!(rgba(&invalid).is_err());
    }
}
