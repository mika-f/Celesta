//! The `inspect` tool: evaluates a React entry's frames through the same
//! bridge an export uses, without rendering pixels, and summarizes them as
//! `inspect.mjs` does.

use std::collections::HashSet;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use celesta_composition::{Rational, Time};
use celesta_exporter::report::{self, HintStyle};
use celesta_exporter::{ExportError, ReactRuntimeOptions};
use celesta_react_bridge::{
    FrameEvaluation, ReactBridge, ReactBridgeError, RuntimeStop, SpawnOptions, resolve_props_file,
};
use rmcp::model::{CallToolResult, ContentBlock};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::args::reject_unknown;
use crate::server::{error_result, usage_error};
use crate::summary::{FileCheck, describe_frame, diff_scenes, fmt, format_value};

/// At most this many differences are printed per compared pair.
const LOOP_DIFFERENCE_LIMIT: usize = 20;

/// Most frames one inspection evaluates, as many as one still export
/// writes. Every frame of a longer video is checked with `inspect.mjs
/// --every 1` or `Celesta-export --inspect`, which print as they go.
pub const MAX_INSPECT_FRAMES: usize = 1_000;

/// How long an inspection runs without `timeout`, as `inspect.mjs`'s.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InspectArgs {
    /// The React entry (`.tsx`) to inspect.
    entry: PathBuf,
    /// Root composition ID; defaults to the first declared root.
    composition: Option<String>,
    /// Zero-based frames to evaluate; negative numbers count from the end
    /// (`-1` is the last frame). Default: the first, middle, and last frames.
    frames: Option<Vec<i64>>,
    /// Evaluate every `n`th frame, plus the last one, instead of `frames`.
    every: Option<u64>,
    /// Check a seamless loop instead: compare the frame after the last
    /// (evaluated as if the video went on) with frame 0, and the next with 1.
    /// Picks its own frames, so it takes neither `frames` nor `every`.
    #[serde(rename = "loop", default)]
    loop_check: bool,
    /// Seconds before the inspection stops, loading the entry (`prepare()`)
    /// included; default 180. At most 1000 frames are evaluated per call.
    timeout: Option<f64>,
    /// Project property values, over the props file's.
    props: Option<Map<String, Value>>,
    /// The props file; default: the entry's `props.json`.
    props_file: Option<PathBuf>,
    /// Arguments no field above takes; any are a usage error.
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: Map<String, Value>,
}

impl InspectArgs {
    /// How long the inspection may run: `timeout`, or [`DEFAULT_TIMEOUT`].
    pub fn timeout(&self) -> Result<Duration, String> {
        let Some(seconds) = self.timeout else {
            return Ok(DEFAULT_TIMEOUT);
        };
        Duration::try_from_secs_f64(seconds)
            .ok()
            .filter(|timeout| !timeout.is_zero())
            .ok_or_else(|| "timeout must be a positive number of seconds".to_owned())
    }
}

/// Why an inspection ended before it finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Halt {
    /// The client cancelled the request.
    Cancelled,
    /// The inspection ran past its timeout.
    TimedOut,
}

/// Ends an inspection from another thread: [`Self::halt`] kills its
/// runtime, also while the entry's `prepare()` is still running, and the
/// inspection stops before its next frame.
#[derive(Clone, Debug)]
pub struct Stop {
    timeout: Duration,
    runtime: RuntimeStop,
    halt: Arc<OnceLock<Halt>>,
}

impl Stop {
    /// A stop for an inspection that may run for `timeout`.
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            runtime: RuntimeStop::default(),
            halt: Arc::default(),
        }
    }

    /// Stops the inspection for `halt`; only the first reason counts.
    pub fn halt(&self, halt: Halt) {
        if self.halt.set(halt).is_ok() {
            self.runtime.stop();
        }
    }

    fn halted(&self) -> Option<Halt> {
        self.halt.get().copied()
    }

    /// The result of an inspection halted for `halt` while it was `doing`.
    fn result(&self, halt: Halt, doing: &str) -> CallToolResult {
        match halt {
            Halt::Cancelled => error_result("cancelled", "inspect was cancelled"),
            Halt::TimedOut => error_result(
                "timeout",
                format!(
                    "inspect timed out after {} s while {doing}; pass a larger timeout{}",
                    fmt(self.timeout.as_secs_f64()),
                    if doing == LOADING {
                        ", or check whether prepare() is waiting on something"
                    } else {
                        " or fewer frames"
                    }
                ),
            ),
        }
    }
}

