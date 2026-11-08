use std::path::{Path, PathBuf};

use celesta_codec_test::{
    StoredAudio, StoredVideo, TestCodec, TestMedia, write_audio, write_rgba_video,
};
use celesta_composition::{Rational, Time, TimeRange};
use celesta_exporter::{
    AudioChoice, AudioLoudness, EncoderChoice, ExportCancellation, ExportError, ExportJob,
    ExportOptions, ExportProgress, ExportRange, ExportedFile, Exporter, LoudnessTarget,
    OutputChoice, ReactRuntimeOptions, SettingInput,
};
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::{Decoder, MediaError, PixelFormat, measure_loudness};
use celesta_project::{
    Animatable, AnimatablePoint, Asset, AssetSource, DuckGroups, Project, ProjectDuck,
    TimelineContent, TimelineItem, Track, TrackKind, Transform,
};

/// Reports a test skipped for want of a GPU adapter, or fails it when
/// `CELESTA_REQUIRE_GPU=1` says the machine has one.
fn skip_without_gpu(error: &impl std::fmt::Display) {
    assert!(
        std::env::var("CELESTA_REQUIRE_GPU").as_deref() != Ok("1"),
        "GPU tests require an adapter: {error}"
    );
    eprintln!("skipping live export test: no GPU adapter is available: {error}");
}

/// What the test codec stored in `path`: the frames its encoder was given
/// and the audio muxed in, as they were.
fn read(path: &Path) -> TestMedia {
    celesta_codec_test::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn stored_video(media: &TestMedia) -> &StoredVideo {
    media.video.as_ref().expect("the export has video")
}

fn stored_audio(media: &TestMedia) -> &StoredAudio {
    media.audio.as_ref().expect("the export has audio")
}

/// The default components, with the test codec's video encoder given
/// `options` (`input`, `alpha`) as command-line text.
fn video_options(options: &[(&str, &str)]) -> OutputChoice {
    OutputChoice {
        video: EncoderChoice {
            component: None,
            options: options
                .iter()
                .map(|&(key, value)| (key.to_owned(), SettingInput::Text(value.to_owned())))
                .collect(),
        },
        ..OutputChoice::default()
    }
}

/// The names of the files in `directory`.
fn files_in(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

/// A 64x64, 2 fps, 8 kHz project whose only item plays `source.ctc`, a 64x64
/// video of one solid frame of each of `colors` in turn, over the whole
/// canvas for as long as it lasts.
fn solid_video_project(dir: &Path, colors: &[[u8; 4]]) -> Project {
    let frames: Vec<Vec<u8>> = colors.iter().map(|color| color.repeat(64 * 64)).collect();
    write_rgba_video(
        &dir.join("source.ctc"),
        64,
        64,
        Rational::new(2, 1),
        &frames,
    )
    .unwrap();
    let duration = Time::new(colors.len() as i64, 2);

    let mut project =
        Project::load(workspace_root().join("examples/minimal.celesta.json")).unwrap();
    project.settings.width = 64;
    project.settings.height = 64;
    project.settings.frame_rate = Rational::new(2, 1);
    project.settings.sample_rate = 8_000;
    project.settings.duration = Some(duration);
    project.assets.insert(
        "source".to_owned(),
        Asset::Video {
            name: Some("Source".to_owned()),
            source: AssetSource::File {
                path: "source.ctc".to_owned(),
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
                duration,
            },
            content: TimelineContent::Video {
                asset: "source".to_owned(),
                source_range: None,
                playback_rate: None,
                volume: None,
                muted: None,
                duck_group: None,
                duck: None,
            },
            enabled: None,
            // Centered on the canvas, so it covers every pixel.
            transform: Some(Transform {
                position: Some(AnimatablePoint {
                    x: Some(Animatable::Static(32.0)),
                    y: Some(Animatable::Static(32.0)),
                }),
                ..Transform::default()
            }),
            opacity: None,
            blend_mode: None,
            effects: None,
        }],
    });
    project
}

/// Fails at the first pixel of the 64x64 RGBA8 `frame` that is not `color`.
fn assert_solid(frame: &[u8], color: [u8; 4], what: &str) {
    assert_eq!(frame.len(), 64 * 64 * 4, "{what}: frame size");
    if let Some((index, pixel)) = frame
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .find(|(_, pixel)| **pixel != color)
    {
        panic!(
            "{what}: pixel ({}, {}) is {pixel:?}, not {color:?}",
            index % 64,
            index / 64
        );
    }
}

#[test]
fn exports_frame_exact_mp4_with_silent_audio() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("export.mp4");
    let colors = [[200, 40, 90, 255], [30, 160, 220, 255]];
    let project = solid_video_project(directory.path(), &colors);

    let exporter = crate::exporter(ExportOptions {
        overwrite: false,
        range: None,
        output: video_options(&[("input", "rgba8")]),
        ..ExportOptions::default()
    });
    let mut written = Vec::new();
    match exporter.export_project_with_progress(&project, directory.path(), &output, |event| {
        if let ExportProgress::Wrote(file) = event {
            written.push(file);
        }
    }) {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            return;
        }
        Err(error) => panic!("export failed: {error}"),
    }
    let [
        ExportedFile::Video {
            muxer,
            video,
            audio,
            alpha,
            ..
        },
    ] = &written[..]
    else {
        panic!("{written:?}");
    };
    assert_eq!(muxer.to_string(), "celesta-test-codec/file");
    assert_eq!(video.codec, "ctst");
    assert!(audio.is_some());
    assert!(!alpha);

    // The encoder gets each frame as rendered: the source's colors, opaque.
    let media = read(&output);
    assert_eq!(media.extension.as_deref(), Some("mp4"));
    let video = stored_video(&media);
    assert_eq!(
        (video.width, video.height, video.frame_rate, video.format),
        (64, 64, Rational::new(2, 1), PixelFormat::Rgba8)
    );
    assert_eq!(video.frames.len(), 2);
    for (index, (frame, color)) in video.frames.iter().zip(colors).enumerate() {
        assert_solid(frame, color, &format!("frame {index}"));
    }
    let audio = stored_audio(&media);
    assert_eq!((audio.sample_rate, audio.channels), (8_000, 2));
    assert_eq!(audio.samples.len(), 8_000 * 2, "one second of stereo");
    assert!(
        audio.samples.iter().all(|&sample| sample == 0.0),
        "the mix is not silent"
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
                celesta_exporter::ExportProgress::Rendering { frame: 1, .. }
            ) {
                cancellation_from_progress.cancel();
            }
        },
    );
    assert!(matches!(result, Err(ExportError::Cancelled)));
    assert!(!cancelled_output.exists());
}

