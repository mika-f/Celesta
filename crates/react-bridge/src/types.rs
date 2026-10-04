use celesta_composition::{Animatable, Layer, Rational, Scene};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A companion project's layers for one exact frame, evaluated up front by
/// the caller (see [`ReactBridge::scene_at_with_project`]).
#[derive(Clone, Copy, Debug)]
pub struct ProjectFrame<'a> {
    /// Every evaluated layer together, in project order — what
    /// `<ProjectTimeline />` embeds.
    pub layers: &'a [Layer],
    /// The same layers, grouped by track id — what `<ProjectTrack />` and
    /// `useProjectTrack()` embed.
    pub tracks: &'a BTreeMap<String, Vec<Layer>>,
}

/// Static composition facts read once from the entry's `<Composition>` root.
#[derive(Clone, Debug, PartialEq)]
pub struct ReactCompositionMetadata {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub duration_in_frames: u64,
    /// Every `ComponentPropertySchema` declared via `registerComponent(name,
    /// component, schema)`, keyed by `name`. Populated at spawn time — every
    /// `registerComponent()` call runs at module scope before the entry's
    /// `Ready` handshake is sent — so this never changes for the lifetime of
    /// a `ReactBridge`. A registered component with no `schema` argument has
    /// no entry here.
    pub component_schemas: BTreeMap<String, ComponentPropertySchema>,
    /// The project property schema declared via `defineProjectProperties()`,
    /// or `None` when the entry declared none (distinct from an empty
    /// schema). Fields share `ComponentPropertyField`'s shape with component
    /// schemas; only where their values live differs (project-level vs per
    /// timeline item).
    pub project_property_schema: Option<BTreeMap<String, ComponentPropertyField>>,
}

/// One audible `<Audio>` element as reported for a single rendered frame
/// (`packages/react/src/render.ts`'s `AudioClipDescriptor`, gathered during
/// the same tree walk that produces that frame's layers). Because collection
/// happens per frame, an `<Audio>` behind an ordinary React conditional or
/// nested inside `<Sequence>`s contributes on exactly the frames where it
/// actually renders; the caller merges these reports into the export's
/// `AudioGraph`.
///
/// `start`/`duration` are composition-space seconds bounded by any enclosing
/// sequences; `source_start` is where in the source file the first audible
/// moment plays, already adjusted so head-clipping by outer sequences keeps
/// the source clock continuous.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactAudioClipDescriptor {
    pub src: String,
    pub source_start: f64,
    pub playback_rate: Animatable<f64>,
    pub volume: Animatable<f64>,
    pub muted: bool,
    pub start: f64,
    pub duration: f64,
}

/// One field of a `ComponentPropertySchema` declared on the TypeScript side
/// (`packages/react/src/registry.ts`). Mirrors `ComponentPropertyField`
/// there field-for-field; kept as a real enum here (rather than opaque JSON)
/// so a GUI Inspector can match on `field_type` to choose a widget without
/// re-parsing JSON itself.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ComponentPropertyField {
    String {
        #[serde(default)]
        label: Option<String>,
        default_value: String,
    },
    Number {
        #[serde(default)]
        label: Option<String>,
        default_value: f64,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
        #[serde(default)]
        step: Option<f64>,
    },
    Boolean {
        #[serde(default)]
        label: Option<String>,
        default_value: bool,
    },
    Color {
        #[serde(default)]
        label: Option<String>,
        default_value: String,
    },
    Select {
        #[serde(default)]
        label: Option<String>,
        default_value: String,
        options: Vec<String>,
    },
}

/// One registered component's declared props, keyed by prop name — the
/// Rust-side counterpart of `ComponentPropertySchema<Props>` in
/// `packages/react/src/registry.ts`.
pub type ComponentPropertySchema = BTreeMap<String, ComponentPropertyField>;

/// One component-resolution request: a `registerComponent()` name plus the
/// timeline item's configured props (see [`ReactBridge::resolve_components`]).
#[derive(Clone, Copy, Debug)]
pub struct ComponentResolutionRequest<'a> {
    pub component: &'a str,
    pub props: &'a BTreeMap<String, serde_json::Value>,
}

/// One frame's evaluation: the visual scene plus every `<Audio>` declaration
/// audible at that frame (see [`ReactAudioClipDescriptor`]).
#[derive(Clone, Debug)]
pub struct FrameEvaluation {
    pub scene: Scene,
    pub audio: Vec<ReactAudioClipDescriptor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReactCompositionConfig {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) frame_rate: Rational,
    pub(crate) duration_in_frames: u64,
}
