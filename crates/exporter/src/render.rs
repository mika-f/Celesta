use crate::RenderQuality;
use crate::control::{ExportCancellation, ExportProgress};
use crate::error::ExportError;
use crate::exporter::Exporter;
use crate::options::{ColorConversion, VideoEncoding};
use crate::project::{absolutize_fonts, absolutize_layers, visual_only_project};
use crate::range::ExportWindow;
use crate::react::ReactVideoRequest;
use celesta_composition::{Rational, Scene, Time, TimeError};
use celesta_evaluator::{EvaluationError, Evaluator};
use celesta_gpu_renderer::{GpuDriver, GpuRenderOptions, GpuRenderer, ReadbackFormat};
use celesta_media::FfmpegBackend;
use celesta_project::Project;
use celesta_react_bridge::ProjectFrame;
use ez_ffmpeg::{FfmpegContext, Input, Output, VideoWriter};
use std::collections::{BTreeMap, HashSet};
use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::{fs, io, thread};

impl Exporter {
    pub(crate) fn render_video(
        &self,
        project: &Project,
        asset_root: &Path,
        window: ExportWindow,
        output: &Path,
        cancellation: &ExportCancellation,
        progress: &mut impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let ExportWindow {
            start_frame,
            frames: frame_count,
            ..
        } = window;
        let frame_rate = project.settings.frame_rate;
        let mut renderer = export_renderer(
            asset_root,
            frame_rate,
            self.options.video.color_conversion,
            self.options.render_quality,
            self.options.driver,
        )?;
        let mut writer = open_video_writer(
            project.settings.width,
            project.settings.height,
            frame_rate,
            self.options.video,
            renderer.readback_format(),
            output,
        )?;

        let mut reported_fallbacks = ReportedFontWarnings::default();
        let result = (|| {
            let evaluator = Evaluator::new(project).map_err(ExportError::Evaluation)?;
            for frame_index in 0..frame_count {
                ensure_not_cancelled(cancellation)?;
                progress(ExportProgress::Rendering {
                    frame: frame_index + 1,
                    total: frame_count,
                });
                let frame_index = i64::try_from(frame_index)
                    .ok()
                    .and_then(|offset: i64| offset.checked_add(start_frame))
                    .ok_or(ExportError::TimelineTooLong)?;
                let time = Time::frames(frame_index, frame_rate).map_err(ExportError::Time)?;
                let scene = evaluator.scene_at(time).map_err(ExportError::Evaluation)?;
                // `submit` keeps a few frames in flight on the GPU rather
                // than blocking on this frame's readback immediately, so the
                // wait (when there is one) overlaps with evaluating and
                // encoding other frames instead of stalling every frame.
                if let Some(frame) = renderer.submit(&scene).map_err(ExportError::Render)? {
                    write_frame(&mut writer, frame)?;
                }
                report_font_fallbacks(&renderer, &mut reported_fallbacks, progress);
            }
            for frame in renderer.drain().map_err(ExportError::Render)? {
                write_frame(&mut writer, frame)?;
            }
            Ok(())
        })();
        finish_encode(writer, result)
    }

