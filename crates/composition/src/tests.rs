use crate::time::{Rational, Time};
use crate::{Clip, LayerContent};
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
