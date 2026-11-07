//! Times drawing an avatar through Celesta's GPU renderer, the plugin's
//! provider answering in this process, as the editor's preview draws it
//! (`render_preview`): the first frame, which opens the model and compiles
//! its material shaders and pipelines, then frames after it at successive
//! times. It draws the synthetic avatar, once with its MToon materials and
//! once with them made plain glTF (drawn unlit), so the first frames'
//! difference is what compiling MToon costs; or, given a path, that `.vrm`
//! file.
//!
//! ```text
//! cargo run --release --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm \
//!     --example render-bench -- [frames] [width height] [model.vrm]
//! ```
//!
//! With `RENDER_BENCH_PNG` set to a folder, it also writes each model's
//! frame at 1 s there as `<name>.png`.

use celesta_composition::{
    AssetLocation, BlendMode, EvaluatedTransform, Layer, LayerContent, Point, Rational,
    ResolvedAsset, Scene, Time,
};
use celesta_gpu_renderer::{Color, GpuRenderOptions, GpuRenderer};
use celesta_plugin_vrm::VrmProvider;
use celesta_plugin_vrm::synthetic::SyntheticVrm;
use serde_json::Value;
use std::env;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn main() {
    let mut args = env::args().skip(1);
    let frames: u32 = args
        .next()
        .map_or(120, |value| value.parse().expect("frames must be a number"));
    let width: u32 = args.next().map_or(1920, |value| value.parse().unwrap());
    let height: u32 = args.next().map_or(1080, |value| value.parse().unwrap());
    let directory = tempfile::TempDir::new().unwrap();
    let models: Vec<(&str, PathBuf)> = match args.next() {
        Some(path) => vec![("file", PathBuf::from(path).canonicalize().unwrap())],
        None => {
            let mtoon = directory.path().join("mtoon.vrm");
            std::fs::write(&mtoon, SyntheticVrm::avatar().glb()).unwrap();
            let plain = directory.path().join("plain.vrm");
            std::fs::write(&plain, SyntheticVrm::avatar().glb_with(without_mtoon)).unwrap();
            vec![("unlit", plain), ("mtoon", mtoon)]
        }
    };
    for (name, path) in &models {
        let cache = tempfile::TempDir::new().unwrap();
        let mut renderer = GpuRenderer::new(GpuRenderOptions {
            background: Color::rgba(0, 0, 0, 0),
            ..GpuRenderOptions::default()
        })
        .expect("a GPU renderer")
        .with_content_provider(VrmProvider::with_texture_cache(cache.path()));
        let started = Instant::now();
        renderer
            .render_preview(&scene(path, width, height, 0.0))
            .expect("the first frame draws");
        let first = started.elapsed();
        let mut later = Duration::ZERO;
        for frame in 1..=frames {
            let scene = scene(path, width, height, f64::from(frame) / 30.0);
            let started = Instant::now();
            std::hint::black_box(renderer.render_preview(&scene).expect("a frame draws"));
            later += started.elapsed();
        }
        if let Some(folder) = env::var_os("RENDER_BENCH_PNG") {
            let frame = renderer
                .render(&scene(path, width, height, 1.0))
                .expect("the frame draws");
            image::save_buffer(
                Path::new(&folder).join(format!("{name}.png")),
                frame.pixels(),
                frame.width(),
                frame.height(),
                image::ExtendedColorType::Rgba8,
            )
            .expect("the PNG is written");
        }
        println!(
            "{name:5}  {width}x{height}  first frame {:7.1} ms  then {:6.2} ms/frame ({} on {})",
            first.as_secs_f64() * 1000.0,
            later.as_secs_f64() * 1000.0 / f64::from(frames),
            frames,
            renderer.adapter_info().name
        );
    }
}

/// The avatar's materials without `VRMC_materials_mtoon`: plain glTF, which
/// Celesta draws unlit.
fn without_mtoon(document: &mut Value) {
    for material in document["materials"].as_array_mut().unwrap() {
        if let Some(extensions) = material["extensions"].as_object_mut() {
            extensions.remove("VRMC_materials_mtoon");
        }
    }
}

/// A `width` by `height` scene of one plugin layer as large as it: the
/// avatar at `path`, at `time`.
fn scene(path: &Path, width: u32, height: u32, time: f64) -> Scene {
    Scene {
        width,
        height,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![Layer {
            id: "avatar".into(),
            transform: EvaluatedTransform {
                anchor: Point { x: 0.0, y: 0.0 },
                scale: Point {
                    x: f64::from(width) / 1920.0,
                    y: f64::from(height) / 1080.0,
                },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Plugin {
                plugin: celesta_plugin_vrm::PLUGIN.into(),
                assets: [(
                    "model".to_owned(),
                    ResolvedAsset {
                        id: "avatar".into(),
                        location: AssetLocation::File {
                            path: path.to_str().unwrap().into(),
                        },
                    },
                )]
                .into(),
                time,
                data: Value::Null,
            },
        }],
        shaders: Vec::new(),
    }
}
