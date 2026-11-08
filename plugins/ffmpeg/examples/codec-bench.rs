//! Times what crossing the codec plugin's pipe costs: decoding a video's
//! frames in order, as preview playback and export do, and encoding RGBA
//! frames to H.264, once with FFmpeg in this process (`FfmpegCodec`) and
//! once through the codec plugin's program (`MediaProcess`), whose frames
//! cross its stdin or stdout. The difference is the transport.
//!
//! ```text
//! node scripts/plugins/build.mts plugins/ffmpeg
//! cargo run --release --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg \
//!     --example codec-bench -- plugins/dist/ffmpeg/bin/macos-aarch64/celesta-codec-ffmpeg \
//!     <video.mp4> [frames]
//! ```
//!
//! The first argument is the built plugin's program. Each line gives the
//! wall-clock time a frame took and, on Unix, the CPU time (user and
//! system) it cost this process and the plugin's, which a busy machine
//! changes less than the wall clock. The plugin's includes opening it and
//! its first frame. Decoded frames come through shared memory when the
//! plugin accepts it; `CELESTA_CODEC_SHARED_MEMORY=0` keeps them in the
//! pipe, to compare the two. Encoded frames go the same way, through the
//! export ring and pipelined three deep; the same variable compares the
//! pipe, pipelined.

use celesta_codec_ffmpeg::DECODER_ID;
use celesta_composition::Rational;
use celesta_media::{
    Answered, Color, FrameTransport, MediaBackend, MediaProcess, MediaProvider, MediaRegistry,
    Pipeline, PixelFormat, ProviderFactory, ProviderUse, RegisteredPlugin, SessionId, SettingValue,
    Settings, VideoEncoderRequest, VideoFrameDecoder, ring_layout,
};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() {
    let usage = "usage: codec-bench <program> <video> [frames]";
    let mut args = env::args().skip(1);
    let program = PathBuf::from(args.next().expect(usage))
        .canonicalize()
        .expect("the program exists");
    let plugin = program
        .ancestors()
        .nth(3)
        .expect("the program is in <plugin>/bin/<platform>/")
        .to_owned();
    let video = PathBuf::from(args.next().expect(usage))
        .canonicalize()
        .expect("the video exists");
    let frames: u32 = args
        .next()
        .map_or(120, |value| value.parse().expect("frames must be a number"));
    let codecs: [(&str, ProviderFactory); 2] = [
        ("in process", celesta_codec_ffmpeg::FfmpegCodec::factory()),
        ("plugin", MediaProcess::factory(program, plugin)),
    ];
    for (name, factory) in &codecs {
        let timing = decode(factory.clone(), &video, frames);
        timing.print("decode", name, frames);
    }
    for (name, factory) in &codecs {
        let timing = encode(factory.clone(), &video, frames);
        timing.print("encode", name, frames);
        if let Some(how) = &timing.how {
            println!("  ({how})");
        }
    }
}

/// What a run of frames took.
struct Timing {
    size: (u32, u32),
    /// How frames reached the encoder, for an encode run.
    how: Option<String>,
    wall: Duration,
    /// This process's CPU time during the frames.
    host: Option<Duration>,
    /// The CPU time of the processes that exited during the run: the
    /// plugin's, from its start.
    plugin: Option<Duration>,
}

impl Timing {
    fn print(&self, what: &str, name: &str, frames: u32) {
        let per_frame = |time: Duration| time.as_secs_f64() * 1000.0 / f64::from(frames);
        let cpu = |time: Option<Duration>| {
            time.map_or("-".to_owned(), |time| format!("{:.2}", per_frame(time)))
        };
        println!(
            "{what}  {name:10}  {}x{}  {:6.2} ms/frame  cpu: host {} plugin {} ms/frame",
            self.size.0,
            self.size.1,
            per_frame(self.wall),
            cpu(self.host),
            cpu(self.plugin),
        );
    }
}

/// This process's CPU time and that of its children that have exited and
/// been waited for.
#[cfg(unix)]
fn cpu() -> Option<(Duration, Duration)> {
    fn of(who: libc::c_int) -> Duration {
        // SAFETY: `usage` is a valid `rusage` for `getrusage` to fill.
        let usage = unsafe {
            let mut usage = std::mem::zeroed::<libc::rusage>();
            libc::getrusage(who, &mut usage);
            usage
        };
        let time = |time: libc::timeval| {
            Duration::from_secs(time.tv_sec as u64) + Duration::from_micros(time.tv_usec as u64)
        };
        time(usage.ru_utime) + time(usage.ru_stime)
    }
    Some((of(libc::RUSAGE_SELF), of(libc::RUSAGE_CHILDREN)))
}

#[cfg(not(unix))]
fn cpu() -> Option<(Duration, Duration)> {
    None
}

fn since(
    start: Option<(Duration, Duration)>,
    pick: fn((Duration, Duration)) -> Duration,
) -> Option<Duration> {
    Some(pick(cpu()?) - pick(start?))
}

/// A backend on the FFmpeg plugin made by `factory`.
fn backend(factory: ProviderFactory) -> MediaBackend {
    MediaBackend::with_registry(Arc::new(MediaRegistry::new(vec![RegisteredPlugin {
        factory,
        ..celesta_codec_ffmpeg::registered()
    }])))
}

