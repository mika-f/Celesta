use crate::renderer::CpuRenderer;
use crate::types::{RenderOptions, RgbaFrame};
use celesta_composition::{
    BlendMode, Clip, DEFAULT_MITER_LIMIT, EvaluatedTransform, GroupMask, Layer, LayerContent,
    LayerShadow, LineCap, LineJoin, MaskMode, Paint, PathCommand, Point, Rational, Scene, Time,
};

fn scene(layers: Vec<Layer>) -> Scene {
    Scene {
        width: 40,
        height: 40,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers,
    }
}

/// A rect with its top-left corner at `(x, y)` in its parent.
fn rect(x: f64, y: f64, width: f64, height: f64, color: &str) -> Layer {
    Layer {
        id: format!("rect-{x}-{y}"),
        transform: EvaluatedTransform {
            position: Point { x, y },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Rect {
            width,
            height,
            fill: Some(Paint::Solid {
                color: color.to_owned(),
            }),
            stroke: None,
            corner_radius: 0.0,
        },
    }
}

fn red_square() -> Layer {
    rect(0.0, 0.0, 40.0, 40.0, "#FF0000FF")
}

fn mask(layers: Vec<Layer>) -> GroupMask {
    GroupMask {
        layers,
        mode: MaskMode::Alpha,
        invert: false,
    }
}

fn masked(transform: EvaluatedTransform, mask: GroupMask, layers: Vec<Layer>) -> Layer {
    Layer {
        id: "masked".to_owned(),
        transform,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Group {
            layers,
            clip: None,
            mask: Some(mask),
        },
    }
}

fn render(layers: Vec<Layer>) -> RgbaFrame {
    CpuRenderer::default().render(&scene(layers)).unwrap()
}

fn pixel(frame: &RgbaFrame, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * frame.width() + x) * 4) as usize;
    frame.pixels()[offset..offset + 4].try_into().unwrap()
}

const RED: [u8; 4] = [255, 0, 0, 255];

fn background() -> [u8; 4] {
    let color = RenderOptions::default().background;
    [color.red, color.green, color.blue, color.alpha]
}

fn assert_close(actual: [u8; 4], expected: [u8; 4]) {
    let close = actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 1);
    assert!(close, "{actual:?} is not within 1 of {expected:?}");
}

#[test]
fn an_alpha_mask_shows_the_children_only_where_it_is_drawn() {
    let frame = render(vec![masked(
        EvaluatedTransform::default(),
        mask(vec![rect(10.0, 10.0, 20.0, 20.0, "#FFFFFFFF")]),
        vec![red_square()],
    )]);
    assert_eq!(pixel(&frame, 20, 20), RED);
    assert_eq!(pixel(&frame, 10, 10), RED);
    for (x, y) in [(9, 20), (30, 20), (20, 9), (20, 30), (0, 0)] {
        assert_eq!(pixel(&frame, x, y), background(), "({x}, {y}) leaked");
    }
}

#[test]
fn a_path_masks_a_group() {
    // A diamond touching the middle of each edge of the scene.
    let diamond = Layer {
        id: "diamond".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Path {
            commands: vec![
                PathCommand::MoveTo { x: 20.0, y: 0.0 },
                PathCommand::LineTo { x: 40.0, y: 20.0 },
                PathCommand::LineTo { x: 20.0, y: 40.0 },
                PathCommand::LineTo { x: 0.0, y: 20.0 },
                PathCommand::Close,
            ],
            fill: Some(Paint::Solid {
                color: "#FFFFFFFF".to_owned(),
            }),
            stroke: None,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: DEFAULT_MITER_LIMIT,
        },
    };
    let frame = render(vec![masked(
        EvaluatedTransform::default(),
        mask(vec![diamond]),
        vec![red_square()],
    )]);
    assert_eq!(pixel(&frame, 20, 20), RED);
    assert_eq!(pixel(&frame, 2, 2), background());
    assert_eq!(pixel(&frame, 37, 37), background());
}

#[test]
fn a_luminance_mask_shows_the_children_by_the_masks_luma() {
    // Gray 0x80 has luma 128 / 255, so half of the red shows over the
    // background (20, 22, 28).
    let frame = render(vec![masked(
        EvaluatedTransform::default(),
        GroupMask {
            mode: MaskMode::Luminance,
            ..mask(vec![rect(0.0, 0.0, 40.0, 40.0, "#808080FF")])
        },
        vec![red_square()],
    )]);
    assert_close(pixel(&frame, 20, 20), [138, 11, 14, 255]);
}

#[test]
fn an_inverted_mask_shows_the_children_where_it_is_not_drawn() {
    let frame = render(vec![masked(
        EvaluatedTransform::default(),
        GroupMask {
            invert: true,
            ..mask(vec![rect(10.0, 10.0, 20.0, 20.0, "#FFFFFFFF")])
        },
        vec![red_square()],
    )]);
    assert_eq!(pixel(&frame, 20, 20), background());
    assert_eq!(pixel(&frame, 5, 5), RED);
}

