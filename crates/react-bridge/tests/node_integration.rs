use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use mikan_composition::{
    Animatable, EvaluatedTransform, Layer, LayerContent, Rational, TextStyle, Time,
};
use mikan_react_bridge::{
    ComponentPropertyField, ComponentResolutionRequest, ProjectFrame, ReactBridge,
};

fn no_tracks() -> BTreeMap<String, Vec<Layer>> {
    BTreeMap::new()
}

/// Depth-first iteration over a layer tree — `<Sequence>` wraps its children
/// in a group, so content of interest can be nested.
fn all_layers(layers: &[Layer]) -> Vec<&Layer> {
    let mut all = Vec::new();
    for layer in layers {
        if let LayerContent::Group { layers: children } = &layer.content {
            all.extend(all_layers(children));
        }
        all.push(layer);
    }
    all
}

#[test]
fn evaluates_the_example_composition_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/title.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let metadata = bridge.metadata().clone();
    assert_eq!((metadata.width, metadata.height), (1920, 1080));
    assert_eq!(metadata.frame_rate, Rational::new(30, 1));
    assert_eq!(metadata.duration_in_frames, 150);

    let scene = bridge.scene_at(Time::new(0, 30)).unwrap();
    assert_eq!((scene.width, scene.height), (1920, 1080));
    assert_eq!(scene.layers.len(), 1);

    // A second request on the same live process exercises the persistent
    // pipe rather than spawning a new Node process per frame.
    let scene_at_frame_fifteen = bridge.scene_at(Time::new(15, 30)).unwrap();
    assert_eq!(scene_at_frame_fifteen.time, Time::new(15, 30));
}

#[test]
fn computes_video_timing_from_the_composition_clock_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-video.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    // <Video src="./clip.mp4" startFrom={1} playbackRate={2} /> has no
    // timeline-item range the way a project clip does, so it plays synced
    // to the composition's own clock from frame 0: at frame 15/30 (0.5s in),
    // sourceTimeSeconds should be startFrom + 0.5 * playbackRate = 2.0.
    let scene = bridge.scene_at(Time::new(15, 30)).unwrap();
    assert_eq!(scene.layers.len(), 1);
    let LayerContent::Video { asset, timing } = &scene.layers[0].content else {
        panic!("expected a video layer");
    };
    assert_eq!(asset.id, "./clip.mp4");
    assert_eq!(timing.playback_rate, 2.0);
    assert_eq!(timing.source_time_seconds, 2.0);
}

#[test]
fn evaluates_a_rect_with_fill_stroke_and_corner_radius_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-rect.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let scene = bridge.scene_at(Time::new(0, 30)).unwrap();
    assert_eq!(scene.layers.len(), 1);
    let LayerContent::Rect {
        width,
        height,
        fill,
        stroke,
        corner_radius,
    } = &scene.layers[0].content
    else {
        panic!("expected a rect layer");
    };
    assert_eq!(*width, 200.0);
    assert_eq!(*height, 100.0);
    assert_eq!(
        fill,
        &Some(mikan_composition::Paint::Solid {
            color: "#3366CC".to_owned()
        })
    );
    let stroke = stroke.as_ref().expect("expected a stroke");
    assert_eq!(
        stroke.paint,
        mikan_composition::Paint::Solid {
            color: "#FFFFFF".to_owned()
        }
    );
    assert_eq!(stroke.width, 4.0);
    assert_eq!(*corner_radius, 16.0);
}

#[test]
fn reports_audio_clips_per_frame_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-audio.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    // <Audio src="./voice.wav" startFrom={1} playbackRate={2} volume={0.5}
    // muted={false} /> is reported by every rendered frame's evaluation
    // (gathered during the same tree walk as the layers), and contributes no
    // visual layer itself.
    let evaluation = bridge.evaluate_at(Time::new(0, 30), None).unwrap();
    assert_eq!(evaluation.scene.layers.len(), 1);
    assert!(matches!(
        &evaluation.scene.layers[0].content,
        LayerContent::Text { .. }
    ));
    assert_eq!(evaluation.audio.len(), 1);
    let clip = &evaluation.audio[0];
    assert_eq!(clip.src, "./voice.wav");
    assert_eq!(clip.source_start, 1.0);
    assert_eq!(clip.playback_rate, Animatable::Static(2.0));
    assert_eq!(clip.volume, Animatable::Static(0.5));
    assert!(!clip.muted);
    // Bare <Audio> spans the whole composition.
    assert_eq!(clip.start, 0.0);
    assert_eq!(clip.duration, 1.0);
}

