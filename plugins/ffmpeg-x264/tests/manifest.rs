//! Discovery agrees with the complete embedded manifest for this build.
use std::process::Command;

#[test]
fn installed_binary_manifest_matches_its_source() {
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg-x264"))
        .arg("--manifest")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let source: serde_json::Value = serde_json::from_str(include_str!("../plugin.json")).unwrap();
    let expected = serde_json::json!({ "id": source["id"], "version": source["version"],
        "roles": source["roles"], "media": source["media"] });
    assert_eq!(actual, expected);
    assert_eq!(actual["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn linked_build_opens_its_encoders() {
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg-x264"))
        .arg("--check-build")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
}

#[test]
fn the_in_process_plugin_without_a_display_name_is_its_id() {
    assert_eq!(celesta_codec_ffmpeg_x264::registered().name, "ffmpeg-x264");
}

#[test]
fn help_says_what_the_program_is_and_what_to_run_instead() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg-x264"))
            .arg(flag)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success(), "{flag}: {output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in [
            concat!("celesta-codec-ffmpeg-x264 ", env!("CARGO_PKG_VERSION")),
            "starts it as a subprocess",
            "celesta doctor",
            "celesta export",
            "--check-build",
        ] {
            assert!(
                stdout.contains(expected),
                "{flag} lacks {expected:?}: {stdout}"
            );
        }
    }
}

#[test]
fn version_prints_the_program_and_its_version() {
    for flag in ["--version", "-V"] {
        let output = Command::new(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg-x264"))
            .arg(flag)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success(), "{flag}: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            concat!(
                "celesta-codec-ffmpeg-x264 ",
                env!("CARGO_PKG_VERSION"),
                "\n"
            )
        );
    }
}
