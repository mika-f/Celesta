use crate::assets::rasterize_psd;
use crate::composite::{PsdBlend, blend, blend_with_mode, psd_blend};
use crate::error::RenderError;
use crate::fonts::TextRasterizer;
use crate::linebreak::{WORD_JOINER, insert_joiners, phrase_segments};
use crate::rect::rasterize_rect;
use crate::renderer::CpuRenderer;
use crate::text::TextMetrics;
use crate::types::{Color, RenderOptions, RgbaFrame};
use celesta_composition::{LetterSpacing, LineBreak, Point};

use std::path::Path;

use celesta_composition::{
    AssetLocation, BlendMode, Clip, EvaluatedTransform, F32Column, InstanceShape, Layer,
    LayerContent, MediaTiming, Paint, Rational, ResolvedAsset, Scene, TextStyle, Time, U8Column,
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
fn measure_reports_ink_bounds_and_cap_and_x_height() {
    let style = TextStyle {
        font_size: Some(100.0),
        ..TextStyle::default()
    };
    let mut rasterizer = TextRasterizer::new();
    let metrics = rasterizer.measure("Hgx ", &style, None);

    // Both are 0 for a font without the OS/2 values, as some system fonts are.
    assert!(metrics.cap_height >= metrics.x_height);
    let [h, g, x, space] = &metrics.glyphs[..] else {
        panic!("expected four clusters");
    };
    // A capital sits on the baseline at the font's cap height.
    if metrics.cap_height > 0.0 {
        assert!((h.ink_ascent - metrics.cap_height).abs() < 2.0);
    }
    assert!(h.ink_ascent > 20.0);
    assert!(h.ink_descent.abs() < 2.0);
    assert!(g.ink_descent > 5.0, "g should descend below the baseline");
    assert!(x.ink_ascent < h.ink_ascent);
    assert_eq!((space.ink_ascent, space.ink_descent), (0.0, 0.0));
}

#[test]
fn wrapped_text_is_anchored_by_its_line_boxes_like_measure() {
    let style = TextStyle {
        font_size: Some(40.0),
        line_height: Some(56.0),
        ..TextStyle::default()
    };
    let mut rasterizer = TextRasterizer::new();
    let max_width = Some(120.0);
    let metrics = rasterizer.measure("Celesta Celesta Celesta", &style, max_width);
    assert!(metrics.lines > 1, "the text should wrap");
    let text = rasterizer
        .rasterize("Celesta Celesta Celesta", &style, max_width, 1.0)
        .unwrap();
    // Like text broken with `\n`: the anchor box is the line boxes, so the
    // first baseline sits `ascent` below its top, where `measure` puts it.
    assert_eq!(f64::from(text.anchor_box.height), metrics.height.ceil());
    let baseline_in_box = f64::from(text.baseline() - text.anchor_box.top as f32);
    assert!(
        (baseline_in_box - metrics.ascent).abs() < 1e-3,
        "baseline {baseline_in_box} is not {} below the top",
        metrics.ascent
    );
}

#[test]
fn wrapped_text_anchored_at_its_top_draws_where_its_measured_baseline_does() {
    let style = TextStyle {
        font_size: Some(40.0),
        line_height: Some(56.0),
        fill: Some(Paint::Solid {
            color: "#FFFFFFFF".to_owned(),
        }),
        ..TextStyle::default()
    };
    let text = "Celesta Celesta Celesta";
    let max_width = 120.0;
    let ascent = TextRasterizer::new()
        .measure(text, &style, Some(max_width))
        .ascent;
    let ink_rows = |y: f64, baseline_anchor: bool| {
        let scene = Scene {
            width: 200,
            height: 300,
            frame_rate: Rational::new(30, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![Layer {
                id: "paragraph".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 20.0, y },
                    anchor: Point { x: 0.0, y: 0.0 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Text {
                    text: text.to_owned(),
                    style: Box::new(style.clone()),
                    max_width: Some(max_width),
                    baseline_anchor,
                },
            }],
            shaders: Vec::new(),
        };
        let frame = CpuRenderer::default().render(&scene).unwrap();
        let rows: Vec<u32> = (0..frame.height())
            .filter(|&y| {
                (0..frame.width()).any(|x| {
                    let offset = ((y * frame.width() + x) * 4) as usize;
                    frame.pixels()[offset] > 128
                })
            })
            .collect();
        (rows[0], rows[rows.len() - 1])
    };
    let (top, bottom) = ink_rows(40.0, false);
    let (measured_top, measured_bottom) = ink_rows(40.0 + ascent, true);
    assert!(
        top.abs_diff(measured_top) <= 1 && bottom.abs_diff(measured_bottom) <= 1,
        "top-anchored ink {top}–{bottom}, measured {measured_top}–{measured_bottom}"
    );
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
        .as_chunks::<4>()
        .0
        .iter()
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
                style: Box::new(TextStyle {
                    font_size: Some(24.0),
                    ..TextStyle::default()
                }),
                max_width: None,
                baseline_anchor: false,
            },
        }],
        shaders: Vec::new(),
    };
    let mut renderer = CpuRenderer::default();
    let frame = renderer.render(&scene).unwrap();
    let png = frame.encode_png().unwrap();

    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert!(
        frame
            .pixels()
            .as_chunks::<4>()
            .0
            .contains(&[255, 255, 255, 255])
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
        shaders: Vec::new(),
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
            mask: None,
            motion_blur: false,
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
fn a_group_scales_about_its_origin() {
    // Scaled 2x about the group's (5, 5), which stays at position + origin
    // = (13, 19), its 0..10 by 0..10 lands on canvas 3..23 by 9..29.
    let clip = Clip {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        corner_radius: 0.0,
    };
    let scene = clip_scene(vec![clipped_group(
        EvaluatedTransform {
            position: Point { x: 8.0, y: 14.0 },
            origin: Point { x: 5.0, y: 5.0 },
            scale: Point { x: 2.0, y: 2.0 },
            ..EvaluatedTransform::default()
        },
        clip,
        vec![red_square()],
    )]);
    let frame = CpuRenderer::default().render(&scene).unwrap();

    assert!(is_red(&frame, 3, 9));
    assert!(is_red(&frame, 22, 28));
    for (x, y) in [(2, 9), (23, 20), (10, 8), (10, 29)] {
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
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == background())
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
            style: Box::new(TextStyle {
                font_size: Some(30.0),
                fill: Some(Paint::Solid {
                    color: "#FFFFFFFF".to_owned(),
                }),
                ..TextStyle::default()
            }),
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
        shaders: Vec::new(),
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
fn a_stroke_without_fill_keeps_its_color_along_the_inner_edge() {
    // A circle's ring: the inner edge is anti-aliased against no fill, which
    // must fade the stroke's alpha, not darken its color.
    let stroke = celesta_composition::Stroke {
        paint: Paint::Solid {
            color: "#FFC040".to_owned(),
        },
        width: 4.0,
    };
    let ring = rasterize_rect(40.6, 40.6, 20.3, None, Some(&stroke)).unwrap();
    let mut inner_edge = 0;
    for pixel in ring.pixels().as_chunks::<4>().0 {
        if pixel[3] > 0 {
            assert_eq!(&pixel[..3], &[0xFF, 0xC0, 0x40], "pixel {pixel:?}");
            inner_edge += usize::from(pixel[3] < 255);
        }
    }
    assert!(inner_edge > 0);
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
                style: Box::new(TextStyle {
                    font_size: Some(96.0),
                    fill: Some(Paint::Solid {
                        color: "#FFA13BFF".to_owned(),
                    }),
                    align: Some(celesta_composition::TextAlign::Center),
                    ..TextStyle::default()
                }),
                max_width: None,
                baseline_anchor: false,
            },
        }],
        shaders: Vec::new(),
    };
    let mut renderer = CpuRenderer::default();
    let frame = renderer.render(&scene).unwrap();
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0;
    let mut max_y = 0;
    for (index, pixel) in frame.pixels().as_chunks::<4>().0.iter().enumerate() {
        if *pixel != [20, 22, 28, 255] {
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
        stroke: Some(
            celesta_composition::Stroke {
                paint: Paint::Solid {
                    color: "#000000FF".to_owned(),
                },
                width: 16.0,
            }
            .into(),
        ),
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
fn text_strokes_in_a_list_are_drawn_around_one_another() {
    let stroke = |color: &str, width: f64| celesta_composition::Stroke {
        paint: Paint::Solid {
            color: color.to_owned(),
        },
        width,
    };
    let style = |stroke: celesta_composition::TextStroke| TextStyle {
        font_size: Some(96.0),
        fill: Some(Paint::Solid {
            color: "#0000FFFF".to_owned(),
        }),
        stroke: Some(stroke),
        ..TextStyle::default()
    };
    let mut rasterizer = TextRasterizer::new();
    let mut rasterize = |stroke: celesta_composition::TextStroke| {
        rasterizer
            .rasterize("MW", &style(stroke), None, 1.0)
            .unwrap()
    };
    let double = rasterize(vec![stroke("#00FF00FF", 16.0), stroke("#FFFFFFFF", 8.0)].into());
    let reversed = rasterize(vec![stroke("#FFFFFFFF", 8.0), stroke("#00FF00FF", 16.0)].into());
    let single = rasterize(stroke("#00FF00FF", 16.0).into());

    // Padded for the widest stroke, as a single stroke that wide is.
    assert_eq!(
        (double.width(), double.height()),
        (single.width(), single.height())
    );
    // Each width is from the glyph outline, whatever the order.
    assert_eq!(double.pixels(), reversed.pixels());
    let mut colors = [0; 3];
    for (pixel, alone) in double
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(single.pixels().as_chunks::<4>().0.iter())
    {
        match pixel {
            [0, 0, 255, 255] => colors[0] += 1,
            [255, 255, 255, 255] => colors[1] += 1,
            [0, 255, 0, 255] => colors[2] += 1,
            _ => {}
        }
        // The inner stroke meets the outer one without a seam: together they
        // cover what the outer one alone does.
        assert!(
            pixel[3].abs_diff(alone[3]) <= 1,
            "{pixel:?} where one stroke gives {alone:?}"
        );
    }
    assert!(colors.iter().all(|&count| count > 0), "{colors:?}");
}

#[test]
fn a_text_stroke_around_a_transparent_fill_leaves_the_glyphs_hollow() {
    let style = |fill: &str, stroke: &str| TextStyle {
        font_size: Some(96.0),
        fill: Some(Paint::Solid {
            color: fill.to_owned(),
        }),
        stroke: Some(
            celesta_composition::Stroke {
                paint: Paint::Solid {
                    color: stroke.to_owned(),
                },
                width: 8.0,
            }
            .into(),
        ),
        ..TextStyle::default()
    };
    let mut rasterizer = TextRasterizer::new();
    let mut rasterize = |fill: &str, stroke: &str| {
        rasterizer
            .rasterize("MW", &style(fill, stroke), None, 1.0)
            .unwrap()
    };
    let filled = rasterize("#0000FFFF", "#FF0000FF");
    let hollow = rasterize("#00000000", "#FF0000FF");
    let faint = rasterize("#00000000", "#FF000033");
    assert_eq!(
        (hollow.width(), hollow.height()),
        (filled.width(), filled.height())
    );

    let mut inside = 0;
    let mut outline = 0;
    for ((filled, hollow), faint) in filled
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(hollow.pixels().as_chunks::<4>().0.iter())
        .zip(faint.pixels().as_chunks::<4>().0.iter())
    {
        match filled {
            // Inside a glyph: before, the stroke's color showed through here.
            [0, 0, 255, 255] => {
                inside += 1;
                assert_eq!(hollow[3], 0, "a hollow glyph's inside is drawn");
            }
            // Clear of the glyphs, the stroke is the same with either fill.
            [255, 0, 0, 255] => {
                outline += 1;
                assert_eq!(hollow, &[255, 0, 0, 255]);
            }
            _ => {}
        }
        assert!(faint[3] <= 0x33, "a faint stroke is denser than its color");
    }
    assert!(inside > 0 && outline > 0);
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
                    style: Box::new(TextStyle {
                        font_size: Some(72.0),
                        fill: Some(Paint::Solid {
                            color: "#FFFFFFFF".to_owned(),
                        }),
                        stroke: stroke.map(Into::into),
                        ..TextStyle::default()
                    }),
                    max_width: None,
                    baseline_anchor: false,
                },
            }],
            shaders: Vec::new(),
        };
        let frame = CpuRenderer::default().render(&scene).unwrap();
        // The white fill only: the stroke is black.
        let (mut min_x, mut max_x) = (u32::MAX, 0);
        for (index, pixel) in frame.pixels().as_chunks::<4>().0.iter().enumerate() {
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

/// Each line's left and right edge in `metrics`, from its glyphs.
fn line_edges(metrics: &TextMetrics) -> Vec<(f64, f64)> {
    (0..metrics.lines)
        .map(|line| {
            let glyphs = metrics.glyphs.iter().filter(|glyph| glyph.line == line);
            glyphs.fold((f64::MAX, f64::MIN), |(left, right), glyph| {
                (left.min(glyph.x), right.max(glyph.x + glyph.width))
            })
        })
        .collect()
}

#[test]
fn align_without_max_width_measures_lines_within_the_widest() {
    let mut rasterizer = TextRasterizer::new();
    let style = |align| TextStyle {
        font_size: Some(40.0),
        align: Some(align),
        ..TextStyle::default()
    };
    let text = "a\nbbbbbbbb\nccc";
    let center = rasterizer.measure(text, &style(celesta_composition::TextAlign::Center), None);
    let edges = line_edges(&center);
    assert_eq!(edges.len(), 3);
    for (left, right) in &edges {
        assert!(
            (left + right - center.width).abs() < 1e-3,
            "line {left}–{right} is not centered in {}",
            center.width
        );
    }
    assert!(edges[0].0 > 0.0, "the short line should be moved right");
    let right = rasterizer.measure(text, &style(celesta_composition::TextAlign::Right), None);
    for (_, line_right) in line_edges(&right) {
        assert!(
            (line_right - right.width).abs() < 1e-3,
            "line ends at {line_right}, not {}",
            right.width
        );
    }
    // Alignment moves lines, not the box.
    let left = rasterizer.measure(text, &style(celesta_composition::TextAlign::Left), None);
    assert_eq!((center.width, center.height), (left.width, left.height));
    assert_eq!((right.width, right.height), (left.width, left.height));
}

#[test]
fn align_without_max_width_draws_lines_within_the_widest() {
    let line_height = 60.0;
    // Each line's leftmost and rightmost ink column.
    let ink_columns = |align| {
        let scene = Scene {
            width: 400,
            height: 240,
            frame_rate: Rational::new(30, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![Layer {
                id: "lines".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 20.0, y: 20.0 },
                    anchor: Point { x: 0.0, y: 0.0 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Text {
                    text: "a\nbbbbbbbb\nccc".to_owned(),
                    style: Box::new(TextStyle {
                        font_size: Some(40.0),
                        line_height: Some(line_height),
                        align: Some(align),
                        fill: Some(Paint::Solid {
                            color: "#FFFFFFFF".to_owned(),
                        }),
                        ..TextStyle::default()
                    }),
                    max_width: None,
                    baseline_anchor: false,
                },
            }],
            shaders: Vec::new(),
        };
        let frame = CpuRenderer::default().render(&scene).unwrap();
        (0..3)
            .map(|line| {
                let top = 20 + line * line_height as u32;
                let columns: Vec<u32> = (0..frame.width())
                    .filter(|&x| {
                        (top..top + line_height as u32).any(|y| pixel(&frame, x, y)[0] > 128)
                    })
                    .collect();
                (columns[0], columns[columns.len() - 1])
            })
            .collect::<Vec<_>>()
    };
    let center = ink_columns(celesta_composition::TextAlign::Center);
    let middle = |(left, right): (u32, u32)| f64::from(left + right) / 2.0;
    for line in [0, 2] {
        // Side bearings differ between letters, so allow a few pixels.
        assert!(
            (middle(center[line]) - middle(center[1])).abs() <= 4.0,
            "centered lines {center:?} do not share a center"
        );
    }
    let right = ink_columns(celesta_composition::TextAlign::Right);
    for line in [0, 2] {
        assert!(
            right[line].1.abs_diff(right[1].1) <= 4,
            "right-aligned lines {right:?} do not end together"
        );
    }
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
fn gradient_text_fading_out_leaves_transparent_pixels_empty() {
    let mut rasterizer = TextRasterizer::new();
    let style = TextStyle {
        font_size: Some(64.0),
        fill: Some(Paint::Linear {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 100.0, y: 0.0 },
            stops: vec![stop(0.0, "#ff0000"), stop(1.0, "#ff000000")],
        }),
        ..TextStyle::default()
    };
    let text = rasterizer.rasterize("MMMM", &style, None, 1.0).unwrap();
    let pixels = text.pixels().as_chunks::<4>().0.iter();
    assert!(pixels.clone().any(|pixel| pixel[3] > 200));
    assert!(
        pixels
            .filter(|pixel| pixel[3] == 0)
            .all(|pixel| *pixel == [0; 4])
    );
}

#[test]
fn solid_text_fill_alpha_scales_the_glyphs_alpha() {
    let mut rasterizer = TextRasterizer::new();
    let mut max_alpha = |color: &str| {
        let style = TextStyle {
            font_size: Some(64.0),
            fill: Some(Paint::Solid {
                color: color.to_owned(),
            }),
            ..TextStyle::default()
        };
        let text = rasterizer.rasterize("MMMM", &style, None, 1.0).unwrap();
        let pixels = text.pixels().as_chunks::<4>().0.iter();
        assert!(
            pixels
                .clone()
                .all(|pixel| pixel[3] == 0 || pixel[..3] == [255, 0, 0])
        );
        pixels.map(|pixel| pixel[3]).max().unwrap()
    };
    assert_eq!(max_alpha("#FF0000"), 255);
    assert_eq!(max_alpha("#FF000080"), 128);
    assert_eq!(max_alpha("#FF000000"), 0);
}

#[test]
fn solid_text_fill_alpha_leaves_color_emoji_glyphs_alone() {
    let mut rasterizer = TextRasterizer::new();
    let mut pixels = |color: &str| {
        let style = TextStyle {
            font_size: Some(64.0),
            fill: Some(Paint::Solid {
                color: color.to_owned(),
            }),
            ..TextStyle::default()
        };
        rasterizer
            .rasterize("\u{1F525}", &style, None, 1.0)
            .unwrap()
            .pixels()
            .to_vec()
    };
    let (opaque, translucent) = (pixels("#FFFFFF"), pixels("#FFFFFF80"));
    let distinct_colors: std::collections::HashSet<[u8; 3]> = opaque
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    if distinct_colors.len() <= 1 {
        eprintln!("skipping: no color-emoji font available in this environment");
        return;
    }
    // Even emoji pixels whose color matches the fill keep their own alpha.
    assert_eq!(opaque, translucent);
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
    let (base, wide, tight) = (
        width(None),
        width(Some(10.0.into())),
        width(Some((-4.0).into())),
    );
    assert!(wide > base + 30, "wide {wide}, base {base}");
    assert!(tight < base - 10, "tight {tight}, base {base}");
}

#[test]
fn em_letter_spacing_scales_with_the_font_size() {
    let mut rasterizer = TextRasterizer::new();
    let mut width = |font_size, letter_spacing| {
        let style = TextStyle {
            font_size: Some(font_size),
            letter_spacing: Some(letter_spacing),
            ..TextStyle::default()
        };
        rasterizer.measure("ABCD", &style, None).width
    };
    for font_size in [24.0, 48.0, 96.0] {
        assert_eq!(
            width(font_size, LetterSpacing::Em { em: -0.05 }),
            width(font_size, LetterSpacing::Px(-0.05 * font_size)),
            "{font_size}px"
        );
    }
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
        letter_spacing: letter_spacing.map(Into::into),
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
            style: Box::new(TextStyle {
                font_size: Some(font_size),
                ..TextStyle::default()
            }),
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
        shaders: Vec::new(),
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
    assert_eq!(blended(BlendMode::Darken, 1.0), [100, 100, 50, 255]);
    assert_eq!(blended(BlendMode::Lighten, 1.0), [200, 200, 250, 255]);
    assert_eq!(blended(BlendMode::Exclusion, 1.0), [143, 143, 202, 255]);
    // The backdrop lifted by 0.2, the difference in the spec's luminosity.
    assert_eq!(blended(BlendMode::Luminosity, 1.0), [251, 151, 101, 255]);
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
                    mask: None,
                    motion_blur: false,
                },
            },
        ],
        shaders: Vec::new(),
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
            mask: None,
            motion_blur: false,
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
            shaders: Vec::new(),
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
fn fades_a_translucent_group_as_one_layer() {
    let render = |layers: Vec<Layer>| {
        let scene = Scene {
            width: 4,
            height: 1,
            frame_rate: Rational::new(30, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![
                rect_layer("gray", 0.0, 4.0, "#808080", BlendMode::Normal),
                Layer {
                    id: "group".to_owned(),
                    transform: EvaluatedTransform {
                        anchor: Point { x: 0.0, y: 0.0 },
                        ..EvaluatedTransform::default()
                    },
                    opacity: 0.5,
                    blend_mode: BlendMode::Normal,
                    effects: Default::default(),
                    content: LayerContent::Group {
                        layers,
                        clip: None,
                        mask: None,
                        motion_blur: false,
                    },
                },
            ],
            shaders: Vec::new(),
        };
        CpuRenderer::new(RenderOptions {
            background: Color::rgba(0, 0, 0, 255),
        })
        .render(&scene)
        .unwrap()
    };
    let faded = |color: Color| {
        let mut destination = [128, 128, 128, 255];
        blend(&mut destination, color, 0.5);
        destination.to_vec()
    };
    let white = Color::rgba(255, 255, 255, 255);
    let red = Color::rgba(255, 0, 0, 255);

    // The white backing does not show through the red card over it.
    let frame = render(vec![
        rect_layer("white", 0.0, 3.0, "#ffffff", BlendMode::Normal),
        rect_layer("red", 1.0, 1.0, "#ff0000", BlendMode::Normal),
    ]);
    let pixel = |x: usize| frame.pixels()[x * 4..x * 4 + 4].to_vec();
    assert_eq!(pixel(0), faded(white));
    assert_eq!(pixel(1), faded(red));
    assert_eq!(pixel(3), [128, 128, 128, 255]);

    // A lone shape just takes the group's opacity as well.
    let frame = render(vec![rect_layer(
        "red",
        1.0,
        1.0,
        "#ff0000",
        BlendMode::Normal,
    )]);
    assert_eq!(frame.pixels()[4..8], faded(red));

    // A lone blended child blends with the group's own transparent layer,
    // so it composites unchanged, rather than multiplying the gray.
    let frame = render(vec![rect_layer(
        "red",
        1.0,
        1.0,
        "#ff0000",
        BlendMode::Multiply,
    )]);
    assert_eq!(frame.pixels()[4..8], faded(red));
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
                animation: None,
                asset: ResolvedAsset {
                    id: "checker".to_owned(),
                    location: AssetLocation::File {
                        path: "tests/assets/checker.ppm".to_owned(),
                    },
                },
            },
        }],
        shaders: Vec::new(),
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
            // The red channel says which time was asked for.
            Ok(VideoFrame {
                width: 1,
                height: 1,
                pixels: vec![(source_time_seconds * 10.0) as u8, 34, 56, 255].into(),
            })
        }

        fn video_duration(&mut self, _: &Path) -> Result<Option<f64>, MediaError> {
            Ok(Some(1.5))
        }
    }

    let mut scene = Scene {
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
                    r#loop: false,
                },
                width: None,
                height: None,
            },
        }],
        shaders: Vec::new(),
    };
    let mut renderer = CpuRenderer::default().with_video_decoder(Decoder);
    let frame = renderer.render(&scene).unwrap();
    assert_eq!(frame.pixels(), &[20, 34, 56, 255]);

    // A looping clip 2 s into a 1.5 s video is 0.5 s into its second pass.
    let LayerContent::Video { timing, .. } = &mut scene.layers[0].content else {
        unreachable!()
    };
    timing.r#loop = true;
    let frame = renderer.render(&scene).unwrap();
    assert_eq!(frame.pixels(), &[5, 34, 56, 255]);
}

