use crate::clock::TimelineClock;
use crate::document::EditorDocument;
use crate::summary::ClipKind;
use celesta_composition::{Rational, Time};
use celesta_project::TrackKind;
use std::fs;
use std::path::Path;

const MINIMAL: &str = include_str!("../../../examples/minimal.celesta.json");
const VOICEROID: &str = include_str!("../../../examples/voiceroid.celesta.json");

#[test]
fn loads_an_empty_document() {
    let document = EditorDocument::from_json(MINIMAL, ".").unwrap();

    assert_eq!(document.display_name(), "Untitled");
    assert!(document.assets().is_empty());
    assert!(document.tracks().is_empty());
    assert_eq!(document.scene_at(Time::ZERO).unwrap().layers.len(), 0);
}

#[test]
fn exposes_editor_summaries_without_changing_the_project() {
    let document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

    assert_eq!(document.assets().len(), 2);
    assert_eq!(document.tracks()[0].kind, TrackKind::Dialogue);
    assert_eq!(document.tracks()[0].item_count, 1);
    assert_eq!(document.tracks()[0].clips[0].start, Time::new(5, 1));
    assert_eq!(document.tracks()[0].clips[0].duration, Time::new(3, 1));
    assert_eq!(document.tracks()[0].clips[0].kind, ClipKind::Dialogue);
    assert_eq!(document.duration(), Time::new(8, 1));
}

#[test]
fn timeline_clock_uses_exact_project_frames_and_clamps() {
    let mut clock = TimelineClock::new(Time::new(3, 2), Rational::new(60, 1)).unwrap();

    assert_eq!(clock.end_frame(), 90);
    clock.seek(75);
    assert_eq!(clock.time().unwrap(), Time::new(75, 60));
    clock.step(30);
    assert_eq!(clock.frame(), 90);
    assert!(clock.is_at_end());
    clock.step(-100);
    assert_eq!(clock.frame(), 0);
    assert_eq!(clock.frame_for_time(Time::new(5, 4)).unwrap(), 75);
}

#[test]
fn timeline_clock_rounds_partial_final_frames_up() {
    let clock = TimelineClock::new(Time::new(1, 100), Rational::new(60, 1)).unwrap();

    assert_eq!(clock.end_frame(), 1);
}

#[test]
fn timeline_clock_maps_scrub_positions_to_clamped_frames() {
    let mut clock = TimelineClock::new(Time::new(10, 1), Rational::new(60, 1)).unwrap();

    assert_eq!(clock.frame_at_fraction(0.25), 150);
    clock.seek_fraction(0.75);
    assert_eq!(clock.frame(), 450);
    clock.seek_fraction(2.0);
    assert_eq!(clock.frame(), 600);
    clock.seek_fraction(f32::NAN);
    assert_eq!(clock.frame(), 0);
}

#[test]
fn loads_the_editor_demo_timeline() {
    let document = EditorDocument::from_json(
        include_str!("../../../examples/editor-demo.celesta.json"),
        "examples",
    )
    .unwrap();

    assert_eq!(document.duration(), Time::new(10, 1));
    assert_eq!(document.tracks()[0].clips[0].start, Time::ZERO);
    assert_eq!(document.tracks()[0].clips[0].duration, Time::new(10, 1));
}

#[test]
fn voiceroid_example_renders_during_dialogue_when_a_gpu_is_available() {
    let asset_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let document = EditorDocument::from_json(VOICEROID, asset_root).unwrap();
    let Ok(mut renderer) =
        celesta_gpu_renderer::GpuRenderer::new(celesta_gpu_renderer::GpuRenderOptions::default())
    else {
        return;
    };
    renderer = renderer.with_asset_root(document.asset_root());

    let scene = document.scene_at(Time::new(11, 2)).unwrap();
    let frame = renderer.render(&scene).unwrap();

    assert_eq!(frame.width(), 1920);
    assert_eq!(frame.height(), 1080);
}

#[test]
fn react_preview_builds_a_synthetic_document_from_composition_facts() {
    let root = std::env::temp_dir().join(format!(
        "celesta-editor-react-preview-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let entry_path = root.join("title.tsx");
    fs::write(
        &entry_path,
        b"export default function Root() { return null; }",
    )
    .unwrap();

    let document =
        EditorDocument::react_preview(&entry_path, 1920, 1080, Rational::new(30, 1), 48_000, 150)
            .unwrap();

    assert_eq!(document.project().settings.width, 1920);
    assert_eq!(document.project().settings.height, 1080);
    assert!(document.project().tracks.is_empty());
    assert_eq!(document.react_entry(), Some("title.tsx"));
    assert_eq!(
        document.react_entry_absolute_path().unwrap(),
        fs::canonicalize(&entry_path).unwrap()
    );
    // 150 frames at 30fps is exactly five seconds.
    assert_eq!(document.duration().as_seconds().unwrap(), 5.0);
}

#[test]
fn track_mute_and_solo_toggle_the_audio_graph() {
    let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

    assert!(document.toggle_track_muted("dialogue").unwrap());
    assert!(document.tracks()[0].muted);
    assert!(document.audio_graph().unwrap().clips.is_empty());
    assert!(!document.toggle_track_muted("dialogue").unwrap());
    assert!(!document.audio_graph().unwrap().clips.is_empty());

    assert!(document.toggle_track_solo("dialogue").unwrap());
    assert!(document.tracks()[0].solo);
    assert!(document.toggle_track_muted("missing").is_err());
}

#[test]
fn react_entry_resolves_relative_to_the_project_file() {
    let root =
        std::env::temp_dir().join(format!("celesta-editor-react-entry-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let project_path = root.join("project.celesta.json");
    let mut project: serde_json::Value = serde_json::from_str(MINIMAL).unwrap();
    project["settings"]["reactEntry"] = "./react/entry.tsx".into();
    fs::write(&project_path, project.to_string()).unwrap();

    let document = EditorDocument::load(&project_path).unwrap();

    assert_eq!(document.display_name(), "project");
    assert_eq!(document.react_entry(), Some("./react/entry.tsx"));
    assert_eq!(
        document.react_entry_absolute_path().unwrap(),
        fs::canonicalize(&root).unwrap().join("./react/entry.tsx")
    );
    fs::remove_dir_all(root).unwrap();
}