#[test]
fn exports_only_the_selected_range_shifted_to_zero() {
    let directory = tempfile::tempdir().unwrap();
    // Four distinct frames at 2 fps over two seconds.
    let project = solid_video_project(
        directory.path(),
        &[
            [200, 40, 90, 255],
            [30, 160, 220, 255],
            [250, 210, 20, 255],
            [90, 90, 90, 255],
        ],
    );

    let full_output = directory.path().join("full.mp4");
    let full = crate::exporter(ExportOptions {
        overwrite: false,
        range: None,
        ..ExportOptions::default()
    });
    match full.export_project(&project, directory.path(), &full_output) {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            return;
        }
        Err(error) => panic!("full export failed: {error}"),
    }

    // Export only the second half; the window's start becomes the file's 00:00.
    let windowed_output = directory.path().join("windowed.mp4");
    crate::exporter(ExportOptions {
        overwrite: false,
        range: Some(ExportRange::new(Time::new(1, 1), Time::new(2, 1))),
        ..ExportOptions::default()
    })
    .export_project(&project, directory.path(), &windowed_output)
    .expect("windowed export");

    let (full, windowed) = (read(&full_output), read(&windowed_output));
    let (full_video, windowed_video) = (stored_video(&full), stored_video(&windowed));
    assert_eq!(
        (
            windowed_video.width,
            windowed_video.height,
            windowed_video.frame_rate
        ),
        (64, 64, Rational::new(2, 1))
    );
    // 1.0s .. 2.0s at 2 fps: two frames, not the full timeline's four.
    assert_eq!(full_video.frames.len(), 4);
    assert_eq!(windowed_video.frames.len(), 2);
    assert!(windowed.audio.is_some());
    // Every full-export frame differs from the others, so matching frames 2
    // and 3 below cannot pass with frames from anywhere else.
    for (index, frame) in full_video.frames.iter().enumerate() {
        for (other, other_frame) in full_video.frames.iter().enumerate().skip(index + 1) {
            assert!(
                frame != other_frame,
                "the full export's frames {index} and {other} are the same"
            );
        }
    }

    // The window is shifted to start at the file's 00:00, so its frames are
    // the full export's frames at 1.0s and 1.5s.
    for (index, frame) in windowed_video.frames.iter().enumerate() {
        assert!(
            *frame == full_video.frames[index + 2],
            "windowed frame {index} is not the full export's frame {}",
            index + 2
        );
    }
}

#[test]
fn rejects_an_empty_export_range() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("export.mp4");
    let mut project =
        Project::load(workspace_root().join("examples/minimal.celesta.json")).unwrap();
    project.settings.duration = Some(Time::new(1, 1));

    let result = crate::exporter(ExportOptions {
        overwrite: false,
        range: Some(ExportRange::new(Time::new(2, 1), Time::new(3, 1))),
        ..ExportOptions::default()
    })
    .export_project(&project, directory.path(), &output);
    assert!(matches!(result, Err(ExportError::EmptyRange)));
    assert!(!output.exists());
}

#[test]
fn cancellation_before_export_does_not_create_output() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("export.mp4");
    let mut project =
        Project::load(workspace_root().join("examples/minimal.celesta.json")).unwrap();
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

#[test]
fn the_encoder_chooses_the_frames_it_gets() {
    let directory = tempfile::tempdir().unwrap();
    let project = solid_video_project(directory.path(), &[[200, 40, 90, 255], [30, 160, 220, 255]]);

    // 4:2:0 YUV comes from the GPU (64x64 luma and two 32x32 chroma planes,
    // 16 bits a sample for 10 bits); RGBA is read back for the encoder to
    // convert. A software renderer offers no YUV, which the encoder then
    // refuses.
    for (input, format, bytes) in [
        ("yuv420p", PixelFormat::Yuv420p, 6_144),
        ("yuv420p10", PixelFormat::Yuv420p10, 12_288),
        ("rgba16", PixelFormat::Rgba16, 32_768),
        ("rgba8", PixelFormat::Rgba8, 16_384),
    ] {
        let output = directory.path().join(format!("{input}.mp4"));
        let result = crate::exporter(ExportOptions {
            output: video_options(&[("input", input)]),
            ..ExportOptions::default()
        })
        .export_project(&project, directory.path(), &output);
        match result {
            Ok(()) => {}
            Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
                skip_without_gpu(&error);
                return;
            }
            Err(ExportError::Codec {
                source: MediaError::InvalidSettings { key, .. },
                ..
            }) if key == "input" && input.starts_with("yuv") => {
                eprintln!("skipping {input}: this renderer does not offer it");
                continue;
            }
            Err(error) => panic!("{input} export failed: {error}"),
        }
        let media = read(&output);
        let video = stored_video(&media);
        assert_eq!(video.format, format, "{input}");
        assert_eq!(video.frames.len(), 2, "{input}");
        assert!(
            video.frames.iter().all(|frame| frame.len() == bytes),
            "{input}: frames of {:?} bytes, not {bytes}",
            video.frames.iter().map(Vec::len).collect::<Vec<_>>()
        );
    }
}

/// A 64x64, 2 fps, one-second project whose only item is a 64x64 image of
/// straight `(255, 0, 0, 128)`. Images are centered on the canvas origin, so
/// it covers the top-left 32x32 quadrant and leaves the rest undrawn.
fn transparent_project(dir: &Path) -> Project {
    let mut project =
        Project::load(workspace_root().join("examples/minimal.celesta.json")).unwrap();
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

/// Exports `project` with `output`'s components to `name` in `dir`, or
/// `None` without a GPU.
fn export_with(project: &Project, dir: &Path, output: OutputChoice, name: &str) -> Option<PathBuf> {
    let path = dir.join(name);
    match crate::exporter(ExportOptions {
        output,
        ..ExportOptions::default()
    })
    .export_project(project, dir, &path)
    {
        Ok(()) => Some(path),
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            None
        }
        Err(error) => panic!("{name} export failed: {error}"),
    }
}

/// Exports `project` with the default components to `name` in `dir`, or
/// `None` without a GPU.
fn export_as(project: &Project, dir: &Path, name: &str) -> Option<PathBuf> {
    export_with(project, dir, OutputChoice::default(), name)
}

/// The pixel at `(x, y)` of a 64x64 `Rgba16` frame.
fn rgba16_pixel(frame: &[u8], x: usize, y: usize) -> [u16; 4] {
    let start = (y * 64 + x) * 8;
    let (channels, _) = frame[start..start + 8].as_chunks::<2>();
    std::array::from_fn(|channel| u16::from_le_bytes(channels[channel]))
}

