use crate::EDITOR_DEMO_PROJECT;
use crate::audio::{AudioCacheKey, CachedAudioDecoder, take_latest};
use crate::audio_cache::DiskAudioCache;
use crate::editor_launch::{
    Invocation, LaunchError, default_opener, editor_invocation, editor_value,
    parse_login_shell_output, read_until_closing_marker, split_command,
};
use crate::export_worker::{
    ExportEvent, ExportRequest, ExportSource, ExportWorker, export_options,
};
use crate::helpers::{
    ExportLoudness, export_loudness_label, export_output, export_progress_label, export_range_for,
    export_suggested_name, loop_range_for,
};
use crate::layer_select::{
    SourceLookup, display_path, launch_outcome_applies, letterbox, outline_quads, parent_layer_id,
    scene_point, source_lookup, source_lookup_target, source_position,
};
use crate::react_audio::{react_audio_clip_id, react_tracks};
use crate::refresh::{LookupFate, SelectionKeep};
use crate::source::is_react_entry;
use crate::transform_drag::presses_body;
use crate::transform_edit::{
    CommitError, TransformEdit, TransformProp, commit_text, inside_project, keyframe_jump_frames,
    keyframe_timeline_frame, lock_outside_project, reason_text, resolve_targets,
};
use crate::transform_handles::{
    Handle, drag_label, drag_values, handle_at, handles, override_transform, prop_values,
    shown_handles, turn,
};
use crate::transform_rows::field_text;
use crate::waveform::{
    clip_level_envelope, level_at_time, map_clip_waveform, master_volume_from_drag, waveform_peaks,
    waveform_segment,
};
use celesta_editor_core::ClipKind;
use celesta_gpu_renderer::{GpuDriver, LayerFrame};
use celesta_source_edit::{EditTarget, LockReason};

use crate::preview::{
    ReactPreviewContext, ReactPreviewMode, apply_override, collect_component_requests,
    records_hit_regions, splice_component_resolutions, strip_missing_components,
};
use celesta_composition::{
    Animatable, AssetLocation, AudioClip, EvaluatedTransform, GroupMask, Keyframe,
    KeyframeAnimation, KeyframeAnimationType, Layer, LayerContent, LayerEffects, LayerFilter,
    LayerShader, MaskMode, Point, Rational, ResolvedAsset, Scene, ShaderSource, Time, TimeRange,
};
use celesta_editor_core::ClipSummary;
use celesta_exporter::{
    AudioLoudness, ExportCancellation, ExportProgress, LoudnessTarget, VideoFormat,
};
use celesta_media::{AudioBuffer, AudioDecoder};
use celesta_project::{Project, TrackKind};
use celesta_react_bridge::{
    ComponentResolutions, ElementKind, LayerSource, LayerSourceReport, ReceivedProps,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn background_workers_coalesce_queued_requests() {
    let (sender, receiver) = mpsc::channel();
    sender.send(2).unwrap();
    sender.send(3).unwrap();

    assert_eq!(take_latest(1, &receiver), 3);
}

#[test]
fn export_worker_reports_cancellation_without_creating_output() {
    let worker = ExportWorker::spawn().unwrap();
    let cancellation = ExportCancellation::default();
    cancellation.cancel();
    let output = std::env::temp_dir().join(format!(
        "celesta-cancelled-export-{}.mp4",
        std::process::id()
    ));
    let _ = fs::remove_file(&output);
    worker
        .request(ExportRequest {
            source: ExportSource::Project(Box::new(
                Project::from_json(EDITOR_DEMO_PROJECT).unwrap(),
            )),
            asset_root: PathBuf::from("examples"),
            output: output.clone(),
            format: VideoFormat::Mp4,
            range: None,
            loudness: None,
            cancellation,
            driver: GpuDriver::default(),
        })
        .unwrap();

    match worker.events.recv_timeout(Duration::from_secs(2)).unwrap() {
        ExportEvent::Finished {
            result, cancelled, ..
        } => {
            assert!(cancelled);
            assert_eq!(result.unwrap_err(), "export was cancelled");
        }
        ExportEvent::Progress(progress) => panic!("unexpected export progress: {progress:?}"),
    }
    assert!(!output.exists());
}

#[test]
fn export_options_carry_the_chosen_loudness() {
    let request = |loudness: ExportLoudness| ExportRequest {
        source: ExportSource::Project(Box::new(Project::from_json(EDITOR_DEMO_PROJECT).unwrap())),
        asset_root: PathBuf::from("examples"),
        output: PathBuf::from("out.mov"),
        format: VideoFormat::Mov,
        range: None,
        loudness: loudness.target(),
        cancellation: ExportCancellation::default(),
        driver: GpuDriver::default(),
    };

    let off = export_options(&request(ExportLoudness::Off));
    assert_eq!(off.loudness, None);
    assert!(off.overwrite);
    assert_eq!(off.video_format, VideoFormat::Mov);
    assert_eq!(
        export_options(&request(ExportLoudness::Streaming)).loudness,
        Some(LoudnessTarget::STREAMING)
    );
    assert_eq!(
        export_options(&request(ExportLoudness::Broadcast)).loudness,
        Some(LoudnessTarget::BROADCAST)
    );
}

#[test]
fn export_loudness_choices_name_their_targets() {
    assert_eq!(ExportLoudness::default(), ExportLoudness::Off);
    let labels: Vec<_> = ExportLoudness::ALL
        .iter()
        .map(|loudness| loudness.menu_label())
        .collect();
    assert_eq!(
        labels,
        ["Off", "Streaming (-14 LUFS)", "Broadcast (-23 LUFS)"]
    );
}

#[test]
fn open_recognizes_react_entries_but_not_projects() {
    assert!(is_react_entry(PathBuf::from("title.tsx").as_path()));
    assert!(is_react_entry(PathBuf::from("scene.mjs").as_path()));
    assert!(!is_react_entry(
        PathBuf::from("demo.celesta.json").as_path()
    ));
    assert!(!is_react_entry(PathBuf::from("notes.txt").as_path()));
}

#[test]
fn export_range_needs_both_markers_ordered() {
    let rate = Rational::new(30, 1);
    assert_eq!(export_range_for(None, Some(30), rate), None);
    assert_eq!(export_range_for(Some(30), None, rate), None);
    assert_eq!(export_range_for(Some(30), Some(30), rate), None);
    assert_eq!(export_range_for(Some(30), Some(20), rate), None);

    let range = export_range_for(Some(30), Some(90), rate).unwrap();
    assert_eq!(range.start, Time::frames(30, rate).unwrap());
    assert_eq!(range.end, Some(Time::frames(90, rate).unwrap()));
}

#[test]
fn loop_range_falls_back_when_marks_leave_the_composition() {
    assert_eq!(loop_range_for(Some(10), Some(20), 100), (10, 20));
    assert_eq!(loop_range_for(Some(10), Some(200), 100), (10, 100));
    // A reload shortened the composition under both marks.
    assert_eq!(loop_range_for(Some(150), Some(200), 100), (0, 100));
    assert_eq!(loop_range_for(Some(20), Some(10), 100), (0, 100));
    assert_eq!(loop_range_for(Some(10), None, 100), (0, 100));
}

#[test]
fn export_name_replaces_project_extensions() {
    assert_eq!(
        export_suggested_name(
            Some(PathBuf::from("demo.celesta.json").as_path()),
            "ignored"
        ),
        "demo.mp4"
    );
    assert_eq!(export_suggested_name(None, "Untitled"), "Untitled.mp4");
}

#[test]
fn muxing_label_names_no_container() {
    assert_eq!(export_progress_label(&ExportProgress::Muxing), "Muxing…");
}

#[test]
fn the_finished_export_names_its_loudness() {
    let label = |integrated_lufs, true_peak_dbtp| {
        export_loudness_label(&AudioLoudness {
            integrated_lufs,
            true_peak_dbtp,
            gain_db: None,
        })
    };
    assert_eq!(label(Some(-16.24), Some(-0.96)), "-16.2 LUFS, -1.0 dBTP");
    assert_eq!(
        label(None, Some(-3.0)),
        "too short to measure LUFS, -3.0 dBTP"
    );
    assert_eq!(label(None, None), "silent");
}

#[test]
fn export_output_keeps_video_extensions() {
    let case = |path: &str| export_output(PathBuf::from(path));
    assert_eq!(case("a.mov"), (PathBuf::from("a.mov"), VideoFormat::Mov));
    assert_eq!(case("a.WebM"), (PathBuf::from("a.WebM"), VideoFormat::Webm));
    assert_eq!(case("a.mp4"), (PathBuf::from("a.mp4"), VideoFormat::Mp4));
    assert_eq!(case("a"), (PathBuf::from("a.mp4"), VideoFormat::Mp4));
    assert_eq!(case("a.gif"), (PathBuf::from("a.mp4"), VideoFormat::Mp4));
}

#[test]
fn waveform_peaks_are_aligned_and_downsampled_for_clips() {
    let buffer = AudioBuffer {
        sample_rate: 4,
        channels: 2,
        samples: vec![0.1, -0.2, 0.4, -0.3, 0.8, -0.7, 0.2, -0.1],
    };

    let peaks = waveform_peaks(&buffer, 4);
    assert_eq!(peaks, vec![0.2, 0.4, 0.8, 0.2]);
    assert_eq!(waveform_segment(&peaks, 0.5, 0.5, 1), vec![0.8]);
}

#[test]
fn clip_waveforms_follow_source_ranges_and_playback_rate() {
    let mut clip = AudioClip {
        id: "clip".to_owned(),
        asset: ResolvedAsset {
            id: "audio".to_owned(),
            location: AssetLocation::File {
                path: "audio.wav".to_owned(),
            },
        },
        range: TimeRange {
            start: Time::ZERO,
            duration: Time::new(1, 1),
        },
        source_start: Time::new(1, 2),
        source_duration: Some(Time::new(1, 2)),
        playback_rate: Animatable::Static(1.0),
        volume: Animatable::Static(1.0),
        muted: false,
        r#loop: false,
        pitch: Animatable::Static(0.0),
        preserve_pitch: false,
        fade_in: Time::ZERO,
        fade_out: Time::ZERO,
        duck_group: None,
        duck: None,
    };
    let peaks = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];

    assert_eq!(
        map_clip_waveform(&peaks, 8, 4, &clip, 4),
        vec![0.3, 0.4, 0.0, 0.0]
    );

    clip.source_duration = None;
    clip.playback_rate = Animatable::Static(2.0);
    assert_eq!(
        map_clip_waveform(&peaks, 8, 4, &clip, 4),
        vec![0.4, 0.6, 0.8, 0.0]
    );

    // Looping, the source starts over where it ran out.
    clip.r#loop = true;
    assert_eq!(
        map_clip_waveform(&peaks, 8, 4, &clip, 4),
        vec![0.4, 0.6, 0.8, 0.2]
    );
    clip.r#loop = false;

    clip.volume = Animatable::Static(0.5);
    assert_eq!(
        clip_level_envelope(&[0.4, 0.6, 0.8, 0.0], &clip, None),
        vec![0.2, 0.3, 0.4, 0.0]
    );

    // A ducking curve is on the timeline: a clip starting at 10 s dips from
    // 10.4 s on.
    let starting_later = AudioClip {
        range: TimeRange {
            start: Time::new(10, 1),
            duration: Time::new(1, 1),
        },
        ..clip.clone()
    };
    let at = |seconds: f64| Time::new((seconds * 10.0).round() as i64, 10);
    let curve = Animatable::Keyframes(KeyframeAnimation {
        kind: KeyframeAnimationType::Keyframes,
        keyframes: vec![
            Keyframe {
                time: at(10.4),
                value: 1.0,
                easing: None,
            },
            Keyframe {
                time: at(10.4),
                value: 0.5,
                easing: None,
            },
        ],
    });
    assert_eq!(
        clip_level_envelope(&[0.4, 0.6, 0.8, 0.8], &starting_later, Some(&curve)),
        vec![0.2, 0.3, 0.2, 0.2]
    );

    // Fades scale the level as the mixer does.
    clip.fade_in = Time::new(1, 2);
    clip.fade_out = Time::new(1, 2);
    assert_eq!(
        clip_level_envelope(&[0.4, 0.6, 0.8, 0.8], &clip, None),
        vec![0.0, 0.15, 0.4, 0.2]
    );
}

