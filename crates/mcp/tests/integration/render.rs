//! `render_frames` and `contact_sheet` usage errors, which the server reports
//! before it starts a runtime; the renders themselves are in the runtime
//! target (`tests/runtime/render.rs`).

use crate::common::{call, client, is_error, structured};
use serde_json::json;

#[tokio::test]
async fn react_arguments_on_json_are_usage_errors() {
    let (client, _) = client().await;
    let result = call(
        &client,
        "render_frames",
        json!({ "source": "a.celesta.json", "frames": [0], "props": { "a": 1 } }),
    )
    .await;
    assert!(is_error(&result));
    assert_eq!(structured(&result)["error"]["code"], "usage");
}

#[tokio::test]
async fn still_quality_is_checked_before_rendering() {
    let (client, _) = client().await;
    for (tool, args) in [
        (
            "render_frames",
            json!({ "source": "a.tsx", "frames": [0], "quality": 80 }),
        ),
        (
            "render_frames",
            json!({ "source": "a.tsx", "frames": [0], "format": "jpeg", "quality": 0 }),
        ),
        (
            "contact_sheet",
            json!({ "source": "a.tsx", "every": 5, "output": "a.png", "quality": 80 }),
        ),
        (
            "contact_sheet",
            json!({ "source": "a.tsx", "every": 5, "output": "a.gif" }),
        ),
    ] {
        let result = call(&client, tool, args.clone()).await;
        assert!(is_error(&result), "{args}");
        assert_eq!(structured(&result)["error"]["code"], "usage", "{args}");
    }
}
