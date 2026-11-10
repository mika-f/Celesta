use crate::control::{ExportCancellation, ExportProgress};
use crate::encode::{ExportSession, readback_of};
use crate::error::ExportError;
use crate::exporter::Exporter;
use crate::project::{absolutize_fonts, absolutize_layers};
use crate::range::ExportWindow;
use crate::react::ReactVideoRequest;
use celesta_composition::{Rational, Time, TimeError};
use celesta_evaluator::{EvaluationError, Evaluator};
use celesta_gpu_renderer::{Color, GpuRenderOptions, GpuRenderer, NoticeLevel};
use celesta_media::MediaBackend;
use celesta_project::Project;
use celesta_react_bridge::{ProjectFrame, ReactBridge};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::mpsc;
use std::thread;

impl Exporter {
    /// Renders `window`'s frames of `project` with `renderer` and encodes
    /// each through `session`, calling `after_frame` once each frame is
    /// submitted (the project's mix is polled there).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_video(
        &self,
        project: &Project,
        window: ExportWindow,
        renderer: &mut GpuRenderer,
        session: &mut ExportSession,
        cancellation: &ExportCancellation,
        progress: &mut impl FnMut(ExportProgress),
        after_frame: &mut dyn FnMut(&mut ExportSession) -> Result<(), ExportError>,
    ) -> Result<(), ExportError> {
        let ExportWindow {
            start_frame,
            frames: frame_count,
            ..
        } = window;
        let frame_rate = project.settings.frame_rate;
        session
            .set_first_frame(u64::try_from(start_frame).map_err(|_| ExportError::TimelineTooLong)?);
        let mut reported_warnings = ReportedWarnings::default();
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
            // `submit` keeps a few frames in flight on the GPU rather than
            // blocking on this frame's readback immediately, so the wait
            // (when there is one) overlaps with evaluating and encoding
            // other frames instead of stalling every frame. A failure is
            // this frame's: preparing its scene is what fails.
            if let Some(frame) = renderer
                .submit(&scene)
                .map_err(ExportError::Render)
                .map_err(at_frame)?
            {
                session.encode_frame(frame)?;
            }
            report_render_warnings(renderer, &mut reported_warnings, progress);
            after_frame(session)?;
        }
        for frame in renderer.drain().map_err(ExportError::Render)? {
            session.encode_frame(frame)?;
        }
        Ok(())
    }

    /// Renders a React entry's frames with `renderer` and encodes each
    /// through `session`.
    pub(crate) fn render_react_video(
        &self,
        request: ReactVideoRequest<'_>,
        renderer: &mut GpuRenderer,
        session: &mut ExportSession,
        cancellation: &ExportCancellation,
        progress: &mut impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let ReactVideoRequest {
            bridge,
            metadata,
            start_frame,
            frame_count,
            project,
            audio: react_audio,
            subtitles: react_subtitles,
        } = request;
        session.set_first_frame(start_frame);

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

        let mut reported_warnings = ReportedWarnings::default();
        thread::scope(|scope| {
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
                    session.encode_frame(frame)?;
                }
                report_render_warnings(renderer, &mut reported_warnings, progress);
            }
            for frame in renderer.drain().map_err(ExportError::Render)? {
                session.encode_frame(frame)?;
            }
            Ok(())
        })
    }
}

/// Fails when `output` exists and is not to be replaced.
pub(crate) fn validate_output(output: &Path, overwrite: bool) -> Result<(), ExportError> {
    if !overwrite && output.exists() {
        return Err(ExportError::OutputExists(output.to_owned()));
    }
    Ok(())
}

impl Exporter {
    /// The renderer an export draws its frames with, at the options' render
    /// quality, on their driver, with their plugins, whose processes (and
    /// its video decoder's) `cancellation` kills. It clears to the default
    /// opaque background and reads back RGBA8 until [`open_video_output`]
    /// sets what the encoder takes.
    pub(crate) fn export_renderer(
        &self,
        asset_root: &Path,
        frame_rate: Rational,
        cancellation: &ExportCancellation,
    ) -> Result<GpuRenderer, ExportError> {
        let driver = self.options.driver;
        let renderer = GpuRenderer::new(GpuRenderOptions {
            driver,
            ..GpuRenderOptions::default()
        })
        .map_err(ExportError::Render)?
        .with_render_quality(self.options.render_quality)
        .with_asset_root(asset_root)
        .with_video_decoder(
            MediaBackend::new()
                .with_sequential_video(frame_rate)
                .with_kill_switch(cancellation.plugins()),
        )
        // Plugin content comes from the plugins' own processes.
        .with_content_provider(
            self.plugins()
                .host()
                .with_kill_switch(cancellation.plugins()),
        );
        Ok(renderer)
    }
}

/// Opens `session`'s video encoder for `width`x`height` frames at
/// `frame_rate`, offering what `renderer` can read back, and sets the
/// renderer to the encoder's answer: its input format, and a transparent
/// background when the encoder keeps alpha, else the default opaque one.
/// The muxer writes `staged`.
pub(crate) fn open_video_output(
    session: &mut ExportSession,
    renderer: &mut GpuRenderer,
    width: u32,
    height: u32,
    frame_rate: Rational,
    staged: &Path,
) -> Result<(), ExportError> {
    let offered = ExportSession::offered_formats(renderer);
    let opened = session.open_video(width, height, frame_rate, &offered, staged)?;
    renderer
        .set_readback_format(readback_of(opened.input))
        .map_err(ExportError::Render)?;
    renderer.set_background(if opened.alpha {
        Color::TRANSPARENT
    } else {
        GpuRenderOptions::default().background
    });
    Ok(())
}

/// The render warnings an export has already reported, so a warning that
/// holds on every frame is reported once.
#[derive(Default)]
pub(crate) struct ReportedWarnings {
    /// Families (or font stacks) and weights with no face.
    pub(crate) fallbacks: HashSet<(Vec<String>, u16)>,
    /// Characters that a family (or font stack) and weight has no glyph for.
    pub(crate) missing_glyphs: HashSet<(Vec<String>, u16, char)>,
    /// Plugins and what they said about the models they opened.
    pub(crate) plugin_notices: HashSet<(String, String)>,
}

/// Reports each font family and weight the frame `renderer` last submitted
/// draws with a fallback font, each character it draws with a fallback font
/// because its family has no glyph for it, and each notice a plugin gave
/// for a model the frame draws (a warning, or a note for an info notice),
/// unless an earlier frame already did.
pub(crate) fn report_render_warnings(
    renderer: &GpuRenderer,
    reported: &mut ReportedWarnings,
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
    for notice in renderer.plugin_notices() {
        if reported
            .plugin_notices
            .insert((notice.plugin.clone(), notice.message.clone()))
        {
            // An info notice's text is labelled `note: ` already.
            progress(match notice.level {
                NoticeLevel::Info => ExportProgress::Note(notice.to_string()),
                NoticeLevel::Warning => ExportProgress::Warning(notice.to_string()),
            });
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
