//! `export_video` with a JSON project; React entries are exported in the
//! runtime target (`tests/runtime/export.rs`).

use std::path::Path;

use crate::common::{Client, call, client, is_error, serve, server, structured, text};
use celesta_codec_test::{PLUGIN_PATH_ENV, TestCodecPlugin};
use rmcp::model::ProgressNotificationParam;
use serde_json::json;
use tokio::sync::mpsc;

/// [`client_with_test_codec`](crate::common::client_with_test_codec) with the user's folders (`HOME`,
/// `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `APPDATA`) all at `home`, so the
/// server reads no presets file or plugin of the developer's.
async fn client_with_test_codec_in(
    plugin: &TestCodecPlugin,
    home: &Path,
) -> (Client, mpsc::UnboundedReceiver<ProgressNotificationParam>) {
    let mut command = server();
    command.env(PLUGIN_PATH_ENV, plugin.folder());
    for variable in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "APPDATA"] {
        command.env(variable, home);
    }
    serve(command).await
}

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
    let (client, _) = client_with_test_codec_in(&plugin, home.path()).await;
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
