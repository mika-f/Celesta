use crate::audio::{AudioMixWorker, MasterVolumeDrag};
use crate::component_schema::ComponentSchemaWorker;
use crate::edit_history::EditHistory;
use crate::element_edit::ElementEdit;
use crate::element_rows::ElementRows;
use crate::export_worker::ExportWorker;
use crate::helpers::ExportLoudness;
use crate::layer_select::SourceLookup;
use crate::media_probe::{MediaAssetInfo, MediaProbeWorker};
use crate::meter::MasterLevels;
use crate::occurrences::{OccurrenceState, OccurrenceWorker};
use crate::preview::{AudioPreview, PreviewPresentation, PreviewWorker, ReactPreview};
use crate::problems::{Problems, Source};
use crate::property_panel::PropertyPanelState;
use crate::react_audio::ReactAudioWorker;
use crate::refresh::{ReloadOrder, SelectionKeep};
use crate::source::{PropertyArgs, load_source};
use crate::timeline::LaneGesture;
use crate::transform_drag::TransformDrag;
use celesta_composition::{AudioClip, Rational};
use celesta_editor_core::{AssetSummary, EditorDocument, TimelineClock, TrackSummary};
use celesta_exporter::{ExportCancellation, ExportProgress};
use celesta_gpu_renderer::{GpuDriver, GpuRenderOptions, GpuRenderer, HitRegion, LayerFrame};
use celesta_media::FfmpegBackend;
use celesta_react_bridge::{
    ComponentPropertyField, ComponentPropertySchema, PrimitivePropertyTable,
};
use chrono::{DateTime, Local};
#[cfg(not(target_os = "macos"))]
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::resizable::ResizableState;
use gpui_kit::{Bounds, Entity, FocusHandle, Pixels, ScrollHandle, SharedString};
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

/// Which tab the right dock shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RightTab {
    /// The inspector, or the React Preview panel in React-preview mode.
    #[default]
    Details,
    /// What can be edited in React-preview mode: the selected layer and the
    /// props file. Not offered for a project, whose inspector edits in place.
    Properties,
    Problems,
}

impl RightTab {
    /// The tab `ShowProblems` switches to: Problems, or back to Details from
    /// Problems.
    pub(crate) fn toggled_problems(self) -> Self {
        match self {
            Self::Details | Self::Properties => Self::Problems,
            Self::Problems => Self::Details,
        }
    }

    /// The tabs the right dock offers, in order.
    pub(crate) fn offered(react_preview: bool) -> &'static [Self] {
        if react_preview {
            &[Self::Details, Self::Properties, Self::Problems]
        } else {
            &[Self::Details, Self::Problems]
        }
    }

    /// The tab shown for this one: Properties falls back to Details where it
    /// is not offered (a project was opened while it was selected).
    pub(crate) fn shown(self, react_preview: bool) -> Self {
        if Self::offered(react_preview).contains(&self) {
            self
        } else {
            Self::Details
        }
    }
}

