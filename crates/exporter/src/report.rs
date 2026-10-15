//! The machine-readable result of one export: `Celesta-export --json`
//! prints it, and the MCP server returns it from its rendering tools.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use celesta_composition::Rational;
use celesta_project::LoadError;
use celesta_react_bridge::{PropsFileChoice, ReactBridgeError};
use serde::Serialize;

use crate::{CompositionInfo, ExportError, ExportProgress, ExportedFile};

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
    hint: Option<&'static str>,
    /// Every project validation error, each with its JSON path, or every
    /// rejected project property, each with its key.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    issues: Vec<Issue>,
}

#[derive(Serialize)]
struct Issue {
    /// Absent for a property issue about a whole source rather than one key.
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Composition {
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
enum Output {
    Video {
        path: PathBuf,
        first_frame: u64,
        frames: u64,
        start: f64,
        duration: f64,
        audio: bool,
        format: &'static str,
        alpha: bool,
    },
    Frame {
        path: PathBuf,
        frame: u64,
        time: f64,
    },
    Subtitles {
        path: PathBuf,
        format: &'static str,
        /// The language of its `<Subtitles lang>` cues, when they have one.
        #[serde(skip_serializing_if = "Option::is_none")]
        lang: Option<String>,
        cues: usize,
    },
    ContactSheet {
        path: PathBuf,
        columns: u32,
        /// Tile frames in reading order, left to right and top to bottom.
        frames: Vec<u64>,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Stats {
    elapsed_seconds: f64,
    /// Average rendering speed in frames per second.
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
            ExportProgress::Warning(warning) => warnings.push(warning.clone()),
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
        composition: composition.map(|info| Composition {
            width: info.width,
            height: info.height,
            fps: fps(info.frame_rate),
            frames: info.frames,
            duration: seconds(info.frames, info.frame_rate),
        }),
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
    Report {
        status: Status::Error,
        error: Some(ErrorReport {
            code: "usage",
            message: message.trim().trim_start_matches("error: ").to_owned(),
            hint: None,
            issues: Vec::new(),
        }),
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

fn output(file: ExportedFile, frame_rate: Option<Rational>) -> Output {
    // Every file is written after the composition is reported.
    let rate = frame_rate.unwrap_or(Rational::new(1, 1));
    match file {
        ExportedFile::Video {
            path,
            first_frame,
            frames,
            audio,
            format,
        } => Output::Video {
            path: absolute(&path),
            first_frame,
            frames,
            start: seconds(first_frame, rate),
            duration: seconds(frames, rate),
            audio,
            format: format.as_str(),
            alpha: format.has_alpha(),
        },
        ExportedFile::Frame { path, frame } => Output::Frame {
            path: absolute(&path),
            frame,
            time: seconds(frame, rate),
        },
        ExportedFile::Subtitles {
            path,
            format,
            lang,
            cues,
        } => Output::Subtitles {
            path: absolute(&path),
            format: format.as_str(),
            lang,
            cues,
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
    let cli = hints == HintStyle::Cli;
    let mut issues = Vec::new();
    let (code, hint) = match error {
        ExportError::Project(LoadError::Io(_)) => ("project_io", None),
        ExportError::Project(LoadError::Json(_)) => ("project_json", None),
        ExportError::Project(LoadError::Validation(errors)) => {
            issues = errors
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
        ExportError::Render(_) => ("render", None),
        ExportError::Audio(_) => ("audio", None),
        ExportError::React(ReactBridgeError::InvalidProperties(invalid)) => {
            issues = invalid
                .iter()
                .map(|issue| Issue {
                    path: (!issue.key.is_empty()).then(|| issue.key.clone()),
                    message: format!("{}: {}", issue.source, issue.message),
                })
                .collect();
            (
                "invalid_properties",
                Some("pass values matching the entry's defineProjectProperties() schema"),
            )
        }
        ExportError::React(_) => ("react", None),
        ExportError::Properties(_) => ("invalid_properties", None),
        ExportError::Time(_) => ("time", None),
        ExportError::Io { .. } => ("io", None),
        ExportError::Ffmpeg { .. } => ("ffmpeg", None),
        ExportError::OutputExists(_) => (
            "output_exists",
            Some(if cli {
                "pass --overwrite to replace it, or write to another path"
            } else {
                "set overwrite to replace it, or write to another path"
            }),
        ),
        ExportError::InvalidSelection(_) => ("invalid_selection", None),
        ExportError::UnsupportedOutput { .. } => (
            "unsupported_output",
            Some(if cli {
                "write video to a .mp4, .mov or .webm path, or select frames with --frame or --every for PNG"
            } else {
                "write video to a .mp4, .mov or .webm path with export_video, or render PNG frames with render_frames"
            }),
        ),
        ExportError::UnsupportedSubtitles(_) => (
            "unsupported_subtitles",
            Some("write subtitles to a .srt or .vtt path"),
        ),
        ExportError::SubtitleLanguages { languages, .. } => (
            "subtitle_languages",
            Some(if languages.is_empty() {
                "give <Subtitles> a lang, or leave {lang} out of the path"
            } else {
                "put {lang} in the path, such as film.{lang}.srt, to write one file per language"
            }),
        ),
        ExportError::DuplicateOutput(_) => ("duplicate_output", None),
        ExportError::NothingToExport => (
            "nothing_to_export",
            Some(if cli {
                "give an <OUTPUT> file, --subtitles, or both"
            } else {
                "give output, subtitles, or both"
            }),
        ),
        ExportError::EncoderUnavailable { .. } => ("encoder_unavailable", None),
        ExportError::UnsupportedDimensions { .. } => (
            "unsupported_dimensions",
            Some("make the composition width and height even; PNG frame exports accept odd sizes"),
        ),
        ExportError::InvalidCrf(_) => ("invalid_crf", None),
        ExportError::EmptyTimeline => ("empty_timeline", None),
        ExportError::EmptyRange => (
            "empty_range",
            Some(if cli {
                "choose --from/--to inside the composition (composition.duration seconds)"
            } else {
                "choose start/end inside the composition (composition.duration seconds)"
            }),
        ),
        ExportError::Cancelled => ("cancelled", None),
        ExportError::TimelineTooLong => ("timeline_too_long", None),
    };
    ErrorReport {
        code,
        message: error.to_string(),
        hint,
        issues,
    }
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
    use crate::VideoFormat;

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
            progress(ExportProgress::Wrote(ExportedFile::Video {
                path: PathBuf::from("/tmp/check.mp4"),
                first_frame: 30,
                frames: 60,
                audio: false,
                format: VideoFormat::Mov,
            }));
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
                    "start": 1.001, "duration": 2.002, "audio": false,
                    "format": "mov", "alpha": true
                }
            ])
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
            progress(ExportProgress::Wrote(ExportedFile::Frame {
                path: PathBuf::from("/tmp/c-000000.png"),
                frame: 0,
            }));
            Err(ExportError::InvalidSelection("boom".into()))
        });
        assert_eq!(value["error"]["code"], "invalid_selection");
        assert_eq!(value["outputs"][0]["frame"], 0);
        assert_eq!(value["warnings"], serde_json::json!(["font fallback"]));
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
    fn mcp_hints_name_tool_arguments() {
        let errors = || {
            [
                ExportError::OutputExists(PathBuf::from("/tmp/a.mp4")),
                ExportError::UnsupportedOutput {
                    path: PathBuf::from("/tmp/a.gif"),
                    format: VideoFormat::Mp4,
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
                "write video to a .mp4, .mov or .webm path with export_video, or render PNG frames with render_frames",
                "choose start/end inside the composition (composition.duration seconds)",
            ]
        );
        for ((mcp_code, mcp_hint), (cli_code, _)) in mcp.iter().zip(&cli) {
            assert_eq!(mcp_code, cli_code);
            assert!(!mcp_hint.contains("--"), "{mcp_hint}");
        }
    }

    #[test]
    fn missing_encoders_are_reported_by_name() {
        let value = serde_json::to_value(
            build(Path::new("/tmp/film.tsx"), None, HintStyle::Cli, |_| {
                Err(ExportError::EncoderUnavailable {
                    format: VideoFormat::Webm,
                    encoder: "libvpx-vp9",
                })
            })
            .0,
        )
        .unwrap();
        assert_eq!(value["error"]["code"], "encoder_unavailable");
        let message = value["error"]["message"].as_str().unwrap();
        assert!(
            message.contains("libvpx-vp9") && message.contains("webm"),
            "{message}"
        );
    }
}
