use std::collections::BTreeSet;

use crate::common::{call, client, is_error, structured, text};
use celesta_codec_test::TestCodecPlugin;
use serde_json::json;

#[tokio::test]
async fn lists_the_nine_tools() {
    let (client, _) = client().await;
    let names: BTreeSet<String> = client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect();
    let expected: BTreeSet<String> = [
        "inspect",
        "render_frames",
        "contact_sheet",
        "export_video",
        "list_templates",
        "init_project",
        "read_docs",
        "list_components",
        "describe_component",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(names, expected);
}

#[tokio::test]
async fn instructions_name_the_workflow() {
    let (client, _) = client().await;
    let info = client.peer_info().unwrap();
    let instructions = info.instructions.as_deref().unwrap();
    for tool in ["inspect", "render_frames", "read_docs"] {
        assert!(
            instructions.contains(tool),
            "{tool} missing:\n{instructions}"
        );
    }
}

#[tokio::test]
async fn read_docs_lists_and_reads() {
    let (client, _) = client().await;
    let index = call(&client, "read_docs", json!({})).await;
    assert!(!is_error(&index));
    assert!(text(&index).contains("animation"), "{}", text(&index));

    let export = call(&client, "read_docs", json!({ "topic": "export" })).await;
    assert!(!is_error(&export));
    assert!(text(&export).contains("JSON results"), "{}", text(&export));

    let unknown = call(&client, "read_docs", json!({ "topic": "nope" })).await;
    assert!(is_error(&unknown));
}

#[tokio::test]
async fn optional_arguments_are_not_required() {
    let (client, _) = client().await;
    let tools = client.list_all_tools().await.unwrap();
    let required = |name: &str| -> Vec<String> {
        let tool = tools.iter().find(|tool| tool.name == name).unwrap();
        tool.input_schema
            .get("required")
            .and_then(|required| required.as_array())
            .map(|names| {
                names
                    .iter()
                    .map(|n| n.as_str().unwrap().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    assert!(
        required("read_docs").is_empty(),
        "{:?}",
        required("read_docs")
    );
    assert_eq!(required("inspect"), ["entry"]);
    assert_eq!(required("export_video"), ["source"]);
}

#[tokio::test]
async fn unknown_arguments_are_usage_errors() {
    let (client, _) = client().await;
    for (tool, args) in [
        (
            "render_frames",
            json!({ "source": "a.tsx", "every": 5, "from": "1", "to": "2" }),
        ),
        (
            "contact_sheet",
            json!({ "source": "a.tsx", "every": 5, "frame": 0 }),
        ),
        (
            "export_video",
            json!({ "source": "a.tsx", "output": "a.mp4", "from": "1" }),
        ),
        ("inspect", json!({ "entry": "a.tsx", "frame": 0 })),
    ] {
        let result = call(&client, tool, args).await;
        assert!(is_error(&result), "{tool}: {}", text(&result));
        let error = &structured(&result)["error"];
        assert_eq!(error["code"], "usage", "{tool}");
        let message = error["message"].as_str().unwrap();
        assert!(message.contains("unknown argument"), "{tool}: {message}");
    }
}

#[tokio::test]
async fn list_components_returns_the_test_codec() {
    let plugin = TestCodecPlugin::new();
    let (client, _) = crate::common::client_with_test_codec(&plugin).await;
    let result = call(&client, "list_components", json!({})).await;
    assert!(!is_error(&result), "{}", text(&result));
    let components = structured(&result)["components"]
        .as_array()
        .unwrap()
        .clone();
    let has = |id: &str, kind: &str| {
        components
            .iter()
            .any(|c| c["id"] == id && c["kind"] == kind)
    };
    assert!(
        has("celesta-test-codec/video", "videoEncoder"),
        "{components:?}"
    );
    assert!(has("celesta-test-codec/file", "muxer"), "{components:?}");
}

#[tokio::test]
async fn describe_component_describes_and_rejects() {
    let plugin = TestCodecPlugin::new();
    let (client, _) = crate::common::client_with_test_codec(&plugin).await;
    let result = call(
        &client,
        "describe_component",
        json!({ "component": "celesta-test-codec/video" }),
    )
    .await;
    assert!(!is_error(&result), "{}", text(&result));
    let described = structured(&result);
    assert_eq!(described["id"], "celesta-test-codec/video");
    assert_eq!(described["kind"], "videoEncoder");
    assert_eq!(described["available"], true);
    assert!(described["settings"].is_array());

    let unknown = call(
        &client,
        "describe_component",
        json!({ "component": "celesta-test-codec/nope" }),
    )
    .await;
    assert!(is_error(&unknown));
    assert_eq!(
        structured(&unknown)["error"]["code"],
        "component_unavailable"
    );

    let malformed = call(
        &client,
        "describe_component",
        json!({ "component": "video" }),
    )
    .await;
    assert!(is_error(&malformed));
    assert_eq!(structured(&malformed)["error"]["code"], "usage");
}
