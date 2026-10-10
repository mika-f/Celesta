//! Custom shader filters (`shader.rs`). The CPU reference renderer cannot
//! run WGSL, so these compare against expected pixels.

use super::*;
use celesta_composition::{LayerEffects, LayerShader, ShaderParam, ShaderParamType, ShaderSource};

const IDENTITY: &str =
    "fn effect(input: EffectInput) -> vec4f {\n    return source_at(input.position);\n}\n";

fn black_background() -> GpuRenderOptions {
    GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }
}

fn source(id: &str, wgsl: &str, params: &[(&str, ShaderParamType)]) -> ShaderSource {
    ShaderSource {
        id: id.to_owned(),
        name: Some(id.to_owned()),
        wgsl: wgsl.to_owned(),
        params: params
            .iter()
            .map(|&(name, ty)| ShaderParam {
                name: name.to_owned(),
                ty,
            })
            .collect(),
    }
}

fn shaded(mut layer: Layer, id: &str, params: Vec<f64>, padding: f64) -> Layer {
    layer.effects.shader = Some(LayerShader {
        id: id.to_owned(),
        params,
        padding,
    });
    layer
}

fn scene(shaders: Vec<ShaderSource>, layers: Vec<Layer>) -> Scene {
    let mut scene = empty_scene(64, 64);
    scene.shaders = shaders;
    scene.layers = layers;
    scene
}

fn render_error(renderer: &mut GpuRenderer, scene: &Scene) -> String {
    match renderer.render(scene) {
        Ok(_) => panic!("the frame rendered"),
        Err(error) => error.to_string(),
    }
}

fn assert_near(actual: [u8; 4], expected: [u8; 4], tolerance: u8, case: &str) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= tolerance),
        "{case}: {actual:?} is not within {tolerance} of {expected:?}"
    );
}

#[test]
fn an_identity_shader_leaves_the_layer_as_it_was() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let rect = || corner_rect("rect", 10.5, 12.25, 30.0, 20.0, "#4080c0b0");
    let plain = renderer.render(&scene(Vec::new(), vec![rect()])).unwrap();
    let identity = renderer
        .render(&scene(
            vec![source("identity", IDENTITY, &[])],
            vec![shaded(rect(), "identity", Vec::new(), 0.0)],
        ))
        .unwrap();
    let difference = plain
        .pixels()
        .iter()
        .zip(identity.pixels())
        .map(|(plain, identity)| plain.abs_diff(*identity))
        .max()
        .unwrap();
    assert!(difference <= 1, "channels differ by up to {difference}");
}

#[test]
fn shades_each_pixel_of_the_layer() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let invert = source(
        "invert",
        "fn effect(input: EffectInput) -> vec4f {\n    let color = source_load(vec2i(floor(input.position)));\n    return vec4f(color.a - color.rgb, color.a);\n}\n",
        &[],
    );
    let frame = renderer
        .render(&scene(
            vec![invert],
            vec![shaded(
                corner_rect("rect", 8.0, 8.0, 32.0, 32.0, "#ff8000"),
                "invert",
                Vec::new(),
                0.0,
            )],
        ))
        .unwrap();
    assert_eq!(pixel_at(&frame, 20, 20), [0, 127, 255, 255]);
    assert_eq!(pixel_at(&frame, 50, 50), [0, 0, 0, 255]);
}

