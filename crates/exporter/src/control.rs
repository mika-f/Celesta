use crate::error::ExportError;
use crate::resolve::OutputPlan;
use crate::subtitles::SubtitleFormat;
use celesta_composition::Rational;
use celesta_media::ComponentRef;
use celesta_plugin_host::PluginKillSwitch;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, PartialEq)]
pub enum ExportProgress {
    /// The composition being exported, reported once as soon as it is known
    /// (before the export range or frame selection is checked against it).
    Composition(CompositionInfo),
    /// The muxer and encoders the video, or the stills, are written with,
    /// reported once they are chosen and before any frame renders. A still
    /// export has no audio encoder.
    Output(OutputPlan),
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
    /// Something to tell the user that is not a problem, such as a plugin's
    /// note on a model's terms of use; it says what it is (`note: …`). Each
    /// is reported once per export. The JSON report lists it among the
    /// warnings.
    Note(String),
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
#[derive(Clone, Debug, PartialEq)]
#[expect(
    clippy::large_enum_variant,
    reason = "one is reported per file written, and matching it is plainer unboxed"
)]
pub enum ExportedFile {
    /// A video of `frames` frames starting at composition frame `first_frame`.
    Video {
        path: PathBuf,
        first_frame: u64,
        frames: u64,
        /// The muxer that wrote it.
        muxer: ComponentRef,
        video: TrackInfo,
        /// The audio track, when the file has one.
        audio: Option<TrackInfo>,
        /// Whether the encoded video keeps the composition's alpha, as its
        /// encoder answered.
        alpha: bool,
        /// Every file the muxer wrote, where they are now: `path` for a
        /// single file.
        files: Vec<PathBuf>,
        /// The loudness of the mix as it was encoded, when there is one.
        loudness: Option<AudioLoudness>,
    },
    /// A subtitle file, with its number of cues and their language when
    /// the composition names one.
    Subtitles {
        path: PathBuf,
        format: SubtitleFormat,
        cues: usize,
        lang: Option<String>,
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

/// The encoder of one track of an exported video, and the codec string it
/// gave the track (`avc1.640028`, `lpcm.f32le`, …).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackInfo {
    pub component: ComponentRef,
    pub codec: String,
}

/// The loudness of an exported video's mix, measured on the samples handed
/// to the audio encoder (AAC and Opus then move the true peak by up to about
/// half a dB).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioLoudness {
    /// Integrated loudness, LUFS; `None` for a silent mix or one shorter
    /// than 400 ms.
    pub integrated_lufs: Option<f64>,
    /// The highest true peak, dBTP; `None` for digital silence.
    pub true_peak_dbtp: Option<f64>,
    /// The gain a loudness target applied to the whole mix, dB (before any
    /// limiting); `None` without a target.
    pub gain_db: Option<f64>,
}

/// Cancels an export from another thread. The export stops before its next
/// frame or stage, and the plugin and codec processes it runs are killed, so
/// a frame or media request in progress (a plugin that hangs, say) ends at
/// once too. Clones share it.
#[derive(Clone, Debug, Default)]
pub struct ExportCancellation {
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) plugins: PluginKillSwitch,
}

impl ExportCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.plugins.kill();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// What kills the export's plugin and codec processes on cancel.
    pub(crate) fn plugins(&self) -> PluginKillSwitch {
        self.plugins.clone()
    }

    /// `error`, or [`ExportError::Cancelled`] once the export was
    /// cancelled: the processes cancel kills fail what they were doing.
    pub(crate) fn settle(&self, error: ExportError) -> ExportError {
        if self.is_cancelled() {
            ExportError::Cancelled
        } else {
            error
        }
    }
}