pub(crate) struct EditorView {
    /// The file this view was opened from (`None` for the built-in demo);
    /// File > Reload opens it again.
    pub(crate) source_path: Option<PathBuf>,
    /// Bumped each time another file replaces this view's contents, so the
    /// previous React entry's reload watcher knows to stop.
    pub(crate) session: u64,
    /// A file picked from File > Open… is loading in the background.
    pub(crate) opening: bool,
    /// Every error and warning, and the log since this project opened.
    pub(crate) problems: Problems,
    pub(crate) right_tab: RightTab,
    /// History rows the user expanded, by `LogEntry::id`.
    pub(crate) expanded_log_entries: HashSet<u64>,
    /// Sources whose error or warning toast this window shows; each closes
    /// once its source has no active problem.
    pub(crate) toasted_sources: HashSet<Source>,
    /// The in-window menu bar. macOS shows the same menus natively instead.
    #[cfg(not(target_os = "macos"))]
    pub(crate) app_menu_bar: Option<Entity<AppMenuBar>>,
    pub(crate) document: EditorDocument,
    /// `Some` in standalone React composition preview mode: the document is a
    /// synthetic project and every previewed frame comes from the React
    /// bridge. The asset list, timeline tracks, and inspector have nothing to
    /// show.
    pub(crate) react_preview: Option<ReactPreview>,
    pub(crate) react_audio_worker: ReactAudioWorker,
    /// The standalone React composition's audio clips, from its last audio
    /// sweep, and the timeline tracks they and its videos are laid out on.
    pub(crate) react_audio_clips: Vec<AudioClip>,
    pub(crate) react_tracks: Vec<TrackSummary>,
    /// Bumped on every reload of the React entry (watcher or manual button);
    /// threaded into `PreviewRequest::react_reload` so the preview worker
    /// respawns Node against the freshly re-bundled code.
    pub(crate) react_reload_generation: u64,
    pub(crate) occurrence_worker: OccurrenceWorker,
    /// Where each element of the React entry renders, tagged with the
    /// `react_reload_generation` it was swept for. `None` outside React
    /// preview and while Select layers is off.
    pub(crate) occurrences: Option<(u64, OccurrenceState)>,
    /// The preview frame a sweep held back after the editor's own write
    /// waits for before it starts.
    pub(crate) occurrence_sweep_after: Option<u64>,
    /// Whether a task looks for the running sweep's result (see
    /// `watch_occurrence_sweep`).
    pub(crate) occurrence_watch: bool,
    /// Numbers the React entry's reloads so a slow older one is not applied
    /// over a newer one.
    pub(crate) react_reloads: ReloadOrder,
    /// The next reload number when `ReactPreview::watched_mtime` was last
    /// sampled outside a reload; older reloads' samples are not applied.
    pub(crate) watched_since: u64,
    /// The React Preview panel's Properties section.
    pub(crate) property_panel: PropertyPanelState,
    /// File writes Cmd+Z can take back; kept for as long as the same entry
    /// stays open.
    pub(crate) edit_history: EditHistory,
    pub(crate) preview_worker: PreviewWorker,
    pub(crate) preview_generation: u64,
    /// Generation of the newest preview frame actually shown. Results are
    /// accepted while their generation only moves forward, so a slow render
    /// (a React entry's Node round-trip easily outruns one frame interval)
    /// keeps the preview tracking the playhead a render behind instead of
    /// freezing until every queued frame drains — which looked like a long
    /// delay before playback caught up.
    pub(crate) preview_shown_generation: u64,
    pub(crate) preview_pending: bool,
    pub(crate) media_probe_worker: MediaProbeWorker,
    pub(crate) media_generation: u64,
    pub(crate) media_pending: bool,
    pub(crate) media_cache: HashMap<String, Result<MediaAssetInfo, String>>,
    pub(crate) audio_mix_worker: AudioMixWorker,
    pub(crate) audio_generation: u64,
    pub(crate) audio_pending: bool,
    pub(crate) audio_preview: Option<AudioPreview>,
    /// Peak envelope of the mixed preview audio for the master meter.
    pub(crate) master_levels: Option<MasterLevels>,
    pub(crate) clip_waveforms: HashMap<String, Vec<f32>>,
    pub(crate) clip_levels: HashMap<String, Vec<f32>>,
    pub(crate) clock: TimelineClock,
    pub(crate) frame_rate_value: Rational,
    pub(crate) playing: bool,
    pub(crate) playback_started_at: Option<Instant>,
    pub(crate) playback_started_frame: i64,
    /// Playback wraps from the end (or the Out mark) back to the start (or
    /// the In mark) instead of stopping.
    pub(crate) loop_playback: bool,
    /// Play was pressed while the preview audio was still being prepared;
    /// playback starts once it is ready.
    pub(crate) play_when_audio_ready: bool,
    /// Draws action-safe and title-safe frames over the viewer.
    pub(crate) show_safe_areas: bool,
    /// The React preview's interactive mode: clicking the viewer selects a
    /// layer and shows its JSX source. Off, the preview skips recording
    /// source positions and hit regions, which large scenes pay for.
    pub(crate) interactive: bool,
    pub(crate) scrubbing: bool,
    /// Playback gain for the preview only (0..=2). It scales the mixed
    /// buffer at the output device and never touches the project, so it has
    /// no effect on exports.
    pub(crate) monitor_volume: f64,
    pub(crate) master_volume_drag: Option<MasterVolumeDrag>,
    pub(crate) selected_clip_id: Option<String>,
    pub(crate) selected_asset_id: Option<String>,
    pub(crate) selected_track_id: Option<String>,
    pub(crate) component_schema_worker: ComponentSchemaWorker,
    pub(crate) component_schema_generation: u64,
    pub(crate) component_schema_pending: bool,
    pub(crate) component_schemas: BTreeMap<String, ComponentPropertySchema>,
    pub(crate) project_property_schema: Option<BTreeMap<String, ComponentPropertyField>>,
    pub(crate) project_name: SharedString,
    pub(crate) dimensions: SharedString,
    pub(crate) frame_rate_label: SharedString,
    pub(crate) assets: Vec<AssetSummary>,
    pub(crate) tracks: Vec<TrackSummary>,
    pub(crate) preview: Option<PreviewPresentation>,
    pub(crate) export_worker: ExportWorker,
    pub(crate) export_progress: Option<ExportProgress>,
    pub(crate) export_path: Option<PathBuf>,
    pub(crate) export_cancellation: Option<ExportCancellation>,
    pub(crate) export_cancelling: bool,
    /// Optional export in/out points, in composition frames. `out` is
    /// exclusive (one past the last frame to include). Both set and `in < out`
    /// means "Export…" renders only that span; otherwise the whole
    /// composition is exported.
    pub(crate) export_in_frame: Option<i64>,
    pub(crate) export_out_frame: Option<i64>,
    pub(crate) choosing_export_path: bool,
    /// The loudness Export… brings the mix to, for the rest of the session.
    pub(crate) export_loudness: ExportLoudness,
    pub(crate) gpu_name: SharedString,
    /// The graphics API from `--driver`, kept for projects opened later and
    /// for exports.
    pub(crate) driver: GpuDriver,
    pub(crate) focus_handle: Option<FocusHandle>,
    pub(crate) master_volume_focus: Option<FocusHandle>,
    /// Split positions for the workspace shell: `dock_split` is the
    /// asset-list / monitor / inspector row, `body_split` is the
    /// work-area / timeline column. Held here so the drags persist across
    /// redraws.
    pub(crate) dock_split: Option<Entity<ResizableState>>,
    pub(crate) body_split: Option<Entity<ResizableState>>,
    /// Timeline horizontal zoom (>= 1; 1 = whole composition fits) and the
    /// fraction of the composition at the left edge of the visible window.
    /// The mouse wheel over the timeline adjusts the zoom about the cursor.
    pub(crate) timeline_zoom: f64,
    pub(crate) timeline_view_start: f64,
    /// Middle-button pan of the timeline: `(pointer x at grab, view_start at
    /// grab)`. `Some` while the middle button is held over the timeline.
    pub(crate) timeline_pan: Option<(f32, f64)>,
    /// Swipe scrubbing over the clip lanes: the partial frame carried
    /// between wheel events, and the axis the current trackpad gesture is
    /// locked to.
    pub(crate) timeline_scrub_remainder: f64,
    pub(crate) timeline_lane_gesture: LaneGesture,
    /// The tracks' vertical scroll, moved directly by an Option pan.
    pub(crate) timeline_tracks_scroll: ScrollHandle,
    /// Drag of the overview bar's thumb: `(pointer x at grab, view_start at
    /// grab)`.
    pub(crate) timeline_overview_drag: Option<(f32, f64)>,
    /// Window bounds of the clip lanes and the overview bar as last painted,
    /// for mapping pointer positions onto the composition.
    pub(crate) timeline_lane_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) timeline_overview_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Width of the transport bar as last painted; a narrow viewer drops its
    /// secondary controls (they stay in the menus and on the keyboard).
    pub(crate) transport_width: Rc<Cell<Option<Pixels>>>,
    /// Scroll position of the preview error's detail over the viewer.
    pub(crate) preview_error_scroll: ScrollHandle,
    /// When the preview error that scroll position belongs to was set.
    pub(crate) preview_error_time: Option<DateTime<Local>>,
    /// The React preview layer picked by clicking the viewer, by id; kept
    /// across frames until a click misses or the composition reloads.
    pub(crate) selected_layer: Option<String>,
    /// Where each leaf layer of the shown frame was drawn, for picking.
    pub(crate) hit_regions: Vec<HitRegion>,
    /// The selected layer's JSX source, as the Layer section shows it.
    pub(crate) source_lookup: Option<SourceLookup>,
    /// The timeline frame `source_lookup`'s answer was looked up at.
    pub(crate) source_lookup_frame: i64,
    /// `source_lookup` was answered by the code before an own write's reload;
    /// it stays shown until the reloaded code answers.
    pub(crate) source_lookup_stale: bool,
    /// Window bounds of the viewer's frame area as last painted, for mapping
    /// clicks onto the scene.
    pub(crate) viewer_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Why the last attempt to open a source position failed.
    pub(crate) launch_error: Option<String>,
    /// The Properties tab's "Source chain" disclosure is expanded.
    pub(crate) source_chain_open: bool,
    /// Which reloads keep `selected_layer` (those after the editor's own writes).
    pub(crate) selection_keep: SelectionKeep,
    /// The selected layer's targets in its source.
    pub(crate) element_edit: ElementEdit,
    /// The controls of the selected layer's property rows.
    pub(crate) element_rows: ElementRows,
    /// The selected layer's frame as the shown frame measured it: the layer
    /// id, and `None` inside when it has no affine frame. `None` when the
    /// frame measured nothing (during playback, or with no selection).
    pub(crate) layer_frame: Option<(String, Option<LayerFrame>)>,
    /// The canvas drag of the selected layer's transform, if one is running
    /// or being written.
    pub(crate) transform_drag: Option<TransformDrag>,
}