#[test]
fn uv_spans_the_layers_shapes() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let uv = source(
        "uv",
        "fn effect(input: EffectInput) -> vec4f {\n    return vec4f(input.uv, 0.0, 1.0);\n}\n",
        &[],
    );
    let near = |value: f32| (value * 255.0).round() as u8;
    // A rect alone; a group of two rects; a group whose rect is blurred,
    // which spreads its pixels but not its shape.
    let mut blurred = corner_rect("blurred", 16.0, 8.0, 32.0, 40.0, "#ffffff");
    blurred.effects.blur = 4.0;
    let cases = [
        (
            "rect",
            corner_rect("rect", 16.0, 8.0, 32.0, 40.0, "#ffffff"),
        ),
        (
            "group",
            group(
                EvaluatedTransform::default(),
                vec![
                    corner_rect("top", 16.0, 8.0, 10.0, 10.0, "#ffffff"),
                    corner_rect("bottom", 38.0, 38.0, 10.0, 10.0, "#ffffff"),
                ],
            ),
        ),
        (
            "blurred",
            group(EvaluatedTransform::default(), vec![blurred]),
        ),
    ];
    for (case, layer) in cases {
        let frame = renderer
            .render(&scene(
                vec![uv.clone()],
                vec![shaded(layer, "uv", Vec::new(), 0.0)],
            ))
            .unwrap();
        // Pixel centres half a pixel inside the box's corners.
        assert_near(
            pixel_at(&frame, 16, 8),
            [near(0.5 / 32.0), near(0.5 / 40.0), 0, 255],
            1,
            case,
        );
        assert_near(
            pixel_at(&frame, 47, 47),
            [near(31.5 / 32.0), near(39.5 / 40.0), 0, 255],
            1,
            case,
        );
    }
}

#[test]
fn padding_lets_a_shader_write_beyond_the_content() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let shift = source(
        "shift",
        "fn effect(input: EffectInput) -> vec4f {\n    return source_at(input.position - vec2f(params.distance, 0.0));\n}\n",
        &[("distance", ShaderParamType::F32)],
    );
    let render = |renderer: &mut GpuRenderer, padding: f64| {
        renderer
            .render(&scene(
                vec![shift.clone()],
                vec![shaded(
                    corner_rect("rect", 8.0, 8.0, 20.0, 20.0, "#ffffff"),
                    "shift",
                    vec![10.0],
                    padding,
                )],
            ))
            .unwrap()
    };
    let padded = render(&mut renderer, 10.0);
    assert_eq!(
        pixel_at(&padded, 12, 16),
        [0, 0, 0, 255],
        "left of the moved rect"
    );
    assert_eq!(pixel_at(&padded, 20, 16), [255, 255, 255, 255]);
    assert_eq!(
        pixel_at(&padded, 34, 16),
        [255, 255, 255, 255],
        "in the padding"
    );
    assert_eq!(pixel_at(&padded, 40, 16), [0, 0, 0, 255]);

    let unpadded = render(&mut renderer, 0.0);
    assert_eq!(pixel_at(&unpadded, 20, 16), [255, 255, 255, 255]);
    assert_eq!(
        pixel_at(&unpadded, 34, 16),
        [0, 0, 0, 255],
        "cut at the content box"
    );
}

#[test]
fn reads_transparency_beyond_the_canvas() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let far = source(
        "far",
        "fn effect(input: EffectInput) -> vec4f {\n    return source_at(input.position + vec2f(10000.0, 0.0)) + source_load(vec2i(-5, -5));\n}\n",
        &[],
    );
    let frame = renderer
        .render(&scene(
            vec![far],
            vec![shaded(
                corner_rect("rect", 0.0, 0.0, 64.0, 64.0, "#ffffff"),
                "far",
                Vec::new(),
                0.0,
            )],
        ))
        .unwrap();
    assert_eq!(pixel_at(&frame, 32, 32), [0, 0, 0, 255]);
    assert_eq!(pixel_at(&frame, 0, 0), [0, 0, 0, 255]);
}

#[test]
fn clamps_the_result_to_a_premultiplied_color() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let wild = source(
        "wild",
        "fn effect(input: EffectInput) -> vec4f {\n    return vec4f(2.0, -1.0, 0.5, 0.5);\n}\n",
        &[],
    );
    let frame = renderer
        .render(&scene(
            vec![wild],
            vec![shaded(
                corner_rect("rect", 8.0, 8.0, 32.0, 32.0, "#ffffff"),
                "wild",
                Vec::new(),
                0.0,
            )],
        ))
        .unwrap();
    // (0.5, 0, 0.5) at alpha 0.5 over black.
    assert_near(pixel_at(&frame, 20, 20), [128, 0, 128, 255], 1, "clamped");
}

