#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::ffi::OsStr;
use std::num::{NonZeroU16, NonZeroU32};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use celesta_composition::{
    Animatable, AudioClip, AudioGraph, Layer, LayerContent, Rational, Scene, Time, evaluate_f64,
    integrate_f64,
};
use celesta_editor_core::{
    AssetSummary, CharacterSummary, ClipKind, ComponentClipSummary, DialogueClipSummary,
    EditorDocument, TimelineClock, TrackSummary,
};
use celesta_exporter::{
    ExportCancellation, ExportError, ExportOptions, ExportProgress, ExportRange, Exporter,
    ReactRuntimeOptions,
};
use celesta_gpu_renderer::{GpuRenderOptions, GpuRenderer, PreviewFrame as GpuPreviewFrame};
use celesta_media::{
    AudioBuffer, AudioDecoder, FfmpegBackend, MediaError, MediaProbe, mix_audio_graph_cancellable,
};
use celesta_project::{AssetKind, Project, TrackKind};
use celesta_react_bridge::{
    ComponentPropertyField, ComponentPropertySchema, ComponentResolutionRequest, ReactBridge,
    ReactCompositionMetadata,
};
use celesta_remote::resolve_asset_path;
use gpui_kit::base::GlobalState;
use gpui_kit::{
    App, Bounds, ClickEvent, Context, Entity, FocusHandle, KeyBinding, KeyDownEvent, Menu,
    MenuItem, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    PathPromptOptions, Pixels, Point, RenderImage, SharedString, StyledImage, Window, WindowBounds,
    WindowOptions, actions, div, img, prelude::*, px, relative, rgb, size,
};
use image::{Frame, ImageBuffer, Rgba};
use rodio::{DeviceSinkBuilder, Player, buffer::SamplesBuffer};

mod audio_cache;

use audio_cache::DiskAudioCache;
use celesta_editor_theme as theme;
use gpui_kit::component::button::{Button, ButtonVariants as _};
#[cfg(not(target_os = "macos"))]
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::resizable::{ResizableState, h_resizable, resizable_panel, v_resizable};
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Root, Selectable as _, Sizable as _, TitleBar,
    WindowExt as _,
};

const EDITOR_DEMO_PROJECT: &str = include_str!("../../../examples/editor-demo.celesta.json");

use celesta_react_bridge::runtime_paths as react_runtime_paths;
use celesta_react_bridge::{
    ProjectTsconfig, project_types_template, refresh_project_types, set_up_project_types,
};

actions!(
    celesta_editor,
    [
        OpenProject,
        ReloadProject,
        CloseWindow,
        Quit,
        ExportProject,
        SetExportIn,
        SetExportOut,
        ClearExportRange,
        SetUpTypeScript,
        TogglePlayback,
        PreviousFrame,
        NextFrame
    ]
);

struct AudioPreview {
    _device_sink: rodio::MixerDeviceSink,
    player: Player,
    source: SamplesBuffer,
    /// Monitor gain applied on top of the mixed buffer. Carried here because
    /// every seek reconnects a fresh `Player`, which starts at unity gain.
    volume: f32,
}

struct PreviewRequest {
    generation: u64,
    scene: Scene,
    asset_root: PathBuf,
    /// The project's `react_entry` and the runtime that serves it, when one
    /// is configured. `None` means component clips have nothing to resolve
    /// against this frame.
    react: Option<ReactPreviewContext>,
    /// `WholeScene` replaces `scene` entirely with the React entry's own
    /// evaluation at `scene.time` (standalone `.tsx` preview); the default
    /// `ResolveComponents` only fills in `TimelineContent::Component` clips.
    react_mode: ReactPreviewMode,
    /// Bumped by the reload watcher so the preview worker drops its cached
    /// `ReactPreviewBridge` and respawns Node, picking up a re-bundle after
    /// the composition's code changed.
    react_reload: u64,
}

struct PreviewResult {
    generation: u64,
    frame: Result<PreviewPresentation, String>,
    /// Recoverable diagnostics for this frame: unresolved `registerComponent`
    /// names (hidden from the preview) and React runtime failures.
    warnings: Vec<String>,
}

/// The Node.js runtime a preview's `TimelineContent::Component` clips resolve
/// against — the same paths `ComponentSchemaWorker` uses.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ReactPreviewContext {
    node: PathBuf,
    cli_script: PathBuf,
    entry: PathBuf,
}

/// How the preview worker should treat this frame's React entry: resolve just
/// the `TimelineContent::Component` clips a `project.json` placed (the normal
/// GUI-project case), or render the whole composition the entry describes
/// (standalone `.tsx` preview mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReactPreviewMode {
    ResolveComponents,
    WholeScene,
}

/// Editor state for previewing a standalone React composition entry (`.tsx`).
/// The [`EditorDocument`] is a synthetic, unsaveable project holding only the
/// composition's dimensions/frame rate/duration; every previewed frame and the
/// audio graph come from the React bridge instead of evaluating tracks.
struct ReactPreview {
    entry: PathBuf,
    /// Newest source-file modification time seen under the entry's directory.
    /// The reload watcher compares against this to notice code edits.
    watched_mtime: Option<SystemTime>,
}

/// True when `path` should open as a standalone React composition rather than
/// a `project.json`. A `*.celesta.json` is always a project; a JS/TS module
/// extension is a React entry.
fn is_react_entry(path: &Path) -> bool {
    if path
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.ends_with(".celesta.json"))
    {
        return false;
    }
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some("tsx" | "ts" | "jsx" | "js" | "mjs" | "cjs")
    )
}

/// Loads what the viewer shows for `path`: a `project.json`, a standalone React
/// entry (whose composition facts come from a blocking Node handshake), or the
/// built-in demo when `path` is `None`. Free of GPUI state so it can run on a
/// background thread when a file is opened from the menu.
fn load_source(path: Option<&Path>) -> Result<(EditorDocument, Option<ReactPreview>), String> {
    let loaded = match path {
        Some(path) if is_react_entry(path) => {
            let (node, cli_script) = react_runtime_paths();
            let metadata = ReactBridge::spawn(&node, &cli_script, path)
                .map_err(|error| error.to_string())?
                .metadata()
                .clone();
            let document = EditorDocument::react_preview(
                path,
                metadata.width,
                metadata.height,
                metadata.frame_rate,
                REACT_PREVIEW_SAMPLE_RATE,
                metadata.duration_in_frames,
            )
            .map_err(|error| error.to_string())?;
            let entry = document
                .react_entry_absolute_path()
                .unwrap_or_else(|| path.to_owned());
            let watched_mtime = entry.parent().and_then(newest_source_mtime);
            Ok((
                document,
                Some(ReactPreview {
                    entry,
                    watched_mtime,
                }),
            ))
        }
        Some(path) => EditorDocument::load(path)
            .map(|document| (document, None))
            .map_err(|error| error.to_string()),
        None => EditorDocument::from_json(EDITOR_DEMO_PROJECT, "examples")
            .map(|document| (document, None))
            .map_err(|error| error.to_string()),
    };
    if let Ok((document, _)) = &loaded {
        refresh_typescript_support(document);
    }
    loaded
}

/// Keeps a project that ran File > Set Up TypeScript on this build's
/// declarations. Best effort: stale types must not block opening the project.
fn refresh_typescript_support(document: &EditorDocument) {
    let Some(entry) = document.react_entry_absolute_path() else {
        return;
    };
    let (_, cli_script) = react_runtime_paths();
    if let Err(error) = refresh_project_types(&project_types_template(&cli_script), &entry) {
        eprintln!("Celesta: couldn’t refresh TypeScript support: {error}");
    }
}

/// Default sample rate for a standalone React entry's audio graph — the entry
/// has no `project.json` to source one from. Matches `celesta-exporter`'s
/// `DEFAULT_REACT_AUDIO_SAMPLE_RATE` and every checked-in example project.
const REACT_PREVIEW_SAMPLE_RATE: u32 = 48_000;

/// Newest mtime among the JS/TS/JSON source files under `dir` (recursively,
/// skipping `node_modules`, `dist`, and `.tmp`). Used to notice when a React
/// composition's code — the entry or any module it bundles — changed on disk.
fn newest_source_mtime(dir: &Path) -> Option<SystemTime> {
    fn walk(dir: &Path, newest: &mut Option<SystemTime>, depth: u32) {
        if depth > 8 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if !matches!(
                    path.file_name().and_then(OsStr::to_str),
                    Some("node_modules" | "dist" | ".tmp" | ".git")
                ) {
                    walk(&path, newest, depth + 1);
                }
            } else if matches!(
                path.extension().and_then(OsStr::to_str),
                Some("tsx" | "ts" | "jsx" | "js" | "mjs" | "cjs" | "json" | "css")
            ) && let Ok(modified) = entry.metadata().and_then(|meta| meta.modified())
            {
                *newest = Some(newest.map_or(modified, |current| current.max(modified)));
            }
        }
    }
    let mut newest = None;
    walk(dir, &mut newest, 0);
    newest
}

struct CpuPreviewFrame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

#[derive(Clone)]
enum PreviewPresentation {
    Image(Arc<RenderImage>),
    #[cfg(target_os = "macos")]
    Surface(celesta_gpu_renderer::NativePreviewFrame),
}

struct PreviewWorker {
    requests: mpsc::Sender<PreviewRequest>,
    results: mpsc::Receiver<PreviewResult>,
}

struct MediaProbeRequest {
    generation: u64,
    assets: Vec<ProbeAsset>,
}

struct ProbeAsset {
    id: String,
    path: PathBuf,
}

struct MediaProbeResult {
    generation: u64,
    assets: Vec<(String, Result<MediaAssetInfo, String>)>,
}

#[derive(Clone)]
struct MediaAssetInfo {
    duration: Option<Time>,
    video_size: Option<(u32, u32)>,
    has_audio: bool,
}

struct MediaProbeWorker {
    requests: mpsc::Sender<MediaProbeRequest>,
    results: mpsc::Receiver<MediaProbeResult>,
}

