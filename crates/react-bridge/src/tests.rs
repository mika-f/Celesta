use crate::audio::{merge_react_audio_clips, react_audio_clips};
use crate::bridge::ReactBridge;
use crate::error::ReactBridgeError;
use crate::properties::{PropertyInputError, PropertyInputs, PropertySource};
use crate::protocol::{PropertyInputsMessage, ReadyMessage, Response};
use crate::types::{ComponentPropertyField, ReactAudioClipDescriptor};
use celesta_composition::{Animatable, AssetLocation, Time};
use std::path::Path;

use celesta_composition::{Keyframe, KeyframeAnimation, KeyframeAnimationType};

#[test]
fn reports_a_missing_node_executable() {
    let result = ReactBridge::spawn(
        "/definitely-not-installed/celesta-node",
        "cli.js",
        "entry.tsx",
    );
    assert!(matches!(result, Err(ReactBridgeError::Executable { .. })));
}

#[test]
fn deserializes_component_schemas_from_the_ready_message() {
    let json = serde_json::json!({
        "config": {
            "width": 640,
            "height": 360,
            "frameRate": {"numerator": 30, "denominator": 1},
            "durationInFrames": 30
        },
        "componentSchemas": {
            "BossIntroduction": {
                "bossName": {"type": "string", "label": "Boss Name", "defaultValue": "Golem"},
                "level": {
                    "type": "number",
                    "defaultValue": 1,
                    "min": 1,
                    "max": 999
                }
            }
        }
    })
    .to_string();

    let ReadyMessage::Ready {
        config,
        component_schemas,
        ..
    } = serde_json::from_str(&json).unwrap()
    else {
        panic!("expected a Ready message");
    };
    assert_eq!(config.width, 640);
    let schema = component_schemas.get("BossIntroduction").unwrap();
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
            label: None,
            default_value: 1.0,
            min: Some(1.0),
            max: Some(999.0),
            step: None,
        })
    );
}

#[test]
fn ready_message_without_component_schemas_defaults_to_empty() {
    let json = serde_json::json!({
        "config": {
            "width": 640,
            "height": 360,
            "frameRate": {"numerator": 30, "denominator": 1},
            "durationInFrames": 30
        }
    })
    .to_string();

    let ReadyMessage::Ready {
        component_schemas, ..
    } = serde_json::from_str(&json).unwrap()
    else {
        panic!("expected a Ready message");
    };
    assert!(component_schemas.is_empty());
}

#[test]
fn deserializes_the_project_property_schema_from_the_ready_message() {
    let json = serde_json::json!({
        "config": {
            "width": 640,
            "height": 360,
            "frameRate": {"numerator": 30, "denominator": 1},
            "durationInFrames": 30
        },
        "componentSchemas": {},
        "propertySchema": {
            "title": {"type": "string", "label": "Title", "defaultValue": "Celesta"},
            "accent": {"type": "color", "defaultValue": "#ff8800"},
            "opacity": {"type": "number", "defaultValue": 1.0, "min": 0.0, "max": 1.0},
            "visible": {"type": "boolean", "defaultValue": true},
            "style": {"type": "select", "defaultValue": "bold", "options": ["bold", "light"]}
        }
    })
    .to_string();

    let ReadyMessage::Ready {
        project_property_schema,
        ..
    } = serde_json::from_str(&json).unwrap()
    else {
        panic!("expected a Ready message");
    };
    let schema = project_property_schema.expect("a declared property schema");
    assert_eq!(
        schema.get("title"),
        Some(&ComponentPropertyField::String {
            label: Some("Title".to_owned()),
            default_value: "Celesta".to_owned(),
        })
    );
    assert_eq!(
        schema.get("accent"),
        Some(&ComponentPropertyField::Color {
            label: None,
            default_value: "#ff8800".to_owned(),
        })
    );
    assert_eq!(
        schema.get("opacity"),
        Some(&ComponentPropertyField::Number {
            label: None,
            default_value: 1.0,
            min: Some(0.0),
            max: Some(1.0),
            step: None,
        })
    );
    assert_eq!(
        schema.get("visible"),
        Some(&ComponentPropertyField::Boolean {
            label: None,
            default_value: true,
        })
    );
    assert_eq!(
        schema.get("style"),
        Some(&ComponentPropertyField::Select {
            label: None,
            default_value: "bold".to_owned(),
            options: vec!["bold".to_owned(), "light".to_owned()],
        })
    );
}

