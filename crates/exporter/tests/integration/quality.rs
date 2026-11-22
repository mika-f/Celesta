//! The quality checks through the command line: `--check-quality` renders
//! a JSON project's frames and reports what it measures, and `--info`
//! reports the checks the timeline alone shows.

use std::path::{Path, PathBuf};
use std::process::Command;

use celesta_codec_test::{PLUGIN_PATH_ENV, TestCodecPlugin, isolated_home_env};
use serde_json::{Value, json};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 90;

/// A white `WIDTH`×`HEIGHT` PNG in `dir`.
fn white_png(dir: &Path) -> PathBuf {
    let path = dir.join("white.png");
    let file = std::fs::File::create(&path).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&vec![255; (WIDTH * HEIGHT * 3) as usize])
        .unwrap();
    path
}

fn time(frames: i64) -> Value {
    json!({ "value": frames, "timescale": 30 })
}

/// A two-second, 30 fps project of `tracks` in `dir`.
fn project(dir: &Path, tracks: Value) -> PathBuf {
    white_png(dir);
    let path = dir.join("project.celesta.json");
    let project = json!({
        "version": 0,
        "settings": {
            "width": WIDTH, "height": HEIGHT,
            "frameRate": { "numerator": 30, "denominator": 1 },
            "duration": { "value": 2, "timescale": 1 },
            "sampleRate": 48000
        },
        "assets": { "white": { "type": "image", "source": { "type": "file", "path": "./white.png" } } },
        "characters": {},
        "tracks": tracks,
        "properties": {}
    });
    std::fs::write(&path, project.to_string()).unwrap();
    path
}

fn text_track(color: &str, frames: (i64, i64)) -> Value {
    json!({
        "id": "titles", "name": "Titles", "kind": "overlay",
        "items": [{
            "id": "title", "range": { "start": time(frames.0), "duration": time(frames.1) },
            "content": { "type": "text", "text": "Celesta", "style": { "fontSize": 40, "fill": { "type": "solid", "color": color } } },
            "transform": { "position": { "x": 80, "y": 45 } }
        }]
    })
}

/// Runs the exporter with `--json` and `args` (the test codec its only
/// media plugin); its exit code and report, or `None` without a GPU.
fn exporter(args: &[&str]) -> Option<(Option<i32>, Value)> {
    let plugin = TestCodecPlugin::new();
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .env_remove(PLUGIN_PATH_ENV)
        .envs(isolated_home_env(home.path()))
        .arg("--plugin-dir")
        .arg(plugin.folder())
        .arg("--json")
        .args(args)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() && stdout.contains("adapter") {
        assert!(
            std::env::var("CELESTA_REQUIRE_GPU").as_deref() != Ok("1"),
            "GPU tests require an adapter: {stdout}"
        );
        eprintln!("skipping quality check test: no GPU adapter is available");
        return None;
    }
    let report = serde_json::from_str(stdout.trim()).unwrap_or_else(|_| panic!("{stdout}"));
    Some((output.status.code(), report))
}

fn codes(report: &Value) -> Vec<&str> {
    report["quality"]
        .as_array()
        .unwrap_or_else(|| panic!("{report}"))
        .iter()
        .map(|issue| issue["code"].as_str().unwrap())
        .collect()
}

fn issue<'a>(report: &'a Value, code: &str) -> &'a Value {
    report["quality"]
        .as_array()
        .unwrap()
        .iter()
        .find(|issue| issue["code"] == code)
        .unwrap_or_else(|| panic!("no {code}: {report}"))
}

#[test]
fn check_quality_finds_flashing_short_scenes_and_a_blank_first_frame() {
    let dir = tempfile::tempdir().unwrap();
    // A white frame for 3 frames every 6 from frame 3: 5 flashes a second.
    let items: Vec<Value> = (0..9)
        .map(|index| {
            json!({
                "id": format!("flash-{index}"),
                "range": { "start": time(3 + 6 * index), "duration": time(3) },
                "content": { "type": "image", "asset": "white", "width": WIDTH, "height": HEIGHT },
                "transform": { "position": { "x": 80, "y": 45 } }
            })
        })
        .collect();
    let source = project(
        dir.path(),
        json!([{ "id": "strobe", "name": "Strobe", "kind": "video", "items": items }]),
    );
    let Some((code, report)) = exporter(&["--check-quality", source.to_str().unwrap()]) else {
        return;
    };
    assert_eq!(report["status"], "ok", "{report}");
    assert_eq!(code, Some(1), "{report}");
    let codes = codes(&report);
    for expected in ["flashing", "short_scene", "blank_first_frame"] {
        assert!(codes.contains(&expected), "{expected}: {report}");
    }
    let flashing = issue(&report, "flashing");
    assert_eq!(flashing["measured"], "pixels");
    assert!(flashing["value"].as_f64().unwrap() > 3.0, "{flashing}");
    assert_eq!(
        flashing["region"],
        json!({ "x": 0, "y": 0, "width": WIDTH, "height": HEIGHT })
    );
    assert!(
        flashing["frames"][0]["from"].as_u64().unwrap() <= 6,
        "{flashing}"
    );
    assert_eq!(issue(&report, "blank_first_frame")["measured"], "pixels");
    // The warnings carry the codes too, and nothing is written.
    let warnings = report["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|warning| warning.as_str().unwrap().starts_with("flashing: ")),
        "{report}"
    );
    assert_eq!(report["outputs"], json!([]));
}

