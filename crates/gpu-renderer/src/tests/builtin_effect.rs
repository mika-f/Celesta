//! Built-in stylize effects and the chroma key (`builtin_effect.wgsl`). The
//! CPU reference renderer refuses filters, so these check what each effect
//! is defined to do; `write_web_effect_fixtures` records what this GPU draws
//! for `@celesta.studio/web`'s test of the same arithmetic.

use super::*;
use celesta_composition::{GradientStop, LayerFilter};

fn transparent() -> GpuRenderOptions {
    GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 0),
        ..GpuRenderOptions::default()
    }
}

fn filtered(mut layer: Layer, filters: Vec<LayerFilter>) -> Layer {
    layer.effects.filters.extend(filters);
    layer
}

fn scene(size: u32, layers: Vec<Layer>) -> Scene {
    let mut scene = empty_scene(size, size);
    scene.layers = layers;
    scene
}

/// A 48x48 scene's test pattern, its content box 8..40: a red-to-blue
/// gradient over key green, a translucent skin-colored square across both,
/// and a shadowed patch of the key.
fn pattern() -> Layer {
    let mut gradient = corner_rect("gradient", 8.0, 8.0, 32.0, 16.0, "#000000");
    if let LayerContent::Rect { fill, .. } = &mut gradient.content {
        let stop = |offset, color: &str| GradientStop {
            offset,
            color: color.to_owned(),
        };
        *fill = Some(Paint::Linear {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 32.0, y: 0.0 },
            stops: vec![stop(0.0, "#e83a30"), stop(1.0, "#3070e0")],
        });
    }
    group(
        EvaluatedTransform::default(),
        vec![
            gradient,
            corner_rect("key", 8.0, 24.0, 32.0, 16.0, "#00b140"),
            corner_rect("skin", 16.0, 18.0, 14.0, 14.0, "#f0c8a0cc"),
            corner_rect("shadow", 30.0, 30.0, 8.0, 8.0, "#00702a"),
        ],
    )
}

fn effects() -> Vec<(&'static str, LayerFilter)> {
    vec![
        ("pixelate", LayerFilter::Pixelate { size: 6.0 }),
        (
            "halftone",
            LayerFilter::Halftone {
                size: 6.0,
                angle: 30.0,
            },
        ),
        (
            "chromatic-aberration",
            LayerFilter::ChromaticAberration {
                offset: 3.0,
                angle: 20.0,
            },
        ),
        (
            "wave",
            LayerFilter::Wave {
                amplitude: 3.0,
                wavelength: 16.0,
                phase: 40.0,
                angle: 0.0,
            },
        ),
        (
            "noise",
            LayerFilter::Noise {
                amount: 0.3,
                seed: 0x0001_0007,
                monochrome: false,
            },
        ),
        (
            "noise-monochrome",
            LayerFilter::Noise {
                amount: 0.3,
                seed: 7,
                monochrome: true,
            },
        ),
        (
            "scanlines",
            LayerFilter::Scanlines {
                amount: 0.6,
                spacing: 4.0,
                thickness: 0.5,
                offset: 1.0,
            },
        ),
        (
            "zoom-blur",
            LayerFilter::ZoomBlur {
                amount: 0.3,
                center_x: 0.4,
                center_y: 0.6,
            },
        ),
        (
            "chroma-key",
            LayerFilter::ChromaKey {
                color: "#00b140".to_owned(),
                tolerance: 0.2,
                softness: 0.1,
                spill: 0.5,
            },
        ),
    ]
}

fn render(renderer: &mut GpuRenderer, size: u32, layer: Layer) -> GpuFrame {
    renderer.render(&scene(size, vec![layer])).unwrap()
}

