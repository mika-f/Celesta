use crate::component_schema::ComponentSchemaRequest;
use crate::helpers::format_time;
use crate::media_probe::{MediaProbeRequest, ProbeAsset};
use crate::preview::{PreviewRequest, ReactPreviewContext, ReactPreviewMode};
use crate::source::{REACT_PREVIEW_SAMPLE_RATE, newest_source_mtime};
use crate::timecode::format_timecode;
use crate::view::EditorView;
use celesta_composition::Time;
use celesta_editor_core::{EditorDocument, TimelineClock};
use celesta_gpu_renderer::RenderQuality;
use celesta_project::AssetKind;
use celesta_react_bridge::{
    ReactBridge, ReactCompositionMetadata, runtime_paths as react_runtime_paths,
};
use gpui_kit::Context;
use std::path::Path;
use std::time::Duration;

impl EditorView {
    /// `HH:MM:SS:FF` for a composition time, rounded to the nearest frame.
    pub(crate) fn timecode_for(&self, time: Time) -> String {
        self.clock.frame_for_time(time).map_or_else(
            |_| format_time(time),
            |frame| format_timecode(frame, self.frame_rate_value),
        )
    }

    pub(crate) fn is_react_preview(&self) -> bool {
        self.react_preview.is_some()
    }

    pub(crate) fn current_time(&self) -> Time {
        self.clock.time().unwrap_or(Time::ZERO)
    }

    pub(crate) fn refresh_preview(&mut self) {
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
                    render_quality: if self.playing {
                        RenderQuality::Draft
                    } else {
                        RenderQuality::Final
                    },
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
    pub(crate) fn request_react_reload(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn apply_react_reload(
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
    pub(crate) fn watch_react_entry(&self, cx: &mut Context<Self>) {
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

    pub(crate) fn refresh_media_cache(&mut self) {
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
    pub(crate) fn refresh_component_schemas(&mut self) {
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
}