/// `frames` frames of `video` from its start, in order, after a first frame
/// that opens the codec and the decode run.
fn decode(factory: ProviderFactory, video: &Path, frames: u32) -> Timing {
    let opened = cpu();
    let frame_rate = Rational::new(30, 1);
    let mut backend = backend(factory).with_sequential_video(frame_rate);
    let first = backend
        .decode_frame_for("bench", video, 0.0)
        .expect("the first frame decodes");
    let started = Instant::now();
    let host = cpu();
    for frame in 1..=frames {
        let decoded = backend
            .decode_frame_for("bench", video, f64::from(frame) / 30.0)
            .expect("a frame decodes");
        std::hint::black_box(decoded.pixels.len());
    }
    let wall = started.elapsed();
    let host = since(host, |(own, _)| own);
    // Stops the plugin, which is waited for.
    drop(backend);
    Timing {
        size: (first.width, first.height),
        how: None,
        wall,
        host,
        plugin: since(opened, |(_, children)| children),
    }
}

/// `frames` RGBA frames of `video`'s size encoded to H.264 (`ultrafast`, so
/// the encoder's own work hides less of the transport) after one frame that
/// opens the encoder, and the encoder finished; the packets are dropped.
/// The host's CPU time covers the whole run, the encoder's threads in this
/// process included.
fn encode(factory: ProviderFactory, video: &Path, frames: u32) -> Timing {
    let opened = cpu();
    // The size comes from a plugin that decodes, before the export's own
    // starts with a ring for it.
    let (width, height) = {
        let mut probing =
            factory(&Default::default(), ProviderUse::Decode).expect("the plugin starts");
        let probe = probing
            .decoder()
            .expect("the plugin decodes")
            .probe(DECODER_ID, video)
            .expect("the video probe");
        let stream = probe.video.expect("the file has a video stream");
        (stream.width, stream.height)
    };
    let ring = ring_layout(&[PixelFormat::Rgba8], width, height);
    let mut provider =
        factory(&Default::default(), ProviderUse::Export { ring }).expect("the plugin starts");
    let settings: Settings = [(
        "preset".to_owned(),
        SettingValue::String("ultrafast".to_owned()),
    )]
    .into_iter()
    .collect();
    let color = Color::bt709_limited();
    let session = provider
        .video_encoder()
        .expect("the plugin encodes video")
        .open_video(&VideoEncoderRequest {
            component: "libx264",
            settings: &settings,
            width,
            height,
            frame_rate: Rational::new(30, 1),
            timescale: 30,
            offered: &[PixelFormat::Rgba8],
            color: &color,
        })
        .expect("the encoder opens")
        .session;
    let frame = |index: u32| -> Vec<u8> {
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        for (offset, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let value = (offset as u32).wrapping_add(index * 7) as u8;
            *pixel = [value, value / 2, 255 - value, 255];
        }
        pixels
    };
    let pixels: Vec<Vec<u8>> = (0..4).map(frame).collect();
    // A plugin pipelines when it has a pipeline; an encoder in this process
    // has none, and takes one frame at a time.
    let depth = provider.pipeline().map(|pipeline| pipeline.depth());
    let how = match depth {
        None => "in process".to_owned(),
        Some(depth) => {
            let shared = ring.is_some()
                && matches!(
                    FrameTransport::from_environment(),
                    FrameTransport::SharedMemory
                );
            format!("{}, {depth} deep", if shared { "ring" } else { "pipe" })
        }
    };
    let mut packets = 0;
    send(
        provider.as_mut(),
        session,
        0,
        pixels[0].clone(),
        &mut packets,
    );
    drain(provider.as_mut(), &mut packets);
    // The whole loop is timed, copying each frame (as the renderer's
    // readback makes one) included: an encoder in this process goes on
    // encoding on its own threads while the next frame is copied, so timing
    // only `encode_video` would leave its work out.
    let started = Instant::now();
    let before = cpu();
    for index in 1..=frames {
        let frame = pixels[index as usize % pixels.len()].clone();
        send(
            provider.as_mut(),
            session,
            i64::from(index),
            frame,
            &mut packets,
        );
    }
    drain(provider.as_mut(), &mut packets);
    let finished = provider
        .video_encoder()
        .expect("the plugin encodes video")
        .finish_video(session)
        .expect("the encoder finishes");
    std::hint::black_box(finished.packets.len() + packets);
    let wall = started.elapsed();
    let host = since(before, |(own, _)| own);
    // Stops the plugin, which is waited for.
    drop(provider);
    Timing {
        size: (width, height),
        how: Some(how),
        wall,
        host,
        plugin: since(opened, |(_, children)| children),
    }
}

/// Encodes one frame: through the pipeline, after receiving an answer when
/// its window is full, when `provider` has one, else on the spot.
fn send(
    provider: &mut dyn MediaProvider,
    session: SessionId,
    pts: i64,
    frame: Vec<u8>,
    packets: &mut usize,
) {
    if let Some(pipeline) = provider.pipeline() {
        while pipeline.in_flight() >= pipeline.depth() {
            receive(pipeline, packets);
        }
        pipeline
            .send_video_frame(session, pts, frame)
            .expect("a frame is sent");
    } else {
        let encoded = provider
            .video_encoder()
            .expect("the plugin encodes video")
            .encode_video(session, pts, frame)
            .expect("a frame encodes");
        *packets += encoded.packets.len();
    }
}

/// Receives every answer still in flight.
fn drain(provider: &mut dyn MediaProvider, packets: &mut usize) {
    if let Some(pipeline) = provider.pipeline() {
        while pipeline.in_flight() > 0 {
            receive(pipeline, packets);
        }
    }
}

fn receive(pipeline: &mut dyn Pipeline, packets: &mut usize) {
    match pipeline.receive().expect("a frame encodes") {
        Answered::VideoFrame(encoded) => *packets += encoded.packets.len(),
        other => panic!("expected a video frame answer, got {other:?}"),
    }
}
