//! The plugin's MToon drawn by Celesta's GPU renderer: Celesta's former
//! built-in MToon tests, moved here with the material, with the same
//! expected pixels. A provider made up here answers one fixed scene, its
//! MToon materials custom materials on the plugin's MToon shader, their
//! parameters packed by the plugin at the frame's time.
//!
//! They return early without a GPU adapter, unless `CELESTA_REQUIRE_GPU=1`.

use celesta_composition::{
    BlendMode, EvaluatedTransform, Layer, LayerContent, Point, Rational, Scene, Time,
};
use celesta_gpu_renderer::{Color, GpuFrame, GpuRenderError, GpuRenderOptions, GpuRenderer};
use celesta_plugin::{
    AlphaMode, Camera, ColorSpace, ContentFrame, ContentProvider, DirectionalLight, DrawInfo,
    DrawState, Filter, Geometry, Lights, MaterialKind, ModelId, MorphTarget, Opened, PluginError,
    Projection, Sampler, SceneFrame, SceneModel, SceneTexture, TextureRef, TextureSource,
    TextureTransform, UnlitMaterial, Wrap,
};
use celesta_plugin_vrm::mtoon::{
    self, Mtoon, MtoonOverrides, OutlineWidthMode, ScaledTextureSlot, TextureSlot,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// The plugin the test layer names.
const PLUGIN: &str = "scenes";

/// A test scene's material: Celesta's unlit one of a colour, or the
/// plugin's MToon.
#[derive(Clone, Debug)]
struct Material {
    alpha_mode: AlphaMode,
    double_sided: bool,
    kind: Kind,
}

#[derive(Clone, Debug)]
enum Kind {
    Unlit([f32; 4]),
    Mtoon(Box<Mtoon>),
}

/// One scene to draw: `model` (its materials and shaders made from
/// `materials` when it is drawn) and `frame`, whose MToon draws pack their
/// material's parameters at the frame's time with `binds` (by frame draw).
struct Scenes {
    model: SceneModel,
    materials: Vec<Material>,
    frame: SceneFrame,
    binds: Vec<MtoonOverrides>,
}

/// Answers every open with `model` and every frame with `frame`.
struct FixedScene {
    model: SceneModel,
    frame: SceneFrame,
}

impl ContentProvider for FixedScene {
    fn open(
        &mut self,
        plugin: &str,
        assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        if plugin != PLUGIN {
            return Err(PluginError::UnknownPlugin(plugin.to_owned()));
        }
        assert!(assets.is_empty());
        Ok(Opened {
            model: ModelId(9),
            content: self.model.clone().into(),
            notices: vec![],
        })
    }

    fn frame(
        &mut self,
        model: ModelId,
        _time: f64,
        _data: &serde_json::Value,
    ) -> Result<ContentFrame, PluginError> {
        assert_eq!(model, ModelId(9));
        Ok(self.frame.clone().into())
    }
}

impl Scenes {
    /// The scene as the plugin sends it: an MToon material is a custom
    /// material on the MToon shader, shader 0, and each of its draws sends
    /// its parameters packed at the frame's time with the draw's binds.
    fn fixed(self) -> FixedScene {
        let Scenes {
            mut model,
            materials,
            mut frame,
            binds,
        } = self;
        let uses_mtoon = materials
            .iter()
            .any(|material| matches!(material.kind, Kind::Mtoon(_)));
        model.shaders = if uses_mtoon {
            vec![mtoon::shader()]
        } else {
            vec![]
        };
        model.materials = materials
            .iter()
            .map(|material| celesta_plugin::Material {
                alpha_mode: material.alpha_mode,
                double_sided: material.double_sided,
                kind: match &material.kind {
                    Kind::Unlit(color) => MaterialKind::Unlit(UnlitMaterial {
                        base_color: *color,
                        base_color_texture: None,
                    }),
                    Kind::Mtoon(mtoon) => {
                        MaterialKind::Custom(mtoon::custom_material(mtoon, 0, &model.textures))
                    }
                },
            })
            .collect();
        let time = frame.time;
        for (position, draw) in frame.draws.iter_mut().enumerate() {
            let material = model.draws.get(draw.index).map(|info| info.material);
            if let Some(Kind::Mtoon(mtoon)) = material
                .and_then(|material| materials.get(material))
                .map(|material| &material.kind)
            {
                let binds = binds.get(position).copied().unwrap_or_default();
                draw.params = Some(mtoon::params(mtoon, &binds, time, &model.textures).to_vec());
            }
        }
        FixedScene { model, frame }
    }
}

/// A 1 m square at z = 0 facing +Z, with one morph target moving it +0.5 m
/// in x.
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
        targets: vec![MorphTarget {
            positions: vec![[0.5, 0.0, 0.0]; 4],
            normals: vec![],
        }],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

fn translation(x: f32, y: f32, z: f32) -> [f32; 16] {
    let mut matrix = IDENTITY;
    matrix[12] = x;
    matrix[13] = y;
    matrix[14] = z;
    matrix
}

fn scaling(x: f32, y: f32, z: f32) -> [f32; 16] {
    let mut matrix = IDENTITY;
    matrix[0] = x;
    matrix[5] = y;
    matrix[10] = z;
    matrix
}

/// An opaque, single-sided unlit material of `color`, without a texture.
fn plain(color: [f32; 4]) -> Material {
    Material {
        alpha_mode: AlphaMode::Opaque,
        double_sided: false,
        kind: Kind::Unlit(color),
    }
}

/// A 64x64 scene of one white square (draw 0, unskinned, with `square`'s
/// morph target), seen from 2 m away through an orthographic camera 2 m
/// high: the square covers the central 32x32 pixels.
fn one_square() -> Scenes {
    Scenes {
        model: SceneModel {
            canvas: [64.0, 64.0],
            textures: vec![],
            geometries: vec![square()],
            skins: vec![],
            shaders: vec![],
            materials: vec![],
            draws: vec![DrawInfo {
                geometry: 0,
                material: 0,
                skin: None,
            }],
        },
        materials: vec![plain([1.0; 4])],
        frame: SceneFrame {
            camera: Camera {
                view: translation(0.0, 0.0, -2.0),
                projection: Projection::Orthographic {
                    height: 2.0,
                    near: 0.1,
                    far: 100.0,
                },
            },
            lights: Lights::default(),
            time: 0.0,
            draws: vec![DrawState {
                index: 0,
                visible: true,
                world: IDENTITY,
                morph_weights: vec![0.0],
                center: None,
                overrides: Default::default(),
                params: None,
            }],
            skins: vec![],
        },
        binds: vec![],
    }
}

/// A `width` by `height` scene of the plugin's layer, at time 0, unscaled.
fn scene_of(width: u32, height: u32) -> Scene {
    Scene {
        width,
        height,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![Layer {
            id: PLUGIN.into(),
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
                time: 0.0,
                data: serde_json::Value::Null,
            },
        }],
        shaders: Vec::new(),
    }
}

fn scene() -> Scene {
    scene_of(64, 64)
}

/// GPU devices made at once in one process can stall the driver, so the
/// tests draw one at a time.
static GPU: Mutex<()> = Mutex::new(());

fn gpu() -> MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A renderer onto a transparent background; `None` without an adapter,
/// unless `CELESTA_REQUIRE_GPU=1`.
fn renderer() -> Option<GpuRenderer> {
    match GpuRenderer::new(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 0),
        ..GpuRenderOptions::default()
    }) {
        Ok(renderer) => Some(renderer),
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

/// `scenes` drawn as `scene`'s layer; `None` without a GPU.
fn render_scenes_in(scenes: Scenes, scene: &Scene) -> Option<GpuFrame> {
    let _gpu = gpu();
    let mut renderer = renderer()?.with_content_provider(scenes.fixed());
    Some(renderer.render(scene).unwrap())
}

/// `scenes` drawn at time 0, unscaled; `None` without a GPU.
fn render_scenes(scenes: Scenes) -> Option<GpuFrame> {
    render_scenes_in(scenes, &scene())
}

/// Why drawing `scenes` fails, which it must for the model or frame not
/// fitting; `None` without a GPU.
fn invalid(scenes: Scenes) -> Option<String> {
    let _gpu = gpu();
    let mut renderer = renderer()?.with_content_provider(scenes.fixed());
    match renderer.render(&scene()) {
        Err(GpuRenderError::InvalidPluginContent { message, .. }) => Some(message),
        Err(error) => panic!("expected invalid plugin content, got {error}"),
        Ok(_) => panic!("expected invalid plugin content, got a frame"),
    }
}

fn pixel(frame: &GpuFrame, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * frame.width() + x) * 4) as usize;
    frame.pixels()[offset..offset + 4].try_into().unwrap()
}

