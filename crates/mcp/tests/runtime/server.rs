//! The tools that read the runtime's staged templates.

use crate::common::{call, client, is_error, structured, text};
use serde_json::json;

#[tokio::test]
async fn list_templates_starts_with_blank() {
    crate::require_runtime();
    let (client, _) = client().await;
    let result = call(&client, "list_templates", json!({})).await;
    assert!(!is_error(&result), "{}", text(&result));
    assert_eq!(structured(&result)["templates"][0]["name"], "blank");
}

#[tokio::test]
async fn init_project_creates_a_project() {
    crate::require_runtime();
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