/// How far right and down the pixels `covered` picks reach in `frame`: the
/// drawn size of a layer placed at the origin.
fn drawn_size(frame: &RgbaFrame, covered: impl Fn(&[u8; 4]) -> bool) -> (u32, u32) {
    frame
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, pixel)| covered(pixel))
        .fold((0, 0), |(right, bottom), (index, _)| {
            let (x, y) = (index as u32 % frame.width, index as u32 / frame.width);
            (right.max(x + 1), bottom.max(y + 1))
        })
}

#[test]
fn sizes_a_video_frame_like_an_image() {
    /// A 320 x 180 video frame, all red.
    struct Decoder;

    impl VideoFrameDecoder for Decoder {
        fn decode_frame(&mut self, _: &Path, _: f64) -> Result<VideoFrame, MediaError> {
            Ok(VideoFrame {
                width: 320,
                height: 180,
                pixels: [255, 0, 0, 255].repeat(320 * 180).into(),
            })
        }
    }

    let scene = |width, height| Scene {
        width: 320,
        height: 180,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![Layer {
            id: "video".to_owned(),
            transform: EvaluatedTransform {
                anchor: Point { x: 0.0, y: 0.0 },
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
                    local_time: Time::ZERO,
                    source_start: Time::ZERO,
                    source_time_seconds: 0.0,
                    playback_rate: 1.0,
                    r#loop: false,
                },
                width,
                height,
            },
        }],
        shaders: Vec::new(),
    };
    let sizes = [
        (Some(160.0), None, (160, 90)),
        (Some(160.0), Some(160.0), (160, 160)),
        (None, Some(90.0), (160, 90)),
        (None, None, (320, 180)),
    ];
    let mut renderer = CpuRenderer::default().with_video_decoder(Decoder);
    for (width, height, drawn) in sizes {
        let frame = renderer.render(&scene(width, height)).unwrap();
        assert_eq!(
            drawn_size(&frame, |pixel| pixel[0] >= 128),
            drawn,
            "width {width:?}, height {height:?}"
        );
    }

    // Without a decoder the 320 x 180 placeholder is sized the same way.
    let mut renderer = CpuRenderer::default();
    for (width, height, drawn) in sizes {
        let frame = renderer.render(&scene(width, height)).unwrap();
        assert_eq!(
            drawn_size(&frame, |pixel| pixel[2] >= 128),
            drawn,
            "placeholder width {width:?}, height {height:?}"
        );
    }
}

