use crate::Evaluator;
use celesta_composition::{Animatable, BlendMode, Scene};

use celesta_composition::{Keyframe, KeyframeAnimation, KeyframeAnimationType, LayerContent, Time};
use celesta_project::{
    LipSyncCue, LipSyncDefinition, MouthShape, Project, SourceRange, TimelineContent,
    TimelineEffects, TimelineGlow, TimelineShadow,
};

fn example() -> Project {
    Project::from_json(include_str!("../../../examples/voiceroid.celesta.json")).unwrap()
}

#[test]
fn expands_dialogue_into_visual_and_audio_composition() {
    let project = example();
    let evaluator = Evaluator::new(&project).unwrap();

    assert!(
        evaluator
            .scene_at(Time::new(499, 100))
            .unwrap()
            .layers
            .is_empty()
    );

    let scene = evaluator.scene_at(Time::new(21, 4)).unwrap();
    assert_eq!(scene.layers.len(), 1);
    assert_eq!(scene.layers[0].opacity, 1.0);
    let LayerContent::Group { layers, .. } = &scene.layers[0].content else {
        panic!("dialogue must expand to a group");
    };
    assert_eq!(layers.len(), 2);
    assert!(matches!(layers[0].content, LayerContent::Image { .. }));
    assert!(matches!(layers[1].content, LayerContent::Text { .. }));

    let audio = evaluator.audio_graph().unwrap();
    assert_eq!(audio.clips.len(), 1);
    assert_eq!(audio.clips[0].id, "dialogue-001:voice");
    assert_eq!(audio.master_volume, 1.0);
}

#[test]
fn carries_an_items_blend_mode_onto_its_layer() {
    let mut project = example();
    let scene_time = Time::new(21, 4);
    let evaluator = Evaluator::new(&project).unwrap();
    assert_eq!(
        evaluator.scene_at(scene_time).unwrap().layers[0].blend_mode,
        BlendMode::Normal
    );

    project.tracks[0].items[0].blend_mode = Some(BlendMode::Screen);
    let evaluator = Evaluator::new(&project).unwrap();
    assert_eq!(
        evaluator.scene_at(scene_time).unwrap().layers[0].blend_mode,
        BlendMode::Screen
    );
}

#[test]
fn evaluates_animated_effects_at_item_local_time() {
    let mut project = example();
    let animated = Animatable::Keyframes(KeyframeAnimation {
        kind: KeyframeAnimationType::Keyframes,
        keyframes: vec![
            Keyframe {
                time: Time::ZERO,
                value: 0.0,
                easing: None,
            },
            Keyframe {
                time: Time::new(1, 1),
                value: 8.0,
                easing: None,
            },
        ],
    });
    project.tracks[0].items[0].effects = Some(TimelineEffects {
        blur: Some(animated.clone()),
        shadow: Some(TimelineShadow {
            color: Animatable::Keyframes(KeyframeAnimation {
                kind: KeyframeAnimationType::Keyframes,
                keyframes: vec![
                    Keyframe {
                        time: Time::ZERO,
                        value: "#00000080".to_owned(),
                        easing: None,
                    },
                    Keyframe {
                        time: Time::new(1, 1),
                        value: "#FF0000FF".to_owned(),
                        easing: None,
                    },
                ],
            }),
            blur: Animatable::Static(2.0),
            offset_x: animated,
            offset_y: Animatable::Static(3.0),
        }),
        glow: Some(TimelineGlow {
            color: Animatable::Static("#ffffff".to_owned()),
            blur: Animatable::Static(4.0),
        }),
    });
    let start = project.tracks[0].items[0].range.start;
    let time = start.checked_add(Time::new(1, 2)).unwrap();
    let scene = Evaluator::new(&project).unwrap().scene_at(time).unwrap();
    let layer = &scene.layers[0];
    assert_eq!(layer.effects.blur, 4.0);
    assert_eq!(layer.effects.shadow.as_ref().unwrap().offset_x, 4.0);
    assert_eq!(layer.effects.shadow.as_ref().unwrap().color, "#800000C0");
    assert_eq!(layer.effects.glow.as_ref().unwrap().blur, 4.0);
}

