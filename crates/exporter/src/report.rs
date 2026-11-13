//! The machine-readable result of one export: `Celesta-export --json`
//! prints it, and the MCP server returns it from its rendering tools.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use celesta_composition::Rational;
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::{AudioMixError, MediaError};
use celesta_project::LoadError;
use celesta_react_bridge::{PropsFileChoice, ReactBridgeError};
use serde::Serialize;

use crate::resolve::{NOT_INSTALLED, is_no_default_encoder};
use crate::{
    AudioLoudness, CompositionInfo, EncoderPlan, ExportError, ExportProgress, ExportedFile,
    OutputPlan, StillFormat, TrackInfo,
};

/// Whose options an error's hint names: `Celesta-export`'s flags or the MCP
/// server's tool arguments. Error codes are the same either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintStyle {
    Cli,
    Mcp,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<PathBuf>,
    /// The property file in effect: `--props-file`, else the entry's `props.json`.
    #[serde(skip_serializing_if = "Option::is_none")]
    props_file: Option<PathBuf>,
    /// True when `props_file` was found beside the entry rather than named.
    #[serde(skip_serializing_if = "Option::is_none")]
    props_file_auto: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    composition: Option<Composition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    composition_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compositions: Option<Vec<celesta_react_bridge::ReactCompositionInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outputs: Option<Vec<Output>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warnings: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stats: Option<Stats>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum Status {
    Ok,
    Error,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorReport {
    /// A stable identifier to branch on; `message` is for reading.
    code: &'static str,
    message: String,
    /// What to change to get past the error, when that is a CLI option.
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<Cow<'static, str>>,
    /// Every project validation error, each with its JSON path, or every
    /// rejected project property, each with its key.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    issues: Vec<Issue>,
    /// The zero-based composition frame that failed, when one did.
    #[serde(skip_serializing_if = "Option::is_none")]
    frame: Option<u64>,
    /// With `frames_failed`: each frame the contact sheet shows as a failed
    /// tile, in sheet order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    failed_frames: Vec<FailedFrame>,
}

/// One frame of a contact sheet that could not be evaluated or rendered.
#[derive(Serialize)]
struct FailedFrame {
    frame: u64,
    /// The code the error would have on its own, such as `react` or `render`.
    code: &'static str,
    message: String,
}

#[derive(Serialize)]
pub(crate) struct Issue {
    /// Absent for a property issue about a whole source rather than one key.
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Composition {
    width: u32,
    height: u32,
    fps: f64,
    /// Total frames; frame numbers are zero-based, so when nonzero the last is
    /// `frames - 1`.
    frames: u64,
    /// Seconds.
    duration: f64,
}

/// Times are seconds, rounded to milliseconds.
#[derive(Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Output {
    Video {
        path: PathBuf,
        first_frame: u64,
        frames: u64,
        start: f64,
        duration: f64,
        /// The muxer that wrote it, `<plugin>/<component>`.
        muxer: String,
        video: Encoded,
        /// `null` when the video has no audio track.
        audio: Option<Encoded>,
        /// The mix's loudness, when the video has sound.
        #[serde(skip_serializing_if = "Option::is_none")]
        loudness: Option<Loudness>,
        /// Whether the video keeps the composition's alpha.
        alpha: bool,
        /// Every file the muxer wrote: the video's path for one file.
        files: Vec<PathBuf>,
    },
    Frame {
        path: PathBuf,
        frame: u64,
        time: f64,
    },
    Subtitles {
        path: PathBuf,
        format: &'static str,
        cues: usize,
        /// The language of the cues, when the composition names one.
        #[serde(skip_serializing_if = "Option::is_none")]
        lang: Option<String>,
    },
    ContactSheet {
        path: PathBuf,
        columns: u32,
        /// Tile frames in reading order, left to right and top to bottom.
        frames: Vec<u64>,
    },
}

/// A track's encoder and the codec string it wrote.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Encoded {
    component: String,
    codec: String,
}

impl From<TrackInfo> for Encoded {
    fn from(track: TrackInfo) -> Self {
        Self {
            component: track.component.to_string(),
            codec: track.codec,
        }
    }
}

/// An encoder as the `output` progress event names it: its reference,
/// name, and the settings that differ from their defaults, as labels.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Encoder {
    component: String,
    name: String,
    settings: Vec<String>,
}

impl From<EncoderPlan> for Encoder {
    fn from(plan: EncoderPlan) -> Self {
        Self {
            component: plan.component.to_string(),
            name: plan.name,
            settings: plan.settings,
        }
    }
}

/// An exported mix's loudness, rounded to 0.01. `null` loudness is a silent
/// mix or one shorter than 400 ms, a `null` peak digital silence.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loudness {
    integrated_lufs: Option<f64>,
    true_peak_dbtp: Option<f64>,
    /// The gain a loudness target applied; absent without one.
    #[serde(skip_serializing_if = "Option::is_none")]
    gain_db: Option<f64>,
}

