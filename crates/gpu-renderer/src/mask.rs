use crate::compositor::begin_pass;
use crate::texture::CanvasTexture;
use celesta_composition::{GroupMask, MaskMode};

/// How a masked group's canvas is multiplied by its mask's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MaskSpec {
    pub(crate) mode: MaskMode,
    pub(crate) invert: bool,
}

impl MaskSpec {
    pub(crate) fn new(mask: &GroupMask) -> Self {
        Self {
            mode: mask.mode,
            invert: mask.invert,
        }
    }

    /// The index of this spec's entry point in `ENTRY_POINTS`.
    fn index(self) -> usize {
        let mode = match self.mode {
            MaskMode::Alpha => 0,
            MaskMode::Luminance => 2,
        };
        mode + usize::from(self.invert)
    }
}

/// `mask.wgsl`'s fragment entry points, one per `MaskSpec`.
const ENTRY_POINTS: [&str; 4] = ["alpha", "alpha_inverted", "luminance", "luminance_inverted"];

/// The pass that shows a group's children through its mask.
pub(crate) struct MaskPipelines {
    pipelines: [wgpu::RenderPipeline; 4],
}

impl MaskPipelines {
    /// Reads both canvases through `texture_layout` (`layer.wgsl`'s), so a
    /// canvas's own bind group serves every shader.
    pub(crate) fn new(device: &wgpu::Device, texture_layout: &wgpu::BindGroupLayout) -> Self {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Celesta mask pipeline layout"),
            bind_group_layouts: &[Some(texture_layout), Some(texture_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("mask.wgsl"));
        let pipelines = ENTRY_POINTS.map(|entry_point| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Celesta mask pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
        Self { pipelines }
    }

    /// Writes `children` shown through `matte` onto `target`. All three
    /// cover the same region, so the pass covers the whole target.
    pub(crate) fn apply(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        children: &CanvasTexture,
        matte: &CanvasTexture,
        target: &CanvasTexture,
        spec: MaskSpec,
    ) {
        let mut pass = begin_pass(
            encoder,
            &target.view,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        pass.set_pipeline(&self.pipelines[spec.index()]);
        pass.set_bind_group(0, &children.bind_group, &[]);
        pass.set_bind_group(1, &matte.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
