//! `LayerContent::Plugin` with scene content: a skinned, morphing scene
//! from a content provider. A provider made up here stands in for a plugin
//! process.

use super::*;
use crate::scene::{MAX_CACHED_PIPELINES, MAX_CACHED_SHADERS, MAX_MODEL_PIPELINES, ScenePipelines};
use celesta_plugin::{
    AlphaMode, Camera, ColorSpace, ContentFrame, ContentProvider, CullFace, CustomMaterial,
    DepthBias, DirectionalLight, DrawInfo, DrawState, EmptyTexel, Filter, Geometry, Lights,
    Material, MaterialKind, ModelId, MorphTarget, Opened, OutlinePass, PluginError, Projection,
    Sampler, SceneFrame, SceneModel, SceneShader, SceneTexture, ShaderTexture, Skin, TextureSource,
    UnlitMaterial,
};
use std::collections::BTreeMap;
use std::sync::Arc;

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// Answers `scenes` with `model` and every frame with `frame`, changed by
/// `animate` for the frame's time when there is one.
struct Scenes {
    model: SceneModel,
    frame: SceneFrame,
    animate: Option<fn(&mut SceneFrame, f64)>,
}

impl ContentProvider for Scenes {
    fn open(
        &mut self,
        plugin: &str,
        assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        if plugin != "scenes" {
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
        time: f64,
        _data: &serde_json::Value,
    ) -> Result<ContentFrame, PluginError> {
        assert_eq!(model, ModelId(9));
        let mut frame = self.frame.clone();
        if let Some(animate) = self.animate {
            animate(&mut frame, time);
        }
        Ok(frame.into())
    }
}

/// A 1 m square at z = 0 facing +Z, with one morph target moving it +0.5 m
/// in x; skinned to joints 0 and 1 when `skinned`.
fn square(skinned: bool) -> Geometry {
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
        joints: skinned.then(|| vec![[0, 1, 0, 0]; 4]),
        weights: skinned.then(|| vec![[0.5, 0.5, 0.0, 0.0]; 4]),
        targets: vec![MorphTarget {
            positions: vec![[0.5, 0.0, 0.0]; 4],
            normals: vec![],
        }],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

/// A 64x64 scene: draw 0 an unskinned square, draw 1 a square skinned to
/// skin 0's two joints, both unlit with a white sRGB texture; seen from
/// 2 m away.
fn valid() -> Scenes {
    let draw = |index| DrawState {
        index,
        visible: true,
        world: IDENTITY,
        morph_weights: vec![0.0],
        center: None,
        overrides: Default::default(),
        params: None,
    };
    let mut view = IDENTITY;
    view[14] = -2.0;
    Scenes {
        model: SceneModel {
            canvas: [64.0, 64.0],
            textures: vec![SceneTexture {
                source: TextureSource::Rgba {
                    width: 2,
                    height: 2,
                    pixels: Arc::new(vec![255; 16]),
                },
                color_space: ColorSpace::Srgb,
                sampler: Sampler::default(),
            }],
            geometries: vec![square(false), square(true)],
            skins: vec![Skin { joints: 2 }],
            shaders: vec![],
            materials: vec![Material {
                alpha_mode: AlphaMode::Opaque,
                double_sided: false,
                kind: MaterialKind::Unlit(UnlitMaterial {
                    base_color: [1.0; 4],
                    base_color_texture: Some(celesta_plugin::TextureRef {
                        texture: 0,
                        transform: None,
                    }),
                }),
            }],
            draws: vec![
                DrawInfo {
                    geometry: 0,
                    material: 0,
                    skin: None,
                },
                DrawInfo {
                    geometry: 1,
                    material: 0,
                    skin: Some(0),
                },
            ],
        },
        frame: SceneFrame {
            camera: Camera {
                view,
                projection: Projection::Perspective {
                    yfov: 1.0,
                    near: 0.1,
                    far: 100.0,
                },
            },
            lights: Lights::default(),
            time: 0.0,
            draws: vec![draw(0), draw(1)],
            skins: vec![vec![IDENTITY; 2]],
        },
        animate: None,
    }
}

fn scene() -> Scene {
    scene_at(0.0, 1.0)
}

/// The scene layer at `time`, scaled by `scale` from the top-left corner.
fn scene_at(time: f64, scale: f64) -> Scene {
    layer_on(empty_scene(64, 64), time, scale)
}

/// `scene` with the scene layer at `time`, scaled by `scale` from the
/// top-left corner.
fn layer_on(mut scene: Scene, time: f64, scale: f64) -> Scene {
    scene.layers.push(Layer {
        id: "scenes".into(),
        transform: EvaluatedTransform {
            anchor: Point { x: 0.0, y: 0.0 },
            scale: Point { x: scale, y: scale },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Plugin {
            plugin: "scenes".into(),
            assets: BTreeMap::new(),
            time,
            data: serde_json::Value::Null,
        },
    });
    scene
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
        kind: MaterialKind::Unlit(UnlitMaterial {
            base_color: color,
            base_color_texture: None,
        }),
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
            geometries: vec![square(false)],
            skins: vec![],
            shaders: vec![],
            materials: vec![plain([1.0; 4])],
            draws: vec![DrawInfo {
                geometry: 0,
                material: 0,
                skin: None,
            }],
        },
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
        animate: None,
    }
}

/// `one_square`, its square skinned wholly to joint 1 of a two-joint skin
/// whose joint 1 moves it +0.5 m in x.
fn skinned_square() -> Scenes {
    let mut scenes = one_square();
    let mut geometry = square(true);
    geometry.joints = Some(vec![[1, 0, 0, 0]; 4]);
    geometry.weights = Some(vec![[1.0, 0.0, 0.0, 0.0]; 4]);
    scenes.model.geometries = vec![geometry];
    scenes.model.skins = vec![Skin { joints: 2 }];
    scenes.model.draws[0].skin = Some(0);
    scenes.frame.skins = vec![vec![IDENTITY, translation(0.5, 0.0, 0.0)]];
    scenes
}

/// `scenes` drawn at time 0, unscaled; `None` without a GPU.
fn render_scenes(scenes: Scenes) -> Option<GpuFrame> {
    render_scenes_in(scenes, &scene())
}

/// `scenes` drawn as `scene`'s layer; `None` without a GPU.
fn render_scenes_in(scenes: Scenes, scene: &Scene) -> Option<GpuFrame> {
    let mut renderer = renderer(transparent())?;
    renderer.renderer = renderer.renderer.with_content_provider(scenes);
    Some(renderer.render(scene).unwrap())
}

fn pixel(frame: &GpuFrame, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * frame.width + x) * 4) as usize;
    frame.pixels[offset..offset + 4].try_into().unwrap()
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

fn transparent() -> GpuRenderOptions {
    GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 0),
        ..GpuRenderOptions::default()
    }
}

/// Why rendering `scenes` fails, which it must for the model or frame not
/// fitting; `None` without a GPU.
fn invalid(scenes: Scenes) -> Option<String> {
    let mut renderer = renderer(transparent())?;
    renderer.renderer = renderer.renderer.with_content_provider(scenes);
    match renderer.render(&scene()) {
        Err(GpuRenderError::InvalidPluginContent { message, .. }) => Some(message),
        Err(error) => panic!("expected invalid plugin content, got {error}"),
        Ok(_) => panic!("expected invalid plugin content, got a frame"),
    }
}

#[test]
fn a_valid_scene_renders() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    renderer.renderer = renderer.renderer.with_content_provider(valid());
    renderer.render(&scene()).unwrap();
}

#[test]
fn scene_with_bad_index_is_invalid() {
    let mut scenes = valid();
    scenes.frame.draws[1].index = 3;
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("draw 3"), "{message}");

    let mut scenes = valid();
    scenes.frame.draws[1].index = 0;
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("draw 0"), "{message}");
}

#[test]
fn scene_with_wrong_weight_count_is_invalid() {
    let mut scenes = valid();
    scenes.frame.draws[0].morph_weights = vec![0.5, 0.5];
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("weights"), "{message}");
}

#[test]
fn scene_with_short_skin_is_invalid() {
    let mut scenes = valid();
    scenes.frame.skins[0].pop();
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("skin 0"), "{message}");
}

#[test]
fn scene_with_joint_out_of_range_is_invalid() {
    let mut scenes = valid();
    scenes.model.geometries[1].joints.as_mut().unwrap()[2] = [0, 2, 0, 0];
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("joint"), "{message}");
}

#[test]
fn scene_with_bad_camera_is_invalid() {
    let projections = [
        Projection::Perspective {
            yfov: 1.0,
            near: 0.0,
            far: 100.0,
        },
        Projection::Perspective {
            yfov: 1.0,
            near: 10.0,
            far: 1.0,
        },
        Projection::Orthographic {
            height: 2.0,
            near: -1.0,
            far: 1.0,
        },
    ];
    for projection in projections {
        let mut scenes = valid();
        scenes.frame.camera.projection = projection;
        let Some(message) = invalid(scenes) else {
            return;
        };
        assert!(message.contains("near"), "{projection:?}: {message}");
    }

    let mut scenes = valid();
    scenes.frame.camera.projection = Projection::Perspective {
        yfov: std::f32::consts::PI,
        near: 0.1,
        far: 100.0,
    };
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("yfov"), "{message}");

    let mut scenes = valid();
    scenes.frame.camera.projection = Projection::Orthographic {
        height: 0.0,
        near: 0.1,
        far: 100.0,
    };
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("height"), "{message}");

    let mut scenes = valid();
    scenes.frame.camera.view[0] = 0.0;
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("view"), "{message}");

    let mut scenes = valid();
    scenes.frame.camera.view[5] = f32::NAN;
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("view"), "{message}");
}

#[test]
fn scene_with_eye_beyond_f32_is_invalid() {
    // Scaled by 10⁻⁵ (determinant 10⁻¹⁵, invertible) and moved 3 × 10³⁸
    // in x, every number finite: the eye, `−M⁻¹t`, is at −3 × 10⁴³.
    let mut scenes = valid();
    let mut view = scaling(1e-5, 1e-5, 1e-5);
    view[12] = 3e38;
    scenes.frame.camera.view = view;
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("beyond f32's range"), "{message}");
}

#[test]
fn nearer_quad_covers_farther_in_either_order() {
    // Red 2 m and green 4 m in front of a camera at the origin.
    let mut scenes = one_square();
    scenes.model.materials = vec![plain([1.0, 0.0, 0.0, 1.0]), plain([0.0, 1.0, 0.0, 1.0])];
    scenes.model.draws.push(DrawInfo {
        geometry: 0,
        material: 1,
        skin: None,
    });
    scenes.frame.camera = Camera {
        view: IDENTITY,
        projection: Projection::Perspective {
            yfov: 1.0,
            near: 0.1,
            far: 100.0,
        },
    };
    let red = scenes.frame.draws[0].clone();
    let green = DrawState {
        index: 1,
        world: translation(0.0, 0.0, -4.0),
        ..red.clone()
    };
    let red = DrawState {
        world: translation(0.0, 0.0, -2.0),
        ..red
    };
    for draws in [
        vec![red.clone(), green.clone()],
        vec![green.clone(), red.clone()],
    ] {
        let mut scenes = Scenes {
            model: scenes.model.clone(),
            frame: scenes.frame.clone(),
            animate: None,
        };
        scenes.frame.draws = draws;
        let Some(frame) = render_scenes(scenes) else {
            return;
        };
        assert_pixel(&frame, 32, 32, [255, 0, 0, 255]);
    }
}

