use crate::EDITOR_DEMO_PROJECT;
use crate::audio::{AudioCacheKey, CachedAudioDecoder, take_latest};
use crate::audio_cache::DiskAudioCache;
use crate::edit_history::{EditHistory, HistoryEntry, Texts, write_all_recorded};
use crate::editor_launch::{
    Invocation, LaunchError, default_opener, editor_invocation, editor_value,
    parse_login_shell_output, read_until_closing_marker, split_command,
};
use crate::element_edit::{
    CommitError, ElementEdit, ElementTargets, PropKey, Target, TransformProp, apply_checked,
    commit_files, inside_project, keyframe_jump_frames, keyframe_timeline_frame,
    lock_outside_project, reason_text, resolve_targets, table_default, type_component_props,
};
use crate::element_rows::{
    HeldCommit, HeldCommits, InteractiveToggle, Scrub, SelectionGate, TypedText, cleared_path,
    commit_owns_row, control_kind, edited_rows, interactive_toggle, moved_off_keyframe,
    parse_row_value, removable, row_field, row_kind, row_label, row_text, scrub_takes_focus,
    scrub_value, set_at_text, typed_text, written_text,
};
use crate::export_worker::{
    ExportEvent, ExportRequest, ExportSource, ExportWorker, export_options,
};
use crate::helpers::{
    ExportLoudness, export_loudness_label, export_output, export_progress_label, export_range_for,
    export_suggested_name, loop_range_for,
};
use crate::layer_panel::locked_summary;
use crate::layer_select::{
    SourceLookup, display_path, launch_outcome_applies, letterbox, lookup_allowed, outline_quads,
    owner_call_site, parent_layer_id, scene_point, source_lookup, source_lookup_target,
    source_position,
};
use crate::property_controls::ControlKind;
use crate::react_audio::{react_audio_clip_id, react_tracks};
use crate::refresh::{LookupFate, SelectionKeep};
use crate::source::is_react_entry;
use crate::transform_drag::{
    OverrideSite, TransformDrag, override_sites, presses_body, prop_overrides,
};
use crate::transform_handles::{
    Handle, drag_label, drag_values, handle_at, handles, preview_values, prop_values,
    shown_handles, turn,
};
use crate::waveform::{
    clip_level_envelope, level_at_time, map_clip_waveform, master_volume_from_drag, waveform_peaks,
    waveform_segment,
};
use celesta_editor_core::ClipKind;
use celesta_gpu_renderer::{GpuDriver, LayerFrame};
use celesta_source_edit::{Edit, EditTarget, EditValue, LiteralForm, LockReason, Span, element_at};

use crate::preview::{
    ReactPreviewContext, ReactPreviewMode, collect_component_requests, records_hit_regions,
    splice_component_resolutions, strip_missing_components,
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
    ComponentResolutions, ElementKind, LayerSource, LayerSourceReport, PrimitivePropertyField,
    PrimitivePropertyGroup, PrimitivePropertyRow, PrimitivePropertyTable, PropOverride,
    ReceivedProps, ReceivedValue, SourcePos,
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

/// Resolves `report` with `text` as the text of every file.
fn resolve_in(text: &str, report: &LayerSourceReport) -> Result<ElementTargets, String> {
    resolve_targets(|_| Ok(text.to_owned()), report, None)
}

/// `prop` set to the number `value`, as a commit takes it.
fn num(prop: TransformProp, value: f64) -> (PropKey, Option<EditValue>) {
    (prop.key(), Some(EditValue::Number(value)))
}

/// The row keys of `targets`, in order.
fn keys(targets: &ElementTargets) -> Vec<PropKey> {
    targets.targets.iter().map(|(key, _)| key.clone()).collect()
}

/// Commits `values` with `text` as the element's file, returning its new text.
fn commit_in(
    text: &str,
    expected: &ElementTargets,
    report: &LayerSourceReport,
    values: &[(PropKey, Option<EditValue>)],
) -> Result<String, CommitError> {
    let texts = Texts::from([(expected.file.clone(), text.to_owned())]);
    let files = commit_files(&texts, expected, report, values)?;
    Ok(files
        .into_iter()
        .next()
        .map_or_else(|| text.to_owned(), |(_, text)| text))
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

    let targets = resolve_in(text, &report).expect("the element resolves");

    assert_eq!(
        keys(&targets),
        [
            TransformProp::X.key(),
            TransformProp::Y.key(),
            TransformProp::Rotation.key(),
            TransformProp::ScaleX.key(),
            TransformProp::ScaleY.key()
        ]
    );
    assert!(matches!(
        targets.targets[3].1.target,
        EditTarget::Literal { value: EditValue::Number(value), .. } if value == 2.0
    ));
    assert!(matches!(
        targets.targets[4].1.target,
        EditTarget::Missing { default: Some(EditValue::Number(default)), .. } if default == 1.0
    ));
    assert_eq!(targets.file, PathBuf::from("/project/scene.tsx"));
}

#[test]
fn transform_resolve_targets_uniform_scale() {
    let targets = resolve_in(TRANSFORM_FIXTURE, &primitive_report(2, 3, x_only()))
        .expect("the element resolves");

    assert_eq!(
        keys(&targets),
        [
            TransformProp::X.key(),
            TransformProp::Y.key(),
            TransformProp::Rotation.key(),
            TransformProp::Scale.key()
        ]
    );
    assert_eq!(TransformProp::ScaleX.name(), "scaleX");
    assert!(matches!(
        targets.targets[0].1.target,
        EditTarget::Literal { value: EditValue::Number(value), .. } if value == 100.0
    ));
}

#[test]
fn transform_resolve_targets_fails_without_the_element() {
    assert!(resolve_in(TRANSFORM_FIXTURE, &primitive_report(1, 1, x_only())).is_err());
    assert!(resolve_in(TRANSFORM_FIXTURE, &report_of(Vec::new())).is_err());
}

#[test]
fn transform_commit_writes_literal_and_inserts() {
    let report = primitive_report(2, 3, x_only());
    let expected = resolve_in(TRANSFORM_FIXTURE, &report).unwrap();

    let written = commit_in(
        TRANSFORM_FIXTURE,
        &expected,
        &report,
        &[
            num(TransformProp::X, 120.0),
            num(TransformProp::Rotation, 45.5),
        ],
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
    let expected = resolve_in(TRANSFORM_FIXTURE, &report).unwrap();
    // The element moved within its line, so the report's position misses it.
    let moved = TRANSFORM_FIXTURE.replace("  <Rect", "    <Rect");
    // The element stayed but its `x` became longer, so the span moved.
    let widened = TRANSFORM_FIXTURE.replace("x={100}", "x={1000}");

    for text in [moved, widened] {
        let error = commit_in(&text, &expected, &report, &[num(TransformProp::X, 120.0)])
            .expect_err("the source changed");
        assert_eq!(error, CommitError::SourceChanged);
        assert_eq!(error.to_string(), "The source changed while editing");
    }
}

#[test]
fn transform_commit_refuses_locked_and_non_finite_values() {
    let report = primitive_report(2, 3, x_only());
    let expected = resolve_in(TRANSFORM_FIXTURE, &report).unwrap();

    assert!(matches!(
        commit_in(
            TRANSFORM_FIXTURE,
            &expected,
            &report,
            &[num(TransformProp::X, f64::NAN)]
        ),
        Err(CommitError::NotANumber)
    ));
    // `scaleX` is not a target of a uniformly scaled element.
    assert_eq!(
        commit_in(
            TRANSFORM_FIXTURE,
            &expected,
            &report,
            &[num(TransformProp::ScaleX, 2.0)]
        ),
        Err(CommitError::NotEditable("Scale X".to_owned()))
    );
    assert_eq!(
        CommitError::NotEditable("Scale X".to_owned()).to_string(),
        "Scale X can't be edited here"
    );
}

/// `<Text>Sale</Text>` / `<Video loop />` at line 2, column 3 of
/// `/project/scene.tsx`, received with `value` as `prop`.
fn other_prop_report(name: &str, prop: &str, value: ReceivedValue) -> LayerSourceReport {
    let mut report = primitive_report(2, 3, ReceivedProps::default());
    report.chain[0].name = name.to_owned();
    report.props.others.insert(prop.to_owned(), value);
    report
}

/// A hand-built target set of `text`'s element at line 2, column 3 with the
/// single row `prop` holding `target`.
fn single_row(text: &str, prop: &str, target: EditTarget) -> ElementTargets {
    let file = PathBuf::from("/project/scene.tsx");
    ElementTargets {
        file: file.clone(),
        files: vec![file.clone()],
        element: element_at(text, 2, 3).expect("the element is at 2:3"),
        targets: vec![(
            PropKey::Prop(prop.to_owned()),
            Target {
                file,
                path: prop.to_owned(),
                target,
                call_site: None,
                inputs: Vec::new(),
            },
        )],
        table: None,
        component: None,
    }
}

#[test]
fn commit_files_writes_a_string() {
    let text = "export const Scene = () => (\n  <Text>Sale</Text>\n);\n";
    let report = other_prop_report("Text", "children", ReceivedValue::String("Sale".to_owned()));
    let start = text.find("Sale").unwrap() as u32;
    let expected = single_row(
        text,
        "children",
        EditTarget::Literal {
            span: Span {
                start,
                end: start + 4,
            },
            value: EditValue::String("Sale".to_owned()),
            form: LiteralForm::JsxText,
        },
    );

    let written = commit_in(
        text,
        &expected,
        &report,
        &[(
            PropKey::Prop("children".to_owned()),
            Some(EditValue::String("Sold out".to_owned())),
        )],
    );

    assert_eq!(
        written,
        Ok("export const Scene = () => (\n  <Text>Sold out</Text>\n);\n".to_owned())
    );
}

#[test]
fn commit_files_writes_a_bool() {
    let text = "export const Scene = () => (\n  <Video loop />\n);\n";
    let report = other_prop_report("Video", "loop", ReceivedValue::Bool(true));
    let at = (text.find("loop").unwrap() + "loop".len()) as u32;
    let expected = single_row(
        text,
        "loop",
        EditTarget::Literal {
            span: Span { start: at, end: at },
            value: EditValue::Bool(true),
            form: LiteralForm::BareTrue,
        },
    );

    let written = commit_in(
        text,
        &expected,
        &report,
        &[(
            PropKey::Prop("loop".to_owned()),
            Some(EditValue::Bool(false)),
        )],
    );

    assert_eq!(
        written,
        Ok("export const Scene = () => (\n  <Video loop={false} />\n);\n".to_owned())
    );
}

#[test]
fn written_values_keep_their_type() {
    let mut state = ElementEdit::default();
    state.invalidate();
    let children = PropKey::Prop("children".to_owned());
    state.record_written(
        Some("a"),
        &[(
            children.clone(),
            Some(EditValue::String("Sold out".to_owned())),
        )],
    );
    assert_eq!(
        state.written_value(Some("a"), &children),
        Some(&Some(EditValue::String("Sold out".to_owned())))
    );
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
        (LockReason::StyleMaySet, "May be set by `style`"),
    ];
    for (reason, text) in cases {
        assert_eq!(reason_text(&reason), text);
    }
}

#[test]
fn pass_through_reason_texts() {
    assert_eq!(
        reason_text(&LockReason::ParameterOfCaller {
            component: "Card".to_owned()
        }),
        "Comes from `<Card>`'s caller"
    );
    assert_eq!(
        reason_text(&LockReason::CallSiteNotFound {
            component: "PriceTag".to_owned()
        }),
        "Can't find where `<PriceTag>` is called"
    );
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
    let targets = || resolve_in(TRANSFORM_FIXTURE, &report);
    let mut state = ElementEdit::default();

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

    let targets = resolve_in(TRANSFORM_FIXTURE, &report).expect("the element resolves");

    assert_eq!(
        keys(&targets),
        [
            TransformProp::X.key(),
            TransformProp::Y.key(),
            TransformProp::Rotation.key(),
            TransformProp::Scale.key()
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
    let mut state = ElementEdit::default();

    let first = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(first, Some("a"), resolve_in(before, &report)));
    state.set_error(
        Some("a"),
        Some("The source changed while editing".to_owned()),
    );

    // The reload cleared the selection; selecting the layer again.
    state.reset();
    assert_eq!(state.error(Some("a")), None, "the old error is gone");
    let second = state.begin("a", &report, 0).expect("resolved again");
    assert!(state.finish(second, Some("a"), resolve_in(&after, &report)));

    let (targets, report) = state.targets(Some("a"), Some((&report, 0))).unwrap();
    let written = commit_in(&after, targets, report, &[num(TransformProp::X, 120.0)]).unwrap();
    assert_eq!(written, after.replace("x={100}", "x={120}"));
}

#[test]
fn transform_explains_why_there_is_nothing_to_commit() {
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();
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
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
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
    let targets = resolve_in(text, &report).unwrap();
    let target = targets.transform(TransformProp::X).unwrap();
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
fn transform_drag_props() {
    let mut frame = drag_frame(IDENTITY, pt(10.0, 20.0));
    frame.transform.scale = pt(2.0, 2.0);
    let values = [
        (TransformProp::X, 30.0),
        (TransformProp::Rotation, 15.0),
        (TransformProp::Scale, 3.0),
    ];

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
    let targets = resolve_in(ABOVE_SEQUENCE, &above_sequence_report(vec![])).unwrap();
    let Some(EditTarget::Keyframe { span, frame, .. }) = targets.transform(TransformProp::X) else {
        panic!(
            "expected a keyframe, got {:?}",
            targets.transform(TransformProp::X)
        );
    };
    assert_eq!(span.start as usize, ABOVE_SEQUENCE.rfind("100").unwrap());
    assert_eq!(*frame, 60.0);

    // The frame could be the Sequence's 30 as well, which is the keyframe
    // whose value is also 100: no telling which.
    let targets = resolve_in(ABOVE_SEQUENCE, &above_sequence_report(vec![30.0, 60.0])).unwrap();
    assert_eq!(
        targets.transform(TransformProp::X),
        Some(&EditTarget::Locked(LockReason::FrameUncertain))
    );
    // A literal elsewhere on the element stays editable.
    assert!(matches!(
        targets.transform(TransformProp::Y),
        Some(EditTarget::Missing { .. })
    ));
}

#[test]
fn transform_targets_need_the_current_lookup() {
    let report = primitive_report(2, 3, x_only());
    let other = primitive_report(2, 3, ReceivedProps::default());
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 10).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
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
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));

    // A lookup answered by the old runtime after the write resolves, but the
    // file it was read from is about to be reloaded.
    state.invalidate();
    let request = state.begin("a", &newer, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &newer)));
    assert!(state.targets(Some("a"), Some((&newer, 0))).is_none());

    // The kept reload resets the state.
    state.reset();
    let request = state.begin("a", &newer, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &newer)));
    assert!(state.targets(Some("a"), Some((&newer, 0))).is_some());
}

