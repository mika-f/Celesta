//! Frame-exact project export through the shared evaluator and renderers.

mod control;
mod encode;
mod error;
mod exporter;
mod interleave;
mod job;
mod loudness;
mod media;
mod options;
mod output;
mod project;
mod range;
mod react;
mod react_entry;
mod render;
pub mod report;
mod resolve;
mod subtitles;
#[cfg(test)]
mod tests;
mod timecode;

pub use celesta_media::{LoudnessTarget, PeakHandling};
pub use control::{
    AudioLoudness, CompositionInfo, ExportCancellation, ExportProgress, ExportedFile, TrackInfo,
};
pub use error::{ExportError, FrameFailure};
pub use exporter::Exporter;
pub use job::ExportJob;
pub use media::{media_registry, use_installed_media, use_media};
pub use options::{AudioChoice, EncoderChoice, ExportOptions, OutputChoice, SettingInput};
pub use range::ExportRange;
pub use react::{CompanionProject, ReactRuntimeOptions};
pub use resolve::{
    EncoderPlan, OutputPlan, ResolvedEncoder, ResolvedMuxer, ResolvedOutput, X264_PRESETS,
    encoding_label, resolve_output,
};
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
