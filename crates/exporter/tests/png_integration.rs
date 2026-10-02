use std::path::{Path, PathBuf};
use std::process::Command;

use celesta_composition::{Rational, Time, TimeRange};
use celesta_exporter::{ExportOptions, ExportProgress, Exporter, ReactRuntimeOptions};
use celesta_project::{
    Asset, AssetSource, Project, TimelineContent, TimelineItem, Track, TrackKind,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pixels(path: &Path) -> Vec<u8> {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
        .read_info()
        .unwrap();
    assert_eq!((reader.info().width, reader.info().height), (3, 3));
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut pixels).unwrap();
    pixels
}

#[test]
fn json_cli_exports_first_last_and_rejects_overwrite_and_invalid_frames() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::load(root().join("examples/minimal.celesta.json")).unwrap();
    project.settings.width = 3;
    project.settings.height = 3;
    project.settings.duration = Some(celesta_composition::Time::new(1, 1));
    project.settings.frame_rate = celesta_composition::Rational::new(4, 1);
    let source = dir.path().join("project.celesta.json");
    std::fs::write(&source, serde_json::to_vec(&project).unwrap()).unwrap();
    let output = dir.path().join("check.png");
    let invoke = |frames: &str, extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
            .arg(&source)
            .args(["--frames", frames])
            .args(extra)
            .arg(&output)
            .output()
            .unwrap()
    };
    let result = invoke("0,3", &[]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
    assert!(
        !result.stderr.contains(&0x1b),
        "redirected progress contains ANSI escapes"
    );
    assert!(
        !result.stderr.contains(&b'\r'),
        "redirected progress contains carriage returns"
    );
    assert_eq!(pixels(&dir.path().join("check-000000.png")).len(), 36);
    assert_eq!(pixels(&dir.path().join("check-000003.png")).len(), 36);
    assert!(!invoke("0,3", &[]).status.success());
    assert!(invoke("0,3", &["--overwrite"]).status.success());
    assert!(invoke("0,3", &["--overwrite", "--no-ui"]).status.success());
    let invalid = invoke("0,4", &["--overwrite"]);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("frame 4 is out of range"));
    assert!(!invoke("-1", &[]).status.success());
    assert!(!invoke("0", &["--from", "0"]).status.success());
    assert!(!invoke("0", &["--output-format", "mp4"]).status.success());
}

#[test]
fn react_png_preserves_sequence_boundaries_and_prepares_once_without_audio_mix() {
    let runtime_path = root().join("packages/react/dist/cli.js");
    if !runtime_path.exists() {
        eprintln!("skipping React PNG test: build packages/react first");
        return;
    }
    let dir = tempfile::tempdir_in(root().join("packages/react")).unwrap();
    let entry = dir.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect, Sequence, Audio } from '@celesta/react';
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
    Exporter::new(ExportOptions::default())
        .export_react_png(
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

fn decode(path: &Path) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
        .read_info()
        .unwrap();
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    let (width, height) = (reader.info().width, reader.info().height);
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut pixels).unwrap();
    (width, height, pixels)
}

const COLORS: [[u8; 3]; 4] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]];

