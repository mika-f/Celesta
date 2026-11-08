//! Compare the original per-frame protocol with batched audio collection.
//! Media is decoded by an installed media plugin's process, as in the editor:
//! the media plugins found, as Celesta finds them. Without one it exits
//! with "no media plugin found". In a source checkout, build the plugin into
//! `plugins/dist` and pass that folder's absolute path. Run after
//! `pnpm run build:runtime` and generating any media:
//!
//! ```text
//! node scripts/plugins/build.mts plugins/ffmpeg
//! CELESTA_PLUGIN_PATH="$PWD/plugins/dist" cargo run --release -p celesta-react-bridge --example audio-bench -- <entry> [runs]
//! ```

use std::{env, error::Error, path::PathBuf, process, sync::Arc, time::Instant};

use celesta_composition::{AudioGraph, Time};
use celesta_media::{
    MediaBackend, MediaProcess, MediaRegistry, RegisteredPlugin, mix_audio_graph, set_registry,
};
use celesta_plugin_host::InstalledPlugins;
use celesta_react_bridge::{ReactBridge, react_audio_clips};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let entry =
        PathBuf::from(args.next().ok_or("usage: audio-bench <entry> [runs]")?).canonicalize()?;
    let runs: usize = args.next().map_or(Ok(5), |value| value.parse())?;
    let media = InstalledPlugins::find_reporting(&[], &[]).media();
    if media.is_empty() {
        eprintln!(
            "no media plugin found: build one with node scripts/plugins/build.mts plugins/ffmpeg \
             and add the absolute path of plugins/dist to CELESTA_PLUGIN_PATH"
        );
        process::exit(1);
    }
    set_registry(Some(Arc::new(MediaRegistry::new(
        media
            .into_iter()
            .map(|plugin| RegisteredPlugin {
                id: plugin.id,
                origin: plugin.origin,
                decoders: plugin.media.decoders,
                factory: MediaProcess::factory(plugin.executable, plugin.directory),
            })
            .collect(),
    ))));
    let entry_dir = entry.parent().ok_or("entry has no parent")?;
    let cli = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/cli/dist/cli.js");
    println!("mode,run,frames,clips,spawn_ms,scan_ms,mix_ms,total_ms");
    for run in 0..runs {
        let mut expected = None;
        // Alternate order to reduce systematic warm-cache bias.
        for legacy in if run % 2 == 0 {
            [true, false]
        } else {
            [false, true]
        } {
            let started = Instant::now();
            let mut bridge = ReactBridge::spawn("node", &cli, &entry)?;
            let metadata = bridge.metadata().clone();
            let spawn = started.elapsed();
            let scanning = Instant::now();
            let graph = if legacy {
                let mut reports = Vec::new();
                for frame in 0..metadata.duration_in_frames {
                    reports.extend(
                        bridge
                            .evaluate_at(Time::frames(frame as i64, metadata.frame_rate)?, None)?
                            .audio,
                    );
                }
                AudioGraph {
                    sample_rate: 48_000,
                    master_volume: 1.0,
                    clips: react_audio_clips(&reports, entry_dir).unwrap(),
                }
            } else {
                bridge.collect_audio_graph(48_000, 1.0, entry_dir)?
            };
            let scan = scanning.elapsed();
            if let Some(ref expected) = expected {
                assert_eq!(&graph, expected, "batched and per-frame graphs differ");
            } else {
                expected = Some(graph.clone());
            }
            let mixing = Instant::now();
            let duration = Time::frames(metadata.duration_in_frames as i64, metadata.frame_rate)?;
            let _buffer = mix_audio_graph(&graph, entry_dir, duration, &mut MediaBackend::new())?;
            let mix = mixing.elapsed();
            println!(
                "{},{run},{},{},{:.3},{:.3},{:.3},{:.3}",
                if legacy { "per-frame" } else { "batch" },
                metadata.duration_in_frames,
                graph.clips.len(),
                spawn.as_secs_f64() * 1000.0,
                scan.as_secs_f64() * 1000.0,
                mix.as_secs_f64() * 1000.0,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
    Ok(())
}