fn near(actual: [u8; 4], expected: [u8; 4]) -> bool {
    actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2)
}

#[track_caller]
fn assert_pixel(frame: &GpuFrame, x: u32, y: u32, expected: [u8; 4]) {
    let actual = pixel(frame, x, y);
    assert!(
        near(actual, expected),
        "pixel ({x}, {y}) is {actual:?}, not {expected:?}"
    );
}

const WHITE: [u8; 4] = [255; 4];
const CLEAR: [u8; 4] = [0; 4];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

fn transform(offset: [f32; 2], rotation: f32, scale: [f32; 2]) -> TextureTransform {
    TextureTransform {
        offset,
        rotation,
        scale,
    }
}

/// An opaque, single-sided MToon material.
fn toon(mtoon: Mtoon) -> Material {
    Material {
        alpha_mode: AlphaMode::Opaque,
        double_sided: false,
        kind: Kind::Mtoon(Box::new(mtoon)),
    }
}

/// MToon lit white and shaded black, a linear step across the whole of
/// `dot(N, L)` from −1 to 1 (toony 0, no shift), and ambient light
/// unflattened.
fn flat_toon() -> Mtoon {
    Mtoon {
        base_color: [1.0; 4],
        shade_color_factor: [0.0; 3],
        shading_shift_factor: 0.0,
        shading_toony_factor: 0.0,
        gi_equalization_factor: 0.0,
        ..Mtoon::default()
    }
}

fn light(direction: [f32; 3], color: [f32; 3], intensity: f32) -> DirectionalLight {
    DirectionalLight {
        direction,
        color,
        intensity,
    }
}

/// A light travelling down and away from the camera: `dot(N, L)` is 0.5
/// for a normal facing +Z.
const HALF_LIT: [f32; 3] = [0.0, -0.866_025_4, -0.5];

/// `one_square` drawn with `mtoon` under `lights`, without ambient light.
/// The camera is orthographic and looks down −Z, so `V` is +Z.
fn toon_square(mtoon: Mtoon, lights: Vec<DirectionalLight>) -> Scenes {
    let mut scenes = one_square();
    scenes.materials = vec![toon(mtoon)];
    scenes.frame.lights.directional = lights;
    scenes
}

/// `scenes` with every vertex's normal `normal`.
fn with_normals(mut scenes: Scenes, normal: [f32; 3]) -> Scenes {
    for geometry in &mut scenes.model.geometries {
        geometry.normals = vec![normal; geometry.positions.len()];
    }
    scenes
}

/// A 1x1 texture of `rgba`.
fn texel(rgba: [u8; 4], color_space: ColorSpace) -> SceneTexture {
    SceneTexture {
        source: TextureSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::new(rgba.to_vec()),
        },
        color_space,
        sampler: Sampler::default(),
    }
}

fn grey(value: u8) -> [u8; 4] {
    [value, value, value, 255]
}

/// The pixel at the centre of `scenes`, where the square is.
fn centre(scenes: Scenes) -> Option<[u8; 4]> {
    render_scenes(scenes).map(|frame| pixel(&frame, 32, 32))
}

#[track_caller]
fn assert_centre(scenes: Scenes, expected: [u8; 4]) {
    let Some(actual) = centre(scenes) else {
        return;
    };
    assert!(near(actual, expected), "{actual:?}, not {expected:?}");
}

/// Red where lit, blue where shaded.
fn red_and_blue(toony: f32, shift: f32) -> Mtoon {
    Mtoon {
        base_color: [1.0, 0.0, 0.0, 1.0],
        shade_color_factor: [0.0, 0.0, 1.0],
        shading_toony_factor: toony,
        shading_shift_factor: shift,
        ..flat_toon()
    }
}

#[test]
fn lit_main_light_steps_by_toony_and_shift() {
    // `dot(N, L)` 0.5 shifted by −0.25 is three quarters of the way from
    // −0.5 to 0.5: linear (0.75, 0, 0.25).
    for (color, intensity) in [([1.0; 3], 1.0), ([0.5; 3], 2.0)] {
        let scenes = toon_square(
            red_and_blue(0.5, -0.25),
            vec![light(HALF_LIT, color, intensity)],
        );
        assert_centre(scenes, [225, 0, 137, 255]);
    }
}

#[test]
fn lit_shading_shift_texture_adds_red_times_scale() {
    // Red 1 × 0.25 takes the shift from −0.5 to −0.25; green and blue add
    // nothing, leaving it at −0.5: halfway, linear (0.5, 0, 0.5).
    for (rgba, expected) in [
        ([255, 0, 0, 255], [225, 0, 137, 255]),
        ([0, 255, 255, 255], [188, 0, 188, 255]),
    ] {
        let mut scenes = toon_square(
            Mtoon {
                shading_shift_texture: Some(ScaledTextureSlot {
                    texture: 0,
                    scale: 0.25,
                }),
                ..red_and_blue(0.5, -0.5)
            },
            vec![light(HALF_LIT, [1.0; 3], 1.0)],
        );
        scenes.model.textures = vec![texel(rgba, ColorSpace::Linear)];
        assert_centre(scenes, expected);
    }
}

#[test]
fn lit_shade_multiply_texture_tints_the_shade() {
    // Lit from behind, `dot(N, L)` −1: all shade, white times the texel.
    let mut scenes = toon_square(
        Mtoon {
            shade_color_factor: [1.0; 3],
            shade_multiply_texture: Some(TextureSlot { texture: 0 }),
            shading_toony_factor: 0.5,
            ..flat_toon()
        },
        vec![light([0.0, 0.0, 1.0], [1.0; 3], 1.0)],
    );
    scenes.model.textures = vec![texel([128, 64, 200, 255], ColorSpace::Srgb)];
    assert_centre(scenes, [128, 64, 200, 255]);
}

#[test]
fn lit_flat_ambient_is_the_constant_term() {
    // No lights: the white surface shows the ambient alone, linear 0.2,
    // whatever the equalization.
    let mut scenes = toon_square(
        Mtoon {
            gi_equalization_factor: 0.9,
            ..flat_toon()
        },
        vec![],
    );
    scenes.frame.lights.ambient[3] = [0.2; 3];
    assert_centre(scenes, grey(124));
    // Nor any ambient: black, opaque.
    assert_centre(toon_square(flat_toon(), vec![]), BLACK);
}

#[test]
fn lit_ambient_terms_follow_shade_sh9() {
    // Each coefficient alone at 0.5 for the normal (0.48, 0.6, 0.64):
    // 0.5 × x, y, z, 1, xy, yz, z², zx, and −0.5 × (x² − y²).
    let expected = [134, 149, 153, 188, 106, 121, 125, 109, 72];
    for (k, expected) in expected.into_iter().enumerate() {
        let mut scenes = with_normals(toon_square(flat_toon(), vec![]), [0.48, 0.6, 0.64]);
        scenes.frame.lights.ambient[k] = if k == 8 { [-0.5; 3] } else { [0.5; 3] };
        let Some(actual) = centre(scenes) else {
            return;
        };
        assert!(near(actual, grey(expected)), "coefficient {k}: {actual:?}");
    }
}

#[test]
fn lit_gi_equalization_blends_toward_the_vertical_average() {
    // Facing +Y: SH 0.5 + 0.25 = 0.75; the average of +Y and −Y, 0.25.
    for (equalization, expected) in [(0.0, 225), (1.0, 137), (0.5, 188)] {
        let mut scenes = with_normals(
            toon_square(
                Mtoon {
                    gi_equalization_factor: equalization,
                    ..flat_toon()
                },
                vec![],
            ),
            [0.0, 1.0, 0.0],
        );
        scenes.frame.lights.ambient[1] = [0.5; 3];
        scenes.frame.lights.ambient[3] = [0.25; 3];
        let Some(actual) = centre(scenes) else {
            return;
        };
        assert!(
            near(actual, grey(expected)),
            "equalization {equalization}: {actual:?}"
        );
    }
}

#[test]
fn lit_normals_follow_the_inverse_transpose_of_world() {
    // Stretched ×2 in x, the normal (0.6, 0.8, 0) leans towards +Y:
    // (0.3, 0.8, 0) normalised, `dot(N, L)` 0.9363, linear 0.9682. Taken
    // through `world` itself it would lean the other way, to 0.8 (228).
    let mut scenes = with_normals(
        toon_square(flat_toon(), vec![light([0.0, -1.0, 0.0], [1.0; 3], 1.0)]),
        [0.6, 0.8, 0.0],
    );
    scenes.frame.draws[0].world = scaling(2.0, 1.0, 1.0);
    assert_centre(scenes, grey(251));
}