/// A 64x36, 4 fps, 8-frame project whose frames 0-1 are red, 2-3 green,
/// 4-5 blue and 6-7 white.
fn colored_project(dir: &Path) -> PathBuf {
    let mut project = Project::load(root().join("examples/minimal.celesta.json")).unwrap();
    project.settings.width = 64;
    project.settings.height = 36;
    project.settings.duration = Some(Time::new(2, 1));
    project.settings.frame_rate = Rational::new(4, 1);
    let mut items = Vec::new();
    for (index, color) in COLORS.iter().enumerate() {
        let name = format!("color-{index}.png");
        let file = std::fs::File::create(dir.join(&name)).unwrap();
        // Images are centered on the canvas origin, so twice the canvas size
        // covers all of it.
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 128, 72);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&color.repeat(128 * 72)).unwrap();
        writer.finish().unwrap();
        project.assets.insert(
            name.clone(),
            Asset::Image {
                name: None,
                source: AssetSource::File { path: name.clone() },
            },
        );
        items.push(TimelineItem {
            id: format!("item-{index}"),
            name: None,
            range: TimeRange {
                start: Time::new(index as i64, 2),
                duration: Time::new(1, 2),
            },
            content: TimelineContent::Image {
                asset: name,
                width: None,
                height: None,
                fit: None,
            },
            enabled: None,
            transform: None,
            opacity: None,
            blend_mode: None,
            effects: None,
        });
    }
    project.tracks.push(Track {
        id: "images".to_owned(),
        name: "Images".to_owned(),
        kind: TrackKind::Video,
        enabled: None,
        locked: None,
        muted: None,
        solo: None,
        items,
    });
    let source = dir.join("project.celesta.json");
    std::fs::write(&source, serde_json::to_vec(&project).unwrap()).unwrap();
    source
}

