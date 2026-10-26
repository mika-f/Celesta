//! Skew and 3D rotation: the affine path of the GPU renderer, and the
//! projective planes perspective layers draw through.

use super::*;
use crate::bounds::PixelBounds;
use crate::plan::content_bounds;
use celesta_composition::{BackfaceVisibility, multiply3, project};

pub(super) fn transformed(mut layer: Layer, edit: impl FnOnce(&mut EvaluatedTransform)) -> Layer {
    edit(&mut layer.transform);
    layer
}

pub(super) fn transparent_renderer() -> Option<TestRenderer> {
    renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    })
}

fn scene_of(layers: Vec<Layer>) -> Scene {
    let mut scene = empty_scene(64, 64);
    scene.layers = layers;
    scene
}

#[test]
fn skews_rects_by_the_tangent() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let rect = transformed(
        corner_rect("rect", 10.0, 10.0, 20.0, 20.0, "#FF0000FF"),
        |transform| transform.skew.x = 45.0,
    );
    let frame = renderer.render(&scene_of(vec![rect])).unwrap();
    // A row at y = 25 is 15 below the anchor, so the rect shifts right 15.
    assert_eq!(pixel_at(&frame, 35, 25), [255, 0, 0, 255]);
    assert_eq!(pixel_at(&frame, 22, 25)[3], 0);
    assert_eq!(pixel_at(&frame, 47, 25)[3], 0);
}

#[test]
fn orthographic_rotation_y_matches_scale_x() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let rotated = transformed(
        corner_rect("rotated", 10.0, 10.0, 40.0, 30.0, "#FF0000FF"),
        |transform| transform.rotation_y = 60.0,
    );
    let scaled = transformed(
        corner_rect("scaled", 10.0, 10.0, 40.0, 30.0, "#FF0000FF"),
        |transform| transform.scale.x = 0.5,
    );
    let rotated = renderer.render(&scene_of(vec![rotated])).unwrap();
    let scaled = renderer.render(&scene_of(vec![scaled])).unwrap();
    assert_eq!(pixel_at(&scaled, 20, 20), [255, 0, 0, 255]);
    let difference = rotated
        .pixels()
        .iter()
        .zip(scaled.pixels())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert_eq!(difference, 0);
}

#[test]
fn hides_backfaces_through_parents() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let hidden = |mut layer: Layer| {
        layer.transform.backface_visibility = BackfaceVisibility::Hidden;
        layer
    };
    let scene = |group_rotation_y: f64| {
        let red = hidden(corner_rect("a", -10.0, -10.0, 20.0, 20.0, "#FF0000FF"));
        let blue = corner_rect("b", -10.0, -10.0, 20.0, 20.0, "#0000FFFF");
        let inner = hidden(group(
            EvaluatedTransform {
                anchor: Point { x: 0.0, y: 0.0 },
                rotation_y: 180.0,
                ..EvaluatedTransform::default()
            },
            vec![blue],
        ));
        scene_of(vec![group(
            EvaluatedTransform {
                position: Point { x: 32.0, y: 32.0 },
                anchor: Point { x: 0.0, y: 0.0 },
                rotation_y: group_rotation_y,
                ..EvaluatedTransform::default()
            },
            vec![red, inner],
        )])
    };
    // 120° turns the red rect's back to the viewer, and 180° more turns the
    // inner group (and so the blue rect) round to the front again.
    let frame = renderer.render(&scene(120.0)).unwrap();
    assert_eq!(pixel_at(&frame, 32, 32), [0, 0, 255, 255]);
    let frame = renderer.render(&scene(30.0)).unwrap();
    assert_eq!(pixel_at(&frame, 32, 32), [255, 0, 0, 255]);
}

#[test]
fn edge_on_layers_draw_nothing() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let cases: [(&str, f64, Option<f64>, f64); 3] = [
        ("rotation y", 90.0, None, 0.0),
        ("rotation y with perspective", 90.0, Some(400.0), 0.0),
        ("skew", 0.0, None, 90.0),
    ];
    for (case, rotation_y, perspective, skew_x) in cases {
        let rect = transformed(
            corner_rect("rect", 10.0, 10.0, 40.0, 40.0, "#FF0000FF"),
            |transform| {
                transform.rotation_y = rotation_y;
                transform.perspective = perspective;
                transform.skew.x = skew_x;
            },
        );
        let frame = renderer.render(&scene_of(vec![rect])).unwrap();
        assert!(
            frame.pixels().iter().all(|channel| *channel == 0),
            "{case} drew something"
        );
    }
}

