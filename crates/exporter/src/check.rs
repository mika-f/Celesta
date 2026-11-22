//! `--check`: renders every frame of the composition (or of `--from`/`--to`)
//! and reports the quality checks that need pixels, as
//! [`ExportProgress::Quality`] events, writing no file. See
//! [`crate::quality`].

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use celesta_composition::{Rational, ResolvedAsset, Scene, Time};
use celesta_evaluator::{EvaluationError, Evaluator};
use celesta_gpu_renderer::{GpuFrame, GpuRenderer};
use celesta_project::Project;
use celesta_react_bridge::{ProjectFrame, ReactBridge};

use crate::control::{CompositionInfo, ExportCancellation, ExportProgress};
use crate::error::ExportError;
use crate::exporter::Exporter;
use crate::project::{absolutize_fonts, absolutize_layers};
use crate::quality::{
    self, PROBE_FILLS, PixelLint, QualityCode, SceneLint, TextContrast, TextSample,
    with_probe_fill, without_layer,
};
use crate::range::{ExportRange, resolve_window};
use crate::react::{CompanionProject, ReactRuntimeOptions};
use crate::render::{ReportedWarnings, ensure_not_cancelled, frame_count, report_render_warnings};

/// Text contrast is measured every this many seconds' worth of frames
/// (and on the first), as it renders each text twice more.
const CONTRAST_INTERVAL_SECONDS: f64 = 0.5;

/// A React entry's frames as scenes, the companion project's layers
/// embedded, as stills and checks evaluate them: one at a time, in any
/// order.
pub(crate) struct ReactScenes<'a> {
    pub(crate) bridge: ReactBridge,
    frame_rate: Rational,
    project: Option<(&'a Project, &'a Path)>,
    evaluator: Option<Evaluator<'a>>,
    fonts: Vec<ResolvedAsset>,
}

impl<'a> ReactScenes<'a> {
    /// Scenes from `bridge`, with `project` (and the root its assets
    /// resolve from) evaluated beside it.
    pub(crate) fn new(
        bridge: ReactBridge,
        project: Option<(&'a Project, &'a Path)>,
    ) -> Result<Self, ExportError> {
        let evaluator = project
            .map(|(project, _)| Evaluator::new(project))
            .transpose()
            .map_err(ExportError::Evaluation)?;
        let fonts = evaluator
            .as_ref()
            .map(|evaluator| -> Result<_, EvaluationError> {
                let mut fonts = evaluator.scene_at(Time::ZERO)?.fonts;
                if let Some((_, asset_root)) = project {
                    absolutize_fonts(&mut fonts, asset_root);
                }
                Ok(fonts)
            })
            .transpose()
            .map_err(ExportError::Evaluation)?
            .unwrap_or_default();
        Ok(Self {
            frame_rate: bridge.metadata().frame_rate,
            bridge,
            project,
            evaluator,
            fonts,
        })
    }

    /// Composition frame `frame`'s scene.
    pub(crate) fn scene_at(&mut self, frame: u64) -> Result<Scene, ExportError> {
        let time = Time::frames(
            i64::try_from(frame).map_err(|_| ExportError::TimelineTooLong)?,
            self.frame_rate,
        )
        .map_err(ExportError::Time)?;
        let project_frame = self
            .evaluator
            .as_ref()
            .zip(self.project)
            .map(
                |(evaluator, (project, asset_root))| -> Result<_, EvaluationError> {
                    let mut scene = evaluator.scene_at(time)?;
                    let mut tracks = BTreeMap::new();
                    for track in &project.tracks {
                        let mut layers = evaluator.layers_for_track(&track.id, time)?;
                        absolutize_layers(&mut layers, asset_root);
                        tracks.insert(track.id.clone(), layers);
                    }
                    absolutize_layers(&mut scene.layers, asset_root);
                    Ok((scene.layers, tracks))
                },
            )
            .transpose()
            .map_err(ExportError::Evaluation)?;
        let evaluation = self
            .bridge
            .evaluate_at(
                time,
                project_frame.as_ref().map(|(layers, tracks)| ProjectFrame {
                    layers: layers.as_slice(),
                    tracks,
                }),
            )
            .map_err(ExportError::React)?;
        let mut scene = evaluation.scene;
        scene.fonts.extend(self.fonts.iter().cloned());
        Ok(scene)
    }
}