impl From<AudioLoudness> for Loudness {
    fn from(loudness: AudioLoudness) -> Self {
        let hundredths = |value: Option<f64>| value.map(|value| round(value, 100.0));
        Self {
            integrated_lufs: hundredths(loudness.integrated_lufs),
            true_peak_dbtp: hundredths(loudness.true_peak_dbtp),
            gain_db: hundredths(loudness.gain_db),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Stats {
    elapsed_seconds: f64,
    /// Average export speed in frames per second. Encoding runs alongside
    /// rendering, so this is the speed of the slower of the two.
    #[serde(skip_serializing_if = "Option::is_none")]
    render_fps: Option<f64>,
}

/// Runs `export`, collecting what it reports, and returns its report and
/// whether it succeeded.
pub fn build(
    source: &Path,
    props_file: Option<PropsFileChoice>,
    hints: HintStyle,
    export: impl FnOnce(&mut dyn FnMut(ExportProgress)) -> Result<(), ExportError>,
) -> (Report, bool) {
    let mut timing = Timing::new();
    let mut composition = None;
    let mut composition_id = None;
    let mut compositions = None;
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let result = export(&mut |event| {
        match &event {
            ExportProgress::Composition(info) => composition = Some(*info),
            ExportProgress::ReactCompositions {
                selected,
                compositions: roots,
            } => {
                composition_id = Some(selected.clone());
                compositions = Some(roots.clone());
            }
            ExportProgress::Wrote(file) => files.push(file.clone()),
            ExportProgress::Warning(warning) | ExportProgress::Note(warning) => {
                warnings.push(warning.clone());
            }
            _ => {}
        }
        timing.update(&event);
    });
    timing.finish(result.is_ok());
    let frame_rate = composition.map(|info: CompositionInfo| info.frame_rate);
    let report = Report {
        composition_id,
        compositions,
        status: if result.is_ok() {
            Status::Ok
        } else {
            Status::Error
        },
        error: result
            .as_ref()
            .err()
            .map(|error| error_report(error, hints)),
        source: Some(absolute(source)),
        props_file_auto: props_file.as_ref().map(|choice| choice.auto),
        props_file: props_file.map(|choice| choice.path),
        composition: composition.map(composition_report),
        outputs: Some(
            files
                .into_iter()
                .map(|file| output(file, frame_rate))
                .collect(),
        ),
        warnings: Some(warnings),
        stats: Some(Stats {
            elapsed_seconds: round(timing.elapsed().as_secs_f64(), 1_000.0),
            render_fps: timing.render_fps().map(|fps| round(fps, 10.0)),
        }),
    };
    (report, result.is_ok())
}

/// One `--json-progress` line: an export event as it happens, before the
/// report. Each line has an `event` naming its kind; the report has none.
#[derive(Serialize)]
#[serde(
    tag = "event",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    /// The report's `composition`, as soon as it is known.
    Composition {
        composition: Composition,
    },
    /// The muxer and encoders the video is written with, once they are
    /// chosen and before any frame renders.
    Output {
        muxer: String,
        muxer_name: String,
        video: Encoder,
        /// `null` when the video has no audio track.
        audio: Option<Encoder>,
    },
    /// The report's `compositionId` and `compositions`.
    Compositions {
        composition_id: String,
        compositions: Vec<celesta_react_bridge::ReactCompositionInfo>,
    },
    /// Frame `frame` of `total` has started, counting from 1.
    Rendering {
        frame: u64,
        total: u64,
    },
    MixingAudio,
    Muxing,
    Warning {
        message: String,
    },
    /// One of the report's `outputs`, once it is in place.
    Wrote {
        output: Output,
    },
}

/// Turns export events into [`Event`]s, keeping the frame rate the times of
/// written files are counted in.
#[derive(Default)]
pub struct Events {
    frame_rate: Option<Rational>,
}

impl Events {
    pub fn event(&mut self, event: &ExportProgress) -> Event {
        match event {
            ExportProgress::Composition(info) => {
                self.frame_rate = Some(info.frame_rate);
                Event::Composition {
                    composition: composition_report(*info),
                }
            }
            ExportProgress::ReactCompositions {
                selected,
                compositions,
            } => Event::Compositions {
                composition_id: selected.clone(),
                compositions: compositions.clone(),
            },
            ExportProgress::Output(plan) => output_event(plan.clone()),
            &ExportProgress::Rendering { frame, total } => Event::Rendering { frame, total },
            ExportProgress::MixingAudio => Event::MixingAudio,
            ExportProgress::Muxing => Event::Muxing,
            // A note keeps its `note: ` label among the warnings.
            ExportProgress::Warning(message) | ExportProgress::Note(message) => Event::Warning {
                message: message.clone(),
            },
            ExportProgress::Wrote(file) => Event::Wrote {
                output: output(file.clone(), self.frame_rate),
            },
        }
    }
}

fn output_event(plan: OutputPlan) -> Event {
    Event::Output {
        muxer: plan.muxer.to_string(),
        muxer_name: plan.muxer_name,
        video: plan.video.into(),
        audio: plan.audio.map(Encoder::from),
    }
}

/// How long an export took and how fast it rendered, measured the way the
/// CLI's progress display measures it.
struct Timing {
    started: Instant,
    rendering_started: Option<Instant>,
    rendering_finished: Option<Instant>,
    finished: Option<Instant>,
    frame: u64,
    total: u64,
}

impl Timing {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            rendering_started: None,
            rendering_finished: None,
            finished: None,
            frame: 0,
            total: 0,
        }
    }

    fn update(&mut self, event: &ExportProgress) {
        match event {
            ExportProgress::Rendering { frame, total } => {
                self.rendering_started.get_or_insert_with(Instant::now);
                self.frame = *frame;
                self.total = *total;
            }
            ExportProgress::MixingAudio | ExportProgress::Muxing => self.end_rendering(),
            _ => {}
        }
    }

    fn end_rendering(&mut self) {
        if self.rendering_started.is_some() {
            self.rendering_finished.get_or_insert_with(Instant::now);
        }
    }

    fn finish(&mut self, ok: bool) {
        self.finished = Some(Instant::now());
        if ok {
            self.end_rendering();
        }
    }

    fn elapsed(&self) -> Duration {
        self.finished
            .unwrap_or_else(Instant::now)
            .duration_since(self.started)
    }

    /// Frames rendered per second, once any frame has finished.
    fn render_fps(&self) -> Option<f64> {
        let started = self.rendering_started?;
        let end = self.rendering_finished.or(self.finished)?;
        // Rendering events mark frame starts, including the last frame.
        let done = if self.rendering_finished.is_some() {
            self.total
        } else {
            self.frame.saturating_sub(1).min(self.total)
        };
        let seconds = end.duration_since(started).as_secs_f64();
        (done > 0 && seconds > 0.0).then(|| done as f64 / seconds)
    }
}