impl MediaProbeWorker {
    fn spawn() -> Result<Self, Box<dyn Error>> {
        let (request_tx, request_rx) = mpsc::channel::<MediaProbeRequest>();
        let (result_tx, result_rx) = mpsc::channel::<MediaProbeResult>();
        thread::Builder::new()
            .name("celesta-media-probe".to_owned())
            .spawn(move || {
                let mut backend = FfmpegBackend::new();
                while let Ok(first) = request_rx.recv() {
                    let request = take_latest(first, &request_rx);
                    let assets = request
                        .assets
                        .into_iter()
                        .map(|asset| {
                            let result = backend
                                .probe(&asset.path)
                                .map(media_asset_info)
                                .map_err(|error| error.to_string());
                            (asset.id, result)
                        })
                        .collect();
                    if result_tx
                        .send(MediaProbeResult {
                            generation: request.generation,
                            assets,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests: request_tx,
            results: result_rx,
        })
    }

    fn request(&self, request: MediaProbeRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "media probe worker stopped unexpectedly".to_owned())
    }
}

struct ComponentSchemaRequest {
    generation: u64,
    node: PathBuf,
    cli_script: PathBuf,
    entry: PathBuf,
}

struct ComponentSchemaResult {
    generation: u64,
    schemas: Result<BTreeMap<String, ComponentPropertySchema>, String>,
    project_property_schema: Option<BTreeMap<String, ComponentPropertyField>>,
}

/// Queries a `.tsx` entry's registered `registerComponent()` schemas by
/// spawning the same `@celesta/react` Node.js runtime `celesta-exporter --react`
/// uses, reading them off `ReactBridge::metadata` (populated during the
/// startup handshake, before any frame is requested), then dropping the
/// process — the editor's own preview never renders React content, so
/// nothing else needs this connection to stay open. `ReactBridge::spawn` is
/// a blocking call (it blocks on the child process's first stdout line), so
/// this runs on its own thread like the other editor workers rather than on
/// the GPUI render thread.
struct ComponentSchemaWorker {
    requests: mpsc::Sender<ComponentSchemaRequest>,
    results: mpsc::Receiver<ComponentSchemaResult>,
}

impl ComponentSchemaWorker {
    fn spawn() -> Result<Self, Box<dyn Error>> {
        let (request_tx, request_rx) = mpsc::channel::<ComponentSchemaRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ComponentSchemaResult>();
        thread::Builder::new()
            .name("celesta-component-schema".to_owned())
            .spawn(move || {
                while let Ok(first) = request_rx.recv() {
                    let request = take_latest(first, &request_rx);
                    let result =
                        ReactBridge::spawn(&request.node, &request.cli_script, &request.entry);
                    let schemas = result
                        .as_ref()
                        .map(|bridge| {
                            bridge
                                .metadata()
                                .component_schemas
                                .iter()
                                .map(|(name, schema)| (name.clone(), schema.clone()))
                                .collect()
                        })
                        .map_err(|error| error.to_string());
                    let project_property_schema = result
                        .ok()
                        .and_then(|bridge| bridge.metadata().project_property_schema.clone());
                    if result_tx
                        .send(ComponentSchemaResult {
                            generation: request.generation,
                            schemas,
                            project_property_schema,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests: request_tx,
            results: result_rx,
        })
    }

    fn request(&self, request: ComponentSchemaRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "component schema worker stopped unexpectedly".to_owned())
    }
}

struct ReactAudioRequest {
    generation: u64,
    node: PathBuf,
    cli_script: PathBuf,
    entry: PathBuf,
    sample_rate: u32,
    master_volume: f64,
}

struct ReactAudioResult {
    generation: u64,
    graph: Result<AudioGraph, String>,
}

/// Spawns a transient `@celesta/react` process, sweeps every frame of a
/// standalone entry for its `<Audio>` declarations, and returns the assembled
/// [`AudioGraph`] — the standalone-preview counterpart of the audio graph
/// `celesta-exporter` accumulates while rendering. Runs on its own thread
/// because `ReactBridge::spawn` and the per-frame sweep both block.
struct ReactAudioWorker {
    requests: mpsc::Sender<ReactAudioRequest>,
    results: mpsc::Receiver<ReactAudioResult>,
}

impl ReactAudioWorker {
    fn spawn() -> Result<Self, Box<dyn Error>> {
        let (request_tx, request_rx) = mpsc::channel::<ReactAudioRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ReactAudioResult>();
        thread::Builder::new()
            .name("celesta-react-audio".to_owned())
            .spawn(move || {
                while let Ok(first) = request_rx.recv() {
                    let request = take_latest(first, &request_rx);
                    let entry_dir = request
                        .entry
                        .parent()
                        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
                    let graph =
                        ReactBridge::spawn(&request.node, &request.cli_script, &request.entry)
                            .map_err(|error| error.to_string())
                            .and_then(|mut bridge| {
                                bridge
                                    .collect_audio_graph(
                                        request.sample_rate,
                                        request.master_volume,
                                        &entry_dir,
                                    )
                                    .map_err(|error| error.to_string())
                            });
                    if result_tx
                        .send(ReactAudioResult {
                            generation: request.generation,
                            graph,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests: request_tx,
            results: result_rx,
        })
    }

    fn request(&self, request: ReactAudioRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "react audio worker stopped unexpectedly".to_owned())
    }
}

/// The preview worker's long-lived connection to a React entry, respawned
/// whenever the project's `react_entry` (or runtime paths) change. A failed
/// spawn or a dead connection is remembered per context so a permanent
/// failure does not restart Node on every rendered frame.
struct ReactPreviewBridge {
    context: ReactPreviewContext,
    bridge: Option<ReactBridge>,
    failure: Option<String>,
}

impl PreviewWorker {
    fn spawn(mut renderer: GpuRenderer) -> Result<Self, Box<dyn Error>> {
        let (request_tx, request_rx) = mpsc::channel::<PreviewRequest>();
        let (result_tx, result_rx) = mpsc::channel::<PreviewResult>();
        thread::Builder::new()
            .name("celesta-preview".to_owned())
            .spawn(move || {
                let mut react_bridge: Option<ReactPreviewBridge> = None;
                let mut react_reload = 0u64;
                while let Ok(first) = request_rx.recv() {
                    let request = take_latest(first, &request_rx);
                    renderer.set_asset_root(&request.asset_root);
                    if request.react_reload != react_reload {
                        react_reload = request.react_reload;
                        react_bridge = None;
                    }
                    let mut scene = request.scene;
                    let warnings = match request.react_mode {
                        ReactPreviewMode::WholeScene => render_whole_react_scene(
                            &mut scene,
                            &mut react_bridge,
                            request.react.as_ref(),
                        ),
                        ReactPreviewMode::ResolveComponents => resolve_preview_components(
                            &mut scene,
                            &mut react_bridge,
                            request.react.as_ref(),
                        ),
                    };
                    let frame = renderer
                        .render_preview(&scene)
                        .map_err(|error| error.to_string())
                        .map(prepare_preview_frame);
                    if result_tx
                        .send(PreviewResult {
                            generation: request.generation,
                            frame,
                            warnings,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests: request_tx,
            results: result_rx,
        })
    }

    fn request(&self, request: PreviewRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "preview worker stopped unexpectedly".to_owned())
    }
}

/// Spawns (or reuses) the preview worker's long-lived React connection for
/// `react`. Respawns only when the entry or runtime paths changed; a spawn
/// failure is remembered on the returned state so callers do not restart Node
/// on every frame.
fn ensure_react_bridge<'a>(
    react_bridge: &'a mut Option<ReactPreviewBridge>,
    react: &ReactPreviewContext,
) -> &'a mut ReactPreviewBridge {
    if react_bridge
        .as_ref()
        .is_none_or(|state| state.context != *react)
    {
        *react_bridge = Some(
            match ReactBridge::spawn(&react.node, &react.cli_script, &react.entry) {
                Ok(bridge) => ReactPreviewBridge {
                    context: react.clone(),
                    bridge: Some(bridge),
                    failure: None,
                },
                Err(error) => ReactPreviewBridge {
                    context: react.clone(),
                    bridge: None,
                    failure: Some(error.to_string()),
                },
            },
        );
    }
    react_bridge.as_mut().expect("react bridge was just set")
}

/// Replaces the whole preview scene with the standalone React entry's own
/// evaluation at `scene.time` — the editor's synthetic project has no tracks,
/// so this is the only source of layers in `.tsx` preview mode. A bridge
/// failure is remembered and surfaced as a warning; the (empty) scene is left
/// in place so the preview still clears to the composition background.
fn render_whole_react_scene(
    scene: &mut Scene,
    react_bridge: &mut Option<ReactPreviewBridge>,
    react: Option<&ReactPreviewContext>,
) -> Vec<String> {
    let Some(react) = react else {
        return vec!["React runtime is unavailable for this preview".to_owned()];
    };
    let state = ensure_react_bridge(react_bridge, react);
    match &mut state.bridge {
        Some(bridge) => match bridge.scene_at_with_project(scene.time, None) {
            Ok(evaluated) => {
                *scene = evaluated;
                Vec::new()
            }
            Err(error) => {
                state.bridge = None;
                state.failure = Some(error.to_string());
                vec![format!("React preview failed: {error}")]
            }
        },
        None => vec![format!(
            "React preview unavailable: {}",
            state.failure.as_deref().unwrap_or("unknown error")
        )],
    }
}

/// Replaces every `missingComponent` layer in the scene with its registered
/// component's rendered layers (wrapped in the original layer shell so the
/// timeline item's evaluated transform and opacity still place it), dropping
/// unresolved ones from the preview and reporting them as warnings instead —
/// an unrenderable component would otherwise fail the whole GPU render.
fn resolve_preview_components(
    scene: &mut Scene,
    react_bridge: &mut Option<ReactPreviewBridge>,
    react: Option<&ReactPreviewContext>,
) -> Vec<String> {
    let mut requests = Vec::new();
    collect_component_requests(&scene.layers, &mut requests);
    if requests.is_empty() {
        return Vec::new();
    }

    let Some(react) = react else {
        let mut warnings =
            vec!["React Entry is not set; component clips are hidden from the preview".to_owned()];
        append_component_names(&requests, &mut warnings);
        strip_missing_components(&mut scene.layers);
        return warnings;
    };

    let state = ensure_react_bridge(react_bridge, react);
    let mut warnings = Vec::new();
    match &mut state.bridge {
        Some(bridge) => {
            let resolution_requests: Vec<ComponentResolutionRequest> = requests
                .iter()
                .map(|(component, props)| ComponentResolutionRequest { component, props })
                .collect();
            match bridge.resolve_components(&resolution_requests, scene.time) {
                Ok(resolutions) => {
                    let mut cursor = 0;
                    let mut unresolved = Vec::new();
                    splice_resolved_components(
                        &mut scene.layers,
                        &resolutions,
                        &mut cursor,
                        &mut unresolved,
                    );
                    if !unresolved.is_empty() {
                        warnings
                            .push("Unresolved component(s) hidden from the preview:".to_owned());
                        for name in unresolved {
                            warnings.push(format!("  {name}"));
                        }
                        warnings.push(
                            "Register them with registerComponent() in the React entry.".to_owned(),
                        );
                    }
                }
                Err(error) => {
                    // The Node process is no longer trustworthy; remember the
                    // failure until the entry changes rather than respawning
                    // on every frame.
                    state.bridge = None;
                    state.failure = Some(error.to_string());
                    warnings.push("React component resolution failed; component clips are hidden from the preview".to_owned());
                    append_component_names(&requests, &mut warnings);
                    strip_missing_components(&mut scene.layers);
                }
            }
        }
        None => {
            if let Some(failure) = &state.failure {
                warnings.push(format!("React preview unavailable: {failure}"));
            }
            warnings.push("Component clips are hidden from the preview".to_owned());
            append_component_names(&requests, &mut warnings);
            strip_missing_components(&mut scene.layers);
        }
    }
    warnings
}

fn append_component_names(
    requests: &[(String, BTreeMap<String, serde_json::Value>)],
    warnings: &mut Vec<String>,
) {
    let mut seen = Vec::new();
    for (component, _) in requests {
        if !seen.contains(component) {
            seen.push(component.clone());
        }
    }
    for name in seen {
        warnings.push(format!("  {name}"));
    }
}

fn collect_component_requests(
    layers: &[Layer],
    out: &mut Vec<(String, BTreeMap<String, serde_json::Value>)>,
) {
    for layer in layers {
        match &layer.content {
            LayerContent::MissingComponent { component, props } => {
                out.push((component.clone(), props.clone()));
            }
            LayerContent::Group { layers } => collect_component_requests(layers, out),
            _ => {}
        }
    }
}

fn strip_missing_components(layers: &mut Vec<Layer>) {
    let mut index = 0;
    while index < layers.len() {
        match &mut layers[index].content {
            LayerContent::Group { layers: children } => strip_missing_components(children),
            LayerContent::MissingComponent { .. } => {
                layers.remove(index);
                continue;
            }
            _ => {}
        }
        index += 1;
    }
}

/// Walks in the same order [`collect_component_requests`] did, replacing the
/// `cursor`-th missing-component layer with its resolution (a group of the
/// component's own layers inside the original layer shell) or removing it
/// when unresolved.
fn splice_resolved_components(
    layers: &mut Vec<Layer>,
    resolutions: &[Option<Vec<Layer>>],
    cursor: &mut usize,
    unresolved: &mut Vec<String>,
) {
    let mut index = 0;
    while index < layers.len() {
        match &layers[index].content {
            LayerContent::Group { .. } => {
                if let LayerContent::Group { layers: children } = &mut layers[index].content {
                    splice_resolved_components(children, resolutions, cursor, unresolved);
                }
                index += 1;
            }
            LayerContent::MissingComponent { component, .. } => {
                let resolved = resolutions.get(*cursor).cloned().flatten();
                *cursor += 1;
                match resolved {
                    Some(children) => {
                        layers[index].content = LayerContent::Group { layers: children };
                        index += 1;
                    }
                    None => {
                        unresolved.push(component.clone());
                        layers.remove(index);
                    }
                }
            }
            _ => index += 1,
        }
    }
}

struct AudioMixRequest {
    generation: u64,
    graph: AudioGraph,
    asset_root: PathBuf,
    duration: Time,
}

struct AudioMixResult {
    generation: u64,
    output: Result<AudioMixOutput, String>,
}

struct AudioMixOutput {
    clip_waveforms: HashMap<String, Vec<f32>>,
    clip_levels: HashMap<String, Vec<f32>>,
    buffer: AudioBuffer,
}

struct ExportRequest {
    source: ExportSource,
    asset_root: PathBuf,
    output: PathBuf,
    range: Option<ExportRange>,
    cancellation: ExportCancellation,
}

/// What the export worker renders: a normal `project.json`, or a standalone
/// React composition entry (`.tsx` preview mode). The React path has no
/// export range (the whole composition is always rendered).
enum ExportSource {
    Project(Box<Project>),
    ReactEntry {
        entry: PathBuf,
        node: PathBuf,
        cli_script: PathBuf,
    },
}

enum ExportEvent {
    Progress(ExportProgress),
    Finished {
        output: PathBuf,
        result: Result<(), String>,
        cancelled: bool,
    },
}

struct ExportWorker {
    requests: mpsc::Sender<ExportRequest>,
    events: mpsc::Receiver<ExportEvent>,
}

impl ExportWorker {
    fn spawn() -> Result<Self, Box<dyn Error>> {
        let (request_tx, request_rx) = mpsc::channel::<ExportRequest>();
        let (event_tx, event_rx) = mpsc::channel::<ExportEvent>();
        thread::Builder::new()
            .name("celesta-export".to_owned())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    let exporter = Exporter::new(ExportOptions {
                        overwrite: true,
                        range: request.range,
                        ..ExportOptions::default()
                    });
                    let progress = |progress| {
                        let _ = event_tx.send(ExportEvent::Progress(progress));
                    };
                    let result = match &request.source {
                        ExportSource::Project(project) => exporter.export_project_cancellable(
                            project,
                            &request.asset_root,
                            &request.output,
                            &request.cancellation,
                            progress,
                        ),
                        ExportSource::ReactEntry {
                            entry,
                            node,
                            cli_script,
                        } => exporter.export_react_entry_cancellable(
                            entry,
                            &ReactRuntimeOptions::new(node.clone(), cli_script.clone()),
                            &request.output,
                            &request.cancellation,
                            progress,
                        ),
                    };
                    let cancelled = matches!(result, Err(ExportError::Cancelled));
                    if event_tx
                        .send(ExportEvent::Finished {
                            output: request.output,
                            result: result.map_err(|error| error.to_string()),
                            cancelled,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests: request_tx,
            events: event_rx,
        })
    }

    fn request(&self, request: ExportRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "export worker stopped unexpectedly".to_owned())
    }
}

struct MasterVolumeDrag {
    pointer_x: Pixels,
    start_volume: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct AudioCacheKey {
    path: PathBuf,
    sample_rate: u32,
    channels: u16,
}

struct CachedAudioDecoder {
    backend: FfmpegBackend,
    buffers: HashMap<AudioCacheKey, AudioBuffer>,
    waveforms: HashMap<AudioCacheKey, Vec<f32>>,
    disk_cache: DiskAudioCache,
}

impl CachedAudioDecoder {
    fn new() -> Self {
        Self::with_disk_cache(DiskAudioCache::standard())
    }

    fn with_disk_cache(disk_cache: DiskAudioCache) -> Self {
        Self {
            backend: FfmpegBackend::new(),
            buffers: HashMap::new(),
            waveforms: HashMap::new(),
            disk_cache,
        }
    }

    fn clip_waveform(
        &self,
        path: &Path,
        sample_rate: u32,
        channels: u16,
        clip: &AudioClip,
    ) -> Option<Vec<f32>> {
        let key = AudioCacheKey {
            path: path.to_owned(),
            sample_rate,
            channels,
        };
        let peaks = self.waveforms.get(&key)?;
        let source_frames = self.buffers.get(&key)?.frame_count();
        Some(map_clip_waveform(
            peaks,
            source_frames,
            sample_rate,
            clip,
            peaks.len(),
        ))
    }
}

impl AudioDecoder for CachedAudioDecoder {
    fn decode_audio(
        &mut self,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        let key = AudioCacheKey {
            path: path.to_owned(),
            sample_rate,
            channels,
        };
        if let Some(buffer) = self.buffers.get(&key) {
            return Ok(buffer.clone());
        }
        if let Ok(Some(cached)) = self.disk_cache.load(path, sample_rate, channels) {
            self.waveforms.insert(key.clone(), cached.waveform);
            self.buffers.insert(key, cached.buffer.clone());
            return Ok(cached.buffer);
        }
        let buffer = self.backend.decode_audio(path, sample_rate, channels)?;
        let waveform = waveform_peaks(&buffer, 512);
        let _ = self.disk_cache.store(path, &buffer, &waveform);
        self.waveforms.insert(key.clone(), waveform);
        self.buffers.insert(key, buffer.clone());
        Ok(buffer)
    }
}

struct AudioMixWorker {
    requests: mpsc::Sender<AudioMixRequest>,
    results: mpsc::Receiver<AudioMixResult>,
    current_generation: Arc<AtomicU64>,
}

impl AudioMixWorker {
    fn spawn() -> Result<Self, Box<dyn Error>> {
        let (request_tx, request_rx) = mpsc::channel::<AudioMixRequest>();
        let (result_tx, result_rx) = mpsc::channel::<AudioMixResult>();
        let current_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&current_generation);
        thread::Builder::new()
            .name("celesta-audio-mix".to_owned())
            .spawn(move || {
                let mut decoder = CachedAudioDecoder::new();
                while let Ok(first) = request_rx.recv() {
                    let request = take_latest(first, &request_rx);
                    let output = mix_audio_graph_cancellable(
                        &request.graph,
                        &request.asset_root,
                        request.duration,
                        &mut decoder,
                        || worker_generation.load(Ordering::Acquire) != request.generation,
                    )
                    .map(|buffer| {
                        let mut clip_waveforms = HashMap::new();
                        let mut clip_levels = HashMap::new();
                        for clip in &request.graph.clips {
                            let Some((clip_id, waveform)) = (|| {
                                // The mix above already downloaded any URL.
                                let path =
                                    resolve_asset_path(&request.asset_root, &clip.asset.location)
                                        .ok()?;
                                let waveform = decoder.clip_waveform(
                                    &path,
                                    request.graph.sample_rate,
                                    2,
                                    clip,
                                )?;
                                let clip_id = clip
                                    .id
                                    .strip_suffix(":voice")
                                    .unwrap_or(&clip.id)
                                    .to_owned();
                                Some((clip_id, waveform))
                            })() else {
                                continue;
                            };
                            if !clip.muted {
                                clip_levels
                                    .insert(clip_id.clone(), clip_level_envelope(&waveform, clip));
                            }
                            clip_waveforms.insert(clip_id, waveform);
                        }
                        AudioMixOutput {
                            clip_waveforms,
                            clip_levels,
                            buffer,
                        }
                    })
                    .map_err(|error| error.to_string());
                    if result_tx
                        .send(AudioMixResult {
                            generation: request.generation,
                            output,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests: request_tx,
            results: result_rx,
            current_generation,
        })
    }

    fn request(&self, request: AudioMixRequest) -> Result<(), String> {
        self.cancel_before(request.generation);
        self.requests
            .send(request)
            .map_err(|_| "audio mix worker stopped unexpectedly".to_owned())
    }

    fn cancel_before(&self, generation: u64) {
        self.current_generation.store(generation, Ordering::Release);
    }
}

fn take_latest<T>(mut latest: T, receiver: &mpsc::Receiver<T>) -> T {
    while let Ok(next) = receiver.try_recv() {
        latest = next;
    }
    latest
}

impl AudioPreview {
    fn from_buffer(buffer: AudioBuffer, volume: f64) -> Result<Self, Box<dyn Error>> {
        let channels = NonZeroU16::new(buffer.channels).ok_or("audio has zero channels")?;
        let sample_rate =
            NonZeroU32::new(buffer.sample_rate).ok_or("audio has a zero sample rate")?;
        let source = SamplesBuffer::new(channels, sample_rate, buffer.samples);
        let device_sink = DeviceSinkBuilder::open_default_sink()?;
        let player = Player::connect_new(device_sink.mixer());
        let volume = volume as f32;
        player.set_volume(volume);
        player.append(source.clone());
        player.pause();
        Ok(Self {
            _device_sink: device_sink,
            player,
            source,
            volume,
        })
    }

    fn set_volume(&mut self, volume: f64) {
        self.volume = volume as f32;
        self.player.set_volume(self.volume);
    }

    fn seek(&mut self, time: Time, playing: bool) -> Result<(), Box<dyn Error>> {
        self.player.stop();
        self.player = Player::connect_new(self._device_sink.mixer());
        self.player.set_volume(self.volume);
        self.player.append(self.source.clone());
        self.player.pause();
        let seconds = time.as_seconds()?.max(0.0);
        self.player.try_seek(Duration::from_secs_f64(seconds))?;
        if playing {
            self.player.play();
        }
        Ok(())
    }

    fn pause(&self) {
        self.player.pause();
    }
}

struct EditorView {
    /// The file this view was opened from (`None` for the built-in demo);
    /// File > Reload opens it again.
    source_path: Option<PathBuf>,
    /// Bumped each time another file replaces this view's contents, so the
    /// previous React entry's reload watcher knows to stop.
    session: u64,
    /// A file picked from File > Open… is loading in the background.
    opening: bool,
    open_error: Option<SharedString>,
    /// The in-window menu bar. macOS shows the same menus natively instead.
    #[cfg(not(target_os = "macos"))]
    app_menu_bar: Option<Entity<AppMenuBar>>,
    document: EditorDocument,
    /// `Some` in standalone React composition preview mode: the document is a
    /// synthetic project and every previewed frame comes from the React
    /// bridge. The asset list, timeline tracks, and inspector have nothing to
    /// show.
    react_preview: Option<ReactPreview>,
    react_audio_worker: ReactAudioWorker,
    /// Bumped on every reload of the React entry (watcher or manual button);
    /// threaded into `PreviewRequest::react_reload` so the preview worker
    /// respawns Node against the freshly re-bundled code.
    react_reload_generation: u64,
    preview_worker: PreviewWorker,
    preview_generation: u64,
    /// Generation of the newest preview frame actually shown. Results are
    /// accepted while their generation only moves forward, so a slow render
    /// (a React entry's Node round-trip easily outruns one frame interval)
    /// keeps the preview tracking the playhead a render behind instead of
    /// freezing until every queued frame drains — which looked like a long
    /// delay before playback caught up.
    preview_shown_generation: u64,
    preview_pending: bool,
    media_probe_worker: MediaProbeWorker,
    media_generation: u64,
    media_pending: bool,
    media_cache: HashMap<String, Result<MediaAssetInfo, String>>,
    audio_mix_worker: AudioMixWorker,
    audio_generation: u64,
    audio_pending: bool,
    audio_preview: Option<AudioPreview>,
    clip_waveforms: HashMap<String, Vec<f32>>,
    clip_levels: HashMap<String, Vec<f32>>,
    clock: TimelineClock,
    frame_rate_value: Rational,
    playing: bool,
    playback_started_at: Option<Instant>,
    playback_started_frame: i64,
    scrubbing: bool,
    /// Playback gain for the preview only (0..=2). It scales the mixed
    /// buffer at the output device and never touches the project, so it has
    /// no effect on exports.
    monitor_volume: f64,
    master_volume_drag: Option<MasterVolumeDrag>,
    selected_clip_id: Option<String>,
    selected_asset_id: Option<String>,
    selected_track_id: Option<String>,
    component_schema_worker: ComponentSchemaWorker,
    component_schema_generation: u64,
    component_schema_pending: bool,
    component_schemas: BTreeMap<String, ComponentPropertySchema>,
    project_property_schema: Option<BTreeMap<String, ComponentPropertyField>>,
    component_schema_error: Option<SharedString>,
    project_name: SharedString,
    dimensions: SharedString,
    frame_rate_label: SharedString,
    duration: SharedString,
    assets: Vec<AssetSummary>,
    tracks: Vec<TrackSummary>,
    preview: Option<PreviewPresentation>,
    preview_error: Option<SharedString>,
    preview_warnings: Vec<SharedString>,
    media_error: Option<SharedString>,
    audio_error: Option<SharedString>,
    export_worker: ExportWorker,
    export_progress: Option<ExportProgress>,
    export_path: Option<PathBuf>,
    export_cancellation: Option<ExportCancellation>,
    export_cancelling: bool,
    export_error: Option<SharedString>,
    export_message: Option<SharedString>,
    typescript_error: Option<SharedString>,
    typescript_message: Option<SharedString>,
    /// Optional export in/out points, in composition frames. `out` is
    /// exclusive (one past the last frame to include). Both set and `in < out`
    /// means "Export…" renders only that span; otherwise the whole
    /// composition is exported.
    export_in_frame: Option<i64>,
    export_out_frame: Option<i64>,
    choosing_export_path: bool,
    gpu_name: SharedString,
    focus_handle: Option<FocusHandle>,
    master_volume_focus: Option<FocusHandle>,
    /// Split positions for the workspace shell: `dock_split` is the
    /// asset-list / monitor / inspector row, `body_split` is the
    /// work-area / timeline column. Held here so the drags persist across
    /// redraws.
    dock_split: Option<Entity<ResizableState>>,
    body_split: Option<Entity<ResizableState>>,
    /// Timeline horizontal zoom (>= 1; 1 = whole composition fits) and the
    /// fraction of the composition at the left edge of the visible window.
    /// The mouse wheel over the timeline adjusts the zoom about the cursor.
    timeline_zoom: f64,
    timeline_view_start: f64,
    /// Middle-button pan of the timeline: `(pointer x at grab, view_start at
    /// grab)`. `Some` while the middle button is held over the timeline.
    timeline_pan: Option<(f32, f64)>,
}

impl EditorView {
    fn open(path: Option<&Path>) -> Result<Self, Box<dyn Error>> {
        let (document, react_preview) = load_source(path)?;
        Self::from_document(path.map(Path::to_path_buf), document, react_preview)
    }

    fn from_document(
        source_path: Option<PathBuf>,
        document: EditorDocument,
        react_preview: Option<ReactPreview>,
    ) -> Result<Self, Box<dyn Error>> {
        let settings = &document.project().settings;
        let frame_rate_value = settings.frame_rate;
        let clock = TimelineClock::new(document.duration(), frame_rate_value)?;
        let project_name = document.display_name().into();
        let dimensions = format!("{} x {}", settings.width, settings.height).into();
        let frame_rate_label = format!(
            "{:.2} fps",
            f64::from(settings.frame_rate.numerator) / f64::from(settings.frame_rate.denominator)
        )
        .into();
        let duration = format_time(document.duration()).into();
        let assets = document.assets();
        let tracks = document.tracks();

        // Without `with_sequential_video` every previewed frame opens its own
        // FFmpeg decode run and seeks from the nearest keyframe, which costs
        // far more than everything else the preview does put together (~150ms
        // vs ~2ms of React evaluation on a 1080p source). Sharing one decode
        // run across the playhead's forward progress is what makes playback
        // track in real time; a backwards seek or a long jump still re-seeks.
        let renderer = GpuRenderer::new(GpuRenderOptions::default())?
            .with_asset_root(document.asset_root())
            .with_video_decoder(FfmpegBackend::new().with_sequential_video(frame_rate_value));
        let gpu_name = renderer.adapter_info().name.clone().into();
        let preview_worker = PreviewWorker::spawn(renderer)?;
        let media_probe_worker = MediaProbeWorker::spawn()?;
        let audio_mix_worker = AudioMixWorker::spawn()?;
        let export_worker = ExportWorker::spawn()?;
        let component_schema_worker = ComponentSchemaWorker::spawn()?;
        let react_audio_worker = ReactAudioWorker::spawn()?;
        let mut editor = Self {
            source_path,
            session: 0,
            opening: false,
            open_error: None,
            #[cfg(not(target_os = "macos"))]
            app_menu_bar: None,
            document,
            react_preview,
            react_audio_worker,
            react_reload_generation: 0,
            preview_worker,
            preview_generation: 0,
            preview_shown_generation: 0,
            preview_pending: false,
            media_probe_worker,
            media_generation: 0,
            media_pending: false,
            media_cache: HashMap::new(),
            audio_mix_worker,
            audio_generation: 0,
            audio_pending: false,
            audio_preview: None,
            clip_waveforms: HashMap::new(),
            clip_levels: HashMap::new(),
            clock,
            frame_rate_value,
            playing: false,
            playback_started_at: None,
            playback_started_frame: 0,
            scrubbing: false,
            monitor_volume: 1.0,
            master_volume_drag: None,
            selected_clip_id: None,
            selected_asset_id: None,
            selected_track_id: None,
            component_schema_worker,
            component_schema_generation: 0,
            component_schema_pending: false,
            component_schemas: BTreeMap::new(),
            project_property_schema: None,
            component_schema_error: None,
            project_name,
            dimensions,
            frame_rate_label,
            duration,
            assets,
            tracks,
            preview: None,
            preview_error: None,
            preview_warnings: Vec::new(),
            media_error: None,
            audio_error: None,
            export_worker,
            export_progress: None,
            export_path: None,
            export_cancellation: None,
            export_cancelling: false,
            export_error: None,
            export_message: None,
            typescript_error: None,
            typescript_message: None,
            export_in_frame: None,
            export_out_frame: None,
            choosing_export_path: false,
            gpu_name,
            focus_handle: None,
            master_volume_focus: None,
            dock_split: None,
            body_split: None,
            timeline_zoom: 1.0,
            timeline_view_start: 0.0,
            timeline_pan: None,
        };
        editor.refresh_preview();
        editor.refresh_audio_preview();
        if editor.react_preview.is_none() {
            editor.refresh_media_cache();
            editor.refresh_component_schemas();
        }
        Ok(editor)
    }

    /// File > Open…: pick a project or React entry and show it in this window.
    fn open_project_action(
        &mut self,
        _: &OpenProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_open(window, cx);
    }

    fn open_project_click(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.request_open(window, cx);
    }

    fn request_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.opening || !self.can_replace_contents(window, cx) {
            return;
        }
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let path = match selection.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    view.update_in(cx, |this, _, cx| {
                        this.open_error =
                            Some(format!("Couldn’t show the Open dialog: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    None
                }
                Err(_) => None,
            };
            if let Some(path) = path {
                view.update_in(cx, |this, window, cx| {
                    this.load_path(Some(path), window, cx)
                })
                .ok();
            }
        })
        .detach();
    }

    /// File > Reload: read the current file again from disk. A standalone
    /// React entry re-bundles in place; anything else is reopened.
    fn reload_project_action(
        &mut self,
        _: &ReloadProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_react_preview() {
            self.request_react_reload(cx);
            cx.notify();
            return;
        }
        if self.opening || !self.can_replace_contents(window, cx) {
            return;
        }
        self.load_path(self.source_path.clone(), window, cx);
    }

    /// File > Set Up TypeScript: copies this build's `@celesta/react`, React,
    /// and Node declarations into the React entry's project as `.celesta/`,
    /// so editors type-check against the runtime that actually runs the entry.
    fn set_up_typescript_action(
        &mut self,
        _: &SetUpTypeScript,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.typescript_error = None;
        self.typescript_message = None;
        let Some(entry) = self.document.react_entry_absolute_path() else {
            self.typescript_error = Some("Open a React entry to set up TypeScript".into());
            cx.notify();
            return;
        };
        cx.spawn_in(window, async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let (_, cli_script) = react_runtime_paths();
                    set_up_project_types(&project_types_template(&cli_script), &entry)
                })
                .await;
            view.update_in(cx, |this, _, cx| {
                match result {
                    Ok(setup) if setup.tsconfig == ProjectTsconfig::MissingExtends => {
                        this.typescript_error = Some(
                            "Types installed — add \"extends\": \"./.celesta/tsconfig.json\" to tsconfig.json"
                                .into(),
                        );
                    }
                    Ok(setup) => {
                        let name = setup.root.file_name().unwrap_or(setup.root.as_os_str());
                        this.typescript_message =
                            Some(format!("TypeScript set up in {}", name.to_string_lossy()).into());
                    }
                    Err(error) => {
                        this.typescript_error =
                            Some(format!("TypeScript setup failed: {error}").into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn close_window_action(&mut self, _: &CloseWindow, window: &mut Window, _: &mut Context<Self>) {
        window.remove_window();
    }

    /// Replacing the contents drops the export worker, so a running export
    /// must be cancelled first.
    fn can_replace_contents(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.export_cancellation.is_none() {
            return true;
        }
        window.push_notification("Cancel the export before opening another file.", cx);
        false
    }

    /// Loads `path` on a background thread (a React entry blocks on a Node
    /// handshake), then swaps it into this view. On failure the current
    /// contents stay and the error shows in the title bar.
    fn load_path(&mut self, path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.opening = true;
        self.open_error = None;
        self.pause();
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let load_path = path.clone();
            let loaded = cx
                .background_executor()
                .spawn(async move { load_source(load_path.as_deref()) })
                .await;
            view.update_in(cx, |this, _, cx| {
                this.opening = false;
                let result = loaded.and_then(|(document, react_preview)| {
                    EditorView::from_document(path, document, react_preview)
                        .map_err(|error| error.to_string())
                });
                match result {
                    Ok(next) => this.replace_contents(next, cx),
                    Err(error) => {
                        this.open_error = Some(format!("Couldn’t open: {error}").into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Swaps in a freshly opened view while keeping the window-owned UI state
    /// (focus, pane sizes, menu bar, and the preview volume).
    fn replace_contents(&mut self, mut next: EditorView, cx: &mut Context<Self>) {
        next.session = self.session.wrapping_add(1);
        next.focus_handle = self.focus_handle.take();
        next.master_volume_focus = self.master_volume_focus.take();
        next.dock_split = self.dock_split.take();
        next.body_split = self.body_split.take();
        #[cfg(not(target_os = "macos"))]
        {
            next.app_menu_bar = self.app_menu_bar.take();
        }
        next.monitor_volume = self.monitor_volume;
        *self = next;
        if self.is_react_preview() {
            self.watch_react_entry(cx);
        }
    }

    fn is_react_preview(&self) -> bool {
        self.react_preview.is_some()
    }

    fn current_time(&self) -> Time {
        self.clock.time().unwrap_or(Time::ZERO)
    }

    fn refresh_preview(&mut self) {
        self.preview_generation = self.preview_generation.wrapping_add(1);
        let generation = self.preview_generation;
        // Component clips resolve against the project's React entry when one
        // is set; without it the preview still renders everything else and
        // surfaces a warning instead of failing outright.
        let react = self.document.react_entry_absolute_path().map(|entry| {
            let (node, cli_script) = react_runtime_paths();
            ReactPreviewContext {
                node,
                cli_script,
                entry,
            }
        });
        let react_mode = if self.is_react_preview() {
            ReactPreviewMode::WholeScene
        } else {
            ReactPreviewMode::ResolveComponents
        };
        match self.document.scene_at(self.current_time()) {
            Ok(scene) => {
                self.preview_pending = true;
                self.preview_error = None;
                self.preview_warnings.clear();
                if let Err(error) = self.preview_worker.request(PreviewRequest {
                    generation,
                    scene,
                    asset_root: self.document.asset_root().to_owned(),
                    react,
                    react_mode,
                    react_reload: self.react_reload_generation,
                }) {
                    self.preview_pending = false;
                    self.preview_error = Some(error.into());
                }
            }
            Err(error) => {
                self.preview_pending = false;
                self.preview_error = Some(error.to_string().into());
            }
        }
    }

    /// Reloads the standalone React composition: re-reads its `<Composition>`
    /// facts on a background thread (dimensions/fps/duration may have changed),
    /// then bumps the preview worker's reload generation so it respawns Node
    /// against the freshly re-bundled code. Used by the reload watcher and the
    /// manual Reload button.
    fn request_react_reload(&mut self, cx: &mut Context<Self>) {
        let Some(react) = self.react_preview.as_ref() else {
            return;
        };
        let entry = react.entry.clone();
        let (node, cli_script) = react_runtime_paths();
        let frame = self.clock.frame();
        cx.spawn(async move |view, cx| {
            let entry_for_meta = entry.clone();
            let metadata = cx
                .background_executor()
                .spawn(async move {
                    ReactBridge::spawn(&node, &cli_script, &entry_for_meta)
                        .map(|bridge| bridge.metadata().clone())
                        .map_err(|error| error.to_string())
                })
                .await;
            view.update(cx, |this, cx| {
                this.apply_react_reload(&entry, metadata, frame, cx);
            })
            .ok();
        })
        .detach();
    }

    fn apply_react_reload(
        &mut self,
        entry: &Path,
        metadata: Result<ReactCompositionMetadata, String>,
        frame: i64,
        cx: &mut Context<Self>,
    ) {
        if let Some(react) = self.react_preview.as_mut() {
            react.watched_mtime = entry.parent().and_then(newest_source_mtime);
        }
        match metadata {
            Ok(metadata) => {
                if let Ok(document) = EditorDocument::react_preview(
                    entry,
                    metadata.width,
                    metadata.height,
                    metadata.frame_rate,
                    REACT_PREVIEW_SAMPLE_RATE,
                    metadata.duration_in_frames,
                ) {
                    self.document = document;
                    self.frame_rate_value = metadata.frame_rate;
                    if let Ok(mut clock) =
                        TimelineClock::new(self.document.duration(), self.frame_rate_value)
                    {
                        clock.seek(frame);
                        self.clock = clock;
                    }
                    self.dimensions = format!("{} x {}", metadata.width, metadata.height).into();
                    self.frame_rate_label = format!(
                        "{:.2} fps",
                        f64::from(metadata.frame_rate.numerator)
                            / f64::from(metadata.frame_rate.denominator)
                    )
                    .into();
                    self.duration = format_time(self.document.duration()).into();
                }
                self.react_reload_generation = self.react_reload_generation.wrapping_add(1);
                self.preview_error = None;
                self.refresh_preview();
                self.refresh_audio_preview();
            }
            Err(error) => {
                self.preview_error = Some(format!("React reload failed: {error}").into());
            }
        }
        cx.notify();
    }

    /// The periodic task (started once for a React-preview document) that
    /// notices source-file edits under the entry's directory and triggers a
    /// reload. Runs off the render loop so it fires even while the editor is
    /// idle.
    fn watch_react_entry(&self, cx: &mut Context<Self>) {
        let session = self.session;
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(800))
                    .await;
                let Ok(Some(dir)) = view.update(cx, |this, _| {
                    this.react_preview
                        .as_ref()
                        .filter(|_| this.session == session)
                        .and_then(|react| react.entry.parent().map(Path::to_path_buf))
                }) else {
                    break;
                };
                let latest = cx
                    .background_executor()
                    .spawn(async move { newest_source_mtime(&dir) })
                    .await;
                let changed = view
                    .update(cx, |this, _| match this.react_preview.as_ref() {
                        Some(react) => latest.is_some() && latest != react.watched_mtime,
                        None => false,
                    })
                    .unwrap_or(false);
                if changed
                    && view
                        .update(cx, |this, cx| {
                            if let Some(react) = this.react_preview.as_mut() {
                                react.watched_mtime = latest;
                            }
                            this.request_react_reload(cx);
                        })
                        .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn refresh_media_cache(&mut self) {
        self.media_generation = self.media_generation.wrapping_add(1);
        let generation = self.media_generation;
        let assets = self
            .assets
            .iter()
            .filter(|asset| matches!(asset.kind, AssetKind::Video | AssetKind::Audio))
            .filter_map(|asset| {
                Some(ProbeAsset {
                    id: asset.id.clone(),
                    path: asset.path.clone()?,
                })
            })
            .collect::<Vec<_>>();
        self.media_cache
            .retain(|id, _| assets.iter().any(|asset| asset.id == *id));
        if assets.is_empty() {
            self.media_pending = false;
            return;
        }
        self.media_pending = true;
        if let Err(error) = self
            .media_probe_worker
            .request(MediaProbeRequest { generation, assets })
        {
            self.media_pending = false;
            self.media_error = Some(error.into());
        }
    }

    /// Queries the project's `react_entry` for its registered components'
    /// property schemas, which the inspector uses to label component props
    /// and project properties. Runs once per opened project rather than on
    /// every clip selection, since spawning Node is comparatively slow;
    /// File > Reload picks up entry edits.
    fn refresh_component_schemas(&mut self) {
        self.component_schema_generation = self.component_schema_generation.wrapping_add(1);
        let generation = self.component_schema_generation;
        let Some(entry) = self.document.react_entry_absolute_path() else {
            self.component_schema_pending = false;
            self.component_schema_error = None;
            self.component_schemas.clear();
            self.project_property_schema = None;
            return;
        };
        self.component_schema_pending = true;
        self.component_schema_error = None;
        let (node, cli_script) = react_runtime_paths();
        if let Err(error) = self
            .component_schema_worker
            .request(ComponentSchemaRequest {
                generation,
                node,
                cli_script,
                entry,
            })
        {
            self.component_schema_pending = false;
            self.component_schema_error = Some(error.into());
        }
    }

    fn poll_background_work(&mut self) {
        loop {
            match self.preview_worker.results.try_recv() {
                Ok(result) => {
                    // Accept every result whose generation moves forward, not
                    // just the newest request's: while a render is slower than
                    // the frame interval the newest request is always still in
                    // flight, and requiring an exact match would drop every
                    // frame until playback stopped.
                    if result.generation <= self.preview_shown_generation {
                        continue;
                    }
                    self.preview_shown_generation = result.generation;
                    if result.generation == self.preview_generation {
                        self.preview_pending = false;
                    }
                    match result.frame {
                        Ok(preview) => {
                            self.preview = Some(preview);
                            self.preview_error = None;
                        }
                        Err(error) => {
                            self.preview_error = Some(error.into());
                        }
                    }
                    self.preview_warnings = result.warnings.into_iter().map(Into::into).collect();
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.preview_pending {
                        self.preview_pending = false;
                        self.preview_error = Some("preview worker stopped unexpectedly".into());
                    }
                    break;
                }
            }
        }

        loop {
            match self.media_probe_worker.results.try_recv() {
                Ok(result) => {
                    if result.generation != self.media_generation {
                        continue;
                    }
                    self.media_pending = false;
                    self.media_cache.extend(result.assets);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.media_pending {
                        self.media_pending = false;
                        self.media_error = Some("media probe worker stopped unexpectedly".into());
                    }
                    break;
                }
            }
        }

        loop {
            match self.component_schema_worker.results.try_recv() {
                Ok(result) => {
                    if result.generation != self.component_schema_generation {
                        continue;
                    }
                    self.component_schema_pending = false;
                    match result.schemas {
                        Ok(schemas) => {
                            self.component_schemas = schemas;
                            self.project_property_schema = result.project_property_schema;
                            self.component_schema_error = None;
                        }
                        Err(error) => {
                            self.component_schemas.clear();
                            self.project_property_schema = None;
                            self.component_schema_error = Some(error.into());
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.component_schema_pending {
                        self.component_schema_pending = false;
                        self.component_schema_error =
                            Some("component schema worker stopped unexpectedly".into());
                    }
                    break;
                }
            }
        }

        loop {
            match self.react_audio_worker.results.try_recv() {
                Ok(result) => {
                    if result.generation != self.audio_generation {
                        continue;
                    }
                    match result.graph {
                        Ok(graph) if graph.clips.is_empty() => {
                            self.audio_pending = false;
                            self.audio_preview = None;
                            self.clip_waveforms.clear();
                            self.clip_levels.clear();
                            self.audio_error = None;
                        }
                        Ok(graph) => {
                            if let Err(error) = self.audio_mix_worker.request(AudioMixRequest {
                                generation: self.audio_generation,
                                graph,
                                asset_root: self.document.asset_root().to_owned(),
                                duration: self.document.duration(),
                            }) {
                                self.audio_pending = false;
                                self.audio_error = Some(error.into());
                            }
                        }
                        Err(error) => {
                            self.audio_pending = false;
                            self.audio_preview = None;
                            self.clip_waveforms.clear();
                            self.clip_levels.clear();
                            self.audio_error = Some(error.into());
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.audio_pending {
                        self.audio_pending = false;
                        self.audio_error = Some("react audio worker stopped unexpectedly".into());
                    }
                    break;
                }
            }
        }

        loop {
            match self.audio_mix_worker.results.try_recv() {
                Ok(result) => {
                    if result.generation != self.audio_generation {
                        continue;
                    }
                    self.audio_pending = false;
                    match result.output.and_then(|output| {
                        self.clip_waveforms = output.clip_waveforms;
                        self.clip_levels = output.clip_levels;
                        AudioPreview::from_buffer(output.buffer, self.monitor_volume)
                            .map_err(|error| error.to_string())
                    }) {
                        Ok(mut preview) => {
                            if let Err(error) = preview.seek(self.current_time(), self.playing) {
                                self.audio_preview = None;
                                self.audio_error = Some(error.to_string().into());
                            } else {
                                self.audio_preview = Some(preview);
                                self.audio_error = None;
                            }
                        }
                        Err(error) => {
                            self.clip_waveforms.clear();
                            self.clip_levels.clear();
                            self.audio_preview = None;
                            self.audio_error = Some(error.into());
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.audio_pending {
                        self.audio_pending = false;
                        self.clip_waveforms.clear();
                        self.clip_levels.clear();
                        self.audio_preview = None;
                        self.audio_error = Some("audio mix worker stopped unexpectedly".into());
                    }
                    break;
                }
            }
        }

        loop {
            match self.export_worker.events.try_recv() {
                Ok(ExportEvent::Progress(progress)) => {
                    self.export_progress = Some(progress);
                }
                Ok(ExportEvent::Finished {
                    output,
                    result,
                    cancelled,
                }) => {
                    self.export_progress = None;
                    self.export_cancellation = None;
                    self.export_cancelling = false;
                    match result {
                        Ok(()) => {
                            self.export_error = None;
                            self.export_path = Some(output.clone());
                            self.export_message =
                                Some(format!("Exported {}", output.display()).into());
                        }
                        Err(_) if cancelled => {
                            self.export_path = None;
                            self.export_error = None;
                            self.export_message = Some("Export cancelled".into());
                        }
                        Err(error) => {
                            self.export_path = None;
                            self.export_message = None;
                            self.export_error = Some(error.into());
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.export_cancellation.is_some() {
                        self.export_progress = None;
                        self.export_path = None;
                        self.export_cancellation = None;
                        self.export_cancelling = false;
                        self.export_message = None;
                        self.export_error = Some("export worker stopped unexpectedly".into());
                    }
                    break;
                }
            }
        }
    }

    fn pause(&mut self) {
        self.playing = false;
        self.playback_started_at = None;
        if let Some(audio) = &self.audio_preview {
            audio.pause();
        }
    }

    fn seek_frame(&mut self, frame: i64) {
        self.clock.seek(frame);
        self.refresh_preview();
    }

    fn toggle_playback(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_playback_state();
        // GPUI only permits `request_animation_frame` during rendering. This
        // notification enters `render`, where `update_playback` requests it.
        cx.notify();
    }

    fn toggle_playback_state(&mut self) {
        if self.playing {
            self.pause();
        } else if self.clock.end_frame() > 0 {
            if self.clock.is_at_end() {
                self.seek_frame(0);
            }
            self.playing = true;
            self.playback_started_at = Some(Instant::now());
            self.playback_started_frame = self.clock.frame();
            if let Some(audio) = &mut self.audio_preview
                && let Err(error) = audio.seek(self.clock.time().unwrap_or(Time::ZERO), true)
            {
                self.audio_error = Some(error.to_string().into());
                self.audio_preview = None;
            }
        }
    }

    fn step_backward(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.pause();
        self.seek_frame(self.clock.frame().saturating_sub(1));
        cx.notify();
    }

    fn step_forward(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.pause();
        self.seek_frame(self.clock.frame().saturating_add(1));
        cx.notify();
    }

    fn update_playback(&mut self, window: &mut Window) {
        if !self.playing {
            return;
        }
        let Some(started_at) = self.playback_started_at else {
            self.pause();
            return;
        };
        let frames_per_second = f64::from(self.frame_rate_value.numerator)
            / f64::from(self.frame_rate_value.denominator);
        let elapsed_frames = (started_at.elapsed().as_secs_f64() * frames_per_second) as i64;
        let target = self.playback_started_frame.saturating_add(elapsed_frames);
        if target != self.clock.frame() {
            self.seek_frame(target);
        }
        if self.clock.is_at_end() {
            self.pause();
        } else {
            window.request_animation_frame();
        }
    }

    /// Pixel width of the timeline clip lane (window minus the 230px header
    /// column and the 8px right gutter).
    fn timeline_lane_width(window: &Window) -> f32 {
        (f32::from(window.bounds().size.width) - 230.0 - 8.0).max(1.0)
    }

    /// Fraction (0..1) of the whole composition at horizontal window position
    /// `x`, accounting for the 230px track-header column and the current
    /// timeline zoom / scroll.
    fn timeline_fraction_at(&self, x: Pixels, window: &Window) -> f64 {
        let lane_width = Self::timeline_lane_width(window);
        let local = ((f32::from(x) - 230.0).clamp(0.0, lane_width) / lane_width) as f64;
        let (zoom, view_start) = self.timeline_view();
        (view_start + local / zoom).clamp(0.0, 1.0)
    }

    fn frame_for_timeline_position(&self, position: Point<Pixels>, window: &Window) -> i64 {
        self.clock
            .frame_at_fraction(self.timeline_fraction_at(position.x, window) as f32)
    }

    /// Clamped `(zoom, view_start)` for the timeline: zoom is at least 1, and
    /// the visible window `[view_start, view_start + 1/zoom]` stays inside
    /// `[0, 1]`.
    fn timeline_view(&self) -> (f64, f64) {
        let zoom = self.timeline_zoom.clamp(1.0, 40.0);
        let view_start = self
            .timeline_view_start
            .clamp(0.0, (1.0 - 1.0 / zoom).max(0.0));
        (zoom, view_start)
    }

    /// Mouse wheel over the timeline: zoom about the cursor, keeping the
    /// composition fraction under the pointer fixed.
    fn timeline_wheel(
        &mut self,
        event: &gpui_kit::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = match event.delta {
            gpui_kit::ScrollDelta::Lines(point) => point.y,
            gpui_kit::ScrollDelta::Pixels(point) => f32::from(point.y) / 40.0,
        };
        if delta == 0.0 {
            return;
        }
        let (zoom, view_start) = self.timeline_view();
        let cursor = self.timeline_fraction_at(event.position.x, window);
        let local = ((cursor - view_start) * zoom).clamp(0.0, 1.0);
        let new_zoom = (zoom * (1.0 + f64::from(delta) * 0.15)).clamp(1.0, 40.0);
        self.timeline_zoom = new_zoom;
        self.timeline_view_start = (cursor - local / new_zoom).clamp(0.0, 1.0);
        cx.stop_propagation();
        cx.notify();
    }

    /// Middle-button press over the timeline: start a pan.
    fn begin_timeline_pan(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.timeline_pan = Some((f32::from(event.position.x), self.timeline_view().1));
        cx.stop_propagation();
    }

    fn continue_timeline_pan(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((grab_x, grab_view_start)) = self.timeline_pan else {
            return;
        };
        // `MouseMoveEvent::dragging()` is Left-button only, so check the middle
        // button explicitly; the button being released ends the pan.
        if event.pressed_button != Some(MouseButton::Middle) {
            self.timeline_pan = None;
            return;
        }
        let (zoom, _) = self.timeline_view();
        let dx = f64::from(f32::from(event.position.x) - grab_x)
            / f64::from(Self::timeline_lane_width(window));
        self.timeline_view_start = (grab_view_start - dx / zoom).clamp(0.0, 1.0);
        cx.notify();
    }

    fn end_timeline_pan(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.timeline_pan.take().is_some() {
            cx.notify();
        }
    }

    fn scrub_to(&mut self, position: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        self.pause();
        let frame = self.frame_for_timeline_position(position, window);
        if frame != self.clock.frame() {
            self.seek_frame(frame);
            cx.notify();
        }
    }

    fn begin_scrub(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.scrubbing = true;
        self.scrub_to(event.position, window, cx);
    }

    fn continue_scrub(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.scrubbing && event.dragging() {
            self.scrub_to(event.position, window, cx);
        }
    }

    fn end_scrub(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.scrubbing {
            self.scrub_to(event.position, window, cx);
            self.scrubbing = false;
        }
    }

    fn refresh_audio_preview(&mut self) {
        self.audio_generation = self.audio_generation.wrapping_add(1);
        let generation = self.audio_generation;
        self.audio_mix_worker.cancel_before(generation);
        self.audio_pending = false;
        self.audio_preview = None;
        self.clip_levels.clear();

        if let Some(react) = &self.react_preview {
            // No tracks to evaluate: sweep the React entry for its `<Audio>`
            // declarations on the dedicated worker, then feed the resulting
            // graph into the shared mix pipeline (see `poll_background_work`).
            let (node, cli_script) = react_runtime_paths();
            self.clip_waveforms.clear();
            self.audio_pending = true;
            self.audio_error = None;
            if let Err(error) = self.react_audio_worker.request(ReactAudioRequest {
                generation,
                node,
                cli_script,
                entry: react.entry.clone(),
                sample_rate: self.document.project().settings.sample_rate,
                master_volume: self.document.master_volume(),
            }) {
                self.audio_pending = false;
                self.audio_error = Some(error.into());
            }
            return;
        }

        match self.document.audio_graph() {
            Ok(graph) if graph.clips.is_empty() => {
                self.clip_waveforms.clear();
                self.clip_levels.clear();
                self.audio_error = None;
            }
            Ok(graph) => {
                self.audio_pending = true;
                self.audio_error = None;
                if let Err(error) = self.audio_mix_worker.request(AudioMixRequest {
                    generation,
                    graph,
                    asset_root: self.document.asset_root().to_owned(),
                    duration: self.document.duration(),
                }) {
                    self.audio_pending = false;
                    self.audio_error = Some(error.into());
                }
            }
            Err(error) => {
                self.clip_waveforms.clear();
                self.clip_levels.clear();
                self.audio_error = Some(error.to_string().into());
            }
        }
    }

    fn toggle_track_mute(&mut self, track_id: &str, cx: &mut Context<Self>) {
        match self.document.toggle_track_muted(track_id) {
            Ok(_) => {
                self.tracks = self.document.tracks();
                self.refresh_audio_preview();
            }
            Err(error) => self.audio_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn toggle_track_solo(&mut self, track_id: &str, cx: &mut Context<Self>) {
        match self.document.toggle_track_solo(track_id) {
            Ok(_) => {
                self.tracks = self.document.tracks();
                self.refresh_audio_preview();
            }
            Err(error) => self.audio_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn adjust_master_volume(&mut self, delta_percent: i32, cx: &mut Context<Self>) {
        let current_percent = (self.monitor_volume * 100.0).round() as i32;
        let next_percent = current_percent.saturating_add(delta_percent).clamp(0, 200);
        self.set_master_volume(f64::from(next_percent) / 100.0, cx);
    }

    fn set_master_volume(&mut self, volume: f64, cx: &mut Context<Self>) {
        self.monitor_volume = volume.clamp(0.0, 2.0);
        if let Some(audio) = &mut self.audio_preview {
            audio.set_volume(self.monitor_volume);
        }
        cx.notify();
    }

    fn begin_master_volume_drag(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(focus) = &self.master_volume_focus {
            focus.focus(window, cx);
        }
        self.master_volume_drag = Some(MasterVolumeDrag {
            pointer_x: event.position.x,
            start_volume: self.monitor_volume,
        });
        cx.notify();
    }

    fn continue_master_volume_drag(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            return;
        }
        let Some(drag) = &self.master_volume_drag else {
            return;
        };
        let volume = master_volume_from_drag(
            drag.start_volume,
            f64::from(event.position.x - drag.pointer_x),
        );
        if (volume - self.monitor_volume).abs() >= f64::EPSILON {
            self.set_master_volume(volume, cx);
        }
    }

    fn end_master_volume_drag(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.master_volume_drag.take().is_some() {
            cx.notify();
        }
    }

    fn master_volume_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let step = if event.keystroke.modifiers.shift {
            10
        } else {
            5
        };
        match event.keystroke.key.as_str() {
            "left" | "down" => self.adjust_master_volume(-step, cx),
            "right" | "up" => self.adjust_master_volume(step, cx),
            "home" => self.set_master_volume(0.0, cx),
            "end" => self.set_master_volume(2.0, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn export_project_action(
        &mut self,
        _: &ExportProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_export(window, cx);
    }

    fn export_project_click(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_export(window, cx);
    }

    fn request_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.choosing_export_path || self.export_cancellation.is_some() {
            return;
        }
        self.pause();
        self.choosing_export_path = true;
        self.export_error = None;
        self.export_message = None;
        let directory = self.document.path().and_then(Path::parent).map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            Path::to_path_buf,
        );
        let suggested_name = export_suggested_name(self.document.path(), &self.project_name);
        let selection = cx.prompt_for_new_path(&directory, Some(&suggested_name));
        cx.spawn_in(window, async move |view, cx| {
            let selected_path = match selection.await {
                Ok(Ok(path)) => path,
                Ok(Err(error)) => {
                    view.update_in(cx, |this, _, cx| {
                        this.choosing_export_path = false;
                        this.export_error =
                            Some(format!("could not open Export dialog: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    view.update_in(cx, |this, _, cx| {
                        this.choosing_export_path = false;
                        this.export_error =
                            Some(format!("Export dialog was interrupted: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            view.update_in(cx, |this, _, cx| {
                this.choosing_export_path = false;
                let Some(mut output) = selected_path else {
                    cx.notify();
                    return;
                };
                if output.extension() != Some(OsStr::new("mp4")) {
                    output.set_extension("mp4");
                }
                this.start_export(output);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn start_export(&mut self, output: PathBuf) {
        let cancellation = ExportCancellation::default();
        let (source, range) = match &self.react_preview {
            Some(react) => {
                let (node, cli_script) = react_runtime_paths();
                (
                    ExportSource::ReactEntry {
                        entry: react.entry.clone(),
                        node,
                        cli_script,
                    },
                    None,
                )
            }
            None => (
                ExportSource::Project(Box::new(self.document.project().clone())),
                export_range_for(
                    self.export_in_frame,
                    self.export_out_frame,
                    self.frame_rate_value,
                ),
            ),
        };
        let request = ExportRequest {
            source,
            asset_root: self.document.asset_root().to_owned(),
            output: output.clone(),
            range,
            cancellation: cancellation.clone(),
        };
        self.export_path = Some(output);
        self.export_progress = Some(ExportProgress::Rendering { frame: 0, total: 0 });
        self.export_cancellation = Some(cancellation);
        self.export_cancelling = false;
        self.export_error = None;
        self.export_message = None;
        if let Err(error) = self.export_worker.request(request) {
            self.export_path = None;
            self.export_progress = None;
            self.export_cancellation = None;
            self.export_error = Some(error.into());
        }
    }

    /// Marks the current playhead frame as the export in-point, dropping a
    /// stale out-point that would now sit at or before it.
    fn set_export_in(&mut self, cx: &mut Context<Self>) {
        let frame = self.clock.frame();
        self.export_in_frame = Some(frame);
        if self.export_out_frame.is_some_and(|out| out <= frame) {
            self.export_out_frame = None;
        }
        self.export_error = None;
        cx.notify();
    }

    /// Marks the frame just after the playhead as the export out-point (so the
    /// current frame is included), dropping a stale in-point.
    fn set_export_out(&mut self, cx: &mut Context<Self>) {
        let frame = self.clock.frame().saturating_add(1);
        self.export_out_frame = Some(frame);
        if self.export_in_frame.is_some_and(|start| start >= frame) {
            self.export_in_frame = None;
        }
        self.export_error = None;
        cx.notify();
    }

    fn clear_export_range(&mut self, cx: &mut Context<Self>) {
        self.export_in_frame = None;
        self.export_out_frame = None;
        cx.notify();
    }

    /// `in – out` as timecodes for the toolbar, or `None` when neither marker
    /// is set. An unset side shows as `—`.
    fn export_range_label(&self) -> Option<SharedString> {
        if self.export_in_frame.is_none() && self.export_out_frame.is_none() {
            return None;
        }
        let frame_rate = self.frame_rate_value;
        let marker = |frame: Option<i64>| {
            frame
                .and_then(|frame| Time::frames(frame, frame_rate).ok())
                .map_or_else(|| "—".to_owned(), format_time)
        };
        Some(
            format!(
                "{} – {}",
                marker(self.export_in_frame),
                marker(self.export_out_frame)
            )
            .into(),
        )
    }

    /// The `[start, end]` fractions (0..1 of the timeline) to highlight for a
    /// valid export range, or `None` when no full range is set.
    fn export_range_band(&self) -> Option<(f32, f32)> {
        export_range_for(
            self.export_in_frame,
            self.export_out_frame,
            self.frame_rate_value,
        )?;
        let span = self.clock.end_frame().max(1) as f32;
        let start = (self.export_in_frame? as f32 / span).clamp(0.0, 1.0);
        let end = (self.export_out_frame? as f32 / span).clamp(0.0, 1.0);
        Some((start, end))
    }

    fn set_export_in_action(&mut self, _: &SetExportIn, _: &mut Window, cx: &mut Context<Self>) {
        self.set_export_in(cx);
    }

    fn set_export_out_action(&mut self, _: &SetExportOut, _: &mut Window, cx: &mut Context<Self>) {
        self.set_export_out(cx);
    }

    fn clear_export_range_action(
        &mut self,
        _: &ClearExportRange,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_export_range(cx);
    }

    fn cancel_export_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(cancellation) = &self.export_cancellation {
            cancellation.cancel();
            self.export_cancelling = true;
            cx.notify();
        }
    }

    fn toggle_playback_action(
        &mut self,
        _: &TogglePlayback,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_playback_state();
        cx.notify();
    }

    fn previous_frame_action(&mut self, _: &PreviousFrame, _: &mut Window, cx: &mut Context<Self>) {
        self.pause();
        self.seek_frame(self.clock.frame().saturating_sub(1));
        cx.notify();
    }

    fn next_frame_action(&mut self, _: &NextFrame, _: &mut Window, cx: &mut Context<Self>) {
        self.pause();
        self.seek_frame(self.clock.frame().saturating_add(1));
        cx.notify();
    }

    /// The window's title bar: the menu bar (outside macOS, which shows it
    /// natively), the open file, status messages, and the window-wide
    /// commands — Open…, Export…, and the preview volume.
    fn title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let volume = self.monitor_volume.clamp(0.0, 2.0);
        let exporting = self.export_cancellation.is_some();
        let danger = cx.theme().danger;
        let warning = cx.theme().warning;
        let success = cx.theme().success;
        let muted = cx.theme().muted_foreground;
        let message = |text: String, color| {
            div()
                .max_w(px(420.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_xs()
                .text_color(color)
                .child(text)
        };
        let leading = div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .overflow_hidden();
        #[cfg(not(target_os = "macos"))]
        let leading = leading.when_some(self.app_menu_bar.clone(), |leading, menu_bar| {
            leading.child(menu_bar)
        });
        let leading = leading
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .whitespace_nowrap()
                    .child(self.project_name.clone()),
            )
            .when(self.is_react_preview(), |leading| {
                leading.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .whitespace_nowrap()
                        .child("React composition"),
                )
            });
        TitleBar::new().child(leading).child(
            div()
                .id("title-bar-actions")
                .flex()
                .items_center()
                .gap_2()
                .pr_2()
                .on_mouse_move(cx.listener(Self::continue_master_volume_drag))
                .on_mouse_up(MouseButton::Left, cx.listener(Self::end_master_volume_drag))
                .when_some(self.open_error.clone(), |bar, error| {
                    bar.child(message(error.to_string(), danger))
                })
                .when_some(self.media_error.clone(), |bar, error| {
                    bar.child(message(format!("Media probe failed: {error}"), danger))
                })
                .when_some(self.audio_error.clone(), |bar, error| {
                    bar.child(message(format!("Audio unavailable: {error}"), warning))
                })
                .when_some(self.export_error.clone(), |bar, error| {
                    bar.child(message(format!("Export failed: {error}"), danger))
                })
                .when_some(self.export_message.clone(), |bar, text| {
                    bar.child(message(text.to_string(), success))
                })
                .when_some(self.typescript_error.clone(), |bar, error| {
                    bar.child(message(error.to_string(), warning))
                })
                .when_some(self.typescript_message.clone(), |bar, text| {
                    bar.child(message(text.to_string(), success))
                })
                .child(
                    Button::new("open-project")
                        .small()
                        .ghost()
                        .icon(IconName::FolderOpen)
                        .label(if self.opening {
                            "Opening…"
                        } else {
                            "Open…"
                        })
                        .loading(self.opening)
                        .disabled(self.opening || exporting)
                        .on_click(cx.listener(Self::open_project_click)),
                )
                .child(if exporting {
                    Button::new("export-project")
                        .small()
                        .danger()
                        .label(if self.export_cancelling {
                            "Cancelling…"
                        } else {
                            "Cancel export"
                        })
                        .disabled(self.export_cancelling)
                        .on_click(cx.listener(Self::cancel_export_click))
                } else {
                    Button::new("export-project")
                        .small()
                        .outline()
                        .label("Export…")
                        .disabled(self.choosing_export_path || self.opening)
                        .on_click(cx.listener(Self::export_project_click))
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(muted)
                        .child("Volume")
                        .child(
                            div()
                                .id("preview-volume-slider")
                                .relative()
                                .w(px(88.0))
                                .h(px(20.0))
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().secondary)
                                .tooltip(|window, cx| {
                                    Tooltip::new("Preview volume (doesn’t affect exports)")
                                        .build(window, cx)
                                })
                                .when_some(self.master_volume_focus.as_ref(), |slider, focus| {
                                    slider.track_focus(focus)
                                })
                                .focus(|slider| slider.border_1().border_color(cx.theme().ring))
                                .hover(|style| style.bg(cx.theme().secondary_hover))
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(5.0))
                                        .right(px(5.0))
                                        .top(px(8.0))
                                        .h(px(4.0))
                                        .rounded_full()
                                        .overflow_hidden()
                                        .bg(cx.theme().background)
                                        .child(
                                            div()
                                                .h_full()
                                                .w(relative((volume / 2.0) as f32))
                                                .rounded_full()
                                                .bg(cx.theme().primary),
                                        ),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .left(px((volume / 2.0 * 78.0) as f32))
                                        .top(px(5.0))
                                        .size(px(10.0))
                                        .rounded_full()
                                        .bg(cx.theme().foreground),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(Self::begin_master_volume_drag),
                                )
                                .on_key_down(cx.listener(Self::master_volume_key_down)),
                        )
                        .child(
                            div()
                                .w(px(38.0))
                                .text_right()
                                .child(format!("{}%", (volume * 100.0).round() as i32)),
                        ),
                ),
        )
    }

    fn asset_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let missing_color = cx.theme().danger;
        let meta_color = cx.theme().muted_foreground;
        let rows = self.assets.iter().map(|asset| {
            let asset_id = asset.id.clone();
            let selected = self.selected_asset_id.as_deref() == Some(asset.id.as_str());
            let element_id: SharedString = format!("asset-row-{}", asset.id).into();
            let media_detail = match (asset.missing, self.media_cache.get(&asset.id)) {
                (true, _) => Some("File not found".to_owned()),
                (false, Some(Ok(info))) => Some(format_media_asset_info(info)),
                (false, Some(Err(_))) => Some("Probe failed".to_owned()),
                (false, None) if matches!(asset.kind, AssetKind::Video | AssetKind::Audio) => {
                    Some("Probing…".to_owned())
                }
                (false, None) => None,
            };
            let thumbnail = (asset.kind == AssetKind::Image && !asset.missing)
                .then(|| asset.path.clone())
                .flatten();
            div()
                .id(element_id)
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .cursor_pointer()
                .when(selected, |row| row.bg(cx.theme().list_active))
                .hover(|style| style.bg(cx.theme().list_hover))
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(
                    div()
                        .flex_none()
                        .w(px(44.0))
                        .h(px(26.0))
                        .rounded_sm()
                        .overflow_hidden()
                        .bg(cx.theme().background)
                        .flex()
                        .items_center()
                        .justify_center()
                        .map(|slot| match thumbnail {
                            Some(path) => {
                                slot.child(img(path).size_full().object_fit(ObjectFit::Cover))
                            }
                            None => slot.child(
                                div()
                                    .text_xs()
                                    .text_color(theme::accent())
                                    .child(asset.kind.to_string().to_uppercase()),
                            ),
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .overflow_hidden()
                        .child(asset.id.clone())
                        .when_some(media_detail, |column, detail| {
                            column.child(
                                div()
                                    .text_xs()
                                    .text_color(if asset.missing {
                                        missing_color
                                    } else {
                                        meta_color
                                    })
                                    .child(detail),
                            )
                        }),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.selected_asset_id = Some(asset_id.clone());
                    cx.notify();
                }))
        });
        let contents = div()
            .id("asset-list-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .when(self.assets.is_empty(), |contents| {
                contents.child(
                    div()
                        .p_3()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("No assets in this project"),
                )
            })
            .children(rows);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .bg(cx.theme().sidebar)
            .child(panel_header("Assets", self.assets.len(), cx))
            .child(contents)
    }

    fn preview_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let canvas = div()
            .relative()
            .flex()
            .flex_1()
            .w_full()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .bg(cx.theme().background)
            .when_some(self.preview.clone(), |canvas, preview| match preview {
                PreviewPresentation::Image(preview) => {
                    canvas.child(img(preview).size_full().object_fit(ObjectFit::Contain))
                }
                #[cfg(target_os = "macos")]
                PreviewPresentation::Surface(preview) => canvas.child(
                    gpui_kit::surface(preview.pixel_buffer())
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                ),
            })
            .when_some(self.preview_error.clone(), |canvas, error| {
                canvas.child(
                    div()
                        .p_4()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(format!("Preview unavailable: {error}")),
                )
            })
            .when(!self.preview_warnings.is_empty(), |canvas| {
                canvas.child(
                    div()
                        .absolute()
                        .bottom(px(8.0))
                        .left(px(8.0))
                        .right(px(8.0))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().warning.opacity(0.5))
                        .bg(cx.theme().warning.opacity(0.12))
                        .p_2()
                        .text_xs()
                        .text_color(cx.theme().warning)
                        .children(self.preview_warnings.iter().cloned()),
                )
            });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(canvas)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .h(px(44.0))
                    .w_full()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .bg(cx.theme().secondary)
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_color(cx.theme().foreground)
                    .child(
                        Button::new("previous-frame")
                            .small()
                            .ghost()
                            .icon(IconName::ChevronLeft)
                            .tooltip("Previous frame")
                            .on_click(cx.listener(Self::step_backward)),
                    )
                    .child(
                        Button::new("toggle-playback")
                            .small()
                            .primary()
                            .icon(if self.playing {
                                IconName::Pause
                            } else {
                                IconName::Play
                            })
                            .label(if self.playing { "Pause" } else { "Play" })
                            .on_click(cx.listener(Self::toggle_playback)),
                    )
                    .child(
                        Button::new("next-frame")
                            .small()
                            .ghost()
                            .icon(IconName::ChevronRight)
                            .tooltip("Next frame")
                            .on_click(cx.listener(Self::step_forward)),
                    )
                    .child(
                        div()
                            .w(px(110.0))
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} / {}f",
                                self.clock.frame(),
                                self.clock.end_frame()
                            )),
                    ),
            )
    }

    /// Read-only facts about the composition and whatever is selected. The
    /// project is edited in its source file; File > Reload picks up changes.
    fn inspector_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let characters = self.document.characters();
        let selected_asset = self.selected_asset_id.as_deref().and_then(|selected| {
            self.assets
                .iter()
                .find(|asset| asset.id == selected)
                .cloned()
        });
        let selected_track = self.selected_track_id.as_deref().and_then(|selected| {
            self.tracks
                .iter()
                .find(|track| track.id == selected)
                .cloned()
        });
        let selected_clip = self.selected_clip_id.as_deref().and_then(|selected| {
            self.tracks
                .iter()
                .flat_map(|track| &track.clips)
                .find(|clip| clip.id == selected)
                .cloned()
        });
        let contents = div()
            .id("inspector-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .child(inspector_row("Canvas", self.dimensions.clone(), cx))
            .child(inspector_row(
                "Frame rate",
                self.frame_rate_label.clone(),
                cx,
            ))
            .child(inspector_row("Duration", self.duration.clone(), cx))
            .when_some(self.document.react_entry(), |panel, entry| {
                panel.child(inspector_row("React entry", entry.to_owned(), cx))
            })
            .when_some(self.component_schema_error.clone(), |panel, error| {
                panel.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(format!("Couldn’t load component schemas: {error}")),
                )
            })
            .when(self.document.react_entry().is_some(), |panel| {
                self.render_project_properties(panel, cx)
            })
            .when(!characters.is_empty(), |panel| {
                self.render_characters(panel, &characters, cx)
            })
            .when_some(selected_asset, |panel, asset| {
                let media = match (asset.missing, self.media_cache.get(&asset.id)) {
                    (true, _) => Some("File not found".to_owned()),
                    (false, Some(Ok(info))) => Some(format_media_asset_info(info)),
                    (false, Some(Err(error))) => Some(format!("Probe failed: {error}")),
                    (false, None) => None,
                };
                panel
                    .child(inspector_section("Asset", cx))
                    .child(inspector_row("ID", asset.id.clone(), cx))
                    .child(inspector_row("Type", asset.kind.to_string(), cx))
                    .when_some(asset.path.clone(), |panel, path| {
                        panel.child(inspector_row("File", path.display().to_string(), cx))
                    })
                    .when_some(media, |panel, media| {
                        panel.child(inspector_row("Media", media, cx))
                    })
            })
            .when_some(selected_track, |panel, track| {
                let mut state = vec![if track.enabled { "Enabled" } else { "Disabled" }];
                if track.locked {
                    state.push("Locked");
                }
                if track.muted {
                    state.push("Muted");
                }
                if track.solo {
                    state.push("Solo");
                }
                panel
                    .child(inspector_section("Track", cx))
                    .child(inspector_row("Name", track.name.clone(), cx))
                    .child(inspector_row("ID", track.id.clone(), cx))
                    .child(inspector_row("Type", format!("{:?}", track.kind), cx))
                    .child(inspector_row("State", state.join(" · "), cx))
                    .child(inspector_row("Clips", track.item_count.to_string(), cx))
            })
            .when_some(selected_clip, |panel, clip| {
                panel
                    .child(inspector_section("Clip", cx))
                    .child(inspector_row("Name", clip.name.clone(), cx))
                    .child(inspector_row("Type", clip_kind_label(clip.kind), cx))
                    .child(inspector_row("Start", format_time(clip.start), cx))
                    .child(inspector_row("Length", format_time(clip.duration), cx))
                    .when(!clip.enabled, |panel| {
                        panel.child(inspector_row("State", "Disabled", cx))
                    })
                    .when_some(clip.volume.as_ref(), |panel, volume| {
                        let local_time = clip_local_time(self.current_time(), &clip);
                        let current = evaluate_f64(volume, local_time).unwrap_or(1.0);
                        let current = format!("{}%", (current * 100.0).round() as i32);
                        panel.child(inspector_row(
                            "Volume",
                            match volume {
                                Animatable::Keyframes(animation) => {
                                    format!("{current} · {} keyframes", animation.keyframes.len())
                                }
                                Animatable::Static(_) => current,
                            },
                            cx,
                        ))
                    })
                    .when_some(clip.component.clone(), |panel, component| {
                        self.render_component_props(panel, &component, cx)
                    })
                    .when_some(clip.dialogue.clone(), |panel, dialogue| {
                        self.render_dialogue_fields(panel, &dialogue, &characters, cx)
                    })
            });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .bg(cx.theme().sidebar)
            .child(panel_header("Inspector", 0, cx))
            .child(contents)
    }

    fn render_characters<E: ParentElement + Sized>(
        &self,
        panel: E,
        characters: &[CharacterSummary],
        cx: &mut Context<Self>,
    ) -> E {
        let panel = panel.child(inspector_section(
            format!("Characters ({})", characters.len()),
            cx,
        ));
        characters.iter().fold(panel, |panel, character| {
            panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(character.name.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} · {} expression(s)",
                                character.id,
                                character.expressions.len()
                            )),
                    )
                    .when_some(character.lip_sync.clone(), |details, lip_sync| {
                        details.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "Lip sync: a={} i={} u={} e={} o={} closed={}",
                                    lip_sync.a,
                                    lip_sync.i,
                                    lip_sync.u,
                                    lip_sync.e,
                                    lip_sync.o,
                                    lip_sync.closed.as_deref().unwrap_or("portrait")
                                )),
                        )
                    }),
            )
        })
    }

    fn render_dialogue_fields<E: ParentElement + Sized>(
        &self,
        panel: E,
        dialogue: &DialogueClipSummary,
        characters: &[CharacterSummary],
        cx: &mut Context<Self>,
    ) -> E {
        let character = characters
            .iter()
            .find(|character| character.id == dialogue.character);
        let expression = dialogue
            .expression
            .clone()
            .or_else(|| character.and_then(|character| character.default_expression.clone()))
            .unwrap_or_else(|| "Default".to_owned());
        panel
            .child(inspector_section("Dialogue", cx))
            .child(inspector_row("Text", dialogue.text.clone(), cx))
            .child(inspector_row(
                "Character",
                character.map_or_else(
                    || dialogue.character.clone(),
                    |character| character.name.clone(),
                ),
                cx,
            ))
            .child(inspector_row("Expression", expression, cx))
            .child(inspector_row(
                "Voice",
                dialogue.audio.clone().unwrap_or_else(|| "None".to_owned()),
                cx,
            ))
            .children(dialogue.audio.is_some().then(|| {
                inspector_row(
                    "Lip sync",
                    if dialogue.lip_sync_cue_count == 0 {
                        "None".to_owned()
                    } else {
                        format!("{} mouth cues", dialogue.lip_sync_cue_count)
                    },
                    cx,
                )
            }))
    }

    /// One row per field of a registered component's property schema, or the
    /// raw configured props when no schema is available.
    fn render_component_props<E: ParentElement + Sized>(
        &self,
        panel: E,
        component: &ComponentClipSummary,
        cx: &mut Context<Self>,
    ) -> E {
        let panel = panel
            .child(inspector_section("Component", cx))
            .child(inspector_row("Registered as", component.name.clone(), cx));
        match self.component_schemas.get(&component.name) {
            Some(schema) => schema.iter().fold(panel, |panel, (key, field)| {
                let (label, value) = property_display(key, field, component.props.get(key));
                panel.child(inspector_row(label, value, cx))
            }),
            None if self.component_schema_pending => {
                panel.child(inspector_note("Loading property schema…", cx))
            }
            None => component.props.iter().fold(panel, |panel, (key, value)| {
                panel.child(inspector_row(key.clone(), json_display(value), cx))
            }),
        }
    }

    /// The entry-declared project properties with their current values.
    /// Values not set in the project fall back to each field's declared
    /// default — the same rule `useProjectProperty` applies in React.
    fn render_project_properties<E: ParentElement + Sized>(
        &self,
        panel: E,
        cx: &mut Context<Self>,
    ) -> E {
        match self.project_property_schema.as_ref() {
            Some(schema) if !schema.is_empty() => schema.iter().fold(
                panel.child(inspector_section("Project properties", cx)),
                |panel, (key, field)| {
                    let (label, value) =
                        property_display(key, field, self.document.project_properties().get(key));
                    panel.child(inspector_row(label, value, cx))
                },
            ),
            _ => panel,
        }
    }

    fn reload_react_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.request_react_reload(cx);
        cx.notify();
    }

    /// Right-hand info panel shown instead of the asset/inspector panels while
    /// previewing a standalone React composition. Read-only facts plus a
    /// manual Reload button; live editing happens in the `.tsx` file.
    fn react_preview_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entry = self
            .react_preview
            .as_ref()
            .map(|react| react.entry.display().to_string())
            .unwrap_or_default();
        let label_color = cx.theme().muted_foreground;
        let value_color = cx.theme().foreground;
        let row = move |label: &str, value: String| {
            div()
                .flex()
                .justify_between()
                .gap_2()
                .text_xs()
                .child(div().text_color(label_color).child(label.to_owned()))
                .child(div().text_color(value_color).text_right().child(value))
        };
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex_col()
            .bg(cx.theme().sidebar)
            .child(panel_header("React Preview", 0, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .child(div().text_xs().text_color(label_color).child("Entry"))
                    .child(div().text_xs().text_color(value_color).child(entry))
                    .child(div().h(px(4.0)))
                    .child(row("Size", self.dimensions.to_string()))
                    .child(row("Frame rate", self.frame_rate_label.to_string()))
                    .child(row("Duration", self.duration.to_string()))
                    .child(row("Renderer", self.gpu_name.to_string()))
                    .child(div().h(px(4.0)))
                    .child(
                        Button::new("reload-react-entry")
                            .small()
                            .w_full()
                            .label("Reload composition")
                            .on_click(cx.listener(Self::reload_react_click)),
                    )
                    .when(!self.preview_warnings.is_empty(), |panel| {
                        panel.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .mt_2()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(cx.theme().warning.opacity(0.5))
                                .bg(cx.theme().warning.opacity(0.12))
                                .p_2()
                                .text_xs()
                                .text_color(cx.theme().warning)
                                .children(self.preview_warnings.iter().cloned()),
                        )
                    }),
            )
    }

    fn timeline(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let progress = if self.clock.end_frame() == 0 {
            0.0
        } else {
            self.clock.frame() as f32 / self.clock.end_frame() as f32
        };
        let total_duration = self.document.duration().as_seconds().unwrap_or(0.0);
        let current_time = self.current_time();
        let (zoom, view_start) = self.timeline_view();
        // Map a whole-composition fraction / width into the visible window.
        let vx = move |fraction: f32| ((f64::from(fraction) - view_start) * zoom) as f32;
        let vw = move |width: f32| (f64::from(width) * zoom) as f32;
        let t = cx.theme();
        let row_border = t.border;
        let row_selected_bg = t.list_active;
        let clip_label = theme::waveform();
        let header_text = t.foreground;
        let muted_text = t.muted_foreground;
        let meter_track_bg = t.background;
        let rows = self.tracks.iter().map(|track| {
            let selected_track = self.selected_track_id.as_deref() == Some(track.id.as_str());
            let track_level = track
                .clips
                .iter()
                .filter_map(|clip| {
                    self.clip_levels
                        .get(&clip.id)
                        .map(|levels| level_at_time(levels, clip, current_time))
                })
                .fold(0.0_f32, f32::max)
                .sqrt()
                .clamp(0.0, 1.0);
            let meter_color = theme::meter(track_level);
            let color = theme::clip_fill(track.kind);
            let clips = track.clips.iter().map(|clip| {
                let start = if total_duration > 0.0 {
                    (clip.start.as_seconds().unwrap_or(0.0) / total_duration).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                };
                let duration = if total_duration > 0.0 {
                    (clip.duration.as_seconds().unwrap_or(0.0) / total_duration).clamp(0.0, 1.0)
                        as f32
                } else {
                    0.0
                };
                let kind = match clip.kind {
                    ClipKind::Video => "VIDEO",
                    ClipKind::Audio => "AUDIO",
                    ClipKind::Image => "IMAGE",
                    ClipKind::Text => "TEXT",
                    ClipKind::Dialogue => "DIALOGUE",
                    ClipKind::Component => "COMPONENT",
                };
                let clip_id = clip.id.clone();
                let element_id: SharedString = format!("timeline-clip-{clip_id}").into();
                let selected = self.selected_clip_id.as_deref() == Some(clip.id.as_str());
                let waveform = self
                    .clip_waveforms
                    .get(&clip.id)
                    .map(|peaks| waveform_segment(peaks, 0.0, 1.0, 48))
                    .unwrap_or_default();
                div()
                    .id(element_id)
                    .absolute()
                    .left(relative(vx(start)))
                    .top(px(5.0))
                    .h(px(28.0))
                    .w(relative(vw(duration)))
                    .min_w(px(3.0))
                    .overflow_hidden()
                    .rounded_sm()
                    .bg(rgb(color))
                    .when(selected, |clip| {
                        clip.border_2().border_color(theme::clip_selected_border())
                    })
                    .when(!clip.enabled, |clip| clip.opacity(0.4))
                    .px_2()
                    .text_xs()
                    .text_color(clip_label)
                    .when(!waveform.is_empty(), |clip| {
                        clip.child(
                            div()
                                .absolute()
                                .left(px(2.0))
                                .right(px(2.0))
                                .top(px(4.0))
                                .bottom(px(4.0))
                                .flex()
                                .items_center()
                                .opacity(0.48)
                                .children(waveform.into_iter().map(|amplitude| {
                                    div()
                                        .flex_1()
                                        .min_w(px(1.0))
                                        .h(relative(amplitude.clamp(0.04, 1.0)))
                                        .bg(clip_label)
                                })),
                        )
                    })
                    .child(div().relative().child(format!("{kind}  {}", clip.name)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.selected_clip_id = Some(clip_id.clone());
                        this.selected_asset_id = None;
                        cx.notify();
                    }))
            });
            let track_name = match (track.enabled, track.locked) {
                (false, true) => format!("{}  [off, locked]", track.name),
                (false, false) => format!("{}  [off]", track.name),
                (true, true) => format!("{}  [locked]", track.name),
                (true, false) => track.name.clone(),
            };
            let mute_track_id = track.id.clone();
            let solo_track_id = track.id.clone();
            let select_track_id = track.id.clone();
            let mute_element_id: SharedString = format!("track-mute-{}", track.id).into();
            let solo_element_id: SharedString = format!("track-solo-{}", track.id).into();
            let track_element_id: SharedString = format!("timeline-track-{}", track.id).into();
            div()
                .id(track_element_id)
                .flex()
                .flex_none()
                .h(px(38.0))
                .w_full()
                .border_b_1()
                .border_color(row_border)
                .when(selected_track, |row| row.bg(row_selected_bg))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.selected_track_id.as_deref() == Some(select_track_id.as_str()) {
                        this.selected_track_id = None;
                    } else {
                        this.selected_track_id = Some(select_track_id.clone());
                        this.selected_asset_id = None;
                    }
                    cx.notify();
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .w(px(230.0))
                        .px_3()
                        .text_sm()
                        .text_color(header_text)
                        .child(div().flex_1().overflow_hidden().child(track_name))
                        .when(track.kind != TrackKind::Overlay, |header| {
                            header.child(
                                div()
                                    .relative()
                                    .flex_none()
                                    .w(px(34.0))
                                    .h(px(6.0))
                                    .rounded_full()
                                    .overflow_hidden()
                                    .bg(meter_track_bg)
                                    .child(
                                        div()
                                            .h_full()
                                            .w(relative(track_level))
                                            .rounded_full()
                                            .bg(meter_color),
                                    ),
                            )
                        })
                        .when(track.kind != TrackKind::Overlay, |header| {
                            header
                                .child(
                                    Button::new(mute_element_id)
                                        .xsmall()
                                        .ghost()
                                        .label("M")
                                        .selected(track.muted)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.toggle_track_mute(&mute_track_id, cx);
                                        })),
                                )
                                .child(
                                    Button::new(solo_element_id)
                                        .xsmall()
                                        .ghost()
                                        .label("S")
                                        .selected(track.solo)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.toggle_track_solo(&solo_track_id, cx);
                                        })),
                                )
                        }),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("clip-lane-{}", track.id)))
                        .relative()
                        .flex_1()
                        .h_full()
                        .mr_2()
                        .overflow_hidden()
                        .on_scroll_wheel(cx.listener(Self::timeline_wheel))
                        .on_mouse_down(MouseButton::Middle, cx.listener(Self::begin_timeline_pan))
                        .children(clips),
                )
        });
        let track_area = div()
            .id("timeline-tracks-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .when(self.tracks.is_empty() && !self.is_react_preview(), |area| {
                area.child(
                    div()
                        .flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(muted_text)
                        .child("This project has no tracks"),
                )
            })
            .children(rows);
        div()
            .id("timeline-panel")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .bg(cx.theme().secondary)
            .border_t_1()
            .border_color(cx.theme().border)
            .on_mouse_move(cx.listener(Self::continue_timeline_pan))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::end_timeline_pan))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .h(px(38.0))
                    .items_center()
                    .px_3()
                    .justify_between()
                    .child(
                        div().flex().items_center().gap_1().child(
                            div()
                                .text_sm()
                                .mr_1()
                                .text_color(header_text)
                                .child("Timeline"),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_xs().text_color(muted_text).child(format!(
                                "{} / {}",
                                format_time(self.current_time()),
                                self.duration
                            )))
                            .when(zoom > 1.001, |controls| {
                                controls.child(
                                    Button::new("timeline-zoom-fit")
                                        .xsmall()
                                        .ghost()
                                        .label(format!("{zoom:.1}× · Fit"))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.timeline_zoom = 1.0;
                                            this.timeline_view_start = 0.0;
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(
                                Button::new("set-export-in")
                                    .xsmall()
                                    .ghost()
                                    .label("In")
                                    .tooltip("Mark export start (I)")
                                    .selected(self.export_in_frame.is_some())
                                    .on_click(cx.listener(|this, _, _, cx| this.set_export_in(cx))),
                            )
                            .child(
                                Button::new("set-export-out")
                                    .xsmall()
                                    .ghost()
                                    .label("Out")
                                    .tooltip("Mark export end (O)")
                                    .selected(self.export_out_frame.is_some())
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.set_export_out(cx)),
                                    ),
                            )
                            .when_some(self.export_range_label(), |controls, label| {
                                controls
                                    .child(div().text_xs().text_color(theme::accent()).child(label))
                                    .child(
                                        Button::new("clear-export-range")
                                            .xsmall()
                                            .ghost()
                                            .label("✕")
                                            .tooltip("Clear export range (Shift+X)")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.clear_export_range(cx)
                                            })),
                                    )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .h(px(34.0))
                    .w_full()
                    .items_start()
                    .child(div().w(px(230.0)))
                    .child(
                        div()
                            .id("timeline-scrubber")
                            .relative()
                            .flex_1()
                            .h(px(34.0))
                            .mr_2()
                            .overflow_x_hidden()
                            .cursor_pointer()
                            .on_scroll_wheel(cx.listener(Self::timeline_wheel))
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::begin_scrub))
                            .on_mouse_down(
                                MouseButton::Middle,
                                cx.listener(Self::begin_timeline_pan),
                            )
                            .on_mouse_move(cx.listener(Self::continue_scrub))
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::end_scrub))
                            // Ruler band: tick + centred label per mark.
                            .children(
                                ruler_marks(total_duration)
                                    .into_iter()
                                    .filter_map(|(fraction, labelled)| {
                                        let x = vx(fraction);
                                        (0.0..=1.0).contains(&x).then_some((x, fraction, labelled))
                                    })
                                    .flat_map(|(x, fraction, labelled)| {
                                        let tick = div()
                                            .absolute()
                                            .top(px(0.0))
                                            .left(relative(x))
                                            .w(px(1.0))
                                            .h(px(4.0))
                                            .bg(row_border)
                                            .into_any_element();
                                        let label = (labelled && x > 0.02 && x < 0.95).then(|| {
                                            div()
                                                .absolute()
                                                .top(px(5.0))
                                                .left(relative(x))
                                                .w(px(48.0))
                                                .ml(px(-24.0))
                                                .text_center()
                                                .text_xs()
                                                .text_color(muted_text)
                                                .child(format_ruler_label(
                                                    f64::from(fraction) * total_duration,
                                                ))
                                                .into_any_element()
                                        });
                                        std::iter::once(tick).chain(label)
                                    }),
                            )
                            // Scrub track along the bottom of the band.
                            .child(
                                div()
                                    .absolute()
                                    .top(px(24.0))
                                    .left(px(0.0))
                                    .w_full()
                                    .h(px(3.0))
                                    .bg(row_border),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(24.0))
                                    .left(relative(vx(0.0).max(0.0)))
                                    .h(px(3.0))
                                    .w(relative((vx(progress) - vx(0.0).max(0.0)).max(0.0)))
                                    .bg(theme::accent()),
                            )
                            .when_some(self.export_range_band(), |scrubber, (start, end)| {
                                scrubber.child(
                                    div()
                                        .absolute()
                                        .top(px(20.0))
                                        .left(relative(vx(start)))
                                        .w(relative(vw((end - start).max(0.0))))
                                        .h(px(9.0))
                                        .rounded_sm()
                                        .bg(theme::export_range_fill())
                                        .border_1()
                                        .border_color(theme::export_range_border()),
                                )
                            })
                            .child(
                                div()
                                    .absolute()
                                    .top(px(20.0))
                                    .left(relative(vx(progress)))
                                    .ml(px(-5.0))
                                    .size(px(11.0))
                                    .rounded_full()
                                    .bg(theme::accent()),
                            ),
                    ),
            )
            .child(
                // Track scroll region, with a playhead line drawn over the clip
                // lanes. The overlay is inset by the 230px header column and the
                // 8px right gutter (and clips overflow), so the line tracks the
                // clips under the current zoom. A plain (non-interactive) div,
                // so clip clicks pass straight through.
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(track_area)
                    .when(
                        !self.tracks.is_empty() && (0.0..=1.0).contains(&vx(progress)),
                        |region| {
                            region.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .left(px(230.0))
                                    .right(px(8.0))
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .bottom_0()
                                            .w(px(2.0))
                                            .left(relative(vx(progress)))
                                            .bg(theme::accent().opacity(0.7)),
                                    ),
                            )
                        },
                    ),
            )
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.poll_background_work();
        self.update_playback(window);
        if self.preview_pending
            || self.media_pending
            || self.audio_pending
            || self.export_cancellation.is_some()
        {
            window.request_animation_frame();
        }
        window.set_window_title(&format!("{} — Celesta", self.project_name));
        div()
            .key_context("CelestaEditor")
            .on_action(cx.listener(Self::open_project_action))
            .on_action(cx.listener(Self::reload_project_action))
            .on_action(cx.listener(Self::close_window_action))
            .on_action(cx.listener(Self::export_project_action))
            .on_action(cx.listener(Self::set_export_in_action))
            .on_action(cx.listener(Self::set_export_out_action))
            .on_action(cx.listener(Self::clear_export_range_action))
            .on_action(cx.listener(Self::set_up_typescript_action))
            .on_action(cx.listener(Self::toggle_playback_action))
            .on_action(cx.listener(Self::previous_frame_action))
            .on_action(cx.listener(Self::next_frame_action))
            .when_some(self.focus_handle.as_ref(), |view, focus_handle| {
                view.track_focus(focus_handle)
            })
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().background)
            .font_family(".SystemUIFont")
            .child(self.title_bar(cx))
            .child(self.workspace(cx))
            .child(self.status_bar(cx))
    }
}