#[test]
fn ready_message_without_a_project_property_schema_is_none() {
    for json in [
        serde_json::json!({
            "config": {
                "width": 640,
                "height": 360,
                "frameRate": {"numerator": 30, "denominator": 1},
                "durationInFrames": 30
            }
        }),
        // `null` is what the CLI sends when the entry never called
        // defineProjectProperties(); an empty object would be a
        // declared-but-empty schema.
        serde_json::json!({
            "config": {
                "width": 640,
                "height": 360,
                "frameRate": {"numerator": 30, "denominator": 1},
                "durationInFrames": 30
            },
            "propertySchema": null
        }),
    ] {
        let ReadyMessage::Ready {
            project_property_schema,
            ..
        } = serde_json::from_str(&json.to_string()).unwrap()
        else {
            panic!("expected a Ready message");
        };
        assert!(project_property_schema.is_none());
    }
}

#[test]
fn deserializes_audio_clips_from_a_frame_response() {
    let json = serde_json::json!({
        "scene": {
            "width": 640,
            "height": 360,
            "frameRate": {"numerator": 30, "denominator": 1},
            "time": {"value": 0, "timescale": 1},
            "layers": []
        },
        "audio": [
            {
                "src": "./voice.wav",
                "sourceStart": 1.5,
                "playbackRate": {"type": "keyframes", "keyframes": [
                    {"time": {"value": 0, "timescale": 1}, "value": 1.0}
                ]},
                "volume": 0.5,
                "muted": false,
                "start": 2.0,
                "duration": 3.0
            }
        ]
    })
    .to_string();

    let Response::Ok { audio, .. } = serde_json::from_str(&json).unwrap() else {
        panic!("expected an Ok response");
    };
    assert_eq!(
        audio,
        vec![ReactAudioClipDescriptor {
            src: "./voice.wav".to_owned(),
            source_start: 1.5,
            playback_rate: Animatable::Keyframes(KeyframeAnimation {
                kind: KeyframeAnimationType::Keyframes,
                keyframes: vec![Keyframe {
                    time: Time::ZERO,
                    value: 1.0,
                    easing: None,
                }],
            }),
            volume: Animatable::Static(0.5),
            muted: false,
            start: 2.0,
            duration: 3.0,
        }]
    );
}

#[test]
fn frame_response_without_audio_defaults_to_empty() {
    let json = serde_json::json!({
        "scene": {
            "width": 640,
            "height": 360,
            "frameRate": {"numerator": 30, "denominator": 1},
            "time": {"value": 0, "timescale": 1},
            "layers": []
        }
    })
    .to_string();

    let Response::Ok { audio, .. } = serde_json::from_str(&json).unwrap() else {
        panic!("expected an Ok response");
    };
    assert!(audio.is_empty());
}

#[test]
fn deserializes_component_resolutions_from_a_response() {
    let json = serde_json::json!({
        "components": [
            null,
            [
                {
                    "id": "resolved",
                    "transform": {
                        "position": {"x": 0.0, "y": 0.0},
                        "scale": {"x": 1.0, "y": 1.0},
                        "rotation": 0.0,
                        "anchor": {"x": 0.5, "y": 0.5}
                    },
                    "opacity": 1.0,
                    "content": {"type": "text", "text": "hello", "style": {}}
                }
            ]
        ]
    })
    .to_string();

    let Response::Components { components } = serde_json::from_str(&json).unwrap() else {
        panic!("expected a Components response");
    };
    assert_eq!(components.len(), 2);
    assert!(components[0].is_none());
    let resolved = components[1].as_ref().unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].id, "resolved");
}

#[test]
fn fills_omitted_transform_fields_with_defaults() {
    let json = serde_json::json!({
        "scene": {
            "width": 64,
            "height": 64,
            "frameRate": {"numerator": 30, "denominator": 1},
            "time": {"value": 0, "timescale": 1},
            "layers": [{
                "id": "compact",
                "transform": {"position": {"x": 3.0, "y": 4.0}},
                "opacity": 1.0,
                "content": {"type": "rect", "width": 4.0, "height": 4.0}
            }]
        }
    })
    .to_string();

    let Response::Ok { scene, .. } = serde_json::from_str(&json).unwrap() else {
        panic!("expected an Ok response");
    };
    let transform = scene.layers[0].transform;
    assert_eq!(
        transform.position,
        celesta_composition::Point { x: 3.0, y: 4.0 }
    );
    assert_eq!(
        transform,
        celesta_composition::EvaluatedTransform {
            position: transform.position,
            ..Default::default()
        }
    );
}

#[test]
fn deserializes_errors_and_rejects_unknown_responses() {
    let Response::Err { error } = serde_json::from_str(r#"{"error":"boom"}"#).unwrap() else {
        panic!("expected an Err response");
    };
    assert_eq!(error, "boom");
    assert!(serde_json::from_str::<Response>(r#"{"unexpected":1}"#).is_err());
}

fn audio_descriptor(src: &str, start: f64) -> ReactAudioClipDescriptor {
    ReactAudioClipDescriptor {
        src: src.to_owned(),
        source_start: 0.0,
        playback_rate: Animatable::Static(1.0),
        volume: Animatable::Static(1.0),
        muted: false,
        start,
        duration: 2.0,
    }
}

#[test]
fn merge_react_audio_clips_collapses_identical_per_frame_reports() {
    let clips = vec![
        audio_descriptor("a.wav", 0.0),
        audio_descriptor("a.wav", 0.0),
        audio_descriptor("b.wav", 1.0),
        audio_descriptor("a.wav", 0.0),
    ];
    let merged = merge_react_audio_clips(&clips);
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].0.src, "a.wav");
    assert_eq!(merged[0].1, 3, "three frames reported the first clip");
    assert_eq!(merged[1].0.src, "b.wav");
    assert_eq!(merged[1].1, 1);
}