#[test]
fn expire_keeps_the_resolution_visible_but_not_current() {
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));

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
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    state.invalidate();

    state.expire();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    assert!(state.resolved(Some("a")).unwrap().1);
    assert!(state.targets(Some("a"), Some((&report, 0))).is_some());
}

#[test]
fn expire_then_another_write_stays_stale() {
    let report = primitive_report(2, 3, x_only());
    let newer = primitive_report(2, 3, ReceivedProps::default());
    let targets = |report| resolve_in(TRANSFORM_FIXTURE, report);
    let mut state = ElementEdit::default();
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
    let targets = || resolve_in(TRANSFORM_FIXTURE, &report);
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), targets()));

    state.invalidate();
    state.record_written(Some("a"), &[num(TransformProp::X, 184.0)]);
    state.record_written(Some("a"), &[num(TransformProp::Y, 40.0)]);
    let (x, y) = (TransformProp::X.key(), TransformProp::Y.key());
    assert_eq!(
        state.written_value(Some("a"), &x),
        Some(&Some(EditValue::Number(184.0)))
    );
    assert_eq!(
        state.written_value(Some("a"), &y),
        Some(&Some(EditValue::Number(40.0)))
    );
    assert_eq!(
        state.written_value(Some("a"), &TransformProp::Rotation.key()),
        None
    );
    assert_eq!(state.written_value(Some("b"), &x), None);

    // Still shown while the reload runs.
    state.expire();
    assert_eq!(
        state.written_value(Some("a"), &x),
        Some(&Some(EditValue::Number(184.0)))
    );
    // The reloaded code's resolution shows the file's values instead.
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), targets()));
    assert_eq!(state.written_value(Some("a"), &x), None);
}

#[test]
fn written_values_are_dropped_with_the_selection() {
    let mut state = ElementEdit::default();
    state.invalidate();
    state.record_written(Some("a"), &[num(TransformProp::X, 184.0)]);
    state.reset();
    assert_eq!(
        state.written_value(Some("a"), &TransformProp::X.key()),
        None
    );
}

#[test]
fn a_current_resolution_ignores_written_values() {
    // Nothing was written since the targets were read: nothing to show.
    let mut state = ElementEdit::default();
    state.record_written(Some("a"), &[num(TransformProp::X, 184.0)]);
    assert_eq!(
        state.written_value(Some("a"), &TransformProp::X.key()),
        None
    );
}

#[test]
fn an_own_write_reload_keeps_the_lookup() {
    let report = primitive_report(2, 3, x_only());
    let targets = || resolve_in(TRANSFORM_FIXTURE, &report);
    let mut keep = SelectionKeep::default();
    let mut state = ElementEdit::default();
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
    let missing = local_target(
        "scaleY",
        EditTarget::Missing {
            insert_at: 0,
            default: Some(EditValue::Number(1.0)),
        },
    );
    assert_eq!(
        row_text("scaleY", &missing, None, &report),
        (String::new(), "2".to_owned())
    );
    assert_eq!(
        row_text(
            "scaleY",
            &local_target("scaleY", EditTarget::Locked(LockReason::Spread)),
            None,
            &report
        ),
        (String::new(), "2".to_owned())
    );
    // Without a scale, the default.
    let plain = primitive_report(2, 3, ReceivedProps::default());
    assert_eq!(
        row_text("scaleY", &missing, None, &plain),
        (String::new(), "1".to_owned())
    );
    assert_eq!(
        row_text(
            "x",
            &local_target(
                "x",
                EditTarget::Missing {
                    insert_at: 0,
                    default: Some(EditValue::Number(0.0))
                }
            ),
            None,
            &report
        ),
        (String::new(), "0".to_owned())
    );
}

/// A target written on the element itself, at `path`.
fn local_target(path: &str, target: EditTarget) -> Target {
    Target {
        file: PathBuf::from("/project/scene.tsx"),
        path: path.to_owned(),
        target,
        call_site: None,
        inputs: Vec::new(),
    }
}

#[test]
fn transform_missing_parameter_without_a_default_shows_the_drawn_value() {
    // `({ scale }) => <Rect scale={scale} />` called as `<Tag />`: no
    // default is known, and the layer draws at the primitive's own default.
    let missing = EditTarget::Missing {
        insert_at: 0,
        default: None,
    };
    let plain = primitive_report(2, 3, ReceivedProps::default());
    let at = |path: &str| local_target(path, missing.clone());
    assert_eq!(
        row_text("scale", &at("scale"), None, &plain),
        (String::new(), "1".to_owned())
    );
    assert_eq!(
        row_text("x", &at("x"), None, &plain),
        (String::new(), "0".to_owned())
    );
    // `({ scale = base * 2 })`: the value received is the one drawn, also
    // for a scale axis next to a written `scale`.
    let received = primitive_report(
        2,
        3,
        ReceivedProps {
            scale: Some(2.0),
            scale_x: Some(3.0),
            ..ReceivedProps::default()
        },
    );
    assert_eq!(
        row_text("scale", &at("scale"), None, &received),
        (String::new(), "2".to_owned())
    );
    assert_eq!(
        row_text("scaleX", &at("scaleX"), None, &received),
        (String::new(), "3".to_owned())
    );
    // Through the call site, as the row shows it.
    let through = EditTarget::PassThrough {
        call_site: celesta_source_edit::ElementRef {
            start: 0,
            end_of_attributes: 0,
            children: None,
        },
        param: "scale".to_owned(),
        target: Box::new(missing),
    };
    assert_eq!(
        row_text("scale", &local_target("scale", through), None, &plain),
        (String::new(), "1".to_owned())
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
    let mut targets = resolve_in(TRANSFORM_FIXTURE, &report).unwrap();
    lock_outside_project(&mut targets, |_| true);
    assert!(
        targets
            .targets
            .iter()
            .all(|(_, target)| target.target == EditTarget::Locked(LockReason::OutsideProject))
    );
    // A commit refuses a locked target before reading the file's targets.
    assert_eq!(
        commit_in(
            TRANSFORM_FIXTURE,
            &targets,
            &report,
            &[num(TransformProp::X, 5.0)]
        ),
        Err(CommitError::NotEditable("X".to_owned()))
    );
}

const PRICE_TAG: &str =
    "export const PriceTag = ({ x }) => (\n  <Rect x={x} rotation={10} />\n);\n";
const PRICE_TAG_SCENE: &str = "export const Scene = () => (\n  <PriceTag x={40} />\n);\n";

fn entry_at(name: &str, kind: ElementKind, file: &str, line: u32) -> LayerSource {
    LayerSource {
        name: name.to_owned(),
        file: PathBuf::from(file),
        line,
        column: 3,
        kind,
        layer_id: None,
    }
}

/// `<Rect x={x}>` in `PriceTag`, called from the scene as `<PriceTag x={40} />`.
fn price_tag_report() -> LayerSourceReport {
    LayerSourceReport {
        props: ReceivedProps {
            x: Some(40.0),
            rotation: Some(10.0),
            ..ReceivedProps::default()
        },
        ..report_of(vec![
            entry_at("Rect", ElementKind::Primitive, "/project/component.tsx", 2),
            entry_at("PriceTag", ElementKind::Component, "/project/scene.tsx", 2),
        ])
    }
}

fn price_tag_files() -> Texts {
    Texts::from([
        (
            PathBuf::from("/project/component.tsx"),
            PRICE_TAG.to_owned(),
        ),
        (
            PathBuf::from("/project/scene.tsx"),
            PRICE_TAG_SCENE.to_owned(),
        ),
    ])
}

/// Reads `texts`; any other file fails.
fn read_from(texts: &Texts) -> impl Fn(&Path) -> Result<String, String> + '_ {
    |path| {
        texts
            .get(path)
            .cloned()
            .ok_or_else(|| format!("Couldn't read {}", path.display()))
    }
}

#[test]
fn owner_call_site_skips_wrappers_and_children_hosts() {
    // `PriceTag` passes the `<Rect>` it wrote to `<Later>` as children.
    let report = report_of(vec![
        entry_at("Rect", ElementKind::Primitive, "/project/component.tsx", 2),
        entry_at(
            "Sequence",
            ElementKind::Wrapper,
            "/project/component.tsx",
            3,
        ),
        entry_at("Later", ElementKind::Component, "/project/component.tsx", 4),
        entry_at("PriceTag", ElementKind::Component, "/project/scene.tsx", 2),
    ]);

    assert_eq!(owner_call_site(&report, "PriceTag"), Some(&report.chain[3]));
    assert_eq!(owner_call_site(&report, "Card"), None);
}

#[test]
fn owner_call_site_falls_back_to_an_anonymous_component() {
    // `memo(({ x }) => …)` reports no name at runtime.
    let anonymous = report_of(vec![
        entry_at("Rect", ElementKind::Primitive, "/project/component.tsx", 2),
        entry_at(
            "Sequence",
            ElementKind::Wrapper,
            "/project/component.tsx",
            3,
        ),
        entry_at("Anonymous", ElementKind::Component, "/project/scene.tsx", 2),
        entry_at("Anonymous", ElementKind::Component, "/project/index.tsx", 2),
    ]);
    assert_eq!(
        owner_call_site(&anonymous, "PriceTag"),
        Some(&anonymous.chain[2])
    );

    // A component with the name wins over a nearer anonymous one.
    let named = report_of(vec![
        entry_at("Rect", ElementKind::Primitive, "/project/component.tsx", 2),
        entry_at(
            "Anonymous",
            ElementKind::Component,
            "/project/component.tsx",
            4,
        ),
        entry_at("PriceTag", ElementKind::Component, "/project/scene.tsx", 2),
    ]);
    assert_eq!(owner_call_site(&named, "PriceTag"), Some(&named.chain[2]));

    // The element itself is never its own call site.
    let alone = report_of(vec![entry_at(
        "Anonymous",
        ElementKind::Component,
        "/project/component.tsx",
        2,
    )]);
    assert_eq!(owner_call_site(&alone, "PriceTag"), None);
}

#[test]
fn x_passed_at_the_call_site_is_a_pass_through_target() {
    let report = price_tag_report();
    let targets = resolve_targets(read_from(&price_tag_files()), &report, None)
        .expect("the element resolves");

    assert_eq!(targets.file, PathBuf::from("/project/component.tsx"));
    assert_eq!(
        targets.file_of(&TransformProp::X.key()),
        Path::new("/project/scene.tsx")
    );
    let Some(EditTarget::PassThrough { param, target, .. }) = targets.transform(TransformProp::X)
    else {
        panic!(
            "x passes through: {:?}",
            targets.transform(TransformProp::X)
        );
    };
    assert_eq!(param, "x");
    assert!(matches!(
        **target,
        EditTarget::Literal { value: EditValue::Number(value), .. } if value == 40.0
    ));
    assert_eq!(
        targets.targets[0].1.call_site,
        Some(SourcePos {
            file: PathBuf::from("/project/scene.tsx"),
            line: 2,
            column: 3,
        })
    );

    // `rotation` is written on the element itself.
    assert_eq!(
        targets.file_of(&TransformProp::Rotation.key()),
        Path::new("/project/component.tsx")
    );
    assert!(matches!(
        targets.transform(TransformProp::Rotation),
        Some(EditTarget::Literal { value: EditValue::Number(value), .. }) if *value == 10.0
    ));
    assert_eq!(targets.targets[2].1.call_site, None);
}

/// `<Rect x={left} rotation={10} scale={size * 2} />` in `PriceTag`, called
/// as `<PriceTag left={40} size={1} />`: `left` is set at the call site,
/// `rotation` on the element, `y` nowhere, and `scale` is computed.
fn mixed_targets() -> ElementTargets {
    let files = Texts::from([
        (
            PathBuf::from("/project/component.tsx"),
            "export const PriceTag = ({ left, size }) => (\n  <Rect x={left} rotation={10} scale={size * 2} />\n);\n"
                .to_owned(),
        ),
        (
            PathBuf::from("/project/scene.tsx"),
            "export const Scene = () => (\n  <PriceTag left={40} size={1} />\n);\n".to_owned(),
        ),
    ]);
    resolve_targets(read_from(&files), &price_tag_report(), None).expect("the element resolves")
}

/// The override sites of [`mixed_targets`].
fn mixed_override_sites() -> Vec<(PropKey, OverrideSite)> {
    override_sites(&mixed_targets(), &price_tag_report())
}