#[test]
fn shifts_media_inside_sequences_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-sequence.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    // Frame 30 starts the first sequence; the video inside it plays synced
    // to the sequence's own clock (startFrom={1} playbackRate={2}), so at
    // frame 45 — half a second into the sequence — sourceTimeSeconds is
    // 1 + 0.5 * 2 = 2.0, exactly what the same props produce unsequenced at
    // frame 15.
    let mid = bridge.evaluate_at(Time::new(45, 30), None).unwrap();
    let video = all_layers(&mid.scene.layers)
        .into_iter()
        .find_map(|layer| match &layer.content {
            LayerContent::Video { timing, .. } => Some(timing),
            _ => None,
        })
        .expect("the sequenced video renders from its sequence's window on");
    assert_eq!(video.source_time_seconds, 2.0);

    // The audio inside that sequence is audible from the sequence's start to
    // the end of the composition, starting at its own local zero.
    assert_eq!(mid.audio.len(), 1);
    assert_eq!(mid.audio[0].src, "./voice.wav");
    assert_eq!(mid.audio[0].start, 1.0);
    assert_eq!(mid.audio[0].duration, 2.0);
    assert_eq!(mid.audio[0].source_start, 0.0);

    // Before the sequence starts: neither the video nor the audio exists,
    // and neither does the second sequence's text (frames 60-89 only).
    let early = bridge.evaluate_at(Time::ZERO, None).unwrap();
    assert_eq!(early.scene.layers.len(), 0);
    assert_eq!(early.audio.len(), 0);

    // Inside the second sequence, useCurrentFrame() reports the shifted
    // local clock: at composition frame 75 the text says "frame 15 inside".
    let late = bridge.evaluate_at(Time::new(75, 30), None).unwrap();
    assert!(all_layers(&late.scene.layers).iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "frame 15 inside"
    )));
}

#[test]
fn collects_conditionally_rendered_audio_with_keyframed_volume_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-conditional-audio.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    // The <Audio> is behind `{frame >= 15 && ...}`, so frames before 15
    // report nothing while later frames report it with its keyframed volume
    // animation intact.
    let before = bridge.evaluate_at(Time::ZERO, None).unwrap();
    assert_eq!(before.audio.len(), 0);

    let after = bridge.evaluate_at(Time::new(20, 30), None).unwrap();
    assert_eq!(after.audio.len(), 1);
    let clip = &after.audio[0];
    assert_eq!(clip.src, "./voice.wav");
    assert_eq!(clip.start, 0.0);
    assert_eq!(clip.duration, 2.0);
    let Animatable::Keyframes(volume) = &clip.volume else {
        panic!("expected the declared keyframed volume to survive collection");
    };
    assert_eq!(volume.keyframes.len(), 2);
    assert_eq!(volume.keyframes[0].value, 0.25);
    assert_eq!(volume.keyframes[1].value, 1.0);
}

#[test]
fn resolves_individual_components_through_the_bridge_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-registered-component.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let mut props = BTreeMap::new();
    props.insert("bossName".to_owned(), serde_json::json!("Golem"));
    props.insert("level".to_owned(), serde_json::json!(42));
    let requests = [
        ComponentResolutionRequest {
            component: "BossIntroduction",
            props: &props,
        },
        ComponentResolutionRequest {
            component: "SomeOtherThing",
            props: &BTreeMap::new(),
        },
    ];

    let resolved = bridge
        .resolve_components(&requests, Time::new(0, 30))
        .unwrap();
    assert_eq!(resolved.len(), 2);
    let registered = resolved[0]
        .as_ref()
        .expect("the registered component resolves");
    assert!(registered.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "Golem (Lv.42)"
    )));
    assert!(
        resolved[1].is_none(),
        "an unregistered name resolves to none"
    );
}

