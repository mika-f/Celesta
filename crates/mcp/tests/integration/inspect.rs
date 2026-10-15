use crate::common::{call, client, entry, is_error, root, runtime_built, structured, text};
use serde_json::json;

const RECT: &str = r##"
import { Composition, Rect } from '@celesta/react';
export default function Root() {
  return <Composition width={320} height={180} fps={30} durationInFrames={30}>
    <Rect width={100} height={50} fill="#336699" />
  </Composition>;
}
"##;

#[tokio::test]
async fn inspect_lists_layers() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(RECT);
    let (client, _) = client().await;
    let result = call(&client, "inspect", json!({ "entry": entry })).await;
    assert!(!is_error(&result), "{}", text(&result));
    assert!(text(&result).contains("rect 100×50"), "{}", text(&result));
    assert!(text(&result).contains("composition: 320×180 @ 30 fps, 30 frames (1 s)"));
    let frames: Vec<u64> = structured(&result)["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|frame| frame["frame"].as_u64().unwrap())
        .collect();
    assert_eq!(frames, [0, 15, 29]);
}

#[tokio::test]
async fn inspect_resolves_negative_frames() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(RECT);
    let (client, _) = client().await;
    let result = call(
        &client,
        "inspect",
        json!({ "entry": entry, "frames": [-1] }),
    )
    .await;
    assert!(!is_error(&result), "{}", text(&result));
    assert_eq!(structured(&result)["frames"][0]["frame"], 29);
}

#[tokio::test]
async fn inspect_reports_missing_files() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(
        r##"
import { Composition, Image } from '@celesta/react';
export default function Root() {
  return <Composition width={320} height={180} fps={30} durationInFrames={30}>
    <Image src="./nope.png" width={100} height={100} />
  </Composition>;
}
"##,
    );
    let (client, _) = client().await;
    let result = call(&client, "inspect", json!({ "entry": entry, "frames": [0] })).await;
    assert!(is_error(&result), "{}", text(&result));
    assert!(
        text(&result).contains("← MISSING FILE"),
        "{}",
        text(&result)
    );
    assert_eq!(
        structured(&result)["frames"][0]["missingFiles"],
        json!(["./nope.png"])
    );
}

#[tokio::test]
async fn inspect_finds_a_loop_seam() {
    if !runtime_built() {
        return;
    }
    let (_dir, fading) = entry(
        r##"
import { Composition, Rect, useCurrentFrame } from '@celesta/react';
export default function Root() {
  return <Composition width={320} height={180} fps={30} durationInFrames={30}>
    <Fade />
  </Composition>;
}
function Fade() {
  return <Rect width={100} height={50} fill="#336699" opacity={useCurrentFrame() / 30} />;
}
"##,
    );
    let (_dir2, still) = entry(RECT);
    let (client, _) = client().await;
    let seam = call(&client, "inspect", json!({ "entry": fading, "loop": true })).await;
    assert!(is_error(&seam), "{}", text(&seam));
    assert_eq!(structured(&seam)["loop"]["seamless"], false);
    let seamless = call(&client, "inspect", json!({ "entry": still, "loop": true })).await;
    assert!(!is_error(&seamless), "{}", text(&seamless));
    assert_eq!(structured(&seamless)["loop"]["seamless"], true);
}

#[tokio::test]
async fn inspect_measures_text() {
    if !runtime_built() {
        return;
    }
    let (dir, entry) = entry(
        r##"
import { Composition, Font, Rect, useTextMetrics } from '@celesta/react';
const style = { fontFamily: 'Bebas Neue', fontSize: 96 };
function Box() {
  const m = useTextMetrics('CELESTA', style, { fonts: ['./font.ttf'] });
  return <Rect width={m.width} height={m.height} fill="#ffffff" />;
}
export default function Root() {
  return <Composition width={320} height={180} fps={30} durationInFrames={30}>
    <Font src="./font.ttf" /><Box />
  </Composition>;
}
"##,
    );
    std::fs::copy(
        root().join("examples/prism/assets/fonts/BebasNeue-Regular.ttf"),
        dir.path().join("font.ttf"),
    )
    .unwrap();
    let (client, _) = client().await;
    let result = call(&client, "inspect", json!({ "entry": entry, "frames": [0] })).await;
    assert!(!is_error(&result), "{}", text(&result));
    assert!(text(&result).contains("rect "), "{}", text(&result));
}

#[tokio::test]
async fn inspect_reports_a_failing_prepare() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(
        r##"
import { Composition } from '@celesta/react';
export async function prepare() { throw new Error('boom'); }
export default function Root() {
  return <Composition width={320} height={180} fps={30} durationInFrames={30} />;
}
"##,
    );
    let (client, _) = client().await;
    let result = call(&client, "inspect", json!({ "entry": entry })).await;
    assert!(is_error(&result), "{}", text(&result));
    assert_eq!(structured(&result)["error"]["code"], "react");
    assert!(text(&result).contains("boom"), "{}", text(&result));
    let next = call(&client, "list_templates", json!({})).await;
    assert!(!is_error(&next));
}

#[tokio::test]
async fn inspect_reports_invalid_properties() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(
        r##"
import { Composition, defineProjectProperties } from '@celesta/react';
defineProjectProperties({ n: { type: 'number', defaultValue: 1 } });
export default function Root() {
  return <Composition width={320} height={180} fps={30} durationInFrames={30} />;
}
"##,
    );
    let (client, _) = client().await;
    let result = call(
        &client,
        "inspect",
        json!({ "entry": entry, "props": { "n": "x" } }),
    )
    .await;
    assert!(is_error(&result), "{}", text(&result));
    assert_eq!(structured(&result)["error"]["code"], "invalid_properties");
    assert_eq!(structured(&result)["error"]["issues"][0]["path"], "n");
}

#[tokio::test]
async fn inspect_rejects_frames_outside_the_composition() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(RECT);
    let (client, _) = client().await;
    let result = call(
        &client,
        "inspect",
        json!({ "entry": entry, "frames": [0, 30] }),
    )
    .await;
    assert!(is_error(&result), "{}", text(&result));
    assert!(
        text(&result).contains("frame 30 is out of range"),
        "{}",
        text(&result)
    );
    assert!(!text(&result).contains("OK:"), "{}", text(&result));
    let frames = structured(&result)["frames"].as_array().unwrap();
    assert_eq!(frames[0]["frame"], 0);
    assert_eq!(frames[1]["frame"], 30);
    assert!(frames[1]["error"].is_string());
}
