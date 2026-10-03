use std::path::{Path, PathBuf};
use std::process::Command;

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
        .arg(root().join("skills/celesta/scripts/inspect.mjs"))
        .arg(&entry)
        .args(["--frames", "0,-1", "--json", "--native"])
        .arg(env!("CARGO_BIN_EXE_celesta-exporter"))
        .arg("--runtime")
        .arg(root().join("packages/react/dist/cli.js"))
        .args(["--node", "node"])
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
}