#[test]
fn orthographic_maps_height_to_canvas() {
    let Some(frame) = render_scenes(one_square()) else {
        return;
    };
    // 2 m high on 64 pixels: the 1 m square spans pixels 16 to 47.
    assert_pixel(&frame, 31, 31, WHITE);
    assert_pixel(&frame, 8, 8, CLEAR);
    assert_pixel(&frame, 16, 16, WHITE);
    assert_pixel(&frame, 47, 47, WHITE);
    assert_pixel(&frame, 15, 32, CLEAR);
    assert_pixel(&frame, 48, 32, CLEAR);
    assert_pixel(&frame, 32, 15, CLEAR);
    assert_pixel(&frame, 32, 48, CLEAR);
}

#[test]
fn skinning_moves_vertices() {
    let Some(frame) = render_scenes(skinned_square()) else {
        return;
    };
    // Moved 0.5 m, 16 pixels: from 16..48 to 32..64.
    assert_pixel(&frame, 31, 32, CLEAR);
    assert_pixel(&frame, 32, 32, WHITE);
    assert_pixel(&frame, 63, 32, WHITE);
    assert_pixel(&frame, 32, 15, CLEAR);
}

#[test]
fn morph_weight_moves_vertices() {
    let Some(skinned) = render_scenes(skinned_square()) else {
        return;
    };
    let mut scenes = one_square();
    scenes.frame.draws[0].morph_weights = vec![1.0];
    let Some(morphed) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&morphed, 32, 32, WHITE);
    assert!(
        morphed.pixels == skinned.pixels,
        "the morphed square differs from the skinned one"
    );
}

#[test]
fn srgb_texture_round_trips() {
    let mut scenes = one_square();
    scenes.model.textures = vec![SceneTexture {
        source: TextureSource::Rgba {
            width: 2,
            height: 2,
            pixels: Arc::new([128, 64, 200, 255].repeat(4)),
        },
        color_space: ColorSpace::Srgb,
        sampler: Sampler::default(),
    }];
    let MaterialKind::Unlit(unlit) = &mut scenes.model.materials[0].kind else {
        unreachable!("the test scene's material is unlit");
    };
    unlit.base_color_texture = Some(celesta_plugin::TextureRef {
        texture: 0,
        transform: None,
    });
    // Four times the canvas's height: it covers the canvas.
    scenes.frame.draws[0].world = scaling(4.0, 4.0, 1.0);
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    for (x, y) in [(0, 0), (32, 32), (63, 63), (5, 50)] {
        assert_pixel(&frame, x, y, [128, 64, 200, 255]);
    }
}

#[test]
fn back_faces_are_culled_unless_double_sided() {
    // Clockwise as the camera sees it: a back face.
    let reversed = vec![0, 2, 1, 0, 3, 2];
    let cases = [
        (true, false, false, CLEAR),
        (true, true, false, WHITE),
        // Mirrored by its world matrix, the square turns clockwise on
        // screen and still faces front; reversed, it faces back.
        (false, false, true, WHITE),
        (true, false, true, CLEAR),
        (true, true, true, WHITE),
    ];
    for (reverse, double_sided, mirrored, expected) in cases {
        let mut scenes = one_square();
        if reverse {
            scenes.model.geometries[0].indices = reversed.clone();
        }
        scenes.model.materials[0].double_sided = double_sided;
        if mirrored {
            scenes.frame.draws[0].world = scaling(-1.0, 1.0, 1.0);
        }
        let Some(frame) = render_scenes(scenes) else {
            return;
        };
        let actual = pixel(&frame, 32, 32);
        assert!(
            near(actual, expected),
            "reversed {reverse}, double-sided {double_sided}, mirrored {mirrored}: {actual:?}"
        );
    }
}

#[test]
fn mirrored_views_keep_front_faces() {
    // Seen through a view mirrored in x, the square turns clockwise on
    // screen and still faces front; reversed, it faces back. Mirrored by
    // its world matrix as well, the two mirrors cancel.
    let reversed = vec![0, 2, 1, 0, 3, 2];
    let cases = [
        (false, false, WHITE),
        (true, false, CLEAR),
        (false, true, WHITE),
        (true, true, CLEAR),
    ];
    for (reverse, mirrored_world, expected) in cases {
        let mut scenes = one_square();
        scenes.frame.camera.view[0] = -1.0;
        if reverse {
            scenes.model.geometries[0].indices = reversed.clone();
        }
        if mirrored_world {
            scenes.frame.draws[0].world = scaling(-1.0, 1.0, 1.0);
        }
        let Some(frame) = render_scenes(scenes) else {
            return;
        };
        let actual = pixel(&frame, 32, 32);
        assert!(
            near(actual, expected),
            "reversed {reverse}, mirrored world {mirrored_world}: {actual:?}"
        );
    }
}

#[test]
fn hidden_draws_are_not_drawn() {
    let mut scenes = one_square();
    scenes.frame.draws[0].visible = false;
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert!(frame.pixels.iter().all(|value| *value == 0));

    // The second of two squares hidden, the first drawn where it is.
    let mut scenes = one_square();
    scenes.model.draws.push(scenes.model.draws[0]);
    let mut hidden = scenes.frame.draws[0].clone();
    hidden.index = 1;
    hidden.visible = false;
    hidden.world = translation(-0.75, 0.0, 0.0);
    scenes.frame.draws.push(hidden);
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 32, 32, WHITE);
    assert_pixel(&frame, 8, 32, CLEAR);
}

#[test]
fn edges_are_antialiased_at_density_two() {
    // Scaled ×2, the layer draws 128 pixels for its 2 m: 64 a metre. The
    // square's left edge sits at pixel 40.25 of the 64 that show.
    let mut scenes = one_square();
    let left = (40.25 - 64.0) / 64.0;
    scenes.frame.draws[0].world = translation(left + 0.5, 0.0, 0.0);
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    renderer.renderer = renderer.renderer.with_content_provider(scenes);
    let frame = renderer.render(&scene_at(0.0, 2.0)).unwrap();
    assert_pixel(&frame, 39, 48, CLEAR);
    assert_pixel(&frame, 41, 48, WHITE);
    let edge = pixel(&frame, 40, 48);
    assert!(
        (32..=224).contains(&edge[3]) && edge.iter().all(|value| *value == edge[3]),
        "{edge:?}"
    );
}

#[test]
fn mask_cuts_at_cutoff() {
    // A texture whose left column has alpha 0.3 and right 0.8, sampled
    // nearest, on a square four times the canvas: the left half of the
    // canvas shows the left column and the right half the right one.
    let mut scenes = one_square();
    scenes.model.textures = vec![SceneTexture {
        source: TextureSource::Rgba {
            width: 2,
            height: 1,
            pixels: Arc::new(vec![255, 255, 255, 77, 255, 255, 255, 204]),
        },
        color_space: ColorSpace::Srgb,
        sampler: Sampler {
            mag_filter: Filter::Nearest,
            min_filter: Filter::Nearest,
            mipmaps: false,
            ..Sampler::default()
        },
    }];
    let material = &mut scenes.model.materials[0];
    material.alpha_mode = AlphaMode::Mask { cutoff: 0.5 };
    let MaterialKind::Unlit(unlit) = &mut material.kind else {
        unreachable!("the test scene's material is unlit");
    };
    unlit.base_color_texture = Some(celesta_plugin::TextureRef {
        texture: 0,
        transform: None,
    });
    scenes.frame.draws[0].world = scaling(4.0, 4.0, 1.0);
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    for (x, y) in [(0, 32), (24, 10), (30, 60)] {
        assert_pixel(&frame, x, y, CLEAR);
    }
    for (x, y) in [(34, 32), (40, 10), (63, 60)] {
        assert_pixel(&frame, x, y, WHITE);
    }
}

/// An unlit, single-sided material of `color` blended by its alpha.
fn blended(color: [f32; 4]) -> Material {
    Material {
        alpha_mode: AlphaMode::BLEND,
        ..plain(color)
    }
}

/// `one_square` with two squares half transparent: red (draw 0) at z = 0
/// and blue (draw 1) 1 m behind it, listed red first.
fn red_over_blue() -> Scenes {
    let mut scenes = one_square();
    scenes.model.materials = vec![blended([1.0, 0.0, 0.0, 0.5]), blended([0.0, 0.0, 1.0, 0.5])];
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

#[test]
fn blend_draws_back_to_front_whatever_the_list_order() {
    for reversed in [false, true] {
        let mut scenes = red_over_blue();
        if reversed {
            scenes.frame.draws.reverse();
        }
        let Some(frame) = render_scenes(scenes) else {
            return;
        };
        let centre = pixel(&frame, 32, 32);
        assert!(centre[0] > centre[2], "reversed {reversed}: {centre:?}");
        assert_pixel(&frame, 32, 32, RED_OVER_BLUE);
    }
}

#[test]
fn opaque_draws_before_blend_even_if_listed_after() {
    // Half-transparent blue in front, listed before an opaque red behind.
    let mut scenes = red_over_blue();
    scenes.model.materials = vec![blended([0.0, 0.0, 1.0, 0.5]), plain([1.0, 0.0, 0.0, 1.0])];
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    // Linear (0.5, 0, 0.5), opaque.
    assert_pixel(&frame, 32, 32, [188, 0, 188, 255]);
}

#[test]
fn center_overrides_sorting() {
    // Red's centre put behind blue's: red draws first, and blue over it.
    let mut scenes = red_over_blue();
    scenes.frame.draws[0].center = Some([0.0, 0.0, -5.0]);
    scenes.frame.draws[1].center = Some([0.0, 0.0, 1.0]);
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    let centre = pixel(&frame, 32, 32);
    assert!(centre[2] > centre[0], "{centre:?}");
    assert_pixel(&frame, 32, 32, BLUE_OVER_RED);
}

#[test]
fn frames_repeat_exactly() {
    let mut scenes = skinned_square();
    scenes.animate = Some(|frame, time| {
        frame.skins[0][1] = translation(0.25 * time as f32, 0.1 * time as f32, 0.0);
        frame.draws[0].morph_weights = vec![0.3 * time as f32];
    });
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    renderer.renderer = renderer.renderer.with_content_provider(scenes);
    let first = renderer.render(&scene_at(1.0, 1.0)).unwrap();
    let rest = renderer.render(&scene_at(0.0, 1.0)).unwrap();
    let again = renderer.render(&scene_at(1.0, 1.0)).unwrap();
    assert!(first.pixels != rest.pixels, "time does not move the square");
    assert!(first.pixels == again.pixels, "time 1 drew differently");
}

#[test]
fn scene_resources_are_reused_between_frames() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    renderer.renderer = renderer.renderer.with_content_provider(one_square());
    let first = renderer.render(&scene()).unwrap();
    let allocations = renderer.meshes.scene_frames.allocations;
    assert!(allocations > 0, "the first frame allocated nothing");
    let second = renderer.render(&scene()).unwrap();
    assert_eq!(renderer.meshes.scene_frames.allocations, allocations);
    assert!(
        first.pixels == second.pixels,
        "the second frame drew differently"
    );
}

