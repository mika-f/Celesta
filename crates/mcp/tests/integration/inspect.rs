use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::common::{call, client, entry, is_error, params, root, runtime_built, structured, text};
use rmcp::model::{ClientRequest, Request};
use rmcp::service::PeerRequestOptions;
use serde_json::json;

const RECT: &str = r##"
import { Composition, Rect } from '@celesta.studio/react';
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
import { Composition, Image } from '@celesta.studio/react';
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
import { Composition, Rect, useCurrentFrame } from '@celesta.studio/react';
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
import { Composition, Font, Rect, useTextMetrics } from '@celesta.studio/react';
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
import { Composition } from '@celesta.studio/react';
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
import { Composition, defineProjectProperties } from '@celesta.studio/react';
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

#[tokio::test]
async fn inspect_refuses_every_past_the_frame_cap() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(
        r##"
import { Composition, Rect } from '@celesta.studio/react';
export default function Root() {
  return <Composition width={320} height={180} fps={30} durationInFrames={1001}>
    <Rect width={100} height={50} fill="#336699" />
  </Composition>;
}
"##,
    );
    let (client, _) = client().await;
    let result = call(&client, "inspect", json!({ "entry": entry, "every": 1 })).await;
    assert!(is_error(&result), "{}", text(&result));
    assert_eq!(structured(&result)["error"]["code"], "usage");
    assert_eq!(
        structured(&result)["error"]["message"],
        "1001 frames selected; inspect evaluates at most 1000 (choose a larger every)"
    );
}

/// An entry whose `prepare()` never resolves, after writing its runtime's
/// process id to `pid.txt` beside it.
fn hanging_entry() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let (dir, entry) = entry("");
    let pid_file = dir.path().join("pid.txt");
    let source = format!(
        r##"
import {{ writeFileSync }} from 'node:fs';
import {{ Composition }} from '@celesta.studio/react';
export async function prepare() {{
  writeFileSync({}, String(process.pid));
  await new Promise(() => setInterval(() => {{}}, 1000));
}}
export default function Root() {{
  return <Composition width={{320}} height={{180}} fps={{30}} durationInFrames={{30}} />;
}}
"##,
        serde_json::to_string(&pid_file).unwrap()
    );
    std::fs::write(&entry, source).unwrap();
    (dir, entry, pid_file)
}

/// Waits for `prepare()` to write its runtime's process id.
async fn wait_for_pid(pid_file: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(pid) = std::fs::read_to_string(pid_file)
            && !pid.is_empty()
        {
            return pid;
        }
        assert!(Instant::now() < deadline, "prepare() did not start");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Whether process `pid` still runs.
#[cfg(unix)]
fn running(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Whether process `pid` still runs: `tasklist` lists it by its id.
#[cfg(windows)]
fn running(pid: &str) -> bool {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .expect("tasklist runs");
    String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
}

/// Waits for process `pid` to end.
async fn wait_for_exit(pid: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while running(pid) {
        assert!(Instant::now() < deadline, "the runtime {pid} still runs");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn inspect_times_out_a_hanging_prepare_and_ends_its_runtime() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry, pid_file) = hanging_entry();
    let (client, _) = client().await;
    let result = tokio::time::timeout(
        Duration::from_secs(60),
        call(&client, "inspect", json!({ "entry": entry, "timeout": 2 })),
    )
    .await
    .expect("the deadline ended the inspection");
    assert!(is_error(&result), "{}", text(&result));
    assert_eq!(structured(&result)["error"]["code"], "timeout");
    assert_eq!(
        structured(&result)["error"]["message"],
        "inspect timed out after 2 s while loading the entry; pass a larger timeout, or check whether prepare() is waiting on something"
    );
    let pid = wait_for_pid(&pid_file).await;
    wait_for_exit(&pid).await;
    let next = call(&client, "list_templates", json!({})).await;
    assert!(!is_error(&next));
}

#[tokio::test]
async fn inspect_cancel_ends_a_hanging_prepare() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry, pid_file) = hanging_entry();
    let (client, _) = client().await;
    let handle = client
        .send_cancellable_request(
            ClientRequest::CallToolRequest(Request::new(params(
                "inspect",
                json!({ "entry": entry }),
            ))),
            PeerRequestOptions::no_options(),
        )
        .await
        .unwrap();
    let pid = wait_for_pid(&pid_file).await;
    handle.cancel(None).await.unwrap();
    wait_for_exit(&pid).await;
    let next = call(&client, "list_templates", json!({})).await;
    assert!(!is_error(&next));
}