#[test]
fn check_quality_measures_text_contrast_in_pixels() {
    let dir = tempfile::tempdir().unwrap();
    // Dark grey on the export's #14161C background.
    let source = project(dir.path(), json!([text_track("#2A2E36", (0, 60))]));
    let Some((code, report)) = exporter(&["--check-quality", source.to_str().unwrap()]) else {
        return;
    };
    assert_eq!(code, Some(1), "{report}");
    let contrast = issue(&report, "low_contrast_text");
    assert_eq!(contrast["measured"], "pixels");
    assert_eq!(contrast["layer"], "title");
    assert!(contrast["value"].as_f64().unwrap() < 1.5, "{contrast}");
    assert_eq!(contrast["limit"], 3.0);
}

#[test]
fn check_quality_passes_a_calm_titled_video_and_takes_a_span() {
    let dir = tempfile::tempdir().unwrap();
    let source = project(dir.path(), json!([text_track("#FFFFFF", (0, 60))]));
    let Some((code, report)) = exporter(&["--check-quality", source.to_str().unwrap()]) else {
        return;
    };
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(report["quality"], json!([]), "{report}");
    // Rendering each frame shows as progress.
    let Some((code, report)) =
        exporter(&["--check-quality", "--from", "1", source.to_str().unwrap()])
    else {
        return;
    };
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(report["composition"]["frames"], 60);
}

#[test]
fn info_reports_short_scenes_from_the_timeline() {
    let dir = tempfile::tempdir().unwrap();
    let source = project(
        dir.path(),
        json!([{ "id": "cuts", "name": "Cuts", "kind": "video", "items": [{
            "id": "flash", "name": "Flash",
            "range": { "start": time(0), "duration": time(12) },
            "content": { "type": "image", "asset": "white", "width": WIDTH, "height": HEIGHT },
        }] }]),
    );
    // --info renders nothing, so it needs no GPU.
    let (code, report) = exporter(&["--info", source.to_str().unwrap()]).unwrap();
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(codes(&report), ["short_scene"]);
    let short = issue(&report, "short_scene");
    assert_eq!(short["measured"], "layers");
    assert_eq!(short["value"], 0.4);
    assert_eq!(
        short["frames"][0],
        json!({ "from": 0, "to": 12, "start": 0.0, "end": 0.4 })
    );
    assert!(
        short["message"]
            .as_str()
            .unwrap()
            .starts_with("item \"Flash\" on video track \"Cuts\" lasts 0.40 s"),
        "{short}"
    );
}

#[test]
fn a_span_reports_only_the_scenes_it_touches() {
    let dir = tempfile::tempdir().unwrap();
    let source = project(
        dir.path(),
        json!([
            { "id": "cuts", "name": "Cuts", "kind": "video", "items": [{
                "id": "intro", "name": "Intro",
                "range": { "start": time(0), "duration": time(12) },
                "content": { "type": "image", "asset": "white", "width": WIDTH, "height": HEIGHT },
            }] },
            text_track("#FFFFFF", (0, 60)),
        ]),
    );
    let Some((_, whole)) = exporter(&["--check-quality", source.to_str().unwrap()]) else {
        return;
    };
    assert!(codes(&whole).contains(&"short_scene"), "{whole}");
    // From 1 s the intro is never rendered.
    let (code, report) =
        exporter(&["--check-quality", "--from", "1", source.to_str().unwrap()]).unwrap();
    assert_eq!(report["quality"], json!([]), "{report}");
    assert_eq!(code, Some(0), "{report}");
}

#[test]
fn check_quality_takes_no_output() {
    let dir = tempfile::tempdir().unwrap();
    let source = project(dir.path(), json!([]));
    let out = dir.path().join("out.mp4");
    let (code, report) = exporter(&[
        "--check-quality",
        source.to_str().unwrap(),
        out.to_str().unwrap(),
    ])
    .unwrap();
    assert_eq!(code, Some(2), "{report}");
    assert_eq!(report["error"]["code"], "usage");
}

#[test]
fn a_react_check_warns_about_rough_scenes_and_final_refuses_them() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if !root.join("packages/cli/dist/cli.js").exists() {
        eprintln!("skipping React quality check test: build packages/cli first");
        return;
    }
    // In the CLI package, where `@celesta.studio/react` resolves.
    let dir = tempfile::tempdir_in(root.join("packages/cli")).unwrap();
    let entry = dir.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect, Text } from '@celesta.studio/react';
const chapters = [
  { id: 'intro', from: 0, durationInFrames: 30 },
  { id: 'card', name: 'The card', from: 30, durationInFrames: 30, status: 'rough' },
];
export default function Root() {
  return <Composition width={160} height={90} fps={30} durationInFrames={60} chapters={chapters}>
    <Rect width={160} height={90} fill="#101018" />
    <Text x={80} y={45} style={{ fontSize: 24 }}>Title</Text>
  </Composition>;
}
"##,
    )
    .unwrap();
    let entry = entry.to_str().unwrap();
    let Some((code, report)) = exporter(&["--react", "--check-quality", entry]) else {
        return;
    };
    assert_eq!(report["status"], "ok", "{report}");
    // A rough scene is something found: the check fails.
    assert_eq!(code, Some(1), "{report}");
    let coded: Vec<&str> = report["codedWarnings"]
        .as_array()
        .unwrap_or_else(|| panic!("{report}"))
        .iter()
        .map(|warning| warning["code"].as_str().unwrap())
        .collect();
    assert!(coded.contains(&"rough_scenes"), "{report}");
    // --final refuses the span, as it would refuse to export it.
    let (code, report) = exporter(&["--react", "--check-quality", "--final", entry]).unwrap();
    assert_eq!(code, Some(1), "{report}");
    assert_eq!(report["error"]["code"], "rough_scenes", "{report}");
}