    pub(crate) fn render_react_video(
        &self,
        request: ReactVideoRequest<'_>,
        output: &Path,
        cancellation: &ExportCancellation,
        progress: &mut impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let ReactVideoRequest {
            bridge,
            metadata,
            start_frame,
            frame_count,
            asset_root,
            project,
            audio: react_audio,
        } = request;

        let filtered_project = project.map(|(project, _)| visual_only_project(project));
        let project_evaluator = filtered_project
            .as_ref()
            .map(Evaluator::new)
            .transpose()
            .map_err(ExportError::Evaluation)?;
        let project_asset_root = project.map(|(_, asset_root)| asset_root);
        let project_fonts = project_evaluator
            .as_ref()
            .map(|evaluator| -> Result<_, EvaluationError> {
                let mut fonts = evaluator.scene_at(Time::ZERO)?.fonts;
                if let Some(asset_root) = project_asset_root {
                    absolutize_fonts(&mut fonts, asset_root);
                }
                Ok(fonts)
            })
            .transpose()
            .map_err(ExportError::Evaluation)?
            .unwrap_or_default();

        // The video decoder is attached unconditionally: the React entry's
        // own <Video> elements need decoding just as much as a companion
        // project's Video content does, and the React entry's asset_root
        // stays the renderer's single asset_root either way (see
        // absolutize_layers/absolutize_fonts below for how a project's own,
        // differently-rooted assets still resolve).
        let mut renderer = export_renderer(
            asset_root,
            metadata.frame_rate,
            self.options.video.color_conversion,
            self.options.render_quality,
            self.options.driver,
        )?;
        let mut writer = open_video_writer(
            metadata.width,
            metadata.height,
            metadata.frame_rate,
            self.options.video,
            renderer.readback_format(),
            output,
        )?;

        let evaluate = |offset: u64| -> Result<Scene, ExportError> {
            let frame_index =
                i64::try_from(start_frame + offset).map_err(|_| ExportError::TimelineTooLong)?;
            let time = Time::frames(frame_index, metadata.frame_rate).map_err(ExportError::Time)?;
            let project_frame = project_evaluator
                .as_ref()
                .zip(filtered_project.as_ref())
                .map(
                    |(evaluator, filtered_project)| -> Result<_, EvaluationError> {
                        let mut scene = evaluator.scene_at(time)?;
                        let mut tracks = BTreeMap::new();
                        for track in &filtered_project.tracks {
                            let mut layers = evaluator.layers_for_track(&track.id, time)?;
                            if let Some(asset_root) = project_asset_root {
                                absolutize_layers(&mut layers, asset_root);
                            }
                            tracks.insert(track.id.clone(), layers);
                        }
                        if let Some(asset_root) = project_asset_root {
                            absolutize_layers(&mut scene.layers, asset_root);
                        }
                        Ok((scene.layers, tracks))
                    },
                )
                .transpose()
                .map_err(ExportError::Evaluation)?;
            let evaluation = bridge
                .evaluate_at(
                    time,
                    project_frame.as_ref().map(|(layers, tracks)| ProjectFrame {
                        layers: layers.as_slice(),
                        tracks,
                    }),
                )
                .map_err(ExportError::React)?;
            react_audio.extend(evaluation.audio);
            let mut scene = evaluation.scene;
            scene.fonts.extend(project_fonts.iter().cloned());
            Ok(scene)
        };

        let mut reported_fallbacks = ReportedFontWarnings::default();
        let result = thread::scope(|scope| {
            // Node evaluates (and this side parses) the next frame on its own
            // thread while this one prepares and submits the current frame;
            // both take milliseconds of CPU per frame and would otherwise
            // run back to back. Frames arrive in order. The channel holds
            // none, so the evaluator waits with its frame until this thread
            // takes it and stays at most one frame ahead. Leaving early
            // drops `frames`, which stops the evaluator at its next send.
            let (sender, frames) = mpsc::sync_channel(0);
            scope.spawn(move || {
                let mut evaluate = evaluate;
                for offset in 0..frame_count {
                    let scene = evaluate(offset);
                    let failed = scene.is_err();
                    if sender.send(scene).is_err() || failed {
                        break;
                    }
                }
            });
            for offset in 0..frame_count {
                ensure_not_cancelled(cancellation)?;
                progress(ExportProgress::Rendering {
                    frame: offset + 1,
                    total: frame_count,
                });
                // Disconnected only if the evaluator panicked, which the
                // scope re-raises once it joins.
                let Ok(scene) = frames.recv() else {
                    break;
                };
                // See render_video's matching comment: submit overlaps this
                // frame's GPU work with evaluating and encoding other frames
                // instead of blocking here.
                if let Some(frame) = renderer.submit(&scene?).map_err(ExportError::Render)? {
                    write_frame(&mut writer, frame)?;
                }
                report_font_fallbacks(&renderer, &mut reported_fallbacks, progress);
            }
            for frame in renderer.drain().map_err(ExportError::Render)? {
                write_frame(&mut writer, frame)?;
            }
            Ok(())
        });
        finish_encode(writer, result)
    }

