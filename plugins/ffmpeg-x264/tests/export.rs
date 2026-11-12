//! The x264 plugin checked through the real exporter, with both FFmpeg
//! programs registered as Celesta finds them: what an MP4 export holds
//! (libx264's H.264, AAC, colors, its edit list, its loudness), how libx264
//! converts and pads, and what the decoder reads back. The standard plugin's
//! MOV and WebM exports are tested in `plugins/ffmpeg`, against the standard
//! build.

#[path = "../../ffmpeg/tests/support/exporting.rs"]
mod exporting;
#[path = "../../ffmpeg/tests/support/files.rs"]
mod files;
#[path = "../../ffmpeg/tests/support/program.rs"]
mod program;

use celesta_codec_ffmpeg::{FfmpegBackend, FfmpegCodec};
use celesta_composition::{Rational, Time, TimeRange};
use celesta_exporter::{
    AudioChoice, EncoderChoice, ExportCancellation, ExportError, ExportOptions, Exporter,
    LoudnessTarget, OutputChoice,
};
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::{
    AudioDecoder, AudioEncoder, ComponentKind, Decoder, Encoded, MediaError, MediaProvider,
    MediaRegistry, Muxer, PixelFormat, RegisteredPlugin, SessionId, VideoEncoder,
    VideoEncoderOpened, VideoEncoderRequest, VideoFrameDecoder, measure_loudness,
};
use celesta_project::{
    Asset, AssetSource, Project, TimelineContent, TimelineItem, Track, TrackKind,
};
use exporting::{
    component, export_as, export_reporting, export_with, exporter, gpu, hits_project,
    minimal_project, mix_frames, options, skip_without_gpu, transparent_project, with_audio,
};
use ez_ffmpeg::stream_info::{StreamInfo, find_audio_stream_info, find_video_stream_info};
use ez_ffmpeg::{FfmpegContext, Input, Output};
use files::{
    audio_stream, colr_atom, decoded_audio, decoded_audio_seconds, generate_source, stream_color,
};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Both FFmpeg programs, each linked against its own build and started as
/// Celesta starts a media plugin.
fn plugins() -> Vec<RegisteredPlugin> {
    [
        celesta_codec_ffmpeg::registered(),
        celesta_codec_ffmpeg_x264::registered(),
    ]
    .into_iter()
    .map(|mut plugin| {
        plugin.factory = program::factory(&plugin.id);
        plugin
    })
    .collect()
}

/// Registers [`plugins`] for every export.
fn install() {
    celesta_media::set_registry(Some(Arc::new(MediaRegistry::new(plugins()))));
}

#[test]
fn the_x264_codec_has_its_format_encoder() {
    let described = FfmpegCodec::x264().describe().unwrap();
    let libx264 = described
        .iter()
        .find(|component| {
            component.kind == ComponentKind::VideoEncoder && component.id == "libx264"
        })
        .unwrap_or_else(|| panic!("libx264 is described"));
    assert!(libx264.available, "libx264: {:?}", libx264.reason);
}

