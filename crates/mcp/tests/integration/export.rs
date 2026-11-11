//! `export_video` with a JSON project; React entries are exported in the
//! runtime target (`tests/runtime/export.rs`).

use crate::common::{call, client, client_with_test_codec, is_error, structured, text};
use celesta_codec_test::TestCodecPlugin;
use serde_json::json;

#[tokio::test]
async fn export_video_uses_a_project_preset() {
    let dir = tempfile::tempdir().unwrap();
    let second = json!({ "value": 1, "timescale": 1 });
    let source = dir.path().join("card.celesta.json");
    std::fs::write(
        &source,
        json!({
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
        json!({
            "version": 1,
            "presets": [{
                "name": "fast",
                "muxer": "celesta-test-codec/file",
                "video": { "component": "celesta-test-codec/video", "options": { "input": "rgba8" } },
            }],
        })
        .to_string(),
    )
    .unwrap();
    let output = dir.path().join("preset.mp4");
    let home = tempfile::tempdir().unwrap();
    // Bound before the client, so it drops after it.
    let plugin = TestCodecPlugin::new();
    let (client, _) = client_with_test_codec(&plugin, home.path()).await;
    let result = call(
        &client,
        "export_video",
        json!({ "source": source, "output": output, "preset": "fast" }),
    )
    .await;
    if is_error(&result) && text(&result).contains("adapter") {
        eprintln!("skipping: no GPU adapter is available");
    } else {
        assert!(!is_error(&result), "{}", text(&result));
        assert_eq!(structured(&result)["outputs"][0]["type"], "video");
        let media = celesta_codec_test::read(&output).unwrap();
        assert_eq!(media.video.unwrap().frames.len(), 2);
    }
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn export_video_refuses_png_output() {
    let (client, _) = client().await;
    let result = call(
        &client,
        "export_video",
        json!({ "source": "a.tsx", "output": "a.png" }),
    )
    .await;
    assert!(is_error(&result));
    assert_eq!(structured(&result)["error"]["code"], "usage");
}

/// A project declaring a registry plugin it has no `celesta.lock` for
/// fails before the React runtime starts, so this needs no Node build.
#[tokio::test]
async fn export_tool_reports_an_unsatisfied_lock() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("celesta.config.json"),
        r#"{"plugins":{"dependencies":{"acme.charts":"1"}}}"#,
    )
    .unwrap();
    let source = project.path().join("scene.tsx");
    std::fs::write(&source, "export default function Root() { return null; }\n").unwrap();
    let output = project.path().join("out.mp4");
    let home = tempfile::tempdir().unwrap();
    // Bound before the client, so it drops after it.
    let plugin = TestCodecPlugin::new();
    let (client, _) = client_with_test_codec(&plugin, home.path()).await;
    for (tool, args) in [
        (
            "export_video",
            json!({ "source": source, "output": output }),
        ),
        ("inspect", json!({ "entry": source })),
    ] {
        let result = call(&client, tool, args).await;
        assert!(is_error(&result), "{tool}: {}", text(&result));
        let error = &structured(&result)["error"];
        assert_eq!(error["code"], "plugins_unsatisfied", "{tool}: {error}");
        let message = error["message"].as_str().unwrap();
        assert!(message.contains("celesta plugin install"), "{message}");
    }
    assert!(!output.exists());
    assert!(!project.path().join("celesta.lock").exists());
    client.cancel().await.unwrap();
}
