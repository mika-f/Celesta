use crate::audio::take_latest;
use crate::helpers::prepare_preview_frame;
use crate::layer_select::LookupResult;
use crate::problems::error_chain;
use crate::source::PropertyArgs;
use celesta_composition::{
    Layer, LayerContent, Scene, ShaderSource, TextColorRun, TextFontRun, Time,
};
use celesta_gpu_renderer::{
    GpuRenderer, HitRegion, LayerFrame, RenderQuality, TextLayout, layer_frame, parent_space,
};
use celesta_react_bridge::{
    ComponentPropertyField, ComponentPropertySchema, ComponentResolutionRequest,
    ComponentResolutions, PrepareDependency, PrimitivePropertyTable, PropOverride, PropertyInputs,
    PropertyIssue, ReactBridge, ReactChapter, ReactCompositionMetadata, SpawnOptions,
};
use gpui_kit::RenderImage;
use rodio::Player;
use rodio::buffer::SamplesBuffer;
use std::collections::BTreeMap;
use std::ops::Range;
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
    /// The group whose children space to report from this frame's scene
    /// (`WholeScene` only); `None` asks for none.
    pub(crate) space_of: Option<String>,
    /// The parent an element was just added under, whose added layer to find
    /// in this frame's scene (`WholeScene` only); `None` asks for none.
    pub(crate) added_under: Option<AddedUnder>,
    /// The timeline frame `scene` is for, echoed in the result.
    pub(crate) timeline_frame: i64,
    /// The text edited on the canvas, drawn in place of its layer's text;
    /// `None` draws the scene as evaluated.
    pub(crate) text_override: Option<TextOverride>,
    /// Render the scene the runtime last evaluated instead of evaluating again.
    pub(crate) reuse_scene: bool,
}

/// Where an element was just written: as the last child of the group layer
/// `parent`, inside a `<Sequence>` of its own when `wrapped`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AddedUnder {
    pub(crate) parent: String,
    pub(crate) wrapped: bool,
}

/// The added layer: `under.parent`'s last child, or that child's first child
/// when `wrapped`. `None` when the scene has no such group, it has no
/// children, or a wrapped addition's last child is not a group with one.
pub(crate) fn added_layer(scene: &Scene, under: &AddedUnder) -> Option<String> {
    let LayerContent::Group { layers, .. } = &find_layer(&scene.layers, &under.parent)?.content
    else {
        return None;
    };
    let last = layers.last()?;
    if !under.wrapped {
        return Some(last.id.clone());
    }
    let LayerContent::Group { layers, .. } = &last.content else {
        return None;
    };
    layers.first().map(|layer| layer.id.clone())
}

/// A text layer's text as edited on the canvas, drawn without asking the
/// runtime: `layer`'s text becomes `text`, and its runs, which index
/// `original` (the text the edit started from), move with the edit.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextOverride {
    pub(crate) layer: String,
    pub(crate) original: String,
    pub(crate) text: String,
}

/// What the held scene was evaluated for: the request's time, its
/// `react_reload`, and its React preview.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HeldKey {
    time: Time,
    react_reload: u64,
    react: Option<ReactPreviewContext>,
}

impl HeldKey {
    pub(crate) fn of(request: &PreviewRequest) -> Self {
        Self {
            time: request.scene.time,
            react_reload: request.react_reload,
            react: request.react.clone(),
        }
    }
}

/// Where a frame's scene comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SceneSource {
    /// The scene the runtime last evaluated, held by the worker.
    Held,
    /// A new evaluation, which the worker then holds.
    Evaluate,
}

/// Held only when asked to reuse and the held scene's key matches the
/// request's: a keystroke right after a seek or a reload draws the new
/// time's and the new code's scene.
pub(crate) fn scene_source(request: &PreviewRequest, held: Option<&HeldKey>) -> SceneSource {
    if request.reuse_scene && held == Some(&HeldKey::of(request)) {
        SceneSource::Held
    } else {
        SceneSource::Evaluate
    }
}

