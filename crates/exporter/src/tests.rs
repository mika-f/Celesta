use crate::DEFAULT_REACT_AUDIO_SAMPLE_RATE;
use crate::error::ExportError;
use crate::options::{ColorConversion, EncoderPreset, UnknownEncoderPreset, VideoEncoding};
use crate::project::absolutize_layers;
use crate::project::{build_audio_graph, merge_react_audio_clips, visual_only_project};
use crate::range::{ExportRange, resolve_window, shifted_audio_graph};
use crate::render::{frame_count, open_video_writer, validate_dimensions, validate_output};
use crate::timecode::{TimecodeError, parse_timecode};
use celesta_composition::{
    AssetLocation, EvaluatedTransform, GroupMask, Layer, LayerContent, MaskMode, Rational,
    ResolvedAsset, Time,
};
use celesta_evaluator::Evaluator;
use celesta_gpu_renderer::ReadbackFormat;
use celesta_project::Project;
use celesta_react_bridge::ReactAudioClipDescriptor;
use std::fs;
use std::path::Path;

use celesta_composition::{Animatable, Keyframe, KeyframeAnimation, KeyframeAnimationType};

fn static_clip(src: &str, start: f64, duration: f64, volume: f64) -> ReactAudioClipDescriptor {
    ReactAudioClipDescriptor {
        src: src.to_owned(),
        source_start: 0.0,
        playback_rate: Animatable::Static(1.0),
        volume: Animatable::Static(volume),
        muted: false,
        start,
        duration,
    }
}

#[test]
fn merges_identical_frame_reports_while_preserving_multiplicity() {
    let clips = vec![
        static_clip("./a.wav", 0.0, 5.0, 1.0),
        static_clip("./b.wav", 1.0, 2.0, 0.5),
        static_clip("./a.wav", 0.0, 5.0, 1.0),
        static_clip("./a.wav", 1.0, 4.0, 1.0),
    ];

    let merged = merge_react_audio_clips(&clips);
    assert_eq!(merged.len(), 3);
    assert_eq!(merged[0].1, 2, "the same clip reported twice merges");
    assert_eq!(merged[1].1, 1);
    assert_eq!(merged[2].1, 1);
    // A different window or volume is a different clip even for the
    // same source file.
    assert_eq!(merged[0].0.src, "./a.wav");
    assert_eq!(merged[1].0.volume, Animatable::Static(0.5));
    assert_eq!(merged[2].0.start, 1.0);
}

#[test]
fn builds_a_react_audio_graph_with_ranges_and_keyframed_volume() {
    let clips = vec![ReactAudioClipDescriptor {
        src: "./voice.wav".to_owned(),
        source_start: 1.5,
        playback_rate: Animatable::Static(2.0),
        volume: Animatable::Keyframes(KeyframeAnimation {
            kind: KeyframeAnimationType::Keyframes,
            keyframes: vec![Keyframe {
                time: Time::new(500_000, 1_000_000),
                value: 0.25,
                easing: None,
            }],
        }),
        muted: false,
        start: 1.0,
        duration: 2.0,
    }];

    let graph = build_audio_graph(&clips, Path::new("/entry/root"), None).unwrap();
    assert_eq!(graph.sample_rate, DEFAULT_REACT_AUDIO_SAMPLE_RATE);
    assert_eq!(graph.clips.len(), 1);
    let clip = &graph.clips[0];
    assert_eq!(
        clip.range.start.as_seconds().unwrap(),
        1.0,
        "range.start is the composition-space audible start"
    );
    assert_eq!(clip.range.duration.as_seconds().unwrap(), 2.0);
    assert_eq!(
        clip.source_start.as_seconds().unwrap(),
        1.5,
        "source_start carries the head-clipping adjustment"
    );
    assert_eq!(clip.playback_rate, Animatable::Static(2.0));
    assert!(matches!(&clip.volume, Animatable::Keyframes(_)));
    let AssetLocation::File { path } = &clip.asset.location else {
        panic!("expected a file asset");
    };
    assert_eq!(
        path,
        Path::new("/entry/root/./voice.wav"),
        "relative sources resolve against the entry's own directory"
    );
}

