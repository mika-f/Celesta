use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportProgress {
    Rendering {
        frame: u64,
        total: u64,
    },
    MixingAudio,
    Muxing,
    /// A problem that does not stop the export, such as text drawn with a
    /// fallback font. Each is reported once per export.
    Warning(String),
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
