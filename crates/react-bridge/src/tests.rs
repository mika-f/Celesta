use crate::audio::{merge_react_audio_clips, react_audio_clips, react_video_clip_ids};
use crate::bridge::ReactBridge;
use crate::error::ReactBridgeError;
use crate::occurrences::{ElementOccurrences, InstanceOccurrences, OccurrenceIndex};
use crate::properties::{
    ENTRY_PROPS_FILE, EffectiveValue, PropertyInputError, PropertyInputs, PropertySource,
    ValueSource, effective_values, resolve_props_file,
};
use crate::protocol::{
    PropertyInputsMessage, ReadyMessage, Request, Response, ready_property_order,
};
use crate::types::{
    ComponentPropertyField, ElementKind, FrameSpan, LayerSource, LayerSourceReport, PropOverride,
    ReactAudioClipDescriptor, ReactChapter, ReceivedProps, ReceivedValue, SourcePos, TimingWindow,
    chapter_at,
};
use celesta_composition::{
    Animatable, AssetLocation, AudioDuck, LayerFilter, ShaderParam, ShaderParamType, ShaderSource,
    Time,
};
use std::collections::BTreeMap;
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
fn an_unexpected_exit_shows_the_runtime_s_last_output() {
    let bare = ReactBridgeError::UnexpectedExit {
        stderr: String::new(),
    };
    assert_eq!(
        bare.to_string(),
        "the Node.js React runtime exited unexpectedly"
    );
    let with_output = ReactBridgeError::UnexpectedExit {
        stderr: "Error: Cannot find module '@celesta/react'\n\nNode.js v24.0.0".to_owned(),
    };
    assert_eq!(
        with_output.to_string(),
        "the Node.js React runtime exited unexpectedly\nIts last output:\n  \
         Error: Cannot find module '@celesta/react'\n\n  Node.js v24.0.0"
    );
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
fn reads_chapters_from_the_ready_config() {
    let json = serde_json::json!({
        "config": {
            "width": 640,
            "height": 360,
            "frameRate": {"numerator": 30, "denominator": 1},
            "durationInFrames": 90,
            "chapters": [
                {"id": "intro", "name": "Intro", "from": 0, "durationInFrames": 30},
                {"id": "mark", "name": "mark", "from": 30, "durationInFrames": 0}
            ]
        }
    })
    .to_string();
    let ReadyMessage::Ready { config, .. } = serde_json::from_str(&json).unwrap() else {
        panic!("expected a Ready message");
    };
    assert_eq!(config.chapters.len(), 2);
    assert_eq!(config.chapters[0].name, "Intro");
    assert_eq!(config.chapters[1].end(), 30);

    // Older runtimes and compositions without chapters send none.
    let json = r#"{"config":{"width":1,"height":1,"frameRate":{"numerator":30,"denominator":1},"durationInFrames":1}}"#;
    let ReadyMessage::Ready { config, .. } = serde_json::from_str(json).unwrap() else {
        panic!("expected a Ready message");
    };
    assert!(config.chapters.is_empty());
}

#[test]
fn chapter_at_prefers_the_later_overlapping_chapter_and_skips_gaps_and_marks() {
    let chapter = |id: &str, from, duration_in_frames| ReactChapter {
        id: id.to_owned(),
        name: id.to_owned(),
        from,
        duration_in_frames,
    };
    let chapters = [
        chapter("a", 0, 30),
        chapter("mark", 20, 0),
        chapter("b", 20, 20),
        chapter("c", 50, 10),
    ];
    let current = |frame| chapter_at(&chapters, frame).map(|chapter| chapter.id.as_str());
    assert_eq!(current(0), Some("a"));
    assert_eq!(current(19), Some("a"));
    assert_eq!(current(20), Some("b"));
    assert_eq!(current(39), Some("b"));
    assert_eq!(current(40), None);
    assert_eq!(current(50), Some("c"));
    assert_eq!(current(60), None);
    assert_eq!(chapter_at(&[], 0), None);
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
            "style": {"type": "select", "defaultValue": "bold", "options": ["bold", "light"]},
            "data": {"type": "path", "label": "Data", "defaultValue": "./rows.json"}
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
        schema.get("data"),
        Some(&ComponentPropertyField::Path {
            label: Some("Data".to_owned()),
            default_value: "./rows.json".to_owned(),
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
            r#loop: false,
            video: false,
            pitch: Animatable::Static(0.0),
            preserve_pitch: false,
            fade_in: 0.0,
            fade_out: 0.0,
            duck_group: None,
            duck: None,
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

    let Response::Components {
        components,
        shaders,
    } = serde_json::from_str(&json).unwrap()
    else {
        panic!("expected a Components response");
    };
    assert_eq!(components.len(), 2);
    assert!(components[0].is_none());
    let resolved = components[1].as_ref().unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].id, "resolved");
    // Older runtimes send no shaders.
    assert!(shaders.is_empty());
}

#[test]
fn deserializes_the_shaders_component_resolutions_use() {
    let json = serde_json::json!({
        "components": [[{
            "id": "shaded",
            "transform": {},
            "opacity": 1.0,
            "effects": {"blur": 0.0, "filters": [{"type": "shader", "id": "1b0a62b2133ed451", "params": [0.5]}]},
            "content": {"type": "group", "layers": []}
        }]],
        "shaders": [{
            "id": "1b0a62b2133ed451",
            "name": "tint",
            "wgsl": "fn effect(input: EffectInput) -> vec4f { return vec4f(0.0); }",
            "params": [{"name": "amount", "type": "f32"}]
        }]
    })
    .to_string();

    let Response::Components {
        components,
        shaders,
    } = serde_json::from_str(&json).unwrap()
    else {
        panic!("expected a Components response");
    };
    let LayerFilter::Shader(shader) = &components[0].as_ref().unwrap()[0].effects.filters[0] else {
        panic!("expected a custom shader");
    };
    assert_eq!(shader.id, "1b0a62b2133ed451");
    assert_eq!(
        shaders,
        [ShaderSource {
            id: "1b0a62b2133ed451".to_owned(),
            name: Some("tint".to_owned()),
            wgsl: "fn effect(input: EffectInput) -> vec4f { return vec4f(0.0); }".to_owned(),
            params: vec![ShaderParam {
                name: "amount".to_owned(),
                ty: ShaderParamType::F32,
            }],
        }]
    );
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

#[test]
fn deserializes_layer_source_replies() {
    let Response::LayerSource { report } =
        serde_json::from_str(r#"{"layerSource":{"chain":null}}"#).unwrap()
    else {
        panic!("expected a LayerSource response");
    };
    assert_eq!(report, None);

    // No positions recorded: found, with an empty chain and no other keys.
    let Response::LayerSource { report } =
        serde_json::from_str(r#"{"layerSource":{"chain":[]}}"#).unwrap()
    else {
        panic!("expected a LayerSource response");
    };
    assert_eq!(
        report,
        Some(LayerSourceReport {
            chain: vec![],
            local_frame: 0.0,
            candidate_frames: vec![],
            props: ReceivedProps::default(),
            render_count: 1,
            windows: vec![],
            span: None,
        })
    );

    // An older runtime: no kind, localFrame, props, or renderCount.
    let json =
        r#"{"layerSource":{"chain":[{"name":"Rect","file":"/a/title.tsx","line":3,"column":10}]}}"#;
    let Response::LayerSource { report } = serde_json::from_str(json).unwrap() else {
        panic!("expected a LayerSource response");
    };
    assert_eq!(
        report,
        Some(LayerSourceReport {
            chain: vec![LayerSource {
                name: "Rect".to_owned(),
                file: "/a/title.tsx".into(),
                line: 3,
                column: 10,
                kind: ElementKind::Component,
                layer_id: None,
            }],
            local_frame: 0.0,
            candidate_frames: vec![],
            props: ReceivedProps::default(),
            render_count: 1,
            windows: vec![],
            span: None,
        })
    );

    let json = r#"{"layerSource":{"chain":[
        {"name":"Rect","file":"/a/title.tsx","line":3,"column":10,"kind":"primitive","layerId":"root.2"},
        {"name":"Title","file":"/a/main.tsx","line":8,"column":4,"kind":"component"}],
        "localFrame":12.5,
        "candidateFrames":[12.5,40],
        "props":{"x":10,"y":-4.5,"rotation":30,"scale":2,"scaleX":1.5,"scaleY":0.5},
        "renderCount":3}}"#;
    let Response::LayerSource { report } = serde_json::from_str(json).unwrap() else {
        panic!("expected a LayerSource response");
    };
    let report = report.expect("found");
    assert_eq!(report.chain.len(), 2);
    assert_eq!(report.chain[0].kind, ElementKind::Primitive);
    assert_eq!(report.chain[0].layer_id.as_deref(), Some("root.2"));
    assert_eq!(report.chain[1].kind, ElementKind::Component);
    assert_eq!(report.chain[1].layer_id, None);
    assert_eq!(report.local_frame, 12.5);
    assert_eq!(report.candidate_frames, vec![12.5, 40.0]);
    assert_eq!(
        report.props,
        ReceivedProps {
            x: Some(10.0),
            y: Some(-4.5),
            rotation: Some(30.0),
            scale: Some(2.0),
            scale_x: Some(1.5),
            scale_y: Some(0.5),
            others: BTreeMap::new(),
        }
    );
    assert_eq!(report.render_count, 3);
}

#[test]
fn received_props_read_scalars() {
    let props: ReceivedProps = serde_json::from_str(
        r##"{"x":1,"color":"#fff","wrap":true,"children":"Sale","style.fontSize":48}"##,
    )
    .unwrap();
    assert_eq!(props.x, Some(1.0));
    assert_eq!(props.get("x"), Some(ReceivedValue::Number(1.0)));
    assert_eq!(
        props.get("color"),
        Some(ReceivedValue::String("#fff".to_owned()))
    );
    assert_eq!(props.get("wrap"), Some(ReceivedValue::Bool(true)));
    assert_eq!(
        props.get("children"),
        Some(ReceivedValue::String("Sale".to_owned()))
    );
    assert_eq!(
        props.get("style.fontSize"),
        Some(ReceivedValue::Number(48.0))
    );
    assert_eq!(props.get("y"), None);
    assert_eq!(props.get("fontSize"), None);
}

#[test]
fn layer_source_reads_wrapper_entries() {
    let json = r#"{"layerSource":{"chain":[
        {"name":"Rect","file":"/a/title.tsx","line":3,"column":10,"kind":"primitive","layerId":"root.2"},
        {"name":"Sequence","file":"/a/title.tsx","line":2,"column":8,"kind":"wrapper"},
        {"name":"Title","file":"/a/main.tsx","line":8,"column":4,"kind":"component"}]}}"#;
    let Response::LayerSource { report } = serde_json::from_str(json).unwrap() else {
        panic!("expected a LayerSource response");
    };
    let report = report.expect("found");
    let kinds: Vec<_> = report.chain.iter().map(|entry| entry.kind).collect();
    assert_eq!(
        kinds,
        vec![
            ElementKind::Primitive,
            ElementKind::Wrapper,
            ElementKind::Component
        ]
    );
    assert_eq!(report.chain[1].layer_id, None);
}

