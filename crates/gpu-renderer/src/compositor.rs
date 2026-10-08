use crate::effect::EffectProcessor;
use crate::mask::MaskPipelines;
use crate::plan::{GpuStep, PreparedDraws};
use crate::texture::CanvasTexture;
use celesta_composition::BlendMode;

/// Encodes the steps of a frame that composites through canvases.
pub(crate) struct Compositor<'a> {
    pub(crate) device: &'a wgpu::Device,
    pub(crate) texture_layout: &'a wgpu::BindGroupLayout,
    pub(crate) layer_pipeline: &'a wgpu::RenderPipeline,
    pub(crate) blend_pipeline: &'a wgpu::RenderPipeline,
    pub(crate) backdrop: &'a BackdropTexture,
    pub(crate) draws: &'a PreparedDraws,
    pub(crate) effects: &'a mut EffectProcessor,
    pub(crate) masks: &'a MaskPipelines,
}

/// One draw onto a canvas, in painter's order.
pub(crate) struct CanvasDraw<'s> {
    /// The texture drawn: a layer's, or (`Err`) the finished group canvas
    /// at that index of `draw_canvas`'s `groups`.
    pub(crate) texture: Result<&'s wgpu::BindGroup, usize>,
    pub(crate) vertices: std::ops::Range<u32>,
    pub(crate) instances: std::ops::Range<u32>,
    pub(crate) blend_mode: BlendMode,
    /// For blended draws, the canvas pixels the draw can change.
    pub(crate) area: Option<[u32; 4]>,
}

