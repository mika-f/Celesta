//! The CLI's format checks that fail before anything renders, so they need no GPU.

use std::path::PathBuf;
use std::process::Command;

use celesta_codec_test::{PLUGIN_PATH_ENV, TestCodecPlugin};

/// The variables the user's settings and plugin folders are found from,
/// all at `home`.
fn home_env(home: &std::path::Path) -> [(&'static str, &std::path::Path); 4] {
    [
        ("HOME", home),
        ("XDG_CONFIG_HOME", home),
        ("XDG_DATA_HOME", home),
        ("APPDATA", home),
    ]
}

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
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .env_remove(PLUGIN_PATH_ENV)
        .envs(home_env(home.path()))
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
/// plugin (an empty home, so no user plugin folder or settings), and
/// returns its exit status and stdout.
fn run(args: &[&str]) -> (bool, String) {
    let plugin = TestCodecPlugin::new();
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .env_remove(PLUGIN_PATH_ENV)
        .envs(home_env(home.path()))
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

/// The listing's components other than the built-in ones.
fn test_codec_components(listed: &serde_json::Value) -> Vec<&serde_json::Value> {
    listed["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|component| component["plugin"] != "builtin")
        .collect()
}

#[test]
fn json_lists_the_test_codecs_components() {
    let listed = listing(&["--list-components", "--json"]);
    let components = test_codec_components(&listed);
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
    let components = test_codec_components(&checked);
    assert_eq!(components.len(), 4);
    assert!(
        checked["components"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["available"] == true),
        "{checked}"
    );
}

#[test]
fn the_plain_listing_has_a_line_per_component() {
    let (ok, stdout) = run(&["--list-components", "--check"]);
    assert!(ok);
    // The five built-in components, then the test codec's four.
    assert_eq!(stdout.lines().count(), 9, "{stdout}");
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

/// Runs the exporter without `--json` on the minimal example, with only
/// `plugin_dir`'s plugins and an empty home (so no user plugin folder), and
/// returns its stderr.
fn plain_failure(plugin_dir: &std::path::Path, args: &[&str]) -> String {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .env_remove(PLUGIN_PATH_ENV)
        .envs(home_env(home.path()))
        .arg("--plugin-dir")
        .arg(plugin_dir)
        .arg("--no-ui")
        .arg(workspace.join("examples/minimal.celesta.json"))
        .args(args)
        .output()
        .unwrap();
    assert!(!output.status.success());
    String::from_utf8(output.stderr).unwrap()
}

#[test]
fn a_plain_error_prints_its_hint() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.mp4");
    let plugin = TestCodecPlugin::new();
    let stderr = plain_failure(
        plugin.folder(),
        &[output.to_str().unwrap(), "--preset", "fast"],
    );
    assert!(stderr.contains("unknown preset"), "{stderr}");
    assert!(
        stderr.contains("hint: use --video-option preset=fast"),
        "{stderr}"
    );

    // With no media plugin at all, how to install one.
    let empty = tempfile::tempdir().unwrap();
    let stderr = plain_failure(empty.path(), &[output.to_str().unwrap()]);
    assert!(
        stderr.contains("hint: install Celesta's media plugin"),
        "{stderr}"
    );
    assert!(!output.exists());
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

/// A project folder holding a one-second title card, `celesta.config.json`,
/// and `presets` as its `export-presets.json`.
fn project_with_presets(presets: &serde_json::Value) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("card.celesta.json");
    let second = serde_json::json!({ "value": 1, "timescale": 1 });
    std::fs::write(
        &source,
        serde_json::json!({
            "version": 0,
            "settings": {
                "width": 64, "height": 64, "sampleRate": 8000, "duration": second,
                "frameRate": { "numerator": 2, "denominator": 1 },
            },
            "assets": {},
            "characters": {},
            "tracks": [{
                "id": "titles", "name": "Titles", "kind": "overlay",
                "items": [{
                    "id": "card", "name": "Card",
                    "range": { "start": { "value": 0, "timescale": 1 }, "duration": second },
                    "content": { "type": "text", "text": "A" },
                    "transform": { "position": { "x": 32, "y": 32 } },
                }],
            }],
            "properties": {},
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.path().join("celesta.config.json"), "{}").unwrap();
    std::fs::write(
        dir.path().join("export-presets.json"),
        serde_json::to_string_pretty(presets).unwrap(),
    )
    .unwrap();
    (dir, source)
}

/// Exports `source` to `out.mp4` beside it with the test codec and `home`
/// as the user's home, and returns the exit status, stdout, and stderr.
/// `None` when the machine has no GPU adapter.
fn export_json(
    source: &std::path::Path,
    home: &std::path::Path,
    args: &[&str],
) -> Option<(bool, String, String)> {
    let plugin = TestCodecPlugin::new();
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .env_remove(PLUGIN_PATH_ENV)
        .envs(home_env(home))
        .arg("--plugin-dir")
        .arg(plugin.folder())
        .arg("--json")
        .arg(source)
        .arg(source.with_file_name("out.mp4"))
        .args(args)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() && (stdout.contains("adapter") || stderr.contains("adapter")) {
        assert!(
            std::env::var("CELESTA_REQUIRE_GPU").as_deref() != Ok("1"),
            "GPU tests require an adapter: {stdout}"
        );
        eprintln!("skipping live export test: no GPU adapter is available");
        return None;
    }
    Some((output.status.success(), stdout, stderr))
}

fn fast_preset() -> serde_json::Value {
    serde_json::json!({
        "name": "fast",
        "muxer": "celesta-test-codec/file",
        "video": { "component": "celesta-test-codec/video", "options": { "input": "rgba8" } },
    })
}

#[test]
fn a_project_preset_exports() {
    let (_dir, source) =
        project_with_presets(&serde_json::json!({ "version": 1, "presets": [fast_preset()] }));
    let home = tempfile::tempdir().unwrap();
    let Some((ok, stdout, stderr)) = export_json(&source, home.path(), &["--preset", "fast"])
    else {
        return;
    };
    assert!(ok, "{stdout}{stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["status"], "ok", "{report}");
    assert_eq!(
        report["outputs"][0]["video"]["component"],
        "celesta-test-codec/video"
    );
}

#[test]
fn a_user_preset_is_found_and_a_stale_key_warns() {
    let (_dir, source) = project_with_presets(&serde_json::json!({ "version": 1, "presets": [] }));
    let home = tempfile::tempdir().unwrap();
    let folder = if cfg!(target_os = "macos") {
        home.path().join("Library/Application Support/Celesta")
    } else if cfg!(windows) {
        home.path().join("Celesta")
    } else {
        home.path().join("celesta")
    };
    std::fs::create_dir_all(&folder).unwrap();
    let mut preset = fast_preset();
    preset["video"]["options"]["nothing"] = serde_json::json!(1);
    std::fs::write(
        folder.join("export-presets.json"),
        serde_json::json!({ "version": 1, "presets": [preset] }).to_string(),
    )
    .unwrap();
    let Some((ok, stdout, stderr)) = export_json(&source, home.path(), &["--preset", "fast"])
    else {
        return;
    };
    assert!(ok, "{stdout}{stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        report["warnings"].to_string().contains("has no setting"),
        "{report}"
    );
}

#[test]
fn the_project_default_applies() {
    let (_dir, source) = project_with_presets(&serde_json::json!({
        "version": 1,
        "default": "fast",
        "presets": [fast_preset()],
    }));
    let home = tempfile::tempdir().unwrap();
    let Some((ok, stdout, stderr)) = export_json(&source, home.path(), &["--json-progress"]) else {
        return;
    };
    assert!(ok, "{stdout}{stderr}");
    let output = stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|line| line["event"] == "output")
        .unwrap_or_else(|| panic!("no output event: {stdout}"));
    assert_eq!(output["video"]["component"], "celesta-test-codec/video");
    let settings = output["video"]["settings"].to_string();
    assert!(settings.contains("rgba8"), "{settings}");
}

#[test]
fn a_broken_preset_file_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("presets.json");
    std::fs::write(&file, r#"{"version": 2}"#).unwrap();
    let output = dir.path().join("out.mp4");
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let plugin = TestCodecPlugin::new();
    let result = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .env_remove(PLUGIN_PATH_ENV)
        .arg("--plugin-dir")
        .arg(plugin.folder())
        .arg("--json")
        .arg(workspace.join("examples/minimal.celesta.json"))
        .arg(&output)
        .arg("--preset-file")
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("unsupported version 2"), "{stdout}");
    assert!(!output.exists());
}

#[test]
fn stills_need_no_media_plugin() {
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_celesta-exporter"));
    let folder = exe.parent().unwrap();
    if ["celesta-codec-ffmpeg", "celesta-codec-ffmpeg.exe"]
        .iter()
        .any(|name| folder.join(name).exists())
    {
        eprintln!("skipped: celesta-codec-ffmpeg sits beside the test binary");
        return;
    }
    // The folder Celesta's bundled plugins come from, beside this program
    // (`bundled_plugin_dir` in celesta-plugin-host).
    let bundled = [
        folder.join("plugins"),
        folder.join("../lib/celesta/plugins"),
    ];
    if bundled
        .iter()
        .any(|dir| std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_some()))
    {
        eprintln!("skipped: a bundled plugin folder sits beside the test binary");
        return;
    }
    let empty = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let run = |args: &[&std::ffi::OsStr]| {
        Command::new(&exe)
            .env(PLUGIN_PATH_ENV, empty.path())
            .envs(home_env(home.path()))
            .args(args)
            .output()
            .unwrap()
    };
    let output = run(&["--list-components".as_ref(), "--json".as_ref()]);
    assert!(output.status.success());
    let listed: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let ids: Vec<&str> = listed["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|component| component["id"].as_str().unwrap())
        .collect();
    for id in [
        "builtin/png",
        "builtin/jpeg",
        "builtin/webp",
        "builtin/image",
        "builtin/imageSequence",
    ] {
        assert!(ids.contains(&id), "{ids:?}");
    }
    assert!(
        ids.iter().all(|id| id.starts_with("builtin/")),
        "only the built-in components are installed: {ids:?}"
    );

    // And a still exports through them alone.
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = workspace.join("examples/editor-demo.celesta.json");
    let out = tempfile::tempdir().unwrap();
    let png = out.path().join("out.png");
    let output = run(&[
        "--json".as_ref(),
        source.as_os_str(),
        png.as_os_str(),
        "--frame".as_ref(),
        "0".as_ref(),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() && (stdout.contains("adapter") || stderr.contains("adapter")) {
        assert!(
            std::env::var("CELESTA_REQUIRE_GPU").as_deref() != Ok("1"),
            "GPU tests require an adapter: {stdout}"
        );
        eprintln!("skipping the still export: no GPU adapter is available");
        return;
    }
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let bytes = std::fs::read(&png).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "{stdout}");
    assert_eq!(&bytes[12..16], b"IHDR");
    let side = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    assert!(side(16) > 0 && side(20) > 0, "{stdout}");
    assert!(stdout.contains("out.png"), "{stdout}");
}
