use crate::audio::take_latest;
use crate::helpers::prepare_preview_frame;
use crate::layer_select::LookupResult;
use crate::problems::error_chain;
use crate::source::PropertyArgs;
use celesta_composition::{Layer, LayerContent, Scene, ShaderSource, Time};
use celesta_gpu_renderer::{GpuRenderer, HitRegion, LayerFrame, RenderQuality, layer_frame};
use celesta_react_bridge::{
    ComponentPropertyField, ComponentResolutionRequest, ComponentResolutions, PrepareDependency,
    PrimitivePropertyTable, PropOverride, PropertyInputs, PropertyIssue, ReactBridge, ReactChapter,
    ReactCompositionMetadata, SpawnOptions,
};
use gpui_kit::RenderImage;
use rodio::Player;
use rodio::buffer::SamplesBuffer;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::SystemTime;

pub(crate) struct AudioPreview {
    pub(crate) _device_sink: rodio::MixerDeviceSink,
    pub(crate) player: Player,
    pub(crate) source: SamplesBuffer,
    /// Monitor gain applied on top of the mixed buffer. Carried here because
    /// every seek reconnects a fresh `Player`, which starts at unity gain.
    pub(crate) volume: f32,
}

pub(crate) struct PreviewRequest {
    pub(crate) generation: u64,
    pub(crate) scene: Scene,
    pub(crate) asset_root: PathBuf,
    /// The project's `react_entry` and the runtime that serves it, when one
    /// is configured. `None` means component clips have nothing to resolve
    /// against this frame.
    pub(crate) react: Option<ReactPreviewContext>,
    /// `WholeScene` replaces `scene` entirely with the React entry's own
    /// evaluation at `scene.time` (standalone `.tsx` preview); the default
    /// `ResolveComponents` only fills in `TimelineContent::Component` clips.
    pub(crate) react_mode: ReactPreviewMode,
    /// Bumped by the reload watcher so the preview worker drops its cached
    /// `ReactPreviewBridge` and respawns Node, picking up a re-bundle after
    /// the composition's code changed.
    pub(crate) react_reload: u64,
    /// `Draft` while playing, so scaled text is not re-rasterized for every
    /// frame; `Final` otherwise, so a paused or scrubbed frame looks exactly
    /// like the export.
    pub(crate) render_quality: RenderQuality,
    /// The selected layer whose JSX source to look up in this frame
    /// (`WholeScene` only); `None` sends no lookup, as during playback.
    pub(crate) source_of: Option<String>,
    /// Values the React entry evaluates the props written at their positions
    /// with in this frame, while a layer is dragged (`WholeScene` only).
    pub(crate) prop_overrides: Vec<PropOverride>,
    /// The layer whose frame to measure from this frame's draw
    /// (`WholeScene` only); `None` measures nothing, as during playback.
    pub(crate) frame_of: Option<String>,
    /// The timeline frame `scene` is for, echoed in the result.
    pub(crate) timeline_frame: i64,
}

pub(crate) struct PreviewResult {
    pub(crate) generation: u64,
    pub(crate) frame: Result<PreviewPresentation, String>,
    /// Recoverable diagnostics for this frame: unresolved `registerComponent`
    /// names (hidden from the preview) and React runtime failures.
    pub(crate) warnings: Vec<String>,
    /// Where each leaf layer was drawn, for selecting layers by clicking the
    /// preview. Recorded in interactive `WholeScene` frames only; empty
    /// otherwise.
    pub(crate) hit_regions: Vec<HitRegion>,
    /// The request's `source_of` and what looking it up returned: `Ok(None)`
    /// when the frame has no layer with that id. `None` when nothing was
    /// looked up.
    pub(crate) layer_source: Option<(String, LookupResult)>,
    /// The request's `frame_of` and what measuring it returned: `None` inside
    /// when the layer is absent or has no affine frame (see `layer_frame`).
    /// `None` when nothing was measured.
    pub(crate) layer_frame: Option<(String, Option<LayerFrame>)>,
    /// The request's `timeline_frame`.
    pub(crate) timeline_frame: i64,
}