#[test]
fn a_group_moved_far_away_still_draws_children_moved_back() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    // Only the linear entries are capped: a far translation is not a
    // stretch, and this one cancels out.
    let rect = corner_rect("rect", -2e7 + 10.0, 10.0, 40.0, 40.0, "#FF0000FF");
    let far = group_at(2e7, 0.0, vec![rect]);
    let frame = renderer.render(&scene_of(vec![far])).unwrap();
    assert_eq!(pixel_at(&frame, 32, 32), [255, 0, 0, 255]);
}

/// A `width`x`height` rect of `color` centred on `(x, y)`.
fn centred_rect(x: f64, y: f64, width: f64, height: f64, color: &str) -> Layer {
    let mut rect = corner_rect("rect", x, y, width, height, color);
    rect.transform.anchor = Point { x: 0.5, y: 0.5 };
    rect
}

/// `layer` turned by `rotation_x` and `rotation_y` under `perspective`.
pub(super) fn in_perspective(
    layer: Layer,
    rotation_x: f64,
    rotation_y: f64,
    perspective: f64,
) -> Layer {
    transformed(layer, |transform| {
        transform.rotation_x = rotation_x;
        transform.rotation_y = rotation_y;
        transform.perspective = Some(perspective);
    })
}

/// A group at `(x, y)` turning about its origin.
pub(super) fn group_at(x: f64, y: f64, layers: Vec<Layer>) -> Layer {
    group(
        EvaluatedTransform {
            position: Point { x, y },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        layers,
    )
}

pub(super) fn sized_scene(width: u32, height: u32, layers: Vec<Layer>) -> Scene {
    let mut scene = empty_scene(width, height);
    scene.layers = layers;
    scene
}

const RED: [u8; 4] = [255, 0, 0, 255];

#[test]
fn projects_a_rect_through_its_perspective() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let rect = in_perspective(
        centred_rect(100.0, 100.0, 100.0, 100.0, "#FF0000FF"),
        0.0,
        45.0,
        300.0,
    );
    let homography = rect.transform.homography();
    let corners = [(-50.0, -50.0), (50.0, -50.0), (50.0, 50.0), (-50.0, 50.0)].map(|(x, y)| {
        let [x, y, _] = project(&homography, x, y);
        (x, y)
    });
    let frame = renderer.render(&sized_scene(200, 200, vec![rect])).unwrap();
    let centre = (
        corners.iter().map(|(x, _)| x).sum::<f64>() / 4.0,
        corners.iter().map(|(_, y)| y).sum::<f64>() / 4.0,
    );
    for index in 0..4 {
        let (from, to) = (corners[index], corners[(index + 1) % 4]);
        let middle = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
        let length = (to.0 - from.0).hypot(to.1 - from.1);
        let mut normal = (-(to.1 - from.1) / length, (to.0 - from.0) / length);
        if normal.0 * (centre.0 - middle.0) + normal.1 * (centre.1 - middle.1) < 0.0 {
            normal = (-normal.0, -normal.1);
        }
        let at = |distance: f64| {
            let x = middle.0 + normal.0 * distance;
            let y = middle.1 + normal.1 * distance;
            pixel_at(&frame, x.floor() as u32, y.floor() as u32)
        };
        assert_eq!(at(3.0), RED, "inside edge {index} at {middle:?}");
        assert_eq!(at(-3.0)[3], 0, "outside edge {index} at {middle:?}");
    }
}

