use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use celesta_composition::{Scene, Time};
use celesta_react_bridge::{ReactAudioClipDescriptor, ReactBridge};
use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn native_inspection_matches_export_bridge_with_prepare_fonts_wrapping_and_glyphs() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        root().join("examples/prism/assets/fonts/BebasNeue-Regular.ttf"),
        dir.path().join("font.ttf"),
    )
    .unwrap();
    let entry = dir.path().join("scene.tsx");
    std::fs::write(
        &entry,
        r#"
        import { Composition, Font, Text, Rect, Audio, measureText, useTextMetrics, useCurrentFrame } from '@celesta/react';
        const style = { fontFamily: 'Bebas Neue', fontSize: 96 };
        let prepared;
        export async function prepare() {
            prepared = await measureText('CELESTA', style, { fonts: ['./font.ttf'] });
        }
        function Title() {
            const m = useTextMetrics('CELESTA', style);
            const wrapped = useTextMetrics('CELESTA CELESTA', style, { maxWidth: 150 });
            if (m.width !== prepared.width || !m.glyphs.length || wrapped.lines < 2) throw new Error('incorrect metrics');
            if (useCurrentFrame() === 1) throw new Error('broken scene');
            return <>
                <Text x={(1920 - m.width) / 2} y={400} style={style}>CELESTA</Text>
                <Rect x={m.glyphs[0].x} width={m.width} height={wrapped.height} />
            </>;
        }
        export default function Root() {
            return <Composition width={1920} height={1080} fps={30} durationInFrames={30}>
                <Font src="./font.ttf" /><Title /><Audio src="./voice.wav" />
            </Composition>;
        }
        "#,
    )
    .unwrap();
    // JSON inspection reports audio paths without decoding or rendering them.
    let result = Command::new("node")
        .current_dir(dir.path())
        .env_remove("CELESTA_REACT_CLI")
        .env_remove("CELESTA_NODE")
        .arg(root().join("skills/celesta/scripts/inspect.mjs"))
        .arg(&entry)
        .args(["--frames", "0,-1", "--json", "--native"])
        .arg(env!("CARGO_BIN_EXE_celesta-exporter"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["ready"]["config"]["durationInFrames"], 30);
    let mut bridge =
        ReactBridge::spawn("node", root().join("packages/react/dist/cli.js"), &entry).unwrap();
    for (index, frame) in [0, 29].into_iter().enumerate() {
        let evaluation = bridge.evaluate_at(Time::new(frame, 30), None).unwrap();
        let inspected_scene: Scene =
            serde_json::from_value(report["frames"][index]["scene"].clone()).unwrap();
        let inspected_audio: Vec<ReactAudioClipDescriptor> =
            serde_json::from_value(report["frames"][index]["audio"].clone()).unwrap();
        assert_eq!(report["frames"][index]["frame"], frame);
        assert_eq!(inspected_scene, evaluation.scene);
        assert_eq!(inspected_audio, evaluation.audio);
    }

    let error = Command::new("node")
        .arg(root().join("skills/celesta/scripts/inspect.mjs"))
        .arg(&entry)
        .args(["--frames", "1", "--native"])
        .arg(env!("CARGO_BIN_EXE_celesta-exporter"))
        .output()
        .unwrap();
    assert_eq!(error.status.code(), Some(1));
    let output = String::from_utf8_lossy(&error.stdout);
    assert!(
        output.contains("ERROR:") && output.contains("broken scene"),
        "{output}"
    );
    assert!(!output.contains("UNSUPPORTED"), "{output}");
}

