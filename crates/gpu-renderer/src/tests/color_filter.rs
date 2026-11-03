//! Built-in color filters (`color_filter.wgsl`). The CPU reference renderer
//! refuses filters, so these compare against the arithmetic below, which
//! `LayerFilter`'s documentation and `@celesta.studio/web` follow.

use super::*;
use celesta_composition::{LayerFilter, LayerShader, ShaderSource};

fn black_background() -> GpuRenderOptions {
    GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }
}

fn filtered(mut layer: Layer, filters: Vec<LayerFilter>) -> Layer {
    layer.effects.filters.extend(filters);
    layer
}

fn scene(shaders: Vec<ShaderSource>, layers: Vec<Layer>) -> Scene {
    let mut scene = empty_scene(64, 64);
    scene.shaders = shaders;
    scene.layers = layers;
    scene
}

const LUMA: [f64; 3] = [0.2126, 0.7152, 0.0722];

fn saturated(rgb: [f64; 3], amount: f64) -> [f64; 3] {
    let luma: f64 = rgb.iter().zip(LUMA).map(|(c, w)| c * w).sum();
    rgb.map(|c| luma + (c - luma) * amount)
}

/// `filter` applied to the straight color `rgb` at `uv` of the content box.
fn reference(filter: &LayerFilter, rgb: [f64; 3], uv: [f64; 2]) -> [f64; 3] {
    let mix = |a: [f64; 3], b: [f64; 3], t: f64| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
    let result = match filter {
        LayerFilter::Brightness { amount } => rgb.map(|c| c * amount.max(0.0)),
        LayerFilter::Contrast { amount } => rgb.map(|c| (c - 0.5) * amount.max(0.0) + 0.5),
        LayerFilter::Saturation { amount } => saturated(rgb, amount.max(0.0)),
        LayerFilter::Hue { degrees } => {
            let (s, c) = degrees.to_radians().sin_cos();
            let [r, g, b] = rgb;
            let [lr, lg, lb] = LUMA;
            [
                (lr + (1.0 - lr) * c - lr * s) * r
                    + (lg - lg * c - lg * s) * g
                    + (lb - lb * c + (1.0 - lb) * s) * b,
                (lr - lr * c + 0.143 * s) * r
                    + (lg + (1.0 - lg) * c + 0.140 * s) * g
                    + (lb - lb * c - 0.283 * s) * b,
                (lr - lr * c - (1.0 - lr) * s) * r
                    + (lg - lg * c + lg * s) * g
                    + (lb + (1.0 - lb) * c + lb * s) * b,
            ]
        }
        LayerFilter::Grayscale { amount } => saturated(rgb, 1.0 - amount.clamp(0.0, 1.0)),
        LayerFilter::Invert { amount } => mix(rgb, rgb.map(|c| 1.0 - c), amount.clamp(0.0, 1.0)),
        LayerFilter::Tint { color, amount } => {
            let color = celesta_renderer::Color::from_hex(color).unwrap();
            let tint = [color.red, color.green, color.blue].map(|c| f64::from(c) / 255.0);
            mix(
                rgb,
                tint,
                amount.clamp(0.0, 1.0) * f64::from(color.alpha) / 255.0,
            )
        }
        LayerFilter::Vignette {
            amount,
            radius,
            softness,
        } => {
            let distance = (uv[0] - 0.5).hypot(uv[1] - 0.5) * 2f64.sqrt();
            let t = ((distance - radius) / softness.max(1e-4)).clamp(0.0, 1.0);
            rgb.map(|c| c * (1.0 - amount.clamp(0.0, 1.0) * t * t * (3.0 - 2.0 * t)))
        }
        _ => unreachable!("not a color filter"),
    };
    result.map(|c| c.clamp(0.0, 1.0))
}