impl Compositor<'_> {
    /// Clears `canvas` to `clear` and draws `steps` onto it. The groups
    /// among the steps are composited onto canvases of their own first, so
    /// that `canvas` itself is drawn in as few render passes as possible: a
    /// pass loads and stores its whole target, and the root canvas is
    /// scene-sized. Only a blended draw, which reads a copy of what is
    /// beneath it, ends a pass.
    pub(crate) fn draw_canvas(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        canvas: &CanvasTexture,
        clear: wgpu::Color,
        steps: &[GpuStep],
    ) {
        let mut groups: Vec<CanvasTexture> = Vec::new();
        let mut list: Vec<CanvasDraw<'_>> = Vec::new();
        let mut index = 0;
        while index < steps.len() {
            match &steps[index] {
                GpuStep::Draw(draw) => list.push(CanvasDraw {
                    texture: Ok(&draw.texture.bind_group),
                    vertices: draw.vertices.clone(),
                    instances: draw.instances.clone(),
                    blend_mode: BlendMode::Normal,
                    area: None,
                }),
                GpuStep::Blend {
                    draw,
                    blend_mode,
                    area,
                } => {
                    if area.is_some() {
                        list.push(CanvasDraw {
                            texture: Ok(&draw.texture.bind_group),
                            vertices: draw.vertices.clone(),
                            instances: draw.instances.clone(),
                            blend_mode: *blend_mode,
                            area: *area,
                        });
                    }
                }
                GpuStep::BeginGroup { .. } | GpuStep::BeginMask { .. } => {
                    let end = group_end(steps, index);
                    let (instance, blend_mode, area) = match &steps[end] {
                        GpuStep::EndGroup {
                            instance,
                            blend_mode,
                            area,
                        } => (*instance, *blend_mode, *area),
                        GpuStep::EndEffect {
                            final_instance,
                            blend_mode,
                            area,
                            ..
                        } => (*final_instance, *blend_mode, *area),
                        GpuStep::EndMask {
                            instance,
                            blend_mode,
                            area,
                            ..
                        } => (*instance, *blend_mode, *area),
                        _ => unreachable!("group_end returns a group's end"),
                    };
                    match self.draw_group(encoder, &steps[index..=end]) {
                        Some(group) if area.is_some() => {
                            list.push(CanvasDraw {
                                texture: Err(groups.len()),
                                vertices: 0..6,
                                instances: instance..instance + 1,
                                blend_mode,
                                area,
                            });
                            groups.push(group);
                        }
                        Some(group) => self.effects.recycle(group),
                        None => {}
                    }
                    index = end;
                }
                GpuStep::EndGroup { .. }
                | GpuStep::EndEffect { .. }
                | GpuStep::MaskContent
                | GpuStep::EndMask { .. } => {
                    unreachable!("draw_group consumes every group's steps")
                }
            }
            index += 1;
        }

        let mut load = wgpu::LoadOp::Clear(clear);
        let texture = |draw: &CanvasDraw<'_>| -> wgpu::BindGroup {
            match draw.texture {
                Ok(texture) => texture.clone(),
                Err(group) => groups[group].bind_group.clone(),
            }
        };
        let mut draws = list.iter().peekable();
        while let Some(draw) = draws.next() {
            if draw.blend_mode.is_normal() {
                // One pass for the whole run of source-over draws.
                let mut pass = begin_pass(encoder, &canvas.view, load);
                pass.set_pipeline(self.layer_pipeline);
                pass.set_vertex_buffer(0, self.draws.instances.slice(..));
                pass.set_bind_group(2, &self.draws.clips, &[]);
                pass.set_bind_group(0, &texture(draw), &[]);
                pass.draw(draw.vertices.clone(), draw.instances.clone());
                while let Some(next) = draws.next_if(|next| next.blend_mode.is_normal()) {
                    pass.set_bind_group(0, &texture(next), &[]);
                    pass.draw(next.vertices.clone(), next.instances.clone());
                }
            } else {
                if let wgpu::LoadOp::Clear(_) = load {
                    drop(begin_pass(encoder, &canvas.view, load));
                }
                let [x, y, width, height] = draw.area.expect("blended draws have an area");
                // `fs_blend` reads the backdrop only under the draw, so only
                // that part of the canvas needs copying.
                let origin = wgpu::Origin3d { x, y, z: 0 };
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        origin,
                        ..canvas.texture.as_image_copy()
                    },
                    wgpu::TexelCopyTextureInfo {
                        origin,
                        ..self.backdrop.texture.as_image_copy()
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
                let mut pass = begin_pass(encoder, &canvas.view, wgpu::LoadOp::Load);
                pass.set_pipeline(self.blend_pipeline);
                pass.set_vertex_buffer(0, self.draws.instances.slice(..));
                pass.set_bind_group(0, &texture(draw), &[]);
                pass.set_bind_group(1, &self.backdrop.bind_group, &[]);
                pass.set_bind_group(2, &self.draws.clips, &[]);
                pass.draw(draw.vertices.clone(), draw.instances.clone());
            }
            load = wgpu::LoadOp::Load;
        }
        if let wgpu::LoadOp::Clear(_) = load {
            drop(begin_pass(encoder, &canvas.view, load));
        }
        for group in groups {
            self.effects.recycle(group);
        }
    }

    /// Composites the group `steps` (from its `BeginGroup` or `BeginMask` to
    /// its end) onto a canvas of its own and applies its effect or mask,
    /// returning the canvas to draw onto the group's parent, or `None` when
    /// it holds nothing.
    pub(crate) fn draw_group(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        steps: &[GpuStep],
    ) -> Option<CanvasTexture> {
        let (
            Some(GpuStep::BeginGroup { canvas: region } | GpuStep::BeginMask { canvas: region }),
            Some(end),
        ) = (steps.first(), steps.last())
        else {
            unreachable!("a group runs from its BeginGroup or BeginMask to its end");
        };
        if let GpuStep::EndMask { mask, .. } = end {
            let split = mask_content(steps);
            // Copied out of `self`, so the closure borrows nothing of it
            // while `draw_canvas` takes `&mut self`.
            let (device, layout) = (self.device, self.texture_layout);
            let take = |effects: &mut EffectProcessor| {
                effects.take_canvas(device, layout, region.width, region.height)
            };
            let matte = take(self.effects);
            self.draw_canvas(encoder, &matte, wgpu::Color::TRANSPARENT, &steps[1..split]);
            let children = take(self.effects);
            self.draw_canvas(
                encoder,
                &children,
                wgpu::Color::TRANSPARENT,
                &steps[split + 1..steps.len() - 1],
            );
            let result = take(self.effects);
            self.masks.apply(encoder, &children, &matte, &result, *mask);
            self.effects.recycle(matte);
            self.effects.recycle(children);
            return Some(result);
        }
        let canvas = self.effects.take_canvas(
            self.device,
            self.texture_layout,
            region.width,
            region.height,
        );
        self.draw_canvas(
            encoder,
            &canvas,
            wgpu::Color::TRANSPARENT,
            &steps[1..steps.len() - 1],
        );
        let GpuStep::EndEffect {
            inner_instance,
            effects,
            content,
            ..
        } = end
        else {
            return Some(canvas);
        };
        // Layers that drew nothing leave nothing to filter.
        let Some(content) = *content else {
            self.effects.recycle(canvas);
            return None;
        };
        let result = self.effects.take_canvas(
            self.device,
            self.texture_layout,
            region.width,
            region.height,
        );
        let mut filtered = Vec::new();
        for shadow in [effects.shadow, effects.glow].into_iter().flatten() {
            filtered.push(self.effects.apply(
                self.device,
                encoder,
                &canvas,
                self.texture_layout,
                content,
                shadow.blur,
                shadow.offset,
                Some(shadow.color),
            ));
        }
        let blurred = (effects.blur > 0.0).then(|| {
            self.effects.apply(
                self.device,
                encoder,
                &canvas,
                self.texture_layout,
                content,
                effects.blur,
                [0.0, 0.0],
                None,
            )
        });
        // Shadows, then the glow, then the (blurred) layers on top.
        let mut pass = begin_pass(
            encoder,
            &result.view,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        pass.set_pipeline(self.layer_pipeline);
        pass.set_vertex_buffer(0, self.draws.instances.slice(..));
        pass.set_bind_group(2, &self.draws.clips, &[]);
        for texture in filtered.iter().chain([blurred.as_ref().unwrap_or(&canvas)]) {
            pass.set_bind_group(0, &texture.bind_group, &[]);
            pass.draw(0..6, *inner_instance..*inner_instance + 1);
        }
        drop(pass);
        for texture in filtered.into_iter().chain(blurred).chain([canvas]) {
            self.effects.recycle(texture);
        }
        Some(result)
    }
}

