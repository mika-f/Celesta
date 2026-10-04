use crate::assets::rasterize_psd;
use crate::composite::{blend, blend_with_mode, psd_blend_channel};
use crate::fonts::TextRasterizer;
use crate::linebreak::{WORD_JOINER, insert_joiners, phrase_segments};
use crate::rect::rasterize_rect;
use crate::renderer::CpuRenderer;
use crate::text::TextMetrics;
use crate::types::{Color, RenderOptions, RgbaFrame};
use celesta_composition::{LineBreak, Point};

use std::path::Path;

use celesta_composition::{
    AssetLocation, BlendMode, Clip, EvaluatedTransform, Layer, LayerContent, MediaTiming, Paint,
    Rational, ResolvedAsset, Scene, TextStyle, Time,
};
use celesta_media::{MediaError, VideoFrame, VideoFrameDecoder};

#[test]
fn measure_reports_line_metrics_and_per_glyph_advances() {
    let style = TextStyle {
        font_size: Some(40.0),
        ..TextStyle::default()
    };
    let mut rasterizer = TextRasterizer::new();
    let metrics = rasterizer.measure("ab\ncd", &style, None);

    assert_eq!(metrics.lines, 2);
    assert_eq!(metrics.glyphs.len(), 4);
    assert_eq!(metrics.line_height, 48.0);
    assert_eq!(metrics.height, 96.0);
    assert!((metrics.ascent + metrics.descent - metrics.line_height).abs() < 1e-6);
    let first_line = &metrics.glyphs[..2];
    assert_eq!(first_line[0].x, 0.0);
    assert!((first_line[1].x - first_line[0].width).abs() < 1e-6);
    assert!(metrics.width >= first_line[1].x + first_line[1].width - 1e-6);
    assert_eq!(metrics.glyphs[2].line, 1);
}

#[test]
fn rasterizes_color_emoji_glyphs_when_a_color_font_is_available() {
    let mut rasterizer = TextRasterizer::new();
    let rasterized = rasterizer
        .rasterize(
            "\u{1F525}", // fire emoji: multi-colored (red/orange/yellow) on
            // any real color-emoji font, unlike a flat single-fill glyph.
            &TextStyle {
                font_size: Some(64.0),
                fill: Some(Paint::Solid {
                    color: "#FFFFFFFF".to_owned(),
                }),
                ..TextStyle::default()
            },
            None,
            1.0,
        )
        .unwrap();

    let distinct_colors: std::collections::HashSet<[u8; 3]> = rasterized
        .pixels()
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 0)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();

    // A regression back to alpha-only mask compositing would flatten
    // every opaque pixel to the single fill color (white here), so this
    // is the direct check that color glyph data survives rasterization.
    // Environments without any color-emoji font (uncommon, but possible
    // outside macOS) fall back to a monochrome glyph outline instead of
    // failing — not a regression this test can detect there.
    if distinct_colors.len() <= 1 {
        eprintln!(
            "skipping color emoji assertion: no color-emoji font available in this environment"
        );
        return;
    }
    assert!(distinct_colors.len() > 1);
}

#[test]
fn renders_text_to_a_png() {
    let scene = Scene {
        width: 320,
        height: 180,
        frame_rate: Rational::new(60, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![Layer {
            id: "hello".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 160.0, y: 90.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Text {
                text: "Hello, Celesta!".to_owned(),
                style: TextStyle {
                    font_size: Some(24.0),
                    ..TextStyle::default()
                },
                max_width: None,
                baseline_anchor: false,
            },
        }],
    };
    let mut renderer = CpuRenderer::default();
    let frame = renderer.render(&scene).unwrap();
    let png = frame.encode_png().unwrap();

    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert!(
        frame
            .pixels()
            .chunks_exact(4)
            .any(|pixel| pixel == [255, 255, 255, 255])
    );
}

fn clip_scene(layers: Vec<Layer>) -> Scene {
    Scene {
        width: 40,
        height: 40,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers,
    }
}

/// A 40x40 red rect at the group's origin.
fn red_square() -> Layer {
    Layer {
        id: "red".to_owned(),
        transform: EvaluatedTransform {
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Rect {
            width: 40.0,
            height: 40.0,
            fill: Some(Paint::Solid {
                color: "#FF0000FF".to_owned(),
            }),
            stroke: None,
            corner_radius: 0.0,
        },
    }
}

fn clipped_group(transform: EvaluatedTransform, clip: Clip, children: Vec<Layer>) -> Layer {
    Layer {
        id: "clipped".to_owned(),
        transform,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Group {
            layers: children,
            clip: Some(clip),
        },
    }
}

fn pixel(frame: &RgbaFrame, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * frame.width() + x) * 4) as usize;
    frame.pixels()[offset..offset + 4].try_into().unwrap()
}

fn is_red(frame: &RgbaFrame, x: u32, y: u32) -> bool {
    pixel(frame, x, y) == [255, 0, 0, 255]
}

/// The renderer's default background, which nothing has drawn over.
fn background() -> [u8; 4] {
    let color = RenderOptions::default().background;
    [color.red, color.green, color.blue, color.alpha]
}

fn is_untouched(frame: &RgbaFrame, x: u32, y: u32) -> bool {
    pixel(frame, x, y) == background()
}

#[test]
fn clips_a_groups_children_to_its_rectangle() {
    let clip = Clip {
        x: 10.0,
        y: 5.0,
        width: 20.0,
        height: 10.0,
        corner_radius: 0.0,
    };
    let scene = clip_scene(vec![clipped_group(
        EvaluatedTransform::default(),
        clip,
        vec![red_square()],
    )]);
    let frame = CpuRenderer::default().render(&scene).unwrap();

    // Inside, right at each edge, and one pixel beyond it.
    assert!(is_red(&frame, 10, 5));
    assert!(is_red(&frame, 29, 14));
    assert!(is_red(&frame, 20, 10));
    for (x, y) in [(9, 5), (30, 10), (20, 4), (20, 15), (0, 0), (39, 39)] {
        assert!(is_untouched(&frame, x, y), "({x}, {y}) leaked");
    }
}