#[test]
fn passes_parameters_of_every_type_in_order() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let params = [
        ("a", ShaderParamType::F32),
        ("b", ShaderParamType::F32),
        ("c", ShaderParamType::Vec2),
        ("d", ShaderParamType::Vec3),
        ("e", ShaderParamType::Vec4),
    ];
    let values = vec![
        0.1, 0.2, // a, b
        0.3, 0.4, // c
        0.5, 0.6, 0.7, // d
        0.8, 0.9, 1.0, 0.0, // e
    ];
    let cases = [
        ("params.a, params.b, params.c.x", [0.1, 0.2, 0.3]),
        ("params.c.y, params.d.x, params.d.z", [0.4, 0.5, 0.7]),
        ("params.e.x, params.e.y, params.e.z", [0.8, 0.9, 1.0]),
    ];
    for (index, (channels, expected)) in cases.into_iter().enumerate() {
        let id = format!("params{index}");
        let wgsl = format!(
            "fn effect(input: EffectInput) -> vec4f {{\n    return vec4f({channels}, 1.0 + params.e.w);\n}}\n"
        );
        let frame = renderer
            .render(&scene(
                vec![source(&id, &wgsl, &params)],
                vec![shaded(
                    corner_rect("rect", 8.0, 8.0, 32.0, 32.0, "#ffffff"),
                    &id,
                    values.clone(),
                    0.0,
                )],
            ))
            .unwrap();
        let [r, g, b] = expected.map(|value: f32| (value * 255.0).round() as u8);
        assert_near(pixel_at(&frame, 20, 20), [r, g, b, 255], 1, channels);
    }
}

#[test]
fn other_effects_filter_the_shaded_result() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let shift = source(
        "shift",
        "fn effect(input: EffectInput) -> vec4f {\n    return source_at(input.position - vec2f(12.0, 0.0));\n}\n",
        &[],
    );
    let mut layer = shaded(
        corner_rect("rect", 4.0, 4.0, 16.0, 16.0, "#ffffff"),
        "shift",
        Vec::new(),
        12.0,
    );
    layer.effects.shadow = Some(LayerShadow {
        color: "#ff0000".to_owned(),
        blur: 0.0,
        offset_x: 0.0,
        offset_y: 20.0,
    });
    let frame = renderer.render(&scene(vec![shift], vec![layer])).unwrap();
    assert_eq!(pixel_at(&frame, 24, 12), [255, 255, 255, 255], "shifted");
    // The shadow lies under the shifted rect, not under the original one.
    assert_eq!(pixel_at(&frame, 24, 32), [255, 0, 0, 255]);
    assert_eq!(pixel_at(&frame, 6, 32), [0, 0, 0, 255]);
}

#[test]
fn shades_a_group_after_its_mask() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    // Green wherever the masked group left a hole, red where it shows.
    let holes = source(
        "holes",
        "fn effect(input: EffectInput) -> vec4f {\n    let shown = source_at(input.position).a > 0.5;\n    return select(vec4f(0.0, 1.0, 0.0, 1.0), vec4f(1.0, 0.0, 0.0, 1.0), shown);\n}\n",
        &[],
    );
    let mut group = masked_group(
        EvaluatedTransform::default(),
        GroupMask {
            invert: true,
            ..alpha_mask(vec![corner_rect("hole", 24.0, 24.0, 16.0, 16.0, "#ffffff")])
        },
        vec![corner_rect("rect", 8.0, 8.0, 48.0, 48.0, "#ffffff")],
    );
    group = shaded(group, "holes", Vec::new(), 0.0);
    let frame = renderer.render(&scene(vec![holes], vec![group])).unwrap();
    assert_eq!(pixel_at(&frame, 12, 12), [255, 0, 0, 255]);
    assert_eq!(pixel_at(&frame, 32, 32), [0, 255, 0, 255], "the hole");
}