impl EditorView {
    /// The resizable shell: the asset list, the monitor, and the inspector,
    /// stacked above the timeline. In React-preview mode the asset list is
    /// hidden and the right dock shows composition facts instead of the
    /// inspector.
    fn workspace(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let react = self.is_react_preview();
        let dock_state = self.dock_split.clone();
        let body_state = self.body_split.clone();
        let border = cx.theme().border;
        let dock_row = h_resizable("celesta-dock-row")
            .when_some(dock_state.as_ref(), |group, state| group.with_state(state))
            .child(
                resizable_panel()
                    .size(px(260.0))
                    .size_range(px(200.0)..px(440.0))
                    .visible(!react)
                    .child(
                        div()
                            .size_full()
                            .min_h_0()
                            .overflow_hidden()
                            .border_r_1()
                            .border_color(border)
                            .child(self.asset_panel(cx)),
                    ),
            )
            .child(resizable_panel().child(self.preview_panel(cx)))
            .child(
                resizable_panel()
                    .size(px(300.0))
                    .size_range(px(240.0)..px(520.0))
                    .child(
                        div()
                            .size_full()
                            .min_h_0()
                            .overflow_hidden()
                            .border_l_1()
                            .border_color(border)
                            .child(if react {
                                self.react_preview_panel(cx).into_any_element()
                            } else {
                                self.inspector_panel(cx).into_any_element()
                            }),
                    ),
            );
        div()
            .flex()
            .flex_1()
            .w_full()
            .min_h_0()
            .overflow_hidden()
            .child(
                v_resizable("celesta-body")
                    .when_some(body_state.as_ref(), |group, state| group.with_state(state))
                    .child(resizable_panel().child(dock_row))
                    .child(
                        resizable_panel()
                            .size(px(240.0))
                            .size_range(px(140.0)..px(560.0))
                            .child(self.timeline(cx)),
                    ),
            )
    }

    fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        StatusBar::new()
            .left(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{}  ·  {}  ·  {}",
                        self.dimensions, self.frame_rate_label, self.gpu_name
                    )),
            )
            .when_some(
                self.export_progress.map(export_progress_label),
                |bar, label| {
                    bar.right(
                        div()
                            .text_xs()
                            .text_color(cx.theme().foreground)
                            .child(label),
                    )
                },
            )
    }
}

fn panel_header(title: &'static str, count: usize, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .h(px(40.0))
        .items_center()
        .justify_between()
        .px_3()
        .border_b_1()
        .border_color(cx.theme().border)
        .text_sm()
        .text_color(cx.theme().foreground)
        .child(title)
        .when(count > 0, |header| {
            header.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(count.to_string()),
            )
        })
}

fn waveform_peaks(buffer: &AudioBuffer, requested_buckets: usize) -> Vec<f32> {
    let frame_count = buffer.frame_count();
    if frame_count == 0 || requested_buckets == 0 || buffer.channels == 0 {
        return Vec::new();
    }
    let bucket_count = requested_buckets.min(frame_count);
    let channels = usize::from(buffer.channels);
    let mut peaks = vec![0.0_f32; bucket_count];
    for (frame_index, frame) in buffer.samples.chunks_exact(channels).enumerate() {
        let bucket = frame_index * bucket_count / frame_count;
        let amplitude = frame
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        peaks[bucket] = peaks[bucket].max(amplitude);
    }
    peaks
}

