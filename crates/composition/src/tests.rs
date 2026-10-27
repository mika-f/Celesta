use crate::time::{Rational, Time};
use crate::{
    Animatable, AssetLocation, AudioClip, AudioDuck, BackfaceVisibility, Clip, Color, DuckError,
    DuckGroups, DuckLevel, EvaluatedTransform, F32Column, GroupMask, IDENTITY3, ImageFit,
    InstanceShape, Layer, LayerContent, LetterSpacing, LineCap, LineDash, LineJoin, MaskMode,
    Matrix3, MediaTiming, Paint, PathCommand, Point, ResolvedAsset, Stroke, TextStyle, U8Column,
    check_duck_group, duck_curve, duck_curves, faces_away, multiply3, multiply3_flat, project,
    stretch_at,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::cmp::Ordering;
use std::fmt::Debug;

#[test]
fn compares_times_without_floating_point() {
    let one_second = Time::new(48_000, 48_000);
    let sixty_frames = Time::frames(60, Rational::new(60, 1)).unwrap();
    assert_eq!(one_second.cmp_exact(sixty_frames), Ok(Ordering::Equal));
}

#[test]
fn adds_and_reduces_times() {
    let sum = Time::new(1, 2).checked_add(Time::new(1, 3)).unwrap();
    assert_eq!(sum, Time::new(5, 6));
}

#[test]
fn a_group_without_a_clip_or_mask_serializes_and_parses_as_before() {
    let content = LayerContent::Group {
        layers: Vec::new(),
        clip: None,
        mask: None,
        motion_blur: false,
    };
    let json = serde_json::to_value(&content).unwrap();
    assert_eq!(json, serde_json::json!({ "type": "group", "layers": [] }));
    assert_eq!(
        serde_json::from_value::<LayerContent>(json).unwrap(),
        content
    );
}

#[test]
fn a_groups_mask_round_trips_and_omits_its_defaults() {
    let json = serde_json::json!({
        "type": "group",
        "layers": [],
        "mask": { "layers": [] },
    });
    let content = serde_json::from_value::<LayerContent>(json.clone()).unwrap();
    let LayerContent::Group {
        mask: Some(mask), ..
    } = &content
    else {
        panic!("the mask was dropped: {content:?}");
    };
    assert_eq!(mask.mode, MaskMode::Alpha);
    assert!(!mask.invert);
    assert_eq!(serde_json::to_value(&content).unwrap(), json);

    let luminance = LayerContent::Group {
        layers: Vec::new(),
        clip: None,
        mask: Some(GroupMask {
            layers: Vec::new(),
            mode: MaskMode::Luminance,
            invert: true,
        }),
        motion_blur: false,
    };
    let json = serde_json::to_value(&luminance).unwrap();
    assert_eq!(
        json["mask"],
        serde_json::json!({ "layers": [], "mode": "luminance", "invert": true })
    );
    assert_eq!(
        serde_json::from_value::<LayerContent>(json).unwrap(),
        luminance
    );
}

#[test]
fn a_groups_clip_round_trips_and_defaults_its_corner_radius() {
    let json = serde_json::json!({
        "type": "group",
        "layers": [],
        "clip": { "x": 10.0, "y": 20.0, "width": 300.0, "height": 40.0 },
    });
    let content = serde_json::from_value::<LayerContent>(json).unwrap();
    let LayerContent::Group {
        clip: Some(clip), ..
    } = &content
    else {
        panic!("the clip was dropped: {content:?}");
    };
    assert_eq!(clip.corner_radius, 0.0);

    let rounded = Clip {
        corner_radius: 8.0,
        ..*clip
    };
    let json = serde_json::to_value(LayerContent::Group {
        layers: Vec::new(),
        clip: Some(rounded),
        mask: None,
        motion_blur: false,
    })
    .unwrap();
    assert_eq!(
        json["clip"],
        serde_json::json!({
            "x": 10.0, "y": 20.0, "width": 300.0, "height": 40.0, "cornerRadius": 8.0,
        })
    );
}

#[test]
fn a_video_size_round_trips() {
    let video = LayerContent::Video {
        asset: ResolvedAsset {
            id: "clip".to_owned(),
            location: AssetLocation::File {
                path: "clip.mp4".to_owned(),
            },
        },
        timing: MediaTiming {
            local_time: Time::new(1, 2),
            source_start: Time::ZERO,
            source_time_seconds: 0.5,
            playback_rate: 1.0,
            r#loop: false,
        },
        width: Some(160.0),
        height: None,
    };
    let mut json = serde_json::to_value(&video).unwrap();
    assert_eq!(json["width"], serde_json::json!(160.0));
    assert!(json.get("height").is_none(), "{json}");
    assert_eq!(
        serde_json::from_value::<LayerContent>(json.clone()).unwrap(),
        video
    );

    json.as_object_mut().unwrap().remove("width");
    let LayerContent::Video { width, height, .. } =
        serde_json::from_value::<LayerContent>(json).unwrap()
    else {
        panic!("not a video");
    };
    assert_eq!((width, height), (None, None));
}

#[test]
fn a_motion_blurred_group_round_trips_and_is_omitted_when_off() {
    let json = serde_json::json!({ "type": "group", "layers": [], "motionBlur": true });
    let content = serde_json::from_value::<LayerContent>(json.clone()).unwrap();
    assert!(matches!(
        content,
        LayerContent::Group {
            motion_blur: true,
            ..
        }
    ));
    assert_eq!(serde_json::to_value(&content).unwrap(), json);

    let off = serde_json::json!({ "type": "group", "layers": [], "motionBlur": false });
    let content = serde_json::from_value::<LayerContent>(off).unwrap();
    assert_eq!(
        serde_json::to_value(&content).unwrap(),
        serde_json::json!({ "type": "group", "layers": [] })
    );
}

#[test]
fn a_clip_is_empty_without_area_or_with_a_non_finite_value() {
    let clip = Clip {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        corner_radius: 0.0,
    };
    assert!(!clip.is_empty());
    assert!(Clip { width: 0.0, ..clip }.is_empty());
    assert!(
        Clip {
            height: -1.0,
            ..clip
        }
        .is_empty()
    );
    assert!(
        Clip {
            x: f64::NAN,
            ..clip
        }
        .is_empty()
    );
    assert!(
        Clip {
            width: f64::INFINITY,
            ..clip
        }
        .is_empty()
    );
}

#[test]
fn a_clips_corner_radius_stays_within_half_the_shorter_side() {
    let clip = Clip {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 20.0,
        corner_radius: 50.0,
    };
    assert_eq!(clip.effective_corner_radius(), 10.0);
    assert_eq!(
        Clip {
            corner_radius: -4.0,
            ..clip
        }
        .effective_corner_radius(),
        0.0
    );
    assert_eq!(
        Clip {
            corner_radius: f64::NAN,
            ..clip
        }
        .effective_corner_radius(),
        0.0
    );
}

#[test]
fn tagged_content_parses_with_its_type_first_or_later() {
    let first = r##"{"type":"rect","width":2,"height":3,"fill":{"type":"solid","color":"#fff"}}"##;
    let later = r##"{"width":2,"fill":{"color":"#fff","type":"solid"},"type":"rect","height":3}"##;
    let expected = LayerContent::Rect {
        width: 2.0,
        height: 3.0,
        fill: Some(Paint::Solid {
            color: "#fff".to_owned(),
        }),
        stroke: None,
        corner_radius: 0.0,
    };
    assert_eq!(
        serde_json::from_str::<LayerContent>(first).unwrap(),
        expected
    );
    assert_eq!(
        serde_json::from_str::<LayerContent>(later).unwrap(),
        expected
    );
}

#[test]
fn tagged_content_fills_omitted_fields_with_their_defaults() {
    let content = serde_json::from_str::<LayerContent>(
        r#"{"type":"path","commands":[{"type":"close","x":1}]}"#,
    )
    .unwrap();
    let LayerContent::Path {
        commands,
        fill,
        line_cap,
        miter_limit,
        line_dash,
        ..
    } = content
    else {
        panic!("expected a path, got {content:?}");
    };
    assert_eq!(commands, [PathCommand::Close]);
    assert_eq!(fill, None);
    assert_eq!(line_cap, LineCap::Butt);
    assert_eq!(miter_limit, crate::DEFAULT_MITER_LIMIT);
    assert_eq!(line_dash, None);
}

#[test]
fn path_dash_offset_defaults_to_zero_and_lengths_cannot_be_negative() {
    let content = serde_json::from_str::<LayerContent>(
        r#"{"type":"path","commands":[],"lineDash":{"pattern":[4,2]}}"#,
    )
    .unwrap();
    let LayerContent::Path { line_dash, .. } = content else {
        panic!("expected a path, got {content:?}");
    };
    assert_eq!(
        line_dash.as_deref(),
        Some(&LineDash {
            pattern: vec![4.0, 2.0],
            offset: 0.0
        })
    );
    let error = serde_json::from_str::<LayerContent>(
        r#"{"type":"path","commands":[],"lineDash":{"pattern":[4,-1]}}"#,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("lineDash pattern lengths must be finite and at least 0"),
        "{error}"
    );
}

#[test]
fn tagged_content_reports_a_missing_or_unknown_type() {
    let missing = serde_json::from_str::<Paint>(r##"{"color":"#fff"}"##).unwrap_err();
    assert!(
        missing.to_string().contains("missing field `type`"),
        "{missing}"
    );
    let unknown =
        serde_json::from_str::<Paint>(r##"{"type":"conic","color":"#fff"}"##).unwrap_err();
    assert!(
        unknown.to_string().contains("unknown variant `conic`"),
        "{unknown}"
    );
}

#[test]
fn nested_groups_round_trip() {
    let leaf = LayerContent::Path {
        commands: vec![
            PathCommand::MoveTo { x: 1.0, y: 2.0 },
            PathCommand::LineTo { x: 3.0, y: 4.0 },
            PathCommand::Close,
        ],
        fill: Some(Paint::Linear {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 1.0, y: 0.0 },
            stops: Vec::new(),
        }),
        stroke: None,
        line_cap: LineCap::Round,
        line_join: Default::default(),
        miter_limit: 2.0,
        line_dash: Some(Box::new(LineDash {
            pattern: vec![0.0, 6.0],
            offset: -3.0,
        })),
    };
    let json = serde_json::to_string(&leaf).unwrap();
    let group = format!(
        r#"{{"type":"group","layers":[{{"id":"a","transform":{{}},"opacity":1,"content":{{"type":"group","layers":[{{"id":"b","transform":{{}},"opacity":1,"content":{json}}}]}}}}]}}"#
    );
    let LayerContent::Group { layers, .. } = serde_json::from_str::<LayerContent>(&group).unwrap()
    else {
        panic!("expected a group");
    };
    let LayerContent::Group { layers, .. } = &layers[0].content else {
        panic!("expected a nested group");
    };
    assert_eq!(layers[0].content, leaf);
}

#[test]
fn tagged_content_rejects_a_repeated_type() {
    for json in [
        r##"{"type":"solid","color":"#fff","type":"solid"}"##,
        r##"{"color":"#fff","type":"solid","type":"solid"}"##,
    ] {
        let error = serde_json::from_str::<Paint>(json).unwrap_err();
        assert!(
            error.to_string().contains("duplicate field `type`"),
            "{error}"
        );
    }
}

/// Every variant of the `type`-tagged enums, once with its optional fields
/// at their defaults (which `Serialize` leaves out) and once with them set.
/// Their deserialization mirrors repeat the fields' serde attributes by
/// hand; a missing `default` or a different name fails to round trip.
#[test]
fn every_tagged_variant_round_trips() {
    fn round_trip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: T) {
        // A string keeps `type` first; a `Value` sorts it among the keys.
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<T>(&json).unwrap(), value, "{json}");
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(
            serde_json::from_value::<T>(json.clone()).unwrap(),
            value,
            "{json}"
        );
    }

    let asset = || ResolvedAsset {
        id: "a".to_owned(),
        location: AssetLocation::File {
            path: "a.png".to_owned(),
        },
    };
    let solid = || Paint::Solid {
        color: "#fff".to_owned(),
    };
    let stroke = || Stroke {
        paint: solid(),
        width: 2.0,
    };
    let point = |x, y| Point { x, y };

    round_trip(AssetLocation::Url {
        url: "https://example.com/a.png".to_owned(),
    });
    round_trip(solid());
    round_trip(Paint::Linear {
        start: point(0.0, 0.0),
        end: point(1.0, 2.0),
        stops: Vec::new(),
    });
    round_trip(Paint::Radial {
        center: point(1.0, 2.0),
        radius: 3.0,
        stops: Vec::new(),
    });

    let mut contents = vec![
        LayerContent::Video {
            asset: asset(),
            timing: MediaTiming {
                local_time: Time::new(1, 2),
                source_start: Time::new(0, 1),
                source_time_seconds: 0.5,
                playback_rate: 1.0,
                r#loop: false,
            },
            width: Some(160.0),
            height: Some(90.0),
        },
        LayerContent::MissingComponent {
            component: "Card".to_owned(),
            props: [("title".to_owned(), serde_json::json!("hi"))].into(),
        },
    ];
    for set in [false, true] {
        contents.extend([
            LayerContent::Image {
                asset: asset(),
                width: set.then_some(10.0),
                height: set.then_some(20.0),
                fit: set.then_some(ImageFit::Cover),
                animation: None,
            },
            LayerContent::Psd {
                asset: asset(),
                visible_layers: if set {
                    vec!["a".to_owned()]
                } else {
                    Vec::new()
                },
                enabled_layers: if set {
                    vec!["b".to_owned()]
                } else {
                    Vec::new()
                },
                disabled_layers: if set {
                    vec!["c".to_owned()]
                } else {
                    Vec::new()
                },
            },
            LayerContent::Text {
                text: "hi".to_owned(),
                style: Box::new(TextStyle::default()),
                max_width: set.then_some(100.0),
                baseline_anchor: set,
            },
            LayerContent::Group {
                layers: vec![Layer {
                    id: "child".to_owned(),
                    transform: EvaluatedTransform::default(),
                    opacity: 1.0,
                    blend_mode: Default::default(),
                    effects: Default::default(),
                    content: LayerContent::Group {
                        layers: Vec::new(),
                        clip: None,
                        mask: None,
                        motion_blur: false,
                    },
                }],
                clip: set.then_some(Clip {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                    corner_radius: 2.0,
                }),
                mask: set.then(|| GroupMask {
                    layers: Vec::new(),
                    mode: MaskMode::Luminance,
                    invert: true,
                }),
                motion_blur: set,
            },
            LayerContent::StaticRef { key: 7 },
            LayerContent::Rect {
                width: 10.0,
                height: 20.0,
                fill: set.then(solid),
                stroke: set.then(stroke),
                corner_radius: if set { 3.0 } else { 0.0 },
            },
            LayerContent::Path {
                commands: vec![
                    PathCommand::MoveTo { x: 0.0, y: 0.0 },
                    PathCommand::LineTo { x: 1.0, y: 0.0 },
                    PathCommand::QuadTo {
                        x1: 1.0,
                        y1: 1.0,
                        x: 0.0,
                        y: 1.0,
                    },
                    PathCommand::CubicTo {
                        x1: 0.0,
                        y1: 2.0,
                        x2: 1.0,
                        y2: 2.0,
                        x: 1.0,
                        y: 3.0,
                    },
                    PathCommand::Close,
                ],
                fill: set.then(solid),
                stroke: set.then(stroke),
                line_cap: if set { LineCap::Square } else { LineCap::Butt },
                line_join: if set {
                    LineJoin::Bevel
                } else {
                    LineJoin::Miter
                },
                miter_limit: if set { 2.0 } else { crate::DEFAULT_MITER_LIMIT },
                line_dash: set.then(|| {
                    Box::new(LineDash {
                        pattern: vec![4.0, 2.0],
                        offset: 1.5,
                    })
                }),
            },
        ]);
        for shape in [
            InstanceShape::Rect {
                corner_radius: if set { 2.0 } else { 0.0 },
            },
            InstanceShape::Circle,
            InstanceShape::Image { asset: asset() },
        ] {
            round_trip(shape.clone());
            contents.push(LayerContent::Instances {
                count: 2,
                shape: Box::new(shape),
                width: set.then_some(4.0),
                height: set.then_some(5.0),
                color: set.then(|| "#ff0000".to_owned()),
                x: F32Column::from(vec![1.0, 2.0]),
                y: F32Column::from(vec![3.0, 4.0]),
                widths: set.then(|| F32Column::from(vec![6.0, 7.0])),
                heights: set.then(|| F32Column::from(vec![8.0, 9.0])),
                rotation: set.then(|| F32Column::from(vec![10.0, 11.0])),
                colors: set.then(|| U8Column::from(vec![1, 2, 3, 4, 5, 6, 7, 8])),
            });
        }
    }
    for content in contents {
        round_trip(content);
    }
}