#[test]
fn layer_source_reply_reads_windows_span_and_wrapper_kind() {
    let json = r#"{"layerSource":{"chain":[
        {"name":"Rect","file":"/a/title.tsx","line":3,"column":10,"kind":"primitive","layerId":"root.2"},
        {"name":"Sequence","file":"/a/title.tsx","line":2,"column":8,"kind":"wrapper"}],
        "windows":[
            {"kind":"Sequence","from":30.5,"to":90,"source":{"file":"/a/title.tsx","line":2,"column":8}},
            {"kind":"Loop","from":0,"to":120,"remapped":true}],
        "span":{"from":30.5,"to":90}}}"#;
    let Response::LayerSource { report } = serde_json::from_str(json).unwrap() else {
        panic!("expected a LayerSource response");
    };
    let report = report.expect("found");
    assert_eq!(report.chain[1].kind, ElementKind::Wrapper);
    assert_eq!(
        report.windows,
        vec![
            TimingWindow {
                kind: "Sequence".to_owned(),
                from: 30.5,
                to: 90.0,
                source: Some(SourcePos {
                    file: "/a/title.tsx".into(),
                    line: 2,
                    column: 8,
                }),
                remapped: false,
            },
            TimingWindow {
                kind: "Loop".to_owned(),
                from: 0.0,
                to: 120.0,
                source: None,
                remapped: true,
            },
        ]
    );
    assert_eq!(
        report.span,
        Some(FrameSpan {
            from: 30.5,
            to: 90.0
        })
    );
}