fn media_asset_info(probe: &MediaProbe) -> MediaAssetInfo {
    let duration = probe
        .duration
        .or_else(|| probe.video.as_ref().and_then(|video| video.duration))
        .or_else(|| probe.audio.iter().find_map(|audio| audio.duration));
    MediaAssetInfo {
        duration,
        video_size: probe
            .video
            .as_ref()
            .map(|video| (video.width, video.height)),
        has_audio: !probe.audio.is_empty(),
    }
}

fn format_media_asset_info(info: &MediaAssetInfo) -> String {
    let mut parts = Vec::new();
    if let Some((width, height)) = info.video_size {
        parts.push(format!("{width}×{height}"));
    }
    if let Some(duration) = info
        .duration
        .and_then(|duration| duration.as_seconds().ok())
    {
        parts.push(format!("{duration:.3}s"));
    }
    if info.has_audio && info.video_size.is_some() {
        parts.push("audio".to_owned());
    }
    if parts.is_empty() {
        "No media streams".to_owned()
    } else {
        parts.join(" · ")
    }
}

fn map_clip_waveform(
    peaks: &[f32],
    source_frames: usize,
    sample_rate: u32,
    clip: &AudioClip,
    requested_buckets: usize,
) -> Vec<f32> {
    if peaks.is_empty() || source_frames == 0 || sample_rate == 0 || requested_buckets == 0 {
        return Vec::new();
    }
    let Ok(source_start) = clip.source_start.as_seconds() else {
        return Vec::new();
    };
    let source_limit = match clip.source_duration {
        Some(duration) => {
            let Ok(duration) = duration.as_seconds() else {
                return Vec::new();
            };
            Some(source_start + duration)
        }
        None => None,
    };
    let source_duration = source_frames as f64 / f64::from(sample_rate);
    let bucket_count = requested_buckets.min(peaks.len().max(1));
    (0..bucket_count)
        .map(|bucket| {
            let Some(local_start) = time_fraction(clip.range.duration, bucket, bucket_count) else {
                return 0.0;
            };
            let Some(local_end) = time_fraction(clip.range.duration, bucket + 1, bucket_count)
            else {
                return 0.0;
            };
            let Ok(mapped_start) = integrate_f64(&clip.playback_rate, local_start) else {
                return 0.0;
            };
            let Ok(mapped_end) = integrate_f64(&clip.playback_rate, local_end) else {
                return 0.0;
            };
            let start = (source_start + mapped_start).max(0.0);
            let mut end = (source_start + mapped_end).max(start);
            if let Some(limit) = source_limit {
                if start >= limit {
                    return 0.0;
                }
                end = end.min(limit);
            }
            if start >= source_duration || end <= start {
                return 0.0;
            }
            let start_fraction = (start / source_duration).clamp(0.0, 1.0) as f32;
            let duration_fraction =
                ((end.min(source_duration) - start) / source_duration).clamp(0.0, 1.0) as f32;
            waveform_segment(peaks, start_fraction, duration_fraction, 1)
                .into_iter()
                .next()
                .unwrap_or(0.0)
        })
        .collect()
}