#[test]
fn scene_resources_follow_the_canvas_size() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    renderer.renderer = renderer.renderer.with_content_provider(one_square());
    let allocations = |renderer: &TestRenderer| renderer.meshes.scene_frames.allocations;
    renderer.render(&scene_at(0.0, 1.0)).unwrap();
    let a = allocations(&renderer);
    // ×2, the canvas is 128² texels: new targets.
    renderer.render(&scene_at(0.0, 2.0)).unwrap();
    let b = allocations(&renderer);
    assert!(b > a, "{b} after {a}");
    renderer.render(&scene_at(0.0, 2.0)).unwrap();
    assert_eq!(allocations(&renderer), b);
    // The 64² targets went with the frame that did not draw them.
    renderer.render(&scene_at(0.0, 1.0)).unwrap();
    assert!(allocations(&renderer) > b, "the 64² targets were kept");
}

#[test]
fn scene_resources_keep_every_size_a_frame_draws() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    renderer.renderer = renderer.renderer.with_content_provider(one_square());
    let mut scene = scene_at(0.0, 1.0);
    let mut doubled = scene_at(0.0, 2.0).layers.remove(0);
    doubled.id = "scenes-2x".into();
    scene.layers.push(doubled);
    renderer.render(&scene).unwrap();
    let allocations = renderer.meshes.scene_frames.allocations;
    renderer.render(&scene).unwrap();
    assert_eq!(renderer.meshes.scene_frames.allocations, allocations);
}

/// A `width` by 64 scene of scene layers, each given as its id, time,
/// scale, and left edge.
fn scene_layers(width: u32, layers: &[(&str, f64, f64, f64)]) -> Scene {
    let mut scene = empty_scene(width, 64);
    for &(id, time, scale, x) in layers {
        let mut layer = scene_at(time, scale).layers.remove(0);
        layer.id = id.into();
        layer.transform.position = Point { x, y: 0.0 };
        scene.layers.push(layer);
    }
    scene
}

#[test]
fn scene_resources_evict_unused_sizes_over_budget() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    renderer.renderer = renderer.renderer.with_content_provider(one_square());
    // Room for a 128² canvas and a 64² one.
    renderer.meshes.scene_frames.target_texels = 128 * 128 + 64 * 64;
    let sizes = |renderer: &TestRenderer| renderer.meshes.scene_frames.cached_sizes();
    let layers = [
        ("scenes", 0.0, 1.0, 0.0),
        ("scenes-2x", 0.0, 2.0, 0.0),
        ("scenes-3x", 0.0, 3.0, 0.0),
    ];
    renderer.render(&scene_layers(64, &layers[..2])).unwrap();
    assert_eq!(sizes(&renderer), [(64, 64), (128, 128)]);
    // 192² does not fit beside them: the sizes this frame does not draw
    // go, though the last frame drew them.
    renderer.render(&scene_layers(64, &layers[2..])).unwrap();
    assert_eq!(sizes(&renderer), [(192, 192)]);
    renderer.render(&scene_layers(64, &layers[..1])).unwrap();
    assert_eq!(sizes(&renderer), [(64, 64)]);
    // A frame drawing more than the budget still gets every size.
    renderer.render(&scene_layers(64, &layers)).unwrap();
    assert_eq!(sizes(&renderer), [(64, 64), (128, 128), (192, 192)]);
}

#[test]
fn scene_frame_buffers_are_released_when_no_scene_is_drawn() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    renderer.renderer = renderer.renderer.with_content_provider(one_square());
    let first = renderer.render(&scene()).unwrap();
    assert!(renderer.meshes.scene_frames.holds_buffers());
    let allocations = renderer.meshes.scene_frames.allocations;
    // The frame after one that drew no scene starts with nothing.
    renderer.render(&empty_scene(64, 64)).unwrap();
    renderer.render(&empty_scene(64, 64)).unwrap();
    assert!(!renderer.meshes.scene_frames.holds_buffers());
    assert_eq!(renderer.meshes.scene_frames.cached_sizes(), []);
    let again = renderer.render(&scene()).unwrap();
    assert!(renderer.meshes.scene_frames.allocations > allocations);
    assert!(first.pixels == again.pixels, "the scene drew differently");
}

#[test]
fn scene_resources_survive_a_frame_that_fails_or_draws_nothing() {
    // At time 1 the frame is refused (a morph weight short), or its only
    // draw is hidden. Either way the canvas and buffers stay for time 0.
    let spoilers: [fn(&mut SceneFrame, f64); 2] = [
        |frame, time| {
            if time > 0.5 {
                frame.draws[0].morph_weights.clear();
            }
        },
        |frame, time| {
            if time > 0.5 {
                frame.draws[0].visible = false;
            }
        },
    ];
    for (refused, spoil) in [true, false].into_iter().zip(spoilers) {
        let Some(mut renderer) = renderer(transparent()) else {
            return;
        };
        let mut scenes = one_square();
        scenes.animate = Some(spoil);
        renderer.renderer = renderer.renderer.with_content_provider(scenes);
        let first = renderer.render(&scene_at(0.0, 1.0)).unwrap();
        let allocations = renderer.meshes.scene_frames.allocations;
        let spoiled = renderer.render(&scene_at(1.0, 1.0));
        assert_eq!(spoiled.is_err(), refused, "refused {refused}");
        // Without the spoiled frame's mark, this frame would begin with
        // nothing and allocate it all again.
        let again = renderer.render(&scene_at(0.0, 1.0)).unwrap();
        assert_eq!(
            renderer.meshes.scene_frames.allocations, allocations,
            "refused {refused}"
        );
        assert!(first.pixels == again.pixels, "refused {refused}");
    }
}

/// `one_square` with two more draws of its square, small, red and blue, in
/// front of it at the top corners; at time 0 the frame lists only draw 0.
fn three_squares() -> Scenes {
    let mut scenes = one_square();
    let model = &mut scenes.model;
    model
        .materials
        .extend([plain([1.0, 0.0, 0.0, 1.0]), plain([0.0, 0.0, 1.0, 1.0])]);
    for material in [1, 2] {
        model.draws.push(DrawInfo {
            geometry: 0,
            material,
            skin: None,
        });
    }
    for (index, x) in [(1, -0.75), (2, 0.75)] {
        let mut world = scaling(0.4, 0.4, 1.0);
        world[12] = x;
        world[13] = 0.75;
        world[14] = 0.1;
        let mut state = scenes.frame.draws[0].clone();
        state.index = index;
        state.world = world;
        scenes.frame.draws.push(state);
    }
    scenes.animate = Some(|frame, time| {
        if time == 0.0 {
            frame.draws.truncate(1);
        }
    });
    scenes
}

#[test]
fn scene_frame_buffers_regrow_and_rebind() {
    // Each time drawn by a renderer of its own.
    let mut expected = Vec::new();
    for time in [0.0, 1.0] {
        let Some(mut renderer) = renderer(transparent()) else {
            return;
        };
        renderer.renderer = renderer.renderer.with_content_provider(three_squares());
        expected.push(renderer.render(&scene_at(time, 1.0)).unwrap());
    }
    assert!(expected[0].pixels != expected[1].pixels);
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    renderer.renderer = renderer.renderer.with_content_provider(three_squares());
    let one = renderer.render(&scene_at(0.0, 1.0)).unwrap();
    assert!(one.pixels == expected[0].pixels, "one draw");
    let allocations = renderer.meshes.scene_frames.allocations;
    // Three draws' uniforms outgrow one's buffers.
    let three = renderer.render(&scene_at(1.0, 1.0)).unwrap();
    assert!(renderer.meshes.scene_frames.allocations > allocations);
    assert!(three.pixels == expected[1].pixels, "three draws");
    let one = renderer.render(&scene_at(0.0, 1.0)).unwrap();
    assert!(one.pixels == expected[0].pixels, "one draw again");
}

#[test]
fn two_same_size_scene_layers_keep_their_own_pixels() {
    let mut scenes = one_square();
    scenes.animate = Some(|frame, time| {
        if time == 1.0 {
            frame.draws[0].overrides.base_color = Some([1.0, 0.0, 0.0, 1.0]);
        }
    });
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    renderer.renderer = renderer.renderer.with_content_provider(scenes);
    // Both 64² canvases: the white square, then beside it the red one.
    let scene = scene_layers(128, &[("white", 0.0, 1.0, 0.0), ("red", 1.0, 1.0, 64.0)]);
    let frame = renderer.render(&scene).unwrap();
    assert_eq!(renderer.meshes.scene_frames.cached_sizes(), [(64, 64)]);
    assert_pixel(&frame, 32, 32, WHITE);
    assert_pixel(&frame, 96, 32, RED);
    assert_pixel(&frame, 2, 2, CLEAR);
    assert_pixel(&frame, 66, 2, CLEAR);
}

#[test]
fn zoomed_scene_canvas_is_capped() {
    // ×64, the 64-unit canvas would be 4096² texels, twice the cap.
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    renderer.renderer = renderer.renderer.with_content_provider(one_square());
    let assets = BTreeMap::new();
    let content = crate::mesh::PluginContent {
        plugin: "scenes",
        assets: &assets,
        time: 0.0,
        data: &serde_json::Value::Null,
    };
    let (texture, canvas) = renderer.plugin_texture("scenes", &content, 64.0).unwrap();
    assert_eq!(canvas, [64.0, 64.0]);
    assert_eq!(texture.width, texture.height);
    assert!(
        u64::from(texture.width) * u64::from(texture.height) <= crate::scene::SCENE_CANVAS_TEXELS,
        "{}x{}",
        texture.width,
        texture.height
    );
    // The whole layer still renders, zoomed.
    renderer.render(&scene_at(0.0, 64.0)).unwrap();
}

#[test]
fn scene_too_large_for_the_gpu_budget_is_invalid() {
    // 100,000 vertices without triangles, drawn 230 times: each draw's
    // skinned copy takes 4.8 MB, and the geometry's streams 4 MB, over
    // 1 GiB in all. It is refused before any of it is allocated.
    let mut scenes = one_square();
    scenes.model.geometries = vec![Geometry {
        positions: vec![[0.0; 3]; 100_000],
        normals: vec![[0.0, 0.0, 1.0]; 100_000],
        uvs: vec![[0.0; 2]; 100_000],
        tangents: None,
        joints: None,
        weights: None,
        targets: vec![],
        indices: vec![],
    }];
    scenes.model.draws = vec![scenes.model.draws[0]; 230];
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("1108000032 bytes"), "{message}");
}

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// `one_square` four times the canvas, textured with a 2x2 texture, red
/// and green on its top row and blue and white below, sampled nearest and
/// repeated, through `transform`: untransformed, the canvas's quarters
/// show the texture's.
fn quarters(transform: Option<celesta_plugin::TextureTransform>) -> Scenes {
    let mut scenes = one_square();
    scenes.model.textures = vec![SceneTexture {
        source: TextureSource::Rgba {
            width: 2,
            height: 2,
            pixels: Arc::new([RED, GREEN, BLUE, WHITE].concat()),
        },
        color_space: ColorSpace::Srgb,
        sampler: Sampler {
            mag_filter: Filter::Nearest,
            min_filter: Filter::Nearest,
            mipmaps: false,
            ..Sampler::default()
        },
    }];
    let MaterialKind::Unlit(unlit) = &mut scenes.model.materials[0].kind else {
        unreachable!("the test scene's material is unlit");
    };
    unlit.base_color_texture = Some(celesta_plugin::TextureRef {
        texture: 0,
        transform,
    });
    scenes.frame.draws[0].world = scaling(4.0, 4.0, 1.0);
    scenes
}