    /// Muxes the encoded, audio-less `video` with the mixed `audio` (staged as
    /// a raw `f32le` PCM sidecar file) into `output`, stream-copying the video
    /// and encoding AAC — the library-linked equivalent of a second
    /// `ffmpeg -i video -f f32le -i pcm -c:v copy -c:a aac` pass.
    pub(crate) fn mux_audio(
        &self,
        video: &Path,
        output: &Path,
        audio: &celesta_media::AudioBuffer,
        cancellation: &ExportCancellation,
    ) -> Result<(), ExportError> {
        ensure_not_cancelled(cancellation)?;
        let pcm_path = video.with_extension("pcm");
        let mut pcm =
            io::BufWriter::new(
                fs::File::create(&pcm_path).map_err(|source| ExportError::Io {
                    operation: "create mixed audio sidecar",
                    source,
                })?,
            );
        for sample in &audio.samples {
            pcm.write_all(&sample.to_le_bytes())
                .map_err(|source| ExportError::Io {
                    operation: "stage mixed audio",
                    source,
                })?;
        }
        pcm.into_inner()
            .map_err(|error| ExportError::Io {
                operation: "flush mixed audio",
                source: error.into_error(),
            })?
            .sync_all()
            .map_err(|source| ExportError::Io {
                operation: "flush mixed audio",
                source,
            })?;
        ensure_not_cancelled(cancellation)?;

        let context = FfmpegContext::builder()
            .input(Input::from(path_to_url(video)))
            .input(
                Input::from(path_to_url(&pcm_path))
                    .set_format("f32le")
                    .set_format_opt("sample_rate", audio.sample_rate.to_string())
                    .set_format_opt("ch_layout", format!("{}c", audio.channels)),
            )
            .output(
                Output::from(path_to_url(output))
                    .add_stream_map_with_copy("0:v:0")
                    .add_stream_map("1:a:0")
                    .set_audio_codec("aac")
                    .set_audio_codec_opt("b", "192k")
                    .set_shortest(true)
                    .set_format_opt("movflags", "+faststart"),
            )
            .build()
            .map_err(|source| ExportError::Ffmpeg {
                stage: "audio muxing",
                source,
            })?;
        let result = context
            .start()
            .and_then(|running| running.wait())
            .map_err(|source| ExportError::Ffmpeg {
                stage: "audio muxing",
                source,
            });
        let _ = fs::remove_file(&pcm_path);
        result
    }
}

pub(crate) fn validate_output(output: &Path, overwrite: bool) -> Result<(), ExportError> {
    if output.extension() != Some(OsStr::new("mp4")) {
        return Err(ExportError::UnsupportedOutput(output.to_owned()));
    }
    if !overwrite && output.exists() {
        return Err(ExportError::OutputExists(output.to_owned()));
    }
    Ok(())
}

