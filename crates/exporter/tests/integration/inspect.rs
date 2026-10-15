use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use celesta_composition::{Scene, Time};
use celesta_react_bridge::{ReactAudioClipDescriptor, ReactBridge};
use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn runtime() -> Option<PathBuf> {
    let cli = root().join("packages/cli/dist/cli.js");
    if cli.exists() {
        Some(cli)
    } else {
        eprintln!("skipping inspection test: build packages/cli first");
        None
    }
}

/// An entry in a directory where `@celesta/react` resolves.
fn entry(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = dir.path().join("scene.tsx");
    std::fs::write(&entry, source).unwrap();
    (dir, entry)
}

fn inspect_mjs(entry: &Path, args: &[&str]) -> Output {
    Command::new("node")
        .env_remove("CELESTA_REACT_CLI")
        .env_remove("CELESTA_NODE")
        .arg(root().join("skills/celesta/scripts/inspect.mjs"))
        .arg(entry)
        .args(args)
        .args(["--native", env!("CARGO_BIN_EXE_celesta-exporter")])
        .output()
        .unwrap()
}

/// Runs `--inspect` directly with `requests` on stdin; returns its exit code
/// and stdout lines.
fn inspect_exporter(entry: &Path, args: &[&str], requests: &str) -> (Option<i32>, Vec<Value>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .args(["--inspect", "--react"])
        .args(args)
        .arg(entry)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // A runtime that died early closes stdin; the replies still tell why.
    let _ = child.stdin.take().unwrap().write_all(requests.as_bytes());
    let output = child.wait_with_output().unwrap();
    let lines = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (output.status.code(), lines)
}

fn request(frame: u64) -> String {
    format!("{{\"time\":{{\"value\":{frame},\"timescale\":30}}}}\n")
}