/// Replaces layer `layer`'s text (through groups and masks): its runs carried
/// through the edit (`shift_runs`), `visible_characters` cleared, so all of
/// the text shows. False when there is no such text layer.
pub(crate) fn replace_text(scene: &mut Scene, edit: &TextOverride) -> bool {
    let Some(layer) = find_layer_mut(&mut scene.layers, &edit.layer) else {
        return false;
    };
    let LayerContent::Text { text, style, .. } = &mut layer.content else {
        return false;
    };
    style.color_runs = shift_runs(
        &style.color_runs,
        |run| run.start..run.end,
        |run, span| TextColorRun {
            start: span.start,
            end: span.end,
            ..run.clone()
        },
        &edit.original,
        &edit.text,
    );
    style.font_runs = shift_runs(
        &style.font_runs,
        |run| run.start..run.end,
        |run, span| TextFontRun {
            start: span.start,
            end: span.end,
            ..run.clone()
        },
        &edit.original,
        &edit.text,
    );
    style.visible_characters = None;
    text.clone_from(&edit.text);
    true
}

/// Layer `id` among `layers`, their groups' children and their masks'
/// layers, in the order the renderer finds it.
fn find_layer_mut<'a>(layers: &'a mut [Layer], id: &str) -> Option<&'a mut Layer> {
    for layer in layers {
        if layer.id == id {
            return Some(layer);
        }
        if let LayerContent::Group { layers, mask, .. } = &mut layer.content {
            if let Some(found) = find_layer_mut(layers, id) {
                return Some(found);
            }
            if let Some(found) = mask
                .as_mut()
                .and_then(|mask| find_layer_mut(&mut mask.layers, id))
            {
                return Some(found);
            }
        }
    }
    None
}

/// Layer `id`, found as `find_layer_mut` finds it.
fn find_layer<'a>(layers: &'a [Layer], id: &str) -> Option<&'a Layer> {
    for layer in layers {
        if layer.id == id {
            return Some(layer);
        }
        if let LayerContent::Group { layers, mask, .. } = &layer.content {
            if let Some(found) = find_layer(layers, id) {
                return Some(found);
            }
            if let Some(found) = mask.as_ref().and_then(|mask| find_layer(&mask.layers, id)) {
                return Some(found);
            }
        }
    }
    None
}