#[test]
fn lit_back_faces_of_double_sided_materials_flip_the_normal() {
    // Turned half about Y, the square shows its back, whose normal faces
    // the camera and the light behind it: lit, red.
    let mut scenes = toon_square(
        red_and_blue(0.5, 0.0),
        vec![light([0.0, 0.0, -1.0], [1.0; 3], 1.0)],
    );
    scenes.materials[0].double_sided = true;
    scenes.frame.draws[0].world = scaling(-1.0, 1.0, -1.0);
    assert_centre(scenes, RED);
}

#[test]
fn lit_toony_one_is_a_hard_step() {
    // `linearstep(0, 0, t)`: shade at `dot(N, L)` 0, lit above.
    let cases = [([0.0, -1.0, 0.0], BLUE), (HALF_LIT, RED)];
    for (direction, expected) in cases {
        let scenes = toon_square(
            red_and_blue(1.0, 0.0),
            vec![light(direction, [1.0; 3], 1.0)],
        );
        assert_centre(scenes, expected);
    }
}

#[test]
fn lit_toony_above_one_inverts_the_ramp() {
    // Toony 1.5: `linearstep(0.5, −0.5, t)`, UniVRM's
    // `saturate((t − 0.5) / (−0.5 − 0.5))`. `dot(N, L)` 0.5 shifted by
    // −0.25 is t = 0.25: (0.25 − 0.5) / −1 = 0.25, a quarter of the way
    // from shade to lit, linear (0.25, 0, 0.75). Lit from behind, t = −1:
    // 1.5, saturated to 1, all lit. A hard step would give blue for both.
    let cases = [
        (HALF_LIT, -0.25, [137, 0, 225, 255]),
        ([0.0, 0.0, 1.0], 0.0, RED),
    ];
    for (direction, shift, expected) in cases {
        let scenes = toon_square(
            red_and_blue(1.5, shift),
            vec![light(direction, [1.0; 3], 1.0)],
        );
        assert_centre(scenes, expected);
    }
}

#[test]
fn lit_degenerate_worlds_draw_without_nan() {
    // Scaled to nothing, the square draws nothing, without an error.
    let mut scenes = toon_square(red_and_blue(0.5, 0.0), vec![]);
    scenes.frame.draws[0].world = scaling(0.0, 0.0, 0.0);
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 32, 32, CLEAR);
    // Mirrored, it lights as its mirror image: its normal still faces the
    // camera and the light behind it.
    let mut scenes = toon_square(
        red_and_blue(0.5, 0.0),
        vec![light([0.0, 0.0, -1.0], [1.0; 3], 1.0)],
    );
    scenes.frame.draws[0].world = scaling(-1.0, 1.0, 1.0);
    assert_centre(scenes, RED);
}

#[test]
fn lit_lights_the_shader_cannot_use_are_refused() {
    let scenes = toon_square(flat_toon(), vec![light(HALF_LIT, [1.0; 3], 1.0); 5]);
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("5 directional lights"), "{message}");

    let scenes = toon_square(flat_toon(), vec![light([0.0; 3], [1.0; 3], 1.0)]);
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(
        message.contains("directional light 0 has no direction"),
        "{message}"
    );

    // Four are fine, and so is none.
    for lights in [vec![light(HALF_LIT, [0.25; 3], 1.0); 4], vec![]] {
        let Some(frame) = render_scenes(toon_square(flat_toon(), lights)) else {
            return;
        };
        assert_eq!(pixel(&frame, 32, 32)[3], 255);
    }
}

#[test]
fn lit_mtoon_texture_out_of_range_is_refused() {
    // Celesta checks a custom material's slots and names the slot by the
    // shader's name for it. (The plugin refuses this before it sends it:
    // `mtoon`'s `mtoon_slots_and_numbers_are_checked`.)
    let scenes = toon_square(
        Mtoon {
            shade_multiply_texture: Some(TextureSlot { texture: 3 }),
            ..flat_toon()
        },
        vec![],
    );
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(
        message.contains("(shadeMultiplyTexture) uses texture 3"),
        "{message}"
    );
}

/// MToon black where lit and where shaded, so that only what a test adds
/// shows.
fn black_toon() -> Mtoon {
    Mtoon {
        base_color: [0.0, 0.0, 0.0, 1.0],
        ..flat_toon()
    }
}

#[test]
fn shading_additional_lights_use_half_lambert_shadow() {
    // The main light is black; the second, at `dot(N, L)` 0.5, steps fully
    // to lit (−0.5 to 0.5 at toony 0.5) and is halved by its shadow
    // `0.5 × (min(0, 0.5) + 1)`: linear 0.5 red.
    let dark = light([0.0, -1.0, 0.0], [1.0; 3], 0.0);
    let scenes = toon_square(
        red_and_blue(0.5, 0.0),
        vec![dark, light(HALF_LIT, [1.0; 3], 1.0)],
    );
    assert_centre(scenes, [188, 0, 0, 255]);
    // From the other side, `dot(N, L)` −0.5: all shade, times
    // `0.5 × (−0.5 + 1)`, linear 0.25 blue.
    let scenes = toon_square(
        red_and_blue(0.5, 0.0),
        vec![dark, light([0.0, 0.866_025_4, 0.5], [1.0; 3], 1.0)],
    );
    assert_centre(scenes, [0, 0, 137, 255]);
    // The same light as the main light has no such shadow.
    let scenes = toon_square(red_and_blue(0.5, 0.0), vec![light(HALF_LIT, [1.0; 3], 1.0)]);
    assert_centre(scenes, RED);
}

#[test]
fn shading_emission_adds_factor_times_texture() {
    // (0.5, 0.25, 0) × (1, 128 / 255, 1): linear (0.5, 0.1255, 0).
    let mut scenes = toon_square(
        Mtoon {
            emissive_factor: [0.5, 0.25, 0.0],
            emissive_texture: Some(TextureSlot { texture: 0 }),
            ..black_toon()
        },
        vec![],
    );
    scenes.model.textures = vec![texel([255, 128, 255, 255], ColorSpace::Linear)];
    assert_centre(scenes, [188, 99, 0, 255]);
}

/// A white parametric rim lifted by 0.5 at fresnel power 1, lit by the
/// scene as far as `mix`: facing the camera, `1 − dot(N, V) + 0.5` is 0.5.
fn rim(mix: f32) -> Mtoon {
    Mtoon {
        parametric_rim_color_factor: [1.0; 3],
        parametric_rim_lift_factor: 0.5,
        parametric_rim_fresnel_power_factor: 1.0,
        rim_lighting_mix_factor: mix,
        ..black_toon()
    }
}

#[test]
fn shading_parametric_rim_follows_lift_power_and_mix() {
    // Unlit by the scene: 0.5 itself.
    assert_centre(toon_square(rim(0.0), vec![]), grey(188));
    // Lit by the main light's 1 and the ambient 0.25: 0.5 × 1.25, or at
    // half the mix 0.5 × 1.125.
    for (mix, expected) in [(1.0, 207), (0.5, 198)] {
        let mut scenes = toon_square(rim(mix), vec![light(HALF_LIT, [1.0; 3], 1.0)]);
        scenes.frame.lights.ambient[3] = [0.25; 3];
        let Some(actual) = centre(scenes) else {
            return;
        };
        assert!(near(actual, grey(expected)), "mix {mix}: {actual:?}");
    }
    // A second light at `dot(N, L)` 0 adds its shadowed 0.5: 0.5 × 1.75.
    let mut scenes = toon_square(
        rim(1.0),
        vec![
            light(HALF_LIT, [1.0; 3], 1.0),
            light([0.0, -1.0, 0.0], [1.0; 3], 1.0),
        ],
    );
    scenes.frame.lights.ambient[3] = [0.25; 3];
    assert_centre(scenes, grey(240));
    // At half the mix, with a black main light and no ambient, the main
    // light's 0.5 × (1 − 0.5) and the second light's 0.5 × 0.5 × 0.5 make
    // 0.375; without the mix on the second light, 0.5.
    let scenes = toon_square(
        rim(0.5),
        vec![
            light(HALF_LIT, [1.0; 3], 0.0),
            light([0.0, -1.0, 0.0], [1.0; 3], 1.0),
        ],
    );
    assert_centre(scenes, grey(165));
}