/// The index of the step that ends the group `steps[begin]` begins.
pub(crate) fn group_end(steps: &[GpuStep], begin: usize) -> usize {
    let mut depth = 0;
    for (index, step) in steps.iter().enumerate().skip(begin) {
        match step {
            GpuStep::BeginGroup { .. } | GpuStep::BeginMask { .. } => depth += 1,
            GpuStep::EndGroup { .. } | GpuStep::EndEffect { .. } | GpuStep::EndMask { .. } => {
                depth -= 1;
                if depth == 0 {
                    return index;
                }
            }
            GpuStep::Draw(_) | GpuStep::Blend { .. } | GpuStep::MaskContent => {}
        }
    }
    unreachable!("every group has an end")
}

/// The index of the `MaskContent` that ends the mask `steps[0]` begins.
fn mask_content(steps: &[GpuStep]) -> usize {
    let mut depth = 0;
    for (index, step) in steps.iter().enumerate() {
        match step {
            GpuStep::BeginGroup { .. } | GpuStep::BeginMask { .. } => depth += 1,
            GpuStep::EndGroup { .. } | GpuStep::EndEffect { .. } | GpuStep::EndMask { .. } => {
                depth -= 1;
            }
            GpuStep::MaskContent if depth == 1 => return index,
            GpuStep::Draw(_) | GpuStep::Blend { .. } | GpuStep::MaskContent => {}
        }
    }
    unreachable!("every mask has its content")
}

pub(crate) struct BackdropTexture {
    pub(crate) texture: wgpu::Texture,
    /// Binds the backdrop as group 1 of a `PipelineKind::Blend` pipeline.
    pub(crate) bind_group: wgpu::BindGroup,
}

pub(crate) fn begin_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Celesta layer pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}
