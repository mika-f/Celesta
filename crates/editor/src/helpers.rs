use crate::preview::{CpuPreviewFrame, PreviewPresentation};
use celesta_composition::{Rational, Time};
use celesta_editor_core::ClipKind;
use celesta_exporter::{ExportProgress, ExportRange};
use celesta_gpu_renderer::PreviewFrame as GpuPreviewFrame;
use gpui_kit::RenderImage;
use image::{Frame, ImageBuffer, Rgba};
use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;

pub(crate) fn prepare_preview_frame(frame: GpuPreviewFrame) -> PreviewPresentation {
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
        ExportProgress::Rendering { frame: 0, total: 0 } => "Starting export…".to_owned(),
        ExportProgress::Rendering { frame, total } => {
            format!("Exporting frame {frame}/{total}")
        }
        ExportProgress::MixingAudio => "Mixing export audio…".to_owned(),
        ExportProgress::Muxing => "Muxing MP4…".to_owned(),
        ExportProgress::Warning(warning) => warning.clone(),
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

pub(crate) fn export_suggested_name(path: Option<&Path>, project_name: &str) -> String {
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
