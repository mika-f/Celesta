//! Host-side sequential decode benchmark, using an unchanged media plugin.
//! `cargo run --release -p celesta-codec-test --example decode-prefetch -- <plugin> <video>`
//! Measures 120 frames through MediaBackend with 0, 5 and 15 ms of busy
//! host work per frame, three interleaved off/on runs for each workload.

use celesta_composition::Rational;
use celesta_media::{
    FrameTransport, MediaBackend, MediaProcess, MediaRegistry, ProviderUse, RegisteredPlugin,
    VideoFrameDecoder,
};
use celesta_plugin_host::{MediaManifest, MediaOrigin};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let program = PathBuf::from(args.next().expect("plugin executable"))
        .canonicalize()
        .unwrap();
    let video = PathBuf::from(args.next().expect("input video"))
        .canonicalize()
        .unwrap();
    let output = std::process::Command::new(&program)
        .arg("--manifest")
        .output()
        .unwrap();
    assert!(output.status.success());
    let manifest: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let (media, errors) = MediaManifest::from_json(manifest["media"].clone());
    assert!(errors.is_empty(), "{errors:?}");
    for repeat in 1..=3 {
        for work_ms in [0, 5, 15] {
            for enabled in [false, true] {
                let executable = program.clone();
                let plugin = RegisteredPlugin {
                    id: manifest["id"].as_str().unwrap().to_owned(),
                    name: "Decode benchmark".to_owned(),
                    version: manifest["version"].as_str().unwrap().to_owned(),
                    origin: MediaOrigin::Bundled,
                    directory: program.parent().unwrap().to_owned(),
                    executable: Some(program.clone()),
                    media: media.clone(),
                    factory: Arc::new(move |kill, purpose| {
                        let mut process = MediaProcess::start_with(
                            &executable,
                            executable.parent().unwrap(),
                            kill,
                            FrameTransport::SharedMemory,
                            purpose,
                        )?;
                        if purpose == ProviderUse::Decode {
                            assert!(process.shares_frames());
                        }
                        process.set_decode_prefetch(enabled);
                        Ok(Box::new(process))
                    }),
                };
                let mut backend =
                    MediaBackend::with_registry(Arc::new(MediaRegistry::new(vec![plugin])))
                        .with_sequential_video(Rational::new(30, 1));
                let probe = backend.probe(&video).unwrap();
                let stream = probe.video.as_ref().unwrap();
                let (width, height, rate) =
                    (stream.width, stream.height, stream.frame_rate.unwrap());
                let fps = f64::from(rate.numerator) / f64::from(rate.denominator);
                for index in 0..4 {
                    std::hint::black_box(
                        backend
                            .decode_frame_for("layer", &video, f64::from(index) / fps)
                            .unwrap(),
                    );
                }
                let started = Instant::now();
                for index in 4..124 {
                    std::hint::black_box(
                        backend
                            .decode_frame_for("layer", &video, f64::from(index) / fps)
                            .unwrap(),
                    );
                    let work = Instant::now();
                    while work.elapsed() < Duration::from_millis(work_ms) {
                        std::hint::spin_loop();
                    }
                }
                println!(
                    "{}x{} work={work_ms}ms prefetch={enabled} repeat={repeat} {:.3} ms/frame (shared memory)",
                    width,
                    height,
                    started.elapsed().as_secs_f64() * 1000.0 / 120.0
                );
            }
        }
    }
}