fn clip_level_envelope(waveform: &[f32], clip: &AudioClip) -> Vec<f32> {
    let bucket_count = waveform.len();
    waveform
        .iter()
        .enumerate()
        .map(|(bucket, peak)| {
            let Some(local_time) = time_fraction(clip.range.duration, bucket, bucket_count) else {
                return 0.0;
            };
            let Ok(volume) = evaluate_f64(&clip.volume, local_time) else {
                return 0.0;
            };
            (*peak * volume.clamp(0.0, 16.0) as f32).clamp(0.0, 1.0)
        })
        .collect()
}

fn master_volume_from_drag(start_volume: f64, delta_pixels: f64) -> f64 {
    const SLIDER_WIDTH: f64 = 88.0;
    ((start_volume + delta_pixels / SLIDER_WIDTH * 2.0).clamp(0.0, 2.0) * 100.0).round() / 100.0
}

fn level_at_time(levels: &[f32], clip: &celesta_editor_core::ClipSummary, time: Time) -> f32 {
    if levels.is_empty() {
        return 0.0;
    }
    let (Ok(now), Ok(start), Ok(duration)) = (
        time.as_seconds(),
        clip.start.as_seconds(),
        clip.duration.as_seconds(),
    ) else {
        return 0.0;
    };
    if duration <= 0.0 || now < start || now >= start + duration {
        return 0.0;
    }
    let index = (((now - start) / duration) * levels.len() as f64).floor() as usize;
    levels[index.min(levels.len() - 1)]
}

