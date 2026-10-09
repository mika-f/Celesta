use crate::time::{Rational, Time};
use crate::{
    AssetLocation, Clip, EvaluatedTransform, GroupMask, ImageFit, Layer, LayerContent, LineCap,
    LineJoin, MaskMode, MediaTiming, Paint, PathCommand, Point, ResolvedAsset, Stroke, TextStyle,
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
        ..
    } = content
    else {
        panic!("expected a path, got {content:?}");
    };
    assert_eq!(commands, [PathCommand::Close]);
    assert_eq!(fill, None);
    assert_eq!(line_cap, LineCap::Butt);
    assert_eq!(miter_limit, crate::DEFAULT_MITER_LIMIT);
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
            },
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
                style: TextStyle::default(),
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
            },
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
            },
        ]);
    }
    for content in contents {
        round_trip(content);
    }
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
                font_family: None,
            },
            TextFontRun {
                start: 3,
                end: 4,
                font_weight: None,
                font_family: Some("Bebas Neue".to_owned()),
            },
        ]
    );
    assert_eq!(serde_json::to_value(&style).unwrap(), json);
}

#[test]
fn custom_shaders_round_trip_and_omit_their_defaults() {
    use crate::{LayerEffects, LayerShader, Scene, ShaderParam, ShaderParamType, ShaderSource};

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

    let effects = LayerEffects {
        shader: Some(LayerShader {
            id: "9f3c".to_owned(),
            params: vec![1.25, 1.0, 0.5, 0.0, 1.0],
            padding: 8.0,
        }),
        ..LayerEffects::default()
    };
    assert!(!effects.is_empty());
    assert_eq!(
        serde_json::to_value(&effects).unwrap(),
        serde_json::json!({ "shader": { "id": "9f3c", "params": [1.25, 1.0, 0.5, 0.0, 1.0], "padding": 8.0 } })
    );
    let bare = serde_json::json!({ "shader": { "id": "9f3c" } });
    let parsed = serde_json::from_value::<LayerEffects>(bare.clone()).unwrap();
    assert_eq!(
        parsed.shader,
        Some(LayerShader {
            id: "9f3c".to_owned(),
            params: Vec::new(),
            padding: 0.0,
        })
    );
    assert_eq!(serde_json::to_value(&parsed).unwrap(), bare);
    assert!(LayerEffects::default().is_empty());
}