/// The canvas's top-left, top-right, bottom-left, and bottom-right quarters
/// of `scenes`, sampled well inside each.
fn quarter_colors(scenes: Scenes) -> Option<[[u8; 4]; 4]> {
    let frame = render_scenes(scenes)?;
    let quarters = [(16, 16), (48, 16), (16, 48), (48, 48)];
    for (x, y) in quarters {
        // The quarter is one color throughout.
        for (dx, dy) in [(-8, -8), (8, -8), (-8, 8), (8, 8)] {
            let corner = pixel(&frame, (x + dx) as u32, (y + dy) as u32);
            assert_eq!(corner, pixel(&frame, x as u32, y as u32), "({x}, {y})");
        }
    }
    Some(quarters.map(|(x, y)| pixel(&frame, x as u32, y as u32)))
}

fn transform(offset: [f32; 2], rotation: f32, scale: [f32; 2]) -> celesta_plugin::TextureTransform {
    celesta_plugin::TextureTransform {
        offset,
        rotation,
        scale,
    }
}

#[track_caller]
fn assert_quarters(scenes: Scenes, expected: [[u8; 4]; 4]) {
    let Some(actual) = quarter_colors(scenes) else {
        return;
    };
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(near(*actual, expected), "{actual:?}, not {expected:?}");
    }
}

#[test]
fn texture_transform_offsets_scales_and_rotates() {
    // The canvas shows the middle of the square: u and v 0.25 to 0.75, v
    // counting down from the texture's top row.
    assert_quarters(quarters(None), [RED, GREEN, BLUE, WHITE]);
    // Offset half a texture in u: each row's columns swap.
    assert_quarters(
        quarters(Some(transform([0.5, 0.0], 0.0, [1.0; 2]))),
        [GREEN, RED, WHITE, BLUE],
    );
    // Halved in u: u 0.125 to 0.375, the left column throughout.
    assert_quarters(
        quarters(Some(transform([0.0; 2], 0.0, [0.5, 1.0]))),
        [RED, RED, BLUE, BLUE],
    );
    // KHR_texture_transform's rotation, `mat3(cos, sin, 0, -sin, cos, 0,
    // 0, 0, 1)` column by column: a quarter turn takes (u, v) to (-v, u).
    // The top-left quarter, u and v 0.25 to 0.5, samples u -0.5 to -0.25
    // (0.5 to 0.75 repeated) and v 0.25 to 0.5: green.
    assert_quarters(
        quarters(Some(transform(
            [0.0; 2],
            std::f32::consts::FRAC_PI_2,
            [1.0; 2],
        ))),
        [GREEN, WHITE, RED, BLUE],
    );
    // Scaled, then rotated, then offset: halved in v, turned, and moved
    // half a texture in u, (u, v) goes to (0.5 - v / 2, u). u' stays
    // within 0.125 to 0.375, the left column, and v' = u picks the row by
    // the quarter's column. (Rotated before it is scaled, the right
    // quarters would be red too.)
    assert_quarters(
        quarters(Some(transform(
            [0.5, 0.0],
            std::f32::consts::FRAC_PI_2,
            [1.0, 0.5],
        ))),
        [RED, BLUE, RED, BLUE],
    );
}

#[test]
fn overrides_replace_the_material_texture_transform_and_color() {
    // The material offsets half a texture in u and halves u; the frame's
    // overrides put both back.
    let mut scenes = quarters(Some(transform([0.5, 0.0], 0.0, [0.5, 1.0])));
    scenes.frame.draws[0].overrides.texture_offset = Some([0.0, 0.0]);
    scenes.frame.draws[0].overrides.texture_scale = Some([1.0, 1.0]);
    assert_quarters(scenes, [RED, GREEN, BLUE, WHITE]);
    // An offset override in v alone: the rows swap, the material's u
    // offset gone.
    let mut scenes = quarters(Some(transform([0.5, 0.0], 0.0, [1.0; 2])));
    scenes.frame.draws[0].overrides.texture_offset = Some([0.0, 0.5]);
    assert_quarters(scenes, [BLUE, WHITE, RED, GREEN]);

    // A blue base color in place of the material's red, not multiplied by
    // it.
    let mut scenes = one_square();
    scenes.model.materials = vec![plain([1.0, 0.0, 0.0, 1.0])];
    scenes.frame.draws[0].overrides.base_color = Some([0.0, 0.0, 1.0, 1.0]);
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 32, 32, BLUE);
    // It multiplies the texture as the material's would: green leaves the
    // red and blue texels black.
    let mut scenes = quarters(None);
    scenes.frame.draws[0].overrides.base_color = Some([0.0, 1.0, 0.0, 1.0]);
    assert_quarters(scenes, [BLACK, GREEN, BLACK, GREEN]);
}

#[test]
fn override_not_finite_is_refused() {
    type Set = fn(&mut celesta_plugin::MaterialOverrides);
    let cases: [(&str, Set); 3] = [
        ("base color override", |o| {
            o.base_color = Some([f32::NAN, 0.0, 0.0, 1.0]);
        }),
        ("texture offset override", |o| {
            o.texture_offset = Some([0.0, f32::INFINITY]);
        }),
        ("texture scale override", |o| {
            o.texture_scale = Some([f32::NEG_INFINITY, 1.0]);
        }),
    ];
    for (name, set) in cases {
        let mut scenes = one_square();
        set(&mut scenes.frame.draws[0].overrides);
        let Some(message) = invalid(scenes) else {
            return;
        };
        assert!(
            message.contains("draw 0") && message.contains(name),
            "{name}: {message}"
        );
    }
}

const BLACK: [u8; 4] = [0, 0, 0, 255];

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
            wrap_s: celesta_plugin::Wrap::ClampToEdge,
            wrap_t: celesta_plugin::Wrap::ClampToEdge,
            mag_filter: Filter::Nearest,
            min_filter: Filter::Nearest,
            mipmaps: false,
        },
    }
}

/// `scenes` with every vertex's tangent +X (towards +u), bitangent sign 1.
fn with_tangents(mut scenes: Scenes) -> Scenes {
    for geometry in &mut scenes.model.geometries {
        geometry.tangents = Some(vec![[1.0, 0.0, 0.0, 1.0]; geometry.positions.len()]);
    }
    scenes
}

/// An unlit, single-sided material of `color` blended by `alpha`, writing
/// depth or not, moved by `queue_offset` within its render queue.
fn see_through(color: [f32; 3], alpha: f32, depth_write: bool, queue_offset: i32) -> Material {
    let [r, g, b] = color;
    Material {
        alpha_mode: AlphaMode::Blend {
            depth_write,
            queue_offset,
        },
        ..plain([r, g, b, alpha])
    }
}

const SEE_RED: [f32; 3] = [1.0, 0.0, 0.0];
const SEE_BLUE: [f32; 3] = [0.0, 0.0, 1.0];

/// `red_over_blue`'s squares drawn with `red` (draw 0, at z = 0) and `blue`
/// (draw 1, 1 m behind it).
fn front_and_back(red: Material, blue: Material) -> Scenes {
    let mut scenes = red_over_blue();
    scenes.model.materials = vec![red, blue];
    scenes
}

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
    let mut scenes = one_square();
    scenes.model.materials = vec![plain([1.0, 0.0, 0.0, 1.0]), plain([0.0, 1.0, 0.0, 1.0])];
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
        ..square(false)
    }
}

/// A world width that grows the square by 0.125 m, 4 px, on every side:
/// the corners' normals lean 45°.
const FOUR_PX_IN_METRES: f32 = 0.176_776_7;

/// A plugin's material shader of `wgsl` with a texture slot for each of
/// `textures`, which an empty slot samples.
fn shader(wgsl: &str, textures: &[EmptyTexel]) -> SceneShader {
    SceneShader {
        name: None,
        wgsl: wgsl.to_owned(),
        textures: textures
            .iter()
            .map(|&empty| ShaderTexture { name: None, empty })
            .collect(),
    }
}

/// An opaque, single-sided material of the model's shader `shader` and
/// `params`, its texture slots empty.
fn custom(shader: usize, params: Vec<u8>) -> Material {
    Material {
        alpha_mode: AlphaMode::Opaque,
        double_sided: false,
        kind: MaterialKind::Custom(CustomMaterial {
            shader,
            params,
            textures: vec![],
            outline: None,
            requires_tangents: false,
        }),
    }
}

/// `values` as little-endian bytes.
fn bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// `one_square` with `shaders`, drawn with `material`.
fn custom_square(shaders: Vec<SceneShader>, material: Material) -> Scenes {
    let mut scenes = one_square();
    scenes.model.shaders = shaders;
    scenes.model.materials = vec![material];
    scenes
}

/// `custom`'s `CustomMaterial`, to change.
fn custom_of(material: &mut Material) -> &mut CustomMaterial {
    match &mut material.kind {
        MaterialKind::Custom(custom) => custom,
        _ => panic!("not a custom material"),
    }
}

/// Its parameters' color.
const FLAT: &str = "struct Params { color: vec4f };
fn material_fragment(fragment: Fragment) -> vec4f { return params.color; }";

/// N·L under the main light, plus the ambient light.
const LAMBERT: &str = "struct Params { albedo: vec4f };
fn material_fragment(fragment: Fragment) -> vec4f {
    let light = frame.lights[0];
    let lit = light.color.rgb * max(dot(fragment.normal, light.to_light.xyz), 0.0) + ambient_light(fragment.normal);
    return vec4f(params.albedo.rgb * lit, 1.0);
}";

/// Of three slots (white, black, white when empty), the one `slot` names.
const SLOTS: &str = "struct Params { slot: u32 };
fn material_fragment(fragment: Fragment) -> vec4f {
    let a = textureSample(texture_0, sampler_0, fragment.uv);
    let b = textureSample(texture_1, sampler_1, fragment.uv);
    let c = textureSample(texture_2, sampler_2, fragment.uv);
    return select(select(c, b, params.slot == 1u), a, params.slot == 0u);
}";

