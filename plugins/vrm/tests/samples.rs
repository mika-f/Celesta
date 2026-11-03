//! The VRM Consortium's and pixiv's official sample models, opened, framed,
//! and drawn. They are never committed: each test reads the folder
//! `CELESTA_VRM_SAMPLES_DIR` names (holding `Seed-san.vrm`,
//! `VRM1_Constraint_Twist_Sample.vrm`,
//! `VRMC_materials_mtoon_UV_Animation_Test.vrm`, and the VRM animation
//! `test.vrma`), and passes, saying it skipped, without it.
//!
//! A frame sends no node's pose, only skinning matrices, so the tests read
//! a joint's world back from its matrix and the file's inverse bind matrix.
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
use glam::{Mat4, Quat};
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

/// What a test needs of a sample's glTF to read a frame's skinning
/// matrices back: each node's parent and rest pose, and each skin joint.
struct Skeleton {
    parents: Vec<Option<usize>>,
    /// Each node's rest local rotation.
    rest: Vec<Quat>,
    /// Each node's rest world, in the default scene's root space.
    rest_worlds: Vec<Mat4>,
    /// (skin, joint, node, inverse bind matrix), every skin's joints.
    joints: Vec<(usize, usize, usize, Mat4)>,
}

impl Skeleton {
    fn read(path: &Path) -> Self {
        let gltf = gltf::Gltf::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let count = gltf.nodes().len();
        let mut parents = vec![None; count];
        for node in gltf.nodes() {
            for child in node.children() {
                parents[child.index()] = Some(node.index());
            }
        }
        let rest = gltf
            .nodes()
            .map(|node| Quat::from_array(node.transform().decomposed().1))
            .collect();
        let mut rest_worlds = vec![Mat4::IDENTITY; count];
        let scene = gltf
            .default_scene()
            .or_else(|| gltf.scenes().next())
            .unwrap();
        let mut stack: Vec<(gltf::Node, Mat4)> =
            scene.nodes().map(|node| (node, Mat4::IDENTITY)).collect();
        while let Some((node, parent)) = stack.pop() {
            let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
            rest_worlds[node.index()] = world;
            stack.extend(node.children().map(|child| (child, world)));
        }
        let mut joints = Vec::new();
        for skin in gltf.skins() {
            let reader = skin.reader(|_| gltf.blob.as_deref());
            let binds: Vec<Mat4> = match reader.read_inverse_bind_matrices() {
                Some(binds) => binds.map(|bind| Mat4::from_cols_array_2d(&bind)).collect(),
                None => vec![Mat4::IDENTITY; skin.joints().len()],
            };
            for ((joint, node), bind) in skin.joints().enumerate().zip(binds) {
                joints.push((skin.index(), joint, node.index(), bind));
            }
        }
        Self {
            parents,
            rest,
            rest_worlds,
            joints,
        }
    }

    /// Every skin joint's world in `frame`: its skinning matrix times the
    /// inverse of its inverse bind matrix.
    fn worlds(&self, frame: &SceneFrame) -> BTreeMap<usize, Mat4> {
        self.joints
            .iter()
            .map(|&(skin, joint, node, bind)| {
                let matrix = Mat4::from_cols_array(&frame.skins[skin][joint]);
                (node, matrix * bind.inverse())
            })
            .collect()
    }

    /// The local rotation in `frame` of every skin joint whose parent is a
    /// skin joint too.
    fn locals(&self, frame: &SceneFrame) -> BTreeMap<usize, Quat> {
        let worlds = self.worlds(frame);
        let rotation = |world: &Mat4| world.to_scale_rotation_translation().1;
        worlds
            .iter()
            .filter_map(|(&node, world)| {
                let parent = worlds.get(&self.parents[node]?)?;
                Some((node, rotation(parent).inverse() * rotation(world)))
            })
            .collect()
    }
}

/// The angle in degrees between two rotations.
fn degrees(a: Quat, b: Quat) -> f32 {
    a.angle_between(b).to_degrees()
}

/// A track of one key at 0 s and another at 1 s, frame-rate timed.
fn ramp(from: f64, to: f64) -> Value {
    json!({
        "type": "keyframes",
        "keyframes": [
            { "time": { "value": 0, "timescale": 30 }, "value": from },
            { "time": { "value": 30, "timescale": 30 }, "value": to },
        ],
    })
}