/// The Node.js runtime a preview's `TimelineContent::Component` clips resolve
/// against — the same paths `ComponentSchemaWorker` uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReactPreviewContext {
    pub(crate) node: PathBuf,
    pub(crate) cli_script: PathBuf,
    pub(crate) entry: PathBuf,
    /// Project property values; changed values respawn the bridge.
    pub(crate) properties: PropertyInputs,
    /// Whether the runtime records where each JSX element was written, so
    /// layers can be picked and their source looked up (the React preview's
    /// interactive mode). Switching respawns the bridge: the plain preview
    /// pays nothing for it.
    pub(crate) source_locations: bool,
    pub(crate) composition_id: Option<String>,
}

/// Whether the renderer records where each leaf layer was drawn for this
/// frame: only for a whole React scene in interactive mode, the only frames
/// the viewer picks layers from.
pub(crate) fn records_hit_regions(
    mode: ReactPreviewMode,
    react: Option<&ReactPreviewContext>,
) -> bool {
    mode == ReactPreviewMode::WholeScene && react.is_some_and(|react| react.source_locations)
}

/// How the preview worker should treat this frame's React entry: resolve just
/// the `TimelineContent::Component` clips a `project.json` placed (the normal
/// GUI-project case), or render the whole composition the entry describes
/// (standalone `.tsx` preview mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReactPreviewMode {
    ResolveComponents,
    WholeScene,
}

/// Editor state for previewing a standalone React composition entry (`.tsx`).
/// The [`EditorDocument`] is a synthetic, unsaveable project holding only the
/// composition's dimensions/frame rate/duration; every previewed frame and the
/// audio graph come from the React bridge instead of evaluating tracks.
pub(crate) struct ReactPreview {
    pub(crate) composition_id: String,
    pub(crate) compositions: Vec<celesta_react_bridge::ReactCompositionInfo>,
    /// The selected composition's `<Composition chapters>` as last loaded;
    /// kept across failed reloads.
    pub(crate) chapters: Vec<ReactChapter>,
    /// The primitives' property tables as last loaded, keyed by primitive
    /// name; kept across failed reloads.
    pub(crate) primitive_properties: BTreeMap<String, PrimitivePropertyTable>,
    pub(crate) entry: PathBuf,
    /// Newest source-file modification time seen under the entry's directory
    /// (or of the `--props-file`). The reload watcher compares against this
    /// to notice edits.
    pub(crate) watched_mtime: Option<SystemTime>,
    /// Files and directories the entry's `prepare()` read (voice folders and
    /// the like, possibly outside the entry's directory), as last read. The
    /// reload watcher compares their state on disk against this.
    pub(crate) dependencies: Vec<PrepareDependency>,
    /// The command line's `--props`/`--props-file`, re-read on every reload.
    pub(crate) property_args: PropertyArgs,
    /// The values `property_args` held at the last *successful* (re)load: what
    /// the preview worker renders with, so a bad value keeps showing the last
    /// good render.
    pub(crate) properties: PropertyInputs,
    /// The inputs most recently read from disk, even when the entry then
    /// rejected them. Equal to `properties` after a successful load; the
    /// property panel computes effective values from this one.
    pub(crate) disk_properties: PropertyInputs,
    /// The last schema a load of this entry returned; kept across failed loads.
    pub(crate) property_schema: Option<KeptSchema>,
    /// Issues from the last load, empty after a successful one.
    pub(crate) property_issues: Vec<PropertyIssue>,
    /// Why the last reload could not read the props file; cleared by the next
    /// reload that read it.
    pub(crate) property_load_error: Option<String>,
    /// The number of the newest applied reload that read the props file.
    pub(crate) properties_read_reload: u64,
}