#[test]
fn a_clip_follows_the_groups_position_and_scale() {
    // The clip covers the group's 0..10 by 0..10, which the group's
    // position and 2x scale put at canvas 8..28 by 4..24.
    let clip = Clip {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        corner_radius: 0.0,
    };
    let scene = clip_scene(vec![clipped_group(
        EvaluatedTransform {
            position: Point { x: 8.0, y: 4.0 },
            scale: Point { x: 2.0, y: 2.0 },
            ..EvaluatedTransform::default()
        },
        clip,
        vec![red_square()],
    )]);
    let frame = CpuRenderer::default().render(&scene).unwrap();

    assert!(is_red(&frame, 8, 4));
    assert!(is_red(&frame, 27, 23));
    for (x, y) in [(7, 4), (28, 10), (10, 3), (10, 24)] {
        assert!(is_untouched(&frame, x, y), "({x}, {y}) leaked");
    }
}

#[test]
fn rounds_the_corners_of_a_clip() {
    let clip = Clip {
        x: 0.0,
        y: 0.0,
        width: 40.0,
        height: 40.0,
        corner_radius: 10.0,
    };
    let scene = clip_scene(vec![clipped_group(
        EvaluatedTransform::default(),
        clip,
        vec![red_square()],
    )]);
    let frame = CpuRenderer::default().render(&scene).unwrap();

    assert!(is_red(&frame, 20, 20));
    assert!(is_red(&frame, 20, 0));
    assert!(is_untouched(&frame, 0, 0));
    // On the rounded edge the coverage is partial, so the pixel is a
    // blend of the red and the background rather than either.
    let edge = pixel(&frame, 2, 3);
    assert!(edge[0] > background()[0] && edge[0] < 255, "edge {edge:?}");
}

#[test]
fn nested_clips_intersect() {
    let outer = Clip {
        x: 0.0,
        y: 0.0,
        width: 20.0,
        height: 40.0,
        corner_radius: 0.0,
    };
    let inner = Clip {
        x: 10.0,
        y: 0.0,
        width: 30.0,
        height: 10.0,
        corner_radius: 0.0,
    };
    let scene = clip_scene(vec![clipped_group(
        EvaluatedTransform::default(),
        outer,
        vec![clipped_group(
            EvaluatedTransform::default(),
            inner,
            vec![red_square()],
        )],
    )]);
    let frame = CpuRenderer::default().render(&scene).unwrap();

    // Only 10..20 by 0..10 is inside both.
    assert!(is_red(&frame, 10, 0));
    assert!(is_red(&frame, 19, 9));
    for (x, y) in [(9, 5), (20, 5), (15, 10)] {
        assert!(is_untouched(&frame, x, y), "({x}, {y}) leaked");
    }
}

#[test]
fn a_clip_without_area_hides_its_children() {
    let clip = Clip {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 40.0,
        corner_radius: 0.0,
    };
    let scene = clip_scene(vec![clipped_group(
        EvaluatedTransform::default(),
        clip,
        vec![red_square()],
    )]);
    let frame = CpuRenderer::default().render(&scene).unwrap();

    assert!(
        frame
            .pixels()
            .chunks_exact(4)
            .all(|pixel| pixel == background())
    );
}

