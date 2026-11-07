//! A sample content plugin: a 3D scene of a checkered cube, toon-shaded and
//! outlined by a WGSL material of its own, turning on a turntable under one
//! light, a half-transparent blue pane in front of it, and a leaf cut out of
//! a square, half stuck in the cube. It needs no files: the scene's
//! geometry, textures, and shader are made here.
//!
//! What the layer gives the plugin:
//!
//! - no assets;
//! - `time`: seconds on the layer's clock;
//! - `data`: `{ speed }`, optional: turns of the cube each second, default
//!   0.5.
//!
//! A frame depends on nothing else, as Celesta requires: the cube's turn is
//! a function of the time.
//!
//! The scene follows glTF's conventions: right-handed, +Y up, in metres,
//! the camera looking down its −Z, matrices column-major, texture
//! coordinates from the top-left, and triangles counter-clockwise from the
//! side they face.

use celesta_plugin::{
    AlphaMode, Camera, ColorSpace, ContentFrame, ContentProvider, CustomMaterial, DirectionalLight,
    DrawInfo, DrawState, EmptyTexel, Filter, Geometry, Lights, Material, MaterialKind, ModelId,
    Opened, OutlinePass, PluginError, Projection, Sampler, SceneFrame, SceneModel, SceneShader,
    SceneTexture, ShaderTexture, TextureRef, TextureSource, UnlitMaterial,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::f64::consts::TAU;
use std::path::PathBuf;
use std::sync::Arc;

/// The plugin's id, as its manifest and its layers name it.
pub const PLUGIN: &str = "turntable";

/// The layer's size in pixels.
const CANVAS: [f32; 2] = [640.0, 360.0];
/// Where the camera stands; it looks at the origin, the cube's centre.
const EYE: [f32; 3] = [0.0, 1.2, 3.0];
/// The camera's vertical field of view, in radians.
const YFOV: f32 = 0.8;
/// Turns of the cube each second when `data` does not say.
const SPEED: f64 = 0.5;
/// The leaf texture's size a side.
const LEAF_SIZE: u32 = 64;

/// The model's draws, in `SceneModel::draws` order.
const CUBE: usize = 0;
const PANE: usize = 1;
const LEAF: usize = 2;

type Matrix = [f32; 16];

const IDENTITY: Matrix = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// `a × b`, both column-major.
fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|index| {
        let (column, row) = (index / 4, index % 4);
        (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum()
    })
}

fn translation([x, y, z]: [f32; 3]) -> Matrix {
    let mut matrix = IDENTITY;
    matrix[12..15].copy_from_slice(&[x, y, z]);
    matrix
}

fn scaling([x, y, z]: [f32; 3]) -> Matrix {
    let mut matrix = IDENTITY;
    (matrix[0], matrix[5], matrix[10]) = (x, y, z);
    matrix
}

/// A turn of `angle` radians about +Y: counter-clockwise seen from above.
fn rotation_y(angle: f64) -> Matrix {
    let (sin, cos) = (angle.sin() as f32, angle.cos() as f32);
    let mut matrix = IDENTITY;
    (matrix[0], matrix[2], matrix[8], matrix[10]) = (cos, -sin, sin, cos);
    matrix
}

/// A turn of `angle` radians about +Z: counter-clockwise seen from the front.
fn rotation_z(angle: f32) -> Matrix {
    let (sin, cos) = angle.sin_cos();
    let mut matrix = IDENTITY;
    (matrix[0], matrix[1], matrix[4], matrix[5]) = (cos, sin, -sin, cos);
    matrix
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let length = dot(a, a).sqrt();
    a.map(|value| value / length)
}

/// The view from `eye` looking at the origin with +Y up: world space to
/// the camera's, whose −Z points at the origin.
fn look_at_origin(eye: [f32; 3]) -> Matrix {
    let z = normalize(eye);
    let x = normalize(cross([0.0, 1.0, 0.0], z));
    let y = cross(z, x);
    let mut view = IDENTITY;
    for (row, axis) in [x, y, z].into_iter().enumerate() {
        for (column, value) in axis.into_iter().enumerate() {
            view[column * 4 + row] = value;
        }
        view[12 + row] = -dot(axis, eye);
    }
    view
}

