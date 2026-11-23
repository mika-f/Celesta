use crate::common::{call, client, is_error, structured, text};
use crate::entry;
use serde_json::{Value, json};

/// Two chapters of 30 frames, and a layer with an explicit id in the second.
const CHAPTERS: &str = r##"
import { Composition, Rect, Sequence } from '@celesta.studio/react';
const chapters = [
  { id: 'intro', name: 'Intro', from: 0, durationInFrames: 30 },
  { id: 'main', name: 'Main', from: 30, durationInFrames: 30 },
];
export default function Root() {
  return <Composition width={320} height={180} fps={30} durationInFrames={60} chapters={chapters}>
    <Sequence from={30} durationInFrames={30}>
      <Rect id="box" width={100} height={50} fill="#336699" />
    </Sequence>
  </Composition>;
}
"##;

fn comment(id: &str, anchor: Value) -> Value {
    json!({
        "version": 1,
        "id": id,
        "entry": "scene.tsx",
        "anchor": anchor,
        "author": "Reviewer",
        "createdAt": format!("2026-11-22T10:00:0{}Z", &id[id.len() - 1..]),
        "body": format!("about {id}"),
    })
}

#[tokio::test]
async fn list_comments_places_them_and_finds_orphans() {
    crate::require_runtime();
    let (dir, entry) = entry(CHAPTERS);
    let folder = dir.path().join("celesta-comments");
    std::fs::create_dir_all(&folder).unwrap();
    for (id, anchor) in [
        (
            "c-1",
            json!({ "chapter": "main", "from": 5, "to": 15, "layer": "box" }),
        ),
        ("c-2", json!({ "chapter": "credits", "from": 0 })),
        (
            "c-3",
            json!({ "chapter": "intro", "from": 10, "layer": "gone" }),
        ),
    ] {
        std::fs::write(
            folder.join(format!("{id}.json")),
            comment(id, anchor).to_string(),
        )
        .unwrap();
    }
    let (client, _) = client().await;
    let result = call(&client, "list_comments", json!({ "entry": entry })).await;
    assert!(!is_error(&result), "{}", text(&result));
    let comments = structured(&result)["comments"].as_array().unwrap().clone();
    assert_eq!(comments.len(), 3, "{}", text(&result));
    assert_eq!(comments[0]["frame"], 35);
    assert_eq!(comments[0]["toFrame"], 45);
    assert_eq!(comments[0]["chapterName"], "Main");
    assert!(comments[0]["orphaned"].is_null());
    assert_eq!(comments[1]["orphaned"], "chapter");
    assert_eq!(comments[2]["frame"], 10);
    assert_eq!(comments[2]["orphaned"], "layer");
    assert!(
        text(&result).contains("ORPHANED: chapter \"credits\""),
        "{}",
        text(&result)
    );

    let inspected = call(&client, "inspect", json!({ "entry": entry, "frames": [0] })).await;
    assert!(
        text(&inspected).contains("comments: 3 open (1 orphaned), 0 resolved"),
        "{}",
        text(&inspected)
    );

    let resolved = call(
        &client,
        "resolve_comment",
        json!({ "entry": entry, "id": "c-1", "reply": "Shorter now", "author": "claude" }),
    )
    .await;
    assert!(!is_error(&resolved), "{}", text(&resolved));
    let open = call(
        &client,
        "list_comments",
        json!({ "entry": entry, "check": false }),
    )
    .await;
    assert_eq!(structured(&open)["comments"].as_array().unwrap().len(), 2);
    let all = call(
        &client,
        "list_comments",
        json!({ "entry": entry, "status": "all", "chapter": "main", "check": false }),
    )
    .await;
    let listed = &structured(&all)["comments"][0];
    assert_eq!(listed["status"], "resolved");
    assert_eq!(listed["replies"][0]["body"], "Shorter now");
    assert_eq!(listed["resolved"]["by"], "claude");
}
