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
//! pipe, to compare the two.

use celesta_composition::Rational;
use celesta_media::{
    MediaBackend, MediaProcess, MediaRegistry, ProviderFactory, RegisteredPlugin, VideoFrameDecoder,
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
    }
}

/// What a run of frames took.
struct Timing {
    size: (u32, u32),
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
        wall,
        host,
        plugin: since(opened, |(_, children)| children),
    }
}

/// `frames` RGBA frames of `video`'s size encoded to H.264 (`ultrafast`, so
/// the encoder's own work hides less of the transport) after one frame that
/// opens the encoder, and the encoder finished. The host's CPU time covers
/// the whole run, the encoder's threads in this process included.
fn encode(_factory: ProviderFactory, _video: &Path, _frames: u32) -> Timing {
    unimplemented!("ported in Task 13")
}