#[test]
fn react_audio_clips_take_one_track_per_file_and_more_only_to_overlap() {
    let clip = |index: usize, src: &str, start: i64, end: i64, muted: bool| AudioClip {
        id: format!("react-audio:{index}"),
        asset: ResolvedAsset {
            id: src.to_owned(),
            location: AssetLocation::File {
                path: format!("/project/{src}"),
            },
        },
        range: TimeRange {
            start: Time::new(start, 1),
            duration: Time::new(end - start, 1),
        },
        source_start: Time::ZERO,
        source_duration: None,
        playback_rate: Animatable::Static(1.0),
        volume: Animatable::Static(0.5),
        muted,
        r#loop: false,
        pitch: Animatable::Static(0.0),
        preserve_pitch: false,
        fade_in: Time::ZERO,
        fade_out: Time::ZERO,
        duck_group: None,
        duck: None,
    };
    let tracks = react_tracks(
        &[
            clip(0, "./voice.wav", 2, 4, false),
            clip(1, "https://example.com/clip.mp4", 0, 6, false),
            clip(2, "./voice.wav", 3, 5, false),
            clip(3, "./voice.wav", 0, 2, true),
        ],
        &HashMap::new(),
    );

    let layout = tracks
        .iter()
        .map(|track| {
            let clips = track
                .clips
                .iter()
                .map(|clip| clip.id.as_str())
                .collect::<Vec<_>>();
            (
                track.id.as_str(),
                track.name.as_str(),
                track.item_count,
                clips,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        layout,
        vec![
            // The two voice clips that overlap take a track each; the muted
            // one before them shares the first.
            (
                "react-audio-track:0",
                "voice.wav",
                2,
                vec!["react-audio:3", "react-audio:0"]
            ),
            ("react-audio-track:1", "voice.wav", 1, vec!["react-audio:2"]),
            ("react-audio-track:2", "clip.mp4", 1, vec!["react-audio:1"]),
        ]
    );
    let muted = &tracks[0].clips[0];
    assert_eq!((muted.kind, muted.enabled), (ClipKind::Audio, false));
    assert_eq!(muted.volume, Some(Animatable::Static(0.5)));
    assert_eq!(
        (muted.start, muted.duration),
        (Time::new(0, 1), Time::new(2, 1))
    );
}

#[test]
fn react_videos_take_video_tracks_and_keep_their_sound_on_audio_tracks() {
    let clip = |index: usize, src: &str, start: i64, end: i64, muted: bool| AudioClip {
        id: format!("react-audio:{index}"),
        asset: ResolvedAsset {
            id: src.to_owned(),
            location: AssetLocation::File {
                path: format!("/project/{src}"),
            },
        },
        range: TimeRange {
            start: Time::new(start, 1),
            duration: Time::new(end - start, 1),
        },
        source_start: Time::ZERO,
        source_duration: None,
        playback_rate: Animatable::Static(1.0),
        volume: Animatable::Static(1.0),
        muted,
        r#loop: false,
        pitch: Animatable::Static(0.0),
        preserve_pitch: false,
        fade_in: Time::ZERO,
        fade_out: Time::ZERO,
        duck_group: None,
        duck: None,
    };
    let videos = HashMap::from([
        ("react-audio:1".to_owned(), true),
        ("react-audio:2".to_owned(), true),
        ("react-audio:3".to_owned(), false),
    ]);
    let tracks = react_tracks(
        &[
            clip(0, "./score.wav", 0, 24, false),
            clip(1, "./film.mp4", 0, 4, true),
            clip(2, "./film.mp4", 2, 6, false),
            clip(3, "./silent.mp4", 0, 2, false),
        ],
        &videos,
    );

    let layout = tracks
        .iter()
        .map(|track| {
            let clips = track
                .clips
                .iter()
                .map(|clip| (clip.id.as_str(), clip.kind, clip.enabled))
                .collect::<Vec<_>>();
            (track.id.as_str(), track.kind, track.name.as_str(), clips)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        layout,
        vec![
            // Pictures first, overlapping ones on a track each; a muted
            // video still shows.
            (
                "react-video-track:0",
                TrackKind::Video,
                "film.mp4",
                vec![("react-audio:1:video", ClipKind::Video, true)]
            ),
            (
                "react-video-track:1",
                TrackKind::Video,
                "film.mp4",
                vec![("react-audio:2:video", ClipKind::Video, true)]
            ),
            (
                "react-video-track:2",
                TrackKind::Video,
                "silent.mp4",
                vec![("react-audio:3:video", ClipKind::Video, true)]
            ),
            // Sound after, without the video that has none.
            (
                "react-audio-track:0",
                TrackKind::Audio,
                "score.wav",
                vec![("react-audio:0", ClipKind::Audio, true)]
            ),
            (
                "react-audio-track:1",
                TrackKind::Audio,
                "film.mp4",
                vec![("react-audio:1", ClipKind::Audio, false)]
            ),
            (
                "react-audio-track:2",
                TrackKind::Audio,
                "film.mp4",
                vec![("react-audio:2", ClipKind::Audio, true)]
            ),
        ]
    );
    assert_eq!(tracks[0].clips[0].volume, None);
    assert_eq!(react_audio_clip_id("react-audio:1:video"), "react-audio:1");
    assert_eq!(react_audio_clip_id("react-audio:1"), "react-audio:1");
}

#[test]
fn track_levels_follow_the_active_clip_and_master_drag_is_clamped() {
    let clip = ClipSummary {
        id: "clip".to_owned(),
        name: "Clip".to_owned(),
        start: Time::new(10, 1),
        duration: Time::new(2, 1),
        kind: ClipKind::Audio,
        enabled: true,
        volume: Some(Animatable::Static(1.0)),
        component: None,
        dialogue: None,
    };

    assert_eq!(level_at_time(&[0.2, 0.8], &clip, Time::new(10, 1)), 0.2);
    assert_eq!(level_at_time(&[0.2, 0.8], &clip, Time::new(11, 1)), 0.8);
    assert_eq!(level_at_time(&[0.2, 0.8], &clip, Time::new(12, 1)), 0.0);
    assert_eq!(master_volume_from_drag(1.0, 22.0), 1.5);
    assert_eq!(master_volume_from_drag(1.0, -100.0), 0.0);
    assert_eq!(master_volume_from_drag(1.0, 100.0), 2.0);
}

#[test]
fn audio_decoder_reuses_session_pcm_cache() {
    let path = PathBuf::from("/does/not/need/to/exist.wav");
    let key = AudioCacheKey {
        path: path.clone(),
        sample_rate: 48_000,
        channels: 2,
    };
    let expected = AudioBuffer {
        sample_rate: 48_000,
        channels: 2,
        samples: vec![0.25, -0.25],
    };
    let mut decoder = CachedAudioDecoder::new();
    decoder.buffers.insert(key, expected.clone());

    let decoded = decoder.decode_audio(&path, 48_000, 2).unwrap();

    assert_eq!(decoded, expected);
}

#[test]
fn audio_decoder_reuses_pcm_cache_across_sessions() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "celesta-editor-disk-cache-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("voice.wav");
    fs::write(&path, b"not valid audio; cache must be used").unwrap();
    let expected = AudioBuffer {
        sample_rate: 48_000,
        channels: 2,
        samples: vec![0.25, -0.25],
    };
    let disk_cache = DiskAudioCache::new(root.join("cache"));
    disk_cache.store(&path, &expected, &[0.25]).unwrap();
    let mut decoder = CachedAudioDecoder::with_disk_cache(disk_cache);

    let decoded = decoder.decode_audio(&path, 48_000, 2).unwrap();

    assert_eq!(decoded, expected);
    assert_eq!(decoder.waveforms.values().next().unwrap(), &[0.25]);
    fs::remove_dir_all(root).unwrap();
}

fn missing(name: &str) -> Layer {
    Layer {
        id: name.to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: Default::default(),
        effects: Default::default(),
        content: LayerContent::MissingComponent {
            component: name.to_owned(),
            props: Default::default(),
        },
    }
}

fn masked_group(children: Vec<Layer>, mask: Vec<Layer>) -> Layer {
    Layer {
        content: LayerContent::Group {
            layers: children,
            clip: None,
            mask: Some(GroupMask {
                layers: mask,
                mode: MaskMode::Alpha,
                invert: false,
            }),
            motion_blur: false,
        },
        ..missing("group")
    }
}

fn scene_of(layers: Vec<Layer>) -> Scene {
    Scene {
        width: 64,
        height: 64,
        frame_rate: Rational::new(30, 1),
        time: Time::default(),
        fonts: Vec::new(),
        shaders: Vec::new(),
        layers,
    }
}

#[test]
fn an_override_replaces_the_transform_of_a_nested_layer() {
    let inner = Layer {
        content: LayerContent::Group {
            layers: vec![missing("target"), missing("other")],
            clip: None,
            mask: None,
            motion_blur: false,
        },
        ..missing("group")
    };
    let mut scene = scene_of(vec![missing("first"), inner]);
    let moved = EvaluatedTransform {
        rotation: 30.0,
        ..EvaluatedTransform::default()
    };

    assert!(apply_override(&mut scene, "target", &moved));

    let LayerContent::Group { layers, .. } = &scene.layers[1].content else {
        unreachable!()
    };
    assert_eq!(layers[0].transform, moved);
    assert_eq!(layers[1].transform, EvaluatedTransform::default());
    assert_eq!(scene.layers[0].transform, EvaluatedTransform::default());
}

#[test]
fn an_override_of_an_absent_layer_changes_nothing() {
    let mut scene = scene_of(vec![missing("a")]);
    let moved = EvaluatedTransform {
        rotation: 30.0,
        ..EvaluatedTransform::default()
    };

    assert!(!apply_override(&mut scene, "b", &moved));
    assert_eq!(scene, scene_of(vec![missing("a")]));
}

#[test]
fn component_requests_reach_into_masks_after_the_children() {
    let layers = vec![masked_group(vec![missing("Child")], vec![missing("Matte")])];
    let mut requests = Vec::new();
    collect_component_requests(&layers, &mut requests);
    let names: Vec<_> = requests.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["Child", "Matte"]);
}

#[test]
fn strips_missing_components_inside_masks() {
    let mut layers = vec![masked_group(vec![missing("Child")], vec![missing("Matte")])];
    strip_missing_components(&mut layers);
    let LayerContent::Group {
        layers: children,
        mask: Some(mask),
        ..
    } = &layers[0].content
    else {
        panic!("the group lost its mask");
    };
    assert!(children.is_empty());
    assert!(mask.layers.is_empty());
}

fn shader(id: &str) -> ShaderSource {
    ShaderSource {
        id: id.to_owned(),
        name: None,
        wgsl: format!("fn effect(input: EffectInput) -> vec4f {{ return vec4f(0.0); }} // {id}"),
        params: Vec::new(),
    }
}

fn shaded(id: &str) -> Layer {
    Layer {
        effects: LayerEffects {
            filters: vec![LayerFilter::Shader(LayerShader {
                id: id.to_owned(),
                params: Vec::new(),
                padding: 0.0,
            })],
            ..Default::default()
        },
        content: LayerContent::Group {
            layers: Vec::new(),
            clip: None,
            mask: None,
            motion_blur: false,
        },
        ..missing(id)
    }
}

#[test]
fn splicing_component_resolutions_lists_their_shaders_once() {
    let mut scene = Scene {
        width: 16,
        height: 16,
        frame_rate: Rational::new(30, 1),
        time: Time::new(0, 30),
        fonts: Vec::new(),
        shaders: vec![shader("listed")],
        layers: vec![
            missing("Ripple"),
            masked_group(vec![missing("Unregistered")], vec![missing("Matte")]),
        ],
    };
    let unresolved = splice_component_resolutions(
        &mut scene,
        ComponentResolutions {
            components: vec![
                Some(vec![shaded("ripple"), shaded("listed")]),
                None,
                Some(vec![shaded("ripple")]),
            ],
            shaders: vec![shader("ripple"), shader("listed")],
        },
    );

    assert_eq!(unresolved, ["Unregistered"]);
    let ids: Vec<_> = scene
        .shaders
        .iter()
        .map(|shader| shader.id.as_str())
        .collect();
    assert_eq!(ids, ["listed", "ripple"]);
    let LayerContent::Group { layers, .. } = &scene.layers[0].content else {
        panic!("the component did not resolve to a group");
    };
    assert_eq!(layers.len(), 2);
    let LayerContent::Group {
        layers: children,
        mask: Some(mask),
        ..
    } = &scene.layers[1].content
    else {
        panic!("the group lost its mask");
    };
    assert!(children.is_empty());
    assert!(
        matches!(&mask.layers[0].content, LayerContent::Group { layers, .. } if layers.len() == 1)
    );
}

fn layer_source_at(file: &str, line: u32, column: u32) -> LayerSource {
    LayerSource {
        name: "Title".to_string(),
        file: PathBuf::from(file),
        line,
        column,
        kind: ElementKind::Component,
        layer_id: None,
    }
}

fn report_of(chain: Vec<LayerSource>) -> LayerSourceReport {
    LayerSourceReport {
        chain,
        local_frame: 0.0,
        candidate_frames: vec![],
        props: ReceivedProps::default(),
        render_count: 1,
        windows: vec![],
        span: None,
    }
}

fn invocation(program: &str, args: &[&str]) -> Invocation {
    Invocation {
        program: program.to_string(),
        args: args.iter().map(|arg| arg.to_string()).collect(),
    }
}

#[test]
fn editor_value_prefers_visual_over_editor() {
    let env = |name: &str| match name {
        "VISUAL" => Some("code".to_string()),
        "EDITOR" => Some("vim".to_string()),
        _ => None,
    };

    assert_eq!(editor_value(env), Some("code".to_string()));
}

#[test]
fn editor_value_falls_back_to_editor_when_visual_is_empty() {
    let env = |name: &str| match name {
        "VISUAL" => Some(String::new()),
        "EDITOR" => Some("zed".to_string()),
        _ => None,
    };

    assert_eq!(editor_value(env), Some("zed".to_string()));
}

#[test]
fn editor_value_is_none_when_both_are_unset_or_empty() {
    assert_eq!(editor_value(|_| None), None);
    assert_eq!(editor_value(|_| Some(String::new())), None);
    assert_eq!(editor_value(|_| Some("  ".to_string())), None);
}

#[test]
fn editor_invocation_passes_the_position_the_way_each_editor_expects() {
    let source = layer_source_at("/p/a.tsx", 12, 5);
    let goto = ["--goto", "/p/a.tsx:12:5"];
    let cases: Vec<(&str, Invocation)> = vec![
        ("code", invocation("code", &goto)),
        (
            "code -n",
            invocation("code", &["-n", "--goto", "/p/a.tsx:12:5"]),
        ),
        ("cursor", invocation("cursor", &goto)),
        ("windsurf", invocation("windsurf", &goto)),
        ("codium", invocation("codium", &goto)),
        ("code-insiders", invocation("code-insiders", &goto)),
        ("zed", invocation("zed", &["/p/a.tsx:12:5"])),
        ("subl", invocation("subl", &["/p/a.tsx:12:5"])),
        (
            "sublime_text",
            invocation("sublime_text", &["/p/a.tsx:12:5"]),
        ),
        (
            "idea",
            invocation("idea", &["--line", "12", "--column", "5", "/p/a.tsx"]),
        ),
        (
            "rustrover",
            invocation("rustrover", &["--line", "12", "--column", "5", "/p/a.tsx"]),
        ),
        ("mate", invocation("mate", &["--line", "12", "/p/a.tsx"])),
        ("emacs", invocation("emacs", &["+12", "/p/a.tsx"])),
        ("gedit", invocation("gedit", &["+12", "/p/a.tsx"])),
        (
            "emacsclient -c",
            invocation("emacsclient", &["-c", "+12", "/p/a.tsx"]),
        ),
    ];

    for (value, expected) in cases {
        assert_eq!(
            editor_invocation(value, &source),
            Ok(expected),
            "value {value:?}"
        );
    }
}

#[test]
fn editor_invocation_keeps_a_quoted_program_path_with_spaces() {
    let source = layer_source_at("/p/a.tsx", 12, 5);

    assert_eq!(
        editor_invocation(
            "'/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl'",
            &source
        ),
        Ok(invocation(
            "/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl",
            &["/p/a.tsx:12:5"]
        ))
    );
}

#[test]
fn editor_invocation_matches_a_windows_program_path_without_extension() {
    let source = layer_source_at("/p/a.tsx", 12, 5);

    assert_eq!(
        editor_invocation("\"C:\\Tools\\Code.exe\"", &source),
        Ok(invocation(
            "C:\\Tools\\Code.exe",
            &["--goto", "/p/a.tsx:12:5"]
        ))
    );
    assert_eq!(
        split_command("C:\\Tools\\Code.exe --wait", true),
        Some(vec![
            "C:\\Tools\\Code.exe".to_string(),
            "--wait".to_string()
        ])
    );
    assert_eq!(
        split_command("\"C:\\Program Files\\Code.exe\" -n", true),
        Some(vec![
            "C:\\Program Files\\Code.exe".to_string(),
            "-n".to_string()
        ])
    );
}

#[test]
fn editor_invocation_refuses_terminal_editors() {
    let source = layer_source_at("/p/a.tsx", 12, 5);
    let cases = [
        ("vim", "vim"),
        ("nvim", "nvim"),
        ("nano", "nano"),
        ("hx", "hx"),
        ("/usr/bin/vim", "vim"),
        ("emacs -nw", "emacs"),
        ("emacsclient -t", "emacsclient"),
        ("emacs --tty", "emacs"),
    ];

    for (value, name) in cases {
        assert_eq!(
            editor_invocation(value, &source),
            Err(LaunchError::TerminalEditor(name.to_string())),
            "value {value:?}"
        );
    }
}

#[test]
fn editor_invocation_rejects_an_unbalanced_quote() {
    let source = layer_source_at("/p/a.tsx", 12, 5);

    assert_eq!(
        editor_invocation("code '--goto", &source),
        Err(LaunchError::Unparsable("code '--goto".to_string()))
    );
    assert!(matches!(
        editor_invocation("  ", &source),
        Err(LaunchError::Unparsable(_))
    ));
}

#[test]
fn parse_login_shell_output_reads_both_variables_between_the_markers() {
    let output = "motd noise\n__CELESTA_EDITOR__vim\0code__CELESTA_EDITOR__\ntrailing";
    assert_eq!(parse_login_shell_output(output), Some("vim".to_string()));

    let empty_visual = "banner\n__CELESTA_EDITOR__\0code__CELESTA_EDITOR__\n";
    assert_eq!(
        parse_login_shell_output(empty_visual),
        Some("code".to_string())
    );
}

#[test]
fn parse_login_shell_output_is_none_without_a_value_or_markers() {
    assert_eq!(parse_login_shell_output("no markers here\n"), None);
    assert_eq!(
        parse_login_shell_output("__CELESTA_EDITOR__\0__CELESTA_EDITOR__\n"),
        None
    );
    assert_eq!(parse_login_shell_output("__CELESTA_EDITOR__code"), None);
}

#[test]
fn default_opener_uses_the_os_default_application() {
    let expected = if cfg!(target_os = "macos") {
        invocation("open", &["/p/a.tsx"])
    } else if cfg!(windows) {
        invocation("cmd", &["/C", "start", "", "/p/a.tsx"])
    } else {
        invocation("xdg-open", &["/p/a.tsx"])
    };

    assert_eq!(default_opener(Path::new("/p/a.tsx")), expected);
}

#[test]
fn launch_error_displays_the_panel_copy() {
    assert_eq!(
        LaunchError::TerminalEditor("vim".to_string()).to_string(),
        "VISUAL/EDITOR is a terminal editor (vim); set VISUAL to a graphical editor"
    );
    assert_eq!(
        LaunchError::Unparsable("code '--goto".to_string()).to_string(),
        "VISUAL/EDITOR could not be parsed: code '--goto"
    );
    assert_eq!(
        LaunchError::Spawn("No such file or directory".to_string()).to_string(),
        "Could not open the editor: No such file or directory"
    );
}

/// Yields `chunks`, then panics: a read past them means the caller waited for
/// an end of stream that a daemon holding the pipe would never deliver.
struct ChunksThenForever {
    chunks: std::vec::IntoIter<&'static [u8]>,
}

impl std::io::Read for ChunksThenForever {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let chunk = self.chunks.next().expect("read past the closing marker");
        buf[..chunk.len()].copy_from_slice(chunk);
        Ok(chunk.len())
    }
}

