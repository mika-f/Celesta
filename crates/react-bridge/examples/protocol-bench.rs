//! Times `ReactBridge::scene_at` round trips (Node evaluation, JSON transfer
//! and decoding) for a React entry. Evaluates 30 warmup frames from `start`,
//! then measures the next `frames` (wrapping at the composition's end). Run
//! as:
//!
//! ```text
//! cargo run --release -p celesta-react-bridge --example protocol-bench -- <entry.tsx> [frames] [start]
//! ```

use celesta_composition::Time;
use celesta_react_bridge::ReactBridge;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let entry = PathBuf::from(
        args.next()
            .expect("usage: protocol-bench <entry> [frames] [start]"),
    )
    .canonicalize()
    .expect("entry exists");
    let frames: u64 = args.next().map_or(240, |value| {
        value
            .parse()
            .ok()
            .filter(|&frames| frames > 0)
            .expect("frames must be a positive number")
    });
    let start: u64 = args.next().map_or(0, |value| {
        value
            .parse()
            .expect("start must be a frame number of zero or more")
    });
    let cli = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/react/dist/cli.js");
    let mut bridge = ReactBridge::spawn("node", &cli, &entry).expect("bridge spawns");
    let metadata = bridge.metadata().clone();
    let warmup = 30;
    let mut times = Vec::new();
    for index in 0..warmup + frames {
        let frame = (start + index) % metadata.duration_in_frames.max(1);
        let time = Time::frames(frame as i64, metadata.frame_rate).expect("valid frame");
        let started = Instant::now();
        std::hint::black_box(bridge.scene_at(time).expect("scene evaluates"));
        if index >= warmup {
            times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "median {:.3} ms  mean {:.3} ms  p90 {:.3} ms",
        times[times.len() / 2],
        mean,
        times[times.len() * 9 / 10],
    );
}