#[test]
fn alpha_formats_store_straight_16_bit_frames() {
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    // An encoder that keeps alpha takes straight 16-bit RGBA, whatever the
    // renderer offers first.
    for (format, name) in [("mov", "clip.mov"), ("webm", "clip.webm")] {
        let choice = video_options(&[("input", "rgba16"), ("alpha", "true")]);
        let Some(output) = export_with(&project, directory.path(), choice, name) else {
            return;
        };
        let media = read(&output);
        assert_eq!(media.extension.as_deref(), Some(format));
        let video = stored_video(&media);
        assert_eq!(video.format, PixelFormat::Rgba16, "{format}");

        // The red quadrant keeps its straight color and half alpha, within
        // one 8-bit code of the half-float canvas; undrawn pixels are fully
        // transparent.
        let (inside, outside) = (
            rgba16_pixel(&video.frames[0], 8, 8),
            rgba16_pixel(&video.frames[0], 48, 48),
        );
        let near = |value: u16, expected: u16| value.abs_diff(expected) <= 257;
        assert!(
            near(inside[0], 65_535)
                && near(inside[1], 0)
                && near(inside[2], 0)
                && near(inside[3], 32_896),
            "{format}: {inside:?}"
        );
        assert_eq!(outside, [0; 4], "{format}");
    }
}

/// `seconds` of mono samples at `sample_rate`, each `signal` of its time.
fn generate(sample_rate: u32, seconds: f64, signal: impl Fn(f64) -> f64) -> Vec<f32> {
    let rate = f64::from(sample_rate);
    let frames = (seconds * rate).round() as usize;
    (0..frames)
        .map(|frame| signal(frame as f64 / rate) as f32)
        .collect()
}

/// A sine of `frequency` at amplitude 0.125, as FFmpeg's `sine` source
/// makes it.
fn sine(frequency: f64, sample_rate: u32, seconds: f64) -> Vec<f32> {
    generate(sample_rate, seconds, |t| {
        0.125 * (std::f64::consts::TAU * frequency * t).sin()
    })
}

/// `path`'s sound from the video's start, decoded to mono at `sample_rate`.
fn decoded_audio(path: &Path, sample_rate: u32) -> Vec<f32> {
    TestCodec::new()
        .decode_audio(celesta_codec_test::DECODER_ID, path, sample_rate, 1)
        .unwrap()
        .samples
}

