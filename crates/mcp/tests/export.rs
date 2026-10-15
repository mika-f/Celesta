mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use common::{call, client, entry, is_error, params, runtime_built, structured, text};
use rmcp::model::{ClientRequest, Request, ServerResult};
use rmcp::service::PeerRequestOptions;
use serde_json::{Value, json};

fn scene(frames: u32, width: u32, height: u32) -> String {
    format!(
        r##"
import {{ Composition, Rect, useCurrentFrame }} from '@celesta/react';
function Box() {{
  return <Rect x={{useCurrentFrame() % 100}} width={{32}} height={{32}} fill="#ff3366" />;
}}
export default function Root() {{
  return <Composition width={{{width}}} height={{{height}}} fps={{30}} durationInFrames={{{frames}}}>
    <Rect width={{{width}}} height={{{height}}} fill="#101820" /><Box />
  </Composition>;
}}
"##
    )
}

#[tokio::test]
async fn export_video_writes_an_mp4_with_progress() {
    if !runtime_built() {
        return;
    }
    let (dir, entry) = entry(&scene(60, 320, 180));
    let output = dir.path().join("out.mp4");
    let (client, mut progress) = client().await;
    let handle = client
        .send_cancellable_request(
            ClientRequest::CallToolRequest(Request::new(params(
                "export_video",
                json!({ "source": entry, "output": output }),
            ))),
            PeerRequestOptions::no_options(),
        )
        .await
        .unwrap();
    let ServerResult::CallToolResult(result) = handle.await_response().await.unwrap() else {
        panic!("not a tool result")
    };
    assert!(!is_error(&result), "{}", text(&result));
    assert!(output.exists());
    assert_eq!(structured(&result)["outputs"][0]["type"], "video");

    let mut notifications = Vec::new();
    while let Ok(notification) = progress.try_recv() {
        notifications.push(notification);
    }
    assert!(!notifications.is_empty());
    assert!(notifications.iter().any(|n| n.total == Some(60.0)));
    assert!(
        notifications
            .windows(2)
            .all(|pair| pair[0].progress < pair[1].progress),
        "progress must increase: {notifications:?}"
    );
    // The container follows the output extension, so no message names one.
    assert!(
        notifications
            .iter()
            .filter_map(|n| n.message.as_deref())
            .all(|message| !message.contains("MP4")),
        "{notifications:?}"
    );
}

#[tokio::test]
async fn export_video_cancel_stops_and_frees_the_gpu() {
    if !runtime_built() {
        return;
    }
    let (dir, long) = entry(&scene(600, 640, 360));
    let output = dir.path().join("long.mp4");
    let (client, mut progress) = client().await;
    let handle = client
        .send_cancellable_request(
            ClientRequest::CallToolRequest(Request::new(params(
                "export_video",
                json!({ "source": long, "output": output }),
            ))),
            PeerRequestOptions::no_options(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(60), progress.recv())
        .await
        .expect("a progress notification")
        .unwrap();
    handle.cancel(None).await.unwrap();

    let (_dir2, short) = entry(&scene(10, 160, 90));
    let next = tokio::time::timeout(
        Duration::from_secs(60),
        call(
            &client,
            "render_frames",
            json!({ "source": short, "frames": [0] }),
        ),
    )
    .await
    .expect("the cancelled export released the GPU");
    assert!(!is_error(&next), "{}", text(&next));
    assert!(!output.exists());
}

#[tokio::test]
async fn export_video_without_output_writes_each_languages_subtitles() {
    if !runtime_built() {
        return;
    }
    let (dir, entry) = entry(
        r#"
import { Composition, Subtitles } from '@celesta/react';
export default function Root() {
  return <Composition width={64} height={64} fps={10} durationInFrames={20}>
    <Subtitles lang="en" cues={[{ from: 0, durationInFrames: 10, text: 'Hello' }]} />
    <Subtitles lang="ja" cues={[{ from: 0, durationInFrames: 10, text: 'こんにちは' }]} />
  </Composition>;
}
"#,
    );
    let (client, _) = client().await;
    let result = call(
        &client,
        "export_video",
        json!({
            "source": entry,
            "subtitles": [dir.path().join("a.{lang}.srt"), dir.path().join("a.{lang}.vtt")],
        }),
    )
    .await;
    assert!(!is_error(&result), "{}", text(&result));
    let outputs = structured(&result)["outputs"].as_array().unwrap().clone();
    let summary: Vec<_> = outputs
        .iter()
        .map(|output| {
            (
                output["type"].as_str().unwrap().to_owned(),
                output["lang"].as_str().unwrap().to_owned(),
                output["format"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("subtitles", "en", "srt"),
            ("subtitles", "ja", "srt"),
            ("subtitles", "en", "vtt"),
            ("subtitles", "ja", "vtt"),
        ]
        .map(|(kind, lang, format)| (kind.to_owned(), lang.to_owned(), format.to_owned()))
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.ja.srt")).unwrap(),
        "1\n00:00:00,000 --> 00:00:01,000\nこんにちは\n"
    );
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

#[test]
fn stdout_carries_only_json_rpc() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(&scene(10, 160, 90));
    let mut child = Command::new(env!("CARGO_BIN_EXE_celesta-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let requests = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": { "name": "test", "version": "0" } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "render_frames", "arguments": { "source": entry, "frames": [0, -1] } } }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
            "name": "inspect", "arguments": { "entry": entry } } }),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {
            "name": "read_docs", "arguments": {} } }),
    ];
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
    }
    let stdout = child.stdout.take().unwrap();
    let (lines, received) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if lines.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    let mut answered = Vec::new();
    while answered.len() < 4 {
        let line = received
            .recv_timeout(Duration::from_secs(120))
            .expect("a reply from the server");
        let message: Value = serde_json::from_str(&line)
            .unwrap_or_else(|error| panic!("stdout line is not JSON ({error}): {line}"));
        assert_eq!(message["jsonrpc"], "2.0", "{line}");
        if let Some(id) = message.get("id") {
            answered.push(id.clone());
        }
    }
    drop(stdin);
    let _ = child.wait();
}

#[cfg(unix)]
#[test]
fn sigterm_removes_the_temporary_outputs() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(&scene(10, 160, 90));
    let mut child = Command::new(env!("CARGO_BIN_EXE_celesta-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for request in [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": { "name": "test", "version": "0" } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "render_frames", "arguments": { "source": entry, "frames": [0] } } }),
    ] {
        writeln!(stdin, "{request}").unwrap();
    }
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let reply = loop {
        let message: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
        if message["id"] == 2 {
            break message;
        }
    };
    let png = reply["result"]["structuredContent"]["outputs"][0]["path"]
        .as_str()
        .unwrap()
        .to_owned();
    // <temporary root>/<call folder>/frame.png
    let root = std::path::Path::new(&png)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    assert!(root.exists());

    let killed = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    // The client keeps stdin open, as MCP clients do while they stop a server.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            panic!("the server did not exit after SIGTERM");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!root.exists(), "{} was left behind", root.display());
    drop(stdin);
}