/// Runs over `original` moved onto `edited` by their common prefix and
/// suffix, in code points: an index in the prefix stays, one in the suffix
/// moves by the length change, and a run's end inside the changed span is
/// cut to the span's start and its start to the span's end. So a run keeps
/// its own characters, stretches over text typed inside it, and is dropped
/// when the edit empties it; the runs stay ordered and apart, within
/// `edited`.
pub(crate) fn shift_runs<R: Clone>(
    runs: &[R],
    span: impl Fn(&R) -> Range<usize>,
    with: impl Fn(&R, Range<usize>) -> R,
    original: &str,
    edited: &str,
) -> Vec<R> {
    let original_len = original.chars().count();
    let edited_len = edited.chars().count();
    let prefix = original
        .chars()
        .zip(edited.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = original
        .chars()
        .rev()
        .zip(edited.chars().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(original_len.min(edited_len) - prefix);
    // The changed span: `prefix..original_len - suffix` of `original`,
    // `prefix..edited_len - suffix` of `edited`.
    let old_end = original_len - suffix;
    let new_end = edited_len - suffix;
    let shift = |index: usize| index - old_end + new_end;
    let start_of = |index: usize| {
        if index >= old_end {
            shift(index)
        } else if index <= prefix {
            index
        } else {
            new_end
        }
    };
    let end_of = |index: usize| {
        if index <= prefix {
            index
        } else if index >= old_end {
            shift(index)
        } else {
            prefix
        }
    };
    runs.iter()
        .filter_map(|run| {
            let Range { start, end } = span(run);
            let moved = start_of(start).min(edited_len)..end_of(end).min(edited_len);
            (moved.start < moved.end).then(|| with(run, moved))
        })
        .collect()
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
    /// The request's `space_of` and the map from that group's children space
    /// to scene pixels (see `parent_space`): `None` inside when the frame has
    /// no such group or it turns in 3D. The map may collapse the plane (a
    /// group scaled to zero), so it may have no inverse. `None` when nothing
    /// was asked.
    pub(crate) parent_space: Option<(String, Option<[f64; 6]>)>,
    /// The request's `added_under` parent and the layer `added_layer` found
    /// under it: `None` inside when there is none. `None` when nothing was
    /// asked.
    pub(crate) added: Option<(String, Option<String>)>,
    /// The layout of the request's `text_override` layer as drawn in this
    /// frame; `None` without an override, or when the frame failed or has
    /// no such text layer.
    pub(crate) text_layout: Option<TextLayout>,
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
    /// What `registerComponent()` declared, keyed by component name, as last
    /// loaded; kept across failed reloads. It types the Component section.
    pub(crate) component_schemas: BTreeMap<String, ComponentPropertySchema>,
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
    pub(crate) fn spawn(mut renderer: GpuRenderer) -> anyhow::Result<Self> {
        let (request_tx, request_rx) = mpsc::channel::<PreviewRequest>();
        let (result_tx, result_rx) = mpsc::channel::<PreviewResult>();
        thread::Builder::new()
            .name("celesta-preview".to_owned())
            .spawn(move || {
                let mut react_bridge: Option<ReactPreviewBridge> = None;
                let mut react_reload = 0u64;
                // The scene the runtime last evaluated, and what for: a
                // keystroke's request draws it again with its text
                // replaced, without evaluating.
                let mut held: Option<(HeldKey, Scene)> = None;
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
                    let time = request.scene.time;
                    let source = scene_source(&request, held.as_ref().map(|(key, _)| key));
                    let key = HeldKey::of(&request);
                    let mut warnings = Vec::new();
                    let evaluated = match source {
                        SceneSource::Held => {
                            Ok(&held.as_ref().expect("scene_source found it held").1)
                        }
                        SceneSource::Evaluate => {
                            // A failed evaluation is not held, so no
                            // keystroke draws a scene the runtime refused.
                            held = None;
                            let mut scene = request.scene;
                            // In `.tsx` preview mode the React entry is the
                            // whole frame, so its failure is the frame's
                            // error; the last good frame stays on screen.
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
                            match evaluated {
                                Ok(()) => Ok(&held.insert((key, scene)).1),
                                Err(error) => Err(error),
                            }
                        }
                    };
                    // The edited text is drawn on a copy: the held scene
                    // stays as the runtime evaluated it.
                    let edited = match (&evaluated, &request.text_override) {
                        (Ok(scene), Some(edit)) => {
                            let mut scene = (*scene).clone();
                            replace_text(&mut scene, edit).then_some(scene)
                        }
                        _ => None,
                    };
                    let drawn = evaluated.map(|scene| edited.as_ref().unwrap_or(scene));
                    let frame = drawn.clone().and_then(|scene| {
                        let frame = renderer
                            .render_preview(scene)
                            .map_err(|error| error_chain(&error))
                            .map(prepare_preview_frame);
                        warnings.extend(renderer.font_fallbacks().iter().map(ToString::to_string));
                        warnings.extend(renderer.missing_glyphs().iter().map(ToString::to_string));
                        frame
                    });
                    // Measured after the render, which loaded its fonts.
                    let text_layout = request
                        .text_override
                        .as_ref()
                        .zip(drawn.as_ref().ok().filter(|_| frame.is_ok()))
                        .and_then(|(edit, scene)| renderer.text_layout(scene, &edit.layer));
                    let layer_source = request.source_of.filter(|_| whole_scene).map(|id| {
                        let time = drawn.as_ref().map_or(time, |scene| scene.time);
                        let source = look_up_layer_source(&mut react_bridge, time, &id);
                        (id, source)
                    });
                    let hit_regions = renderer.take_hit_regions();
                    let layer_frame = request.frame_of.filter(|_| whole_scene).map(|id| {
                        let measured = drawn
                            .as_ref()
                            .ok()
                            .and_then(|scene| layer_frame(scene, &hit_regions, &id));
                        (id, measured)
                    });
                    let children_space = request.space_of.filter(|_| whole_scene).map(|id| {
                        let space = drawn
                            .as_ref()
                            .ok()
                            .and_then(|scene| parent_space(scene, &id));
                        (id, space)
                    });
                    let added = request.added_under.filter(|_| whole_scene).map(|under| {
                        let layer = drawn
                            .as_ref()
                            .ok()
                            .and_then(|scene| added_layer(scene, &under));
                        (under.parent, layer)
                    });
                    if result_tx
                        .send(PreviewResult {
                            generation: request.generation,
                            frame,
                            warnings,
                            hit_regions,
                            layer_source,
                            layer_frame,
                            parent_space: children_space,
                            added,
                            text_layout,
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

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_composition::{EvaluatedTransform, GroupMask, MaskMode, Rational};

    fn leaf(id: &str) -> Layer {
        Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform::default(),
            opacity: 1.0,
            blend_mode: Default::default(),
            effects: Default::default(),
            content: LayerContent::MissingComponent {
                component: id.to_owned(),
                props: Default::default(),
            },
        }
    }

    fn group(id: &str, layers: Vec<Layer>, mask: Option<Vec<Layer>>) -> Layer {
        Layer {
            content: LayerContent::Group {
                layers,
                clip: None,
                mask: mask.map(|layers| GroupMask {
                    layers,
                    mode: MaskMode::Alpha,
                    invert: false,
                }),
                motion_blur: false,
            },
            ..leaf(id)
        }
    }

    fn scene_of(layers: Vec<Layer>) -> Scene {
        Scene {
            width: 16,
            height: 16,
            frame_rate: Rational::new(30, 1),
            time: Time::new(0, 30),
            fonts: Vec::new(),
            shaders: Vec::new(),
            layers,
        }
    }

    /// Two instances of a `.map()` under the root group, the second with the
    /// explicit id `card`; each takes a mask (the group's child 0) and three
    /// children, the last of `card`'s being `last`.
    fn two_instances(last: Layer) -> Scene {
        scene_of(vec![group(
            "0",
            vec![
                group(
                    "0.0",
                    vec![leaf("0.0.1"), leaf("0.0.2"), leaf("0.0.3")],
                    Some(vec![leaf("0.0.0.0")]),
                ),
                group(
                    "card",
                    vec![leaf("0.1.1"), leaf("0.1.2"), last],
                    Some(vec![leaf("0.1.0.0")]),
                ),
            ],
            None,
        )])
    }

    fn under(parent: &str, wrapped: bool) -> AddedUnder {
        AddedUnder {
            parent: parent.to_owned(),
            wrapped,
        }
    }

    #[test]
    fn the_added_layer_is_the_named_parent_s_last_child() {
        let scene = two_instances(leaf("0.1.3"));
        assert_eq!(
            added_layer(&scene, &under("card", false)),
            Some("0.1.3".to_owned())
        );
        assert_eq!(
            added_layer(&scene, &under("0.0", false)),
            Some("0.0.3".to_owned())
        );
    }

    #[test]
    fn a_wrapped_addition_is_the_sequence_s_child() {
        let scene = two_instances(group("0.1.3", vec![leaf("0.1.3.0")], None));
        assert_eq!(
            added_layer(&scene, &under("card", true)),
            Some("0.1.3.0".to_owned())
        );
        // A last child that is not a group holds nothing it wrapped.
        let unwrapped = two_instances(leaf("0.1.3"));
        assert_eq!(added_layer(&unwrapped, &under("card", true)), None);
    }

    #[test]
    fn a_missing_parent_adds_nothing() {
        let scene = two_instances(leaf("0.1.3"));
        assert_eq!(added_layer(&scene, &under("gone", false)), None);
        // A leaf has no children to add after.
        assert_eq!(added_layer(&scene, &under("0.1.2", false)), None);
    }
}
