use crate::color::VideoColor;
use celesta_composition::{Rational, Time};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaProbe {
    pub duration: Option<Time>,
    pub video: Option<VideoStream>,
    pub audio: Vec<AudioStream>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoStream {
    pub index: u32,
    pub codec: Option<String>,
    pub width: u32,
    pub height: u32,
    pub frame_rate: Option<Rational>,
    pub duration: Option<Time>,
    pub color: VideoColor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStream {
    pub index: u32,
    pub codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub duration: Option<Time>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub pixel_format: DecodedPixelFormat,
    pub color: Option<DecodedColor>,
    /// Shared so a decoder can keep the frame it last served (to answer a
    /// repeat request or freeze past the source end) without copying a full
    /// RGBA frame — 8MB at 1080p — for every frame it hands out.
    pub pixels: Arc<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DecodedPixelFormat {
    #[default]
    #[serde(rename = "rgba8")]
    Rgba8,
    #[serde(rename = "nv12")]
    Nv12,
    #[serde(rename = "yuv420p")]
    Yuv420p,
}

impl DecodedPixelFormat {
    pub fn frame_bytes(self, width: u32, height: u32) -> Result<usize, crate::MediaError> {
        let invalid = || crate::MediaError::FrameTooLarge { width, height };
        if width == 0 || height == 0 {
            return Err(invalid());
        }
        let (w, h) = (width as usize, height as usize);
        let luma = w.checked_mul(h).ok_or_else(invalid)?;
        let bytes = match self {
            Self::Rgba8 => luma.checked_mul(4),
            Self::Nv12 | Self::Yuv420p => w
                .div_ceil(2)
                .checked_mul(h.div_ceil(2))
                .and_then(|n| n.checked_mul(2))
                .and_then(|n| n.checked_add(luma)),
        }
        .ok_or_else(invalid)?;
        if bytes > isize::MAX as usize {
            return Err(invalid());
        }
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecodedMatrix {
    #[serde(rename = "bt601")]
    Bt601,
    #[serde(rename = "bt709")]
    Bt709,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecodedRange {
    Limited,
    Full,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecodedChromaLocation {
    #[default]
    Left,
}

/// I420 replicates each chroma sample over its 2x2 luma block. NV12 uses
/// vertical bicubic reconstruction (B=0, C=0.6), matching FFmpeg's SDR path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecodedColor {
    pub matrix: DecodedMatrix,
    pub range: DecodedRange,
    #[serde(default)]
    pub chroma_location: DecodedChromaLocation,
}

impl VideoFrame {
    pub fn new(
        width: u32,
        height: u32,
        pixel_format: DecodedPixelFormat,
        color: Option<DecodedColor>,
        pixels: impl Into<Arc<Vec<u8>>>,
    ) -> Result<Self, crate::MediaError> {
        let frame = Self {
            width,
            height,
            pixel_format,
            color,
            pixels: pixels.into(),
        };
        frame.validate()?;
        Ok(frame)
    }

    pub fn rgba(
        width: u32,
        height: u32,
        pixels: impl Into<Arc<Vec<u8>>>,
    ) -> Result<Self, crate::MediaError> {
        Self::new(width, height, DecodedPixelFormat::Rgba8, None, pixels)
    }

    pub fn validate(&self) -> Result<(), crate::MediaError> {
        let expected = self.pixel_format.frame_bytes(self.width, self.height)?;
        if self.pixels.len() != expected
            || (self.pixel_format == DecodedPixelFormat::Rgba8) != self.color.is_none()
        {
            return Err(crate::MediaError::Codec(
                "invalid decoded frame layout or color".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn to_rgba(&self) -> Result<Self, crate::MediaError> {
        self.validate()?;
        if self.pixel_format == DecodedPixelFormat::Rgba8 {
            return Ok(self.clone());
        }
        let color = self.color.expect("validated YUV color");
        let (w, h) = (self.width as usize, self.height as usize);
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        let (kr, kb) = match color.matrix {
            DecodedMatrix::Bt601 => (0.299, 0.114),
            DecodedMatrix::Bt709 => (0.2126, 0.0722),
        };
        let full = color.range == DecodedRange::Full;
        let mut pixels = vec![0; DecodedPixelFormat::Rgba8.frame_bytes(self.width, self.height)?];
        for y in 0..h {
            for x in 0..w {
                let c = (y / 2) * cw + x / 2;
                let (u, v) = match self.pixel_format {
                    DecodedPixelFormat::Nv12 => {
                        let position = y as f64 * 0.5 - 0.25;
                        let taps = 5.min(ch.saturating_sub(2)).max(1);
                        let first = (position - (taps as f64 - 2.) * 0.5).trunc() as isize;
                        let kernel = |distance: f64| {
                            if distance < 1. {
                                1.4 * distance.powi(3) - 2.4 * distance.powi(2) + 1.
                            } else if distance < 2. {
                                -0.6 * distance.powi(3) + 3. * distance.powi(2) - 4.8 * distance
                                    + 2.4
                            } else {
                                0.
                            }
                        };
                        let sum: f64 = (first..first + taps as isize)
                            .map(|row| kernel((position - row as f64).abs()))
                            .sum();
                        let mut chroma = [0.; 2];
                        for row in first..first + taps as isize {
                            let weight = kernel((position - row as f64).abs()) / sum;
                            let row = row.clamp(0, ch as isize - 1) as usize;
                            let offset = w * h + 2 * (row * cw + x / 2);
                            chroma[0] += weight * self.pixels[offset] as f64;
                            chroma[1] += weight * self.pixels[offset + 1] as f64;
                        }
                        (chroma[0], chroma[1])
                    }
                    _ => (
                        self.pixels[w * h + c] as f64,
                        self.pixels[w * h + cw * ch + c] as f64,
                    ),
                };
                let luma = if full {
                    self.pixels[y * w + x] as f64
                } else {
                    (self.pixels[y * w + x] as f64 - 16.) * 255. / 219.
                };
                let scale = if full { 1. } else { 255. / 224. };
                let u = (u - 128.) * scale;
                let v = (v - 128.) * scale;
                let rgb = [
                    luma + 2. * (1. - kr) * v,
                    luma - 2. * kb * (1. - kb) / (1. - kr - kb) * u
                        - 2. * kr * (1. - kr) / (1. - kr - kb) * v,
                    luma + 2. * (1. - kb) * u,
                ];
                let offset = (y * w + x) * 4;
                for (channel, value) in rgb.into_iter().enumerate() {
                    let value = value.round();
                    pixels[offset + channel] = value.clamp(0., 255.) as u8;
                }
                pixels[offset + 3] = 255;
            }
        }
        Self::rgba(self.width, self.height, pixels)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl AudioBuffer {
    pub fn frame_count(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    #[test]
    fn rgba_is_shared_and_yuv_layouts_are_validated() {
        let rgba = VideoFrame::rgba(1, 1, vec![1, 2, 3, 4]).unwrap();
        assert!(Arc::ptr_eq(&rgba.pixels, &rgba.to_rgba().unwrap().pixels));
        assert!(VideoFrame::rgba(0, 1, Vec::new()).is_err());
        assert!(VideoFrame::rgba(1, 1, vec![0; 3]).is_err());
        let color = Some(DecodedColor {
            chroma_location: Default::default(),
            matrix: DecodedMatrix::Bt601,
            range: DecodedRange::Limited,
        });
        for format in [DecodedPixelFormat::Yuv420p, DecodedPixelFormat::Nv12] {
            assert_eq!(format.frame_bytes(3, 3).unwrap(), 17);
            assert!(VideoFrame::new(3, 3, format, None, vec![0; 17]).is_err());
            assert!(VideoFrame::new(3, 3, format, color, vec![0; 16]).is_err());
            let mut planes = vec![16; 9];
            planes.extend([128; 8]);
            let frame = VideoFrame::new(3, 3, format, color, planes)
                .unwrap()
                .to_rgba()
                .unwrap();
            assert_eq!(*frame.pixels, [0, 0, 0, 255].repeat(9));
        }
        assert!(
            DecodedPixelFormat::Rgba8
                .frame_bytes(u32::MAX, u32::MAX)
                .is_err()
        );
    }

    #[test]
    fn interleaved_and_planar_chroma_materialize_identically() {
        for matrix in [DecodedMatrix::Bt601, DecodedMatrix::Bt709] {
            for range in [DecodedRange::Limited, DecodedRange::Full] {
                let color = Some(DecodedColor {
                    matrix,
                    range,
                    chroma_location: Default::default(),
                });
                let luma = [16, 235, 81, 145, 128, 64, 192, 32];
                let mut i420 = luma.to_vec();
                i420.extend([90, 180, 240, 40]);
                let mut nv12 = luma.to_vec();
                nv12.extend([90, 240, 180, 40]);
                let planar =
                    VideoFrame::new(4, 2, DecodedPixelFormat::Yuv420p, color, i420).unwrap();
                let interleaved =
                    VideoFrame::new(4, 2, DecodedPixelFormat::Nv12, color, nv12).unwrap();
                assert_eq!(planar.to_rgba().unwrap(), interleaved.to_rgba().unwrap());
                assert!(
                    planar
                        .to_rgba()
                        .unwrap()
                        .pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|p| p[3] == 255)
                );
            }
        }
    }
}
