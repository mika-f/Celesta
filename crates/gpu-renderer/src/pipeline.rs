use crate::draw::{LAYER_INSTANCE_ATTRIBUTES, LAYER_INSTANCE_SIZE};

/// How a pipeline writes its fragments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PipelineKind {
    /// Source-over onto the target.
    Layer,
    /// Replaces the target: copies a finished canvas onto it.
    Blit,
    /// Blends with a backdrop copy of the target (`fs_blend`), replacing it.
    Blend,
}

pub(crate) fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    kind: PipelineKind,
) -> wgpu::RenderPipeline {
    let (entry_point, blend) = match kind {
        PipelineKind::Layer => ("fs_main", Some(wgpu::BlendState::ALPHA_BLENDING)),
        PipelineKind::Blit => ("fs_main", None),
        PipelineKind::Blend => ("fs_blend", None),
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Celesta layer pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: LAYER_INSTANCE_SIZE,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &LAYER_INSTANCE_ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