#[test]
fn projective_text_stays_sharp() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let scene = |rotation_x: f64| {
        let mut text = text_layer("h", Point { x: 0.0, y: 0.0 }, 1.0, 120.0);
        text.transform.anchor = Point { x: 0.5, y: 0.5 };
        if let LayerContent::Text { text, .. } = &mut text.content {
            *text = "H".to_owned();
        }
        let mut group = group_at(100.0, 100.0, vec![text]);
        if rotation_x != 0.0 {
            group = in_perspective(group, rotation_x, 0.0, 800.0);
        }
        sized_scene(200, 200, vec![group])
    };
    // The largest alpha step between neighbours on the rows through the
    // stems, above the crossbar.
    let sharpness = |frame: &GpuFrame| {
        (62..78)
            .flat_map(|y| (1..frame.width()).map(move |x| (x, y)))
            .map(|(x, y)| pixel_at(frame, x, y)[3].abs_diff(pixel_at(frame, x - 1, y)[3]))
            .max()
            .unwrap()
    };
    let flat = sharpness(&renderer.render(&scene(0.0)).unwrap());
    let projected = sharpness(&renderer.render(&scene(30.0)).unwrap());
    assert!(flat > 128, "the flat H has sharp stems: {flat}");
    assert!(
        f64::from(projected) >= 0.75 * f64::from(flat),
        "projected {projected} vs flat {flat}"
    );
}

#[test]
fn flips_a_card() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let hidden = |mut layer: Layer| {
        layer.transform.backface_visibility = BackfaceVisibility::Hidden;
        layer
    };
    let card = |rotation_y: f64| {
        let front = hidden(corner_rect(
            "front",
            -60.0,
            -80.0,
            120.0,
            160.0,
            "#F4EFE6FF",
        ));
        let back = hidden(transformed(
            group_at(
                0.0,
                0.0,
                vec![corner_rect("back", -60.0, -80.0, 120.0, 160.0, "#1D3557FF")],
            ),
            |transform| transform.rotation_y = 180.0,
        ));
        let card = in_perspective(
            group_at(200.0, 200.0, vec![front, back]),
            0.0,
            rotation_y,
            600.0,
        );
        sized_scene(400, 400, vec![card])
    };
    let frame = renderer.render(&card(30.0)).unwrap();
    assert_eq!(pixel_at(&frame, 200, 200), [0xF4, 0xEF, 0xE6, 255]);
    let frame = renderer.render(&card(150.0)).unwrap();
    assert_eq!(pixel_at(&frame, 200, 200), [0x1D, 0x35, 0x57, 255]);
    let frame = renderer.render(&card(90.0)).unwrap();
    assert_eq!(pixel_at(&frame, 190, 200)[3], 0, "edge-on");
}

/// A group turned about its `origin` draws as the nested groups it stands
/// for: one at `position + origin` that turns, holding one moved back.
#[test]
fn turns_a_group_about_its_origin() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let content = || {
        vec![
            corner_rect("a", 40.0, 50.0, 120.0, 100.0, "#FF0000FF"),
            corner_rect("b", 90.0, 70.0, 40.0, 40.0, "#0000FFFF"),
        ]
    };
    type Turn = fn(&mut EvaluatedTransform);
    let cases: [(&str, Turn); 2] = [
        ("perspective", |transform| {
            transform.rotation_y = 50.0;
            transform.rotation_x = -20.0;
            transform.perspective = Some(400.0);
        }),
        ("flat", |transform| {
            transform.rotation = 30.0;
            transform.scale = Point { x: 0.5, y: 1.5 };
        }),
    ];
    for (case, turn) in cases {
        let about_origin = transformed(group_at(10.0, -5.0, content()), |transform| {
            transform.origin = Point { x: 110.0, y: 90.0 };
            turn(transform);
        });
        let nested = transformed(
            group_at(120.0, 85.0, vec![group_at(-110.0, -90.0, content())]),
            turn,
        );
        let about_origin = renderer
            .render(&sized_scene(200, 200, vec![about_origin]))
            .unwrap();
        let nested = renderer
            .render(&sized_scene(200, 200, vec![nested]))
            .unwrap();
        // The blue rect around the origin stays put as the group turns.
        assert_eq!(pixel_at(&about_origin, 120, 85), [0, 0, 255, 255], "{case}");
        // The two homographies are the same up to rounding, which can move
        // a projected edge's coverage by a level on some GPUs (lavapipe).
        let difference = max_difference(&about_origin, &nested, 0..200);
        assert!(difference <= 1, "{case}: {difference}");
    }
}

