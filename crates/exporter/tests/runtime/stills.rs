//! React entries exported to stills, sequences, and contact sheets.

use std::path::{Path, PathBuf};
use std::process::Command;

use celesta_codec_test::{PLUGIN_PATH_ENV, TestCodecPlugin};
use celesta_exporter::{ExportOptions, ExportProgress, ReactRuntimeOptions};
use celesta_project::Project;

use crate::common::{decode, json, pixels, root, staging_in, stderr, tiny_project};

#[test]
fn react_png_preserves_sequence_boundaries_and_prepares_once_without_audio_mix() {
    let runtime_path = crate::runtime();
    let dir = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = dir.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect, Sequence, Audio } from '@celesta.studio/react';
import { appendFileSync } from 'node:fs';
export async function prepare() { appendFileSync(PREPARED_PATH, 'once\n'); }
export default function Root() {
  return <Composition width={3} height={3} fps={4} durationInFrames={4}>
    <Sequence from={0} durationInFrames={2}><Rect width={3} height={3} fill="#ff0000" /></Sequence>
    <Sequence from={2} durationInFrames={2}><Rect width={3} height={3} fill="#0000ff" /></Sequence>
    <Audio src="./missing-audio.wav" />
  </Composition>;
}
"##
        .replace(
            "PREPARED_PATH",
            &serde_json::to_string(&dir.path().join("prepared.txt")).unwrap(),
        ),
    )
    .unwrap();
    let mut progress = Vec::new();
    crate::exporter(ExportOptions::default())
        .export_react_stills(
            &entry,
            &ReactRuntimeOptions::new("node", runtime_path),
            None,
            &[0, 1, 2, 3],
            &dir.path().join("check.png"),
            |event| progress.push(event),
        )
        .unwrap();
    for frame in [0, 1] {
        assert_eq!(
            &pixels(&dir.path().join(format!("check-{frame:06}.png")))[..4],
            &[255, 0, 0, 255]
        );
    }
    for frame in [2, 3] {
        assert_eq!(
            &pixels(&dir.path().join(format!("check-{frame:06}.png")))[..4],
            &[0, 0, 255, 255]
        );
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("prepared.txt")).unwrap(),
        "once\n"
    );
    assert!(
        !progress
            .iter()
            .any(|event| matches!(event, ExportProgress::MixingAudio | ExportProgress::Muxing))
    );
}

#[test]
fn react_png_runs_a_custom_shader_from_a_wgsl_file() {
    let runtime_path = crate::runtime();
    let dir = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    std::fs::write(
        dir.path().join("tint.wgsl"),
        "fn effect(input: EffectInput) -> vec4f {\n    return premultiply(params.tint) * source_at(input.position).a;\n}\n",
    )
    .unwrap();
    let entry = dir.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect } from '@celesta.studio/react';
import { defineShader } from '@celesta.studio/shader';
import source from './tint.wgsl';
const tint = defineShader({ name: 'tint', wgsl: source, params: { tint: 'color' } });
export default function Root() {
  return <Composition width={3} height={3} fps={4} durationInFrames={1}>
    <Rect width={3} height={3} fill="#ff0000" shader={tint({ tint: '#00ff00' })} />
  </Composition>;
}
"##,
    )
    .unwrap();
    crate::exporter(ExportOptions::default())
        .export_react_stills(
            &entry,
            &ReactRuntimeOptions::new("node", runtime_path),
            None,
            &[0],
            &dir.path().join("check.png"),
            |_| {},
        )
        .unwrap();
    assert_eq!(
        &pixels(&dir.path().join("check.png"))[..4],
        &[0, 255, 0, 255]
    );
}

#[test]
fn react_png_draws_a_masked_group() {
    let runtime_path = crate::runtime();
    let dir = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = dir.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Group, Rect } from '@celesta.studio/react';
