use crate::range::ExportRange;
use crate::{GpuDriver, RenderQuality};
use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportOptions {
    pub overwrite: bool,
    /// When set, only this composition-time span is rendered; the encoded
    /// output starts at its own `t = 0` (both video and the mixed audio are
    /// shifted so the span's start becomes the file's start). `None` exports
    /// the whole composition, byte-for-byte as before.
    pub range: Option<ExportRange>,
    /// The container and codecs the video is written with.
    pub video_format: VideoFormat,
    /// H.264 encoder settings; the default matches the historical output.
    pub video: VideoEncoding,
    /// How carefully scaled and rotated layers are drawn. Exports default to
    /// [`RenderQuality::Final`]; this is independent of the encoder preset,
    /// which only trades encoding speed against file size.
    pub render_quality: RenderQuality,
    /// The graphics API frames are rendered through.
    pub driver: GpuDriver,
    /// A video export, or an export without an output, writes its subtitles
    /// to each of these, as SRT or WebVTT by the extension, timed like the
    /// video: a React entry's `<Subtitles>` cues and `<Dialogue>` lines, or a
    /// project's dialogue items. A path with `{lang}` is one file per
    /// `<Subtitles lang>`, holding that language's cues and the untagged ones.
    pub subtitles: Vec<PathBuf>,
}

/// The container (and, with it, the codecs) an exported video is written in.
///
/// `.mov` (ProRes 4444) and `.webm` (VP9) keep the composition's alpha
/// channel; `.mp4` (H.264) has none, so transparent pixels are flattened
/// onto the default opaque background.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VideoFormat {
    #[default]
    Mp4,
    Mov,
    Webm,
}

impl VideoFormat {
    pub const ALL: [Self; 3] = [Self::Mp4, Self::Mov, Self::Webm];

    /// The output file extension, which is also the name `--output-format` takes.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::Webm => "webm",
        }
    }

    /// The format whose extension `path` has, ignoring case.
    pub fn from_extension(path: &Path) -> Option<Self> {
        let extension = path.extension().and_then(OsStr::to_str)?;
        Self::ALL
            .into_iter()
            .find(|format| format.as_str().eq_ignore_ascii_case(extension))
    }

    /// Whether the encoded video keeps the composition's transparency.
    pub const fn has_alpha(self) -> bool {
        !matches!(self, Self::Mp4)
    }

    /// The video and audio encoders this format needs, which a stripped
    /// FFmpeg build may lack.
    pub(crate) const fn encoders(self) -> [&'static str; 2] {
        match self {
            Self::Mp4 => ["libx264", "aac"],
            Self::Mov => ["prores_ks", "pcm_s24le"],
            Self::Webm => ["libvpx-vp9", "libopus"],
        }
    }
}

impl fmt::Display for VideoFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for VideoFormat {
    type Err = UnknownVideoFormat;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|format| format.as_str() == name)
            .ok_or_else(|| UnknownVideoFormat(name.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownVideoFormat(pub String);

impl fmt::Display for UnknownVideoFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown video format '{}' (expected one of",
            self.0
        )?;
        for format in VideoFormat::ALL {
            write!(formatter, " {format}")?;
        }
        formatter.write_str(")")
    }
}

impl Error for UnknownVideoFormat {}

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
    /// Bits per sample of the encoded video.
    pub bit_depth: BitDepth,
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
            bit_depth: BitDepth::Eight,
        }
    }
}

/// Bits per sample of the exported H.264 stream.
///
/// Ten bits encode `yuv420p10le` with libx264's High 10 profile: each step
/// is a quarter of an 8-bit one, so dark, gentle gradients hold together
/// even after compression. Fewer players decode High 10 in hardware (many
/// phones, browsers and editing tools play only 8-bit H.264), so 8 bits stay
/// the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BitDepth {
    #[default]
    Eight,
    Ten,
}

impl BitDepth {
    pub const ALL: [Self; 2] = [Self::Eight, Self::Ten];

    /// The name `--bit-depth` takes.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eight => "8",
            Self::Ten => "10",
        }
    }

    /// The pixel format libx264 encodes.
    pub(crate) const fn pixel_format(self) -> &'static str {
        match self {
            Self::Eight => "yuv420p",
            Self::Ten => "yuv420p10le",
        }
    }
}

impl fmt::Display for BitDepth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for BitDepth {
    type Err = UnknownBitDepth;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|depth| depth.as_str() == name)
            .ok_or_else(|| UnknownBitDepth(name.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownBitDepth(pub String);

impl fmt::Display for UnknownBitDepth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown bit depth '{}' (expected 8 or 10)",
            self.0
        )
    }
}

impl Error for UnknownBitDepth {}

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

/// Where rendered RGBA frames are converted to yuv420p (or yuv420p10le) for
/// the encoder.
///
/// On the GPU, only 1.5 instead of 4 bytes per pixel are read back and the
/// encoder skips its own conversion. Both use the BT.601 limited-range
/// matrix; the GPU averages each 2x2 block for chroma where libswscale
/// filters bicubically, a difference of a few code values at hard color
/// edges. The GPU rounds each plane from the frame's own precision with a
/// dither; for 8-bit output the encoder converts the dithered 8-bit RGBA
/// frame, so a dark gradient bands somewhat more there, and for 10-bit
/// output a 16-bit RGBA one (8 bytes per pixel read back).
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