#[test]
fn reading_login_shell_output_stops_at_the_closing_marker_without_eof() {
    let reader = ChunksThenForever {
        chunks: vec![
            b"noise\n__CELESTA_EDI".as_slice(),
            b"TOR__code\0vim__CELESTA_EDITOR__\n".as_slice(),
        ]
        .into_iter(),
    };

    let output = read_until_closing_marker(reader);

    assert_eq!(parse_login_shell_output(&output), Some("code".to_string()));
}

#[test]
fn reading_login_shell_output_returns_what_it_got_at_eof() {
    let output = read_until_closing_marker(&b"only noise"[..]);

    assert_eq!(output, "only noise");
    assert_eq!(parse_login_shell_output(&output), None);
}

/// The viewer area the layer selection tests click in: 960 x 960 at the origin.
fn square_viewer() -> gpui_kit::Bounds<gpui_kit::Pixels> {
    gpui_kit::Bounds::new(
        gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(0.0)),
        gpui_kit::size(gpui_kit::px(960.0), gpui_kit::px(960.0)),
    )
}

#[test]
fn layer_select_letterboxes_a_wide_frame_in_a_square_area() {
    assert_eq!(
        letterbox(square_viewer(), (1920.0, 1080.0)),
        Some((0.0, 210.0, 0.5))
    );
}