/// What the interface gives a material, by `mode`: 0 the tangent's `w` and
/// the draw's handedness, each as 0.5 + 0.5 x; 1 red on a front face, blue
/// on a back one; 2 `V`'s x as 0.5 + 0.5 x; 3 `linearstep(p.x, p.y, p.z)`
/// and the length of `safe_normalize` of zero; 4 the color of light
/// `p.x`; 5 the light count over 4, the time, and the height over the
/// width; 6 the view's z translation over −4 and the projection's `[1][1]`
/// over 4; 7 the normal's z and x, each as 0.5 + 0.5 x.
const PROBE: &str = "struct Params { mode: u32, p: vec4f };
fn material_fragment(fragment: Fragment) -> vec4f {
    switch params.mode {
        case 0u: { return vec4f(0.5 + 0.5 * fragment.tangent.w, 0.5 + 0.5 * draw.handedness, 0.0, 1.0); }
        case 1u: { return select(vec4f(0.0, 0.0, 1.0, 1.0), vec4f(1.0, 0.0, 0.0, 1.0), fragment.front_facing); }
        case 2u: { return vec4f(0.5 + 0.5 * fragment.view.x, 0.0, 0.0, 1.0); }
        case 3u: { return vec4f(linearstep(params.p.x, params.p.y, params.p.z), length(safe_normalize(vec3f(0.0))), 0.0, 1.0); }
        case 4u: { return vec4f(frame.lights[u32(params.p.x)].color.rgb, 1.0); }
        case 5u: { return vec4f(f32(frame.light_count) / 4.0, frame.time, frame.height_over_width, 1.0); }
        case 6u: { return vec4f(frame.view[3].z / -4.0, frame.projection[1][1] / 4.0, 0.0, 1.0); }
        default: { return vec4f(0.5 + 0.5 * fragment.normal.z, 0.5 + 0.5 * fragment.normal.x, 0.0, 1.0); }
    }
}";

/// `PROBE`'s 32 parameter bytes: `mode` at 0, `p` at 16.
fn probe(mode: u32, p: [f32; 4]) -> Vec<u8> {
    let mut params = mode.to_le_bytes().to_vec();
    params.resize(16, 0);
    params.extend(bytes(&p));
    params
}

/// `one_square` drawn with `PROBE` in `mode`.
fn probe_square(mode: u32, p: [f32; 4]) -> Scenes {
    custom_square(vec![shader(PROBE, &[])], custom(0, probe(mode, p)))
}

/// `one_square` drawn with `LAMBERT` in white under `lights`.
fn lambert_square(lights: Vec<DirectionalLight>) -> Scenes {
    let mut scenes = custom_square(vec![shader(LAMBERT, &[])], custom(0, bytes(&[1.0; 4])));
    scenes.frame.lights.directional = lights;
    scenes
}

#[test]
fn material_params_colour_the_surface() {
    let scenes = custom_square(
        vec![shader(FLAT, &[])],
        custom(0, bytes(&[0.5, 0.25, 1.0, 1.0])),
    );
    assert_centre(scenes, [188, 137, 255, 255]);
}

#[test]
fn material_texture_slots_sample_their_texture_or_their_empty_texel() {
    let slots = |slot: u32, filled: Vec<Option<usize>>| {
        let mut material = custom(0, slot.to_le_bytes().to_vec());
        custom_of(&mut material).textures = filled;
        let mut scenes = custom_square(
            vec![shader(
                SLOTS,
                &[EmptyTexel::White, EmptyTexel::Black, EmptyTexel::White],
            )],
            material,
        );
        scenes.model.textures = vec![texel([255, 0, 0, 255], ColorSpace::Srgb)];
        scenes
    };
    assert_centre(slots(0, vec![Some(0)]), RED);
    assert_centre(slots(1, vec![Some(0)]), BLACK);
    assert_centre(slots(2, vec![Some(0)]), WHITE);
    assert_centre(slots(1, vec![Some(0), Some(0)]), RED);
}

#[test]
fn material_lights_and_ambient_reach_the_shader() {
    // `dot(N, L)` 0.5 plus an ambient light of 0.25: linear 0.75.
    let mut scenes = lambert_square(vec![light(HALF_LIT, [1.0; 3], 1.0)]);
    scenes.frame.lights.ambient[3] = [0.25; 3];
    assert_centre(scenes, grey(225));
    let mut scenes = lambert_square(vec![]);
    scenes.frame.lights.ambient[3] = [0.25; 3];
    assert_centre(scenes, grey(137));
}

#[test]
fn material_lights_the_frame_cannot_hold_are_refused() {
    let scenes = lambert_square(vec![light(HALF_LIT, [1.0; 3], 1.0); 5]);
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(message.contains("5 directional lights"), "{message}");

    let scenes = lambert_square(vec![light([0.0; 3], [1.0; 3], 1.0)]);
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(
        message.contains("directional light 0 has no direction"),
        "{message}"
    );

    // Four are fine, and so is none.
    for lights in [vec![light(HALF_LIT, [0.25; 3], 1.0); 4], vec![]] {
        let Some(frame) = render_scenes(lambert_square(lights)) else {
            return;
        };
        assert_eq!(pixel(&frame, 32, 32)[3], 255);
    }
}

#[test]
fn material_ambient_follows_the_sh_polynomial() {
    // Each coefficient alone at 0.5 for the normal (0.48, 0.6, 0.64):
    // 0.5 × x, y, z, 1, xy, yz, z², zx, and −0.5 × (x² − y²).
    let expected = [134, 149, 153, 188, 106, 121, 125, 109, 72];
    for (k, expected) in expected.into_iter().enumerate() {
        let mut scenes = with_normals(lambert_square(vec![]), [0.48, 0.6, 0.64]);
        scenes.frame.lights.ambient[k] = if k == 8 { [-0.5; 3] } else { [0.5; 3] };
        let Some(actual) = centre(scenes) else {
            return;
        };
        assert!(near(actual, grey(expected)), "coefficient {k}: {actual:?}");
    }
}

#[test]
fn material_normals_follow_the_inverse_transpose_of_world() {
    // Stretched ×2 in x, the normal (0.6, 0.8, 0) leans towards +Y:
    // (0.3, 0.8, 0) normalised, `dot(N, L)` 0.9363. Taken through `world`
    // itself it would lean the other way, to 0.5547 (196).
    let mut scenes = with_normals(
        lambert_square(vec![light([0.0, -1.0, 0.0], [1.0; 3], 1.0)]),
        [0.6, 0.8, 0.0],
    );
    scenes.frame.draws[0].world = scaling(2.0, 1.0, 1.0);
    assert_centre(scenes, grey(248));
}

#[test]
fn material_tangents_and_handedness_follow_world() {
    // A material that requires tangents draws on a geometry that has them.
    let scenes = |world| {
        let mut scenes = with_tangents(probe_square(0, [0.0; 4]));
        custom_of(&mut scenes.model.materials[0]).requires_tangents = true;
        scenes.frame.draws[0].world = world;
        scenes
    };
    assert_centre(scenes(IDENTITY), [255, 255, 0, 255]);
    assert_centre(scenes(scaling(-1.0, 1.0, 1.0)), [0, 0, 0, 255]);
    // Without tangents, `w` is 0.
    assert_centre(probe_square(0, [0.0; 4]), [188, 255, 0, 255]);
}

/// Green, its vertices moved right by `params.offset.x`: in metres when
/// `y` is 0, of normalised device coordinates when it is 1.
const MOVED: &str = "struct Params { offset: vec4f };
fn material_fragment(fragment: Fragment) -> vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }
fn material_vertex(vertex: Vertex) -> VertexOffset {
    if params.offset.y == 0.0 { return VertexOffset(vec3f(params.offset.x, 0.0, 0.0), vec2f(0.0)); }
    return VertexOffset(vec3f(0.0), vec2f(params.offset.x, 0.0) * vertex.clip.w);
}";

#[test]
fn material_vertex_moves_the_surface_in_world_and_clip_space() {
    // 0.125 m, or 0.125 of normalised device coordinates, is 4 px: the
    // square moves from 16..48 to 20..52.
    for offset in [[0.125, 0.0, 0.0, 0.0], [0.125, 1.0, 0.0, 0.0]] {
        let scenes = custom_square(vec![shader(MOVED, &[])], custom(0, bytes(&offset)));
        let Some(frame) = render_scenes(scenes) else {
            return;
        };
        assert_pixel(&frame, 18, 32, CLEAR);
        assert_pixel(&frame, 50, 32, GREEN);
    }
}

#[test]
fn material_fragments_know_their_facing() {
    let scenes = |world| {
        let mut scenes = probe_square(1, [0.0; 4]);
        scenes.model.materials[0].double_sided = true;
        scenes.frame.draws[0].world = world;
        scenes
    };
    assert_centre(scenes(IDENTITY), RED);
    // Turned half about Y, the square shows its back.
    assert_centre(scenes(scaling(-1.0, 1.0, -1.0)), BLUE);
}

#[test]
fn material_normals_are_not_turned_on_back_faces() {
    let scenes = |world| {
        let mut scenes = probe_square(7, [0.0; 4]);
        scenes.model.materials[0].double_sided = true;
        scenes.frame.draws[0].world = world;
        scenes
    };
    // The front face's normal is +Z. Turned half about Y, the square shows
    // its back, whose normal is −Z, away from the viewer.
    assert_centre(scenes(IDENTITY), [255, 188, 0, 255]);
    assert_centre(scenes(scaling(-1.0, 1.0, -1.0)), [0, 188, 0, 255]);
}

#[test]
fn material_lights_are_their_colour_times_their_intensity_in_order() {
    // Three lights at intensity 0.5, each of its own primary: light k is
    // linear 0.5 in channel k, and light 3, which the frame lacks, black.
    let lights = vec![
        light(HALF_LIT, [1.0, 0.0, 0.0], 0.5),
        light(HALF_LIT, [0.0, 1.0, 0.0], 0.5),
        light(HALF_LIT, [0.0, 0.0, 1.0], 0.5),
    ];
    let expected = [[188, 0, 0, 255], [0, 188, 0, 255], [0, 0, 188, 255], BLACK];
    for (k, expected) in expected.into_iter().enumerate() {
        let mut scenes = probe_square(4, [k as f32, 0.0, 0.0, 0.0]);
        scenes.frame.lights.directional = lights.clone();
        let Some(actual) = centre(scenes) else {
            return;
        };
        assert!(near(actual, expected), "light {k}: {actual:?}");
    }
}

#[test]
fn material_frames_count_their_lights_and_know_the_time_and_aspect() {
    // Three lights (0.75 of 4), at time 0.5, on a canvas twice as wide as
    // it is high (0.5): the square, 32 px a metre, is at the middle of the
    // 128x64 canvas.
    let mut scenes = probe_square(5, [0.0; 4]);
    scenes.model.canvas = [128.0, 64.0];
    scenes.frame.lights.directional = vec![light(HALF_LIT, [1.0; 3], 1.0); 3];
    scenes.frame.time = 0.5;
    let Some(frame) = render_scenes_in(scenes, &layer_on(empty_scene(128, 64), 0.0, 1.0)) else {
        return;
    };
    assert_pixel(&frame, 64, 32, [225, 188, 188, 255]);
}

#[test]
fn material_frames_keep_the_view_and_the_projection_apart() {
    // Seen from 2 m, the view's z translation is −2: 0.5. Through a
    // perspective camera of `yfov` 1, the projection's `[1][1]` is
    // 1 / tan 0.5 = 1.8305: 0.4576, sRGB 180. Swapped, they would read
    // 0.025 and 0.25.
    let mut scenes = probe_square(6, [0.0; 4]);
    scenes.frame.camera.projection = Projection::Perspective {
        yfov: 1.0,
        near: 0.1,
        far: 100.0,
    };
    assert_centre(scenes, [188, 180, 0, 255]);
}