/// The report for arguments rejected before anything ran.
pub fn usage(message: &str) -> Report {
    error_only(ErrorReport {
        code: "usage",
        message: message.trim().trim_start_matches("error: ").to_owned(),
        hint: None,
        issues: Vec::new(),
        frame: None,
        failed_frames: Vec::new(),
    })
}

/// The report for an error outside an export, such as `--describe`'s.
pub fn failure(error: &ExportError, hints: HintStyle) -> Report {
    error_only(error_report(error, hints))
}

/// What to change to get past `error`, when there is something to say.
pub fn hint(error: &ExportError, hints: HintStyle) -> Option<Cow<'static, str>> {
    classify(error, hints, &mut Vec::new()).1
}

fn error_only(error: ErrorReport) -> Report {
    Report {
        status: Status::Error,
        error: Some(error),
        source: None,
        props_file: None,
        props_file_auto: None,
        composition: None,
        composition_id: None,
        compositions: None,
        outputs: None,
        warnings: None,
        stats: None,
    }
}

fn composition_report(info: CompositionInfo) -> Composition {
    Composition {
        width: info.width,
        height: info.height,
        fps: fps(info.frame_rate),
        frames: info.frames,
        duration: seconds(info.frames, info.frame_rate),
    }
}

fn output(file: ExportedFile, frame_rate: Option<Rational>) -> Output {
    // Every file is written after the composition is reported.
    let rate = frame_rate.unwrap_or(Rational::new(1, 1));
    match file {
        ExportedFile::Video {
            path,
            first_frame,
            frames,
            muxer,
            video,
            audio,
            alpha,
            files,
            loudness,
        } => Output::Video {
            path: absolute(&path),
            first_frame,
            frames,
            start: seconds(first_frame, rate),
            duration: seconds(frames, rate),
            muxer: muxer.to_string(),
            video: video.into(),
            audio: audio.map(Encoded::from),
            loudness: loudness.map(Loudness::from),
            alpha,
            files: files.iter().map(|file| absolute(file)).collect(),
        },
        ExportedFile::Frame { path, frame } => Output::Frame {
            path: absolute(&path),
            frame,
            time: seconds(frame, rate),
        },
        ExportedFile::Subtitles {
            path,
            format,
            cues,
            lang,
        } => Output::Subtitles {
            path: absolute(&path),
            format: format.as_str(),
            cues,
            lang,
        },
        ExportedFile::ContactSheet {
            path,
            frames,
            columns,
        } => Output::ContactSheet {
            path: absolute(&path),
            columns,
            frames,
        },
    }
}

fn error_report(error: &ExportError, hints: HintStyle) -> ErrorReport {
    let mut issues = Vec::new();
    let (frame, failed_frames) = match error {
        ExportError::Frame(failure) => (Some(failure.frame), Vec::new()),
        ExportError::FramesFailed { failures, .. } => (
            None,
            failures
                .iter()
                .map(|failure| FailedFrame {
                    frame: failure.frame,
                    code: classify(&failure.error, hints, &mut Vec::new()).0,
                    message: failure.error.to_string(),
                })
                .collect(),
        ),
        _ => (None, Vec::new()),
    };
    let (code, hint) = classify(error, hints, &mut issues);
    ErrorReport {
        code,
        message: error.to_string(),
        hint,
        issues,
        frame,
        failed_frames,
    }
}

const NO_DECODER_HINT: &str = "install a media plugin beside an installed Celesta or in a plugin \
     folder; in a source checkout, build the media plugins under plugins/ with node \
     scripts/plugins/build.mts and add the absolute path of plugins/dist to CELESTA_PLUGIN_PATH";