#[test]
fn exports_a_pitched_audio_clip() {
    let directory = tempfile::tempdir().unwrap();
    let mut project = transparent_project(directory.path());
    // Two seconds of 440 Hz at the project's sample rate; the item plays them
    // at 2x in one second, which `preservePitch` keeps at 440 Hz, and `pitch` 7
    // lifts it a fifth to 440 * 2^(7/12) = 659.26 Hz. Dropping either field
    // gives 880 Hz, 1318 Hz or 440 Hz instead.
    write_audio(
        &directory.path().join("tone.ctc"),
        8_000,
        1,
        &sine(440.0, 8_000, 2.0),
    )
    .unwrap();
    project.assets.insert(
        "tone".to_owned(),
        Asset::Audio {
            name: None,
            source: AssetSource::File {
                path: "tone.ctc".to_owned(),
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
            id: "tone".to_owned(),
            name: None,
            range: TimeRange {
                start: Time::ZERO,
                duration: Time::new(1, 1),
            },
            content: TimelineContent::Audio {
                asset: "tone".to_owned(),
                source_range: None,
                playback_rate: Some(Animatable::Static(2.0)),
                volume: None,
                muted: None,
                pitch: Some(Animatable::Static(7.0)),
                preserve_pitch: Some(true),
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
    let Some(output) = export_as(&project, directory.path(), "clip.mp4") else {
        return;
    };

    let samples = decoded_audio(&output, 8_000);
    let rate = 8_000.0;
    let window = &samples[(0.2 * rate) as usize..(0.8 * rate) as usize];
    let rising = window
        .windows(2)
        .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
        .count();
    let frequency = rising as f64 / 0.6;
    let expected = 440.0 * (7.0_f64 / 12.0).exp2();
    assert!(
        (expected * 0.97..=expected * 1.03).contains(&frequency),
        "expected about {expected} Hz, measured {frequency} Hz"
    );
}

#[test]
fn every_format_stores_the_mix_as_long_as_the_video() {
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    for format in ["mp4", "mov", "webm"] {
        let name = format!("clip.{format}");
        let Some(output) = export_as(&project, directory.path(), &name) else {
            return;
        };
        // One second at 8 kHz, with nothing before the video's start.
        let media = read(&output);
        let audio = stored_audio(&media);
        assert_eq!((audio.sample_rate, audio.channels), (8_000, 2), "{format}");
        assert_eq!(audio.lead_in_frames, 0, "{format}");
        assert_eq!(
            audio.samples.len(),
            8_000 * usize::from(audio.channels),
            "{format}: {} samples in {} channels",
            audio.samples.len(),
            audio.channels
        );
    }
}

/// `project` with a one-second clip from 0 s of `samples`, mono at the
/// project's sample rate, written to `name` in `dir`.
fn with_audio(mut project: Project, dir: &Path, samples: &[f32], name: &str) -> Project {
    write_audio(&dir.join(name), project.settings.sample_rate, 1, samples).unwrap();
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

#[test]
fn exports_an_audio_clip_with_fades() {
    let directory = tempfile::tempdir().unwrap();
    let mut project = with_audio(
        transparent_project(directory.path()),
        directory.path(),
        &sine(440.0, 8_000, 1.0),
        "sound.ctc",
    );
    let TimelineContent::Audio {
        fade_in, fade_out, ..
    } = &mut project.tracks.last_mut().unwrap().items[0].content
    else {
        unreachable!();
    };
    *fade_in = Some(Time::new(1, 4));
    *fade_out = Some(Time::new(1, 2));
    let Some(output) = export_as(&project, directory.path(), "clip.mp4") else {
        return;
    };

    let samples = decoded_audio(&output, 8_000);
    let rms = |from: f64, to: f64| {
        let rate = 8_000.0;
        let window = &samples[(from * rate) as usize..(to * rate) as usize];
        (window
            .iter()
            .map(|sample| f64::from(sample * sample))
            .sum::<f64>()
            / window.len() as f64)
            .sqrt()
    };
    // A linear ramp scales a sine's RMS by 1 / √3.
    let full = rms(0.3, 0.45);
    let near = |actual: f64, expected: f64| {
        assert!(
            (actual - expected).abs() <= expected * 0.1,
            "RMS {actual}, expected about {expected}"
        );
    };
    near(full, 0.125 / 2.0_f64.sqrt());
    near(rms(0.0, 0.25), full / 3.0_f64.sqrt());
    near(rms(0.5, 1.0), full / 3.0_f64.sqrt());
    near(rms(0.7, 0.8), full * 0.5);
    assert!(rms(0.0, 0.02) < full * 0.1);
    assert!(rms(0.98, 1.0) < full * 0.1);
}

#[test]
fn exports_a_bed_ducked_under_a_voice() {
    let directory = tempfile::tempdir().unwrap();
    let mut project = with_audio(
        transparent_project(directory.path()),
        directory.path(),
        &sine(440.0, 8_000, 1.0),
        "sound.ctc",
    );
    let track = project.tracks.last_mut().unwrap();
    // A silent voice from 0.4 to 0.6 s: only the bed is heard.
    let mut voice = track.items[0].clone();
    voice.id = "voice".to_owned();
    voice.range = TimeRange {
        start: Time::new(2, 5),
        duration: Time::new(1, 5),
    };
    let TimelineContent::Audio {
        volume, duck_group, ..
    } = &mut voice.content
    else {
        unreachable!();
    };
    *volume = Some(Animatable::Static(0.0));
    *duck_group = Some("voice".to_owned());
    let TimelineContent::Audio { duck, .. } = &mut track.items[0].content else {
        unreachable!();
    };
    *duck = Some(ProjectDuck {
        under: DuckGroups::One("voice".to_owned()),
        to: None,
        attack: None,
        release: None,
        hold_gap: None,
    });
    track.items.push(voice);
    let Some(output) = export_as(&project, directory.path(), "ducked.mp4") else {
        return;
    };

    let samples = decoded_audio(&output, 8_000);
    let rms = |from: f64, to: f64| {
        let rate = 8_000.0;
        let window = &samples[(from * rate) as usize..(to * rate) as usize];
        (window
            .iter()
            .map(|sample| f64::from(sample * sample))
            .sum::<f64>()
            / window.len() as f64)
            .sqrt()
    };
    let full = rms(0.05, 0.15);
    let ducked = rms(0.45, 0.55);
    assert!(full > 0.05, "RMS {full} before the voice");
    assert!(
        (ducked - full * 0.3).abs() <= full * 0.3 * 0.1,
        "RMS {ducked} under the voice, expected about {}",
        full * 0.3
    );
}

#[test]
fn a_looping_mp4_starts_its_sound_from_its_end() {
    let directory = tempfile::tempdir().unwrap();
    // Four hits a second, each a chord at full strength from its first
    // sample: played on repeat, the last one runs into the first as each
    // runs into the next.
    let hit = |t: f64| {
        let tau = std::f64::consts::TAU;
        0.2 * (-40.0 * t).exp()
            * ((tau * 1000.0 * t).cos()
                + (tau * 2500.0 * t).sin()
                + (tau * 4400.0 * t).sin()
                + (tau * 6000.0 * t).cos())
    };
    let mut project = transparent_project(directory.path());
    project.settings.sample_rate = 48_000;
    let project = with_audio(
        project,
        directory.path(),
        &generate(48_000, 1.0, |t| hit(t % 0.25)),
        "hits.ctc",
    );
    let export = |looping, name: &str| {
        let output = directory.path().join(name);
        match crate::exporter(ExportOptions {
            looping,
            ..ExportOptions::default()
        })
        .export_project(&project, directory.path(), &output)
        {
            Ok(()) => Some(output),
            Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
                skip_without_gpu(&error);
                None
            }
            Err(error) => panic!("export failed: {error}"),
        }
    };
    let (Some(once), Some(looped)) = (export(false, "once.mp4"), export(true, "looped.mp4")) else {
        return;
    };

    // The looping file's sound starts with the mix's last 16 384 frames,
    // which play before the video's start; from the start on it is the mix.
    let (once, looped) = (read(&once), read(&looped));
    let (once, looped) = (stored_audio(&once), stored_audio(&looped));
    assert_eq!(once.lead_in_frames, 0);
    assert_eq!(looped.lead_in_frames, 16 * 1024);
    let lead_in = 16 * 1024 * usize::from(once.channels);
    assert!(
        looped.lead_in() == &once.samples[once.samples.len() - lead_in..],
        "the lead-in is not the end of the mix"
    );
    assert!(
        looped.played() == once.samples.as_slice(),
        "from the video's start the looping sound is not the mix"
    );
}

/// Exports `project` with `options` to `name` in `dir`, with the loudness
/// and warnings it reported, or `None` without a GPU.
fn export_reporting(
    project: &Project,
    dir: &Path,
    options: ExportOptions,
    name: &str,
) -> Option<(PathBuf, Option<AudioLoudness>, Vec<String>)> {
    let output = dir.join(name);
    let (mut loudness, mut warnings) = (None, Vec::new());
    let result =
        crate::exporter(options).export_project_with_progress(project, dir, &output, |event| {
            match event {
                ExportProgress::Wrote(ExportedFile::Video {
                    loudness: measured, ..
                }) => {
                    loudness = measured;
                }
                ExportProgress::Warning(warning) => warnings.push(warning),
                _ => {}
            }
        });
    match result {
        Ok(()) => Some((output, loudness, warnings)),
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            None
        }
        Err(error) => panic!("export failed: {error}"),
    }
}

#[test]
fn brings_the_mix_to_a_loudness_target() {
    let directory = tempfile::tempdir().unwrap();
    let mut project = transparent_project(directory.path());
    project.settings.sample_rate = 48_000;
    // A quiet tone under short, loud hits: at -14 LUFS the hits would go
    // over the -1 dBTP ceiling, so the limiter holds them there.
    let tau = std::f64::consts::TAU;
    let project = with_audio(
        project,
        directory.path(),
        &generate(48_000, 1.0, |t| {
            let hit = if t % 0.25 < 0.005 {
                0.5 * (tau * 2000.0 * t).sin()
            } else {
                0.0
            };
            0.02 * (tau * 440.0 * t).sin() + hit
        }),
        "tone.ctc",
    );
    let options = ExportOptions {
        loudness: Some(LoudnessTarget::STREAMING),
        ..ExportOptions::default()
    };
    let Some((first, reported, warnings)) =
        export_reporting(&project, directory.path(), options.clone(), "first.mov")
    else {
        return;
    };
    assert!(warnings.is_empty(), "{warnings:?}");
    let reported = reported.expect("a video with sound reports its loudness");
    let decode = |path: &Path| {
        TestCodec::new()
            .decode_audio(celesta_codec_test::DECODER_ID, path, 48_000, 2)
            .unwrap()
    };
    let decoded = decode(&first);
    let measured = measure_loudness(&decoded);

    // The file keeps the mix as it was measured, so it measures as reported.
    let integrated = measured.integrated_lufs.unwrap();
    assert!((integrated + 14.0).abs() <= 0.1, "{integrated} LUFS");
    assert_eq!(measured.integrated_lufs, reported.integrated_lufs);
    let peak = measured.true_peak_dbtp.unwrap();
    assert!(peak <= -1.0 + 1e-3, "{peak} dBTP");
    // The gain alone would have taken the hits over the ceiling.
    let source = measure_loudness(&decode(&directory.path().join("tone.ctc")));
    let gain = reported.gain_db.unwrap();
    assert!(source.true_peak_dbtp.unwrap() + gain > 0.0, "{gain} dB");

    // The same project comes out the same.
    let (second, ..) = export_reporting(&project, directory.path(), options, "second.mov").unwrap();
    assert!(decode(&second).samples == decoded.samples);
}

#[test]
fn a_silent_mix_reports_no_loudness_and_is_left_alone() {
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let Some((_, measured, warnings)) = export_reporting(
        &project,
        directory.path(),
        ExportOptions::default(),
        "plain.mp4",
    ) else {
        return;
    };
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        measured,
        Some(AudioLoudness {
            integrated_lufs: None,
            true_peak_dbtp: None,
            gain_db: None,
        })
    );

    let (_, measured, warnings) = export_reporting(
        &project,
        directory.path(),
        ExportOptions {
            loudness: Some(LoudnessTarget::STREAMING),
            ..ExportOptions::default()
        },
        "target.mp4",
    )
    .unwrap();
    assert_eq!(measured.unwrap().gain_db, Some(0.0));
    assert!(
        warnings.len() == 1 && warnings[0].contains("silent"),
        "{warnings:?}"
    );
}

/// With no audio track there is no mix to bring to a loudness target, which
/// the export says rather than ignoring the target.
#[test]
fn a_loudness_target_without_an_audio_track_is_a_warning() {
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let Some((output, measured, warnings)) = export_reporting(
        &project,
        directory.path(),
        ExportOptions {
            output: OutputChoice {
                audio: AudioChoice::None,
                ..OutputChoice::default()
            },
            loudness: Some(LoudnessTarget::STREAMING),
            ..ExportOptions::default()
        },
        "silent.mp4",
    ) else {
        return;
    };
    assert_eq!(measured, None);
    assert_eq!(
        warnings,
        ["the output has no audio track, so there is no loudness to bring to the target"]
    );
    assert!(read(&output).audio.is_none());
}

#[test]
fn exports_a_silent_react_entry_to_webm() {
    let runtime_path = workspace_root().join("packages/cli/dist/cli.js");
    if !runtime_path.exists() {
        eprintln!("skipping React WebM test: build packages/cli first");
        return;
    }
    let directory = tempfile::tempdir_in(workspace_root().join("packages/cli")).unwrap();
    let entry = directory.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect } from '@celesta.studio/react';
export default function Root() {
  return <Composition width={64} height={64} fps={2} durationInFrames={2}>
    <Rect width={32} height={32} fill="#ff0000" />
  </Composition>;
}
"##,
    )
    .unwrap();
    let output = directory.path().join("film.webm");
    let (mut written, mut warnings) = (Vec::new(), Vec::new());
    // Without sound, a loudness target has nothing to adjust.
    match crate::exporter(ExportOptions {
        loudness: Some(LoudnessTarget::STREAMING),
        ..ExportOptions::default()
    })
    .export_react_entry_with_progress(
        &entry,
        &ReactRuntimeOptions::new("node", runtime_path),
        &output,
        |event| match event {
            ExportProgress::Wrote(file) => written.push(file),
            ExportProgress::Warning(warning) => warnings.push(warning),
            _ => {}
        },
    ) {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            return;
        }
        Err(error) => panic!("React WebM export failed: {error}"),
    }

    let media = read(&output);
    assert_eq!(media.extension.as_deref(), Some("webm"));
    assert!(media.audio.is_none());
    assert!(
        matches!(
            written[..],
            [ExportedFile::Video {
                audio: None,
                loudness: None,
                ..
            }]
        ),
        "{written:?}"
    );
    assert!(
        warnings.len() == 1 && warnings[0].contains("no sound"),
        "{warnings:?}"
    );
}

