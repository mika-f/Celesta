use crate::range::ExportRange;
use crate::{GpuDriver, RenderQuality};
use std::error::Error;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportOptions {
    pub overwrite: bool,
    /// When set, only this composition-time span is rendered; the encoded
    /// output starts at its own `t = 0` (both video and the mixed audio are
    /// shifted so the span's start becomes the file's start). `None` exports
    /// the whole composition, byte-for-byte as before.
    pub range: Option<ExportRange>,
    /// H.264 encoder settings; the default matches the historical output.
    pub video: VideoEncoding,
    /// How carefully scaled and rotated layers are drawn. Exports default to
    /// [`RenderQuality::Final`]; this is independent of the encoder preset,
    /// which only trades encoding speed against file size.
    pub render_quality: RenderQuality,
    /// The graphics API frames are rendered through.
    pub driver: GpuDriver,
}

/// Settings for the exported H.264 video stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoEncoding {
    /// Speed/compression trade-off. Faster presets encode much faster at the
    /// cost of a larger file for the same `crf`; they do not lower quality.
    pub preset: EncoderPreset,
    /// Constant rate factor, `0..=51`; lower is higher quality and larger.
    pub crf: u8,
    /// Where the rendered RGBA frames become the encoder's yuv420p.
    pub color_conversion: ColorConversion,
}

impl VideoEncoding {
    /// Highest CRF libx264 accepts for 8-bit output.
    pub const MAX_CRF: u8 = 51;
}

impl Default for VideoEncoding {
    fn default() -> Self {
        Self {
            preset: EncoderPreset::Medium,
            crf: 18,
            color_conversion: ColorConversion::Auto,
        }
    }
}

/// libx264's `-preset` values, fastest first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EncoderPreset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    #[default]
    Medium,
    Slow,
    Slower,
    Veryslow,
}

impl EncoderPreset {
    pub const ALL: [Self; 9] = [
        Self::Ultrafast,
        Self::Superfast,
        Self::Veryfast,
        Self::Faster,
        Self::Fast,
        Self::Medium,
        Self::Slow,
        Self::Slower,
        Self::Veryslow,
    ];

    /// The name libx264 (and `ffmpeg -preset`) uses.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ultrafast => "ultrafast",
            Self::Superfast => "superfast",
            Self::Veryfast => "veryfast",
            Self::Faster => "faster",
            Self::Fast => "fast",
            Self::Medium => "medium",
            Self::Slow => "slow",
            Self::Slower => "slower",
            Self::Veryslow => "veryslow",
        }
    }
}

impl fmt::Display for EncoderPreset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for EncoderPreset {
    type Err = UnknownEncoderPreset;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|preset| preset.as_str() == name)
            .ok_or_else(|| UnknownEncoderPreset(name.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownEncoderPreset(pub String);

impl fmt::Display for UnknownEncoderPreset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown encoder preset '{}' (expected one of",
            self.0
        )?;
        for preset in EncoderPreset::ALL {
            write!(formatter, " {preset}")?;
        }
        formatter.write_str(")")
    }
}

impl Error for UnknownEncoderPreset {}

/// Where rendered RGBA frames are converted to yuv420p for the encoder.
///
/// On the GPU, only 1.5 instead of 4 bytes per pixel are read back and the
/// encoder skips its own conversion. Both use the BT.601 limited-range
/// matrix; the GPU averages each 2x2 block for chroma where libswscale
/// filters bicubically, a difference of a few code values at hard color
/// edges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorConversion {
    /// On the GPU when it is a hardware GPU that supports it, otherwise in
    /// the encoder: a software renderer (lavapipe, WARP) runs the
    /// conversion passes slower than libswscale converts.
    #[default]
    Auto,
    /// Always on the GPU; the export fails if the GPU cannot.
    Gpu,
    /// Always in the encoder (libswscale), reading RGBA back.
    Encoder,
}

impl ColorConversion {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Gpu, Self::Encoder];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Gpu => "gpu",
            Self::Encoder => "encoder",
        }
    }
}

impl fmt::Display for ColorConversion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ColorConversion {
    type Err = UnknownColorConversion;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|conversion| conversion.as_str() == name)
            .ok_or_else(|| UnknownColorConversion(name.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownColorConversion(pub String);

impl fmt::Display for UnknownColorConversion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown color conversion '{}' (expected auto, gpu, or encoder)",
            self.0
        )
    }
}

impl Error for UnknownColorConversion {}
