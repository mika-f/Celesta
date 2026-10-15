use std::path::{Path, PathBuf};

use celesta_composition::{Rational, Time, TimeRange};
use celesta_exporter::{
    BitDepth, ColorConversion, EncoderPreset, ExportCancellation, ExportError, ExportOptions,
    ExportProgress, ExportRange, ExportedFile, Exporter, ReactRuntimeOptions, VideoEncoding,
    VideoFormat,
};
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::{FfmpegBackend, VideoFrameDecoder};
use celesta_project::{
    Asset, AssetSource, Project, TimelineContent, TimelineItem, Track, TrackKind,
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
            .chunks_exact(4)
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

/// How many seconds of audio `path` decodes to, after the decoder drops the
/// encoder's priming samples.
fn decoded_audio_seconds(path: &Path) -> f64 {
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
    std::fs::metadata(raw).unwrap().len() as f64 / 4.0 / f64::from(sample_rate)
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

/// A React entry with English cues from an SRT file and their French
/// counterparts, both inside a sequence that starts at 0.5 s, in a
/// temporary directory beside the runtime; `None` when the runtime is not
/// built.
fn bilingual_subtitle_entry() -> Option<(tempfile::TempDir, PathBuf, ReactRuntimeOptions)> {
    let runtime_path = workspace_root().join("packages/cli/dist/cli.js");
    if !runtime_path.exists() {
        eprintln!("skipping React subtitle test: build packages/cli first");
        return None;
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
const french = cues.map((cue) => ({ ...cue, text: `fr: ${cue.text}` }));
export default function Root() {
  return <Composition width={64} height={64} fps={10} durationInFrames={40} lang="ja-JP">
    <Rect width={32} height={32} fill="#ff0000" />
    <Sequence from={5}>
      <Subtitles lang="en" cues={cues} />
      <Subtitles lang="fr" cues={french} />
    </Sequence>
  </Composition>;
}
"##,
    )
    .unwrap();
    Some((
        directory,
        entry,
        ReactRuntimeOptions::new("node", runtime_path),
    ))
}

/// The sequence shifts the cues by 0.5 s; a 2 s to 3.5 s range starts the
/// files at 2 s and ends them at 3.5 s, so "Before" is never shown and the
/// cues on either edge are cut.
const ENGLISH_VTT: &str = "WEBVTT\n\n\
    00:00:00.000 --> 00:00:00.500\nAcross start\n\n\
    00:00:00.500 --> 00:00:01.000\nInside\n\n\
    00:00:01.000 --> 00:00:01.500\nAcross end\n";
const FRENCH_SRT: &str = "1\n00:00:00,000 --> 00:00:00,500\nfr: Across start\n\n\
    2\n00:00:00,500 --> 00:00:01,000\nfr: Inside\n\n\
    3\n00:00:01,000 --> 00:00:01,500\nfr: Across end\n";

fn subtitle_range() -> Option<ExportRange> {
    Some(ExportRange::new(Time::new(2, 1), Time::new(7, 2)))
}

/// The subtitle files `written` reports, as (file name, format, language,
/// cues).
fn written_subtitles(written: &[ExportedFile]) -> Vec<(String, &'static str, Option<&str>, usize)> {
    written
        .iter()
        .filter_map(|file| match file {
            ExportedFile::Subtitles {
                path,
                format,
                lang,
                cues,
            } => Some((
                path.file_name().unwrap().to_string_lossy().into_owned(),
                format.as_str(),
                lang.as_deref(),
                *cues,
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn writes_a_react_entrys_subtitles_per_language_and_format_shifted_to_the_exported_range() {
    let Some((directory, entry, runtime)) = bilingual_subtitle_entry() else {
        return;
    };
    let output = directory.path().join("film.mp4");
    let mut written = Vec::new();
    match Exporter::new(ExportOptions {
        range: subtitle_range(),
        subtitles: vec![
            directory.path().join("film.{lang}.vtt"),
            directory.path().join("film.{lang}.srt"),
        ],
        ..ExportOptions::default()
    })
    .export_react_entry_with_progress(&entry, &runtime, &output, |event| {
        if let ExportProgress::Wrote(file) = event {
            written.push(file);
        }
    }) {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            eprintln!("skipping live export test: no GPU adapter is available: {error}");
            return;
        }
        Err(error) => panic!("React subtitle export failed: {error}"),
    }

    let read = |name: &str| std::fs::read_to_string(directory.path().join(name)).unwrap();
    assert_eq!(read("film.en.vtt"), ENGLISH_VTT);
    assert_eq!(read("film.fr.srt"), FRENCH_SRT);
    assert!(read("film.fr.vtt").contains("fr: Inside"));
    assert!(read("film.en.srt").contains("\nInside\n"));
    assert!(
        matches!(&written[0], ExportedFile::Video { frames: 15, .. }),
        "{written:?}"
    );
    assert_eq!(
        written_subtitles(&written),
        [
            ("film.en.vtt".to_owned(), "vtt", Some("en"), 3),
            ("film.fr.vtt".to_owned(), "vtt", Some("fr"), 3),
            ("film.en.srt".to_owned(), "srt", Some("en"), 3),
            ("film.fr.srt".to_owned(), "srt", Some("fr"), 3),
        ]
    );
}

#[test]
fn writes_a_react_entrys_subtitles_without_rendering_it() {
    let Some((directory, entry, runtime)) = bilingual_subtitle_entry() else {
        return;
    };
    let mut written = Vec::new();
    let exporter = |subtitles: Vec<PathBuf>| {
        Exporter::new(ExportOptions {
            range: subtitle_range(),
            subtitles,
            ..ExportOptions::default()
        })
    };
    exporter(vec![
        directory.path().join("film.{lang}.vtt"),
        directory.path().join("film.{lang}.srt"),
    ])
    .export_react_subtitles(
        &entry,
        &runtime,
        None,
        &ExportCancellation::default(),
        |event| {
            if let ExportProgress::Wrote(file) = event {
                written.push(file);
            }
        },
    )
    .unwrap();

    // The same files a video export of the span writes, and nothing else.
    let read = |name: &str| std::fs::read_to_string(directory.path().join(name)).unwrap();
    assert_eq!(read("film.en.vtt"), ENGLISH_VTT);
    assert_eq!(read("film.fr.srt"), FRENCH_SRT);
    assert_eq!(written_subtitles(&written).len(), written.len());
    assert_eq!(written.len(), 4);
    assert!(!directory.path().join("film.mp4").exists());

    // One file cannot hold both languages.
    let error = exporter(vec![directory.path().join("both.srt")])
        .export_react_subtitles(
            &entry,
            &runtime,
            None,
            &ExportCancellation::default(),
            |_| {},
        )
        .unwrap_err();
    assert!(
        matches!(&error, ExportError::SubtitleLanguages { languages, .. } if languages == &["en", "fr"]),
        "{error}"
    );
    assert!(!directory.path().join("both.srt").exists());
}

#[test]
fn writes_a_projects_dialogue_as_subtitles_without_rendering_it() {
    let directory = tempfile::tempdir().unwrap();
    let project = Project::from_json(
        r#"{
            "version": 0,
            "settings": {"width": 64, "height": 64, "frameRate": {"numerator": 10, "denominator": 1}, "sampleRate": 48000},
            "assets": {},
            "characters": {"akane": {"name": "Akane"}},
            "tracks": [{"id": "a", "name": "A", "kind": "dialogue", "items": [
                {"id": "one", "range": {"start": {"value": 0, "timescale": 1}, "duration": {"value": 1, "timescale": 1}},
                 "content": {"type": "dialogue", "character": "akane", "text": "First"}},
                {"id": "two", "range": {"start": {"value": 2, "timescale": 1}, "duration": {"value": 1, "timescale": 1}},
                 "content": {"type": "dialogue", "character": "akane", "text": "Second"}}
            ]}],
            "properties": {}
        }"#,
    )
    .unwrap();
    let srt = directory.path().join("dialogue.srt");
    let vtt = directory.path().join("dialogue.vtt");
    let mut written = Vec::new();
    Exporter::new(ExportOptions {
        subtitles: vec![srt.clone(), vtt.clone()],
        ..ExportOptions::default()
    })
    .export_project_subtitles(&project, |event| {
        if let ExportProgress::Wrote(file) = event {
            written.push(file);
        }
    })
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(&srt).unwrap(),
        "1\n00:00:00,000 --> 00:00:01,000\nFirst\n\n\
         2\n00:00:02,000 --> 00:00:03,000\nSecond\n"
    );
    assert!(
        std::fs::read_to_string(&vtt)
            .unwrap()
            .starts_with("WEBVTT\n\n")
    );
    assert_eq!(
        written_subtitles(&written),
        [
            ("dialogue.srt".to_owned(), "srt", None, 2),
            ("dialogue.vtt".to_owned(), "vtt", None, 2),
        ]
    );

    // A project's dialogue has no language for `{lang}` to name.
    let error = Exporter::new(ExportOptions {
        subtitles: vec![directory.path().join("dialogue.{lang}.srt")],
        overwrite: true,
        ..ExportOptions::default()
    })
    .export_project_subtitles(&project, |_| {})
    .unwrap_err();
    assert!(
        matches!(&error, ExportError::SubtitleLanguages { languages, .. } if languages.is_empty()),
        "{error}"
    );
}