#[test]
fn shading_parametric_rim_sees_the_eye_in_perspective() {
    // From (0, 0, 2) through a perspective camera, `V` at the centre is
    // towards the eye, +Z: `dot(N, V)` 1, the rim 0.5. Away from the eye,
    // it would be −1 and the rim 1.
    let mut scenes = toon_square(rim(0.0), vec![]);
    scenes.frame.camera.projection = Projection::Perspective {
        yfov: 1.0,
        near: 0.1,
        far: 100.0,
    };
    assert_centre(scenes, grey(188));
}

#[test]
fn shading_parametric_rim_raises_its_base_to_the_fresnel_power() {
    // `dot(N, V)` 0.8 lifted by 0.3: 0.5, squared 0.25.
    let scenes = with_normals(
        toon_square(
            Mtoon {
                parametric_rim_lift_factor: 0.3,
                parametric_rim_fresnel_power_factor: 2.0,
                ..rim(0.0)
            },
            vec![],
        ),
        [0.6, 0.0, 0.8],
    );
    assert_centre(scenes, grey(137));
    // Facing the camera without a lift, the base is 0: no rim.
    let scenes = toon_square(
        Mtoon {
            parametric_rim_lift_factor: 0.0,
            parametric_rim_fresnel_power_factor: 5.0,
            ..rim(0.0)
        },
        vec![],
    );
    assert_centre(scenes, BLACK);
}

#[test]
fn shading_rim_multiply_texture_scales_rim_and_matcap() {
    // 0.5 × sRGB 128 (linear 0.2158): 0.1079.
    let mut scenes = toon_square(
        Mtoon {
            rim_multiply_texture: Some(TextureSlot { texture: 0 }),
            ..rim(0.0)
        },
        vec![],
    );
    scenes.model.textures = vec![texel([128, 128, 128, 255], ColorSpace::Srgb)];
    assert_centre(scenes, grey(92));
}

/// A white matcap of `texture` (texture 0), unlit by the scene, on a black
/// surface.
fn matcap() -> Mtoon {
    Mtoon {
        matcap_factor: [1.0; 3],
        matcap_texture: Some(TextureSlot { texture: 0 }),
        rim_lighting_mix_factor: 0.0,
        ..black_toon()
    }
}

/// A `width` by `height` sRGB texture of `texels`, row by row from the
/// top, sampled nearest and clamped.
fn nearest(width: u32, height: u32, texels: &[[u8; 4]]) -> SceneTexture {
    SceneTexture {
        source: TextureSource::Rgba {
            width,
            height,
            pixels: Arc::new(texels.concat()),
        },
        color_space: ColorSpace::Srgb,
        sampler: Sampler {
            wrap_s: Wrap::ClampToEdge,
            wrap_t: Wrap::ClampToEdge,
            mag_filter: Filter::Nearest,
            min_filter: Filter::Nearest,
            mipmaps: false,
        },
    }
}

#[test]
fn shading_matcap_maps_screen_right_and_up_to_texture_right_and_top() {
    const YELLOW: [u8; 4] = [255, 255, 0, 255];
    let cases = [
        // A normal leaning to screen right samples the texture's right.
        (nearest(2, 1, &[RED, GREEN]), [0.6, 0.0, 0.8], GREEN),
        (nearest(2, 1, &[RED, GREEN]), [-0.6, 0.0, 0.8], RED),
        // One leaning up samples its top row.
        (nearest(1, 2, &[BLUE, YELLOW]), [0.0, 0.6, 0.8], BLUE),
        (nearest(1, 2, &[BLUE, YELLOW]), [0.0, -0.6, 0.8], YELLOW),
    ];
    for (texture, normal, expected) in cases {
        let mut scenes = with_normals(toon_square(matcap(), vec![]), normal);
        scenes.model.textures = vec![texture];
        let Some(actual) = centre(scenes) else {
            return;
        };
        assert!(near(actual, expected), "{normal:?}: {actual:?}");
    }
}

#[test]
fn shading_matcap_looking_straight_down_is_finite() {
    // From (0, 2, 0) down −Y: `cross(+Y, V)` has no length, so screen right
    // is the view's x axis. The square is turned to face +Y.
    let down = |normal: [f32; 3], texture: SceneTexture| {
        let mut scenes = with_normals(toon_square(matcap(), vec![]), normal);
        scenes.frame.camera.view = [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, -1.0, 0.0, 0.0, //
            0.0, 0.0, -2.0, 1.0,
        ];
        scenes.frame.draws[0].world = [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 0.0, -1.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];
        scenes.model.textures = vec![texture];
        scenes
    };
    let facing = [0.0, 0.0, 1.0];
    assert_centre(
        down(facing, texel([128, 64, 200, 255], ColorSpace::Srgb)),
        [128, 64, 200, 255],
    );
    // Screen right is world +X and screen up world −Z, the camera's: a
    // normal leaning to (0.6, 0.8, 0) samples u 0.8, the right third; one
    // leaning to (0, 0.8, −0.6) v 0.2, the top third.
    let thirds = [RED, GREEN, BLUE];
    assert_centre(down([0.6, 0.0, 0.8], nearest(3, 1, &thirds)), BLUE);
    assert_centre(down([0.0, 0.6, 0.8], nearest(1, 3, &thirds)), RED);
}

/// `scenes` with every vertex's tangent +X (towards +u), bitangent sign 1.
fn with_tangents(mut scenes: Scenes) -> Scenes {
    for geometry in &mut scenes.model.geometries {
        geometry.tangents = Some(vec![[1.0, 0.0, 0.0, 1.0]; geometry.positions.len()]);
    }
    scenes
}

/// `flat_toon` with texture 0 as its normal texture at `scale`, lit by a
/// white light along `direction`, on a square with tangents.
fn normal_mapped(texel_rgb: [u8; 3], scale: f32, direction: [f32; 3]) -> Scenes {
    let [r, g, b] = texel_rgb;
    let mut scenes = with_tangents(toon_square(
        Mtoon {
            normal_texture: Some(ScaledTextureSlot { texture: 0, scale }),
            ..flat_toon()
        },
        vec![light(direction, [1.0; 3], 1.0)],
    ));
    scenes.model.textures = vec![texel([r, g, b, 255], ColorSpace::Linear)];
    scenes
}

#[test]
fn surface_normal_texture_tilts_the_normal() {
    // Tangent-space x 0.6 (204), y about 0: at scale 1 the normal is
    // (0.6, 0, 0.8), `dot(N, L)` 0.6 towards +X, linear 0.8. At scale 0.5,
    // x 0.3 and z √0.91: 0.3, linear 0.65. At scale 0, flat: 0.5. Blue is
    // not read.
    for (scale, expected) in [(1.0, 231), (0.5, 211), (0.0, 188)] {
        let scenes = normal_mapped([204, 128, 230], scale, [-1.0, 0.0, 0.0]);
        let Some(actual) = centre(scenes) else {
            return;
        };
        assert!(near(actual, grey(expected)), "scale {scale}: {actual:?}");
    }
}

#[test]
fn surface_mirrored_worlds_keep_normal_maps_upright() {
    // A texel tilted up (+v's opposite, the bitangent) on a square mirrored
    // in x: the tangent turns to −X, and the bitangent, signed by the
    // world's determinant, still points up, `dot(N, L)` 0.6 under a light
    // from above. Unsigned it would point down: 0.2 (124).
    let mut scenes = normal_mapped([128, 204, 230], 1.0, [0.0, -1.0, 0.0]);
    scenes.frame.draws[0].world = scaling(-1.0, 1.0, 1.0);
    assert_centre(scenes, grey(231));
}

#[test]
fn surface_normal_texture_without_tangents_is_refused() {
    let mut scenes = normal_mapped([204, 128, 230], 1.0, [-1.0, 0.0, 0.0]);
    scenes.model.geometries = vec![square()];
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(
        message.contains("draw 0") && message.contains("tangents"),
        "{message}"
    );
}

/// `flat_toon` of texture 0, red, green, blue, and white quarters sampled
/// nearest, through `transform`, then `uv_animation`, at `time`, under an
/// ambient light of 1: a pixel shows its texel. Texture 1, if there is one,
/// is the UV animation mask.
fn animated(
    transform: Option<TextureTransform>,
    uv_animation: fn(Mtoon) -> Mtoon,
    time: f64,
) -> Scenes {
    let mut scenes = toon_square(
        uv_animation(Mtoon {
            base_color_texture: Some(TextureRef {
                texture: 0,
                transform,
            }),
            ..flat_toon()
        }),
        vec![],
    );
    scenes.model.textures = vec![nearest(2, 2, &[RED, GREEN, BLUE, WHITE])];
    scenes.frame.lights.ambient[3] = [1.0; 3];
    scenes.frame.time = time;
    scenes
}