/// Records the pattern unfiltered and with each of `effects()` as raw
/// premultiplied RGBA for `packages/web/test/filters.test.mts`. Run it after
/// changing `builtin_effect.wgsl`:
/// `cargo test -p celesta-gpu-renderer write_web_effect_fixtures -- --ignored`.
#[test]
#[ignore = "writes fixtures for @celesta.studio/web's tests"]
fn write_web_effect_fixtures() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/web/test/fixtures/effects");
    std::fs::create_dir_all(&directory).unwrap();
    let source = render(&mut renderer, 48, pattern());
    std::fs::write(directory.join("source.rgba"), source.pixels()).unwrap();
    for (name, effect) in effects() {
        let frame = render(&mut renderer, 48, filtered(pattern(), vec![effect]));
        std::fs::write(directory.join(format!("{name}.rgba")), frame.pixels()).unwrap();
    }
}

#[test]
fn every_effect_compiles_and_draws() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let source = render(&mut renderer, 48, pattern());
    for (name, effect) in effects() {
        let frame = render(&mut renderer, 48, filtered(pattern(), vec![effect]));
        assert_ne!(frame.pixels(), source.pixels(), "{name} changed nothing");
    }
}

#[test]
fn pixelate_averages_cells_from_the_content_box() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    // Black and white columns, one pixel each, from x = 5: the box starts
    // at 5, so every 4-pixel cell holds two of each.
    let stripes = (0..16)
        .map(|index| {
            let color = if index % 2 == 0 { "#000000" } else { "#ffffff" };
            corner_rect("stripe", 5.0 + f64::from(index), 4.0, 1.0, 8.0, color)
        })
        .collect();
    let layer = filtered(
        group(EvaluatedTransform::default(), stripes),
        vec![LayerFilter::Pixelate { size: 4.0 }],
    );
    let frame = render(&mut renderer, 32, layer);
    for x in 5..21 {
        let [r, g, b, a] = pixel_at(&frame, x, 6);
        assert!(
            r.abs_diff(128) <= 1 && r == g && g == b && a == 255,
            "{x}: {r},{g},{b},{a}"
        );
    }
    // Outside the box, cells left of it hold nothing.
    assert_eq!(pixel_at(&frame, 3, 6), [0, 0, 0, 0]);
    // A size of 1 or less keeps the pixels.
    let sharp = filtered(
        corner_rect("rect", 4.5, 4.0, 8.0, 8.0, "#ffffff"),
        vec![LayerFilter::Pixelate { size: 1.0 }],
    );
    let plain = corner_rect("rect", 4.5, 4.0, 8.0, 8.0, "#ffffff");
    assert_eq!(
        render(&mut renderer, 32, sharp).pixels(),
        render(&mut renderer, 32, plain).pixels()
    );
}

#[test]
fn halftone_dots_take_their_cells_color_and_area_from_luma() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let draw = |renderer: &mut GpuRenderer, color: &str| {
        let layer = filtered(
            corner_rect("rect", 0.0, 0.0, 40.0, 40.0, color),
            vec![LayerFilter::Halftone {
                size: 20.0,
                angle: 0.0,
            }],
        );
        render(renderer, 40, layer)
    };
    let covered = |frame: &GpuFrame| {
        frame
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| f64::from(pixel[3]) / 255.0)
            .sum::<f64>()
            / 1600.0
    };
    // A mid gray's dots cover its luma of each cell, in its own color.
    let gray = draw(&mut renderer, "#808080");
    let luma = f64::from(0x80u8) / 255.0;
    assert!((covered(&gray) - luma).abs() < 0.02, "{}", covered(&gray));
    assert_eq!(pixel_at(&gray, 10, 10), [128, 128, 128, 255]);
    assert_eq!(pixel_at(&gray, 0, 0), [0, 0, 0, 0], "between the dots");
    // Black has no dots.
    assert_eq!(covered(&draw(&mut renderer, "#000000")), 0.0);
}

#[test]
fn chromatic_aberration_moves_red_and_blue_apart() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let layer = filtered(
        corner_rect("rect", 8.0, 8.0, 16.0, 16.0, "#ffffff"),
        vec![LayerFilter::ChromaticAberration {
            offset: 3.0,
            angle: 0.0,
        }],
    );
    let frame = render(&mut renderer, 32, layer);
    // Red moved right, past the content box (its padding), blue left.
    assert_eq!(pixel_at(&frame, 25, 12), [255, 0, 0, 255]);
    assert_eq!(pixel_at(&frame, 6, 12), [0, 0, 255, 255]);
    assert_eq!(pixel_at(&frame, 16, 12), [255, 255, 255, 255]);
    assert_eq!(pixel_at(&frame, 28, 12), [0, 0, 0, 0]);
}