#[test]
fn exports_frame_exact_mp4_with_silent_audio() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("export.mp4");
    let source = directory.path().join("source.mkv");
    generate_source(&source, "testsrc2=size=64x64:rate=2:duration=1", (2, 1));

    let mut project = minimal_project();
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

    let exporter = exporter(ExportOptions {
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
    assert_eq!(audio_stream(&url).0, "aac");

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

/// What a video encoder was offered and what it chose.
type Opened = Arc<Mutex<Vec<(Vec<PixelFormat>, PixelFormat)>>>;

/// The FFmpeg plugin, noting in `opened` what each video encoder was offered
/// and chose. With `rgba_only`, it sees what a renderer that reads back no
/// YUV offers: only the RGBA formats, so libx264 converts them itself.
struct Watched {
    codec: FfmpegCodec,
    rgba_only: bool,
    opened: Opened,
}

impl MediaProvider for Watched {
    fn describe(&mut self) -> Result<Vec<celesta_media::ComponentDescription>, MediaError> {
        self.codec.describe()
    }

    fn decoder(&mut self) -> Option<&mut dyn Decoder> {
        self.codec.decoder()
    }

    fn video_encoder(&mut self) -> Option<&mut dyn VideoEncoder> {
        Some(self)
    }

    fn audio_encoder(&mut self) -> Option<&mut dyn AudioEncoder> {
        self.codec.audio_encoder()
    }

    fn muxer(&mut self) -> Option<&mut dyn Muxer> {
        self.codec.muxer()
    }
}

impl VideoEncoder for Watched {
    fn open_video(
        &mut self,
        request: &VideoEncoderRequest<'_>,
    ) -> Result<VideoEncoderOpened, MediaError> {
        let offered: Vec<PixelFormat> = request
            .offered
            .iter()
            .copied()
            .filter(|format| {
                !self.rgba_only || matches!(format, PixelFormat::Rgba8 | PixelFormat::Rgba16)
            })
            .collect();
        let opened = self.codec.open_video(&VideoEncoderRequest {
            offered: &offered,
            ..*request
        })?;
        self.opened
            .lock()
            .unwrap()
            .push((request.offered.to_vec(), opened.input));
        Ok(opened)
    }

    fn encode_video(
        &mut self,
        session: SessionId,
        pts: i64,
        frame: Vec<u8>,
    ) -> Result<Encoded, MediaError> {
        self.codec.encode_video(session, pts, frame)
    }

    fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        self.codec.finish_video(session)
    }

    fn abort_video(&mut self, session: SessionId) {
        self.codec.abort_video(session);
    }
}

/// Makes a [`Watched`] FFmpeg plugin the media plugin of every export.
fn watch(rgba_only: bool, opened: &Opened) {
    let opened = opened.clone();
    let mut plugins = plugins();
    plugins.retain(|plugin| plugin.id != "ffmpeg-x264");
    plugins.push(RegisteredPlugin {
        factory: Arc::new(move |_, _| {
            Ok(Box::new(Watched {
                codec: FfmpegCodec::x264(),
                rgba_only,
                opened: opened.clone(),
            }) as Box<_>)
        }),
        ..celesta_codec_ffmpeg_x264::registered()
    });
    celesta_media::set_registry(Some(Arc::new(MediaRegistry::new(plugins))));
}

/// Runs `export` with a [`Watched`] FFmpeg plugin, then restores the plain
/// one: its result, with what libx264 was offered and the input it chose.
fn watching<T>(
    rgba_only: bool,
    export: impl FnOnce() -> Result<T, ExportError>,
) -> Result<(T, (Vec<PixelFormat>, PixelFormat)), ExportError> {
    let opened = Opened::default();
    watch(rgba_only, &opened);
    let result = export();
    install();
    let mut opened = opened.lock().unwrap();
    assert!(opened.len() <= 1, "{opened:?}");
    result.map(|value| (value, opened.pop().expect("libx264 was opened")))
}

#[test]
fn converting_colors_on_the_gpu_matches_the_encoder_conversion() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.mkv");
    generate_source(&source, "testsrc2=size=64x64:rate=2:duration=1", (2, 1));

    let mut project = minimal_project();
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
    let export = |bit_depth: &str, name: &str| {
        let output = directory.path().join(name);
        Exporter::new(ExportOptions {
            output: OutputChoice {
                video: EncoderChoice {
                    component: Some(component("ffmpeg-x264/libx264")),
                    options: options(&[
                        ("preset", "ultrafast"),
                        ("crf", "0"),
                        ("bitDepth", bit_depth),
                    ]),
                },
                ..OutputChoice::default()
            },
            ..ExportOptions::default()
        })
        .export_project(&project, directory.path(), &output)
        .map(|()| output)
    };
    // The GPU's conversion when the renderer offers YUV; libswscale's of
    // RGBA when it is not offered. Each export answers the input libx264
    // chose and what it was offered.
    let watched = |rgba_only: bool, bit_depth: &str, name: &str| {
        watching(rgba_only, || export(bit_depth, name))
    };
    let mut pixel_formats: Vec<i32> = Vec::new();
    for (bit_depth, yuv, rgba) in [
        ("8", PixelFormat::Yuv420p, PixelFormat::Rgba8),
        ("10", PixelFormat::Yuv420p10, PixelFormat::Rgba16),
    ] {
        let (on_gpu, (offered, input)) =
            match watched(false, bit_depth, &format!("gpu-{bit_depth}.mp4")) {
                Ok(exported) => exported,
                Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
                    skip_without_gpu(&error);
                    return;
                }
                Err(error) => panic!("GPU conversion export failed: {error}"),
            };
        if !offered.contains(&yuv) {
            // A software renderer, such as CI's lavapipe, reads back no YUV
            // (deviation 18): there is no GPU conversion to compare.
            eprintln!(
                "skipping GPU color conversion test: the renderer offers no YUV ({offered:?})"
            );
            return;
        }
        assert_eq!(input, yuv, "{bit_depth}-bit on the GPU");
        let (in_encoder, (_, input)) =
            watched(true, bit_depth, &format!("encoder-{bit_depth}.mp4")).unwrap();
        assert_eq!(input, rgba, "{bit_depth}-bit in the encoder");

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

/// A 64x64, 2 fps, one-second project whose only item is the video clip
/// `source.mkv` in `dir`.
fn one_clip_project(_dir: &Path) -> Project {
    let mut project = minimal_project();
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
    project
}

/// Every frame's Y, U, and V samples of `path`, decoded to rawvideo in
/// `pix_fmt` (`yuv420p` or `yuv420p10le`).
fn decoded_planes(path: &Path, pix_fmt: &str) -> Vec<Vec<u16>> {
    let raw = path.with_extension("yuv");
    FfmpegContext::builder()
        .input(Input::from(path.to_string_lossy().into_owned()))
        .output(
            Output::from(raw.to_string_lossy().into_owned())
                .add_stream_map("0:v:0")
                .set_format("rawvideo")
                .set_pix_fmt(pix_fmt),
        )
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
    let bytes = std::fs::read(raw).unwrap();
    let samples: Vec<u16> = if pix_fmt == "yuv420p" {
        bytes.iter().map(|&byte| u16::from(byte)).collect()
    } else {
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&sample| u16::from_le_bytes(sample))
            .collect()
    };
    samples
        .as_chunks::<{ 64 * 64 * 3 / 2 }>()
        .0
        .iter()
        .map(|frame| frame.to_vec())
        .collect()
}

