//! Viewer-owned project state and renderer-facing preview evaluation.

mod clock;
mod document;
mod error;
mod summary;
#[cfg(test)]
mod tests;

pub use clock::TimelineClock;
pub use document::EditorDocument;
pub use error::EditorDocumentError;
pub use summary::{
    AssetSummary, CharacterSummary, ClipKind, ClipSummary, ComponentClipSummary,
    DialogueClipSummary, LipSyncSummary, TrackSummary,
};