#[test]
fn layer_select_letterboxes_nothing_for_a_frame_without_size() {
    assert_eq!(letterbox(square_viewer(), (0.0, 1080.0)), None);
    assert_eq!(letterbox(square_viewer(), (1920.0, 0.0)), None);
}

#[test]
fn layer_select_maps_a_click_in_the_frame_to_scene_pixels() {
    let point = scene_point(
        square_viewer(),
        (1920.0, 1080.0),
        gpui_kit::point(gpui_kit::px(480.0), gpui_kit::px(480.0)),
    );

    assert_eq!(
        point,
        Some(celesta_composition::Point { x: 960.0, y: 540.0 })
    );
}

#[test]
fn layer_select_maps_a_click_in_the_letterbox_bar_to_nothing() {
    let point = scene_point(
        square_viewer(),
        (1920.0, 1080.0),
        gpui_kit::point(gpui_kit::px(480.0), gpui_kit::px(100.0)),
    );

    assert_eq!(point, None);
}

#[test]
fn layer_select_reads_each_lookup_outcome() {
    let chain = vec![layer_source_at("/project/title.tsx", 12, 5)];

    assert_eq!(source_lookup(Ok(None)), SourceLookup::NotInFrame);
    assert_eq!(
        source_lookup(Ok(Some(report_of(Vec::new())))),
        SourceLookup::NoPosition
    );
    assert_eq!(
        source_lookup(Ok(Some(report_of(chain.clone())))),
        SourceLookup::Found(report_of(chain))
    );
    assert_eq!(
        source_lookup(Err("bridge closed".to_string())),
        SourceLookup::Failed("bridge closed".to_string())
    );
}

#[test]
fn layer_select_shows_a_file_inside_the_entry_directory_relative_to_it() {
    let entry_dir = Path::new("/project/src");
    // Joined, so the separators are the platform's, as the runtime's are.
    let file = entry_dir.join("components").join("title.tsx");
    let source = layer_source_at(file.to_str().expect("an ASCII path"), 12, 5);

    assert_eq!(
        display_path(&source.file, entry_dir),
        Path::new("components")
            .join("title.tsx")
            .display()
            .to_string()
    );
    assert_eq!(
        source_position(&source, entry_dir),
        format!(
            "{}:12:5",
            Path::new("components").join("title.tsx").display()
        )
    );
}

#[test]
fn layer_select_shows_a_launch_error_only_under_the_layer_it_opened() {
    let started = (3, Some("title"));

    assert!(launch_outcome_applies(started, (3, Some("title"))));
    assert!(!launch_outcome_applies(started, (3, Some("subtitle"))));
    assert!(!launch_outcome_applies(started, (3, None)));
    assert!(!launch_outcome_applies(started, (4, Some("title"))));
}

#[test]
fn layer_select_shows_a_file_outside_the_entry_directory_in_full() {
    let entry_dir = Path::new("/project/src");
    let file = Path::new("/project/shared/title.tsx");

    assert_eq!(display_path(file, entry_dir), file.display().to_string());
}

fn react_context(source_locations: bool) -> ReactPreviewContext {
    ReactPreviewContext {
        composition_id: None,
        node: PathBuf::from("node"),
        cli_script: PathBuf::from("cli.js"),
        entry: PathBuf::from("entry.tsx"),
        properties: celesta_react_bridge::PropertyInputs::default(),
        source_locations,
    }
}

#[test]
fn layer_select_respawns_the_bridge_when_the_mode_changes() {
    // `ensure_react_bridge` respawns Node whenever the context differs.
    assert_ne!(react_context(true), react_context(false));
}

#[test]
fn layer_select_records_hit_regions_only_in_interactive_mode() {
    let interactive = react_context(true);
    let preview = react_context(false);

    assert!(records_hit_regions(
        ReactPreviewMode::WholeScene,
        Some(&interactive)
    ));
    assert!(!records_hit_regions(
        ReactPreviewMode::WholeScene,
        Some(&preview)
    ));
    assert!(!records_hit_regions(
        ReactPreviewMode::ResolveComponents,
        Some(&interactive)
    ));
    assert!(!records_hit_regions(ReactPreviewMode::WholeScene, None));
}

#[test]
fn layer_select_looks_up_sources_only_in_interactive_mode() {
    let found = SourceLookup::Found(report_of(Vec::new()));

    // Paused with a selection: looked up while picking, never otherwise.
    assert_eq!(
        source_lookup_target(Some("root.2"), true, false, Some(&found)),
        Some("root.2".to_string())
    );
    assert_eq!(
        source_lookup_target(Some("root.2"), false, false, Some(&found)),
        None
    );
    // Playing: only a selection still waiting for its first answer.
    assert_eq!(
        source_lookup_target(Some("root.2"), true, true, Some(&SourceLookup::Pending)),
        Some("root.2".to_string())
    );
    assert_eq!(
        source_lookup_target(Some("root.2"), true, true, Some(&found)),
        None
    );
    assert_eq!(source_lookup_target(None, true, false, None), None);
}

const TRANSFORM_FIXTURE: &str = "export const Scene = () => (\n  <Rect x={100} width={10} />\n);\n";

fn primitive_report(line: u32, column: u32, props: ReceivedProps) -> LayerSourceReport {
    LayerSourceReport {
        chain: vec![LayerSource {
            name: "Rect".to_string(),
            file: PathBuf::from("/project/scene.tsx"),
            line,
            column,
            kind: ElementKind::Primitive,
            layer_id: None,
        }],
        local_frame: 0.0,
        candidate_frames: vec![],
        props,
        render_count: 1,
        windows: vec![],
        span: None,
    }
}

fn x_only() -> ReceivedProps {
    ReceivedProps {
        x: Some(100.0),
        ..ReceivedProps::default()
    }
}