#[test]
fn wave_sways_rows_along_a_sine_across_them() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    // At phase 90 the box's top row moves the whole amplitude right; a
    // quarter wavelength lower, not at all.
    let layer = filtered(
        corner_rect("rect", 8.0, 8.0, 16.0, 16.0, "#ffffff"),
        vec![LayerFilter::Wave {
            amplitude: 4.0,
            wavelength: 2000.0,
            phase: 90.0,
            angle: 0.0,
        }],
    );
    let frame = render(&mut renderer, 32, layer);
    assert_eq!(pixel_at(&frame, 9, 8)[3], 0);
    assert_eq!(pixel_at(&frame, 27, 8)[3], 255);
    assert_eq!(pixel_at(&frame, 28, 8)[3], 0);
    let turned = filtered(
        corner_rect("rect", 8.0, 8.0, 16.0, 16.0, "#ffffff"),
        vec![LayerFilter::Wave {
            amplitude: 4.0,
            wavelength: 64.0,
            phase: 0.0,
            angle: 90.0,
        }],
    );
    // Turned a quarter, the whole wave turns: columns sway vertically, the
    // sine running right to left. 12.5 pixels into the box it is -0.94, so
    // that column moves 3.8 pixels up.
    let frame = render(&mut renderer, 32, turned);
    assert_eq!(pixel_at(&frame, 20, 5)[3], 255);
    assert_eq!(pixel_at(&frame, 20, 21)[3], 0);
    assert_eq!(
        pixel_at(&frame, 9, 4)[3],
        0,
        "less sway near the box's left"
    );
}

#[test]
fn noise_is_the_same_for_the_same_seed_and_differs_between_seeds() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let noise = |seed, monochrome| {
        filtered(
            corner_rect("rect", 0.0, 0.0, 32.0, 32.0, "#808080"),
            vec![LayerFilter::Noise {
                amount: 0.25,
                seed,
                monochrome,
            }],
        )
    };
    let first = render(&mut renderer, 32, noise(5, false));
    let again = render(&mut renderer, 32, noise(5, false));
    assert_eq!(first.pixels(), again.pixels());
    // The seed's upper half counts too.
    let upper = render(&mut renderer, 32, noise(5 + 0x1_0000, false));
    assert_ne!(first.pixels(), upper.pixels());
    let monochrome = render(&mut renderer, 32, noise(5, true));
    let mut total = 0.0;
    for pixel in monochrome.pixels().as_chunks::<4>().0 {
        assert!(pixel[0] == pixel[1] && pixel[1] == pixel[2], "{pixel:?}");
        assert!(pixel[0].abs_diff(128) <= 65 && pixel[3] == 255, "{pixel:?}");
        total += f64::from(pixel[0]);
    }
    // Centred on the color.
    assert!((total / 1024.0 - 128.0).abs() < 4.0, "{}", total / 1024.0);
    assert!(
        first
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[0] != pixel[1])
    );
}

#[test]
fn scanlines_darken_the_part_of_each_pixel_their_bands_cover() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let layer = filtered(
        corner_rect("rect", 0.0, 4.0, 16.0, 16.0, "#ffffff"),
        vec![LayerFilter::Scanlines {
            amount: 0.5,
            spacing: 4.0,
            thickness: 0.25,
            offset: 0.5,
        }],
    );
    let frame = render(&mut renderer, 24, layer);
    // Bands start at 4.5, 8.5, …: rows 4 and 5 are each half covered.
    let rows: Vec<u8> = (4..12).map(|y| pixel_at(&frame, 8, y)[0]).collect();
    for (row, expected) in rows.iter().zip([191, 191, 255, 255, 191, 191, 255, 255]) {
        assert!(row.abs_diff(expected) <= 1, "{rows:?}");
    }
}