/// A sampled frame whose text contrast is measured once the frame after it
/// is known: only text that holds still from the frame before to the frame
/// after counts. Each such text is rendered with a black and a white fill
/// (where it shows) and left out (what is behind it).
struct Pending {
    frame: u64,
    scene: Scene,
    full: GpuFrame,
    samples: HashMap<String, TextSample>,
    before: HashMap<String, TextSample>,
}

impl Pending {
    /// Renders each steady text's probes and the frame without it, and
    /// measures it; `after` is the next frame's text, `None` past the last.
    fn measure(
        self,
        after: Option<&HashMap<String, TextSample>>,
        renderer: &mut GpuRenderer,
        contrast: &mut TextContrast,
        cancellation: &ExportCancellation,
    ) -> Result<(), ExportError> {
        let at_frame = |error: ExportError| cancellation.settle(error).at_frame(self.frame);
        let mut samples: Vec<&TextSample> = self.samples.values().collect();
        samples.sort_by(|a, b| a.id.cmp(&b.id));
        for sample in samples {
            let steady = sample.steady(self.before.get(&sample.id))
                && sample.steady(after.and_then(|after| after.get(&sample.id)));
            if !steady {
                continue;
            }
            let mut render = |scene: &Scene| {
                renderer
                    .render(scene)
                    .map_err(ExportError::Render)
                    .map_err(at_frame)
            };
            let probes = PROBE_FILLS.map(|fill| {
                with_probe_fill(&self.scene, &sample.id, fill).map(|probe| render(&probe))
            });
            let [Some(dark), Some(light)] = probes else {
                continue;
            };
            let (dark, light) = (dark?, light?);
            let hidden = render(&without_layer(&self.scene, &sample.id))?;
            contrast.measure(
                self.frame,
                sample,
                self.full.pixels(),
                hidden.pixels(),
                [dark.pixels(), light.pixels()],
            );
        }
        Ok(())
    }
}

/// What one check covers.
struct CheckRun {
    width: u32,
    height: u32,
    frame_rate: Rational,
    /// Composition frames `[first, end)`.
    first: u64,
    end: u64,
    /// The composition's frame count.
    total: u64,
}

impl Exporter {
    /// Checks a JSON project's frames ([`crate::quality`]), rendering each
    /// without writing anything. The export range, if any, narrows the
    /// frames; `cancellation` is checked before each.
    pub fn check_project(
        &self,
        project: &Project,
        asset_root: &Path,
        cancellation: &ExportCancellation,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let duration = project.effective_duration().map_err(ExportError::Time)?;
        let frame_rate = project.settings.frame_rate;
        let total = frame_count(duration, frame_rate)?;
        progress(ExportProgress::Composition(CompositionInfo {
            width: project.settings.width,
            height: project.settings.height,
            frame_rate,
            frames: total,
        }));
        let run = self.check_run(
            (project.settings.width, project.settings.height),
            frame_rate,
            duration,
        )?;
        quality::report(
            quality::within(quality::short_items(project), (run.first, run.end)),
            &mut progress,
        );
        let evaluator = Evaluator::new(project).map_err(ExportError::Evaluation)?;
        let mut renderer = self.export_renderer(asset_root, frame_rate, cancellation)?;
        check_frames(&run, &mut renderer, cancellation, &mut progress, |frame| {
            let time = Time::frames(frame as i64, frame_rate).map_err(ExportError::Time)?;
            evaluator.scene_at(time).map_err(ExportError::Evaluation)
        })
    }

    /// [`Self::check_project`] for a React entry, with its companion
    /// project when it has one.
    pub fn check_react_entry(
        &self,
        entry: &Path,
        runtime: &ReactRuntimeOptions,
        companion: Option<CompanionProject<'_>>,
        cancellation: &ExportCancellation,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let asset_root = crate::project::folder_of(entry);
        let asset_root = &fs::canonicalize(asset_root).unwrap_or_else(|_| asset_root.to_owned());
        let project_owned = companion.map(|companion| {
            (
                companion.project,
                fs::canonicalize(companion.project_asset_root)
                    .unwrap_or_else(|_| companion.project_asset_root.to_owned()),
            )
        });
        let project = project_owned
            .as_ref()
            .map(|(project, root)| (*project, root.as_path()));
        let bridge = runtime.spawn(entry, project, self)?;
        let metadata = bridge.metadata().clone();
        crate::react_entry::report_composition(&metadata, &mut progress);
        let duration = Time::frames(
            i64::try_from(metadata.duration_in_frames).map_err(|_| ExportError::TimelineTooLong)?,
            metadata.frame_rate,
        )
        .map_err(ExportError::Time)?;
        let run = self.check_run(
            (metadata.width, metadata.height),
            metadata.frame_rate,
            duration,
        )?;
        crate::react_entry::report_short_chapters(
            &metadata,
            Some((run.first, run.end)),
            &mut progress,
        );
        let mut scenes = ReactScenes::new(bridge, project)?;
        let mut renderer = self.export_renderer(asset_root, metadata.frame_rate, cancellation)?;
        check_frames(&run, &mut renderer, cancellation, &mut progress, |frame| {
            scenes.scene_at(frame)
        })
    }