#[test]
fn transform_resolve_targets_prefers_scale_xy() {
    let text = "export const Scene = () => (\n  <Rect x={1} scaleX={2} />\n);\n";
    let report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(1.0),
            scale_x: Some(2.0),
            ..ReceivedProps::default()
        },
    );

    let targets = resolve_targets(text, &report).expect("the element resolves");

    let props: Vec<_> = targets.targets.iter().map(|(prop, _)| *prop).collect();
    assert_eq!(
        props,
        [
            TransformProp::X,
            TransformProp::Y,
            TransformProp::Rotation,
            TransformProp::ScaleX,
            TransformProp::ScaleY
        ]
    );
    assert!(matches!(
        targets.targets[3].1,
        EditTarget::Literal { value, .. } if value == 2.0
    ));
    assert!(matches!(
        targets.targets[4].1,
        EditTarget::Missing { default, .. } if default == 1.0
    ));
    assert_eq!(targets.file, PathBuf::from("/project/scene.tsx"));
}

#[test]
fn transform_resolve_targets_uniform_scale() {
    let targets = resolve_targets(TRANSFORM_FIXTURE, &primitive_report(2, 3, x_only()))
        .expect("the element resolves");

    let props: Vec<_> = targets.targets.iter().map(|(prop, _)| *prop).collect();
    assert_eq!(
        props,
        [
            TransformProp::X,
            TransformProp::Y,
            TransformProp::Rotation,
            TransformProp::Scale
        ]
    );
    assert_eq!(TransformProp::ScaleX.name(), "scaleX");
    assert!(matches!(
        targets.targets[0].1,
        EditTarget::Literal { value, .. } if value == 100.0
    ));
}

#[test]
fn transform_resolve_targets_fails_without_the_element() {
    assert!(resolve_targets(TRANSFORM_FIXTURE, &primitive_report(1, 1, x_only())).is_err());
    assert!(resolve_targets(TRANSFORM_FIXTURE, &report_of(Vec::new())).is_err());
}

#[test]
fn transform_commit_writes_literal_and_inserts() {
    let report = primitive_report(2, 3, x_only());
    let expected = resolve_targets(TRANSFORM_FIXTURE, &report).unwrap();

    let written = commit_text(
        TRANSFORM_FIXTURE,
        &expected,
        &report,
        &[(TransformProp::X, 120.0), (TransformProp::Rotation, 45.5)],
    )
    .expect("both targets are editable");

    assert_eq!(
        written,
        "export const Scene = () => (\n  <Rect x={120} width={10} rotation={45.5} />\n);\n"
    );
}

#[test]
fn transform_commit_aborts_when_target_moved() {
    let report = primitive_report(2, 3, x_only());
    let expected = resolve_targets(TRANSFORM_FIXTURE, &report).unwrap();
    // The element moved within its line, so the report's position misses it.
    let moved = TRANSFORM_FIXTURE.replace("  <Rect", "    <Rect");
    // The element stayed but its `x` became longer, so the span moved.
    let widened = TRANSFORM_FIXTURE.replace("x={100}", "x={1000}");

    for text in [moved, widened] {
        let error = commit_text(&text, &expected, &report, &[(TransformProp::X, 120.0)])
            .expect_err("the source changed");
        assert_eq!(error, CommitError::SourceChanged);
        assert_eq!(error.to_string(), "The source changed during the edit");
    }
}

#[test]
fn transform_commit_refuses_locked_and_non_finite_values() {
    let report = primitive_report(2, 3, x_only());
    let expected = resolve_targets(TRANSFORM_FIXTURE, &report).unwrap();

    assert!(matches!(
        commit_text(
            TRANSFORM_FIXTURE,
            &expected,
            &report,
            &[(TransformProp::X, f64::NAN)]
        ),
        Err(CommitError::NotANumber)
    ));
    // `scaleX` is not a target of a uniformly scaled element.
    assert!(matches!(
        commit_text(
            TRANSFORM_FIXTURE,
            &expected,
            &report,
            &[(TransformProp::ScaleX, 2.0)]
        ),
        Err(CommitError::NotEditable(TransformProp::ScaleX))
    ));
}

#[test]
fn transform_reason_texts_match_spec() {
    let cases = [
        (LockReason::SyntaxError, "The source has syntax errors"),
        (
            LockReason::ElementNotFound,
            "Couldn't find this element in the source",
        ),
        (
            LockReason::NotPrimitive,
            "Set by a component, not written on this element",
        ),
        (
            LockReason::Expression {
                source: "W / 2".to_string(),
            },
            "Written as an expression: W / 2",
        ),
        (
            LockReason::UnresolvedIdentifier {
                name: "name".to_string(),
            },
            "Comes from name, which can't be traced",
        ),
        (
            LockReason::SharedBinding {
                name: "enter".to_string(),
                uses: 3,
            },
            "enter is used in 3 places",
        ),
        (
            LockReason::BetweenKeyframes {
                previous: 0.0,
                next: 30.0,
                frame: 15.0,
            },
            "Between keyframes at 0 and 30",
        ),
        (LockReason::OutsideKeyframes, "Outside the keyframes"),
        (
            LockReason::UnsupportedCall {
                callee: "spring".to_string(),
            },
            "Computed by spring()",
        ),
        (
            LockReason::OverriddenByParent,
            "A layout component sets this value",
        ),
        (LockReason::Spread, "May be set by a spread ({...props})"),
        (
            LockReason::Repeated { count: 12 },
            "Rendered 12 times from this element",
        ),
        (
            LockReason::FrameUncertain,
            "Can't tell which frame this animation reads",
        ),
        (
            LockReason::OutsideProject,
            "This file is outside the project",
        ),
    ];
    for (reason, text) in cases {
        assert_eq!(reason_text(&reason), text);
    }
}

#[test]
fn transform_own_write_reload_keeps_selection() {
    let mut keep = SelectionKeep::default();

    // A reload requested after the editor's own write keeps the selection.
    keep.own_write();
    assert!(keep.requested(1, true));
    assert!(keep.applies(1));
    assert!(!keep.applies(1), "each kept reload applies once");

    // A reload from an edit elsewhere clears it.
    assert!(!keep.requested(2, true));
    assert!(!keep.applies(2));

    // Switching compositions never keeps it.
    keep.own_write();
    assert!(!keep.requested(3, false));
    assert!(!keep.applies(3));
}

#[test]
fn transform_kept_selection_waits_for_the_reloaded_frame() {
    let mut keep = SelectionKeep::default();
    keep.confirm_from(7);

    // A frame of the old code says nothing about the new one.
    assert_eq!(keep.lookup_arrived(6, true), LookupFate::Stale);
    // The reloaded code no longer draws the layer.
    assert_eq!(keep.lookup_arrived(7, true), LookupFate::Gone);
    // After the first answer, lookups show as usual.
    assert_eq!(keep.lookup_arrived(8, true), LookupFate::Show);

    keep.confirm_from(9);
    assert_eq!(keep.lookup_arrived(9, false), LookupFate::Show);
    assert_eq!(keep.lookup_arrived(10, true), LookupFate::Show);
}

#[test]
fn transform_state_drops_stale_resolutions() {
    let report = primitive_report(2, 3, x_only());
    let moved = primitive_report(2, 3, ReceivedProps::default());
    let targets = || resolve_targets(TRANSFORM_FIXTURE, &report);
    let mut state = TransformEdit::default();

    let first = state.begin("a", &report, 0).expect("nothing resolved yet");
    assert_eq!(state.begin("a", &report, 0), None, "already resolving it");
    assert!(state.finish(first, Some("a"), targets()));
    let lookup = Some((&report, 0));
    assert!(state.targets(Some("a"), lookup).is_some());
    assert!(
        state.targets(Some("b"), lookup).is_none(),
        "belongs to layer a"
    );

    // After a write the targets are unknown until resolved again.
    state.invalidate();
    assert!(state.targets(Some("a"), lookup).is_none());
    // The old report describes the old code; wait for the reload's lookup.
    assert_eq!(state.begin("a", &report, 0), None);
    state.reset();
    let second = state
        .begin("a", &report, 0)
        .expect("the reload's lookup resolves again");

    // A newer report supersedes the one in flight.
    let third = state.begin("a", &moved, 0).expect("another report");
    assert!(!state.finish(second, Some("a"), targets()));
    // The selection changed while it resolved.
    assert!(!state.finish(third, Some("b"), targets()));
    assert!(state.targets(Some("a"), Some((&moved, 0))).is_none());
}

#[test]
fn transform_resolve_targets_ignores_a_scale_axis_only_a_parent_sets() {
    // A parent passes `scaleX`, but the element writes neither axis.
    let report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(100.0),
            scale_x: Some(2.0),
            ..ReceivedProps::default()
        },
    );

    let targets = resolve_targets(TRANSFORM_FIXTURE, &report).expect("the element resolves");

    let props: Vec<_> = targets.targets.iter().map(|(prop, _)| *prop).collect();
    assert_eq!(
        props,
        [
            TransformProp::X,
            TransformProp::Y,
            TransformProp::Rotation,
            TransformProp::Scale
        ]
    );
}

#[test]
fn transform_reset_resolves_an_identical_report_again() {
    // `<Rect width={10} x={100} />` became `width={100}` elsewhere: the report
    // is the same, but `x` moved.
    let before = "export const Scene = () => (\n  <Rect width={10} x={100} />\n);\n";
    let after = before.replace("width={10}", "width={100}");
    let report = primitive_report(2, 3, x_only());
    let mut state = TransformEdit::default();

    let first = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(first, Some("a"), resolve_targets(before, &report)));
    state.set_error(
        Some("a"),
        Some("The source changed during the edit".to_owned()),
    );

    // The reload cleared the selection; selecting the layer again.
    state.reset();
    assert_eq!(state.error(Some("a")), None, "the old error is gone");
    let second = state.begin("a", &report, 0).expect("resolved again");
    assert!(state.finish(second, Some("a"), resolve_targets(&after, &report)));

    let (targets, report) = state.targets(Some("a"), Some((&report, 0))).unwrap();
    let written = commit_text(&after, targets, report, &[(TransformProp::X, 120.0)]).unwrap();
    assert_eq!(written, after.replace("x={100}", "x={120}"));
}