// `examples/assets/lipsync-fixture.psd` mimics a real "tachie" PSD: a
// 240x320 canvas with every folder saved hidden and a `face/mouth`
// group of six small vowel shapes at their true positions. Regenerate it
// with `packages/cli/scripts/make-lipsync-fixture.mjs`.
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
        frame
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 0),
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
fn rasterize_psd_mixes_whole_colors_for_hue_saturation_color_and_luminosity() {
    // 4×1: an opaque rgb(200, 100, 50) base; over it one pixel each of a
    // hue layer of rgb(0, 0, 255), a saturation layer of rgb(128, 128, 128),
    // a color layer of rgb(255, 0, 0) and a luminosity layer of
    // rgb(100, 200, 250). Written with ag-psd, as `blend-modes.psd`.
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/non-separable-blend-modes.psd");
    let layers = ["base", "hue", "saturation", "color", "luminosity"].map(str::to_owned);
    let frame = rasterize_psd("fixture", &fixture, &layers, &[], &[]).unwrap();
    let close = |actual: [u8; 4], expected: [u8; 4]| {
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 1)
    };
    // Worked out from the spec's formulas; the base's luminosity is 0.488.
    let expected = [
        // Blue at the base's saturation, lifted to its luminosity and
        // clipped below 1.
        ("hue", [108, 108, 255, 255]),
        // A gray source has no saturation: gray at the base's luminosity.
        ("saturation", [125, 125, 125, 255]),
        // Red lifted to the base's luminosity, then clipped below 1.
        ("color", [255, 69, 69, 255]),
        // The base lifted by 0.2, the difference in luminosity.
        ("luminosity", [251, 151, 101, 255]),
    ];
    for (x, (mode, expected)) in expected.into_iter().enumerate() {
        let actual = pixel_at(&frame, x as u32, 0);
        assert!(close(actual, expected), "{mode}: {actual:?}");
    }
}