/// The code and hint of `error`, collecting its issues into `issues`. A
/// frame's failure has the code of what failed.
pub(crate) fn classify(
    error: &ExportError,
    hints: HintStyle,
    issues: &mut Vec<Issue>,
) -> (&'static str, Option<Cow<'static, str>>) {
    let cli = hints == HintStyle::Cli;
    match error {
        ExportError::Frame(failure) => classify(&failure.error, hints, issues),
        ExportError::InvalidPresets { .. } => ("invalid_presets", None),
        ExportError::UnknownPreset { .. } => ("unknown_preset", None),
        ExportError::UnsupportedOutput { path, extensions } => (
            "unsupported_output",
            Some(unsupported_output_hint(path, extensions, cli)),
        ),
        ExportError::IncompatibleComponents { .. } => ("incompatible_components", None),
        // A component no installed plugin offers: its plugin is missing, has
        // no such component, or is installed but not allowed, which is
        // skipped and so reads as not installed.
        ExportError::ComponentUnavailable { component, reason } if reason == NOT_INSTALLED => (
            "component_unavailable",
            Some(Cow::Owned(if cli {
                format!(
                    "--list-components shows the installed components; if the {plugin} \
                     plugin is installed but not allowed, allow it with --allow-plugin {plugin}",
                    plugin = component.plugin
                )
            } else {
                format!(
                    "list_components shows the installed components; if the {plugin} \
                     plugin is installed but not allowed, allow it in Celesta's Plugins tab",
                    plugin = component.plugin
                )
            })),
        ),
        // A muxer whose default codec no installed encoder writes.
        ExportError::ComponentUnavailable { reason, .. } if is_no_default_encoder(reason) => (
            "component_unavailable",
            fixed(if cli {
                "install a media plugin with an encoder for that codec, or write another format; \
                 --list-components shows the installed components"
            } else {
                "install a media plugin with an encoder for that codec, or write another format; \
                 list_components shows the installed components"
            }),
        ),
        ExportError::ComponentUnavailable { .. }
        | ExportError::Codec {
            source: MediaError::ComponentUnavailable { .. },
            ..
        } => ("component_unavailable", None),
        // A plugin refusing a combination of settings at `open`.
        ExportError::InvalidOption { .. }
        | ExportError::Codec {
            source: MediaError::InvalidSettings { .. },
            ..
        } => ("invalid_option", None),
        ExportError::FramesFailed { .. } => (
            "frames_failed",
            fixed(
                "the sheet was written with failed tiles; fix the frames in failedFrames and export again",
            ),
        ),
        ExportError::Project(LoadError::Io(_)) => ("project_io", None),
        ExportError::Project(LoadError::Json(_)) => ("project_json", None),
        ExportError::Project(LoadError::Validation(errors)) => {
            *issues = errors
                .as_slice()
                .iter()
                .map(|error| Issue {
                    path: Some(error.path.clone()),
                    message: error.message.clone(),
                })
                .collect();
            ("project_validation", None)
        }
        ExportError::Evaluation(_) => ("evaluation", None),
        // A video frame or audio file no decoder could read: say why. The
        // `Codec` arms are an encoder or muxer that answered so, which
        // decoding does not reach.
        ExportError::Render(GpuRenderError::Media(MediaError::NoDecoder))
        | ExportError::Audio(AudioMixError::Media(MediaError::NoDecoder))
        | ExportError::Codec {
            source: MediaError::NoDecoder,
            ..
        } => ("no_decoder", fixed(NO_DECODER_HINT)),
        ExportError::Render(GpuRenderError::Media(MediaError::Unsupported { .. }))
        | ExportError::Audio(AudioMixError::Media(MediaError::Unsupported { .. }))
        | ExportError::Codec {
            source: MediaError::Unsupported { .. },
            ..
        } => ("unsupported_media", None),
        ExportError::Render(_) => ("render", None),
        ExportError::Audio(_) => ("audio", None),
        ExportError::React(ReactBridgeError::InvalidProperties(invalid)) => {
            *issues = invalid
                .iter()
                .map(|issue| Issue {
                    path: (!issue.key.is_empty()).then(|| issue.key.clone()),
                    message: format!("{}: {}", issue.source, issue.message),
                })
                .collect();
            (
                "invalid_properties",
                fixed("pass values matching the entry's defineProjectProperties() schema"),
            )
        }
        ExportError::React(_) => ("react", None),
        ExportError::Properties(_) => ("invalid_properties", None),
        ExportError::Time(_) => ("time", None),
        ExportError::Io { .. } => ("io", None),
        ExportError::Codec { .. } => ("codec", None),
        ExportError::OutputExists(_) => (
            "output_exists",
            fixed(if cli {
                "pass --overwrite to replace it, or write to another path"
            } else {
                "set overwrite to replace it, or write to another path"
            }),
        ),
        ExportError::InvalidSelection(_) => ("invalid_selection", None),
        ExportError::UnsupportedSubtitles(_) => (
            "unsupported_subtitles",
            fixed("write subtitles to a .srt or .vtt path"),
        ),
        ExportError::MixedSubtitleLanguages { .. } => (
            "mixed_subtitle_languages",
            fixed(
                "put {lang} in the subtitle path to write one file per language, or set the same lang on every <Subtitles>",
            ),
        ),
        ExportError::NoOutput => (
            "no_output",
            fixed(if cli {
                "give an output video, or --subtitles to write only subtitle files"
            } else {
                "give output, or subtitles to write only subtitle files"
            }),
        ),
        ExportError::UnsupportedDimensions { .. } => (
            "unsupported_dimensions",
            fixed(
                "make the composition width and height even; still frame exports accept odd sizes",
            ),
        ),
        ExportError::EmptyTimeline => ("empty_timeline", None),
        ExportError::EmptyRange => (
            "empty_range",
            fixed(if cli {
                "choose --from/--to inside the composition (composition.duration seconds)"
            } else {
                "choose start/end inside the composition (composition.duration seconds)"
            }),
        ),
        ExportError::Cancelled => ("cancelled", None),
        ExportError::TimelineTooLong => ("timeline_too_long", None),
    }
}

fn fixed(hint: &'static str) -> Option<Cow<'static, str>> {
    Some(Cow::Borrowed(hint))
}

/// The extensions a video may be written to, and for a still's extension
/// how to export stills instead; with no muxer at all, how to install the
/// media plugin.
fn unsupported_output_hint(path: &Path, extensions: &[String], cli: bool) -> Cow<'static, str> {
    let dotted: Vec<String> = extensions
        .iter()
        .map(|extension| format!(".{extension}"))
        .collect();
    let Some((last, rest)) = dotted.split_last() else {
        return Cow::Borrowed(NO_DECODER_HINT);
    };
    let listed = if rest.is_empty() {
        last.clone()
    } else {
        format!("{} or {last}", rest.join(", "))
    };
    let still = StillFormat::from_extension(path).is_some();
    Cow::Owned(match (cli, still) {
        (true, false) => format!("write video to a {listed} path"),
        (true, true) => format!(
            "write video to a {listed} path, or select frames with --frame or --every for PNG, JPEG or WebP stills"
        ),
        (false, false) => format!("write video to a {listed} path with export_video"),
        (false, true) => format!(
            "write video to a {listed} path with export_video, or render PNG, JPEG or WebP stills with render_frames"
        ),
    })
}

/// `path` from the filesystem root, so it does not depend on the caller's
/// working directory.
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}

fn fps(rate: Rational) -> f64 {
    round(
        f64::from(rate.numerator) / f64::from(rate.denominator),
        1_000.0,
    )
}

fn seconds(frames: u64, rate: Rational) -> f64 {
    round(
        frames as f64 * f64::from(rate.denominator) / f64::from(rate.numerator),
        1_000.0,
    )
}