#[test]
fn transform_explains_why_there_is_nothing_to_commit() {
    let report = primitive_report(2, 3, x_only());
    let mut state = TransformEdit::default();
    assert_eq!(
        state.unavailable(Some("a")),
        "The source is still being read"
    );

    let request = state.begin("a", &report, 0).unwrap();
    assert_eq!(
        state.unavailable(Some("a")),
        "The source is still being read"
    );
    assert!(state.finish(
        request,
        Some("a"),
        Err("Couldn't read scene.tsx".to_owned())
    ));
    assert_eq!(state.unavailable(Some("a")), "Couldn't read scene.tsx");

    let request = state.begin("a", &report, 1).unwrap();
    assert!(state.finish(
        request,
        Some("a"),
        resolve_targets(TRANSFORM_FIXTURE, &report)
    ));
    state.invalidate();
    assert_eq!(state.unavailable(Some("a")), "Select the layer again");
}

#[test]
fn transform_keyframe_jumps_use_the_lookup_frame() {
    // Looked up at timeline frame 100 inside a sequence starting at 40.
    assert_eq!(keyframe_timeline_frame(0.0, 100, 60.0), 40);
    assert_eq!(keyframe_timeline_frame(90.0, 100, 60.0), 130);
    // Not shifted by a sequence.
    assert_eq!(keyframe_timeline_frame(30.0, 12, 12.0), 30);
}

#[test]
fn transform_keyframe_jumps_use_the_candidate_frame() {
    // The report's owner guess is 60, but `frame` reads the Sequence's 30
    // (frame 15 of [0, 30] gives the received 250, frame 60 would be outside).
    let text = "export const Scene = () => {\n  const frame = useCurrentFrame();\n  return <Sequence from={30}><Rect x={interpolate(frame - 15, [0, 30], [100, 400])} /></Sequence>;\n};\n";
    let report = LayerSourceReport {
        local_frame: 60.0,
        candidate_frames: vec![60.0, 30.0],
        ..primitive_report(
            3,
            30,
            ReceivedProps {
                x: Some(250.0),
                ..ReceivedProps::default()
            },
        )
    };
    let targets = resolve_targets(text, &report).unwrap();
    let target = targets.target(TransformProp::X).unwrap();
    assert_eq!(
        target,
        &EditTarget::Locked(LockReason::BetweenKeyframes {
            previous: 15.0,
            next: 45.0,
            frame: 30.0,
        })
    );
    // Looked up at timeline frame 60: keyframes 15 and 45 of the Sequence's
    // frames are timeline frames 45 and 75.
    assert_eq!(keyframe_jump_frames(target, 60), Some((45, 75)));
    assert_eq!(
        keyframe_jump_frames(&EditTarget::Locked(LockReason::OutsideKeyframes), 60),
        None
    );
}

/// A 100x50 layer at `position` with its anchor at the top left (React's
/// default), under `parent`, unrotated and unscaled.
fn drag_frame(parent: [f64; 6], position: Point) -> LayerFrame {
    let corner = |x: f64, y: f64| Point {
        x: parent[0] * (position.x + x) + parent[2] * (position.y + y) + parent[4],
        y: parent[1] * (position.x + x) + parent[3] * (position.y + y) + parent[5],
    };
    LayerFrame {
        transform: EvaluatedTransform {
            position,
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        parent,
        outline: [
            corner(0.0, 0.0),
            corner(100.0, 0.0),
            corner(100.0, 50.0),
            corner(0.0, 50.0),
        ],
        leaves: vec!["a".to_owned()],
    }
}

const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
const UNIFORM: [TransformProp; 4] = [
    TransformProp::X,
    TransformProp::Y,
    TransformProp::Rotation,
    TransformProp::Scale,
];
const PER_AXIS: [TransformProp; 5] = [
    TransformProp::X,
    TransformProp::Y,
    TransformProp::Rotation,
    TransformProp::ScaleX,
    TransformProp::ScaleY,
];

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn assert_values(actual: &[(TransformProp, f64)], expected: &[(TransformProp, f64)]) {
    assert_eq!(
        actual.iter().map(|(prop, _)| *prop).collect::<Vec<_>>(),
        expected.iter().map(|(prop, _)| *prop).collect::<Vec<_>>(),
        "{actual:?}"
    );
    for ((_, a), (_, e)) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 1e-9, "{actual:?} != {expected:?}");
    }
}

#[test]
fn drag_values_move_inside_a_parent_scaled_by_two_halves_the_delta() {
    let frame = drag_frame([2.0, 0.0, 0.0, 2.0, 30.0, 40.0], pt(10.0, 20.0));
    let values = drag_values(
        &frame,
        Handle::Move,
        (pt(100.0, 100.0), pt(140.0, 160.0)),
        false,
        0.0,
        &UNIFORM,
    );
    assert_values(
        &values,
        &[(TransformProp::X, 30.0), (TransformProp::Y, 50.0)],
    );
}

#[test]
fn drag_values_move_with_y_locked_changes_only_x() {
    let frame = drag_frame(IDENTITY, pt(10.0, 20.0));
    let editable = [
        TransformProp::X,
        TransformProp::Rotation,
        TransformProp::Scale,
    ];
    let values = drag_values(
        &frame,
        Handle::Move,
        (pt(0.0, 0.0), pt(5.0, 7.0)),
        false,
        0.0,
        &editable,
    );
    assert_values(&values, &[(TransformProp::X, 15.0)]);
}

#[test]
fn drag_values_rotate_turns_about_the_anchor_and_snaps_with_shift() {
    let frame = drag_frame(IDENTITY, pt(100.0, 100.0));
    let angle = 37f64.to_radians();
    let now = pt(100.0 + 50.0 * angle.cos(), 100.0 + 50.0 * angle.sin());
    let free = drag_values(
        &frame,
        Handle::Rotate,
        (pt(200.0, 100.0), now),
        false,
        0.0,
        &UNIFORM,
    );
    assert_values(&free, &[(TransformProp::Rotation, 37.0)]);
    let snapped = drag_values(
        &frame,
        Handle::Rotate,
        (pt(200.0, 100.0), now),
        true,
        0.0,
        &UNIFORM,
    );
    assert_values(&snapped, &[(TransformProp::Rotation, 30.0)]);
}

#[test]
fn drag_values_rotate_snaps_the_resulting_angle() {
    let mut frame = drag_frame(IDENTITY, pt(0.0, 0.0));
    frame.transform.rotation = 10.0;
    // +24° from 10° is 34°, which snaps to 30°.
    let angle = 24f64.to_radians();
    let now = pt(10.0 * angle.cos(), 10.0 * angle.sin());
    let values = drag_values(
        &frame,
        Handle::Rotate,
        (pt(10.0, 0.0), now),
        true,
        0.0,
        &UNIFORM,
    );
    assert_values(&values, &[(TransformProp::Rotation, 30.0)]);
}

#[test]
fn drag_values_corner_with_uniform_scale_keeps_the_aspect() {
    let mut frame = drag_frame(IDENTITY, pt(0.0, 0.0));
    frame.transform.scale = pt(2.0, 2.0);
    // Along the diagonal to twice the distance from the anchor.
    let values = drag_values(
        &frame,
        Handle::Corner(2),
        (pt(100.0, 50.0), pt(200.0, 100.0)),
        false,
        0.0,
        &UNIFORM,
    );
    assert_values(&values, &[(TransformProp::Scale, 4.0)]);
    // Off the diagonal: one ratio, the projection on it.
    let values = drag_values(
        &frame,
        Handle::Corner(2),
        (pt(100.0, 50.0), pt(200.0, 80.0)),
        false,
        0.0,
        &UNIFORM,
    );
    assert_values(&values, &[(TransformProp::Scale, 2.0 * 24000.0 / 12500.0)]);
}

#[test]
fn drag_values_corner_with_per_axis_scale_scales_each_axis() {
    let frame = drag_frame(IDENTITY, pt(0.0, 0.0));
    let values = drag_values(
        &frame,
        Handle::Corner(2),
        (pt(100.0, 50.0), pt(200.0, 80.0)),
        false,
        0.0,
        &PER_AXIS,
    );
    assert_values(
        &values,
        &[(TransformProp::ScaleX, 2.0), (TransformProp::ScaleY, 1.6)],
    );
}

#[test]
fn drag_values_edge_changes_only_scale_x() {
    let frame = drag_frame(IDENTITY, pt(0.0, 0.0));
    let values = drag_values(
        &frame,
        Handle::Edge(1),
        (pt(100.0, 25.0), pt(150.0, 40.0)),
        false,
        0.0,
        &PER_AXIS,
    );
    assert_values(&values, &[(TransformProp::ScaleX, 1.5)]);
}

#[test]
fn drag_values_scale_in_the_layers_rotated_axes() {
    let mut frame = drag_frame(IDENTITY, pt(0.0, 0.0));
    frame.transform.rotation = 90.0;
    // Turned 90° clockwise, the layer's x axis points down the screen.
    let values = drag_values(
        &frame,
        Handle::Edge(1),
        (pt(0.0, 100.0), pt(0.0, 300.0)),
        false,
        0.0,
        &PER_AXIS,
    );
    assert_values(&values, &[(TransformProp::ScaleX, 3.0)]);
}

#[test]
fn drag_values_corner_on_the_anchor_leaves_the_scale() {
    let frame = drag_frame(IDENTITY, pt(0.0, 0.0));
    let values = drag_values(
        &frame,
        Handle::Corner(0),
        (pt(0.0, 0.0), pt(20.0, 20.0)),
        false,
        0.0,
        &UNIFORM,
    );
    assert_values(&values, &[]);
}