/// An override of the element or call site at line 2, column 3 of `file`.
fn override_at(file: &str, prop: &str, value: f64) -> PropOverride {
    PropOverride {
        file: PathBuf::from(file),
        line: 2,
        column: 3,
        prop: prop.to_owned(),
        value,
    }
}

#[test]
fn a_drag_overrides_the_prop_its_write_changes() {
    let sites = mixed_override_sites();
    let at = override_at;
    assert_eq!(
        prop_overrides(
            &sites,
            &[
                (TransformProp::X, 64.0),
                (TransformProp::Y, 8.0),
                (TransformProp::Rotation, 25.0),
            ]
        ),
        vec![
            at("/project/scene.tsx", "left", 64.0),
            at("/project/component.tsx", "y", 8.0),
            at("/project/component.tsx", "rotation", 25.0),
        ]
    );
    assert_eq!(prop_overrides(&sites, &[(TransformProp::Scale, 3.0)]), []);
}

#[test]
fn a_drag_overrides_nothing_before_the_pointer_moves_or_while_playing() {
    let frame = drag_frame(IDENTITY, pt(10.0, 20.0));
    let editable = vec![TransformProp::X, TransformProp::Y, TransformProp::Rotation];
    let mut drag = TransformDrag::new(
        frame.clone(),
        Handle::Move,
        pt(0.0, 0.0),
        editable.clone(),
        price_tag_report().props,
        mixed_override_sites(),
    );
    assert_eq!(drag.overrides(false), []);

    let values = drag_values(
        &frame,
        Handle::Move,
        (pt(0.0, 0.0), pt(4.0, 0.0)),
        false,
        0.0,
        &editable,
    );
    assert!(drag.follow(values.clone()));
    assert!(!drag.follow(values));
    assert_eq!(drag.overrides(false).len(), 2);
    assert_eq!(drag.overrides(true), []);
}

#[test]
fn each_handle_previews_every_prop_it_changes() {
    let frame = drag_frame(IDENTITY, pt(10.0, 20.0));
    let received = ReceivedProps {
        rotation: Some(5.0),
        ..ReceivedProps::default()
    };
    let preview = |handle, editable: &[TransformProp], values: &[(TransformProp, f64)]| {
        preview_values(&frame, handle, editable, values, &received)
    };
    // Unchanged props keep the value the runtime saw, or the frame's.
    assert_values(
        &preview(Handle::Rotate, &UNIFORM, &[]),
        &[(TransformProp::Rotation, 5.0)],
    );
    assert_values(
        &preview(Handle::Rotate, &UNIFORM, &[(TransformProp::Rotation, 30.0)]),
        &[(TransformProp::Rotation, 35.0)],
    );
    assert_values(
        &preview(Handle::Corner(0), &UNIFORM, &[]),
        &[(TransformProp::Scale, 1.0)],
    );
    assert_values(
        &preview(
            Handle::Corner(2),
            &PER_AXIS,
            &[(TransformProp::ScaleX, 2.0)],
        ),
        &[(TransformProp::ScaleX, 2.0), (TransformProp::ScaleY, 1.0)],
    );
    // Edges 1 and 3 scale x, edges 0 and 2 y; a uniform scale either way.
    assert_values(
        &preview(Handle::Edge(1), &PER_AXIS, &[]),
        &[(TransformProp::ScaleX, 1.0)],
    );
    assert_values(
        &preview(Handle::Edge(0), &PER_AXIS, &[]),
        &[(TransformProp::ScaleY, 1.0)],
    );
    assert_values(
        &preview(Handle::Edge(0), &UNIFORM, &[]),
        &[(TransformProp::Scale, 1.0)],
    );
    // A locked prop is left out.
    assert_values(
        &preview(Handle::Corner(0), &[TransformProp::ScaleX], &[]),
        &[(TransformProp::ScaleX, 1.0)],
    );
}

#[test]
fn a_move_drag_overrides_both_axes_from_its_first_frame() {
    // The runtime saw `x` 40 (`left`) and no `y`; the layer is drawn at
    // (10, 20).
    let sites = mixed_override_sites();
    let frame = drag_frame(IDENTITY, pt(10.0, 20.0));
    let received = price_tag_report().props;
    let editable = [TransformProp::X, TransformProp::Y, TransformProp::Rotation];
    let overrides = |now: Point| {
        let values = drag_values(
            &frame,
            Handle::Move,
            (pt(0.0, 0.0), now),
            false,
            0.0,
            &editable,
        );
        prop_overrides(
            &sites,
            &preview_values(&frame, Handle::Move, &editable, &values, &received),
        )
    };
    // Only `x` moved; `y` keeps its start value, at the element.
    assert_eq!(
        overrides(pt(4.0, 0.0)),
        vec![
            override_at("/project/scene.tsx", "left", 44.0),
            override_at("/project/component.tsx", "y", 20.0),
        ]
    );
    // The positions stay the same for the whole drag, so the runtime does
    // not remount: only the values change.
    assert_eq!(
        overrides(pt(4.0, 6.0)),
        vec![
            override_at("/project/scene.tsx", "left", 44.0),
            override_at("/project/component.tsx", "y", 26.0),
        ]
    );
    assert_eq!(
        overrides(pt(0.0, 0.0)),
        vec![
            override_at("/project/scene.tsx", "left", 40.0),
            override_at("/project/component.tsx", "y", 20.0),
        ]
    );
}

#[test]
fn a_value_written_on_the_element_reads_only_its_file() {
    // Only the element's file can be read: no prop passes through.
    let texts = Texts::from([(
        PathBuf::from("/project/scene.tsx"),
        TRANSFORM_FIXTURE.to_owned(),
    )]);
    let report = LayerSourceReport {
        chain: vec![
            primitive_report(2, 3, x_only()).chain[0].clone(),
            entry_at("Scene", ElementKind::Component, "/project/index.tsx", 2),
        ],
        ..primitive_report(2, 3, x_only())
    };

    let targets = resolve_targets(read_from(&texts), &report, None).expect("the element resolves");

    assert!(
        targets
            .targets
            .iter()
            .all(|(_, target)| target.file == Path::new("/project/scene.tsx"))
    );
}

#[test]
fn a_drag_writes_two_files() {
    let texts = price_tag_files();
    let report = price_tag_report();
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();

    let written = commit_files(
        &texts,
        &targets,
        &report,
        &[
            num(TransformProp::X, 60.0),
            num(TransformProp::Rotation, 45.0),
        ],
    )
    .expect("both targets are editable");

    assert_eq!(
        written,
        vec![
            (
                PathBuf::from("/project/component.tsx"),
                PRICE_TAG.replace("rotation={10}", "rotation={45}")
            ),
            (
                PathBuf::from("/project/scene.tsx"),
                PRICE_TAG_SCENE.replace("x={40}", "x={60}")
            ),
        ]
    );
}

#[test]
fn a_value_missing_at_the_call_site_is_inserted_there() {
    let component = PRICE_TAG.replace("({ x })", "({ x = 0 })");
    let scene = PRICE_TAG_SCENE.replace(" x={40}", "");
    let texts = Texts::from([
        (PathBuf::from("/project/component.tsx"), component),
        (PathBuf::from("/project/scene.tsx"), scene.clone()),
    ]);
    let report = LayerSourceReport {
        props: ReceivedProps {
            x: Some(0.0),
            rotation: Some(10.0),
            ..ReceivedProps::default()
        },
        ..price_tag_report()
    };
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    let Some(EditTarget::PassThrough { target, .. }) = targets.transform(TransformProp::X) else {
        panic!(
            "x passes through: {:?}",
            targets.transform(TransformProp::X)
        );
    };
    assert!(matches!(**target, EditTarget::Missing { .. }));

    let written = commit_files(&texts, &targets, &report, &[num(TransformProp::X, 60.0)]).unwrap();

    assert_eq!(
        written,
        vec![(
            PathBuf::from("/project/scene.tsx"),
            scene.replace("<PriceTag />", "<PriceTag x={60} />")
        )]
    );
}

#[test]
fn a_component_called_in_its_own_file_is_written_once() {
    let text = format!("{PRICE_TAG}{PRICE_TAG_SCENE}");
    let texts = Texts::from([(PathBuf::from("/project/scene.tsx"), text.clone())]);
    let report = LayerSourceReport {
        chain: vec![
            entry_at("Rect", ElementKind::Primitive, "/project/scene.tsx", 2),
            entry_at("PriceTag", ElementKind::Component, "/project/scene.tsx", 5),
        ],
        ..price_tag_report()
    };
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();

    let written = commit_files(
        &texts,
        &targets,
        &report,
        &[
            num(TransformProp::X, 60.0),
            num(TransformProp::Rotation, 45.0),
        ],
    )
    .unwrap();

    assert_eq!(
        written,
        vec![(
            PathBuf::from("/project/scene.tsx"),
            text.replace("rotation={10}", "rotation={45}")
                .replace("x={40}", "x={60}")
        )]
    );
}

#[test]
fn a_changed_call_site_aborts() {
    let report = price_tag_report();
    let targets = resolve_targets(read_from(&price_tag_files()), &report, None).unwrap();
    // `<PriceTag` moved within its line, so the report's position misses it.
    let moved = PRICE_TAG_SCENE.replace("  <PriceTag", "    <PriceTag");
    // The call stayed but its `x` became longer, so the span moved.
    let widened = PRICE_TAG_SCENE.replace("x={40}", "x={400}");

    for scene in [moved, widened] {
        let mut texts = price_tag_files();
        texts.insert(PathBuf::from("/project/scene.tsx"), scene);
        // Rotation is in the unchanged file, but the edit is one: nothing
        // is written.
        assert_eq!(
            commit_files(
                &texts,
                &targets,
                &report,
                &[
                    num(TransformProp::X, 60.0),
                    num(TransformProp::Rotation, 45.0)
                ],
            ),
            Err(CommitError::SourceChanged)
        );
    }
}

#[test]
fn a_call_site_outside_the_project_locks_only_its_targets() {
    let texts = price_tag_files();
    let report = price_tag_report();
    let mut targets = resolve_targets(read_from(&texts), &report, None).unwrap();

    lock_outside_project(&mut targets, |file| file == Path::new("/project/scene.tsx"));

    assert_eq!(
        targets.transform(TransformProp::X),
        Some(&EditTarget::Locked(LockReason::OutsideProject))
    );
    assert_eq!(
        commit_files(&texts, &targets, &report, &[num(TransformProp::X, 60.0)]),
        Err(CommitError::NotEditable("X".to_owned()))
    );
    // The element's own file is still written.
    assert_eq!(
        commit_files(
            &texts,
            &targets,
            &report,
            &[num(TransformProp::Rotation, 45.0)]
        ),
        Ok(vec![(
            PathBuf::from("/project/component.tsx"),
            PRICE_TAG.replace("rotation={10}", "rotation={45}")
        )])
    );
}

#[test]
fn a_value_passed_on_again_still_commits_the_element_s_own_props() {
    // `Card` passes its own `x` on to `<PriceTag>`: a second hop.
    let mut texts = price_tag_files();
    texts.insert(
        PathBuf::from("/project/scene.tsx"),
        "export const Card = ({ x }) => (\n  <PriceTag x={x} />\n);\n".to_owned(),
    );
    let report = price_tag_report();
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();

    assert_eq!(
        targets.transform(TransformProp::X),
        Some(&EditTarget::Locked(LockReason::ParameterOfCaller {
            component: "Card".to_owned()
        }))
    );
    // The call site was read, so a commit reads it again.
    assert_eq!(
        targets.files,
        [
            PathBuf::from("/project/component.tsx"),
            PathBuf::from("/project/scene.tsx")
        ]
    );
    let read: Texts = targets
        .files
        .iter()
        .map(|file| (file.clone(), texts[file].clone()))
        .collect();
    assert_eq!(
        commit_files(
            &read,
            &targets,
            &report,
            &[num(TransformProp::Rotation, 45.0)]
        ),
        Ok(vec![(
            PathBuf::from("/project/component.tsx"),
            PRICE_TAG.replace("rotation={10}", "rotation={45}")
        )])
    );
}

/// `PriceTag` and the `Frame` it passes `<Rect>` to as children are both
/// `memo` of an arrow, which the runtime reports as "Anonymous".
const MEMO_PRICE_TAG: &str = "export const Frame = memo(({ children }) => <Group>{children}</Group>);\nexport const PriceTag = memo(({ x = 0 }) => (\n  <Frame>\n    <Rect x={x} />\n  </Frame>\n));\n";

fn memo_report(chain: Vec<LayerSource>) -> LayerSourceReport {
    LayerSourceReport {
        props: ReceivedProps {
            x: Some(40.0),
            ..ReceivedProps::default()
        },
        ..report_of(chain)
    }
}

#[test]
fn an_anonymous_call_site_must_call_the_enclosing_component() {
    let texts = Texts::from([
        (
            PathBuf::from("/project/component.tsx"),
            MEMO_PRICE_TAG.to_owned(),
        ),
        (
            PathBuf::from("/project/scene.tsx"),
            PRICE_TAG_SCENE.to_owned(),
        ),
    ]);
    let rect = LayerSource {
        column: 5,
        ..entry_at("Rect", ElementKind::Primitive, "/project/component.tsx", 4)
    };
    let price_tag_call = entry_at("Anonymous", ElementKind::Component, "/project/scene.tsx", 2);

    // The nearest anonymous entry is `<Frame>`, not a call of `PriceTag`.
    let report = memo_report(vec![
        rect.clone(),
        entry_at(
            "Anonymous",
            ElementKind::Component,
            "/project/component.tsx",
            3,
        ),
        price_tag_call.clone(),
    ]);
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    assert_eq!(
        targets.transform(TransformProp::X),
        Some(&EditTarget::Locked(LockReason::CallSiteNotFound {
            component: "PriceTag".to_owned()
        }))
    );
    assert_eq!(
        commit_files(&texts, &targets, &report, &[num(TransformProp::X, 60.0)]),
        Err(CommitError::NotEditable("X".to_owned()))
    );

    // An anonymous entry whose tag is `<PriceTag>` is its call.
    let report = memo_report(vec![rect, price_tag_call]);
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    assert_eq!(
        targets.file_of(&TransformProp::X.key()),
        Path::new("/project/scene.tsx")
    );
    assert!(matches!(
        targets.transform(TransformProp::X),
        Some(EditTarget::PassThrough { .. })
    ));
}