#[test]
fn nests_projective_groups() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let rect = in_perspective(
        centred_rect(0.0, 0.0, 100.0, 100.0, "#FF0000FF"),
        0.0,
        30.0,
        600.0,
    );
    let outer = in_perspective(group_at(100.0, 100.0, vec![rect]), 20.0, 0.0, 800.0);
    let frame = renderer
        .render(&sized_scene(200, 200, vec![outer]))
        .unwrap();
    assert_eq!(pixel_at(&frame, 100, 100), RED);
    // Both halves of the rect are drawn: the nested plane covers its whole
    // content, left of its origin too.
    assert_eq!(pixel_at(&frame, 70, 100), RED);
    assert_eq!(pixel_at(&frame, 130, 100), RED);
}

#[test]
fn clips_inside_and_outside_a_projective_group() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let rect = || {
        in_perspective(
            centred_rect(100.0, 100.0, 100.0, 100.0, "#FF0000FF"),
            0.0,
            30.0,
            600.0,
        )
    };
    let left_half = Clip {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 200.0,
        corner_radius: 0.0,
    };
    let outside = clipped_group(EvaluatedTransform::default(), left_half, vec![rect()]);
    let frame = renderer
        .render(&sized_scene(200, 200, vec![outside]))
        .unwrap();
    assert_eq!(pixel_at(&frame, 80, 100), RED);
    assert_eq!(
        pixel_at(&frame, 120, 100)[3],
        0,
        "outside the parent's clip"
    );

    let inside = in_perspective(
        clipped_group(
            EvaluatedTransform {
                position: Point { x: 100.0, y: 100.0 },
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            Clip {
                x: -50.0,
                y: -50.0,
                width: 50.0,
                height: 100.0,
                corner_radius: 0.0,
            },
            vec![centred_rect(0.0, 0.0, 100.0, 100.0, "#FF0000FF")],
        ),
        0.0,
        30.0,
        600.0,
    );
    let frame = renderer
        .render(&sized_scene(200, 200, vec![inside]))
        .unwrap();
    assert_eq!(pixel_at(&frame, 80, 100), RED);
    assert_eq!(
        pixel_at(&frame, 120, 100)[3],
        0,
        "outside the group's own clip"
    );
}

#[test]
fn draws_only_what_is_in_front_of_the_viewer() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let rect = in_perspective(
        centred_rect(100.0, 100.0, 1000.0, 100.0, "#FF0000FF"),
        0.0,
        80.0,
        200.0,
    );
    let frame = renderer.render(&sized_scene(200, 200, vec![rect])).unwrap();
    assert!(
        frame.pixels().as_chunks::<4>().0.contains(&RED),
        "something of the rect is drawn"
    );
}

#[test]
fn caps_huge_planes_at_the_texture_limit() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let rect = in_perspective(
        centred_rect(100.0, 100.0, 50_000.0, 50_000.0, "#FF0000FF"),
        60.0,
        0.0,
        1000.0,
    );
    renderer.render(&sized_scene(200, 200, vec![rect])).unwrap();
}

#[test]
fn blends_a_projective_layer_with_its_mode() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let background = corner_rect("background", 0.0, 0.0, 200.0, 200.0, "#FF0000FF");
    let mut white = in_perspective(
        centred_rect(100.0, 100.0, 100.0, 100.0, "#FFFFFFFF"),
        0.0,
        30.0,
        600.0,
    );
    white.blend_mode = BlendMode::Difference;
    let frame = renderer
        .render(&sized_scene(200, 200, vec![background, white]))
        .unwrap();
    assert_eq!(pixel_at(&frame, 100, 100), [0, 255, 255, 255]);
}