/// A React entry of two 64x64 frames at 2 fps with no sound, written to
/// `film.tsx` in a new directory under `packages/cli`; `None` until
/// `packages/cli` is built.
fn silent_react_entry() -> Option<(tempfile::TempDir, PathBuf, PathBuf)> {
    let runtime_path = workspace_root().join("packages/cli/dist/cli.js");
    if !runtime_path.exists() {
        eprintln!("skipping React export test: build packages/cli first");
        return None;
    }
    let directory = tempfile::tempdir_in(workspace_root().join("packages/cli")).unwrap();
    let entry = directory.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect } from '@celesta.studio/react';
export default function Root() {
  return <Composition width={64} height={64} fps={2} durationInFrames={2}>
    <Rect width={32} height={32} fill="#ff0000" />
  </Composition>;
}
"##,
    )
    .unwrap();
    Some((directory, entry, runtime_path))
}

/// Without a mix there is no pre-roll to send, so a looping React entry
/// with no sound gets a video-only file.
#[test]
fn a_silent_react_entry_with_loop_has_no_audio_track() {
    let Some((directory, entry, runtime_path)) = silent_react_entry() else {
        return;
    };
    let output = directory.path().join("film.mp4");
    let mut written = Vec::new();
    match crate::exporter(ExportOptions {
        looping: true,
        ..ExportOptions::default()
    })
    .export_react_entry_with_progress(
        &entry,
        &ReactRuntimeOptions::new("node", runtime_path),
        &output,
        |event| {
            if let ExportProgress::Wrote(file) = event {
                written.push(file);
            }
        },
    ) {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            return;
        }
        Err(error) => panic!("React export failed: {error}"),
    }
    assert!(
        matches!(written[..], [ExportedFile::Video { audio: None, .. }]),
        "{written:?}"
    );
    assert!(read(&output).audio.is_none());
}

