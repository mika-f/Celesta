use crate::error::GpuRenderError;
use crate::readback::{ReadbackLayout, YuvPlane, YuvReadback};
use crate::types::ReadbackFormat;

/// The render pipelines behind the YUV [`ReadbackFormat`]s
/// (`yuv420p.wgsl`), one converter each.
pub(crate) struct YuvConverter {
    pub(crate) luma: wgpu::RenderPipeline,
    pub(crate) chroma: wgpu::RenderPipeline,
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    /// `R8Unorm` for 8-bit luma, `R16Uint` for 10-bit.
    luma_format: wgpu::TextureFormat,
    /// What each chroma plane is: `R8Unorm` / `R16Uint` for the planar
    /// formats (two planes), `Rg8Unorm` / `Rg16Uint` for the semi-planar
    /// ones (one interleaved plane).
    chroma_format: wgpu::TextureFormat,
    chroma_targets: usize,
}

impl YuvConverter {
    /// A converter to `format`, which must be a YUV format.
    pub(crate) fn new(device: &wgpu::Device, format: ReadbackFormat) -> Self {
        use wgpu::TextureFormat::{R8Unorm, R16Uint, Rg8Unorm, Rg16Uint};
        let (luma_format, luma, chroma_format, chroma, chroma_targets) = match format {
            ReadbackFormat::Yuv420p => (R8Unorm, "luma", R8Unorm, "chroma", 2),
            ReadbackFormat::Yuv420p10 => (R16Uint, "luma10", R16Uint, "chroma10", 2),
            ReadbackFormat::Nv12 => (R8Unorm, "luma", Rg8Unorm, "chroma_nv12", 1),
            ReadbackFormat::P010 => (R16Uint, "luma_p010", Rg16Uint, "chroma_p010", 1),
            _ => unreachable!("{format:?} is not a YUV format"),
        };
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta yuv420p bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Celesta yuv420p pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Celesta yuv420p shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(include_str!("dither.wgsl"), include_str!("yuv420p.wgsl")).into(),
            ),
        });
        let target = |format| {
            Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })
        };
        let pipeline = |label, entry_point, targets: &[Option<wgpu::ColorTargetState>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
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
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let chroma_target = vec![target(chroma_format); chroma_targets];
        Self {
            luma: pipeline("Celesta yuv420p luma", luma, &[target(luma_format)]),
            chroma: pipeline("Celesta yuv420p chroma", chroma, &chroma_target),
            bind_group_layout,
            luma_format,
            chroma_format,
            chroma_targets,
        }
    }

    /// What `encode` converts `frame`, a frame composited in
    /// `canvas_format`, through.
    pub(crate) fn frame_bind_group(
        &self,
        device: &wgpu::Device,
        frame: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Celesta yuv420p bind group"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(frame),
            }],
        })
    }

    /// The plane textures and bind group converting one `width`x`height`
    /// slot whose frame, composited in `canvas_format`, is `frame`.
    pub(crate) fn readback(
        &self,
        device: &wgpu::Device,
        frame: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<YuvReadback, GpuRenderError> {
        let bind_group = self.frame_bind_group(device, frame);
        let mut offset = 0;
        let mut plane = |label,
                         format: wgpu::TextureFormat,
                         width: u32,
                         height: u32|
         -> Result<YuvPlane, GpuRenderError> {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bytes_per_sample = format.block_copy_size(None).unwrap_or(1);
            let layout = ReadbackLayout::with_bytes_per_pixel(width, height, bytes_per_sample)?;
            // Each plane's size is a whole number of aligned rows, so every
            // plane starts at an offset the copy accepts.
            let plane_offset = offset;
            offset += layout.buffer_size;
            Ok(YuvPlane {
                texture,
                view,
                height,
                layout,
                offset: plane_offset,
            })
        };
        let mut planes = vec![plane(
            "Celesta yuv420p Y plane",
            self.luma_format,
            width,
            height,
        )?];
        for index in 0..self.chroma_targets {
            let label = match (self.chroma_targets, index) {
                (1, _) => "Celesta yuv420p UV plane",
                (_, 0) => "Celesta yuv420p U plane",
                _ => "Celesta yuv420p V plane",
            };
            planes.push(plane(label, self.chroma_format, width / 2, height / 2)?);
        }
        Ok(YuvReadback { bind_group, planes })
    }

    /// Renders the frame `frame` binds (`frame_bind_group`; the slot's own
    /// is `yuv.bind_group`) into the slot's luma and chroma planes.
    pub(crate) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        yuv: &YuvReadback,
        frame: &wgpu::BindGroup,
    ) {
        fn attachment(plane: &YuvPlane) -> Option<wgpu::RenderPassColorAttachment<'_>> {
            Some(wgpu::RenderPassColorAttachment {
                view: &plane.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })
        }
        let (luma, chroma) = yuv.planes.split_first().expect("a slot has planes");
        for (label, pipeline, targets) in [
            (
                "Celesta yuv420p luma pass",
                &self.luma,
                vec![attachment(luma)],
            ),
            (
                "Celesta yuv420p chroma pass",
                &self.chroma,
                chroma.iter().map(attachment).collect(),
            ),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &targets,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, frame, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