#[test]
fn decodes_a_projective_video_once_per_frame() {
    struct Decoder(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl VideoFrameDecoder for Decoder {
        fn decode_frame(&mut self, _: &Path, _: f64) -> Result<VideoFrame, MediaError> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(VideoFrame {
                width: 4,
                height: 4,
                pixels: [255, 0, 0, 255].repeat(16).into(),
            })
        }
    }

    let Some(renderer) = transparent_renderer() else {
        return;
    };
    let decodes = std::sync::Arc::default();
    let mut renderer = renderer.with_video_decoder(Decoder(std::sync::Arc::clone(&decodes)));
    let video = Layer {
        id: "video".to_owned(),
        transform: EvaluatedTransform {
            position: Point { x: 100.0, y: 100.0 },
            scale: Point { x: 25.0, y: 25.0 },
            rotation_y: 30.0,
            perspective: Some(600.0),
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
            width: None,
            height: None,
        },
    };
    let frame = renderer
        .render(&sized_scene(200, 200, vec![video]))
        .unwrap();
    assert_eq!(pixel_at(&frame, 100, 100), RED);
    assert_eq!(decodes.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn draws_nothing_closer_than_the_near_clip() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    // Turned about two axes, the near clip crosses the rect at a slant, so
    // the box of the part in front also holds some of what is too close.
    let mut rect = in_perspective(
        centred_rect(100.0, 100.0, 400.0, 40.0, "#FF0000FF"),
        0.0,
        80.0,
        20.0,
    );
    rect.transform.rotation = 30.0;
    let homography = rect.transform.homography();
    let in_front = crate::plane::clip_in_front(
        &homography,
        &[
            [-200.0, -20.0],
            [200.0, -20.0],
            [200.0, 20.0],
            [-200.0, 20.0],
        ],
    );
    let projected: Vec<[f64; 3]> = in_front
        .iter()
        .map(|&[x, y]| project(&homography, x, y))
        .collect();
    let bound = |pick: fn(&[f64; 3]) -> f64, fold: fn(f64, f64) -> f64, start: f64| {
        projected.iter().map(pick).fold(start, fold)
    };
    let left = bound(|p| p[0], f64::min, f64::INFINITY) - 2.0;
    let top = bound(|p| p[1], f64::min, f64::INFINITY) - 2.0;
    let right = bound(|p| p[0], f64::max, f64::NEG_INFINITY) + 2.0;
    let bottom = bound(|p| p[1], f64::max, f64::NEG_INFINITY) + 2.0;
    let frame = renderer.render(&sized_scene(200, 200, vec![rect])).unwrap();
    let mut drawn = 0;
    for y in 0..200 {
        for x in 0..200 {
            let (centre_x, centre_y) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let inside = (left..=right).contains(&centre_x) && (top..=bottom).contains(&centre_y);
            let alpha = pixel_at(&frame, x, y)[3];
            assert!(
                inside || alpha == 0,
                "({x}, {y}) is drawn outside [{left}, {top}, {right}, {bottom}]"
            );
            drawn += usize::from(alpha > 0);
        }
    }
    assert!(drawn > 0, "the part in front is drawn");
}

/// A group at `(x, y)` scaled by `scale`, holding a `size` rect of red
/// blurred by `blur`, turned a hair under a perspective when `projective`.
fn blurred_in_group(x: f64, scale: f64, size: f64, blur: f64, projective: bool) -> Layer {
    let mut rect = centred_rect(0.0, 0.0, size, size, "#FF0000FF");
    rect.effects.blur = blur;
    let mut layer = transformed(group_at(x, 100.0, vec![rect]), |transform| {
        transform.scale = Point { x: scale, y: scale };
    });
    if projective {
        layer = in_perspective(layer, 0.01, 0.0, 1000.0);
    }
    layer
}

fn max_difference(a: &GpuFrame, b: &GpuFrame, columns: std::ops::Range<u32>) -> u8 {
    let mut difference = 0;
    for y in 0..a.height() {
        for x in columns.clone() {
            let (a, b) = (pixel_at(a, x, y), pixel_at(b, x, y));
            difference = difference.max((0..4).map(|i| a[i].abs_diff(b[i])).max().unwrap());
        }
    }
    difference
}

#[test]
fn blurs_inside_a_plane_in_output_pixels() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    // Scaled by 100, the plane is rasterized at the largest scale, 64, so
    // its pixels are 100 / 64 output pixels wide and a blur is scaled to 0.64.
    let scene = |projective| {
        sized_scene(
            200,
            200,
            vec![blurred_in_group(100.0, 100.0, 0.6, 8.0, projective)],
        )
    };
    let flat = renderer.render(&scene(false)).unwrap();
    let projected = renderer.render(&scene(true)).unwrap();
    assert_eq!(pixel_at(&flat, 100, 100), RED);
    // Unscaled, the blur would be 12.5 output pixels, and differ by 25.
    let difference = max_difference(&flat, &projected, 0..200);
    assert!(difference <= 8, "{difference}");
}