#[test]
fn react_audio_clips_resolve_relative_paths_and_carry_ranges() {
    let clips = vec![
        audio_descriptor("./voice.wav", 1.0),
        audio_descriptor("/abs/music.wav", 0.0),
        audio_descriptor("https://example.com/bgm.mp3", 0.0),
    ];
    let built = react_audio_clips(&clips, Path::new("/entry/dir"));
    assert_eq!(built.len(), 3);
    assert_eq!(built[0].id, "react-audio:0");
    assert_eq!(built[0].range.start.as_seconds().unwrap(), 1.0);
    assert_eq!(built[0].range.duration.as_seconds().unwrap(), 2.0);
    let AssetLocation::File { path } = &built[0].asset.location else {
        panic!("expected a file asset");
    };
    assert!(
        path.replace('\\', "/").ends_with("/entry/dir/./voice.wav"),
        "relative src joined against the entry dir, got {path}"
    );
    let AssetLocation::File { path } = &built[1].asset.location else {
        panic!("expected a file asset");
    };
    assert_eq!(path, "/abs/music.wav", "absolute src left untouched");
    assert_eq!(
        built[2].asset.location,
        AssetLocation::Url {
            url: "https://example.com/bgm.mp3".to_owned()
        },
        "URL src kept as a URL"
    );
}

#[test]
fn loads_props_file_below_inline_props_with_their_own_path_bases() {
    let dir = std::env::temp_dir().join(format!("celesta-props-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("variants")).unwrap();
    let file = dir.join("variants/spring.json");
    std::fs::write(&file, r#"{"title":"Spring","data":"rows.json"}"#).unwrap();

    let inputs = PropertyInputs::load(Some(&file), Some(r#"{"title":"Inline"}"#)).unwrap();
    let layers = inputs.layers();
    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].source, PropertySource::PropsFile);
    assert_eq!(layers[0].base_dir, dir.join("variants"));
    assert_eq!(layers[0].values["data"], "rows.json");
    assert_eq!(layers[1].source, PropertySource::Props);
    assert_eq!(layers[1].base_dir, std::env::current_dir().unwrap());

    // A companion project's values go below both.
    let project = std::collections::BTreeMap::from([("title".to_owned(), serde_json::json!("P"))]);
    let with_project = inputs.with_project(&project, Some(Path::new("p.celesta.json")), &dir);
    assert_eq!(with_project.layers()[0].source, PropertySource::Project);
    let message = serde_json::to_value(PropertyInputsMessage {
        property_inputs: with_project.layers(),
    })
    .unwrap();
    assert_eq!(message["propertyInputs"][0]["source"], "project");
    assert_eq!(message["propertyInputs"][0]["file"], "p.celesta.json");
    assert_eq!(message["propertyInputs"][1]["source"], "propsFile");
    assert!(message["propertyInputs"][2].get("file").is_none());

    // An empty project map adds nothing, so the CLI is not told to read stdin.
    let empty = PropertyInputs::default().with_project(&Default::default(), None, &dir);
    assert!(empty.is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn rejects_props_that_are_not_json_objects() {
    assert!(matches!(
        PropertyInputs::load(None, Some("[1]")),
        Err(PropertyInputError::NotAnObject(origin)) if origin == "--props"
    ));
    assert!(matches!(
        PropertyInputs::load(None, Some("{title:")),
        Err(PropertyInputError::Parse { .. })
    ));
    let missing = PropertyInputs::load(Some(Path::new("no-such-props.json")), None).unwrap_err();
    assert!(
        missing
            .to_string()
            .starts_with("could not read --props-file no-such-props.json")
    );
}

#[test]
fn reads_invalid_properties_before_ready() {
    let message: ReadyMessage = serde_json::from_str(
        r#"{"invalidProperties":[{"key":"accent","source":"--props","message":"expected a color"},{"key":"","source":"--props-file a.json","message":"no schema"}]}"#,
    )
    .unwrap();
    let ReadyMessage::InvalidProperties { invalid_properties } = message else {
        panic!("expected invalid properties");
    };
    let error = ReactBridgeError::InvalidProperties(invalid_properties);
    assert_eq!(
        error.to_string(),
        "invalid project properties:\n  - --props: accent: expected a color\n  - --props-file a.json: no schema"
    );
}
