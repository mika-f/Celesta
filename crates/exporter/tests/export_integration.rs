use std::path::{Path, PathBuf};
use std::process::Command;

use mikan_composition::{Rational, Time, TimeRange};
use mikan_exporter::{ExportCancellation, ExportError, ExportOptions, Exporter};
use mikan_gpu_renderer::GpuRenderError;
use mikan_media::{FfmpegBackend, VideoFrameDecoder};
use mikan_project::{Asset, AssetSource, Project, TimelineContent, TimelineItem, Track, TrackKind};

#[test]
fn exports_frame_exact_mp4_with_silent_audio_when_ffmpeg_is_available() {
    let Some(ffmpeg) = find_executable(
        "MIKAN_FFMPEG",
        "ffmpeg",
        "/opt/homebrew/opt/ffmpeg/bin/ffmpeg",
    ) else {
        eprintln!("skipping live export test: ffmpeg was not found");
        return;
    };
    let Some(ffprobe) = find_executable(
        "MIKAN_FFPROBE",
        "ffprobe",
        "/opt/homebrew/opt/ffmpeg/bin/ffprobe",
    ) else {
        eprintln!("skipping live export test: ffprobe was not found");
        return;
    };
    if !supports_libx264(&ffmpeg) {
        eprintln!("skipping live export test: FFmpeg does not provide libx264");
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("export.mp4");
    let source = directory.path().join("source.mkv");
    let generated = Command::new(&ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("testsrc2=size=64x64:rate=2:duration=1")
        .args(["-c:v", "ffv1"])
        .arg(&source)
        .status()
        .unwrap();
    assert!(generated.success());
    let mut project = Project::load(workspace_root().join("examples/minimal.mikan.json")).unwrap();
    project.settings.width = 64;
    project.settings.height = 64;
    project.settings.frame_rate = Rational::new(2, 1);
    project.settings.sample_rate = 8_000;
    project.settings.duration = Some(Time::new(1, 1));
    project.assets.insert(
        "source".to_owned(),
        Asset::Video {
            name: Some("Source".to_owned()),
            source: AssetSource::File {
                path: "source.mkv".to_owned(),
            },
        },
    );
    project.tracks.push(Track {
        id: "video".to_owned(),
        name: "Video".to_owned(),
        kind: TrackKind::Video,
        enabled: None,
        locked: None,
        muted: None,
        solo: None,
        items: vec![TimelineItem {
            id: "video-clip".to_owned(),
            name: Some("Video".to_owned()),
            range: TimeRange {
                start: Time::ZERO,
                duration: Time::new(1, 1),
            },
            content: TimelineContent::Video {
                asset: "source".to_owned(),
                source_range: None,
                playback_rate: None,
                volume: None,
                muted: None,
            },
            enabled: None,
            transform: None,
            opacity: None,
        }],
    });

    let exporter = Exporter::new(ExportOptions {
        ffmpeg: ffmpeg.clone(),
        ffprobe: ffprobe.clone(),
        overwrite: false,
    });
    match exporter.export_project(&project, directory.path(), &output) {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            eprintln!("skipping live export test: no GPU adapter is available: {error}");
            return;
        }
        Err(error) => panic!("export failed: {error}"),
    }

    let probe = Command::new(&ffprobe)
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_entries",
            "stream=codec_type,width,height,r_frame_rate,sample_rate,channels,nb_read_frames",
            "-of",
            "compact=p=0:nk=0",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(probe.status.success());
    let probe = String::from_utf8(probe.stdout).unwrap();
    assert!(probe.contains("codec_type=video"), "{probe}");
    assert!(probe.contains("width=64"), "{probe}");
    assert!(probe.contains("height=64"), "{probe}");
    assert!(probe.contains("r_frame_rate=2/1"), "{probe}");
    assert!(probe.contains("nb_read_frames=2"), "{probe}");
    assert!(probe.contains("codec_type=audio"), "{probe}");
    assert!(probe.contains("sample_rate=8000"), "{probe}");
    assert!(probe.contains("channels=2"), "{probe}");
    let mut decoder = FfmpegBackend::with_executables(ffmpeg, ffprobe.clone());
    let first_frame = decoder.decode_frame(&output, 0.0).unwrap();
    assert!(
        first_frame
            .pixels
            .chunks_exact(4)
            .any(|pixel| { pixel[0] != pixel[1] || pixel[1] != pixel[2] || pixel[0] != 0 })
    );

    let cancelled_output = directory.path().join("cancelled.mp4");
    let cancellation = ExportCancellation::default();
    let cancellation_from_progress = cancellation.clone();
    let result = exporter.export_project_cancellable(
        &project,
        directory.path(),
        &cancelled_output,
        &cancellation,
        move |progress| {
            if matches!(
                progress,
                mikan_exporter::ExportProgress::Rendering { frame: 1, .. }
            ) {
                cancellation_from_progress.cancel();
            }
        },
    );
    assert!(matches!(result, Err(ExportError::Cancelled)));
    assert!(!cancelled_output.exists());
}

#[test]
fn removes_temporary_outputs_when_ffmpeg_cannot_start() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("export.mp4");
    let mut project = Project::load(workspace_root().join("examples/minimal.mikan.json")).unwrap();
    project.settings.width = 64;
    project.settings.height = 64;
    project.settings.frame_rate = Rational::new(2, 1);
    project.settings.duration = Some(Time::new(1, 1));
    let exporter = Exporter::new(ExportOptions {
        ffmpeg: directory.path().join("missing-ffmpeg"),
        ffprobe: directory.path().join("missing-ffprobe"),
        overwrite: false,
    });

    assert!(matches!(
        exporter.export_project(&project, directory.path(), &output),
        Err(ExportError::Executable { .. })
    ));
    assert!(!output.exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn cancellation_before_export_does_not_create_output() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("export.mp4");
    let mut project = Project::load(workspace_root().join("examples/minimal.mikan.json")).unwrap();
    project.settings.duration = Some(Time::new(1, 1));
    let cancellation = ExportCancellation::default();
    cancellation.cancel();

    assert!(matches!(
        Exporter::default().export_project_cancellable(
            &project,
            directory.path(),
            &output,
            &cancellation,
            |_| {}
        ),
        Err(ExportError::Cancelled)
    ));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

fn find_executable(environment: &str, command: &str, homebrew: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(environment).map(PathBuf::from)
        && executable_works(&path)
    {
        return Some(path);
    }
    let command = PathBuf::from(command);
    if executable_works(&command) {
        return Some(command);
    }
    let homebrew = PathBuf::from(homebrew);
    executable_works(&homebrew).then_some(homebrew)
}

fn executable_works(path: &Path) -> bool {
    Command::new(path)
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn supports_libx264(ffmpeg: &Path) -> bool {
    Command::new(ffmpeg)
        .args(["-v", "error", "-encoders"])
        .output()
        .is_ok_and(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains("libx264")
        })
}
