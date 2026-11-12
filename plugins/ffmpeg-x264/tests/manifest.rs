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
