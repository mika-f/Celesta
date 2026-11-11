//! The FFmpeg codec checked through the real exporter: what it writes for
//! each format (codecs, pixel formats, colors, alpha, audio streams), how its
//! encoders convert and pad, and what its decoder reads back. The exporter's
//! own tests run on the test codec; these hold the contract with FFmpeg.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

mod support {
    include!("support/mod.rs");
    pub mod registry;
}
use celesta_codec_ffmpeg::{FfmpegBackend, FfmpegCodec};
use celesta_composition::{Rational, Time, TimeRange};
use celesta_exporter::{
    AudioChoice, AudioLoudness, EncoderChoice, ExportCancellation, ExportError, ExportOptions,
    ExportProgress, ExportedFile, Exporter, LoudnessTarget, OutputChoice, SettingInput,
};
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::{
    AudioDecoder, AudioEncoder, ComponentKind, ComponentRef, Decoder, Encoded, MediaError,
    MediaProvider, MediaRegistry, Muxer, PixelFormat, RegisteredPlugin, SessionId, VideoEncoder,
    VideoEncoderOpened, VideoEncoderRequest, VideoFrameDecoder, measure_loudness,
};
use celesta_project::{
    Asset, AssetSource, Project, TimelineContent, TimelineItem, Track, TrackKind,
};
use ez_ffmpeg::stream_info::{StreamInfo, find_audio_stream_info, find_video_stream_info};
use ez_ffmpeg::{FfmpegContext, Input, Output};

/// GPU devices made at once in one process are unsafe on some drivers:
/// Mesa's lavapipe, which Linux CI renders with, crashes the test binary
/// with SIGSEGV. Every test that exports holds this for its whole body, so
/// one exporter renders at a time.
static GPU: Mutex<()> = Mutex::new(());

/// The GPU lock; a test that panicked while holding it does not fail the
/// rest.
fn gpu() -> MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(PoisonError::into_inner)
}

fn install() {
    celesta_media::set_registry(Some(Arc::new(MediaRegistry::new(
        support::registry::plugins(),
    ))));
}

/// An exporter using independently linked sibling codec processes.
fn exporter(options: ExportOptions) -> Exporter {
    install();
    Exporter::new(options)
}

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

/// An empty 1920x1080 project, a copy of `examples/minimal.celesta.json`
/// shared under the standard plugin, whose inputs trigger every profile
/// that depends on it.
fn minimal_project() -> Project {
    Project::load(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../ffmpeg/tests/fixtures/minimal.celesta.json"),
    )
    .unwrap()
}

#[test]
fn the_ffmpeg_codec_has_every_format_encoder() {
    let mut described = FfmpegCodec::new().describe().unwrap();
    described.extend(FfmpegCodec::x264().describe().unwrap());
    described.extend(support::process("ffmpeg-opus").describe().unwrap());
    described.extend(support::process("ffmpeg-vpx").describe().unwrap());
    for (kind, id) in [
        (ComponentKind::VideoEncoder, "libx264"),
        (ComponentKind::VideoEncoder, "prores"),
        (ComponentKind::VideoEncoder, "vp9"),
        (ComponentKind::AudioEncoder, "aac"),
        (ComponentKind::AudioEncoder, "opus"),
        (ComponentKind::AudioEncoder, "pcm"),
        (ComponentKind::Muxer, "mp4"),
        (ComponentKind::Muxer, "mov"),
        (ComponentKind::Muxer, "webm"),
    ] {
        let component = described
            .iter()
            .find(|component| component.kind == kind && component.id == id)
            .unwrap_or_else(|| panic!("{id} is described"));
        assert!(component.available, "{id}: {:?}", component.reason);
    }
}

/// `plugin/component` as a reference.
fn component(text: &str) -> ComponentRef {
    text.parse().unwrap()
}

/// Options given as command-line text.
fn options(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, SettingInput> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), SettingInput::Text((*value).to_owned())))
        .collect()
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
    let mut plugins = support::registry::plugins();
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

