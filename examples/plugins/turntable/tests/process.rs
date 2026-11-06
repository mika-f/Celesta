//! The turntable plugin end to end: Celesta's plugin host finds it through a
//! plugin directory, starts its process, and the GPU renderer draws the
//! scene it answers.

use celesta_composition::{
    BlendMode, EvaluatedTransform, Layer, LayerContent, Point, Rational, Scene, Time,
};
use celesta_gpu_renderer::{Color, GpuFrame, GpuRenderError, GpuRenderOptions, GpuRenderer};
use celesta_plugin::{ContentFrame, ContentModel, ContentProvider};
use celesta_plugin_host::PluginHost;
use celesta_plugin_turntable::{PLUGIN, TurntableProvider};
use std::collections::BTreeMap;
use std::path::Path;

/// A plugin directory holding this build's executable.
fn plugin_directory() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let plugin = directory.path().join("turntable");
    std::fs::create_dir(&plugin).unwrap();
    // A manifest names files inside its directory only.
    let executable = Path::new(env!("CARGO_BIN_EXE_celesta-plugin-turntable"));
    let name = executable.file_name().unwrap().to_str().unwrap();
    std::fs::copy(executable, plugin.join(name)).unwrap();
    let (os, arch) = celesta_plugin_host::platform();
    std::fs::write(
        plugin.join("celesta-plugin.json"),
        serde_json::json!({
            "id": PLUGIN,
            "version": "0.1.0",
            "protocol": 1,
            "celesta": format!("={}", celesta_plugin_host::CELESTA_VERSION),
            "executable": { format!("{os}-{arch}"): name },
        })
        .to_string(),
    )
    .unwrap();
    directory
}

#[test]
fn the_process_answers_as_the_provider_does() {
    let directory = plugin_directory();
    let mut host = PluginHost::discover(&[directory.path().to_path_buf()]);
    let mut provider = TurntableProvider::default();
    let remote = host.open(PLUGIN, &BTreeMap::new()).unwrap();
    let local = provider.open(PLUGIN, &BTreeMap::new()).unwrap();
    assert!(
        matches!(remote.content, ContentModel::Scene(_)),
        "{:?}",
        remote.content
    );
    assert_eq!(remote.content, local.content);
    assert_eq!(remote.notices, local.notices);
    let (remote, local) = (remote.model, local.model);
    let data = serde_json::json!({ "speed": 0.25 });
    // Out of order, as a renderer may ask.
    for time in [2.0, 0.0, 2.0] {
        let frame = host.frame(remote, time, &data).unwrap();
        assert!(matches!(frame, ContentFrame::Scene(_)), "{frame:?}");
        assert_eq!(frame, provider.frame(local, time, &data).unwrap());
    }
}

fn scene(time: f64) -> Scene {
    Scene {
        width: 640,
        height: 360,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![Layer {
            id: "turntable".into(),
            transform: EvaluatedTransform {
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Plugin {
                plugin: PLUGIN.into(),
                assets: BTreeMap::new(),
                time,
                data: serde_json::Value::Null,
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
fn the_renderer_draws_the_process_scene() {
    let directory = plugin_directory();
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

    // The camera looks at the cube's centre, so the canvas's centre is the
    // cube, opaque at any turn; the corners are empty.
    for time in [0.0, 0.3] {
        let frame = renderer.render(&scene(time)).unwrap();
        assert_eq!(pixel(&frame, 320, 180)[3], 255, "at {time} s");
        assert_eq!(pixel(&frame, 2, 2), [0; 4], "at {time} s");
    }
}