/// What an inspection is doing until the entry is loaded.
const LOADING: &str = "loading the entry";

/// The usage error for a selection of `count` frames, more than
/// [`MAX_INSPECT_FRAMES`].
fn too_many(count: u64, fix: &str) -> String {
    format!("{count} frames selected; inspect evaluates at most {MAX_INSPECT_FRAMES} ({fix})")
}

pub fn inspect(args: InspectArgs, stop: &Stop) -> CallToolResult {
    if let Err(message) = reject_unknown(&args.unknown) {
        return usage_error(&message);
    }
    if args.loop_check && (args.frames.is_some() || args.every.is_some()) {
        return usage_error("loop picks its own frames; leave out frames and every");
    }
    if args.frames.is_some() && args.every.is_some() {
        return usage_error("pass frames or every, not both");
    }
    if args.every == Some(0) {
        return usage_error("every must be at least 1");
    }
    if let Some(frames) = &args.frames
        && frames.len() > MAX_INSPECT_FRAMES
    {
        return usage_error(&too_many(frames.len() as u64, "pass fewer frames"));
    }
    if let Err(message) = args.timeout() {
        return usage_error(&message);
    }
    if let Some(halt) = stop.halted() {
        return stop.result(halt, LOADING);
    }
    let entry = std::path::absolute(&args.entry).unwrap_or_else(|_| args.entry.clone());
    let props = args.props.map(|props| Value::Object(props).to_string());
    let failed = |error: ExportError| {
        let choice = resolve_props_file(&entry, args.props_file.as_deref());
        let (report, _) = report::build(&entry, choice, HintStyle::Mcp, |_| Err(error));
        CallToolResult::structured_error(serde_json::to_value(report).expect("reports serialize"))
    };
    let runtime = match ReactRuntimeOptions::for_entry(
        &entry,
        args.props_file.as_deref(),
        props.as_deref(),
    ) {
        Ok(runtime) => runtime,
        Err(error) => return failed(error),
    };
    let bridge = ReactBridge::spawn_with_options(
        &runtime.node,
        &runtime.cli_script,
        &entry,
        &runtime.properties,
        SpawnOptions {
            composition_id: args.composition,
            stop: Some(stop.runtime.clone()),
            ..SpawnOptions::default()
        },
    );
    let mut bridge = match bridge {
        Ok(bridge) => bridge,
        Err(error) => {
            return match stop.halted() {
                Some(halt) => stop.result(halt, LOADING),
                None => failed(ExportError::React(error)),
            };
        }
    };
    let entry_dir = entry.parent().unwrap_or_else(|| Path::new(".")).to_owned();
    let mut inspection = Inspection::new(&bridge, entry_dir);
    let finished = if args.loop_check {
        inspection
            .check_loop(&mut bridge, stop)
            .map_err(|halt| (halt, "checking the loop".to_owned()))
    } else {
        let frames = match select(inspection.total, args.frames.as_deref(), args.every) {
            Ok(frames) => frames,
            Err(message) => return usage_error(&message),
        };
        inspection
            .evaluate(&mut bridge, &frames, stop)
            .map_err(|halt| {
                let doing = format!(
                    "evaluating frames ({} of {} done)",
                    inspection.frames.len(),
                    frames.len()
                );
                (halt, doing)
            })
    };
    match finished {
        Ok(()) => inspection.finish(),
        Err((halt, doing)) => stop.result(halt, &doing),
    }
}

struct Inspection {
    frame_rate: Rational,
    total: u64,
    entry_dir: PathBuf,
    text: String,
    composition: Value,
    composition_id: String,
    compositions: Vec<celesta_react_bridge::ReactCompositionInfo>,
    frames: Vec<Value>,
    loop_check: Option<Value>,
    warnings: Vec<String>,
    missing: Vec<String>,
    problems: usize,
}

