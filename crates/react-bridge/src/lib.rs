//! Spawns the `@celesta/react` Node.js runtime and evaluates a React
//! composition into the shared `celesta_composition::Scene` model.
//!
//! A single Node process is kept alive for the lifetime of a [`ReactBridge`]
//! and answers one JSON request per requested frame over its stdin/stdout
//! pipe, following the same "one long-lived process instead of one process
//! per frame" shape as `celesta-media`'s sequential video decoding session.

mod audio;
mod bridge;
mod error;
mod measure;
mod probe;
mod properties;
mod protocol;
#[cfg(test)]
mod tests;
mod types;

pub use audio::{merge_react_audio_clips, react_audio_clips};
pub use bridge::ReactBridge;
pub use error::ReactBridgeError;
pub use properties::{
    PropertyInputError, PropertyInputs, PropertyIssue, PropertyLayer, PropertySource,
};
pub use types::{
    ComponentPropertyField, ComponentPropertySchema, ComponentResolutionRequest, FrameEvaluation,
    ProjectFrame, ReactAudioClipDescriptor, ReactCompositionMetadata,
};

mod project_types;
mod runtime;
pub use project_types::{
    PROJECT_TYPES_DIR, ProjectTsconfig, ProjectTypesSetup, initialize_project,
    project_types_template, refresh_project_types, set_up_project_types, set_up_project_types_in,
};
pub use runtime::runtime_paths;