#[test]
fn material_view_points_to_the_viewer() {
    // Orthographic, `V` is the camera's +Z everywhere.
    let Some(frame) = render_scenes(probe_square(2, [0.0; 4])) else {
        return;
    };
    for (x, y) in [(17, 17), (32, 32), (46, 46)] {
        assert_pixel(&frame, x, y, [188, 0, 0, 255]);
    }
    // In perspective from 2 m, towards the eye: at pixel 45, x 0.461 m,
    // `V`'s x is −0.2246, linear 0.3877. (Pixel 46, x 0.495 m, is 0.24 at
    // its centre, but its farthest samples lie past the square's edge.)
    let mut scenes = probe_square(2, [0.0; 4]);
    scenes.frame.camera.projection = Projection::Perspective {
        yfov: 1.0,
        near: 0.1,
        far: 100.0,
    };
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 45, 32, [167, 0, 0, 255]);
}

#[test]
fn material_helpers_follow_their_formulas() {
    for (p, red) in [
        ([0.5, 0.5, 0.5], 0),
        ([0.5, 0.5, 0.6], 255),
        ([0.0, 1.0, 0.5], 188),
        ([1.0, 0.0, 0.25], 225),
    ] {
        let [start, end, t] = p;
        let Some(actual) = centre(probe_square(3, [start, end, t, 0.0])) else {
            return;
        };
        assert!(near(actual, [red, 0, 0, 255]), "{p:?}: {actual:?}");
    }
}

#[test]
fn material_alpha_modes_wrap_the_returned_alpha() {
    let half_red = |alpha_mode| {
        let mut material = custom(0, bytes(&[1.0, 0.0, 0.0, 0.5]));
        material.alpha_mode = alpha_mode;
        custom_square(vec![shader(FLAT, &[])], material)
    };
    assert_centre(half_red(AlphaMode::Opaque), RED);
    assert_centre(half_red(AlphaMode::BLEND), HALF_RED);
    assert_centre(half_red(AlphaMode::Mask { cutoff: 0.4 }), RED);
    assert_centre(half_red(AlphaMode::Mask { cutoff: 0.6 }), CLEAR);
    // A nearly clear depth-writing blend writes no depth: blue shows
    // through it.
    let mut red = custom(0, bytes(&[1.0, 0.0, 0.0, 0.0005]));
    red.alpha_mode = AlphaMode::Blend {
        depth_write: true,
        queue_offset: 0,
    };
    let mut scenes = front_and_back(red, see_through(SEE_BLUE, 0.5, false, 0));
    scenes.model.shaders = vec![shader(FLAT, &[])];
    assert_centre(scenes, HALF_BLUE);
}

#[test]
fn material_degenerate_worlds_draw_without_nan() {
    let mut scenes = lambert_square(vec![light(HALF_LIT, [1.0; 3], 1.0)]);
    scenes.frame.draws[0].world = scaling(0.0, 0.0, 0.0);
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 32, 32, CLEAR);
}

#[test]
fn material_broken_wgsl_is_refused_naming_the_material() {
    let broken = "struct Params { color: vec4f };
fn material_fragment(fragment: Fragment) -> vec4f { return nope; }";
    let Some(message) = invalid(custom_square(
        vec![shader(broken, &[])],
        custom(0, bytes(&[1.0; 4])),
    )) else {
        return;
    };
    assert!(message.contains("material 0's shader 0"), "{message}");
    assert!(message.contains("2:"), "{message}");

    let own = format!("@group(3) @binding(0) var<uniform> mine: vec4f;\n{FLAT}");
    let Some(message) = invalid(custom_square(
        vec![shader(&own, &[])],
        custom(0, bytes(&[1.0; 4])),
    )) else {
        return;
    };
    assert!(message.contains("declares the resource"), "{message}");

    // Listed but used by no material, it is refused all the same.
    let Some(message) = invalid(custom_square(
        vec![shader(FLAT, &[]), shader(broken, &[])],
        custom(0, bytes(&[1.0; 4])),
    )) else {
        return;
    };
    assert!(message.contains("shader 1"), "{message}");
    assert!(!message.contains("material"), "{message}");
}

#[test]
fn material_params_must_match_the_shader() {
    let Some(message) = invalid(custom_square(
        vec![shader(FLAT, &[])],
        custom(0, bytes(&[1.0; 3])),
    )) else {
        return;
    };
    assert!(
        message.contains("material 0's parameters are 12 bytes, but shader 0's Params is 16"),
        "{message}"
    );
    let Some(message) = invalid(custom_square(vec![shader(FLAT, &[])], custom(0, vec![]))) else {
        return;
    };
    assert!(message.contains("are 0 bytes"), "{message}");
}

#[test]
fn material_limits_are_refused() {
    let flat = || custom(0, bytes(&[1.0; 4]));
    let refused = |scenes: Scenes, expected: &str| {
        let Some(message) = invalid(scenes) else {
            return;
        };
        assert!(message.contains(expected), "{expected}: {message}");
    };
    refused(
        custom_square(vec![shader(FLAT, &[]); 33], flat()),
        "33 shaders, more than 32",
    );

    let mut material = flat();
    custom_of(&mut material).textures = vec![None, None];
    refused(
        custom_square(vec![shader(FLAT, &[])], material),
        "fills 2 texture slots, but shader 0 has 0",
    );

    let mut material = custom(0, 0_u32.to_le_bytes().to_vec());
    custom_of(&mut material).textures = vec![Some(5)];
    refused(
        custom_square(vec![shader(SLOTS, &[EmptyTexel::White; 3])], material),
        "texture slot 0 uses texture 5, of 0",
    );

    let mut material = flat();
    custom_of(&mut material).shader = 3;
    refused(
        custom_square(vec![shader(FLAT, &[])], material),
        "uses shader 3, of 1",
    );

    let mut long = FLAT.to_owned();
    long.push_str(&" ".repeat(262_145 - long.len()));
    refused(
        custom_square(vec![shader(&long, &[])], flat()),
        "more than 262144",
    );
}

#[test]
fn material_tangents_need_tangent_geometry() {
    let mut material = custom(0, bytes(&[1.0; 4]));
    custom_of(&mut material).requires_tangents = true;
    let Some(message) = invalid(custom_square(vec![shader(FLAT, &[])], material)) else {
        return;
    };
    assert!(
        message.contains("draw 0")
            && message.contains("material 0")
            && message.contains("tangents"),
        "{message}"
    );
}

#[test]
fn material_pipelines_the_backend_refuses_are_refused_naming_the_material() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    // naga accepts 16 MB of private memory a fragment; Apple GPUs' Metal
    // compiler refuses the pipeline (an internal error, not a validation
    // one), though another Metal device may have the stack for it. On
    // other backends the shader may well run, slowly, so it is not tried.
    if renderer.adapter_info().backend != wgpu::Backend::Metal {
        return;
    }
    let deep = "var<private> deep: array<vec4f, 1048576>;
struct Params { color: vec4f };
fn material_fragment(fragment: Fragment) -> vec4f { return params.color + deep[u32(fragment.pixel.x)]; }";
    renderer.renderer = renderer.renderer.with_content_provider(custom_square(
        vec![shader(deep, &[])],
        custom(0, bytes(&[1.0; 4])),
    ));
    // Drawn, or refused naming the material; never the device's panic.
    // Refused, it is refused again without being compiled again.
    for attempt in 0..2 {
        match renderer.render(&scene()) {
            Ok(_) => break,
            Err(GpuRenderError::InvalidPluginContent { message, .. }) => {
                assert!(message.contains("material 0's pipelines"), "{message}");
                assert_eq!(
                    pipelines(&mut renderer).pipelines_made,
                    1,
                    "attempt {attempt}"
                );
            }
            Err(error) => panic!("expected invalid plugin content, got {error}"),
        }
    }
    // Nothing of a refusal is kept: the next model draws.
    renderer.renderer = renderer.renderer.with_content_provider(custom_square(
        vec![shader(FLAT, &[])],
        custom(0, bytes(&[0.0, 1.0, 0.0, 1.0])),
    ));
    let frame = renderer.render(&scene()).unwrap();
    assert_pixel(&frame, 32, 32, GREEN);
}

#[test]
fn material_shaders_compile_once() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let mut scenes = one_square();
    let mut renamed = shader(FLAT, &[]);
    renamed.name = Some("flat again".to_owned());
    scenes.model.shaders = vec![shader(FLAT, &[]), renamed, shader(LAMBERT, &[])];
    scenes.model.materials = (0..3)
        .map(|index| custom(index, bytes(&[1.0; 4])))
        .collect();
    renderer.renderer = renderer.renderer.with_content_provider(scenes);
    renderer.render(&scene()).unwrap();
    let compiles = |renderer: &TestRenderer| {
        renderer
            .meshes
            .scene_pipelines
            .as_ref()
            .expect("a scene was drawn")
            .compiles
    };
    assert_eq!(compiles(&renderer), 2);

    // A broken shader is refused each time, but checked once.
    let Some(mut renderer) = self::renderer(transparent()) else {
        return;
    };
    let broken = "struct Params { color: vec4f };
fn material_fragment(fragment: Fragment) -> vec4f { return nope; }";
    renderer.renderer = renderer.renderer.with_content_provider(custom_square(
        vec![shader(broken, &[])],
        custom(0, bytes(&[1.0; 4])),
    ));
    for _ in 0..2 {
        match renderer.render(&scene()) {
            Err(GpuRenderError::InvalidPluginContent { message, .. }) => {
                assert!(message.contains("material 0's shader 0"), "{message}");
            }
            other => panic!("expected invalid plugin content, got {:?}", other.err()),
        }
    }
    assert_eq!(compiles(&renderer), 1);
}

#[test]
fn material_shaders_of_other_slot_counts_compile_apart() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    // The same WGSL with one slot and with two: two shaders, each bound
    // through its own layout. The first draws red on the left, the second
    // green on the right.
    let mut scenes = one_square();
    scenes.model.shaders = vec![
        shader(FLAT, &[EmptyTexel::White]),
        shader(FLAT, &[EmptyTexel::White; 2]),
    ];
    scenes.model.materials = vec![custom(0, bytes(&FLAT_RED)), custom(1, bytes(&FLAT_GREEN))];
    scenes.model.draws.push(DrawInfo {
        geometry: 0,
        material: 1,
        skin: None,
    });
    let mut right = scenes.frame.draws[0].clone();
    right.index = 1;
    right.world = translation(0.5, 0.0, 0.0);
    scenes.frame.draws[0].world = translation(-0.5, 0.0, 0.0);
    scenes.frame.draws.push(right);
    renderer.renderer = renderer.renderer.with_content_provider(scenes);
    let frame = renderer.render(&scene()).unwrap();
    assert_pixel(&frame, 16, 32, RED);
    assert_pixel(&frame, 48, 32, GREEN);
    assert_eq!(pipelines(&mut renderer).compiles, 2);
}

/// The renderer's scene pipelines, once a scene was opened.
fn pipelines(renderer: &mut TestRenderer) -> &mut ScenePipelines {
    renderer
        .meshes
        .scene_pipelines
        .as_mut()
        .expect("a scene was opened")
}