impl ReactPreview {
    /// The property table of the primitive `name` (`Rect`, `Star`, ...), as
    /// a source chain names it.
    pub(crate) fn primitive_table(&self, name: &str) -> Option<&PrimitivePropertyTable> {
        self.primitive_properties.get(name)
    }
}

/// The entry's project property schema with the order it declared its keys in
/// (the map itself sorts them).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KeptSchema {
    pub(crate) fields: BTreeMap<String, ComponentPropertyField>,
    pub(crate) order: Vec<String>,
}

impl KeptSchema {
    /// The schema a load reported, or `None` when the entry declared none.
    pub(crate) fn from_metadata(metadata: &ReactCompositionMetadata) -> Option<Self> {
        metadata.project_property_schema.clone().map(|fields| Self {
            fields,
            order: metadata.project_property_order.clone(),
        })
    }
}

pub(crate) struct CpuPreviewFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

#[derive(Clone)]
pub(crate) enum PreviewPresentation {
    Image(Arc<RenderImage>),
    #[cfg(target_os = "macos")]
    Surface(celesta_gpu_renderer::NativePreviewFrame),
}

pub(crate) struct PreviewWorker {
    pub(crate) requests: mpsc::Sender<PreviewRequest>,
    pub(crate) results: mpsc::Receiver<PreviewResult>,
}

/// The preview worker's long-lived connection to a React entry, respawned
/// whenever the project's `react_entry` (or runtime paths) change. A failed
/// spawn or a dead connection is remembered per context so a permanent
/// failure does not restart Node on every rendered frame.
pub(crate) struct ReactPreviewBridge {
    pub(crate) context: ReactPreviewContext,
    pub(crate) bridge: Option<ReactBridge>,
    pub(crate) failure: Option<String>,
}