#[test]
fn instance_columns_round_trip_as_base64() {
    let floats = F32Column::from(vec![1.5, -2.0]);
    assert_eq!(serde_json::to_string(&floats).unwrap(), r#""AADAPwAAAMA=""#);
    assert_eq!(
        serde_json::from_str::<F32Column>(r#""AADAPwAAAMA=""#).unwrap(),
        floats
    );
    let bytes = U8Column::from(vec![255, 0, 128, 64]);
    assert_eq!(serde_json::to_string(&bytes).unwrap(), r#""/wCAQA==""#);
    assert_eq!(
        serde_json::from_str::<U8Column>(r#""/wCAQA==""#).unwrap(),
        bytes
    );
    let empty = F32Column::from(Vec::new());
    assert_eq!(serde_json::to_string(&empty).unwrap(), r#""""#);
    assert_eq!(serde_json::from_str::<F32Column>(r#""""#).unwrap(), empty);
    // Not a whole number of f32s.
    assert!(serde_json::from_str::<F32Column>(r#""/wCA""#).is_err());
}

fn instances(count: u32) -> LayerContent {
    LayerContent::Instances {
        count,
        shape: Box::new(InstanceShape::Circle),
        width: Some(4.0),
        height: None,
        color: None,
        x: F32Column::from(vec![1.0; count as usize]),
        y: F32Column::from(vec![2.0; count as usize]),
        widths: None,
        heights: Some(F32Column::from(vec![3.0; count as usize])),
        rotation: Some(F32Column::from(vec![4.0; count as usize])),
        colors: Some(U8Column::from(vec![5; 4 * count as usize])),
    }
}

#[test]
fn instances_round_trip_with_every_column() {
    let content = instances(2);
    let json = serde_json::to_string(&content).unwrap();
    assert!(json.starts_with(r#"{"type":"instances","#), "{json}");
    for key in [r#""heights":"#, r#""rotation":"#, r#""colors":"#] {
        assert!(json.contains(key), "{key} in {json}");
    }
    for key in [r#""widths":"#, r#""height":"#, r#""color":"#] {
        assert!(!json.contains(key), "{key} in {json}");
    }
    assert_eq!(
        serde_json::from_str::<LayerContent>(&json).unwrap(),
        content
    );
    let value = serde_json::to_value(&content).unwrap();
    assert_eq!(
        serde_json::from_value::<LayerContent>(value).unwrap(),
        content
    );
}

#[test]
fn instances_reject_columns_of_the_wrong_length() {
    let mut value = serde_json::to_value(instances(2)).unwrap();
    value["widths"] = value["heights"].clone();
    let three = serde_json::to_value(F32Column::from(vec![0.0; 3])).unwrap();
    for column in ["x", "y", "widths", "heights", "rotation"] {
        let mut wrong = value.clone();
        wrong[column] = three.clone();
        let error = serde_json::from_value::<LayerContent>(wrong)
            .unwrap_err()
            .to_string();
        assert!(error.contains(&format!("`{column}`")), "{error}");
        assert!(error.contains("count 2"), "{error}");
    }
    let mut wrong = value;
    wrong["colors"] = serde_json::to_value(U8Column::from(vec![0; 7])).unwrap();
    let error = serde_json::from_value::<LayerContent>(wrong)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("`colors`") && error.contains("count 2"),
        "{error}"
    );
}

#[test]
fn instances_with_no_particles_parse() {
    let content: LayerContent = serde_json::from_str(
        r#"{"type":"instances","count":0,"shape":{"type":"rect","cornerRadius":0},"x":"","y":""}"#,
    )
    .unwrap();
    let LayerContent::Instances { count, x, y, .. } = content else {
        panic!("{content:?}");
    };
    assert_eq!((count, x.len(), y.len()), (0, 0, 0));
}

#[test]
fn text_font_runs_round_trip_and_stay_out_of_plain_styles() {
    use crate::TextFontRun;
    assert_eq!(
        serde_json::to_value(TextStyle::default()).unwrap(),
        serde_json::json!({})
    );
    let json = serde_json::json!({
        "fontRuns": [
            { "start": 1, "end": 3, "fontWeight": 700 },
            { "start": 3, "end": 4, "fontFamily": "Bebas Neue" }
        ]
    });
    let style: TextStyle = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(
        style.font_runs,
        vec![
            TextFontRun {
                start: 1,
                end: 3,
                font_weight: Some(700),
                ..TextFontRun::default()
            },
            TextFontRun {
                start: 3,
                end: 4,
                font_family: Some("Bebas Neue".into()),
                ..TextFontRun::default()
            },
        ]
    );
    assert_eq!(serde_json::to_value(&style).unwrap(), json);
}

#[test]
fn font_families_take_a_name_or_a_stack() {
    use crate::FontFamily;
    let json = serde_json::json!({
        "fontFamily": ["Inter", "Noto Sans JP"],
        "fontRuns": [
            { "start": 0, "end": 1, "fontFamily": "Bebas Neue" },
            { "start": 1, "end": 2, "fontFamily": ["IBM Plex Mono"] }
        ]
    });
    let style: TextStyle = serde_json::from_value(json.clone()).unwrap();
    let stack = style.font_family.as_ref().unwrap();
    assert_eq!(stack.names(), ["Inter", "Noto Sans JP"]);
    assert_eq!(stack.first(), "Inter");
    assert!(stack.is_stack());
    let name = style.font_runs[0].font_family.as_ref().unwrap();
    assert_eq!(name, &FontFamily::Name("Bebas Neue".to_owned()));
    assert_eq!(name.names(), ["Bebas Neue"]);
    assert!(!name.is_stack());
    // A one-name list is not a stack, and keeps its form.
    assert!(!style.font_runs[1].font_family.as_ref().unwrap().is_stack());
    assert_eq!(serde_json::to_value(&style).unwrap(), json);

    // Rust code can build an empty stack; it reads as one empty name.
    let empty = FontFamily::Stack(Vec::new());
    assert_eq!(empty.names(), [""]);
    assert_eq!(empty.first(), "");
    assert!(!empty.is_stack());

    let error = serde_json::from_value::<TextStyle>(serde_json::json!({ "fontFamily": [] }))
        .unwrap_err()
        .to_string();
    assert!(error.contains("at least one family"), "{error}");
    let error = serde_json::from_value::<TextStyle>(serde_json::json!({ "fontFamily": 3 }))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("a font family name or a list of names"),
        "{error}"
    );
}

#[test]
fn custom_shaders_round_trip_and_omit_their_defaults() {
    use crate::{
        LayerEffects, LayerFilter, LayerShader, Scene, ShaderParam, ShaderParamType, ShaderSource,
    };

    let scene = serde_json::json!({
        "width": 64,
        "height": 64,
        "frameRate": { "numerator": 30, "denominator": 1 },
        "time": { "value": 0, "timescale": 1 },
        "shaders": [{
            "id": "9f3c",
            "name": "ripple",
            "wgsl": "fn effect(input: EffectInput) -> vec4f { return vec4f(0.0); }",
            "params": [{ "name": "time", "type": "f32" }, { "name": "tint", "type": "vec4" }],
        }],
        "layers": [],
    });
    let parsed = serde_json::from_value::<Scene>(scene.clone()).unwrap();
    assert_eq!(
        parsed.shaders,
        vec![ShaderSource {
            id: "9f3c".to_owned(),
            name: Some("ripple".to_owned()),
            wgsl: "fn effect(input: EffectInput) -> vec4f { return vec4f(0.0); }".to_owned(),
            params: vec![
                ShaderParam {
                    name: "time".to_owned(),
                    ty: ShaderParamType::F32,
                },
                ShaderParam {
                    name: "tint".to_owned(),
                    ty: ShaderParamType::Vec4,
                },
            ],
        }]
    );
    assert_eq!(serde_json::to_value(&parsed).unwrap(), scene);

    let mut without = scene;
    without.as_object_mut().unwrap().remove("shaders");
    let parsed = serde_json::from_value::<Scene>(without.clone()).unwrap();
    assert!(parsed.shaders.is_empty());
    assert_eq!(serde_json::to_value(&parsed).unwrap(), without);

    let shader = |id: &str, params: Vec<f64>, padding| {
        LayerFilter::Shader(LayerShader {
            id: id.to_owned(),
            params,
            padding,
        })
    };
    let effects = LayerEffects {
        filters: vec![
            shader("9f3c", vec![1.25, 1.0, 0.5, 0.0, 1.0], 8.0),
            shader("a1b2", Vec::new(), 0.0),
        ],
        ..LayerEffects::default()
    };
    assert!(!effects.is_empty());
    assert_eq!(
        serde_json::to_value(&effects).unwrap(),
        serde_json::json!({ "filters": [
            { "type": "shader", "id": "9f3c", "params": [1.25, 1.0, 0.5, 0.0, 1.0], "padding": 8.0 },
            { "type": "shader", "id": "a1b2" },
        ] })
    );
    let bare = serde_json::json!({ "filters": [{ "type": "shader", "id": "9f3c" }] });
    let parsed = serde_json::from_value::<LayerEffects>(bare.clone()).unwrap();
    assert_eq!(parsed.filters, vec![shader("9f3c", Vec::new(), 0.0)]);
    assert_eq!(serde_json::to_value(&parsed).unwrap(), bare);
    assert!(LayerEffects::default().is_empty());
}

#[test]
fn color_filters_round_trip_in_order_with_shaders() {
    use crate::{LayerEffects, LayerFilter, LayerShader};

    let json = serde_json::json!({ "filters": [
        { "type": "brightness", "amount": 1.25 },
        { "type": "contrast", "amount": 0.5 },
        { "type": "shader", "id": "9f3c" },
        { "type": "saturation", "amount": 2.0 },
        { "type": "hue", "degrees": -90.0 },
        { "type": "grayscale", "amount": 1.0 },
        { "type": "invert", "amount": 0.5 },
        { "type": "tint", "color": "#ff880080", "amount": 0.25 },
        { "type": "vignette", "amount": 0.5, "radius": 0.25, "softness": 0.75 },
    ] });
    let parsed = serde_json::from_value::<LayerEffects>(json.clone()).unwrap();
    assert_eq!(
        parsed.filters,
        vec![
            LayerFilter::Brightness { amount: 1.25 },
            LayerFilter::Contrast { amount: 0.5 },
            LayerFilter::Shader(LayerShader {
                id: "9f3c".to_owned(),
                params: Vec::new(),
                padding: 0.0,
            }),
            LayerFilter::Saturation { amount: 2.0 },
            LayerFilter::Hue { degrees: -90.0 },
            LayerFilter::Grayscale { amount: 1.0 },
            LayerFilter::Invert { amount: 0.5 },
            LayerFilter::Tint {
                color: "#ff880080".to_owned(),
                amount: 0.25,
            },
            LayerFilter::Vignette {
                amount: 0.5,
                radius: 0.25,
                softness: 0.75,
            },
        ]
    );
    assert_eq!(serde_json::to_value(&parsed).unwrap(), json);
    // `Serialize` writes `type` first, which the deserializer reads without
    // buffering; a later `type` loads too.
    let text = serde_json::to_string(&parsed).unwrap();
    assert_eq!(serde_json::from_str::<LayerEffects>(&text).unwrap(), parsed);
    let later = r#"{ "filters": [{ "amount": 0.5, "type": "invert" }] }"#;
    assert_eq!(
        serde_json::from_str::<LayerEffects>(later).unwrap().filters,
        vec![LayerFilter::Invert { amount: 0.5 }]
    );
    let error = serde_json::from_str::<LayerEffects>(r#"{ "filters": [{ "type": "sepia" }] }"#)
        .unwrap_err()
        .to_string();
    assert!(error.contains("sepia"), "{error}");
}

#[test]
fn stylize_filters_and_chroma_keys_round_trip() {
    use crate::{LayerEffects, LayerFilter};

    let json = serde_json::json!({ "filters": [
        { "type": "chromaKey", "color": "#00b140", "tolerance": 0.2, "softness": 0.1, "spill": 0.5 },
        { "type": "pixelate", "size": 8.0 },
        { "type": "wave", "amplitude": 6.0, "wavelength": 80.0, "phase": 90.0, "angle": 0.0 },
        { "type": "zoomBlur", "amount": 0.25, "centerX": 0.5, "centerY": 0.25 },
        { "type": "chromaticAberration", "offset": 4.0, "angle": 30.0 },
        { "type": "halftone", "size": 10.0, "angle": 45.0 },
        { "type": "noise", "amount": 0.2, "seed": 42, "monochrome": false },
        { "type": "scanlines", "amount": 0.5, "spacing": 4.0, "thickness": 0.5, "offset": 1.5 },
    ] });
    let parsed = serde_json::from_value::<LayerEffects>(json.clone()).unwrap();
    assert_eq!(
        parsed.filters,
        vec![
            LayerFilter::ChromaKey {
                color: "#00b140".to_owned(),
                tolerance: 0.2,
                softness: 0.1,
                spill: 0.5,
            },
            LayerFilter::Pixelate { size: 8.0 },
            LayerFilter::Wave {
                amplitude: 6.0,
                wavelength: 80.0,
                phase: 90.0,
                angle: 0.0,
            },
            LayerFilter::ZoomBlur {
                amount: 0.25,
                center_x: 0.5,
                center_y: 0.25,
            },
            LayerFilter::ChromaticAberration {
                offset: 4.0,
                angle: 30.0,
            },
            LayerFilter::Halftone {
                size: 10.0,
                angle: 45.0,
            },
            LayerFilter::Noise {
                amount: 0.2,
                seed: 42,
                monochrome: false,
            },
            LayerFilter::Scanlines {
                amount: 0.5,
                spacing: 4.0,
                thickness: 0.5,
                offset: 1.5,
            },
        ]
    );
    assert_eq!(serde_json::to_value(&parsed).unwrap(), json);
    assert!(parsed.filters.iter().all(|filter| !filter.is_identity()));
    for identity in [
        LayerFilter::Pixelate { size: 1.0 },
        LayerFilter::Halftone {
            size: 0.0,
            angle: 45.0,
        },
        LayerFilter::ChromaticAberration {
            offset: 0.0,
            angle: 0.0,
        },
        LayerFilter::Wave {
            amplitude: 0.0,
            wavelength: 10.0,
            phase: 0.0,
            angle: 0.0,
        },
        LayerFilter::Noise {
            amount: 0.0,
            seed: 1,
            monochrome: true,
        },
    ] {
        assert!(identity.is_identity(), "{identity:?}");
    }
}

#[test]
fn a_tint_is_identity_only_when_its_color_is_transparent() {
    let tint = |color: &str| crate::LayerFilter::Tint {
        color: color.to_owned(),
        amount: 1.0,
    };
    assert!(tint("#FF000000").is_identity());
    assert!(tint("#ff000000").is_identity());
    assert!(!tint("#FF0000").is_identity());
    assert!(!tint("#FF000001").is_identity());
    // Invalid colors reach the renderer, which reports them.
    assert!(!tint("#xyzxyz00").is_identity());
    assert!(!tint("red").is_identity());
}

#[test]
fn colors_parse_six_or_eight_hex_digits() {
    assert_eq!(
        Color::from_hex("#1a2B3c"),
        Some(Color::rgba(0x1A, 0x2B, 0x3C, 255))
    );
    assert_eq!(
        Color::from_hex("#1A2B3C80"),
        Some(Color::rgba(0x1A, 0x2B, 0x3C, 0x80))
    );
    for invalid in [
        "",
        "#",
        "1A2B3C",
        "#1A2B3",
        "#1A2B3C8",
        "#1A2B3C800",
        "#xyzxyz",
        "#+1A2B3",
        " #1A2B3C",
        // Six or eight bytes that split a UTF-8 character must not panic.
        "#aéaaa",
        "#aaaaéaa",
        "#ééé",
    ] {
        assert_eq!(Color::from_hex(invalid), None, "{invalid:?}");
    }
}

#[test]
fn colors_format_as_eight_upper_case_digits_and_interpolate_per_channel() {
    assert_eq!(Color::rgba(0x1A, 0x2B, 0x3C, 255).to_string(), "#1A2B3CFF");
    let from = Color::rgba(0, 100, 255, 0);
    let to = Color::rgba(255, 200, 0, 255);
    assert_eq!(from.lerp(to, 0.0), from);
    assert_eq!(from.lerp(to, 1.0), to);
    assert_eq!(from.lerp(to, 0.5), Color::rgba(128, 150, 128, 128));
    // An overshooting easing saturates rather than wrapping.
    assert_eq!(from.lerp(to, 1.5), Color::rgba(255, 250, 0, 255));
    assert_eq!(from.lerp(to, -0.5), Color::rgba(0, 50, 255, 0));
}

#[test]
fn letter_spacing_is_px_or_em() {
    let parse = |json| {
        serde_json::from_value::<TextStyle>(json)
            .unwrap()
            .letter_spacing
    };
    assert_eq!(
        parse(serde_json::json!({ "letterSpacing": 2.5 })),
        Some(LetterSpacing::Px(2.5))
    );
    assert_eq!(
        parse(serde_json::json!({ "letterSpacing": { "em": -0.02 } })),
        Some(LetterSpacing::Em { em: -0.02 })
    );
    assert_eq!(
        serde_json::to_value(LetterSpacing::Em { em: -0.02 }).unwrap(),
        serde_json::json!({ "em": -0.02 })
    );
    assert_eq!(LetterSpacing::Px(3.0).em(60.0), 0.05);
    assert_eq!(LetterSpacing::Em { em: -0.02 }.em(60.0), -0.02);
    assert!(
        serde_json::from_value::<TextStyle>(serde_json::json!({ "letterSpacing": "1em" })).is_err()
    );
}

#[test]
fn a_layer_without_opacity_is_opaque() {
    let layer: Layer = serde_json::from_str(
        r#"{"id":"a","transform":{},"content":{"type":"rect","width":1,"height":1}}"#,
    )
    .unwrap();
    assert_eq!(layer.opacity, 1.0);
    // Written out, opacity is still explicit.
    assert!(
        serde_json::to_string(&layer)
            .unwrap()
            .contains(r#""opacity":1.0"#)
    );
}

/// A frame of 100,000 layers is moved and copied layer by layer while it is
/// parsed and rendered; the rarely used text style and effects live behind
/// boxes so every layer stays small. The 3D transform fields took it from 312
/// to 368 bytes, and the transform origin to 384.
#[test]
fn layers_stay_small() {
    assert!(
        std::mem::size_of::<Layer>() <= 384,
        "Layer is {} bytes",
        std::mem::size_of::<Layer>()
    );
}

#[test]
fn static_ref_round_trips() {
    let json = r#"{"type":"staticRef","key":7}"#;
    let content: LayerContent = serde_json::from_str(json).unwrap();
    assert_eq!(content, LayerContent::StaticRef { key: 7 });
    assert_eq!(serde_json::to_string(&content).unwrap(), json);
}

fn close(a: &Matrix3, b: &Matrix3) -> bool {
    a.iter()
        .flatten()
        .zip(b.iter().flatten())
        .all(|(x, y)| (x - y).abs() < 1e-9)
}

#[test]
fn default_transform_is_the_identity() {
    let t = EvaluatedTransform::default();
    assert!(close(&t.linear(), &IDENTITY3) && close(&t.homography(), &IDENTITY3));
    assert!(!t.is_projective());
    assert_eq!(
        serde_json::to_value(t).unwrap(),
        serde_json::json!({
            "position": {"x": 0.0, "y": 0.0}, "scale": {"x": 1.0, "y": 1.0},
            "rotation": 0.0, "anchor": {"x": 0.5, "y": 0.5}
        })
    );
}

/// Rotations and scales a flat layer takes, mirrors and zeros included.
fn flat_transforms() -> impl Iterator<Item = EvaluatedTransform> {
    [0.0, -0.0, 30.0, -45.0, 90.0, 180.0, 360.0, 1234.5]
        .into_iter()
        .flat_map(|rotation| {
            [(1.0, 1.0), (2.0, 0.5), (-1.0, 1.0), (0.0, 3.0)].map(|(x, y)| EvaluatedTransform {
                position: Point { x: 12.0, y: -7.0 },
                scale: Point { x, y },
                rotation,
                ..Default::default()
            })
        })
}

#[test]
fn a_flat_linear_part_is_the_general_one() {
    for t in flat_transforms() {
        assert!(t.is_flat());
        assert_eq!(t.linear(), t.linear_3d(), "{t:?}");
    }
    let tilted = EvaluatedTransform {
        rotation_x: 1e-9,
        ..Default::default()
    };
    assert!(!tilted.is_flat());
}

#[test]
fn multiplying_by_a_flat_part_is_the_general_product() {
    let orientation = EvaluatedTransform {
        rotation_x: 30.0,
        rotation_y: -60.0,
        rotation: 10.0,
        skew: Point { x: 5.0, y: 0.0 },
        ..Default::default()
    }
    .linear();
    for t in flat_transforms() {
        let flat = t.linear();
        for o in [IDENTITY3, orientation] {
            assert_eq!(multiply3_flat(&o, &flat), multiply3(&o, &flat), "{t:?}");
        }
    }
}

#[test]
fn skew_shears_by_the_tangent() {
    let t = EvaluatedTransform {
        skew: Point { x: 45.0, y: 0.0 },
        ..Default::default()
    };
    assert!(close(
        &t.linear(),
        &[[1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    ));
}

#[test]
fn orthographic_rotation_y_is_affine() {
    let t = EvaluatedTransform {
        rotation_y: 60.0,
        ..Default::default()
    };
    assert!(close(
        &t.homography(),
        &[[0.5, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    ));
    assert!(!t.is_projective());
}

#[test]
fn perspective_rotation_y_turns_the_right_edge_away() {
    let t = EvaluatedTransform {
        position: Point { x: 10.0, y: 20.0 },
        rotation_y: 90.0,
        perspective: Some(1000.0),
        ..Default::default()
    };
    let h = t.homography();
    assert!((h[2][0] - 0.001).abs() < 1e-12 && h[2][1].abs() < 1e-12 && h[2][2] == 1.0);
    assert!((h[0][2] - 10.0).abs() < 1e-12 && (h[1][2] - 20.0).abs() < 1e-12);
    assert!(t.is_projective());
    assert!(project(&h, 100.0, 0.0)[2] > 1.0); // farther
}

#[test]
fn perspective_distance_must_be_finite_and_positive() {
    for (value, expected) in [
        (500.0, Some(500.0)),
        (0.0, None),
        (-1.0, None),
        (f64::NAN, None),
        (f64::INFINITY, None),
    ] {
        let t = EvaluatedTransform {
            perspective: Some(value),
            rotation_y: 30.0,
            ..Default::default()
        };
        assert_eq!(t.perspective_distance(), expected);
        assert_eq!(t.is_projective(), expected.is_some());
    }
}

#[test]
fn skew_of_ninety_degrees_is_not_projective() {
    let t = EvaluatedTransform {
        skew: Point { x: 90.0, y: 0.0 },
        perspective: Some(500.0),
        ..Default::default()
    };
    assert!(!t.is_projective());
}

#[test]
fn facing_follows_the_accumulated_orientation() {
    let o = |t: EvaluatedTransform| t.linear();
    let ry = |deg| EvaluatedTransform {
        rotation_y: deg,
        ..Default::default()
    };
    assert!(!faces_away(&IDENTITY3));
    assert!(!faces_away(&o(EvaluatedTransform {
        scale: Point { x: -1.0, y: 1.0 },
        ..Default::default()
    })));
    assert!(faces_away(&o(ry(180.0))));
    assert!(!faces_away(&o(ry(90.0))));
    assert!(!faces_away(&multiply3(&o(ry(100.0)), &o(ry(180.0)))));
    assert!(faces_away(&multiply3(&o(ry(30.0)), &o(ry(180.0)))));
}

/// A group turned about `origin` is the group moved to `position + origin`
/// and turned, holding a group moved back by `origin`.
#[test]
fn origin_is_a_nested_group_moved_back() {
    let origin = Point { x: 960.0, y: 540.0 };
    for (perspective, rotation_y, rotation, scale) in [
        (Some(2000.0), 40.0, 0.0, 1.0),
        (None, 0.0, 30.0, 1.5),
        (Some(800.0), -70.0, 10.0, 0.5),
    ] {
        let t = EvaluatedTransform {
            position: Point { x: 12.0, y: -7.0 },
            origin,
            rotation_y,
            rotation,
            scale: Point { x: scale, y: scale },
            perspective,
            ..Default::default()
        };
        let outer = EvaluatedTransform {
            position: Point { x: 972.0, y: 533.0 },
            origin: Point { x: 0.0, y: 0.0 },
            ..t
        };
        let inner = EvaluatedTransform {
            position: Point {
                x: -960.0,
                y: -540.0,
            },
            ..Default::default()
        };
        let nested = multiply3(&outer.homography(), &inner.homography());
        assert!(close(&t.homography(), &nested), "{t:?}");
        assert_eq!(t.is_projective(), perspective.is_some());
        // The vanishing point: the plane's `origin` lands at position + origin.
        let [x, y, w] = project(&t.homography(), origin.x, origin.y);
        assert!((x - 972.0).abs() < 1e-9 && (y - 533.0).abs() < 1e-9 && (w - 1.0).abs() < 1e-12);
    }
}

#[test]
fn translation_is_the_affine_homography_s_offset() {
    let t = EvaluatedTransform {
        position: Point { x: 5.0, y: 6.0 },
        origin: Point { x: 100.0, y: 50.0 },
        rotation: 90.0,
        scale: Point { x: 2.0, y: 1.0 },
        ..Default::default()
    };
    let h = t.homography();
    let Point { x, y } = t.translation(&t.linear());
    assert!((x - h[0][2]).abs() < 1e-9 && (y - h[1][2]).abs() < 1e-9);
    // Untransformed, the origin does not move the layer.
    let still = EvaluatedTransform {
        position: Point { x: 5.0, y: 6.0 },
        origin: Point { x: 100.0, y: 50.0 },
        ..Default::default()
    };
    assert_eq!(still.translation(&still.linear()), still.position);
    assert!(close(
        &still.homography(),
        &[[1.0, 0.0, 5.0], [0.0, 1.0, 6.0], [0.0, 0.0, 1.0]]
    ));
}

#[test]
fn origin_round_trips_and_stays_out_of_json_at_zero() {
    let t: EvaluatedTransform =
        serde_json::from_value(serde_json::json!({"origin": {"x": 960.0, "y": 540.0}})).unwrap();
    assert_eq!(t.origin, Point { x: 960.0, y: 540.0 });
    assert_eq!(
        serde_json::to_value(t).unwrap()["origin"],
        serde_json::json!({"x": 960.0, "y": 540.0})
    );
    let json = serde_json::to_value(EvaluatedTransform::default()).unwrap();
    assert!(json.get("origin").is_none());
}

#[test]
fn stretch_is_none_behind_the_viewer() {
    let h = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.01, 0.0, 1.0]];
    assert!((stretch_at(&h, 0.0, 0.0).unwrap() - 1.0).abs() < 1e-9);
    assert_eq!(stretch_at(&h, -200.0, 0.0), None);
}

#[test]
fn new_transform_fields_round_trip() {
    let json = serde_json::json!({"skew": {"x": 10.0, "y": -5.0}, "rotationX": 15.0, "rotationY": 30.0,
        "perspective": 800.0, "backfaceVisibility": "hidden"});
    let t: EvaluatedTransform = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(t.backface_visibility, BackfaceVisibility::Hidden);
    let back = serde_json::to_value(t).unwrap();
    for key in [
        "skew",
        "rotationX",
        "rotationY",
        "perspective",
        "backfaceVisibility",
    ] {
        assert_eq!(back[key], json[key]);
    }
}

#[test]
fn audio_clip_pitch_fields_default_and_stay_out_of_json() {
    let json = r#"{"id":"a","asset":{"id":"a","location":{"type":"file","path":"a.wav"}},"range":{"start":{"value":0,"timescale":1},"duration":{"value":1,"timescale":1}},"sourceStart":{"value":0,"timescale":1},"playbackRate":1.0,"volume":1.0,"muted":false}"#;
    let clip: AudioClip = serde_json::from_str(json).unwrap();
    assert_eq!(clip.pitch, Animatable::Static(0.0));
    assert!(!clip.preserve_pitch);
    assert_eq!(serde_json::to_string(&clip).unwrap(), json);

    let shifted = AudioClip {
        pitch: Animatable::Static(-2.5),
        preserve_pitch: true,
        ..clip
    };
    let text = serde_json::to_string(&shifted).unwrap();
    assert!(text.contains(r#""pitch":-2.5"#) && text.contains(r#""preservePitch":true"#));
    assert_eq!(serde_json::from_str::<AudioClip>(&text).unwrap(), shifted);
}

#[test]
fn audio_clip_fades_default_and_stay_out_of_json() {
    let json = r#"{"id":"a","asset":{"id":"a","location":{"type":"file","path":"a.wav"}},"range":{"start":{"value":0,"timescale":1},"duration":{"value":4,"timescale":1}},"sourceStart":{"value":0,"timescale":1},"playbackRate":1.0,"volume":1.0,"muted":false}"#;
    let clip: AudioClip = serde_json::from_str(json).unwrap();
    assert_eq!(clip.fade_in, Time::ZERO);
    assert_eq!(clip.fade_out, Time::ZERO);
    assert_eq!(serde_json::to_string(&clip).unwrap(), json);

    let faded = AudioClip {
        fade_in: Time::new(3, 2),
        fade_out: Time::new(1, 1),
        ..clip
    };
    let text = serde_json::to_string(&faded).unwrap();
    assert!(
        text.contains(r#""fadeIn":{"value":3,"timescale":2}"#)
            && text.contains(r#""fadeOut":{"value":1,"timescale":1}"#)
    );
    assert_eq!(serde_json::from_str::<AudioClip>(&text).unwrap(), faded);
}

#[test]
fn audio_fade_ramps_linearly_at_both_edges_of_the_range() {
    let json = r#"{"id":"a","asset":{"id":"a","location":{"type":"file","path":"a.wav"}},"range":{"start":{"value":10,"timescale":1},"duration":{"value":4,"timescale":1}},"sourceStart":{"value":0,"timescale":1},"playbackRate":1.0,"volume":1.0,"muted":false}"#;
    let clip: AudioClip = serde_json::from_str(json).unwrap();
    assert_eq!(clip.fade().unwrap().gain(0.0), 1.0);

    let fade = AudioClip {
        fade_in: Time::new(1, 1),
        fade_out: Time::new(2, 1),
        ..clip.clone()
    }
    .fade()
    .unwrap();
    // Local seconds, not timeline seconds: the range starting at 10 does not
    // matter.
    assert_eq!(fade.gain(0.0), 0.0);
    assert_eq!(fade.gain(0.5), 0.5);
    assert_eq!(fade.gain(1.0), 1.0);
    assert_eq!(fade.gain(2.0), 1.0);
    assert_eq!(fade.gain(3.0), 0.5);
    assert_eq!(fade.gain(4.0), 0.0);
    assert_eq!(fade.gain(-1.0), 0.0);
    assert_eq!(fade.gain(5.0), 0.0);

    // Fades longer than the clip overlap, and their gains multiply.
    let overlapping = AudioClip {
        fade_in: Time::new(4, 1),
        fade_out: Time::new(4, 1),
        ..clip
    }
    .fade()
    .unwrap();
    assert_eq!(overlapping.gain(2.0), 0.25);
    assert_eq!(overlapping.gain(1.0), 0.25 * 0.75);
}

fn voice() -> DuckGroups {
    DuckGroups::One("voice".to_owned())
}

#[test]
fn resolve_applies_defaults_and_decibels() {
    let duck = AudioDuck::resolve(&voice(), None, None, None, None).unwrap();
    assert_eq!(duck.under, ["voice"]);
    assert_eq!(duck.to, 0.3);
    assert_eq!(duck.attack.as_seconds().unwrap(), 0.15);
    assert_eq!(duck.release.as_seconds().unwrap(), 0.4);
    assert_eq!(duck.hold_gap.as_seconds().unwrap(), 0.6);
    let quieter = AudioDuck::resolve(
        &DuckGroups::Many(vec!["voice".to_owned(), "sfx".to_owned()]),
        Some(DuckLevel::Db { db: -6.0 }),
        Some(Time::ZERO),
        None,
        None,
    )
    .unwrap();
    assert_eq!(quieter.under, ["voice", "sfx"]);
    assert!((quieter.to - 10_f64.powf(-6.0 / 20.0)).abs() < 1e-15);
    assert_eq!(quieter.attack, Time::ZERO);
}

#[test]
fn resolve_rejects_invalid_values() {
    let to = |level| AudioDuck::resolve(&voice(), Some(level), None, None, None);
    for level in [
        DuckLevel::Gain(1.5),
        DuckLevel::Gain(-0.1),
        DuckLevel::Gain(f64::NAN),
        DuckLevel::Db { db: 1.0 },
        DuckLevel::Db {
            db: f64::NEG_INFINITY,
        },
    ] {
        assert_eq!(to(level), Err(DuckError::To));
    }
    let negative = Some(Time::new(-1, 10));
    assert_eq!(
        AudioDuck::resolve(&voice(), None, negative, None, None),
        Err(DuckError::Attack)
    );
    assert_eq!(
        AudioDuck::resolve(&voice(), None, None, negative, None),
        Err(DuckError::Release)
    );
    assert_eq!(
        AudioDuck::resolve(&voice(), None, None, None, negative),
        Err(DuckError::HoldGap)
    );
    for under in [DuckGroups::Many(Vec::new()), DuckGroups::One(String::new())] {
        assert_eq!(
            AudioDuck::resolve(&under, None, None, None, None),
            Err(DuckError::Under)
        );
    }
    assert_eq!(check_duck_group(""), Err(DuckError::Group));
    assert_eq!(check_duck_group("voice"), Ok(()));
}

fn duck_clip(start: f64, end: f64) -> AudioClip {
    let seconds = |value: f64| Time::new((value * 1000.0).round() as i64, 1000);
    let json = r#"{"id":"a","asset":{"id":"a","location":{"type":"file","path":"a.wav"}},"range":{"start":{"value":0,"timescale":1},"duration":{"value":1,"timescale":1}},"sourceStart":{"value":0,"timescale":1},"playbackRate":1.0,"volume":1.0,"muted":false}"#;
    let mut clip: AudioClip = serde_json::from_str(json).unwrap();
    clip.range.start = seconds(start);
    clip.range.duration = seconds(end - start);
    clip
}

fn marked(start: f64, end: f64) -> AudioClip {
    AudioClip {
        duck_group: Some("voice".to_owned()),
        ..duck_clip(start, end)
    }
}

fn ducked(start: f64, end: f64, under: &str) -> AudioClip {
    let under = DuckGroups::One(under.to_owned());
    AudioClip {
        duck: Some(AudioDuck::resolve(&under, None, None, None, None).unwrap()),
        ..duck_clip(start, end)
    }
}

fn range_ends(clips: &[AudioClip]) -> Vec<f64> {
    clips
        .iter()
        .map(|clip| clip.range.end().unwrap().as_seconds().unwrap())
        .collect()
}

#[test]
fn duck_curves_follow_marked_clips_only() {
    let clips = [
        marked(1.0, 2.0),
        AudioClip {
            muted: true,
            ..marked(3.0, 4.0)
        },
        ducked(0.0, 6.0, "voice"),
    ];
    let curves = duck_curves(&clips, &range_ends(&clips)).unwrap();
    let bed = clips[2].duck.as_ref().unwrap();
    assert_eq!(curves[0], None);
    assert_eq!(curves[1], None);
    assert_eq!(curves[2], Some(duck_curve(&[(1.0, 2.0)], bed).unwrap()));
}

#[test]
fn duck_curves_exclude_the_clip_itself() {
    let clips = [AudioClip {
        duck_group: Some("voice".to_owned()),
        ..ducked(0.0, 6.0, "voice")
    }];
    let curves = duck_curves(&clips, &range_ends(&clips)).unwrap();
    assert_eq!(curves[0], Some(Animatable::Static(1.0)));
}

#[test]
fn duck_curves_use_the_audible_end() {
    let clips = [marked(1.0, 5.0), ducked(0.0, 6.0, "voice")];
    let curves = duck_curves(&clips, &[2.5, 6.0]).unwrap();
    let bed = clips[1].duck.as_ref().unwrap();
    assert_eq!(curves[1], Some(duck_curve(&[(1.0, 2.5)], bed).unwrap()));
}

#[test]
fn duck_curves_ignore_unknown_groups() {
    let clips = [marked(1.0, 2.0), ducked(0.0, 6.0, "sfx")];
    let curves = duck_curves(&clips, &range_ends(&clips)).unwrap();
    assert_eq!(curves[1], Some(Animatable::Static(1.0)));
}

#[test]
fn clips_without_ducking_serialize_as_before() {
    let json = r#"{"id":"a","asset":{"id":"a","location":{"type":"file","path":"a.wav"}},"range":{"start":{"value":0,"timescale":1},"duration":{"value":1,"timescale":1}},"sourceStart":{"value":0,"timescale":1},"playbackRate":1.0,"volume":1.0,"muted":false}"#;
    let clip: AudioClip = serde_json::from_str(json).unwrap();
    assert_eq!((&clip.duck_group, &clip.duck), (&None, &None));
    assert_eq!(serde_json::to_string(&clip).unwrap(), json);

    let both = AudioClip {
        duck_group: Some("sfx".to_owned()),
        ..ducked(0.0, 1.0, "voice")
    };
    let text = serde_json::to_string(&both).unwrap();
    assert!(text.contains(r#""duckGroup":"sfx""#) && text.contains(r#""duck":{"under":["voice"]"#));
    assert_eq!(serde_json::from_str::<AudioClip>(&text).unwrap(), both);
}