export default function Root() {
  return <Composition width={3} height={3} fps={4} durationInFrames={1}>
    <Group mask={<Rect width={1} height={3} fill="#ffffff" />}>
      <Rect width={3} height={3} fill="#ff0000" />
    </Group>
  </Composition>;
}
"##,
    )
    .unwrap();
    crate::exporter(ExportOptions::default())
        .export_react_stills(
            &entry,
            &ReactRuntimeOptions::new("node", runtime_path),
            None,
            &[0],
            &dir.path().join("check.png"),
            |_| {},
        )
        .unwrap();
    let pixels = pixels(&dir.path().join("check.png"));
    // The mask's first column shows the red; the last column does not.
    assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
    assert_ne!(&pixels[8..12], &[255, 0, 0, 255]);
}

#[test]
fn react_cli_props_win_over_the_companion_project_and_are_checked_first() {
    crate::runtime();
    let dir = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = dir.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect, defineProjectProperties, getProjectProperty } from '@celesta.studio/react';
import { useProjectProperty } from '@celesta.studio/project';
defineProjectProperties({
  fill: { type: 'color', defaultValue: '#ff0000' },
  frames: { type: 'number', defaultValue: 4 },
});
function Fill() {
  return <Rect width={3} height={3} fill={useProjectProperty<string>('fill')} />;
}
export default function Root() {
  return <Composition width={3} height={3} fps={4} durationInFrames={getProjectProperty<number>('frames')}>
    <Fill />
  </Composition>;
}
"##,
    )
    .unwrap();
    let mut project = Project::load(tiny_project(dir.path())).unwrap();
    project
        .properties
        .insert("fill".to_owned(), serde_json::json!("#00ff00"));
    let companion = dir.path().join("project.celesta.json");
    std::fs::write(&companion, serde_json::to_vec(&project).unwrap()).unwrap();
    std::fs::create_dir(dir.path().join("variants")).unwrap();
    let variant = dir.path().join("variants/blue.json");
    std::fs::write(&variant, r##"{ "fill": "#0000ff", "frames": 2 }"##).unwrap();

    let run = |args: &[&str]| {
        let output = dir.path().join("out.png");
        let mut command = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"));
        command
            .args(["--react", "--json", "--overwrite", "--frame", "0"])
            .args(args)
            .arg(&entry)
            .arg(&output);
        let result = command.output().unwrap();
        let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        (report, output)
    };
    let companion_arg = companion.to_str().unwrap();
    let variant_arg = variant.to_str().unwrap();

    let (report, output) = run(&["--project", companion_arg]);
    assert_eq!(report["status"], "ok", "{report}");
    assert_eq!(report["composition"]["frames"], 4);
    assert_eq!(&pixels(&output)[..4], &[0, 255, 0, 255]);

    let (report, output) = run(&["--project", companion_arg, "--props-file", variant_arg]);
    assert_eq!(report["status"], "ok", "{report}");
    assert_eq!(report["composition"]["frames"], 2);
    assert_eq!(&pixels(&output)[..4], &[0, 0, 255, 255]);

    let (report, output) = run(&[
        "--props-file",
        variant_arg,
        "--props",
        r##"{ "fill": "#ffffff" }"##,
    ]);
    assert_eq!(report["status"], "ok", "{report}");
    assert_eq!(&pixels(&output)[..4], &[255, 255, 255, 255]);

    let (report, _) = run(&["--props", r#"{ "fill": "blue", "frame": 1 }"#]);
    assert_eq!(report["status"], "error");
    assert_eq!(report["error"]["code"], "invalid_properties");
    assert!(report.get("composition").is_none(), "{report}");
    let keys: Vec<_> = report["error"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|issue| issue["path"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["fill", "frame"]);
}

#[test]
fn debug_guides_draw_into_png_frames_and_contact_sheets_only_when_asked() {
    crate::runtime();
    let dir = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = dir.path().join("film.tsx");
    // The safe area's shade covers the corner pixel; its outline and the
    // other guides are transparent.
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect } from '@celesta.studio/react';
import { DebugOverlay } from '@celesta.studio/debug';
export default function Root() {
  return <Composition width={3} height={3} fps={4} durationInFrames={4}>
    <Rect width={3} height={3} fill="#ff0000" />
    <DebugOverlay safeArea={{ top: 1 }} shade="#00ff00" color="#00000000" showFrame={false} />
  </Composition>;
}
"##,
    )
    .unwrap();
    let run = |args: &[&str], output: &Path| {
        Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
            .args(["--react", "--overwrite"])
            .args(args)
            .arg(&entry)
            .arg(output)
            .output()
            .unwrap()
    };

    let plain = dir.path().join("plain.png");
    let result = run(&["--frame", "0"], &plain);
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(&pixels(&plain)[..4], &[255, 0, 0, 255]);

    let guided = dir.path().join("guided.png");
    let result = run(&["--frame", "0", "--debug-guides"], &guided);
    assert!(result.status.success(), "{}", stderr(&result));
    let guided = pixels(&guided);
    assert_eq!(&guided[..4], &[0, 255, 0, 255]);
    // The bottom row is inside the safe area.
    assert_eq!(&guided[24..28], &[255, 0, 0, 255]);

    let sheet = dir.path().join("sheet.png");
    let result = run(
        &[
            "--every",
            "2",
            "--contact-sheet",
            "--tile-width",
            "3",
            "--debug-guides",
        ],
        &sheet,
    );
    assert!(result.status.success(), "{}", stderr(&result));
    let (_, _, sheet_pixels) = decode(&sheet);
    assert!(sheet_pixels.as_chunks::<4>().0.contains(&[0, 255, 0, 255]));

    let video = dir.path().join("film.mp4");
    let result = run(&["--debug-guides"], &video);
    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("--debug-guides applies to still output"),
        "{}",
        stderr(&result)
    );
    assert!(!video.exists());
}

