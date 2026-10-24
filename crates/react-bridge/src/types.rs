use celesta_composition::{
    Animatable, DuckGroups, DuckLevel, Layer, Rational, Scene, ShaderSource,
};
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
#[serde(rename_all = "camelCase")]
pub struct LayerSource {
    /// The element's component name, e.g. `Rect` or `Title`.
    pub name: String,
    /// The module holding the element, as an absolute path.
    pub file: PathBuf,
    /// 1-based line of the element's opening `<`.
    pub line: u32,
    /// 1-based column of the element's opening `<`.
    pub column: u32,
    /// Whether the element is a built-in primitive (`<Rect>`), a timing
    /// wrapper (`<Sequence>`), or a user-written component. Older runtimes do
    /// not say; they read as
    /// [`ElementKind::Component`].
    #[serde(default)]
    pub kind: ElementKind,
    /// The id of the layer this element emitted, when it is a primitive that
    /// emitted one.
    #[serde(default)]
    pub layer_id: Option<String>,
}

/// What kind of JSX element a [`LayerSource`] is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElementKind {
    /// A built-in element such as `<Rect>` that emits a layer itself.
    Primitive,
    /// A Celesta component that draws its caller's JSX, such as
    /// `<Sequence>` or `<Scenes>`.
    Wrapper,
    /// A user-written component that renders other elements.
    #[default]
    Component,
}

/// One scalar prop value an element received.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ReceivedValue {
    /// A finite number.
    Number(f64),
    /// A string.
    String(String),
    /// A boolean.
    Bool(bool),
}

/// The scalar props an element received, as written in the source. Each
/// transform prop is `None` when the element was not given it (or it was not
/// a plain number); every other number, string, or boolean prop is in
/// `others`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivedProps {
    /// The `x` prop.
    pub x: Option<f64>,
    /// The `y` prop.
    pub y: Option<f64>,
    /// The `rotation` prop.
    pub rotation: Option<f64>,
    /// The `scale` prop.
    pub scale: Option<f64>,
    /// The `scaleX` prop.
    pub scale_x: Option<f64>,
    /// The `scaleY` prop.
    pub scale_y: Option<f64>,
    /// Every other scalar prop, keyed as written (`fontSize`, `children`);
    /// a scalar in the `style` object is keyed `style.<key>`.
    #[serde(flatten)]
    pub others: BTreeMap<String, ReceivedValue>,
}

impl ReceivedProps {
    /// The value of `prop` (`x`, `fontSize`, `style.fontSize`, ...), from the
    /// named fields for the transform props and `others` for the rest.
    pub fn get(&self, prop: &str) -> Option<ReceivedValue> {
        let transform = match prop {
            "x" => self.x,
            "y" => self.y,
            "rotation" => self.rotation,
            "scale" => self.scale,
            "scaleX" => self.scale_x,
            "scaleY" => self.scale_y,
            _ => return self.others.get(prop).cloned(),
        };
        transform.map(ReceivedValue::Number)
    }
}

/// Everything the runtime knows about where a layer came from (see
/// [`ReactBridge::layer_source`](crate::ReactBridge::layer_source)).
#[derive(Clone, Debug, PartialEq)]
pub struct LayerSourceReport {
    /// The JSX chain, innermost element first; empty when the runtime did
    /// not record source positions.
    pub chain: Vec<LayerSource>,
    /// The frame `useCurrentFrame()` reports in the component that wrote the
    /// innermost element (above any `<Sequence>` inside it); 0 from runtimes
    /// that do not report it.
    pub local_frame: f64,
    /// The distinct frames `useCurrentFrame()` returns under each frame
    /// provider above the innermost element, nearest first. The component
    /// that read the frame an `interpolate(frame, …)` written for the element
    /// uses sits above it, so that frame is one of these. Empty from runtimes
    /// that do not report them; read those as `[local_frame]`.
    pub candidate_frames: Vec<f64>,
    /// The scalar props the innermost element received.
    pub props: ReceivedProps,
    /// How many layers of the frame the innermost element rendered; 1 from
    /// runtimes that do not count.
    pub render_count: u32,
    /// The timing wrappers around the innermost element, nearest first.
    /// Empty from runtimes that do not report them.
    pub windows: Vec<TimingWindow>,
    /// The composition frames the innermost element can show in: where all
    /// of `windows` overlap, cut to the composition. Empty when `to` is not
    /// past `from`. `None` from runtimes that do not report it.
    pub span: Option<FrameSpan>,
}

/// Where a JSX element was written.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
pub struct SourcePos {
    /// The module holding the element, as an absolute path.
    pub file: PathBuf,
    /// 1-based line of the element's opening `<`.
    pub line: u32,
    /// 1-based column of the element's opening `<`.
    pub column: u32,
}

/// A value the elements written at one position take for one frame, in
/// place of the written one: the editor's preview of a drag (see
/// [`ReactBridge::scene_at_with_overrides`](crate::ReactBridge::scene_at_with_overrides)).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PropOverride {
    /// The module holding the element, as an absolute path.
    pub file: PathBuf,
    /// 1-based line of the element's opening `<`, as in [`SourcePos`].
    pub line: u32,
    /// 1-based column of the element's opening `<`.
    pub column: u32,
    pub prop: String,
    pub value: f64,
}

impl From<&LayerSource> for SourcePos {
    fn from(source: &LayerSource) -> Self {
        Self {
            file: source.file.clone(),
            line: source.line,
            column: source.column,
        }
    }
}