#[test]
fn psd_blend_modes_mix_like_photoshop() {
    let mix = |mode| {
        let blend = psd_blend(mode).unwrap();
        move |b: f64, s: f64| blend.mix([b; 3], [s; 3])[0]
    };
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    for mode in [
        "Normal",
        "PassThrough",
        "Dissolve",
        "DarkerColor",
        "LighterColor",
    ] {
        assert!(psd_blend(mode).is_none(), "{mode} draws as normal");
    }
    // The modes layers also have use the same functions.
    for (mode, shared) in [
        ("LinearDodge", BlendMode::Add),
        ("SoftLight", BlendMode::SoftLight),
        ("Hue", BlendMode::Hue),
        ("Saturation", BlendMode::Saturation),
        ("Color", BlendMode::Color),
        ("Luminosity", BlendMode::Luminosity),
    ] {
        assert!(
            matches!(psd_blend(mode), Some(PsdBlend::Mode(m)) if m == shared),
            "{mode}"
        );
    }
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
    // Vivid light: color-burn by 2Cs below a half, color-dodge by 2Cs - 1 above.
    assert!(close(mix("VividLight")(0.75, 0.25), 0.5));
    assert!(close(mix("VividLight")(0.75, 0.75), 1.0));
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

// `examples/assets/saved-pose-fixture.psd` is saved showing a pose, like a
// public character PSD: the body, a prop, and one variant per face folder,
// with an `extras` folder hidden. Regenerate it with
// `packages/cli/scripts/make-saved-pose-fixture.mjs`.
fn saved_pose_fixture_psd() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/saved-pose-fixture.psd")
}

#[test]
fn rasterize_psd_without_a_preset_renders_the_saved_visibility() {
    let frame = rasterize_psd("fixture", &saved_pose_fixture_psd(), &[], &[], &[]).unwrap();
    assert_eq!(pixel_at(&frame, 120, 220), [40, 60, 130, 255], "outfit");
    assert_eq!(pixel_at(&frame, 120, 94), [90, 60, 40, 255], "saved brow");
    assert_eq!(
        pixel_at(&frame, 120, 85),
        [250, 224, 205, 255],
        "hidden brow"
    );
    assert_eq!(
        pixel_at(&frame, 120, 151),
        [180, 90, 90, 255],
        "saved mouth"
    );
    assert_eq!(pixel_at(&frame, 195, 200), [200, 40, 40, 255], "prop");
    // The hat, in a folder saved hidden, would cover this clear pixel.
    assert_eq!(pixel_at(&frame, 120, 15), [0, 0, 0, 0], "hidden folder");
}

#[test]
fn rasterize_psd_swaps_a_face_part_on_top_of_the_saved_visibility() {
    let frame = rasterize_psd(
        "fixture",
        &saved_pose_fixture_psd(),
        &[],
        &["face/brows/smile".to_owned()],
        &["face/brows".to_owned(), "props".to_owned()],
    )
    .unwrap();
    assert_eq!(
        pixel_at(&frame, 120, 85),
        [90, 60, 40, 255],
        "expression brow"
    );
    assert_eq!(
        pixel_at(&frame, 120, 94),
        [250, 224, 205, 255],
        "saved brow"
    );
    assert_eq!(
        pixel_at(&frame, 195, 200),
        [120, 70, 40, 255],
        "hidden prop"
    );
    assert_eq!(pixel_at(&frame, 120, 220), [40, 60, 130, 255], "outfit");
}

#[test]
fn rasterize_psd_disabled_folders_hide_their_layers_except_enabled_ones() {
    let preset = ["body/base", "face/brows/normal", "face/eyes/open"].map(str::to_owned);
    // Hide the brows folder, then show one brow in it: a face swap on top
    // of whatever the preset (or saved visibility) shows.
    let frame = rasterize_psd(
        "fixture",
        &lipsync_fixture_psd(),
        &preset,
        &["face/brows/angry".to_owned()],
        &["face/brows".to_owned()],
    )
    .unwrap();
    // The normal brow alone covers y 94..99 and the angry one y 96..104.
    assert_eq!(pixel_at(&frame, 120, 95), [250, 224, 205, 255]);
    assert_eq!(pixel_at(&frame, 120, 102), [90, 60, 40, 255]);
    // Layers outside the folder are untouched.
    assert_eq!(pixel_at(&frame, 120, 110), [40, 40, 60, 255]);

    let error = rasterize_psd(
        "fixture",
        &lipsync_fixture_psd(),
        &preset,
        &[],
        &["face/brow".to_owned()],
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("face/brow"), "{error}");
}