#[test]
fn evaluates_lip_sync_as_a_mouth_overlay_without_replacing_the_expression() {
    let mut project = example();
    let portrait = project
        .characters
        .get_mut("akane")
        .unwrap()
        .portrait
        .as_mut()
        .unwrap();
    portrait.lip_sync = Some(LipSyncDefinition {
        a: "akane-a".to_owned(),
        i: "akane-default".to_owned(),
        u: "akane-default".to_owned(),
        e: "akane-default".to_owned(),
        o: "akane-default".to_owned(),
        closed: Some("akane-default".to_owned()),
        transform: None,
    });
    project.assets.insert(
        "akane-a".to_owned(),
        project.assets["akane-default"].clone(),
    );
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
    let evaluator = Evaluator::new(&project).unwrap();

    let closed = evaluator.scene_at(Time::new(21, 4)).unwrap();
    let open = evaluator.scene_at(Time::new(23, 4)).unwrap();
    let mouth_asset = |scene: &Scene| {
        let LayerContent::Group { layers, .. } = &scene.layers[0].content else {
            panic!("dialogue must expand to a group")
        };
        assert_eq!(layers.len(), 3);
        let LayerContent::Image { asset, .. } = &layers[1].content else {
            panic!("second dialogue layer must be the mouth")
        };
        asset.id.clone()
    };

    assert_eq!(mouth_asset(&closed), "akane-default");
    assert_eq!(mouth_asset(&open), "akane-a");

    project
        .characters
        .get_mut("akane")
        .unwrap()
        .portrait
        .as_mut()
        .unwrap()
        .lip_sync
        .as_mut()
        .unwrap()
        .closed = None;
    let closed = Evaluator::new(&project)
        .unwrap()
        .scene_at(Time::new(21, 4))
        .unwrap();
    let LayerContent::Group { layers, .. } = &closed.layers[0].content else {
        panic!("dialogue must expand to a group")
    };
    assert_eq!(layers.len(), 2, "no closed overlay uses the base portrait");
}

#[test]
fn applies_track_mute_and_project_master_volume_to_audio_graph() {
    let mut project = example();
    project.settings.master_volume = Some(0.5);
    project.tracks[0].muted = Some(true);

    let audio = Evaluator::new(&project).unwrap().audio_graph().unwrap();

    assert!(audio.clips.is_empty());
    assert_eq!(audio.master_volume, 0.5);
}

#[test]
fn audio_graph_preserves_source_range_and_playback_mapping() {
    let mut project = example();
    project.tracks[0].items[0].content = TimelineContent::Audio {
        asset: "voice-001".to_owned(),
        source_range: Some(SourceRange {
            start: Time::new(1, 2),
            duration: Some(Time::new(3, 2)),
        }),
        playback_rate: Some(Animatable::Static(2.0)),
        volume: None,
        muted: None,
    };

    let audio = Evaluator::new(&project).unwrap().audio_graph().unwrap();

    assert_eq!(audio.clips[0].source_start, Time::new(1, 2));
    assert_eq!(audio.clips[0].source_duration, Some(Time::new(3, 2)));
    assert_eq!(audio.clips[0].playback_rate, Animatable::Static(2.0));
}

#[test]
fn solo_tracks_exclude_audio_from_other_tracks() {
    let mut project = example();
    let mut solo = project.tracks[0].clone();
    solo.id = "solo-dialogue".to_owned();
    solo.items[0].id = "solo-dialogue-item".to_owned();
    solo.solo = Some(true);
    project.tracks.push(solo);

    let audio = Evaluator::new(&project).unwrap().audio_graph().unwrap();

    assert_eq!(audio.clips.len(), 1);
    assert_eq!(audio.clips[0].id, "solo-dialogue-item:voice");
}

#[test]
fn evaluates_a_single_track_independent_of_the_others() {
    let mut project = example();
    let mut other = project.tracks[0].clone();
    other.id = "other".to_owned();
    other.items[0].id = "other-item".to_owned();
    project.tracks.push(other);
    let evaluator = Evaluator::new(&project).unwrap();

    let time = Time::new(21, 4);
    let dialogue_layers = evaluator.layers_for_track("dialogue", time).unwrap();
    let other_layers = evaluator.layers_for_track("other", time).unwrap();
    // Both tracks hold an equivalent dialogue item, evaluated the same
    // way, but each track's own evaluation stays scoped to itself.
    assert_eq!(dialogue_layers.len(), 1);
    assert_eq!(other_layers.len(), 1);
    assert_eq!(dialogue_layers[0].id, "dialogue-001");
    assert_eq!(other_layers[0].id, "other-item");
    assert_eq!(evaluator.scene_at(time).unwrap().layers.len(), 2);
}

#[test]
fn unknown_or_disabled_track_evaluates_to_no_layers() {
    let mut project = example();
    let evaluator = Evaluator::new(&project).unwrap();
    let time = Time::new(21, 4);

    assert!(
        evaluator
            .layers_for_track("does-not-exist", time)
            .unwrap()
            .is_empty()
    );

    project.tracks[0].enabled = Some(false);
    let evaluator = Evaluator::new(&project).unwrap();
    assert!(
        evaluator
            .layers_for_track("dialogue", time)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn item_end_is_exclusive() {
    let project = example();
    let evaluator = Evaluator::new(&project).unwrap();
    assert!(
        evaluator
            .scene_at(Time::new(8, 1))
            .unwrap()
            .layers
            .is_empty()
    );
}

#[test]
fn returns_missing_components_as_placeholder_layers() {
    let mut project = example();
    project.tracks[0].items[0].content = TimelineContent::Component {
        component: "BossIntroduction".to_owned(),
        props: None,
    };
    let evaluator = Evaluator::new(&project).unwrap();
    let scene = evaluator.scene_at(Time::new(6, 1)).unwrap();
    assert!(matches!(
        scene.layers[0].content,
        LayerContent::MissingComponent { .. }
    ));
}