fn clip_local_time(time: Time, clip: &celesta_editor_core::ClipSummary) -> Time {
    if time
        .cmp_exact(clip.start)
        .is_ok_and(|ordering| !ordering.is_gt())
    {
        return Time::ZERO;
    }
    let local = time.checked_sub(clip.start).unwrap_or(Time::ZERO);
    if local
        .cmp_exact(clip.duration)
        .is_ok_and(|ordering| ordering.is_gt())
    {
        clip.duration
    } else {
        local
    }
}

fn time_fraction(duration: Time, numerator: usize, denominator: usize) -> Option<Time> {
    let value = i128::from(duration.value).checked_mul(i128::try_from(numerator).ok()?)?;
    let timescale = u64::from(duration.timescale).checked_mul(u64::try_from(denominator).ok()?)?;
    Some(Time::new(
        i64::try_from(value).ok()?,
        u32::try_from(timescale).ok()?,
    ))
}

fn waveform_segment(
    peaks: &[f32],
    start_fraction: f32,
    duration_fraction: f32,
    max_bars: usize,
) -> Vec<f32> {
    if peaks.is_empty() || duration_fraction <= 0.0 || max_bars == 0 {
        return Vec::new();
    }
    let start = (start_fraction.clamp(0.0, 1.0) * peaks.len() as f32).floor() as usize;
    let end =
        ((start_fraction + duration_fraction).clamp(0.0, 1.0) * peaks.len() as f32).ceil() as usize;
    let values = &peaks[start.min(peaks.len())..end.max(start + 1).min(peaks.len())];
    let bar_count = values.len().min(max_bars);
    (0..bar_count)
        .map(|bar| {
            let from = bar * values.len() / bar_count;
            let to = ((bar + 1) * values.len()).div_ceil(bar_count);
            values[from..to].iter().copied().fold(0.0_f32, f32::max)
        })
        .collect()
}

