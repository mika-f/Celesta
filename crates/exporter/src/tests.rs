use crate::DEFAULT_REACT_AUDIO_SAMPLE_RATE;
use crate::error::ExportError;
use crate::options::{
    BitDepth, ColorConversion, EncoderPreset, UnknownBitDepth, UnknownEncoderPreset, VideoEncoding,
    VideoFormat,
};
use crate::project::absolutize_layers;
use crate::project::{build_audio_graph, merge_react_audio_clips};
use crate::range::{ExportRange, resolve_window, shifted_audio_graph};
use crate::render::{
    LOOP_LEAD_IN_FRAMES, ensure_encoders, frame_count, loop_lead_in, open_video_writer,
    validate_dimensions, validate_output,
};
use crate::timecode::{TimecodeError, parse_timecode};
use celesta_composition::{
    AssetLocation, EvaluatedTransform, F32Column, GroupMask, InstanceShape, Layer, LayerContent,
    MaskMode, Rational, ResolvedAsset, Time,
};
use celesta_composition::{AudioDuck, DuckGroups, DuckLevel};
use celesta_evaluator::Evaluator;
use celesta_gpu_renderer::ReadbackFormat;
use celesta_project::Project;
use celesta_react_bridge::{ReactAudioClipDescriptor, ReactDuck};
use std::fs;
use std::path::Path;

use celesta_composition::{Animatable, Keyframe, KeyframeAnimation, KeyframeAnimationType};
use strum::VariantArray;