/// A renderer that opened a scene, which a model refused before its
/// shaders are compiled; `None` without a GPU.
fn opened_renderer() -> Option<TestRenderer> {
    let mut renderer = renderer(transparent())?;
    let mut refused = one_square();
    refused.model.draws[0].material = 1;
    renderer.renderer = renderer.renderer.with_content_provider(refused);
    assert!(renderer.render(&scene()).is_err());
    Some(renderer)
}

#[test]
fn material_pipelines_the_device_refuses_are_not_made_again() {
    let Some(mut renderer) = opened_renderer() else {
        return;
    };
    pipelines(&mut renderer).refuse_pipeline = Some(wgpu::Error::Internal {
        source: "the test's device".into(),
        description: "the test refuses it".to_owned(),
    });
    renderer.renderer = renderer.renderer.with_content_provider(custom_square(
        vec![shader(FLAT, &[])],
        custom(0, bytes(&FLAT_GREEN)),
    ));
    for _ in 0..2 {
        match renderer.render(&scene()) {
            Err(GpuRenderError::InvalidPluginContent { message, .. }) => {
                assert!(message.contains("material 0's pipelines"), "{message}");
                assert!(message.contains("the test refuses it"), "{message}");
            }
            other => panic!("expected invalid plugin content, got {:?}", other.err()),
        }
    }
    // The first pipeline was refused; the model sent again tries none.
    assert_eq!(pipelines(&mut renderer).pipelines_made, 1);

    // Running out of memory is not kept. Masked, the same shader asks for
    // pipelines of another fragment entry point, which the refusal does
    // not stand for.
    pipelines(&mut renderer).refuse_pipeline = Some(wgpu::Error::OutOfMemory {
        source: "the test's device".into(),
    });
    let mut material = custom(0, bytes(&FLAT_GREEN));
    material.alpha_mode = AlphaMode::Mask { cutoff: 0.5 };
    renderer.renderer = renderer
        .renderer
        .with_content_provider(custom_square(vec![shader(FLAT, &[])], material));
    match renderer.render(&scene()) {
        Err(GpuRenderError::InvalidPluginContent { message, .. }) => {
            assert!(message.contains("material 0's pipelines"), "{message}");
            assert!(message.contains("Out of Memory"), "{message}");
        }
        other => panic!("expected invalid plugin content, got {:?}", other.err()),
    }
    let frame = renderer.render(&scene()).unwrap();
    assert_pixel(&frame, 32, 32, GREEN);
    assert_eq!(pipelines(&mut renderer).pipelines_made, 4);
}

#[test]
fn material_shader_cache_drops_unused_shaders_past_its_cap() {
    let Some(renderer) = renderer(transparent()) else {
        return;
    };
    let device = &renderer.device;
    let mut pipelines = ScenePipelines::new(device, &renderer.queue);
    // `FLAT` as shader `n`: another digest each, the same module.
    let model = |n: usize, params: Vec<u8>| {
        let wgsl = format!("{FLAT}\n// {n}");
        custom_square(vec![shader(&wgsl, &[])], custom(0, params)).model
    };
    // Shader 0 is held, as a loaded model holds its shaders. Shader 1's
    // model is dropped once its pipelines are made, and shader 2's
    // pipeline is refused.
    let held = pipelines.prepare(device, &model(0, bytes(&FLAT_GREEN)));
    assert!(held.is_ok());
    pipelines
        .prepare(device, &model(1, bytes(&FLAT_GREEN)))
        .unwrap();
    pipelines.refuse_pipeline = Some(wgpu::Error::Internal {
        source: "the test's device".into(),
        description: "the test refuses it".to_owned(),
    });
    assert!(
        pipelines
            .prepare(device, &model(2, bytes(&FLAT_GREEN)))
            .is_err()
    );
    assert_eq!(pipelines.kept(), (3, 4, 1));
    // The rest compile, and their models are refused for their parameters.
    // The cache fills with shaders no model holds, and drops them.
    for n in 3..=MAX_CACHED_SHADERS {
        assert!(pipelines.prepare(device, &model(n, vec![])).is_err());
    }
    // Shader 0 and its two pipelines, and the last shader compiled.
    assert_eq!(pipelines.kept(), (2, 2, 0));
    let (compiles, made) = (pipelines.compiles, pipelines.pipelines_made);
    pipelines
        .prepare(device, &model(0, bytes(&FLAT_GREEN)))
        .unwrap();
    assert_eq!(
        (pipelines.compiles, pipelines.pipelines_made),
        (compiles, made)
    );
    // A dropped shader is checked and compiled again.
    pipelines
        .prepare(device, &model(1, bytes(&FLAT_GREEN)))
        .unwrap();
    assert_eq!(pipelines.compiles, compiles + 1);
    drop(held);
}

/// `ringed`'s model, its outline culling `cull` and pushed back by
/// `constant`: whatever the two, one shader and the same surface
/// pipelines, and two outline pipelines for each pair.
fn outlined(constant: i32, cull: CullFace) -> SceneModel {
    let mut scenes = ringed(WORLD);
    custom_of(&mut scenes.model.materials[0]).outline = Some(OutlinePass {
        cull,
        depth_bias: DepthBias {
            constant,
            slope_scale: 1.0,
        },
    });
    scenes.model
}

/// An error a backend's compiler might give.
fn refusal() -> wgpu::Error {
    wgpu::Error::Internal {
        source: "the test's device".into(),
        description: "the test refuses it".to_owned(),
    }
}

#[test]
fn material_pipeline_refusals_stand_for_any_culling_and_bias() {
    let Some(renderer) = renderer(transparent()) else {
        return;
    };
    let device = &renderer.device;
    let mut pipelines = ScenePipelines::new(device, &renderer.queue);
    // Bias 0's pipelines are made. Bias 1's first outline pipeline, the
    // first it lacks, is refused.
    pipelines
        .prepare(device, &outlined(0, CullFace::Front))
        .unwrap();
    pipelines.refuse_pipeline = Some(refusal());
    assert!(
        pipelines
            .prepare(device, &outlined(1, CullFace::Front))
            .is_err()
    );
    let (made, kept) = (pipelines.pipelines_made, pipelines.kept());
    assert_eq!(kept, (1, 4, 1));
    // Every other bias and culling compiles the same outline stages: each
    // is refused without being made, and nothing more is kept.
    let others = (2..10)
        .map(|n| (n, CullFace::Front))
        .chain([(1, CullFace::None), (1, CullFace::Back)]);
    for (constant, cull) in others {
        let error = pipelines
            .prepare(device, &outlined(constant, cull))
            .err()
            .unwrap_or_else(|| panic!("bias {constant}, {cull:?}: made"));
        assert!(error.contains("material 0's pipelines"), "{error}");
        assert!(error.contains("the test refuses it"), "{error}");
    }
    assert_eq!((pipelines.pipelines_made, pipelines.kept()), (made, kept));
}

#[test]
fn material_models_needing_too_many_pipelines_are_refused() {
    let Some(renderer) = renderer(transparent()) else {
        return;
    };
    let device = &renderer.device;
    let mut pipelines = ScenePipelines::new(device, &renderer.queue);
    // `RINGED` once for each of `biases` depth biases: the two surface
    // pipelines, and two outline pipelines for each bias.
    let model = |biases: i32| {
        let mut model = outlined(0, CullFace::Front);
        model.materials = (0..biases)
            .map(|constant| outlined(constant, CullFace::Front).materials[0].clone())
            .collect();
        model
    };
    // 512 biases need 1026 pipelines: refused before any is made.
    let error = pipelines.prepare(device, &model(512)).err().expect("made");
    assert!(
        error.contains(&format!(
            "the model's materials need 1026 pipelines, more than {MAX_MODEL_PIPELINES}"
        )),
        "{error}"
    );
    assert_eq!(pipelines.pipelines_made, 0);
    // 511 need 1024, which a model may: its first is made, here refused.
    pipelines.refuse_pipeline = Some(refusal());
    let error = pipelines.prepare(device, &model(511)).err().expect("made");
    assert!(error.contains("the test refuses it"), "{error}");
    assert_eq!(pipelines.pipelines_made, 1);
}

#[test]
fn material_pipeline_cache_drops_unused_pipelines_past_its_cap() {
    let Some(renderer) = renderer(transparent()) else {
        return;
    };
    let device = &renderer.device;
    let mut pipelines = ScenePipelines::new(device, &renderer.queue);
    let model = |n: i32| outlined(n, CullFace::Front);
    // Model 0 is held, as a loaded model holds its pipelines; every other
    // model is dropped once its pipelines are made. The shader stays held,
    // so the shader cache never drops them.
    let held = pipelines.prepare(device, &model(0)).unwrap();
    assert_eq!(pipelines.kept(), (1, 4, 0));
    for n in 1..=(MAX_CACHED_PIPELINES / 2) as i32 {
        pipelines.prepare(device, &model(n)).unwrap();
    }
    let (shaders, kept, _) = pipelines.kept();
    assert_eq!(shaders, 1);
    assert!(kept < MAX_CACHED_PIPELINES, "{kept} pipelines kept");
    // Model 0's are kept; a dropped model's are made again.
    let made = pipelines.pipelines_made;
    pipelines.prepare(device, &model(0)).unwrap();
    assert_eq!(pipelines.pipelines_made, made);
    pipelines.prepare(device, &model(1)).unwrap();
    assert_eq!(pipelines.pipelines_made, made + 2);
    drop(held);
}

#[test]
fn material_shaders_out_of_memory_is_tried_again() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    // A model refused before its shaders are compiled makes the pipelines
    // and keeps nothing.
    let mut refused = one_square();
    refused.model.draws[0].material = 1;
    renderer.renderer = renderer.renderer.with_content_provider(refused);
    assert!(renderer.render(&scene()).is_err());
    // The next shader compiled runs out of memory, which another load may
    // not.
    pipelines(&mut renderer).out_of_memory = true;
    renderer.renderer = renderer.renderer.with_content_provider(custom_square(
        vec![shader(FLAT, &[])],
        custom(0, bytes(&FLAT_GREEN)),
    ));
    match renderer.render(&scene()) {
        Err(GpuRenderError::InvalidPluginContent { message, .. }) => {
            assert!(message.contains("material 0's shader 0"), "{message}");
            assert!(message.contains("Out of Memory"), "{message}");
        }
        other => panic!("expected invalid plugin content, got {:?}", other.err()),
    }
    let frame = renderer.render(&scene()).unwrap();
    assert_pixel(&frame, 32, 32, GREEN);
    assert_eq!(pipelines(&mut renderer).compiles, 2);
}

const FLAT_RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const FLAT_GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

#[test]
fn frame_params_replace_the_material_params() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let mut scenes = custom_square(vec![shader(FLAT, &[])], custom(0, bytes(&FLAT_RED)));
    // Frame 1's draw is green; the frames around it are not.
    scenes.animate = Some(|frame, time| {
        if time == 1.0 {
            frame.draws[0].params = Some(bytes(&FLAT_GREEN));
        }
    });
    renderer.renderer = renderer.renderer.with_content_provider(scenes);
    let frame = renderer.render(&scene_at(1.0, 1.0)).unwrap();
    assert_pixel(&frame, 32, 32, GREEN);
    // Nothing of frame 1's parameters is kept.
    let frame = renderer.render(&scene_at(2.0, 1.0)).unwrap();
    assert_pixel(&frame, 32, 32, RED);
}