fn inspector_row(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(value.into()),
        )
}

fn inspector_section(title: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .mt_3()
        .px_3()
        .py_2()
        .border_t_1()
        .border_b_1()
        .border_color(cx.theme().border)
        .text_sm()
        .text_color(cx.theme().foreground)
        .child(title.into())
}

fn inspector_note(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

/// The label and current value of one schema-described property, falling
/// back to the field's declared default when the value is unset.
fn property_display(
    key: &str,
    field: &ComponentPropertyField,
    current: Option<&serde_json::Value>,
) -> (SharedString, String) {
    let (label, value) = match field {
        ComponentPropertyField::Boolean {
            label,
            default_value,
            ..
        } => {
            let value = current
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(*default_value);
            (label, if value { "On" } else { "Off" }.to_owned())
        }
        ComponentPropertyField::Number {
            label,
            default_value,
            ..
        } => (
            label,
            format_component_number(
                current
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(*default_value),
            ),
        ),
        ComponentPropertyField::String {
            label,
            default_value,
            ..
        }
        | ComponentPropertyField::Color {
            label,
            default_value,
            ..
        }
        | ComponentPropertyField::Select {
            label,
            default_value,
            ..
        } => (
            label,
            current
                .and_then(serde_json::Value::as_str)
                .map_or_else(|| default_value.clone(), str::to_owned),
        ),
    };
    (
        label
            .clone()
            .map_or_else(|| key.to_owned().into(), Into::into),
        value,
    )
}

/// A prop value without a schema: strings unquoted, everything else as JSON.
fn json_display(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn format_component_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

fn prepare_preview_frame(frame: GpuPreviewFrame) -> PreviewPresentation {
    match frame {
        GpuPreviewFrame::Cpu(frame) => {
            let width = frame.width();
            let height = frame.height();
            let mut pixels = frame.into_pixels();
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            PreviewPresentation::Image(frame_to_image(CpuPreviewFrame {
                width,
                height,
                pixels,
            }))
        }
        #[cfg(target_os = "macos")]
        GpuPreviewFrame::Native(frame) => PreviewPresentation::Surface(frame),
    }
}

fn frame_to_image(frame: CpuPreviewFrame) -> Arc<RenderImage> {
    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(frame.width, frame.height, frame.pixels)
        .expect("GPU preview frame dimensions match its pixel buffer");
    Arc::new(RenderImage::new([Frame::new(buffer)]))
}

fn clip_kind_label(kind: ClipKind) -> &'static str {
    match kind {
        ClipKind::Video => "Video",
        ClipKind::Audio => "Audio",
        ClipKind::Image => "Image",
        ClipKind::Text => "Text",
        ClipKind::Dialogue => "Dialogue",
        ClipKind::Component => "Component",
    }
}

fn format_time(time: Time) -> String {
    let total = time.as_seconds().unwrap_or(0.0).max(0.0);
    let hours = (total / 3600.0).floor() as u64;
    let minutes = ((total % 3600.0) / 60.0).floor() as u64;
    let seconds = total % 60.0;
    format!("{hours:02}:{minutes:02}:{seconds:06.3}")
}

/// Compact `m:ss` / `s.s` label for a timeline ruler mark.
fn format_ruler_label(seconds: f64) -> String {
    if seconds >= 60.0 {
        format!("{}:{:02}", (seconds / 60.0) as u64, (seconds % 60.0) as u64)
    } else if seconds.fract().abs() < f64::EPSILON {
        format!("{}s", seconds as u64)
    } else {
        format!("{seconds:.1}s")
    }
}

/// Evenly spaced ruler marks across `total` seconds: `(fraction, is_labelled)`.
/// The step is chosen so the ruler shows roughly 6–14 marks, and every other
/// (or every) mark carries a time label.
fn ruler_marks(total: f64) -> Vec<(f32, bool)> {
    if total.is_nan() || total <= 0.0 {
        return Vec::new();
    }
    const STEPS: [f64; 8] = [0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 300.0];
    let step = STEPS
        .into_iter()
        .find(|step| total / step <= 14.0)
        .unwrap_or(600.0);
    let count = (total / step).floor() as u64;
    let label_every = if (count as f64 / 2.0) > 7.0 { 2 } else { 1 };
    (0..=count)
        .map(|i| {
            let seconds = i as f64 * step;
            ((seconds / total) as f32, i % label_every == 0)
        })
        .collect()
}

fn export_progress_label(progress: ExportProgress) -> String {
    match progress {
        ExportProgress::Rendering { frame: 0, total: 0 } => "Starting export…".to_owned(),
        ExportProgress::Rendering { frame, total } => {
            format!("Exporting frame {frame}/{total}")
        }
        ExportProgress::MixingAudio => "Mixing export audio…".to_owned(),
        ExportProgress::Muxing => "Muxing MP4…".to_owned(),
    }
}

/// Turns the editor's in/out frame markers into an [`ExportRange`]. Returns
/// `None` (export the whole composition) unless both are set with `in < out`.
fn export_range_for(
    in_frame: Option<i64>,
    out_frame: Option<i64>,
    frame_rate: Rational,
) -> Option<ExportRange> {
    let (start, end) = (in_frame?, out_frame?);
    if start < 0 || end <= start {
        return None;
    }
    Some(ExportRange::new(
        Time::frames(start, frame_rate).ok()?,
        Time::frames(end, frame_rate).ok()?,
    ))
}

fn export_suggested_name(path: Option<&Path>, project_name: &str) -> String {
    let name = path
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .unwrap_or(project_name);
    let stem = name
        .strip_suffix(".celesta.json")
        .or_else(|| name.strip_suffix(".json"))
        .unwrap_or(name);
    format!("{stem}.mp4")
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Celesta: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    let mut editor = EditorView::open(path.as_deref())?;

    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    app.run(move |cx: &mut App| {
        gpui_kit::init(cx);
        theme::init(cx);
        cx.bind_keys([
            KeyBinding::new("secondary-o", OpenProject, Some("CelestaEditor")),
            KeyBinding::new("secondary-r", ReloadProject, Some("CelestaEditor")),
            KeyBinding::new("secondary-w", CloseWindow, Some("CelestaEditor")),
            KeyBinding::new("secondary-q", Quit, None),
            KeyBinding::new("secondary-shift-e", ExportProject, Some("CelestaEditor")),
            KeyBinding::new("i", SetExportIn, Some("CelestaEditor")),
            KeyBinding::new("o", SetExportOut, Some("CelestaEditor")),
            KeyBinding::new("shift-x", ClearExportRange, Some("CelestaEditor")),
            KeyBinding::new("space", TogglePlayback, Some("CelestaEditor")),
            KeyBinding::new("left", PreviousFrame, Some("CelestaEditor")),
            KeyBinding::new("right", NextFrame, Some("CelestaEditor")),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        // macOS shows these in the system menu bar; elsewhere the title bar's
        // `AppMenuBar` renders the same list.
        cx.set_menus(app_menus());
        GlobalState::global_mut(cx)
            .set_app_menus(app_menus().into_iter().map(Menu::owned).collect());
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(960.0), px(600.0))),
                ..TitleBar::window_options()
            },
            |window, cx| {
                let view = cx.new(|cx| {
                    let focus_handle = cx.focus_handle();
                    let master_volume_focus = cx.focus_handle().tab_stop(true).tab_index(0);
                    focus_handle.focus(window, cx);
                    editor.focus_handle = Some(focus_handle);
                    editor.master_volume_focus = Some(master_volume_focus);
                    editor.dock_split = Some(cx.new(|_| ResizableState::default()));
                    editor.body_split = Some(cx.new(|_| ResizableState::default()));
                    #[cfg(not(target_os = "macos"))]
                    {
                        editor.app_menu_bar = Some(AppMenuBar::new(cx));
                    }
                    if editor.is_react_preview() {
                        editor.watch_react_entry(cx);
                    }
                    editor
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("could not open the Celesta window");
        cx.activate(true);
    });
    Ok(())
}

fn app_menus() -> Vec<Menu> {
    vec![
        Menu::new("Celesta").items([MenuItem::action("Quit Celesta", Quit)]),
        Menu::new("File").items([
            MenuItem::action("Open…", OpenProject),
            MenuItem::action("Reload", ReloadProject),
            MenuItem::separator(),
            MenuItem::action("Export…", ExportProject),
            MenuItem::separator(),
            MenuItem::action("Set Up TypeScript", SetUpTypeScript),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        Menu::new("Playback").items([
            MenuItem::action("Play/Pause", TogglePlayback),
            MenuItem::action("Previous Frame", PreviousFrame),
            MenuItem::action("Next Frame", NextFrame),
            MenuItem::separator(),
            MenuItem::action("Mark Export Start", SetExportIn),
            MenuItem::action("Mark Export End", SetExportOut),
            MenuItem::action("Clear Export Range", ClearExportRange),
        ]),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        AudioCacheKey, CachedAudioDecoder, ClipKind, DiskAudioCache, EDITOR_DEMO_PROJECT,
        ExportEvent, ExportRequest, ExportSource, ExportWorker, clip_level_envelope,
        export_range_for, export_suggested_name, is_react_entry, level_at_time, map_clip_waveform,
        master_volume_from_drag, take_latest, waveform_peaks, waveform_segment,
    };
    use celesta_composition::{
        Animatable, AssetLocation, AudioClip, Rational, ResolvedAsset, Time, TimeRange,
    };
    use celesta_editor_core::ClipSummary;
    use celesta_exporter::ExportCancellation;
    use celesta_media::{AudioBuffer, AudioDecoder};
    use celesta_project::Project;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn background_workers_coalesce_queued_requests() {
        let (sender, receiver) = mpsc::channel();
        sender.send(2).unwrap();
        sender.send(3).unwrap();

        assert_eq!(take_latest(1, &receiver), 3);
    }

    #[test]
    fn export_worker_reports_cancellation_without_creating_output() {
        let worker = ExportWorker::spawn().unwrap();
        let cancellation = ExportCancellation::default();
        cancellation.cancel();
        let output = std::env::temp_dir().join(format!(
            "celesta-cancelled-export-{}.mp4",
            std::process::id()
        ));
        let _ = fs::remove_file(&output);
        worker
            .request(ExportRequest {
                source: ExportSource::Project(Box::new(
                    Project::from_json(EDITOR_DEMO_PROJECT).unwrap(),
                )),
                asset_root: PathBuf::from("examples"),
                output: output.clone(),
                range: None,
                cancellation,
            })
            .unwrap();

        match worker.events.recv_timeout(Duration::from_secs(2)).unwrap() {
            ExportEvent::Finished {
                result, cancelled, ..
            } => {
                assert!(cancelled);
                assert_eq!(result.unwrap_err(), "export was cancelled");
            }
            ExportEvent::Progress(progress) => panic!("unexpected export progress: {progress:?}"),
        }
        assert!(!output.exists());
    }

    #[test]
    fn open_recognizes_react_entries_but_not_projects() {
        assert!(is_react_entry(PathBuf::from("title.tsx").as_path()));
        assert!(is_react_entry(PathBuf::from("scene.mjs").as_path()));
        assert!(!is_react_entry(
            PathBuf::from("demo.celesta.json").as_path()
        ));
        assert!(!is_react_entry(PathBuf::from("notes.txt").as_path()));
    }

    #[test]
    fn export_range_needs_both_markers_ordered() {
        let rate = Rational::new(30, 1);
        assert_eq!(export_range_for(None, Some(30), rate), None);
        assert_eq!(export_range_for(Some(30), None, rate), None);
        assert_eq!(export_range_for(Some(30), Some(30), rate), None);
        assert_eq!(export_range_for(Some(30), Some(20), rate), None);

        let range = export_range_for(Some(30), Some(90), rate).unwrap();
        assert_eq!(range.start, Time::frames(30, rate).unwrap());
        assert_eq!(range.end, Some(Time::frames(90, rate).unwrap()));
    }

    #[test]
    fn export_name_replaces_project_extensions() {
        assert_eq!(
            export_suggested_name(
                Some(PathBuf::from("demo.celesta.json").as_path()),
                "ignored"
            ),
            "demo.mp4"
        );
        assert_eq!(export_suggested_name(None, "Untitled"), "Untitled.mp4");
    }

    #[test]
    fn waveform_peaks_are_aligned_and_downsampled_for_clips() {
        let buffer = AudioBuffer {
            sample_rate: 4,
            channels: 2,
            samples: vec![0.1, -0.2, 0.4, -0.3, 0.8, -0.7, 0.2, -0.1],
        };

        let peaks = waveform_peaks(&buffer, 4);
        assert_eq!(peaks, vec![0.2, 0.4, 0.8, 0.2]);
        assert_eq!(waveform_segment(&peaks, 0.5, 0.5, 1), vec![0.8]);
    }

    #[test]
    fn clip_waveforms_follow_source_ranges_and_playback_rate() {
        let mut clip = AudioClip {
            id: "clip".to_owned(),
            asset: ResolvedAsset {
                id: "audio".to_owned(),
                location: AssetLocation::File {
                    path: "audio.wav".to_owned(),
                },
            },
            range: TimeRange {
                start: Time::ZERO,
                duration: Time::new(1, 1),
            },
            source_start: Time::new(1, 2),
            source_duration: Some(Time::new(1, 2)),
            playback_rate: Animatable::Static(1.0),
            volume: Animatable::Static(1.0),
            muted: false,
        };
        let peaks = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];

        assert_eq!(
            map_clip_waveform(&peaks, 8, 4, &clip, 4),
            vec![0.3, 0.4, 0.0, 0.0]
        );

        clip.source_duration = None;
        clip.playback_rate = Animatable::Static(2.0);
        assert_eq!(
            map_clip_waveform(&peaks, 8, 4, &clip, 4),
            vec![0.4, 0.6, 0.8, 0.0]
        );

        clip.volume = Animatable::Static(0.5);
        assert_eq!(
            clip_level_envelope(&[0.4, 0.6, 0.8, 0.0], &clip),
            vec![0.2, 0.3, 0.4, 0.0]
        );
    }

    #[test]
    fn track_levels_follow_the_active_clip_and_master_drag_is_clamped() {
        let clip = ClipSummary {
            id: "clip".to_owned(),
            name: "Clip".to_owned(),
            start: Time::new(10, 1),
            duration: Time::new(2, 1),
            kind: ClipKind::Audio,
            enabled: true,
            volume: Some(Animatable::Static(1.0)),
            component: None,
            dialogue: None,
        };

        assert_eq!(level_at_time(&[0.2, 0.8], &clip, Time::new(10, 1)), 0.2);
        assert_eq!(level_at_time(&[0.2, 0.8], &clip, Time::new(11, 1)), 0.8);
        assert_eq!(level_at_time(&[0.2, 0.8], &clip, Time::new(12, 1)), 0.0);
        assert_eq!(master_volume_from_drag(1.0, 22.0), 1.5);
        assert_eq!(master_volume_from_drag(1.0, -100.0), 0.0);
        assert_eq!(master_volume_from_drag(1.0, 100.0), 2.0);
    }

    #[test]
    fn audio_decoder_reuses_session_pcm_cache() {
        let path = PathBuf::from("/does/not/need/to/exist.wav");
        let key = AudioCacheKey {
            path: path.clone(),
            sample_rate: 48_000,
            channels: 2,
        };
        let expected = AudioBuffer {
            sample_rate: 48_000,
            channels: 2,
            samples: vec![0.25, -0.25],
        };
        let mut decoder = CachedAudioDecoder::new();
        decoder.buffers.insert(key, expected.clone());

        let decoded = decoder.decode_audio(&path, 48_000, 2).unwrap();

        assert_eq!(decoded, expected);
    }

    #[test]
    fn audio_decoder_reuses_pcm_cache_across_sessions() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "celesta-editor-disk-cache-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("voice.wav");
        fs::write(&path, b"not valid audio; cache must be used").unwrap();
        let expected = AudioBuffer {
            sample_rate: 48_000,
            channels: 2,
            samples: vec![0.25, -0.25],
        };
        let disk_cache = DiskAudioCache::new(root.join("cache"));
        disk_cache.store(&path, &expected, &[0.25]).unwrap();
        let mut decoder = CachedAudioDecoder::with_disk_cache(disk_cache);

        let decoded = decoder.decode_audio(&path, 48_000, 2).unwrap();

        assert_eq!(decoded, expected);
        assert_eq!(decoder.waveforms.values().next().unwrap(), &[0.25]);
        fs::remove_dir_all(root).unwrap();
    }
}
