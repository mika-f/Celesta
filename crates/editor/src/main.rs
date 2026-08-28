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
use std::time::{Duration, Instant};

use gpui::{
    App, Application, Bounds, ClickEvent, Context, CursorStyle, Entity, FocusHandle, KeyBinding,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    PathPromptOptions, Pixels, Point, PromptButton, PromptLevel, RenderImage, SharedString,
    StyledImage, Window, WindowBounds, WindowOptions, actions, div, img, prelude::*, px, relative,
    rgb, size,
};
use image::{Frame, ImageBuffer, Rgba};
use mikan_composition::{
    Animatable, AssetLocation, AudioClip, AudioGraph, Layer, LayerContent, Rational, Scene, Time,
    evaluate_f64, integrate_f64,
};
use mikan_editor::{
    AssetSummary, CharacterSummary, ClipKind, ComponentClipSummary, DialogueClipSummary,
    EditorDocument, TimelineClock, TrackSummary,
};
use mikan_exporter::{ExportCancellation, ExportError, ExportOptions, ExportProgress, Exporter};
use mikan_gpu_renderer::{GpuRenderOptions, GpuRenderer, PreviewFrame as GpuPreviewFrame};
use mikan_media::{
    AudioBuffer, AudioDecoder, FfmpegBackend, MediaError, MediaProbe, mix_audio_graph_cancellable,
};
use mikan_project::{AssetKind, Project, TrackKind};
use mikan_react_bridge::{
    ComponentPropertyField, ComponentPropertySchema, ComponentResolutionRequest, ReactBridge,
};
use rodio::{DeviceSinkBuilder, Player, buffer::SamplesBuffer};

mod audio_cache;
mod text_input;

use audio_cache::DiskAudioCache;
use text_input::{TextInput, TextInputEvent};

const EDITOR_DEMO_PROJECT: &str = include_str!("../../../examples/editor-demo.mikan.json");

/// The `node` executable and `@mikan/react` CLI script `ComponentSchemaWorker`
/// spawns to query a `react_entry`'s registered component schemas — the same
/// `node` on `PATH` and workspace-relative `packages/react/dist/cli.js` that
/// `mikan-exporter --react` resolves (see its `main.rs`), so a developer only
/// needs `pnpm install && pnpm run build` in `packages/react` once for both.
fn react_runtime_paths() -> (PathBuf, PathBuf) {
    let cli_script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/react/dist/cli.js");
    (PathBuf::from("node"), cli_script)
}

actions!(
    mikan_editor,
    [
        SaveProject,
        SaveProjectAs,
        ExportProject,
        UndoEdit,
        RedoEdit,
        TogglePlayback,
        PreviousFrame,
        NextFrame,
        ImportAssets,
        InsertSelectedAsset,
        DeleteSelectedClip
    ]
);

#[derive(Clone, Copy)]
enum ClipDragKind {
    Move,
    TrimStart,
    TrimEnd,
}

struct ClipDrag {
    clip_id: String,
    kind: ClipDragKind,
    pointer_frame: i64,
    start_frame: i64,
    duration_frames: i64,
}

#[derive(Clone)]
struct AssetDrag {
    id: String,
    kind: AssetKind,
}

impl Render for AssetDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .w(px(180.0))
            .h(px(34.0))
            .px_3()
            .rounded_sm()
            .bg(rgb(0x343842))
            .border_1()
            .border_color(rgb(0xffa13b))
            .shadow_md()
            .text_xs()
            .text_color(rgb(0xf0f1f4))
            .child(format!(
                "{}  {}",
                self.kind.to_string().to_uppercase(),
                self.id
            ))
    }
}

struct AudioPreview {
    _device_sink: rodio::MixerDeviceSink,
    player: Player,
    source: SamplesBuffer,
}