#[test]
fn reports_a_declared_project_property_schema_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-properties.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let schema = bridge
        .metadata()
        .project_property_schema
        .as_ref()
        .expect("the entry declares a project property schema");
    assert_eq!(
        schema.get("title"),
        Some(&ComponentPropertyField::String {
            label: Some("Title".to_owned()),
            default_value: "Mikan".to_owned(),
        })
    );
    assert_eq!(
        schema.get("accent"),
        Some(&ComponentPropertyField::Color {
            label: Some("Accent".to_owned()),
            default_value: "#ff8800".to_owned(),
        })
    );
    assert_eq!(
        schema.get("fontSize"),
        Some(&ComponentPropertyField::Number {
            label: Some("Font Size".to_owned()),
            default_value: 48.0,
            min: Some(8.0),
            max: Some(200.0),
            step: Some(2.0),
        })
    );
    assert_eq!(
        schema.get("showSubtitle"),
        Some(&ComponentPropertyField::Boolean {
            label: Some("Show Subtitle".to_owned()),
            default_value: false,
        })
    );
    assert_eq!(
        schema.get("weight"),
        Some(&ComponentPropertyField::Select {
            label: Some("Weight".to_owned()),
            default_value: "bold".to_owned(),
            options: vec!["bold".to_owned(), "light".to_owned()],
        })
    );

    // An entry that never calls defineProjectProperties() leaves the
    // metadata field None — distinct from a declared-but-empty schema.
    let plain = package_root.join("examples/title.tsx");
    let plain_bridge = ReactBridge::spawn(&node, &cli_script, &plain).unwrap();
    assert!(plain_bridge.metadata().project_property_schema.is_none());

    // The entry's own useProjectProperty() call reads its embedded project's
    // `properties.title` ("Chapter 3"), not the declared default.
    let scene = bridge.scene_at(Time::new(0, 30)).unwrap();
    assert!(scene.layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "Chapter 3"
    )));
}

#[test]
fn resolves_components_against_the_requested_time_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-frame-component.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    // The entry's own <Composition> declares 640x360@30fps; a resolved
    // component's useVideoConfig() must see those same facts.
    let request = [ComponentResolutionRequest {
        component: "FrameCaption",
        props: &BTreeMap::new(),
    }];

    let at_frame_fifteen = bridge
        .resolve_components(&request, Time::new(15, 30))
        .unwrap();
    let layers = at_frame_fifteen[0]
        .as_ref()
        .expect("the registered component resolves");
    assert!(layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "frame 15 of 640 at 30fps"
    )));

    // A second call on the same persistent root follows the new time rather
    // than staying on the first one (hook state persists; the clock moves).
    let at_frame_seven = bridge
        .resolve_components(&request, Time::new(7, 30))
        .unwrap();
    let layers = at_frame_seven[0]
        .as_ref()
        .expect("the registered component resolves");
    assert!(layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "frame 7 of 640 at 30fps"
    )));
}

#[test]
fn embeds_pre_evaluated_project_layers_into_project_timeline_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-project.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let project_layer = Layer {
        id: "from-project".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        content: LayerContent::Text {
            text: "from the project".to_owned(),
            style: TextStyle::default(),
            max_width: None,
        },
    };

    let layers = [project_layer];
    let tracks = no_tracks();
    let scene = bridge
        .scene_at_with_project(
            Time::new(0, 30),
            Some(ProjectFrame {
                layers: &layers,
                tracks: &tracks,
            }),
        )
        .unwrap();

    // <ProjectTimeline /> passes the given layers through untouched, and the
    // entry's own <Text> sibling still renders alongside them.
    assert!(scene.layers.iter().any(|layer| layer.id == "from-project"));
    assert!(scene.layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "React overlay"
    )));
}

