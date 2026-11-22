use celesta_media::{DecodedMatrix, DecodedPixelFormat, DecodedRange, VideoFrame};
use std::sync::Arc;

pub(crate) struct Plane {
    pub(crate) texture: wgpu::Texture,
    view: wgpu::TextureView,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) channels: u32,
}

struct KeptPlane {
    plane: Arc<Plane>,
    used: bool,
}

pub(crate) struct VideoYuv {
    pipeline: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    planes: Vec<KeptPlane>,
    pub(crate) allocations: u64,
}

impl VideoYuv {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Celesta decoded YUV conversion"),
            source: wgpu::ShaderSource::Wgsl(include_str!("video_yuv.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Celesta decoded YUV conversion"),
            layout: None,
            module: &shader,
            entry_point: Some("convert"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            pipeline,
            params: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Celesta decoded YUV color"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            planes: Vec::new(),
            allocations: 0,
        }
    }

    pub(crate) fn begin_render(&mut self) {
        self.planes
            .retain_mut(|kept| std::mem::take(&mut kept.used));
    }

    pub(crate) fn planes(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: DecodedPixelFormat,
    ) -> Vec<Arc<Plane>> {
        let mut shapes = vec![(width, height, 1)];
        if format == DecodedPixelFormat::Nv12 {
            shapes.push((width.div_ceil(2), height.div_ceil(2), 2));
        } else {
            shapes.extend([(width.div_ceil(2), height.div_ceil(2), 1); 2]);
        }
        shapes
            .into_iter()
            .map(|(width, height, channels)| {
                if let Some(kept) = self.planes.iter_mut().find(|kept| {
                    Arc::strong_count(&kept.plane) == 1
                        && (kept.plane.width, kept.plane.height, kept.plane.channels)
                            == (width, height, channels)
                }) {
                    kept.used = true;
                    return Arc::clone(&kept.plane);
                }
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Celesta decoded video plane"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: if channels == 2 {
                        wgpu::TextureFormat::Rg8Unorm
                    } else {
                        wgpu::TextureFormat::R8Unorm
                    },
                    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let view = texture.create_view(&Default::default());
                let plane = Arc::new(Plane {
                    texture,
                    view,
                    width,
                    height,
                    channels,
                });
                self.planes.push(KeptPlane {
                    plane: Arc::clone(&plane),
                    used: true,
                });
                self.allocations += 1;
                plane
            })
            .collect()
    }

    pub(crate) fn convert(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        planes: &[Arc<Plane>],
        output: &wgpu::Texture,
        frame: &VideoFrame,
    ) {
        let color = frame.color.expect("validated YUV color");
        let params = [
            frame.width,
            frame.height,
            u32::from(frame.pixel_format == DecodedPixelFormat::Nv12),
            0,
            u32::from(color.matrix == DecodedMatrix::Bt709),
            u32::from(color.range == DecodedRange::Full),
            0,
            0,
        ];
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&params));
        let view = output.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Celesta decoded YUV planes"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&planes[0].view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&planes[1].view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &planes.get(2).unwrap_or(&planes[0]).view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.params.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Celesta decoded YUV conversion"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Celesta decoded YUV conversion"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(frame.width.div_ceil(8), frame.height.div_ceil(8), 1);
        }
        queue.submit([encoder.finish()]);
    }
}