#[test]
fn layer_source_reply_from_an_older_runtime_has_no_windows() {
    let json =
        r#"{"layerSource":{"chain":[{"name":"Rect","file":"/a/title.tsx","line":3,"column":10}]}}"#;
    let Response::LayerSource { report } = serde_json::from_str(json).unwrap() else {
        panic!("expected a LayerSource response");
    };
    let report = report.expect("found");
    assert!(report.windows.is_empty());
    assert_eq!(report.span, None);
}

#[test]
fn collected_occurrences_response_parses() {
    let json = r#"{"collectedOccurrences":{"elements":[{
        "element":{"file":"/a/title.tsx","line":3,"column":10},
        "instances":[{
            "chain":[
                {"file":"/a/title.tsx","line":3,"column":10},
                {"file":"/a/main.tsx","line":8,"column":4}],
            "ranges":[[0,30],[60,90]],
            "maxPerFrame":2}]}]}}"#;
    let Response::CollectedOccurrences {
        collected_occurrences,
    } = serde_json::from_str(json).unwrap()
    else {
        panic!("expected a CollectedOccurrences response");
    };
    let pos = |file: &str, line, column| SourcePos {
        file: file.into(),
        line,
        column,
    };
    assert_eq!(
        collected_occurrences,
        OccurrenceIndex {
            elements: vec![ElementOccurrences {
                element: pos("/a/title.tsx", 3, 10),
                instances: vec![InstanceOccurrences {
                    chain: vec![pos("/a/title.tsx", 3, 10), pos("/a/main.tsx", 8, 4)],
                    ranges: vec![(0, 30), (60, 90)],
                    max_per_frame: 2,
                }],
            }],
        }
    );
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
        r#loop: false,
        video: false,
        pitch: Animatable::Static(0.0),
        preserve_pitch: false,
        fade_in: 0.0,
        fade_out: 0.0,
        duck_group: None,
        duck: None,
    }
}