impl PreviewWorker {
    pub(crate) fn spawn(mut renderer: GpuRenderer) -> Result<Self, Box<dyn Error>> {
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
                    renderer.set_render_quality(request.render_quality);
                    if request.react_reload != react_reload {
                        react_reload = request.react_reload;
                        react_bridge = None;
                    }
                    let whole_scene = request.react_mode == ReactPreviewMode::WholeScene;
                    renderer.set_record_hit_regions(records_hit_regions(
                        request.react_mode,
                        request.react.as_ref(),
                    ));
                    let mut scene = request.scene;
                    let mut warnings = Vec::new();
                    // In `.tsx` preview mode the React entry is the whole
                    // frame, so its failure is the frame's error; the last
                    // good frame stays on screen.
                    let evaluated = match request.react_mode {
                        ReactPreviewMode::WholeScene => render_whole_react_scene(
                            &mut scene,
                            &mut react_bridge,
                            request.react.as_ref(),
                            &request.prop_overrides,
                        ),
                        ReactPreviewMode::ResolveComponents => {
                            warnings = resolve_preview_components(
                                &mut scene,
                                &mut react_bridge,
                                request.react.as_ref(),
                            );
                            Ok(())
                        }
                    };
                    let frame = evaluated.and_then(|()| {
                        let frame = renderer
                            .render_preview(&scene)
                            .map_err(|error| error_chain(&error))
                            .map(prepare_preview_frame);
                        warnings.extend(renderer.font_fallbacks().iter().map(ToString::to_string));
                        warnings.extend(renderer.missing_glyphs().iter().map(ToString::to_string));
                        frame
                    });
                    let layer_source = request.source_of.filter(|_| whole_scene).map(|id| {
                        let source = look_up_layer_source(&mut react_bridge, scene.time, &id);
                        (id, source)
                    });
                    let hit_regions = renderer.take_hit_regions();
                    let layer_frame = request.frame_of.filter(|_| whole_scene).map(|id| {
                        let measured = layer_frame(&scene, &hit_regions, &id);
                        (id, measured)
                    });
                    if result_tx
                        .send(PreviewResult {
                            generation: request.generation,
                            frame,
                            warnings,
                            hit_regions,
                            layer_source,
                            layer_frame,
                            timeline_frame: request.timeline_frame,
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

    pub(crate) fn request(&self, request: PreviewRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "preview worker stopped unexpectedly".to_owned())
    }
}

/// The JSX chain that produced layer `id` at `time`, from the bridge the
/// frame was just evaluated with. A bridge that is not running answers with
/// why, so the panel does not wait for a lookup that never comes.
fn look_up_layer_source(
    react_bridge: &mut Option<ReactPreviewBridge>,
    time: Time,
    id: &str,
) -> LookupResult {
    let Some(state) = react_bridge.as_mut() else {
        return Err("React runtime is unavailable for this preview".to_owned());
    };
    match &mut state.bridge {
        Some(bridge) => bridge
            .layer_source(time, id)
            .map_err(|error| error.to_string()),
        None => Err(format!(
            "React preview unavailable: {}",
            state.failure.as_deref().unwrap_or("unknown error")
        )),
    }
}

/// Spawns (or reuses) the preview worker's long-lived React connection for
/// `react`. Respawns only when the entry, runtime paths, or property values
/// changed; a spawn
/// failure is remembered on the returned state so callers do not restart Node
/// on every frame.
pub(crate) fn ensure_react_bridge<'a>(
    react_bridge: &'a mut Option<ReactPreviewBridge>,
    react: &ReactPreviewContext,
) -> &'a mut ReactPreviewBridge {
    if react_bridge
        .as_ref()
        .is_none_or(|state| state.context != *react)
    {
        *react_bridge = Some(
            match ReactBridge::spawn_with_options(
                &react.node,
                &react.cli_script,
                &react.entry,
                &react.properties,
                // `layer_source` answers only when the runtime recorded
                // where each JSX element was written.
                SpawnOptions {
                    source_locations: react.source_locations,
                    composition_id: react.composition_id.clone(),
                },
            ) {
                Ok(mut bridge) => {
                    // Whole `.tsx` frames are requested like an export's;
                    // say they are the preview, so `useIsPreview()` is true
                    // and <DebugOverlay>/<DebugBounds> draw.
                    bridge.set_preview(true);
                    ReactPreviewBridge {
                        context: react.clone(),
                        bridge: Some(bridge),
                        failure: None,
                    }
                }
                Err(error) => ReactPreviewBridge {
                    context: react.clone(),
                    bridge: None,
                    failure: Some(error_chain(&error)),
                },
            },
        );
    }
    react_bridge.as_mut().expect("react bridge was just set")
}

/// Replaces the whole preview scene with the standalone React entry's own
/// evaluation at `scene.time` — the editor's synthetic project has no tracks,
/// so this is the only source of layers in `.tsx` preview mode. A bridge
/// failure is remembered, so later frames report the same error without
/// restarting Node, and returned as the frame's error. `prop_overrides` are
/// a drag's values, evaluated with in this frame only.
pub(crate) fn render_whole_react_scene(
    scene: &mut Scene,
    react_bridge: &mut Option<ReactPreviewBridge>,
    react: Option<&ReactPreviewContext>,
    prop_overrides: &[PropOverride],
) -> Result<(), String> {
    let Some(react) = react else {
        return Err("React runtime is unavailable for this preview".to_owned());
    };
    let state = ensure_react_bridge(react_bridge, react);
    match &mut state.bridge {
        Some(bridge) => match bridge.scene_at_with_overrides(scene.time, prop_overrides) {
            Ok(evaluated) => {
                *scene = evaluated;
                Ok(())
            }
            Err(error) => {
                let failure = error_chain(&error);
                state.bridge = None;
                state.failure = Some(failure.clone());
                Err(failure)
            }
        },
        None => Err(state
            .failure
            .clone()
            .unwrap_or_else(|| "the React runtime is unavailable".to_owned())),
    }
}

