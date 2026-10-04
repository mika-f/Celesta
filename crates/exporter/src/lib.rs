//! Frame-exact project export through the shared evaluator and renderers.

mod control;
mod error;
mod exporter;
mod options;
mod project;
mod range;
mod react;
mod react_entry;
mod render;
#[cfg(test)]
mod tests;
mod timecode;

pub use control::{ExportCancellation, ExportProgress};
pub use error::ExportError;
pub use exporter::Exporter;
pub use options::{
    ColorConversion, EncoderPreset, ExportOptions, UnknownColorConversion, UnknownEncoderPreset,
    VideoEncoding,
};
pub use range::ExportRange;
pub use react::{CompanionProject, ReactRuntimeOptions};
pub use timecode::{TimecodeError, parse_timecode};

mod contact_sheet;
mod stills;

pub use celesta_gpu_renderer::{RenderQuality, UnknownRenderQuality};
pub use stills::{ContactSheet, FrameSelection, MAX_PNG_FRAMES, PngExport};

/// Sample rate used to mix a React export's audio when no companion project
/// supplies its own `AudioGraph.sample_rate` (the project format has no
/// default of its own; every checked-in example project's `sampleRate` is
/// 48000, so this matches that convention).
const DEFAULT_REACT_AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Timescale `<Audio>`'s `startFrom` (seconds) is converted at when built
/// into a `Time`, matching `packages/react/src/render.ts`'s
/// `SECONDS_TIMESCALE` for `<Video>`'s `startFrom`/`sourceTimeSeconds`.
const AUDIO_SECONDS_TIMESCALE: u32 = 1_000_000;