#[test]
fn writes_a_react_entrys_subtitles_shifted_to_the_exported_range() {
    let runtime_path = workspace_root().join("packages/cli/dist/cli.js");
    if !runtime_path.exists() {
        eprintln!("skipping React subtitle test: build packages/cli first");
        return;
    }
    let directory = tempfile::tempdir_in(workspace_root().join("packages/cli")).unwrap();
    let entry = directory.path().join("film.tsx");
    std::fs::write(
        directory.path().join("captions.srt"),
        "\u{feff}1\r\n00:00:00,000 --> 00:00:01,000\r\nBefore\r\n\r\n\
         2\r\n00:00:01,000 --> 00:00:02,000\r\nAcross <i>start</i>\r\n\r\n\
         3\r\n00:00:02,000 --> 00:00:02,500\r\nInside\r\n\r\n\
         4\r\n00:00:02,500 --> 00:00:04,000\r\nAcross end\r\n",
    )
    .unwrap();
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect, Sequence, Subtitles, parseSubtitles } from '@celesta.studio/react';
import captions from './captions.srt';
const cues = parseSubtitles(captions, { fps: 10 });
export default function Root() {
  return <Composition width={64} height={64} fps={10} durationInFrames={40}>
    <Rect width={32} height={32} fill="#ff0000" />
    <Sequence from={5}><Subtitles cues={cues} /></Sequence>
  </Composition>;
}
"##,
    )
    .unwrap();
    let output = directory.path().join("film.mp4");
    let subtitles = directory.path().join("film.vtt");
    let mut written = Vec::new();
    match crate::exporter(ExportOptions {
        range: Some(ExportRange::new(Time::new(2, 1), Time::new(7, 2))),
        subtitles: vec![subtitles.clone()],
        ..ExportOptions::default()
    })
    .export_react_entry_with_progress(
        &entry,
        &ReactRuntimeOptions::new("node", runtime_path),
        &output,
        |event| {
            if let ExportProgress::Wrote(file) = event {
                written.push(file);
            }
        },
    ) {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            return;
        }
        Err(error) => panic!("React subtitle export failed: {error}"),
    }

    // The sequence shifts the cues by 0.5 s; the range starts the file at
    // 2 s and ends it at 3.5 s, so "Before" is never shown and the cues on
    // either edge are cut.
    assert_eq!(
        std::fs::read_to_string(&subtitles).unwrap(),
        "WEBVTT\n\n\
         00:00:00.000 --> 00:00:00.500\nAcross start\n\n\
         00:00:00.500 --> 00:00:01.000\nInside\n\n\
         00:00:01.000 --> 00:00:01.500\nAcross end\n"
    );
    assert!(
        matches!(
            &written[..],
            [
                ExportedFile::Video { frames: 15, .. },
                ExportedFile::Subtitles { cues: 3, .. }
            ]
        ),
        "{written:?}"
    );
}

#[test]
fn writes_only_a_react_entrys_subtitles_per_language_and_format() {
    let runtime_path = workspace_root().join("packages/cli/dist/cli.js");
    if !runtime_path.exists() {
        eprintln!("skipping React subtitle test: build packages/cli first");
        return;
    }
    let directory = tempfile::tempdir_in(workspace_root().join("packages/cli")).unwrap();
    let entry = directory.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Sequence, Subtitles } from '@celesta.studio/react';
const ja = [{ from: 0, durationInFrames: 10, text: 'こんにちは' }, { from: 20, durationInFrames: 10, text: '世界' }];
const en = [{ from: 0, durationInFrames: 10, text: 'Hello' }, { from: 20, durationInFrames: 10, text: 'World' }];
export default function Root() {
  return <Composition width={64} height={64} fps={10} durationInFrames={30} lang="ja">
    <Subtitles cues={ja} />
    <Sequence from={5}><Subtitles lang="en" cues={en} /></Sequence>
  </Composition>;
}
"##,
    )
    .unwrap();
    let per_language =
        |extension: &str| directory.path().join(format!("film.{{lang}}.{extension}"));
    let mut written = Vec::new();
    let mut rendered = Vec::new();
    let job = ExportJob {
        source: entry.clone(),
        react: true,
        composition_id: None,
        companion: None,
        props_file: None,
        props: None,
        output: None,
        stills: None,
        options: ExportOptions {
            subtitles: vec![per_language("srt"), per_language("vtt")],
            ..ExportOptions::default()
        },
    };
    let runtime = ReactRuntimeOptions::new("node", &runtime_path);
    crate::exporter(job.options.clone())
        .export_react_subtitles(
            &entry,
            &runtime,
            None,
            &ExportCancellation::default(),
            |event| match event {
                ExportProgress::Wrote(file) => written.push(file),
                ExportProgress::Rendering { frame, total } => rendered.push((frame, total)),
                _ => {}
            },
        )
        .unwrap();

    // No video, one file per language and format; the English cues are
    // shifted by their sequence and cut to the composition.
    assert!(!directory.path().join("film.mp4").exists());
    assert_eq!(
        std::fs::read_to_string(directory.path().join("film.ja.srt")).unwrap(),
        "1\n00:00:00,000 --> 00:00:01,000\nこんにちは\n\n\
         2\n00:00:02,000 --> 00:00:03,000\n世界\n"
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("film.en.vtt")).unwrap(),
        "WEBVTT\n\n\
         00:00:00.500 --> 00:00:01.500\nHello\n\n\
         00:00:02.500 --> 00:00:03.000\nWorld\n"
    );
    let mut files: Vec<_> = written
        .iter()
        .map(|file| match file {
            ExportedFile::Subtitles {
                path, cues, lang, ..
            } => (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                *cues,
                lang.clone(),
            ),
            other => panic!("only subtitles should be written, not {other:?}"),
        })
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            ("film.en.srt".into(), 2, Some("en".into())),
            ("film.en.vtt".into(), 2, Some("en".into())),
            ("film.ja.srt".into(), 2, Some("ja".into())),
            ("film.ja.vtt".into(), 2, Some("ja".into())),
        ]
    );
    assert_eq!(rendered.last(), Some(&(30, 30)));

    // A path without {lang} cannot take both languages, and writes nothing.
    let mixed = directory.path().join("mixed.srt");
    let error = crate::exporter(ExportOptions {
        subtitles: vec![directory.path().join("other.{lang}.srt"), mixed.clone()],
        ..ExportOptions::default()
    })
    .export_react_subtitles(
        &entry,
        &runtime,
        None,
        &ExportCancellation::default(),
        |_| {},
    )
    .unwrap_err();
    assert!(
        matches!(&error, ExportError::MixedSubtitleLanguages { languages, .. } if languages == &["en", "ja"]),
        "{error}"
    );
    assert!(!mixed.exists());
    assert!(!directory.path().join("other.ja.srt").exists());

    // The job runs the same export when it has no output.
    std::fs::remove_file(directory.path().join("film.ja.srt")).unwrap();
    let mut job = job;
    job.options.subtitles = vec![per_language("srt")];
    job.options.overwrite = true;
    job.run(&ExportCancellation::default(), &mut |_| {})
        .unwrap();
    assert!(directory.path().join("film.ja.srt").exists());
}

