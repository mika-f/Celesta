//! The VRM Consortium's and pixiv's official sample models, opened, framed,
//! and drawn. They are never committed: each test reads the folder
//! `CELESTA_VRM_SAMPLES_DIR` names (holding `Seed-san.vrm`,
//! `VRM1_Constraint_Twist_Sample.vrm`, and
//! `VRMC_materials_mtoon_UV_Animation_Test.vrm`), and passes, saying it
//! skipped, without it.
//!
//! The drawing test returns early without a GPU adapter, unless
//! `CELESTA_REQUIRE_GPU=1`.

use celesta_composition::{
    AssetLocation, BlendMode, EvaluatedTransform, Layer, LayerContent, Point, Rational,
    ResolvedAsset, Scene, Time,
};
use celesta_gpu_renderer::{Color, GpuFrame, GpuRenderError, GpuRenderOptions, GpuRenderer};
use celesta_plugin::{Lights, NoticeLevel, Projection, SceneFrame};
use celesta_plugin_vrm::{HumanBone, PLUGIN, PerformanceData, VrmModel, VrmProvider};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The sample `name` in `CELESTA_VRM_SAMPLES_DIR`; `None`, having said the
/// test is skipped, without the variable.
fn sample(name: &str) -> Option<PathBuf> {
    let Some(directory) = std::env::var_os("CELESTA_VRM_SAMPLES_DIR") else {
        println!("skipped: CELESTA_VRM_SAMPLES_DIR is not set");
        return None;
    };
    let path = Path::new(&directory).join(name);
    assert!(path.is_file(), "{} is not a file", path.display());
    Some(path)
}