impl Inspection {
    fn new(bridge: &ReactBridge, entry_dir: PathBuf) -> Self {
        let metadata = bridge.metadata();
        let rate = metadata.frame_rate;
        let fps = f64::from(rate.numerator) / f64::from(rate.denominator);
        let total = metadata.duration_in_frames;
        let mut text = format!(
            "composition: {}×{} @ {} fps, {total} frames ({} s)\n",
            metadata.width,
            metadata.height,
            fmt(fps),
            fmt(total as f64 / fps)
        );
        if !metadata.component_schemas.is_empty() {
            let names: Vec<&str> = metadata
                .component_schemas
                .keys()
                .map(String::as_str)
                .collect();
            let _ = writeln!(
                text,
                "registered components with schemas: {}",
                names.join(", ")
            );
        }
        if metadata.project_property_schema.is_some() {
            let _ = writeln!(
                text,
                "project properties: {}",
                metadata.project_property_order.join(", ")
            );
        }
        if !metadata.chapters.is_empty() {
            let chapters: Vec<String> = metadata
                .chapters
                .iter()
                .map(|chapter| format!("{} [{}, {})", chapter.name, chapter.from, chapter.end()))
                .collect();
            let _ = writeln!(text, "chapters: {}", chapters.join(", "));
        }
        let mut warnings = Vec::new();
        if !metadata.width.is_multiple_of(2) || !metadata.height.is_multiple_of(2) {
            warnings.push(format!(
                "width/height {}×{} must be even for video export",
                metadata.width, metadata.height
            ));
        }
        Self {
            frame_rate: rate,
            total,
            entry_dir,
            text,
            composition_id: metadata.composition_id.clone(),
            compositions: metadata.compositions.clone(),
            composition: json!({
                "width": metadata.width,
                "height": metadata.height,
                "fps": fps,
                "frames": total,
                "duration": total as f64 / fps,
            }),
            frames: Vec::new(),
            loop_check: None,
            warnings,
            missing: Vec::new(),
            problems: 0,
        }
    }

    fn seconds(&self, frame: u64) -> f64 {
        frame as f64 * f64::from(self.frame_rate.denominator) / f64::from(self.frame_rate.numerator)
    }

    fn time(&self, frame: u64) -> Option<Time> {
        Time::frames(i64::try_from(frame).ok()?, self.frame_rate).ok()
    }