impl EditorView {
    pub(crate) fn open(
        path: Option<&Path>,
        property_args: PropertyArgs,
        driver: GpuDriver,
    ) -> Result<Self, Box<dyn Error>> {
        let (document, react_preview) = load_source(path, property_args)?;
        Self::from_document(path.map(Path::to_path_buf), document, react_preview, driver)
    }

    pub(crate) fn from_document(
        source_path: Option<PathBuf>,
        document: EditorDocument,
        react_preview: Option<ReactPreview>,
        driver: GpuDriver,
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
        let assets = document.assets();
        let tracks = document.tracks();

        // Without `with_sequential_video` every previewed frame opens its own
        // FFmpeg decode run and seeks from the nearest keyframe, which costs
        // far more than everything else the preview does put together (~150ms
        // vs ~2ms of React evaluation on a 1080p source). Sharing one decode
        // run across the playhead's forward progress is what makes playback
        // track in real time; a backwards seek or a long jump still re-seeks.
        let renderer = GpuRenderer::new(GpuRenderOptions {
            driver,
            ..GpuRenderOptions::default()
        })?
        .with_asset_root(document.asset_root())
        .with_video_decoder(FfmpegBackend::new().with_sequential_video(frame_rate_value));
        let adapter = renderer.adapter_info();
        let gpu_name = format!("{} ({})", adapter.name, adapter.backend).into();
        let preview_worker = PreviewWorker::spawn(renderer)?;
        let media_probe_worker = MediaProbeWorker::spawn()?;
        let audio_mix_worker = AudioMixWorker::spawn()?;
        let export_worker = ExportWorker::spawn()?;
        let component_schema_worker = ComponentSchemaWorker::spawn()?;
        let react_audio_worker = ReactAudioWorker::spawn()?;
        let occurrence_worker = OccurrenceWorker::spawn()?;
        let mut editor = Self {
            source_path,
            session: 0,
            opening: false,
            problems: Problems::default(),
            right_tab: RightTab::Details,
            expanded_log_entries: HashSet::new(),
            toasted_sources: HashSet::new(),
            #[cfg(not(target_os = "macos"))]
            app_menu_bar: None,
            document,
            react_preview,
            react_audio_worker,
            react_audio_clips: Vec::new(),
            react_tracks: Vec::new(),
            react_reload_generation: 0,
            occurrence_worker,
            occurrences: None,
            occurrence_sweep_after: None,
            occurrence_watch: false,
            react_reloads: ReloadOrder::default(),
            watched_since: 0,
            property_panel: PropertyPanelState::default(),
            edit_history: EditHistory::default(),
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
            master_levels: None,
            clip_waveforms: HashMap::new(),
            clip_levels: HashMap::new(),
            clock,
            frame_rate_value,
            playing: false,
            playback_started_at: None,
            playback_started_frame: 0,
            loop_playback: false,
            play_when_audio_ready: false,
            show_safe_areas: false,
            interactive: false,
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
            project_name,
            dimensions,
            frame_rate_label,
            assets,
            tracks,
            preview: None,
            export_worker,
            export_progress: None,
            export_path: None,
            export_cancellation: None,
            export_cancelling: false,
            export_in_frame: None,
            export_out_frame: None,
            choosing_export_path: false,
            export_loudness: ExportLoudness::default(),
            gpu_name,
            driver,
            focus_handle: None,
            master_volume_focus: None,
            dock_split: None,
            body_split: None,
            timeline_zoom: 1.0,
            timeline_view_start: 0.0,
            timeline_pan: None,
            timeline_scrub_remainder: 0.0,
            timeline_lane_gesture: LaneGesture::default(),
            timeline_tracks_scroll: ScrollHandle::new(),
            timeline_overview_drag: None,
            timeline_lane_bounds: Rc::default(),
            timeline_overview_bounds: Rc::default(),
            transport_width: Rc::default(),
            preview_error_scroll: ScrollHandle::new(),
            preview_error_time: None,
            selected_layer: None,
            hit_regions: Vec::new(),
            source_lookup: None,
            source_lookup_frame: 0,
            source_lookup_stale: false,
            viewer_bounds: Rc::default(),
            launch_error: None,
            source_chain_open: false,
            selection_keep: SelectionKeep::default(),
            element_edit: ElementEdit::default(),
            element_rows: ElementRows::default(),
            layer_frame: None,
            transform_drag: None,
        };
        editor.refresh_preview();
        editor.refresh_audio_preview();
        editor.refresh_occurrences(false);
        if editor.react_preview.is_none() {
            editor.refresh_media_cache();
            editor.refresh_component_schemas();
        }
        Ok(editor)
    }

