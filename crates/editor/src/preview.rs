use crate::audio::take_latest;
use crate::helpers::prepare_preview_frame;
use celesta_composition::{Layer, LayerContent, Scene};
use celesta_gpu_renderer::{GpuRenderer, RenderQuality};
use celesta_react_bridge::{ComponentResolutionRequest, ReactBridge};
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
}

pub(crate) struct PreviewResult {
    pub(crate) generation: u64,
    pub(crate) frame: Result<PreviewPresentation, String>,
    /// Recoverable diagnostics for this frame: unresolved `registerComponent`
    /// names (hidden from the preview) and React runtime failures.
    pub(crate) warnings: Vec<String>,
}

/// The Node.js runtime a preview's `TimelineContent::Component` clips resolve
/// against — the same paths `ComponentSchemaWorker` uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReactPreviewContext {
    pub(crate) node: PathBuf,
    pub(crate) cli_script: PathBuf,
    pub(crate) entry: PathBuf,
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
    pub(crate) entry: PathBuf,
    /// Newest source-file modification time seen under the entry's directory.
    /// The reload watcher compares against this to notice code edits.
    pub(crate) watched_mtime: Option<SystemTime>,
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
                    let mut scene = request.scene;
                    let mut warnings = match request.react_mode {
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
                    warnings.extend(renderer.font_fallbacks().iter().map(ToString::to_string));
                    warnings.extend(renderer.missing_glyphs().iter().map(ToString::to_string));
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

    pub(crate) fn request(&self, request: PreviewRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "preview worker stopped unexpectedly".to_owned())
    }
}

/// Spawns (or reuses) the preview worker's long-lived React connection for
/// `react`. Respawns only when the entry or runtime paths changed; a spawn
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
pub(crate) fn render_whole_react_scene(
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