#[test]
fn layers_share_a_shader_with_their_own_values() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let tint = source(
        "tint",
        "fn effect(input: EffectInput) -> vec4f {\n    return premultiply(params.color) * source_at(input.position).a;\n}\n",
        &[("color", ShaderParamType::Vec4)],
    );
    let frame = renderer
        .render(&scene(
            vec![tint],
            vec![
                shaded(
                    corner_rect("left", 4.0, 4.0, 16.0, 16.0, "#ffffff"),
                    "tint",
                    vec![1.0, 0.0, 0.0, 1.0],
                    0.0,
                ),
                shaded(
                    corner_rect("right", 40.0, 4.0, 16.0, 16.0, "#ffffff"),
                    "tint",
                    vec![0.0, 0.0, 1.0, 1.0],
                    0.0,
                ),
            ],
        ))
        .unwrap();
    assert_eq!(pixel_at(&frame, 10, 10), [255, 0, 0, 255]);
    assert_eq!(pixel_at(&frame, 46, 10), [0, 0, 255, 255]);
}

#[test]
fn shades_in_scene_space_whatever_the_rotation() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let split = source(
        "split",
        "fn effect(input: EffectInput) -> vec4f {\n    let alpha = source_at(input.position).a;\n    let left = input.position.x < celesta.scene_size.x * 0.5;\n    return select(vec4f(0.0, 0.0, alpha, alpha), vec4f(alpha, 0.0, 0.0, alpha), left);\n}\n",
        &[],
    );
    let mut rect = corner_rect("rect", 32.0, 32.0, 24.0, 24.0, "#ffffff");
    rect.transform.anchor = Point { x: 0.5, y: 0.5 };
    rect.transform.rotation = 45.0;
    let frame = renderer
        .render(&scene(
            vec![split],
            vec![shaded(rect, "split", Vec::new(), 0.0)],
        ))
        .unwrap();
    assert_eq!(pixel_at(&frame, 24, 32), [255, 0, 0, 255]);
    assert_eq!(pixel_at(&frame, 40, 32), [0, 0, 255, 255]);
}

