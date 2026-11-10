use crate::preview::{CpuPreviewFrame, PreviewPresentation};
use crate::view::EditorView;
use celesta_composition::{Rational, Time};
use celesta_editor_core::ClipKind;
use celesta_exporter::{AudioLoudness, ExportProgress, ExportRange, LoudnessTarget};
use celesta_gpu_renderer::PreviewFrame as GpuPreviewFrame;
use celesta_react_bridge::import_path;
use gpui_kit::{App, Context, PathPromptOptions, RenderImage};
use image::{Frame, ImageBuffer, Rgba};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

pub(crate) fn prepare_preview_frame(frame: GpuPreviewFrame) -> PreviewPresentation {
    match frame {
        GpuPreviewFrame::Cpu(frame) => {
            let width = frame.width();
            let height = frame.height();
            let mut pixels = frame.into_pixels();
            for pixel in pixels.as_chunks_mut::<4>().0 {
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

pub(crate) fn frame_to_image(frame: CpuPreviewFrame) -> Arc<RenderImage> {
    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(frame.width, frame.height, frame.pixels)
        .expect("GPU preview frame dimensions match its pixel buffer");
    Arc::new(RenderImage::new([Frame::new(buffer)]))
}

pub(crate) fn clip_kind_label(kind: ClipKind) -> &'static str {
    match kind {
        ClipKind::Video => "Video",
        ClipKind::Audio => "Audio",
        ClipKind::Image => "Image",
        ClipKind::Text => "Text",
        ClipKind::Dialogue => "Dialogue",
        ClipKind::Component => "Component",
    }
}

pub(crate) fn format_time(time: Time) -> String {
    let total = time.as_seconds().unwrap_or(0.0).max(0.0);
    let hours = (total / 3600.0).floor() as u64;
    let minutes = ((total % 3600.0) / 60.0).floor() as u64;
    let seconds = total % 60.0;
    format!("{hours:02}:{minutes:02}:{seconds:06.3}")
}

pub(crate) fn export_progress_label(progress: &ExportProgress) -> String {
    match progress {
        ExportProgress::Rendering { frame: 0, total: 0 }
        | ExportProgress::Composition(_)
        | ExportProgress::Output(_)
        | ExportProgress::ReactCompositions { .. } => "Starting export…".to_owned(),
        ExportProgress::Rendering { frame, total } => {
            format!("Exporting frame {frame}/{total}")
        }
        ExportProgress::MixingAudio => "Mixing export audio…".to_owned(),
        ExportProgress::Muxing | ExportProgress::Wrote(_) => "Muxing…".to_owned(),
        ExportProgress::Warning(message) | ExportProgress::Note(message) => message.clone(),
    }
}

/// The loudness an export reports for its mix, as `-16.2 LUFS, -1.0 dBTP`.
pub(crate) fn export_loudness_label(loudness: &AudioLoudness) -> String {
    match (loudness.integrated_lufs, loudness.true_peak_dbtp) {
        (Some(lufs), Some(peak)) => format!("{lufs:.1} LUFS, {peak:.1} dBTP"),
        (None, Some(peak)) => format!("too short to measure LUFS, {peak:.1} dBTP"),
        (_, None) => "silent".to_owned(),
    }
}

/// The loudness the next export brings its mix to, picked next to Export…
/// and kept for the rest of the session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExportLoudness {
    /// Leave the mix as it is (it is still measured).
    #[default]
    Off,
    Streaming,
    Broadcast,
}

impl ExportLoudness {
    pub(crate) const ALL: [Self; 3] = [Self::Off, Self::Streaming, Self::Broadcast];

    pub(crate) fn target(self) -> Option<LoudnessTarget> {
        match self {
            Self::Off => None,
            Self::Streaming => Some(LoudnessTarget::STREAMING),
            Self::Broadcast => Some(LoudnessTarget::BROADCAST),
        }
    }

    /// The short name the title bar button shows.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Streaming => "Streaming",
            Self::Broadcast => "Broadcast",
        }
    }

    /// The menu item: the name and, for a target, its loudness.
    pub(crate) fn menu_label(self) -> String {
        match self.target() {
            Some(target) => format!("{} ({} LUFS)", self.name(), target.integrated_lufs),
            None => self.name().to_owned(),
        }
    }
}