#[test]
fn parses_timecodes_of_every_length() {
    assert_eq!(parse_timecode("5").unwrap(), Time::new(5_000, 1_000));
    assert_eq!(parse_timecode("1.5").unwrap(), Time::new(1_500, 1_000));
    assert_eq!(parse_timecode("0:02").unwrap(), Time::new(2_000, 1_000));
    assert_eq!(parse_timecode("01:30").unwrap(), Time::new(90_000, 1_000));
    assert_eq!(
        parse_timecode("01:02:03.250").unwrap(),
        Time::new(3_723_250, 1_000)
    );
    assert_eq!(
        parse_timecode(" 00:00:01 ").unwrap(),
        Time::new(1_000, 1_000)
    );
    assert_eq!(parse_timecode(""), Err(TimecodeError::Empty));
    assert!(matches!(
        parse_timecode("1:2:3:4"),
        Err(TimecodeError::Malformed(_))
    ));
    assert!(matches!(
        parse_timecode("-5"),
        Err(TimecodeError::Malformed(_))
    ));
    assert!(matches!(
        parse_timecode("00:00:01.2500"),
        Err(TimecodeError::Malformed(_))
    ));
}

#[test]
fn encoder_presets_and_color_conversions_round_trip_through_their_names() {
    for preset in EncoderPreset::ALL {
        assert_eq!(preset.as_str().parse::<EncoderPreset>(), Ok(preset));
    }
    assert_eq!(VideoEncoding::default().preset, EncoderPreset::Medium);
    assert_eq!(VideoEncoding::default().crf, 18);
    for conversion in ColorConversion::ALL {
        assert_eq!(
            conversion.as_str().parse::<ColorConversion>(),
            Ok(conversion)
        );
    }
    assert_eq!(
        VideoEncoding::default().color_conversion,
        ColorConversion::Auto
    );
    assert_eq!(
        "Medium".parse::<EncoderPreset>(),
        Err(UnknownEncoderPreset("Medium".to_owned()))
    );
}

#[test]
fn rejects_an_out_of_range_crf_before_encoding() {
    let directory = tempfile::tempdir().unwrap();
    let result = open_video_writer(
        64,
        64,
        Rational::new(30, 1),
        VideoEncoding {
            crf: VideoEncoding::MAX_CRF + 1,
            ..VideoEncoding::default()
        },
        ReadbackFormat::Rgba8,
        &directory.path().join("video.mp4"),
    );
    assert!(matches!(result, Err(ExportError::InvalidCrf(52))));
}

#[test]
fn resolve_window_clamps_and_snaps_to_frames() {
    let rate = Rational::new(30, 1);
    let duration = Time::new(10, 1);

    // A mid-composition span, start snapped down to the frame boundary.
    let window = resolve_window(
        &ExportRange::new(Time::new(1001, 1000), Time::new(3, 1)),
        duration,
        rate,
    )
    .unwrap();
    assert_eq!(window.start_frame, 30);
    assert_eq!(window.start, Time::frames(30, rate).unwrap());
    assert_eq!(window.frames, 60);

    // `end` past the composition is clamped; `None` means "to the end".
    let full_tail = resolve_window(&ExportRange::from(Time::new(9, 1)), duration, rate).unwrap();
    assert_eq!(full_tail.start_frame, 270);
    assert_eq!(full_tail.frames, 30);

    // A zero-length span is rejected.
    assert!(matches!(
        resolve_window(
            &ExportRange::new(Time::new(5, 1), Time::new(5, 1)),
            duration,
            rate
        ),
        Err(ExportError::EmptyRange)
    ));
}

#[test]
fn shifted_audio_graph_moves_clip_starts_earlier() {
    let graph = build_audio_graph(
        &[static_clip("./a.wav", 5.0, 2.0, 1.0)],
        Path::new("/root"),
        None,
    )
    .unwrap();
    let shifted = shifted_audio_graph(&graph, Time::new(3, 1)).unwrap();
    assert_eq!(shifted.clips[0].range.start.as_seconds().unwrap(), 2.0);

    // A clip that began before the window gets a negative start so the
    // mixer keeps advancing it from the right source position.
    let before = shifted_audio_graph(&graph, Time::new(7, 1)).unwrap();
    assert_eq!(before.clips[0].range.start.as_seconds().unwrap(), -2.0);
}

#[test]
fn rounds_partial_project_frames_up_exactly() {
    assert_eq!(
        frame_count(Time::new(1001, 1000), Rational::new(30_000, 1001)).unwrap(),
        30
    );
    assert_eq!(
        frame_count(Time::new(1002, 1000), Rational::new(30_000, 1001)).unwrap(),
        31
    );
}

