//! Spawns the `@celesta/cli` Node.js runtime and evaluates a React
//! composition into the shared `celesta_composition::Scene` model.
//!
//! A single Node process is kept alive for the lifetime of a [`ReactBridge`]
//! and answers one JSON request per requested frame over its stdin/stdout
//! pipe, following the same "one long-lived process instead of one process
//! per frame" shape as `celesta-media`'s sequential video decoding session.

mod animated_images;
mod audio;
mod bridge;
mod decode;
mod error;
mod measure;
mod occurrences;
mod probe;
mod properties;
mod protocol;
mod statics;
mod stderr;
#[cfg(test)]
mod tests;
mod types;

pub use audio::{merge_react_audio_clips, react_audio_clips, react_ducking, react_video_clip_ids};
pub use bridge::{PendingFrame, ReactBridge, ReactMedia};
pub use error::ReactBridgeError;
pub use occurrences::{
    ElementOccurrences, InstanceOccurrences, OccurrenceIndex, outermost_chapter_at, scenes_of,
};
pub use properties::{
    ENTRY_PROPS_FILE, EffectiveValue, PropertyInputError, PropertyInputs, PropertyIssue,
    PropertyLayer, PropertySource, PropsFileChoice, ValueSource, effective_values,
    resolve_props_file,
};
pub use types::{
    ComponentPropertyField, ComponentPropertySchema, ComponentResolutionRequest,
    ComponentResolutions, ElementKind, FrameEvaluation, FrameSpan, LayerSource, LayerSourceReport,
    PrepareDependency, ProjectFrame, PropOverride, ReactAudioClipDescriptor, ReactChapter,
    ReactCompositionInfo, ReactCompositionMetadata, ReactDuck, ReactSubtitleDescriptor,
    ReceivedProps, ReceivedValue, SourcePos, SpawnOptions, TimingWindow, chapter_at,
};

mod project_templates;
mod project_types;
mod props_edit;
mod runtime;
pub use project_templates::{
    DEFAULT_TEMPLATE, ProjectSetup, TemplateError, TemplateInfo, initialize_project,
    list_templates, project_templates_dir,
};
pub use project_types::{
    PROJECT_TYPES_DIR, ProjectTsconfig, ProjectTypesSetup, project_types_template,
    refresh_project_types, set_up_project_types, set_up_project_types_in,
};
pub use props_edit::{
    OrderedObject, PropsEditError, import_path, replace_file, set_property, set_property_text,
};
pub use runtime::runtime_paths;