#[test]
fn an_unreadable_call_site_locks_only_the_props_passed_through() {
    let texts = Texts::from([(
        PathBuf::from("/project/component.tsx"),
        PRICE_TAG.to_owned(),
    )]);
    let report = price_tag_report();

    let targets = resolve_targets(read_from(&texts), &report, None).expect("the element resolves");

    assert_eq!(
        targets.transform(TransformProp::X),
        Some(&EditTarget::Locked(LockReason::CallSiteNotFound {
            component: "PriceTag".to_owned()
        }))
    );
    assert_eq!(targets.files, [PathBuf::from("/project/component.tsx")]);
    assert_eq!(
        commit_files(
            &texts,
            &targets,
            &report,
            &[num(TransformProp::Rotation, 45.0)]
        ),
        Ok(vec![(
            PathBuf::from("/project/component.tsx"),
            PRICE_TAG.replace("rotation={10}", "rotation={45}")
        )])
    );
}

#[test]
fn an_edit_is_written_at_the_call_site_only_when_every_editable_prop_passes_through() {
    let price_tag_call = SourcePos {
        file: PathBuf::from("/project/scene.tsx"),
        line: 2,
        column: 3,
    };
    let component = "export const PriceTag = ({ x, y = 0, rotation = 0, scale = 1 }) => (\n  <Rect x={x} y={y} rotation={rotation} scale={scale} />\n);\n";
    let texts = Texts::from([
        (
            PathBuf::from("/project/component.tsx"),
            component.to_owned(),
        ),
        (
            PathBuf::from("/project/scene.tsx"),
            PRICE_TAG_SCENE.to_owned(),
        ),
    ]);
    let report = LayerSourceReport {
        props: ReceivedProps {
            x: Some(40.0),
            y: Some(0.0),
            rotation: Some(0.0),
            scale: Some(1.0),
            ..ReceivedProps::default()
        },
        ..price_tag_report()
    };
    let mut targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    assert_eq!(targets.written_at(), Some(&price_tag_call));

    // A locked prop on the element is not written, so it does not count.
    targets.targets[2].1 = Target {
        file: PathBuf::from("/project/component.tsx"),
        path: "rotation".to_owned(),
        target: EditTarget::Locked(LockReason::Spread),
        call_site: None,
        inputs: Vec::new(),
    };
    assert_eq!(targets.written_at(), Some(&price_tag_call));

    // `rotation` written on the element changes every instance.
    let targets =
        resolve_targets(read_from(&price_tag_files()), &price_tag_report(), None).unwrap();
    assert_eq!(targets.written_at(), None);

    // Nothing editable writes nowhere.
    let mut locked = targets;
    lock_outside_project(&mut locked, |_| true);
    assert_eq!(locked.written_at(), None);
}

#[test]
fn a_pass_through_row_names_its_call_site() {
    let call_site = SourcePos {
        file: PathBuf::from("/project/scenes/price.tsx"),
        line: 18,
        column: 5,
    };
    assert_eq!(
        set_at_text(&call_site, Path::new("/project")),
        "Set at scenes/price.tsx:18"
    );
}

fn prop(name: &str) -> PropKey {
    PropKey::Prop(name.to_owned())
}

fn number_field(default_value: Option<f64>) -> PrimitivePropertyField {
    PrimitivePropertyField::Number {
        default_value,
        min: None,
        max: None,
        step: None,
        integer: None,
    }
}

/// A number field scrubbed `step` per pixel, as the tables declare `scale`
/// and the anchors.
fn stepped_field(default_value: f64, step: f64) -> PrimitivePropertyField {
    PrimitivePropertyField::Number {
        default_value: Some(default_value),
        min: None,
        max: None,
        step: Some(step),
        integer: None,
    }
}

fn property_table(
    groups: Vec<(&str, Vec<(&str, PrimitivePropertyField)>)>,
) -> PrimitivePropertyTable {
    PrimitivePropertyTable {
        size: None,
        groups: groups
            .into_iter()
            .map(|(name, fields)| PrimitivePropertyGroup {
                name: name.to_owned(),
                fields: fields
                    .into_iter()
                    .map(|(key, field)| PrimitivePropertyRow {
                        key: key.to_owned(),
                        field,
                        required: false,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn rect_table() -> PrimitivePropertyTable {
    property_table(vec![
        (
            "Transform",
            vec![
                ("x", number_field(Some(0.0))),
                ("y", number_field(Some(0.0))),
                ("rotation", number_field(Some(0.0))),
                ("scale", stepped_field(1.0, 0.01)),
                (
                    "opacity",
                    PrimitivePropertyField::Number {
                        default_value: Some(1.0),
                        min: Some(0.0),
                        max: Some(1.0),
                        step: None,
                        integer: None,
                    },
                ),
            ],
        ),
        (
            "Appearance",
            vec![
                (
                    "fill",
                    PrimitivePropertyField::Color {
                        default_value: None,
                    },
                ),
                ("cornerRadius", number_field(Some(0.0))),
            ],
        ),
    ])
}

fn text_table() -> PrimitivePropertyTable {
    property_table(vec![
        (
            "Transform",
            vec![
                ("x", number_field(Some(0.0))),
                ("anchorY", number_field(Some(0.0))),
            ],
        ),
        (
            "Text",
            vec![
                (
                    "children",
                    PrimitivePropertyField::Text {
                        default_value: None,
                    },
                ),
                (
                    "fontFamily",
                    PrimitivePropertyField::String {
                        default_value: None,
                    },
                ),
                ("fontSize", number_field(None)),
                ("fontWeight", number_field(None)),
                (
                    "color",
                    PrimitivePropertyField::Color {
                        default_value: None,
                    },
                ),
            ],
        ),
    ])
}

/// The `<Text>` at line 2, column 3 of `file`, which received `props`.
fn text_report(file: &Path, props: &[(&str, ReceivedValue)]) -> LayerSourceReport {
    let mut report = primitive_report(2, 3, ReceivedProps::default());
    report.chain[0].name = "Text".to_owned();
    report.chain[0].file = file.to_path_buf();
    for (name, value) in props {
        report
            .props
            .others
            .insert((*name).to_owned(), value.clone());
    }
    report
}

/// Resolves `report` with `table`, `text` being the text of every file.
fn resolve_table(
    text: &str,
    report: &LayerSourceReport,
    table: &PrimitivePropertyTable,
) -> ElementTargets {
    resolve_targets(|_| Ok(text.to_owned()), report, Some(table)).expect("the element resolves")
}

#[test]
fn table_defaults_per_kind() {
    assert_eq!(
        table_default(&number_field(Some(2.0))),
        Some(EditValue::Number(2.0))
    );
    assert_eq!(table_default(&number_field(None)), None);
    assert_eq!(
        table_default(&PrimitivePropertyField::Select {
            options: vec!["contain".to_owned(), "cover".to_owned()],
            default_value: Some("cover".to_owned()),
        }),
        Some(EditValue::String("cover".to_owned()))
    );
    assert_eq!(
        table_default(&PrimitivePropertyField::Boolean {
            default_value: Some(false)
        }),
        Some(EditValue::Bool(false))
    );
    assert_eq!(table_default(&PrimitivePropertyField::Path), None);
}

#[test]
fn a_table_resolves_every_row_in_order() {
    let text = "export const Scene = () => (\n  <Rect x={10} fill=\"#fff\" width={10} height={10} />\n);\n";
    let mut report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(10.0),
            ..ReceivedProps::default()
        },
    );
    report
        .props
        .others
        .insert("fill".to_owned(), ReceivedValue::String("#fff".to_owned()));

    let targets = resolve_table(text, &report, &rect_table());

    assert_eq!(
        keys(&targets),
        [
            "x",
            "y",
            "rotation",
            "scale",
            "opacity",
            "fill",
            "cornerRadius"
        ]
        .map(prop)
    );
    assert!(matches!(
        targets.target(&prop("fill")),
        Some(EditTarget::Literal { value: EditValue::String(fill), .. }) if fill == "#fff"
    ));
    assert!(matches!(
        targets.target(&prop("cornerRadius")),
        Some(EditTarget::Missing { .. })
    ));
    assert!(matches!(
        targets.target(&prop("scale")),
        Some(EditTarget::Missing { .. })
    ));
}

#[test]
fn per_axis_scale_replaces_scale_in_place() {
    let text = "export const Scene = () => (\n  <Rect x={10} scaleX={2} />\n);\n";
    let report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(10.0),
            scale_x: Some(2.0),
            ..ReceivedProps::default()
        },
    );

    let targets = resolve_table(text, &report, &rect_table());

    assert_eq!(
        keys(&targets),
        [
            "x",
            "y",
            "rotation",
            "scaleX",
            "scaleY",
            "opacity",
            "fill",
            "cornerRadius"
        ]
        .map(prop)
    );
}

#[test]
fn text_shorthand_edits_the_style_property() {
    let file = Path::new("/project/scene.tsx");
    let text = "export const Scene = () => (\n  <Text style={{ fontSize: 48 }} fontWeight={700}>A</Text>\n);\n";
    let report = text_report(
        file,
        &[
            ("style.fontSize", ReceivedValue::Number(48.0)),
            ("fontWeight", ReceivedValue::Number(700.0)),
            ("children", ReceivedValue::String("A".to_owned())),
        ],
    );

    let targets = resolve_table(text, &report, &text_table());

    let font_size = targets.entry(&prop("fontSize")).unwrap();
    assert_eq!(font_size.path, "style.fontSize");
    assert!(matches!(
        font_size.target,
        EditTarget::Literal { value: EditValue::Number(size), .. } if size == 48.0
    ));
    let font_weight = targets.entry(&prop("fontWeight")).unwrap();
    assert_eq!(font_weight.path, "fontWeight");
    assert!(matches!(
        font_weight.target,
        EditTarget::Literal { value: EditValue::Number(weight), .. } if weight == 700.0
    ));

    // `style` is not an object literal: it may set either.
    let computed = text.replace("{{ fontSize: 48 }}", "{mono(20)}");
    let targets = resolve_table(&computed, &report, &text_table());
    for key in ["fontSize", "fontWeight"] {
        let row = targets.entry(&prop(key)).unwrap();
        assert_eq!(row.path, key);
        assert_eq!(row.target, EditTarget::Locked(LockReason::StyleMaySet));
    }
}

#[test]
fn a_literal_of_another_kind_locks() {
    let text = "export const Scene = () => (\n  <Text anchorY=\"baseline\">A</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[
            ("anchorY", ReceivedValue::String("baseline".to_owned())),
            ("children", ReceivedValue::String("A".to_owned())),
        ],
    );

    let targets = resolve_table(text, &report, &text_table());

    assert_eq!(
        targets.target(&prop("anchorY")),
        Some(&EditTarget::Locked(LockReason::Expression {
            source: "\"baseline\"".to_owned()
        }))
    );
    // A string row with a string literal stays editable.
    assert!(matches!(
        targets.target(&prop("children")),
        Some(EditTarget::Literal { .. })
    ));
}

const TAG: &str = "export function Tag({ padding = 8 }) {\n  const w = 100;\n  return <Rect width={w + padding * 2} height={10} />;\n}\n";
const TAG_SCENE: &str = "export const Scene = () => (\n  <Tag padding={16} />\n);\n";

fn tag_files() -> Texts {
    Texts::from([
        (PathBuf::from("/project/component.tsx"), TAG.to_owned()),
        (PathBuf::from("/project/scene.tsx"), TAG_SCENE.to_owned()),
    ])
}

/// The `<Rect>` in `Tag`, called from the scene as `<Tag padding={16} />`.
fn tag_report() -> LayerSourceReport {
    let rect = LayerSource {
        column: 10,
        ..entry_at("Rect", ElementKind::Primitive, "/project/component.tsx", 3)
    };
    LayerSourceReport {
        props: ReceivedProps {
            others: [
                ("width".to_owned(), ReceivedValue::Number(132.0)),
                ("height".to_owned(), ReceivedValue::Number(10.0)),
            ]
            .into(),
            ..ReceivedProps::default()
        },
        ..report_of(vec![
            rect,
            entry_at("Tag", ElementKind::Component, "/project/scene.tsx", 2),
        ])
    }
}

fn size_table() -> PrimitivePropertyTable {
    property_table(vec![(
        "Size",
        vec![
            ("width", number_field(None)),
            ("height", number_field(None)),
        ],
    )])
}