#[test]
fn native_inspection_measures_text_like_an_export() {
    let Some(cli) = runtime() else { return };
    let (dir, entry) = entry(
        r#"
import { Composition, Font, Text, Rect, Audio, measureText, useTextMetrics, useCurrentFrame, defineProjectProperties, getProjectProperty } from '@celesta/react';
defineProjectProperties({ title: { type: 'string', defaultValue: 'CELESTA' } });
const style = { fontFamily: 'Bebas Neue', fontSize: 96 };
let prepared;
export async function prepare() {
  prepared = await measureText(getProjectProperty('title'), style, { fonts: ['./font.ttf'] });
}
function Title() {
  const title = getProjectProperty('title');
  const m = useTextMetrics(title, style);
  const wrapped = useTextMetrics(`${title} ${title}`, style, { maxWidth: 150 });
  if (m.width !== prepared.width || !m.glyphs.length || wrapped.lines < 2) throw new Error('incorrect metrics');
  if (useCurrentFrame() === 1) throw new Error('broken scene');
  return <>
    <Text x={(1920 - m.width) / 2} y={400} style={style}>{title}</Text>
    <Rect x={m.glyphs[0].x} width={m.width} height={wrapped.height} />
  </>;
}
export default function Root() {
  return <Composition width={1920} height={1080} fps={30} durationInFrames={30}>
    <Font src="./font.ttf" /><Title /><Audio src="./voice.wav" />
  </Composition>;
}
"#,
    );
    std::fs::copy(
        root().join("examples/prism/assets/fonts/BebasNeue-Regular.ttf"),
        dir.path().join("font.ttf"),
    )
    .unwrap();

    let result = inspect_mjs(
        &entry,
        &[
            "--frames",
            "0,-1",
            "--json",
            "--props",
            r#"{"title":"VEGA"}"#,
        ],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["ready"]["config"]["durationInFrames"], 30);
    assert!(report["ready"]["propertySchema"]["title"].is_object());
    // The same scene and audio the export bridge evaluates.
    let properties = celesta_react_bridge::PropertyInputs::load_for_entry(
        &entry,
        None,
        Some(r#"{"title":"VEGA"}"#),
    )
    .unwrap();
    let mut bridge = ReactBridge::spawn_with_properties("node", &cli, &entry, &properties).unwrap();
    for (index, frame) in [0, 29].into_iter().enumerate() {
        let evaluation = bridge.evaluate_at(Time::new(frame, 30), None).unwrap();
        let inspected = &report["frames"][index];
        assert_eq!(inspected["frame"], frame);
        let scene: Scene = serde_json::from_value(inspected["scene"].clone()).unwrap();
        let audio: Vec<ReactAudioClipDescriptor> =
            serde_json::from_value(inspected["audio"].clone()).unwrap();
        assert_eq!(scene, evaluation.scene);
        assert_eq!(audio, evaluation.audio);
    }

    // Text output lists the layers; a real scene error is still an ERROR.
    let text = inspect_mjs(&entry, &["--frames", "0,1"]);
    assert_eq!(text.status.code(), Some(1));
    let output = String::from_utf8_lossy(&text.stdout);
    assert!(output.contains(r#"text "CELESTA""#), "{output}");
    assert!(output.contains("ERROR: broken scene"), "{output}");
    assert!(!output.contains("UNSUPPORTED"), "{output}");
}

#[test]
fn inspection_reports_request_scene_and_runtime_errors() {
    if runtime().is_none() {
        return;
    }
    let (_dir, entry) = entry(
        r#"
import { Composition, Text, useCurrentFrame } from '@celesta/react';
function Title() {
  const frame = useCurrentFrame();
  if (frame === 1) throw new Error('broken scene');
  if (frame === 2) process.exit(9);
  return <Text>Celesta</Text>;
}
export default function Root() {
  return <Composition width={400} height={200} fps={30} durationInFrames={4}><Title /></Composition>;
}
"#,
    );
    let requests = format!("invalid JSON\n{}{}", request(1), request(0));
    let (code, lines) = inspect_exporter(&entry, &[], &requests);
    assert_eq!(code, Some(1), "a failed request fails the run");
    assert_eq!(lines.len(), 4);
    assert!(lines[0]["config"].is_object());
    assert_eq!(lines[1]["status"], "requestError");
    assert_eq!(lines[2]["status"], "sceneError");
    assert_eq!(lines[2]["error"], "broken scene");
    assert!(lines[3]["scene"].is_object());

    let requests = format!("{}{}", request(2), request(0));
    let (code, lines) = inspect_exporter(&entry, &[], &requests);
    assert_eq!(code, Some(1));
    assert_eq!(lines.len(), 2, "inspection stops when the runtime is gone");
    assert_eq!(lines[1]["status"], "runtimeError");
    assert_eq!(lines[1]["fatal"], true);

    let wrapped = inspect_mjs(&entry, &["--frames", "2,3", "--json"]);
    assert_eq!(wrapped.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&wrapped.stdout).unwrap();
    assert_eq!(report["frames"].as_array().unwrap().len(), 1);
    assert_eq!(report["frames"][0]["status"], "runtimeError");
}

#[test]
fn startup_failures_replace_the_handshake() {
    if runtime().is_none() {
        return;
    }
    let (_dir, entry) = entry(
        r#"
import { Composition, defineProjectProperties } from '@celesta/react';
defineProjectProperties({ count: { type: 'number', defaultValue: 1 } });
export async function prepare() { throw new Error('prepare exploded'); }
export default function Root() {
  return <Composition width={400} height={200} fps={30} durationInFrames={1} />;
}
"#,
    );
    let (code, lines) = inspect_exporter(&entry, &[], "");
    assert_eq!(code, Some(1));
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0]["error"]
            .as_str()
            .unwrap()
            .contains("prepare exploded")
    );
    let (code, lines) = inspect_exporter(&entry, &["--props", r#"{"count":"x"}"#], "");
    assert_eq!(code, Some(1));
    assert_eq!(lines[0]["invalidProperties"][0]["key"], "count");
    let (code, lines) = inspect_exporter(&entry, &["--props", "{"], "");
    assert_eq!(code, Some(1));
    assert!(lines[0]["error"].as_str().unwrap().contains("--props"));

    let text = inspect_mjs(&entry, &[]);
    assert_eq!(text.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&text.stderr);
    assert!(
        stderr.contains("ERROR loading entry: ") && stderr.contains("prepare exploded"),
        "{stderr}"
    );
    let json = inspect_mjs(&entry, &["--json"]);
    assert_eq!(json.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&json.stdout).unwrap();
    assert!(
        report["error"]
            .as_str()
            .unwrap()
            .contains("prepare exploded")
    );
}

#[test]
fn inspect_flags_do_not_mix_with_export_flags() {
    let exporter = env!("CARGO_BIN_EXE_celesta-exporter");
    let run = |args: &[&str]| Command::new(exporter).args(args).output().unwrap();
    let missing_output = run(&["--react", "scene.tsx"]);
    assert!(!missing_output.status.success());
    assert!(String::from_utf8_lossy(&missing_output.stderr).contains("<OUTPUT>"));
    let without_react = run(&["--inspect", "scene.tsx"]);
    assert!(!without_react.status.success());
    for flags in [["--runtime", "cli.js"], ["--node", "node"]] {
        let result = run(&[&["--react", "scene.tsx", "out.mp4"][..], &flags].concat());
        assert!(
            !result.status.success(),
            "accepted {flags:?} without --inspect"
        );
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("require --inspect"),
            "{flags:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for flags in [
        &["out.mp4"][..],
        &["--project", "project.celesta.json"],
        &["--overwrite"],
        &["--from", "0"],
        &["--frame", "0"],
        &["--every", "1"],
        &["--contact-sheet"],
        &["--crf", "18"],
        &["--driver", "auto"],
    ] {
        let result = run(&[&["--inspect", "--react", "scene.tsx"][..], flags].concat());
        assert!(
            !result.status.success(),
            "accepted {flags:?} with --inspect"
        );
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("cannot be used"),
            "{flags:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    // --json reports usage errors on stdout.
    let json = run(&["--inspect", "--react", "scene.tsx", "--json"]);
    assert!(!json.status.success());
    assert!(String::from_utf8_lossy(&json.stdout).contains("cannot be used"));
}

#[test]
fn inspection_finds_a_source_build_of_the_exporter() {
    if runtime().is_none() {
        return;
    }
    // A checkout whose build is the exporter under test.
    let (dir, entry) = entry(
        r#"
import { Composition, Text, useTextMetrics } from '@celesta/react';
function Title() {
  const m = useTextMetrics('Celesta', { fontSize: 96 });
  return <Text x={(1920 - m.width) / 2} y={400} style={{ fontSize: 96 }}>Celesta</Text>;
}
export default function Root() {
  return <Composition width={1920} height={1080} fps={30} durationInFrames={30}><Title /></Composition>;
}
"#,
    );
    let build = dir.path().join("target/debug");
    std::fs::create_dir_all(&build).unwrap();
    let exporter = build.join(
        Path::new(env!("CARGO_BIN_EXE_celesta-exporter"))
            .file_name()
            .unwrap(),
    );
    if std::fs::hard_link(env!("CARGO_BIN_EXE_celesta-exporter"), &exporter).is_err() {
        std::fs::copy(env!("CARGO_BIN_EXE_celesta-exporter"), &exporter).unwrap();
    }
    let run = |args: &[&str]| {
        Command::new("node")
            .env_remove("CELESTA_REACT_CLI")
            .env_remove("CELESTA_NODE")
            .arg(root().join("skills/celesta/scripts/inspect.mjs"))
            .arg(&entry)
            .args(["--frames", "0"])
            .args(args)
            .output()
            .unwrap()
    };
    let found = run(&[]);
    let output = String::from_utf8_lossy(&found.stdout);
    assert_eq!(found.status.code(), Some(0), "{output}");
    // `root()` is not normalized; the script prints the resolved path.
    let checkout = dir.path().file_name().unwrap().to_string_lossy();
    let native = output.lines().next().unwrap_or_default();
    assert!(
        native.contains(&*checkout) && native.ends_with(" (found)"),
        "{output}"
    );
    assert!(output.contains(r#"text "Celesta""#), "{output}");

    let node_only = run(&["--no-native"]);
    assert_eq!(node_only.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&node_only.stderr).contains("UNSUPPORTED inspection"));
}

#[test]
fn native_inspection_evaluates_past_the_end_for_loop_checks() {
    let Some(_cli) = runtime() else { return };
    let scene = |x: &str| {
        format!(
            r##"
import {{ Composition, Rect, Sequence, useCurrentFrame }} from '@celesta/react';
function Mover() {{
  const frame = useCurrentFrame();
  return <Rect x={{{x}}} width={{4}} height={{4}} fill="#ffffff" />;
}}
export default function Film() {{
  return <Composition width={{64}} height={{64}} fps={{30}} durationInFrames={{4}}>
    <Sequence from={{0}}><Mover /></Sequence>
  </Composition>;
}}
"##
        )
    };
    // Frame 4 is one past the last: the sequence that runs to the end stays open.
    let (_dir, looping) = entry(&scene("(frame % 4) * 10"));
    let requests = "{\"time\":{\"value\":4,\"timescale\":30},\"openEnd\":true}\n";
    let (code, lines) = inspect_exporter(&looping, &[], requests);
    assert_eq!(code, Some(0));
    assert_eq!(lines[1]["openEnd"], true, "{}", lines[1]);
    assert_eq!(lines[1]["scene"]["layers"].as_array().unwrap().len(), 1);

    let seamless = inspect_mjs(&looping, &["--loop"]);
    let stdout = String::from_utf8_lossy(&seamless.stdout);
    assert_eq!(seamless.status.code(), Some(0), "{stdout}");
    assert!(
        stdout.contains("frame 4 vs frame 0: same Scene"),
        "{stdout}"
    );

    let (_drifting_dir, drifting) = entry(&scene("frame * 10"));
    let seam = inspect_mjs(&drifting, &["--loop"]);
    let stdout = String::from_utf8_lossy(&seam.stdout);
    assert_eq!(seam.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("transform.position.x: 40 at frame 4, 0 at frame 0"),
        "{stdout}"
    );
}

#[test]
fn selects_compositions_for_native_inspection_and_json_exports() {
    let Some(cli) = runtime() else { return };
    let (dir, entry) = entry(
        r##"
import { Composition, Rect, useCurrentFrame, useVideoConfig } from '@celesta/react';
function Scene() {
  const { width, height, fps } = useVideoConfig();
  if (width === 2 && (height !== 4 || fps !== 24)) throw new Error('wrong selected config');
  return <Rect id={`frame-${useCurrentFrame()}`} width={width} height={height} fill="#ff0000" />;
}
export default () => <>
  <Composition id="wide" width={4} height={2} fps={30} durationInFrames={3}><Scene /></Composition>
  <Composition id="tall" width={2} height={4} fps={24} durationInFrames={2}><Scene /></Composition>
</>;
"##,
    );
    let result = inspect_mjs(
        &entry,
        &["--composition", "tall", "--frames", "0,-1", "--json"],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(result["ready"]["compositionId"], "tall");
    assert_eq!(result["ready"]["compositions"][0]["id"], "wide");
    assert_eq!(result["ready"]["compositions"][1]["id"], "tall");
    assert_eq!(result["frames"][1]["scene"]["layers"][0]["id"], "frame-1");
    assert_eq!(result["frames"][1]["scene"]["width"], 2);

    for (id, width, height) in [("wide", 4, 2), ("tall", 2, 4)] {
        let output = dir.path().join(format!("{id}.png"));
        let result = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
            .args(["--react", "--json", "--composition", id, "--frame", "0"])
            .arg(&entry)
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        let report: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["compositionId"], id);
        assert_eq!(report["compositions"].as_array().unwrap().len(), 2);
        assert_eq!(
            (
                report["composition"]["width"].as_u64(),
                report["composition"]["height"].as_u64()
            ),
            (Some(width), Some(height))
        );
        let image = png::Decoder::new(std::io::BufReader::new(
            std::fs::File::open(output).unwrap(),
        ))
        .read_info()
        .unwrap();
        assert_eq!(
            (
                u64::from(image.info().width),
                u64::from(image.info().height)
            ),
            (width, height)
        );
    }
    let (status, ready) = inspect_exporter(&entry, &["--runtime", cli.to_str().unwrap()], "");
    assert_eq!(status, Some(0));
    assert_eq!(ready[0]["compositionId"], "wide");
    let (status, ready) = inspect_exporter(
        &entry,
        &[
            "--composition",
            "missing",
            "--runtime",
            cli.to_str().unwrap(),
        ],
        "",
    );
    assert_eq!(status, Some(1));
    assert!(
        ready[0]["error"]
            .as_str()
            .unwrap()
            .contains("unknown composition id: missing")
    );
}
