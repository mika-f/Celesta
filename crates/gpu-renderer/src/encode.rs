use crate::compositor::{Compositor, begin_pass};
use crate::effect::PASSES_PER_FILTER;
use crate::error::GpuRenderError;
use crate::pipeline::{PipelineKind, create_pipeline};
use crate::plan::{GpuStep, PreparedDraws};
use crate::renderer::GpuRenderer;
use crate::types::{GpuRenderTarget, PreviewViewport};
use celesta_composition::Scene;

impl GpuRenderer {
    pub(crate) fn encode_draws(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        target: GpuRenderTarget<'_>,
        draws: &PreparedDraws,
    ) -> Result<(), GpuRenderError> {
        let viewport =
            PreviewViewport::fit(scene.width, scene.height, target.width, target.height)?;
        let background = self.options.background.as_wgpu();
        if draws.composite.is_some() {
            self.encode_composited(encoder, scene, draws);
        }
        let kind = if draws.composite.is_some() {
            PipelineKind::Blit
        } else {
            PipelineKind::Layer
        };
        self.ensure_pipeline(target.format, kind);
        let pipeline = &self.pipelines[&(target.format, kind)];
        let mut pass = begin_pass(encoder, target.view, wgpu::LoadOp::Clear(background));
        pass.set_viewport(
            viewport.x,
            viewport.y,
            viewport.width,
            viewport.height,
            0.0,
            1.0,
        );
        pass.set_pipeline(pipeline);
        pass.set_vertex_buffer(0, draws.instances.slice(..));
        pass.set_bind_group(2, &draws.clips, &[]);
        if let Some(plan) = &draws.composite {
            let canvas = self.canvas.as_ref().expect("encode_composited creates it");
            pass.set_bind_group(0, &canvas.bind_group, &[]);
            pass.draw(0..6, plan.blit_instance..plan.blit_instance + 1);
            return Ok(());
        }
        for step in &draws.steps {
            let GpuStep::Draw(draw) = step else {
                unreachable!("a frame with blend steps is composited");
            };
            pass.set_bind_group(0, &draw.texture.bind_group, &[]);
            pass.draw(draw.vertices.clone(), draw.instances.clone());
        }
        Ok(())
    }

    /// Draws a frame that uses blend modes, isolated groups, effects or masks
    /// onto the scene-sized root canvas (each group through a canvas of its
    /// own), leaving the result in `self.canvas` for `encode_draws` to copy
    /// onto the target. Canvases hold premultiplied alpha, which is what
    /// source-over blending onto a cleared texture produces.
    pub(crate) fn encode_composited(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        draws: &PreparedDraws,
    ) {
        const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
        self.prepare_canvases(scene.width, scene.height);
        self.ensure_pipeline(FORMAT, PipelineKind::Layer);
        self.ensure_pipeline(FORMAT, PipelineKind::Blend);
        // Each effect filters its shadow, glow and blur through at most
        // `PASSES_PER_FILTER` passes each.
        let effects = draws
            .steps
            .iter()
            .filter(|step| matches!(step, GpuStep::EndEffect { .. }))
            .count();
        self.effects
            .begin_frame(&self.device, effects * 3 * PASSES_PER_FILTER);
        let root = self.canvas.clone().expect("prepare_canvases creates it");
        let mut compositor = Compositor {
            device: &self.device,
            texture_layout: &self.texture_bind_group_layout,
            layer_pipeline: &self.pipelines[&(FORMAT, PipelineKind::Layer)],
            blend_pipeline: &self.pipelines[&(FORMAT, PipelineKind::Blend)],
            backdrop: self.backdrop.as_ref().expect("prepare_canvases creates it"),
            draws,
            effects: &mut self.effects,
            masks: &self.masks,
        };
        compositor.draw_canvas(
            encoder,
            &root,
            self.options.background.as_wgpu(),
            &draws.steps,
        );
        self.effects.end_frame(&self.queue);
    }
}

impl GpuRenderer {
    pub(crate) fn ensure_pipeline(&mut self, format: wgpu::TextureFormat, kind: PipelineKind) {
        self.pipelines.entry((format, kind)).or_insert_with(|| {
            let layout = match kind {
                PipelineKind::Blend => &self.blend_pipeline_layout,
                PipelineKind::Layer | PipelineKind::Blit => &self.pipeline_layout,
            };
            create_pipeline(&self.device, layout, &self.shader, format, kind)
        });
    }
}