/// The pixel at (23, 23), where the square's texture coordinates are about
/// (0.234, 0.234): the texture's top-left, red, unmoved.
fn top_left(scenes: Scenes) -> Option<[u8; 4]> {
    render_scenes(scenes).map(|frame| pixel(&frame, 23, 23))
}

#[track_caller]
fn assert_top_left(scenes: Scenes, expected: [u8; 4]) {
    let Some(actual) = top_left(scenes) else {
        return;
    };
    assert!(near(actual, expected), "{actual:?}, not {expected:?}");
}

fn scroll_half_in_x(mtoon: Mtoon) -> Mtoon {
    Mtoon {
        uv_animation_scroll_x_speed_factor: 0.5,
        ..mtoon
    }
}

fn quarter_turn_a_second(mtoon: Mtoon) -> Mtoon {
    Mtoon {
        uv_animation_rotation_speed_factor: std::f32::consts::FRAC_PI_2,
        ..mtoon
    }
}

#[test]
fn surface_uv_animation_scrolls_and_turns_with_time() {
    // Scrolled 0.5 a second in u: unmoved at 0, the top-right after 1 s.
    assert_top_left(animated(None, scroll_half_in_x, 0.0), RED);
    assert_top_left(animated(None, scroll_half_in_x, 1.0), GREEN);
    // A quarter turn about the centre after 1 s: (u, v) samples
    // (v, 1 − u), the bottom-left.
    assert_top_left(animated(None, quarter_turn_a_second, 1.0), BLUE);
}

#[test]
fn surface_uv_animation_mask_scales_time_by_blue() {
    // The mask's blue, not its red or green, scales the time.
    for (mask, expected) in [([255, 255, 0, 255], RED), ([0, 0, 255, 255], GREEN)] {
        let mut scenes = animated(
            None,
            |mtoon| Mtoon {
                uv_animation_mask_texture: Some(TextureSlot { texture: 1 }),
                ..scroll_half_in_x(mtoon)
            },
            1.0,
        );
        scenes.model.textures.push(texel(mask, ColorSpace::Linear));
        let Some(actual) = top_left(scenes) else {
            return;
        };
        assert!(near(actual, expected), "mask {mask:?}: {actual:?}");
    }
}

#[test]
fn surface_texture_transform_applies_before_uv_animation() {
    // Offset half a texture in u to (0.734, 0.234), then a quarter turn to
    // (0.234, 0.266): red. Turned first and offset after, it would be
    // (0.734, 0.766): white.
    assert_top_left(
        animated(
            Some(transform([0.5, 0.0], 0.0, [1.0; 2])),
            quarter_turn_a_second,
            1.0,
        ),
        RED,
    );
}

#[test]
fn surface_uv_animation_scrolls_v_down_the_texture() {
    // glTF's v counts down from the top row: scrolled 0.5 a second in v,
    // the top-left samples (0.234, 0.734) after 1 s, the bottom-left.
    assert_top_left(
        animated(
            None,
            |mtoon| Mtoon {
                uv_animation_scroll_y_speed_factor: 0.5,
                ..mtoon
            },
            1.0,
        ),
        BLUE,
    );
}

/// `animated`'s texture repeated in both directions.
fn repeated(mut scenes: Scenes) -> Scenes {
    let sampler = &mut scenes.model.textures[0].sampler;
    sampler.wrap_s = Wrap::Repeat;
    sampler.wrap_t = Wrap::Repeat;
    scenes
}

#[test]
fn surface_uv_animation_scroll_keeps_its_precision_over_long_videos() {
    // Scrolled 0.5 a second in u on a repeated texture, an odd number of
    // seconds moves it half a texture: green. After an hour f32 still holds
    // the half; past 2^25 s it rounds the time to an even number of seconds
    // and the scroll to whole textures, and red would show.
    for time in [1.0, 3601.0, 33_554_433.0] {
        let Some(actual) = top_left(repeated(animated(None, scroll_half_in_x, time))) else {
            return;
        };
        assert!(near(actual, GREEN), "time {time}: {actual:?}");
    }
}

#[test]
fn surface_uv_animation_scroll_wraps_only_where_whole_textures_vanish() {
    // Scrolled 0.5 a second in u for 4 s, two whole textures: dropping them
    // would sample (0.234, 0.234), red. Each case below would show red if
    // they were dropped.
    // A clamped texture stays at its right edge.
    assert_top_left(animated(None, scroll_half_in_x, 4.0), GREEN);
    // A mask's blue of 0.251 scales the scroll to 0.502 of a texture.
    let mut scenes = repeated(animated(
        None,
        |mtoon| Mtoon {
            uv_animation_mask_texture: Some(TextureSlot { texture: 1 }),
            ..scroll_half_in_x(mtoon)
        },
        4.0,
    ));
    scenes
        .model
        .textures
        .push(texel([0, 0, 64, 255], ColorSpace::Linear));
    assert_top_left(scenes, GREEN);
    // A twelfth of a turn in 4 s turns the scrolled (1.734, −0.266) from
    // the centre to (1.369, −1.097): (1.869, −0.597) repeated is (0.869,
    // 0.403), the top-right. Unscrolled, (0.137, 0.403) would be red.
    let scenes = repeated(animated(
        None,
        |mtoon| Mtoon {
            uv_animation_rotation_speed_factor: std::f32::consts::PI / 24.0,
            ..scroll_half_in_x(mtoon)
        },
        4.0,
    ));
    assert_top_left(scenes, GREEN);
}

/// An MToon material shown as `color` alone, by its emission on a black
/// surface under no light, blended by `alpha`, writing depth or not, moved
/// by `queue_offset` within its render queue: Celesta's unlit
/// `see_through` squares, drawn by MToon.
fn see_through(color: [f32; 3], alpha: f32, depth_write: bool, queue_offset: i32) -> Material {
    Material {
        alpha_mode: AlphaMode::Blend {
            depth_write,
            queue_offset,
        },
        ..toon(Mtoon {
            base_color: [0.0, 0.0, 0.0, alpha],
            emissive_factor: color,
            ..black_toon()
        })
    }
}

const SEE_RED: [f32; 3] = [1.0, 0.0, 0.0];
const SEE_BLUE: [f32; 3] = [0.0, 0.0, 1.0];

/// Two squares: `red` (draw 0, at z = 0) and `blue` (draw 1, 1 m behind
/// it), listed red first.
fn front_and_back(red: Material, blue: Material) -> Scenes {
    let mut scenes = one_square();
    scenes.materials = vec![red, blue];
    scenes.model.draws.push(DrawInfo {
        geometry: 0,
        material: 1,
        skin: None,
    });
    let red = scenes.frame.draws[0].clone();
    let blue = DrawState {
        index: 1,
        world: translation(0.0, 0.0, -1.0),
        ..red.clone()
    };
    scenes.frame.draws = vec![red, blue];
    scenes
}

/// Half red over half blue over nothing: linear (0.5, 0, 0.25) at alpha
/// 0.75, premultiplied and sRGB-encoded.
const RED_OVER_BLUE: [u8; 4] = [160, 0, 117, 191];
/// Half blue over half red: the same with red and blue swapped.
const BLUE_OVER_RED: [u8; 4] = [117, 0, 160, 191];
/// Half red alone over nothing, premultiplied and sRGB-encoded.
const HALF_RED: [u8; 4] = [128, 0, 0, 128];
/// Half blue alone over nothing.
const HALF_BLUE: [u8; 4] = [0, 0, 128, 128];

#[test]
fn queue_offsets_order_blend_draws_before_distance() {
    let scenes = |red_offset| {
        front_and_back(
            see_through(SEE_RED, 0.5, false, red_offset),
            see_through(SEE_BLUE, 0.5, false, 0),
        )
    };
    // Both at 3000: blue, farther, first.
    assert_centre(scenes(0), RED_OVER_BLUE);
    // Red at 2999 draws first, though it is nearer; blue is blended over it.
    assert_centre(scenes(-1), BLUE_OVER_RED);
}

#[test]
fn queue_depth_write_blend_draws_first_and_hides_what_is_behind() {
    // Red at 2501 writes its depth before blue at 3000 draws behind it.
    assert_centre(
        front_and_back(
            see_through(SEE_RED, 0.5, true, 0),
            see_through(SEE_BLUE, 0.5, false, 0),
        ),
        HALF_RED,
    );
    // Offsets out of range clamp, to 2510 and 2991: red still first.
    assert_centre(
        front_and_back(
            see_through(SEE_RED, 0.5, true, 20),
            see_through(SEE_BLUE, 0.5, false, -20),
        ),
        HALF_RED,
    );
}

