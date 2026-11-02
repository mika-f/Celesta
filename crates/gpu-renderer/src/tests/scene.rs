//! `LayerContent::Plugin` with scene content: a skinned, morphing scene
//! from a content provider. A provider made up here stands in for a plugin
//! process.

use super::*;
use celesta_plugin::{
    AlphaMode, Camera, ColorSpace, ContentFrame, ContentProvider, DrawInfo, DrawState, Filter,
    Geometry, Lights, Material, MaterialKind, ModelId, MorphTarget, Opened, PluginError,
    Projection, Sampler, SceneFrame, SceneModel, SceneTexture, Skin, TextureSource, UnlitMaterial,
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
    let mut scene = empty_scene(64, 64);
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
    let mut renderer = renderer(transparent())?;
    renderer.renderer = renderer.renderer.with_content_provider(scenes);
    Some(renderer.render(&scene()).unwrap())
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
    let MaterialKind::Unlit(unlit) = &mut scenes.model.materials[0].kind;
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
    let MaterialKind::Unlit(unlit) = &mut material.kind;
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
        alpha_mode: AlphaMode::Blend,
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
    let MaterialKind::Unlit(unlit) = &mut scenes.model.materials[0].kind;
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

const BLACK: [u8; 4] = [0, 0, 0, 255];