#[test]
fn an_expression_row_lists_its_inputs() {
    let report = tag_report();

    let targets = resolve_targets(read_from(&tag_files()), &report, Some(&size_table()))
        .expect("the element resolves");

    let width = targets.entry(&prop("width")).unwrap();
    assert!(matches!(
        width.target,
        EditTarget::Locked(LockReason::Expression { .. })
    ));
    let names: Vec<&str> = width.inputs.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["w", "padding"]);
    let (_, w) = &width.inputs[0];
    assert_eq!(w.file, Path::new("/project/component.tsx"));
    assert_eq!(w.call_site, None);
    assert!(matches!(
        w.target,
        EditTarget::Literal { value: EditValue::Number(value), .. } if value == 100.0
    ));
    let (_, padding) = &width.inputs[1];
    assert_eq!(padding.file, Path::new("/project/scene.tsx"));
    assert_eq!(
        padding.call_site,
        Some(SourcePos {
            file: PathBuf::from("/project/scene.tsx"),
            line: 2,
            column: 3,
        })
    );
    assert!(matches!(padding.target, EditTarget::PassThrough { .. }));
    // Only an expression row has inputs.
    assert!(targets.entry(&prop("height")).unwrap().inputs.is_empty());
    assert_eq!(
        targets.files,
        [
            PathBuf::from("/project/component.tsx"),
            PathBuf::from("/project/scene.tsx")
        ]
    );
}

#[test]
fn committing_an_input_writes_its_target() {
    let texts = tag_files();
    let report = tag_report();
    let targets = resolve_targets(read_from(&texts), &report, Some(&size_table())).unwrap();
    let padding = PropKey::Input {
        row: "width".to_owned(),
        name: "padding".to_owned(),
    };

    let written = commit_files(
        &texts,
        &targets,
        &report,
        &[(padding, Some(EditValue::Number(24.0)))],
    );

    assert_eq!(
        written,
        Ok(vec![(
            PathBuf::from("/project/scene.tsx"),
            TAG_SCENE.replace("padding={16}", "padding={24}")
        )])
    );
}

#[test]
fn a_text_with_outer_spaces_is_written_in_braces() {
    let text = "export const Scene = () => (\n  <Text>Sale</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[("children", ReceivedValue::String("Sale".to_owned()))],
    );
    let targets = resolve_table(text, &report, &text_table());

    let written = commit_in(
        text,
        &targets,
        &report,
        &[(
            prop("children"),
            Some(EditValue::String(" Sale ".to_owned())),
        )],
    );

    assert_eq!(written, Ok(text.replace(">Sale<", ">{\" Sale \"}<")));
}

#[test]
fn a_font_row_beside_a_style_object_inserts_the_shorthand() {
    let dir = std::env::temp_dir().join(format!(
        "celesta-font-row-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("scene.tsx");
    let text = "export const Scene = () => (\n  <Text style={{ fontWeight: 700 }}>A</Text>\n);\n";
    fs::write(&file, text).unwrap();
    let report = text_report(
        &file,
        &[
            ("style.fontWeight", ReceivedValue::Number(700.0)),
            ("children", ReceivedValue::String("A".to_owned())),
        ],
    );
    let read = |path: &Path| fs::read_to_string(path).map_err(|error| error.to_string());
    let targets = resolve_targets(read, &report, Some(&text_table())).unwrap();

    // The object does not set it: the shorthand is the prop to write.
    let font_size = targets.entry(&prop("fontSize")).unwrap();
    assert_eq!(font_size.path, "fontSize");
    assert!(matches!(font_size.target, EditTarget::Missing { .. }));

    let files = write_all_recorded(targets.files.clone(), |texts| {
        commit_files(
            texts,
            &targets,
            &report,
            &[(prop("fontSize"), Some(EditValue::Number(48.0)))],
        )
        .map_err(|error| error.to_string())
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "export const Scene = () => (\n  <Text style={{ fontWeight: 700 }} fontSize={48}>A</Text>\n);\n"
    );
    let mut history = EditHistory::default();
    history.push(HistoryEntry { files });
    history.undo().unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), text);
    fs::remove_dir_all(&dir).ok();

    // An object that sets it is written inside.
    let both = "export const Scene = () => (\n  <Text style={{ fontSize: 40, fontWeight: 700 }}>A</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[
            ("style.fontSize", ReceivedValue::Number(40.0)),
            ("style.fontWeight", ReceivedValue::Number(700.0)),
            ("children", ReceivedValue::String("A".to_owned())),
        ],
    );
    let targets = resolve_table(both, &report, &text_table());
    assert_eq!(
        commit_in(
            both,
            &targets,
            &report,
            &[(prop("fontSize"), Some(EditValue::Number(48.0)))]
        ),
        Ok(both.replace("fontSize: 40", "fontSize: 48"))
    );
}

#[test]
fn impact_and_the_locked_summary_stay_transform_only() {
    let scene = SourcePos {
        file: PathBuf::from("/project/scene.tsx"),
        line: 2,
        column: 3,
    };
    let other = SourcePos {
        file: PathBuf::from("/project/other.tsx"),
        line: 7,
        column: 3,
    };
    let call = celesta_source_edit::ElementRef {
        start: 0,
        end_of_attributes: 0,
        children: None,
    };
    let passed = |path: &str, site: &SourcePos| Target {
        file: site.file.clone(),
        path: path.to_owned(),
        target: EditTarget::PassThrough {
            call_site: call,
            param: path.to_owned(),
            target: Box::new(EditTarget::Literal {
                span: Span { start: 1, end: 2 },
                value: EditValue::Number(1.0),
                form: LiteralForm::Number,
            }),
        },
        call_site: Some(site.clone()),
        inputs: Vec::new(),
    };
    let targets = ElementTargets {
        file: PathBuf::from("/project/component.tsx"),
        files: vec![PathBuf::from("/project/component.tsx")],
        element: call,
        targets: vec![
            (prop("x"), passed("x", &scene)),
            (
                prop("fontSize"),
                Target {
                    file: PathBuf::from("/project/component.tsx"),
                    path: "fontSize".to_owned(),
                    target: EditTarget::Locked(LockReason::StyleMaySet),
                    call_site: None,
                    inputs: Vec::new(),
                },
            ),
            (prop("color"), passed("color", &other)),
        ],
        table: Some(text_table()),
        component: None,
    };

    assert_eq!(targets.written_at(), Some(&scene));
    assert!(locked_summary(&targets).is_empty());
}

fn num_range(default: f64, min: f64, max: f64) -> PrimitivePropertyField {
    PrimitivePropertyField::Number {
        default_value: Some(default),
        min: Some(min),
        max: Some(max),
        step: None,
        integer: None,
    }
}

fn fit_field() -> PrimitivePropertyField {
    PrimitivePropertyField::Select {
        options: vec!["contain".to_owned(), "cover".to_owned()],
        default_value: Some("cover".to_owned()),
    }
}

#[test]
fn row_labels() {
    for (key, label) in [
        ("cornerRadius", "Corner radius"),
        ("children", "Text"),
        ("scaleX", "Scale X"),
        ("anchorY", "Anchor Y"),
        ("strokeWidth", "Stroke width"),
        ("x", "X"),
        ("opacity", "Opacity"),
        ("src", "Source"),
    ] {
        assert_eq!(row_label(key), label, "{key}");
    }
    // A commit error names the row the way the panel labels it.
    assert_eq!(
        CommitError::NotEditable(prop("fontSize").label()).to_string(),
        "Font size can't be edited here"
    );
}

#[test]
fn control_kinds() {
    let none = || None;
    assert_eq!(control_kind(&number_field(None)), ControlKind::Number);
    assert_eq!(
        control_kind(&PrimitivePropertyField::String {
            default_value: none()
        }),
        ControlKind::String
    );
    assert_eq!(
        control_kind(&PrimitivePropertyField::Text {
            default_value: none()
        }),
        ControlKind::Text
    );
    assert_eq!(
        control_kind(&PrimitivePropertyField::Boolean {
            default_value: None
        }),
        ControlKind::Boolean
    );
    assert_eq!(
        control_kind(&PrimitivePropertyField::Color {
            default_value: none()
        }),
        ControlKind::Color
    );
    assert_eq!(
        control_kind(&fit_field()),
        ControlKind::Select(vec!["contain".to_owned(), "cover".to_owned()])
    );
    assert_eq!(
        control_kind(&PrimitivePropertyField::Path),
        ControlKind::Path
    );
}

#[test]
fn parse_row_values() {
    let number = number_field(None);
    assert_eq!(
        parse_row_value(&ControlKind::Number, Some(&number), "12.5"),
        Ok(EditValue::Number(12.5))
    );
    assert_eq!(
        parse_row_value(&ControlKind::Number, Some(&number), "twelve"),
        Err("Enter a number".to_owned())
    );
    let opacity = num_range(1.0, 0.0, 1.0);
    assert_eq!(
        parse_row_value(&ControlKind::Number, Some(&opacity), "1.5"),
        Err("Enter a number from 0 to 1".to_owned())
    );
    assert_eq!(
        parse_row_value(&ControlKind::Number, Some(&opacity), "0.5"),
        Ok(EditValue::Number(0.5))
    );
    for color in ["#ff8800", "accent"] {
        assert_eq!(
            parse_row_value(&ControlKind::Color, None, color),
            Ok(EditValue::String(color.to_owned()))
        );
    }
    for color in ["#ff88", "two words", "9lives"] {
        assert_eq!(
            parse_row_value(&ControlKind::Color, None, color),
            Err("Enter a color: #RRGGBB, #RRGGBBAA, or a theme color name".to_owned()),
            "{color}"
        );
    }
    let fit = control_kind(&fit_field());
    assert!(parse_row_value(&fit, Some(&fit_field()), "fill").is_err());
    assert_eq!(
        parse_row_value(&fit, Some(&fit_field()), "cover"),
        Ok(EditValue::String("cover".to_owned()))
    );
    assert_eq!(
        parse_row_value(&ControlKind::Text, None, "A\nB"),
        Ok(EditValue::String("A\nB".to_owned()))
    );
    // Text keeps its outer spaces.
    assert_eq!(
        parse_row_value(&ControlKind::String, None, " Sale "),
        Ok(EditValue::String(" Sale ".to_owned()))
    );
}

#[test]
fn select_value_outside_the_options_is_shown() {
    let report = primitive_report(2, 3, ReceivedProps::default());
    let fit = local_target(
        "fit",
        EditTarget::Literal {
            span: Span { start: 1, end: 7 },
            value: EditValue::String("fill".to_owned()),
            form: LiteralForm::Quoted('"'),
        },
    );
    assert_eq!(
        row_text("fit", &fit, Some(&fit_field()), &report),
        ("fill".to_owned(), String::new())
    );
}

#[test]
fn missing_rows_show_the_table_default() {
    let missing = local_target(
        "cornerRadius",
        EditTarget::Missing {
            insert_at: 0,
            default: None,
        },
    );
    let field = number_field(Some(2.0));
    let plain = primitive_report(2, 3, ReceivedProps::default());
    assert_eq!(
        row_text("cornerRadius", &missing, Some(&field), &plain),
        (String::new(), "2".to_owned())
    );
    let mut received = plain.clone();
    received
        .props
        .others
        .insert("cornerRadius".to_owned(), ReceivedValue::Number(5.0));
    assert_eq!(
        row_text("cornerRadius", &missing, Some(&field), &received),
        (String::new(), "5".to_owned())
    );
}

#[test]
fn the_scale_axis_placeholder_of_a_parameter_default() {
    // `({ sx = 3 }) => <Rect scale={2} scaleX={sx} />` called without `sx`.
    let report = primitive_report(
        2,
        3,
        ReceivedProps {
            scale: Some(2.0),
            ..ReceivedProps::default()
        },
    );
    let through = local_target(
        "scaleX",
        EditTarget::PassThrough {
            call_site: celesta_source_edit::ElementRef {
                start: 0,
                end_of_attributes: 0,
                children: None,
            },
            param: "sx".to_owned(),
            target: Box::new(EditTarget::Missing {
                insert_at: 0,
                default: Some(EditValue::Number(3.0)),
            }),
        },
    );
    let field = number_field(Some(1.0));
    assert_eq!(
        row_text("scaleX", &through, Some(&field), &report),
        (String::new(), "3".to_owned())
    );
    // An axis absent from the element itself is drawn at the written scale.
    let missing = local_target(
        "scaleX",
        EditTarget::Missing {
            insert_at: 0,
            default: Some(EditValue::Number(1.0)),
        },
    );
    assert_eq!(
        row_text("scaleX", &missing, Some(&field), &report),
        (String::new(), "2".to_owned())
    );
}

#[test]
fn locked_rows_show_the_received_value() {
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[("children", ReceivedValue::String("Sale".to_owned()))],
    );
    let locked = local_target(
        "children",
        EditTarget::Locked(LockReason::Expression {
            source: "label".to_owned(),
        }),
    );
    let field = PrimitivePropertyField::Text {
        default_value: None,
    };
    assert_eq!(
        row_text("children", &locked, Some(&field), &report),
        ("Sale".to_owned(), String::new())
    );
}

const PROPS_TAG: &str = "export function PriceTag({ x = 0, label }) {\n  return <Rect x={x} width={10} height={10} />;\n}\n";
const PROPS_TAG_SCENE: &str =
    "export const Scene = () => (\n  <PriceTag x={40} label=\"Sale\" tone={2} on />\n);\n";

fn props_tag_files() -> Texts {
    Texts::from([
        (
            PathBuf::from("/project/component.tsx"),
            PROPS_TAG.to_owned(),
        ),
        (
            PathBuf::from("/project/scene.tsx"),
            PROPS_TAG_SCENE.to_owned(),
        ),
    ])
}

/// The `<Rect>` in `PriceTag`, called with props of every literal kind.
fn props_tag_report() -> LayerSourceReport {
    let rect = LayerSource {
        column: 10,
        ..entry_at("Rect", ElementKind::Primitive, "/project/component.tsx", 2)
    };
    LayerSourceReport {
        props: ReceivedProps {
            x: Some(40.0),
            ..ReceivedProps::default()
        },
        ..report_of(vec![
            rect,
            entry_at("PriceTag", ElementKind::Component, "/project/scene.tsx", 2),
        ])
    }
}