#[test]
fn reports_errors_in_the_authors_code_by_line_and_column() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let rect = || corner_rect("rect", 8.0, 8.0, 16.0, 16.0, "#ffffff");
    let error = |renderer: &mut GpuRenderer, wgsl: &str| {
        render_error(
            renderer,
            &scene(
                vec![source("broken", wgsl, &[])],
                vec![shaded(rect(), "broken", Vec::new(), 0.0)],
            ),
        )
    };
    let typo = error(
        &mut renderer,
        "fn effect(input: EffectInput) -> vec4f {\n    return source_at(input.positon);\n}\n",
    );
    assert!(
        typo.starts_with("invalid custom shader broken: 2:"),
        "{typo}"
    );
    assert!(typo.contains("positon"), "{typo}");

    let missing = error(&mut renderer, "fn other() {}\n");
    assert!(
        missing.contains("fn effect(input: EffectInput) -> vec4f"),
        "{missing}"
    );

    let signature = error(
        &mut renderer,
        "fn effect(input: EffectInput) -> f32 {\n    return 1.0;\n}\n",
    );
    assert!(
        signature.contains("fn effect(input: EffectInput) -> vec4f"),
        "{signature}"
    );

    let binding = error(
        &mut renderer,
        &format!("@group(2) @binding(0) var<uniform> extra: vec4f;\n{IDENTITY}"),
    );
    assert!(binding.contains("resource `extra`"), "{binding}");
    assert!(
        binding.starts_with("invalid custom shader broken: 1:"),
        "{binding}"
    );

    // A shader without parameters has no `params` of its own to bind.
    let params = error(
        &mut renderer,
        &format!("@group(1) @binding(1) var<uniform> params: vec4f;\n{IDENTITY}"),
    );
    assert!(params.contains("resource `params`"), "{params}");

    let entry = error(
        &mut renderer,
        &format!("@compute @workgroup_size(1) fn main() {{}}\n{IDENTITY}"),
    );
    assert!(entry.contains("entry point `main`"), "{entry}");
    assert!(
        entry.starts_with("invalid custom shader broken: 1:"),
        "{entry}"
    );

    let reserved = error(
        &mut renderer,
        &format!("{IDENTITY}fn celesta_helper() -> f32 {{\n    return 1.0;\n}}\n"),
    );
    assert!(reserved.contains("`celesta_helper`"), "{reserved}");
    assert!(
        reserved.starts_with("invalid custom shader broken: 4:"),
        "{reserved}"
    );

    let keyword = render_error(
        &mut renderer,
        &scene(
            vec![source(
                "keyword",
                IDENTITY,
                &[("celestaTime", ShaderParamType::F32)],
            )],
            vec![shaded(rect(), "keyword", vec![0.0], 0.0)],
        ),
    );
    assert!(keyword.contains("`celestaTime`"), "{keyword}");

    // The renderer still renders after the errors.
    renderer
        .render(&scene(
            vec![source("identity", IDENTITY, &[])],
            vec![shaded(rect(), "identity", Vec::new(), 0.0)],
        ))
        .unwrap();
}

#[test]
fn rejects_uses_that_do_not_match_the_scenes_shaders() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let rect = || corner_rect("rect", 8.0, 8.0, 16.0, 16.0, "#ffffff");
    let one = source("one", IDENTITY, &[("amount", ShaderParamType::Vec2)]);

    let unknown = render_error(
        &mut renderer,
        &scene(
            vec![one.clone()],
            vec![shaded(rect(), "two", Vec::new(), 0.0)],
        ),
    );
    assert!(
        unknown.contains("layer `rect` uses a shader the scene does not list"),
        "{unknown}"
    );

    let count = render_error(
        &mut renderer,
        &scene(
            vec![one.clone()],
            vec![shaded(rect(), "one", vec![1.0], 0.0)],
        ),
    );
    assert!(count.contains("gives 1 parameter values"), "{count}");

    // Finite as `f64`, but not as the `f32` the shader gets.
    let overflow = render_error(
        &mut renderer,
        &scene(
            vec![one.clone()],
            vec![shaded(rect(), "one", vec![1e39, 0.0], 0.0)],
        ),
    );
    assert!(overflow.contains("not finite"), "{overflow}");

    let twice = render_error(&mut renderer, &scene(vec![one.clone(), one], Vec::new()));
    assert!(twice.contains("two shaders with this id"), "{twice}");
}

#[test]
fn compiles_each_shader_once_until_it_changes() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let rect = || corner_rect("rect", 8.0, 8.0, 16.0, 16.0, "#ffffff");
    let broken = scene(
        vec![source("edited", "fn effect(", &[])],
        vec![shaded(rect(), "edited", Vec::new(), 0.0)],
    );
    for _ in 0..3 {
        render_error(&mut renderer, &broken);
    }
    assert_eq!(
        renderer.shaders.compiles, 1,
        "a broken shader compiles once"
    );

    let fixed = scene(
        vec![source("edited", IDENTITY, &[])],
        vec![shaded(rect(), "edited", Vec::new(), 0.0)],
    );
    for _ in 0..3 {
        renderer.render(&fixed).unwrap();
    }
    assert_eq!(
        renderer.shaders.compiles, 2,
        "a changed source compiles again"
    );

    let other = scene(
        vec![source("other", IDENTITY, &[])],
        vec![shaded(rect(), "other", Vec::new(), 0.0)],
    );
    renderer.render(&other).unwrap();
    assert_eq!(renderer.shaders.cached(), 2);
    let renderer: &mut GpuRenderer = &mut renderer;
    for _ in 0..300 {
        renderer
            .shaders
            .begin_scene(&renderer.device, &other)
            .unwrap();
    }
    assert_eq!(renderer.shaders.cached(), 1, "unused shaders are dropped");
}

