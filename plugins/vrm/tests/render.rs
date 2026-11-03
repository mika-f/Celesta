//! The synthetic avatar drawn by Celesta's GPU renderer, the plugin's
//! provider answering in process: a 1920x1080 layer at density 0.25 (480x270
//! texels), framed by the `full` camera.
//!
//! It returns early without a GPU adapter, unless `CELESTA_REQUIRE_GPU=1`.

use celesta_composition::{
    AssetLocation, BlendMode, EvaluatedTransform, Layer, LayerContent, Point, Rational,
    ResolvedAsset, Scene, Time,
};
use celesta_gpu_renderer::{Color, GpuFrame, GpuRenderError, GpuRenderOptions, GpuRenderer};
use celesta_plugin_vrm::synthetic::SyntheticVrm;
use celesta_plugin_vrm::{PLUGIN, VrmProvider};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use tempfile::TempDir;

/// A 480x270 scene of one plugin layer: the avatar at `path`, 1920x1080
/// layer pixels scaled by 0.25 from the top-left corner, at `time` with
/// `data`.
fn scene(path: &Path, time: f64, data: Value) -> Scene {
    Scene {
        width: 480,
        height: 270,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![Layer {
            id: "avatar".into(),
            transform: EvaluatedTransform {
                anchor: Point { x: 0.0, y: 0.0 },
                scale: Point { x: 0.25, y: 0.25 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Plugin {
                plugin: PLUGIN.into(),
                assets: BTreeMap::from([(
                    "model".to_owned(),
                    ResolvedAsset {
                        id: "avatar".into(),
                        location: AssetLocation::File {
                            path: path.to_str().unwrap().into(),
                        },
                    },
                )]),
                time,
                data,
            },
        }],
        shaders: Vec::new(),
    }
}

/// A renderer onto a transparent background whose plugin layers the VRM
/// provider answers, writing textures to `cache`; `None` without an
/// adapter, unless `CELESTA_REQUIRE_GPU=1`.
fn renderer(cache: &Path) -> Option<GpuRenderer> {
    match GpuRenderer::new(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 0),
        ..GpuRenderOptions::default()
    }) {
        Ok(renderer) => {
            Some(renderer.with_content_provider(VrmProvider::with_texture_cache(cache)))
        }
        Err(GpuRenderError::RequestAdapter(error)) => {
            assert!(
                std::env::var("CELESTA_REQUIRE_GPU").as_deref() != Ok("1"),
                "GPU tests require an adapter: {error}"
            );
            eprintln!("skipping GPU test because no adapter is available: {error}");
            None
        }
        Err(error) => panic!("could not initialize GPU renderer: {error}"),
    }
}

fn pixel(frame: &GpuFrame, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * frame.width() + x) * 4) as usize;
    frame.pixels()[offset..offset + 4].try_into().unwrap()
}

#[test]
fn the_avatar_draws_and_repeats() {
    let directory = TempDir::new().unwrap();
    let cache = TempDir::new().unwrap();
    let path = directory.path().join("avatar.vrm");
    std::fs::write(&path, SyntheticVrm::avatar().glb()).unwrap();
    let Some(mut renderer) = renderer(cache.path()) else {
        return;
    };

    let frame = renderer.render(&scene(&path, 1.0, Value::Null)).unwrap();
    assert_eq!((frame.width(), frame.height()), (480, 270));
    // The `full` camera frames the whole avatar, so the canvas's middle is
    // the Body box, opaque; a corner is empty.
    assert_eq!(
        pixel(&frame, 240, 135)[3],
        255,
        "{:?}",
        pixel(&frame, 240, 135)
    );
    assert_eq!(pixel(&frame, 2, 2), [0; 4]);

    let again = renderer.render(&scene(&path, 1.0, Value::Null)).unwrap();
    assert!(
        frame.pixels() == again.pixels(),
        "frame 1.0 drew differently twice"
    );

    // `happy` binds the Body material's base colour to red.
    let happy = renderer
        .render(&scene(&path, 1.0, json!({ "expressions": { "happy": 1 } })))
        .unwrap();
    assert!(frame.pixels() != happy.pixels(), "happy changed no pixel");
}