#[test]
fn transform_handles_leave_out_edges_for_uniform_scale() {
    let frame = drag_frame(IDENTITY, pt(0.0, 0.0));
    let all = handles(&frame, false, 20.0);
    assert_eq!(all.len(), 9);
    assert!(all.contains(&(Handle::Corner(2), pt(100.0, 50.0))));
    assert!(all.contains(&(Handle::Edge(1), pt(100.0, 25.0))));
    assert!(all.contains(&(Handle::Rotate, pt(50.0, -20.0))));
    let uniform = handles(&frame, true, 20.0);
    assert_eq!(uniform.len(), 5);
    assert!(
        uniform
            .iter()
            .all(|(handle, _)| !matches!(handle, Handle::Edge(_)))
    );
}

#[test]
fn transform_handle_at_prefers_handles_then_the_body() {
    let frame = drag_frame(IDENTITY, pt(0.0, 0.0));
    let all = handles(&frame, false, 20.0);
    assert_eq!(
        handle_at(&frame, &all, pt(102.0, 49.0), 4.0),
        Some(Handle::Corner(2))
    );
    assert_eq!(
        handle_at(&frame, &all, pt(40.0, 30.0), 4.0),
        Some(Handle::Move)
    );
    assert_eq!(handle_at(&frame, &all, pt(140.0, 30.0), 4.0), None);
}

#[test]
fn transform_drag_override_and_props() {
    let mut frame = drag_frame(IDENTITY, pt(10.0, 20.0));
    frame.transform.scale = pt(2.0, 2.0);
    let values = [
        (TransformProp::X, 30.0),
        (TransformProp::Rotation, 15.0),
        (TransformProp::Scale, 3.0),
    ];
    let transform = override_transform(&frame, &values);
    assert_eq!(transform.position, pt(30.0, 20.0));
    assert_eq!(transform.rotation, 15.0);
    assert_eq!(transform.scale, pt(3.0, 3.0));

    // A prop the runtime saw as another value than the transform's keeps its
    // offset; a scale its ratio.
    let props = ReceivedProps {
        x: Some(110.0),
        scale: Some(4.0),
        ..ReceivedProps::default()
    };
    assert_eq!(
        prop_values(&frame, &values, &props),
        vec![
            (TransformProp::X, 130.0),
            (TransformProp::Rotation, 15.0),
            (TransformProp::Scale, 6.0),
        ]
    );
    // The label shows what is written: the received values, not the
    // transform's.
    assert_eq!(
        drag_label(&frame, &values, &props),
        "x 110 → 130  rotation 0 → 15  scale 4 → 6"
    );
    assert_eq!(
        drag_label(&frame, &values, &ReceivedProps::default()),
        "x 10 → 30  rotation 0 → 15  scale 2 → 3"
    );
}

#[test]
fn transform_drag_back_at_the_start_writes_nothing() {
    let frame = drag_frame(IDENTITY, pt(10.0, 20.0));
    let props = ReceivedProps {
        x: Some(10.0),
        y: Some(20.0),
        ..ReceivedProps::default()
    };
    let values = drag_values(
        &frame,
        Handle::Move,
        (pt(5.0, 5.0), pt(5.0, 5.0)),
        false,
        0.0,
        &UNIFORM,
    );
    assert_eq!(values.len(), 2);
    assert!(prop_values(&frame, &values, &props).is_empty());
    assert_eq!(drag_label(&frame, &values, &props), "");
    // Only the changed axis is written.
    let values = drag_values(
        &frame,
        Handle::Move,
        (pt(5.0, 5.0), pt(9.0, 5.0)),
        false,
        0.0,
        &UNIFORM,
    );
    assert_eq!(
        prop_values(&frame, &values, &props),
        vec![(TransformProp::X, 14.0)]
    );
}

#[test]
fn drag_values_rotation_keeps_counting_past_a_half_turn() {
    let frame = drag_frame(IDENTITY, pt(0.0, 0.0));
    let at = |degrees: f64| {
        let radians = degrees.to_radians();
        pt(10.0 * radians.cos(), 10.0 * radians.sin())
    };
    let start = at(0.0);
    // Turning clockwise in steps, each move unwrapped against the last.
    let mut turned = 0.0;
    for degrees in [60.0, 120.0, 170.0, 190.0] {
        turned = turn(&frame, start, at(degrees), turned).unwrap();
    }
    assert!((turned - 190.0).abs() < 1e-9, "{turned}");
    let values = drag_values(
        &frame,
        Handle::Rotate,
        (start, at(190.0)),
        false,
        170.0,
        &UNIFORM,
    );
    assert_values(&values, &[(TransformProp::Rotation, 190.0)]);
    // From a fresh start the same pointer is the short way round.
    let values = drag_values(
        &frame,
        Handle::Rotate,
        (start, at(190.0)),
        false,
        0.0,
        &UNIFORM,
    );
    assert_values(&values, &[(TransformProp::Rotation, -170.0)]);
}

fn shown(frame: &LayerFrame, editable: &[TransformProp]) -> Vec<Handle> {
    shown_handles(frame, editable, 20.0)
        .into_iter()
        .map(|(handle, _)| handle)
        .collect()
}

#[test]
fn transform_shown_handles_leave_out_handles_that_scale_nothing() {
    // The anchor is the top-left corner (React's default).
    let frame = drag_frame(IDENTITY, pt(0.0, 0.0));
    // Uniform: the corner on the anchor goes.
    assert_eq!(
        shown(&frame, &UNIFORM),
        vec![
            Handle::Corner(1),
            Handle::Corner(2),
            Handle::Corner(3),
            Handle::Rotate
        ]
    );
    // Per axis: the top edge (scaleY, on the anchor's row) and the left edge
    // (scaleX, on its column) go too.
    assert_eq!(
        shown(&frame, &PER_AXIS),
        vec![
            Handle::Corner(1),
            Handle::Corner(2),
            Handle::Corner(3),
            Handle::Edge(1),
            Handle::Edge(2),
            Handle::Rotate
        ]
    );
    // scaleY locked: its edge and the corner that only scales y go.
    let x_only = [TransformProp::X, TransformProp::ScaleX];
    assert_eq!(
        shown(&frame, &x_only),
        vec![Handle::Corner(1), Handle::Corner(2), Handle::Edge(1)]
    );
    // An anchor in the middle: every handle scales something.
    let mut centered = drag_frame(IDENTITY, pt(50.0, 25.0));
    centered.outline = [pt(0.0, 0.0), pt(100.0, 0.0), pt(100.0, 50.0), pt(0.0, 50.0)];
    assert_eq!(shown(&centered, &PER_AXIS).len(), 9);
}

fn chain_entry(name: &str, kind: ElementKind, layer_id: Option<&str>) -> LayerSource {
    LayerSource {
        name: name.to_string(),
        file: PathBuf::from("/project/scene.tsx"),
        line: 1,
        column: 1,
        kind,
        layer_id: layer_id.map(str::to_owned),
    }
}

#[test]
fn select_parent_group_sets_selection() {
    let group = chain_entry("Group", ElementKind::Primitive, Some("g"));
    assert_eq!(parent_layer_id(&group), Some("g"));
    // A component or a primitive that emitted no layer has nothing to select.
    let title = chain_entry("Title", ElementKind::Component, Some("g"));
    assert_eq!(parent_layer_id(&title), None);
    let bare = chain_entry("Group", ElementKind::Primitive, None);
    assert_eq!(parent_layer_id(&bare), None);
}

#[test]
fn select_outline_falls_back_to_the_frame_for_a_group() {
    let region = [Point { x: 0.0, y: 0.0 }; 4];
    let frame_box = [Point { x: 1.0, y: 1.0 }; 4];
    assert_eq!(outline_quads(vec![region], Some(frame_box)), vec![region]);
    assert_eq!(outline_quads(Vec::new(), Some(frame_box)), vec![frame_box]);
    assert!(outline_quads(Vec::new(), None).is_empty());
}

// `const frame = useCurrentFrame()` above `<Sequence from={30}>`: the runtime
// reports the frame of the component that wrote the Rect.
const ABOVE_SEQUENCE: &str = "export const Scene = () => {\n  const frame = useCurrentFrame();\n  return <Sequence from={30}><Rect x={interpolate(frame, [0, 30, 60], [0, 100, 100])} /></Sequence>;\n};\n";

fn above_sequence_report(candidate_frames: Vec<f64>) -> LayerSourceReport {
    LayerSourceReport {
        local_frame: 60.0,
        candidate_frames,
        ..primitive_report(
            3,
            30,
            ReceivedProps {
                x: Some(100.0),
                ..ReceivedProps::default()
            },
        )
    }
}

#[test]
fn transform_keyframe_is_the_one_the_owner_frame_reads() {
    let targets = resolve_targets(ABOVE_SEQUENCE, &above_sequence_report(vec![])).unwrap();
    let Some(EditTarget::Keyframe { span, frame, .. }) = targets.target(TransformProp::X) else {
        panic!(
            "expected a keyframe, got {:?}",
            targets.target(TransformProp::X)
        );
    };
    assert_eq!(span.start as usize, ABOVE_SEQUENCE.rfind("100").unwrap());
    assert_eq!(*frame, 60.0);

    // The frame could be the Sequence's 30 as well, which is the keyframe
    // whose value is also 100: no telling which.
    let targets =
        resolve_targets(ABOVE_SEQUENCE, &above_sequence_report(vec![30.0, 60.0])).unwrap();
    assert_eq!(
        targets.target(TransformProp::X),
        Some(&EditTarget::Locked(LockReason::FrameUncertain))
    );
    // A literal elsewhere on the element stays editable.
    assert!(matches!(
        targets.target(TransformProp::Y),
        Some(EditTarget::Missing { .. })
    ));
}

#[test]
fn transform_targets_need_the_current_lookup() {
    let report = primitive_report(2, 3, x_only());
    let other = primitive_report(2, 3, ReceivedProps::default());
    let mut state = TransformEdit::default();
    let request = state.begin("a", &report, 10).unwrap();
    assert!(state.finish(
        request,
        Some("a"),
        resolve_targets(TRANSFORM_FIXTURE, &report)
    ));
    assert!(state.targets(Some("a"), Some((&report, 10))).is_some());

    // Seeked: the lookup at the new frame has not been resolved yet.
    assert!(state.targets(Some("a"), Some((&report, 11))).is_none());
    // A newer report for the same frame.
    assert!(state.targets(Some("a"), Some((&other, 10))).is_none());
    // Still looking up.
    assert!(state.targets(Some("a"), None).is_none());
}

