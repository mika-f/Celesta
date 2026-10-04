use crate::bounds::{CanvasRegion, PixelBounds};
use crate::effect::EffectSpec;
use crate::layer::PreparedItem;
use crate::texture::LayerTexture;
use celesta_composition::BlendMode;

/// A frame's draws, ready to encode: the buffer holding every layer's
/// instance, and what to draw with them in painter's order.
pub(crate) struct PreparedDraws {
    pub(crate) instances: wgpu::Buffer,
    /// The bind group exposing the frame's clips to the fragment shader.
    pub(crate) clips: wgpu::BindGroup,
    /// Only `GpuStep::Draw`s unless `composite` is set.
    pub(crate) steps: Vec<GpuStep>,
    pub(crate) composite: Option<CompositePlan>,
}

/// How a frame that uses blend modes composites through canvases.
pub(crate) struct CompositePlan {
    /// The instance that copies the root canvas onto the target.
    pub(crate) blit_instance: u32,
}

/// The canvas an isolated group or an effect draws its layers onto.
#[derive(Clone, Copy)]
pub(crate) struct GroupPlan {
    pub(crate) canvas: CanvasRegion,
    /// What the group's layers cover, in scene pixels.
    pub(crate) content: Option<PixelBounds>,
    /// Whether anything of the group (or its effect) reaches the scene.
    /// When not, its layers still draw onto `canvas`, a single pixel none
    /// of them can touch, and the group draws nothing onto its parent.
    pub(crate) drawn: bool,
}

/// Plans the canvas of every isolated group and effect in `items`, in the
/// order they begin, from the bounds of what their layers draw.
pub(crate) fn plan_groups(items: &[PreparedItem], scene: CanvasRegion) -> Vec<GroupPlan> {
    let mut plans: Vec<GroupPlan> = Vec::new();
    // Each open group's index in `plans` and what its layers cover so far.
    let mut open: Vec<(usize, Option<PixelBounds>)> = Vec::new();
    let cover = |open: &mut Vec<(usize, Option<PixelBounds>)>, bounds: Option<PixelBounds>| {
        if let Some((_, covered)) = open.last_mut() {
            *covered = PixelBounds::union(*covered, bounds);
        }
    };
    let plan = |content: Option<PixelBounds>, output: Option<PixelBounds>| {
        let canvas = CanvasRegion::covering(output, scene);
        GroupPlan {
            canvas: canvas.unwrap_or(CanvasRegion {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }),
            content,
            drawn: canvas.is_some(),
        }
    };
    for item in items {
        match item {
            PreparedItem::Layer(layer) => cover(&mut open, Some(layer.bounds())),
            PreparedItem::PendingText => unreachable!("pending text is resolved"),
            PreparedItem::BeginGroup => {
                open.push((plans.len(), None));
                plans.push(plan(None, None));
            }
            PreparedItem::EndGroup(_) => {
                let (index, content) = open.pop().expect("every group was begun");
                plans[index] = plan(content, content);
                cover(&mut open, content);
            }
            PreparedItem::EndEffect(_, effects) => {
                let (index, content) = open.pop().expect("every group was begun");
                let output = content.map(|content| effects.output_bounds(content));
                plans[index] = plan(content, output);
                cover(&mut open, output);
            }
        }
    }
    plans
}

pub(crate) enum GpuStep {
    /// A run of instances that sample the same texture, drawn source-over.
    Draw(GpuDraw),
    /// One instance blended onto the current canvas through `blend_mode`.
    Blend {
        draw: GpuDraw,
        blend_mode: BlendMode,
        /// The canvas pixels the draw can change, `[x, y, width, height]`;
        /// `None` when it misses the canvas.
        area: Option<[u32; 4]>,
    },
    /// Starts drawing onto a fresh transparent canvas covering `canvas`.
    BeginGroup { canvas: CanvasRegion },
    /// Draws the finished group canvas onto its parent with `instance`.
    EndGroup {
        instance: u32,
        blend_mode: BlendMode,
        /// The parent canvas pixels the group's canvas covers; `None` when
        /// it draws nothing there.
        area: Option<[u32; 4]>,
    },
    EndEffect {
        inner_instance: u32,
        final_instance: u32,
        blend_mode: BlendMode,
        effects: EffectSpec,
        /// The part of the effect's canvas its layers drew on, in that
        /// canvas's pixels; `None` when they drew nothing.
        content: Option<PixelBounds>,
        /// As for `EndGroup`.
        area: Option<[u32; 4]>,
    },
}

pub(crate) struct GpuDraw {
    pub(crate) texture: LayerTexture,
    /// `0..6` (one quad) except for a path, which draws a quad per tile it
    /// covers.
    pub(crate) vertices: std::ops::Range<u32>,
    pub(crate) instances: std::ops::Range<u32>,
}