/// The red channel along row 2 (or, `vertical`, column 2) of a 32x32 black
/// canvas holding a white rect `length` long and 2 units across, its start
/// at `at` with `anchor` along it, scaled by `along` and `across`.
fn rect_line(
    vertical: bool,
    at: f64,
    length: f64,
    anchor: f64,
    (along, across): (f64, f64),
) -> Vec<u8> {
    let swap = |along: f64, across: f64| {
        if vertical {
            Point {
                x: across,
                y: along,
            }
        } else {
            Point {
                x: along,
                y: across,
            }
        }
    };
    let size = swap(length, 2.0);
    let layer = Layer {
        transform: EvaluatedTransform {
            position: swap(at, 1.0),
            anchor: swap(anchor, 0.0),
            scale: swap(along, across),
            ..EvaluatedTransform::default()
        },
        content: LayerContent::Rect {
            width: size.x,
            height: size.y,
            fill: Some(Paint::Solid {
                color: "#FFFFFFFF".to_owned(),
            }),
            stroke: None,
            corner_radius: 0.0,
        },
        ..red_square()
    };
    let scene = Scene {
        width: 32,
        height: 32,
        ..clip_scene(vec![layer])
    };
    let frame = CpuRenderer::new(RenderOptions {
        background: Color::rgba(0, 0, 0, 255),
    })
    .render(&scene)
    .unwrap();
    (0..32)
        .map(|i| {
            let (x, y) = if vertical { (2, i) } else { (i, 2) };
            pixel(&frame, x, y)[0]
        })
        .collect()
}

#[test]
fn rects_keep_fractional_positions_sizes_anchors_and_scales() {
    /// A black line with `pixels` from pixel `first` on.
    fn expected(first: usize, pixels: &[u8]) -> Vec<u8> {
        let mut line = vec![0; 32];
        line[first..first + pixels.len()].copy_from_slice(pixels);
        line
    }
    let full = |count: usize| vec![255; count];
    let with = |start: u8, middle: Vec<u8>, end: u8| [vec![start], middle, vec![end]].concat();
    // Along x, then along y.
    let check = |at: f64, length: f64, anchor: f64, scale: (f64, f64), line: Vec<u8>| {
        for vertical in [false, true] {
            assert_eq!(
                rect_line(vertical, at, length, anchor, scale),
                line,
                "vertical: {vertical}, at {at}, length {length}, anchor {anchor}, scale {scale:?}"
            );
        }
    };

    // Half-pixel position: both edges half covered, not snapped.
    check(
        10.5,
        10.0,
        0.0,
        (1.0, 1.0),
        expected(10, &with(128, full(9), 128)),
    );
    // Centred on a fractional length: [14.75, 25.25], symmetric.
    check(
        20.0,
        10.5,
        0.5,
        (1.0, 1.0),
        expected(14, &with(64, full(10), 64)),
    );
    // Scaled up: 10.25 * 2 = 20.5 pixels, not 11 texels stretched to 22.
    check(
        4.0,
        10.25,
        0.0,
        (2.0, 2.0),
        expected(4, &[full(20), vec![128]].concat()),
    );
    // A fractional scale: 10 * 1.25 = 12.5 pixels.
    check(
        4.0,
        10.0,
        0.0,
        (1.25, 1.25),
        expected(4, &[full(12), vec![128]].concat()),
    );
    // A non-uniform scale keeps the edge along the stretched axis exact too.
    check(
        4.0,
        10.0,
        0.0,
        (1.25, 1.0),
        expected(4, &[full(12), vec![128]].concat()),
    );
    // A negative scale mirrors the rect about its position.
    check(
        20.5,
        10.0,
        0.0,
        (-1.0, 1.0),
        expected(10, &with(128, full(9), 128)),
    );
}

#[test]
fn rects_thinner_than_a_pixel_cover_only_their_width() {
    for vertical in [false, true] {
        let total = |at: f64, length: f64| -> u32 {
            rect_line(vertical, at, length, 0.0, (1.0, 1.0))
                .into_iter()
                .map(u32::from)
                .sum()
        };
        // Empty: nothing, even centred on a pixel.
        assert_eq!(total(16.5, 0.0), 0, "vertical: {vertical}");
        // A tenth of a pixel covers a tenth of one, on a pixel's centre or
        // across a pixel boundary alike.
        assert_eq!(total(16.45, 0.1), 26, "vertical: {vertical}");
        assert_eq!(total(15.95, 0.1), 26, "vertical: {vertical}");
    }
}

#[test]
fn rects_of_negative_size_draw_nothing() {
    for vertical in [false, true] {
        assert_eq!(
            rect_line(vertical, 16.5, -5.0, 0.0, (1.0, 1.0)),
            vec![0; 32],
            "vertical: {vertical}"
        );
    }
}

/// The straight RGBA pixels of a rect painted white (and stroked black at
/// `stroke_width`, if any) through `transform`, on a 32x16 canvas.
fn transformed_rect(
    width: f64,
    height: f64,
    stroke_width: Option<f64>,
    transform: crate::PathTransform,
) -> RgbaFrame {
    let stroke = stroke_width.map(|width| celesta_composition::Stroke {
        paint: Paint::Solid {
            color: "#000000".to_owned(),
        },
        width,
    });
    let paint = crate::rect::resolve_rect_paint(
        Some(&Paint::Solid {
            color: "#FFFFFF".to_owned(),
        }),
        stroke.as_ref(),
    )
    .unwrap();
    let rect =
        crate::rect::rasterize_rect_transformed(width, height, 0.0, &paint, transform, 32, 16)
            .unwrap();
    let mut frame = RgbaFrame {
        width: 32,
        height: 16,
        pixels: vec![0; 32 * 16 * 4],
    };
    for (index, texel) in rect.image.pixels().as_chunks::<4>().0.iter().enumerate() {
        let x = rect.left as u32 + index as u32 % rect.image.width();
        let y = rect.top as u32 + index as u32 / rect.image.width();
        let offset = ((y * 32 + x) * 4) as usize;
        frame.pixels[offset..offset + 4].copy_from_slice(texel);
    }
    frame
}

/// What a white rect `width` x `height` drawn through `transform` (its
/// translation then moved by each of ten sub-pixel offsets) covers, over its
/// area: the lowest and the highest.
fn coverage_over_offsets(width: f64, height: f64, transform: crate::PathTransform) -> (f64, f64) {
    let paint = crate::rect::resolve_rect_paint(
        Some(&Paint::Solid {
            color: "#FFFFFF".to_owned(),
        }),
        None,
    )
    .unwrap();
    let area = width * height * (transform.a * transform.d - transform.b * transform.c).abs();
    (0..10)
        .map(|step| {
            let offset = f64::from(step) / 10.0;
            let transform = crate::PathTransform {
                tx: transform.tx + offset,
                ty: transform.ty + offset * 0.37,
                ..transform
            };
            let rect = crate::rect::rasterize_rect_transformed(
                width, height, 0.0, &paint, transform, 200, 120,
            )
            .unwrap();
            let covered: f64 = rect
                .image
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| f64::from(pixel[3]) / 255.0)
                .sum();
            covered / area
        })
        .fold((f64::MAX, f64::MIN), |(low, high), ratio| {
            (low.min(ratio), high.max(ratio))
        })
}

/// Rotations and shears (`x' = a x + c y`, `y' = b x + d y`) that lean
/// edges off the pixel grid.
fn leaning_transforms() -> [(&'static str, crate::PathTransform); 4] {
    let rotation = |degrees: f64| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        crate::PathTransform {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            tx: 60.0,
            ty: 30.0,
        }
    };
    let shear = |c: f64| crate::PathTransform {
        a: 1.0,
        b: 0.0,
        c,
        d: 1.0,
        tx: 60.0,
        ty: 30.0,
    };
    [
        ("rotated 30", rotation(30.0)),
        ("rotated 45", rotation(45.0)),
        ("sheared 1", shear(1.0)),
        ("sheared 2", shear(2.0)),
    ]
}

