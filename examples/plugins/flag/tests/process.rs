//! The flag plugin end to end: Celesta's plugin host finds it through a
//! plugin directory, starts its process, and the GPU renderer draws what it
//! answers.

use celesta_composition::{
    AssetLocation, BlendMode, EvaluatedTransform, Layer, LayerContent, Point, Rational,
    ResolvedAsset, Scene, Time,
};
use celesta_gpu_renderer::{Color, GpuFrame, GpuRenderError, GpuRenderOptions, GpuRenderer};
use celesta_plugin::ContentProvider;
use celesta_plugin_flag::{FlagProvider, PLUGIN};
use celesta_plugin_host::PluginHost;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A plugin directory whose manifest names this build's executable, and a
/// 160x100 image: red on the left half, blue on the right.
fn plugin_directory() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let plugin = directory.path().join("flag");
    std::fs::create_dir(&plugin).unwrap();
    let (os, arch) = celesta_plugin_host::platform();
    std::fs::write(
        plugin.join("celesta-plugin.json"),
        serde_json::json!({
            "id": PLUGIN,
            "version": "0.1.0",
            "protocol": 1,
            "executable": { format!("{os}-{arch}"): env!("CARGO_BIN_EXE_celesta-plugin-flag") },
        })
        .to_string(),
    )
    .unwrap();
    let image = directory.path().join("flag.png");
    image::RgbaImage::from_fn(160, 100, |x, _| {
        if x < 80 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 0, 255, 255])
        }
    })
    .save(&image)
    .unwrap();
    (directory, image)
}

#[test]
fn the_process_answers_as_the_provider_does() {
    let (directory, image) = plugin_directory();
    let assets = BTreeMap::from([("image".to_owned(), image)]);
    let mut host = PluginHost::discover(&[directory.path().to_path_buf()]);
    let mut provider = FlagProvider::default();
    let (remote, remote_model) = host.open(PLUGIN, &assets).unwrap();
    let (local, local_model) = provider.open(PLUGIN, &assets).unwrap();
    assert_eq!(remote_model, local_model);
    let data = serde_json::json!({ "amplitude": 0.1, "speed": 2 });
    // Out of order, as a renderer may ask.
    for time in [1.5, 0.0, 0.75, 1.5] {
        assert_eq!(
            host.frame(remote, time, &data).unwrap(),
            provider.frame(local, time, &data).unwrap()
        );
    }
    let error = host.open(PLUGIN, &BTreeMap::new()).unwrap_err();
    assert!(error.to_string().contains("`image`"), "{error}");
}

fn scene(image: &Path, time: f64) -> Scene {
    let asset = ResolvedAsset {
        id: "flag.png".into(),
        location: AssetLocation::File {
            path: image.to_string_lossy().into_owned(),
        },
    };
    Scene {
        width: 160,
        height: 130,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![Layer {
            id: "flag".into(),
            transform: EvaluatedTransform {
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Plugin {
                plugin: PLUGIN.into(),
                assets: BTreeMap::from([("image".to_owned(), asset)]),
                time,
                data: serde_json::json!({ "amplitude": 0.15, "shading": 0 }),
            },
        }],
        shaders: Vec::new(),
    }
}

fn pixel(frame: &GpuFrame, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * frame.width() + x) * 4) as usize;
    frame.pixels()[offset..offset + 4].try_into().unwrap()
}

#[test]
fn the_renderer_draws_the_process_meshes() {
    let (directory, image) = plugin_directory();
    let renderer = match GpuRenderer::new(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 0),
        ..GpuRenderOptions::default()
    }) {
        Ok(renderer) => renderer,
        Err(GpuRenderError::RequestAdapter(error)) => {
            assert!(
                std::env::var("CELESTA_REQUIRE_GPU").as_deref() != Ok("1"),
                "GPU tests require an adapter: {error}"
            );
            eprintln!("skipping GPU test because no adapter is available: {error}");
            return;
        }
        Err(error) => panic!("could not initialize GPU renderer: {error}"),
    };
    let mut renderer =
        renderer.with_content_provider(PluginHost::discover(&[directory.path().to_path_buf()]));

    let still = renderer.render(&scene(&image, 0.0)).unwrap();
    // The image sits between 15 px of room above and below; by the pole it
    // barely moves.
    for (y, expected) in [(50, [255, 0, 0, 255]), (5, [0; 4]), (124, [0; 4])] {
        assert!(
            near(pixel(&still, 4, y), expected),
            "{:?} at y {y}",
            pixel(&still, 4, y)
        );
    }

    // The free edge swings: a column near it changes over time.
    let later = renderer.render(&scene(&image, 0.3)).unwrap();
    let column = |frame: &GpuFrame| (0..130).map(|y| pixel(frame, 150, y)).collect::<Vec<_>>();
    assert_ne!(column(&still), column(&later));
    assert!(
        column(&later)
            .iter()
            .any(|&color| near(color, [0, 0, 255, 255]))
    );
}

fn near(actual: [u8; 4], expected: [u8; 4]) -> bool {
    actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2)
}