/// A 64x36, 4 fps, 8-frame React entry in `dir`: red, except frame 6, whose
/// fill is a color name no `<Theme>` defines. Either the entry or the
/// renderer rejects it, so its failure's code is `react` or `render`.
fn entry_with_a_bad_frame(dir: &Path) -> PathBuf {
    let entry = dir.join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect, useCurrentFrame } from '@celesta.studio/react';
function Frame() {
  const frame = useCurrentFrame();
  return <Rect width={64} height={36} fill={frame === 6 ? 'ink' : '#ff0000'} />;
}
export default function Root() {
  return <Composition width={64} height={36} fps={4} durationInFrames={8}>
    <Frame />
  </Composition>;
}
"##,
    )
    .unwrap();
    entry
}

/// Runs the exporter CLI on a React entry, ignoring the codec the developer's
/// `CELESTA_PLUGIN_PATH` names. A video `output` gets the test codec; stills
/// need no codec, so they run without the `celesta-codec-test` program.
fn react_export(entry: &Path, args: &[&str], output: &Path) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"));
    command.env_remove(PLUGIN_PATH_ENV);
    // Kept until the child exits: `output` waits for it.
    let video = output
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| ["mp4", "mov", "webm"].contains(&extension));
    let plugin = video.then(TestCodecPlugin::new);
    if let Some(plugin) = &plugin {
        command.arg("--plugin-dir").arg(plugin.folder());
    }
    command
        .arg("--react")
        .args(args)
        .arg(entry)
        .arg(output)
        .output()
        .unwrap()
}

