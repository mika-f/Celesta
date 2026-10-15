use crate::options::VideoFormat;
use crate::subtitles::SubtitleFormat;
use celesta_composition::Rational;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportProgress {
    /// The composition being exported, reported once as soon as it is known
    /// (before the export range or frame selection is checked against it).
    Composition(CompositionInfo),
    /// All roots and the selected ID of a React entry.
    ReactCompositions {
        selected: String,
        compositions: Vec<celesta_react_bridge::ReactCompositionInfo>,
    },
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
    /// Total frames; when nonzero, the last zero-based frame is `frames - 1`.
    pub frames: u64,
}

/// A file an export wrote. Frame numbers are zero-based composition frames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportedFile {
    /// A video of `frames` frames starting at composition frame `first_frame`.
    Video {
        path: PathBuf,
        first_frame: u64,
        frames: u64,
        /// Whether the file has an audio stream.
        audio: bool,
        format: VideoFormat,
    },
    /// A subtitle file, with its number of cues and the language of its
    /// `<Subtitles lang>` cues, when they have one.
    Subtitles {
        path: PathBuf,
        format: SubtitleFormat,
        lang: Option<String>,
        cues: usize,
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