fn export(source: &Path, args: &[&str], output: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
        .arg(source)
        .args(args)
        .arg(output)
        .output()
        .unwrap()
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn every_selects_interval_frames_within_from_to_and_the_last_frame() {
    let dir = tempfile::tempdir().unwrap();
    let source = colored_project(dir.path());
    let output = dir.path().join("out").join("check.png");
    let pngs = |output: &Path| {
        let mut names: Vec<String> = std::fs::read_dir(output.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };

    let result = export(&source, &["--every", "3"], &output);
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(
        pngs(&output),
        [
            "check-000000.png",
            "check-000003.png",
            "check-000006.png",
            "check-000007.png"
        ]
    );
    for (frame, color) in [(0, COLORS[0]), (3, COLORS[1]), (6, COLORS[3])] {
        let (_, _, pixels) = decode(&dir.path().join(format!("out/check-{frame:06}.png")));
        assert_eq!(&pixels[..3], &color, "frame {frame}");
    }

    // --from 0.5 s is frame 2; --to 1.75 s is exclusive, so the span ends on frame 6.
    let ranged = dir.path().join("ranged").join("check.png");
    let result = export(
        &source,
        &["--every", "2", "--from", "0.5", "--to", "1.75"],
        &ranged,
    );
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(
        pngs(&ranged),
        ["check-000002.png", "check-000004.png", "check-000006.png"]
    );
    // A one-frame span is a single selection, written to the exact output name.
    let single = dir.path().join("single").join("check.png");
    let result = export(
        &source,
        &["--every", "100", "--from", "1", "--to", "1.25"],
        &single,
    );
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(pngs(&single), ["check.png"]);
}

#[test]
fn contact_sheet_lays_out_reduced_labelled_tiles_in_selection_order() {
    let dir = tempfile::tempdir().unwrap();
    let source = colored_project(dir.path());
    let output = dir.path().join("sheet.png");
    let args = [
        "--every",
        "2",
        "--contact-sheet",
        "--columns",
        "3",
        "--tile-width",
        "32",
    ];
    let result = export(&source, &args, &output);
    assert!(result.status.success(), "{}", stderr(&result));
    // Frames 0, 2, 4, 6, 7: two rows of three 32x18 tiles with 8 px gaps and
    // an 11 px label band (7 px font + 4) under each tile.
    let (width, height, pixels) = decode(&output);
    assert_eq!((width, height), (8 + 3 * (32 + 8), 8 + 2 * (18 + 11 + 8)));
    let at = |x: u32, y: u32| &pixels[((y * width + x) * 4) as usize..][..4];
    let tile = |index: u32| (8 + index % 3 * 40, 8 + index / 3 * 37);
    for (index, color) in [COLORS[0], COLORS[1], COLORS[2], COLORS[3], COLORS[3]]
        .into_iter()
        .enumerate()
    {
        let (left, top) = tile(index as u32);
        for (x, y) in [(left, top), (left + 31, top + 17), (left + 16, top + 9)] {
            assert_eq!(&at(x, y)[..3], &color, "tile {index} at ({x}, {y})");
        }
        // Background just outside the tile, label text in the band under it.
        assert_eq!(at(left - 1, top), &[0x20, 0x20, 0x20, 255]);
        let label = (top + 18..top + 29)
            .flat_map(|y| (left..left + 32).map(move |x| (x, y)))
            .filter(|&(x, y)| at(x, y) == [0xf0, 0xf0, 0xf0, 255])
            .count();
        assert!(label > 0, "tile {index} has no label");
    }
    // The unused sixth slot stays background.
    let (left, top) = tile(5);
    assert_eq!(at(left + 16, top + 9), &[0x20, 0x20, 0x20, 255]);

    // A wide enough tile carries "#<frame> HH:MM:SS:FF"; it starts at the
    // tile's left edge and at least doubles the 2-glyph "#0" label.
    let wide = dir.path().join("wide.png");
    let result = export(
        &source,
        &["--frames", "7,0", "--contact-sheet", "--tile-width", "160"],
        &wide,
    );
    assert!(result.status.success(), "{}", stderr(&result));
    let (width, height, pixels) = decode(&wide);
    assert_eq!((width, height), (8 + 2 * 168, 8 + 90 + 11 + 8));
    let at = |x: u32, y: u32| &pixels[((y * width + x) * 4) as usize..][..4];
    // --frames keeps the given order: frame 7 (white) first, then frame 0.
    assert_eq!(&at(8 + 80, 8 + 45)[..3], &COLORS[3]);
    assert_eq!(&at(176 + 80, 8 + 45)[..3], &COLORS[0]);
    let rightmost = (98..109)
        .flat_map(|y| (8..168).map(move |x| (x, y)))
        .filter(|&(x, y)| at(x, y) == [0xf0, 0xf0, 0xf0, 255])
        .map(|(x, _)| x)
        .max()
        .unwrap();
    assert!(rightmost > 8 + 60, "timecode missing from the label");

    // The sheet is one file and obeys --overwrite like any other output.
    assert!(!export(&source, &args, &output).status.success());
    assert!(
        export(&source, &[&args[..], &["--overwrite"]].concat(), &output)
            .status
            .success()
    );
}

#[test]
fn rejects_conflicting_and_oversized_png_selections() {
    let dir = tempfile::tempdir().unwrap();
    let source = colored_project(dir.path());
    let output = dir.path().join("check.png");
    let fails = |args: &[&str], message: &str| {
        let result = export(&source, args, &output);
        assert!(!result.status.success(), "{args:?} succeeded");
        assert!(
            stderr(&result).contains(message),
            "{args:?}: {}",
            stderr(&result)
        );
    };
    fails(
        &["--every", "2", "--frames", "0,1"],
        "--every cannot be combined with --frame/--frames",
    );
    fails(
        &["--frames", "0", "--from", "0"],
        "cannot be combined with --from/--to",
    );
    fails(&["--every", "0"], "--every must be a positive integer");
    fails(
        &["--every", "2", "--columns", "3"],
        "--columns/--tile-width require --contact-sheet",
    );
    fails(
        &["--every", "2", "--tile-width", "64"],
        "--columns/--tile-width require --contact-sheet",
    );
    fails(
        &["--contact-sheet"],
        "PNG output requires --frame, --frames or --every",
    );
    fails(
        &["--every", "2", "--contact-sheet", "--columns", "0"],
        "--columns must be a positive integer",
    );
    fails(
        &["--every", "2", "--output-format", "mp4"],
        "require PNG output",
    );
    fails(
        &["--every", "2", "--from", "3"],
        "does not cover any frames",
    );
    fails(
        &[
            "--every",
            "1",
            "--contact-sheet",
            "--columns",
            "1",
            "--tile-width",
            "16384",
        ],
        "the limit is 16384 on each side",
    );
    assert!(std::fs::read_dir(dir.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("check")
    }));
}