struct PreviewRequest {
    generation: u64,
    scene: Scene,
    asset_root: PathBuf,
    /// The project's `react_entry` and the runtime that serves it, when one
    /// is configured. `None` means component clips have nothing to resolve
    /// against this frame.
    react: Option<ReactPreviewContext>,
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

struct CpuPreviewFrame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

#[derive(Clone)]
enum PreviewPresentation {
    Image(Arc<RenderImage>),
    #[cfg(target_os = "macos")]
    Surface(mikan_gpu_renderer::NativePreviewFrame),
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
            .name("mikan-media-probe".to_owned())
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
/// spawning the same `@mikan/react` Node.js runtime `mikan-exporter --react`
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
            .name("mikan-component-schema".to_owned())
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
            .name("mikan-preview".to_owned())
            .spawn(move || {
                let mut react_bridge: Option<ReactPreviewBridge> = None;
                while let Ok(first) = request_rx.recv() {
                    let request = take_latest(first, &request_rx);
                    renderer.set_asset_root(&request.asset_root);
                    let mut scene = request.scene;
                    let warnings = resolve_preview_components(
                        &mut scene,
                        &mut react_bridge,
                        request.react.as_ref(),
                    );
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

    // Respawn only when the entry (or runtime) changed; a remembered failure
    // keeps retrying cheap until then.
    if react_bridge
        .as_ref()
        .is_none_or(|state| state.context != *react)
    {
        match ReactBridge::spawn(&react.node, &react.cli_script, &react.entry) {
            Ok(bridge) => {
                *react_bridge = Some(ReactPreviewBridge {
                    context: react.clone(),
                    bridge: Some(bridge),
                    failure: None,
                });
            }
            Err(error) => {
                *react_bridge = Some(ReactPreviewBridge {
                    context: react.clone(),
                    bridge: None,
                    failure: Some(error.to_string()),
                });
            }
        }
    }

    let Some(state) = react_bridge else {
        return Vec::new();
    };
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
    cache_epoch: u64,
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
    project: Project,
    asset_root: PathBuf,
    output: PathBuf,
    cancellation: ExportCancellation,
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
            .name("mikan-export".to_owned())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    let exporter = Exporter::new(ExportOptions {
                        overwrite: true,
                        ..ExportOptions::default()
                    });
                    let result = exporter.export_project_cancellable(
                        &request.project,
                        &request.asset_root,
                        &request.output,
                        &request.cancellation,
                        |progress| {
                            let _ = event_tx.send(ExportEvent::Progress(progress));
                        },
                    );
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

/// A `string`- or `color`-typed schema field currently being edited through
/// `property_input`, one at a time (the same shape as track renaming's
/// single shared `TextInput`). The target says which value store the commit
/// goes into: the selected component clip's `props`, or the project-level
/// `properties` map declared by the entry's `defineProjectProperties()`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PropertyEditTarget {
    Project,
    Clip { clip_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyEdit {
    target: PropertyEditTarget,
    key: String,
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

    fn clear(&mut self) {
        self.buffers.clear();
        self.waveforms.clear();
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
            .name("mikan-audio-mix".to_owned())
            .spawn(move || {
                let mut decoder = CachedAudioDecoder::new();
                let mut cache_epoch = 0;
                while let Ok(first) = request_rx.recv() {
                    let request = take_latest(first, &request_rx);
                    if request.cache_epoch != cache_epoch {
                        decoder.clear();
                        cache_epoch = request.cache_epoch;
                    }
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
                                let path = resolve_asset_location(
                                    &clip.asset.location,
                                    &request.asset_root,
                                )?;
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
    fn from_buffer(buffer: AudioBuffer) -> Result<Self, Box<dyn Error>> {
        let channels = NonZeroU16::new(buffer.channels).ok_or("audio has zero channels")?;
        let sample_rate =
            NonZeroU32::new(buffer.sample_rate).ok_or("audio has a zero sample rate")?;
        let source = SamplesBuffer::new(channels, sample_rate, buffer.samples);
        let device_sink = DeviceSinkBuilder::open_default_sink()?;
        let player = Player::connect_new(device_sink.mixer());
        player.append(source.clone());
        player.pause();
        Ok(Self {
            _device_sink: device_sink,
            player,
            source,
        })
    }

    fn seek(&mut self, time: Time, playing: bool) -> Result<(), Box<dyn Error>> {
        self.player.stop();
        self.player = Player::connect_new(self._device_sink.mixer());
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
    document: EditorDocument,
    preview_worker: PreviewWorker,
    preview_generation: u64,
    preview_pending: bool,
    media_probe_worker: MediaProbeWorker,
    media_generation: u64,
    media_pending: bool,
    media_cache: HashMap<String, Result<MediaAssetInfo, String>>,
    audio_mix_worker: AudioMixWorker,
    audio_generation: u64,
    audio_cache_epoch: u64,
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
    clip_drag: Option<ClipDrag>,
    clip_drag_hover_track_id: Option<String>,
    clip_drag_target_track_id: Option<String>,
    master_volume_drag: Option<MasterVolumeDrag>,
    selected_clip_id: Option<String>,
    selected_asset_id: Option<String>,
    selected_track_id: Option<String>,
    renaming_track_id: Option<String>,
    track_name_input: Option<Entity<TextInput>>,
    editing_dialogue_clip_id: Option<String>,
    dialogue_text_input: Option<Entity<TextInput>>,
    component_schema_worker: ComponentSchemaWorker,
    component_schema_generation: u64,
    component_schema_pending: bool,
    component_schema_entry: Option<String>,
    component_schemas: BTreeMap<String, ComponentPropertySchema>,
    project_property_schema: Option<BTreeMap<String, ComponentPropertyField>>,
    component_schema_error: Option<SharedString>,
    editing_property: Option<PropertyEdit>,
    property_input: Option<Entity<TextInput>>,
    project_name: SharedString,
    dimensions: SharedString,
    frame_rate_label: SharedString,
    duration: SharedString,
    assets: Vec<AssetSummary>,
    tracks: Vec<TrackSummary>,
    preview: Option<PreviewPresentation>,
    preview_error: Option<SharedString>,
    preview_warnings: Vec<SharedString>,
    save_error: Option<SharedString>,
    edit_error: Option<SharedString>,
    audio_error: Option<SharedString>,
    export_worker: ExportWorker,
    export_progress: Option<ExportProgress>,
    export_path: Option<PathBuf>,
    export_cancellation: Option<ExportCancellation>,
    export_cancelling: bool,
    export_error: Option<SharedString>,
    export_message: Option<SharedString>,
    choosing_export_path: bool,
    gpu_name: SharedString,
    focus_handle: Option<FocusHandle>,
    master_volume_focus: Option<FocusHandle>,
    saving_as: bool,
    importing_assets: bool,
    asset_operation_active: bool,
    track_delete_prompt_active: bool,
    close_prompt_active: bool,
    force_close: bool,
}

impl EditorView {
    fn open(path: Option<&Path>) -> Result<Self, Box<dyn Error>> {
        let document = match path {
            Some(path) => EditorDocument::load(path)?,
            None => EditorDocument::from_json(EDITOR_DEMO_PROJECT, "examples")?,
        };
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

        let renderer = GpuRenderer::new(GpuRenderOptions::default())?
            .with_asset_root(document.asset_root())
            .with_video_decoder(FfmpegBackend::new());
        let gpu_name = renderer.adapter_info().name.clone().into();
        let preview_worker = PreviewWorker::spawn(renderer)?;
        let media_probe_worker = MediaProbeWorker::spawn()?;
        let audio_mix_worker = AudioMixWorker::spawn()?;
        let export_worker = ExportWorker::spawn()?;
        let component_schema_worker = ComponentSchemaWorker::spawn()?;
        let mut editor = Self {
            document,
            preview_worker,
            preview_generation: 0,
            preview_pending: false,
            media_probe_worker,
            media_generation: 0,
            media_pending: false,
            media_cache: HashMap::new(),
            audio_mix_worker,
            audio_generation: 0,
            audio_cache_epoch: 0,
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
            clip_drag: None,
            clip_drag_hover_track_id: None,
            clip_drag_target_track_id: None,
            master_volume_drag: None,
            selected_clip_id: None,
            selected_asset_id: None,
            selected_track_id: None,
            renaming_track_id: None,
            track_name_input: None,
            editing_dialogue_clip_id: None,
            dialogue_text_input: None,
            component_schema_worker,
            component_schema_generation: 0,
            component_schema_pending: false,
            component_schema_entry: None,
            component_schemas: BTreeMap::new(),
            project_property_schema: None,
            component_schema_error: None,
            editing_property: None,
            property_input: None,
            project_name,
            dimensions,
            frame_rate_label,
            duration,
            assets,
            tracks,
            preview: None,
            preview_error: None,
            preview_warnings: Vec::new(),
            save_error: None,
            edit_error: None,
            audio_error: None,
            export_worker,
            export_progress: None,
            export_path: None,
            export_cancellation: None,
            export_cancelling: false,
            export_error: None,
            export_message: None,
            choosing_export_path: false,
            gpu_name,
            focus_handle: None,
            master_volume_focus: None,
            saving_as: false,
            importing_assets: false,
            asset_operation_active: false,
            track_delete_prompt_active: false,
            close_prompt_active: false,
            force_close: false,
        };
        editor.refresh_preview();
        editor.refresh_media_cache();
        editor.refresh_audio_preview();
        editor.refresh_component_schemas();
        Ok(editor)
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
            self.edit_error = Some(error.into());
        }
    }

    /// Re-queries the project's `react_entry` for its registered components'
    /// property schemas. Called whenever the entry changes (on load, and
    /// after `set_react_entry`/`clear_react_entry`) — the result is cached
    /// by entry path (`component_schema_entry`) rather than re-fetched on
    /// every clip selection, since spawning Node is comparatively slow and
    /// the schemas cannot change without the entry file changing (Node
    /// isn't re-run to pick up entry edits made after this cache is filled;
    /// re-select the entry, or reload the project, to refresh it).
    fn refresh_component_schemas(&mut self) {
        self.component_schema_generation = self.component_schema_generation.wrapping_add(1);
        let generation = self.component_schema_generation;
        let Some(entry) = self.document.react_entry_absolute_path() else {
            self.component_schema_entry = None;
            self.component_schema_pending = false;
            self.component_schema_error = None;
            self.component_schemas.clear();
            self.project_property_schema = None;
            return;
        };
        self.component_schema_entry = self.document.react_entry().map(str::to_owned);
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
                    if result.generation != self.preview_generation {
                        continue;
                    }
                    self.preview_pending = false;
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
                        self.edit_error = Some("media probe worker stopped unexpectedly".into());
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
            match self.audio_mix_worker.results.try_recv() {
                Ok(result) => {
                    if result.generation != self.audio_generation {
                        continue;
                    }
                    self.audio_pending = false;
                    match result.output.and_then(|output| {
                        self.clip_waveforms = output.clip_waveforms;
                        self.clip_levels = output.clip_levels;
                        AudioPreview::from_buffer(output.buffer).map_err(|error| error.to_string())
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

    fn frame_for_timeline_position(&self, position: Point<Pixels>, window: &Window) -> i64 {
        const TRACK_LABEL_WIDTH: f32 = 230.0;
        const RIGHT_INSET: f32 = 8.0;
        let window_width = f32::from(window.bounds().size.width);
        let timeline_width = (window_width - TRACK_LABEL_WIDTH - RIGHT_INSET).max(1.0);
        let local_x = (f32::from(position.x) - TRACK_LABEL_WIDTH).clamp(0.0, timeline_width);
        let progress = local_x / timeline_width;
        self.clock.frame_at_fraction(progress)
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

    fn begin_clip_drag(
        &mut self,
        clip_id: &str,
        kind: ClipDragKind,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source_track) = self
            .tracks
            .iter()
            .find(|track| track.clips.iter().any(|clip| clip.id == clip_id))
        else {
            return;
        };
        if source_track.locked {
            self.edit_error = Some(format!("track `{}` is locked", source_track.id).into());
            cx.notify();
            return;
        }
        let source_track_id = source_track.id.clone();
        let Some((start, duration)) = self
            .tracks
            .iter()
            .flat_map(|track| &track.clips)
            .find(|clip| clip.id == clip_id)
            .and_then(|clip| {
                Some((
                    self.clock.frame_for_time(clip.start).ok()?,
                    self.clock.frame_for_time(clip.duration).ok()?.max(1),
                ))
            })
        else {
            return;
        };
        let pointer_frame = self.frame_for_timeline_position(event.position, window);
        self.pause();
        self.scrubbing = false;
        self.document.begin_history_group();
        self.selected_clip_id = Some(clip_id.to_owned());
        self.clip_drag_target_track_id =
            matches!(kind, ClipDragKind::Move).then_some(source_track_id.clone());
        self.clip_drag_hover_track_id =
            matches!(kind, ClipDragKind::Move).then_some(source_track_id);
        self.clip_drag = Some(ClipDrag {
            clip_id: clip_id.to_owned(),
            kind,
            pointer_frame,
            start_frame: start,
            duration_frames: duration,
        });
        cx.notify();
    }

    fn update_clip_drag_target(
        &mut self,
        track_id: &str,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging()
            || !self
                .clip_drag
                .as_ref()
                .is_some_and(|drag| matches!(drag.kind, ClipDragKind::Move))
        {
            return;
        }
        let clip_kind = self.clip_drag.as_ref().and_then(|drag| {
            self.tracks
                .iter()
                .flat_map(|track| &track.clips)
                .find(|clip| clip.id == drag.clip_id)
                .map(|clip| clip.kind)
        });
        let target = self.tracks.iter().find(|track| track.id == track_id);
        let next = target
            .filter(|track| {
                !track.locked && clip_kind.is_some_and(|kind| track_accepts_clip(track.kind, kind))
            })
            .map(|track| track.id.clone());
        if self.clip_drag_target_track_id != next
            || self.clip_drag_hover_track_id.as_deref() != Some(track_id)
        {
            self.clip_drag_target_track_id = next;
            self.clip_drag_hover_track_id = Some(track_id.to_owned());
            cx.notify();
        }
    }

    fn continue_clip_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            return;
        }
        let Some(drag) = self.clip_drag.as_ref() else {
            return;
        };
        let pointer_frame = self.frame_for_timeline_position(event.position, window);
        let (start, duration) = dragged_clip_range(drag, pointer_frame, self.clock.end_frame());
        let clip_id = drag.clip_id.clone();
        let unchanged = self
            .tracks
            .iter()
            .flat_map(|track| &track.clips)
            .find(|clip| clip.id == clip_id)
            .and_then(|clip| {
                Some((
                    self.clock.frame_for_time(clip.start).ok()?,
                    self.clock.frame_for_time(clip.duration).ok()?,
                ))
            })
            == Some((start, duration));
        if unchanged {
            return;
        }
        if let Err(error) = self.document.edit_clip_frames(&clip_id, start, duration) {
            self.preview_error = Some(error.to_string().into());
            cx.notify();
            return;
        }
        self.tracks = self.document.tracks();
        self.refresh_preview();
        cx.notify();
    }

    fn end_clip_drag(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let drag = self.clip_drag.take();
        self.clip_drag_hover_track_id = None;
        let target = self.clip_drag_target_track_id.take();
        if let Some(drag) = drag {
            if matches!(drag.kind, ClipDragKind::Move)
                && let Some(target) = target
                && let Err(error) = self.document.move_clip_to_track(&drag.clip_id, &target)
            {
                self.edit_error = Some(error.to_string().into());
            }
            if self.document.commit_history_group() {
                self.tracks = self.document.tracks();
                self.refresh_preview();
                self.refresh_audio_preview();
            }
            cx.notify();
        }
    }

    fn sync_document_state(&mut self) {
        let frame = self.clock.frame();
        let assets = self.document.assets();
        let assets_changed = assets != self.assets;
        self.assets = assets;
        self.tracks = self.document.tracks();
        self.duration = format_time(self.document.duration()).into();
        if let Ok(mut clock) = TimelineClock::new(self.document.duration(), self.frame_rate_value) {
            clock.seek(frame);
            self.clock = clock;
        }
        self.refresh_preview();
        if assets_changed {
            self.audio_cache_epoch = self.audio_cache_epoch.wrapping_add(1);
            self.refresh_media_cache();
        }
        self.refresh_audio_preview();
    }

    fn refresh_audio_preview(&mut self) {
        self.audio_generation = self.audio_generation.wrapping_add(1);
        let generation = self.audio_generation;
        self.audio_mix_worker.cancel_before(generation);
        self.audio_pending = false;
        self.audio_preview = None;
        self.clip_levels.clear();
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
                    cache_epoch: self.audio_cache_epoch,
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

    fn add_track(&mut self, kind: TrackKind, cx: &mut Context<Self>) {
        let track_id = self.document.add_track(kind);
        self.selected_track_id = Some(track_id);
        self.edit_error = None;
        self.tracks = self.document.tracks();
        cx.notify();
    }

    fn move_track(&mut self, track_id: &str, offset: isize, cx: &mut Context<Self>) {
        match self.document.move_track(track_id, offset) {
            Ok(true) => {
                self.tracks = self.document.tracks();
                self.refresh_preview();
                self.refresh_audio_preview();
                self.edit_error = None;
            }
            Ok(false) => {}
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn begin_track_rename(&mut self, track_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(track) = self.tracks.iter().find(|track| track.id == track_id) else {
            return;
        };
        if track.locked {
            self.edit_error = Some(format!("track `{track_id}` is locked").into());
            cx.notify();
            return;
        }
        self.renaming_track_id = Some(track_id.to_owned());
        self.edit_error = None;
        if let Some(input) = &self.track_name_input {
            input.update(cx, |input, cx| input.set_text(track.name.clone(), cx));
            input.read(cx).focus(window);
        }
        cx.notify();
    }

    fn commit_track_rename(&mut self, cx: &mut Context<Self>) {
        let Some(track_id) = self.renaming_track_id.clone() else {
            return;
        };
        let Some(name) = self
            .track_name_input
            .as_ref()
            .map(|input| input.read(cx).text())
        else {
            return;
        };
        match self.document.rename_track(&track_id, &name) {
            Ok(()) => {
                self.renaming_track_id = None;
                self.tracks = self.document.tracks();
                self.edit_error = None;
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn cancel_track_rename(&mut self, cx: &mut Context<Self>) {
        self.renaming_track_id = None;
        self.edit_error = None;
        cx.notify();
    }

    fn begin_dialogue_text_edit(
        &mut self,
        clip_id: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editing_dialogue_clip_id = Some(clip_id.to_owned());
        self.edit_error = None;
        if let Some(input) = &self.dialogue_text_input {
            input.update(cx, |input, cx| input.set_text(text.to_owned(), cx));
            input.read(cx).focus(window);
        }
        cx.notify();
    }

    fn commit_dialogue_text_edit(&mut self, cx: &mut Context<Self>) {
        let Some(clip_id) = self.editing_dialogue_clip_id.clone() else {
            return;
        };
        let Some(text) = self
            .dialogue_text_input
            .as_ref()
            .map(|input| input.read(cx).text())
        else {
            return;
        };
        match self.document.set_dialogue_text(&clip_id, &text) {
            Ok(()) => {
                self.editing_dialogue_clip_id = None;
                self.tracks = self.document.tracks();
                self.refresh_preview();
                self.edit_error = None;
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn cancel_dialogue_text_edit(&mut self, cx: &mut Context<Self>) {
        self.editing_dialogue_clip_id = None;
        self.edit_error = None;
        cx.notify();
    }

    fn set_dialogue_character(
        &mut self,
        clip_id: &str,
        character_id: &str,
        cx: &mut Context<Self>,
    ) {
        match self.document.set_dialogue_character(clip_id, character_id) {
            Ok(()) => {
                self.tracks = self.document.tracks();
                self.refresh_preview();
                self.edit_error = None;
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn apply_property(
        &mut self,
        target: &PropertyEditTarget,
        key: &str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let result = match target {
            PropertyEditTarget::Project => {
                self.document.set_project_property(key, value);
                Ok(())
            }
            PropertyEditTarget::Clip { clip_id } => {
                self.document.set_component_prop(clip_id, key, value)
            }
        };
        match result {
            Ok(()) => {
                if matches!(target, PropertyEditTarget::Clip { .. }) {
                    self.tracks = self.document.tracks();
                }
                self.edit_error = None;
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn toggle_property_boolean(
        &mut self,
        target: &PropertyEditTarget,
        key: &str,
        current: bool,
        cx: &mut Context<Self>,
    ) {
        self.apply_property(target, key, serde_json::Value::Bool(!current), cx);
    }

    fn step_property_number(
        &mut self,
        target: &PropertyEditTarget,
        key: &str,
        current: f64,
        delta: f64,
        bounds: (Option<f64>, Option<f64>),
        cx: &mut Context<Self>,
    ) {
        let (min, max) = bounds;
        let mut next = current + delta;
        if let Some(min) = min {
            next = next.max(min);
        }
        if let Some(max) = max {
            next = next.min(max);
        }
        let Some(value) = serde_json::Number::from_f64(next).map(serde_json::Value::Number) else {
            return;
        };
        self.apply_property(target, key, value, cx);
    }

    fn cycle_property_select(
        &mut self,
        target: &PropertyEditTarget,
        key: &str,
        options: &[String],
        current: &str,
        cx: &mut Context<Self>,
    ) {
        if options.is_empty() {
            return;
        }
        let next_index = options
            .iter()
            .position(|option| option == current)
            .map_or(0, |index| (index + 1) % options.len());
        self.apply_property(
            target,
            key,
            serde_json::Value::String(options[next_index].clone()),
            cx,
        );
    }

    fn begin_property_edit(
        &mut self,
        target: &PropertyEditTarget,
        key: &str,
        current: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editing_property = Some(PropertyEdit {
            target: target.clone(),
            key: key.to_owned(),
        });
        self.edit_error = None;
        if let Some(input) = &self.property_input {
            input.update(cx, |input, cx| input.set_text(current.to_owned(), cx));
            input.read(cx).focus(window);
        }
        cx.notify();
    }

    fn commit_property_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.editing_property.clone() else {
            return;
        };
        let Some(text) = self
            .property_input
            .as_ref()
            .map(|input| input.read(cx).text())
        else {
            return;
        };
        self.editing_property = None;
        self.apply_property(&edit.target, &edit.key, serde_json::Value::String(text), cx);
    }

    fn cancel_property_edit(&mut self, cx: &mut Context<Self>) {
        self.editing_property = None;
        self.edit_error = None;
        cx.notify();
    }

    fn toggle_track_enabled(&mut self, track_id: &str, cx: &mut Context<Self>) {
        match self.document.toggle_track_enabled(track_id) {
            Ok(_) => {
                self.tracks = self.document.tracks();
                self.refresh_preview();
                self.refresh_audio_preview();
                self.edit_error = None;
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn toggle_track_locked(&mut self, track_id: &str, cx: &mut Context<Self>) {
        match self.document.toggle_track_locked(track_id) {
            Ok(_) => {
                self.renaming_track_id = None;
                self.tracks = self.document.tracks();
                self.edit_error = None;
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn delete_track_now(&mut self, track_id: &str, cx: &mut Context<Self>) {
        self.pause();
        match self.document.delete_track(track_id, true) {
            Ok(()) => {
                if self.selected_track_id.as_deref() == Some(track_id) {
                    self.selected_track_id = None;
                }
                self.renaming_track_id = None;
                self.sync_document_state();
                if self.selected_clip_id.as_ref().is_some_and(|selected| {
                    !self
                        .tracks
                        .iter()
                        .flat_map(|track| &track.clips)
                        .any(|clip| &clip.id == selected)
                }) {
                    self.selected_clip_id = None;
                }
                self.edit_error = None;
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn request_delete_track(
        &mut self,
        track_id: &str,
        item_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.track_delete_prompt_active {
            return;
        }
        if item_count == 0 {
            self.delete_track_now(track_id, cx);
            return;
        }
        self.track_delete_prompt_active = true;
        let answer = window.prompt(
            PromptLevel::Warning,
            "Delete non-empty track?",
            Some(&format!(
                "Track `{track_id}` contains {item_count} clip(s). Deleting the track also deletes all of them."
            )),
            &[PromptButton::ok("Delete Track"), PromptButton::cancel("Cancel")],
            cx,
        );
        let track_id = track_id.to_owned();
        cx.spawn_in(window, async move |view, cx| {
            let answer = answer.await.unwrap_or(1);
            view.update_in(cx, |this, _, cx| {
                this.track_delete_prompt_active = false;
                if answer == 0 {
                    this.delete_track_now(&track_id, cx);
                } else {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn selected_audio_clip(&self) -> Option<mikan_editor::ClipSummary> {
        let selected = self.selected_clip_id.as_deref()?;
        self.tracks
            .iter()
            .flat_map(|track| &track.clips)
            .find(|clip| clip.id == selected && clip.volume.is_some())
            .cloned()
    }

    fn apply_selected_clip_volume(
        &mut self,
        volume: f64,
        force_keyframe: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(clip) = self.selected_audio_clip() else {
            return;
        };
        let local_time = clip_local_time(self.current_time(), &clip);
        let animated = matches!(clip.volume, Some(Animatable::Keyframes(_)));
        let result = if force_keyframe || animated {
            self.document
                .set_clip_volume_keyframe(&clip.id, local_time, volume.clamp(0.0, 2.0))
        } else {
            self.document
                .set_clip_volume_static(&clip.id, volume.clamp(0.0, 2.0))
        };
        match result {
            Ok(()) => {
                self.tracks = self.document.tracks();
                self.refresh_audio_preview();
                self.edit_error = None;
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn adjust_selected_clip_volume(&mut self, delta_percent: i32, cx: &mut Context<Self>) {
        let Some(clip) = self.selected_audio_clip() else {
            return;
        };
        let local_time = clip_local_time(self.current_time(), &clip);
        let current = clip
            .volume
            .as_ref()
            .and_then(|volume| evaluate_f64(volume, local_time).ok())
            .unwrap_or(1.0);
        let percent = (current * 100.0).round() as i32;
        let next = percent.saturating_add(delta_percent).clamp(0, 200);
        self.apply_selected_clip_volume(f64::from(next) / 100.0, false, cx);
    }

    fn decrease_selected_clip_volume(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.adjust_selected_clip_volume(-5, cx);
    }

    fn increase_selected_clip_volume(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.adjust_selected_clip_volume(5, cx);
    }

    fn set_selected_clip_volume_keyframe(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(clip) = self.selected_audio_clip() else {
            return;
        };
        let local_time = clip_local_time(self.current_time(), &clip);
        let volume = clip
            .volume
            .as_ref()
            .and_then(|volume| evaluate_f64(volume, local_time).ok())
            .unwrap_or(1.0);
        self.apply_selected_clip_volume(volume, true, cx);
    }

    fn remove_selected_clip_volume_keyframe(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(clip) = self.selected_audio_clip() else {
            return;
        };
        let local_time = clip_local_time(self.current_time(), &clip);
        match self
            .document
            .remove_clip_volume_keyframe(&clip.id, local_time)
        {
            Ok(true) => {
                self.tracks = self.document.tracks();
                self.refresh_audio_preview();
                self.edit_error = None;
            }
            Ok(false) => {}
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn flatten_selected_clip_volume(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(clip) = self.selected_audio_clip() else {
            return;
        };
        let local_time = clip_local_time(self.current_time(), &clip);
        match self.document.flatten_clip_volume(&clip.id, local_time) {
            Ok(()) => {
                self.tracks = self.document.tracks();
                self.refresh_audio_preview();
                self.edit_error = None;
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn adjust_master_volume(&mut self, delta_percent: i32, cx: &mut Context<Self>) {
        let current_percent = (self.document.master_volume() * 100.0).round() as i32;
        let next_percent = current_percent.saturating_add(delta_percent).clamp(0, 200);
        self.set_master_volume(f64::from(next_percent) / 100.0, cx);
    }

    fn set_master_volume(&mut self, volume: f64, cx: &mut Context<Self>) {
        match self.document.set_master_volume(volume.clamp(0.0, 2.0)) {
            Ok(()) => self.refresh_audio_preview(),
            Err(error) => self.audio_error = Some(error.to_string().into()),
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
            focus.focus(window);
        }
        self.document.begin_history_group();
        self.master_volume_drag = Some(MasterVolumeDrag {
            pointer_x: event.position.x,
            start_volume: self.document.master_volume(),
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
        if (volume - self.document.master_volume()).abs() >= f64::EPSILON {
            self.set_master_volume(volume, cx);
        }
    }

    fn end_master_volume_drag(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.master_volume_drag.take().is_some() {
            self.document.commit_history_group();
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

    fn request_import_assets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.importing_assets {
            return;
        }
        self.importing_assets = true;
        self.edit_error = None;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Import".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let selected_paths = match selection.await {
                Ok(Ok(paths)) => paths,
                Ok(Err(error)) => {
                    view.update_in(cx, |this, _, cx| {
                        this.importing_assets = false;
                        this.edit_error =
                            Some(format!("could not open asset picker: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    view.update_in(cx, |this, _, cx| {
                        this.importing_assets = false;
                        this.edit_error =
                            Some(format!("asset picker was interrupted: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            view.update_in(cx, |this, _, cx| {
                this.importing_assets = false;
                let Some(paths) = selected_paths else {
                    cx.notify();
                    return;
                };
                match this.document.import_assets(paths) {
                    Ok(imported) => {
                        this.selected_asset_id = imported.last().map(|asset| asset.id.clone());
                        this.assets = this.document.assets();
                        this.audio_cache_epoch = this.audio_cache_epoch.wrapping_add(1);
                        this.refresh_media_cache();
                        this.edit_error = None;
                    }
                    Err(error) => this.edit_error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn import_assets_action(
        &mut self,
        _: &ImportAssets,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_import_assets(window, cx);
    }

    fn import_assets_click(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.request_import_assets(window, cx);
    }

    fn request_relink_asset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(asset_id) = self.selected_asset_id.clone() else {
            return;
        };
        if self.asset_operation_active {
            return;
        }
        self.asset_operation_active = true;
        self.edit_error = None;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Relink".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let selected_paths = match selection.await {
                Ok(Ok(paths)) => paths,
                Ok(Err(error)) => {
                    view.update_in(cx, |this, _, cx| {
                        this.asset_operation_active = false;
                        this.edit_error =
                            Some(format!("could not open relink picker: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    view.update_in(cx, |this, _, cx| {
                        this.asset_operation_active = false;
                        this.edit_error =
                            Some(format!("relink picker was interrupted: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            view.update_in(cx, |this, _, cx| {
                this.asset_operation_active = false;
                let Some(path) = selected_paths.and_then(|paths| paths.into_iter().next()) else {
                    cx.notify();
                    return;
                };
                match this.document.relink_asset(&asset_id, path) {
                    Ok(()) => {
                        this.edit_error = None;
                        this.sync_document_state();
                    }
                    Err(error) => this.edit_error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn relink_asset_click(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.request_relink_asset(window, cx);
    }

    fn request_set_react_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.asset_operation_active {
            return;
        }
        self.asset_operation_active = true;
        self.edit_error = None;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Set React Entry".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let selected_paths = match selection.await {
                Ok(Ok(paths)) => paths,
                Ok(Err(error)) => {
                    view.update_in(cx, |this, _, cx| {
                        this.asset_operation_active = false;
                        this.edit_error =
                            Some(format!("could not open React entry picker: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    view.update_in(cx, |this, _, cx| {
                        this.asset_operation_active = false;
                        this.edit_error =
                            Some(format!("React entry picker was interrupted: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            view.update_in(cx, |this, _, cx| {
                this.asset_operation_active = false;
                let Some(path) = selected_paths.and_then(|paths| paths.into_iter().next()) else {
                    cx.notify();
                    return;
                };
                match this.document.set_react_entry(path) {
                    Ok(()) => {
                        this.edit_error = None;
                        this.refresh_component_schemas();
                    }
                    Err(error) => this.edit_error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn set_react_entry_click(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_set_react_entry(window, cx);
    }

    fn clear_react_entry_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.document.clear_react_entry();
        self.refresh_component_schemas();
        cx.notify();
    }

    fn remove_selected_asset_now(
        &mut self,
        asset_id: &str,
        remove_references: bool,
        cx: &mut Context<Self>,
    ) {
        match self.document.remove_asset(asset_id, remove_references) {
            Ok(()) => {
                self.selected_asset_id = None;
                self.selected_clip_id = None;
                self.edit_error = None;
                self.sync_document_state();
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn request_remove_asset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(asset_id) = self.selected_asset_id.clone() else {
            return;
        };
        if self.asset_operation_active {
            return;
        }
        let references = match self.document.asset_references(&asset_id) {
            Ok(references) => references,
            Err(error) => {
                self.edit_error = Some(error.to_string().into());
                cx.notify();
                return;
            }
        };
        if references.is_empty() {
            self.remove_selected_asset_now(&asset_id, false, cx);
            return;
        }
        self.asset_operation_active = true;
        let mut detail = format!(
            "This asset has {} reference(s). Removing it will also update or remove:",
            references.len()
        );
        for reference in references.iter().take(4) {
            detail.push_str(&format!("\n\n• {reference}"));
        }
        if references.len() > 4 {
            detail.push_str(&format!("\n\n• …and {} more", references.len() - 4));
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "Remove referenced asset?",
            Some(&detail),
            &[
                PromptButton::ok("Remove Asset and References"),
                PromptButton::cancel("Cancel"),
            ],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            let answer = answer.await.unwrap_or(1);
            view.update_in(cx, |this, _, cx| {
                this.asset_operation_active = false;
                if answer == 0 {
                    this.remove_selected_asset_now(&asset_id, true, cx);
                } else {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn remove_asset_click(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.request_remove_asset(window, cx);
    }

    fn insert_asset_at(
        &mut self,
        asset_id: &str,
        target_track_id: Option<&str>,
        requested_start: i64,
        cx: &mut Context<Self>,
    ) {
        let frame_rate = self.document.project().settings.frame_rate;
        let mut start = requested_start;
        let mut duration = initial_clip_duration_frames(self.media_cache.get(asset_id), frame_rate);
        if self.document.project().settings.duration.is_some() {
            if self.clock.end_frame() == 0 {
                self.edit_error = Some("the fixed project timeline has no available frames".into());
                cx.notify();
                return;
            }
            start = start.min(self.clock.end_frame() - 1);
            duration = duration.min(self.clock.end_frame() - start);
        }
        match self
            .document
            .insert_asset_clip_on_track(asset_id, target_track_id, start, duration)
        {
            Ok(clip_id) => {
                self.selected_clip_id = Some(clip_id);
                self.edit_error = None;
                self.sync_document_state();
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn insert_selected_asset(&mut self, cx: &mut Context<Self>) {
        let Some(asset_id) = self.selected_asset_id.clone() else {
            self.edit_error = Some("select an asset before adding it to the timeline".into());
            cx.notify();
            return;
        };
        let target_track_id = self.selected_track_id.clone();
        self.insert_asset_at(
            &asset_id,
            target_track_id.as_deref(),
            self.clock.frame(),
            cx,
        );
    }

    fn insert_selected_asset_as_dialogue(&mut self, cx: &mut Context<Self>) {
        let Some(asset_id) = self.selected_asset_id.clone() else {
            return;
        };
        let Some(character) = self.document.characters().into_iter().next() else {
            self.edit_error =
                Some("define at least one project character before adding dialogue".into());
            cx.notify();
            return;
        };
        let frame_rate = self.document.project().settings.frame_rate;
        let mut start = self.clock.frame();
        let mut duration =
            initial_clip_duration_frames(self.media_cache.get(&asset_id), frame_rate);
        if self.document.project().settings.duration.is_some() {
            if self.clock.end_frame() == 0 {
                self.edit_error = Some("the fixed project timeline has no available frames".into());
                cx.notify();
                return;
            }
            start = start.min(self.clock.end_frame() - 1);
            duration = duration.min(self.clock.end_frame() - start);
        }
        let target_track_id = self.selected_track_id.clone();
        match self.document.insert_dialogue_clip(
            &asset_id,
            &character.id,
            target_track_id.as_deref(),
            start,
            duration,
        ) {
            Ok(clip_id) => {
                self.selected_clip_id = Some(clip_id);
                self.edit_error = None;
                self.sync_document_state();
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn drop_asset_on_track(
        &mut self,
        asset: &AssetDrag,
        track_id: &str,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let start = self.frame_for_timeline_position(window.mouse_position(), window);
        self.selected_asset_id = Some(asset.id.clone());
        self.selected_track_id = Some(track_id.to_owned());
        self.insert_asset_at(&asset.id, Some(track_id), start, cx);
    }

    fn drop_asset_without_track(
        &mut self,
        asset: &AssetDrag,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let start = self.frame_for_timeline_position(window.mouse_position(), window);
        self.selected_asset_id = Some(asset.id.clone());
        self.selected_track_id = None;
        self.insert_asset_at(&asset.id, None, start, cx);
    }

    fn insert_selected_asset_action(
        &mut self,
        _: &InsertSelectedAsset,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.insert_selected_asset(cx);
    }

    fn insert_selected_asset_click(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.insert_selected_asset(cx);
    }

    fn insert_selected_dialogue_click(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.insert_selected_asset_as_dialogue(cx);
    }

    fn delete_selected_clip(&mut self, cx: &mut Context<Self>) {
        let Some(clip_id) = self.selected_clip_id.clone() else {
            return;
        };
        self.pause();
        match self.document.delete_clip(&clip_id) {
            Ok(()) => {
                self.selected_clip_id = None;
                self.edit_error = None;
                self.sync_document_state();
            }
            Err(error) => self.edit_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn delete_selected_clip_action(
        &mut self,
        _: &DeleteSelectedClip,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_selected_clip(cx);
    }

    fn delete_selected_clip_click(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_selected_clip(cx);
    }

    fn save_project(&mut self, _: &SaveProject, window: &mut Window, cx: &mut Context<Self>) {
        if self.document.path().is_none() {
            self.request_save_as(window, cx, false);
            return;
        }
        match self.document.save() {
            Ok(()) => self.save_error = None,
            Err(error) => self.save_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn save_project_as(&mut self, _: &SaveProjectAs, window: &mut Window, cx: &mut Context<Self>) {
        self.request_save_as(window, cx, false);
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
        let request = ExportRequest {
            project: self.document.project().clone(),
            asset_root: self.document.asset_root().to_owned(),
            output: output.clone(),
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

    fn cancel_export_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(cancellation) = &self.export_cancellation {
            cancellation.cancel();
            self.export_cancelling = true;
            cx.notify();
        }
    }

    fn request_save_as(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        close_after_save: bool,
    ) {
        if self.saving_as {
            return;
        }
        self.saving_as = true;
        self.save_error = None;
        let directory = self.document.path().and_then(Path::parent).map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            Path::to_path_buf,
        );
        let suggested_name = self
            .document
            .path()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled.mikan.json")
            .to_owned();
        let selection = cx.prompt_for_new_path(&directory, Some(&suggested_name));
        cx.spawn_in(window, async move |view, cx| {
            let selected_path = match selection.await {
                Ok(Ok(path)) => path,
                Ok(Err(error)) => {
                    view.update_in(cx, |this, _, cx| {
                        this.saving_as = false;
                        this.close_prompt_active = false;
                        this.save_error = Some(format!("could not open Save As: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    view.update_in(cx, |this, _, cx| {
                        this.saving_as = false;
                        this.close_prompt_active = false;
                        this.save_error = Some(format!("Save As was interrupted: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            view.update_in(cx, |this, window, cx| {
                this.saving_as = false;
                let Some(path) = selected_path else {
                    this.close_prompt_active = false;
                    cx.notify();
                    return;
                };
                match this.document.save_as(path) {
                    Ok(()) => {
                        this.save_error = None;
                        this.project_name = this.document.display_name().into();
                        this.assets = this.document.assets();
                        this.audio_cache_epoch = this.audio_cache_epoch.wrapping_add(1);
                        this.refresh_preview();
                        this.refresh_media_cache();
                        this.refresh_audio_preview();
                        if close_after_save {
                            this.force_close = true;
                            window.remove_window();
                        } else {
                            this.close_prompt_active = false;
                        }
                    }
                    Err(error) => {
                        this.close_prompt_active = false;
                        this.save_error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn prompt_to_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_prompt_active {
            return;
        }
        self.close_prompt_active = true;
        let answer = window.prompt(
            PromptLevel::Warning,
            "Save changes before closing?",
            Some("Unsaved changes will be lost if you choose Don't Save."),
            &[
                PromptButton::ok("Save"),
                PromptButton::new("Don't Save"),
                PromptButton::cancel("Cancel"),
            ],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            let answer = answer.await.unwrap_or(2);
            view.update_in(cx, move |this, window, cx| match answer {
                0 if this.document.path().is_some() => match this.document.save() {
                    Ok(()) => {
                        this.save_error = None;
                        this.force_close = true;
                        window.remove_window();
                    }
                    Err(error) => {
                        this.close_prompt_active = false;
                        this.save_error = Some(error.to_string().into());
                        cx.notify();
                    }
                },
                0 => this.request_save_as(window, cx, true),
                1 => {
                    this.force_close = true;
                    window.remove_window();
                }
                _ => {
                    this.close_prompt_active = false;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn undo_edit(&mut self, _: &UndoEdit, _: &mut Window, cx: &mut Context<Self>) {
        self.pause();
        self.clip_drag = None;
        self.clip_drag_hover_track_id = None;
        self.clip_drag_target_track_id = None;
        self.renaming_track_id = None;
        match self.document.undo() {
            Ok(true) => {
                self.save_error = None;
                self.sync_document_state();
            }
            Ok(false) => {}
            Err(error) => self.save_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn redo_edit(&mut self, _: &RedoEdit, _: &mut Window, cx: &mut Context<Self>) {
        self.pause();
        self.clip_drag = None;
        self.clip_drag_hover_track_id = None;
        self.clip_drag_target_track_id = None;
        self.renaming_track_id = None;
        match self.document.redo() {
            Ok(true) => {
                self.save_error = None;
                self.sync_document_state();
            }
            Ok(false) => {}
            Err(error) => self.save_error = Some(error.to_string().into()),
        }
        cx.notify();
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

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let master_volume = self.document.master_volume().clamp(0.0, 2.0);
        let exporting = self.export_cancellation.is_some();
        let export_label = self.export_progress.map(export_progress_label);
        div()
            .id("toolbar")
            .flex()
            .flex_none()
            .h(px(48.0))
            .w_full()
            .px_4()
            .items_center()
            .justify_between()
            .bg(rgb(0x181a20))
            .border_b_1()
            .border_color(rgb(0x30333d))
            .on_mouse_move(cx.listener(Self::continue_master_volume_drag))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::end_master_volume_drag))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().text_lg().text_color(rgb(0xffa13b)).child("Mikan"))
                    .child(div().text_sm().text_color(rgb(0xd8dae2)).child(
                        if self.document.is_dirty() {
                            format!("{} *", self.project_name)
                        } else {
                            self.project_name.to_string()
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .when_some(self.save_error.clone(), |toolbar, error| {
                        toolbar.child(
                            div()
                                .max_w(px(520.0))
                                .overflow_hidden()
                                .text_sm()
                                .text_color(rgb(0xff8b8b))
                                .child(format!("Save failed: {error}")),
                        )
                    })
                    .when_some(self.edit_error.clone(), |toolbar, error| {
                        toolbar.child(
                            div()
                                .max_w(px(520.0))
                                .overflow_hidden()
                                .text_sm()
                                .text_color(rgb(0xff8b8b))
                                .child(format!("Edit failed: {error}")),
                        )
                    })
                    .when_some(self.audio_error.clone(), |toolbar, error| {
                        toolbar.child(
                            div()
                                .max_w(px(520.0))
                                .overflow_hidden()
                                .text_sm()
                                .text_color(rgb(0xffc46b))
                                .child(format!("Audio unavailable: {error}")),
                        )
                    })
                    .when_some(self.export_error.clone(), |toolbar, error| {
                        toolbar.child(
                            div()
                                .max_w(px(520.0))
                                .overflow_hidden()
                                .text_sm()
                                .text_color(rgb(0xff8b8b))
                                .child(format!("Export failed: {error}")),
                        )
                    })
                    .when_some(self.export_message.clone(), |toolbar, message| {
                        toolbar.child(
                            div()
                                .max_w(px(360.0))
                                .overflow_hidden()
                                .text_sm()
                                .text_color(rgb(0x7ee2a8))
                                .child(message),
                        )
                    })
                    .when_some(export_label, |toolbar, label| {
                        toolbar.child(div().text_xs().text_color(rgb(0xffc46b)).child(
                            if self.export_cancelling {
                                "Cancelling export…".to_owned()
                            } else {
                                label
                            },
                        ))
                    })
                    .child(
                        div()
                            .id("export-project")
                            .rounded_sm()
                            .px_2()
                            .py_1()
                            .bg(rgb(if exporting { 0x4a292c } else { 0x343842 }))
                            .text_xs()
                            .text_color(rgb(0xd8dae2))
                            .child(if self.choosing_export_path {
                                "Choosing…"
                            } else if self.export_cancelling {
                                "Cancelling…"
                            } else if exporting {
                                "Cancel Export"
                            } else {
                                "Export…"
                            })
                            .when(
                                !self.choosing_export_path && !self.export_cancelling,
                                |button| {
                                    if exporting {
                                        button
                                            .cursor_pointer()
                                            .hover(|style| style.bg(rgb(0x6b363b)))
                                            .on_click(cx.listener(Self::cancel_export_click))
                                    } else {
                                        button
                                            .cursor_pointer()
                                            .hover(|style| style.bg(rgb(0x4a4f5b)))
                                            .on_click(cx.listener(Self::export_project_click))
                                    }
                                },
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_xs()
                            .text_color(rgb(0xb8bbc5))
                            .child("Master")
                            .child(
                                div()
                                    .id("master-volume-slider")
                                    .relative()
                                    .w(px(88.0))
                                    .h(px(20.0))
                                    .cursor_pointer()
                                    .rounded_sm()
                                    .bg(rgb(0x292c34))
                                    .when_some(
                                        self.master_volume_focus.as_ref(),
                                        |slider, focus| slider.track_focus(focus),
                                    )
                                    .focus(|slider| slider.border_1().border_color(rgb(0xffb466)))
                                    .hover(|style| style.bg(rgb(0x404550)))
                                    .child(
                                        div()
                                            .absolute()
                                            .left(px(5.0))
                                            .right(px(5.0))
                                            .top(px(8.0))
                                            .h(px(4.0))
                                            .rounded_full()
                                            .overflow_hidden()
                                            .bg(rgb(0x15171c))
                                            .child(
                                                div()
                                                    .h_full()
                                                    .w(relative((master_volume / 2.0) as f32))
                                                    .rounded_full()
                                                    .bg(rgb(0x70d99a)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .absolute()
                                            .left(px((master_volume / 2.0 * 78.0) as f32))
                                            .top(px(5.0))
                                            .size(px(10.0))
                                            .rounded_full()
                                            .bg(rgb(0xf0f1f4)),
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
                                    .text_center()
                                    .child(format!("{}%", (master_volume * 100.0).round() as i32)),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(rgb(0x243b30))
                            .text_xs()
                            .text_color(rgb(0x7ee2a8))
                            .child("GPU PREVIEW"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x9da1ad))
                            .child(format_time(self.current_time())),
                    ),
            )
    }

    fn asset_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_selected = self.selected_asset_id.is_some();
        let can_insert_selected = self.selected_asset_id.as_deref().is_some_and(|selected| {
            self.assets
                .iter()
                .any(|asset| asset.id == selected && asset.kind != mikan_project::AssetKind::Font)
        });
        let can_insert_dialogue = self.selected_asset_id.as_deref().is_some_and(|selected| {
            self.assets
                .iter()
                .any(|asset| asset.id == selected && asset.kind == AssetKind::Audio)
        }) && !self.document.characters().is_empty()
            && self.selected_track_id.as_deref().is_none_or(|selected| {
                self.tracks.iter().any(|track| {
                    track.id == selected && track.kind == TrackKind::Dialogue && !track.locked
                })
            });
        let rows = self.assets.iter().map(|asset| {
            let asset_id = asset.id.clone();
            let drag = AssetDrag {
                id: asset.id.clone(),
                kind: asset.kind,
            };
            let selected = self.selected_asset_id.as_deref() == Some(asset.id.as_str());
            let element_id: SharedString = format!("asset-row-{}", asset.id).into();
            let media_detail = match (asset.missing, self.media_cache.get(&asset.id)) {
                (true, _) => Some("Missing file — Relink required".to_owned()),
                (false, Some(Ok(info))) => Some(format_media_asset_info(info)),
                (false, Some(Err(_))) => Some("Probe failed".to_owned()),
                (false, None) if matches!(asset.kind, AssetKind::Video | AssetKind::Audio) => {
                    Some("Probing…".to_owned())
                }
                (false, None) => None,
            };
            div()
                .id(element_id)
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .cursor_pointer()
                .when(selected, |row| row.bg(rgb(0x343842)))
                .hover(|style| style.bg(rgb(0x292c34)))
                .text_sm()
                .text_color(rgb(0xc8cad2))
                .child(
                    div()
                        .w(px(46.0))
                        .text_xs()
                        .text_color(rgb(0xffb466))
                        .child(asset.kind.to_string().to_uppercase()),
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
                                    .text_color(rgb(if asset.missing {
                                        0xff8b8b
                                    } else {
                                        0x858a96
                                    }))
                                    .child(detail),
                            )
                        }),
                )
                .on_drag(drag, |asset, _, _, cx| {
                    let asset = asset.clone();
                    cx.new(|_| asset)
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.selected_asset_id = Some(asset_id.clone());
                    this.edit_error = None;
                    cx.notify();
                }))
        });
        let contents = div()
            .id("asset-list-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .w_full()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .when(self.assets.is_empty(), |contents| {
                contents.child(
                    div()
                        .p_3()
                        .text_sm()
                        .text_color(rgb(0x737783))
                        .child("No assets in this project"),
                )
            })
            .children(rows);
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(230.0))
            .h_full()
            .bg(rgb(0x1d2027))
            .border_r_1()
            .border_color(rgb(0x30333d))
            .child(panel_header("Assets", self.assets.len()))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .h(px(if has_selected { 68.0 } else { 38.0 }))
                    .justify_center()
                    .gap_1()
                    .px_3()
                    .border_b_1()
                    .border_color(rgb(0x30333d))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("import-assets")
                                    .cursor_pointer()
                                    .rounded_sm()
                                    .px_2()
                                    .py_1()
                                    .bg(rgb(0x343842))
                                    .hover(|style| style.bg(rgb(0x4a4f5b)))
                                    .text_xs()
                                    .text_color(rgb(0xd8dae2))
                                    .child(if self.importing_assets {
                                        "Importing…"
                                    } else {
                                        "Import…"
                                    })
                                    .on_click(cx.listener(Self::import_assets_click)),
                            )
                            .child(
                                div()
                                    .id("insert-selected-asset")
                                    .rounded_sm()
                                    .px_2()
                                    .py_1()
                                    .bg(rgb(if can_insert_selected {
                                        0x3c674d
                                    } else {
                                        0x292c34
                                    }))
                                    .text_xs()
                                    .text_color(rgb(if can_insert_selected {
                                        0xd8f3df
                                    } else {
                                        0x737783
                                    }))
                                    .child("Add")
                                    .when(can_insert_selected, |button| {
                                        button
                                            .cursor_pointer()
                                            .hover(|style| style.bg(rgb(0x4c805f)))
                                            .on_click(
                                                cx.listener(Self::insert_selected_asset_click),
                                            )
                                    }),
                            )
                            .child(
                                div()
                                    .id("insert-selected-dialogue")
                                    .rounded_sm()
                                    .px_2()
                                    .py_1()
                                    .bg(rgb(if can_insert_dialogue {
                                        0x6b4a2f
                                    } else {
                                        0x292c34
                                    }))
                                    .text_xs()
                                    .text_color(rgb(if can_insert_dialogue {
                                        0xffdbb5
                                    } else {
                                        0x737783
                                    }))
                                    .child("Dialogue")
                                    .when(can_insert_dialogue, |button| {
                                        button
                                            .cursor_pointer()
                                            .hover(|style| style.bg(rgb(0x865e3d)))
                                            .on_click(
                                                cx.listener(Self::insert_selected_dialogue_click),
                                            )
                                    }),
                            ),
                    )
                    .when(has_selected, |actions| {
                        actions.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .id("relink-selected-asset")
                                        .cursor_pointer()
                                        .rounded_sm()
                                        .px_2()
                                        .py_1()
                                        .bg(rgb(0x343842))
                                        .hover(|style| style.bg(rgb(0x4a4f5b)))
                                        .text_xs()
                                        .text_color(rgb(0xd8dae2))
                                        .child("Relink…")
                                        .on_click(cx.listener(Self::relink_asset_click)),
                                )
                                .child(
                                    div()
                                        .id("remove-selected-asset")
                                        .cursor_pointer()
                                        .rounded_sm()
                                        .px_2()
                                        .py_1()
                                        .bg(rgb(0x4a292c))
                                        .hover(|style| style.bg(rgb(0x6b363b)))
                                        .text_xs()
                                        .text_color(rgb(0xffb7b7))
                                        .child("Remove")
                                        .on_click(cx.listener(Self::remove_asset_click)),
                                ),
                        )
                    }),
            )
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
            .bg(rgb(0x0d0f13))
            .when_some(self.preview.clone(), |canvas, preview| match preview {
                PreviewPresentation::Image(preview) => {
                    canvas.child(img(preview).size_full().object_fit(ObjectFit::Contain))
                }
                #[cfg(target_os = "macos")]
                PreviewPresentation::Surface(preview) => canvas.child(
                    gpui::surface(preview.pixel_buffer())
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                ),
            })
            .when_some(self.preview_error.clone(), |canvas, error| {
                canvas.child(
                    div()
                        .p_4()
                        .text_sm()
                        .text_color(rgb(0xff8b8b))
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
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(0x6b5327))
                        .bg(rgb(0x241f14))
                        .p_2()
                        .text_xs()
                        .text_color(rgb(0xffc46e))
                        .children(self.preview_warnings.iter().cloned()),
                )
            });
        div()
            .flex()
            .flex_col()
            .flex_1()
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
                    .gap_3()
                    .bg(rgb(0x181a20))
                    .border_t_1()
                    .border_color(rgb(0x30333d))
                    .text_color(rgb(0xc8cad2))
                    .child(
                        div()
                            .id("previous-frame")
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x404550)))
                            .px_3()
                            .py_1()
                            .rounded_sm()
                            .bg(rgb(0x292c34))
                            .child("-1f")
                            .on_click(cx.listener(Self::step_backward)),
                    )
                    .child(
                        div()
                            .id("toggle-playback")
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x4a4f5b)))
                            .px_3()
                            .py_1()
                            .rounded_sm()
                            .bg(rgb(0x343842))
                            .child(if self.playing { "Pause" } else { "Play" })
                            .on_click(cx.listener(Self::toggle_playback)),
                    )
                    .child(
                        div()
                            .id("next-frame")
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x404550)))
                            .px_3()
                            .py_1()
                            .rounded_sm()
                            .bg(rgb(0x292c34))
                            .child("+1f")
                            .on_click(cx.listener(Self::step_forward)),
                    )
                    .child(
                        div()
                            .w(px(110.0))
                            .text_sm()
                            .text_color(rgb(0x9da1ad))
                            .child(format!(
                                "{} / {}f",
                                self.clock.frame(),
                                self.clock.end_frame()
                            )),
                    ),
            )
    }

    fn inspector_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
            .w_full()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .child(inspector_row("Canvas", self.dimensions.clone()))
            .child(inspector_row("Frame rate", self.frame_rate_label.clone()))
            .child(inspector_row("Duration", self.duration.clone()))
            .child(inspector_row("Renderer", "wgpu"))
            .child(inspector_row("Adapter", self.gpu_name.clone()))
            .child(inspector_row(
                "React Entry",
                self.document
                    .react_entry()
                    .map_or_else(|| "Not set".to_owned(), str::to_owned),
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0x292c34))
                    .child(
                        inspector_button("react-entry-set", "Set…")
                            .on_click(cx.listener(Self::set_react_entry_click)),
                    )
                    .when(self.document.react_entry().is_some(), |controls| {
                        controls.child(
                            inspector_button("react-entry-clear", "Clear")
                                .on_click(cx.listener(Self::clear_react_entry_click)),
                        )
                    })
                    .when(self.component_schema_pending, |controls| {
                        controls.child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x8d919c))
                                .child("Loading component schemas…"),
                        )
                    })
                    .when_some(self.component_schema_error.clone(), |controls, error| {
                        controls.child(div().text_xs().text_color(rgb(0xff9a9a)).child(error))
                    }),
            )
            .when(self.document.react_entry().is_some(), |panel| {
                self.render_project_properties(panel, cx)
            })
            .when_some(selected_track, |panel, track| {
                let rename_track_id = track.id.clone();
                let enabled_track_id = track.id.clone();
                let locked_track_id = track.id.clone();
                let delete_track_id = track.id.clone();
                let renaming = self.renaming_track_id.as_deref() == Some(track.id.as_str());
                panel
                    .child(
                        div()
                            .mt_3()
                            .px_3()
                            .py_2()
                            .border_t_1()
                            .border_b_1()
                            .border_color(rgb(0x30333d))
                            .text_sm()
                            .text_color(rgb(0xffb466))
                            .child("Selected track"),
                    )
                    .child(inspector_row("ID", track.id.clone()))
                    .when(!renaming, |panel| {
                        panel
                            .child(inspector_row("Name", track.name.clone()))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .px_3()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(rgb(0x292c34))
                                    .child(
                                        inspector_button(
                                            "track-enabled",
                                            if track.enabled { "Disable" } else { "Enable" },
                                        )
                                        .when(!track.locked, |button| {
                                            button.on_click(cx.listener(move |this, _, _, cx| {
                                                this.toggle_track_enabled(&enabled_track_id, cx);
                                            }))
                                        })
                                        .when(track.locked, |button| button.opacity(0.45)),
                                    )
                                    .child(
                                        inspector_button(
                                            "track-locked",
                                            if track.locked { "Unlock" } else { "Lock" },
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.toggle_track_locked(&locked_track_id, cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        inspector_button("track-rename", "Rename")
                                            .when(!track.locked, |button| {
                                                button.on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.begin_track_rename(
                                                            &rename_track_id,
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                ))
                                            })
                                            .when(track.locked, |button| button.opacity(0.45)),
                                    )
                                    .child(
                                        inspector_button("track-delete", "Delete")
                                            .text_color(rgb(if track.locked {
                                                0x777b86
                                            } else {
                                                0xff9a9a
                                            }))
                                            .when(!track.locked, |button| {
                                                button.on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.request_delete_track(
                                                            &delete_track_id,
                                                            track.item_count,
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                ))
                                            }),
                                    ),
                            )
                    })
                    .when(renaming, |panel| {
                        panel.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .px_3()
                                .py_2()
                                .border_b_1()
                                .border_color(rgb(0x292c34))
                                .child(
                                    div()
                                        .id("track-name-input")
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .border_1()
                                        .border_color(rgb(0xffa13b))
                                        .bg(rgb(0x17191f))
                                        .text_sm()
                                        .text_color(rgb(0xffffff))
                                        .when_some(
                                            self.track_name_input.clone(),
                                            |field, input| field.child(input),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .child(
                                            inspector_button("track-rename-save", "Save").on_click(
                                                cx.listener(|this, _, _, cx| {
                                                    this.commit_track_rename(cx);
                                                }),
                                            ),
                                        )
                                        .child(
                                            inspector_button("track-rename-cancel", "Cancel")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.cancel_track_rename(cx);
                                                })),
                                        ),
                                ),
                        )
                    })
            })
            .when_some(selected_clip, |panel, clip| {
                panel
                    .child(
                        div()
                            .mt_3()
                            .px_3()
                            .py_2()
                            .border_t_1()
                            .border_b_1()
                            .border_color(rgb(0x30333d))
                            .text_sm()
                            .text_color(rgb(0xffb466))
                            .child("Selected clip"),
                    )
                    .child(inspector_row("Name", clip.name.clone()))
                    .child(inspector_row("Type", clip_kind_label(clip.kind)))
                    .child(inspector_row("Start", format_time(clip.start)))
                    .child(inspector_row("Length", format_time(clip.duration)))
                    .when_some(clip.volume.as_ref(), |panel, volume| {
                        let local_time = clip_local_time(self.current_time(), &clip);
                        let current_volume = evaluate_f64(volume, local_time).unwrap_or(1.0);
                        let keyframe_count = match volume {
                            Animatable::Static(_) => 0,
                            Animatable::Keyframes(animation) => animation.keyframes.len(),
                        };
                        let animated = keyframe_count > 0;
                        let has_keyframe = volume_keyframe_at(volume, local_time);
                        panel.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .px_3()
                                .py_2()
                                .border_b_1()
                                .border_color(rgb(0x292c34))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(0x737783))
                                        .child("Clip volume"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(inspector_button("clip-volume-down", "−").on_click(
                                            cx.listener(Self::decrease_selected_clip_volume),
                                        ))
                                        .child(
                                            div()
                                                .w(px(64.0))
                                                .text_center()
                                                .text_sm()
                                                .text_color(rgb(0xc8cad2))
                                                .child(format!(
                                                    "{}%",
                                                    (current_volume * 100.0).round() as i32
                                                )),
                                        )
                                        .child(inspector_button("clip-volume-up", "+").on_click(
                                            cx.listener(Self::increase_selected_clip_volume),
                                        )),
                                )
                                .child(div().text_xs().text_color(rgb(0x8d919c)).child(
                                    if animated {
                                        format!("Automation · {keyframe_count} keyframes")
                                    } else {
                                        "Automation · Static".to_owned()
                                    },
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap_2()
                                        .child(
                                            inspector_button(
                                                "clip-volume-keyframe",
                                                if has_keyframe {
                                                    "Update keyframe"
                                                } else {
                                                    "Add keyframe"
                                                },
                                            )
                                            .on_click(
                                                cx.listener(
                                                    Self::set_selected_clip_volume_keyframe,
                                                ),
                                            ),
                                        )
                                        .when(has_keyframe, |controls| {
                                            controls.child(
                                                inspector_button(
                                                    "clip-volume-remove-keyframe",
                                                    "Remove",
                                                )
                                                .on_click(cx.listener(
                                                    Self::remove_selected_clip_volume_keyframe,
                                                )),
                                            )
                                        })
                                        .when(animated, |controls| {
                                            controls.child(
                                                inspector_button("clip-volume-flatten", "Flatten")
                                                    .on_click(cx.listener(
                                                        Self::flatten_selected_clip_volume,
                                                    )),
                                            )
                                        }),
                                ),
                        )
                    })
                    .when_some(clip.component.clone(), |panel, component| {
                        self.render_component_props(panel, &clip.id, &component, cx)
                    })
                    .when_some(clip.dialogue.clone(), |panel, dialogue| {
                        let characters = self.document.characters();
                        self.render_dialogue_fields(panel, &clip.id, &dialogue, &characters, cx)
                    })
            });
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(260.0))
            .h_full()
            .bg(rgb(0x1d2027))
            .border_l_1()
            .border_color(rgb(0x30333d))
            .child(panel_header("Inspector", 0))
            .child(contents)
    }

    fn render_dialogue_fields<E: ParentElement + Sized>(
        &self,
        panel: E,
        clip_id: &str,
        dialogue: &DialogueClipSummary,
        characters: &[CharacterSummary],
        cx: &mut Context<Self>,
    ) -> E {
        let editing = self.editing_dialogue_clip_id.as_deref() == Some(clip_id);
        let edit_clip_id = clip_id.to_owned();
        let edit_text = dialogue.text.clone();
        let panel = panel
            .child(
                div()
                    .mt_3()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_b_1()
                    .border_color(rgb(0x30333d))
                    .text_sm()
                    .text_color(rgb(0xffb466))
                    .child("Dialogue"),
            )
            .child(inspector_row(
                "Voice asset",
                dialogue.audio.clone().unwrap_or_else(|| "None".to_owned()),
            ));
        let panel = if editing {
            panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0x292c34))
                    .child(div().text_xs().text_color(rgb(0x737783)).child("Text"))
                    .child(
                        div()
                            .id("dialogue-text-input")
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(0xffa13b))
                            .bg(rgb(0x17191f))
                            .text_sm()
                            .text_color(rgb(0xffffff))
                            .when_some(self.dialogue_text_input.clone(), |field, input| {
                                field.child(input)
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(inspector_button("dialogue-text-save", "Save").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.commit_dialogue_text_edit(cx);
                                }),
                            ))
                            .child(inspector_button("dialogue-text-cancel", "Cancel").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.cancel_dialogue_text_edit(cx);
                                }),
                            )),
                    ),
            )
        } else {
            panel
                .child(inspector_row("Text", dialogue.text.clone()))
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(rgb(0x292c34))
                        .child(
                            inspector_button("dialogue-text-edit", "Edit text").on_click(
                                cx.listener(move |this, _, window, cx| {
                                    this.begin_dialogue_text_edit(
                                        &edit_clip_id,
                                        &edit_text,
                                        window,
                                        cx,
                                    );
                                }),
                            ),
                        ),
                )
        };
        panel.child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(rgb(0x292c34))
                .child(div().text_xs().text_color(rgb(0x737783)).child("Character"))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .children(characters.iter().map(|character| {
                            let clip_id = clip_id.to_owned();
                            let character_id = character.id.clone();
                            let selected = dialogue.character == character.id;
                            let element_id: SharedString =
                                format!("dialogue-character-{}", character.id).into();
                            inspector_dynamic_button(element_id, character.name.clone())
                                .when(selected, |button| {
                                    button.bg(rgb(0x6b4a2f)).text_color(rgb(0xffdbb5))
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.set_dialogue_character(&clip_id, &character_id, cx);
                                }))
                        })),
                ),
        )
    }

    /// Renders one editable row per field of a registered component's
    /// `ComponentPropertySchema`, or an explanatory fallback when no schema
    /// is available yet (or ever, for a component registered without one).
    fn render_component_props<E: ParentElement + Sized>(
        &self,
        panel: E,
        clip_id: &str,
        component: &ComponentClipSummary,
        cx: &mut Context<Self>,
    ) -> E {
        let panel = panel.child(
            div()
                .mt_3()
                .px_3()
                .py_2()
                .border_t_1()
                .border_b_1()
                .border_color(rgb(0x30333d))
                .text_sm()
                .text_color(rgb(0xffb466))
                .child("Component"),
        );
        let panel = panel.child(inspector_row("Registered as", component.name.clone()));
        let Some(schema) = self.component_schemas.get(&component.name) else {
            let hint = if self.document.react_entry().is_none() {
                "Set a React Entry to edit this component's properties."
            } else if self.component_schema_pending {
                "Loading this component's property schema…"
            } else {
                "This component has no declared property schema."
            };
            return panel.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(0x8d919c))
                    .child(hint),
            );
        };
        let target = PropertyEditTarget::Clip {
            clip_id: clip_id.to_owned(),
        };
        schema.iter().fold(panel, |panel, (key, field)| {
            self.render_property_field(panel, &target, key, field, component.props.get(key), cx)
        })
    }

    /// Renders the entry-declared project property schema as one editable
    /// row per field, writing into the project-level `properties` map.
    /// Values not yet set fall back to each field's declared default — the
    /// same rule `useProjectProperty` applies when React reads them.
    fn render_project_properties<E: ParentElement + Sized>(
        &self,
        panel: E,
        cx: &mut Context<Self>,
    ) -> E {
        let panel = panel.child(
            div()
                .mt_3()
                .px_3()
                .py_2()
                .border_t_1()
                .border_b_1()
                .border_color(rgb(0x30333d))
                .text_sm()
                .text_color(rgb(0xffb466))
                .child("Project Properties"),
        );
        if !self.component_schema_pending && self.project_property_schema.is_none() {
            let hint = if self.component_schema_error.is_some() {
                "Project properties could not be loaded; see the error above."
            } else {
                "This React entry has not declared any project properties."
            };
            return panel.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(0x8d919c))
                    .child(hint),
            );
        }
        let Some(schema) = self.project_property_schema.as_ref() else {
            return panel.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(0x8d919c))
                    .child("Loading this entry's project property schema…"),
            );
        };
        let target = PropertyEditTarget::Project;
        schema.iter().fold(panel, |panel, (key, field)| {
            self.render_property_field(
                panel,
                &target,
                key,
                field,
                self.document.project_properties().get(key),
                cx,
            )
        })
    }

    fn render_property_field<E: ParentElement + Sized>(
        &self,
        panel: E,
        target: &PropertyEditTarget,
        key: &str,
        field: &ComponentPropertyField,
        current: Option<&serde_json::Value>,
        cx: &mut Context<Self>,
    ) -> E {
        let label: SharedString = match field {
            ComponentPropertyField::String { label, .. }
            | ComponentPropertyField::Number { label, .. }
            | ComponentPropertyField::Boolean { label, .. }
            | ComponentPropertyField::Color { label, .. }
            | ComponentPropertyField::Select { label, .. } => label
                .clone()
                .map_or_else(|| key.to_owned().into(), Into::into),
        };
        let editing = self
            .editing_property
            .as_ref()
            .is_some_and(|edit| edit.target == *target && edit.key == key);
        let id_prefix = match target {
            PropertyEditTarget::Project => "project-prop",
            PropertyEditTarget::Clip { .. } => "component-prop",
        };
        let field_id: SharedString = match target {
            PropertyEditTarget::Project => format!("{id_prefix}-project-{key}"),
            PropertyEditTarget::Clip { clip_id } => format!("{id_prefix}-{clip_id}-{key}"),
        }
        .into();

        panel.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(rgb(0x292c34))
                .child(div().text_xs().text_color(rgb(0x737783)).child(label))
                .child(match field {
                    ComponentPropertyField::Boolean { default_value, .. } => {
                        let value = current
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(*default_value);
                        let target = target.clone();
                        let key = key.to_owned();
                        inspector_dynamic_button(field_id, if value { "True" } else { "False" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_property_boolean(&target, &key, value, cx);
                            }))
                            .into_any_element()
                    }
                    ComponentPropertyField::Number {
                        default_value,
                        min,
                        max,
                        step,
                        ..
                    } => {
                        let value = current
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or(*default_value);
                        let step = step.unwrap_or(1.0);
                        let (min, max) = (*min, *max);
                        let down_target = target.clone();
                        let down_key = key.to_owned();
                        let up_target = target.clone();
                        let up_key = key.to_owned();
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                inspector_dynamic_button(
                                    SharedString::from(format!("{field_id}-down")),
                                    "−",
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.step_property_number(
                                            &down_target,
                                            &down_key,
                                            value,
                                            -step,
                                            (min, max),
                                            cx,
                                        );
                                    },
                                )),
                            )
                            .child(
                                div()
                                    .w(px(64.0))
                                    .text_center()
                                    .text_sm()
                                    .text_color(rgb(0xc8cad2))
                                    .child(format_component_number(value)),
                            )
                            .child(
                                inspector_dynamic_button(
                                    SharedString::from(format!("{field_id}-up")),
                                    "+",
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.step_property_number(
                                            &up_target,
                                            &up_key,
                                            value,
                                            step,
                                            (min, max),
                                            cx,
                                        );
                                    },
                                )),
                            )
                            .into_any_element()
                    }
                    ComponentPropertyField::Select {
                        default_value,
                        options,
                        ..
                    } => {
                        let value = current
                            .and_then(serde_json::Value::as_str)
                            .map_or_else(|| default_value.clone(), str::to_owned);
                        let target = target.clone();
                        let key = key.to_owned();
                        let options = options.clone();
                        inspector_dynamic_button(field_id, value.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.cycle_property_select(&target, &key, &options, &value, cx);
                            }))
                            .into_any_element()
                    }
                    ComponentPropertyField::String { default_value, .. }
                    | ComponentPropertyField::Color { default_value, .. } => {
                        let value = current
                            .and_then(serde_json::Value::as_str)
                            .map_or_else(|| default_value.clone(), str::to_owned);
                        if editing {
                            div()
                                .id(SharedString::from(format!("{field_id}-input")))
                                .px_2()
                                .py_1()
                                .rounded_sm()
                                .border_1()
                                .border_color(rgb(0xffa13b))
                                .bg(rgb(0x17191f))
                                .text_sm()
                                .text_color(rgb(0xffffff))
                                .when_some(self.property_input.clone(), |field, input| {
                                    field.child(input)
                                })
                                .into_any_element()
                        } else {
                            let target = target.clone();
                            let key = key.to_owned();
                            let value_for_edit = value.clone();
                            inspector_dynamic_button(field_id, value)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.begin_property_edit(
                                        &target,
                                        &key,
                                        &value_for_edit,
                                        window,
                                        cx,
                                    );
                                }))
                                .into_any_element()
                        }
                    }
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
        let track_count = self.tracks.len();
        let rows = self.tracks.iter().enumerate().map(|(track_index, track)| {
            let selected_track = self.selected_track_id.as_deref() == Some(track.id.as_str());
            let clip_drag_hover =
                self.clip_drag_hover_track_id.as_deref() == Some(track.id.as_str());
            let clip_drag_target =
                self.clip_drag_target_track_id.as_deref() == Some(track.id.as_str());
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
            let meter_color = if track_level >= 0.9 {
                0xe05d5d
            } else if track_level >= 0.7 {
                0xe4b34c
            } else {
                0x70d99a
            };
            let color = match track.kind {
                TrackKind::Video => 0x4b7bec,
                TrackKind::Audio => 0x26a269,
                TrackKind::Overlay => 0x9b59b6,
                TrackKind::Dialogue => 0xe58e26,
            };
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
                let drag_clip_id = clip_id.clone();
                let trim_start_clip_id = clip_id.clone();
                let trim_end_clip_id = clip_id.clone();
                let trim_start_element_id: SharedString = format!("trim-start-{clip_id}").into();
                let trim_end_element_id: SharedString = format!("trim-end-{clip_id}").into();
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
                    .left(relative(start))
                    .top(px(5.0))
                    .h(px(28.0))
                    .w(relative(duration))
                    .min_w(px(3.0))
                    .overflow_hidden()
                    .rounded_sm()
                    .bg(rgb(color))
                    .cursor_pointer()
                    .when(selected, |clip| clip.border_2().border_color(rgb(0xffd29d)))
                    .when(!clip.enabled, |clip| clip.opacity(0.4))
                    .px_2()
                    .text_xs()
                    .text_color(rgb(0xffffff))
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
                                        .bg(rgb(0xffffff))
                                })),
                        )
                    })
                    .child(div().relative().child(format!("{kind}  {}", clip.name)))
                    .when(selected, |clip| {
                        clip.child(
                            div()
                                .id(trim_start_element_id)
                                .absolute()
                                .left(px(0.0))
                                .top(px(0.0))
                                .bottom(px(0.0))
                                .w(px(8.0))
                                .cursor(CursorStyle::ResizeLeftRight)
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(1.0))
                                        .top(px(4.0))
                                        .bottom(px(4.0))
                                        .w(px(3.0))
                                        .rounded_full()
                                        .bg(rgb(0xffffff)),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, event, window, cx| {
                                        cx.stop_propagation();
                                        this.begin_clip_drag(
                                            &trim_start_clip_id,
                                            ClipDragKind::TrimStart,
                                            event,
                                            window,
                                            cx,
                                        );
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .id(trim_end_element_id)
                                .absolute()
                                .right(px(0.0))
                                .top(px(0.0))
                                .bottom(px(0.0))
                                .w(px(8.0))
                                .cursor(CursorStyle::ResizeLeftRight)
                                .child(
                                    div()
                                        .absolute()
                                        .right(px(1.0))
                                        .top(px(4.0))
                                        .bottom(px(4.0))
                                        .w(px(3.0))
                                        .rounded_full()
                                        .bg(rgb(0xffffff)),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, event, window, cx| {
                                        cx.stop_propagation();
                                        this.begin_clip_drag(
                                            &trim_end_clip_id,
                                            ClipDragKind::TrimEnd,
                                            event,
                                            window,
                                            cx,
                                        );
                                    }),
                                ),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event, window, cx| {
                            this.begin_clip_drag(
                                &drag_clip_id,
                                ClipDragKind::Move,
                                event,
                                window,
                                cx,
                            );
                        }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_clip_id = Some(clip_id.clone());
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
            let hover_track_id = track.id.clone();
            let move_up_track_id = track.id.clone();
            let move_down_track_id = track.id.clone();
            let drop_track_id = track.id.clone();
            let drop_track_kind = track.kind;
            let drop_track_locked = track.locked;
            let mute_element_id: SharedString = format!("track-mute-{}", track.id).into();
            let solo_element_id: SharedString = format!("track-solo-{}", track.id).into();
            let up_element_id: SharedString = format!("track-up-{}", track.id).into();
            let down_element_id: SharedString = format!("track-down-{}", track.id).into();
            let track_element_id: SharedString = format!("timeline-track-{}", track.id).into();
            div()
                .id(track_element_id)
                .flex()
                .flex_none()
                .h(px(38.0))
                .w_full()
                .border_b_1()
                .border_color(rgb(0x292c34))
                .when(selected_track, |row| row.bg(rgb(0x252a34)))
                .when(clip_drag_target, |row| row.bg(rgb(0x294636)))
                .when(clip_drag_hover && !clip_drag_target, |row| {
                    row.bg(rgb(0x4b3032))
                })
                .on_mouse_move(cx.listener(move |this, event, _, cx| {
                    this.update_clip_drag_target(&hover_track_id, event, cx);
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.renaming_track_id.as_deref() != Some(select_track_id.as_str()) {
                        this.renaming_track_id = None;
                    }
                    if this.selected_track_id.as_deref() == Some(select_track_id.as_str()) {
                        this.selected_track_id = None;
                    } else {
                        this.selected_track_id = Some(select_track_id.clone());
                    }
                    this.edit_error = None;
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
                        .text_color(rgb(0xc8cad2))
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
                                    .bg(rgb(0x15171c))
                                    .child(
                                        div()
                                            .h_full()
                                            .w(relative(track_level))
                                            .rounded_full()
                                            .bg(rgb(meter_color)),
                                    ),
                            )
                        })
                        .child(
                            div()
                                .id(up_element_id)
                                .flex_none()
                                .cursor_pointer()
                                .px_1()
                                .text_xs()
                                .text_color(rgb(if track_index > 0 && !track.locked {
                                    0xc8cad2
                                } else {
                                    0x555964
                                }))
                                .child("↑")
                                .when(track_index > 0 && !track.locked, |button| {
                                    button.on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.move_track(&move_up_track_id, -1, cx);
                                    }))
                                }),
                        )
                        .child(
                            div()
                                .id(down_element_id)
                                .flex_none()
                                .cursor_pointer()
                                .px_1()
                                .text_xs()
                                .text_color(rgb(
                                    if track_index + 1 < track_count && !track.locked {
                                        0xc8cad2
                                    } else {
                                        0x555964
                                    },
                                ))
                                .child("↓")
                                .when(track_index + 1 < track_count && !track.locked, |button| {
                                    button.on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.move_track(&move_down_track_id, 1, cx);
                                    }))
                                }),
                        )
                        .when(track.kind != TrackKind::Overlay, |header| {
                            header
                                .child(
                                    div()
                                        .id(mute_element_id)
                                        .flex_none()
                                        .cursor_pointer()
                                        .rounded_sm()
                                        .px_2()
                                        .py_1()
                                        .text_xs()
                                        .bg(rgb(if track.muted { 0xb84c4c } else { 0x292c34 }))
                                        .hover(|style| style.bg(rgb(0x555b68)))
                                        .child("M")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.toggle_track_mute(&mute_track_id, cx);
                                        })),
                                )
                                .child(
                                    div()
                                        .id(solo_element_id)
                                        .flex_none()
                                        .cursor_pointer()
                                        .rounded_sm()
                                        .px_2()
                                        .py_1()
                                        .text_xs()
                                        .bg(rgb(if track.solo { 0xb28a2e } else { 0x292c34 }))
                                        .hover(|style| style.bg(rgb(0x555b68)))
                                        .child("S")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.toggle_track_solo(&solo_track_id, cx);
                                        })),
                                )
                        }),
                )
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .h_full()
                        .mr_2()
                        .overflow_hidden()
                        .drag_over::<AssetDrag>(move |style, asset, _, _| {
                            if drop_track_locked {
                                style
                                    .bg(rgb(0x552b30))
                                    .border_1()
                                    .border_color(rgb(0xff747f))
                            } else if track_accepts_asset(drop_track_kind, asset.kind) {
                                style
                                    .bg(rgb(0x294636))
                                    .border_1()
                                    .border_color(rgb(0x70d99a))
                            } else {
                                style
                                    .bg(rgb(0x4b3032))
                                    .border_1()
                                    .border_color(rgb(0xe27980))
                            }
                        })
                        .on_drop(cx.listener(move |this, asset: &AssetDrag, window, cx| {
                            this.drop_asset_on_track(asset, &drop_track_id, window, cx);
                        }))
                        .children(clips),
                )
        });
        let track_area = div()
            .id("timeline-tracks-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .w_full()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .when(self.tracks.is_empty(), |area| {
                area.child(
                    div()
                        .id("empty-timeline-drop-target")
                        .flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(rgb(0x737783))
                        .drag_over::<AssetDrag>(|style, asset, _, _| {
                            if asset.kind == AssetKind::Font {
                                style.bg(rgb(0x4b3032))
                            } else {
                                style.bg(rgb(0x294636))
                            }
                        })
                        .on_drop(cx.listener(|this, asset: &AssetDrag, window, cx| {
                            this.drop_asset_without_track(asset, window, cx);
                        }))
                        .child("No tracks yet — drop an asset here"),
                )
            })
            .children(rows);
        div()
            .id("timeline-panel")
            .flex()
            .flex_col()
            .flex_none()
            .h(px(230.0))
            .w_full()
            .bg(rgb(0x181a20))
            .border_t_1()
            .border_color(rgb(0x30333d))
            .on_mouse_move(cx.listener(Self::continue_clip_drag))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::end_clip_drag))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .h(px(38.0))
                    .items_center()
                    .px_3()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_sm().text_color(rgb(0xd8dae2)).child("Timeline"))
                            .children(
                                [
                                    (TrackKind::Video, "+ Video", "add-video-track"),
                                    (TrackKind::Audio, "+ Audio", "add-audio-track"),
                                    (TrackKind::Overlay, "+ Overlay", "add-overlay-track"),
                                    (TrackKind::Dialogue, "+ Dialogue", "add-dialogue-track"),
                                ]
                                .into_iter()
                                .map(
                                    |(kind, label, element_id)| {
                                        div()
                                            .id(element_id)
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .px_2()
                                            .py_1()
                                            .bg(rgb(0x292c34))
                                            .hover(|style| style.bg(rgb(0x404550)))
                                            .text_xs()
                                            .text_color(rgb(0xb8bbc5))
                                            .child(label)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.add_track(kind, cx);
                                            }))
                                    },
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().text_xs().text_color(rgb(0x737783)).child(format!(
                                "{} / {}",
                                format_time(self.current_time()),
                                self.duration
                            )))
                            .when(self.selected_clip_id.is_some(), |controls| {
                                controls.child(
                                    div()
                                        .id("delete-selected-clip")
                                        .cursor_pointer()
                                        .rounded_sm()
                                        .px_2()
                                        .py_1()
                                        .bg(rgb(0x4a292c))
                                        .hover(|style| style.bg(rgb(0x6b363b)))
                                        .text_xs()
                                        .text_color(rgb(0xffb7b7))
                                        .child("Delete")
                                        .on_click(cx.listener(Self::delete_selected_clip_click)),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .h(px(24.0))
                    .w_full()
                    .items_center()
                    .child(div().w(px(230.0)))
                    .child(
                        div()
                            .id("timeline-scrubber")
                            .relative()
                            .flex_1()
                            .h(px(16.0))
                            .mr_2()
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::begin_scrub))
                            .on_mouse_move(cx.listener(Self::continue_scrub))
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::end_scrub))
                            .child(
                                div()
                                    .absolute()
                                    .top(px(7.0))
                                    .left(px(0.0))
                                    .w_full()
                                    .h(px(3.0))
                                    .bg(rgb(0x30333d)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(7.0))
                                    .left(px(0.0))
                                    .h(px(3.0))
                                    .w(relative(progress))
                                    .bg(rgb(0xffa13b)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(2.0))
                                    .left(relative(progress))
                                    .ml(px(-5.0))
                                    .size(px(11.0))
                                    .rounded_full()
                                    .bg(rgb(0xffa13b)),
                            ),
                    ),
            )
            .child(track_area)
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
        let title = if self.document.is_dirty() {
            format!("{} * — Mikan", self.project_name)
        } else {
            format!("{} — Mikan", self.project_name)
        };
        window.set_window_title(&title);
        window.set_window_edited(self.document.is_dirty());
        div()
            .key_context("MikanEditor")
            .on_action(cx.listener(Self::save_project))
            .on_action(cx.listener(Self::save_project_as))
            .on_action(cx.listener(Self::export_project_action))
            .on_action(cx.listener(Self::undo_edit))
            .on_action(cx.listener(Self::redo_edit))
            .on_action(cx.listener(Self::toggle_playback_action))
            .on_action(cx.listener(Self::previous_frame_action))
            .on_action(cx.listener(Self::next_frame_action))
            .on_action(cx.listener(Self::import_assets_action))
            .on_action(cx.listener(Self::insert_selected_asset_action))
            .on_action(cx.listener(Self::delete_selected_clip_action))
            .when_some(self.focus_handle.as_ref(), |view, focus_handle| {
                view.track_focus(focus_handle)
            })
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(rgb(0x14161b))
            .font_family(".SystemUIFont")
            .child(self.toolbar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .w_full()
                    .overflow_hidden()
                    .child(self.asset_panel(cx))
                    .child(self.preview_panel(cx))
                    .child(self.inspector_panel(cx)),
            )
            .child(self.timeline(cx))
    }
}

fn panel_header(title: &'static str, count: usize) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .h(px(40.0))
        .items_center()
        .justify_between()
        .px_3()
        .border_b_1()
        .border_color(rgb(0x30333d))
        .text_sm()
        .text_color(rgb(0xd8dae2))
        .child(title)
        .when(count > 0, |header| {
            header.child(
                div()
                    .text_xs()
                    .text_color(rgb(0x737783))
                    .child(count.to_string()),
            )
        })
}

fn dragged_clip_range(drag: &ClipDrag, pointer_frame: i64, timeline_end: i64) -> (i64, i64) {
    let delta = pointer_frame.saturating_sub(drag.pointer_frame);
    let original_end = drag.start_frame.saturating_add(drag.duration_frames);
    match drag.kind {
        ClipDragKind::Move => {
            let latest_start = timeline_end.saturating_sub(drag.duration_frames);
            (
                drag.start_frame
                    .saturating_add(delta)
                    .clamp(0, latest_start),
                drag.duration_frames,
            )
        }
        ClipDragKind::TrimStart => {
            let start = drag
                .start_frame
                .saturating_add(delta)
                .clamp(0, original_end.saturating_sub(1));
            (start, original_end.saturating_sub(start))
        }
        ClipDragKind::TrimEnd => {
            let end = original_end
                .saturating_add(delta)
                .clamp(drag.start_frame.saturating_add(1), timeline_end);
            (drag.start_frame, end.saturating_sub(drag.start_frame))
        }
    }
}

fn initial_clip_duration_frames(
    media: Option<&Result<MediaAssetInfo, String>>,
    frame_rate: Rational,
) -> i64 {
    let fallback = (u64::from(frame_rate.numerator) * 5)
        .div_ceil(u64::from(frame_rate.denominator))
        .max(1)
        .min(i64::MAX as u64) as i64;
    media
        .and_then(|info| info.as_ref().ok())
        .and_then(|info| info.duration)
        .and_then(|duration| TimelineClock::new(duration, frame_rate).ok())
        .map(TimelineClock::end_frame)
        .filter(|frames| *frames > 0)
        .unwrap_or(fallback)
}

fn track_accepts_asset(track: TrackKind, asset: AssetKind) -> bool {
    matches!(
        (track, asset),
        (TrackKind::Video, AssetKind::Video)
            | (TrackKind::Audio, AssetKind::Audio)
            | (TrackKind::Overlay, AssetKind::Image)
    )
}

fn track_accepts_clip(track: TrackKind, clip: ClipKind) -> bool {
    matches!(
        (track, clip),
        (TrackKind::Video, ClipKind::Video)
            | (TrackKind::Audio, ClipKind::Audio)
            | (
                TrackKind::Overlay,
                ClipKind::Image | ClipKind::Text | ClipKind::Component
            )
            | (TrackKind::Dialogue, ClipKind::Dialogue)
    )
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

fn resolve_asset_location(location: &AssetLocation, asset_root: &Path) -> Option<PathBuf> {
    match location {
        AssetLocation::File { path } => {
            let path = Path::new(path);
            Some(if path.is_absolute() {
                path.to_owned()
            } else {
                asset_root.join(path)
            })
        }
        AssetLocation::Url { .. } => None,
    }
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

fn level_at_time(levels: &[f32], clip: &mikan_editor::ClipSummary, time: Time) -> f32 {
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

fn clip_local_time(time: Time, clip: &mikan_editor::ClipSummary) -> Time {
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

fn volume_keyframe_at(volume: &Animatable<f64>, time: Time) -> bool {
    let Animatable::Keyframes(animation) = volume else {
        return false;
    };
    animation.keyframes.iter().any(|keyframe| {
        keyframe
            .time
            .cmp_exact(time)
            .is_ok_and(|ordering| ordering.is_eq())
    })
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

fn inspector_row(label: &'static str, value: impl Into<SharedString>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(rgb(0x292c34))
        .child(div().text_xs().text_color(rgb(0x737783)).child(label))
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xc8cad2))
                .child(value.into()),
        )
}

fn inspector_button(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    inspector_dynamic_button(id, label)
}

/// Same styling as [`inspector_button`], but for a `component_prop_field_id`,
/// derived from a dynamic clip id and prop key, that cannot be a `&'static
/// str`.
fn inspector_dynamic_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id.into())
        .cursor_pointer()
        .rounded_sm()
        .bg(rgb(0x292c34))
        .hover(|style| style.bg(rgb(0x454a56)))
        .px_2()
        .py_1()
        .text_xs()
        .text_color(rgb(0xc8cad2))
        .child(label.into())
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

fn export_suggested_name(path: Option<&Path>, project_name: &str) -> String {
    let name = path
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .unwrap_or(project_name);
    let stem = name
        .strip_suffix(".mikan.json")
        .or_else(|| name.strip_suffix(".json"))
        .unwrap_or(name);
    format!("{stem}.mp4")
}

fn main() {
    if let Err(error) = run() {
        eprintln!("mikan-editor: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    let mut editor = EditorView::open(path.as_deref())?;

    Application::new().run(move |cx: &mut App| {
        TextInput::bind_keys(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-s", SaveProject, Some("MikanEditor")),
            KeyBinding::new("cmd-shift-s", SaveProjectAs, Some("MikanEditor")),
            KeyBinding::new("cmd-shift-e", ExportProject, Some("MikanEditor")),
            KeyBinding::new("cmd-z", UndoEdit, Some("MikanEditor")),
            KeyBinding::new("cmd-shift-z", RedoEdit, Some("MikanEditor")),
            KeyBinding::new("space", TogglePlayback, Some("MikanEditor")),
            KeyBinding::new("left", PreviousFrame, Some("MikanEditor")),
            KeyBinding::new("right", NextFrame, Some("MikanEditor")),
            KeyBinding::new("cmd-i", ImportAssets, Some("MikanEditor")),
            KeyBinding::new("cmd-return", InsertSelectedAsset, Some("MikanEditor")),
            KeyBinding::new("backspace", DeleteSelectedClip, Some("MikanEditor")),
            KeyBinding::new("delete", DeleteSelectedClip, Some("MikanEditor")),
        ]);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Mikan".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| {
                    let focus_handle = cx.focus_handle();
                    let master_volume_focus = cx.focus_handle().tab_stop(true).tab_index(0);
                    let track_name_input = cx.new(TextInput::new);
                    cx.subscribe(
                        &track_name_input,
                        |editor: &mut EditorView, _, event: &TextInputEvent, cx| match event {
                            TextInputEvent::Submit => editor.commit_track_rename(cx),
                            TextInputEvent::Cancel => editor.cancel_track_rename(cx),
                        },
                    )
                    .detach();
                    let property_input = cx.new(TextInput::new);
                    cx.subscribe(
                        &property_input,
                        |editor: &mut EditorView, _, event: &TextInputEvent, cx| match event {
                            TextInputEvent::Submit => editor.commit_property_edit(cx),
                            TextInputEvent::Cancel => editor.cancel_property_edit(cx),
                        },
                    )
                    .detach();
                    let dialogue_text_input = cx.new(TextInput::new);
                    cx.subscribe(
                        &dialogue_text_input,
                        |editor: &mut EditorView, _, event: &TextInputEvent, cx| match event {
                            TextInputEvent::Submit => editor.commit_dialogue_text_edit(cx),
                            TextInputEvent::Cancel => editor.cancel_dialogue_text_edit(cx),
                        },
                    )
                    .detach();
                    focus_handle.focus(window);
                    editor.focus_handle = Some(focus_handle);
                    editor.master_volume_focus = Some(master_volume_focus);
                    editor.track_name_input = Some(track_name_input);
                    editor.property_input = Some(property_input);
                    editor.dialogue_text_input = Some(dialogue_text_input);
                    editor
                });
                let close_view = view.clone();
                window.on_window_should_close(cx, move |window, cx| {
                    if close_view.read(cx).force_close || !close_view.read(cx).document.is_dirty() {
                        return true;
                    }
                    close_view.update(cx, |editor, cx| editor.prompt_to_close(window, cx));
                    false
                });
                view
            },
        )
        .expect("could not open the Mikan editor window");
        cx.activate(true);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AudioCacheKey, CachedAudioDecoder, ClipDrag, ClipDragKind, ClipKind, DiskAudioCache,
        EDITOR_DEMO_PROJECT, ExportEvent, ExportRequest, ExportWorker, MediaAssetInfo,
        clip_level_envelope, dragged_clip_range, export_suggested_name,
        initial_clip_duration_frames, level_at_time, map_clip_waveform, master_volume_from_drag,
        take_latest, track_accepts_asset, track_accepts_clip, waveform_peaks, waveform_segment,
    };
    use mikan_composition::{
        Animatable, AssetLocation, AudioClip, Rational, ResolvedAsset, Time, TimeRange,
    };
    use mikan_editor::ClipSummary;
    use mikan_exporter::ExportCancellation;
    use mikan_media::{AudioBuffer, AudioDecoder};
    use mikan_project::{AssetKind, Project, TrackKind};
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
        let output =
            std::env::temp_dir().join(format!("mikan-cancelled-export-{}.mp4", std::process::id()));
        let _ = fs::remove_file(&output);
        worker
            .request(ExportRequest {
                project: Project::from_json(EDITOR_DEMO_PROJECT).unwrap(),
                asset_root: PathBuf::from("examples"),
                output: output.clone(),
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
    fn export_name_replaces_project_extensions() {
        assert_eq!(
            export_suggested_name(Some(PathBuf::from("demo.mikan.json").as_path()), "ignored"),
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
    fn body_drag_preserves_the_clip_center_from_every_grab_position() {
        let move_from = |pointer_frame| ClipDrag {
            clip_id: "clip".to_owned(),
            kind: ClipDragKind::Move,
            pointer_frame,
            start_frame: 100,
            duration_frames: 60,
        };

        assert_eq!(dragged_clip_range(&move_from(100), 110, 1_000), (110, 60));
        assert_eq!(dragged_clip_range(&move_from(130), 140, 1_000), (110, 60));
        assert_eq!(dragged_clip_range(&move_from(159), 169, 1_000), (110, 60));
    }

    #[test]
    fn probed_duration_replaces_the_five_second_insertion_default() {
        let info = Ok(MediaAssetInfo {
            duration: Some(Time::new(961_104, 1_000_000)),
            video_size: None,
            has_audio: true,
        });

        assert_eq!(
            initial_clip_duration_frames(Some(&info), Rational::new(60, 1)),
            58
        );
        assert_eq!(
            initial_clip_duration_frames(None, Rational::new(60, 1)),
            300
        );
    }

    #[test]
    fn assets_only_highlight_compatible_timeline_tracks() {
        assert!(track_accepts_asset(TrackKind::Video, AssetKind::Video));
        assert!(track_accepts_asset(TrackKind::Audio, AssetKind::Audio));
        assert!(track_accepts_asset(TrackKind::Overlay, AssetKind::Image));
        assert!(!track_accepts_asset(TrackKind::Audio, AssetKind::Video));
        assert!(!track_accepts_asset(TrackKind::Dialogue, AssetKind::Audio));
        assert!(!track_accepts_asset(TrackKind::Overlay, AssetKind::Font));
        assert!(track_accepts_clip(TrackKind::Audio, ClipKind::Audio));
        assert!(track_accepts_clip(TrackKind::Overlay, ClipKind::Text));
        assert!(!track_accepts_clip(TrackKind::Video, ClipKind::Audio));
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
            "mikan-editor-disk-cache-{}-{unique}",
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