#[test]
fn effects_after_a_plane_keep_their_size() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let sibling = || blurred_in_group(160.0, 1.0, 30.0, 8.0, false);
    let alone = renderer
        .render(&sized_scene(200, 200, vec![sibling()]))
        .unwrap();
    let plane = blurred_in_group(60.0, 100.0, 0.6, 8.0, true);
    let after = renderer
        .render(&sized_scene(200, 200, vec![plane, sibling()]))
        .unwrap();
    assert!(pixel_at(&alone, 160, 100)[3] > 200);
    assert_eq!(max_difference(&alone, &after, 130..200), 0);
}

#[test]
fn hit_tests_through_a_projection() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    renderer.set_record_hit_regions(true);
    let card = in_perspective(
        centred_rect(100.0, 100.0, 100.0, 100.0, "#FF0000FF"),
        0.0,
        45.0,
        300.0,
    );
    let homography = card.transform.homography();
    let corners = [(-50.0, -50.0), (50.0, -50.0), (50.0, 50.0), (-50.0, 50.0)].map(|(x, y)| {
        let [x, y, _] = project(&homography, x, y);
        (x, y)
    });
    let [centre_x, centre_y, _] = project(&homography, 0.0, 0.0);
    renderer.render(&sized_scene(200, 200, vec![card])).unwrap();
    let regions = renderer.hit_regions();
    let hit = |x: f64, y: f64| hit_test(regions, Point { x, y });
    assert_eq!(hit(centre_x, centre_y), Some("rect"));

    // The far edge is the shorter of the two vertical ones.
    let length = |from: (f64, f64), to: (f64, f64)| (to.0 - from.0).hypot(to.1 - from.1);
    let (from, to) = if length(corners[1], corners[2]) < length(corners[3], corners[0]) {
        (corners[1], corners[2])
    } else {
        (corners[3], corners[0])
    };
    let middle = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
    let outward = (middle.0 - centre_x).signum();
    assert_eq!(hit(middle.0 - 3.0 * outward, middle.1), Some("rect"));
    assert_eq!(hit(middle.0 + 3.0 * outward, middle.1), None);
}

#[test]
fn hit_tests_a_clip_inside_a_projective_group() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    renderer.set_record_hit_regions(true);
    let group = in_perspective(
        clipped_group(
            EvaluatedTransform {
                position: Point { x: 100.0, y: 100.0 },
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            Clip {
                x: -50.0,
                y: -50.0,
                width: 50.0,
                height: 100.0,
                corner_radius: 0.0,
            },
            vec![centred_rect(0.0, 0.0, 100.0, 100.0, "#FF0000FF")],
        ),
        0.0,
        30.0,
        600.0,
    );
    let homography = group.transform.homography();
    renderer
        .render(&sized_scene(200, 200, vec![group]))
        .unwrap();
    let regions = renderer.hit_regions();
    let at = |x: f64| {
        let [x, y, _] = project(&homography, x, 0.0);
        hit_test(regions, Point { x, y })
    };
    assert_eq!(at(-25.0), Some("rect"));
    assert_eq!(at(25.0), None, "inside the quad, outside the group's clip");
}

#[test]
fn hit_tests_through_nested_planes() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    renderer.set_record_hit_regions(true);
    // A projective group clipped to its left half, in a projective group.
    let inner = in_perspective(
        clipped_group(
            EvaluatedTransform {
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            Clip {
                x: -50.0,
                y: -50.0,
                width: 50.0,
                height: 100.0,
                corner_radius: 0.0,
            },
            vec![centred_rect(0.0, 0.0, 100.0, 100.0, "#FF0000FF")],
        ),
        0.0,
        30.0,
        600.0,
    );
    let inner_homography = inner.transform.homography();
    let outer = in_perspective(group_at(100.0, 100.0, vec![inner]), 20.0, 0.0, 800.0);
    let homography = multiply3(&outer.transform.homography(), &inner_homography);
    renderer
        .render(&sized_scene(200, 200, vec![outer]))
        .unwrap();
    let regions = renderer.hit_regions();
    let at = |x: f64, y: f64| {
        let [x, y, _] = project(&homography, x, y);
        hit_test(regions, Point { x, y })
    };
    // The quad is the rect's corners through both planes.
    let corners = [(-50.0, -50.0), (50.0, -50.0), (50.0, 50.0), (-50.0, 50.0)];
    for (corner, (x, y)) in regions[0].corners.iter().zip(corners) {
        let [x, y, _] = project(&homography, x, y);
        assert!(
            (corner.x - x).hypot(corner.y - y) < 0.01,
            "{corner:?} {x} {y}"
        );
    }
    // Close to the clip's edge, which a map a few pixels off would cross.
    assert_eq!(at(-3.0, 0.0), Some("rect"));
    assert_eq!(at(-3.0, 40.0), Some("rect"));
    assert_eq!(at(3.0, 0.0), None, "outside the inner group's clip");
    assert_eq!(at(3.0, 40.0), None);
    // And to the rect's own, outside the clip's far side.
    assert_eq!(at(-47.0, -47.0), Some("rect"));
    assert_eq!(at(-53.0, 0.0), None, "outside the rect");
}

