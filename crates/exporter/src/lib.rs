//! Frame-exact project export through the shared evaluator and renderers.

mod control;
mod error;
mod exporter;
mod job;
mod loudness;
mod options;
mod output;
mod project;
mod range;
mod react;
mod react_entry;
mod render;
pub mod report;
mod subtitles;
#[cfg(test)]
mod tests;
mod timecode;

pub use celesta_media::{LoudnessTarget, PeakHandling};
pub use control::{
    AudioLoudness, CompositionInfo, ExportCancellation, ExportProgress, ExportedFile,
};
pub use error::{ExportError, FrameFailure};
pub use exporter::Exporter;
pub use job::ExportJob;
pub use options::{
    BitDepth, ColorConversion, EncoderPreset, ExportOptions, UnknownBitDepth,
    UnknownColorConversion, UnknownEncoderPreset, UnknownVideoFormat, VideoEncoding, VideoFormat,
};
pub use range::ExportRange;
pub use react::{CompanionProject, ReactRuntimeOptions};
pub use subtitles::SubtitleFormat;
pub use timecode::{TimecodeError, parse_timecode};

mod contact_sheet;
mod stills;

pub use celesta_gpu_renderer::{GpuDriver, RenderQuality, UnknownGpuDriver, UnknownRenderQuality};
pub use stills::{ContactSheet, FrameSelection, MAX_STILL_FILES, StillExport, StillFormat};

/// Sample rate used to mix a React export's audio when no companion project
/// supplies its own `AudioGraph.sample_rate` (the project format has no
/// default of its own; every checked-in example project's `sampleRate` is
/// 48000, so this matches that convention).
const DEFAULT_REACT_AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Timescale `<Audio>`'s `startFrom` (seconds) is converted at when built
/// into a `Time`, matching `packages/react/src/render.ts`'s
/// `SECONDS_TIMESCALE` for `<Video>`'s `startFrom`/`sourceTimeSeconds`.
const AUDIO_SECONDS_TIMESCALE: u32 = 1_000_000;
