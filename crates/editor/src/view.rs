use crate::audio::{AudioMixWorker, MasterVolumeDrag};
use crate::component_schema::ComponentSchemaWorker;
use crate::export_worker::ExportWorker;
use crate::media_probe::{MediaAssetInfo, MediaProbeWorker};
use crate::meter::MasterLevels;
use crate::preview::{AudioPreview, PreviewPresentation, PreviewWorker, ReactPreview};
use crate::react_audio::ReactAudioWorker;
use crate::source::{PropertyArgs, load_source};
use celesta_composition::Rational;
use celesta_editor_core::{AssetSummary, EditorDocument, TimelineClock, TrackSummary};
use celesta_exporter::{ExportCancellation, ExportProgress};
use celesta_gpu_renderer::{GpuDriver, GpuRenderOptions, GpuRenderer};
use celesta_media::FfmpegBackend;
use celesta_react_bridge::{ComponentPropertyField, ComponentPropertySchema};
#[cfg(not(target_os = "macos"))]
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::resizable::ResizableState;
use gpui_kit::{Bounds, Entity, FocusHandle, Pixels, SharedString};
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

pub(crate) struct EditorView {
    /// The file this view was opened from (`None` for the built-in demo);
    /// File > Reload opens it again.
    pub(crate) source_path: Option<PathBuf>,
    /// Bumped each time another file replaces this view's contents, so the
    /// previous React entry's reload watcher knows to stop.
    pub(crate) session: u64,
    /// A file picked from File > Open… is loading in the background.
    pub(crate) opening: bool,
    pub(crate) open_error: Option<SharedString>,
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
    /// Bumped on every reload of the React entry (watcher or manual button);
    /// threaded into `PreviewRequest::react_reload` so the preview worker
    /// respawns Node against the freshly re-bundled code.
    pub(crate) react_reload_generation: u64,
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
    pub(crate) component_schema_error: Option<SharedString>,
    pub(crate) project_name: SharedString,
    pub(crate) dimensions: SharedString,
    pub(crate) frame_rate_label: SharedString,
    pub(crate) assets: Vec<AssetSummary>,
    pub(crate) tracks: Vec<TrackSummary>,
    pub(crate) preview: Option<PreviewPresentation>,
    pub(crate) preview_error: Option<SharedString>,
    pub(crate) preview_warnings: Vec<SharedString>,
    pub(crate) media_error: Option<SharedString>,
    pub(crate) audio_error: Option<SharedString>,
    pub(crate) export_worker: ExportWorker,
    pub(crate) export_progress: Option<ExportProgress>,
    pub(crate) export_path: Option<PathBuf>,
    pub(crate) export_cancellation: Option<ExportCancellation>,
    pub(crate) export_cancelling: bool,
    pub(crate) export_error: Option<SharedString>,
    pub(crate) export_message: Option<SharedString>,
    pub(crate) typescript_error: Option<SharedString>,
    pub(crate) typescript_message: Option<SharedString>,
    /// Optional export in/out points, in composition frames. `out` is
    /// exclusive (one past the last frame to include). Both set and `in < out`
    /// means "Export…" renders only that span; otherwise the whole
    /// composition is exported.
    pub(crate) export_in_frame: Option<i64>,
    pub(crate) export_out_frame: Option<i64>,
    pub(crate) choosing_export_path: bool,
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
            driver,
            focus_handle: None,
            master_volume_focus: None,
            dock_split: None,
            body_split: None,
            timeline_zoom: 1.0,
            timeline_view_start: 0.0,
            timeline_pan: None,
            timeline_overview_drag: None,
            timeline_lane_bounds: Rc::default(),
            timeline_overview_bounds: Rc::default(),
            transport_width: Rc::default(),
        };
        editor.refresh_preview();
        editor.refresh_audio_preview();
        if editor.react_preview.is_none() {
            editor.refresh_media_cache();
            editor.refresh_component_schemas();
        }
        Ok(editor)
    }
}
