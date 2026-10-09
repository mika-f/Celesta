use crate::bounds::{CanvasRegion, PixelBounds};
use crate::effect::EffectSpec;
use crate::layer::PreparedItem;
use crate::mask::MaskSpec;
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

/// The canvas an isolated group, an effect, or a mask and its group's
/// children draw their layers onto.
#[derive(Clone, Copy)]
pub(crate) struct GroupPlan {
    pub(crate) canvas: CanvasRegion,
    /// What the group's layers cover, in scene pixels.
    pub(crate) content: Option<PixelBounds>,
    /// The box of the group's layers' shapes, in scene pixels: `content`
    /// without the margins added for filtering and rounding.
    pub(crate) shape: Option<PixelBounds>,
    /// Whether anything of the group (or its effect) reaches the scene.
    /// When not, its layers still draw onto `canvas`, a single pixel none
    /// of them can touch, and the group draws nothing onto its parent.
    pub(crate) drawn: bool,
}

/// Plans the canvas of every isolated group, effect, and mask in `items`, in
/// the order they begin, from the bounds of what their layers draw. A
/// mask's canvas covers where its children show through it.
pub(crate) fn plan_groups(items: &[PreparedItem], scene: CanvasRegion) -> Vec<GroupPlan> {
    let mut plans: Vec<GroupPlan> = Vec::new();
    // Each open group's index in `plans` and what its layers cover so far.
    let mut open: Vec<(usize, Covered)> = Vec::new();
    // What each open mask drew, once its group's children have begun.
    let mut masks: Vec<Covered> = Vec::new();
    let cover = |open: &mut Vec<(usize, Covered)>, bounds: Covered| {
        if let Some((_, covered)) = open.last_mut() {
            *covered = covered.union(bounds);
        }
    };
    let plan = |covered: Covered, output: Option<PixelBounds>| {
        let canvas = CanvasRegion::covering(output, scene);
        GroupPlan {
            canvas: canvas.unwrap_or(CanvasRegion {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }),
            content: covered.content,
            shape: covered.shape,
            drawn: canvas.is_some(),
        }
    };
    for item in items {
        match item {
            PreparedItem::Layer(layer) => cover(
                &mut open,
                Covered {
                    content: Some(layer.bounds()),
                    shape: Some(layer.shape()),
                },
            ),
            PreparedItem::PendingText | PreparedItem::PendingPath => {
                unreachable!("pending text and paths are resolved")
            }
            PreparedItem::BeginGroup | PreparedItem::BeginMask => {
                open.push((plans.len(), Covered::default()));
                plans.push(plan(Covered::default(), None));
            }
            PreparedItem::MaskContent => {
                let (index, mask) = open.pop().expect("every mask was begun");
                masks.push(mask);
                open.push((index, Covered::default()));
            }
            PreparedItem::EndMask(_, spec) => {
                let (index, children) = open.pop().expect("every mask was begun");
                let mask = masks.pop().expect("every mask has content");
                // Inverted, the children show wherever the mask is not
                // drawn, which can be anywhere they are.
                let shown = if spec.invert {
                    children
                } else {
                    Covered {
                        content: PixelBounds::intersection(children.content, mask.content),
                        shape: PixelBounds::intersection(children.shape, mask.shape),
                    }
                };
                plans[index] = plan(shown, shown.content);
                cover(&mut open, shown);
            }
            PreparedItem::EndGroup(_) => {
                let (index, covered) = open.pop().expect("every group was begun");
                plans[index] = plan(covered, covered.content);
                cover(&mut open, covered);
            }
            PreparedItem::EndEffect(_, effects) => {
                let (index, covered) = open.pop().expect("every group was begun");
                let output = covered.content.map(|content| effects.output_bounds(content));
                plans[index] = plan(covered, output);
                // A blur or a shadow spreads the pixels, not the shape.
                cover(
                    &mut open,
                    Covered {
                        content: output,
                        shape: covered.shape,
                    },
                );
            }
        }
    }
    plans
}

/// What a group's layers cover so far: `GroupPlan::content` and `shape`.
#[derive(Clone, Copy, Default)]
struct Covered {
    content: Option<PixelBounds>,
    shape: Option<PixelBounds>,
}

impl Covered {
    fn union(self, other: Self) -> Self {
        Self {
            content: PixelBounds::union(self.content, other.content),
            shape: PixelBounds::union(self.shape, other.shape),
        }
    }
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
        /// The box of its layers' shapes, in scene pixels: the content box
        /// a custom shader sees.
        shape: Option<PixelBounds>,
        /// As for `EndGroup`.
        area: Option<[u32; 4]>,
    },
    /// Starts drawing a mask onto a fresh transparent canvas covering
    /// `canvas`; the steps from the matching `MaskContent` draw the group's
    /// children onto a second one covering the same region.
    BeginMask { canvas: CanvasRegion },
    /// Ends the mask's steps; the children's follow, onto the second canvas.
    MaskContent,
    /// Draws the children shown through the mask onto the parent with
    /// `instance`.
    EndMask {
        instance: u32,
        blend_mode: BlendMode,
        mask: MaskSpec,
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