fn byte(value: f64) -> u8 {
    (value * 255.0).round() as u8
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

fn every_filter() -> Vec<LayerFilter> {
    vec![
        LayerFilter::Brightness { amount: 0.6 },
        LayerFilter::Brightness { amount: 1.8 },
        LayerFilter::Contrast { amount: 0.4 },
        LayerFilter::Contrast { amount: 1.6 },
        LayerFilter::Saturation { amount: 0.0 },
        LayerFilter::Saturation { amount: 1.7 },
        LayerFilter::Hue { degrees: 90.0 },
        LayerFilter::Hue { degrees: -200.0 },
        LayerFilter::Hue { degrees: 3690.0 },
        LayerFilter::Grayscale { amount: 1.0 },
        LayerFilter::Grayscale { amount: 0.3 },
        LayerFilter::Invert { amount: 1.0 },
        LayerFilter::Invert { amount: 0.25 },
        LayerFilter::Tint {
            color: "#2040ff".to_owned(),
            amount: 0.5,
        },
        LayerFilter::Tint {
            color: "#ff000080".to_owned(),
            amount: 1.0,
        },
        LayerFilter::Vignette {
            amount: 0.8,
            radius: 0.0,
            softness: 1.0,
        },
    ]
}

#[test]
fn each_color_filter_matches_its_definition() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let rgb = [0xc0, 0x60, 0x30].map(|c: u8| f64::from(c) / 255.0);
    for filter in every_filter() {
        let rect = corner_rect("rect", 8.0, 8.0, 48.0, 48.0, "#c06030");
        let frame = renderer
            .render(&scene(
                Vec::new(),
                vec![filtered(rect, vec![filter.clone()])],
            ))
            .unwrap();
        // The centre, and a pixel near the top-left corner.
        for (x, y) in [(32, 32), (9, 10)] {
            let uv = [
                (f64::from(x) + 0.5 - 8.0) / 48.0,
                (f64::from(y) + 0.5 - 8.0) / 48.0,
            ];
            let [r, g, b] = reference(&filter, rgb, uv).map(byte);
            assert_near(
                pixel_at(&frame, x, y),
                [r, g, b, 255],
                1,
                &format!("{filter:?} at {x},{y}"),
            );
        }
        assert_eq!(pixel_at(&frame, 2, 2), [0, 0, 0, 255], "{filter:?} outside");
    }
}

#[test]
fn filters_the_straight_color_and_keeps_alpha() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let rgb = [0x40, 0x80, 0xc0].map(|c: u8| f64::from(c) / 255.0);
    for filter in every_filter() {
        let rect = corner_rect("rect", 8.0, 8.0, 48.0, 48.0, "#4080c080");
        let frame = renderer
            .render(&scene(
                Vec::new(),
                vec![filtered(rect, vec![filter.clone()])],
            ))
            .unwrap();
        let alpha = f64::from(0x80u8) / 255.0;
        let [r, g, b] =
            reference(&filter, rgb, [24.5 / 48.0, 24.5 / 48.0]).map(|c| byte(c * alpha));
        // Over black, a pixel is the filtered color times the rect's alpha.
        assert_near(
            pixel_at(&frame, 32, 32),
            [r, g, b, 255],
            1,
            &format!("{filter:?}"),
        );
    }
}

#[test]
fn runs_filters_and_shaders_in_the_order_given() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let red = ShaderSource {
        id: "red".to_owned(),
        name: None,
        wgsl: "fn effect(input: EffectInput) -> vec4f {\n    let alpha = source_at(input.position).a;\n    return vec4f(alpha, 0.0, 0.0, alpha);\n}\n".to_owned(),
        params: Vec::new(),
    };
    let shader = || {
        LayerFilter::Shader(LayerShader {
            id: "red".to_owned(),
            params: Vec::new(),
            padding: 0.0,
        })
    };
    let invert = || LayerFilter::Invert { amount: 1.0 };
    let render = |renderer: &mut GpuRenderer, filters| {
        let rect = corner_rect("rect", 8.0, 8.0, 32.0, 32.0, "#00ff00");
        let frame = renderer
            .render(&scene(vec![red.clone()], vec![filtered(rect, filters)]))
            .unwrap();
        pixel_at(&frame, 20, 20)
    };
    assert_eq!(
        render(&mut renderer, vec![shader(), invert()]),
        [0, 255, 255, 255]
    );
    assert_eq!(
        render(&mut renderer, vec![invert(), shader()]),
        [255, 0, 0, 255]
    );
    // Each filter clamps its result, as a pass of its own would.
    let clamped = render(
        &mut renderer,
        vec![
            LayerFilter::Brightness { amount: 4.0 },
            LayerFilter::Brightness { amount: 0.5 },
        ],
    );
    assert_near(clamped, [0, 128, 0, 255], 1, "clamped");
}