/// A 64x64, 2 fps, one-second project whose only item is a 64x64 image of
/// straight `(255, 0, 0, 128)`. Images are centered on the canvas origin, so
/// it covers the top-left 32x32 quadrant and leaves the rest undrawn.
fn transparent_project(dir: &Path) -> Project {
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
fn export_as(project: &Project, dir: &Path, name: &str) -> Option<PathBuf> {
    export_with(project, dir, name, ExportOptions::default())
}

/// [`export_as`] with `options`.
fn export_with(
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

/// The video stream's `(color_primaries, color_trc, color_space,
/// color_range)` as libavformat reads them from the container.
fn stream_color(path: &Path) -> (i32, i32, i32, i32) {
    use ffmpeg_sys_next as ff;
    let url = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
    // SAFETY: the context is opened, read, and closed here; the stream
    // pointers are valid while it is open.
    unsafe {
        let mut context = std::ptr::null_mut();
        assert!(
            ff::avformat_open_input(
                &mut context,
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut()
            ) >= 0
        );
        assert!(ff::avformat_find_stream_info(context, std::ptr::null_mut()) >= 0);
        let streams =
            std::slice::from_raw_parts((*context).streams, (*context).nb_streams as usize);
        let parameters = streams
            .iter()
            .map(|&stream| (*stream).codecpar)
            .find(|&parameters| (*parameters).codec_type == ff::AVMediaType::AVMEDIA_TYPE_VIDEO)
            .expect("a video stream");
        let colors = (
            (*parameters).color_primaries as i32,
            (*parameters).color_trc as i32,
            (*parameters).color_space as i32,
            (*parameters).color_range as i32,
        );
        ff::avformat_close_input(&mut context);
        colors
    }
}

/// The four bytes after the first `colr` in `path` (an ISO BMFF colour
/// type: `nclx` in MP4, `nclc` in QuickTime).
fn colr_atom(path: &Path) -> Option<[u8; 4]> {
    let bytes = std::fs::read(path).unwrap();
    let at = bytes.windows(4).position(|window| window == b"colr")?;
    bytes.get(at + 4..at + 8)?.try_into().ok()
}

/// Whether `path` has a Matroska `Colour` element (`55 B0`) before its first
/// Cluster (`1F 43 B6 75`).
fn webm_has_colour(path: &Path) -> bool {
    let bytes = std::fs::read(path).unwrap();
    let cluster = bytes
        .windows(4)
        .position(|window| window == [0x1F, 0x43, 0xB6, 0x75])
        .expect("a cluster");
    bytes[..cluster]
        .windows(2)
        .any(|window| window == [0x55, 0xB0])
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

/// The data of `path`'s first video packet.
fn first_video_packet(path: &Path) -> Vec<u8> {
    use ffmpeg_sys_next as ff;
    let url = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
    // SAFETY: the context and packet are opened, read, and freed here.
    unsafe {
        let mut context = std::ptr::null_mut();
        assert!(
            ff::avformat_open_input(
                &mut context,
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut()
            ) >= 0
        );
        assert!(ff::avformat_find_stream_info(context, std::ptr::null_mut()) >= 0);
        let streams =
            std::slice::from_raw_parts((*context).streams, (*context).nb_streams as usize);
        let video = streams
            .iter()
            .position(|&stream| {
                (*(*stream).codecpar).codec_type == ff::AVMediaType::AVMEDIA_TYPE_VIDEO
            })
            .expect("a video stream") as i32;
        let mut packet = ff::av_packet_alloc();
        let data = loop {
            assert!(ff::av_read_frame(context, packet) >= 0, "no video packet");
            if (*packet).stream_index == video {
                break std::slice::from_raw_parts((*packet).data, (*packet).size as usize).to_vec();
            }
            ff::av_packet_unref(packet);
        };
        ff::av_packet_free(&mut packet);
        ff::avformat_close_input(&mut context);
        data
    }
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
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let Some(output) = export_as(&project, directory.path(), "clip.mov") else {
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
fn every_format_muxes_audio_as_long_as_the_video() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    for name in ["clip.mp4", "clip.mov", "clip.webm"] {
        let Some(output) = export_as(&project, directory.path(), name) else {
            return;
        };
        // AAC rounds the last frame up to 1024 samples (0.128 s at 8 kHz).
        let seconds = decoded_audio_seconds(&output);
        assert!(
            (0.999..1.129).contains(&seconds),
            "{name}: {seconds} s of audio"
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

#[test]
fn brings_the_mix_to_a_loudness_target() {
    let _gpu = gpu();
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
    let options = || ExportOptions {
        loudness: Some(LoudnessTarget::STREAMING),
        ..ExportOptions::default()
    };
    let Some((first, reported, warnings)) =
        export_reporting(&project, directory.path(), options(), "first.mov")
    else {
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
    let (second, ..) =
        export_reporting(&project, directory.path(), options(), "second.mov").unwrap();
    assert!(decode(&second).samples == decoded.samples);

    // AAC moves the loudness by a few hundredths.
    let (mp4, ..) = export_reporting(&project, directory.path(), options(), "film.mp4").unwrap();
    let integrated = measure_loudness(&decode(&mp4)).integrated_lufs.unwrap();
    assert!(
        (integrated + 14.0).abs() <= 0.3,
        "{integrated} LUFS after AAC"
    );
}

#[test]
fn mp4_mov_and_webm_are_tagged_bt709() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    // Primaries, transfer, matrix (all AVCOL_*_BT709, 1), and range
    // (AVCOL_RANGE_MPEG, 1), as libavformat reads them back.
    for name in ["clip.mp4", "clip.mov", "clip.webm"] {
        let Some(output) = export_as(&project, directory.path(), name) else {
            return;
        };
        assert_eq!(stream_color(&output), (1, 1, 1, 1), "{name}");
        match name {
            "clip.mp4" => assert_eq!(colr_atom(&output), Some(*b"nclx")),
            "clip.mov" => assert_eq!(colr_atom(&output), Some(*b"nclc")),
            _ => assert!(webm_has_colour(&output)),
        }
    }
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
fn vp9_frames_carry_bt709_in_their_header() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let Some(output) = export_as(&project, directory.path(), "clip.webm") else {
        return;
    };
    // The keyframe header's color_space, 2 for BT.709, so a player that
    // ignores the container's Colour still decodes BT.709.
    assert_eq!(first_video_packet(&output)[4] >> 5, 2);
}

/// The hits of `a_looping_mp4_starts_its_sound_from_its_end`, at 48 kHz.
fn hits_project(dir: &Path) -> Project {
    let mut project = transparent_project(dir);
    project.settings.sample_rate = 48_000;
    with_audio(
        project,
        dir,
        "aevalsrc=exprs='0.2*exp(-40*mod(t,0.25))*(cos(2*PI*1000*mod(t,0.25))+sin(2*PI*2500*mod(t,0.25))+sin(2*PI*4400*mod(t,0.25))+cos(2*PI*6000*mod(t,0.25)))':sample_rate=48000:duration=1",
        "hits.wav",
    )
}

#[test]
fn a_looping_webm_is_written_as_without_loop() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = hits_project(directory.path());
    let export = |looping, name| {
        export_with(
            &project,
            directory.path(),
            name,
            ExportOptions {
                looping,
                ..ExportOptions::default()
            },
        )
    };
    let (Some(once), Some(looped)) = (export(false, "once.webm"), export(true, "looped.webm"))
    else {
        return;
    };
    let decode = |path: &Path| FfmpegBackend::new().decode_audio(path, 48_000, 2).unwrap();
    assert!(decode(&once).samples == decode(&looped).samples);
}

#[test]
fn a_looping_mov_decodes_as_without_loop() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = hits_project(directory.path());
    let export = |looping, name| {
        export_with(
            &project,
            directory.path(),
            name,
            ExportOptions {
                looping,
                ..ExportOptions::default()
            },
        )
    };
    let (Some(once), Some(looped)) = (export(false, "once.mov"), export(true, "looped.mov")) else {
        return;
    };
    assert_eq!(
        audio_stream(&looped.to_string_lossy()),
        ("pcm_s24le".to_owned(), 48_000)
    );
    // The edit list hides the PCM lead-in: the sound still starts at 0 and
    // lasts exactly the video's second.
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
    // The looping file holds the lead-in, ...
    assert_ne!(
        std::fs::read(&once).unwrap(),
        std::fs::read(&looped).unwrap(),
        "the looping MOV has no lead-in"
    );
    // ... which decodes to nothing: PCM has no priming to smear, so the
    // rest is the same samples.
    let decode = |path: &Path| FfmpegBackend::new().decode_audio(path, 48_000, 2).unwrap();
    let (once, looped) = (decode(&once), decode(&looped));
    assert_eq!(looped.frame_count(), once.frame_count());
    assert!(looped.samples == once.samples);
}

/// The sample frames of the mix of `project`'s one sound: its source,
/// decoded at the project's rate in stereo, as the mixer reads it.
fn mix_frames(dir: &Path, name: &str, sample_rate: u32) -> celesta_media::AudioBuffer {
    FfmpegBackend::new()
        .decode_audio(&dir.join(name), sample_rate, 2)
        .unwrap()
}

#[test]
fn mp4_and_mov_start_with_an_edit_list_hiding_the_delay() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = hits_project(directory.path());
    let mix = mix_frames(directory.path(), "hits.wav", 48_000);
    // The MOV with AAC too, so both hide AAC's 1024 priming samples.
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
    for name in ["clip.mp4", "clip.mov"] {
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
}

#[test]
fn opus_webm_declares_its_codec_delay() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = hits_project(directory.path());
    let mix = mix_frames(directory.path(), "hits.wav", 48_000);
    let Some(output) = export_as(&project, directory.path(), "clip.webm") else {
        return;
    };
    assert_eq!(
        audio_stream(&output.to_string_lossy()),
        ("opus".to_owned(), 48_000)
    );
    // Without CodecDelay the decoder would keep libopus's 312 priming
    // samples.
    let decoded = FfmpegBackend::new()
        .decode_audio(&output, 48_000, 2)
        .unwrap();
    let difference = decoded.frame_count().abs_diff(mix.frame_count());
    assert!(
        difference < 960,
        "{} decoded frames for a mix of {}",
        decoded.frame_count(),
        mix.frame_count()
    );
}

#[test]
fn pcm_mov_decodes_to_the_mix_exactly() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = hits_project(directory.path());
    let mix = mix_frames(directory.path(), "hits.wav", 48_000);
    let Some(output) = export_as(&project, directory.path(), "clip.mov") else {
        return;
    };
    assert_eq!(
        audio_stream(&output.to_string_lossy()),
        ("pcm_s24le".to_owned(), 48_000)
    );
    let decoded = FfmpegBackend::new()
        .decode_audio(&output, 48_000, 2)
        .unwrap();
    assert_eq!(decoded.samples.len(), mix.samples.len());
    let worst = decoded
        .samples
        .iter()
        .zip(&mix.samples)
        .map(|(decoded, mixed)| (decoded - mixed).abs())
        .fold(0.0, f32::max);
    assert!(worst <= 1.0 / 8_388_608.0, "off by up to {worst}");
}

#[test]
fn prores_and_webm_export_without_the_x264_plugin() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    for name in ["without-x264.mov", "without-x264.webm"] {
        let mut plugins = support::registry::plugins();
        plugins.retain(|plugin| plugin.id != "ffmpeg-x264");
        celesta_media::set_registry(Some(Arc::new(MediaRegistry::new(plugins))));
        let path = directory.path().join(name);
        let result = Exporter::new(ExportOptions::default()).export_project(
            &project,
            directory.path(),
            &path,
        );
        install();
        match result {
            Ok(()) => assert!(path.is_file()),
            Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
                skip_without_gpu(&error);
                return;
            }
            Err(error) => panic!("export without x264 failed: {error}"),
        }
    }
}