#[test]
fn queue_discards_nearly_clear_blend_pixels() {
    // A clear depth-writing square writes no depth: blue shows through it.
    assert_centre(
        front_and_back(
            see_through(SEE_RED, 0.0, true, 0),
            see_through(SEE_BLUE, 0.5, false, 0),
        ),
        HALF_BLUE,
    );
}

/// From the origin looking down −Z through `projection`: a blended red 1 m
/// square 4 m away (distance 4, view z −4) and a blended blue 10 m square
/// centred at (3, 0, −3.5) (distance 4.61, view z −3.5), both covering
/// the centre.
fn near_and_far(projection: Projection) -> Scenes {
    let mut scenes = front_and_back(
        see_through(SEE_RED, 0.5, false, 0),
        see_through(SEE_BLUE, 0.5, false, 0),
    );
    scenes.frame.camera = Camera {
        view: IDENTITY,
        projection,
    };
    scenes.frame.draws[0].world = translation(0.0, 0.0, -4.0);
    // `translation(3, 0, −3.5) × scaling(10, 10, 1)`.
    let mut blue = scaling(10.0, 10.0, 1.0);
    blue[12] = 3.0;
    blue[14] = -3.5;
    scenes.frame.draws[1].world = blue;
    scenes
}

#[test]
fn queue_perspective_blend_sorts_by_distance() {
    // Blue is farther from the eye, though nearer in view z: blue first.
    assert_centre(
        near_and_far(Projection::Perspective {
            yfov: 1.0,
            near: 0.1,
            far: 100.0,
        }),
        RED_OVER_BLUE,
    );
}

#[test]
fn queue_orthographic_blend_sorts_by_view_depth() {
    // Red is lower in view z: red first.
    assert_centre(
        near_and_far(Projection::Orthographic {
            height: 2.0,
            near: 0.1,
            far: 100.0,
        }),
        BLUE_OVER_RED,
    );
}

#[test]
fn queue_coplanar_opaque_later_draw_wins() {
    let opaque = |color| {
        toon(Mtoon {
            emissive_factor: color,
            ..black_toon()
        })
    };
    let mut scenes = one_square();
    scenes.materials = vec![opaque([1.0, 0.0, 0.0]), opaque([0.0, 1.0, 0.0])];
    scenes.model.draws.push(DrawInfo {
        geometry: 0,
        material: 1,
        skin: None,
    });
    let green = DrawState {
        index: 1,
        ..scenes.frame.draws[0].clone()
    };
    scenes.frame.draws.push(green);
    assert_centre(scenes, GREEN);
}

#[test]
fn mask_mtoon_covers_by_cutoff() {
    // White by emission where the alpha passes the cutoff, nothing where
    // it does not.
    for (alpha, expected) in [(0.8, WHITE), (0.3, CLEAR)] {
        let mut scenes = one_square();
        scenes.materials = vec![Material {
            alpha_mode: AlphaMode::Mask { cutoff: 0.5 },
            ..toon(Mtoon {
                base_color: [0.0, 0.0, 0.0, alpha],
                emissive_factor: [1.0; 3],
                ..black_toon()
            })
        }];
        assert_centre(scenes, expected);
    }
}

/// `square`'s 1 m square facing +Z and its twin facing −Z (the same
/// corners wound the other way), every normal pointing out from the centre
/// in the plane z = 0: a back face the outline grows on every side.
fn shell() -> Geometry {
    let mut geometry = radial_square();
    geometry.positions = [geometry.positions.clone(), geometry.positions].concat();
    geometry.normals = [geometry.normals.clone(), geometry.normals].concat();
    geometry.uvs = [geometry.uvs.clone(), geometry.uvs].concat();
    geometry.targets = vec![MorphTarget {
        positions: vec![[0.0; 3]; 8],
        normals: vec![],
    }];
    geometry.indices = vec![0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6];
    geometry
}

/// `square`'s 1 m square facing +Z alone, single-sided, every normal
/// pointing out from the centre in the plane z = 0.
fn radial_square() -> Geometry {
    let out = std::f32::consts::FRAC_1_SQRT_2;
    Geometry {
        normals: vec![
            [-out, -out, 0.0],
            [out, -out, 0.0],
            [out, out, 0.0],
            [-out, out, 0.0],
        ],
        ..square()
    }
}

/// Black, green by emission under no light, with a red outline unlit by
/// the scene (lighting mix 0), `width` wide in `mode`.
fn outlined(mode: OutlineWidthMode, width: f32) -> Mtoon {
    Mtoon {
        emissive_factor: [0.0, 1.0, 0.0],
        outline_width_mode: mode,
        outline_width_factor: width,
        outline_color_factor: [1.0, 0.0, 0.0],
        outline_lighting_mix_factor: 0.0,
        ..black_toon()
    }
}

/// `toon_square`'s orthographic view, 32 px a metre, of `shell` drawn with
/// `mtoon` under `lights`: the square covers 16..48 in x and y.
fn shell_scene(mtoon: Mtoon, lights: Vec<DirectionalLight>) -> Scenes {
    let mut scenes = toon_square(mtoon, lights);
    scenes.model.geometries = vec![shell()];
    scenes
}

/// A world width that grows the square by 0.125 m, 4 px, on every side:
/// the corners' normals lean 45°.
const FOUR_PX_IN_METRES: f32 = 0.176_776_7;
/// A screen width that grows the square by 4 px on every side through an
/// orthographic camera 2 m high on a square canvas.
const FOUR_PX_OF_SCREEN: f32 = 0.088_388_35;

#[test]
fn outline_world_width_is_in_metres() {
    let material = || outlined(OutlineWidthMode::WorldCoordinates, FOUR_PX_IN_METRES);
    let Some(frame) = render_scenes(shell_scene(material(), vec![])) else {
        return;
    };
    // The ring 12..16 and 48..52; the depth bias keeps the draw's own face
    // in front of its outline.
    assert_pixel(&frame, 14, 32, RED);
    assert_pixel(&frame, 50, 32, RED);
    assert_pixel(&frame, 10, 32, CLEAR);
    assert_pixel(&frame, 32, 32, GREEN);
    // Half the size, the square covers 24..40 and its ring is still 4 px.
    let mut scenes = shell_scene(material(), vec![]);
    scenes.frame.draws[0].world = scaling(0.5, 0.5, 0.5);
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 22, 32, RED);
    assert_pixel(&frame, 18, 32, CLEAR);
}

#[test]
fn outline_mirrored_worlds_still_cull_the_front_faces() {
    // The square alone, without its twin, mirrored in x: its front face
    // turns clockwise on screen, and the outline culls it. With the winding
    // not taken from the world's determinant, the outline would take it for
    // a back face and draw a red ring around it.
    let mut scenes = shell_scene(
        outlined(OutlineWidthMode::WorldCoordinates, FOUR_PX_IN_METRES),
        vec![],
    );
    scenes.model.geometries = vec![radial_square()];
    scenes.frame.draws[0].world = scaling(-1.0, 1.0, 1.0);
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 14, 32, CLEAR);
    assert_pixel(&frame, 50, 32, CLEAR);
    assert_pixel(&frame, 32, 32, GREEN);
}

#[test]
fn outline_mirrored_shells_keep_their_ring() {
    // The whole shell mirrored in x: its twin, counter-clockwise on screen
    // now, still faces away, and the outline still grows it into a ring.
    // Dropping outlines from mirrored worlds would leave it clear.
    let mut scenes = shell_scene(
        outlined(OutlineWidthMode::WorldCoordinates, FOUR_PX_IN_METRES),
        vec![],
    );
    scenes.frame.draws[0].world = scaling(-1.0, 1.0, 1.0);
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 14, 32, RED);
    assert_pixel(&frame, 50, 32, RED);
    assert_pixel(&frame, 32, 32, GREEN);
}

#[test]
fn outline_is_drawn_at_width_zero() {
    // The square alone, seen from behind, from (0, 0, −2) looking +Z: the
    // main pass culls its back face, and the outline, 0 wide, draws it.
    let mut scenes = shell_scene(outlined(OutlineWidthMode::WorldCoordinates, 0.0), vec![]);
    scenes.model.geometries = vec![radial_square()];
    scenes.frame.camera.view = [
        -1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, -1.0, 0.0, //
        0.0, 0.0, -2.0, 1.0,
    ];
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 32, 32, RED);
    assert_pixel(&frame, 14, 32, CLEAR);
}