#[test]
fn an_mp4_muxes_audio_as_long_as_the_video() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let Some(output) = export_as(&project, directory.path(), "clip.mp4") else {
        return;
    };
    // AAC rounds the last frame up to 1024 samples (0.128 s at 8 kHz).
    let seconds = decoded_audio_seconds(&output);
    assert!((0.999..1.129).contains(&seconds), "{seconds} s of audio");
}

#[test]
fn a_looping_mp4_starts_its_sound_from_its_end() {
    let _gpu = gpu();
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
        match exporter(ExportOptions {
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

#[test]
fn an_mp4_comes_near_the_loudness_target() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let mut project = transparent_project(directory.path());
    project.settings.sample_rate = 48_000;
    // The tone and hits the standard plugin's MOV is brought to -14 LUFS.
    let project = with_audio(
        project,
        directory.path(),
        "aevalsrc=exprs='0.02*sin(2*PI*440*t)+0.5*sin(2*PI*2000*t)*lt(mod(t,0.25),0.005)':sample_rate=48000:duration=1",
        "tone.wav",
    );
    let options = ExportOptions {
        loudness: Some(LoudnessTarget::STREAMING),
        ..ExportOptions::default()
    };
    let Some((mp4, reported, warnings)) =
        export_reporting(&project, directory.path(), options, "film.mp4")
    else {
        return;
    };
    assert!(warnings.is_empty(), "{warnings:?}");
    reported.expect("a video with sound reports its loudness");
    // AAC moves the loudness by a few hundredths.
    let decoded = FfmpegBackend::new().decode_audio(&mp4, 48_000, 2).unwrap();
    let integrated = measure_loudness(&decoded).integrated_lufs.unwrap();
    assert!(
        (integrated + 14.0).abs() <= 0.3,
        "{integrated} LUFS after AAC"
    );
}

#[test]
fn an_mp4_is_tagged_bt709() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let Some(output) = export_as(&project, directory.path(), "clip.mp4") else {
        return;
    };
    // Primaries, transfer, matrix (all AVCOL_*_BT709, 1), and range
    // (AVCOL_RANGE_MPEG, 1), as libavformat reads them back.
    assert_eq!(stream_color(&output), (1, 1, 1, 1));
    assert_eq!(colr_atom(&output), Some(*b"nclx"));
}

/// The decoded `yuv420p` (or 10-bit) code values of a flat color through
/// the GPU's conversion and a lossless encode. Every export is tagged
/// BT.709; the code values are checked only where the GPU converted.
#[test]
fn mp4_colors_hold_bt709_code_values() {
    let _gpu = gpu();
    for (hex, eight, ten) in [
        ("ff0000", (63, 102, 240), (250, 409, 960)),
        ("00ff00", (173, 42, 26), (691, 167, 105)),
        ("0000ff", (32, 240, 118), (127, 960, 471)),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.mkv");
        generate_source(
            &source,
            &format!("color=c=#{hex}:size=64x64:rate=2:duration=0.5,format=rgba"),
            (2, 1),
        );
        let project = one_clip_project(directory.path());
        for (bit_depth, pix_fmt, yuv, expected) in [
            (
                "8",
                "yuv420p",
                PixelFormat::Yuv420p,
                [eight.0, eight.1, eight.2],
            ),
            (
                "10",
                "yuv420p10le",
                PixelFormat::Yuv420p10,
                [ten.0, ten.1, ten.2],
            ),
        ] {
            let output = directory.path().join(format!("{hex}-{bit_depth}.mp4"));
            let exported = watching(false, || {
                Exporter::new(ExportOptions {
                    output: OutputChoice {
                        video: EncoderChoice {
                            component: Some(component("ffmpeg-x264/libx264")),
                            options: options(&[
                                ("preset", "ultrafast"),
                                ("crf", "0"),
                                ("bitDepth", bit_depth),
                            ]),
                        },
                        ..OutputChoice::default()
                    },
                    ..ExportOptions::default()
                })
                .export_project(&project, directory.path(), &output)
            });
            let offered = match exported {
                Ok(((), (offered, _))) => offered,
                Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
                    skip_without_gpu(&error);
                    return;
                }
                Err(error) => panic!("{hex} {bit_depth}-bit export failed: {error}"),
            };
            assert_eq!(stream_color(&output), (1, 1, 1, 1), "{hex} {bit_depth}");
            if !offered.contains(&yuv) {
                // A software renderer, such as CI's lavapipe, reads back no
                // YUV (deviation 18), so libx264 converts RGBA with
                // libswscale: its 10-bit codes differ from the formula by up
                // to 2, and its bicubic chroma bleeds the background into
                // the quadrant's last chroma row.
                eprintln!(
                    "skipping BT.709 code value check of {hex} {bit_depth}-bit: \
                     the renderer offers no YUV ({offered:?})"
                );
                continue;
            }
            let frames = decoded_planes(&output, pix_fmt);
            assert!(!frames.is_empty(), "{hex} {bit_depth}: no frames");
            // Video clips are centered on the canvas origin, as images are:
            // the source covers the top-left 32x32 quadrant (16x16 of
            // chroma) and the rest is the background.
            for planes in frames {
                let (luma, rest) = planes.split_at(64 * 64);
                let (blue, red) = rest.split_at(32 * 32);
                for (plane, width, want) in [
                    (luma, 64, expected[0]),
                    (blue, 32, expected[1]),
                    (red, 32, expected[2]),
                ] {
                    for y in 0..width / 2 {
                        for &code in &plane[y * width..y * width + width / 2] {
                            assert!(
                                (i32::from(code) - want).abs() <= 1,
                                "{hex} {bit_depth}-bit: code {code}, expected {want}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn an_mp4_starts_with_an_edit_list_hiding_the_delay() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = hits_project(directory.path());
    let mix = mix_frames(directory.path(), "hits.wav", 48_000);
    let aac = || ExportOptions {
        output: OutputChoice {
            audio: AudioChoice::Encode(EncoderChoice {
                component: Some(component("ffmpeg/aac")),
                options: Default::default(),
            }),
            ..OutputChoice::default()
        },
        ..ExportOptions::default()
    };
    let name = "clip.mp4";
    let Some(output) = export_with(&project, directory.path(), name, aac()) else {
        return;
    };
    let Some(StreamInfo::Audio {
        codec_name,
        start_time,
        ..
    }) = find_audio_stream_info(output.to_string_lossy().into_owned()).unwrap()
    else {
        panic!("{name} has no audio stream");
    };
    assert_eq!(codec_name, "aac", "{name}");
    assert_eq!(start_time, 0, "{name}");
    let decoded = FfmpegBackend::new()
        .decode_audio(&output, 48_000, 2)
        .unwrap();
    assert_eq!(decoded.frame_count(), mix.frame_count(), "{name}");
}