#[test]
fn twist_sample_constraints_follow_their_sources() {
    let Some(path) = sample("VRM1_Constraint_Twist_Sample.vrm") else {
        return;
    };
    let cache = TempDir::new().unwrap();
    let mut model = load(&path, &cache);
    let skeleton = Skeleton::read(&path);
    let gltf = gltf::Gltf::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    // The nodes a roll constraint turns, by their rolls' weights.
    let rolls: BTreeMap<usize, f32> = gltf
        .nodes()
        .filter_map(|node| {
            let roll = node.extension_value("VRMC_node_constraint")?["constraint"].get("roll")?;
            Some((node.index(), roll["weight"].as_f64().unwrap_or(1.0) as f32))
        })
        .collect();
    assert!(!rolls.is_empty(), "the sample has roll constraints");

    // Each roll node's turn at 1 s with `tracks`, from where it is at 1 s
    // without them: the sample's spring bones swing alike in both, so only
    // the constraints differ.
    let still = skeleton.locals(&model.frame(1.0, &PerformanceData::default()).unwrap());
    let turned = |model: &mut VrmModel, tracks: Value| {
        let frame = model
            .frame(1.0, &data(json!({ "tracks": tracks })))
            .unwrap();
        assert_finite(&frame);
        let locals = skeleton.locals(&frame);
        rolls
            .keys()
            .map(|&node| (node, degrees(still[&node], locals[&node])))
            .collect::<BTreeMap<usize, f32>>()
    };

    // A roll track of +90 tilts the T-posed left forearm (along +X) up to
    // +Y in the body's plane: a swing, with no twist about any roll's axis,
    // so no roll turns.
    let swing = turned(
        &mut model,
        json!({ "leftLowerArm": { "roll": ramp(0.0, 90.0) } }),
    );
    assert!(
        swing.values().all(|&angle| angle < 0.1),
        "a swing turned a roll: {swing:?}"
    );

    // A pitch of +90 twists the left hand about the arm's length (Rx(−90°),
    // its front, +Z, turning up), which its twist bones follow
    // (`J_Roll_L_Hand` by weight 1, `J_Roll_L_LowerArm` by 0.5); a yaw of
    // +90 bends the elbow about Y (Ry(−90°), the forearm from +X to +Z,
    // forward), which `J_Roll_L_Elbow` follows by 0.5. The angles are
    // unsigned, so the expected turns are 90° times each weight. The right
    // arm's rolls stay put.
    let twist = turned(
        &mut model,
        json!({ "leftHand": { "pitch": ramp(0.0, 90.0) } }),
    );
    let bend = turned(
        &mut model,
        json!({ "leftLowerArm": { "yaw": ramp(0.0, 90.0) } }),
    );
    let name = |node: usize| gltf.nodes().nth(node).unwrap().name().unwrap().to_owned();
    let mut followed = 0;
    for (node, weight) in &rolls {
        let (expected_twist, expected_bend) = match name(*node).as_str() {
            "J_Roll_L_Hand" | "J_Roll_L_LowerArm" => (90.0 * weight, 0.0),
            "J_Roll_L_Elbow" => (0.0, 90.0 * weight),
            _ => (0.0, 0.0),
        };
        followed += usize::from(expected_twist + expected_bend > 0.0);
        for (turns, expected) in [(&twist, expected_twist), (&bend, expected_bend)] {
            assert!(
                (turns[node] - expected).abs() < 0.5,
                "{} turned {}°, not {expected}°",
                name(*node),
                turns[node]
            );
        }
    }
    assert_eq!(followed, 3, "the sample's left-arm rolls: {rolls:?}");
}