#[test]
fn frame_params_follow_each_draw() {
    // Two squares of material 0 side by side, 32 px each: only the right
    // one, draw 1, is given green.
    let mut scenes = custom_square(vec![shader(FLAT, &[])], custom(0, bytes(&FLAT_RED)));
    scenes.model.draws.push(scenes.model.draws[0]);
    let left = scenes.frame.draws[0].clone();
    scenes.frame.draws = vec![
        DrawState {
            world: translation(-0.5, 0.0, 0.0),
            ..left.clone()
        },
        DrawState {
            index: 1,
            world: translation(0.5, 0.0, 0.0),
            params: Some(bytes(&FLAT_GREEN)),
            ..left
        },
    ];
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 16, 32, RED);
    assert_pixel(&frame, 48, 32, GREEN);
}

#[test]
fn frame_params_must_match_the_material() {
    let mut scenes = custom_square(vec![shader(FLAT, &[])], custom(0, bytes(&FLAT_RED)));
    scenes.frame.draws[0].params = Some(bytes(&[1.0; 2]));
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(
        message.contains("draw 0's parameters are 8 bytes, but its material 0's shader 0 takes 16"),
        "{message}"
    );

    let mut scenes = one_square();
    scenes.model.materials = vec![plain(FLAT_RED)];
    scenes.frame.draws[0].params = Some(bytes(&FLAT_GREEN));
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(
        message.contains("draw 0's material 0 is unlit, which takes no parameters"),
        "{message}"
    );

    let mut scenes = custom_square(vec![shader(FLAT, &[])], custom(0, bytes(&FLAT_RED)));
    scenes.frame.draws[0].overrides.base_color = Some([0.0, 1.0, 0.0, 1.0]);
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(
        message.contains("draw 0's material 0 is custom: overrides are for unlit materials"),
        "{message}"
    );
}

/// Green, with a red outline of `params.width.w` alpha whose width is
/// `params.width.x`: in metres when `y` is 0, of the screen when it is 1,
/// times `texture_0`'s green when `z` is 1. One slot, empty white.
const RINGED: &str = "struct Params { width: vec4f };   // x width, y 0 world / 1 clip, z 1 to scale by texture_0's green
fn material_fragment(fragment: Fragment) -> vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }
fn material_outline_fragment(fragment: Fragment) -> vec4f { return vec4f(1.0, 0.0, 0.0, params.width.w); }
fn material_outline_vertex(vertex: Vertex) -> VertexOffset {
    var width = params.width.x;
    if params.width.z == 1.0 { width *= textureSampleLevel(texture_0, sampler_0, vertex.uv, 0.0).g; }
    if params.width.y == 0.0 { return VertexOffset(vertex.normal * width, vec2f(0.0)); }
    return VertexOffset(vec3f(0.0), normalize(vertex.normal.xy) * width * vertex.clip.w);
}";

/// `one_square`'s orthographic view, 32 px a metre, of `shell` drawn with
/// `RINGED` and the default outline pass, its parameters `width`: the
/// square covers 16..48 in x and y.
fn ringed(width: [f32; 4]) -> Scenes {
    let mut material = custom(0, bytes(&width));
    custom_of(&mut material).outline = Some(OutlinePass::default());
    let mut scenes = custom_square(vec![shader(RINGED, &[EmptyTexel::White])], material);
    scenes.model.geometries = vec![shell()];
    scenes
}

/// `RINGED`'s outline in metres, then of the screen, opaque.
const WORLD: [f32; 4] = [FOUR_PX_IN_METRES, 0.0, 0.0, 1.0];
/// 0.125 of normalised device coordinates a side, 4 px.
const CLIP: [f32; 4] = [FOUR_PX_IN_METRES, 1.0, 0.0, 1.0];

#[test]
fn outline_vertex_offsets_in_world_and_clip_space() {
    for width in [WORLD, CLIP] {
        let Some(frame) = render_scenes(ringed(width)) else {
            return;
        };
        // The ring 12..16 and 48..52; the depth bias keeps the surface in
        // front of its outline.
        assert_pixel(&frame, 14, 32, RED);
        assert_pixel(&frame, 50, 32, RED);
        assert_pixel(&frame, 10, 32, CLEAR);
        assert_pixel(&frame, 32, 32, GREEN);
    }
}

#[test]
fn outline_texture_slots_reach_the_vertex_stage() {
    // Green 1 on the left corners (u 0), 0 on the right ones (u 1): the
    // ring grows on the left only.
    let mut scenes = ringed([FOUR_PX_IN_METRES, 0.0, 1.0, 1.0]);
    custom_of(&mut scenes.model.materials[0]).textures = vec![Some(0)];
    scenes.model.textures = vec![SceneTexture {
        color_space: ColorSpace::Linear,
        ..nearest(2, 1, &[[0, 255, 0, 255], [0, 0, 0, 255]])
    }];
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 14, 32, RED);
    assert_pixel(&frame, 50, 32, CLEAR);
}

#[test]
fn outline_culls_and_biases_as_asked() {
    // 0 wide, the outline lies on the surface. By default it culls the
    // front faces and is pushed behind the twin's back face.
    let Some(frame) = render_scenes(ringed([0.0, 0.0, 0.0, 1.0])) else {
        return;
    };
    assert_pixel(&frame, 32, 32, GREEN);
    // Culling nothing and unbiased, it draws the front face too, after the
    // surface and coplanar with it, which `LessEqual` lets through.
    let mut scenes = ringed([0.0, 0.0, 0.0, 1.0]);
    custom_of(&mut scenes.model.materials[0]).outline = Some(OutlinePass {
        cull: CullFace::None,
        depth_bias: DepthBias {
            constant: 0,
            slope_scale: 0.0,
        },
    });
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 32, 32, RED);
}

#[test]
fn outline_mirrored_worlds_cull_the_front_faces() {
    let mut scenes = ringed(WORLD);
    scenes.frame.draws[0].world = scaling(-1.0, 1.0, 1.0);
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 14, 32, RED);
    assert_pixel(&frame, 50, 32, RED);
    assert_pixel(&frame, 10, 32, CLEAR);
    assert_pixel(&frame, 32, 32, GREEN);
}

/// `ringed(WORLD)` on `radial_square` alone, one winding facing the
/// camera, its outline culling `cull` (the default pass's when `None`),
/// drawn in `world`: where the outline keeps the square's front face, a
/// ring 12..16 and 48..52 in x.
fn ringed_square(cull: Option<CullFace>, world: [f32; 16]) -> Scenes {
    let mut scenes = ringed(WORLD);
    scenes.model.geometries = vec![radial_square()];
    if let Some(cull) = cull {
        custom_of(&mut scenes.model.materials[0])
            .outline
            .as_mut()
            .unwrap()
            .cull = cull;
    }
    scenes.frame.draws[0].world = world;
    scenes
}

#[test]
fn outline_culls_the_faces_it_asks_to() {
    // `shell`'s twin would keep a ring whatever is culled; the square
    // alone has a ring only where its front face is kept.
    for (cull, expected) in [
        (None, CLEAR),
        (Some(CullFace::Back), RED),
        (Some(CullFace::None), RED),
    ] {
        let Some(frame) = render_scenes(ringed_square(cull, IDENTITY)) else {
            return;
        };
        assert_pixel(&frame, 14, 32, expected);
        assert_pixel(&frame, 32, 32, GREEN);
    }
}

#[test]
fn outline_mirrored_squares_still_cull_the_front_face() {
    // Mirrored in x, the square's front face turns clockwise on screen,
    // and the outline still culls it. With the winding not taken from the
    // world's determinant, the outline would take it for a back face and
    // draw a ring around it.
    let Some(frame) = render_scenes(ringed_square(None, scaling(-1.0, 1.0, 1.0))) else {
        return;
    };
    assert_pixel(&frame, 14, 32, CLEAR);
    assert_pixel(&frame, 32, 32, GREEN);
}

#[test]
fn outline_draws_with_the_material_alpha() {
    let mut scenes = ringed([FOUR_PX_IN_METRES, 0.0, 0.0, 0.5]);
    scenes.model.materials[0].alpha_mode = AlphaMode::BLEND;
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    let alpha = pixel(&frame, 14, 32)[3];
    assert!(alpha.abs_diff(128) <= 2, "alpha {alpha}");

    let mut scenes = ringed([FOUR_PX_IN_METRES, 0.0, 0.0, 0.3]);
    scenes.model.materials[0].alpha_mode = AlphaMode::Mask { cutoff: 0.5 };
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    assert_pixel(&frame, 14, 32, CLEAR);
    assert_pixel(&frame, 32, 32, GREEN);
}

#[test]
fn outline_every_alpha_side_and_mode_draws() {
    // One `RINGED` material for each alpha mode (opaque, mask, blend, and
    // blend writing depth), side (single, double), and outline (none, in
    // metres, of the screen), each drawing the shell, every other draw
    // mirrored: opening the model makes every surface and outline pipeline
    // in both windings, and the frame draws with both.
    let alphas = [
        AlphaMode::Opaque,
        AlphaMode::Mask { cutoff: 0.5 },
        AlphaMode::BLEND,
        AlphaMode::Blend {
            depth_write: true,
            queue_offset: 0,
        },
    ];
    let mut materials = Vec::new();
    for alpha_mode in alphas {
        for double_sided in [false, true] {
            for width in [None, Some(WORLD), Some(CLIP)] {
                let mut material = custom(0, bytes(&width.unwrap_or(WORLD)));
                custom_of(&mut material).outline = width.map(|_| OutlinePass::default());
                materials.push(Material {
                    alpha_mode,
                    double_sided,
                    ..material
                });
            }
        }
    }
    assert_eq!(materials.len(), 24);
    let mut scenes = ringed(WORLD);
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
    scenes.model.materials = materials;
    let Some(frame) = render_scenes(scenes) else {
        return;
    };
    // The surfaces cover the centre; the outlines the ring.
    assert_pixel(&frame, 32, 32, GREEN);
    assert_pixel(&frame, 14, 32, RED);
    assert_pixel(&frame, 50, 32, RED);
    assert_pixel(&frame, 10, 32, CLEAR);
}

#[test]
fn outline_needs_its_fragment_function() {
    let mut material = custom(0, bytes(&FLAT_RED));
    custom_of(&mut material).outline = Some(OutlinePass::default());
    let Some(message) = invalid(custom_square(vec![shader(FLAT, &[])], material)) else {
        return;
    };
    assert!(
        message.contains(
            "material 0 asks for an outline, but shader 0 defines no `material_outline_fragment`"
        ),
        "{message}"
    );

    let mut scenes = ringed(WORLD);
    custom_of(&mut scenes.model.materials[0])
        .outline
        .as_mut()
        .unwrap()
        .depth_bias
        .slope_scale = f32::NAN;
    let Some(message) = invalid(scenes) else {
        return;
    };
    assert!(
        message.contains("outline depth bias is not finite"),
        "{message}"
    );
}