#[test]
fn an_empty_mask_hides_the_group_unless_inverted() {
    let hidden = render(vec![masked(
        EvaluatedTransform::default(),
        mask(Vec::new()),
        vec![red_square()],
    )]);
    assert_eq!(pixel(&hidden, 20, 20), background());
    let shown = render(vec![masked(
        EvaluatedTransform::default(),
        GroupMask {
            invert: true,
            ..mask(Vec::new())
        },
        vec![red_square()],
    )]);
    assert_eq!(pixel(&shown, 20, 20), RED);
}

#[test]
fn a_mask_follows_the_groups_position_and_scale() {
    // The mask's 5x5 rect at the group's origin covers canvas 10..20.
    let frame = render(vec![masked(
        EvaluatedTransform {
            position: Point { x: 10.0, y: 10.0 },
            scale: Point { x: 2.0, y: 2.0 },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        mask(vec![rect(0.0, 0.0, 5.0, 5.0, "#FFFFFFFF")]),
        vec![rect(-10.0, -10.0, 40.0, 40.0, "#FF0000FF")],
    )]);
    assert_eq!(pixel(&frame, 15, 15), RED);
    assert_eq!(pixel(&frame, 25, 15), background());
    assert_eq!(pixel(&frame, 5, 15), background());
}

#[test]
fn a_clip_and_a_mask_both_limit_the_children() {
    let mut layer = masked(
        EvaluatedTransform::default(),
        mask(vec![rect(5.0, 0.0, 20.0, 40.0, "#FFFFFFFF")]),
        vec![red_square()],
    );
    if let LayerContent::Group { clip, .. } = &mut layer.content {
        *clip = Some(Clip {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 40.0,
            corner_radius: 0.0,
        });
    }
    let frame = render(vec![layer]);
    assert_eq!(pixel(&frame, 7, 20), RED);
    assert_eq!(pixel(&frame, 3, 20), background());
    assert_eq!(pixel(&frame, 12, 20), background());
}

#[test]
fn a_masked_group_is_isolated() {
    // Multiplied with the dark background the blue would darken; isolated,
    // it multiplies with nothing and shows as itself.
    let frame = render(vec![masked(
        EvaluatedTransform::default(),
        mask(vec![rect(0.0, 0.0, 40.0, 40.0, "#FFFFFFFF")]),
        vec![Layer {
            blend_mode: BlendMode::Multiply,
            ..rect(0.0, 0.0, 40.0, 40.0, "#0000FFFF")
        }],
    )]);
    assert_eq!(pixel(&frame, 20, 20), [0, 0, 255, 255]);
}

#[test]
fn a_shadow_follows_the_cut_out_shape() {
    // The mask leaves x 0..10 of the red; a hard shadow 10 px to the right
    // covers x 10..20, not the uncut red's 10..50.
    let mut layer = masked(
        EvaluatedTransform::default(),
        mask(vec![rect(0.0, 0.0, 10.0, 40.0, "#FFFFFFFF")]),
        vec![red_square()],
    );
    layer.effects.shadow = Some(LayerShadow {
        color: "#000000FF".to_owned(),
        blur: 0.0,
        offset_x: 10.0,
        offset_y: 0.0,
    });
    let frame = render(vec![layer]);
    assert_eq!(pixel(&frame, 5, 20), RED);
    assert_eq!(pixel(&frame, 15, 20), [0, 0, 0, 255]);
    assert_eq!(pixel(&frame, 30, 20), background());
}

#[test]
fn nested_masks_intersect() {
    let inner = masked(
        EvaluatedTransform::default(),
        mask(vec![rect(10.0, 0.0, 30.0, 40.0, "#FFFFFFFF")]),
        vec![red_square()],
    );
    let frame = render(vec![masked(
        EvaluatedTransform::default(),
        mask(vec![rect(0.0, 0.0, 20.0, 40.0, "#FFFFFFFF")]),
        vec![inner],
    )]);
    assert_eq!(pixel(&frame, 15, 20), RED);
    assert_eq!(pixel(&frame, 5, 20), background());
    assert_eq!(pixel(&frame, 25, 20), background());
}

#[test]
fn group_opacity_applies_once_and_a_mask_layers_opacity_counts() {
    // 0.5 x 0.5 of the red over the background: 255 * 0.25 + 20 * 0.75.
    let mut layer = masked(
        EvaluatedTransform::default(),
        mask(vec![Layer {
            opacity: 0.5,
            ..rect(0.0, 0.0, 40.0, 40.0, "#FFFFFFFF")
        }]),
        vec![red_square()],
    );
    layer.opacity = 0.5;
    let frame = render(vec![layer]);
    assert_close(pixel(&frame, 20, 20), [79, 17, 21, 255]);
}