/// The frames a timing wrapper around an element shows its children in (see
/// [`LayerSourceReport::windows`]).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TimingWindow {
    /// The wrapper: `Sequence`, `FreezeFrame` or `Loop`.
    pub kind: String,
    /// First frame, in composition frames; may be fractional.
    pub from: f64,
    /// The frame after the last, in composition frames; may be fractional.
    pub to: f64,
    /// Where the wrapper was written; `None` for one the runtime made, such
    /// as a `<Scenes>` scene.
    #[serde(default)]
    pub source: Option<SourcePos>,
    /// Inside a `<Loop>` or `<FreezeFrame>` (or that wrapper itself), whose
    /// children see other frames than the composition's: `from` and `to` are
    /// then the window around the remapping, not one of the wrapper's own.
    #[serde(default)]
    pub remapped: bool,
}

/// A span of composition frames, `to` exclusive; may be fractional.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct FrameSpan {
    pub from: f64,
    pub to: f64,
}

/// A file or directory the entry's `prepare()` read (for instance a voice
/// folder outside the entry's directory), with its state when it was read.
/// The editor reloads the entry when one of these changes on disk.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PrepareDependency {
    /// Absolute path.
    pub path: PathBuf,
    /// Modification time in nanoseconds since the Unix epoch, as a decimal
    /// string; `None` when the path did not exist.
    #[serde(rename = "mtimeNs")]
    pub mtime_ns: Option<String>,
    /// Size in bytes; `None` when the path did not exist or is a directory
    /// (Node and Rust report a directory's size differently on Windows).
    pub size: Option<u64>,
}

impl PrepareDependency {
    /// `path`'s state now, in the form the runtime reports it.
    pub fn sample(path: &std::path::Path) -> Self {
        let metadata = std::fs::metadata(path).ok();
        Self {
            path: path.to_owned(),
            mtime_ns: metadata
                .as_ref()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|since| since.as_nanos().to_string()),
            size: metadata
                .filter(|metadata| !metadata.is_dir())
                .map(|metadata| metadata.len()),
        }
    }
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
    /// Files and directories `prepare()` read, beyond the entry's own
    /// sources. Empty for an entry that reads none.
    pub prepare_dependencies: Vec<PrepareDependency>,
    /// The selected composition's `<Composition chapters>`; empty when it
    /// declares none.
    pub chapters: Vec<ReactChapter>,
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
    /// Transposition in semitones. Reported only when not 0.
    #[serde(
        default = "celesta_composition::zero_pitch",
        skip_serializing_if = "celesta_composition::is_zero_pitch"
    )]
    pub pitch: Animatable<f64>,
    /// Playback rate changes duration only, not pitch. Reported only when
    /// set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub preserve_pitch: bool,
    /// Seconds the clip rises from silence over after `start`. Reported only
    /// when not 0.
    #[serde(default, skip_serializing_if = "is_zero_seconds")]
    pub fade_in: f64,
    /// Seconds the clip falls to silence over before its end. Reported only
    /// when not 0.
    #[serde(default, skip_serializing_if = "is_zero_seconds")]
    pub fade_out: f64,
    /// The soundtrack of a `<Video>`, not an `<Audio>`. Reported only when
    /// set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub video: bool,
    /// The group the clip's sound belongs to. Reported only when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duck_group: Option<String>,
    /// How the clip ducks, as authored. Reported only when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duck: Option<ReactDuck>,
}

/// `<Audio duck>` as authored, in seconds; `AudioDuck::resolve` applies the
/// defaults.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactDuck {
    pub under: DuckGroups,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<DuckLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_gap: Option<f64>,
}

fn is_zero_seconds(seconds: &f64) -> bool {
    *seconds == 0.0
}

/// One subtitle a frame shows, from `<Subtitles>` or a `<Dialogue>` line:
/// its text and the composition seconds it is shown for, already cut to the
/// enclosing sequences. Every frame showing it reports it again.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ReactSubtitleDescriptor {
    pub text: String,
    pub start: f64,
    pub end: f64,
    /// `<Subtitles lang>`, else the `lang` of the enclosing sequence, group
    /// or composition; `None` when nothing sets one.
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
    /// Absent when the composition declares none, and from older runtimes.
    #[serde(default)]
    pub(crate) chapters: Vec<ReactChapter>,
}

/// One chapter of `<Composition chapters>`: a named span of the composition
/// the editor lists and seeks to. The runtime has checked it: ids are unique
/// and non-empty, the list is ordered by `from`, every chapter starts before
/// the composition's end and is cut there. Chapters may overlap or leave
/// gaps; one of length 0 marks a point.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactChapter {
    pub id: String,
    /// The label to show; the runtime fills in `id` when none was given.
    pub name: String,
    /// First frame, in the composition's frames.
    pub from: u64,
    pub duration_in_frames: u64,
}

impl ReactChapter {
    /// The frame after the chapter's last.
    pub fn end(&self) -> u64 {
        self.from + self.duration_in_frames
    }
}

/// The chapter showing at `frame`: the latest-starting one whose span holds
/// it, so where chapters overlap the later one is current. `None` in a gap,
/// past the last chapter, and for chapters of length 0, which are never
/// current.
pub fn chapter_at(chapters: &[ReactChapter], frame: u64) -> Option<&ReactChapter> {
    chapters
        .iter()
        .filter(|chapter| chapter.from <= frame && frame < chapter.end())
        .max_by_key(|chapter| chapter.from)
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
