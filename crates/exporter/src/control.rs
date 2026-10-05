use celesta_composition::Rational;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportProgress {
    /// The composition being exported, reported once as soon as it is known
    /// (before the export range or frame selection is checked against it).
    Composition(CompositionInfo),
    Rendering {
        frame: u64,
        total: u64,
    },
    MixingAudio,
    Muxing,
    /// A problem that does not stop the export, such as text drawn with a
    /// fallback font. Each is reported once per export.
    Warning(String),
    /// A finished output file, reported once it is in place.
    Wrote(ExportedFile),
}

/// The size, rate, and length of an exported composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositionInfo {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    /// Total frames; the last zero-based frame is `frames - 1`.
    pub frames: u64,
}

/// A file an export wrote. Frame numbers are zero-based composition frames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportedFile {
    /// An MP4 of `frames` frames starting at composition frame `first_frame`.
    Video {
        path: PathBuf,
        first_frame: u64,
        frames: u64,
        /// Whether the file has an audio stream.
        audio: bool,
    },
    /// One PNG of composition frame `frame`.
    Frame { path: PathBuf, frame: u64 },
    /// One PNG grid of `frames`, in this order, `columns` tiles per row.
    ContactSheet {
        path: PathBuf,
        frames: Vec<u64>,
        columns: u32,
    },
}

#[derive(Clone, Debug, Default)]
pub struct ExportCancellation {
    pub(crate) cancelled: Arc<AtomicBool>,
}

impl ExportCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}