#[test]
fn react_audio_fades_default_to_zero_and_become_clip_times() {
    let mut descriptor: ReactAudioClipDescriptor = serde_json::from_value(serde_json::json!({
        "src": "./bgm.wav",
        "sourceStart": 0.0,
        "playbackRate": 1.0,
        "volume": 0.6,
        "muted": false,
        "start": 0.0,
        "duration": 10.0,
    }))
    .unwrap();
    assert_eq!((descriptor.fade_in, descriptor.fade_out), (0.0, 0.0));
    assert!(!serde_json::to_string(&descriptor).unwrap().contains("fade"));

    descriptor = serde_json::from_value(serde_json::json!({
        "src": "./bgm.wav",
        "sourceStart": 0.0,
        "playbackRate": 1.0,
        "volume": 0.6,
        "muted": false,
        "start": 0.0,
        "duration": 10.0,
        "fadeIn": 1.5,
        "fadeOut": 3.0,
    }))
    .unwrap();
    let clips = react_audio_clips(&[descriptor], Path::new("/entry")).unwrap();
    assert_eq!(clips[0].fade_in.as_seconds().unwrap(), 1.5);
    assert_eq!(clips[0].fade_out.as_seconds().unwrap(), 3.0);
}

fn ducking_descriptor(duck: serde_json::Value) -> ReactAudioClipDescriptor {
    serde_json::from_value(serde_json::json!({
        "src": "./bgm.wav",
        "sourceStart": 0.0,
        "playbackRate": 1.0,
        "volume": 0.6,
        "muted": false,
        "start": 0.0,
        "duration": 10.0,
        "duckGroup": "music",
        "duck": duck,
    }))
    .unwrap()
}

#[test]
fn react_audio_clips_resolve_ducking() {
    let descriptor = ducking_descriptor(serde_json::json!({
        "under": ["voice", "sfx"],
        "to": {"db": -20},
        "release": 1.0,
    }));
    let clips = react_audio_clips(&[descriptor], Path::new("/entry")).unwrap();
    assert_eq!(clips[0].duck_group.as_deref(), Some("music"));
    let duck = clips[0].duck.as_ref().unwrap();
    assert_eq!(duck.under, ["voice", "sfx"]);
    assert!((duck.to - 0.1).abs() < 1e-12);
    assert_eq!(duck.attack, AudioDuck::DEFAULT_ATTACK);
    assert_eq!(duck.release.as_seconds().unwrap(), 1.0);
    assert_eq!(duck.hold_gap, AudioDuck::DEFAULT_HOLD_GAP);
}

