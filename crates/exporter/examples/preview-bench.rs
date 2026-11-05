//! Times the editor preview's per-frame cost for a standalone React entry,
//! split into the Node round-trip and the GPU render, with and without the
//! sequential video decoder. Frames are decoded by the codec plugin's
//! process, as in the editor, so the times include sending them across it:
//! the `celesta-codec-ffmpeg` built beside this example's folder. Build it
//! first, then run:
//!
//! ```text
//! cargo build --release -p celesta-codec-ffmpeg
//! cargo run --release --example preview-bench -- <entry.tsx> [frames]
//! ```

use std::env;
use std::path::PathBuf;
use std::time::Instant;

use celesta_composition::Time;
use celesta_gpu_renderer::{GpuRenderOptions, GpuRenderer};
use celesta_media::{CodecProcess, MediaBackend};
use celesta_react_bridge::ReactBridge;

fn main() {
    // Cargo puts examples in `target/<profile>/examples`, a folder below the
    // plugin's program.
    let profile = env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.to_path_buf()))
        .expect("the example's folder");
    let codec = profile.join(format!(
        "{}{}",
        celesta_plugin_host::CODEC_PROGRAM,
        env::consts::EXE_SUFFIX
    ));
    assert!(
        codec.is_file(),
        "build the codec plugin first: cargo build --release -p celesta-codec-ffmpeg"
    );
    celesta_media::set_codec(Some(CodecProcess::factory(codec, profile)));
    let mut args = env::args().skip(1);
    let entry = PathBuf::from(args.next().expect("usage: preview-bench <entry> [frames]"));
    let frames: i64 = args
        .next()
        .map_or(30, |value| value.parse().expect("frames must be a number"));
    let sequential = env::var("SEQUENTIAL").is_ok();

    let cli_script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/cli/dist/cli.js");
    let entry = entry.canonicalize().expect("entry exists");
    let asset_root = entry.parent().expect("entry has a parent").to_owned();

    let spawn_started = Instant::now();
    let mut bridge = ReactBridge::spawn(PathBuf::from("node"), &cli_script, &entry)
        .expect("react bridge spawns");
    let metadata = bridge.metadata().clone();
    println!(
        "spawn: {:?}  {}x{} @ {}/{} fps, {} frames",
        spawn_started.elapsed(),
        metadata.width,
        metadata.height,
        metadata.frame_rate.numerator,
        metadata.frame_rate.denominator,
        metadata.duration_in_frames
    );

    let decoder = if sequential {
        MediaBackend::new().with_sequential_video(metadata.frame_rate)
    } else {
        MediaBackend::new()
    };
    let mut renderer = GpuRenderer::new(GpuRenderOptions::default())
        .expect("gpu renderer")
        .with_asset_root(asset_root)
        .with_video_decoder(decoder);
    println!(
        "gpu: {}  sequential video: {sequential}",
        renderer.adapter_info().name
    );

    let fps = f64::from(metadata.frame_rate.numerator) / f64::from(metadata.frame_rate.denominator);
    let realtime = env::var("REALTIME").is_ok();
    let mut node_total = 0.0f64;
    let mut gpu_total = 0.0f64;
    let mut rendered = 0i64;
    let mut window = 0.0f64;
    let mut frame = 0i64;
    let playback_started = Instant::now();
    while frame < frames {
        let time = Time::frames(frame, metadata.frame_rate).expect("valid frame");
        let node_started = Instant::now();
        let scene = bridge.scene_at(time).expect("scene evaluates");
        let node = node_started.elapsed();
        let gpu_started = Instant::now();
        renderer.render_preview(&scene).expect("frame renders");
        let gpu = gpu_started.elapsed();
        node_total += node.as_secs_f64();
        gpu_total += gpu.as_secs_f64();
        rendered += 1;
        println!(
            "frame {frame:4}  node {:7.1}ms  gpu {:7.1}ms  total {:7.1}ms",
            node.as_secs_f64() * 1000.0,
            gpu.as_secs_f64() * 1000.0,
            (node.as_secs_f64() + gpu.as_secs_f64()) * 1000.0
        );
        if rendered % 100 == 0 {
            println!(
                "  -- {rendered} rendered, playhead at frame {frame}, last 100 avg {:.1}ms",
                (node_total + gpu_total - window) / 100.0 * 1000.0
            );
            window = node_total + gpu_total;
        }
        frame = if realtime {
            // What the editor actually asks for: the playhead runs on the wall
            // clock, so a render slower than the frame interval skips ahead
            // rather than falling behind.
            (playback_started.elapsed().as_secs_f64() * fps) as i64
        } else {
            frame + 1
        }
        .max(frame + 1);
    }

    let count = rendered as f64;
    let total = node_total + gpu_total;
    println!(
        "\n{rendered} frames rendered covering {frames} composition frames \
         ({:.0}% shown): node {:.1}ms  gpu {:.1}ms  total {:.1}ms  => {:.1} fps \
         (composition is {fps:.0} fps)",
        count / frames as f64 * 100.0,
        node_total / count * 1000.0,
        gpu_total / count * 1000.0,
        total / count * 1000.0,
        count / total
    );
}
