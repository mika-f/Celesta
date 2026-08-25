use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use mikan_composition::{EvaluatedTransform, Layer, LayerContent, Rational, TextStyle, Time};
use mikan_react_bridge::{ProjectFrame, ReactBridge};

fn no_tracks() -> BTreeMap<String, Vec<Layer>> {
    BTreeMap::new()
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