/// A 1 m cube about the origin: four vertices a face, so each face has its
/// own normal and its own texture coordinates, the whole texture across it.
fn cube() -> Geometry {
    // Each face's outward normal, and the directions right and up across
    // it as seen from outside.
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    let mut geometry = Geometry {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        tangents: None,
        joints: None,
        weights: None,
        targets: Vec::new(),
        indices: Vec::new(),
    };
    for (face, (normal, right, up)) in faces.into_iter().enumerate() {
        // Bottom-left, bottom-right, top-right, top-left; `v` counts down.
        for ([across, high], uv) in [
            ([-0.5, -0.5], [0.0, 1.0]),
            ([0.5, -0.5], [1.0, 1.0]),
            ([0.5, 0.5], [1.0, 0.0]),
            ([-0.5, 0.5], [0.0, 0.0]),
        ] {
            geometry.positions.push(std::array::from_fn(|axis| {
                0.5 * normal[axis] + across * right[axis] + high * up[axis]
            }));
            geometry.normals.push(normal);
            geometry.uvs.push(uv);
        }
        let first = 4 * face as u32;
        geometry
            .indices
            .extend([0, 1, 2, 0, 2, 3].map(|corner| first + corner));
    }
    geometry
}

/// A 1 m square about the origin facing +Z.
fn square() -> Geometry {
    Geometry {
        positions: vec![
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        uvs: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        tangents: None,
        joints: None,
        weights: None,
        targets: Vec::new(),
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

/// Two by two sRGB texels, light and orange, sampled without blending so
/// each face shows four sharp squares.
fn checker() -> SceneTexture {
    let (light, orange) = ([235, 235, 235, 255], [230, 110, 40, 255]);
    SceneTexture {
        source: TextureSource::Rgba {
            width: 2,
            height: 2,
            pixels: Arc::new([light, orange, orange, light].concat()),
        },
        color_space: ColorSpace::Srgb,
        sampler: Sampler {
            mag_filter: Filter::Nearest,
            min_filter: Filter::Nearest,
            mipmaps: false,
            ..Sampler::default()
        },
    }
}

/// A green leaf pointing up, with a darker midrib and a short stem, on
/// transparent black texels that a cutoff throws away. Their black never
/// shows: only cut-out and blended materials show the texture, so Celesta
/// builds its mipmaps weighting color by alpha and gives the clear texels
/// the leaf's green, and filtering does not darken the leaf's edge.
fn leaf() -> SceneTexture {
    let (green, rib) = ([80, 160, 60], [50, 110, 40]);
    let size = LEAF_SIZE as f32;
    let pixels = (0..LEAF_SIZE * LEAF_SIZE)
        .flat_map(|texel| {
            let u = ((texel % LEAF_SIZE) as f32 + 0.5) / size;
            // 0 at the bottom row, 1 at the top.
            let height = 1.0 - ((texel / LEAF_SIZE) as f32 + 0.5) / size;
            let off_middle = (u - 0.5).abs();
            // The blade starts above the stem and is widest halfway up it.
            let blade = ((height - 0.12) / 0.88).max(0.0);
            let half_width = 0.42 * (std::f32::consts::PI * blade).sin();
            let (color, alpha) = if off_middle < 0.025 && height > 0.02 {
                (rib, 255)
            } else if off_middle < half_width {
                (green, 255)
            } else {
                ([0, 0, 0], 0)
            };
            [color[0], color[1], color[2], alpha]
        })
        .collect();
    SceneTexture {
        source: TextureSource::Rgba {
            width: LEAF_SIZE,
            height: LEAF_SIZE,
            pixels: Arc::new(pixels),
        },
        color_space: ColorSpace::Srgb,
        sampler: Sampler::default(),
    }
}

fn unlit(alpha_mode: AlphaMode, double_sided: bool, base_color: [f32; 4]) -> Material {
    Material {
        alpha_mode,
        double_sided,
        kind: MaterialKind::Unlit(UnlitMaterial {
            base_color,
            base_color_texture: None,
        }),
    }
}

/// The cube's shader, `toon.wgsl`: the checker toon-shaded under the main
/// light, with a rim and an outline pass. Its one slot is the checker.
fn toon_shader() -> SceneShader {
    SceneShader {
        name: Some("turntable toon".into()),
        wgsl: include_str!("toon.wgsl").into(),
        textures: vec![ShaderTexture {
            name: Some("checker".into()),
            empty: EmptyTexel::White,
        }],
    }
}

/// `toon.wgsl`'s `Params`, four `vec4f`s, 64 bytes.
fn toon_params() -> Vec<u8> {
    [
        // The shade color, a bluish lilac times the checker, and the
        // shading shift.
        [0.4, 0.42, 0.75, -0.05],
        // A faint blue-grey rim; power 8: only faces seen at a slant glow,
        // most where they slant most.
        [0.25, 0.25, 0.32, 8.0],
        // A near-black outline, half mixed towards the surface's color.
        [0.01, 0.008, 0.012, 0.5],
        // A nearly hard step from shade to lit; the outline 2% of the
        // canvas's height where a metre fills it: at the camera's 3.2 m,
        // about 5 of the layer's pixels where the face beyond the
        // silhouette is seen edge-on, fewer as it turns toward the camera.
        [0.95, 0.02, 0.0, 0.0],
    ]
    .as_flattened()
    .iter()
    .flat_map(|value: &f32| value.to_le_bytes())
    .collect()
}

/// The cube's look: the checker toon-shaded, each texel multiplied by a
/// bluish lilac on the side away from the light, a faint blue-grey rim
/// where a face turns away from the camera, and a near-black outline a few
/// pixels wide.
fn toon() -> Material {
    Material {
        alpha_mode: AlphaMode::Opaque,
        double_sided: false,
        kind: MaterialKind::Custom(CustomMaterial {
            shader: 0,
            params: toon_params(),
            textures: vec![Some(0)],
            outline: Some(OutlinePass::default()),
            requires_tangents: false,
        }),
    }
}

/// The plugin's reference lighting: one white light from above, behind,
/// and to the right, and a flat ambient a little blue. Light and ambient
/// add up to about 1 (1.02, and the rim adds to that), so the checker's
/// light squares stay below white, though a lit white texel would not.
fn lights() -> Lights {
    let mut ambient = [[0.0; 3]; 9];
    ambient[3] = [0.22, 0.22, 0.24];
    Lights {
        directional: vec![DirectionalLight {
            direction: [-0.65, -0.65, 0.3],
            color: [1.0; 3],
            intensity: 0.8,
        }],
        ambient,
    }
}

fn textured(mut material: Material, texture: usize) -> Material {
    let MaterialKind::Unlit(unlit) = &mut material.kind else {
        unreachable!("only the turntable's unlit materials take a base color texture");
    };
    unlit.base_color_texture = Some(TextureRef {
        texture,
        transform: None,
    });
    material
}

fn model() -> SceneModel {
    let draw = |geometry, material| DrawInfo {
        geometry,
        material,
        skin: None,
    };
    SceneModel {
        canvas: CANVAS,
        textures: vec![checker(), leaf()],
        geometries: vec![cube(), square()],
        skins: Vec::new(),
        shaders: vec![toon_shader()],
        materials: vec![
            toon(),
            // Linear blue at half alpha, blended over what is behind.
            unlit(AlphaMode::BLEND, false, [0.1, 0.3, 1.0, 0.5]),
            textured(unlit(AlphaMode::Mask { cutoff: 0.5 }, true, [1.0; 4]), 1),
        ],
        // CUBE, PANE, LEAF.
        draws: vec![draw(0, 0), draw(1, 1), draw(1, 2)],
    }
}

/// The plugin's [`ContentProvider`]: what its process serves, and what tests
/// can call in-process. The scene is the same for every model, so a model
/// is only an id.
#[derive(Default)]
pub struct TurntableProvider {
    models: HashSet<ModelId>,
    next: u64,
}

fn failed(message: impl Into<String>) -> PluginError {
    PluginError::Failed {
        plugin: PLUGIN.into(),
        message: message.into(),
    }
}

impl ContentProvider for TurntableProvider {
    fn open(
        &mut self,
        plugin: &str,
        _assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        if plugin != PLUGIN {
            return Err(PluginError::UnknownPlugin(plugin.to_owned()));
        }
        self.next += 1;
        let id = ModelId(self.next);
        self.models.insert(id);
        Ok(Opened {
            model: id,
            content: model().into(),
            notices: vec![],
        })
    }

    fn frame(
        &mut self,
        model: ModelId,
        time: f64,
        data: &Value,
    ) -> Result<ContentFrame, PluginError> {
        if !self.models.contains(&model) {
            return Err(failed(format!("no model {}", model.0)));
        }
        let speed = data["speed"].as_f64().unwrap_or(SPEED);
        let draw = |index, world| DrawState {
            index,
            visible: true,
            world,
            morph_weights: Vec::new(),
            center: None,
            overrides: Default::default(),
            params: None,
        };
        // The pane stands in front of the cube's left half; the leaf leans
        // out of the top of its right half, its stem inside the cube at
        // every turn and its tip above it.
        let pane = multiply(&translation([-0.45, 0.25, 1.0]), &scaling([0.7, 0.7, 1.0]));
        let leaf = [
            translation([0.6, 0.55, 0.2]),
            rotation_z(-0.7),
            scaling([0.6, 1.2, 1.0]),
        ]
        .iter()
        .fold(IDENTITY, |world, step| multiply(&world, step));
        Ok(SceneFrame {
            canvas: None,
            camera: Camera {
                view: look_at_origin(EYE),
                projection: Projection::Perspective {
                    yfov: YFOV,
                    near: 0.1,
                    far: 100.0,
                },
            },
            lights: lights(),
            time,
            draws: vec![
                draw(CUBE, rotation_y(TAU * speed * time)),
                draw(PANE, pane),
                draw(LEAF, leaf),
            ],
            skins: Vec::new(),
        }
        .into())
    }

    fn close(&mut self, model: ModelId) {
        self.models.remove(&model);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_plugin::ContentModel;

    /// Where `matrix` takes the point `p`.
    fn transform(matrix: &Matrix, p: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|row| {
            (0..3).map(|k| matrix[k * 4 + row] * p[k]).sum::<f32>() + matrix[12 + row]
        })
    }

    fn subtract(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|axis| a[axis] - b[axis])
    }

    fn near(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-5)
    }

    fn scene(frame: ContentFrame) -> SceneFrame {
        let ContentFrame::Scene(frame) = frame else {
            panic!("a turntable is a scene");
        };
        frame
    }

    /// Opens a model on `provider`: its id and its scene.
    fn opened(provider: &mut TurntableProvider) -> (ModelId, SceneModel) {
        let opened = provider.open(PLUGIN, &BTreeMap::new()).unwrap();
        let ContentModel::Scene(model) = opened.content else {
            panic!("a turntable is a scene");
        };
        (opened.model, model)
    }

    #[test]
    fn the_cube_faces_out_counter_clockwise() {
        let cube = cube();
        assert_eq!(cube.positions.len(), 24);
        assert_eq!(cube.indices.len(), 36);
        for triangle in cube.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|corner| cube.positions[triangle[corner] as usize]);
            let facing = cross(subtract(b, a), subtract(c, a));
            let centroid: [f32; 3] =
                std::array::from_fn(|axis| (a[axis] + b[axis] + c[axis]) / 3.0);
            assert!(dot(facing, centroid) > 0.0, "{triangle:?} faces in");
            assert_eq!(normalize(facing), cube.normals[triangle[0] as usize]);
        }
        // Every face shows the whole texture, its top row at the top.
        assert_eq!(
            cube.uvs[16..20],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]
        );
    }

    #[test]
    fn the_camera_looks_at_the_origin() {
        let view = look_at_origin(EYE);
        let distance = dot(EYE, EYE).sqrt();
        assert!(near(
            transform(&view, [0.0, 0.0, 0.0]),
            [0.0, 0.0, -distance]
        ));
        assert!(near(transform(&view, EYE), [0.0, 0.0, 0.0]));
        // +X stays to the right; up stays up.
        assert!(transform(&view, [1.0, 0.0, 0.0])[0] > 0.0);
        assert!(transform(&view, [0.0, 1.0, 0.0])[1] > 0.0);
    }

    #[test]
    fn a_quarter_turn_takes_x_to_minus_z() {
        let quarter = rotation_y(TAU / 4.0);
        assert!(near(transform(&quarter, [1.0, 0.0, 0.0]), [0.0, 0.0, -1.0]));
        let leaning = multiply(&translation([1.0, 0.0, 0.0]), &rotation_z(TAU as f32 / 4.0));
        assert!(near(transform(&leaning, [1.0, 0.0, 0.0]), [1.0, 1.0, 0.0]));
    }

    #[test]
    fn the_leaf_is_half_in_the_cube_at_every_turn() {
        let mut provider = TurntableProvider::default();
        let (id, _) = opened(&mut provider);
        let frame = scene(provider.frame(id, 0.0, &Value::Null).unwrap());
        let leaf = &frame.draws[LEAF].world;
        let (stem, tip) = (
            transform(leaf, [0.0, -0.5, 0.0]),
            transform(leaf, [0.0, 0.5, 0.0]),
        );
        // Within 0.5 m of the axis, a point is inside the cube however it
        // has turned; above 0.5 m, it is outside.
        assert!(
            stem[0].hypot(stem[2]) < 0.5 && stem[1].abs() < 0.5,
            "{stem:?}"
        );
        assert!(tip[1] > 0.5, "{tip:?}");
    }

    #[test]
    fn the_cube_turns_speed_times_a_second() {
        let mut provider = TurntableProvider::default();
        let (id, model) = opened(&mut provider);
        assert_eq!(model.canvas, [640.0, 360.0]);
        assert_eq!(model.draws.len(), 3);
        let cube_at = |provider: &mut TurntableProvider, time, data: &Value| {
            scene(provider.frame(id, time, data).unwrap()).draws[CUBE].world
        };
        // Half a turn a second by default: +X is at −X after a second.
        let world = cube_at(&mut provider, 1.0, &Value::Null);
        assert!(near(transform(&world, [1.0, 0.0, 0.0]), [-1.0, 0.0, 0.0]));
        // A quarter turn after a second at 0.25.
        let data = serde_json::json!({ "speed": 0.25 });
        let world = cube_at(&mut provider, 1.0, &data);
        assert!(near(transform(&world, [1.0, 0.0, 0.0]), [0.0, 0.0, -1.0]));
        // Asked again, a time gives the same frame.
        assert_eq!(
            provider.frame(id, 0.7, &data).unwrap(),
            provider.frame(id, 0.7, &data).unwrap()
        );
        provider.close(id);
        assert!(provider.frame(id, 0.0, &data).is_err());
    }

    #[test]
    fn the_leaf_is_cut_out_and_the_rest_kept() {
        let leaf = leaf();
        let TextureSource::Rgba { pixels, .. } = &leaf.source else {
            panic!("the leaf is made here");
        };
        let alpha = |x: u32, y: u32| pixels[((y * LEAF_SIZE + x) * 4 + 3) as usize];
        // Its middle is leaf, its top corners and its stem's sides are not.
        assert_eq!(alpha(LEAF_SIZE / 2, LEAF_SIZE / 2), 255);
        assert_eq!(alpha(LEAF_SIZE / 2, LEAF_SIZE - 2), 255);
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(LEAF_SIZE - 1, 0), 0);
        assert_eq!(alpha(LEAF_SIZE / 4, LEAF_SIZE - 2), 0);
    }

    #[test]
    fn the_cube_is_toon_shaded_by_its_own_shader() {
        let (_, model) = opened(&mut TurntableProvider::default());
        let cube = &model.materials[0];
        assert_eq!(cube.alpha_mode, AlphaMode::Opaque);
        assert!(!cube.double_sided);
        let MaterialKind::Custom(toon) = &cube.kind else {
            panic!("the cube is the sample's own shader: {:?}", cube.kind);
        };
        assert_eq!(toon.shader, 0);
        // The checker is both the lit and the shaded color.
        assert_eq!(toon.textures, [Some(0)]);
        assert_eq!(toon.outline, Some(OutlinePass::default()));
        assert!(!toon.requires_tangents);
        assert_eq!(toon.params.len(), 64);
        // The outline's width, a share of the canvas's height, so it stays
        // a few pixels wide.
        let width = f32::from_le_bytes(toon.params[52..56].try_into().unwrap());
        assert_eq!(width, 0.02);
        assert_eq!(model.shaders.len(), 1);
        assert_eq!(model.shaders[0].textures.len(), 1);
        // The pane and the leaf stay unlit.
        for material in &model.materials[1..] {
            assert!(
                matches!(material.kind, MaterialKind::Unlit(_)),
                "{material:?}"
            );
        }
    }

    #[test]
    fn the_scene_is_lit_by_one_light_and_ambient() {
        let mut provider = TurntableProvider::default();
        let (id, _) = opened(&mut provider);
        for time in [0.0, 1.3] {
            let lights = scene(provider.frame(id, time, &Value::Null).unwrap()).lights;
            assert_eq!(lights.directional.len(), 1, "at {time} s");
            // A flat ambient: the constant coefficient alone.
            for (k, coefficient) in lights.ambient.iter().enumerate() {
                assert_eq!(*coefficient == [0.0; 3], k != 3, "ambient[{k}] at {time} s");
            }
        }
    }

    #[test]
    fn opens_only_its_own_plugin() {
        let error = TurntableProvider::default()
            .open("flag", &BTreeMap::new())
            .unwrap_err();
        assert!(matches!(error, PluginError::UnknownPlugin(_)), "{error}");
    }
}