#[test]
fn component_section_lists_the_call_sites_props() {
    let targets = resolve_targets(read_from(&props_tag_files()), &props_tag_report(), None)
        .expect("the element resolves");

    let component = targets.component.as_ref().expect("a component section");
    assert_eq!(component.name, "PriceTag");
    assert_eq!(
        component.call_site,
        SourcePos {
            file: PathBuf::from("/project/scene.tsx"),
            line: 2,
            column: 3,
        }
    );
    // `x` shows once, in its own row passing through to the call site.
    let props: Vec<(&str, &ControlKind)> = component
        .props
        .iter()
        .map(|prop| (prop.name.as_str(), &prop.kind))
        .collect();
    assert_eq!(
        props,
        [
            ("label", &ControlKind::String),
            ("tone", &ControlKind::Number),
            ("on", &ControlKind::Boolean),
        ]
    );
    let label = &component.props[0].target;
    assert_eq!(label.file, Path::new("/project/scene.tsx"));
    assert!(matches!(
        &label.target,
        EditTarget::Literal { value: EditValue::String(text), .. } if text == "Sale"
    ));
    assert!(matches!(
        targets.transform(TransformProp::X),
        Some(EditTarget::PassThrough { .. })
    ));
}

#[test]
fn component_props_use_the_registered_schema() {
    let mut targets =
        resolve_targets(read_from(&props_tag_files()), &props_tag_report(), None).unwrap();
    let schemas = std::collections::BTreeMap::from([(
        "PriceTag".to_owned(),
        std::collections::BTreeMap::from([(
            "label".to_owned(),
            celesta_react_bridge::ComponentPropertyField::Color {
                label: None,
                default_value: "#000000".to_owned(),
            },
        )]),
    )]);

    type_component_props(&mut targets, &schemas, read_from(&props_tag_files()));

    let component = targets.component.as_ref().unwrap();
    assert_eq!(component.props[0].name, "label");
    assert_eq!(component.props[0].kind, ControlKind::Color);
    // A prop the schema does not declare keeps its literal's kind.
    assert_eq!(component.props[1].kind, ControlKind::Number);
}

fn schema_number(
    min: Option<f64>,
    max: Option<f64>,
) -> celesta_react_bridge::ComponentPropertyField {
    celesta_react_bridge::ComponentPropertyField::Number {
        label: None,
        default_value: 0.0,
        min,
        max,
        step: None,
    }
}

/// `PriceTag` registered with `label: number`, `tone: number` from 0 to 10,
/// and `on: boolean`.
fn price_tag_schemas()
-> std::collections::BTreeMap<String, celesta_react_bridge::ComponentPropertySchema> {
    std::collections::BTreeMap::from([(
        "PriceTag".to_owned(),
        std::collections::BTreeMap::from([
            ("label".to_owned(), schema_number(None, None)),
            ("tone".to_owned(), schema_number(Some(0.0), Some(10.0))),
            (
                "on".to_owned(),
                celesta_react_bridge::ComponentPropertyField::Boolean {
                    label: None,
                    default_value: false,
                },
            ),
        ]),
    )])
}

/// `PriceTag`'s files with `on={1}` at the call.
fn typed_props_tag_files() -> Texts {
    let mut texts = props_tag_files();
    let scene = PROPS_TAG_SCENE.replace(" on />", " on={1} />");
    texts.insert(PathBuf::from("/project/scene.tsx"), scene);
    texts
}

/// `PriceTag`'s targets typed by [`price_tag_schemas`].
fn typed_props_tag_targets(texts: &Texts) -> ElementTargets {
    let mut targets = resolve_targets(read_from(texts), &props_tag_report(), None).unwrap();
    type_component_props(&mut targets, &price_tag_schemas(), read_from(texts));
    targets
}

#[test]
fn component_props_that_do_not_fit_their_schema_lock() {
    let texts = typed_props_tag_files();
    let targets = typed_props_tag_targets(&texts);
    let call_site = |name: &str| PropKey::CallSite(name.to_owned());

    assert_eq!(
        targets.target(&call_site("label")),
        Some(&EditTarget::Locked(LockReason::Expression {
            source: "\"Sale\"".to_owned()
        }))
    );
    assert!(matches!(
        targets.target(&call_site("on")),
        Some(EditTarget::Locked(LockReason::Expression { .. }))
    ));
    // A number in its range stays editable, and typed text is checked
    // against the range.
    assert!(matches!(
        targets.target(&call_site("tone")),
        Some(EditTarget::Literal { .. })
    ));
    let field = row_field(&targets, &call_site("tone"));
    assert_eq!(
        parse_row_value(&ControlKind::Number, field, "11"),
        Err("Enter a number from 0 to 10".to_owned())
    );
}

#[test]
fn a_schema_locked_prop_does_not_block_other_commits() {
    let texts = typed_props_tag_files();
    let targets = typed_props_tag_targets(&texts);
    let report = props_tag_report();

    let written = commit_files(
        &texts,
        &targets,
        &report,
        &[(
            PropKey::CallSite("tone".to_owned()),
            Some(EditValue::Number(5.0)),
        )],
    );

    let scene = &texts[Path::new("/project/scene.tsx")];
    assert_eq!(
        written,
        Ok(vec![(
            PathBuf::from("/project/scene.tsx"),
            scene.replace("tone={2}", "tone={5}")
        )])
    );
}

#[test]
fn a_moved_call_site_attribute_refuses_the_commit() {
    let texts = props_tag_files();
    let report = props_tag_report();
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    // `label` moved at the call; the Rect's own `y` is what is written.
    let mut moved = texts.clone();
    moved.insert(
        PathBuf::from("/project/scene.tsx"),
        PROPS_TAG_SCENE.replace("label=\"Sale\"", "label=\"On sale\""),
    );

    assert_eq!(
        commit_files(&moved, &targets, &report, &[num(TransformProp::Y, 5.0)]),
        Err(CommitError::SourceChanged)
    );
}

#[test]
fn none_on_an_absent_path_writes_nothing() {
    let missing = EditTarget::Missing {
        insert_at: 0,
        default: None,
    };
    assert_eq!(cleared_path(&missing), None);
    let written = EditTarget::Literal {
        span: Span { start: 1, end: 7 },
        value: EditValue::String("a.png".to_owned()),
        form: LiteralForm::Quoted('"'),
    };
    assert_eq!(
        cleared_path(&written),
        Some(EditValue::String(String::new()))
    );
}

#[test]
fn the_component_section_leaves_out_children() {
    let mut texts = props_tag_files();
    texts.insert(
        PathBuf::from("/project/scene.tsx"),
        PROPS_TAG_SCENE.replace(" on />", " children=\"A\" />"),
    );
    let targets = resolve_targets(read_from(&texts), &props_tag_report(), None).unwrap();
    let names: Vec<&str> = targets
        .component
        .as_ref()
        .unwrap()
        .props
        .iter()
        .map(|prop| prop.name.as_str())
        .collect();
    assert_eq!(names, ["label", "tone"]);
}

#[test]
fn component_prop_commit_writes_the_call_site() {
    let texts = props_tag_files();
    let report = props_tag_report();
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();

    let written = commit_files(
        &texts,
        &targets,
        &report,
        &[(
            PropKey::CallSite("label".to_owned()),
            Some(EditValue::String("Sold".to_owned())),
        )],
    );

    assert_eq!(
        written,
        Ok(vec![(
            PathBuf::from("/project/scene.tsx"),
            PROPS_TAG_SCENE.replace("label=\"Sale\"", "label=\"Sold\"")
        )])
    );
}

#[test]
fn no_component_section_without_a_call_site() {
    let report = primitive_report(2, 3, x_only());
    let targets = resolve_in(TRANSFORM_FIXTURE, &report).unwrap();
    assert_eq!(targets.component, None);
}

#[test]
fn scrub_steps() {
    let free = number_field(None);
    let unit = num_range(1.0, 0.0, 1.0);
    let near = |actual: f64, expected: f64| {
        assert!(
            (actual - expected).abs() < 1e-9,
            "{actual} is not {expected}"
        );
    };
    // One per pixel.
    assert_eq!(scrub_value(10.0, 5.0, Some(&free), false), 15.0);
    assert_eq!(scrub_value(10.0, 5.0, None, false), 15.0);
    // 0.01 per pixel for a field that ends at 1 or below, inside its range.
    near(scrub_value(0.5, 10.0, Some(&unit), false), 0.6);
    assert_eq!(scrub_value(0.5, 100.0, Some(&unit), false), 1.0);
    assert_eq!(scrub_value(0.5, -100.0, Some(&unit), false), 0.0);
    // Shift steps ten times as far.
    assert_eq!(scrub_value(10.0, 5.0, Some(&free), true), 60.0);
    near(scrub_value(0.5, 2.0, Some(&unit), true), 0.7);
    // The step a field declares (an anchor, a scale): unbounded.
    let anchor = stepped_field(0.0, 0.01);
    near(scrub_value(0.5, 10.0, Some(&anchor), false), 0.6);
    near(scrub_value(0.5, 10.0, Some(&anchor), true), 1.5);
    near(scrub_value(0.5, -150.0, Some(&anchor), false), -1.0);
    // A declared step wins over the range's.
    let coarse = PrimitivePropertyField::Number {
        default_value: None,
        min: Some(0.0),
        max: Some(1.0),
        step: Some(0.1),
        integer: None,
    };
    near(scrub_value(0.5, 2.0, Some(&coarse), false), 0.7);

    // A component prop takes the step its registered schema declares.
    let texts = typed_props_tag_files();
    let schemas = std::collections::BTreeMap::from([(
        "PriceTag".to_owned(),
        std::collections::BTreeMap::from([(
            "tone".to_owned(),
            celesta_react_bridge::ComponentPropertyField::Number {
                label: None,
                default_value: 0.0,
                min: Some(0.0),
                max: Some(100.0),
                step: Some(5.0),
            },
        )]),
    )]);
    let mut targets = resolve_targets(read_from(&texts), &props_tag_report(), None).unwrap();
    type_component_props(&mut targets, &schemas, read_from(&texts));
    let tone = (targets, props_tag_report());
    assert_eq!(
        scrubbed_to(&tone, &PropKey::CallSite("tone".to_owned()), 102.0),
        12.0
    );

    // A table's `scale` declares 0.01; without a table, the transform rows'
    // scale scrubs the same.
    let text = "export const Scene = () => (\n  <Rect x={10} scale={2} />\n);\n";
    let report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(10.0),
            scale: Some(2.0),
            ..ReceivedProps::default()
        },
    );
    let table = (resolve_table(text, &report, &rect_table()), report.clone());
    near(scrubbed_to(&table, &prop("scale"), 110.0), 2.1);
    let bare = (resolve_in(text, &report).unwrap(), report);
    near(scrubbed_to(&bare, &prop("scale"), 110.0), 2.1);
}

/// `<Rect x={10} {attributes} />` at line 2, column 3 of
/// `/project/scene.tsx`, drawn with `x` 10 and `received`.
fn drawn_rect(attributes: &str, received: &[(&str, f64)]) -> (ElementTargets, LayerSourceReport) {
    let text = format!("export const Scene = () => (\n  <Rect x={{10}} {attributes} />\n);\n");
    let mut report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(10.0),
            ..ReceivedProps::default()
        },
    );
    for (name, value) in received {
        report
            .props
            .others
            .insert((*name).to_owned(), ReceivedValue::Number(*value));
    }
    (resolve_table(&text, &report, &rect_table()), report)
}

/// The override value a scrub of `key` pressed at x 100 sends at `x`.
fn scrubbed_to(
    (targets, report): &(ElementTargets, LayerSourceReport),
    key: &PropKey,
    x: f32,
) -> f64 {
    let mut scrub = Scrub::start(targets, report, key, 100.0).expect("the row scrubs");
    scrub.follow(x, false);
    let overrides = scrub.overrides(false);
    assert_eq!(overrides.len(), 1, "{overrides:?}");
    overrides[0].value
}

#[test]
fn a_scrub_starts_from_the_value_the_row_shows() {
    // The placeholder of an absent prop without a default in the source
    // (`({ r = base * 2 })`): what the element received, unrounded.
    let text = "export const Scene = () => (\n  <Rect x={10} />\n);\n";
    let missing = EditTarget::Missing {
        insert_at: 0,
        default: None,
    };
    let received = (
        single_row(text, "cornerRadius", missing),
        other_prop_report("Rect", "cornerRadius", ReceivedValue::Number(2.125)),
    );
    assert_eq!(scrubbed_to(&received, &prop("cornerRadius"), 104.0), 6.125);
    // Else the table's default.
    let absent = drawn_rect("", &[]);
    assert!((scrubbed_to(&absent, &prop("opacity"), 90.0) - 0.9).abs() < 1e-9);
    // A parameter's default in the source, at the call that leaves it out.
    let texts = Texts::from([
        (
            PathBuf::from("/project/component.tsx"),
            PRICE_TAG.replace("({ x })", "({ x = 7 })"),
        ),
        (
            PathBuf::from("/project/scene.tsx"),
            PRICE_TAG_SCENE.replace(" x={40}", ""),
        ),
    ]);
    let report = LayerSourceReport {
        props: ReceivedProps {
            x: Some(7.0),
            rotation: Some(10.0),
            ..ReceivedProps::default()
        },
        ..price_tag_report()
    };
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    assert_eq!(scrubbed_to(&(targets, report), &prop("x"), 104.0), 11.0);
}