    /// The frames a check of a `width`×`height` composition of `duration`
    /// covers: the export range's, or all of them.
    fn check_run(
        &self,
        (width, height): (u32, u32),
        frame_rate: Rational,
        duration: Time,
    ) -> Result<CheckRun, ExportError> {
        if width == 0 || height == 0 {
            return Err(ExportError::UnsupportedDimensions { width, height });
        }
        let total = frame_count(duration, frame_rate)?;
        if total == 0 {
            return Err(ExportError::EmptyTimeline);
        }
        let window = resolve_window(
            self.options
                .range
                .as_ref()
                .unwrap_or(&ExportRange::from(Time::ZERO)),
            duration,
            frame_rate,
        )?;
        let first = u64::try_from(window.start_frame).map_err(|_| ExportError::TimelineTooLong)?;
        Ok(CheckRun {
            width,
            height,
            frame_rate,
            first,
            end: first + window.frames,
            total,
        })
    }
}

/// Renders frames `[run.first, run.end)` of the scenes `scene_at` builds and
/// reports what the quality checks find. A frame that fails to evaluate or
/// render fails the check, naming it.
fn check_frames(
    run: &CheckRun,
    renderer: &mut GpuRenderer,
    cancellation: &ExportCancellation,
    progress: &mut impl FnMut(ExportProgress),
    mut scene_at: impl FnMut(u64) -> Result<Scene, ExportError>,
) -> Result<(), ExportError> {
    let mut scenes = SceneLint::new(run.frame_rate, false);
    let mut pixels = PixelLint::new(run.width, run.height, run.frame_rate);
    let mut contrast = TextContrast::new(run.frame_rate);
    let fps = f64::from(run.frame_rate.numerator) / f64::from(run.frame_rate.denominator.max(1));
    let interval = ((fps * CONTRAST_INTERVAL_SECONDS).round() as u64).max(1);
    let mut reported = ReportedWarnings::default();
    // The text of the frame before, and a sampled frame waiting for the
    // next to say which of its texts hold still.
    let mut before: HashMap<String, TextSample> = HashMap::new();
    let mut pending: Option<Pending> = None;
    let total = run.end - run.first;
    for frame in run.first..run.end {
        ensure_not_cancelled(cancellation)?;
        progress(ExportProgress::Rendering {
            frame: frame - run.first + 1,
            total,
        });
        let at_frame = |error: ExportError| cancellation.settle(error).at_frame(frame);
        let scene = scene_at(frame).map_err(at_frame)?;
        scenes.observe(frame, &scene);
        let full = renderer
            .render(&scene)
            .map_err(ExportError::Render)
            .map_err(at_frame)?;
        report_render_warnings(renderer, &mut reported, progress);
        pixels.observe(frame, full.pixels());
        let samples: HashMap<String, TextSample> = TextSample::of(&scene)
            .into_iter()
            .map(|sample| (sample.id.clone(), sample))
            .collect();
        if let Some(pending) = pending.take() {
            pending.measure(Some(&samples), renderer, &mut contrast, cancellation)?;
        }
        if (frame - run.first).is_multiple_of(interval) {
            pending = Some(Pending {
                frame,
                scene,
                full,
                samples: samples.clone(),
                before: std::mem::take(&mut before),
            });
        }
        before = samples;
    }
    if let Some(pending) = pending {
        pending.measure(None, renderer, &mut contrast, cancellation)?;
    }
    let mut issues = scenes.finish();
    // Rendered, frame 0 is measured in pixels.
    if run.first == 0 {
        issues.retain(|issue| issue.code != QualityCode::BlankFirstFrame);
    }
    issues.extend(pixels.finish(run.end == run.total));
    issues.extend(contrast.finish());
    quality::report(issues, progress);
    Ok(())
}
