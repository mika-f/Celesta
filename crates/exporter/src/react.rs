use celesta_project::Project;
use celesta_react_bridge::{ReactAudioClipDescriptor, ReactBridge, ReactCompositionMetadata};
use std::path::{Path, PathBuf};

/// Locates the `@celesta/react` Node.js runtime used to evaluate a React entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactRuntimeOptions {
    pub node: PathBuf,
    pub cli_script: PathBuf,
}

impl ReactRuntimeOptions {
    pub fn new(node: impl Into<PathBuf>, cli_script: impl Into<PathBuf>) -> Self {
        Self {
            node: node.into(),
            cli_script: cli_script.into(),
        }
    }
}

/// A GUI-editor-owned project a React export evaluates alongside its entry,
/// for the entry's `<ProjectTimeline />` to embed.
#[derive(Clone, Copy, Debug)]
pub struct CompanionProject<'a> {
    pub project: &'a Project,
    pub project_asset_root: &'a Path,
}

pub(crate) struct ReactVideoRequest<'a> {
    pub(crate) bridge: &'a mut ReactBridge,
    pub(crate) metadata: &'a ReactCompositionMetadata,
    /// First composition frame to render (0 unless an export range is set).
    pub(crate) start_frame: u64,
    /// Number of frames to render (the whole composition unless a range is set).
    pub(crate) frame_count: u64,
    pub(crate) asset_root: &'a Path,
    pub(crate) project: Option<(&'a Project, &'a Path)>,
    /// Every frame's reported `<Audio>` clips accumulate here (one entry per
    /// clip per frame; duplicates are merged afterwards — see
    /// `merge_react_audio_clips`).
    pub(crate) audio: &'a mut Vec<ReactAudioClipDescriptor>,
}