#[test]
fn inspection_preserves_export_output_requirement_and_rejects_companion_projects() {
    let exporter = env!("CARGO_BIN_EXE_celesta-exporter");
    let missing_output = Command::new(exporter)
        .args(["--react", "scene.tsx"])
        .output()
        .unwrap();
    assert!(!missing_output.status.success());
    assert!(String::from_utf8_lossy(&missing_output.stderr).contains("<OUTPUT>"));
    let unsupported_project = Command::new(exporter)
        .args([
            "--inspect",
            "--react",
            "scene.tsx",
            "--project",
            "project.celesta.json",
        ])
        .output()
        .unwrap();
    assert!(!unsupported_project.status.success());
    assert!(String::from_utf8_lossy(&unsupported_project.stderr).contains("cannot be used"));
    for flags in [
        vec!["--runtime", "/nope/cli.js"],
        vec!["--node", "/nope/node"],
    ] {
        let result = Command::new(exporter)
            .args(["--react", "scene.tsx", "out.mp4"])
            .args(&flags)
            .output()
            .unwrap();
        assert!(!result.status.success(), "accepted {flags:?}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("require --inspect"),
            "{flags:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for flags in [
        vec!["--overwrite"],
        vec!["--from", "0"],
        vec!["--to", "1"],
        vec!["--frame", "0"],
        vec!["--frames", "0"],
        vec!["--every", "1"],
        vec!["--contact-sheet"],
        vec!["--columns", "5"],
        vec!["--tile-width", "320"],
        vec!["--output-format", "png"],
        vec!["--preset", "medium"],
        vec!["--crf", "18"],
        vec!["--color-conversion", "auto"],
        vec!["--render-quality", "final"],
        vec!["--no-ui"],
    ] {
        let result = Command::new(exporter)
            .args(["--inspect", "--react", "scene.tsx"])
            .args(&flags)
            .output()
            .unwrap();
        assert!(!result.status.success(), "accepted {flags:?}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("cannot be used"),
            "{flags:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn native_inspection_returns_failure_after_recoverable_errors_and_stops_on_runtime_exit() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("scene.tsx");
    std::fs::write(&entry, r#"
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
    "#).unwrap();
    let invoke = |requests: &str| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
            .args(["--inspect", "--react"])
            .arg(&entry)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(requests.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let recovered = invoke(
        "invalid JSON\n{\"time\":{\"value\":1,\"timescale\":30}}\n{\"time\":{\"value\":0,\"timescale\":30}}\n",
    );
    assert_eq!(recovered.status.code(), Some(1));
    let messages: Vec<Value> = String::from_utf8(recovered.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[1]["status"], "requestError");
    assert_eq!(messages[2]["status"], "sceneError");
    assert!(messages[3]["scene"].is_object());

    let died = invoke(
        "{\"time\":{\"value\":2,\"timescale\":30}}\n{\"time\":{\"value\":0,\"timescale\":30}}\n",
    );
    assert_eq!(died.status.code(), Some(1));
    let messages: Vec<Value> = String::from_utf8(died.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1]["status"], "runtimeError");
    assert_eq!(messages[1]["fatal"], true);

    let wrapped = Command::new("node")
        .env_remove("CELESTA_REACT_CLI")
        .env_remove("CELESTA_NODE")
        .arg(root().join("skills/celesta/scripts/inspect.mjs"))
        .arg(&entry)
        .args(["--frames", "2,3", "--json", "--native"])
        .arg(env!("CARGO_BIN_EXE_celesta-exporter"))
        .output()
        .unwrap();
    assert_eq!(wrapped.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&wrapped.stdout).unwrap();
    assert_eq!(report["frames"].as_array().unwrap().len(), 1);
    assert_eq!(report["frames"][0]["status"], "runtimeError");
}

#[test]
fn startup_failure_reports_error_and_exits_with_failure() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.tsx");
    std::fs::write(
        &entry,
        r#"
        import { Composition } from '@celesta/react';
        export async function prepare() { throw new Error('prepare exploded'); }
        export default function Root() {
            return <Composition width={400} height={200} fps={30} durationInFrames={1} />;
        }
        "#,
    )
    .unwrap();
    let direct = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .args(["--inspect", "--react"])
        .arg(&entry)
        .output()
        .unwrap();
    assert_eq!(direct.status.code(), Some(1));
    let message: Value = serde_json::from_slice(&direct.stdout).unwrap();
    assert!(
        message["error"]
            .as_str()
            .unwrap()
            .contains("prepare exploded")
    );
    for json in [false, true] {
        let mut command = Command::new("node");
        command
            .env_remove("CELESTA_REACT_CLI")
            .env_remove("CELESTA_NODE")
            .arg(root().join("skills/celesta/scripts/inspect.mjs"))
            .arg(&entry)
            .arg("--native")
            .arg(env!("CARGO_BIN_EXE_celesta-exporter"));
        if json {
            command.arg("--json");
        }
        let wrapped = command.output().unwrap();
        assert_eq!(wrapped.status.code(), Some(1));
        let output = String::from_utf8_lossy(&wrapped.stdout).into_owned()
            + &String::from_utf8_lossy(&wrapped.stderr);
        assert!(output.contains("prepare exploded"), "{output}");
        assert!(!output.contains("UNSUPPORTED"), "{output}");
        if !json {
            assert!(output.contains("ERROR loading entry"), "{output}");
        }
    }
}