#[test]
fn visual_only_project_keeps_dialogue_and_component_but_drops_audio() {
    const WITH_DIALOGUE: &str = r#"{
        "version": 0,
        "settings": {
            "width": 640,
            "height": 360,
            "frameRate": {"numerator": 30, "denominator": 1},
            "sampleRate": 48000
        },
        "assets": {
            "akane-default": {"type": "image", "name": "Akane", "source": {"type": "file", "path": "./portrait.png"}},
            "voice-001": {"type": "audio", "source": {"type": "file", "path": "./voice.wav"}}
        },
        "characters": {
            "akane": {
                "name": "Akane",
                "portrait": {
                    "defaultExpression": "default",
                    "expressions": {"default": "akane-default"}
                },
                "subtitle": {}
            }
        },
        "tracks": [
            {
                "id": "dialogue",
                "name": "Dialogue",
                "kind": "dialogue",
                "items": [
                    {
                        "id": "line-1",
                        "range": {"start": {"value": 0, "timescale": 1}, "duration": {"value": 1, "timescale": 1}},
                        "content": {"type": "dialogue", "character": "akane", "text": "Hello", "audio": "voice-001"}
                    },
                    {
                        "id": "voice-only",
                        "range": {"start": {"value": 0, "timescale": 1}, "duration": {"value": 1, "timescale": 1}},
                        "content": {"type": "audio", "asset": "voice-001"}
                    }
                ]
            }
        ],
        "properties": {}
    }"#;
    let project = Project::from_json(WITH_DIALOGUE).unwrap();

    let filtered = visual_only_project(&project);
    let item_ids: Vec<&str> = filtered.tracks[0]
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(item_ids, vec!["line-1"]);

    let evaluator = Evaluator::new(&filtered).unwrap();
    let scene = evaluator.scene_at(Time::ZERO).unwrap();
    assert_eq!(scene.layers.len(), 1);
    let LayerContent::Group { layers, .. } = &scene.layers[0].content else {
        panic!("dialogue should evaluate to a group of portrait + subtitle layers");
    };
    assert!(layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Image { asset, .. } if asset.id == "akane-default"
    )));
    assert!(layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "Hello"
    )));
}

#[test]
fn refuses_non_mp4_and_existing_outputs() {
    let directory = tempfile::tempdir().unwrap();
    let existing = directory.path().join("existing.mp4");
    fs::write(&existing, []).unwrap();
    assert!(matches!(
        validate_output(&existing, false),
        Err(ExportError::OutputExists(_))
    ));
    assert!(matches!(
        validate_output(&directory.path().join("video.mov"), false),
        Err(ExportError::UnsupportedOutput(_))
    ));
    assert!(matches!(
        validate_dimensions(63, 64),
        Err(ExportError::UnsupportedDimensions {
            width: 63,
            height: 64
        })
    ));
}

#[test]
fn absolutizes_assets_inside_a_groups_mask() {
    let image = |path: &str| Layer {
        id: path.to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: Default::default(),
        effects: Default::default(),
        content: LayerContent::Image {
            asset: ResolvedAsset {
                id: path.to_owned(),
                location: AssetLocation::File {
                    path: path.to_owned(),
                },
            },
            width: None,
            height: None,
            fit: None,
        },
    };
    let mut layers = vec![Layer {
        content: LayerContent::Group {
            layers: vec![image("child.png")],
            clip: None,
            mask: Some(GroupMask {
                layers: vec![image("matte.png")],
                mode: MaskMode::Alpha,
                invert: false,
            }),
        },
        ..image("group")
    }];
    let root = Path::new("/project");
    absolutize_layers(&mut layers, root);
    let LayerContent::Group {
        layers: children,
        mask: Some(mask),
        ..
    } = &layers[0].content
    else {
        panic!("the group lost its mask");
    };
    for (layer, name) in [(&children[0], "child.png"), (&mask.layers[0], "matte.png")] {
        let LayerContent::Image { asset, .. } = &layer.content else {
            panic!("not an image");
        };
        assert_eq!(
            asset.location,
            AssetLocation::File {
                path: root.join(name).to_string_lossy().into_owned()
            }
        );
    }
}