/// `path` opened with its textures written to `cache`, never the user's
/// texture cache.
fn load(path: &Path, cache: &TempDir) -> VrmModel {
    VrmModel::load_with_cache(path, cache.path())
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn data(value: Value) -> PerformanceData {
    serde_json::from_value(value).unwrap()
}

/// Every number `frame` sends, but its draws' parameter bytes, which are
/// MToon's `f32`s (see `assert_finite`).
fn numbers(frame: &SceneFrame) -> Vec<f32> {
    let mut numbers = Vec::new();
    numbers.extend(frame.canvas.into_iter().flatten());
    numbers.extend(frame.camera.view);
    match frame.camera.projection {
        Projection::Perspective { yfov, near, far } => numbers.extend([yfov, near, far]),
        Projection::Orthographic { height, near, far } => numbers.extend([height, near, far]),
    }
    let Lights {
        directional,
        ambient,
    } = &frame.lights;
    for light in directional {
        numbers.extend(light.direction);
        numbers.extend(light.color);
        numbers.push(light.intensity);
    }
    numbers.extend(ambient.iter().flatten());
    for draw in &frame.draws {
        numbers.extend(draw.world);
        numbers.extend(&draw.morph_weights);
        numbers.extend(draw.center.into_iter().flatten());
        numbers.extend(draw.overrides.base_color.into_iter().flatten());
        numbers.extend(draw.overrides.texture_offset.into_iter().flatten());
        numbers.extend(draw.overrides.texture_scale.into_iter().flatten());
    }
    numbers.extend(frame.skins.iter().flatten().flatten());
    numbers
}

/// Every number in `frame` is finite, its MToon parameters (little-endian
/// `f32`s) too.
fn assert_finite(frame: &SceneFrame) {
    assert!(frame.time.is_finite());
    let numbers = numbers(frame);
    assert!(!numbers.is_empty());
    if let Some(number) = numbers.iter().find(|number| !number.is_finite()) {
        panic!("frame at {} sends {number}", frame.time);
    }
    for draw in &frame.draws {
        for chunk in draw.params.iter().flatten().collect::<Vec<_>>().chunks(4) {
            let value = f32::from_le_bytes([*chunk[0], *chunk[1], *chunk[2], *chunk[3]]);
            assert!(
                value.is_finite(),
                "draw {}'s parameters send {value}",
                draw.index
            );
        }
    }
}

#[test]
fn seed_san_opens() {
    let Some(path) = sample("Seed-san.vrm") else {
        return;
    };
    let cache = TempDir::new().unwrap();
    let mut model = load(&path, &cache);
    assert!(!model.scene_model().draws.is_empty());
    let notices = model.notices();
    assert_eq!(notices[0].level, NoticeLevel::Info, "{notices:?}");
    assert!(
        notices[0].message.contains("https://vrm.dev/licenses/1.0/"),
        "{notices:?}"
    );
    assert!(
        notices
            .iter()
            .any(|notice| notice.message.contains("drawn as unlit base colour")),
        "{notices:?}"
    );

    // Out of order, as a renderer may ask, a frame is what playing the
    // times in order on a fresh model gives.
    let data = data(json!({ "expressions": { "blink": 1, "aa": 0.5 } }));
    let mut fresh = load(&path, &cache);
    let in_order: BTreeMap<u64, SceneFrame> = [0.0, 1.5, 3.0]
        .into_iter()
        .map(|time: f64| (time.to_bits(), fresh.frame(time, &data).unwrap()))
        .collect();
    for time in [3.0, 0.0, 1.5, 3.0] {
        let frame = model.frame(time, &data).unwrap();
        assert_finite(&frame);
        assert!(
            frame == in_order[&time.to_bits()],
            "the frame at {time} s differs"
        );
    }
}

#[test]
fn constraint_twist_sample_opens_with_bone_look_at() {
    let Some(path) = sample("VRM1_Constraint_Twist_Sample.vrm") else {
        return;
    };
    let cache = TempDir::new().unwrap();
    let mut model = load(&path, &cache);
    let eyes = [HumanBone::LeftEye, HumanBone::RightEye].map(|bone| {
        model
            .humanoid()
            .node(bone)
            .expect("the sample has eye bones")
    });
    // An eye's world reaches the frame as the skinning matrices of the
    // joints it is.
    let gltf = gltf::Gltf::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let joints: Vec<(usize, usize)> = gltf
        .skins()
        .flat_map(|skin| {
            skin.joints()
                .enumerate()
                .filter(|(_, joint)| eyes.contains(&joint.index()))
                .map(move |(joint, _)| (skin.index(), joint))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        !joints.is_empty(),
        "no skin has the eyes {eyes:?} as joints"
    );

    let rest = model.frame(0.0, &PerformanceData::default()).unwrap();
    let looking = model
        .frame(0.0, &data(json!({ "lookAt": { "yaw": 20, "pitch": 0 } })))
        .unwrap();
    assert_finite(&looking);
    for (skin, joint) in joints {
        assert_ne!(
            rest.skins[skin][joint], looking.skins[skin][joint],
            "skin {skin}'s joint {joint} (an eye) did not turn"
        );
    }
}

#[test]
fn uv_animation_sample_keeps_its_speeds() {
    let Some(path) = sample("VRMC_materials_mtoon_UV_Animation_Test.vrm") else {
        return;
    };
    let cache = TempDir::new().unwrap();
    let model = load(&path, &cache);
    // Materials ScrollX, ScrollY, and ScrollRot each animate at speed 1 in
    // one way. The file has no mask texture, so the whole surface moves.
    for (material, speeds) in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .into_iter()
        .enumerate()
    {
        let mtoon = model
            .mtoon(material)
            .unwrap_or_else(|| panic!("material {material} is MToon"));
        assert_eq!(
            [
                mtoon.uv_animation_scroll_x_speed_factor,
                mtoon.uv_animation_scroll_y_speed_factor,
                mtoon.uv_animation_rotation_speed_factor,
            ],
            speeds,
            "material {material}"
        );
        assert_eq!(mtoon.uv_animation_mask_texture, None, "material {material}");
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

/// A 480x270 scene of the avatar at `path`: a 1920x1080 layer scaled by
/// 0.25 from the top-left corner, at time 0 with `data`.
fn scene(path: &Path, data: Value) -> Scene {
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
                time: 0.0,
                data,
            },
        }],
        shaders: Vec::new(),
    }
}

fn alpha(frame: &GpuFrame, x: u32, y: u32) -> u8 {
    frame.pixels()[((y * frame.width() + x) * 4 + 3) as usize]
}

#[test]
fn seed_san_draws() {
    let Some(path) = sample("Seed-san.vrm") else {
        return;
    };
    let cache = TempDir::new().unwrap();
    let Some(mut renderer) = renderer(cache.path()) else {
        return;
    };
    let frame = renderer
        .render(&scene(&path, json!({ "camera": "bust" })))
        .unwrap();
    let (width, height) = (frame.width(), frame.height());
    let opaque = (height / 3..2 * height / 3)
        .flat_map(|y| (width / 3..2 * width / 3).map(move |x| (x, y)))
        .filter(|&(x, y)| alpha(&frame, x, y) == 255)
        .count();
    assert!(
        opaque > 0,
        "nothing opaque in the bust frame's middle third"
    );
}
