use crate::range::ExportRange;
use crate::{GpuDriver, RenderQuality};
use celesta_media::LoudnessTarget;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use strum::VariantArray;

#[derive(Clone, Debug, Default, PartialEq)]
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
    /// A video export also writes its subtitles to each of these, as SRT or
    /// WebVTT by the extension, timed like the video: a React entry's
    /// `<Subtitles>` cues and `<Dialogue>` lines, or a project's dialogue
    /// items. A `{lang}` in a path writes one file per language, with the
    /// language in its place (`und` for subtitles without one); a path
    /// without it takes subtitles of one language only.
    pub subtitles: Vec<PathBuf>,
    /// The video is played in a loop, so its sound runs from its end into
    /// its start. An MP4's AAC encoder then starts from the end of the mix
    /// instead of silence, which keeps the first hit from smearing on every
    /// pass; the decoded sound is still exactly the mix. MOV's PCM needs
    /// nothing, and WebM's Opus is written as without it.
    pub looping: bool,
    /// Brings the whole mix to this integrated loudness before it is
    /// encoded, keeping its true peaks under the target's ceiling. A video
    /// export measures its mix either way and reports the result in
    /// [`ExportedFile::Video`](crate::ExportedFile::Video).
    pub loudness: Option<LoudnessTarget>,
}

/// The container (and, with it, the codecs) an exported video is written in.
///
/// `.mov` (ProRes 4444) and `.webm` (VP9) keep the composition's alpha
/// channel; `.mp4` (H.264) has none, so transparent pixels are flattened
/// onto the default opaque background.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    strum::Display,
    strum::EnumString,
    strum::IntoStaticStr,
    strum::VariantArray,
)]
#[strum(
    serialize_all = "lowercase",
    parse_err_ty = UnknownVideoFormat,
    parse_err_fn = UnknownVideoFormat::new
)]
pub enum VideoFormat {
    #[default]
    Mp4,
    Mov,
    Webm,
}

impl VideoFormat {
    /// The output file extension, which is also the name `--output-format` takes.
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    /// The format whose extension `path` has, ignoring case.
    pub fn from_extension(path: &Path) -> Option<Self> {
        let extension = path.extension().and_then(OsStr::to_str)?;
        Self::VARIANTS
            .iter()
            .copied()
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

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "unknown video format '{0}' (expected one of {expected})",
    expected = names(VideoFormat::VARIANTS)
)]
pub struct UnknownVideoFormat(pub String);

impl UnknownVideoFormat {
    fn new(name: &str) -> Self {
        Self(name.to_owned())
    }
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
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    strum::Display,
    strum::EnumString,
    strum::IntoStaticStr,
    strum::VariantArray,
    clap::ValueEnum,
)]
#[strum(
    parse_err_ty = UnknownBitDepth,
    parse_err_fn = UnknownBitDepth::new
)]
pub enum BitDepth {
    #[default]
    #[strum(serialize = "8")]
    #[value(name = "8")]
    Eight,
    #[strum(serialize = "10")]
    #[value(name = "10")]
    Ten,
}

impl BitDepth {
    /// The name `--bit-depth` takes.
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    /// The pixel format libx264 encodes.
    pub(crate) const fn pixel_format(self) -> &'static str {
        match self {
            Self::Eight => "yuv420p",
            Self::Ten => "yuv420p10le",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown bit depth '{0}' (expected 8 or 10)")]
pub struct UnknownBitDepth(pub String);

impl UnknownBitDepth {
    fn new(name: &str) -> Self {
        Self(name.to_owned())
    }
}

/// libx264's `-preset` values, fastest first.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    strum::Display,
    strum::EnumString,
    strum::IntoStaticStr,
    strum::VariantArray,
    clap::ValueEnum,
)]
#[strum(
    serialize_all = "lowercase",
    parse_err_ty = UnknownEncoderPreset,
    parse_err_fn = UnknownEncoderPreset::new
)]
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
    /// The name libx264 (and `ffmpeg -preset`) uses.
    pub fn as_str(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "unknown encoder preset '{0}' (expected one of {expected})",
    expected = names(EncoderPreset::VARIANTS)
)]
pub struct UnknownEncoderPreset(pub String);

impl UnknownEncoderPreset {
    fn new(name: &str) -> Self {
        Self(name.to_owned())
    }
}

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
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    strum::Display,
    strum::EnumString,
    strum::IntoStaticStr,
    strum::VariantArray,
    clap::ValueEnum,
)]
#[strum(
    serialize_all = "lowercase",
    parse_err_ty = UnknownColorConversion,
    parse_err_fn = UnknownColorConversion::new
)]
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
    pub fn as_str(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown color conversion '{0}' (expected auto, gpu, or encoder)")]
pub struct UnknownColorConversion(pub String);

impl UnknownColorConversion {
    fn new(name: &str) -> Self {
        Self(name.to_owned())
    }
}

/// `values`' names, separated by spaces.
fn names(values: &[impl Copy + Into<&'static str>]) -> String {
    let names: Vec<&str> = values.iter().map(|&value| value.into()).collect();
    names.join(" ")
}