#[test]
fn a_vignette_follows_the_content_box() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let vignette = LayerFilter::Vignette {
        amount: 1.0,
        radius: 0.5,
        softness: 0.0,
    };
    // Two rects whose box is the group's, 8..56 by 16..48.
    let layer = filtered(
        group(
            EvaluatedTransform::default(),
            vec![
                corner_rect("left", 8.0, 16.0, 24.0, 32.0, "#ffffff"),
                corner_rect("right", 32.0, 16.0, 24.0, 32.0, "#ffffff"),
            ],
        ),
        vec![vignette],
    );
    let frame = renderer.render(&scene(Vec::new(), vec![layer])).unwrap();
    // A step at half the way to the box's corners: unchanged within it,
    // black beyond.
    assert_eq!(pixel_at(&frame, 32, 32), [255, 255, 255, 255]);
    assert_eq!(pixel_at(&frame, 44, 32), [255, 255, 255, 255]);
    assert_eq!(pixel_at(&frame, 54, 32), [0, 0, 0, 255]);
    assert_eq!(pixel_at(&frame, 9, 17), [0, 0, 0, 255]);
}

#[test]
fn other_effects_filter_the_color_filtered_result() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let mut layer = filtered(
        corner_rect("rect", 4.0, 4.0, 16.0, 16.0, "#ffffff"),
        vec![LayerFilter::Tint {
            color: "#0000ff".to_owned(),
            amount: 1.0,
        }],
    );
    layer.effects.shadow = Some(Box::new(LayerShadow {
        color: "#ff0000".to_owned(),
        blur: 0.0,
        offset_x: 0.0,
        offset_y: 20.0,
    }));
    let frame = renderer.render(&scene(Vec::new(), vec![layer])).unwrap();
    assert_eq!(pixel_at(&frame, 12, 12), [0, 0, 255, 255]);
    // The shadow keeps its own color.
    assert_eq!(pixel_at(&frame, 12, 32), [255, 0, 0, 255]);
}

#[test]
fn rejects_values_it_cannot_use() {
    let Some(mut renderer) = renderer(black_background()) else {
        return;
    };
    let render = |renderer: &mut GpuRenderer, filter| {
        let rect = corner_rect("rect", 8.0, 8.0, 16.0, 16.0, "#ffffff");
        match renderer.render(&scene(Vec::new(), vec![filtered(rect, vec![filter])])) {
            Ok(_) => panic!("the frame rendered"),
            Err(error) => error.to_string(),
        }
    };
    let error = render(&mut renderer, LayerFilter::Brightness { amount: 1e300 });
    assert!(
        error.contains("brightness") && error.contains("not finite"),
        "{error}"
    );
    let error = render(&mut renderer, LayerFilter::Brightness { amount: f64::NAN });
    assert!(
        error.contains("brightness") && error.contains("not finite"),
        "{error}"
    );
    let error = render(
        &mut renderer,
        LayerFilter::Hue {
            degrees: f64::INFINITY,
        },
    );
    assert!(
        error.contains("hue") && error.contains("not finite"),
        "{error}"
    );
    let error = render(
        &mut renderer,
        LayerFilter::Tint {
            color: "orange".to_owned(),
            amount: 1.0,
        },
    );
    assert!(
        error.contains("tint") && error.contains("orange"),
        "{error}"
    );
}