fn static_clip(src: &str, start: f64, duration: f64, volume: f64) -> ReactAudioClipDescriptor {
    ReactAudioClipDescriptor {
        src: src.to_owned(),
        source_start: 0.0,
        playback_rate: Animatable::Static(1.0),
        volume: Animatable::Static(volume),
        muted: false,
        start,
        duration,
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
        r#loop: false,
        video: false,
        pitch: Animatable::Static(0.0),
        preserve_pitch: false,
        fade_in: 0.25,
        fade_out: 0.5,
        duck_group: Some("music".to_owned()),
        duck: Some(ReactDuck {
            under: DuckGroups::One("voice".to_owned()),
            to: Some(DuckLevel::Gain(0.5)),
            attack: None,
            release: None,
            hold_gap: None,
        }),
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
    assert_eq!(clip.fade_in.as_seconds().unwrap(), 0.25);
    assert_eq!(clip.fade_out.as_seconds().unwrap(), 0.5);
    assert_eq!(clip.duck_group.as_deref(), Some("music"));
    let duck = clip.duck.as_ref().unwrap();
    assert_eq!(
        (duck.under.as_slice(), duck.to),
        (["voice".to_owned()].as_slice(), 0.5)
    );
    assert_eq!(duck.attack, AudioDuck::DEFAULT_ATTACK);
}

#[test]
fn builds_a_react_audio_graph_with_pitch() {
    let clips = vec![ReactAudioClipDescriptor {
        pitch: Animatable::Static(7.0),
        preserve_pitch: true,
        ..static_clip("./voice.wav", 0.0, 1.0, 1.0)
    }];
    let graph = build_audio_graph(&clips, Path::new("/entry/root"), None).unwrap();
    assert_eq!(graph.clips[0].pitch, Animatable::Static(7.0));
    assert!(graph.clips[0].preserve_pitch);
}

#[test]
fn clips_differing_only_in_pitch_are_not_merged() {
    let shifted = ReactAudioClipDescriptor {
        pitch: Animatable::Static(2.0),
        ..static_clip("./a.wav", 0.0, 1.0, 1.0)
    };
    let preserved = ReactAudioClipDescriptor {
        preserve_pitch: true,
        ..static_clip("./a.wav", 0.0, 1.0, 1.0)
    };
    let clips = vec![static_clip("./a.wav", 0.0, 1.0, 1.0), shifted, preserved];
    let merged = merge_react_audio_clips(&clips);
    assert_eq!(merged.len(), 3);
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
    for &preset in EncoderPreset::VARIANTS {
        assert_eq!(preset.as_str().parse::<EncoderPreset>(), Ok(preset));
    }
    assert_eq!(VideoEncoding::default().preset, EncoderPreset::Medium);
    assert_eq!(VideoEncoding::default().crf, 18);
    for &conversion in ColorConversion::VARIANTS {
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
    assert_eq!(
        UnknownEncoderPreset("Medium".to_owned()).to_string(),
        "unknown encoder preset 'Medium' (expected one of ultrafast superfast veryfast \
         faster fast medium slow slower veryslow)"
    );
    assert_eq!(
        "cpu".parse::<ColorConversion>().unwrap_err().to_string(),
        "unknown color conversion 'cpu' (expected auto, gpu, or encoder)"
    );
    for &depth in BitDepth::VARIANTS {
        assert_eq!(depth.as_str().parse::<BitDepth>(), Ok(depth));
    }
    assert_eq!(VideoEncoding::default().bit_depth, BitDepth::Eight);
    assert_eq!(BitDepth::Ten.pixel_format(), "yuv420p10le");
    assert_eq!(
        "12".parse::<BitDepth>(),
        Err(UnknownBitDepth("12".to_owned()))
    );
    assert_eq!(
        UnknownBitDepth("12".to_owned()).to_string(),
        "unknown bit depth '12' (expected 8 or 10)"
    );
}

#[test]
fn rejects_an_out_of_range_crf_before_encoding() {
    let directory = tempfile::tempdir().unwrap();
    let result = open_video_writer(
        64,
        64,
        Rational::new(30, 1),
        VideoFormat::Mp4,
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
fn companion_project_visuals_keep_dialogue_but_skip_audio() {
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

    let evaluator = Evaluator::new(&project).unwrap();
    let scene = evaluator.scene_at(Time::ZERO).unwrap();
    assert_eq!(scene.layers.len(), 1);
    assert_eq!(scene.layers[0].id, "line-1");
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
fn refuses_unmatched_and_existing_outputs() {
    let directory = tempfile::tempdir().unwrap();
    let existing = directory.path().join("existing.mp4");
    fs::write(&existing, []).unwrap();
    assert!(matches!(
        validate_output(&existing, VideoFormat::Mp4, false),
        Err(ExportError::OutputExists(_))
    ));
    assert!(validate_output(&existing, VideoFormat::Mp4, true).is_ok());
    for name in ["video.mov", "VIDEO.MOV"] {
        assert!(validate_output(&directory.path().join(name), VideoFormat::Mov, false).is_ok());
    }
    assert!(matches!(
        validate_output(&directory.path().join("video.mov"), VideoFormat::Mp4, false),
        Err(ExportError::UnsupportedOutput {
            format: VideoFormat::Mp4,
            ..
        })
    ));
    assert!(matches!(
        validate_output(
            &directory.path().join("video.webm"),
            VideoFormat::Mov,
            false
        ),
        Err(ExportError::UnsupportedOutput { .. })
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
            animation: None,
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
            motion_blur: false,
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

#[test]
fn absolutizes_an_instance_image() {
    let mut layers = vec![Layer {
        id: "sparks".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: Default::default(),
        effects: Default::default(),
        content: LayerContent::Instances {
            count: 0,
            shape: Box::new(InstanceShape::Image {
                asset: ResolvedAsset {
                    id: "spark.png".to_owned(),
                    location: AssetLocation::File {
                        path: "spark.png".to_owned(),
                    },
                },
            }),
            width: None,
            height: None,
            color: None,
            x: F32Column::from(Vec::new()),
            y: F32Column::from(Vec::new()),
            widths: None,
            heights: None,
            rotation: None,
            colors: None,
        },
    }];
    let root = Path::new("/project");
    absolutize_layers(&mut layers, root);
    let LayerContent::Instances { shape, .. } = &layers[0].content else {
        panic!("not an instance layer");
    };
    let InstanceShape::Image { asset } = shape.as_ref() else {
        panic!("not an image instance layer");
    };
    assert_eq!(
        asset.location,
        AssetLocation::File {
            path: root.join("spark.png").to_string_lossy().into_owned()
        }
    );
}

#[test]
fn video_formats_follow_the_extension() {
    assert_eq!(
        VideoFormat::from_extension(Path::new("a.mp4")),
        Some(VideoFormat::Mp4)
    );
    assert_eq!(
        VideoFormat::from_extension(Path::new("a.MOV")),
        Some(VideoFormat::Mov)
    );
    assert_eq!(
        VideoFormat::from_extension(Path::new("a.WebM")),
        Some(VideoFormat::Webm)
    );
    assert_eq!(VideoFormat::from_extension(Path::new("a.gif")), None);
    assert!(
        !VideoFormat::Mp4.has_alpha()
            && VideoFormat::Mov.has_alpha()
            && VideoFormat::Webm.has_alpha()
    );
}

#[test]
fn video_format_names_round_trip() {
    for &format in VideoFormat::VARIANTS {
        assert_eq!(format.as_str().parse::<VideoFormat>(), Ok(format));
        assert_eq!(format.to_string(), format.as_str());
    }
    assert_eq!(
        "gif".parse::<VideoFormat>().unwrap_err().to_string(),
        "unknown video format 'gif' (expected one of mp4 mov webm)"
    );
}

#[test]
fn linked_ffmpeg_has_every_format_encoder() {
    for &format in VideoFormat::VARIANTS {
        ensure_encoders(format).unwrap_or_else(|error| panic!("{format}: {error}"));
    }
}

#[test]
fn a_loop_lead_in_is_the_end_of_the_mix() {
    let mix = |frames: usize| celesta_media::AudioBuffer {
        sample_rate: 48_000,
        channels: 2,
        samples: (0..frames)
            .flat_map(|frame| [frame as f32, -(frame as f32)])
            .collect(),
    };
    let long = loop_lead_in(&mix(LOOP_LEAD_IN_FRAMES + 3));
    assert_eq!(long.len(), 2 * LOOP_LEAD_IN_FRAMES);
    assert_eq!(long[..2], [3.0, -3.0]);
    let last = LOOP_LEAD_IN_FRAMES as f32 + 2.0;
    assert_eq!(long[long.len() - 2..], [last, -last]);
    // A mix shorter than the lead-in repeats, still ending on its last frame.
    let short = loop_lead_in(&mix(3));
    assert_eq!(short.len(), 2 * LOOP_LEAD_IN_FRAMES);
    assert_eq!(short[short.len() - 6..], [0.0, -0.0, 1.0, -1.0, 2.0, -2.0]);
    assert!(loop_lead_in(&mix(0)).is_empty());
}
