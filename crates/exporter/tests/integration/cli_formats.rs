//! The CLI's format checks that fail before anything renders, so they need no GPU.

use std::path::PathBuf;
use std::process::Command;

use celesta_codec_test::{PLUGIN_PATH_ENV, TestCodecPlugin};

/// Runs the exporter with `--json` on the minimal example and returns its one
/// stdout line. The test codec is its only media plugin, whatever the
/// developer's `CELESTA_PLUGIN_PATH` names, so the muxers it lists are known.
fn usage_error(args: &[&str]) -> serde_json::Value {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    failure(&workspace.join("examples/minimal.celesta.json"), args)
}

/// [`usage_error`] for the project at `source`.
fn failure(source: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let plugin = TestCodecPlugin::new();
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .env_remove(PLUGIN_PATH_ENV)
        .arg("--plugin-dir")
        .arg(plugin.folder())
        .arg("--json")
        .arg(source)
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

/// Runs the exporter with `args` and the test codec as its only media
/// plugin, and returns its exit status and stdout.
fn run(args: &[&str]) -> (bool, String) {
    let plugin = TestCodecPlugin::new();
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .env_remove(PLUGIN_PATH_ENV)
        .arg("--plugin-dir")
        .arg(plugin.folder())
        .args(args)
        .output()
        .unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
    )
}

fn listing(args: &[&str]) -> serde_json::Value {
    let (ok, stdout) = run(args);
    assert!(ok, "{stdout}");
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    serde_json::from_str(&stdout).unwrap()
}

#[test]
fn json_reports_a_bad_component_reference_as_a_usage_error() {
    let report = usage_error(&["out.mp4", "--video-encoder", "libx264"]);
    assert_eq!(report["status"], "error");
    assert_eq!(report["error"]["code"], "usage");
    let message = report["error"]["message"].as_str().unwrap();
    assert!(message.contains("<plugin>/<component>"), "{message}");
}

#[test]
fn json_lists_the_test_codecs_components() {
    let listed = listing(&["--list-components", "--json"]);
    let components = listed["components"].as_array().unwrap();
    assert_eq!(components.len(), 4, "{listed}");
    let muxer = components
        .iter()
        .find(|component| component["kind"] == "muxer")
        .unwrap();
    assert_eq!(muxer["trimsLeadingAudio"], true);
    assert_eq!(muxer["plugin"], "celesta-test-codec");
    assert!(muxer.get("audio").is_none() || muxer["audio"].is_array());
    assert!(components.iter().all(|c| c.get("available").is_none()));

    let checked = listing(&["--list-components", "--check", "--json"]);
    let components = checked["components"].as_array().unwrap();
    assert_eq!(components.len(), 4);
    assert!(
        components.iter().all(|c| c["available"] == true),
        "{checked}"
    );
}

#[test]
fn the_plain_listing_has_a_line_per_component() {
    let (ok, stdout) = run(&["--list-components", "--check"]);
    assert!(ok);
    assert_eq!(stdout.lines().count(), 4, "{stdout}");
    assert!(
        stdout.lines().all(|line| line.ends_with("available")),
        "{stdout}"
    );
    assert!(
        stdout.contains("celesta-test-codec/video  videoEncoder"),
        "{stdout}"
    );
}

#[test]
fn json_describes_the_test_video_encoder() {
    let described = listing(&["--describe", "celesta-test-codec/video", "--json"]);
    assert_eq!(described["id"], "celesta-test-codec/video");
    assert_eq!(described["available"], true);
    assert_eq!(described["settings"][0]["key"], "input");
    assert!(described["inputFormats"].is_array(), "{described}");
}

#[test]
fn describing_an_unknown_component_fails() {
    let (ok, stdout) = run(&["--describe", "celesta-test-codec/nothing", "--json"]);
    assert!(!ok);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["error"]["code"], "component_unavailable");
    assert!(
        report["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not installed"),
        "{report}"
    );
}

#[test]
fn an_x264_preset_fails_with_the_hint() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.mp4");
    let report = usage_error(&[output.to_str().unwrap(), "--preset", "ultrafast"]);
    assert_eq!(report["error"]["code"], "unknown_preset");
    let hint = report["error"]["hint"].as_str().unwrap();
    assert!(hint.contains("--video-option preset=ultrafast"), "{hint}");
}

#[test]
fn a_plugins_refusal_is_an_invalid_option() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.mp4");
    // A one-second project: an empty timeline fails before any encoder opens.
    let source = dir.path().join("project.celesta.json");
    std::fs::write(
        &source,
        serde_json::json!({
            "version": 0,
            "settings": {
                "width": 4, "height": 4, "duration": [1, 1],
                "frameRate": { "numerator": 4, "denominator": 1 }, "sampleRate": 48000,
            },
            "assets": {}, "characters": {}, "tracks": [], "properties": {},
        })
        .to_string(),
    )
    .unwrap();
    let report = failure(
        &source,
        &[
            output.to_str().unwrap(),
            "--video-option",
            "input=rgba8",
            "--video-option",
            "alpha=true",
        ],
    );
    assert_eq!(report["error"]["code"], "invalid_option", "{report}");
    let message = report["error"]["message"].as_str().unwrap();
    assert!(message.contains("alpha"), "{message}");
}

#[test]
fn the_plain_description_lists_each_setting() {
    let (ok, stdout) = run(&["--describe", "celesta-test-codec/video"]);
    assert!(ok, "{stdout}");
    let mut lines = stdout.lines();
    let header = lines.next().unwrap();
    assert!(
        header.starts_with("celesta-test-codec/video  videoEncoder"),
        "{header}"
    );
    assert!(header.ends_with("available"), "{header}");
    assert!(stdout.contains("\ninput  select  default "), "{stdout}");
    assert!(stdout.contains("    options: "), "{stdout}");
}