#[test]
fn a_scrub_takes_the_focus_unless_another_row_holds_typed_text() {
    let (radius, opacity) = (prop("cornerRadius"), prop("opacity"));
    // Nothing typed, or only in the scrubbed row (whose text gives way):
    // the editor takes the focus, so Cmd+Z undoes the scrub's write.
    assert!(scrub_takes_focus([], &radius));
    assert!(scrub_takes_focus([&radius], &radius));
    // Another row's typed text stays in its focused field, unsubmitted.
    assert!(!scrub_takes_focus([&opacity], &radius));
    assert!(!scrub_takes_focus([&radius, &opacity], &radius));
}

#[test]
fn a_scrub_back_at_its_start_writes_nothing_outside_the_range() {
    // `opacity={1.5}` scrubs from 1, the nearest value inside its range.
    let (targets, report) = drawn_rect("opacity={1.5}", &[("opacity", 1.5)]);
    let mut scrub = Scrub::start(&targets, &report, &prop("opacity"), 100.0).unwrap();
    assert!(scrub.follow(95.0, false));
    assert_eq!(
        scrub.overrides(false),
        [override_at("/project/scene.tsx", "opacity", 0.95)]
    );
    scrub.follow(100.0, false);
    assert!(!scrub.changed());
}

/// The overrides a scrub of `key` pressed at x 100 sends at x 104, 110 and
/// 96; `None` when the row does not scrub.
fn scrub_frames(
    targets: &ElementTargets,
    report: &LayerSourceReport,
    key: &PropKey,
) -> Option<Vec<Vec<PropOverride>>> {
    let mut scrub = Scrub::start(targets, report, key, 100.0)?;
    Some(
        [104.0, 110.0, 96.0]
            .into_iter()
            .map(|x| {
                scrub.follow(x, false);
                scrub.overrides(false)
            })
            .collect(),
    )
}

#[test]
fn scrub_overrides_the_row_prop() {
    // A literal on the element: its own prop there.
    let (targets, report) = corner_radius_rect();
    let at = |prop: &str, value: f64| vec![override_at("/project/scene.tsx", prop, value)];
    assert_eq!(
        scrub_frames(&targets, &report, &prop("cornerRadius")),
        Some(vec![
            at("cornerRadius", 8.0),
            at("cornerRadius", 14.0),
            at("cornerRadius", 0.0),
        ])
    );

    // A `Text` font row the `style` object sets: that key of `style`.
    let text = "export const Scene = () => (\n  <Text style={{ fontSize: 48 }} fontWeight={700}>A</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[("style.fontSize", ReceivedValue::Number(48.0))],
    );
    let targets = resolve_table(text, &report, &text_table());
    assert_eq!(
        scrub_frames(&targets, &report, &prop("fontSize")),
        Some(vec![
            at("style.fontSize", 52.0),
            at("style.fontSize", 58.0),
            at("style.fontSize", 44.0),
        ])
    );

    // A value passed through: the parameter's prop at the call site.
    let targets = mixed_targets();
    let report = price_tag_report();
    let called = |value: f64| vec![override_at("/project/scene.tsx", "left", value)];
    assert_eq!(
        scrub_frames(&targets, &report, &prop("x")),
        Some(vec![called(44.0), called(50.0), called(36.0)])
    );
    // An input is a value the expression reads, not a prop; a computed row
    // is locked.
    let input = PropKey::Input {
        row: "scale".to_owned(),
        name: "size".to_owned(),
    };
    assert_eq!(scrub_frames(&targets, &report, &input), None);
    assert_eq!(scrub_frames(&targets, &report, &prop("scale")), None);

    // A component prop: that prop at the component's call, inside its range.
    let texts = typed_props_tag_files();
    let targets = typed_props_tag_targets(&texts);
    let report = props_tag_report();
    let tone = |value: f64| vec![override_at("/project/scene.tsx", "tone", value)];
    assert_eq!(
        scrub_frames(&targets, &report, &PropKey::CallSite("tone".to_owned())),
        Some(vec![tone(6.0), tone(10.0), tone(0.0)])
    );

    // A keyframe has no single value to override; a color is not a number.
    let report = above_sequence_report(vec![]);
    let targets = resolve_in(ABOVE_SEQUENCE, &report).unwrap();
    assert_eq!(scrub_frames(&targets, &report, &prop("x")), None);
    let (targets, report) = corner_radius_rect();
    assert_eq!(scrub_frames(&targets, &report, &prop("fill")), None);
}

/// `<Rect x={10} fill="#fff" cornerRadius={4} />` at line 2, column 3 of
/// `/project/scene.tsx`, as the runtime drew it.
fn corner_radius_rect() -> (ElementTargets, LayerSourceReport) {
    let text =
        "export const Scene = () => (\n  <Rect x={10} fill=\"#fff\" cornerRadius={4} />\n);\n";
    let mut report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(10.0),
            ..ReceivedProps::default()
        },
    );
    for (name, value) in [
        ("fill", ReceivedValue::String("#fff".to_owned())),
        ("cornerRadius", ReceivedValue::Number(4.0)),
    ] {
        report.props.others.insert(name.to_owned(), value);
    }
    (resolve_table(text, &report, &rect_table()), report)
}

#[test]
fn a_scrub_overrides_nothing_before_it_moves_or_while_playing() {
    let (targets, report) = corner_radius_rect();
    let mut scrub = Scrub::start(&targets, &report, &prop("cornerRadius"), 100.0).unwrap();
    assert_eq!(scrub.overrides(false), []);
    assert!(!scrub.follow(100.0, false));
    assert_eq!(scrub.overrides(false), []);

    assert!(scrub.follow(103.0, false));
    assert!(scrub.changed());
    assert_eq!(
        scrub.overrides(false),
        [override_at("/project/scene.tsx", "cornerRadius", 7.0)]
    );
    assert_eq!(scrub.overrides(true), []);
    // Back at the start it keeps overriding (the same position), and has
    // nothing to write.
    assert!(scrub.follow(100.0, false));
    assert_eq!(
        scrub.overrides(false),
        [override_at("/project/scene.tsx", "cornerRadius", 4.0)]
    );
    assert!(!scrub.changed());
}

/// `table` with the rows `keys` declared required, as `required(…)` does.
fn with_required(mut table: PrimitivePropertyTable, keys: &[&str]) -> PrimitivePropertyTable {
    for row in table.groups.iter_mut().flat_map(|group| &mut group.fields) {
        row.required = keys.contains(&row.key.as_str());
    }
    table
}

/// A `<Rect>` with a required `width`, written optional props, an absent
/// `x`, and a `strokeWidth` read from what can't be traced.
fn removable_rect() -> (String, ElementTargets, LayerSourceReport) {
    let text = "export const Scene = () => (\n  <Rect width={10} cornerRadius={8} fill=\"#fff\" strokeWidth={w} />\n);\n";
    let mut report = primitive_report(2, 3, ReceivedProps::default());
    for (name, value) in [
        ("width", ReceivedValue::Number(10.0)),
        ("cornerRadius", ReceivedValue::Number(8.0)),
        ("fill", ReceivedValue::String("#fff".to_owned())),
        ("strokeWidth", ReceivedValue::Number(2.0)),
    ] {
        report.props.others.insert(name.to_owned(), value);
    }
    let table = with_required(
        property_table(vec![
            ("Transform", vec![("x", number_field(Some(0.0)))]),
            (
                "Size",
                vec![
                    ("width", number_field(None)),
                    ("cornerRadius", number_field(Some(0.0))),
                ],
            ),
            (
                "Appearance",
                vec![
                    (
                        "fill",
                        PrimitivePropertyField::Color {
                            default_value: None,
                        },
                    ),
                    ("strokeWidth", number_field(Some(0.0))),
                ],
            ),
        ]),
        &["width"],
    );
    let targets = resolve_table(text, &report, &table);
    (text.to_owned(), targets, report)
}

#[test]
fn only_a_written_optional_value_is_removable() {
    let (_, targets, _) = removable_rect();
    let can_remove = |key: &PropKey| removable(&targets, key, targets.entry(key).unwrap());
    // Written and optional.
    assert!(can_remove(&prop("cornerRadius")));
    assert!(can_remove(&prop("fill")));
    // Required by the props type, absent, or locked.
    assert!(!can_remove(&prop("width")));
    assert!(!can_remove(&prop("x")));
    assert!(!can_remove(&prop("strokeWidth")));
    // A keyframe is an entry of an animation, not the prop.
    let keyframe = local_target(
        "x",
        EditTarget::Keyframe {
            span: Span { start: 0, end: 1 },
            value: 1.0,
            frame: 0.0,
        },
    );
    assert!(!removable(&targets, &prop("x"), &keyframe));
    // An input is a value an expression reads, never a prop.
    let literal = targets.entry(&prop("cornerRadius")).unwrap();
    let input = PropKey::Input {
        row: "width".to_owned(),
        name: "w".to_owned(),
    };
    assert!(!removable(&targets, &input, literal));

    // A `<Text>`'s children are never removed, table or not.
    let text = "export const Scene = () => (\n  <Text>Sale</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[("children", ReceivedValue::String("Sale".to_owned()))],
    );
    let targets = resolve_table(text, &report, &text_table());
    let children = prop("children");
    assert!(!removable(
        &targets,
        &children,
        targets.entry(&children).unwrap()
    ));
}

#[test]
fn a_value_set_at_the_call_is_removable_there() {
    let texts = props_tag_files();
    let report = props_tag_report();
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    let x = prop("x");
    let label = PropKey::CallSite("label".to_owned());
    assert!(removable(&targets, &x, targets.entry(&x).unwrap()));
    assert!(removable(&targets, &label, targets.entry(&label).unwrap()));

    // The pass-through's attribute goes at the call (`PriceTag`'s `x = 0`
    // applies), and so does a component prop.
    let scene = PathBuf::from("/project/scene.tsx");
    assert_eq!(
        commit_files(&texts, &targets, &report, &[(x, None)]),
        Ok(vec![(
            scene.clone(),
            PROPS_TAG_SCENE.replace(" x={40}", "")
        )])
    );
    assert_eq!(
        commit_files(&texts, &targets, &report, &[(label, None)]),
        Ok(vec![(
            scene,
            PROPS_TAG_SCENE.replace(" label=\"Sale\"", "")
        )])
    );
}

#[test]
fn an_emptied_field_removes_a_removable_row() {
    let number = ControlKind::Number;
    assert_eq!(
        typed_text(&number, None, "  ", true),
        TypedText::Write(None)
    );
    assert_eq!(typed_text(&number, None, "", false), TypedText::Restore);
    // Strings too: an empty string is not written over a removable prop.
    assert_eq!(
        typed_text(&ControlKind::String, None, "", true),
        TypedText::Write(None)
    );
    assert_eq!(
        typed_text(&ControlKind::Text, None, "", false),
        TypedText::Restore
    );
    // Anything typed is parsed as before.
    assert_eq!(
        typed_text(&number, None, "12", true),
        TypedText::Write(Some(EditValue::Number(12.0)))
    );
    assert_eq!(
        typed_text(&number, None, "big", true),
        TypedText::Invalid("Enter a number".to_owned())
    );
}

#[test]
fn removing_a_prop_is_one_undoable_write() {
    let dir = std::env::temp_dir().join(format!(
        "celesta-remove-prop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("scene.tsx");
    let (text, _, mut report) = removable_rect();
    fs::write(&file, &text).unwrap();
    report.chain[0].file = file.clone();
    let read = |path: &Path| fs::read_to_string(path).map_err(|error| error.to_string());
    let table = with_required(rect_table(), &[]);
    let targets = resolve_targets(read, &report, Some(&table)).unwrap();

    let files = write_all_recorded(targets.files.clone(), |texts| {
        commit_files(texts, &targets, &report, &[(prop("cornerRadius"), None)])
            .map_err(|error| error.to_string())
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        text.replace(" cornerRadius={8}", "")
    );
    let mut history = EditHistory::default();
    history.push(HistoryEntry { files });
    history.undo().unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), text);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_edit_whose_result_does_not_parse_is_refused() {
    let text = "export const A = () => <Rect x={1} y={2} />;";
    let at = text.find("x={1}").unwrap() as u32;
    // Deleting the name alone leaves `={1}`.
    let broken = [Edit::Remove {
        start: at,
        end: at + 1,
    }];
    assert_eq!(apply_checked(text, &broken), Err(CommitError::Unparseable));
    assert_eq!(
        CommitError::Unparseable.to_string(),
        "Couldn't edit the source: the result does not parse"
    );
    let whole = [Edit::Remove {
        start: at - 1,
        end: at + 5,
    }];
    assert_eq!(
        apply_checked(text, &whole),
        Ok("export const A = () => <Rect y={2} />;".to_owned())
    );
}

#[test]
fn a_required_or_absent_prop_is_not_removed() {
    let (text, targets, report) = removable_rect();
    assert_eq!(
        commit_in(&text, &targets, &report, &[(prop("width"), None)]),
        Err(CommitError::NotRemovable("Width".to_owned()))
    );
    assert_eq!(
        commit_in(&text, &targets, &report, &[(prop("x"), None)]),
        Err(CommitError::NotRemovable("X".to_owned()))
    );
    assert_eq!(
        CommitError::NotRemovable("Width".to_owned()).to_string(),
        "Width can't be removed here"
    );
}

#[test]
fn a_style_property_row_is_removed_from_the_object() {
    let text = "export const Scene = () => (\n  <Text style={{ fontSize: 40, fontWeight: 700 }}>A</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[
            ("style.fontSize", ReceivedValue::Number(40.0)),
            ("style.fontWeight", ReceivedValue::Number(700.0)),
            ("children", ReceivedValue::String("A".to_owned())),
        ],
    );
    let targets = resolve_table(text, &report, &text_table());
    assert_eq!(
        commit_in(text, &targets, &report, &[(prop("fontSize"), None)]),
        Ok(text.replace("fontSize: 40, ", ""))
    );
}

