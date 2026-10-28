use std::path::{Path, PathBuf};

use celesta_composition::{Rational, Time, TimeRange};
use celesta_exporter::{
    AudioLoudness, BitDepth, ColorConversion, EncoderPreset, ExportCancellation, ExportError,
    ExportJob, ExportOptions, ExportProgress, ExportRange, ExportedFile, Exporter, LoudnessTarget,
    ReactRuntimeOptions, VideoEncoding, VideoFormat,
};
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::{AudioDecoder, FfmpegBackend, VideoFrameDecoder, measure_loudness};
use celesta_project::{
    Animatable, Asset, AssetSource, DuckGroups, Project, ProjectDuck, TimelineContent,
    TimelineItem, Track, TrackKind,
};
use ez_ffmpeg::stream_info::{StreamInfo, find_audio_stream_info, find_video_stream_info};
use ez_ffmpeg::{FfmpegContext, Input, Output};
use strum::VariantArray;

/// Reports a test skipped for want of a GPU adapter, or fails it when
/// `CELESTA_REQUIRE_GPU=1` says the machine has one.
fn skip_without_gpu(error: &impl std::fmt::Display) {
    assert!(
        std::env::var("CELESTA_REQUIRE_GPU").as_deref() != Ok("1"),
        "GPU tests require an adapter: {error}"
    );
    eprintln!("skipping live export test: no GPU adapter is available: {error}");
}

/// Renders a synthetic `lavfi` source to a lossless MKV fixture through the
/// linked FFmpeg libraries, forcing the output frame rate so the container
/// records a real frame-duration hint.
fn generate_source(path: &Path, lavfi: &str, fps: (i32, i32)) {
    FfmpegContext::builder()
        .input(Input::from(lavfi).set_format("lavfi"))
        .output(
            Output::from(path.to_string_lossy().into_owned())
                .set_video_codec("ffv1")
                .set_framerate(fps.0, fps.1),
        )
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
}

