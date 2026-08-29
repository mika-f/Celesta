//! GPU rendering foundation built on `wgpu`.
//!
//! It supports offscreen image, video, and text composition with nested
//! transforms, opacity, painter ordering, and deterministic RGBA readback.
//! Unsupported content returns an error instead of silently disappearing.

use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

use image::ImageReader;
use mikan_composition::{
    AssetLocation, EvaluatedTransform, Layer, LayerContent, Point, ResolvedAsset, Scene,
};
use mikan_media::{MediaError, VideoFrameDecoder};
use mikan_renderer::{RenderError, TextRasterizer, rasterize_rect};
use wgpu::util::DeviceExt;

#[cfg(target_os = "macos")]
mod native_preview;

const BYTES_PER_PIXEL: u32 = 4;

/// How many frames `GpuRenderer::submit` keeps in flight before it blocks to
/// reclaim the oldest one. Each `submit` call only blocks on GPU readback
/// once this many frames are outstanding, so the caller's own per-frame work
/// (Node IPC for a React entry, encoding the previous frame to FFmpeg, ...)
/// overlaps with the GPU actually rendering, instead of a full submit-wait
/// round trip serializing every frame.
const PIPELINE_DEPTH: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);

    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    fn as_wgpu(self) -> wgpu::Color {
        wgpu::Color {
            r: f64::from(self.red) / 255.0,
            g: f64::from(self.green) / 255.0,
            b: f64::from(self.blue) / 255.0,
            a: f64::from(self.alpha) / 255.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuRenderOptions {
    pub background: Color,
}

impl Default for GpuRenderOptions {
    fn default() -> Self {
        Self {
            background: Color::rgba(20, 22, 28, 255),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuFrame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

pub enum PreviewFrame {
    Cpu(GpuFrame),
    #[cfg(target_os = "macos")]
    Native(NativePreviewFrame),
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePreviewFrame(core_video::pixel_buffer::CVPixelBuffer);

#[cfg(target_os = "macos")]
impl NativePreviewFrame {
    pub fn pixel_buffer(&self) -> core_video::pixel_buffer::CVPixelBuffer {
        self.0.clone()
    }
}

// CVPixelBuffer is an immutable, reference-counted CoreVideo object once handed
// to the UI. CoreVideo permits pixel buffers to cross thread boundaries.
#[cfg(target_os = "macos")]
unsafe impl Send for NativePreviewFrame {}

impl GpuFrame {
    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }
}

pub struct GpuRenderTarget<'a> {
    pub view: &'a wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewFrameStatus {
    Presented,
    PresentedSuboptimal,
    SkippedTimeout,
    SkippedOccluded,
    Reconfigure,
    SurfaceLost,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewViewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PreviewViewport {
    pub fn fit(
        scene_width: u32,
        scene_height: u32,
        target_width: u32,
        target_height: u32,
    ) -> Result<Self, GpuRenderError> {
        if scene_width == 0 || scene_height == 0 {
            return Err(GpuRenderError::InvalidSurfaceSize {
                width: scene_width,
                height: scene_height,
            });
        }
        if target_width == 0 || target_height == 0 {
            return Err(GpuRenderError::InvalidTargetSize {
                width: target_width,
                height: target_height,
            });
        }
        let scale = (target_width as f32 / scene_width as f32)
            .min(target_height as f32 / scene_height as f32);
        let width = scene_width as f32 * scale;
        let height = scene_height as f32 * scale;
        Ok(Self {
            x: (target_width as f32 - width) / 2.0,
            y: (target_height as f32 - height) / 2.0,
            width,
            height,
        })
    }
}

pub struct GpuRenderer {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_info: wgpu::AdapterInfo,
    options: GpuRenderOptions,
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    asset_root: PathBuf,
    images: HashMap<String, DecodedImage>,
    video_decoder: Option<Box<dyn VideoFrameDecoder>>,
    text_rasterizer: TextRasterizer,
    #[cfg(target_os = "macos")]
    native_preview: Option<native_preview::NativePreviewBridge>,
    /// Ring of reusable offscreen texture/readback-buffer pairs behind
    /// `submit`/`drain`. Indices not currently rendering or awaiting readback
    /// sit in `readback_free`; in-flight ones are queued in `readback_order`
    /// (submission order, so `drain`/reclaim always returns frames in order).
    readback_slots: Vec<ReadbackSlot>,
    readback_order: VecDeque<usize>,
    readback_free: Vec<usize>,
}

impl GpuRenderer {
    pub fn new(options: GpuRenderOptions) -> Result<Self, GpuRenderError> {
        pollster::block_on(Self::request(options))
    }

    pub fn new_for_surface(
        options: GpuRenderOptions,
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<Self, GpuRenderError> {
        pollster::block_on(Self::request_for_surface(options, instance, surface))
    }

    pub async fn request(options: GpuRenderOptions) -> Result<Self, GpuRenderError> {
        let instance = wgpu::Instance::default();
        Self::request_compatible(options, &instance, None).await
    }

    pub async fn request_for_surface(
        options: GpuRenderOptions,
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<Self, GpuRenderError> {
        Self::request_compatible(options, instance, Some(surface)).await
    }

    async fn request_compatible(
        options: GpuRenderOptions,
        instance: &wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, GpuRenderError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface,
                ..Default::default()
            })
            .await
            .map_err(GpuRenderError::RequestAdapter)?;
        let adapter_info = adapter.get_info();
        let descriptor = wgpu::DeviceDescriptor {
            label: Some("Mikan GPU Renderer"),
            ..Default::default()
        };
        let (device, queue) = adapter
            .request_device(&descriptor)
            .await
            .map_err(GpuRenderError::RequestDevice)?;
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Mikan layer bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Mikan layer pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("layer.wgsl"));
        let pipeline = create_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let mut pipelines = HashMap::new();
        pipelines.insert(wgpu::TextureFormat::Rgba8Unorm, pipeline);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Mikan layer sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        #[cfg(target_os = "macos")]
        let native_preview = native_preview::NativePreviewBridge::new(&device).ok();
        Ok(Self {
            adapter,
            device,
            queue,
            adapter_info,
            options,
            shader,
            pipeline_layout,
            pipelines,
            bind_group_layout,
            sampler,
            asset_root: PathBuf::from("."),
            images: HashMap::new(),
            video_decoder: None,
            text_rasterizer: TextRasterizer::new(),
            #[cfg(target_os = "macos")]
            native_preview,
            readback_slots: Vec::new(),
            readback_order: VecDeque::new(),
            readback_free: Vec::new(),
        })
    }

    pub const fn options(&self) -> GpuRenderOptions {
        self.options
    }

    pub const fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    pub fn with_asset_root(mut self, asset_root: impl Into<PathBuf>) -> Self {
        self.asset_root = asset_root.into();
        self
    }

    pub fn set_asset_root(&mut self, asset_root: impl Into<PathBuf>) {
        self.asset_root = asset_root.into();
    }

    pub fn with_video_decoder(mut self, decoder: impl VideoFrameDecoder + 'static) -> Self {
        self.video_decoder = Some(Box::new(decoder));
        self
    }

    pub fn configure_surface(
        &self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
    ) -> Result<wgpu::SurfaceConfiguration, GpuRenderError> {
        if width == 0 || height == 0 {
            return Err(GpuRenderError::InvalidTargetSize { width, height });
        }
        let mut configuration = surface
            .get_default_config(&self.adapter, width, height)
            .ok_or(GpuRenderError::IncompatibleSurface)?;
        let view_format = configuration.format.remove_srgb_suffix();
        if view_format != configuration.format {
            configuration.view_formats.push(view_format);
        }
        surface.configure(&self.device, &configuration);
        Ok(configuration)
    }

    pub fn render_to_target(
        &mut self,
        scene: &Scene,
        target: GpuRenderTarget<'_>,
    ) -> Result<(), GpuRenderError> {
        let draws = self.prepare_draws(scene)?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Mikan preview commands"),
            });
        self.encode_draws(&mut encoder, scene, target, &draws)?;
        self.queue.submit([encoder.finish()]);
        Ok(())
    }

    pub fn render_to_surface(
        &mut self,
        scene: &Scene,
        surface: &wgpu::Surface<'_>,
    ) -> Result<PreviewFrameStatus, GpuRenderError> {
        let configuration = surface
            .get_configuration()
            .ok_or(GpuRenderError::SurfaceNotConfigured)?;
        let (surface_texture, status) = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => {
                (texture, PreviewFrameStatus::Presented)
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                (texture, PreviewFrameStatus::PresentedSuboptimal)
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                return Ok(PreviewFrameStatus::SkippedTimeout);
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(PreviewFrameStatus::SkippedOccluded);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                return Ok(PreviewFrameStatus::Reconfigure);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                return Ok(PreviewFrameStatus::SurfaceLost);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(GpuRenderError::SurfaceValidation);
            }
        };
        let view_format = configuration.format.remove_srgb_suffix();
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                format: Some(view_format),
                ..Default::default()
            });
        self.render_to_target(
            scene,
            GpuRenderTarget {
                view: &view,
                format: view_format,
                width: configuration.width,
                height: configuration.height,
            },
        )?;
        self.queue.present(surface_texture);
        Ok(status)
    }

    pub fn render(&mut self, scene: &Scene) -> Result<GpuFrame, GpuRenderError> {
        if scene.width == 0 || scene.height == 0 {
            return Err(GpuRenderError::InvalidSurfaceSize {
                width: scene.width,
                height: scene.height,
            });
        }

        let draws = self.prepare_draws(scene)?;

        let layout = ReadbackLayout::new(scene.width, scene.height)?;
        let size = wgpu::Extent3d {
            width: scene.width,
            height: scene.height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Mikan offscreen frame"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mikan RGBA readback"),
            size: layout.buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Mikan offscreen commands"),
            });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.encode_draws(
            &mut encoder,
            scene,
            GpuRenderTarget {
                view: &view,
                format: wgpu::TextureFormat::Rgba8Unorm,
                width: scene.width,
                height: scene.height,
            },
            &draws,
        )?;
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(layout.padded_bytes_per_row),
                    rows_per_image: Some(scene.height),
                },
            },
            size,
        );
        self.queue.submit([encoder.finish()]);

        let slice = output.slice(..);
        let (sender, receiver) = mpsc::sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(GpuRenderError::Poll)?;
        receiver
            .recv()
            .map_err(|_| GpuRenderError::MapCallbackDropped)?
            .map_err(GpuRenderError::Map)?;

        let mapped = slice.get_mapped_range().map_err(GpuRenderError::MapRange)?;
        let capacity = usize::try_from(layout.unpadded_bytes_per_row)
            .ok()
            .and_then(|row| row.checked_mul(scene.height as usize))
            .ok_or(GpuRenderError::SurfaceTooLarge {
                width: scene.width,
                height: scene.height,
            })?;
        let mut pixels = Vec::with_capacity(capacity);
        for row in mapped.chunks_exact(layout.padded_bytes_per_row as usize) {
            pixels.extend_from_slice(&row[..layout.unpadded_bytes_per_row as usize]);
        }
        drop(mapped);
        output.unmap();

        Ok(GpuFrame {
            width: scene.width,
            height: scene.height,
            pixels,
        })
    }

    /// Encodes and submits `scene` for GPU rendering without blocking for its
    /// pixels, for batch callers (frame-exact export) that render many
    /// scenes back to back. Up to [`PIPELINE_DEPTH`] frames are kept
    /// in flight at once, reusing their texture/readback-buffer pair rather
    /// than allocating fresh ones every call like `render` does; once that
    /// many are outstanding, this blocks to reclaim the oldest one (in
    /// submission order) and returns its now-ready pixels — by then, the GPU
    /// has usually already finished rendering it while the caller was busy
    /// evaluating/encoding other frames, so the wait is short or free.
    /// Call `drain` after the last `submit` to collect the remaining
    /// in-flight frames.
    pub fn submit(&mut self, scene: &Scene) -> Result<Option<GpuFrame>, GpuRenderError> {
        if scene.width == 0 || scene.height == 0 {
            return Err(GpuRenderError::InvalidSurfaceSize {
                width: scene.width,
                height: scene.height,
            });
        }
        let draws = self.prepare_draws(scene)?;

        let ready = if self.readback_free.is_empty() && self.readback_order.len() >= PIPELINE_DEPTH
        {
            Some(self.reclaim_oldest()?)
        } else {
            None
        };

        let slot_index = match self.readback_free.pop() {
            Some(index) => {
                if self.readback_slots[index].width != scene.width
                    || self.readback_slots[index].height != scene.height
                {
                    self.readback_slots[index] =
                        ReadbackSlot::new(&self.device, scene.width, scene.height)?;
                }
                index
            }
            None => {
                self.readback_slots.push(ReadbackSlot::new(
                    &self.device,
                    scene.width,
                    scene.height,
                )?);
                self.readback_slots.len() - 1
            }
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Mikan pipelined offscreen commands"),
            });
        // Cloned out first (a cheap handle clone, not a data copy) so it
        // doesn't keep `self.readback_slots` borrowed across the
        // `self.encode_draws` call below, which itself needs `&mut self`.
        let slot_view = self.readback_slots[slot_index].view.clone();
        self.encode_draws(
            &mut encoder,
            scene,
            GpuRenderTarget {
                view: &slot_view,
                format: wgpu::TextureFormat::Rgba8Unorm,
                width: scene.width,
                height: scene.height,
            },
            &draws,
        )?;
        {
            let slot = &self.readback_slots[slot_index];
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &slot.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &slot.buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(slot.layout.padded_bytes_per_row),
                        rows_per_image: Some(scene.height),
                    },
                },
                wgpu::Extent3d {
                    width: scene.width,
                    height: scene.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.queue.submit([encoder.finish()]);

        let slot = &mut self.readback_slots[slot_index];
        let (sender, receiver) = mpsc::sync_channel(1);
        slot.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        slot.pending = Some(receiver);
        self.readback_order.push_back(slot_index);

        Ok(ready)
    }

    /// Blocks until every frame `submit` has queued but not yet returned is
    /// ready, in the order they were submitted.
    pub fn drain(&mut self) -> Result<Vec<GpuFrame>, GpuRenderError> {
        let mut frames = Vec::with_capacity(self.readback_order.len());
        while !self.readback_order.is_empty() {
            frames.push(self.reclaim_oldest()?);
        }
        Ok(frames)
    }

    /// Blocks on the oldest in-flight slot's readback, frees it for reuse,
    /// and returns its pixels.
    fn reclaim_oldest(&mut self) -> Result<GpuFrame, GpuRenderError> {
        let slot_index = self
            .readback_order
            .pop_front()
            .expect("reclaim_oldest called with no in-flight frame");
        let frame = {
            let slot = &mut self.readback_slots[slot_index];
            let receiver = slot
                .pending
                .take()
                .expect("in-flight slot always has a pending readback");
            self.device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(GpuRenderError::Poll)?;
            receiver
                .recv()
                .map_err(|_| GpuRenderError::MapCallbackDropped)?
                .map_err(GpuRenderError::Map)?;

            let slice = slot.buffer.slice(..);
            let mapped = slice.get_mapped_range().map_err(GpuRenderError::MapRange)?;
            let capacity = usize::try_from(slot.layout.unpadded_bytes_per_row)
                .ok()
                .and_then(|row| row.checked_mul(slot.height as usize))
                .ok_or(GpuRenderError::SurfaceTooLarge {
                    width: slot.width,
                    height: slot.height,
                })?;
            let mut pixels = Vec::with_capacity(capacity);
            for row in mapped.chunks_exact(slot.layout.padded_bytes_per_row as usize) {
                pixels.extend_from_slice(&row[..slot.layout.unpadded_bytes_per_row as usize]);
            }
            drop(mapped);
            slot.buffer.unmap();
            GpuFrame {
                width: slot.width,
                height: slot.height,
                pixels,
            }
        };
        self.readback_free.push(slot_index);
        Ok(frame)
    }

    pub fn render_preview(&mut self, scene: &Scene) -> Result<PreviewFrame, GpuRenderError> {
        #[cfg(target_os = "macos")]
        if scene.width.is_multiple_of(2)
            && scene.height.is_multiple_of(2)
            && let Some(mut bridge) = self.native_preview.take()
        {
            let frame = bridge.render(self, scene);
            self.native_preview = Some(bridge);
            if let Ok(frame) = frame {
                return Ok(PreviewFrame::Native(NativePreviewFrame(frame)));
            }
        }

        self.render(scene).map(PreviewFrame::Cpu)
    }

    fn prepare_draws(&mut self, scene: &Scene) -> Result<Vec<GpuDraw>, GpuRenderError> {
        if scene.width == 0 || scene.height == 0 {
            return Err(GpuRenderError::InvalidSurfaceSize {
                width: scene.width,
                height: scene.height,
            });
        }
        self.text_rasterizer
            .load_fonts(&scene.fonts, &self.asset_root)
            .map_err(GpuRenderError::Text)?;
        let mut layers = Vec::new();
        for layer in &scene.layers {
            self.prepare_layer(layer, LayerState::default(), &mut layers)?;
        }
        layers
            .iter()
            .map(|layer| self.create_draw(layer, scene.width, scene.height))
            .collect()
    }

    fn encode_draws(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        target: GpuRenderTarget<'_>,
        draws: &[GpuDraw],
    ) -> Result<(), GpuRenderError> {
        let viewport =
            PreviewViewport::fit(scene.width, scene.height, target.width, target.height)?;
        let background = self.options.background.as_wgpu();
        let pipeline = self.pipeline_for(target.format);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Mikan layer pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(background),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(
            viewport.x,
            viewport.y,
            viewport.width,
            viewport.height,
            0.0,
            1.0,
        );
        pass.set_pipeline(pipeline);
        for draw in draws {
            pass.set_bind_group(0, &draw.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
        Ok(())
    }

    fn pipeline_for(&mut self, format: wgpu::TextureFormat) -> &wgpu::RenderPipeline {
        self.pipelines.entry(format).or_insert_with(|| {
            create_pipeline(&self.device, &self.pipeline_layout, &self.shader, format)
        })
    }

    fn prepare_layer(
        &mut self,
        layer: &Layer,
        parent: LayerState,
        output: &mut Vec<PreparedLayer>,
    ) -> Result<(), GpuRenderError> {
        let state = parent.then(&layer.transform, layer.opacity);
        if state.opacity == 0.0 || state.transform.is_degenerate() {
            return Ok(());
        }
        match &layer.content {
            LayerContent::Group { layers } => {
                for child in layers {
                    self.prepare_layer(child, state, output)?;
                }
            }
            LayerContent::Image { asset } => {
                let image = self.load_image(asset)?.clone();
                output.push(PreparedLayer::new(image, layer.transform.anchor, state));
            }
            LayerContent::Psd {
                asset,
                enabled_layers,
                disabled_layers,
            } => {
                let image = self
                    .load_psd(asset, enabled_layers, disabled_layers)?
                    .clone();
                output.push(PreparedLayer::new(image, layer.transform.anchor, state));
            }
            LayerContent::Video { asset, timing } => {
                let path = self.local_asset_path(asset)?;
                let decoder = self
                    .video_decoder
                    .as_mut()
                    .ok_or_else(|| GpuRenderError::MissingVideoDecoder(layer.id.clone()))?;
                let frame =
                    decoder.decode_frame_for(&layer.id, &path, timing.source_time_seconds)?;
                let image = DecodedImage::new(frame.width, frame.height, frame.pixels)?;
                output.push(PreparedLayer::new(image, layer.transform.anchor, state));
            }
            LayerContent::Text {
                text,
                style,
                max_width,
            } => {
                let text = self
                    .text_rasterizer
                    .rasterize(text, style, *max_width, 1.0)
                    .map_err(GpuRenderError::Text)?;
                let image = DecodedImage::new(text.width(), text.height(), text.into_pixels())?;
                output.push(PreparedLayer::new(image, layer.transform.anchor, state));
            }
            LayerContent::Rect {
                width,
                height,
                fill,
                stroke,
                corner_radius,
            } => {
                let rect = rasterize_rect(
                    *width,
                    *height,
                    *corner_radius,
                    fill.as_ref(),
                    stroke.as_ref(),
                )
                .map_err(GpuRenderError::Text)?;
                let image = DecodedImage::new(rect.width(), rect.height(), rect.into_pixels())?;
                output.push(PreparedLayer::new(image, layer.transform.anchor, state));
            }
            LayerContent::MissingComponent { .. } => {
                return Err(GpuRenderError::UnsupportedContent {
                    layer: layer.id.clone(),
                    content: "missing component",
                });
            }
        }
        Ok(())
    }

    fn load_image(&mut self, asset: &ResolvedAsset) -> Result<&DecodedImage, GpuRenderError> {
        if !self.images.contains_key(&asset.id) {
            let path = self.local_asset_path(asset)?;
            let image = ImageReader::open(&path)
                .map_err(|source| GpuRenderError::AssetIo {
                    asset: asset.id.clone(),
                    source,
                })?
                .decode()
                .map_err(|source| GpuRenderError::ImageDecode {
                    asset: asset.id.clone(),
                    source,
                })?
                .to_rgba8();
            let image = DecodedImage::new(image.width(), image.height(), image.into_raw())?;
            self.images.insert(asset.id.clone(), image);
        }
        Ok(self.images.get(&asset.id).expect("image was cached"))
    }

    fn load_psd(
        &mut self,
        asset: &ResolvedAsset,
        enabled_layers: &[String],
        disabled_layers: &[String],
    ) -> Result<&DecodedImage, GpuRenderError> {
        let key = format!(
            "psd\0{}\0{}\0{}",
            asset.id,
            enabled_layers.join("\0"),
            disabled_layers.join("\0")
        );
        if !self.images.contains_key(&key) {
            let path = self.local_asset_path(asset)?;
            let frame =
                mikan_renderer::rasterize_psd(&asset.id, &path, enabled_layers, disabled_layers)
                    .map_err(GpuRenderError::Psd)?;
            let image = DecodedImage::new(frame.width(), frame.height(), frame.pixels().to_vec())?;
            self.images.insert(key.clone(), image);
        }
        Ok(self.images.get(&key).expect("PSD image was cached"))
    }

    fn local_asset_path(&self, asset: &ResolvedAsset) -> Result<PathBuf, GpuRenderError> {
        match &asset.location {
            AssetLocation::File { path } => {
                let path = Path::new(path);
                Ok(if path.is_absolute() {
                    path.to_owned()
                } else {
                    self.asset_root.join(path)
                })
            }
            AssetLocation::Url { url } => Err(GpuRenderError::UnsupportedAssetUrl {
                asset: asset.id.clone(),
                url: url.clone(),
            }),
        }
    }

    fn create_draw(
        &self,
        layer: &PreparedLayer,
        canvas_width: u32,
        canvas_height: u32,
    ) -> Result<GpuDraw, GpuRenderError> {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Mikan layer texture"),
            size: wgpu::Extent3d {
                width: layer.image.width,
                height: layer.image.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &layer.image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(layer.image.width * BYTES_PER_PIXEL),
                rows_per_image: Some(layer.image.height),
            },
            wgpu::Extent3d {
                width: layer.image.width,
                height: layer.image.height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let uniform = layer.uniform(canvas_width, canvas_height);
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Mikan layer uniform"),
                contents: &uniform,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Mikan layer bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        Ok(GpuDraw {
            _texture: texture,
            _view: view,
            _uniform: uniform,
            bind_group,
        })
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Mikan layer pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[derive(Clone)]
struct DecodedImage {
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
}

impl DecodedImage {
    fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, GpuRenderError> {
        let expected = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(u64::from(BYTES_PER_PIXEL)))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or(GpuRenderError::SurfaceTooLarge { width, height })?;
        if width == 0 || height == 0 || pixels.len() != expected {
            return Err(GpuRenderError::InvalidImageData {
                width,
                height,
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            pixels: pixels.into(),
        })
    }
}

struct PreparedLayer {
    image: DecodedImage,
    anchor: Point,
    state: LayerState,
}

impl PreparedLayer {
    const fn new(image: DecodedImage, anchor: Point, state: LayerState) -> Self {
        Self {
            image,
            anchor,
            state,
        }
    }

    fn uniform(&self, canvas_width: u32, canvas_height: u32) -> Vec<u8> {
        let values = [
            self.state.transform.a,
            self.state.transform.b,
            self.state.transform.c,
            self.state.transform.d,
            self.state.transform.tx,
            self.state.transform.ty,
            self.image.width as f32,
            self.image.height as f32,
            self.anchor.x as f32,
            self.anchor.y as f32,
            self.state.opacity,
            0.0,
            canvas_width as f32,
            canvas_height as f32,
            0.0,
            0.0,
        ];
        values.into_iter().flat_map(f32::to_ne_bytes).collect()
    }
}

struct GpuDraw {
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    _uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

#[derive(Clone, Copy)]
struct LayerState {
    transform: Affine,
    opacity: f32,
}

impl LayerState {
    fn then(self, transform: &EvaluatedTransform, opacity: f64) -> Self {
        Self {
            transform: self.transform.multiply(Affine::from_transform(transform)),
            opacity: (self.opacity * opacity.clamp(0.0, 1.0) as f32).clamp(0.0, 1.0),
        }
    }
}

impl Default for LayerState {
    fn default() -> Self {
        Self {
            transform: Affine::IDENTITY,
            opacity: 1.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Affine {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    tx: f32,
    ty: f32,
}

impl Affine {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    fn from_transform(transform: &EvaluatedTransform) -> Self {
        let radians = transform.rotation.to_radians() as f32;
        let (sin, cos) = radians.sin_cos();
        let scale_x = transform.scale.x as f32;
        let scale_y = transform.scale.y as f32;
        Self {
            a: cos * scale_x,
            b: sin * scale_x,
            c: -sin * scale_y,
            d: cos * scale_y,
            tx: transform.position.x as f32,
            ty: transform.position.y as f32,
        }
    }

    fn multiply(self, child: Self) -> Self {
        Self {
            a: self.a * child.a + self.c * child.b,
            b: self.b * child.a + self.d * child.b,
            c: self.a * child.c + self.c * child.d,
            d: self.b * child.c + self.d * child.d,
            tx: self.a * child.tx + self.c * child.ty + self.tx,
            ty: self.b * child.tx + self.d * child.ty + self.ty,
        }
    }

    fn is_degenerate(self) -> bool {
        (self.a * self.d - self.b * self.c).abs() <= f32::EPSILON
    }
}

struct ReadbackLayout {
    unpadded_bytes_per_row: u32,
    padded_bytes_per_row: u32,
    buffer_size: u64,
}

impl ReadbackLayout {
    fn new(width: u32, height: u32) -> Result<Self, GpuRenderError> {
        let unpadded_bytes_per_row = width
            .checked_mul(BYTES_PER_PIXEL)
            .ok_or(GpuRenderError::SurfaceTooLarge { width, height })?;
        let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row
            .checked_add(alignment - 1)
            .map(|value| value / alignment * alignment)
            .ok_or(GpuRenderError::SurfaceTooLarge { width, height })?;
        let buffer_size = u64::from(padded_bytes_per_row)
            .checked_mul(u64::from(height))
            .ok_or(GpuRenderError::SurfaceTooLarge { width, height })?;
        Ok(Self {
            unpadded_bytes_per_row,
            padded_bytes_per_row,
            buffer_size,
        })
    }
}

/// One reusable texture/readback-buffer pair behind `GpuRenderer::submit`'s
/// ring, plus the receiver for its currently outstanding `map_async` call
/// (`None` when the slot is free).
struct ReadbackSlot {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    layout: ReadbackLayout,
    pending: Option<mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>,
}

impl ReadbackSlot {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Result<Self, GpuRenderError> {
        let layout = ReadbackLayout::new(width, height)?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Mikan pipelined offscreen frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mikan pipelined RGBA readback"),
            size: layout.buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Self {
            texture,
            view,
            buffer,
            width,
            height,
            layout,
            pending: None,
        })
    }
}

#[derive(Debug)]
pub enum GpuRenderError {
    RequestAdapter(wgpu::RequestAdapterError),
    RequestDevice(wgpu::RequestDeviceError),
    Poll(wgpu::PollError),
    Map(wgpu::BufferAsyncError),
    MapRange(wgpu::MapRangeError),
    MapCallbackDropped,
    AssetIo {
        asset: String,
        source: io::Error,
    },
    ImageDecode {
        asset: String,
        source: image::ImageError,
    },
    Psd(RenderError),
    Media(MediaError),
    Text(RenderError),
    InvalidImageData {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    UnsupportedAssetUrl {
        asset: String,
        url: String,
    },
    MissingVideoDecoder(String),
    UnsupportedContent {
        layer: String,
        content: &'static str,
    },
    InvalidSurfaceSize {
        width: u32,
        height: u32,
    },
    InvalidTargetSize {
        width: u32,
        height: u32,
    },
    IncompatibleSurface,
    SurfaceNotConfigured,
    SurfaceValidation,
    SurfaceTooLarge {
        width: u32,
        height: u32,
    },
    NativePreview(i32),
}

impl fmt::Display for GpuRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RequestAdapter(error) => write!(formatter, "could not select a GPU: {error}"),
            Self::RequestDevice(error) => write!(formatter, "could not open the GPU: {error}"),
            Self::Poll(error) => write!(formatter, "could not wait for GPU work: {error}"),
            Self::Map(error) => write!(formatter, "could not read the GPU frame: {error}"),
            Self::MapRange(error) => {
                write!(formatter, "could not access the mapped GPU frame: {error}")
            }
            Self::MapCallbackDropped => formatter.write_str("GPU readback callback was dropped"),
            Self::AssetIo { asset, source } => {
                write!(formatter, "could not read GPU asset {asset}: {source}")
            }
            Self::ImageDecode { asset, source } => {
                write!(formatter, "could not decode GPU image {asset}: {source}")
            }
            Self::Psd(error) => write!(formatter, "could not rasterize GPU PSD: {error}"),
            Self::Media(error) => write!(formatter, "could not decode GPU video frame: {error}"),
            Self::Text(error) => write!(formatter, "could not rasterize GPU text: {error}"),
            Self::InvalidImageData {
                width,
                height,
                expected,
                actual,
            } => write!(
                formatter,
                "invalid {width}x{height} RGBA image: expected {expected} bytes, got {actual}"
            ),
            Self::UnsupportedAssetUrl { asset, url } => {
                write!(formatter, "GPU asset {asset} uses unsupported URL {url}")
            }
            Self::MissingVideoDecoder(layer) => {
                write!(
                    formatter,
                    "GPU video layer {layer} requires a video decoder"
                )
            }
            Self::UnsupportedContent { layer, content } => {
                write!(
                    formatter,
                    "GPU layer {layer} uses unsupported {content} content"
                )
            }
            Self::InvalidSurfaceSize { width, height } => {
                write!(
                    formatter,
                    "GPU frame size must be non-zero, got {width}x{height}"
                )
            }
            Self::InvalidTargetSize { width, height } => {
                write!(
                    formatter,
                    "GPU target size must be non-zero, got {width}x{height}"
                )
            }
            Self::IncompatibleSurface => {
                formatter.write_str("GPU adapter is incompatible with the preview surface")
            }
            Self::SurfaceNotConfigured => {
                formatter.write_str("preview surface has not been configured")
            }
            Self::SurfaceValidation => {
                formatter.write_str("preview surface reported a validation error")
            }
            Self::SurfaceTooLarge { width, height } => {
                write!(formatter, "GPU frame is too large: {width}x{height}")
            }
            Self::NativePreview(status) => {
                write!(
                    formatter,
                    "could not create native preview surface ({status})"
                )
            }
        }
    }
}

impl Error for GpuRenderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::RequestAdapter(error) => Some(error),
            Self::RequestDevice(error) => Some(error),
            Self::Poll(error) => Some(error),
            Self::Map(error) => Some(error),
            Self::MapRange(error) => Some(error),
            Self::AssetIo { source, .. } => Some(source),
            Self::ImageDecode { source, .. } => Some(source),
            Self::Psd(error) => Some(error),
            Self::Media(error) => Some(error),
            Self::Text(error) => Some(error),
            Self::MapCallbackDropped
            | Self::InvalidImageData { .. }
            | Self::UnsupportedAssetUrl { .. }
            | Self::MissingVideoDecoder(_)
            | Self::UnsupportedContent { .. }
            | Self::InvalidSurfaceSize { .. }
            | Self::InvalidTargetSize { .. }
            | Self::IncompatibleSurface
            | Self::SurfaceNotConfigured
            | Self::SurfaceValidation
            | Self::SurfaceTooLarge { .. }
            | Self::NativePreview(_) => None,
        }
    }
}

impl From<MediaError> for GpuRenderError {
    fn from(error: MediaError) -> Self {
        Self::Media(error)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use mikan_composition::{
        AssetLocation, EvaluatedTransform, Layer, LayerContent, MediaTiming, Paint, Point,
        Rational, ResolvedAsset, Scene, Stroke, TextStyle, Time,
    };
    use mikan_media::{MediaError, VideoFrame, VideoFrameDecoder};

    use super::*;

    fn empty_scene(width: u32, height: u32) -> Scene {
        Scene {
            width,
            height,
            frame_rate: Rational::new(30, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: Vec::new(),
        }
    }

    fn renderer(options: GpuRenderOptions) -> Option<GpuRenderer> {
        match GpuRenderer::new(options) {
            Ok(renderer) => Some(renderer),
            Err(GpuRenderError::RequestAdapter(error)) => {
                eprintln!("skipping GPU test because no adapter is available: {error}");
                None
            }
            Err(error) => panic!("could not initialize GPU renderer: {error}"),
        }
    }

    #[test]
    fn pads_readback_rows_to_the_gpu_copy_alignment() {
        let layout = ReadbackLayout::new(3, 2).unwrap();
        assert_eq!(layout.unpadded_bytes_per_row, 12);
        assert_eq!(layout.padded_bytes_per_row, 256);
        assert_eq!(layout.buffer_size, 512);
    }

    #[test]
    fn fits_a_widescreen_scene_inside_a_square_preview() {
        let viewport = PreviewViewport::fit(1920, 1080, 1000, 1000).unwrap();
        assert!((viewport.x - 0.0).abs() < 0.001);
        assert!((viewport.y - 218.75).abs() < 0.001);
        assert!((viewport.width - 1000.0).abs() < 0.001);
        assert!((viewport.height - 562.5).abs() < 0.001);
    }

    #[test]
    fn renders_an_offscreen_background_when_a_gpu_is_available() {
        let background = Color::rgba(51, 102, 153, 255);
        let Some(mut renderer) = renderer(GpuRenderOptions { background }) else {
            return;
        };
        let frame = renderer.render(&empty_scene(3, 2)).unwrap();
        assert_eq!((frame.width(), frame.height()), (3, 2));
        assert_eq!(frame.pixels().len(), 24);
        assert!(
            frame
                .pixels()
                .chunks_exact(4)
                .all(|pixel| pixel == [51, 102, 153, 255])
        );
    }

    #[test]
    fn submit_and_drain_return_frames_in_submission_order_with_correct_content() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };

        // More scenes than PIPELINE_DEPTH so this exercises both the
        // reclaim-while-submitting path and the final drain, each scene
        // filled with a distinct background color to catch a slot mix-up.
        let colors = [
            Color::rgba(10, 20, 30, 255),
            Color::rgba(40, 50, 60, 255),
            Color::rgba(70, 80, 90, 255),
            Color::rgba(100, 110, 120, 255),
            Color::rgba(130, 140, 150, 255),
        ];
        let mut ready = Vec::new();
        for color in colors {
            renderer.options.background = color;
            if let Some(frame) = renderer.submit(&empty_scene(2, 2)).unwrap() {
                ready.push(frame);
            }
        }
        ready.extend(renderer.drain().unwrap());

        assert_eq!(ready.len(), colors.len());
        for (frame, color) in ready.iter().zip(colors) {
            assert_eq!((frame.width(), frame.height()), (2, 2));
            assert!(
                frame
                    .pixels()
                    .chunks_exact(4)
                    .all(|pixel| pixel == [color.red, color.green, color.blue, color.alpha])
            );
        }
    }

    #[test]
    fn falls_back_to_cpu_preview_for_odd_dimensions() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        match renderer.render_preview(&empty_scene(3, 2)).unwrap() {
            PreviewFrame::Cpu(frame) => assert_eq!((frame.width(), frame.height()), (3, 2)),
            #[cfg(target_os = "macos")]
            PreviewFrame::Native(_) => panic!("NV12 preview requires even dimensions"),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn keeps_native_preview_backings_alive_across_dialogue_frame_churn() {
        use core_video::pixel_buffer::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange;

        let Some(renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        std::thread::spawn(move || {
            use std::collections::VecDeque;

            let mut renderer = renderer;
            let mut frames = VecDeque::new();
            for index in 0..120 {
                let mut scene = empty_scene(1280, 720);
                scene.layers.push(Layer {
                    id: "changing-dialogue".to_owned(),
                    transform: EvaluatedTransform {
                        position: Point { x: 640.0, y: 600.0 },
                        ..EvaluatedTransform::default()
                    },
                    opacity: 1.0,
                    content: LayerContent::Text {
                        text: format!("Dialogue preview frame {index}"),
                        style: TextStyle {
                            font_size: Some(48.0),
                            ..TextStyle::default()
                        },
                        max_width: Some(1000.0),
                    },
                });
                match renderer.render_preview(&scene).unwrap() {
                    PreviewFrame::Native(frame) => {
                        let buffer = frame.pixel_buffer();
                        assert_eq!((buffer.get_width(), buffer.get_height()), (1280, 720));
                        assert_eq!(
                            buffer.get_pixel_format(),
                            kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
                        );
                        assert_eq!(buffer.get_plane_count(), 2);
                        frames.push_back(frame);
                        if frames.len() > 3 {
                            frames.pop_front();
                        }
                    }
                    PreviewFrame::Cpu(_) => {
                        panic!("Metal preview unexpectedly used CPU readback")
                    }
                }
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn renders_to_a_bgra_preview_target_without_readback() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Mikan preview test target"),
            size: wgpu::Extent3d {
                width: 320,
                height: 240,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        renderer
            .render_to_target(
                &empty_scene(1920, 1080),
                GpuRenderTarget {
                    view: &view,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    width: 320,
                    height: 240,
                },
            )
            .unwrap();
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
    }

    #[test]
    fn decodes_and_rotates_a_nested_image_on_the_gpu() {
        let Some(renderer) = renderer(GpuRenderOptions {
            background: Color::TRANSPARENT,
        }) else {
            return;
        };
        let mut renderer = renderer.with_asset_root(env!("CARGO_MANIFEST_DIR"));
        let mut scene = empty_scene(2, 2);
        scene.layers.push(Layer {
            id: "group".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 1.0, y: 1.0 },
                rotation: 180.0,
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            content: LayerContent::Group {
                layers: vec![Layer {
                    id: "checker".to_owned(),
                    transform: EvaluatedTransform::default(),
                    opacity: 1.0,
                    content: LayerContent::Image {
                        asset: ResolvedAsset {
                            id: "checker".to_owned(),
                            location: AssetLocation::File {
                                path: "tests/assets/checker.ppm".to_owned(),
                            },
                        },
                    },
                }],
            },
        });

        let frame = renderer.render(&scene).unwrap();
        assert_eq!(
            frame.pixels(),
            &[
                255, 255, 255, 255, 0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255,
            ]
        );
    }

    #[test]
    fn decodes_positions_and_fades_a_video_frame_on_the_gpu() {
        struct Decoder;

        impl VideoFrameDecoder for Decoder {
            fn decode_frame(
                &mut self,
                path: &Path,
                source_time_seconds: f64,
            ) -> Result<VideoFrame, MediaError> {
                assert_eq!(path, Path::new("./clip.mp4"));
                assert_eq!(source_time_seconds, 2.0);
                Ok(VideoFrame {
                    width: 1,
                    height: 1,
                    pixels: vec![12, 34, 56, 255],
                })
            }
        }

        let Some(renderer) = renderer(GpuRenderOptions {
            background: Color::rgba(0, 0, 0, 255),
        }) else {
            return;
        };
        let mut renderer = renderer.with_video_decoder(Decoder);
        let mut scene = empty_scene(2, 1);
        scene.layers.push(Layer {
            id: "video".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 1.5, y: 0.5 },
                ..EvaluatedTransform::default()
            },
            opacity: 0.5,
            content: LayerContent::Video {
                asset: ResolvedAsset {
                    id: "clip".to_owned(),
                    location: AssetLocation::File {
                        path: "clip.mp4".to_owned(),
                    },
                },
                timing: MediaTiming {
                    local_time: Time::new(1, 2),
                    source_start: Time::new(1, 1),
                    source_time_seconds: 2.0,
                    playback_rate: 2.0,
                },
            },
        });

        let frame = renderer.render(&scene).unwrap();
        assert_eq!(frame.pixels(), &[0, 0, 0, 255, 6, 17, 28, 255]);
    }

    #[test]
    fn rasterizes_and_rotates_styled_text_on_the_gpu() {
        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::TRANSPARENT,
        }) else {
            return;
        };
        let mut scene = empty_scene(160, 80);
        scene.layers.push(Layer {
            id: "title".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 80.0, y: 40.0 },
                rotation: 12.0,
                ..EvaluatedTransform::default()
            },
            opacity: 0.75,
            content: LayerContent::Text {
                text: "Mikan".to_owned(),
                style: TextStyle {
                    font_size: Some(32.0),
                    fill: Some(Paint::Solid {
                        color: "#ff8000".to_owned(),
                    }),
                    stroke: Some(Stroke {
                        paint: Paint::Solid {
                            color: "#0040ff".to_owned(),
                        },
                        width: 1.0,
                    }),
                    ..TextStyle::default()
                },
                max_width: None,
            },
        });

        let frame = renderer.render(&scene).unwrap();
        assert!(
            frame
                .pixels()
                .chunks_exact(4)
                .any(|pixel| { pixel[3] > 0 && pixel[0] > pixel[1] && pixel[1] > pixel[2] })
        );
        assert!(
            frame
                .pixels()
                .chunks_exact(4)
                .any(|pixel| { pixel[3] > 0 && pixel[2] > pixel[0] })
        );
    }

    #[test]
    fn centers_visible_single_line_text_on_its_transform() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let mut scene = empty_scene(1280, 720);
        scene.layers.push(Layer {
            id: "title".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 640.0, y: 360.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            content: LayerContent::Text {
                text: "Mikan".to_owned(),
                style: TextStyle {
                    font_size: Some(96.0),
                    fill: Some(Paint::Solid {
                        color: "#FFA13BFF".to_owned(),
                    }),
                    align: Some(mikan_composition::TextAlign::Center),
                    ..TextStyle::default()
                },
                max_width: None,
            },
        });
        let frame = renderer.render(&scene).unwrap();
        let mut min_x = u32::MAX;
        let mut min_y = u32::MAX;
        let mut max_x = 0;
        let mut max_y = 0;
        for (index, pixel) in frame.pixels().chunks_exact(4).enumerate() {
            if pixel != [20, 22, 28, 255] {
                let x = index as u32 % frame.width();
                let y = index as u32 / frame.width();
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
        let center_x = (min_x + max_x) as f32 / 2.0;
        let center_y = (min_y + max_y) as f32 / 2.0;
        assert!((center_x - 640.0).abs() <= 0.5, "center x was {center_x}");
        assert!(
            (center_y - 360.0).abs() <= 0.5,
            "center y was {center_y} ({min_y}..{max_y})"
        );
    }
}
