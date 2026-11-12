//! The exports both FFmpeg plugins' export tests share: the GPU lock, the
//! projects they render, and exports that skip without a GPU. Each test
//! crate defines `install()`, which registers its media plugins before every
//! export.
#![allow(dead_code)]

use celesta_codec_ffmpeg::FfmpegBackend;
use celesta_composition::{Rational, Time, TimeRange};
use celesta_exporter::{
    AudioLoudness, ExportError, ExportOptions, ExportProgress, ExportedFile, Exporter, SettingInput,
};
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::{AudioDecoder, ComponentRef};
use celesta_project::{
    Asset, AssetSource, Project, TimelineContent, TimelineItem, Track, TrackKind,
};
use ez_ffmpeg::{FfmpegContext, Input};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// GPU devices made at once in one process are unsafe on some drivers:
/// Mesa's lavapipe, which Linux CI renders with, crashes the test binary
/// with SIGSEGV. Every test that exports holds this for its whole body, so
/// one exporter renders at a time.
static GPU: Mutex<()> = Mutex::new(());

/// The GPU lock; a test that panicked while holding it does not fail the
/// rest.
pub fn gpu() -> MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An exporter on the media plugins the test crate's `install` registers.
pub fn exporter(options: ExportOptions) -> Exporter {
    crate::install();
    Exporter::new(options)
}

/// Reports a test skipped for want of a GPU adapter, or fails it when
/// `CELESTA_REQUIRE_GPU=1` says the machine has one.
pub fn skip_without_gpu(error: &impl std::fmt::Display) {
    assert!(
        std::env::var("CELESTA_REQUIRE_GPU").as_deref() != Ok("1"),
        "GPU tests require an adapter: {error}"
    );
    eprintln!("skipping live export test: no GPU adapter is available: {error}");
}

/// An empty 1920x1080 project, a copy of `examples/minimal.celesta.json`
/// kept beside the standard plugin's tests so a change to it is a change to
/// the plugin (which is what decides when CI runs them).
pub fn minimal_project() -> Project {
    Project::load(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../ffmpeg/tests/fixtures/minimal.celesta.json"),
    )
    .unwrap()
}

/// `plugin/component` as a reference.
pub fn component(text: &str) -> ComponentRef {
    text.parse().unwrap()
}

/// Options given as command-line text.
pub fn options(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, SettingInput> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), SettingInput::Text((*value).to_owned())))
        .collect()
}

/// A 64x64, 2 fps, one-second project whose only item is a 64x64 image of
/// straight `(255, 0, 0, 128)`. Images are centered on the canvas origin, so
/// it covers the top-left 32x32 quadrant and leaves the rest undrawn.
pub fn transparent_project(dir: &Path) -> Project {
    let mut project = minimal_project();
    project.settings.width = 64;
    project.settings.height = 64;
    project.settings.frame_rate = Rational::new(2, 1);
    project.settings.sample_rate = 8_000;
    project.settings.duration = Some(Time::new(1, 1));
    let file = std::fs::File::create(dir.join("red.png")).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 64, 64);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[255, 0, 0, 128].repeat(64 * 64))
        .unwrap();
    writer.finish().unwrap();
    project.assets.insert(
        "red".to_owned(),
        Asset::Image {
            name: None,
            source: AssetSource::File {
                path: "red.png".to_owned(),
            },
        },
    );
    project.tracks.push(Track {
        id: "images".to_owned(),
        name: "Images".to_owned(),
        kind: TrackKind::Video,
        enabled: None,
        locked: None,
        muted: None,
        solo: None,
        items: vec![TimelineItem {
            id: "red".to_owned(),
            name: None,
            range: TimeRange {
                start: Time::ZERO,
                duration: Time::new(1, 1),
            },
            content: TimelineContent::Image {
                asset: "red".to_owned(),
                width: None,
                height: None,
                fit: None,
            },
            enabled: None,
            transform: None,
            opacity: None,
            blend_mode: None,
            effects: None,
        }],
    });
    project
}