/// Writes plugin `hang` into `directory`: a component, `<Hang>`, whose layer
/// is the plugin's content, and a program that answers `hello` and `open`,
/// then never answers `frame`. The program writes its process id to `pid`
/// once it has answered.
#[cfg(unix)]
fn write_hanging_plugin(directory: &Path) {
    use celesta_plugin::protocol::{Message, encode_opened, write_message};
    use celesta_plugin::{MeshModel, ModelId, Opened};
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(directory.join("bin")).unwrap();
    std::fs::write(
        directory.join(celesta_plugin_host::MANIFEST),
        serde_json::json!({
            "id": "hang",
            "version": "1.0.0",
            "protocol": 1,
            "celesta": concat!("=", env!("CARGO_PKG_VERSION")),
            "module": "index.ts",
            "executable": { celesta_plugin_host::platform().0: "bin/p" },
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        directory.join("index.ts"),
        r#"
import * as React from 'react';
import { registerHostElement } from '@celesta.studio/react/internal';

const ELEMENT = 'celesta-plugin-hang';
export function Hang(): React.ReactElement {
  return React.createElement(ELEMENT, {});
}
registerHostElement(ELEMENT, {
  content: () => ({ type: 'plugin', plugin: 'hang', assets: {}, time: 0 }),
});
"#,
    )
    .unwrap();
    let mut answers = Vec::new();
    let hello = serde_json::json!({
        "type": "hello", "protocol": 1, "plugin": "hang",
        "version": "1.0.0", "roles": ["content"],
    });
    write_message(&mut answers, &Message::new(hello)).unwrap();
    let opened = Opened {
        model: ModelId(1),
        content: MeshModel {
            canvas: [2.0, 2.0],
            textures: Vec::new(),
            meshes: Vec::new(),
        }
        .into(),
        notices: Vec::new(),
    };
    write_message(&mut answers, &encode_opened(&opened)).unwrap();
    std::fs::write(directory.join("answers"), answers).unwrap();
    let program = directory.join("bin/p");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\ncat answers\necho $$ > '{}'\nexec sleep 60\n",
            directory.join("pid").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Cancel kills the plugins an export runs: a frame waiting on a plugin that
/// never answers returns at once, long before the frame's deadline, and the
/// export ends as cancelled with the plugin's process gone.
#[cfg(unix)]
#[test]
fn cancel_ends_a_frame_waiting_on_a_plugin_that_hangs() {
    use std::time::{Duration, Instant};

    let runtime_path = workspace_root().join("packages/cli/dist/cli.js");
    if !runtime_path.exists() {
        eprintln!("skipping the hanging plugin test: build packages/cli first");
        return;
    }
    let directory = tempfile::tempdir_in(workspace_root().join("packages/cli")).unwrap();
    let plugin = directory.path().join("plugins/hang");
    write_hanging_plugin(&plugin);
    let (plugins, errors) = celesta_plugin_host::discover(&[directory.path().join("plugins")]);
    assert!(errors.is_empty(), "{errors:?}");
    let entry = directory.path().join("film.tsx");
    std::fs::write(
        &entry,
        r#"
import { Composition } from '@celesta.studio/react';
import { Hang } from '@celesta-plugin/hang';
export default function Root() {
  return <Composition width={64} height={64} fps={2} durationInFrames={2}>
    <Hang />
  </Composition>;
}
"#,
    )
    .unwrap();

    let cancellation = ExportCancellation::default();
    let pid = plugin.join("pid");
    let canceller = {
        let cancellation = cancellation.clone();
        let pid = pid.clone();
        std::thread::spawn(move || {
            // Once the plugin answered `open`, the frame waits on it.
            let deadline = Instant::now() + Duration::from_secs(60);
            while !pid.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            std::thread::sleep(Duration::from_millis(300));
            cancellation.cancel();
        })
    };
    let output = directory.path().join("film.mp4");
    let begun = Instant::now();
    let result = crate::exporter(ExportOptions {
        plugins: Some(celesta_plugin_host::InstalledPlugins {
            plugins,
            ..Default::default()
        }),
        ..ExportOptions::default()
    })
    .export_react_entry_cancellable(
        &entry,
        &ReactRuntimeOptions::new("node", runtime_path),
        &output,
        &cancellation,
        |_| {},
    );
    if let Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) = &result {
        skip_without_gpu(error);
        cancellation.cancel();
        canceller.join().unwrap();
        return;
    }
    canceller.join().unwrap();
    assert!(matches!(result, Err(ExportError::Cancelled)), "{result:?}");
    assert!(
        begun.elapsed() < Duration::from_secs(25),
        "the frame returned before its 30 s deadline"
    );
    assert!(!output.exists());
    let pid = std::fs::read_to_string(&pid).unwrap();
    let running = std::process::Command::new("kill")
        .args(["-0", pid.trim()])
        .status()
        .unwrap()
        .success();
    assert!(!running, "the plugin's process was stopped");
}

/// The muxer writes a staged file; once it is published, the report names
/// the published path, and no staged file is left beside it.
#[test]
fn a_written_file_is_named_by_its_published_path() {
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let output = directory.path().join("film.mp4");
    let mut written = Vec::new();
    match crate::exporter(ExportOptions::default()).export_project_with_progress(
        &project,
        directory.path(),
        &output,
        |event| {
            if let ExportProgress::Wrote(file) = event {
                written.push(file);
            }
        },
    ) {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            return;
        }
        Err(error) => panic!("export failed: {error}"),
    }
    let [ExportedFile::Video { files, .. }] = &written[..] else {
        panic!("{written:?}");
    };
    assert_eq!(files.as_slice(), std::slice::from_ref(&output));
    assert!(output.exists());
    let names = files_in(directory.path());
    assert!(
        names.iter().all(|name| !name.starts_with(".celesta-")),
        "{names:?}"
    );
}

/// Cancelling while frames are encoded leaves no file in the output's
/// directory: not the staged output, and not a spill file.
#[test]
fn cancel_during_encoding_leaves_no_files() {
    let assets = tempfile::tempdir().unwrap();
    let project = transparent_project(assets.path());
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("cancelled.mp4");
    let cancellation = ExportCancellation::default();
    let canceller = cancellation.clone();
    let result = crate::exporter(ExportOptions::default()).export_project_cancellable(
        &project,
        assets.path(),
        &output,
        &cancellation,
        move |event| {
            if matches!(event, ExportProgress::Rendering { frame: 1, .. }) {
                canceller.cancel();
            }
        },
    );
    if let Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) = &result {
        skip_without_gpu(error);
        return;
    }
    assert!(matches!(result, Err(ExportError::Cancelled)), "{result:?}");
    assert_eq!(files_in(directory.path()), Vec::<String>::new());

    #[cfg(unix)]
    cancel_ends_a_frame_waiting_on_a_media_plugin_that_hangs(&project, assets.path());
}

