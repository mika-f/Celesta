use crate::character::{LipSyncCue, LipSyncDefinition, MouthShape};
use crate::document::{Project, ProjectVersion};
use crate::timeline::{TimelineContent, TimelineEffects, TimelineItem, TimelineShadow};
use crate::{Animatable, BlendMode, Time};
use serde_json::Value;

#[test]
fn image_display_fields_round_trip_and_validate() {
    let mut json: Value =
        serde_json::from_str(include_str!("../../../examples/voiceroid.celesta.json")).unwrap();
    json["tracks"][0]["items"][0]["content"] = serde_json::json!({"type": "image", "asset": "akane-default", "width": 432, "height": 200, "fit": "cover"});
    let project = Project::from_json(&json.to_string()).unwrap();
    assert_eq!(
        Project::from_json(&project.to_json().unwrap()).unwrap(),
        project
    );
    json["tracks"][0]["items"][0]["content"]["width"] = 0.into();
    assert!(Project::from_json(&json.to_string()).is_err());
}
#[test]
fn loads_the_minimal_project() {
    let project =
        Project::from_json(include_str!("../../../examples/minimal.celesta.json")).unwrap();
    assert_eq!(project.version, ProjectVersion::V0);
    assert_eq!(project.effective_duration().unwrap(), Time::ZERO);
}

#[test]
fn calculates_duration_from_the_latest_item_end() {
    let project =
        Project::from_json(include_str!("../../../examples/voiceroid.celesta.json")).unwrap();
    assert_eq!(project.effective_duration().unwrap(), Time::new(8, 1));
}

#[test]
fn pretty_serialization_round_trips() {
    let project =
        Project::from_json(include_str!("../../../examples/voiceroid.celesta.json")).unwrap();

    let json = project.to_json().unwrap();

    assert!(json.ends_with('\n'));
    assert!(json.contains("\n  \"settings\":"));
    assert_eq!(Project::from_json(&json).unwrap(), project);
}

#[test]
fn reads_and_writes_an_items_blend_mode() {
    let mut project =
        Project::from_json(include_str!("../../../examples/voiceroid.celesta.json")).unwrap();
    assert!(!project.to_json().unwrap().contains("blendMode"));

    let mut item = serde_json::to_value(&project.tracks[0].items[0]).unwrap();
    item["blendMode"] = "difference".into();
    project.tracks[0].items[0] = serde_json::from_value(item).unwrap();
    assert_eq!(
        project.tracks[0].items[0].blend_mode,
        Some(BlendMode::Difference)
    );
    assert!(
        project
            .to_json()
            .unwrap()
            .contains("\"blendMode\": \"difference\"")
    );

    let mut item = serde_json::to_value(&project.tracks[0].items[0]).unwrap();
    item["blendMode"] = "hue".into();
    assert!(serde_json::from_value::<TimelineItem>(item).is_err());
}

#[test]
fn validates_and_round_trips_effect_properties() {
    let mut project =
        Project::from_json(include_str!("../../../examples/voiceroid.celesta.json")).unwrap();
    project.tracks[0].items[0].effects = Some(TimelineEffects {
        blur: Some(Animatable::Static(3.0)),
        shadow: Some(TimelineShadow {
            color: Animatable::Static("#00000080".to_owned()),
            blur: Animatable::Static(5.0),
            offset_x: Animatable::Static(4.0),
            offset_y: Animatable::Static(-2.0),
        }),
        glow: None,
    });
    project.validate().unwrap();
    let json = project.to_json().unwrap();
    assert!(json.contains("\"offsetX\": 4.0"));
    assert_eq!(Project::from_json(&json).unwrap(), project);

    project.tracks[0].items[0]
        .effects
        .as_mut()
        .unwrap()
        .shadow
        .as_mut()
        .unwrap()
        .color = Animatable::Static("not-a-color".to_owned());
    let errors = project.validate().unwrap_err();
    assert!(errors.to_string().contains("effects.shadow.color"));
}

#[test]
fn validates_project_master_volume() {
    let mut project =
        Project::from_json(include_str!("../../../examples/voiceroid.celesta.json")).unwrap();
    project.settings.master_volume = Some(-0.1);

    let errors = project.validate().unwrap_err();

    assert_eq!(errors.as_slice()[0].path, "settings.masterVolume");
}

#[test]
fn lip_sync_configuration_and_cues_round_trip() {
    let mut project =
        Project::from_json(include_str!("../../../examples/voiceroid.celesta.json")).unwrap();
    project
        .characters
        .get_mut("akane")
        .unwrap()
        .portrait
        .as_mut()
        .unwrap()
        .lip_sync = Some(LipSyncDefinition {
        a: "akane-default".to_owned(),
        i: "akane-default".to_owned(),
        u: "akane-default".to_owned(),
        e: "akane-default".to_owned(),
        o: "akane-default".to_owned(),
        closed: Some("akane-default".to_owned()),
        transform: None,
    });
    let TimelineContent::Dialogue { lip_sync, .. } = &mut project.tracks[0].items[0].content else {
        panic!("example must contain dialogue")
    };
    *lip_sync = vec![
        LipSyncCue {
            time: Time::ZERO,
            shape: MouthShape::Closed,
        },
        LipSyncCue {
            time: Time::new(1, 2),
            shape: MouthShape::A,
        },
    ];

    let json = project.to_json().unwrap();
    let loaded = Project::from_json(&json).unwrap();

    assert_eq!(loaded, project);
    assert!(json.contains("\"lipSync\""));
    assert!(json.contains("\"shape\": \"a\""));
}

#[test]
fn lip_sync_cues_require_voice_and_strict_time_order() {
    let mut project =
        Project::from_json(include_str!("../../../examples/voiceroid.celesta.json")).unwrap();
    let TimelineContent::Dialogue {
        audio, lip_sync, ..
    } = &mut project.tracks[0].items[0].content
    else {
        panic!("example must contain dialogue")
    };
    *audio = None;
    *lip_sync = vec![
        LipSyncCue {
            time: Time::new(1, 1),
            shape: MouthShape::I,
        },
        LipSyncCue {
            time: Time::new(1, 1),
            shape: MouthShape::Closed,
        },
    ];

    let errors = project.validate().unwrap_err();

    assert!(
        errors
            .as_slice()
            .iter()
            .any(|error| error.path.ends_with("content.lipSync"))
    );
    assert!(errors.as_slice().iter().any(|error| {
        error.path.ends_with("content.lipSync[1].time")
            && error.message.contains("strictly ascending")
    }));
}
