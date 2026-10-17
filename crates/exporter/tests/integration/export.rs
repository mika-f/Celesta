use std::path::{Path, PathBuf};

use celesta_composition::{Rational, Time, TimeRange};
use celesta_exporter::{
    BitDepth, ColorConversion, EncoderPreset, ExportCancellation, ExportError, ExportJob,
    ExportOptions, ExportProgress, ExportRange, ExportedFile, Exporter, ReactRuntimeOptions,
    VideoEncoding, VideoFormat,
};
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::{FfmpegBackend, VideoFrameDecoder};
use celesta_project::{
    Animatable, Asset, AssetSource, Project, TimelineContent, TimelineItem, Track, TrackKind,
};
use ez_ffmpeg::stream_info::{StreamInfo, find_audio_stream_info, find_video_stream_info};
use ez_ffmpeg::{FfmpegContext, Input, Output};

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
            eprintln!("skipping live export test: no GPU adapter is available: {error}");
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
            eprintln!("skipping live export test: no GPU adapter is available: {error}");
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
    for bit_depth in BitDepth::ALL {
        let on_gpu = match export(
            ColorConversion::Gpu,
            bit_depth,
            &format!("gpu-{bit_depth}.mp4"),
        ) {
            Ok(output) => output,
            Err(ExportError::Render(
                GpuRenderError::RequestAdapter(_) | GpuRenderError::UnsupportedReadbackFormat(_),
            )) => {
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
            eprintln!("skipping live export test: no GPU adapter is available: {error}");
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
    let mut written = Vec::new();
    match Exporter::new(ExportOptions {
        video_format: VideoFormat::Webm,
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
            eprintln!("skipping live export test: no GPU adapter is available: {error}");
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
                format: VideoFormat::Webm,
                ..
            }]
        ),
        "{written:?}"
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
            eprintln!("skipping live export test: no GPU adapter is available: {error}");
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