#[test]
fn hit_tests_a_projection_through_the_clip_of_its_parent() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    renderer.set_record_hit_regions(true);
    let rect = || {
        in_perspective(
            centred_rect(100.0, 100.0, 100.0, 100.0, "#FF0000FF"),
            0.0,
            30.0,
            600.0,
        )
    };
    let homography = rect().transform.homography();
    let left_half = Clip {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 200.0,
        corner_radius: 0.0,
    };
    let clipped = clipped_group(EvaluatedTransform::default(), left_half, vec![rect()]);
    renderer
        .render(&sized_scene(200, 200, vec![clipped]))
        .unwrap();
    let clipped_regions = renderer.take_hit_regions();
    renderer
        .render(&sized_scene(200, 200, vec![rect()]))
        .unwrap();
    let regions = renderer.hit_regions();
    for x in [-20.0, 20.0] {
        let [x, y, _] = project(&homography, x, 0.0);
        let point = Point { x, y };
        assert_eq!(hit_test(regions, point), Some("rect"), "unclipped at {x}");
        let expected = (x < 100.0).then_some("rect");
        assert_eq!(
            hit_test(&clipped_regions, point),
            expected,
            "clipped at {x}"
        );
    }
}

/// An `Image` layer `id` showing the file at `path`, `width` units wide.
fn image_layer(id: &str, path: &Path, width: f64) -> Layer {
    Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Image {
            asset: ResolvedAsset {
                id: id.to_owned(),
                location: AssetLocation::File {
                    path: path.to_string_lossy().into_owned(),
                },
            },
            width: Some(width),
            height: None,
            fit: None,
            animation: None,
        },
    }
}

/// A `Psd` layer showing the lip-sync fixture, a 240x320 canvas.
fn portrait() -> Layer {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/lipsync-fixture.psd");
    Layer {
        id: "portrait".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Psd {
            asset: ResolvedAsset {
                id: "fixture".to_owned(),
                location: AssetLocation::File {
                    path: fixture.to_string_lossy().into_owned(),
                },
            },
            visible_layers: vec!["body".to_owned(), "body/base".to_owned()],
            enabled_layers: Vec::new(),
            disabled_layers: Vec::new(),
        },
    }
}

const WIDE_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#;

/// The textures the renderer holds whose keys start with `prefix`, as
/// (width, height).
fn cached_textures(renderer: &GpuRenderer, prefix: &str) -> Vec<(u32, u32)> {
    renderer
        .textures
        .iter()
        .filter(|(key, _)| key.starts_with(prefix))
        .map(|(_, cached)| (cached.texture.width, cached.texture.height))
        .collect()
}

#[test]
fn measures_a_projective_svg_without_rasterizing_it() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wide.svg");
    std::fs::write(&path, WIDE_SVG).unwrap();
    let svg = transformed(image_layer("svg", &path, 20.0), |transform| {
        transform.position = Point { x: 100.0, y: 100.0 };
        transform.scale = Point { x: 4.0, y: 4.0 };
    });
    let svg = in_perspective(svg, 0.0, 30.0, 600.0);
    let frame = renderer.render(&sized_scene(200, 200, vec![svg])).unwrap();
    assert_eq!(pixel_at(&frame, 100, 100), RED);
    // Only the plane's raster: measuring it rasterizes nothing at 1x.
    let textures = cached_textures(&renderer, "image\0");
    assert_eq!(textures.len(), 1, "{textures:?}");
    assert!(textures[0].0 > 20, "{textures:?}");
}