/// Replaces every `missingComponent` layer in the scene with its registered
/// component's rendered layers (wrapped in the original layer shell so the
/// timeline item's evaluated transform and opacity still place it), dropping
/// unresolved ones from the preview and reporting them as warnings instead —
/// an unrenderable component would otherwise fail the whole GPU render.
pub(crate) fn resolve_preview_components(
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
                    let unresolved = splice_component_resolutions(scene, resolutions);
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
                    state.failure = Some(error_chain(&error));
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

pub(crate) fn append_component_names(
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

pub(crate) fn collect_component_requests(
    layers: &[Layer],
    out: &mut Vec<(String, BTreeMap<String, serde_json::Value>)>,
) {
    for layer in layers {
        match &layer.content {
            LayerContent::MissingComponent { component, props } => {
                out.push((component.clone(), props.clone()));
            }
            LayerContent::Group {
                layers,
                clip: _,
                mask,
                motion_blur: _,
            } => {
                collect_component_requests(layers, out);
                if let Some(mask) = mask {
                    collect_component_requests(&mask.layers, out);
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn strip_missing_components(layers: &mut Vec<Layer>) {
    let mut index = 0;
    while index < layers.len() {
        match &mut layers[index].content {
            LayerContent::Group {
                layers: children,
                clip: _,
                mask,
                motion_blur: _,
            } => {
                strip_missing_components(children);
                if let Some(mask) = mask {
                    strip_missing_components(&mut mask.layers);
                }
            }
            LayerContent::MissingComponent { .. } => {
                layers.remove(index);
                continue;
            }
            _ => {}
        }
        index += 1;
    }
}

/// Places `resolutions` at the scene's missing-component layers (see
/// [`splice_resolved_components`]) and adds the shaders they use to
/// `scene.shaders`, returning the names nothing registered.
pub(crate) fn splice_component_resolutions(
    scene: &mut Scene,
    resolutions: ComponentResolutions,
) -> Vec<String> {
    let mut cursor = 0;
    let mut unresolved = Vec::new();
    splice_resolved_components(
        &mut scene.layers,
        &resolutions.components,
        &mut cursor,
        &mut unresolved,
    );
    merge_shaders(&mut scene.shaders, resolutions.shaders);
    unresolved
}

/// Adds each of `incoming` whose id `shaders` doesn't list yet. An id names
/// its source and parameters, and the GPU renderer rejects a scene that
/// lists one twice.
pub(crate) fn merge_shaders(shaders: &mut Vec<ShaderSource>, incoming: Vec<ShaderSource>) {
    for shader in incoming {
        if !shaders.iter().any(|listed| listed.id == shader.id) {
            shaders.push(shader);
        }
    }
}

/// Walks in the same order [`collect_component_requests`] did, replacing the
/// `cursor`-th missing-component layer with its resolution (a group of the
/// component's own layers inside the original layer shell) or removing it
/// when unresolved.
pub(crate) fn splice_resolved_components(
    layers: &mut Vec<Layer>,
    resolutions: &[Option<Vec<Layer>>],
    cursor: &mut usize,
    unresolved: &mut Vec<String>,
) {
    let mut index = 0;
    while index < layers.len() {
        match &layers[index].content {
            LayerContent::Group { .. } => {
                if let LayerContent::Group {
                    layers: children,
                    clip: _,
                    mask,
                    motion_blur: _,
                } = &mut layers[index].content
                {
                    splice_resolved_components(children, resolutions, cursor, unresolved);
                    if let Some(mask) = mask {
                        splice_resolved_components(
                            &mut mask.layers,
                            resolutions,
                            cursor,
                            unresolved,
                        );
                    }
                }
                index += 1;
            }
            LayerContent::MissingComponent { component, .. } => {
                let resolved = resolutions.get(*cursor).cloned().flatten();
                *cursor += 1;
                match resolved {
                    Some(children) => {
                        layers[index].content = LayerContent::Group {
                            layers: children,
                            clip: None,
                            mask: None,
                            motion_blur: false,
                        };
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
