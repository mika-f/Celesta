use celesta_composition::{Animatable, Layer, Rational, Scene, ShaderSource};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

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

/// Options for [`ReactBridge::spawn_with_options`](crate::ReactBridge::spawn_with_options).
#[derive(Clone, Debug, Default)]
pub struct SpawnOptions {
    /// Root composition ID; absent selects the first root.
    pub composition_id: Option<String>,
    /// Makes the runtime record the source position of every JSX element so
    /// [`ReactBridge::layer_source`](crate::ReactBridge::layer_source) can
    /// answer. Costs render time, so only the editor preview asks for it.
    pub source_locations: bool,
}

/// One element of the chain of JSX that produced a layer (see
/// [`ReactBridge::layer_source`](crate::ReactBridge::layer_source)).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct LayerSource {
    /// The element's component name, e.g. `Rect` or `Title`.
    pub name: String,
    /// The module holding the element, as an absolute path.
    pub file: PathBuf,
    /// 1-based line of the element's opening `<`.
    pub line: u32,
    /// 1-based column of the element's opening `<`.
    pub column: u32,
}

/// Static composition facts read once from the entry's `<Composition>` root.
#[derive(Clone, Debug, PartialEq)]
pub struct ReactCompositionMetadata {
    /// The selected root ("default" for an unnamed single root).
    pub composition_id: String,
    /// Root compositions in declaration order. Properties are shared by the entry.
    pub compositions: Vec<ReactCompositionInfo>,
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
    /// The keys of `project_property_schema` in the order the entry's
    /// `defineProjectProperties()` declared them (the map itself sorts).
    /// Empty when the entry declared none.
    pub project_property_order: Vec<String>,
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
    /// Starts the source over from its beginning past its end. Reported only
    /// when set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub r#loop: bool,
    /// The soundtrack of a `<Video>`, not an `<Audio>`. Reported only when
    /// set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub video: bool,
}

/// One subtitle a frame shows, from `<Subtitles>` or a `<Dialogue>` line:
/// its text and the composition seconds it is shown for, already cut to the
/// enclosing sequences. Every frame showing it reports it again.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ReactSubtitleDescriptor {
    pub text: String,
    pub start: f64,
    pub end: f64,
    /// The language tag of its `<Subtitles lang>`; `None` for a `<Dialogue>`
    /// line or a `<Subtitles>` without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
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
    /// A local file or an http(s) URL.
    Path {
        #[serde(default)]
        label: Option<String>,
        default_value: String,
    },
}

impl ComponentPropertyField {
    /// The declared default as the JSON value a props file would hold.
    pub fn default_json(&self) -> serde_json::Value {
        use serde_json::Value;
        match self {
            Self::String { default_value, .. }
            | Self::Color { default_value, .. }
            | Self::Select { default_value, .. }
            | Self::Path { default_value, .. } => Value::String(default_value.clone()),
            Self::Number { default_value, .. } => {
                serde_json::Number::from_f64(*default_value).map_or(Value::Null, Value::Number)
            }
            Self::Boolean { default_value, .. } => Value::Bool(*default_value),
        }
    }
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

/// What one [`ReactBridge::resolve_components`] call produced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComponentResolutions {
    /// One rendered layer list per request, in order, with `None` for names
    /// nothing registered.
    pub components: Vec<Option<Vec<Layer>>>,
    /// The custom shaders those layers use, each once. Whoever places the
    /// layers in a scene adds these to its `shaders`, skipping ids it
    /// already lists, or the GPU renderer rejects the scene.
    pub shaders: Vec<ShaderSource>,
}

/// One frame's evaluation: the visual scene plus every `<Audio>` declaration
/// audible at that frame (see [`ReactAudioClipDescriptor`]) and every
/// subtitle it shows (see [`ReactSubtitleDescriptor`]).
#[derive(Clone, Debug)]
pub struct FrameEvaluation {
    pub scene: Scene,
    pub audio: Vec<ReactAudioClipDescriptor>,
    pub subtitles: Vec<ReactSubtitleDescriptor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReactCompositionConfig {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) frame_rate: Rational,
    pub(crate) duration_in_frames: u64,
}

/// A selectable root composition declared by an entry.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactCompositionInfo {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub duration_in_frames: u64,
}