#[test]
fn a_long_expression_shows_on_one_shortened_line() {
    let source =
        "[\n  { x: 10, label: 'Sale' },\n  { x: 20, label: 'New' },\n  { x: 30, label: 'Hot' },\n]";
    let text = reason_text(&LockReason::Expression {
        source: source.to_owned(),
    });
    assert_eq!(
        text,
        "Written as an expression: [ { x: 10, label: 'Sale' }, { x: 20, label: 'New' }, { x: 30…"
    );
    // A short one stays whole, its whitespace collapsed.
    assert_eq!(
        reason_text(&LockReason::Expression {
            source: "W\n  / 2".to_owned()
        }),
        "Written as an expression: W / 2"
    );
}

#[test]
fn an_input_takes_its_kind_from_its_literal_or_default() {
    let (_, targets, _) = removable_rect();
    let input = PropKey::Input {
        row: "width".to_owned(),
        name: "w".to_owned(),
    };
    let kind = |target: EditTarget| row_kind(&targets, &input, &local_target("w", target));
    let missing = |default: EditValue| EditTarget::Missing {
        insert_at: 0,
        default: Some(default),
    };
    assert_eq!(
        kind(missing(EditValue::String("Sale".to_owned()))),
        ControlKind::String
    );
    assert_eq!(kind(missing(EditValue::Bool(true))), ControlKind::Boolean);
    assert_eq!(kind(missing(EditValue::Number(2.0))), ControlKind::Number);
    assert_eq!(
        kind(EditTarget::Literal {
            span: Span { start: 0, end: 4 },
            value: EditValue::Bool(true),
            form: LiteralForm::Bool,
        }),
        ControlKind::Boolean
    );
    // Through a parameter, the call site's.
    assert_eq!(
        kind(EditTarget::PassThrough {
            call_site: targets.element,
            param: "w".to_owned(),
            target: Box::new(missing(EditValue::String("a".to_owned()))),
        }),
        ControlKind::String
    );
    assert_eq!(
        kind(EditTarget::Missing {
            insert_at: 0,
            default: None
        }),
        ControlKind::Number
    );
}

#[test]
fn a_font_row_style_may_set_shows_what_style_received() {
    let text =
        "export const Scene = () => (\n  <Text style={mono(20)} color=\"red\">A</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[
            ("color", ReceivedValue::String("red".to_owned())),
            ("style.fill", ReceivedValue::String("#00f".to_owned())),
            ("children", ReceivedValue::String("A".to_owned())),
        ],
    );
    let targets = resolve_table(text, &report, &text_table());
    let color = targets.entry(&prop("color")).unwrap();
    assert_eq!(color.target, EditTarget::Locked(LockReason::StyleMaySet));
    let field = PrimitivePropertyField::Color {
        default_value: None,
    };
    // `style` decides what is drawn.
    assert_eq!(
        row_text("color", color, Some(&field), &report),
        ("#00f".to_owned(), String::new())
    );
    // When `style` received none, the shorthand's value.
    let mut without = report.clone();
    without.props.others.remove("style.fill");
    assert_eq!(
        row_text("color", color, Some(&field), &without),
        ("red".to_owned(), String::new())
    );
}

/// A whole-number field, as a polygon's `sides`.
fn whole_field(min: Option<f64>, max: Option<f64>) -> PrimitivePropertyField {
    PrimitivePropertyField::Number {
        default_value: None,
        min,
        max,
        step: None,
        integer: Some(true),
    }
}

#[test]
fn a_whole_number_field_refuses_fractions() {
    let number = ControlKind::Number;
    let sides = whole_field(Some(3.0), None);
    assert_eq!(
        parse_row_value(&number, Some(&sides), "5"),
        Ok(EditValue::Number(5.0))
    );
    for text in ["3.5", "2"] {
        assert_eq!(
            parse_row_value(&number, Some(&sides), text),
            Err("Enter a whole number of at least 3".to_owned()),
            "{text}"
        );
    }
    let ranged = whole_field(Some(1.0), Some(10.0));
    assert_eq!(
        parse_row_value(&number, Some(&ranged), "2.5"),
        Err("Enter a whole number from 1 to 10".to_owned())
    );
    let capped = whole_field(None, Some(10.0));
    assert_eq!(
        parse_row_value(&number, Some(&capped), "0.5"),
        Err("Enter a whole number of at most 10".to_owned())
    );
    assert_eq!(
        parse_row_value(&number, Some(&whole_field(None, None)), "0.5"),
        Err("Enter a whole number".to_owned())
    );
    // A field that does not say so takes fractions, whatever its step.
    assert_eq!(
        parse_row_value(&number, Some(&stepped_field(1.0, 1.0)), "2.5"),
        Ok(EditValue::Number(2.5))
    );
}

#[test]
fn a_whole_number_scrub_moves_in_whole_numbers() {
    let sides = whole_field(Some(3.0), None);
    // A start that is not whole is rounded first.
    assert_eq!(scrub_value(4.4, 0.0, Some(&sides), false), 4.0);
    assert_eq!(scrub_value(4.4, 2.0, Some(&sides), false), 6.0);
    assert_eq!(scrub_value(4.0, -5.0, Some(&sides), false), 3.0);
    // Without the declaration a scrub keeps the fraction.
    assert_eq!(scrub_value(4.4, 2.0, Some(&number_field(None)), false), 6.4);
}

#[test]
fn held_commits_keep_the_latest_value_per_row_in_order() {
    let number = |value: f64| Some(EditValue::Number(value));
    let held_at = |layer: &str, key: PropKey, value, frame| HeldCommit {
        layer: layer.to_owned(),
        key,
        value,
        frame,
    };
    let mut held = HeldCommits::default();
    held.push(held_at("a", prop("x"), number(1.0), 0));
    held.push(held_at("a", prop("fill"), None, 0));
    held.push(held_at("a", prop("x"), number(3.0), 4));
    assert!(held.contains("a", &prop("x")));
    assert!(!held.contains("b", &prop("x")));
    assert!(held.any(Some("a")));
    assert!(!held.any(Some("b")));
    assert!(!held.any(None));
    // The newer value keeps the row's place, with the frame it was made at.
    assert_eq!(
        held.pop(Some("a")),
        Some(held_at("a", prop("x"), number(3.0), 4))
    );
    assert_eq!(
        held.pop(Some("a")),
        Some(held_at("a", prop("fill"), None, 0))
    );
    assert_eq!(held.pop(Some("a")), None);

    // Each layer's commits are its own; another layer's stay held.
    held.push(held_at("a", prop("x"), number(1.0), 0));
    held.push(held_at("b", prop("x"), number(2.0), 0));
    assert_eq!(
        held.pop(Some("b")),
        Some(held_at("b", prop("x"), number(2.0), 0))
    );
    assert!(held.contains("a", &prop("x")));
    assert_eq!(
        held.take_all(),
        vec![held_at("a", prop("x"), number(1.0), 0)]
    );
    assert!(!held.any(Some("a")));
}

#[test]
fn a_selection_waits_for_the_held_commits_of_the_selected_layer() {
    let mut gate = SelectionGate::default();
    // Nothing held: the selection changes at once.
    assert_eq!(
        gate.request(Some("b".to_owned()), Some("a"), false),
        Some(Some("b".to_owned()))
    );
    // Held commits for "a": the click waits, and a second one replaces it.
    assert_eq!(gate.request(Some("b".to_owned()), Some("a"), true), None);
    assert_eq!(gate.request(None, Some("a"), true), None);
    assert!(gate.waiting());
    assert_eq!(gate.release(true), None);
    // Once they are written (or dropped), the latest request goes through.
    assert_eq!(gate.release(false), Some(None));
    assert!(!gate.waiting());
    assert_eq!(gate.release(false), None);
    // Clicking the selected layer again cancels a waiting change.
    gate.request(Some("b".to_owned()), Some("a"), true);
    assert_eq!(gate.request(Some("a".to_owned()), Some("a"), true), None);
    assert_eq!(gate.release(false), None);
}

#[test]
fn edited_rows_of_the_selected_layer_are_submitted_before_it_changes() {
    let (x, y, fill) = (prop("x"), prop("y"), prop("fill"));
    let rows = [
        (&x, true, Some("a")),
        (&y, false, Some("a")),
        (&fill, true, Some("b")),
    ];
    assert_eq!(edited_rows(rows, Some("a")), vec![x.clone()]);
    assert_eq!(edited_rows(rows, None), Vec::<PropKey>::new());
}

#[test]
fn a_held_keyframe_commit_does_not_follow_a_seek() {
    let keyframe = EditTarget::Keyframe {
        span: Span { start: 0, end: 1 },
        value: 1.0,
        frame: 10.0,
    };
    let literal = EditTarget::Literal {
        span: Span { start: 0, end: 1 },
        value: EditValue::Number(1.0),
        form: LiteralForm::Number,
    };
    // Made at frame 10, the targets now read at frame 20.
    assert!(moved_off_keyframe(10, 20, &keyframe));
    assert!(!moved_off_keyframe(10, 10, &keyframe));
    // A plain value is the same at every frame.
    assert!(!moved_off_keyframe(10, 20, &literal));
    // Between keyframes at the new frame: moved off its keyframe too.
    let between = EditTarget::Locked(LockReason::BetweenKeyframes {
        previous: 0.0,
        next: 30.0,
        frame: 20.0,
    });
    assert!(moved_off_keyframe(10, 20, &between));
    assert!(!moved_off_keyframe(20, 20, &between));
}

#[test]
fn a_held_commit_is_stamped_with_the_frame_its_row_showed() {
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();
    assert_eq!(state.shown_frame(Some("a")), None);
    let request = state.begin("a", &report, 10).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    assert_eq!(state.shown_frame(Some("a")), Some(10));
    // A seek's lookup answered (frame 20) but is not resolved yet: the rows
    // still show frame 10's targets, so that is the commit's frame.
    state.begin("a", &report, 20).unwrap();
    assert_eq!(state.shown_frame(Some("a")), Some(10));
    assert_eq!(state.shown_frame(Some("b")), None);
}

#[test]
fn leaving_the_interactive_mode_waits_for_the_held_commits() {
    use InteractiveToggle::*;
    assert_eq!(interactive_toggle(false, false, false), Enter);
    // Nothing held: the selection cleared at once, and the mode goes.
    assert_eq!(interactive_toggle(true, false, false), Leave);
    // The clearing waits for held commits: so does the mode, which keeps
    // looking the source up until they are written.
    assert_eq!(interactive_toggle(true, false, true), LeaveAfterCommits);
    // Toggled again while waiting: it stays interactive.
    assert_eq!(interactive_toggle(true, true, true), StayInteractive);
}

#[test]
fn a_released_scrub_lets_the_frame_look_its_source_up() {
    // A pressed scrub and any drag redraw on every move: no lookup.
    assert!(!lookup_allowed(false, true));
    assert!(!lookup_allowed(true, false));
    // Released (its write held for a reload): the lookup goes, or the write
    // would wait for targets that are never resolved.
    assert!(lookup_allowed(false, false));
}

#[test]
fn a_finished_commit_touches_only_its_own_layers_row() {
    assert!(commit_owns_row(Some("a"), Some("a")));
    // The row shows layer "b" now: a failure of "a"'s write leaves it be.
    assert!(!commit_owns_row(Some("b"), Some("a")));
    assert!(!commit_owns_row(None, Some("a")));
    assert!(!commit_owns_row(Some("a"), None));
}

#[test]
fn a_failed_reload_fails_commits_instead_of_holding_them() {
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    state.invalidate();
    assert!(!state.failed(Some("a")));
    state.reload_failed();
    assert!(state.failed(Some("a")));
    assert_eq!(
        state.unavailable(Some("a")),
        "Not saved: the code failed to reload"
    );
    // The next reload reads the code again.
    state.expire();
    assert!(!state.failed(Some("a")));
}

#[test]
fn a_commit_waits_unless_the_current_resolution_failed() {
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();
    // Still being read: the commit waits for it.
    assert!(!state.failed(Some("a")));
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    assert!(!state.failed(Some("a")));

    let request = state.begin("a", &report, 1).unwrap();
    assert!(state.finish(request, Some("a"), Err("Couldn't read".to_owned())));
    assert!(state.failed(Some("a")));
    assert!(!state.failed(Some("b")));
    // An own write's reload may read it again.
    state.invalidate();
    assert!(!state.failed(Some("a")));
}

#[test]
fn a_removed_row_shows_its_placeholder_until_the_reload_resolves() {
    let mut state = ElementEdit::default();
    state.invalidate();
    state.record_written(Some("a"), &[(prop("cornerRadius"), None)]);
    assert_eq!(
        state.written_value(Some("a"), &prop("cornerRadius")),
        Some(&None)
    );

    let corner = number_field(Some(0.0));
    assert_eq!(
        written_text(&prop("cornerRadius"), Some(&corner), None),
        (String::new(), "0".to_owned())
    );
    // Without a table field, a transform prop's own default.
    assert_eq!(
        written_text(&prop("x"), None, None),
        (String::new(), "0".to_owned())
    );
    assert_eq!(
        written_text(
            &prop("fill"),
            None,
            Some(&EditValue::String("#fff".to_owned()))
        ),
        ("#fff".to_owned(), String::new())
    );
}