#[test]
fn thin_strips_cover_their_area_wherever_they_fall() {
    // A single sample per pixel measured in Euclidean pixels drew a 0.1 px
    // strip at 45 degrees anywhere from 0.71 to 1.45 of its area depending on
    // where its edges fell between pixel centres, so a moving strip
    // flickered.
    for (name, transform) in leaning_transforms() {
        for width in [0.1, 0.25, 0.5, 1.0] {
            let (low, high) = coverage_over_offsets(width, 40.0, transform);
            assert!(
                low >= 0.97 && high <= 1.12,
                "{name}, width {width}: {low:.2} to {high:.2}"
            );
        }
    }
}

#[test]
fn boxes_within_a_pixel_stay_near_their_area_when_leaning() {
    // Smaller than a pixel in both directions, one sample cannot place a
    // box exactly, but it neither vanishes nor draws several times its area.
    for (name, transform) in leaning_transforms() {
        for (width, height) in [(0.2, 0.2), (0.5, 0.3)] {
            let (low, high) = coverage_over_offsets(width, height, transform);
            assert!(
                low >= 0.4 && high <= 1.8,
                "{name}, {width}x{height}: {low:.2} to {high:.2}"
            );
        }
    }
}

#[test]
fn a_thin_stroked_rect_mixes_by_the_covered_area() {
    // 0.1 of a pixel wide on pixel 16's centre, stroked 0.02 on each side:
    // of what it covers, 0.06 / 0.1 is fill.
    let frame = transformed_rect(
        0.1,
        10.0,
        Some(0.02),
        crate::PathTransform::scale_translate(1.0, 1.0, 16.45, 3.0),
    );
    assert_eq!(pixel(&frame, 16, 8), [153, 153, 153, 26]);
}

#[test]
fn a_stroke_filling_the_rect_hides_its_fill() {
    // The stroke reaches the centre from every side: no fill shows, not
    // even half of it along the centre line.
    let rect = rasterize_rect(
        5.0,
        5.0,
        0.0,
        Some(&Paint::Solid {
            color: "#FFFFFF".to_owned(),
        }),
        Some(&celesta_composition::Stroke {
            paint: Paint::Solid {
                color: "#FF0000".to_owned(),
            },
            width: 2.5,
        }),
    )
    .unwrap();
    for pixel in rect.pixels().as_chunks::<4>().0 {
        assert_eq!(*pixel, [255, 0, 0, 255]);
    }
}

#[test]
fn colored_runs_preserve_full_shaping_and_reveal_geometry() {
    use celesta_composition::TextColorRun;
    let mut rasterizer = TextRasterizer::new();
    let plain = TextStyle {
        font_size: Some(36.0),
        font_family: Some("serif".into()),
        ..Default::default()
    };
    for text in [
        "AV office e\u{301} 😀 אבג",
        "AV\r\noffice e\u{301}\n😀 אבג\n",
        "日本語の字幕を色分けする",
    ] {
        let count = text.chars().count();
        let colored = TextStyle {
            color_runs: (0..count)
                .map(|start| TextColorRun {
                    start,
                    end: start + 1,
                    color: if start % 2 == 0 { "#ff0000" } else { "#00ff00" }.into(),
                })
                .collect(),
            ..plain.clone()
        };
        assert_eq!(
            rasterizer.measure(text, &plain, Some(180.0)),
            rasterizer.measure(text, &colored, Some(180.0))
        );
        let original = rasterizer.rasterize(text, &plain, None, 1.0).unwrap();
        let full = rasterizer.rasterize(text, &colored, None, 1.0).unwrap();
        assert_eq!(
            original
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[3])
                .collect::<Vec<_>>(),
            full.pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[3])
                .collect::<Vec<_>>()
        );
        for visible in [0, 1, count / 2, count] {
            let partial = rasterizer
                .rasterize(
                    text,
                    &TextStyle {
                        visible_characters: Some(visible),
                        ..colored.clone()
                    },
                    None,
                    1.0,
                )
                .unwrap();
            assert_eq!(
                (
                    partial.width(),
                    partial.height(),
                    partial.baseline(),
                    partial.anchor_box
                ),
                (
                    full.width(),
                    full.height(),
                    full.baseline(),
                    full.anchor_box
                )
            );
            if visible == 0 {
                assert!(partial.pixels().iter().all(|&p| p == 0));
            } else if visible == count {
                assert_eq!(partial, full);
            }
        }
        let first = rasterizer.shaped_buffer(text, &plain, None, 1.0);
        let second = rasterizer.shaped_buffer(text, &colored, None, 1.0);
        assert!(
            std::sync::Arc::ptr_eq(&first, &second),
            "paint changes reuse the exact layout"
        );
    }
}

#[test]
fn colored_runs_validate_ranges_and_override_gradient_without_recoloring_white() {
    use celesta_composition::TextColorRun;
    let mut rasterizer = TextRasterizer::new();
    let white = TextColorRun {
        start: 0,
        end: 2,
        color: "#ffffff80".into(),
    };
    let solid = TextStyle {
        color_runs: vec![white.clone()],
        font_size: Some(40.0),
        ..Default::default()
    };
    let gradient = TextStyle {
        fill: Some(Paint::Linear {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 80.0, y: 0.0 },
            stops: vec![
                celesta_composition::GradientStop {
                    offset: 0.0,
                    color: "#ff0000".into(),
                },
                celesta_composition::GradientStop {
                    offset: 1.0,
                    color: "#0000ff".into(),
                },
            ],
        }),
        ..solid.clone()
    };
    assert_eq!(
        rasterizer.rasterize("AV", &solid, None, 1.0).unwrap(),
        rasterizer.rasterize("AV", &gradient, None, 1.0).unwrap()
    );
    for runs in [
        vec![TextColorRun {
            end: 3,
            ..white.clone()
        }],
        vec![white.clone(), white.clone()],
        vec![TextColorRun {
            start: 2,
            end: 1,
            ..white
        }],
    ] {
        assert!(matches!(
            rasterizer.rasterize(
                "AV",
                &TextStyle {
                    color_runs: runs,
                    ..solid.clone()
                },
                None,
                1.0
            ),
            Err(crate::RenderError::InvalidTextColorRun { .. })
        ));
    }
}

#[test]
fn glyph_offsets_refer_to_original_code_points_after_wrapping_and_crlf() {
    let mut rasterizer = TextRasterizer::new();
    let text = "😀 e\u{301}\r\nאבג\n日本語の字幕";
    for line_break in [LineBreak::Normal, LineBreak::Phrase] {
        let metrics = rasterizer.measure(
            text,
            &TextStyle {
                font_size: Some(32.0),
                line_break: Some(line_break),
                ..Default::default()
            },
            Some(120.0),
        );
        for glyph in metrics.glyphs {
            assert_eq!(
                text.chars()
                    .skip(glyph.start)
                    .take(glyph.end - glyph.start)
                    .collect::<String>(),
                glyph.text
            );
        }
    }
}

#[test]
fn color_runs_paint_each_cluster_and_reveal_the_expected_prefix() {
    use celesta_composition::TextColorRun;
    let mut rasterizer = TextRasterizer::new();
    let text = "A B C D";
    let style = TextStyle {
        font_family: Some("monospace".into()),
        font_size: Some(40.0),
        color_runs: text
            .chars()
            .enumerate()
            .map(|(start, _)| TextColorRun {
                start,
                end: start + 1,
                color: if start % 4 == 0 { "#ff0000" } else { "#00ff00" }.into(),
            })
            .collect(),
        ..Default::default()
    };
    let full = rasterizer.rasterize(text, &style, None, 1.0).unwrap();
    let metrics = rasterizer.measure(text, &style, None);
    for glyph in metrics.glyphs.iter().filter(|glyph| glyph.text != " ") {
        let expected = if glyph.start % 4 == 0 {
            [255, 0, 0]
        } else {
            [0, 255, 0]
        };
        let pixels: Vec<_> = full
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                let x = (i % full.width() as usize) as f64;
                x >= glyph.x && x < glyph.x + glyph.width && p[3] > 0
            })
            .map(|(_, p)| p)
            .collect();
        assert!(!pixels.is_empty(), "missing glyph {}", glyph.text);
        assert!(
            pixels.iter().all(|p| p[..3] == expected),
            "wrong color for {}",
            glyph.text
        );
    }
    for visible in [1, 3, 5, text.chars().count()] {
        let partial = rasterizer
            .rasterize(
                text,
                &TextStyle {
                    visible_characters: Some(visible),
                    ..style.clone()
                },
                None,
                1.0,
            )
            .unwrap();
        let mut expected = style.clone();
        for run in &mut expected.color_runs {
            if run.start >= visible {
                run.color = "#00000000".into();
            }
        }
        assert_eq!(
            partial,
            rasterizer.rasterize(text, &expected, None, 1.0).unwrap(),
            "visible={visible}"
        );
    }
}

