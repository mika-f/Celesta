//! Discovery agrees with the complete embedded manifest for this build.
use std::process::Command;

#[test]
fn installed_binary_manifest_matches_its_source() {
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg"))
        .arg("--manifest")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let source: serde_json::Value = serde_json::from_str(include_str!("../plugin.json")).unwrap();
    let expected = serde_json::json!({ "id": source["id"], "name": source["name"],
        "version": source["version"], "roles": source["roles"], "media": source["media"] });
    assert_eq!(actual, expected);
    assert_eq!(actual["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn linked_build_opens_its_encoders() {
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg"))
        .arg("--check-build")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
}

#[test]
fn an_unknown_argument_is_a_usage_error() {
    // A misspelled gate, or one an older program lacks, must not pass by
    // serving the empty stdin and exiting 0.
    for arguments in [&["--check-licence"][..], &["--manifest", "--check-build"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg"))
            .args(arguments)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{arguments:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("unknown arguments")
                && stderr.contains("--manifest | --check-build | --check-license"),
            "{stderr}"
        );
    }
}