#[test]
fn does_not_clip_layers_outside_the_group() {
    let clip = Clip {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        corner_radius: 0.0,
    };
    let after = Layer {
        id: "after".to_owned(),
        transform: EvaluatedTransform {
            position: Point { x: 30.0, y: 30.0 },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Rect {
            width: 10.0,
            height: 10.0,
            fill: Some(Paint::Solid {
                color: "#00FF00FF".to_owned(),
            }),
            stroke: None,
            corner_radius: 0.0,
        },
    };
    let scene = clip_scene(vec![
        clipped_group(EvaluatedTransform::default(), clip, vec![red_square()]),
        after,
    ]);
    let frame = CpuRenderer::default().render(&scene).unwrap();

    assert!(is_red(&frame, 9, 9));
    assert!(is_untouched(&frame, 10, 10));
    assert_eq!(pixel(&frame, 30, 30), [0, 255, 0, 255]);
    assert_eq!(pixel(&frame, 39, 39), [0, 255, 0, 255]);
}

#[test]
fn clips_text_to_the_rectangle() {
    let text = Layer {
        id: "text".to_owned(),
        transform: EvaluatedTransform {
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: "MMMM".to_owned(),
            style: TextStyle {
                font_size: Some(30.0),
                fill: Some(Paint::Solid {
                    color: "#FFFFFFFF".to_owned(),
                }),
                ..TextStyle::default()
            },
            max_width: None,
            baseline_anchor: false,
        },
    };
    let clip = Clip {
        x: 0.0,
        y: 0.0,
        width: 40.0,
        height: 15.0,
        corner_radius: 0.0,
    };
    let plain = CpuRenderer::default()
        .render(&clip_scene(vec![text.clone()]))
        .unwrap();
    let clipped = CpuRenderer::default()
        .render(&clip_scene(vec![clipped_group(
            EvaluatedTransform::default(),
            clip,
            vec![text],
        )]))
        .unwrap();

    // Inside the clip the text is untouched; below it nothing is drawn.
    for y in 0..40 {
        for x in 0..40 {
            if y < 15 {
                assert_eq!(pixel(&clipped, x, y), pixel(&plain, x, y), "({x}, {y})");
            } else {
                assert!(is_untouched(&clipped, x, y), "({x}, {y}) leaked");
            }
        }
    }
}

#[test]
fn renders_a_filled_rounded_rect_with_a_stroke() {
    let scene = Scene {
        width: 200,
        height: 120,
        frame_rate: Rational::new(60, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![Layer {
            id: "card".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 100.0, y: 60.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Rect {
                width: 100.0,
                height: 60.0,
                fill: Some(Paint::Solid {
                    color: "#3366CCFF".to_owned(),
                }),
                stroke: Some(celesta_composition::Stroke {
                    paint: Paint::Solid {
                        color: "#FFFFFFFF".to_owned(),
                    },
                    width: 4.0,
                }),
                corner_radius: 12.0,
            },
        }],
    };
    let mut renderer = CpuRenderer::default();
    let frame = renderer.render(&scene).unwrap();

    // Center of the rect is inside the fill, away from the stroke band.
    let center_offset = ((60 * frame.width() + 100) * 4) as usize;
    assert_eq!(
        &frame.pixels()[center_offset..center_offset + 4],
        &[0x33, 0x66, 0xCC, 0xFF]
    );

    // A pixel just outside the corner radius stays background (transparent
    // over the render's own background, so at least distinct from the fill
    // and stroke colors).
    let corner_offset = ((32 * frame.width() + 52) * 4) as usize;
    let corner_pixel = &frame.pixels()[corner_offset..corner_offset + 4];
    assert_ne!(corner_pixel, [0x33, 0x66, 0xCC, 0xFF]);
    assert_ne!(corner_pixel, [0xFF, 0xFF, 0xFF, 0xFF]);
}

#[test]
fn centers_visible_single_line_text_on_its_transform() {
    let scene = Scene {
        width: 1280,
        height: 720,
        frame_rate: Rational::new(60, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![Layer {
            id: "title".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 640.0, y: 360.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Text {
                text: "Celesta".to_owned(),
                style: TextStyle {
                    font_size: Some(96.0),
                    fill: Some(Paint::Solid {
                        color: "#FFA13BFF".to_owned(),
                    }),
                    align: Some(celesta_composition::TextAlign::Center),
                    ..TextStyle::default()
                },
                max_width: None,
                baseline_anchor: false,
            },
        }],
    };
    let mut renderer = CpuRenderer::default();
    let frame = renderer.render(&scene).unwrap();
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0;
    let mut max_y = 0;
    for (index, pixel) in frame.pixels().chunks_exact(4).enumerate() {
        if pixel != [20, 22, 28, 255] {
            let x = index as u32 % frame.width();
            let y = index as u32 / frame.width();
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    let center_x = f64::from(min_x + max_x) / 2.0;
    let center_y = f64::from(min_y + max_y) / 2.0;
    // Horizontally the advance box is centered, so uneven side bearings
    // leave the ink a few pixels off.
    assert!((center_x - 640.0).abs() <= 4.0, "center x was {center_x}");
    assert!((center_y - 360.0).abs() <= 0.5, "center y was {center_y}");
}

#[test]
fn a_text_stroke_reaches_past_both_ends_of_the_line() {
    let plain_style = TextStyle {
        font_size: Some(96.0),
        ..TextStyle::default()
    };
    let stroked_style = TextStyle {
        stroke: Some(celesta_composition::Stroke {
            paint: Paint::Solid {
                color: "#000000FF".to_owned(),
            },
            width: 16.0,
        }),
        ..plain_style.clone()
    };
    let mut rasterizer = TextRasterizer::new();
    let plain = rasterizer.rasterize("MW", &plain_style, None, 1.0).unwrap();
    let stroked = rasterizer
        .rasterize("MW", &stroked_style, None, 1.0)
        .unwrap();

    assert_eq!(stroked.width(), plain.width() + 32);
    let column_has_ink = |x: u32| {
        (0..stroked.height())
            .any(|y| stroked.pixels()[((y * stroked.width() + x) * 4 + 3) as usize] > 0)
    };
    // Before, the stroke was cut off at the advance box on both sides.
    assert!((0..16).any(column_has_ink), "no stroke left of the line");
    assert!(
        (stroked.width() - 16..stroked.width()).any(column_has_ink),
        "no stroke right of the line"
    );
    // Anchors still refer to the advance box, not the padded image.
    let (left, _) = stroked.anchor_in_image(0.0, 0.0);
    let (right, _) = stroked.anchor_in_image(1.0, 0.0);
    assert!((left - 16.0 / f64::from(stroked.width())).abs() < 1e-9);
    assert!((right - f64::from(stroked.width() - 16) / f64::from(stroked.width())).abs() < 1e-9);
    assert_eq!(plain.anchor_in_image(0.25, 0.75), (0.25, 0.75));
}

#[test]
fn a_text_stroke_does_not_move_the_text() {
    let ink_center = |stroke: Option<celesta_composition::Stroke>| {
        let scene = Scene {
            width: 640,
            height: 360,
            frame_rate: Rational::new(30, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![Layer {
                id: "title".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 320.0, y: 180.0 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Text {
                    text: "Celesta".to_owned(),
                    style: TextStyle {
                        font_size: Some(72.0),
                        fill: Some(Paint::Solid {
                            color: "#FFFFFFFF".to_owned(),
                        }),
                        stroke,
                        ..TextStyle::default()
                    },
                    max_width: None,
                    baseline_anchor: false,
                },
            }],
        };
        let frame = CpuRenderer::default().render(&scene).unwrap();
        // The white fill only: the stroke is black.
        let (mut min_x, mut max_x) = (u32::MAX, 0);
        for (index, pixel) in frame.pixels().chunks_exact(4).enumerate() {
            if pixel[0] > 200 && pixel[1] > 200 {
                let x = index as u32 % frame.width();
                min_x = min_x.min(x);
                max_x = max_x.max(x);
            }
        }
        f64::from(min_x + max_x) / 2.0
    };
    let plain = ink_center(None);
    let stroked = ink_center(Some(celesta_composition::Stroke {
        paint: Paint::Solid {
            color: "#000000FF".to_owned(),
        },
        width: 12.0,
    }));
    assert!(
        (plain - stroked).abs() <= 1.0,
        "plain {plain}, stroked {stroked}"
    );
}

#[test]
fn single_line_text_keeps_the_width_of_its_spaces() {
    let mut rasterizer = TextRasterizer::new();
    let style = TextStyle {
        font_size: Some(48.0),
        ..TextStyle::default()
    };
    let mut rasterize = |text| rasterizer.rasterize(text, &style, None, 1.0).unwrap();
    let (equals, spaced, space) = (rasterize("="), rasterize(" = "), rasterize(" "));
    assert!(space.width() > 1, "a space was {} px wide", space.width());
    // Each width is rounded up to whole pixels on its own.
    assert!(
        spaced.width().abs_diff(equals.width() + space.width() * 2) <= 2,
        "\" = \" was {} px wide, \"=\" {} px, \" \" {} px",
        spaced.width(),
        equals.width(),
        space.width()
    );
    assert_eq!(spaced.height(), equals.height());
}

fn stop(offset: f64, color: &str) -> celesta_composition::GradientStop {
    celesta_composition::GradientStop {
        offset,
        color: color.to_owned(),
    }
}

#[test]
fn linear_gradient_fills_a_rect_and_fades_to_transparent() {
    let fill = Paint::Linear {
        start: Point { x: 0.0, y: 0.0 },
        end: Point { x: 100.0, y: 0.0 },
        stops: vec![stop(0.0, "#ff0000"), stop(1.0, "#00000000")],
    };
    let rect = rasterize_rect(100.0, 10.0, 0.0, Some(&fill), None).unwrap();
    let pixels = rect.pixels();
    let at = |x: usize| &pixels[(5 * 100 + x) * 4..(5 * 100 + x) * 4 + 4];
    assert_eq!(at(0)[..3], [255, 0, 0]);
    assert!(at(0)[3] > 245, "start is opaque: {:?}", at(0));
    // Premultiplied blending keeps the hue while fading out.
    assert!(
        at(50)[0] > 250 && (120..135).contains(&at(50)[3]),
        "{:?}",
        at(50)
    );
    assert!(at(99)[3] < 6, "end is transparent: {:?}", at(99));
}

#[test]
fn radial_gradient_and_solid_stroke_paint_a_rect() {
    let fill = Paint::Radial {
        center: Point { x: 20.0, y: 20.0 },
        radius: 20.0,
        stops: vec![stop(0.0, "#ffffff"), stop(1.0, "#000000")],
    };
    let rect = rasterize_rect(40.0, 40.0, 0.0, Some(&fill), None).unwrap();
    let pixels = rect.pixels();
    let red = |x: usize, y: usize| pixels[(y * 40 + x) * 4];
    assert!(red(20, 20) > 240);
    assert!(red(2, 20) < 40);
}

#[test]
fn gradient_text_varies_across_the_glyphs() {
    let mut rasterizer = TextRasterizer::new();
    let style = TextStyle {
        font_size: Some(64.0),
        fill: Some(Paint::Linear {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 200.0, y: 0.0 },
            stops: vec![stop(0.0, "#ff0000"), stop(1.0, "#0000ff")],
        }),
        ..TextStyle::default()
    };
    let text = rasterizer.rasterize("MMMM", &style, None, 1.0).unwrap();
    let (width, pixels) = (text.width() as usize, text.pixels());
    let opaque = |from: usize, to: usize| {
        (0..text.height() as usize)
            .flat_map(|y| (from..to).map(move |x| (x, y)))
            .map(|(x, y)| &pixels[(y * width + x) * 4..(y * width + x) * 4 + 4])
            .find(|pixel| pixel[3] > 250)
            .map(|pixel| (pixel[0], pixel[2]))
            .unwrap()
    };
    let (left_red, left_blue) = opaque(0, width / 4);
    let (right_red, right_blue) = opaque(width * 3 / 4, width);
    assert!(left_red > right_red && right_blue > left_blue);
}

#[test]
fn letter_spacing_widens_and_tightens_text() {
    let mut rasterizer = TextRasterizer::new();
    let mut width = |letter_spacing| {
        let style = TextStyle {
            font_size: Some(48.0),
            letter_spacing,
            ..TextStyle::default()
        };
        rasterizer
            .rasterize("ABCD", &style, None, 1.0)
            .unwrap()
            .width()
    };
    let (base, wide, tight) = (width(None), width(Some(10.0)), width(Some(-4.0)));
    assert!(wide > base + 30, "wide {wide}, base {base}");
    assert!(tight < base - 10, "tight {tight}, base {base}");
}

/// `line` with every phrase segment joined.
fn join_phrases(line: &str) -> String {
    let segments = phrase_segments(line);
    insert_joiners(
        line,
        segments
            .iter()
            .flat_map(|segment| segment.joiners.iter().copied()),
    )
}

#[test]
fn joins_the_characters_of_each_phrase() {
    // BudouX splits this into "Google の" and "使命は、"; a line may
    // still break after the space, and "、" never starts a line anyway.
    assert_eq!(
        join_phrases("Google の使命は、"),
        "Google の使\u{2060}命\u{2060}は、"
    );
    // Nothing to join when a line can only break between phrases.
    assert!(phrase_segments("Celesta renders every frame").is_empty());
}

#[test]
fn splits_a_phrase_at_breaks_the_text_asks_for() {
    // BudouX keeps "第一章　はじめに" as one phrase; the line may break
    // after the ideographic space, so "はじめに" is measured by itself.
    let line = "第一章　はじめに";
    let segments: Vec<&str> = phrase_segments(line)
        .iter()
        .map(|segment| &line[segment.range.clone()])
        .collect();
    assert!(segments.contains(&"はじめに"), "{segments:?}");
}

#[test]
fn keeps_breaks_the_text_asks_for_inside_a_phrase() {
    // A zero width space, a soft hyphen, and an ideographic space.
    let joined = join_phrases("世界\u{200B}中の情\u{00AD}報を整\u{3000}理し");
    assert!(joined.contains(WORD_JOINER), "{joined:?}");
    for character in ['\u{200B}', '\u{00AD}', '\u{3000}'] {
        assert!(
            !joined.contains(&format!("{character}{WORD_JOINER}")),
            "{joined:?}"
        );
    }
}

/// Each line of `text` as `measure` lays it out with `line_break`.
fn measured_lines(
    rasterizer: &mut TextRasterizer,
    text: &str,
    line_break: LineBreak,
    max_width: f64,
) -> Vec<String> {
    let style = TextStyle {
        font_size: Some(40.0),
        line_break: Some(line_break),
        ..TextStyle::default()
    };
    let metrics = rasterizer.measure(text, &style, Some(max_width));
    let mut lines = vec![String::new(); metrics.lines];
    for glyph in metrics.glyphs {
        lines[glyph.line].push_str(&glyph.text);
    }
    lines
}

/// The width of `characters` average characters of `text`, in the
/// font `measured_lines` uses: tests cannot rely on which font the
/// system draws Japanese with.
fn characters_wide(rasterizer: &mut TextRasterizer, text: &str, characters: f64) -> f64 {
    let style = TextStyle {
        font_size: Some(40.0),
        ..TextStyle::default()
    };
    let unwrapped = rasterizer.measure(text, &style, None);
    unwrapped.width / text.chars().count() as f64 * characters
}

/// The byte offsets where each line but the first starts.
fn line_starts(lines: &[String]) -> Vec<usize> {
    lines
        .iter()
        .scan(0, |start, line| {
            *start += line.len();
            Some(*start)
        })
        .take(lines.len().saturating_sub(1))
        .collect()
}

#[test]
fn phrase_line_break_wraps_only_between_phrases() {
    let text = "フレームは時刻の関数なので、どのフレームからでも描き直せるのだ。";
    let phrases = celesta_budoux::Parser::japanese().parse(text);
    let phrase_starts = celesta_budoux::Parser::japanese().parse_boundaries(text);
    let mut rasterizer = TextRasterizer::new();
    // Just wider than the longest phrase.
    let longest = phrases
        .iter()
        .map(|phrase| phrase.chars().count())
        .max()
        .unwrap();
    let max_width = characters_wide(&mut rasterizer, text, longest as f64 + 1.5);

    let normal = measured_lines(&mut rasterizer, text, LineBreak::Normal, max_width);
    assert!(
        line_starts(&normal)
            .iter()
            .any(|start| !phrase_starts.contains(start)),
        "{normal:?}"
    );

    let phrase = measured_lines(&mut rasterizer, text, LineBreak::Phrase, max_width);
    // The word joiners are not in the measured glyphs.
    assert_eq!(phrase.concat(), text);
    assert!(phrase.len() > 1, "{phrase:?}");
    assert!(
        line_starts(&phrase)
            .iter()
            .all(|start| phrase_starts.contains(start)),
        "{phrase:?}"
    );
}

#[test]
fn phrase_line_break_wraps_a_phrase_wider_than_the_line_as_normal_text() {
    let text = "関数なので、";
    assert_eq!(celesta_budoux::Parser::japanese().parse(text), [text]);
    let mut rasterizer = TextRasterizer::new();
    // Room for "関数なので" but not "、", which must not start a line.
    let max_width = characters_wide(&mut rasterizer, text, 5.5);
    let normal = measured_lines(&mut rasterizer, text, LineBreak::Normal, max_width);
    let phrase = measured_lines(&mut rasterizer, text, LineBreak::Phrase, max_width);
    assert!(phrase.len() > 1, "{phrase:?}");
    assert_eq!(phrase, normal);
}

#[test]
fn phrase_line_break_still_wraps_at_spaces() {
    let text = "Celesta renders every frame";
    let mut rasterizer = TextRasterizer::new();
    let style = TextStyle {
        font_size: Some(40.0),
        ..TextStyle::default()
    };
    let max_width = rasterizer.measure("Celesta renders", &style, None).width + 1.0;
    assert_eq!(
        measured_lines(&mut rasterizer, text, LineBreak::Phrase, max_width),
        measured_lines(&mut rasterizer, text, LineBreak::Normal, max_width)
    );
}

fn measure_text(
    rasterizer: &mut TextRasterizer,
    text: &str,
    line_break: LineBreak,
    letter_spacing: Option<f64>,
    max_width: f64,
) -> TextMetrics {
    let style = TextStyle {
        font_size: Some(40.0),
        letter_spacing,
        line_break: Some(line_break),
        ..TextStyle::default()
    };
    rasterizer.measure(text, &style, Some(max_width))
}

#[test]
fn phrase_line_break_keeps_the_width_of_unwrapped_text() {
    let text = "フレームは時刻の関数なので、どのフレームからでも描き直せるのだ。";
    let mut rasterizer = TextRasterizer::new();
    for letter_spacing in [None, Some(10.0)] {
        let normal = measure_text(
            &mut rasterizer,
            text,
            LineBreak::Normal,
            letter_spacing,
            1e5,
        );
        let phrase = measure_text(
            &mut rasterizer,
            text,
            LineBreak::Phrase,
            letter_spacing,
            1e5,
        );
        assert_eq!((normal.lines, phrase.lines), (1, 1));
        // Word joiners add no letter spacing. A joined phrase is shaped
        // as one word, so kerning and fallback fonts inside it may move
        // it by a few pixels, never by a glyph.
        assert!(
            (normal.width - phrase.width).abs() < 20.0,
            "{letter_spacing:?}: normal {}, phrase {}",
            normal.width,
            phrase.width
        );
    }
}

#[test]
fn phrase_line_break_overflows_no_further_than_normal() {
    let mut rasterizer = TextRasterizer::new();
    for text in [
        "フレームは時刻の関数なので、どのフレームからでも描き直せるのだ。",
        "第一章　はじめに読むべきこと",
        "Google の使命は、世界中の情報を整理し、世界中の人がアクセスできて使えるようにすることです。",
    ] {
        for letter_spacing in [None, Some(10.0)] {
            for characters in [4.5, 6.5, 9.5, 14.0] {
                let max_width = characters_wide(&mut rasterizer, text, characters);
                let [normal, phrase] = [LineBreak::Normal, LineBreak::Phrase].map(|line_break| {
                    measure_text(&mut rasterizer, text, line_break, letter_spacing, max_width)
                });
                // A word that cannot break ("Google") overflows either
                // way; a phrase never makes a line overflow further.
                assert!(
                    phrase.width <= max_width.max(normal.width) + 0.01,
                    "{text:?} {letter_spacing:?} at {max_width}: normal {}, phrase {}",
                    normal.width,
                    phrase.width
                );
            }
        }
    }
}

#[test]
fn phrase_line_break_measures_each_part_of_a_phrase_with_a_space() {
    let text = "第一章　はじめに読むべきこと";
    let mut rasterizer = TextRasterizer::new();
    // Room for "はじめに" but not for "第一章　はじめに".
    let max_width = characters_wide(&mut rasterizer, text, 5.5);
    let lines = measured_lines(&mut rasterizer, text, LineBreak::Phrase, max_width);
    assert!(
        lines
            .iter()
            .any(|line| line.trim() == "はじめに" || line.starts_with("はじめに")),
        "{lines:?}"
    );
}

#[test]
fn baseline_anchored_text_layers_share_a_baseline() {
    const BASELINE: u32 = 200;
    let text_layer = |id: &str, text: &str, x: f64, font_size: f64| Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform {
            position: Point {
                x,
                y: f64::from(BASELINE),
            },
            anchor: Point { x: 0.0, y: 0.5 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: text.to_owned(),
            style: TextStyle {
                font_size: Some(font_size),
                ..TextStyle::default()
            },
            max_width: None,
            baseline_anchor: true,
        },
    };
    let scene = Scene {
        width: 400,
        height: 300,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![
            text_layer("small", "x", 20.0, 32.0),
            text_layer("tall", "H", 110.0, 96.0),
            text_layer("round", "o", 230.0, 64.0),
            text_layer("descender", "y", 320.0, 64.0),
        ],
    };
    let frame = CpuRenderer::default().render(&scene).unwrap();
    // The lowest inked row between `left` and `right`, ignoring faint
    // antialiasing at the glyph's bottom edge.
    let ink_bottom = |left: u32, right: u32| {
        (0..frame.height())
            .rev()
            .find(|&y| {
                (left..right).any(|x| {
                    let offset = ((y * frame.width() + x) * 4) as usize;
                    frame.pixels()[offset] > 128
                })
            })
            .unwrap()
    };
    // Glyphs without descenders rest on the baseline; `y` hangs below it.
    for (name, left, right) in [("x", 20, 100), ("H", 110, 220), ("o", 230, 310)] {
        let bottom = ink_bottom(left, right);
        assert!(
            bottom.abs_diff(BASELINE - 1) <= 2,
            "{name} ends on row {bottom}, not above the baseline at {BASELINE}"
        );
    }
    assert!(ink_bottom(320, 400) > BASELINE + 5);
}

#[test]
fn alpha_composites_in_painter_order() {
    let mut destination = [0, 0, 0, 255];
    blend(&mut destination, Color::rgba(200, 100, 0, 255), 0.5);
    assert_eq!(destination, [100, 50, 0, 255]);
}

#[test]
fn blends_each_mode_with_the_backdrop() {
    let blended = |mode: BlendMode, opacity: f64| {
        let mut destination = [200, 100, 50, 255];
        blend_with_mode(
            &mut destination,
            Color::rgba(100, 200, 250, 255),
            opacity,
            mode,
        );
        destination
    };
    assert_eq!(blended(BlendMode::Normal, 1.0), [100, 200, 250, 255]);
    assert_eq!(blended(BlendMode::Multiply, 1.0), [78, 78, 49, 255]);
    assert_eq!(blended(BlendMode::Screen, 1.0), [222, 222, 251, 255]);
    assert_eq!(blended(BlendMode::Overlay, 1.0), [188, 157, 98, 255]);
    assert_eq!(blended(BlendMode::Add, 1.0), [255, 255, 255, 255]);
    assert_eq!(blended(BlendMode::Difference, 1.0), [100, 100, 200, 255]);
    // Opacity fades between the backdrop and the blended color.
    assert_eq!(blended(BlendMode::Difference, 0.5), [150, 100, 125, 255]);

    // Over a transparent backdrop the source composites unchanged.
    let mut destination = [0, 0, 0, 0];
    blend_with_mode(
        &mut destination,
        Color::rgba(100, 200, 250, 128),
        1.0,
        BlendMode::Difference,
    );
    assert_eq!(destination, [100, 200, 250, 128]);
}

fn rect_layer(id: &str, x: f64, width: f64, color: &str, blend_mode: BlendMode) -> Layer {
    Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform {
            position: Point { x, y: 0.0 },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode,
        effects: Default::default(),
        content: LayerContent::Rect {
            width,
            height: 1.0,
            fill: Some(Paint::Solid {
                color: color.to_owned(),
            }),
            stroke: None,
            corner_radius: 0.0,
        },
    }
}

#[test]
fn blends_a_layer_with_everything_beneath_it() {
    let scene = Scene {
        width: 4,
        height: 1,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![
            rect_layer("light", 2.0, 2.0, "#ffffff", BlendMode::Normal),
            // Inside a plain group, a child still blends with what the
            // group's siblings drew.
            Layer {
                id: "group".to_owned(),
                transform: EvaluatedTransform {
                    anchor: Point { x: 0.0, y: 0.0 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Group {
                    layers: vec![rect_layer(
                        "hud",
                        1.0,
                        2.0,
                        "#e0e0e0",
                        BlendMode::Difference,
                    )],
                    clip: None,
                },
            },
        ],
    };
    let frame = CpuRenderer::new(RenderOptions {
        background: Color::rgba(0, 0, 0, 255),
    })
    .render(&scene)
    .unwrap();
    let pixel = |x: usize| frame.pixels()[x * 4..x * 4 + 4].to_vec();
    assert_eq!(pixel(0), [0, 0, 0, 255]);
    // Light over the dark background, dark over the light rect.
    assert_eq!(pixel(1), [224, 224, 224, 255]);
    assert_eq!(pixel(2), [31, 31, 31, 255]);
    assert_eq!(pixel(3), [255, 255, 255, 255]);
}

#[test]
fn blends_an_isolated_group_as_one_layer() {
    let group = |opacity: f64| Layer {
        id: "group".to_owned(),
        transform: EvaluatedTransform {
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity,
        blend_mode: BlendMode::Difference,
        effects: Default::default(),
        content: LayerContent::Group {
            layers: vec![
                rect_layer("white", 0.0, 3.0, "#ffffff", BlendMode::Normal),
                // Composites onto the group's own layer, not the scene.
                rect_layer("red", 1.0, 1.0, "#ff0000", BlendMode::Multiply),
            ],
            clip: None,
        },
    };
    let render = |opacity: f64| {
        let scene = Scene {
            width: 4,
            height: 1,
            frame_rate: Rational::new(30, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![
                rect_layer("gray", 0.0, 4.0, "#808080", BlendMode::Normal),
                group(opacity),
            ],
        };
        CpuRenderer::new(RenderOptions {
            background: Color::rgba(0, 0, 0, 255),
        })
        .render(&scene)
        .unwrap()
    };

    let frame = render(1.0);
    let pixel = |x: usize| frame.pixels()[x * 4..x * 4 + 4].to_vec();
    assert_eq!(pixel(0), [127, 127, 127, 255]);
    // White multiplied by red is red, which then differs from gray.
    assert_eq!(pixel(1), [127, 128, 128, 255]);
    assert_eq!(pixel(2), [127, 127, 127, 255]);
    assert_eq!(pixel(3), [128, 128, 128, 255]);

    // The group's opacity fades the blended result as a whole.
    let frame = render(0.5);
    assert_eq!(frame.pixels()[0..4], [128, 128, 128, 255]);
    assert_eq!(frame.pixels()[4..8], [128, 128, 128, 255]);
}

#[test]
fn decodes_and_draws_a_real_image() {
    let scene = Scene {
        width: 2,
        height: 2,
        frame_rate: Rational::new(60, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![Layer {
            id: "checker".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 1.0, y: 1.0 },
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
                    id: "checker".to_owned(),
                    location: AssetLocation::File {
                        path: "tests/assets/checker.ppm".to_owned(),
                    },
                },
            },
        }],
    };
    let mut renderer = CpuRenderer::default().with_asset_root(env!("CARGO_MANIFEST_DIR"));
    let frame = renderer.render(&scene).unwrap();

    assert_eq!(
        frame.pixels(),
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ]
    );
}

#[test]
fn renders_a_video_frame_from_the_injected_decoder() {
    struct Decoder;

    impl VideoFrameDecoder for Decoder {
        fn decode_frame(
            &mut self,
            path: &Path,
            source_time_seconds: f64,
        ) -> Result<VideoFrame, MediaError> {
            assert_eq!(path, Path::new("./clip.mp4"));
            assert_eq!(source_time_seconds, 2.0);
            Ok(VideoFrame {
                width: 1,
                height: 1,
                pixels: vec![12, 34, 56, 255].into(),
            })
        }
    }

    let scene = Scene {
        width: 1,
        height: 1,
        frame_rate: Rational::new(60, 1),
        time: Time::new(3, 2),
        fonts: Vec::new(),
        layers: vec![Layer {
            id: "video".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 0.5, y: 0.5 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Video {
                asset: ResolvedAsset {
                    id: "clip".to_owned(),
                    location: AssetLocation::File {
                        path: "clip.mp4".to_owned(),
                    },
                },
                timing: MediaTiming {
                    local_time: Time::new(1, 2),
                    source_start: Time::new(1, 1),
                    source_time_seconds: 2.0,
                    playback_rate: 2.0,
                },
            },
        }],
    };
    let mut renderer = CpuRenderer::default().with_video_decoder(Decoder);
    let frame = renderer.render(&scene).unwrap();
    assert_eq!(frame.pixels(), &[12, 34, 56, 255]);
}

// `examples/assets/lipsync-fixture.psd` mimics a real "tachie" PSD: a
// 240x320 canvas with every folder saved hidden and a `face/mouth`
// group of six small vowel shapes at their true positions. Regenerate it
// with `packages/react/scripts/make-lipsync-fixture.mjs`.
fn lipsync_fixture_psd() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/lipsync-fixture.psd")
}

fn pixel_at(frame: &RgbaFrame, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * frame.width + x) * 4) as usize;
    frame.pixels[index..index + 4].try_into().unwrap()
}

#[test]
fn rasterize_psd_without_a_preset_renders_nothing_when_every_folder_is_hidden() {
    let frame = rasterize_psd("fixture", &lipsync_fixture_psd(), &[], &[], &[]).unwrap();
    assert_eq!((frame.width, frame.height), (240, 320));
    assert!(
        frame.pixels.chunks_exact(4).all(|pixel| pixel[3] == 0),
        "a saved-all-hidden PSD with no preset should compose to nothing"
    );
}

#[test]
fn rasterize_psd_composes_the_preset_and_the_selected_mouth_at_real_coordinates() {
    let preset = [
        "body".to_owned(),
        "body/base".to_owned(),
        "body/outfit-navy".to_owned(),
        "face".to_owned(),
        "face/eyes".to_owned(),
        "face/eyes/open".to_owned(),
    ];
    let frame = rasterize_psd(
        "fixture",
        &lipsync_fixture_psd(),
        &preset,
        &["face/mouth/a".to_owned()],
        &["face/mouth/o".to_owned()],
    )
    .unwrap();

    // The navy outfit rect covers (56,176)..(184,296).
    assert_eq!(pixel_at(&frame, 120, 220), [40, 60, 130, 255]);
    // The "a" mouth is a 24x20 rect at (108,142) — force-enabled even
    // though its layer and every ancestor folder are saved hidden. Its
    // red channel dominates, unlike the skin behind it.
    let mouth = pixel_at(&frame, 120, 150);
    assert_eq!(mouth[3], 255);
    assert!(mouth[0] > 150 && mouth[0] > mouth[1] + 40 && mouth[0] > mouth[2] + 40);
    // A point clear of every visible layer stays transparent.
    assert_eq!(pixel_at(&frame, 5, 5), [0, 0, 0, 0]);
}

#[test]
fn rasterize_psd_applies_each_layer_blend_mode() {
    // 3×1: an opaque rgb(200, 100, 50) base; over it, one pixel each of a
    // multiply layer of rgb(128, 128, 255), a screen layer of
    // rgb(128, 128, 128), and a normal layer of rgb(10, 20, 30).
    // Written with ag-psd, whose layers the `psd` crate reads as hidden,
    // so they are listed as the visible set.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blend-modes.psd");
    let layers = ["base", "multiply", "screen", "normal"].map(str::to_owned);
    let frame = rasterize_psd("fixture", &fixture, &layers, &[], &[]).unwrap();
    let close = |actual: [u8; 4], expected: [u8; 4]| {
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 1)
    };
    let multiply = pixel_at(&frame, 0, 0);
    assert!(
        close(multiply, [100, 50, 50, 255]),
        "multiply: {multiply:?}"
    );
    let screen = pixel_at(&frame, 1, 0);
    assert!(close(screen, [228, 178, 153, 255]), "screen: {screen:?}");
    assert_eq!(pixel_at(&frame, 2, 0), [10, 20, 30, 255]);
}

#[test]
fn psd_blend_modes_mix_like_photoshop() {
    let mix = |mode| psd_blend_channel(mode).unwrap();
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(psd_blend_channel("Normal").is_none());
    assert!(psd_blend_channel("Hue").is_none());
    assert!(close(mix("Multiply")(0.5, 0.5), 0.25));
    assert!(close(mix("Screen")(0.5, 0.5), 0.75));
    assert!(close(mix("Overlay")(0.25, 0.5), 0.25));
    assert!(close(mix("Darken")(0.3, 0.6), 0.3));
    assert!(close(mix("Lighten")(0.3, 0.6), 0.6));
    assert!(close(mix("LinearDodge")(0.7, 0.6), 1.0));
    assert!(close(mix("Subtract")(0.3, 0.6), 0.0));
    assert!(close(mix("Difference")(0.3, 0.8), 0.5));
    assert!(close(mix("ColorDodge")(0.25, 0.5), 0.5));
    assert!(close(mix("ColorBurn")(0.75, 0.5), 0.5));
    // White and black are neutral where Photoshop says they are.
    for mode in ["Multiply", "ColorBurn", "LinearBurn"] {
        assert!(close(mix(mode)(0.4, 1.0), 0.4), "{mode:?} with white");
    }
    for mode in ["Screen", "ColorDodge", "LinearDodge"] {
        assert!(close(mix(mode)(0.4, 0.0), 0.4), "{mode:?} with black");
    }
}

#[test]
fn rasterize_psd_disabled_layers_win_over_enabled() {
    let mouth = "face/mouth/a".to_owned();
    let with = rasterize_psd(
        "fixture",
        &lipsync_fixture_psd(),
        &["body".to_owned(), "body/base".to_owned()],
        std::slice::from_ref(&mouth),
        &[],
    )
    .unwrap();
    let without = rasterize_psd(
        "fixture",
        &lipsync_fixture_psd(),
        &["body".to_owned(), "body/base".to_owned()],
        std::slice::from_ref(&mouth),
        std::slice::from_ref(&mouth),
    )
    .unwrap();
    assert_ne!(pixel_at(&with, 120, 150), pixel_at(&without, 120, 150));
    // With the mouth suppressed the pixel is the bare skin base.
    assert_eq!(pixel_at(&without, 120, 150), [250, 224, 205, 255]);
}
