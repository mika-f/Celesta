//! The CLI's format checks that fail before anything renders, so they need no GPU.

use std::path::PathBuf;
use std::process::Command;

use celesta_codec_test::{PLUGIN_PATH_ENV, TestCodecPlugin};

/// Runs the exporter with `--json` on the minimal example and returns its one
/// stdout line. The test codec is its only media plugin, whatever the
/// developer's `CELESTA_PLUGIN_PATH` names, so the muxers it lists are known.
fn usage_error(args: &[&str]) -> serde_json::Value {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let plugin = TestCodecPlugin::new();
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .env_remove(PLUGIN_PATH_ENV)
        .arg("--plugin-dir")
        .arg(plugin.folder())
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