#[test]
fn outline_screen_width_is_a_share_of_the_height() {
    let material = || outlined(OutlineWidthMode::ScreenCoordinates, FOUR_PX_OF_SCREEN);
    let Some(frame) = render_scenes(shell_scene(material(), vec![])) else {
        return;
    };
    assert_pixel(&frame, 14, 32, RED);
    assert_pixel(&frame, 50, 32, RED);
    assert_pixel(&frame, 10, 32, CLEAR);
    assert_pixel(&frame, 32, 32, GREEN);
    // Half the size, the ring keeps its 4 px: 20..24.
    let mut scenes = shell_scene(material(), vec![]);
    scenes.frame.draws[0].world = scaling(0.5, 0.5, 0.5);
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 22, 32, RED);
    assert_pixel(&frame, 18, 32, CLEAR);
    // A camera 4 m high: `P[1][1]` 0.5 caps `clip.w` 1, halving the ring
    // around the square at 24..40 to 2 px.
    let mut scenes = shell_scene(material(), vec![]);
    scenes.frame.camera.projection = Projection::Orthographic {
        height: 4.0,
        near: 0.1,
        far: 100.0,
    };
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 23, 32, RED);
    assert_pixel(&frame, 21, 32, CLEAR);
}

#[test]
fn outline_screen_width_stops_growing_past_p11() {
    // A perspective camera of `P[1][1]` 2 (`tan(yfov / 2)` 0.5) on the
    // square's axis: every vertex has `clip.w` = the distance d. The ring
    // in NDC, per axis, is `0.707 × 2 × width × min(d, 2) / d`, 32 px a unit.
    let at = |distance: f32| {
        let mut scenes = shell_scene(
            outlined(OutlineWidthMode::ScreenCoordinates, FOUR_PX_OF_SCREEN),
            vec![],
        );
        scenes.frame.camera = Camera {
            view: translation(0.0, 0.0, -distance),
            projection: Projection::Perspective {
                yfov: 2.0 * 0.5_f32.atan(),
                near: 0.1,
                far: 100.0,
            },
        };
        render_scenes(scenes)
    };
    // d 4, past `P[1][1]`: the square's half-width is 0.5 × 2 / 4 NDC,
    // 8 px (24..40), and the ring 0.0625 NDC, 2 px (22..24). Without the
    // cap it would stay 4 px.
    let Some(frame) = at(4.0) else {
        return;
    };
    assert_pixel(&frame, 23, 32, RED);
    assert_pixel(&frame, 21, 32, CLEAR);
    // d 1.5, short of it: the half-width is 0.667 NDC, 21.3 px (10.7..53.3),
    // and the ring 0.125 NDC, 4 px (6.7..10.7). Capped at `P[1][1]` alone
    // it would be 5.3 px (5.3..10.7).
    let Some(frame) = at(1.5) else {
        return;
    };
    assert_pixel(&frame, 8, 32, RED);
    assert_pixel(&frame, 5, 32, CLEAR);
}

#[test]
fn outline_screen_width_corrects_for_aspect() {
    // On a 128x64 canvas the square covers 48..80 in x and 16..48 in y. The
    // corners' normals lean (0.447, 0.894) in clip space: the ring is
    // 2.53 px across and 5.06 px up.
    let mut scenes = shell_scene(
        outlined(OutlineWidthMode::ScreenCoordinates, FOUR_PX_OF_SCREEN),
        vec![],
    );
    scenes.model.canvas = [128.0, 64.0];
    let Some(frame) = render_scenes_in(scenes, &scene_of(128, 64)) else {
        return;
    };
    assert_pixel(&frame, 46, 32, RED);
    assert_pixel(&frame, 64, 12, RED);
    assert_pixel(&frame, 44, 32, CLEAR);
    assert_pixel(&frame, 64, 9, CLEAR);
}

#[test]
fn outline_width_texture_uses_raw_uv0_green() {
    // Green 1 at u 0, the left side, and 0 at u 1. Through the base color
    // texture's offset the left side would sample u 0.5, width 0.
    let mut scenes = shell_scene(
        Mtoon {
            base_color_texture: Some(TextureRef {
                texture: 0,
                transform: Some(transform([0.5, 0.0], 0.0, [1.0; 2])),
            }),
            outline_width_multiply_texture: Some(TextureSlot { texture: 1 }),
            ..outlined(OutlineWidthMode::WorldCoordinates, FOUR_PX_IN_METRES)
        },
        vec![],
    );
    let mut width = nearest(2, 1, &[[0, 255, 0, 255], [0, 0, 0, 255]]);
    width.color_space = ColorSpace::Linear;
    scenes.model.textures = vec![texel(WHITE, ColorSpace::Srgb), width];
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 14, 32, RED);
    assert_pixel(&frame, 50, 32, CLEAR);
}

#[test]
fn outline_color_mixes_toward_the_shaded_color() {
    // The shaded color is the emission, (0.5, 1, 0.5); the outline's
    // (1, 0.5, 1) is multiplied by its mix with white.
    for (mix, expected) in [
        (0.0, [255, 188, 255, 255]),
        (1.0, [188, 188, 188, 255]),
        (0.5, [225, 188, 225, 255]),
    ] {
        let scenes = shell_scene(
            Mtoon {
                emissive_factor: [0.5, 1.0, 0.5],
                outline_color_factor: [1.0, 0.5, 1.0],
                outline_lighting_mix_factor: mix,
                ..outlined(OutlineWidthMode::WorldCoordinates, FOUR_PX_IN_METRES)
            },
            vec![],
        );
        let Some(frame) = render_scenes(scenes) else {
            return;
        };
        let actual = pixel(&frame, 14, 32);
        assert!(near(actual, expected), "mix {mix}: {actual:?}");
    }
}

/// White, shaded black, with a white outline as lit as its surface (mix 1).
fn lit_outline(mtoon: Mtoon) -> Mtoon {
    Mtoon {
        outline_width_mode: OutlineWidthMode::WorldCoordinates,
        outline_width_factor: FOUR_PX_IN_METRES,
        outline_color_factor: [1.0; 3],
        outline_lighting_mix_factor: 1.0,
        ..mtoon
    }
}

/// A light travelling −X, towards a normal facing +X.
const FROM_PLUS_X: [f32; 3] = [-1.0, 0.0, 0.0];

#[test]
fn outline_shading_sees_the_main_light_only() {
    // Lit white and shaded white: the main light is black, and the emission
    // 0.25 is all. The second light would add its shadowed 0.5 (225).
    let scenes = shell_scene(
        lit_outline(Mtoon {
            shade_color_factor: [1.0; 3],
            emissive_factor: [0.25; 3],
            ..flat_toon()
        }),
        vec![
            light([0.0, -1.0, 0.0], [1.0; 3], 0.0),
            light(FROM_PLUS_X, [1.0; 3], 1.0),
        ],
    );
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 50, 32, grey(137));
}

#[test]
fn outline_shades_with_the_outward_normal() {
    // The right side's normal faces +X, towards the light: lit. The left
    // side's faces away: shaded. Turned inwards, they would swap.
    let scenes = shell_scene(
        lit_outline(flat_toon()),
        vec![light(FROM_PLUS_X, [1.0; 3], 1.0)],
    );
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 50, 32, WHITE);
    assert_pixel(&frame, 14, 32, BLACK);
}

#[test]
fn outline_surface_follows_the_uv_animation() {
    // The outline at x 12..52 spans u 0..1: (14, 32) is at u 0.06, red; a
    // scroll of 0.5 a second takes it to green after 1 s. The ambient light
    // 1 shows the base color.
    for (time, expected) in [(0.0, RED), (1.0, GREEN)] {
        let mut scenes = shell_scene(
            lit_outline(Mtoon {
                base_color_texture: Some(TextureRef {
                    texture: 0,
                    transform: None,
                }),
                ..scroll_half_in_x(flat_toon())
            }),
            vec![],
        );
        scenes.model.textures = vec![nearest(2, 1, &[RED, GREEN])];
        scenes.frame.lights.ambient[3] = [1.0; 3];
        scenes.frame.time = time;
        let Some(frame) = render_scenes(scenes) else {
            return;
        };
        let actual = pixel(&frame, 14, 32);
        assert!(near(actual, expected), "time {time}: {actual:?}");
    }
}