    /// The property table of the primitive `name` (`Rect`, `Star`, ...) as
    /// the React preview last loaded it; `None` for a project, or a name the
    /// runtime declares no table for.
    pub(crate) fn primitive_table(&self, name: &str) -> Option<&PrimitivePropertyTable> {
        self.react_preview.as_ref()?.primitive_table(name)
    }
}

#[cfg(test)]
mod tests {
    use super::RightTab;

    #[test]
    fn show_problems_toggles_between_problems_and_details() {
        assert_eq!(RightTab::Details.toggled_problems(), RightTab::Problems);
        assert_eq!(RightTab::Problems.toggled_problems(), RightTab::Details);
        assert_eq!(RightTab::Properties.toggled_problems(), RightTab::Problems);
    }

    #[test]
    fn properties_tab_is_offered_only_in_react_preview() {
        assert_eq!(
            RightTab::offered(true),
            [RightTab::Details, RightTab::Properties, RightTab::Problems]
        );
        assert_eq!(
            RightTab::offered(false),
            [RightTab::Details, RightTab::Problems]
        );
        assert_eq!(RightTab::Properties.shown(true), RightTab::Properties);
        assert_eq!(RightTab::Properties.shown(false), RightTab::Details);
        assert_eq!(RightTab::Problems.shown(false), RightTab::Problems);
    }
}