#[test]
fn transform_only_a_reset_clears_an_own_write() {
    let report = primitive_report(2, 3, x_only());
    let newer = primitive_report(2, 3, ReceivedProps::default());
    let mut state = TransformEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(
        request,
        Some("a"),
        resolve_targets(TRANSFORM_FIXTURE, &report)
    ));

    // A lookup answered by the old runtime after the write resolves, but the
    // file it was read from is about to be reloaded.
    state.invalidate();
    let request = state.begin("a", &newer, 0).unwrap();
    assert!(state.finish(
        request,
        Some("a"),
        resolve_targets(TRANSFORM_FIXTURE, &newer)
    ));
    assert!(state.targets(Some("a"), Some((&newer, 0))).is_none());

    // The kept reload resets the state.
    state.reset();
    let request = state.begin("a", &newer, 0).unwrap();
    assert!(state.finish(
        request,
        Some("a"),
        resolve_targets(TRANSFORM_FIXTURE, &newer)
    ));
    assert!(state.targets(Some("a"), Some((&newer, 0))).is_some());
}

#[test]
fn expire_keeps_the_resolution_visible_but_not_current() {
    let report = primitive_report(2, 3, x_only());
    let mut state = TransformEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(
        request,
        Some("a"),
        resolve_targets(TRANSFORM_FIXTURE, &report)
    ));

    state.invalidate();
    state.expire();
    // The rows keep their values, read-only, while the reloaded code answers.
    let (_, current) = state.resolved(Some("a")).expect("still shown");
    assert!(!current);
    assert!(state.targets(Some("a"), Some((&report, 0))).is_none());
    // The reloaded code's answer resolves again even when it is the same.
    assert!(state.begin("a", &report, 0).is_some());
}

#[test]
fn finish_after_expire_is_current_again() {
    let report = primitive_report(2, 3, x_only());
    let mut state = TransformEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(
        request,
        Some("a"),
        resolve_targets(TRANSFORM_FIXTURE, &report)
    ));
    state.invalidate();

    state.expire();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(
        request,
        Some("a"),
        resolve_targets(TRANSFORM_FIXTURE, &report)
    ));
    assert!(state.resolved(Some("a")).unwrap().1);
    assert!(state.targets(Some("a"), Some((&report, 0))).is_some());
}

#[test]
fn expire_then_another_write_stays_stale() {
    let report = primitive_report(2, 3, x_only());
    let newer = primitive_report(2, 3, ReceivedProps::default());
    let targets = |report| resolve_targets(TRANSFORM_FIXTURE, report);
    let mut state = TransformEdit::default();
    state.invalidate();
    state.expire();
    let request = state.begin("a", &report, 0).unwrap();

    // A second write lands before the reloaded code's answer resolves.
    state.invalidate();
    assert!(!state.finish(request, Some("a"), targets(&report)));
    // A lookup the old code answers after that write is not current.
    let request = state.begin("a", &newer, 0).unwrap();
    assert!(state.finish(request, Some("a"), targets(&newer)));
    assert!(state.targets(Some("a"), Some((&newer, 0))).is_none());
}

#[test]
fn written_values_show_until_the_reloaded_code_resolves() {
    let report = primitive_report(2, 3, x_only());
    let targets = || resolve_targets(TRANSFORM_FIXTURE, &report);
    let mut state = TransformEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), targets()));

    state.invalidate();
    state.record_written(Some("a"), &[(TransformProp::X, 184.0)]);
    state.record_written(Some("a"), &[(TransformProp::Y, 40.0)]);
    assert_eq!(
        state.written_value(Some("a"), TransformProp::X),
        Some(184.0)
    );
    assert_eq!(state.written_value(Some("a"), TransformProp::Y), Some(40.0));
    assert_eq!(
        state.written_value(Some("a"), TransformProp::Rotation),
        None
    );
    assert_eq!(state.written_value(Some("b"), TransformProp::X), None);

    // Still shown while the reload runs.
    state.expire();
    assert_eq!(
        state.written_value(Some("a"), TransformProp::X),
        Some(184.0)
    );
    // The reloaded code's resolution shows the file's values instead.
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), targets()));
    assert_eq!(state.written_value(Some("a"), TransformProp::X), None);
}

#[test]
fn written_values_are_dropped_with_the_selection() {
    let mut state = TransformEdit::default();
    state.invalidate();
    state.record_written(Some("a"), &[(TransformProp::X, 184.0)]);
    state.reset();
    assert_eq!(state.written_value(Some("a"), TransformProp::X), None);
}

#[test]
fn a_current_resolution_ignores_written_values() {
    // Nothing was written since the targets were read: nothing to show.
    let mut state = TransformEdit::default();
    state.record_written(Some("a"), &[(TransformProp::X, 184.0)]);
    assert_eq!(state.written_value(Some("a"), TransformProp::X), None);
}

#[test]
fn an_own_write_reload_keeps_the_lookup() {
    let report = primitive_report(2, 3, x_only());
    let targets = || resolve_targets(TRANSFORM_FIXTURE, &report);
    let mut keep = SelectionKeep::default();
    let mut state = TransformEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), targets()));

    // A transform commit wrote the file and asked for a reload.
    keep.own_write();
    state.invalidate();
    assert!(keep.requested(1, true));

    // The reload is applied: the view keeps the lookup answer, marks it
    // stale, and expires the targets.
    assert!(keep.applies(1));
    state.expire();
    keep.confirm_from(5);
    let (_, current) = state.resolved(Some("a")).expect("still shown");
    assert!(!current);

    // A frame of the old code keeps the stale answer up.
    assert_eq!(keep.lookup_arrived(4, false), LookupFate::Stale);
    // The reloaded code still draws the layer: the answer shows and the
    // same report resolves again, current.
    assert_eq!(keep.lookup_arrived(5, false), LookupFate::Show);
    let request = state.begin("a", &report, 0).expect("resolved again");
    assert!(state.finish(request, Some("a"), targets()));
    assert!(state.targets(Some("a"), Some((&report, 0))).is_some());
}

#[test]
fn an_outside_reload_clears_the_selection() {
    let mut keep = SelectionKeep::default();
    // An edit in another editor reloads without an own write.
    assert!(!keep.requested(1, true));
    assert!(!keep.applies(1));
    // An own write whose reload loads another composition.
    keep.own_write();
    assert!(!keep.requested(2, false));
    assert!(!keep.applies(2));
    // The own write was used up; the next outside reload clears too.
    assert!(!keep.requested(3, true));
    assert!(!keep.applies(3));
}

#[test]
fn transform_body_press_on_a_group_child_moves_the_group() {
    let mut frame = drag_frame(IDENTITY, pt(10.0, 20.0));
    frame.leaves = vec!["child-1".to_owned(), "child-2".to_owned()];
    assert!(presses_body(&frame, Some("child-2")));
    assert!(!presses_body(&frame, Some("elsewhere")));
    assert!(!presses_body(&frame, None));

    // A leaf is pressed only on itself.
    let leaf = drag_frame(IDENTITY, pt(10.0, 20.0));
    assert!(presses_body(&leaf, Some("a")));
    assert!(!presses_body(&leaf, Some("b")));
}

#[test]
fn transform_missing_scale_axis_shows_the_written_scale() {
    // `<Rect scale={2} scaleX={3} />`: scaleY is absent but drawn at 2.
    let report = primitive_report(
        2,
        3,
        ReceivedProps {
            scale: Some(2.0),
            scale_x: Some(3.0),
            ..ReceivedProps::default()
        },
    );
    let missing = EditTarget::Missing {
        insert_at: 0,
        default: 1.0,
    };
    assert_eq!(
        field_text(TransformProp::ScaleY, &missing, &report),
        (String::new(), "2".to_owned())
    );
    assert_eq!(
        field_text(
            TransformProp::ScaleY,
            &EditTarget::Locked(LockReason::Spread),
            &report
        ),
        (String::new(), "2".to_owned())
    );
    // Without a scale, the default.
    let plain = primitive_report(2, 3, ReceivedProps::default());
    assert_eq!(
        field_text(TransformProp::ScaleY, &missing, &plain),
        (String::new(), "1".to_owned())
    );
    assert_eq!(
        field_text(
            TransformProp::X,
            &EditTarget::Missing {
                insert_at: 0,
                default: 0.0
            },
            &report
        ),
        (String::new(), "0".to_owned())
    );
}

#[test]
fn transform_files_outside_the_project_are_locked() {
    let entry_dir = Path::new("/project/src");
    assert!(inside_project(
        Path::new("/project/src/scene.tsx"),
        entry_dir
    ));
    assert!(inside_project(
        Path::new("/project/src/parts/title.tsx"),
        entry_dir
    ));
    assert!(!inside_project(
        Path::new("/project/lib/shared.tsx"),
        entry_dir
    ));
    assert!(!inside_project(
        Path::new("/project/src-old/scene.tsx"),
        entry_dir
    ));
    assert!(!inside_project(
        Path::new("/project/src/node_modules/kit/rect.tsx"),
        entry_dir
    ));
    assert!(!inside_project(
        Path::new("/project/src/../lib/shared.tsx"),
        entry_dir
    ));

    let report = primitive_report(2, 3, x_only());
    let mut targets = resolve_targets(TRANSFORM_FIXTURE, &report).unwrap();
    lock_outside_project(&mut targets);
    assert!(
        targets
            .targets
            .iter()
            .all(|(_, target)| *target == EditTarget::Locked(LockReason::OutsideProject))
    );
    // A commit refuses a locked target before reading the file's targets.
    assert_eq!(
        commit_text(
            TRANSFORM_FIXTURE,
            &targets,
            &report,
            &[(TransformProp::X, 5.0)]
        ),
        Err(CommitError::NotEditable(TransformProp::X))
    );
}
