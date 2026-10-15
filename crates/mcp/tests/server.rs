mod common;

use std::collections::BTreeSet;

use common::{call, client, is_error, runtime_built, structured, text};
use serde_json::json;

#[tokio::test]
async fn lists_the_seven_tools() {
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
async fn list_templates_starts_with_blank() {
    if !runtime_built() {
        return;
    }
    let (client, _) = client().await;
    let result = call(&client, "list_templates", json!({})).await;
    assert!(!is_error(&result), "{}", text(&result));
    assert_eq!(structured(&result)["templates"][0]["name"], "blank");
}

#[tokio::test]
async fn init_project_creates_a_project() {
    if !runtime_built() {
        return;
    }
    let (client, _) = client().await;
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("video");
    let result = call(
        &client,
        "init_project",
        json!({ "folder": folder, "template": "slideshow" }),
    )
    .await;
    assert!(!is_error(&result), "{}", text(&result));
    assert!(folder.join("film.tsx").exists());
    assert!(folder.join("props.json").exists());
    let value = structured(&result);
    assert_eq!(value["template"]["name"], "slideshow");
    assert!(!value["edit"].as_str().unwrap().is_empty());

    let unknown = call(
        &client,
        "init_project",
        json!({ "folder": dir.path().join("other"), "template": "nope" }),
    )
    .await;
    assert!(is_error(&unknown));
    assert!(text(&unknown).contains("blank"), "{}", text(&unknown));
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
    // Without output, export_video writes only the subtitle files.
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
