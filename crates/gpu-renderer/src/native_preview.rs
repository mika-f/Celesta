use core_foundation::{
    base::{CFType, TCFType},
    boolean::CFBoolean,
    dictionary::CFDictionary,
    string::CFString,
};
use core_video::{
    metal_texture::{CVMetalTexture, CVMetalTextureGetTexture},
    metal_texture_cache::CVMetalTextureCache,
    pixel_buffer::{
        CVPixelBuffer, CVPixelBufferKeys, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    },
};
use foreign_types::ForeignType;
use objc2::rc::Retained;
use objc2_metal::{MTLTexture, MTLTextureType};

use super::{GpuRenderError, GpuRenderTarget, GpuRenderer, Scene};

pub(super) struct NativePreviewBridge {
    texture_cache: CVMetalTextureCache,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    luma_pipeline: wgpu::RenderPipeline,
    chroma_pipeline: wgpu::RenderPipeline,
}

// The bridge is moved once onto the preview worker and used exclusively there.
// CVMetalTextureCache supports use from arbitrary threads when access is serialized.
unsafe impl Send for NativePreviewBridge {}

impl NativePreviewBridge {
    pub(super) fn new(device: &wgpu::Device) -> Result<Self, ()> {
        let metal_device = unsafe {
            let hal_device = device.as_hal::<wgpu_hal::api::Metal>().ok_or(())?;
            let retained = hal_device.raw_device().clone();
            metal::Device::from_ptr(Retained::into_raw(retained).cast())
        };
        let texture_cache = CVMetalTextureCache::new(None, metal_device, None).map_err(|_| ())?;
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Mikan native preview bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Mikan native preview pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("native_preview.wgsl"));
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Mikan native preview sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let luma_pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            wgpu::TextureFormat::R8Unorm,
            "luma",
        );
        let chroma_pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            wgpu::TextureFormat::Rg8Unorm,
            "chroma",
        );
        Ok(Self {
            texture_cache,
            bind_group_layout,
            sampler,
            luma_pipeline,
            chroma_pipeline,
        })
    }

    pub(super) fn render(
        &mut self,
        renderer: &mut GpuRenderer,
        scene: &Scene,
    ) -> Result<CVPixelBuffer, GpuRenderError> {
        let pixel_buffer = create_pixel_buffer(scene.width, scene.height)?;
        let y = self
            .texture_cache
            .create_texture_from_image(
                pixel_buffer.as_concrete_TypeRef(),
                None,
                metal::MTLPixelFormat::R8Unorm,
                scene.width as usize,
                scene.height as usize,
                0,
            )
            .map_err(GpuRenderError::NativePreview)?;
        let chroma = self
            .texture_cache
            .create_texture_from_image(
                pixel_buffer.as_concrete_TypeRef(),
                None,
                metal::MTLPixelFormat::RG8Unorm,
                (scene.width / 2) as usize,
                (scene.height / 2) as usize,
                1,
            )
            .map_err(GpuRenderError::NativePreview)?;
        let y_texture = wrap_texture(
            &renderer.device,
            y,
            wgpu::TextureFormat::R8Unorm,
            scene.width,
            scene.height,
        )?;
        let chroma_texture = wrap_texture(
            &renderer.device,
            chroma,
            wgpu::TextureFormat::Rg8Unorm,
            scene.width / 2,
            scene.height / 2,
        )?;

        let rgba = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Mikan native preview RGBA intermediate"),
            size: wgpu::Extent3d {
                width: scene.width,
                height: scene.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let rgba_view = rgba.create_view(&wgpu::TextureViewDescriptor::default());
        let y_view = y_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let chroma_view = chroma_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let draws = renderer.prepare_draws(scene)?;
        let mut encoder = renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Mikan native preview commands"),
            });
        renderer.encode_draws(
            &mut encoder,
            scene,
            GpuRenderTarget {
                view: &rgba_view,
                format: wgpu::TextureFormat::Rgba8Unorm,
                width: scene.width,
                height: scene.height,
            },
            &draws,
        )?;
        let bind_group = renderer
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Mikan native preview bind group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&rgba_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
        self.encode_plane(&mut encoder, &bind_group, &y_view, &self.luma_pipeline);
        self.encode_plane(
            &mut encoder,
            &bind_group,
            &chroma_view,
            &self.chroma_pipeline,
        );
        renderer.queue.submit([encoder.finish()]);
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(GpuRenderError::Poll)?;
        Ok(pixel_buffer)
    }

    fn encode_plane(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        view: &wgpu::TextureView,
        pipeline: &wgpu::RenderPipeline,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Mikan native preview conversion pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    entry_point: &'static str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Mikan native preview conversion pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(entry_point),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_pixel_buffer(width: u32, height: u32) -> Result<CVPixelBuffer, GpuRenderError> {
    let empty = CFDictionary::<CFString, CFType>::from_CFType_pairs(&[]);
    let options: CFDictionary<CFString, CFType> = CFDictionary::from_CFType_pairs(&[
        (
            CFString::from(CVPixelBufferKeys::IOSurfaceProperties),
            empty.as_CFType(),
        ),
        (
            CFString::from(CVPixelBufferKeys::MetalCompatibility),
            CFBoolean::true_value().as_CFType(),
        ),
    ]);
    CVPixelBuffer::new(
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        width as usize,
        height as usize,
        Some(&options),
    )
    .map_err(GpuRenderError::NativePreview)
}

fn wrap_texture(
    device: &wgpu::Device,
    texture: CVMetalTexture,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture, GpuRenderError> {
    let raw_texture = unsafe { CVMetalTextureGetTexture(texture.as_concrete_TypeRef()) };
    if raw_texture.is_null() {
        return Err(GpuRenderError::NativePreview(-1));
    }
    let retained = unsafe {
        Retained::retain(raw_texture.cast::<objc2::runtime::ProtocolObject<dyn MTLTexture>>())
            .expect("CoreVideo returned a null Metal texture")
    };
    let owner = CoreVideoTextureOwner { _texture: texture };
    let raw = unsafe {
        wgpu_hal::metal::Device::texture_from_raw(
            retained,
            format,
            MTLTextureType::Type2D,
            1,
            1,
            wgpu_hal::CopyExtent {
                width,
                height,
                depth: 1,
            },
            Some(Box::new(move || drop(owner))),
        )
    };
    Ok(unsafe {
        device.create_texture_from_hal::<wgpu_hal::api::Metal>(
            raw,
            &wgpu::TextureDescriptor {
                label: Some("Mikan CoreVideo plane"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            },
            wgpu::wgt::TextureUses::COLOR_TARGET,
        )
    })
}

// wgpu may defer external texture destruction to another thread. CoreVideo's
// reference-counted texture can be released there as long as nobody mutates it.
struct CoreVideoTextureOwner {
    _texture: CVMetalTexture,
}

unsafe impl Send for CoreVideoTextureOwner {}
unsafe impl Sync for CoreVideoTextureOwner {}