pub(crate) fn path_to_url(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The renderer an export draws its frames with, reading them back as
/// yuv420p when `color_conversion` puts that conversion on the GPU.
pub(crate) fn export_renderer(
    asset_root: &Path,
    frame_rate: Rational,
    color_conversion: ColorConversion,
    render_quality: RenderQuality,
    driver: GpuDriver,
) -> Result<GpuRenderer, ExportError> {
    let mut renderer = GpuRenderer::new(GpuRenderOptions {
        driver,
        ..GpuRenderOptions::default()
    })
    .map_err(ExportError::Render)?
    .with_render_quality(render_quality)
    .with_asset_root(asset_root)
    .with_video_decoder(FfmpegBackend::new().with_sequential_video(frame_rate));
    let on_gpu = match color_conversion {
        ColorConversion::Auto => renderer.supports_yuv420p_readback() && !renderer.is_software(),
        ColorConversion::Gpu => true,
        ColorConversion::Encoder => false,
    };
    if on_gpu {
        renderer
            .set_readback_format(ReadbackFormat::Yuv420p)
            .map_err(ExportError::Render)?;
    }
    Ok(renderer)
}

/// Opens a constant-frame-rate H.264 `VideoWriter` for pushed frames in
/// `input` layout — the library-linked equivalent of piping `rawvideo` into
/// `ffmpeg -c:v libx264 -preset <preset> -crf <crf> -pix_fmt yuv420p -movflags +faststart`
/// (`-preset medium -crf 18` by default). yuv420p input reaches the encoder
/// without a conversion.
pub(crate) fn open_video_writer(
    width: u32,
    height: u32,
    frame_rate: Rational,
    encoding: VideoEncoding,
    input: ReadbackFormat,
    output: &Path,
) -> Result<VideoWriter, ExportError> {
    if encoding.crf > VideoEncoding::MAX_CRF {
        return Err(ExportError::InvalidCrf(encoding.crf));
    }
    let fps_num = i32::try_from(frame_rate.numerator).map_err(|_| ExportError::TimelineTooLong)?;
    let fps_den =
        i32::try_from(frame_rate.denominator).map_err(|_| ExportError::TimelineTooLong)?;
    VideoWriter::builder(width, height)
        .pixel_format(match input {
            ReadbackFormat::Rgba8 => "rgba",
            ReadbackFormat::Yuv420p => "yuv420p",
        })
        .fps(fps_num, fps_den)
        .open(
            Output::from(path_to_url(output))
                .set_video_codec("libx264")
                .set_video_codec_opt("preset", encoding.preset.as_str())
                .set_video_codec_opt("crf", encoding.crf.to_string())
                .set_pix_fmt("yuv420p")
                .set_format_opt("movflags", "+faststart"),
        )
        .map_err(|source| ExportError::Ffmpeg {
            stage: "video encoding",
            source,
        })
}

/// The font warnings an export has already reported, so a warning that holds
/// on every frame is reported once.
#[derive(Default)]
pub(crate) struct ReportedFontWarnings {
    /// Families and weights with no face.
    pub(crate) fallbacks: HashSet<(String, u16)>,
    /// Characters that a family and weight has no glyph for.
    pub(crate) missing_glyphs: HashSet<(String, u16, char)>,
}

/// Reports each font family and weight the frame `renderer` last submitted
/// draws with a fallback font, and each character it draws with a fallback
/// font because its family has no glyph for it, unless an earlier frame
/// already did.
pub(crate) fn report_font_fallbacks(
    renderer: &GpuRenderer,
    reported: &mut ReportedFontWarnings,
    progress: &mut impl FnMut(ExportProgress),
) {
    for fallback in renderer.font_fallbacks() {
        if reported
            .fallbacks
            .insert((fallback.family.clone(), fallback.weight))
        {
            progress(ExportProgress::Warning(fallback.to_string()));
        }
    }
    for missing in renderer.missing_glyphs() {
        // Only the characters no earlier warning named: text that changes
        // over time (subtitles) still reports each character once.
        let characters = missing
            .characters
            .iter()
            .copied()
            .filter(|&character| {
                reported
                    .missing_glyphs
                    .insert((missing.family.clone(), missing.weight, character))
            })
            .collect::<Vec<_>>();
        if !characters.is_empty() {
            let mut missing = missing.clone();
            missing.characters = characters;
            progress(ExportProgress::Warning(missing.to_string()));
        }
    }
}

/// Hands the frame's pixel buffer to the encoder as is (`write_owned`), rather
/// than having `write` copy all of it first.
pub(crate) fn write_frame(
    writer: &mut VideoWriter,
    frame: celesta_gpu_renderer::GpuFrame,
) -> Result<(), ExportError> {
    writer
        .write_owned(frame.into_pixels())
        .map_err(|error| ExportError::Ffmpeg {
            stage: "video encoding",
            source: error.into_parts().1.into(),
        })
}

/// Finalizes an encode: `finish()` on success (writes the container trailer),
/// `abort()` on any earlier failure (dropping the writer would also abort, but
/// this is explicit and drains the worker).
pub(crate) fn finish_encode(
    writer: VideoWriter,
    result: Result<(), ExportError>,
) -> Result<(), ExportError> {
    match result {
        Ok(()) => writer.finish().map_err(|source| ExportError::Ffmpeg {
            stage: "video encoding",
            source,
        }),
        Err(error) => {
            writer.abort();
            Err(error)
        }
    }
}

pub(crate) fn ensure_not_cancelled(cancellation: &ExportCancellation) -> Result<(), ExportError> {
    if cancellation.is_cancelled() {
        return Err(ExportError::Cancelled);
    }
    Ok(())
}

pub(crate) fn validate_dimensions(width: u32, height: u32) -> Result<(), ExportError> {
    if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return Err(ExportError::UnsupportedDimensions { width, height });
    }
    Ok(())
}

/// Largest frame index whose start time is `<= time` (i.e. `time` snapped
/// down to a frame boundary). Negative times snap to frame 0.
pub(crate) fn frame_floor(time: Time, frame_rate: Rational) -> Result<i64, ExportError> {
    if !time.is_valid() {
        return Err(ExportError::Time(TimeError::ZeroTimescale));
    }
    if !frame_rate.is_valid() {
        return Err(ExportError::Time(TimeError::InvalidFrameRate));
    }
    let numerator = i128::from(time.value.max(0)) * i128::from(frame_rate.numerator);
    let denominator = i128::from(time.timescale) * i128::from(frame_rate.denominator);
    i64::try_from(numerator / denominator).map_err(|_| ExportError::TimelineTooLong)
}

pub(crate) fn frame_count(duration: Time, frame_rate: Rational) -> Result<u64, ExportError> {
    if !duration.is_valid() {
        return Err(ExportError::Time(TimeError::ZeroTimescale));
    }
    if !frame_rate.is_valid() {
        return Err(ExportError::Time(TimeError::InvalidFrameRate));
    }
    let numerator = i128::from(duration.value.max(0)) * i128::from(frame_rate.numerator);
    let denominator = i128::from(duration.timescale) * i128::from(frame_rate.denominator);
    let frames = numerator
        .checked_add(denominator - 1)
        .map(|value| value / denominator)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(ExportError::TimelineTooLong)?;
    Ok(frames)
}