#[test]
fn exports_frame_exact_mp4_with_silent_audio() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("export.mp4");
    let source = directory.path().join("source.mkv");
    generate_source(&source, "testsrc2=size=64x64:rate=2:duration=1", (2, 1));

    let mut project =
        Project::load(workspace_root().join("examples/minimal.celesta.json")).unwrap();
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

    let exporter = Exporter::new(ExportOptions {
        overwrite: false,
        range: None,
        ..ExportOptions::default()
    });
    match exporter.export_project(&project, directory.path(), &output) {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            return;
        }
        Err(error) => panic!("export failed: {error}"),
    }

    let url = output.to_string_lossy().into_owned();
    let Some(StreamInfo::Video {
        width,
        height,
        avg_frame_rate,
        ..
    }) = find_video_stream_info(&url).unwrap()
    else {
        panic!("exported file has no video stream");
    };
    assert_eq!((width, height), (64, 64));
    assert_eq!(
        (avg_frame_rate.num, avg_frame_rate.den),
        (2, 1),
        "exported frame rate"
    );
    let Some(StreamInfo::Audio {
        sample_rate,
        nb_channels,
        ..
    }) = find_audio_stream_info(&url).unwrap()
    else {
        panic!("exported file has no (silent) audio stream");
    };
    assert_eq!((sample_rate, nb_channels), (8_000, 2));

    // The two rendered frames are distinct decoded frames carrying the
    // testsrc2 pattern (not a flat fill).
    let mut decoder = FfmpegBackend::new();
    let first_frame = decoder.decode_frame(&output, 0.0).unwrap();
    let second_frame = decoder.decode_frame(&output, 0.5).unwrap();
    assert!(
        first_frame
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| { pixel[0] != pixel[1] || pixel[1] != pixel[2] || pixel[0] != 0 })
    );
    assert_ne!(first_frame.pixels, second_frame.pixels);

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
    let source = directory.path().join("source.mkv");
    // Four distinct frames at 2 fps over two seconds.
    generate_source(&source, "testsrc2=size=64x64:rate=2:duration=2", (2, 1));

    let mut project =
        Project::load(workspace_root().join("examples/minimal.celesta.json")).unwrap();
    project.settings.width = 64;
    project.settings.height = 64;
    project.settings.frame_rate = Rational::new(2, 1);
    project.settings.sample_rate = 8_000;
    project.settings.duration = Some(Time::new(2, 1));
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
                duration: Time::new(2, 1),
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
            transform: None,
            opacity: None,
            blend_mode: None,
            effects: None,
        }],
    });

    let full_output = directory.path().join("full.mp4");
    let full = Exporter::new(ExportOptions {
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
    Exporter::new(ExportOptions {
        overwrite: false,
        range: Some(ExportRange::new(Time::new(1, 1), Time::new(2, 1))),
        ..ExportOptions::default()
    })
    .export_project(&project, directory.path(), &windowed_output)
    .expect("windowed export");

    let url = windowed_output.to_string_lossy().into_owned();
    let Some(StreamInfo::Video {
        width,
        height,
        avg_frame_rate,
        nb_frames,
        ..
    }) = find_video_stream_info(&url).unwrap()
    else {
        panic!("windowed export has no video stream");
    };
    assert_eq!((width, height), (64, 64));
    assert_eq!((avg_frame_rate.num, avg_frame_rate.den), (2, 1));
    // 1.0s .. 2.0s at 2 fps: two frames, not the full timeline's four.
    assert_eq!(nb_frames, 2);
    assert!(find_audio_stream_info(&url).unwrap().is_some());

    // The window is shifted to start at the file's 00:00, so its first frame
    // is the full export's frame at 1.0s, not the one at 0.0s.
    let mut decoder = FfmpegBackend::new();
    let windowed_first = decoder.decode_frame(&windowed_output, 0.0).unwrap();
    assert_ne!(
        windowed_first.pixels,
        decoder.decode_frame(&full_output, 0.0).unwrap().pixels
    );
}

#[test]
fn rejects_an_empty_export_range() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("export.mp4");
    let mut project =
        Project::load(workspace_root().join("examples/minimal.celesta.json")).unwrap();
    project.settings.duration = Some(Time::new(1, 1));

    let result = Exporter::new(ExportOptions {
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
fn converting_colors_on_the_gpu_matches_the_encoder_conversion() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.mkv");
    generate_source(&source, "testsrc2=size=64x64:rate=2:duration=1", (2, 1));

    let mut project =
        Project::load(workspace_root().join("examples/minimal.celesta.json")).unwrap();
    project.settings.width = 64;
    project.settings.height = 64;
    project.settings.frame_rate = Rational::new(2, 1);
    project.settings.sample_rate = 8_000;
    project.settings.duration = Some(Time::new(1, 1));
    project.assets.insert(
        "source".to_owned(),
        Asset::Video {
            name: None,
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
            name: None,
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

    // Lossless encodes, so any difference comes from the conversion itself.
    let export = |color_conversion, bit_depth, name: &str| {
        let output = directory.path().join(name);
        let result = Exporter::new(ExportOptions {
            video: VideoEncoding {
                preset: EncoderPreset::Ultrafast,
                crf: 0,
                color_conversion,
                bit_depth,
            },
            ..ExportOptions::default()
        })
        .export_project(&project, directory.path(), &output);
        result.map(|()| output)
    };
    let mut pixel_formats: Vec<i32> = Vec::new();
    for &bit_depth in BitDepth::VARIANTS {
        let on_gpu = match export(
            ColorConversion::Gpu,
            bit_depth,
            &format!("gpu-{bit_depth}.mp4"),
        ) {
            Ok(output) => output,
            Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
                skip_without_gpu(&error);
                return;
            }
            Err(ExportError::Render(GpuRenderError::UnsupportedReadbackFormat(_))) => {
                eprintln!("skipping GPU color conversion test: not supported here");
                return;
            }
            Err(error) => panic!("GPU conversion export failed: {error}"),
        };
        let in_encoder = export(
            ColorConversion::Encoder,
            bit_depth,
            &format!("encoder-{bit_depth}.mp4"),
        )
        .unwrap();

        let mut decoder = FfmpegBackend::new();
        for time in [0.0, 0.5] {
            let gpu = decoder.decode_frame(&on_gpu, time).unwrap();
            let encoder = decoder.decode_frame(&in_encoder, time).unwrap();
            assert_eq!(gpu.pixels.len(), encoder.pixels.len());
            let difference: u64 = gpu
                .pixels
                .iter()
                .zip(encoder.pixels.iter())
                .map(|(left, right)| u64::from(left.abs_diff(*right)))
                .sum();
            let mean = difference as f64 / gpu.pixels.len() as f64;
            // The two differ only in how chroma is filtered at color edges.
            assert!(
                mean < 2.0,
                "{bit_depth}-bit: mean difference {mean} at {time}s"
            );
        }
        for output in [&on_gpu, &in_encoder] {
            let url = output.to_string_lossy().into_owned();
            let Some(StreamInfo::Video { pixel_format, .. }) =
                find_video_stream_info(&url).unwrap()
            else {
                panic!("exported file has no video stream");
            };
            pixel_formats.push(pixel_format);
        }
    }
    // yuv420p (AV_PIX_FMT_YUV420P is 0) for 8 bits; another, the same for
    // both conversions, for 10.
    let [eight_gpu, eight_encoder, ten_gpu, ten_encoder] = pixel_formats[..] else {
        panic!("{pixel_formats:?}");
    };
    assert_eq!((eight_gpu, eight_encoder), (0, 0));
    assert_eq!(ten_gpu, ten_encoder);
    assert_ne!(ten_gpu, 0);
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

/// Exports `project` as `format` to `name` in `dir`, or `None` without a GPU.
fn export_as(project: &Project, dir: &Path, format: VideoFormat, name: &str) -> Option<PathBuf> {
    let output = dir.join(name);
    match Exporter::new(ExportOptions {
        video_format: format,
        ..ExportOptions::default()
    })
    .export_project(project, dir, &output)
    {
        Ok(()) => Some(output),
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            None
        }
        Err(error) => panic!("{format} export failed: {error}"),
    }
}

/// The first video frame of `path` as `rgba64le` samples, decoded with
/// `decoder` when given (FFmpeg's native VP9 decoder drops the alpha plane).
fn decode_rgba64(path: &Path, decoder: Option<&str>) -> Vec<u16> {
    let raw = path.with_extension("rgba64");
    let mut input = Input::from(path.to_string_lossy().into_owned());
    if let Some(decoder) = decoder {
        input = input.set_video_codec(decoder);
    }
    FfmpegContext::builder()
        .input(input)
        .output(
            Output::from(raw.to_string_lossy().into_owned())
                .add_stream_map("0:v:0")
                .set_format("rawvideo")
                .set_pix_fmt("rgba64le")
                .set_max_video_frames(1),
        )
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
    std::fs::read(raw)
        .unwrap()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&sample| u16::from_le_bytes(sample))
        .collect()
}

fn pixel(pixels: &[u16], x: usize, y: usize) -> [u16; 4] {
    let start = (y * 64 + x) * 4;
    pixels[start..start + 4].try_into().unwrap()
}

/// The video stream's codec, `(color space, primaries, transfer)` and
/// metadata.
fn video_stream(
    url: &str,
) -> (
    String,
    (i32, i32, i32),
    std::collections::HashMap<String, String>,
) {
    let Some(StreamInfo::Video {
        codec_name,
        color_space,
        color_primaries,
        color_transfer,
        metadata,
        ..
    }) = find_video_stream_info(url).unwrap()
    else {
        panic!("exported file has no video stream");
    };
    (
        codec_name,
        (color_space, color_primaries, color_transfer),
        metadata,
    )
}

fn audio_stream(url: &str) -> (String, i32) {
    let Some(StreamInfo::Audio {
        codec_name,
        sample_rate,
        ..
    }) = find_audio_stream_info(url).unwrap()
    else {
        panic!("exported file has no audio stream");
    };
    (codec_name, sample_rate)
}

/// `path`'s first audio stream decoded to mono `f32` samples, with the sample
/// rate the stream reports (the decoder drops the encoder's priming samples).
fn decoded_audio(path: &Path) -> (Vec<f32>, u32) {
    let raw = path.with_extension("f32");
    FfmpegContext::builder()
        .input(Input::from(path.to_string_lossy().into_owned()))
        .output(
            Output::from(raw.to_string_lossy().into_owned())
                .add_stream_map("0:a:0")
                .set_format("f32le")
                .set_audio_codec("pcm_f32le")
                .set_audio_channels(1),
        )
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
    let (_, sample_rate) = audio_stream(&path.to_string_lossy());
    let bytes = std::fs::read(raw).unwrap();
    let samples = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect();
    (samples, sample_rate as u32)
}

/// How many seconds of audio `path` decodes to, after the decoder drops the
/// encoder's priming samples.
fn decoded_audio_seconds(path: &Path) -> f64 {
    let (samples, sample_rate) = decoded_audio(path);
    samples.len() as f64 / f64::from(sample_rate)
}

/// The red quadrant keeps its straight color and half alpha (within
/// `tolerance` 16-bit codes); undrawn pixels are fully transparent.
fn assert_transparent_quadrant(pixels: &[u16], tolerance: u16) {
    let (inside, outside) = (pixel(pixels, 8, 8), pixel(pixels, 48, 48));
    assert!(inside[3].abs_diff(128 * 257) <= 2 * 257, "{inside:?}");
    assert!(
        inside[0].abs_diff(65535) <= tolerance && inside[1] <= tolerance,
        "{inside:?}"
    );
    assert_eq!(outside[3], 0, "{outside:?}");
}

#[test]
fn exports_transparent_prores_4444() {
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let Some(output) = export_as(&project, directory.path(), VideoFormat::Mov, "clip.mov") else {
        return;
    };

    let url = output.to_string_lossy().into_owned();
    // AVCOL_SPC_BT709, AVCOL_PRI_BT709 and AVCOL_TRC_BT709 are all 1.
    let (codec_name, colors, _) = video_stream(&url);
    assert_eq!(codec_name, "prores");
    assert_eq!(colors, (1, 1, 1));
    assert_eq!(audio_stream(&url), ("pcm_s24le".to_owned(), 8_000));
    assert_transparent_quadrant(&decode_rgba64(&output, None), 3 * 257);
}

#[test]
fn exports_transparent_vp9_webm() {
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let Some(output) = export_as(&project, directory.path(), VideoFormat::Webm, "clip.webm") else {
        return;
    };

    let url = output.to_string_lossy().into_owned();
    let (codec_name, colors, metadata) = video_stream(&url);
    assert_eq!(codec_name, "vp9");
    // Matroska tags read back upper-case (`ALPHA_MODE`).
    assert!(
        metadata
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("alpha_mode") && value == "1"),
        "{metadata:?}"
    );
    assert_eq!(colors, (1, 1, 1));
    // `mux_audio` always resamples Opus to 48 kHz.
    assert_eq!(audio_stream(&url), ("opus".to_owned(), 48_000));
    // 4:2:0 chroma blurs the quadrant's color a little more.
    assert_transparent_quadrant(&decode_rgba64(&output, Some("libvpx-vp9")), 6 * 257);
}

#[test]
fn exports_a_pitched_audio_clip() {
    let directory = tempfile::tempdir().unwrap();
    let mut project = transparent_project(directory.path());
    // Two seconds of 440 Hz at the project's sample rate; the item plays them
    // at 2x in one second, which `preservePitch` keeps at 440 Hz, and `pitch` 7
    // lifts it a fifth to 440 * 2^(7/12) = 659.26 Hz. Dropping either field
    // gives 880 Hz, 1318 Hz or 440 Hz instead.
    FfmpegContext::builder()
        .input(Input::from("sine=frequency=440:sample_rate=8000:duration=2").set_format("lavfi"))
        .output(
            directory
                .path()
                .join("tone.wav")
                .to_string_lossy()
                .into_owned(),
        )
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
    project.assets.insert(
        "tone".to_owned(),
        Asset::Audio {
            name: None,
            source: AssetSource::File {
                path: "tone.wav".to_owned(),
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
    let Some(output) = export_as(&project, directory.path(), VideoFormat::Mp4, "clip.mp4") else {
        return;
    };

    let (samples, sample_rate) = decoded_audio(&output);
    let rate = f64::from(sample_rate);
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
fn every_format_muxes_audio_as_long_as_the_video() {
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    for (format, name) in [
        (VideoFormat::Mp4, "clip.mp4"),
        (VideoFormat::Mov, "clip.mov"),
        (VideoFormat::Webm, "clip.webm"),
    ] {
        let Some(output) = export_as(&project, directory.path(), format, name) else {
            return;
        };
        // AAC rounds the last frame up to 1024 samples (0.128 s at 8 kHz).
        let seconds = decoded_audio_seconds(&output);
        assert!(
            (0.999..1.129).contains(&seconds),
            "{format}: {seconds} s of audio"
        );
    }
}

/// `project` with a one-second clip of `lavfi` audio from 0 s, written to
/// `name` in `dir`.
fn with_audio(mut project: Project, dir: &Path, lavfi: &str, name: &str) -> Project {
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

#[test]
fn exports_an_audio_clip_with_fades() {
    let directory = tempfile::tempdir().unwrap();
    let mut project = with_audio(
        transparent_project(directory.path()),
        directory.path(),
        "sine=frequency=440:sample_rate=8000:duration=1",
        "sound.wav",
    );
    let TimelineContent::Audio {
        fade_in, fade_out, ..
    } = &mut project.tracks.last_mut().unwrap().items[0].content
    else {
        unreachable!();
    };
    *fade_in = Some(Time::new(1, 4));
    *fade_out = Some(Time::new(1, 2));
    let Some(output) = export_as(&project, directory.path(), VideoFormat::Mp4, "clip.mp4") else {
        return;
    };

    let (samples, sample_rate) = decoded_audio(&output);
    let rms = |from: f64, to: f64| {
        let rate = f64::from(sample_rate);
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
        "sine=frequency=440:sample_rate=8000:duration=1",
        "sound.wav",
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
    let Some(output) = export_as(&project, directory.path(), VideoFormat::Mp4, "ducked.mp4") else {
        return;
    };

    let (samples, sample_rate) = decoded_audio(&output);
    let rms = |from: f64, to: f64| {
        let rate = f64::from(sample_rate);
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
    // runs into the next. One second is not a whole number of AAC frames.
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
        "aevalsrc=exprs='0.2*exp(-40*mod(t,0.25))*(cos(2*PI*1000*mod(t,0.25))+sin(2*PI*2500*mod(t,0.25))+sin(2*PI*4400*mod(t,0.25))+cos(2*PI*6000*mod(t,0.25)))':sample_rate=48000:duration=1",
        "hits.wav",
    );
    let export = |looping, name: &str| {
        let output = directory.path().join(name);
        match Exporter::new(ExportOptions {
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

    // The edit list hides the lead-in: the sound still starts at 0 and lasts
    // exactly the video's second.
    let Some(StreamInfo::Audio {
        start_time,
        duration,
        time_base,
        ..
    }) = find_audio_stream_info(looped.to_string_lossy().into_owned()).unwrap()
    else {
        panic!("the looping export has no audio stream");
    };
    assert_eq!(start_time, 0);
    assert_eq!(
        duration * i64::from(time_base.num),
        i64::from(time_base.den),
        "the sound lasts {duration} x {}/{} s",
        time_base.num,
        time_base.den
    );
    let (once, _) = decoded_audio(&once);
    let (looped, sample_rate) = decoded_audio(&looped);
    // FFmpeg's decoder keeps the last AAC frame's padding, with or without
    // the lead-in.
    assert_eq!(looped.len(), once.len(), "the lead-in decodes to nothing");

    // Each hit's first 1024 samples against the source, relative to their
    // energy.
    let hit_error = |decoded: &[f32], start: usize| -> f64 {
        let (mut error, mut energy) = (0.0, 0.0);
        for (n, &sample) in decoded.iter().enumerate().skip(start).take(1024) {
            let expected = hit((n % 12_000) as f64 / f64::from(sample_rate));
            error += (f64::from(sample) - expected).powi(2);
            energy += expected.powi(2);
        }
        error / energy
    };
    let later = [12_000, 24_000, 36_000].map(|start| hit_error(&looped, start));
    let worst_later = later.into_iter().fold(0.0, f64::max);
    let (first_once, first_looped) = (hit_error(&once, 0), hit_error(&looped, 0));
    // From the end of the loop, the first hit comes out like the others;
    // encoded from silence, it smears (about six times the error here).
    assert!(
        first_looped < worst_later,
        "with the lead-in the first hit is off by {first_looped}, the later ones by up to {worst_later}"
    );
    assert!(
        first_once > 3.0 * first_looped,
        "without the lead-in the first hit is off by {first_once}, with it by {first_looped}"
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
        Exporter::new(options).export_project_with_progress(project, dir, &output, |event| {
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
    let project = with_audio(
        project,
        directory.path(),
        "aevalsrc=exprs='0.02*sin(2*PI*440*t)+0.5*sin(2*PI*2000*t)*lt(mod(t,0.25),0.005)':sample_rate=48000:duration=1",
        "tone.wav",
    );
    let options = |format| ExportOptions {
        video_format: format,
        loudness: Some(LoudnessTarget::STREAMING),
        ..ExportOptions::default()
    };
    let Some((first, reported, warnings)) = export_reporting(
        &project,
        directory.path(),
        options(VideoFormat::Mov),
        "first.mov",
    ) else {
        return;
    };
    assert!(warnings.is_empty(), "{warnings:?}");
    let reported = reported.expect("a video with sound reports its loudness");
    let decode = |path: &Path| FfmpegBackend::new().decode_audio(path, 48_000, 2).unwrap();
    let decoded = decode(&first);
    let measured = measure_loudness(&decoded);

    // MOV keeps the mix as 24-bit PCM, so the file measures as reported.
    let integrated = measured.integrated_lufs.unwrap();
    assert!((integrated + 14.0).abs() <= 0.1, "{integrated} LUFS");
    assert!((integrated - reported.integrated_lufs.unwrap()).abs() <= 0.01);
    let peak = measured.true_peak_dbtp.unwrap();
    assert!(peak <= -1.0 + 1e-3, "{peak} dBTP");
    // The gain alone would have taken the hits over the ceiling.
    let source = measure_loudness(&decode(&directory.path().join("tone.wav")));
    let gain = reported.gain_db.unwrap();
    assert!(source.true_peak_dbtp.unwrap() + gain > 0.0, "{gain} dB");

    // The same project comes out the same.
    let (second, ..) = export_reporting(
        &project,
        directory.path(),
        options(VideoFormat::Mov),
        "second.mov",
    )
    .unwrap();
    assert!(decode(&second).samples == decoded.samples);

    // AAC moves the loudness by a few hundredths.
    let (mp4, ..) = export_reporting(
        &project,
        directory.path(),
        options(VideoFormat::Mp4),
        "film.mp4",
    )
    .unwrap();
    let integrated = measure_loudness(&decode(&mp4)).integrated_lufs.unwrap();
    assert!(
        (integrated + 14.0).abs() <= 0.3,
        "{integrated} LUFS after AAC"
    );
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
import { Composition, Rect } from '@celesta/react';
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
    match Exporter::new(ExportOptions {
        video_format: VideoFormat::Webm,
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

    let url = output.to_string_lossy().into_owned();
    assert_eq!(video_stream(&url).0, "vp9");
    assert!(find_audio_stream_info(&url).unwrap().is_none());
    assert!(
        matches!(
            written[..],
            [ExportedFile::Video {
                audio: false,
                loudness: None,
                format: VideoFormat::Webm,
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
import { Composition, Rect, Sequence, Subtitles, parseSubtitles } from '@celesta/react';
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
    match Exporter::new(ExportOptions {
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
import { Composition, Sequence, Subtitles } from '@celesta/react';
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
    Exporter::new(job.options.clone())
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
    let error = Exporter::new(ExportOptions {
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
