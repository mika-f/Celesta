use crate::error::GpuRenderError;
use crate::readback::{ReadbackLayout, YuvPlane, YuvReadback};
use crate::types::ReadbackFormat;

/// What a YUV [`ReadbackFormat`] renders: its luma target and entry point,
/// and its chroma targets' format, entry point, and one label each (U and V
/// for the planar formats, UV for the semi-planar ones).
#[derive(Clone, Copy)]
pub(crate) struct YuvShape {
    luma_format: wgpu::TextureFormat,
    luma_entry: &'static str,
    chroma_format: wgpu::TextureFormat,
    chroma_entry: &'static str,
    chroma_labels: &'static [&'static str],
}

const PLANAR: &[&str] = &["Celesta yuv420p U plane", "Celesta yuv420p V plane"];
const SEMI_PLANAR: &[&str] = &["Celesta yuv420p UV plane"];

/// `format`'s [`YuvShape`], or none for an RGBA format. The one list of the
/// YUV formats, which [`ReadbackFormat::is_yuv`] reads.
pub(crate) const fn yuv_shape(format: ReadbackFormat) -> Option<YuvShape> {
    use wgpu::TextureFormat::{R8Unorm, R16Uint, Rg8Unorm, Rg16Uint};
    let (luma_format, luma_entry, chroma_format, chroma_entry, chroma_labels) = match format {
        ReadbackFormat::Yuv420p => (R8Unorm, "luma", R8Unorm, "chroma", PLANAR),
        ReadbackFormat::Yuv420p10 => (R16Uint, "luma10", R16Uint, "chroma10", PLANAR),
        ReadbackFormat::Nv12 => (R8Unorm, "luma", Rg8Unorm, "chroma_nv12", SEMI_PLANAR),
        ReadbackFormat::P010 => (R16Uint, "luma_p010", Rg16Uint, "chroma_p010", SEMI_PLANAR),
        ReadbackFormat::Rgba8 | ReadbackFormat::Rgba16 | ReadbackFormat::Rgba16Straight => {
            return None;
        }
    };
    Some(YuvShape {
        luma_format,
        luma_entry,
        chroma_format,
        chroma_entry,
        chroma_labels,
    })
}

/// The render pipelines behind the YUV [`ReadbackFormat`]s
/// (`yuv420p.wgsl`), one converter each.
pub(crate) struct YuvConverter {
    pub(crate) luma: wgpu::RenderPipeline,
    pub(crate) chroma: wgpu::RenderPipeline,
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    shape: YuvShape,
}

impl YuvConverter {
    /// A converter to the YUV format `shape` describes.
    pub(crate) fn new(device: &wgpu::Device, shape: YuvShape) -> Self {
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
        let chroma_target = vec![target(shape.chroma_format); shape.chroma_labels.len()];
        Self {
            luma: pipeline(
                "Celesta yuv420p luma",
                shape.luma_entry,
                &[target(shape.luma_format)],
            ),
            chroma: pipeline("Celesta yuv420p chroma", shape.chroma_entry, &chroma_target),
            bind_group_layout,
            shape,
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
        let luma = plane(
            "Celesta yuv420p Y plane",
            self.shape.luma_format,
            width,
            height,
        )?;
        let chroma = self
            .shape
            .chroma_labels
            .iter()
            .map(|label| plane(label, self.shape.chroma_format, width / 2, height / 2))
            .collect::<Result<_, _>>()?;
        Ok(YuvReadback {
            bind_group,
            luma,
            chroma,
        })
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
        let (luma, chroma) = (&yuv.luma, &yuv.chroma);
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