#[test]
fn outline_surface_follows_the_normal_texture() {
    // At (32, 14), above the square, the outward normal is about
    // (0.028, 1, 0): `dot(N, L)` 0.028, linear 0.51. The texel tilts it
    // 0.6 towards the tangent, +X: `dot(N, L)` 0.61, linear 0.81.
    for (normal_texture, expected) in [
        (None, 190),
        (
            Some(ScaledTextureSlot {
                texture: 0,
                scale: 1.0,
            }),
            232,
        ),
    ] {
        let mut scenes = with_tangents(shell_scene(
            lit_outline(Mtoon {
                normal_texture,
                ..flat_toon()
            }),
            vec![light(FROM_PLUS_X, [1.0; 3], 1.0)],
        ));
        scenes.model.textures = vec![texel([204, 128, 230, 255], ColorSpace::Linear)];
        let Some(frame) = render_scenes(scenes) else {
            return;
        };
        let actual = pixel(&frame, 32, 14);
        assert!(
            near(actual, grey(expected)),
            "{normal_texture:?}: {actual:?}"
        );
    }
}

#[test]
fn outline_none_draws_nothing() {
    let Some(frame) = render_scenes(shell_scene(
        outlined(OutlineWidthMode::None, FOUR_PX_IN_METRES),
        vec![],
    )) else {
        return;
    };
    assert_pixel(&frame, 14, 32, CLEAR);
}

#[test]
fn outline_every_alpha_side_and_mode_draws() {
    // One MToon material for each alpha mode (opaque, mask, blend, and
    // blend writing depth), side (single, double), and outline mode (none,
    // world, screen), each drawing the shell, every other draw mirrored:
    // opening the model makes every surface and outline pipeline in both
    // windings, and the frame draws with both.
    let alphas = [
        AlphaMode::Opaque,
        AlphaMode::Mask { cutoff: 0.5 },
        AlphaMode::BLEND,
        AlphaMode::Blend {
            depth_write: true,
            queue_offset: 0,
        },
    ];
    let outlines = [
        (OutlineWidthMode::None, 0.0),
        (OutlineWidthMode::WorldCoordinates, FOUR_PX_IN_METRES),
        (OutlineWidthMode::ScreenCoordinates, FOUR_PX_OF_SCREEN),
    ];
    let mut materials = Vec::new();
    for alpha_mode in alphas {
        for double_sided in [false, true] {
            for (mode, width) in outlines {
                materials.push(Material {
                    alpha_mode,
                    double_sided,
                    kind: Kind::Mtoon(Box::new(outlined(mode, width))),
                });
            }
        }
    }
    assert_eq!(materials.len(), 24);
    let mut scenes = shell_scene(black_toon(), vec![light(HALF_LIT, [1.0; 3], 1.0)]);
    let state = scenes.frame.draws[0].clone();
    scenes.model.draws = (0..materials.len())
        .map(|material| DrawInfo {
            geometry: 0,
            material,
            skin: None,
        })
        .collect();
    scenes.frame.draws = (0..materials.len())
        .map(|index| DrawState {
            index,
            world: if index % 2 == 1 {
                scaling(-1.0, 1.0, 1.0)
            } else {
                IDENTITY
            },
            ..state.clone()
        })
        .collect();
    scenes.materials = materials;
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    // The opaque surfaces cover the centre; outlines the ring.
    assert_eq!(pixel(&frame, 32, 32)[3], 255);
    assert_eq!(pixel(&frame, 14, 32)[3], 255);

    // A mask material's outline is cut out with its surface: at alpha 0.8
    // the green surface and its red ring show, at 0.3 neither does.
    for (alpha, surface, ring) in [(0.8, GREEN, RED), (0.3, CLEAR, CLEAR)] {
        let mut scenes = shell_scene(black_toon(), vec![]);
        scenes.materials = vec![Material {
            alpha_mode: AlphaMode::Mask { cutoff: 0.5 },
            ..toon(Mtoon {
                base_color: [0.0, 0.0, 0.0, alpha],
                ..outlined(OutlineWidthMode::WorldCoordinates, FOUR_PX_IN_METRES)
            })
        }];
        let Some(frame) = render_scenes(scenes) else {
            return;
        };
        assert_pixel(&frame, 32, 32, surface);
        assert_pixel(&frame, 14, 32, ring);
    }
}

#[test]
fn outline_uses_the_material_blend() {
    let mut material = toon(Mtoon {
        base_color: [0.0, 0.0, 0.0, 0.5],
        ..outlined(OutlineWidthMode::WorldCoordinates, FOUR_PX_IN_METRES)
    });
    material.alpha_mode = AlphaMode::BLEND;
    let mut scenes = shell_scene(black_toon(), vec![]);
    scenes.materials = vec![material];
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    let alpha = pixel(&frame, 14, 32)[3];
    assert!(alpha.abs_diff(128) <= 2, "alpha {alpha}");
}

#[test]
fn outline_screen_normals_facing_the_camera_stay_put() {
    // Normals along ±Z have no direction on screen: no ring, and no NaN.
    let mut scenes = shell_scene(
        outlined(OutlineWidthMode::ScreenCoordinates, FOUR_PX_OF_SCREEN),
        vec![],
    );
    let geometry = &mut scenes.model.geometries[0];
    geometry.normals = [[[0.0, 0.0, 1.0]; 4], [[0.0, 0.0, -1.0]; 4]].concat();
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 14, 32, CLEAR);
    assert_pixel(&frame, 32, 32, GREEN);
}

const GREEN_BIND: [f32; 3] = [0.0, 1.0, 0.0];

/// `scenes` whose draw 0 packs `binds` into its parameters.
fn bound(mut scenes: Scenes, binds: MtoonOverrides) -> Scenes {
    scenes.binds = vec![binds];
    scenes
}

#[test]
fn override_replaces_mtoon_colors() {
    // Each MToon color is red in the material and green in the frame's
    // binds; the pixel shows that color alone, so a multiplied bind would
    // be black.
    let red = [1.0, 0.0, 0.0];
    let emission = bound(
        toon_square(
            Mtoon {
                emissive_factor: red,
                ..black_toon()
            },
            vec![],
        ),
        MtoonOverrides {
            emission_color: Some(GREEN_BIND),
            ..MtoonOverrides::default()
        },
    );
    // A light from behind, `dot(N, L)` −1: all shade at toony 0.5.
    let shade = bound(
        toon_square(
            Mtoon {
                shade_color_factor: red,
                shading_toony_factor: 0.5,
                ..flat_toon()
            },
            vec![light([0.0, 0.0, 1.0], [1.0; 3], 1.0)],
        ),
        MtoonOverrides {
            shade_color: Some(GREEN_BIND),
            ..MtoonOverrides::default()
        },
    );
    let matcap = {
        let mut scenes = toon_square(
            Mtoon {
                matcap_factor: red,
                ..matcap()
            },
            vec![],
        );
        scenes.model.textures = vec![texel(WHITE, ColorSpace::Srgb)];
        bound(
            scenes,
            MtoonOverrides {
                matcap_color: Some(GREEN_BIND),
                ..MtoonOverrides::default()
            },
        )
    };
    // Facing the camera, `1 − dot(N, V) + 1` is 1.
    let rim = bound(
        toon_square(
            Mtoon {
                parametric_rim_color_factor: red,
                parametric_rim_lift_factor: 1.0,
                ..rim(0.0)
            },
            vec![],
        ),
        MtoonOverrides {
            rim_color: Some(GREEN_BIND),
            ..MtoonOverrides::default()
        },
    );
    let outline = bound(
        shell_scene(
            outlined(OutlineWidthMode::WorldCoordinates, FOUR_PX_IN_METRES),
            vec![],
        ),
        MtoonOverrides {
            outline_color: Some(GREEN_BIND),
            ..MtoonOverrides::default()
        },
    );
    // Under an ambient light of 1, the lit color is the base color.
    let base = {
        let mut scenes = toon_square(
            Mtoon {
                base_color: [1.0, 0.0, 0.0, 1.0],
                ..flat_toon()
            },
            vec![],
        );
        scenes.frame.lights.ambient[3] = [1.0; 3];
        bound(
            scenes,
            MtoonOverrides {
                base_color: Some([0.0, 1.0, 0.0, 1.0]),
                ..MtoonOverrides::default()
            },
        )
    };
    for (name, scenes, (x, y)) in [
        ("emission", emission, (32, 32)),
        ("shade", shade, (32, 32)),
        ("matcap", matcap, (32, 32)),
        ("rim", rim, (32, 32)),
        ("outline", outline, (14, 32)),
        ("base", base, (32, 32)),
    ] {
        let Some(frame) = render_scenes(scenes) else {
            return;
        };
        let actual = pixel(&frame, x, y);
        assert!(near(actual, GREEN), "{name}: {actual:?}");
    }
}