#[test]
fn oversized_text_does_not_evict_cached_layouts() {
    let mut rasterizer = TextRasterizer::new();
    let style = TextStyle::default();
    let first = rasterizer.shaped_buffer("cached 0", &style, None, 1.0);
    for i in 1..32 {
        rasterizer.shaped_buffer(&format!("cached {i}"), &style, None, 1.0);
    }
    rasterizer.shaped_buffer(&" ".repeat(100_001), &style, None, 1.0);
    assert_eq!(rasterizer.shaped_buffers.len(), 32);
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &rasterizer.shaped_buffer("cached 0", &style, None, 1.0)
    ));
}

#[test]
fn metrics_retain_empty_line_source_offsets() {
    let mut rasterizer = TextRasterizer::new();
    let metrics = rasterizer.measure("A\r\n\r\nB\n\n", &TextStyle::default(), None);
    assert_eq!(metrics.line_starts, [0, 3, 5, 7, 8]);
}

#[test]
fn refuses_custom_shaders() {
    use celesta_composition::{LayerFilter, LayerShader};

    let mut layer = Layer {
        id: "shaded".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Rect {
            width: 8.0,
            height: 8.0,
            fill: Some(Paint::Solid {
                color: "#ffffff".to_owned(),
            }),
            stroke: None,
            corner_radius: 0.0,
        },
    };
    layer.effects.filters.push(LayerFilter::Shader(LayerShader {
        id: "identity".to_owned(),
        params: Vec::new(),
        padding: 0.0,
    }));
    let scene = Scene {
        width: 16,
        height: 16,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        shaders: Vec::new(),
        layers: vec![layer],
    };
    let error = CpuRenderer::default().render(&scene).unwrap_err();
    assert!(
        matches!(&error, RenderError::UnsupportedShader { layer } if layer == "shaded"),
        "{error}"
    );

    // Also on a layer that would draw nothing.
    let mut invisible = scene;
    invisible.layers[0].opacity = 0.0;
    let error = CpuRenderer::default().render(&invisible).unwrap_err();
    assert!(
        matches!(error, RenderError::UnsupportedShader { .. }),
        "{error}"
    );
}

fn instance_layer(id: &str, content: LayerContent) -> Layer {
    Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform {
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content,
    }
}

/// Three particles: a shared width, per-particle heights and colours.
fn three_particles(shape: InstanceShape) -> LayerContent {
    LayerContent::Instances {
        count: 3,
        shape: Box::new(shape),
        width: Some(10.0),
        height: None,
        color: None,
        x: F32Column::from(vec![8.0, 20.5, 30.0]),
        y: F32Column::from(vec![10.0, 21.0, 28.25]),
        widths: None,
        heights: Some(F32Column::from(vec![6.0, 8.0, 10.0])),
        rotation: None,
        colors: Some(U8Column::from(vec![
            255, 0, 0, 200, 0, 255, 0, 200, 0, 0, 255, 200,
        ])),
    }
}