#[test]
fn vrma_moves_seed_san() {
    let (Some(path), Some(clip)) = (sample("Seed-san.vrm"), sample("test.vrma")) else {
        return;
    };
    let cache = TempDir::new().unwrap();
    let assets: BTreeMap<String, PathBuf> = [
        ("model".to_owned(), path.clone()),
        ("motion0".to_owned(), clip),
    ]
    .into();
    let open = || VrmModel::open_with_cache(&assets, cache.path()).unwrap();
    let skeleton = Skeleton::read(&path);
    // The clip, looped, with the head swaying: the clip moves only the right
    // arm, and Seed-san's springs have no gravity and swing in the hips'
    // space, so without the sway her hair would hang still.
    let sway = json!({
        "type": "keyframes",
        "keyframes": [
            { "time": { "value": 0, "timescale": 30 }, "value": 0 },
            { "time": { "value": 30, "timescale": 30 }, "value": 30 },
            { "time": { "value": 60, "timescale": 30 }, "value": -30 },
            { "time": { "value": 90, "timescale": 30 }, "value": 0 },
        ],
    });
    let played = data(json!({
        "motions": [{ "clip": "motion0", "loop": true }],
        "tracks": { "head": { "yaw": sway } },
    }));

    // The clip raises the right arm by its one key away from rest, at
    // 0.5 s (Rx(90°); at 1 s and after it rests again), so at 0.5 s a
    // humanoid bone has turned.
    let mut model = open();
    let frame = model.frame(0.5, &played).unwrap();
    assert_finite(&frame);
    let locals = skeleton.locals(&frame);
    let turned = model
        .humanoid()
        .bones
        .values()
        .filter(|node| {
            locals
                .get(node)
                .is_some_and(|&local| degrees(skeleton.rest[**node], local) > 1.0)
        })
        .count();
    assert!(turned > 0, "no humanoid bone turned more than 1° at 0.5 s");
    let arm = model.humanoid().required(HumanBone::RightUpperArm);
    let raised = degrees(skeleton.rest[arm], locals[&arm]);
    assert!(
        (raised - 90.0).abs() < 0.5,
        "rightUpperArm turned {raised}°"
    );

    // Out of order on a model another performance warmed, the frames equal
    // fresh frames played in order, spring bones included.
    let mut warmed = open();
    let looking_up = data(json!({
        "motions": [{ "clip": "motion0", "loop": true }],
        "tracks": { "head": { "pitch": ramp(0.0, 20.0) } },
    }));
    warmed.frame(2.5, &looking_up).unwrap();
    let mut fresh = open();
    let in_order: BTreeMap<u64, SceneFrame> = [0.5, 2.0, 3.0]
        .into_iter()
        .map(|time: f64| (time.to_bits(), fresh.frame(time, &played).unwrap()))
        .collect();
    for time in [3.0, 0.5, 2.0, 0.5] {
        let frame = warmed.frame(time, &played).unwrap();
        assert_finite(&frame);
        assert!(
            frame == in_order[&time.to_bits()],
            "the frame at {time} s differs"
        );
    }

    // At 2 s the head swings back through the middle, so the hair trails
    // it: a spring joint is away from its rest, and one is turned from its
    // rest within its parent, which only the springs do.
    let gltf = gltf::Gltf::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let springs = gltf.extension_value("VRMC_springBone").unwrap()["springs"]
        .as_array()
        .unwrap();
    let joints: Vec<usize> = springs
        .iter()
        .flat_map(|spring| spring["joints"].as_array().unwrap())
        .map(|joint| joint["node"].as_u64().unwrap() as usize)
        .collect();
    let swinging = &in_order[&2.0f64.to_bits()];
    let (worlds, locals) = (skeleton.worlds(swinging), skeleton.locals(swinging));
    let position = |world: &Mat4| world.to_scale_rotation_translation().2;
    let moved = joints.iter().any(|node| {
        worlds.get(node).is_some_and(|world| {
            position(world).distance(position(&skeleton.rest_worlds[*node])) > 1e-3
        })
    });
    assert!(moved, "no spring joint moved from its rest at 2 s");
    let swung = joints.iter().any(|node| {
        locals
            .get(node)
            .is_some_and(|&local| degrees(skeleton.rest[*node], local) > 1.0)
    });
    assert!(swung, "no spring joint turned more than 1° at 2 s");
}

/// The model's spring bones step the whole four hours the plugin steps
/// any avatar's: no notice says they reach less.
fn assert_steps_four_hours(model: &VrmModel) {
    let notices = model.notices();
    assert!(
        !notices
            .iter()
            .any(|notice| notice.message.contains("spring bones are stepped for")),
        "{notices:?}"
    );
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
    assert_steps_four_hours(&model);

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
    // 171 nodes, 57 moving joints, 529 joint–collider pairs, and aims.
    assert_steps_four_hours(&model);
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