#[test]
fn draws_nothing_for_a_layer_that_draws_nothing() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let fill = source(
        "fill",
        "fn effect(input: EffectInput) -> vec4f {\n    return vec4f(1.0);\n}\n",
        &[],
    );
    let empty = Layer {
        effects: LayerEffects::default(),
        ..group(EvaluatedTransform::default(), Vec::new())
    };
    let frame = renderer
        .render(&scene(
            vec![fill],
            vec![shaded(empty, "fill", Vec::new(), 8.0)],
        ))
        .unwrap();
    assert!(
        frame
            .pixels()
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 0, 0, 255])
    );
}

#[test]
fn uv_follows_a_texture_moved_onto_whole_pixels() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let uv = source(
        "uv",
        "fn effect(input: EffectInput) -> vec4f {\n    return vec4f(input.uv, 0.0, 1.0);\n}\n",
        &[],
    );
    renderer.image_sources.insert_raster(
        "square",
        image::RgbaImage::from_raw(8, 8, [255, 255, 255, 255].repeat(64)).unwrap(),
    );
    // Drawn texel for texel, the image lands on x = 10, not 10.4.
    let image = Layer {
        id: "square".to_owned(),
        transform: EvaluatedTransform {
            position: Point { x: 10.4, y: 8.0 },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Image {
            width: None,
            height: None,
            fit: None,
            asset: ResolvedAsset {
                id: "square".to_owned(),
                location: AssetLocation::File {
                    path: "square.png".to_owned(),
                },
            },
        },
    };
    let frame = renderer
        .render(&scene(vec![uv], vec![shaded(image, "uv", Vec::new(), 0.0)]))
        .unwrap();
    let near = |value: f32| (value * 255.0).round() as u8;
    assert_near(
        pixel_at(&frame, 10, 8),
        [near(0.5 / 8.0), near(0.5 / 8.0), 0, 255],
        1,
        "top left",
    );
    assert_near(
        pixel_at(&frame, 17, 15),
        [near(7.5 / 8.0), near(7.5 / 8.0), 0, 255],
        1,
        "bottom right",
    );
}

#[test]
fn uv_spans_a_box_narrower_than_a_pixel() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let uv = source(
        "uv",
        "fn effect(input: EffectInput) -> vec4f {\n    return vec4f(input.uv.x, 0.0, 0.0, 1.0);\n}\n",
        &[],
    );
    let frame = renderer
        .render(&scene(
            vec![uv],
            vec![shaded(
                corner_rect("thin", 8.0, 8.0, 0.5, 32.0, "#ffffff"),
                "uv",
                Vec::new(),
                0.0,
            )],
        ))
        .unwrap();
    // The pixel centre 8.5 is the box's right edge.
    assert_near(pixel_at(&frame, 8, 20), [255, 0, 0, 255], 1, "right edge");
}

#[test]
fn refuses_more_shader_uses_than_one_buffer_holds() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    renderer.shaders.max_uses = 1;
    let layers = (0..2)
        .map(|index| {
            shaded(
                corner_rect(&format!("rect-{index}"), 8.0, 8.0, 4.0, 4.0, "#ffffff"),
                "identity",
                Vec::new(),
                0.0,
            )
        })
        .collect();
    let error = renderer
        .render(&scene(vec![source("identity", IDENTITY, &[])], layers))
        .unwrap_err();
    assert!(matches!(error, GpuRenderError::TooManyLayers(2)), "{error}");
}