/// The `Rect` layers `three_particles` stands for.
fn three_rects(corner_radius: impl Fn(f64, f64) -> f64) -> Vec<Layer> {
    [
        (8.0, 10.0, 6.0, "#FF0000C8"),
        (20.5, 21.0, 8.0, "#00FF00C8"),
        (30.0, 28.25, 10.0, "#0000FFC8"),
    ]
    .into_iter()
    .map(|(x, y, height, color)| Layer {
        id: color.to_owned(),
        transform: EvaluatedTransform {
            position: Point { x, y },
            anchor: Point { x: 0.5, y: 0.5 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Rect {
            width: 10.0,
            height,
            fill: Some(Paint::Solid {
                color: color.to_owned(),
            }),
            stroke: None,
            corner_radius: corner_radius(10.0, height),
        },
    })
    .collect()
}

#[test]
fn instances_draw_like_centred_rects() {
    let render = |layers| CpuRenderer::default().render(&clip_scene(layers)).unwrap();
    let rects = render(vec![instance_layer(
        "rects",
        three_particles(InstanceShape::Rect { corner_radius: 2.0 }),
    )]);
    assert_eq!(rects.pixels(), render(three_rects(|_, _| 2.0)).pixels());
    let circles = render(vec![instance_layer(
        "circles",
        three_particles(InstanceShape::Circle),
    )]);
    assert_eq!(
        circles.pixels(),
        render(three_rects(|width: f64, height| width.min(height) / 2.0)).pixels()
    );
}

#[test]
fn instances_with_no_particles_draw_nothing() {
    let empty = LayerContent::Instances {
        count: 0,
        shape: Box::new(InstanceShape::Circle),
        width: Some(4.0),
        height: Some(4.0),
        color: None,
        x: F32Column::from(Vec::new()),
        y: F32Column::from(Vec::new()),
        widths: None,
        heights: None,
        rotation: None,
        colors: None,
    };
    let frame = CpuRenderer::default()
        .render(&clip_scene(vec![instance_layer("empty", empty)]))
        .unwrap();
    let blank = CpuRenderer::default()
        .render(&clip_scene(Vec::new()))
        .unwrap();
    assert_eq!(frame.pixels(), blank.pixels());
}

#[test]
fn instances_reject_rotated_particles() {
    let mut content = three_particles(InstanceShape::Circle);
    if let LayerContent::Instances { rotation, .. } = &mut content {
        *rotation = Some(F32Column::from(vec![0.0, 15.0, 0.0]));
    }
    let error = CpuRenderer::default()
        .render(&clip_scene(vec![instance_layer("spun", content)]))
        .unwrap_err();
    assert!(
        matches!(&error, RenderError::UnsupportedRotation { layer, degrees } if layer == "spun" && *degrees == 15.0),
        "{error}"
    );
}

#[test]
fn instances_report_a_fill_that_splits_a_character_instead_of_panicking() {
    let mut content = three_particles(InstanceShape::Circle);
    if let LayerContent::Instances { color, colors, .. } = &mut content {
        // Six bytes after `#`, the second inside `é`.
        *color = Some("#aéaaa".to_owned());
        *colors = None;
    }
    let error = CpuRenderer::default()
        .render(&clip_scene(vec![instance_layer("dots", content)]))
        .unwrap_err();
    assert!(
        matches!(&error, RenderError::InvalidColor(color) if color == "#aéaaa"),
        "{error}"
    );
}

/// The 2x2 checker (red, green, blue, white) drawn as one particle.
fn checker_particle(color: [u8; 4]) -> Scene {
    Scene {
        width: 2,
        height: 2,
        frame_rate: Rational::new(60, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![instance_layer(
            "checker",
            LayerContent::Instances {
                count: 1,
                shape: Box::new(InstanceShape::Image {
                    asset: ResolvedAsset {
                        id: "checker".to_owned(),
                        location: AssetLocation::File {
                            path: "tests/assets/checker.ppm".to_owned(),
                        },
                    },
                }),
                width: None,
                height: None,
                color: None,
                x: F32Column::from(vec![1.0]),
                y: F32Column::from(vec![1.0]),
                widths: None,
                heights: None,
                rotation: None,
                colors: Some(U8Column::from(color.to_vec())),
            },
        )],
        shaders: Vec::new(),
    }
}

#[test]
fn instance_images_multiply_their_colour() {
    let render = |scene: &Scene| {
        CpuRenderer::default()
            .with_asset_root(env!("CARGO_MANIFEST_DIR"))
            .render(scene)
            .unwrap()
    };
    assert_eq!(
        render(&checker_particle([255, 255, 255, 255])).pixels(),
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ]
    );
    assert_eq!(
        render(&checker_particle([255, 0, 0, 255])).pixels(),
        &[255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 0, 0, 255]
    );
    // Half the colour's alpha draws like the image at half opacity.
    let mut faded = checker_particle([255, 255, 255, 255]);
    faded.layers[0].opacity = 128.0 / 255.0;
    assert_eq!(
        render(&checker_particle([255, 255, 255, 128])).pixels(),
        render(&faded).pixels()
    );
}

#[test]
fn blended_instances_match_a_blended_group() {
    let backdrop = || Layer {
        content: LayerContent::Rect {
            width: 40.0,
            height: 40.0,
            fill: Some(Paint::Solid {
                color: "#336699FF".to_owned(),
            }),
            stroke: None,
            corner_radius: 0.0,
        },
        ..red_square()
    };
    let instances = Layer {
        blend_mode: BlendMode::Screen,
        ..instance_layer("screen", three_particles(InstanceShape::Circle))
    };
    let group = Layer {
        blend_mode: BlendMode::Screen,
        ..instance_layer(
            "group",
            LayerContent::Group {
                layers: three_rects(|width: f64, height| width.min(height) / 2.0),
                clip: None,
                mask: None,
                motion_blur: false,
            },
        )
    };
    let render = |layers| CpuRenderer::default().render(&clip_scene(layers)).unwrap();
    assert_eq!(
        render(vec![backdrop(), instances]).pixels(),
        render(vec![backdrop(), group]).pixels()
    );
}

#[test]
fn one_instance_image_dimension_keeps_its_aspect_ratio() {
    let mut renderer = CpuRenderer::new(RenderOptions {
        background: Color::rgba(0, 0, 0, 255),
    });
    renderer.image_sources.insert_raster(
        "wide",
        image::RgbaImage::from_raw(4, 2, [255; 4].repeat(8)).unwrap(),
    );
    let scene = Scene {
        width: 16,
        height: 16,
        layers: vec![instance_layer(
            "wide",
            LayerContent::Instances {
                count: 1,
                shape: Box::new(InstanceShape::Image {
                    asset: ResolvedAsset {
                        id: "wide".to_owned(),
                        location: AssetLocation::File {
                            path: "wide.png".to_owned(),
                        },
                    },
                }),
                width: Some(8.0),
                height: None,
                color: None,
                x: F32Column::from(vec![8.0]),
                y: F32Column::from(vec![8.0]),
                widths: None,
                heights: None,
                rotation: None,
                colors: None,
            },
        )],
        ..clip_scene(Vec::new())
    };
    let frame = renderer.render(&scene).unwrap();
    // 8 wide, so 4 tall: rows 6 to 9.
    assert_eq!(pixel(&frame, 8, 6), [255, 255, 255, 255]);
    assert_eq!(pixel(&frame, 8, 9), [255, 255, 255, 255]);
    assert_eq!(pixel(&frame, 8, 5), [0, 0, 0, 255]);
    assert_eq!(pixel(&frame, 8, 10), [0, 0, 0, 255]);
}

#[test]
fn cpu_refuses_a_static_ref() {
    let error = CpuRenderer::default()
        .render(&clip_scene(vec![instance_layer(
            "s",
            LayerContent::StaticRef { key: 3 },
        )]))
        .unwrap_err();
    assert!(
        matches!(&error, RenderError::UnresolvedStatic { layer } if layer == "s"),
        "{error}"
    );
}

fn transformed_square(transform: EvaluatedTransform) -> Scene {
    let mut layer = red_square();
    layer.id = "tilted".to_owned();
    layer.transform = transform;
    clip_scene(vec![layer])
}

#[test]
fn refuses_skew_and_3d_rotation() {
    let cases = [
        (
            "skewX",
            EvaluatedTransform {
                skew: Point { x: 5.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
        ),
        (
            "skewY",
            EvaluatedTransform {
                skew: Point { x: 0.0, y: 5.0 },
                ..EvaluatedTransform::default()
            },
        ),
        (
            "rotationX",
            EvaluatedTransform {
                rotation_x: 5.0,
                ..EvaluatedTransform::default()
            },
        ),
        (
            "rotationY",
            EvaluatedTransform {
                rotation_y: 5.0,
                ..EvaluatedTransform::default()
            },
        ),
    ];
    for (property, transform) in cases {
        let error = CpuRenderer::default()
            .render(&transformed_square(transform))
            .unwrap_err();
        assert!(
            matches!(
                &error,
                RenderError::UnsupportedTransform { layer, property: refused }
                    if layer == "tilted" && *refused == property
            ),
            "{error}"
        );
    }
}

#[test]
fn perspective_and_hidden_backface_alone_render_like_a_plain_rect() {
    let plain = CpuRenderer::default()
        .render(&clip_scene(vec![red_square()]))
        .unwrap();

    let mut layer = red_square();
    layer.transform.perspective = Some(500.0);
    layer.transform.backface_visibility = celesta_composition::BackfaceVisibility::Hidden;
    let flat = CpuRenderer::default()
        .render(&clip_scene(vec![layer]))
        .unwrap();

    assert!(is_red(&flat, 20, 20));
    assert_eq!(plain.pixels(), flat.pixels());
}

/// Thirty lines of text 300 wide, stroked, in `fill`.
fn windowed_style(fill: Paint) -> TextStyle {
    TextStyle {
        font_size: Some(24.0),
        fill: Some(fill),
        stroke: Some(
            celesta_composition::Stroke {
                paint: Paint::Solid {
                    color: "#FF0000FF".to_owned(),
                },
                width: 2.0,
            }
            .into(),
        ),
        ..TextStyle::default()
    }
}

const WINDOWED_TEXT: &str = "Window HHHH Window HHHH";

/// Asserts that `window`'s pixels are the whole raster's where the two
/// place the same layout point, and returns that offset.
fn assert_window_matches_whole(fill: Paint) -> (i64, i64) {
    let text = vec![WINDOWED_TEXT; 30].join("\n");
    let style = windowed_style(fill);
    let mut rasterizer = TextRasterizer::new();
    let whole = rasterizer
        .rasterize(&text, &style, Some(300.0), 1.5)
        .unwrap();
    let window = rasterizer
        .rasterize_window(
            &text,
            &style,
            Some(300.0),
            1.5,
            Some([40.0, 200.0, 260.0, 420.0]),
        )
        .unwrap();
    assert!(window.width() < whole.width() && window.height() < whole.height() / 2);
    assert!(
        (window.width() as f64 - 220.0 * 1.5).abs() <= 2.0,
        "{}",
        window.width()
    );
    // The layout box's top-left, in each image's pixels.
    let ox = i64::from(whole.anchor_box.left) - i64::from(window.anchor_box.left);
    let oy = i64::from(whole.anchor_box.top) - i64::from(window.anchor_box.top);
    assert_eq!(whole.baseline() - window.baseline(), oy as f32);
    let pixel = |image: &crate::RasterizedText, x: i64, y: i64| {
        let offset = (y as usize * image.width() as usize + x as usize) * 4;
        image.pixels()[offset..offset + 4].to_vec()
    };
    let mut inked = 0;
    for y in 0..i64::from(window.height()) {
        for x in 0..i64::from(window.width()) {
            let windowed = pixel(&window, x, y);
            assert_eq!(windowed, pixel(&whole, x + ox, y + oy), "at ({x}, {y})");
            inked += usize::from(windowed[3] > 0);
        }
    }
    assert!(inked > 1000, "{inked}");
    (ox, oy)
}

#[test]
fn a_text_window_is_the_same_pixels_as_the_whole_raster() {
    let (ox, oy) = assert_window_matches_whole(Paint::Solid {
        color: "#FFFFFFFF".to_owned(),
    });
    assert!(ox > 0 && oy > 0, "({ox}, {oy})");
}

#[test]
fn a_window_keeps_gradient_fills_in_place() {
    assert_window_matches_whole(Paint::Linear {
        start: Point { x: 0.0, y: 0.0 },
        end: Point { x: 300.0, y: 700.0 },
        stops: vec![stop(0.0, "#ff0000"), stop(1.0, "#0000ff")],
    });
}

#[test]
fn single_line_text_ignores_the_window() {
    let style = windowed_style(Paint::Solid {
        color: "#FFFFFFFF".to_owned(),
    });
    let mut rasterizer = TextRasterizer::new();
    let whole = rasterizer
        .rasterize(WINDOWED_TEXT, &style, None, 1.5)
        .unwrap();
    let window = rasterizer
        .rasterize_window(
            WINDOWED_TEXT,
            &style,
            None,
            1.5,
            Some([40.0, 0.0, 100.0, 10.0]),
        )
        .unwrap();
    assert_eq!(window, whole);
}