    /// Evaluates `frames` in turn; `Err` when `stop` ended it first.
    fn evaluate(
        &mut self,
        bridge: &mut ReactBridge,
        frames: &[Result<u64, i64>],
        stop: &Stop,
    ) -> Result<(), Halt> {
        for &frame in frames {
            if let Some(halt) = stop.halted() {
                return Err(halt);
            }
            let frame = match frame {
                Ok(frame) => frame,
                Err(asked) => {
                    self.problems += 1;
                    let message = format!(
                        "frame {asked} is out of range; composition has {} frames (zero-based)",
                        self.total
                    );
                    let _ = write!(self.text, "\n── frame {asked} ──\nERROR: {message}\n");
                    self.frames
                        .push(json!({ "frame": asked, "error": message }));
                    continue;
                }
            };
            let _ = write!(
                self.text,
                "\n── frame {frame} ({} s) ──\n",
                fmt(self.seconds(frame))
            );
            let evaluated = match self.time(frame) {
                Some(time) => bridge.evaluate_at(time, None),
                None => Err(ReactBridgeError::Render(format!(
                    "frame {frame} is out of range"
                ))),
            };
            match evaluated {
                Ok(evaluation) => self.describe(frame, &evaluation),
                Err(error) => {
                    // The runtime was killed for it.
                    if let Some(halt) = stop.halted() {
                        return Err(halt);
                    }
                    self.problems += 1;
                    let fatal = is_fatal(&error);
                    let message = message(&error);
                    let _ = writeln!(self.text, "ERROR: {message}");
                    self.frames.push(
                        json!({ "frame": frame, "time": self.seconds(frame), "error": message }),
                    );
                    if fatal {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn describe(&mut self, frame: u64, evaluation: &FrameEvaluation) {
        let scene = serde_json::to_value(&evaluation.scene).expect("scenes serialize");
        let audio: Vec<Value> = evaluation
            .audio
            .iter()
            .map(|clip| serde_json::to_value(clip).expect("audio clips serialize"))
            .collect();
        let mut files = FileCheck::new(&self.entry_dir);
        let lines = describe_frame(&scene, &audio, &mut files, &mut self.warnings);
        self.text.push_str(&lines);
        for src in &files.missing {
            if !self.missing.contains(src) {
                self.missing.push(src.clone());
            }
        }
        self.frames.push(json!({
            "frame": frame,
            "time": self.seconds(frame),
            "layers": scene["layers"],
            "audio": audio,
            "missingFiles": files.missing,
        }));
    }

    /// Compares the frames past the end with the start; `Err` when `stop`
    /// ended it first.
    fn check_loop(&mut self, bridge: &mut ReactBridge, stop: &Stop) -> Result<(), Halt> {
        let total = self.total;
        self.text.push_str(
            "loop check: frames after the last are evaluated as if the video went on, then compared with the start\n\n",
        );
        let mut pairs = Vec::new();
        for start in if total > 1 { vec![0, 1] } else { vec![0] } {
            if let Some(halt) = stop.halted() {
                return Err(halt);
            }
            let label = format!("frame {} vs frame {start}", total + start);
            let first = self.time(start).map(|time| bridge.evaluate_at(time, None));
            let past = self
                .time(total + start)
                .map(|time| bridge.evaluate_past_end(time));
            let (Some(first), Some(past)) = (first, past) else {
                continue;
            };
            if let Some(halt) = stop.halted() {
                return Err(halt);
            }
            match (first, past) {
                (Ok(first), Ok(past)) => {
                    let first = serde_json::to_value(&first.scene).expect("scenes serialize");
                    let past = serde_json::to_value(&past.scene).expect("scenes serialize");
                    let differences = diff_scenes(&past, &first);
                    self.problems += differences.len();
                    if differences.is_empty() {
                        let _ = writeln!(self.text, "{label}: same Scene");
                    } else {
                        let _ = writeln!(self.text, "{label}: {} difference(s)", differences.len());
                        for difference in differences.iter().take(LOOP_DIFFERENCE_LIMIT) {
                            let at = if difference.path.is_empty() {
                                "(scene)"
                            } else {
                                &difference.path
                            };
                            let _ = writeln!(
                                self.text,
                                "  {at}: {} at frame {}, {} at frame {start}",
                                format_value(difference.end.as_ref()),
                                total + start,
                                format_value(difference.start.as_ref()),
                            );
                        }
                        if differences.len() > LOOP_DIFFERENCE_LIMIT {
                            let _ = writeln!(
                                self.text,
                                "  … and {} more (structuredContent lists them all)",
                                differences.len() - LOOP_DIFFERENCE_LIMIT
                            );
                        }
                    }
                    pairs.push(json!({ "frame": total + start, "against": start, "differences": differences }));
                }
                (Err(error), _) | (_, Err(error)) => {
                    self.problems += 1;
                    let _ = writeln!(self.text, "{label}: ERROR: {}", message(&error));
                    pairs.push(json!({ "frame": total + start, "against": start, "error": message(&error) }));
                }
            }
        }
        self.loop_check = Some(json!({ "seamless": self.problems == 0, "pairs": pairs }));
        Ok(())
    }

    fn finish(mut self) -> CallToolResult {
        self.problems += self.missing.len();
        self.text.push('\n');
        let mut warnings = Vec::new();
        for warning in &self.warnings {
            if !warnings.contains(warning) {
                warnings.push(warning.clone());
            }
        }
        for warning in &warnings {
            let _ = writeln!(self.text, "warning: {warning}");
        }
        for src in &self.missing {
            let _ = writeln!(
                self.text,
                "missing: {src} (relative paths resolve from {})",
                self.entry_dir.display()
            );
        }
        let summary = match (&self.loop_check, self.problems) {
            (Some(_), 0) => "OK: the last frame runs into the first (picture only; sound is not compared)".to_owned(),
            (Some(_), _) if self.missing.is_empty() => {
                "The loop has a seam: the frames above should match. Sound is not compared; see read_docs topic loops."
                    .to_owned()
            }
            (None, 0) => "OK: entry loads and every inspected frame evaluates".to_owned(),
            (_, problems) => format!("{problems} problem(s) found"),
        };
        self.text.push_str(&summary);
        let mut value = json!({
            "composition": self.composition,
            "compositionId": self.composition_id,
            "compositions": self.compositions,
            "warnings": warnings,
            "missingFiles": self.missing,
        });
        match self.loop_check {
            Some(loop_check) => value["loop"] = loop_check,
            None => value["frames"] = Value::Array(self.frames),
        }
        let mut result = if self.problems == 0 {
            CallToolResult::success(vec![ContentBlock::text(self.text)])
        } else {
            CallToolResult::error(vec![ContentBlock::text(self.text)])
        };
        result.structured_content = Some(value);
        result
    }
}

/// The frames of a `total`-frame composition to evaluate, as `inspect.mjs` picks them, without
/// repeats. A frame outside the composition is `Err` with the number
/// asked for, so it is reported instead of silently skipped. `every`
/// selecting more than [`MAX_INSPECT_FRAMES`] is a usage error.
fn select(
    total: u64,
    frames: Option<&[i64]>,
    every: Option<u64>,
) -> Result<Vec<Result<u64, i64>>, String> {
    let wanted: Vec<i64> = match (frames, every) {
        (_, Some(step)) => {
            // Each `step`th frame, and the last when that is not one.
            let count =
                total.div_ceil(step) + u64::from(total > 0 && !(total - 1).is_multiple_of(step));
            if count > MAX_INSPECT_FRAMES as u64 {
                return Err(too_many(count, "choose a larger every"));
            }
            let mut frames: Vec<i64> = (0..total)
                .step_by(step as usize)
                .map(|f| f as i64)
                .collect();
            if total > 0 && frames.last() != Some(&(total as i64 - 1)) {
                frames.push(total as i64 - 1);
            }
            frames
        }
        (Some(frames), None) => frames.to_vec(),
        (None, None) => vec![0, (total / 2) as i64, total as i64 - 1],
    };
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    for asked in wanted {
        let frame = if asked < 0 {
            i128::from(total) + i128::from(asked)
        } else {
            i128::from(asked)
        };
        let frame = u64::try_from(frame)
            .ok()
            .filter(|&frame| frame < total)
            .ok_or(asked);
        if seen.insert(frame) {
            selected.push(frame);
        }
    }
    Ok(selected)
}

/// Whether the runtime is gone or out of step after `error`, as
/// `Celesta-export --inspect` decides it.
fn is_fatal(error: &ReactBridgeError) -> bool {
    !matches!(
        error,
        ReactBridgeError::Render(_)
            | ReactBridgeError::Time(_)
            | ReactBridgeError::OpenEndUnsupported
    )
}

/// The entry's own message for errors it raised; the bridge's otherwise.
fn message(error: &ReactBridgeError) -> String {
    match error {
        ReactBridgeError::EntryFailed(message) | ReactBridgeError::Render(message) => {
            message.clone()
        }
        error => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_TIMEOUT, Halt, InspectArgs, MAX_INSPECT_FRAMES, Stop, inspect, select};
    use rmcp::model::CallToolResult;
    use serde_json::{Value, json};
    use std::time::Duration;

    fn args(value: Value) -> InspectArgs {
        serde_json::from_value(value).expect("inspect arguments")
    }

    fn run(value: Value) -> CallToolResult {
        inspect(args(value), &Stop::new(DEFAULT_TIMEOUT))
    }

    fn code(result: &CallToolResult) -> &str {
        assert_eq!(result.is_error, Some(true));
        result.structured_content.as_ref().unwrap()["error"]["code"]
            .as_str()
            .unwrap()
    }

    fn message(result: &CallToolResult) -> &str {
        result.structured_content.as_ref().unwrap()["error"]["message"]
            .as_str()
            .unwrap()
    }

    #[test]
    fn loop_with_frames_or_every_is_a_usage_error() {
        for extra in [json!({ "frames": [5] }), json!({ "every": 10 })] {
            let mut value = json!({ "entry": "missing.tsx", "loop": true });
            value
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let result = run(value);
            assert_eq!(code(&result), "usage");
            assert_eq!(
                message(&result),
                "loop picks its own frames; leave out frames and every"
            );
        }
    }

    #[test]
    fn too_many_frames_are_a_usage_error() {
        let frames: Vec<usize> = (0..=MAX_INSPECT_FRAMES).collect();
        let result = run(json!({ "entry": "missing.tsx", "frames": frames }));
        assert_eq!(code(&result), "usage");
        assert_eq!(
            message(&result),
            "1001 frames selected; inspect evaluates at most 1000 (pass fewer frames)"
        );
    }

    #[test]
    fn every_past_the_cap_is_refused_before_listing_frames() {
        // A 5-minute film at 60 fps.
        let error = select(18_000, None, Some(1)).unwrap_err();
        assert_eq!(
            error,
            "18000 frames selected; inspect evaluates at most 1000 (choose a larger every)"
        );
        // 0, 18, …, 17982, which is the last: exactly the cap.
        assert_eq!(select(17_983, None, Some(18)).unwrap().len(), 1000);
        // The last frame, 17983, makes one more.
        assert!(select(17_984, None, Some(18)).is_err());
        assert_eq!(
            select(10, None, Some(3)).unwrap(),
            [Ok(0), Ok(3), Ok(6), Ok(9)]
        );
        assert_eq!(
            select(10, None, Some(4)).unwrap(),
            [Ok(0), Ok(4), Ok(8), Ok(9)]
        );
    }

    #[test]
    fn select_drops_repeats_and_keeps_order() {
        assert_eq!(
            select(30, Some(&[5, -1, 29, 5, 40, 40, 0]), None).unwrap(),
            [Ok(5), Ok(29), Err(40), Ok(0)]
        );
        assert_eq!(select(30, None, None).unwrap(), [Ok(0), Ok(15), Ok(29)]);
    }

    #[test]
    fn timeout_must_be_positive() {
        for timeout in [json!(0), json!(-1), json!(1e300)] {
            let result = run(json!({ "entry": "missing.tsx", "timeout": timeout }));
            assert_eq!(code(&result), "usage");
            assert_eq!(
                message(&result),
                "timeout must be a positive number of seconds"
            );
        }
        let timeout = |value: Value| args(json!({ "entry": "a.tsx", "timeout": value })).timeout();
        assert_eq!(timeout(json!(0.5)), Ok(Duration::from_millis(500)));
        assert_eq!(
            args(json!({ "entry": "a.tsx" })).timeout(),
            Ok(DEFAULT_TIMEOUT)
        );
    }

    #[test]
    fn an_already_cancelled_inspection_loads_nothing() {
        let stop = Stop::new(DEFAULT_TIMEOUT);
        stop.halt(Halt::Cancelled);
        // The entry does not exist: loading it would fail differently.
        let result = inspect(args(json!({ "entry": "missing.tsx" })), &stop);
        assert_eq!(code(&result), "cancelled");
        assert_eq!(message(&result), "inspect was cancelled");
    }

    #[test]
    fn the_first_halt_wins() {
        let stop = Stop::new(Duration::from_secs(2));
        stop.halt(Halt::TimedOut);
        stop.halt(Halt::Cancelled);
        let result = inspect(args(json!({ "entry": "missing.tsx" })), &stop);
        assert_eq!(code(&result), "timeout");
        assert_eq!(
            message(&result),
            "inspect timed out after 2 s while loading the entry; pass a larger timeout, or check whether prepare() is waiting on something"
        );
    }
}