/// `[start, end)` frames looped playback repeats: the In/Out marks clamped to
/// the composition (a React reload can shorten it under them) when they
/// still enclose at least a frame, else the whole composition.
pub(crate) fn loop_range_for(
    in_frame: Option<i64>,
    out_frame: Option<i64>,
    end_frame: i64,
) -> (i64, i64) {
    let clamp = |frame: i64| frame.clamp(0, end_frame);
    match (in_frame.map(clamp), out_frame.map(clamp)) {
        (Some(start), Some(end)) if start < end => (start, end),
        _ => (0, end_frame),
    }
}

/// Turns the editor's in/out frame markers into an [`ExportRange`]. Returns
/// `None` (export the whole composition) unless both are set with `in < out`.
pub(crate) fn export_range_for(
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

/// The save panel's first name: the document's (or else the project's)
/// name with `extension`, the chosen muxer's first.
pub(crate) fn export_suggested_name(
    path: Option<&Path>,
    project_name: &str,
    extension: &str,
) -> String {
    let name = path
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .unwrap_or(project_name);
    let stem = name
        .strip_suffix(".celesta.json")
        .or_else(|| name.strip_suffix(".json"))
        .unwrap_or(name);
    format!("{stem}.{extension}")
}

/// The file to export to: a path with one of the chosen muxer's
/// `extensions` (ignoring case) is kept, anything else takes the first.
pub(crate) fn export_output(mut path: PathBuf, extensions: &[impl AsRef<str>]) -> PathBuf {
    let kept = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            extensions
                .iter()
                .any(|offered| offered.as_ref().eq_ignore_ascii_case(extension))
        });
    if let (false, Some(first)) = (kept, extensions.first()) {
        path.set_extension(first.as_ref());
    }
    path
}

/// `path` with its `.` and `..` components taken out, without reading the
/// file system. A `..` with nothing left to take off stays (`../x` is
/// `../x`, `a/../../x` is `../x`, and on Windows `C:..\\x` stays
/// drive-relative); one right after a root goes, since nothing is above it.
pub(crate) fn lexically_normalized(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) => {}
                Some(Component::Prefix(_) | Component::ParentDir | Component::CurDir) | None => {
                    out.push(component)
                }
            },
            other => out.push(other),
        }
    }
    out
}

/// What a single-path dialog asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathPick {
    File,
    Folder,
}

/// Shows the platform dialog for one file or folder, its button labeled
/// `prompt`. Resolves to `Ok(None)` when it is dismissed (or goes away
/// unanswered) and to `Err` when it could not be shown.
pub(crate) fn pick_one_path(
    cx: &App,
    pick: PathPick,
    prompt: &str,
) -> impl Future<Output = anyhow::Result<Option<PathBuf>>> + use<> {
    let selection = cx.prompt_for_paths(PathPromptOptions {
        files: pick == PathPick::File,
        directories: pick == PathPick::Folder,
        multiple: false,
        prompt: Some(prompt.to_owned().into()),
    });
    async move {
        match selection.await {
            Ok(Ok(paths)) => Ok(paths.and_then(|paths| paths.into_iter().next())),
            Ok(Err(error)) => Err(error),
            Err(_) => Ok(None),
        }
    }
}

impl EditorView {
    /// `Choose…` on a path row: asks for a file, imports it into the
    /// project ([`import_path`]: a file under `project_root` is referenced
    /// in place, any other is copied into its assets), and hands `done` the
    /// path relative to `relative_to`, or the message saying why there is
    /// none. Nothing happens when the dialog is dismissed, or when `current`
    /// says the row the press came from is gone (another file opened,
    /// another layer was picked).
    pub(crate) fn choose_imported_path(
        &mut self,
        project_root: PathBuf,
        relative_to: PathBuf,
        current: impl Fn(&EditorView) -> bool + 'static,
        done: impl FnOnce(&mut EditorView, Result<String, String>, &mut Context<EditorView>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let selection = pick_one_path(cx, PathPick::File, "Choose");
        cx.spawn(async move |view, cx| {
            let chosen = match selection.await {
                Ok(Some(chosen)) => chosen,
                Ok(None) => return,
                Err(error) => {
                    let message = format!("Couldn’t show the file dialog: {error}");
                    view.update(cx, |this, cx| {
                        if current(this) {
                            done(this, Err(message), cx);
                        }
                    })
                    .ok();
                    return;
                }
            };
            let imported = cx
                .background_executor()
                .spawn(async move { import_path(&project_root, &relative_to, &chosen) })
                .await
                .map_err(|error| format!("Couldn’t use the chosen file: {error}"));
            view.update(cx, |this, cx| {
                if current(this) {
                    done(this, imported, cx);
                }
            })
            .ok();
        })
        .detach();
    }
}
