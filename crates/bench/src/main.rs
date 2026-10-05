//! Renders the synthetic workloads in `workloads.rs` through the GPU
//! renderer's export path (pipelined `submit`/`drain` with readback) and
//! prints one JSON line per workload. `scripts/bench.py` drives it: on a real
//! GPU it times frames, and in CI it runs under Cachegrind with lavapipe
//! and counts instructions instead, of the measured frames only (see
//! `cachegrind.rs`). Run as:
//!
//! ```text
//! cargo run --release -p celesta-bench -- list
//! cargo run --release -p celesta-bench -- run [--frames N] [--warmup N] [--size WxH] [--dump DIR] [workload...]
//! ```
//!
//! `--dump DIR` saves each workload's last frame as `DIR/<workload>.png`,
//! outside the timing, to check what a workload draws.

mod cachegrind;
mod workloads;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use celesta_composition::Scene;
use celesta_gpu_renderer::{GpuRenderOptions, GpuRenderer};

use workloads::{IMAGE_PATH, WORKLOADS, Workload};

struct Options {
    frames: usize,
    warmup: usize,
    width: u32,
    height: u32,
    dump: Option<PathBuf>,
    workloads: Vec<&'static Workload>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("list") => {
            for workload in WORKLOADS {
                println!("{}\t{}", workload.name, workload.description);
            }
            Ok(())
        }
        Some("run") => parse(&args[1..]).and_then(|options| run(&options)),
        _ => Err("usage: celesta-bench list | run [--frames N] [--warmup N] \
                  [--size WxH] [--dump DIR] [workload...]"
            .to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("celesta-bench: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        frames: 120,
        warmup: 10,
        width: 1920,
        height: 1080,
        dump: None,
        workloads: Vec::new(),
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| format!("{arg} needs a value"))
                .map(String::as_str)
        };
        let number = |value: &str| {
            value
                .parse::<usize>()
                .map_err(|_| format!("{arg}: not a number: {value}"))
        };
        match arg.as_str() {
            "--frames" => options.frames = number(value()?)?,
            "--warmup" => options.warmup = number(value()?)?,
            "--size" => {
                let value = value()?;
                let (width, height) = value
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                    .filter(|&(w, h)| w > 0 && h > 0)
                    .ok_or_else(|| format!("--size: expected WxH, got {value}"))?;
                // The workloads are 16:9 and scale by width.
                if u64::from(width) * 9 != u64::from(height) * 16 {
                    return Err(format!("--size: {value} is not 16:9"));
                }
                (options.width, options.height) = (width, height);
            }
            "--dump" => options.dump = Some(PathBuf::from(value()?)),
            name => options.workloads.push(
                WORKLOADS
                    .iter()
                    .find(|workload| workload.name == name)
                    .ok_or_else(|| format!("unknown workload {name}"))?,
            ),
        }
    }
    if options.frames == 0 {
        return Err("--frames must be positive".to_owned());
    }
    if options.workloads.is_empty() {
        options.workloads = WORKLOADS.iter().collect();
    }
    Ok(options)
}

fn run(options: &Options) -> Result<(), String> {
    let assets = options
        .workloads
        .iter()
        .any(|workload| workload.name == "images")
        .then(AssetDir::new)
        .transpose()
        .map_err(|error| format!("writing the image asset: {error}"))?;
    for workload in &options.workloads {
        let fail = |error| format!("{}: {error}", workload.name);
        // Built before any timing, so only rendering is measured.
        let (scenes, asset_root) = workload
            .scenes(
                options.width,
                options.height,
                options.warmup + options.frames,
                assets.as_ref().map(AssetDir::path),
            )
            .map_err(fail)?;
        let (warmup, measured) = scenes.split_at(options.warmup);
        // A fresh renderer per workload, so one workload's caches never
        // help another.
        let mut renderer = GpuRenderer::new(GpuRenderOptions::default())
            .map_err(|error| format!("creating the GPU renderer: {error}"))?
            .with_asset_root(asset_root);
        let render = |renderer: &mut GpuRenderer, scenes: &[Scene]| {
            let mut rendered = 0;
            for scene in scenes {
                rendered += usize::from(renderer.submit(scene)?.is_some());
            }
            rendered += renderer.drain()?.len();
            assert_eq!(rendered, scenes.len(), "every frame comes back");
            Ok::<_, celesta_gpu_renderer::GpuRenderError>(())
        };
        render(&mut renderer, warmup).map_err(|error| fail(error.to_string()))?;
        cachegrind::start();
        let started = Instant::now();
        let result = render(&mut renderer, measured);
        let elapsed = started.elapsed();
        cachegrind::stop();
        result.map_err(|error| fail(error.to_string()))?;
        if let Some(dir) = &options.dump {
            let frame = renderer
                .render(measured.last().expect("--frames is positive"))
                .map_err(|error| fail(error.to_string()))?;
            std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
            let path = dir.join(format!("{}.png", workload.name));
            image::RgbaImage::from_raw(frame.width(), frame.height(), frame.into_pixels())
                .expect("render returns whole RGBA frames")
                .save(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
        let adapter = renderer.adapter_info();
        println!(
            "{{\"workload\":\"{}\",\"frames\":{},\"width\":{},\"height\":{},\
             \"ms_per_frame\":{:.4},\"adapter\":\"{}\",\"backend\":\"{:?}\"}}",
            workload.name,
            options.frames,
            options.width,
            options.height,
            elapsed.as_secs_f64() * 1000.0 / options.frames as f64,
            adapter.name.replace(['"', '\\'], ""),
            adapter.backend,
        );
    }
    Ok(())
}

/// A temporary asset root holding the `images` workload's image, written
/// only when that workload runs.
struct AssetDir(PathBuf);

impl AssetDir {
    fn new() -> Result<Self, image::ImageError> {
        let path = std::env::temp_dir().join(format!("celesta-bench-{}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        let dir = Self(path);
        workloads::image().save(dir.0.join(IMAGE_PATH))?;
        Ok(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for AssetDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
