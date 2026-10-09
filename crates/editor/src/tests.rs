use crate::EDITOR_DEMO_PROJECT;
use crate::audio::{AudioCacheKey, CachedAudioDecoder, take_latest};
use crate::audio_cache::DiskAudioCache;
use crate::export_worker::{ExportEvent, ExportRequest, ExportSource, ExportWorker};
use crate::helpers::{export_range_for, export_suggested_name, loop_range_for};
use crate::source::is_react_entry;
use crate::waveform::{
    clip_level_envelope, level_at_time, map_clip_waveform, master_volume_from_drag, waveform_peaks,
    waveform_segment,
};
use celesta_editor_core::ClipKind;
use celesta_gpu_renderer::GpuDriver;

use crate::preview::{collect_component_requests, strip_missing_components};
use celesta_composition::{
    Animatable, AssetLocation, AudioClip, EvaluatedTransform, GroupMask, Layer, LayerContent,
    MaskMode, Rational, ResolvedAsset, Time, TimeRange,
};
use celesta_editor_core::ClipSummary;
use celesta_exporter::ExportCancellation;
use celesta_media::{AudioBuffer, AudioDecoder};
use celesta_project::Project;
use std::fs;
use std::path::PathBuf;
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
            range: None,
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

    clip.volume = Animatable::Static(0.5);
    assert_eq!(
        clip_level_envelope(&[0.4, 0.6, 0.8, 0.0], &clip),
        vec![0.2, 0.3, 0.4, 0.0]
    );
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