#[test]
fn resolves_a_registered_component_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-registered-component.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let mut props = BTreeMap::new();
    props.insert("bossName".to_owned(), serde_json::json!("Golem"));
    props.insert("level".to_owned(), serde_json::json!(42));
    let registered_layer = Layer {
        id: "boss-intro".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        content: LayerContent::MissingComponent {
            component: "BossIntroduction".to_owned(),
            props,
        },
    };
    let unregistered_layer = Layer {
        id: "unregistered".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        content: LayerContent::MissingComponent {
            component: "SomeOtherThing".to_owned(),
            props: BTreeMap::new(),
        },
    };

    let layers = [registered_layer, unregistered_layer];
    let tracks = no_tracks();
    let scene = bridge
        .scene_at_with_project(
            Time::new(0, 30),
            Some(ProjectFrame {
                layers: &layers,
                tracks: &tracks,
            }),
        )
        .unwrap();

    // The registered component resolved to a real rendered subtree: a group
    // wrapping the Text it rendered, not the original missingComponent
    // layer.
    let resolved_group = scene
        .layers
        .iter()
        .find(|layer| layer.id == "boss-intro")
        .expect("resolved component layer");
    let LayerContent::Group { layers: children } = &resolved_group.content else {
        panic!("expected the resolved component to render as a group");
    };
    assert!(children.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "Golem (Lv.42)"
    )));

    // The unregistered component name passes through unresolved.
    let unregistered = scene
        .layers
        .iter()
        .find(|layer| layer.id == "unregistered")
        .expect("unresolved component layer");
    assert!(matches!(
        &unregistered.content,
        LayerContent::MissingComponent { component, .. } if component == "SomeOtherThing"
    ));
}

#[test]
fn reports_a_registered_components_property_schema_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-registered-component.tsx");
    let bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let schemas = &bridge.metadata().component_schemas;
    let schema = schemas
        .get("BossIntroduction")
        .expect("BossIntroduction declares a schema");
    assert_eq!(
        schema.get("bossName"),
        Some(&ComponentPropertyField::String {
            label: Some("Boss Name".to_owned()),
            default_value: "Golem".to_owned(),
        })
    );
    assert_eq!(
        schema.get("level"),
        Some(&ComponentPropertyField::Number {
            label: Some("Level".to_owned()),
            default_value: 1.0,
            min: Some(1.0),
            max: Some(999.0),
            step: None,
        })
    );
}

#[test]
fn embeds_per_track_layers_for_use_project_track_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-project-track.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let titles_layer = Layer {
        id: "title-1".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        content: LayerContent::Text {
            text: "a title".to_owned(),
            style: TextStyle::default(),
            max_width: None,
        },
    };
    let overlay_layer = Layer {
        id: "overlay-1".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        content: LayerContent::Text {
            text: "an overlay".to_owned(),
            style: TextStyle::default(),
            max_width: None,
        },
    };
    let mut tracks = BTreeMap::new();
    tracks.insert("titles".to_owned(), vec![titles_layer]);
    tracks.insert("overlays".to_owned(), vec![overlay_layer]);
    let layers: Vec<Layer> = Vec::new();

    let scene = bridge
        .scene_at_with_project(
            Time::new(0, 30),
            Some(ProjectFrame {
                layers: &layers,
                tracks: &tracks,
            }),
        )
        .unwrap();

    // useProjectTrack("titles") only saw that one track's layer count.
    assert!(scene.layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "titles track has 1 layer(s)"
    )));
    // <ProjectTrack id="overlays" /> passed its track's layer through as-is.
    assert!(scene.layers.iter().any(|layer| layer.id == "overlay-1"));
    // Neither saw the other's track content directly.
    assert!(!scene.layers.iter().any(|layer| layer.id == "title-1"));
}

fn live_react_runtime() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let Some(node) = find_executable("MIKAN_NODE", "node") else {
        eprintln!("skipping live Node.js test: node was not found");
        return None;
    };

    let package_root = react_package_root();
    let cli_script = package_root.join("dist/cli.js");
    if !package_root.join("node_modules").is_dir() || !cli_script.is_file() {
        eprintln!(
            "skipping live Node.js test: run `pnpm install && pnpm run build` in {} first",
            package_root.display()
        );
        return None;
    }

    Some((node, cli_script, package_root))
}

fn react_package_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/react")
        .canonicalize()
        .expect("packages/react must exist next to the crates/ workspace")
}

fn find_executable(environment: &str, command: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(environment).map(PathBuf::from)
        && executable_works(&path)
    {
        return Some(path);
    }
    let command = PathBuf::from(command);
    executable_works(&command).then_some(command)
}

fn executable_works(path: &Path) -> bool {
    Command::new(path)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}
