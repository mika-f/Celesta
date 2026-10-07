use crate::time::{Rational, Time};
use crate::{Clip, LayerContent, LineCap, Paint, PathCommand, Point};
use std::cmp::Ordering;

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
fn a_group_without_a_clip_serializes_and_parses_as_before() {
    let content = LayerContent::Group {
        layers: Vec::new(),
        clip: None,
    };
    let json = serde_json::to_value(&content).unwrap();
    assert_eq!(json, serde_json::json!({ "type": "group", "layers": [] }));
    assert_eq!(
        serde_json::from_value::<LayerContent>(json).unwrap(),
        content
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