/// Exports `project` to `name` in `dir`, whose extension picks the muxer, or
/// `None` without a GPU.
pub fn export_as(project: &Project, dir: &Path, name: &str) -> Option<PathBuf> {
    export_with(project, dir, name, ExportOptions::default())
}

/// [`export_as`] with `options`.
pub fn export_with(
    project: &Project,
    dir: &Path,
    name: &str,
    options: ExportOptions,
) -> Option<PathBuf> {
    let output = dir.join(name);
    match exporter(options).export_project(project, dir, &output) {
        Ok(()) => Some(output),
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            None
        }
        Err(error) => panic!("{name} export failed: {error}"),
    }
}

/// `project` with a one-second clip of `lavfi` audio from 0 s, written to
/// `name` in `dir`.
pub fn with_audio(mut project: Project, dir: &Path, lavfi: &str, name: &str) -> Project {
    FfmpegContext::builder()
        .input(Input::from(lavfi).set_format("lavfi"))
        .output(dir.join(name).to_string_lossy().into_owned())
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
    project.assets.insert(
        "sound".to_owned(),
        Asset::Audio {
            name: None,
            source: AssetSource::File {
                path: name.to_owned(),
            },
        },
    );
    project.tracks.push(Track {
        id: "audio".to_owned(),
        name: "Audio".to_owned(),
        kind: TrackKind::Audio,
        enabled: None,
        locked: None,
        muted: None,
        solo: None,
        items: vec![TimelineItem {
            id: "sound".to_owned(),
            name: None,
            range: TimeRange {
                start: Time::ZERO,
                duration: Time::new(1, 1),
            },
            content: TimelineContent::Audio {
                asset: "sound".to_owned(),
                source_range: None,
                playback_rate: None,
                volume: None,
                muted: None,
                pitch: None,
                preserve_pitch: None,
                fade_in: None,
                fade_out: None,
                duck_group: None,
                duck: None,
            },
            enabled: None,
            transform: None,
            opacity: None,
            blend_mode: None,
            effects: None,
        }],
    });
    project
}

/// Exports `project` with `options` to `name` in `dir`, with the loudness
/// and warnings it reported, or `None` without a GPU.
pub fn export_reporting(
    project: &Project,
    dir: &Path,
    options: ExportOptions,
    name: &str,
) -> Option<(PathBuf, Option<AudioLoudness>, Vec<String>)> {
    let output = dir.join(name);
    let (mut loudness, mut warnings) = (None, Vec::new());
    let result =
        exporter(options).export_project_with_progress(
            project,
            dir,
            &output,
            |event| match event {
                ExportProgress::Wrote(ExportedFile::Video {
                    loudness: measured, ..
                }) => {
                    loudness = measured;
                }
                ExportProgress::Warning(warning) => warnings.push(warning),
                _ => {}
            },
        );
    match result {
        Ok(()) => Some((output, loudness, warnings)),
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            None
        }
        Err(error) => panic!("export failed: {error}"),
    }
}

/// The hits of `a_looping_mp4_starts_its_sound_from_its_end`, at 48 kHz.
pub fn hits_project(dir: &Path) -> Project {
    let mut project = transparent_project(dir);
    project.settings.sample_rate = 48_000;
    with_audio(
        project,
        dir,
        "aevalsrc=exprs='0.2*exp(-40*mod(t,0.25))*(cos(2*PI*1000*mod(t,0.25))+sin(2*PI*2500*mod(t,0.25))+sin(2*PI*4400*mod(t,0.25))+cos(2*PI*6000*mod(t,0.25)))':sample_rate=48000:duration=1",
        "hits.wav",
    )
}

/// The sample frames of the mix of `project`'s one sound: its source,
/// decoded at the project's rate in stereo, as the mixer reads it.
pub fn mix_frames(dir: &Path, name: &str, sample_rate: u32) -> celesta_media::AudioBuffer {
    FfmpegBackend::new()
        .decode_audio(&dir.join(name), sample_rate, 2)
        .unwrap()
}