#[test]
fn measures_a_projective_psd_without_compositing_it() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let psd = transformed(portrait(), |transform| {
        transform.position = Point { x: 120.0, y: 160.0 };
        transform.scale = Point { x: 0.25, y: 0.25 };
    });
    let psd = in_perspective(psd, 0.0, 30.0, 600.0);
    let frame = renderer.render(&sized_scene(240, 320, vec![psd])).unwrap();
    assert!(pixel_at(&frame, 120, 160)[3] > 0);
    // Only the plane's composite, reduced: measuring it composites nothing
    // at full size.
    let textures = cached_textures(&renderer, "psd\0");
    assert_eq!(textures.len(), 1, "{textures:?}");
    assert!(textures[0].0 < 240, "{textures:?}");
}

/// What `layer`, prepared alone at `scale`, covers: measured as a plane's
/// content is when `measuring`, else drawn.
fn covered(renderer: &mut GpuRenderer, layer: &Layer, scale: f32, measuring: bool) -> PixelBounds {
    let mut items = Vec::new();
    renderer.measuring_plane = measuring;
    let parent = LayerState {
        transform: Affine {
            a: scale,
            d: scale,
            ..Affine::IDENTITY
        },
        ..LayerState::default()
    };
    let prepared = renderer.prepare_layer(layer, parent, &mut items);
    renderer.measuring_plane = false;
    prepared.unwrap();
    assert!(
        items
            .iter()
            .all(|item| matches!(item, PreparedItem::Layer(_))),
        "only layers"
    );
    content_bounds(&items, |_| None).unwrap()
}

#[test]
fn stand_ins_cover_what_their_content_draws() {
    struct Decoder;

    impl VideoFrameDecoder for Decoder {
        fn decode_frame(&mut self, _: &Path, _: f64) -> Result<VideoFrame, MediaError> {
            Ok(VideoFrame {
                width: 150,
                height: 75,
                pixels: [255, 0, 0, 255].repeat(150 * 75).into(),
            })
        }
    }

    let Some(renderer) = transparent_renderer() else {
        return;
    };
    let mut renderer = renderer.with_video_decoder(Decoder);
    let video = Layer {
        id: "video".to_owned(),
        transform: EvaluatedTransform::default(),
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
            width: None,
            height: None,
        },
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wide.svg");
    std::fs::write(&path, WIDE_SVG).unwrap();
    renderer.max_texture_dimension = 100;
    // An SVG 20.5 wide rasterizes 21 texels wide, so it is filtered and
    // its quad reaches a texel past its edge; at 6x it is shrunk to the
    // texture limit too. The portrait and the video's frame are shrunk to
    // it at full size.
    let layers = [
        (image_layer("svg", &path, 20.5), 1.0),
        (image_layer("svg", &path, 20.5), 6.0),
        (portrait(), 1.0),
        (portrait(), 0.3),
        (video, 1.0),
    ];
    for (layer, scale) in &layers {
        let textures = renderer.textures.len();
        let measured = covered(&mut renderer, layer, *scale, true);
        assert_eq!(renderer.textures.len(), textures, "{} at {scale}", layer.id);
        let drawn = covered(&mut renderer, layer, *scale, false);
        assert_eq!(measured, drawn, "{} at {scale}", layer.id);
    }
}

#[test]
fn a_projective_layer_with_effects_takes_its_opacity_and_mode_once() {
    let Some(mut renderer) = transparent_renderer() else {
        return;
    };
    let background = corner_rect("background", 0.0, 0.0, 200.0, 200.0, "#FF0000FF");
    let mut white = in_perspective(
        centred_rect(100.0, 100.0, 100.0, 100.0, "#FFFFFFFF"),
        0.0,
        30.0,
        600.0,
    );
    white.effects.blur = 1.0;
    white.opacity = 0.5;
    white.blend_mode = BlendMode::Difference;
    let frame = renderer
        .render(&sized_scene(200, 200, vec![background, white]))
        .unwrap();
    // Half of the difference with red, cyan, over red: drawn onto the
    // effect's canvas at full opacity, blending normally, then blended once.
    // Taken twice, the opacity would leave red at 191 and the others at 64.
    let pixel = pixel_at(&frame, 100, 100);
    assert_eq!(pixel[3], 255);
    assert!(
        pixel[..3].iter().all(|channel| channel.abs_diff(128) <= 1),
        "{pixel:?}"
    );
}