/// The folder of the media plugin `hang` [`hanging_media_plugin`] writes:
/// one per test process, under Cargo's temporary folder for tests.
#[cfg(unix)]
fn hanging_media_folder() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("hang-media-{}", std::process::id()))
}

/// Writes the media plugin `hang` and registers it: a video encoder
/// `video` writing `ctst`, which the test codec's muxer takes, whose
/// program answers `hello`, `describe` and `openVideoEncoder`, each once it
/// is asked, then never answers `encodeVideoFrame`. The program writes its
/// process id to `pid` once it has read that first `encodeVideoFrame`.
#[cfg(unix)]
pub fn hanging_media_plugin() -> celesta_media::RegisteredPlugin {
    use celesta_media::{ComponentDescription, ComponentKind};
    use celesta_plugin::protocol::{Message, write_message};
    use std::os::unix::fs::PermissionsExt;

    let directory = hanging_media_folder();
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("bin")).unwrap();
    std::fs::write(
        directory.join(celesta_plugin_host::MANIFEST),
        serde_json::json!({
            "id": "hang",
            "version": "1.0.0",
            "protocol": 2,
            "celesta": concat!("=", env!("CARGO_PKG_VERSION")),
            "roles": ["media"],
            "executable": { celesta_plugin_host::platform().0: "bin/p" },
            "media": {
                "videoEncoders": [{ "id": "video", "name": "Hang", "codecs": ["ctst"] }],
            },
        })
        .to_string(),
    )
    .unwrap();
    let video = ComponentDescription {
        available: true,
        reason: None,
        input_formats: vec![PixelFormat::Rgba8],
        transports: vec!["pipe".to_owned()],
        ..ComponentDescription::unavailable(ComponentKind::VideoEncoder, "video", "")
    };
    // Each answer waits for its request. A message is a header and a
    // payload, each after its length as a little-endian u32; `dd bs=1` reads
    // exactly the bytes asked for, so nothing past the request is taken.
    let answers = [
        (
            "hello",
            serde_json::json!({ "type": "hello", "protocol": 2, "roles": ["media"] }),
        ),
        (
            "described",
            serde_json::json!({ "type": "described", "components": [video] }),
        ),
        (
            "opened",
            serde_json::json!({
                "type": "videoEncoderOpened", "session": 1, "input": "rgba8", "alpha": false,
            }),
        ),
    ];
    let mut script = String::from(
        "#!/bin/sh\n\
         part() {\n\
         \x20 n=$(dd bs=1 count=4 2>/dev/null | od -An -tu4 | tr -d ' ')\n\
         \x20 if [ \"${n:-0}\" -gt 0 ]; then dd bs=1 count=\"$n\" of=/dev/null 2>/dev/null; fi\n\
         }\n",
    );
    for (name, answer) in answers {
        let mut bytes = Vec::new();
        write_message(&mut bytes, &Message::new(answer)).unwrap();
        std::fs::write(directory.join(name), bytes).unwrap();
        script.push_str(&format!("part; part; cat {name}\n"));
    }
    // The pid appears once the first `encodeVideoFrame`'s header is read:
    // from then on a frame waits on the plugin.
    script.push_str(&format!(
        "part; echo $$ > '{}'\nexec sleep 60\n",
        directory.join("pid").display()
    ));
    let program = directory.join("bin/p");
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();

    let (plugins, errors) = celesta_plugin_host::discover(&[directory]);
    assert!(errors.is_empty(), "{errors:?}");
    let installed = celesta_plugin_host::InstalledPlugins {
        plugins,
        waiting: Vec::new(),
    };
    let registry = celesta_exporter::media_registry(&installed);
    assert_eq!(registry.plugins().len(), 1);
    registry.plugin(0).clone()
}

/// Cancel kills an export's media plugins: a frame waiting on an encoder
/// that never answers returns at once, the session's abort does not wait
/// beyond its 5 s deadline, and nothing is left in the output's directory.
#[cfg(unix)]
fn cancel_ends_a_frame_waiting_on_a_media_plugin_that_hangs(project: &Project, assets: &Path) {
    use std::time::{Duration, Instant};

    let pid = hanging_media_folder().join("pid");
    let _ = std::fs::remove_file(&pid);
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("hang.mp4");
    let cancellation = ExportCancellation::default();
    let canceller = {
        let cancellation = cancellation.clone();
        let pid = pid.clone();
        std::thread::spawn(move || {
            // Once the plugin has read the frame, the export waits on it.
            let deadline = Instant::now() + Duration::from_secs(60);
            while !pid.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            std::thread::sleep(Duration::from_millis(300));
            cancellation.cancel();
        })
    };
    let begun = Instant::now();
    let result = crate::exporter(ExportOptions {
        output: OutputChoice {
            video: EncoderChoice {
                component: Some("hang/video".parse().unwrap()),
                ..EncoderChoice::default()
            },
            ..OutputChoice::default()
        },
        ..ExportOptions::default()
    })
    .export_project_cancellable(project, assets, &output, &cancellation, |_| {});
    let elapsed = begun.elapsed();
    canceller.join().unwrap();
    assert!(matches!(result, Err(ExportError::Cancelled)), "{result:?}");
    assert!(
        elapsed < Duration::from_secs(5 + 2),
        "the export took {elapsed:?}, past the abort deadline"
    );
    let pid = std::fs::read_to_string(&pid).unwrap();
    let running = std::process::Command::new("kill")
        .args(["-0", pid.trim()])
        .status()
        .unwrap()
        .success();
    assert!(!running, "the plugin's process was stopped");
    assert_eq!(files_in(directory.path()), Vec::<String>::new());
    let _ = std::fs::remove_dir_all(hanging_media_folder());
}
