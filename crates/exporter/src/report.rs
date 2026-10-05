//! `--json`: one machine-readable result document on stdout in place of
//! progress output, for scripts and AI agents. Nothing is written to stderr.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use celesta_composition::Rational;
use celesta_exporter::{CompositionInfo, ExportError, ExportProgress, ExportedFile};
use celesta_project::LoadError;
use serde::Serialize;

use crate::progress::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    composition: Option<Composition>,
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
    /// Every project validation error, each with its JSON path.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    issues: Vec<Issue>,
}

#[derive(Serialize)]
struct Issue {
    path: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Composition {
    width: u32,
    height: u32,
    fps: f64,
    /// Total frames; frame numbers are zero-based, so the last is `frames - 1`.
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
    },
    Frame {
        path: PathBuf,
        frame: u64,
        time: f64,
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

/// Runs `export`, then prints its report. Returns whether it succeeded.
pub fn run(
    source: &Path,
    export: impl FnOnce(&mut dyn FnMut(ExportProgress)) -> Result<(), ExportError>,
) -> bool {
    let (report, ok) = build(source, export);
    print(&report);
    ok
}

fn build(
    source: &Path,
    export: impl FnOnce(&mut dyn FnMut(ExportProgress)) -> Result<(), ExportError>,
) -> (Report, bool) {
    let mut state = State::new();
    let mut composition = None;
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let result = export(&mut |event| {
        match &event {
            ExportProgress::Composition(info) => composition = Some(*info),
            ExportProgress::Wrote(file) => files.push(file.clone()),
            ExportProgress::Warning(warning) => warnings.push(warning.clone()),
            _ => {}
        }
        state.update(event);
    });
    state.finish(&result.as_ref().map(|_| ()).map_err(ToString::to_string));
    let frame_rate = composition.map(|info: CompositionInfo| info.frame_rate);
    let report = Report {
        status: if result.is_ok() {
            Status::Ok
        } else {
            Status::Error
        },
        error: result.as_ref().err().map(error_report),
        source: Some(absolute(source)),
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
            elapsed_seconds: round(state.elapsed().as_secs_f64(), 1_000.0),
            render_fps: state.metrics().0.map(|fps| round(fps, 10.0)),
        }),
    };
    (report, result.is_ok())
}

/// Prints the report for arguments rejected before anything ran.
pub fn usage_error(message: &str) {
    print(&Report {
        status: Status::Error,
        error: Some(ErrorReport {
            code: "usage",
            message: message.trim().trim_start_matches("error: ").to_owned(),
            hint: None,
            issues: Vec::new(),
        }),
        source: None,
        composition: None,
        outputs: None,
        warnings: None,
        stats: None,
    });
}

fn print(report: &Report) {
    let mut stdout = io::stdout().lock();
    // A closed stdout leaves nobody to report to.
    let _ = serde_json::to_writer(&mut stdout, report);
    let _ = writeln!(stdout);
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
        } => Output::Video {
            path: absolute(&path),
            first_frame,
            frames,
            start: seconds(first_frame, rate),
            duration: seconds(frames, rate),
            audio,
        },
        ExportedFile::Frame { path, frame } => Output::Frame {
            path: absolute(&path),
            frame,
            time: seconds(frame, rate),
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

fn error_report(error: &ExportError) -> ErrorReport {
    let mut issues = Vec::new();
    let (code, hint) = match error {
        ExportError::Project(LoadError::Io(_)) => ("project_io", None),
        ExportError::Project(LoadError::Json(_)) => ("project_json", None),
        ExportError::Project(LoadError::Validation(errors)) => {
            issues = errors
                .as_slice()
                .iter()
                .map(|error| Issue {
                    path: error.path.clone(),
                    message: error.message.clone(),
                })
                .collect();
            ("project_validation", None)
        }
        ExportError::Evaluation(_) => ("evaluation", None),
        ExportError::Render(_) => ("render", None),
        ExportError::Audio(_) => ("audio", None),
        ExportError::React(_) => ("react", None),
        ExportError::Time(_) => ("time", None),
        ExportError::Io { .. } => ("io", None),
        ExportError::Ffmpeg { .. } => ("ffmpeg", None),
        ExportError::OutputExists(_) => (
            "output_exists",
            Some("pass --overwrite to replace it, or write to another path"),
        ),
        ExportError::InvalidSelection(_) => ("invalid_selection", None),
        ExportError::UnsupportedOutput(_) => (
            "unsupported_output",
            Some("write video to a .mp4 path, or select frames with --frame or --every for PNG"),
        ),
        ExportError::UnsupportedDimensions { .. } => (
            "unsupported_dimensions",
            Some("make the composition width and height even; PNG frame exports accept odd sizes"),
        ),
        ExportError::InvalidCrf(_) => ("invalid_crf", None),
        ExportError::EmptyTimeline => ("empty_timeline", None),
        ExportError::EmptyRange => (
            "empty_range",
            Some("choose --from/--to inside the composition (composition.duration seconds)"),
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

    fn report(
        export: impl FnOnce(&mut dyn FnMut(ExportProgress)) -> Result<(), ExportError>,
    ) -> serde_json::Value {
        serde_json::to_value(build(Path::new("/tmp/film.tsx"), export).0).unwrap()
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
                    "start": 1.001, "duration": 2.002, "audio": false
                }
            ])
        );
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
}