#[test]
fn a_contact_sheet_marks_failed_frames_and_names_each_one() {
    crate::runtime();
    let dir = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = entry_with_a_bad_frame(dir.path());
    let output = dir.path().join("sheet.png");
    let args = [
        "--every",
        "2",
        "--contact-sheet",
        "--columns",
        "3",
        "--tile-width",
        "64",
    ];

    let result = react_export(&entry, &args, &output);
    assert!(!result.status.success());
    let text = stderr(&result);
    assert!(
        text.contains("1 of 5 contact sheet frames failed")
            && text.contains("\n  frame 6: could not ")
            && text.contains("ink"),
        "{text}"
    );
    // Frames 0, 2, 4, 6 and 7: the failed frame keeps its place, and the
    // frame after it still renders.
    let (width, _, pixels) = decode(&output);
    let at = |x: u32, y: u32| &pixels[((y * width + x) * 4) as usize..][..4];
    // 64x36 tiles with 8 px gaps and an 11 px label band.
    let tile = |index: u32| (8 + index % 3 * 72, 8 + index / 3 * 55);
    for index in [0, 1, 2, 4] {
        let (left, top) = tile(index);
        assert_eq!(at(left + 1, top + 1), &[255, 0, 0, 255], "tile {index}");
    }
    let (left, top) = tile(3);
    assert_eq!(at(left + 1, top + 1), &[0x6e, 0x1f, 0x1f, 255]);
    let marked = (top..top + 36)
        .flat_map(|y| (left..left + 64).map(move |x| (x, y)))
        .filter(|&(x, y)| at(x, y) == [0xf0, 0xf0, 0xf0, 255])
        .count();
    assert!(marked > 0, "the failed tile is not marked");

    let result = react_export(
        &entry,
        &[&args[..], &["--json", "--overwrite"]].concat(),
        &output,
    );
    assert!(!result.status.success());
    let report = json(&result);
    assert_eq!(report["status"], "error");
    assert_eq!(report["error"]["code"], "frames_failed");
    let failed = report["error"]["failedFrames"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{report}");
    assert_eq!(failed[0]["frame"], 6);
    assert!(
        ["react", "render"].contains(&failed[0]["code"].as_str().unwrap()),
        "{report}"
    );
    assert!(
        failed[0]["message"].as_str().unwrap().contains("ink"),
        "{report}"
    );
    assert_eq!(report["outputs"][0]["type"], "contactSheet");
    assert_eq!(
        report["outputs"][0]["frames"],
        serde_json::json!([0, 2, 4, 6, 7])
    );
}

#[test]
fn separate_stills_and_videos_name_the_frame_that_failed() {
    crate::runtime();
    let dir = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = entry_with_a_bad_frame(dir.path());

    let result = react_export(
        &entry,
        &["--frames", "0,6,7", "--json"],
        &dir.path().join("check.png"),
    );
    assert!(!result.status.success());
    let report = json(&result);
    assert!(
        ["react", "render"].contains(&report["error"]["code"].as_str().unwrap()),
        "{report}"
    );
    assert_eq!(report["error"]["frame"], 6);
    let message = report["error"]["message"].as_str().unwrap();
    assert!(message.starts_with("frame 6: "), "{message}");
    // Separate files stop at the failed frame, keeping those before it.
    assert_eq!(report["outputs"][0]["frame"], 0);
    assert_eq!(report["outputs"].as_array().unwrap().len(), 1);

    let result = react_export(&entry, &[], &dir.path().join("film.mp4"));
    assert!(!result.status.success());
    let text = stderr(&result);
    if text.contains("RequestAdapter") || text.contains("adapter") {
        eprintln!("skipping React video failure check: no GPU adapter: {text}");
        return;
    }
    assert!(
        text.contains("frame 6: could not ") && text.contains("ink"),
        "{text}"
    );

    // Up to (excluding) 1 s, before frame 6, the video is written, and by the
    // test codec: only its files read back with `celesta_codec_test::read`.
    let film = dir.path().join("short.mp4");
    let result = react_export(&entry, &["--to", "1"], &film);
    assert!(result.status.success(), "{}", stderr(&result));
    let media = celesta_codec_test::read(&film).unwrap();
    assert_eq!(media.extension.as_deref(), Some("mp4"));
    assert_eq!(media.video.unwrap().frames.len(), 4);
}

#[test]
fn a_failed_sequence_keeps_written_files_and_no_staging() {
    crate::runtime();
    let dir = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = entry_with_a_bad_frame(dir.path());
    let result = react_export(
        &entry,
        &["--frames", "0,6,7"],
        &dir.path().join("check.png"),
    );
    assert!(!result.status.success());
    assert!(dir.path().join("check-000000.png").exists());
    assert!(!dir.path().join("check-000006.png").exists());
    assert!(!dir.path().join("check-000007.png").exists());
    assert_eq!(staging_in(dir.path()), Vec::<String>::new());
}