fn round(value: f64, scale: f64) -> f64 {
    (value * scale).round() / scale
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EncoderPlan;

    fn reference(text: &str) -> celesta_media::ComponentRef {
        text.parse().unwrap()
    }

    /// A video the test codec wrote to `path`, with no audio track.
    fn test_codec_video(path: &str, loudness: Option<AudioLoudness>) -> ExportedFile {
        ExportedFile::Video {
            path: PathBuf::from(path),
            first_frame: 0,
            frames: 30,
            muxer: reference("celesta-test-codec/file"),
            video: TrackInfo {
                component: reference("celesta-test-codec/video"),
                codec: "ctst".to_owned(),
            },
            audio: None,
            alpha: false,
            files: vec![PathBuf::from(path)],
            loudness,
        }
    }

    fn report(
        export: impl FnOnce(&mut dyn FnMut(ExportProgress)) -> Result<(), ExportError>,
    ) -> serde_json::Value {
        serde_json::to_value(build(Path::new("/tmp/film.tsx"), None, HintStyle::Cli, export).0)
            .unwrap()
    }

    #[test]
    fn outputs_name_their_frames_and_times() {
        let value = report(|progress| {
            progress(ExportProgress::Composition(CompositionInfo {
                width: 1920,
                height: 1080,
                frame_rate: Rational::new(30_000, 1_001),
                frames: 300,
            }));
            progress(ExportProgress::Wrote(ExportedFile::Frame {
                path: PathBuf::from("/tmp/check-000090.png"),
                frame: 90,
            }));
            let mut video = test_codec_video("/tmp/check.mp4", None);
            if let ExportedFile::Video {
                first_frame,
                frames,
                alpha,
                ..
            } = &mut video
            {
                (*first_frame, *frames, *alpha) = (30, 60, true);
            }
            progress(ExportProgress::Wrote(video));
            Ok(())
        });
        assert_eq!(value["status"], "ok");
        assert!(value.get("error").is_none());
        assert_eq!(
            value["composition"],
            serde_json::json!({
                "width": 1920, "height": 1080, "fps": 29.97, "frames": 300, "duration": 10.01
            })
        );
        assert_eq!(
            value["outputs"],
            serde_json::json!([
                { "type": "frame", "path": absolute(Path::new("/tmp/check-000090.png")), "frame": 90, "time": 3.003 },
                {
                    "type": "video", "path": absolute(Path::new("/tmp/check.mp4")), "firstFrame": 30, "frames": 60,
                    "start": 1.001, "duration": 2.002,
                    "muxer": "celesta-test-codec/file",
                    "video": { "component": "celesta-test-codec/video", "codec": "ctst" },
                    "audio": null, "alpha": true,
                    "files": [absolute(Path::new("/tmp/check.mp4"))]
                }
            ])
        );
    }

    #[test]
    fn a_video_reports_its_loudness() {
        let video =
            |loudness| ExportProgress::Wrote(test_codec_video("/tmp/film.mp4", Some(loudness)));
        let value = report(|progress| {
            progress(video(AudioLoudness {
                integrated_lufs: Some(-14.004),
                true_peak_dbtp: Some(-1.2345),
                gain_db: Some(3.4),
            }));
            progress(video(AudioLoudness {
                integrated_lufs: None,
                true_peak_dbtp: None,
                gain_db: None,
            }));
            Ok(())
        });
        assert_eq!(
            value["outputs"][0]["loudness"],
            serde_json::json!({ "integratedLufs": -14.0, "truePeakDbtp": -1.23, "gainDb": 3.4 })
        );
        assert_eq!(
            value["outputs"][1]["loudness"],
            serde_json::json!({ "integratedLufs": null, "truePeakDbtp": null })
        );
    }

    #[test]
    fn json_report_names_the_props_file() {
        let choice = PropsFileChoice {
            path: PathBuf::from("/tmp/props.json"),
            auto: true,
        };
        let value = serde_json::to_value(
            build(
                Path::new("/tmp/film.tsx"),
                Some(choice),
                HintStyle::Cli,
                |_| Ok(()),
            )
            .0,
        )
        .unwrap();
        assert_eq!(value["propsFile"], "/tmp/props.json");
        assert_eq!(value["propsFileAuto"], true);

        let value = report(|_| Ok(()));
        assert!(value.get("propsFile").is_none());
        assert!(value.get("propsFileAuto").is_none());
    }

    #[test]
    fn errors_carry_a_code_and_a_hint_for_cli_fixes() {
        let value = report(|_| Err(ExportError::OutputExists(PathBuf::from("/tmp/a.mp4"))));
        assert_eq!(value["status"], "error");
        assert_eq!(value["error"]["code"], "output_exists");
        assert_eq!(
            value["error"]["message"],
            "output already exists: /tmp/a.mp4"
        );
        assert!(
            value["error"]["hint"]
                .as_str()
                .unwrap()
                .contains("--overwrite")
        );
        assert!(value["error"].get("issues").is_none());
        assert!(value.get("composition").is_none());
        assert_eq!(value["outputs"], serde_json::json!([]));
    }

    #[test]
    fn rejected_properties_are_listed_as_issues_by_key() {
        let value = report(|_| {
            Err(ExportError::React(ReactBridgeError::InvalidProperties(
                vec![
                    celesta_react_bridge::PropertyIssue {
                        key: "accent".to_owned(),
                        source: "--props-file spring.json".to_owned(),
                        message: "expected a #RRGGBB or #RRGGBBAA color, got \"pink\"".to_owned(),
                    },
                    celesta_react_bridge::PropertyIssue {
                        key: String::new(),
                        source: "--props".to_owned(),
                        message: "the entry declares no project properties".to_owned(),
                    },
                ],
            )))
        });
        assert_eq!(value["error"]["code"], "invalid_properties");
        assert_eq!(
            value["error"]["issues"],
            serde_json::json!([
                {
                    "path": "accent",
                    "message": "--props-file spring.json: expected a #RRGGBB or #RRGGBBAA color, got \"pink\""
                },
                { "message": "--props: the entry declares no project properties" }
            ])
        );
        assert!(
            value["error"]["hint"]
                .as_str()
                .unwrap()
                .contains("defineProjectProperties")
        );
    }

    #[test]
    fn files_written_before_a_failure_are_still_listed() {
        let value = report(|progress| {
            progress(ExportProgress::Composition(CompositionInfo {
                width: 640,
                height: 360,
                frame_rate: Rational::new(30, 1),
                frames: 90,
            }));
            progress(ExportProgress::Warning("font fallback".into()));
            progress(ExportProgress::Note("note: terms of use".into()));
            progress(ExportProgress::Wrote(ExportedFile::Frame {
                path: PathBuf::from("/tmp/c-000000.png"),
                frame: 0,
            }));
            Err(ExportError::InvalidSelection("boom".into()))
        });
        assert_eq!(value["error"]["code"], "invalid_selection");
        assert_eq!(value["outputs"][0]["frame"], 0);
        assert_eq!(
            value["warnings"],
            serde_json::json!(["font fallback", "note: terms of use"])
        );
        assert_eq!(value["composition"]["frames"], 90);
    }

    fn hint(hints: HintStyle, error: ExportError) -> (String, String) {
        let value =
            serde_json::to_value(build(Path::new("/tmp/film.tsx"), None, hints, |_| Err(error)).0)
                .unwrap();
        (
            value["error"]["code"].as_str().unwrap().to_owned(),
            value["error"]["hint"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        )
    }

    #[test]
    fn presets_errors_have_their_own_codes_and_no_hints() {
        for hints in [HintStyle::Cli, HintStyle::Mcp] {
            let invalid = ExportError::InvalidPresets {
                path: PathBuf::from("/tmp/export-presets.json"),
                message: "unsupported version 2; this Celesta reads version 1".into(),
            };
            assert_eq!(
                hint(hints, invalid),
                ("invalid_presets".into(), String::new())
            );
            // A project default is not a `--preset` the user can swap for an option.
            let unknown = ExportError::UnknownPreset {
                name: "fast".into(),
                default_in: Some(PathBuf::from("/tmp/export-presets.json")),
            };
            assert_eq!(
                unknown.to_string(),
                "the default preset `fast` of /tmp/export-presets.json is in no presets file"
            );
            assert_eq!(
                hint(hints, unknown),
                ("unknown_preset".into(), String::new())
            );
        }
    }

    #[test]
    fn mcp_hints_name_tool_arguments() {
        let errors = || {
            [
                ExportError::OutputExists(PathBuf::from("/tmp/a.mp4")),
                ExportError::UnsupportedOutput {
                    path: PathBuf::from("/tmp/a.gif"),
                    extensions: vec!["mov".into(), "mp4".into(), "webm".into()],
                },
                ExportError::UnsupportedOutput {
                    path: PathBuf::from("/tmp/a.png"),
                    extensions: vec!["mp4".into()],
                },
                ExportError::EmptyRange,
            ]
        };
        let mcp: Vec<_> = errors()
            .into_iter()
            .map(|e| hint(HintStyle::Mcp, e))
            .collect();
        let cli: Vec<_> = errors()
            .into_iter()
            .map(|e| hint(HintStyle::Cli, e))
            .collect();
        assert_eq!(
            mcp.iter()
                .map(|(_, hint)| hint.as_str())
                .collect::<Vec<_>>(),
            [
                "set overwrite to replace it, or write to another path",
                "write video to a .mov, .mp4 or .webm path with export_video",
                "write video to a .mp4 path with export_video, or render PNG, JPEG or WebP stills with render_frames",
                "choose start/end inside the composition (composition.duration seconds)",
            ]
        );
        for ((mcp_code, mcp_hint), (cli_code, _)) in mcp.iter().zip(&cli) {
            assert_eq!(mcp_code, cli_code);
            assert!(!mcp_hint.contains("--"), "{mcp_hint}");
        }
    }

    fn bad_color() -> ExportError {
        ExportError::React(ReactBridgeError::Render(
            "<Rect> fill \"ink\" is not a color".to_owned(),
        ))
    }

    #[test]
    fn a_failed_frame_keeps_its_code_and_names_the_frame() {
        let value = report(|_| Err(bad_color().at_frame(99)));
        assert_eq!(value["error"]["code"], "react");
        assert_eq!(value["error"]["frame"], 99);
        assert_eq!(
            value["error"]["message"],
            "frame 99: could not evaluate React export: could not render the composition: \
             <Rect> fill \"ink\" is not a color"
        );
        assert!(value["error"].get("failedFrames").is_none());
        // Cancelling is never one frame's failure.
        let value = report(|_| Err(ExportError::Cancelled.at_frame(3)));
        assert_eq!(value["error"]["code"], "cancelled");
        assert!(value["error"].get("frame").is_none());
    }

    #[test]
    fn a_contact_sheet_with_failed_frames_lists_each_one() {
        let value = report(|progress| {
            progress(ExportProgress::Wrote(ExportedFile::ContactSheet {
                path: PathBuf::from("/tmp/sheet.png"),
                frames: vec![0, 10, 20],
                columns: 5,
            }));
            Err(ExportError::FramesFailed {
                path: PathBuf::from("/tmp/sheet.png"),
                frames: 3,
                failures: vec![
                    crate::FrameFailure {
                        frame: 10,
                        error: bad_color(),
                    },
                    crate::FrameFailure {
                        frame: 20,
                        error: ExportError::Render(
                            celesta_gpu_renderer::GpuRenderError::MapCallbackDropped,
                        ),
                    },
                ],
            })
        });
        assert_eq!(value["status"], "error");
        assert_eq!(value["error"]["code"], "frames_failed");
        assert!(value["error"].get("frame").is_none());
        assert!(
            value["error"]["hint"]
                .as_str()
                .unwrap()
                .contains("failedFrames")
        );
        let message = value["error"]["message"].as_str().unwrap();
        assert!(
            message.starts_with("2 of 3 contact sheet frames failed; /tmp/sheet.png")
                && message.contains("\n  frame 10: could not evaluate React export")
                && message.contains("\n  frame 20: could not render export"),
            "{message}"
        );
        let failed = &value["error"]["failedFrames"];
        assert_eq!(failed[0]["frame"], 10);
        assert_eq!(failed[0]["code"], "react");
        assert_eq!(
            failed[0]["message"],
            "could not evaluate React export: could not render the composition: \
             <Rect> fill \"ink\" is not a color"
        );
        assert_eq!(failed[1]["frame"], 20);
        assert_eq!(failed[1]["code"], "render");
        // The sheet was written all the same.
        assert_eq!(value["outputs"][0]["type"], "contactSheet");
    }

    #[test]
    fn media_nothing_could_decode_is_reported_as_no_decoder() {
        let errors = [
            ExportError::Codec {
                stage: "encoder check",
                source: MediaError::NoDecoder,
            },
            ExportError::Render(GpuRenderError::Media(MediaError::NoDecoder)),
            ExportError::Audio(AudioMixError::Media(MediaError::NoDecoder)),
            ExportError::Render(GpuRenderError::Media(MediaError::NoDecoder)).at_frame(3),
        ];
        for error in errors {
            let value = serde_json::to_value(
                build(Path::new("/tmp/film.tsx"), None, HintStyle::Cli, |_| {
                    Err(error)
                })
                .0,
            )
            .unwrap();
            assert_eq!(value["error"]["code"], "no_decoder", "{value}");
            let hint = value["error"]["hint"].as_str().unwrap_or_default();
            // A source checkout has no media plugin until it builds one into
            // plugins/dist, which CELESTA_PLUGIN_PATH reaches only as an
            // absolute path.
            for expected in [
                "media plugin",
                "node scripts/plugins/build.mts",
                "absolute path of plugins/dist",
                "CELESTA_PLUGIN_PATH",
            ] {
                assert!(hint.contains(expected), "{expected}: {value}");
            }
        }
    }

    #[test]
    fn an_unreadable_file_is_unsupported_media() {
        let unsupported = || MediaError::Unsupported {
            path: PathBuf::from("/tmp/clip.xyz"),
            tried: vec!["ffmpeg/main".to_owned()],
        };
        let errors = [
            ExportError::Render(GpuRenderError::Media(unsupported())),
            ExportError::Audio(AudioMixError::Media(unsupported())),
            ExportError::Render(GpuRenderError::Media(unsupported())).at_frame(3),
        ];
        for error in errors {
            let value = serde_json::to_value(
                build(Path::new("/tmp/film.tsx"), None, HintStyle::Cli, |_| {
                    Err(error)
                })
                .0,
            )
            .unwrap();
            assert_eq!(value["error"]["code"], "unsupported_media", "{value}");
            assert!(
                value["error"].get("hint").is_none_or(|hint| hint.is_null()),
                "{value}"
            );
        }
    }

    #[test]
    fn an_encoder_that_cannot_write_is_unsupported_media() {
        let error = ExportError::Codec {
            stage: "video encoding",
            source: MediaError::Unsupported {
                path: PathBuf::from("/tmp/clip.xyz"),
                tried: vec!["ffmpeg/main".to_owned()],
            },
        };
        let value = serde_json::to_value(
            build(Path::new("/tmp/film.tsx"), None, HintStyle::Cli, |_| {
                Err(error)
            })
            .0,
        )
        .unwrap();
        assert_eq!(value["error"]["code"], "unsupported_media", "{value}");
    }

    fn classified(error: &ExportError, hints: HintStyle) -> (&'static str, Option<String>) {
        let (code, hint) = classify(error, hints, &mut Vec::new());
        (code, hint.map(Cow::into_owned))
    }

    #[test]
    fn a_plugins_refusal_at_open_is_an_invalid_option() {
        let refused = ExportError::Codec {
            stage: "video encoding",
            source: MediaError::InvalidSettings {
                key: "alpha".into(),
                message: "needs rgba16".into(),
            },
        };
        assert_eq!(classified(&refused, HintStyle::Cli).0, "invalid_option");
        let unavailable = ExportError::Codec {
            stage: "video encoding",
            source: MediaError::ComponentUnavailable {
                component: "ffmpeg/libx264".into(),
                reason: "this FFmpeg build has no libx264".into(),
            },
        };
        assert_eq!(
            classified(&unavailable, HintStyle::Cli).0,
            "component_unavailable"
        );
    }

    #[test]
    fn a_still_extension_keeps_the_frame_hint() {
        let unsupported = |path: &str| ExportError::UnsupportedOutput {
            path: PathBuf::from(path),
            extensions: vec!["mp4".into()],
        };
        for path in ["out.png", "out.JPG", "out.jpeg", "out.webp"] {
            let (code, hint) = classified(&unsupported(path), HintStyle::Cli);
            assert_eq!(code, "unsupported_output");
            let hint = hint.unwrap();
            assert!(hint.contains(".mp4") && hint.contains("--frame"), "{hint}");
        }
        let (code, hint) = classified(&unsupported("out.mkv"), HintStyle::Cli);
        assert_eq!(code, "unsupported_output");
        let hint = hint.unwrap();
        assert!(hint.contains(".mp4") && !hint.contains("--frame"), "{hint}");
        assert_eq!(hint, "write video to a .mp4 path");

        let two = ExportError::UnsupportedOutput {
            path: PathBuf::from("out.mkv"),
            extensions: vec!["ivf".into(), "mp4".into()],
        };
        assert_eq!(
            classified(&two, HintStyle::Cli).1.unwrap(),
            "write video to a .ivf or .mp4 path"
        );
        // With no muxer at all, the media plugin is what is missing.
        let none = ExportError::UnsupportedOutput {
            path: PathBuf::from("out.png"),
            extensions: Vec::new(),
        };
        assert_eq!(
            classified(&none, HintStyle::Cli),
            ("unsupported_output", Some(NO_DECODER_HINT.to_owned()))
        );
    }

    #[test]
    fn a_component_that_is_not_installed_names_its_plugin() {
        let error = ExportError::ComponentUnavailable {
            component: "some-plugin/encoder".parse().unwrap(),
            reason: NOT_INSTALLED.into(),
        };
        let (code, cli) = classified(&error, HintStyle::Cli);
        assert_eq!(code, "component_unavailable");
        let cli = cli.unwrap();
        assert!(cli.contains("--list-components"), "{cli}");
        assert!(cli.contains("--allow-plugin some-plugin"), "{cli}");
        let mcp = classified(&error, HintStyle::Mcp).1.unwrap();
        assert!(mcp.contains("list_components"), "{mcp}");
        assert!(mcp.contains("Plugins tab"), "{mcp}");
    }

    #[test]
    fn a_muxer_without_a_default_encoder_hints_at_installing_one() {
        use crate::resolve::no_default_encoder;
        for kind in ["video encoder", "audio encoder"] {
            let error = ExportError::ComponentUnavailable {
                component: "some-plugin/mp4".parse().unwrap(),
                reason: no_default_encoder(kind, "avc1"),
            };
            for (style, listing) in [
                (HintStyle::Cli, "--list-components"),
                (HintStyle::Mcp, "list_components"),
            ] {
                let (code, hint) = classified(&error, style);
                assert_eq!(code, "component_unavailable");
                let hint = hint.unwrap();
                assert!(hint.contains("install a media plugin"), "{hint}");
                assert!(hint.contains(listing), "{hint}");
            }
        }
    }

    #[test]
    fn other_unavailable_components_have_no_hint() {
        for reason in [
            "no video encoder writes  by default".to_owned(),
            "the plugin did not describe it".to_owned(),
            "VideoToolbox is unavailable".to_owned(),
        ] {
            let error = ExportError::ComponentUnavailable {
                component: "some-plugin/mp4".parse().unwrap(),
                reason: reason.clone(),
            };
            assert_eq!(
                classified(&error, HintStyle::Cli),
                ("component_unavailable", None),
                "{reason}"
            );
        }
    }

    #[test]
    fn component_errors_have_their_codes() {
        let component: celesta_media::ComponentRef = "b/vp9".parse().unwrap();
        let muxer: celesta_media::ComponentRef = "b/mp4".parse().unwrap();
        let errors = [
            (
                ExportError::IncompatibleComponents {
                    component: component.clone(),
                    muxer,
                    reason: "it writes vp09".into(),
                },
                "incompatible_components",
            ),
            (
                ExportError::ComponentUnavailable {
                    component: component.clone(),
                    reason: "not installed".into(),
                },
                "component_unavailable",
            ),
            (
                ExportError::InvalidOption {
                    component,
                    key: "crf".into(),
                    message: "must be a number".into(),
                },
                "invalid_option",
            ),
        ];
        for (error, code) in errors {
            assert_eq!(classified(&error, HintStyle::Cli).0, code, "{error}");
        }
    }

    #[test]
    fn a_video_output_names_its_components() {
        let value = report(|progress| {
            progress(ExportProgress::Wrote(test_codec_video(
                "/tmp/film.mp4",
                None,
            )));
            Ok(())
        });
        let output = &value["outputs"][0];
        assert_eq!(
            *output,
            serde_json::json!({
                "type": "video", "path": absolute(Path::new("/tmp/film.mp4")),
                "firstFrame": 0, "frames": 30, "start": 0.0, "duration": 30.0,
                "muxer": "celesta-test-codec/file",
                "video": { "component": "celesta-test-codec/video", "codec": "ctst" },
                "audio": null, "alpha": false,
                "files": [absolute(Path::new("/tmp/film.mp4"))]
            })
        );
        assert!(output.get("format").is_none());

        let mut events = Events::default();
        let plan = OutputPlan {
            muxer: reference("ffmpeg/mp4"),
            muxer_name: "MP4".to_owned(),
            video: EncoderPlan {
                component: reference("ffmpeg/libx264"),
                name: "H.264 (x264)".to_owned(),
                settings: vec!["Slow".to_owned(), "CRF 20".to_owned()],
            },
            audio: None,
        };
        assert_eq!(
            serde_json::to_value(events.event(&ExportProgress::Output(plan))).unwrap(),
            serde_json::json!({
                "event": "output",
                "muxer": "ffmpeg/mp4",
                "muxerName": "MP4",
                "video": {
                    "component": "ffmpeg/libx264",
                    "name": "H.264 (x264)",
                    "settings": ["Slow", "CRF 20"]
                },
                "audio": null
            })
        );
    }

    #[test]
    fn progress_events_match_the_report() {
        let mut events = Events::default();
        let mut line = |event| serde_json::to_value(events.event(&event)).unwrap();
        assert_eq!(
            line(ExportProgress::Composition(CompositionInfo {
                width: 1920,
                height: 1080,
                frame_rate: Rational::new(30, 1),
                frames: 90,
            })),
            serde_json::json!({
                "event": "composition",
                "composition": { "width": 1920, "height": 1080, "fps": 30.0, "frames": 90, "duration": 3.0 }
            })
        );
        assert_eq!(
            line(ExportProgress::Rendering {
                frame: 1,
                total: 90
            }),
            serde_json::json!({ "event": "rendering", "frame": 1, "total": 90 })
        );
        assert_eq!(
            line(ExportProgress::MixingAudio),
            serde_json::json!({ "event": "mixingAudio" })
        );
        assert_eq!(
            line(ExportProgress::Warning("fallback font".into())),
            serde_json::json!({ "event": "warning", "message": "fallback font" })
        );
        // A note keeps its label among the warnings.
        assert_eq!(
            line(ExportProgress::Note("note: terms of use".into())),
            serde_json::json!({ "event": "warning", "message": "note: terms of use" })
        );
        // Written files are timed in the composition's frame rate.
        assert_eq!(
            line(ExportProgress::Wrote(ExportedFile::Frame {
                path: PathBuf::from("/tmp/check-000045.png"),
                frame: 45,
            })),
            serde_json::json!({
                "event": "wrote",
                "output": { "type": "frame", "path": absolute(Path::new("/tmp/check-000045.png")), "frame": 45, "time": 1.5 }
            })
        );
    }
}
