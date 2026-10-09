use crate::error::ExportError;
use celesta_project::Project;
use celesta_react_bridge::{
    PropertyInputs, ReactAudioClipDescriptor, ReactBridge, ReactCompositionMetadata,
};
use std::path::{Path, PathBuf};

/// Locates the `@celesta/cli` Node.js runtime used to evaluate a React
/// entry, and holds the project property values passed to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactRuntimeOptions {
    pub node: PathBuf,
    pub cli_script: PathBuf,
    /// `--props`/`--props-file` values. A companion project's `properties`
    /// are added below them when one is exported alongside.
    pub properties: PropertyInputs,
}

impl ReactRuntimeOptions {
    pub fn new(node: impl Into<PathBuf>, cli_script: impl Into<PathBuf>) -> Self {
        Self {
            node: node.into(),
            cli_script: cli_script.into(),
            properties: PropertyInputs::default(),
        }
    }

    #[must_use]
    pub fn with_properties(mut self, properties: PropertyInputs) -> Self {
        self.properties = properties;
        self
    }

    /// Starts the entry with these properties over the companion project's.
    pub(crate) fn spawn(
        &self,
        entry: &Path,
        project: Option<(&Project, &Path)>,
    ) -> Result<ReactBridge, ExportError> {
        let properties = match project {
            Some((project, asset_root)) => {
                self.properties
                    .clone()
                    .with_project(&project.properties, None, asset_root)
            }
            None => self.properties.clone(),
        };
        ReactBridge::spawn_with_properties(&self.node, &self.cli_script, entry, &properties)
            .map_err(ExportError::React)
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
