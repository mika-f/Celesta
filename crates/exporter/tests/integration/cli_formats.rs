//! The CLI's format checks that fail before anything renders, so they need no GPU.

use std::path::PathBuf;
use std::process::Command;

/// Runs the exporter with `--json` on the minimal example and returns its one stdout line.
fn usage_error(args: &[&str]) -> serde_json::Value {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .arg("--json")
        .arg(workspace.join("examples/minimal.celesta.json"))
        .args(args)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    serde_json::from_str(&stdout).unwrap()
}

fn assert_usage_error(report: &serde_json::Value, mentions: &str) {
    assert_eq!(report["status"], "error");
    assert_eq!(report["error"]["code"], "usage");
    let message = report["error"]["message"].as_str().unwrap();
    assert!(message.contains(mentions), "{message}");
}

#[test]
fn json_reports_h264_flags_on_mov_as_usage_errors() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.mov");
    let report = usage_error(&[output.to_str().unwrap(), "--crf", "20"]);
    assert_usage_error(&report, "--crf");
}

#[test]
fn json_reports_a_mismatched_output_format_as_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.mov");
    let report = usage_error(&["--output-format", "mp4", output.to_str().unwrap()]);
    assert_usage_error(&report, "does not match");
}

#[test]
fn json_reports_a_png_path_without_frame_selection_as_unsupported_output() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.png");
    let report = usage_error(&[output.to_str().unwrap()]);
    assert_eq!(report["status"], "error");
    assert_eq!(report["error"]["code"], "unsupported_output");
    let hint = report["error"]["hint"].as_str().unwrap();
    assert!(hint.contains("--frame"), "{hint}");
    assert!(!output.exists());
}
