use crate::RenderQuality;
use crate::control::{ExportCancellation, ExportProgress};
use crate::error::ExportError;
use crate::exporter::Exporter;
use crate::options::{BitDepth, ColorConversion, VideoEncoding, VideoFormat};
use crate::project::{absolutize_fonts, absolutize_layers};
use crate::range::ExportWindow;
use crate::react::ReactVideoRequest;
use celesta_composition::{Rational, Time, TimeError};
use celesta_evaluator::{EvaluationError, Evaluator};
use celesta_gpu_renderer::{Color, GpuDriver, GpuRenderOptions, GpuRenderer, ReadbackFormat};
use celesta_media::FfmpegBackend;
use celesta_project::Project;
use celesta_react_bridge::{ProjectFrame, ReactBridge};
use ez_ffmpeg::{FfmpegContext, Input, Output, VideoWriter, capabilities};
use std::collections::{BTreeMap, HashSet};
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
            self.options.video_format,
            self.options.video.color_conversion,
            self.options.video.bit_depth,
            self.options.render_quality,
            self.options.driver,
        )?;
        let mut writer = open_video_writer(
            project.settings.width,
            project.settings.height,
            frame_rate,
            self.options.video_format,
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
                let at_frame = |error: ExportError| error.at_frame(frame_index as u64);
                let time = Time::frames(frame_index, frame_rate)
                    .map_err(ExportError::Time)
                    .map_err(at_frame)?;
                let scene = evaluator
                    .scene_at(time)
                    .map_err(ExportError::Evaluation)
                    .map_err(at_frame)?;
                // `submit` keeps a few frames in flight on the GPU rather
                // than blocking on this frame's readback immediately, so the
                // wait (when there is one) overlaps with evaluating and
                // encoding other frames instead of stalling every frame. A
                // failure is this frame's: preparing its scene is what fails.
                if let Some(frame) = renderer
                    .submit(&scene)
                    .map_err(ExportError::Render)
                    .map_err(at_frame)?
                {
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
            subtitles: react_subtitles,
        } = request;

        let project_evaluator = project
            .map(|(project, _)| Evaluator::new(project))
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
            self.options.video_format,
            self.options.video.color_conversion,
            self.options.video.bit_depth,
            self.options.render_quality,
            self.options.driver,
        )?;
        let mut writer = open_video_writer(
            metadata.width,
            metadata.height,
            metadata.frame_rate,
            self.options.video_format,
            self.options.video,
            renderer.readback_format(),
            output,
        )?;

        // Asks Node for a frame, after evaluating the project layers its
        // entry embeds. The scene is collected later (see the evaluator
        // thread below).
        let submit = |bridge: &mut ReactBridge, offset: u64| -> Result<(), ExportError> {
            let frame_index =
                i64::try_from(start_frame + offset).map_err(|_| ExportError::TimelineTooLong)?;
            let at_frame = |error: ExportError| error.at_frame(start_frame + offset);
            let time = Time::frames(frame_index, metadata.frame_rate)
                .map_err(ExportError::Time)
                .map_err(at_frame)?;
            let project_frame = project_evaluator
                .as_ref()
                .zip(project)
                .map(|(evaluator, (project, _))| -> Result<_, EvaluationError> {
                    let mut scene = evaluator.scene_at(time)?;
                    let mut tracks = BTreeMap::new();
                    for track in &project.tracks {
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
                })
                .transpose()
                .map_err(ExportError::Evaluation)
                .map_err(at_frame)?;
            bridge
                .submit_frame(
                    time,
                    project_frame.as_ref().map(|(layers, tracks)| ProjectFrame {
                        layers: layers.as_slice(),
                        tracks,
                    }),
                )
                .map_err(ExportError::React)
                .map_err(at_frame)
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
                let mut submitted = if frame_count > 0 {
                    submit(bridge, 0)
                } else {
                    Ok(())
                };
                for offset in 0..frame_count {
                    // Frame `offset + 1` is submitted before this one is
                    // decoded, so Node evaluates it while this side parses;
                    // its text measurements are answered once the parse is
                    // done. A failed submission fails that next frame.
                    let at_frame = |error: ExportError| error.at_frame(start_frame + offset);
                    let scene = std::mem::replace(&mut submitted, Ok(())).and_then(|()| {
                        let frame = bridge
                            .receive_frame()
                            .map_err(ExportError::React)
                            .map_err(at_frame)?;
                        if offset + 1 < frame_count {
                            submitted = submit(bridge, offset + 1);
                        }
                        let evaluation = bridge
                            .decode_frame(frame)
                            .map_err(ExportError::React)
                            .map_err(at_frame)?;
                        react_audio.extend(evaluation.audio);
                        react_subtitles.extend(evaluation.subtitles);
                        let mut scene = evaluation.scene;
                        scene.fonts.extend(project_fonts.iter().cloned());
                        Ok(scene)
                    });
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
                if let Some(frame) = renderer
                    .submit(&scene?)
                    .map_err(ExportError::Render)
                    .map_err(|error| error.at_frame(start_frame + offset))?
                {
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
    /// and encoding the format's audio codec — the library-linked equivalent
    /// of a second `ffmpeg -i video -f f32le -i pcm -c:v copy -c:a <codec>`
    /// pass: AAC 192k for MP4, 24-bit PCM for MOV, and Opus 192k at 48 kHz
    /// for WebM. A looping MP4 encodes [`loop_lead_in`] ahead of the mix.
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
        // A looping MP4's AAC encoder first takes the end of the mix, which
        // the edit list then hides as it hides the encoder's own priming.
        let lead_in = if self.options.looping && self.options.video_format == VideoFormat::Mp4 {
            loop_lead_in(audio)
        } else {
            Vec::new()
        };
        for sample in lead_in.iter().chain(&audio.samples) {
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

        // No `-shortest`: the mix already spans the video. With a copied WebM
        // video it cut the audio a frame short, and it lets the muxer finish
        // while the audio encoder still sends, which ez-ffmpeg can report as
        // "Muxer already finished".
        let muxed = Output::from(path_to_url(output))
            .add_stream_map_with_copy("0:v:0")
            .add_stream_map("1:a:0");
        // The lead-in's timestamps go before 0, which the MP4 muxer turns
        // into the edit list.
        let muxed = if lead_in.is_empty() {
            muxed
        } else {
            let frames = lead_in.len() / usize::from(audio.channels);
            muxed.set_audio_filter(format!("asetpts=PTS-{frames}/SR/TB"))
        };
        let context = FfmpegContext::builder()
            .input(Input::from(path_to_url(video)))
            .input(
                Input::from(path_to_url(&pcm_path))
                    .set_format("f32le")
                    .set_format_opt("sample_rate", audio.sample_rate.to_string())
                    .set_format_opt("ch_layout", format!("{}c", audio.channels)),
            )
            .output(match self.options.video_format {
                VideoFormat::Mp4 => muxed
                    .set_audio_codec("aac")
                    .set_audio_codec_opt("b", "192k")
                    .set_format_opt("movflags", "+faststart"),
                VideoFormat::Mov => muxed
                    .set_audio_codec("pcm_s24le")
                    .set_format_opt("movflags", "+faststart"),
                // libopus takes 48, 24, 16, 12 or 8 kHz only, so a 44.1 kHz
                // mix is resampled.
                VideoFormat::Webm => muxed
                    .set_audio_codec("libopus")
                    .set_audio_codec_opt("b", "192k")
                    .set_audio_sample_rate(48_000),
            })
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

/// Sample frames of the mix's end a looping MP4 encodes ahead of its start:
/// sixteen AAC frames. The encoder settles its bit allocation over its first
/// frames: after four, a loud first hit still came out worse than with no
/// lead-in at all, and from sixteen on as cleanly as the same hit later.
pub(crate) const LOOP_LEAD_IN_FRAMES: usize = 16 * 1024;

/// The [`LOOP_LEAD_IN_FRAMES`] sample frames that precede the mix's start
/// when it plays in a loop: its end, repeated as many times as a short mix
/// needs.
pub(crate) fn loop_lead_in(audio: &celesta_media::AudioBuffer) -> Vec<f32> {
    let channels = usize::from(audio.channels);
    let frames = audio.frame_count();
    if frames == 0 {
        return Vec::new();
    }
    (0..LOOP_LEAD_IN_FRAMES)
        .flat_map(|frame| {
            let source = (frames - LOOP_LEAD_IN_FRAMES % frames + frame) % frames * channels;
            audio.samples[source..source + channels].iter().copied()
        })
        .collect()
}

pub(crate) fn validate_output(
    output: &Path,
    format: VideoFormat,
    overwrite: bool,
) -> Result<(), ExportError> {
    if VideoFormat::from_extension(output) != Some(format) {
        return Err(ExportError::UnsupportedOutput {
            path: output.to_owned(),
            format,
        });
    }
    if !overwrite && output.exists() {
        return Err(ExportError::OutputExists(output.to_owned()));
    }
    Ok(())
}

/// Fails when the linked FFmpeg lacks an encoder `format` needs, before any
/// frame is rendered.
pub(crate) fn ensure_encoders(format: VideoFormat) -> Result<(), ExportError> {
    match format
        .encoders()
        .into_iter()
        .find(|encoder| !capabilities::is_encoder_available(encoder))
    {
        Some(encoder) => Err(ExportError::EncoderUnavailable { format, encoder }),
        None => Ok(()),
    }
}

pub(crate) fn path_to_url(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The renderer an export draws its frames with. For an alpha `format` it
/// clears to transparent and reads back straight-alpha 16-bit RGBA, which the
/// encoder converts; for MP4 it clears to the default opaque background and
/// reads back yuv420p (or yuv420p10le for `bit_depth` 10) when
/// `color_conversion` puts that conversion on the GPU, else RGBA of enough
/// bits for `bit_depth`.
pub(crate) fn export_renderer(
    asset_root: &Path,
    frame_rate: Rational,
    format: VideoFormat,
    color_conversion: ColorConversion,
    bit_depth: BitDepth,
    render_quality: RenderQuality,
    driver: GpuDriver,
) -> Result<GpuRenderer, ExportError> {
    let background = if format.has_alpha() {
        Color::TRANSPARENT
    } else {
        GpuRenderOptions::default().background
    };
    let mut renderer = GpuRenderer::new(GpuRenderOptions { background, driver })
        .map_err(ExportError::Render)?
        .with_render_quality(render_quality)
        .with_asset_root(asset_root)
        .with_video_decoder(FfmpegBackend::new().with_sequential_video(frame_rate));
    let readback = if format.has_alpha() {
        ReadbackFormat::Rgba16Straight
    } else {
        let on_gpu = match color_conversion {
            ColorConversion::Auto => {
                renderer.supports_yuv420p_readback() && !renderer.is_software()
            }
            ColorConversion::Gpu => true,
            ColorConversion::Encoder => false,
        };
        match (on_gpu, bit_depth) {
            (true, BitDepth::Eight) => ReadbackFormat::Yuv420p,
            (true, BitDepth::Ten) => ReadbackFormat::Yuv420p10,
            (false, BitDepth::Eight) => ReadbackFormat::Rgba8,
            (false, BitDepth::Ten) => ReadbackFormat::Rgba16,
        }
    };
    renderer
        .set_readback_format(readback)
        .map_err(ExportError::Render)?;
    Ok(renderer)
}

/// Opens a constant-frame-rate `VideoWriter` for pushed frames in `input`
/// layout — the library-linked equivalent of piping `rawvideo` into
///
/// - `Mp4`: `ffmpeg -c:v libx264 -preset <preset> -crf <crf> -pix_fmt yuv420p -movflags +faststart`
///   (`-preset medium -crf 18` by default), with `-pix_fmt yuv420p10le`
///   (High 10) for 10-bit output. Input already in the output's pixel format
///   reaches the encoder without a conversion.
/// - `Mov`: `ffmpeg -vf <BT709_FILTER> -c:v prores_ks -profile:v 4444 -alpha_bits 16 -vendor apl0 -pix_fmt yuva444p10le`
/// - `Webm`: `ffmpeg -vf <BT709_FILTER> -c:v libvpx-vp9 -crf 20 -b:v 0 -row-mt 1 -pix_fmt yuva420p`
///
/// The alpha formats take straight-alpha `rgba64le`, which libswscale
/// converts with the BT.709 matrix in limited range (see [`BT709_FILTER`]);
/// `encoding` applies to `Mp4` only.
pub(crate) fn open_video_writer(
    width: u32,
    height: u32,
    frame_rate: Rational,
    format: VideoFormat,
    encoding: VideoEncoding,
    input: ReadbackFormat,
    output: &Path,
) -> Result<VideoWriter, ExportError> {
    if format == VideoFormat::Mp4 && encoding.crf > VideoEncoding::MAX_CRF {
        return Err(ExportError::InvalidCrf(encoding.crf));
    }
    let fps_num = i32::try_from(frame_rate.numerator).map_err(|_| ExportError::TimelineTooLong)?;
    let fps_den =
        i32::try_from(frame_rate.denominator).map_err(|_| ExportError::TimelineTooLong)?;
    let output = Output::from(path_to_url(output));
    let output = match format {
        VideoFormat::Mp4 => output
            .set_video_codec("libx264")
            .set_video_codec_opt("preset", encoding.preset.as_str())
            .set_video_codec_opt("crf", encoding.crf.to_string())
            .set_pix_fmt(encoding.bit_depth.pixel_format())
            .set_format_opt("movflags", "+faststart"),
        // No faststart here: this is the video-only intermediate, and `mux_audio`
        // sets it on the final file.
        VideoFormat::Mov => output
            .set_video_codec("prores_ks")
            .set_video_codec_opt("profile", "4444")
            .set_video_codec_opt("alpha_bits", "16")
            .set_video_codec_opt("vendor", "apl0")
            .set_pix_fmt("yuva444p10le")
            .set_video_filter(BT709_FILTER),
        VideoFormat::Webm => output
            .set_video_codec("libvpx-vp9")
            .set_video_codec_opt("crf", "20")
            .set_video_codec_opt("b", "0")
            .set_video_codec_opt("row-mt", "1")
            .set_pix_fmt("yuva420p")
            .set_video_filter(BT709_FILTER),
    };
    VideoWriter::builder(width, height)
        .pixel_format(match input {
            ReadbackFormat::Rgba8 => "rgba",
            ReadbackFormat::Rgba16 => "rgba64le",
            ReadbackFormat::Rgba16Straight => "rgba64le",
            ReadbackFormat::Yuv420p => "yuv420p",
            ReadbackFormat::Yuv420p10 => "yuv420p10le",
        })
        .fps(fps_num, fps_den)
        .open(output)
        .map_err(|source| ExportError::Ffmpeg {
            stage: "video encoding",
            source,
        })
}

/// Converts RGB to YUV with the BT.709 matrix in limited range and tags the
/// frames BT.709 (matrix, range, primaries and transfer), since editing
/// software assumes BT.709 for untagged HD material. The tags go on the
/// frames because the encoder copies its color properties from the first
/// frame after applying its options, so `colorspace`, `color_primaries` and
/// `color_trc` encoder options would not take effect.
const BT709_FILTER: &str =
    "scale=out_color_matrix=bt709:out_range=tv,setparams=color_primaries=bt709:color_trc=bt709";

/// The font warnings an export has already reported, so a warning that holds
/// on every frame is reported once.
#[derive(Default)]
pub(crate) struct ReportedFontWarnings {
    /// Families (or font stacks) and weights with no face.
    pub(crate) fallbacks: HashSet<(Vec<String>, u16)>,
    /// Characters that a family (or font stack) and weight has no glyph for.
    pub(crate) missing_glyphs: HashSet<(Vec<String>, u16, char)>,
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
            .insert((fallback.families.clone(), fallback.weight))
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
                reported.missing_glyphs.insert((
                    missing.families.clone(),
                    missing.weight,
                    character,
                ))
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