#[test]
fn zoom_blur_keeps_the_centre_and_streaks_past_the_content() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let layer = filtered(
        corner_rect("rect", 16.0, 16.0, 16.0, 16.0, "#ffffff"),
        vec![LayerFilter::ZoomBlur {
            amount: 0.5,
            center_x: 0.5,
            center_y: 0.5,
        }],
    );
    let frame = render(&mut renderer, 48, layer);
    assert_eq!(pixel_at(&frame, 24, 24), [255, 255, 255, 255]);
    // Beyond the rect's corner, streaks from it.
    let [.., alpha] = pixel_at(&frame, 36, 36);
    assert!(alpha > 0 && alpha < 255, "{alpha}");
    assert_eq!(pixel_at(&frame, 47, 47)[3], 0);
}

#[test]
fn chroma_key_clears_the_key_and_suppresses_its_spill() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let key = |tolerance, spill| LayerFilter::ChromaKey {
        color: "#00b140".to_owned(),
        tolerance,
        softness: 0.1,
        spill,
    };
    let layer = |filter| {
        filtered(
            group(
                EvaluatedTransform::default(),
                vec![
                    corner_rect("key", 0.0, 0.0, 8.0, 8.0, "#00b140"),
                    corner_rect("shadow", 8.0, 0.0, 8.0, 8.0, "#00702a"),
                    corner_rect("gray", 16.0, 0.0, 8.0, 8.0, "#808080"),
                    corner_rect("olive", 24.0, 0.0, 8.0, 8.0, "#c0c040"),
                ],
            ),
            vec![filter],
        )
    };
    let frame = render(&mut renderer, 32, layer(key(0.2, 0.0)));
    assert_eq!(pixel_at(&frame, 4, 4)[3], 0, "the key");
    // A darker shade of the key has less chroma, but not too little.
    assert_eq!(pixel_at(&frame, 12, 4)[3], 0, "the key in shadow");
    assert_eq!(pixel_at(&frame, 20, 4), [128, 128, 128, 255], "gray");
    assert_eq!(pixel_at(&frame, 28, 4), [192, 192, 64, 255], "olive");
    // Spill takes the key's chroma out of what remains, keeping luma: the
    // olive leans less toward the key's green. Gray has none to take.
    let spilled = render(&mut renderer, 32, layer(key(0.2, 1.0)));
    let [r, g, b, a] = pixel_at(&spilled, 28, 4);
    assert!(a == 255 && g < 186 && r > 210 && b > 70, "{r},{g},{b},{a}");
    assert_eq!(pixel_at(&spilled, 20, 4), [128, 128, 128, 255], "gray");
    // Within the softness, partly.
    let [.., a] = pixel_at(&render(&mut renderer, 32, layer(key(0.1, 0.0))), 12, 4);
    assert!(a > 0 && a < 255, "{a}");
}

#[test]
fn rejects_values_it_cannot_use() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let error = |renderer: &mut GpuRenderer, filter| {
        let rect = corner_rect("rect", 8.0, 8.0, 16.0, 16.0, "#ffffff");
        match renderer.render(&scene(32, vec![filtered(rect, vec![filter])])) {
            Ok(_) => panic!("the frame rendered"),
            Err(error) => error.to_string(),
        }
    };
    let message = error(
        &mut renderer,
        LayerFilter::Wave {
            amplitude: 1.0,
            wavelength: f64::INFINITY,
            phase: 0.0,
            angle: 0.0,
        },
    );
    assert!(
        message.contains("invalid wave filter") && message.contains("not finite"),
        "{message}"
    );
    let message = error(
        &mut renderer,
        LayerFilter::ChromaKey {
            color: "green".to_owned(),
            tolerance: 0.2,
            softness: 0.1,
            spill: 0.5,
        },
    );
    assert!(
        message.contains("chromaKey") && message.contains("green"),
        "{message}"
    );
}
