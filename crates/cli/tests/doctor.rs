mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::Home;
use serde_json::{Value, json};

fn report(output: &Output) -> Value {
    let text = String::from_utf8_lossy(&output.stdout);
    let report: Value = serde_json::from_str(&text).unwrap_or_else(|_| panic!("{output:?}"));
    assert_eq!(report["schemaVersion"], 1);
    report
}

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/editor-demo.celesta.json")
}

#[test]
fn doctor_json_usage_has_the_same_schema() {
    let output = Home::new()
        .command()
        .args(["doctor", "--json", "project.celesta.json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let report = report(&output);
    assert_eq!(report["status"], "error");
    assert_eq!(report["checks"]["output"]["code"], "usage");
}

#[test]
fn doctor_rejects_audio_options_without_an_encoder() {
    let output = Home::new()
        .command()
        .args([
            "doctor",
            "--json",
            "film.tsx",
            "film.gif",
            "--react",
            "--audio-encoder",
            "none",
            "--audio-option",
            "key=value",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let report = report(&output);
    assert_eq!(report["checks"]["output"]["code"], "usage");
    assert!(
        report["checks"]["output"]["message"]
            .as_str()
            .unwrap()
            .contains("--audio-option needs an audio encoder")
    );
}

#[test]
fn doctor_checks_range_and_presets_before_prerequisites() {
    let home = Home::new();
    for (args, message) in [
        (
            &["--from", "1", "--to", "0"][..],
            "--to must be after --from",
        ),
        (
            &["--preset-file", "missing.json"][..],
            "--preset-file missing.json",
        ),
    ] {
        let output = home
            .command()
            .args([
                "doctor", "--json", "--driver", "metal", "film.tsx", "film.gif", "--react",
            ])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        let report = report(&output);
        assert_eq!(report["checks"]["graphics"]["status"], "skipped");
        assert_eq!(report["checks"]["output"]["code"], "usage");
        assert!(
            report["checks"]["output"]["message"]
                .as_str()
                .unwrap()
                .contains(message)
        );
    }
}

#[test]
fn missing_bundled_node_fails_preflight() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir(home.path().join("runtime")).unwrap();
    let binary = home
        .path()
        .join(format!("celesta{}", std::env::consts::EXE_SUFFIX));
    let built = env!("CARGO_BIN_EXE_celesta");
    if fs::hard_link(built, &binary).is_err() {
        fs::copy(built, &binary).unwrap();
    }
    let output = Command::new(binary)
        .args(["doctor", "--json", "--react", "film.tsx", "film.mov"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report = report(&output);
    assert_eq!(report["checks"]["node"]["code"], "node_unavailable");
    assert_eq!(report["checks"]["output"]["status"], "skipped");
}

#[test]
#[cfg(target_os = "linux")]
fn requested_adapter_must_work() {
    let output = Home::new()
        .command()
        .args(["doctor", "--json", "--driver", "metal"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report = report(&output);
    assert_eq!(report["checks"]["graphics"]["code"], "graphics_unavailable");
}

#[test]
fn a_render_probe_obeys_the_output_path_and_project_lock() {
    let home = Home::new();
    let source = home.path().join("project.celesta.json");
    fs::copy(project(), &source).unwrap();
    let destination = home.path().join("out/frame.png");
    let run = |extra: &[&str]| {
        home.command()
            .args(["doctor", "--json", "--frame", "0"])
            .args([&source, &destination])
            .args(extra)
            .output()
            .unwrap()
    };

    let output = run(&[]);
    let checks = report(&output);
    assert_eq!(checks["checks"]["node"]["status"], "skipped");
    if checks["checks"]["graphics"]["code"] == "graphics_unavailable" {
        eprintln!("skipping: no graphics adapter");
        return;
    }
    assert!(output.status.success(), "{checks}");
    assert_eq!(checks["checks"]["output"]["status"], "ok");
    assert!(!destination.parent().unwrap().exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let trailing_slash = format!("{}/", destination.display());
        let output = home
            .command()
            .args(["doctor", "--json", "--frame", "0"])
            .arg(&source)
            .arg(trailing_slash)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(report(&output)["checks"]["output"]["code"], "io");
        assert!(!destination.parent().unwrap().exists());

        let parent = destination.parent().unwrap();
        symlink(home.path().join("missing-target"), parent).unwrap();
        let output = run(&[]);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(report(&output)["checks"]["output"]["code"], "io");
        fs::remove_file(parent).unwrap();
    }

    fs::create_dir(destination.parent().unwrap()).unwrap();
    fs::write(&destination, b"keep this file").unwrap();
    let output = run(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report(&output)["checks"]["output"]["code"], "output_exists");
    assert!(run(&["--overwrite"]).status.success());
    assert_eq!(fs::read(&destination).unwrap(), b"keep this file");
    assert_eq!(
        fs::read_dir(destination.parent().unwrap()).unwrap().count(),
        1
    );

    fs::write(
        home.path().join("celesta.config.json"),
        r#"{"plugins":{"dependencies":{"acme.charts":"1"}}}"#,
    )
    .unwrap();
    let version = env!("CARGO_PKG_VERSION");
    let hash = format!("sha256:{}", "0".repeat(64));
    fs::write(
        home.path().join("celesta.lock"),
        json!({
            "format": 1,
            "celesta": version,
            "plugins": {
                "acme.charts": {
                    "celesta": format!("={version}"),
                    "registry": "https://registry.example.test",
                    "targets": { "any": { "sha256": hash, "treeHash": hash } },
                    "version": "1.0.0"
                }
            },
            "targets": []
        })
        .to_string(),
    )
    .unwrap();
    let output = run(&["--overwrite"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        report(&output)["checks"]["output"]["code"],
        "plugins_unsatisfied"
    );
    assert_eq!(fs::read(&destination).unwrap(), b"keep this file");
}

#[test]
#[cfg(unix)]
fn listing_a_required_component_does_not_count_as_launching_it() {
    use std::os::unix::fs::PermissionsExt;

    let home = Home::new();
    let plugin = home.path().join("test-media");
    fs::create_dir(&plugin).unwrap();
    let program = plugin.join("probe");
    fs::write(
        &program,
        "#!/bin/sh\necho launched > \"$PROBE_LOG\"\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        plugin.join(celesta_plugin_host::MANIFEST),
        json!({
            "id": "probe",
            "version": "1.0.0",
            "protocol": 2,
            "celesta": concat!("=", env!("CARGO_PKG_VERSION")),
            "roles": ["media"],
            "executable": { celesta_plugin_host::platform().0: "probe" },
            "media": {
                "videoEncoders": [{ "id": "png", "name": "Probe", "codecs": ["png"] }]
            }
        })
        .to_string(),
    )
    .unwrap();

    let log = home.path().join("probe.log");
    let requested = home.path().join("frame.png");
    let output = home
        .command()
        .env("PROBE_LOG", &log)
        .args([
            "doctor",
            "--json",
            "--frame",
            "0",
            "--video-encoder",
            "probe/png",
        ])
        .arg("--plugin-dir")
        .arg(plugin)
        .arg(project())
        .arg(&requested)
        .output()
        .unwrap();
    let checks = report(&output);
    if checks["checks"]["graphics"]["code"] == "graphics_unavailable" {
        eprintln!("skipping: no graphics adapter");
        return;
    }
    assert_eq!(output.status.code(), Some(1), "{checks}");
    assert_eq!(
        checks["checks"]["output"]["code"], "component_unavailable",
        "{checks}"
    );
    assert_eq!(fs::read_to_string(log).unwrap(), "launched\n");
    assert!(!requested.exists());
}
