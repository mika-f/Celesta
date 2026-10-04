use crate::rect::signed_distance_rounded_box;
use celesta_composition::{BlendMode, Clip, Point};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct ParentState {
    pub(crate) position: Point,
    pub(crate) scale: Point,
    pub(crate) opacity: f64,
    /// The innermost clip the layer is drawn through.
    pub(crate) clip: Option<Arc<ClipNode>>,
    /// The blend mode of the layer being drawn (not inherited by children).
    pub(crate) blend_mode: BlendMode,
}

impl Default for ParentState {
    fn default() -> Self {
        Self {
            position: Point { x: 0.0, y: 0.0 },
            scale: Point { x: 1.0, y: 1.0 },
            opacity: 1.0,
            clip: None,
            blend_mode: BlendMode::Normal,
        }
    }
}

/// One clip in the chain of clips a layer sits inside; a pixel is drawn
/// through all of them, so nested clips intersect.
#[derive(Debug)]
pub(crate) struct ClipNode {
    pub(crate) region: ClipRegion,
    pub(crate) parent: Option<Arc<ClipNode>>,
}

/// A group's clip rectangle with the group's frame it is defined in.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClipRegion {
    /// Canvas position of the group's origin.
    pub(crate) origin: Point,
    /// The group's accumulated scale.
    pub(crate) scale: Point,
    pub(crate) center: Point,
    pub(crate) half: Point,
    pub(crate) radius: f64,
    /// Local distance to canvas pixels, `sqrt(|scale.x * scale.y|)`: exact for
    /// a uniform scale, an approximation otherwise. `celesta-gpu-renderer`
    /// uses the same factor.
    pub(crate) distance_scale: f64,
}

impl ClipRegion {
    pub(crate) fn new(clip: &Clip, position: Point, scale: Point) -> Self {
        Self {
            origin: position,
            scale,
            center: Point {
                x: clip.x + clip.width / 2.0,
                y: clip.y + clip.height / 2.0,
            },
            half: Point {
                x: clip.width / 2.0,
                y: clip.height / 2.0,
            },
            radius: clip.effective_corner_radius(),
            distance_scale: (scale.x * scale.y).abs().sqrt(),
        }
    }

    /// How much of the canvas pixel `(x, y)` lies inside the region, 0 to 1,
    /// anti-aliased over the edge like `rasterize_rect`.
    pub(crate) fn coverage(&self, x: i32, y: i32) -> f64 {
        let local_x = (f64::from(x) + 0.5 - self.origin.x) / self.scale.x - self.center.x;
        let local_y = (f64::from(y) + 0.5 - self.origin.y) / self.scale.y - self.center.y;
        let distance =
            signed_distance_rounded_box(local_x, local_y, self.half.x, self.half.y, self.radius);
        (0.5 - distance * self.distance_scale).clamp(0.0, 1.0)
    }
}

/// How much of the canvas pixel `(x, y)` is inside every clip in the chain.
pub(crate) fn clip_coverage(clip: &Option<Arc<ClipNode>>, x: i32, y: i32) -> f64 {
    let mut coverage = 1.0;
    let mut node = clip.as_deref();
    while let Some(current) = node {
        coverage *= current.region.coverage(x, y);
        if coverage == 0.0 {
            return 0.0;
        }
        node = current.parent.as_deref();
    }
    coverage
}