#[test]
fn react_audio_clips_reject_invalid_ducking() {
    let descriptor = ducking_descriptor(serde_json::json!({"under": "voice", "to": 2}));
    let error = react_audio_clips(&[descriptor], Path::new("/entry"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("./bgm.wav")
            && error.contains("duck.to must be a multiplier from 0 to 1, or { db } at or below 0"),
        "{error}"
    );
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
    let built = react_audio_clips(&clips, Path::new("/entry/dir")).unwrap();
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
fn a_video_s_sound_is_reported_with_the_id_of_its_audio_clip() {
    let video = |src: &str, start: f64| ReactAudioClipDescriptor {
        video: true,
        ..audio_descriptor(src, start)
    };
    let clips = vec![
        audio_descriptor("./music.wav", 0.0),
        video("./clip.mp4", 0.0),
        video("./clip.mp4", 0.0),
        // The same file from an <Audio> is a separate, non-video clip.
        audio_descriptor("./clip.mp4", 0.0),
        video("./clip.mp4", 1.0),
    ];
    let built = react_audio_clips(&clips, Path::new("/entry")).unwrap();
    assert_eq!(built.len(), 4);
    assert_eq!(
        react_video_clip_ids(&clips),
        vec!["react-audio:1".to_owned(), "react-audio:3".to_owned()]
    );
    assert_eq!(built[1].asset.id, "./clip.mp4");
    assert_eq!(built[3].range.start.as_seconds().unwrap(), 1.0);
}

#[test]
fn react_audio_clips_carry_pitch() {
    let clips = vec![ReactAudioClipDescriptor {
        pitch: Animatable::Static(5.0),
        preserve_pitch: true,
        ..audio_descriptor("./voice.wav", 0.0)
    }];
    let built = react_audio_clips(&clips, Path::new("/entry")).unwrap();
    assert_eq!(built[0].pitch, Animatable::Static(5.0));
    assert!(built[0].preserve_pitch);
}

#[test]
fn audio_clips_without_pitch_keys_default_to_no_shift_and_round_trip_without_them() {
    let clip: ReactAudioClipDescriptor = serde_json::from_value(serde_json::json!({
        "src": "./voice.wav",
        "sourceStart": 0.0,
        "playbackRate": 1.0,
        "volume": 1.0,
        "muted": false,
        "start": 0.0,
        "duration": 1.0
    }))
    .unwrap();
    assert_eq!(clip.pitch, Animatable::Static(0.0));
    assert!(!clip.preserve_pitch);
    let json = serde_json::to_value(&clip).unwrap();
    assert!(json.get("pitch").is_none());
    assert!(json.get("preservePitch").is_none());

    let shifted: ReactAudioClipDescriptor = serde_json::from_value(serde_json::json!({
        "src": "./voice.wav",
        "sourceStart": 0.0,
        "playbackRate": 1.0,
        "volume": 1.0,
        "muted": false,
        "start": 0.0,
        "duration": 1.0,
        "pitch": -3.0,
        "preservePitch": true
    }))
    .unwrap();
    assert_eq!(shifted.pitch, Animatable::Static(-3.0));
    assert!(shifted.preserve_pitch);
}

#[test]
fn deserializes_the_video_flag_of_an_audio_clip() {
    let clip: ReactAudioClipDescriptor = serde_json::from_value(serde_json::json!({
        "src": "./clip.mp4",
        "sourceStart": 0.0,
        "playbackRate": 1.0,
        "volume": 1.0,
        "muted": false,
        "start": 0.0,
        "duration": 1.0,
        "video": true
    }))
    .unwrap();
    assert!(clip.video);
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

/// A fresh temp directory holding a `film.tsx` entry, plus an optional `props.json`.
fn entry_dir(name: &str, props: Option<&str>) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("celesta-entry-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("film.tsx");
    std::fs::write(&entry, "export default null;").unwrap();
    if let Some(props) = props {
        std::fs::write(dir.join(ENTRY_PROPS_FILE), props).unwrap();
    }
    (dir, entry)
}

#[test]
fn discovers_props_json_beside_the_entry() {
    let (dir, entry) = entry_dir("discover", Some(r#"{"title":"Hello"}"#));
    let inputs = PropertyInputs::load_for_entry(&entry, None, None).unwrap();
    let layers = inputs.layers();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].source, PropertySource::EntryProps);
    assert_eq!(layers[0].base_dir, dir);
    assert_eq!(
        layers[0].file.as_deref(),
        Some(dir.join("props.json").as_path())
    );
    assert_eq!(layers[0].values["title"], "Hello");
    let message = serde_json::to_value(PropertyInputsMessage {
        property_inputs: layers,
    })
    .unwrap();
    assert_eq!(message["propertyInputs"][0]["source"], "entryProps");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn explicit_props_file_replaces_the_entry_props() {
    let (dir, entry) = entry_dir("explicit", Some(r#"{"title":"Auto"}"#));
    let variant = dir.join("variant.json");
    std::fs::write(&variant, r#"{"title":"Variant"}"#).unwrap();
    let inputs = PropertyInputs::load_for_entry(&entry, Some(&variant), None).unwrap();
    let layers = inputs.layers();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].source, PropertySource::PropsFile);
    assert_eq!(layers[0].values["title"], "Variant");
    let choice = resolve_props_file(&entry, Some(&variant)).unwrap();
    assert_eq!(choice.path, variant);
    assert!(!choice.auto);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn inline_props_stay_on_top() {
    let (dir, entry) = entry_dir("inline", Some(r#"{"title":"Auto"}"#));
    let inputs =
        PropertyInputs::load_for_entry(&entry, None, Some(r#"{"title":"Inline"}"#)).unwrap();
    let sources: Vec<_> = inputs.layers().iter().map(|layer| layer.source).collect();
    assert_eq!(sources, [PropertySource::EntryProps, PropertySource::Props]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn no_props_json_adds_no_layer() {
    let (dir, entry) = entry_dir("none", None);
    assert!(resolve_props_file(&entry, None).is_none());
    assert!(
        PropertyInputs::load_for_entry(&entry, None, None)
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn discovery_follows_the_entry_not_the_current_directory() {
    let (dir, entry) = entry_dir("cwd", Some(r#"{"title":"Hello"}"#));
    assert_ne!(std::env::current_dir().unwrap(), dir);
    let choice = resolve_props_file(&entry, None).unwrap();
    assert_eq!(choice.path, dir.join("props.json"));
    assert!(choice.auto);
    assert!(choice.path.is_absolute());
    let inputs = PropertyInputs::load_for_entry(&entry, None, None).unwrap();
    assert_eq!(inputs.layers()[0].source, PropertySource::EntryProps);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn malformed_entry_props_is_an_error() {
    let (dir, entry) = entry_dir("malformed", Some("["));
    let error = PropertyInputs::load_for_entry(&entry, None, None).unwrap_err();
    assert!(matches!(
        &error,
        PropertyInputError::Parse { origin, .. } if origin.starts_with("props.json ")
    ));
    std::fs::write(dir.join(ENTRY_PROPS_FILE), "[1]").unwrap();
    assert!(matches!(
        PropertyInputs::load_for_entry(&entry, None, None),
        Err(PropertyInputError::NotAnObject(origin)) if origin.starts_with("props.json ")
    ));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unreadable_entry_props_names_props_json_not_the_flag() {
    let (dir, entry) = entry_dir("unreadable", None);
    std::fs::write(dir.join(ENTRY_PROPS_FILE), [0xff, 0xfe, 0x00]).unwrap();
    let error = PropertyInputs::load_for_entry(&entry, None, None).unwrap_err();
    assert!(matches!(error, PropertyInputError::Read { .. }));
    assert!(
        error.to_string().starts_with("could not read props.json "),
        "{error}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
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

#[test]
fn measuring_rejects_invalid_font_runs() {
    use crate::measure::{MeasureTextRequest, measure_text_response};
    use celesta_composition::{TextFontRun, TextStyle};
    let request = MeasureTextRequest {
        text: "AB".to_owned(),
        style: TextStyle {
            font_runs: vec![TextFontRun {
                start: 0,
                end: 3,
                font_weight: Some(700),
                ..TextFontRun::default()
            }],
            ..TextStyle::default()
        },
        max_width: None,
        fonts: Vec::new(),
    };
    let response = measure_text_response(&mut None, &request);
    assert!(response.metrics.is_none());
    assert_eq!(
        response.error.as_deref(),
        Some(
            "could not measure text: text font run 0 (0..3) is invalid for 2 code points; runs must be ordered, non-overlapping ranges within the text"
        )
    );
}

fn effective_schema() -> std::collections::BTreeMap<String, ComponentPropertyField> {
    let json = serde_json::json!({
        "title": {"type": "string", "defaultValue": "Celesta"},
        "accent": {"type": "color", "defaultValue": "#ff8800"},
        "format": {"type": "select", "defaultValue": "16:9", "options": ["16:9", "9:16"]}
    });
    serde_json::from_value(json).unwrap()
}

fn layer(source: PropertySource, values: serde_json::Value) -> crate::properties::PropertyLayer {
    crate::properties::PropertyLayer {
        source,
        file: None,
        base_dir: std::path::PathBuf::new(),
        values: values.as_object().unwrap().clone(),
    }
}

#[test]
fn ready_message_keeps_the_declared_property_order() {
    // A literal string: `json!` builds a sorted map unless `preserve_order` is
    // on, which depends on which crates are built together.
    let json = r##"{
        "config": {
            "width": 640,
            "height": 360,
            "frameRate": {"numerator": 30, "denominator": 1},
            "durationInFrames": 30
        },
        "propertySchema": {
            "title": {"type": "string", "defaultValue": "Celesta"},
            "subtitle": {"type": "string", "defaultValue": ""},
            "accent": {"type": "color", "defaultValue": "#ff8800"}
        }
    }"##;
    assert_eq!(
        ready_property_order(json.as_bytes()),
        ["title", "subtitle", "accent"]
    );

    let without = r#"{"config":{},"propertySchema":null}"#;
    assert!(ready_property_order(without.as_bytes()).is_empty());
}

#[test]
fn effective_values_follow_props_then_file_then_default() {
    let inputs = PropertyInputs::from_layers(vec![
        layer(
            PropertySource::PropsFile,
            serde_json::json!({"accent": "#000000"}),
        ),
        layer(PropertySource::Props, serde_json::json!({"format": "9:16"})),
    ]);
    let values = effective_values(&effective_schema(), &inputs);
    assert_eq!(
        values["title"],
        EffectiveValue {
            value: serde_json::json!("Celesta"),
            source: ValueSource::Default
        }
    );
    assert_eq!(
        values["accent"],
        EffectiveValue {
            value: serde_json::json!("#000000"),
            source: ValueSource::File
        }
    );
    assert_eq!(
        values["format"],
        EffectiveValue {
            value: serde_json::json!("9:16"),
            source: ValueSource::Props
        }
    );
}

#[test]
fn entry_props_count_as_the_file_layer() {
    let inputs = PropertyInputs::from_layers(vec![layer(
        PropertySource::EntryProps,
        serde_json::json!({"title": "Hello"}),
    )]);
    let values = effective_values(&effective_schema(), &inputs);
    assert_eq!(values["title"].source, ValueSource::File);
    assert_eq!(values["title"].value, serde_json::json!("Hello"));
}

#[test]
fn default_json_matches_each_field_type() {
    let schema: std::collections::BTreeMap<String, ComponentPropertyField> =
        serde_json::from_value(serde_json::json!({
            "s": {"type": "string", "defaultValue": "a"},
            "n": {"type": "number", "defaultValue": 1.5},
            "b": {"type": "boolean", "defaultValue": true},
            "c": {"type": "color", "defaultValue": "#fff"},
            "o": {"type": "select", "defaultValue": "x", "options": ["x"]},
            "p": {"type": "path", "defaultValue": "./a.png"}
        }))
        .unwrap();
    assert_eq!(schema["s"].default_json(), serde_json::json!("a"));
    assert_eq!(schema["n"].default_json(), serde_json::json!(1.5));
    assert_eq!(schema["b"].default_json(), serde_json::json!(true));
    assert_eq!(schema["c"].default_json(), serde_json::json!("#fff"));
    assert_eq!(schema["o"].default_json(), serde_json::json!("x"));
    assert_eq!(schema["p"].default_json(), serde_json::json!("./a.png"));
}

#[test]
fn open_end_frame_requests_ask_for_and_read_back_the_echo() {
    let request = |open_end| Request {
        time: Some(Time::new(4, 30)),
        project: None,
        runtime: None,
        components: None,
        compact_transforms: true,
        open_end,
        debug_guides: false,
        preview: false,
        statics: None,
        prop_overrides: &[],
    };
    let past = serde_json::to_value(request(true)).unwrap();
    assert_eq!(past["openEnd"], true);
    let plain = serde_json::to_value(request(false)).unwrap();
    assert!(plain.get("openEnd").is_none(), "{plain}");

    let scene = serde_json::json!({
        "width": 64,
        "height": 64,
        "frameRate": {"numerator": 30, "denominator": 1},
        "time": {"value": 4, "timescale": 30},
        "layers": []
    });
    let echoed = serde_json::json!({ "scene": scene, "openEnd": true }).to_string();
    let Response::Ok { open_end, .. } = serde_json::from_str(&echoed).unwrap() else {
        panic!("expected an Ok response");
    };
    assert!(open_end);
    // An older runtime ignores the request and sends no echo.
    let ignored = serde_json::json!({ "scene": scene }).to_string();
    let Response::Ok { open_end, .. } = serde_json::from_str(&ignored).unwrap() else {
        panic!("expected an Ok response");
    };
    assert!(!open_end);
}

#[test]
fn frame_requests_ask_for_debug_guides_only_when_set() {
    let request = |debug_guides| Request {
        time: Some(Time::ZERO),
        project: None,
        runtime: None,
        components: None,
        compact_transforms: true,
        open_end: false,
        debug_guides,
        preview: false,
        statics: None,
        prop_overrides: &[],
    };
    let guides = serde_json::to_value(request(true)).unwrap();
    assert_eq!(guides["debugGuides"], true);
    let plain = serde_json::to_value(request(false)).unwrap();
    assert!(plain.get("debugGuides").is_none(), "{plain}");
}

#[test]
fn frame_requests_ask_for_the_preview_only_when_set() {
    let request = |preview| Request {
        time: Some(Time::ZERO),
        project: None,
        runtime: None,
        components: None,
        compact_transforms: true,
        open_end: false,
        debug_guides: false,
        preview,
        statics: None,
        prop_overrides: &[],
    };
    let preview = serde_json::to_value(request(true)).unwrap();
    assert_eq!(preview["preview"], true);
    // Exports and inspection leave it out, so older runtimes see the same request.
    let plain = serde_json::to_value(request(false)).unwrap();
    assert!(plain.get("preview").is_none(), "{plain}");
}

#[test]
fn frame_requests_carry_prop_overrides_only_when_there_are_some() {
    let request = |prop_overrides| Request {
        time: Some(Time::ZERO),
        project: None,
        runtime: None,
        components: None,
        compact_transforms: true,
        open_end: false,
        debug_guides: false,
        preview: true,
        statics: None,
        prop_overrides,
    };
    let overrides = [PropOverride {
        file: "/project/label.tsx".into(),
        line: 3,
        column: 9,
        prop: "x".to_owned(),
        value: 184.5,
    }];
    let dragged = serde_json::to_value(request(&overrides)).unwrap();
    assert_eq!(
        dragged["propOverrides"],
        serde_json::json!([{
            "file": "/project/label.tsx",
            "line": 3,
            "column": 9,
            "prop": "x",
            "value": 184.5,
        }])
    );
    let plain = serde_json::to_value(request(&[])).unwrap();
    assert!(plain.get("propOverrides").is_none(), "{plain}");
}

mod statics {
    use crate::error::ReactBridgeError;
    use crate::protocol::{Request, StaticsRequest};
    use crate::statics::{StaticCache, StaticDefinition};
    use celesta_composition::{
        EvaluatedTransform, GroupMask, Layer, LayerContent, MaskMode, Rational, Scene, Time,
    };
    fn definition(key: u32, layers: Vec<Layer>) -> StaticDefinition {
        StaticDefinition { key, layers }
    }

    fn layer(id: &str, content: LayerContent) -> Layer {
        Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform::default(),
            opacity: 1.0,
            blend_mode: Default::default(),
            effects: Default::default(),
            content,
        }
    }

    fn rect(id: &str) -> Layer {
        layer(
            id,
            LayerContent::Rect {
                width: 1.0,
                height: 2.0,
                fill: None,
                stroke: None,
                corner_radius: 0.0,
            },
        )
    }

    fn group(id: &str, layers: Vec<Layer>, mask: Option<GroupMask>) -> Layer {
        layer(
            id,
            LayerContent::Group {
                layers,
                clip: None,
                mask,
                motion_blur: false,
            },
        )
    }

    fn reference(id: &str, key: u32) -> Layer {
        layer(id, LayerContent::StaticRef { key })
    }

    fn scene(layers: Vec<Layer>) -> Scene {
        Scene {
            width: 4,
            height: 4,
            frame_rate: Rational::new(30, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            shaders: Vec::new(),
            layers,
        }
    }

    fn expanded(id: &str, layers: Vec<Layer>) -> Layer {
        group(id, layers, None)
    }

    #[test]
    fn expands_static_refs_at_every_depth() {
        let mut cache = StaticCache::default();
        let mut frame = scene(vec![
            reference("top", 1),
            group("outer", vec![reference("inner", 1)], None),
            group(
                "masked",
                Vec::new(),
                Some(GroupMask {
                    layers: vec![reference("matte", 1)],
                    mode: MaskMode::Alpha,
                    invert: false,
                }),
            ),
        ]);
        cache
            .apply(
                Some(4),
                vec![definition(1, vec![rect("r")])],
                Vec::new(),
                &mut frame,
            )
            .unwrap();
        assert_eq!(frame.layers[0], expanded("top", vec![rect("r")]));
        let LayerContent::Group { layers, .. } = &frame.layers[1].content else {
            panic!("not a group");
        };
        assert_eq!(layers[0], expanded("inner", vec![rect("r")]));
        let LayerContent::Group {
            mask: Some(mask), ..
        } = &frame.layers[2].content
        else {
            panic!("no mask");
        };
        assert_eq!(mask.layers[0], expanded("matte", vec![rect("r")]));
        assert_eq!(cache.decoded(), 4);
    }

    #[test]
    fn keeps_definitions_between_frames() {
        let mut cache = StaticCache::default();
        let mut first = scene(vec![reference("s", 1)]);
        cache
            .apply(
                Some(1),
                vec![definition(1, vec![rect("r")])],
                Vec::new(),
                &mut first,
            )
            .unwrap();
        let mut second = scene(vec![reference("s", 1)]);
        cache
            .apply(Some(2), Vec::new(), Vec::new(), &mut second)
            .unwrap();
        assert_eq!(second.layers[0], expanded("s", vec![rect("r")]));
        assert_eq!(cache.decoded(), 2);
    }

    #[test]
    fn drops_keys_after_expanding() {
        let mut cache = StaticCache::default();
        let mut first = scene(vec![reference("s", 1)]);
        cache
            .apply(
                Some(1),
                vec![definition(1, vec![rect("r")])],
                vec![1],
                &mut first,
            )
            .unwrap();
        assert_eq!(first.layers[0], expanded("s", vec![rect("r")]));
        let mut second = scene(vec![reference("s", 1)]);
        let error = cache
            .apply(Some(2), Vec::new(), Vec::new(), &mut second)
            .unwrap_err();
        assert!(
            matches!(error, ReactBridgeError::MissingStatic(1)),
            "{error}"
        );
    }

    #[test]
    fn a_missing_key_resets_the_cache() {
        let mut cache = StaticCache::default();
        let mut first = scene(vec![reference("s", 1)]);
        cache
            .apply(
                Some(5),
                vec![definition(1, vec![rect("r")])],
                Vec::new(),
                &mut first,
            )
            .unwrap();
        let mut broken = scene(vec![reference("t", 2)]);
        assert!(
            cache
                .apply(Some(6), Vec::new(), Vec::new(), &mut broken)
                .is_err()
        );
        assert_eq!(cache.decoded(), 0);
        let mut again = scene(vec![reference("s", 1)]);
        assert!(matches!(
            cache.apply(Some(7), Vec::new(), Vec::new(), &mut again),
            Err(ReactBridgeError::MissingStatic(1))
        ));
    }

    #[test]
    fn frame_requests_report_what_was_decoded() {
        let request = |statics| Request {
            time: Some(Time::ZERO),
            project: None,
            runtime: None,
            components: None,
            compact_transforms: true,
            open_end: false,
            debug_guides: false,
            preview: false,
            statics,
            prop_overrides: &[],
        };
        let json = serde_json::to_string(&request(Some(StaticsRequest { decoded: 3 }))).unwrap();
        assert!(json.contains(r#""statics":{"decoded":3}"#), "{json}");
        let json = serde_json::to_string(&request(None)).unwrap();
        assert!(!json.contains("statics"), "{json}");
    }
}
