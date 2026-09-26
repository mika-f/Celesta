//! GPU rendering foundation built on `wgpu`.
//!
//! It supports offscreen image, video, and text composition with nested
//! transforms, opacity, painter ordering, and deterministic RGBA readback.
//! Unsupported content returns an error instead of silently disappearing.

use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;

use celesta_composition::{EvaluatedTransform, Layer, LayerContent, Point, ResolvedAsset, Scene};
use celesta_media::{MediaError, VideoFrameDecoder};
use celesta_remote::{RemoteAssetError, resolve_asset_path};
use celesta_renderer::{RenderError, TextRasterizer, rasterize_rect};
use image::ImageReader;
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

/// The pixel layout `GpuRenderer::submit`/`drain` read frames back in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadbackFormat {
    /// Tightly packed RGBA8 rows, 4 bytes per pixel.
    #[default]
    Rgba8,
    /// Planar I420 converted on the GPU: the full-size Y plane followed by
    /// the quarter-size U and V planes, 1.5 bytes per pixel. BT.601 limited
    /// range with 2x2-averaged chroma, the same matrix libswscale applies to
    /// untagged RGB input. Both dimensions must be even.
    Yuv420p,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuFrame {
    width: u32,
    height: u32,
    format: ReadbackFormat,
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

    /// How [`Self::pixels`] is laid out.
    pub const fn format(&self) -> ReadbackFormat {
        self.format
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
    /// The layout newly submitted frames are read back in.
    readback_format: ReadbackFormat,
    /// Created by the first switch to [`ReadbackFormat::Yuv420p`].
    yuv_converter: Option<YuvConverter>,
    /// GPU textures for layer content that is identical from one frame to
    /// the next (images, PSD composites, text, and rects), keyed by what
    /// produced them. A cache hit skips re-rasterizing the text/rect on the
    /// CPU and re-uploading the pixels, which otherwise dominates the cost of
    /// a mostly static frame. Entries a frame does not use are dropped at
    /// the end of that frame's `prepare_draws`.
    textures: HashMap<String, CachedTexture>,
    /// Incremented by every `prepare_draws`; a cache entry's `last_used`
    /// equals it when the frame being prepared uses that entry.
    texture_generation: u64,
    /// `TextRasterizer::loaded_font_count` when the cached text textures
    /// were rasterized; a newly loaded font can change their layout.
    text_font_count: usize,
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
            label: Some("Celesta GPU Renderer"),
            ..Default::default()
        };
        let (device, queue) = adapter
            .request_device(&descriptor)
            .await
            .map_err(GpuRenderError::RequestDevice)?;
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta layer bind group layout"),
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
            label: Some("Celesta layer pipeline layout"),
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
            label: Some("Celesta layer sampler"),
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
            readback_format: ReadbackFormat::Rgba8,
            yuv_converter: None,
            textures: HashMap::new(),
            texture_generation: 0,
            text_font_count: 0,
        })
    }

    pub const fn options(&self) -> GpuRenderOptions {
        self.options
    }

    pub const fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    /// Whether this GPU can convert frames to [`ReadbackFormat::Yuv420p`]
    /// (it renders the U and V planes as two render targets at once).
    pub fn supports_yuv420p_readback(&self) -> bool {
        self.device.limits().max_color_attachments >= 2
    }

    /// Whether the adapter is a software renderer (e.g. lavapipe or WARP)
    /// running on the CPU rather than a hardware GPU.
    pub fn is_software(&self) -> bool {
        self.adapter_info.device_type == wgpu::DeviceType::Cpu
    }

    /// The layout `submit` and `drain` currently return new frames in.
    pub const fn readback_format(&self) -> ReadbackFormat {
        self.readback_format
    }

    /// Chooses the layout `submit` and `drain` return frames in; `render`
    /// always returns RGBA. Frames already in flight keep the format they
    /// were submitted with.
    pub fn set_readback_format(&mut self, format: ReadbackFormat) -> Result<(), GpuRenderError> {
        if format == ReadbackFormat::Yuv420p && self.yuv_converter.is_none() {
            if !self.supports_yuv420p_readback() {
                return Err(GpuRenderError::UnsupportedReadbackFormat(format));
            }
            self.yuv_converter = Some(YuvConverter::new(&self.device));
        }
        self.readback_format = format;
        Ok(())
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
                label: Some("Celesta preview commands"),
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
            label: Some("Celesta offscreen frame"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Celesta RGBA readback"),
            size: layout.buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Celesta offscreen commands"),
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
        let pixels = layout.unpad(&mapped, scene.width, scene.height)?;
        drop(mapped);
        output.unmap();

        Ok(GpuFrame {
            width: scene.width,
            height: scene.height,
            format: ReadbackFormat::Rgba8,
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
        let format = self.readback_format;
        if format == ReadbackFormat::Yuv420p
            && (!scene.width.is_multiple_of(2) || !scene.height.is_multiple_of(2))
        {
            return Err(GpuRenderError::OddYuv420pSize {
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
                let slot = &self.readback_slots[index];
                if slot.width != scene.width
                    || slot.height != scene.height
                    || slot.format() != format
                {
                    self.readback_slots[index] = self.readback_slot(scene.width, scene.height)?;
                }
                index
            }
            None => {
                let slot = self.readback_slot(scene.width, scene.height)?;
                self.readback_slots.push(slot);
                self.readback_slots.len() - 1
            }
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Celesta pipelined offscreen commands"),
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
        let slot = &self.readback_slots[slot_index];
        match &slot.readback {
            SlotReadback::Rgba8(layout) => encoder.copy_texture_to_buffer(
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
                        bytes_per_row: Some(layout.padded_bytes_per_row),
                        rows_per_image: Some(scene.height),
                    },
                },
                wgpu::Extent3d {
                    width: scene.width,
                    height: scene.height,
                    depth_or_array_layers: 1,
                },
            ),
            SlotReadback::Yuv420p(yuv) => {
                let converter = self
                    .yuv_converter
                    .as_ref()
                    .expect("a yuv420p slot is only created with a converter");
                converter.encode(&mut encoder, yuv);
                for plane in &yuv.planes {
                    encoder.copy_texture_to_buffer(
                        plane.texture.as_image_copy(),
                        wgpu::TexelCopyBufferInfo {
                            buffer: &slot.buffer,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: plane.offset,
                                bytes_per_row: Some(plane.layout.padded_bytes_per_row),
                                rows_per_image: Some(plane.height),
                            },
                        },
                        plane.texture.size(),
                    );
                }
            }
        }
        let submission = self.queue.submit([encoder.finish()]);

        let slot = &mut self.readback_slots[slot_index];
        let (sender, receiver) = mpsc::sync_channel(1);
        slot.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        slot.pending = Some((submission, receiver));
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
            let (submission, receiver) = slot
                .pending
                .take()
                .expect("in-flight slot always has a pending readback");
            // Wait for this slot's own submission only. Waiting for the most
            // recent one (`wait_indefinitely`) would also block on every
            // younger frame still in flight, draining the GPU queue on each
            // reclaim and defeating the pipelining.
            self.device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: None,
                })
                .map_err(GpuRenderError::Poll)?;
            receiver
                .recv()
                .map_err(|_| GpuRenderError::MapCallbackDropped)?
                .map_err(GpuRenderError::Map)?;

            let slice = slot.buffer.slice(..);
            let mapped = slice.get_mapped_range().map_err(GpuRenderError::MapRange)?;
            let pixels = match &slot.readback {
                SlotReadback::Rgba8(layout) => layout.unpad(&mapped, slot.width, slot.height)?,
                SlotReadback::Yuv420p(yuv) => yuv.unpad(&mapped),
            };
            drop(mapped);
            slot.buffer.unmap();
            GpuFrame {
                width: slot.width,
                height: slot.height,
                format: slot.format(),
                pixels,
            }
        };
        self.readback_free.push(slot_index);
        Ok(frame)
    }

    fn readback_slot(&self, width: u32, height: u32) -> Result<ReadbackSlot, GpuRenderError> {
        let converter = match self.readback_format {
            ReadbackFormat::Rgba8 => None,
            ReadbackFormat::Yuv420p => Some(
                self.yuv_converter
                    .as_ref()
                    .expect("set_readback_format creates the converter"),
            ),
        };
        ReadbackSlot::new(&self.device, width, height, converter)
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
        let font_count = self.text_rasterizer.loaded_font_count();
        if font_count != self.text_font_count {
            self.textures
                .retain(|key, _| !key.starts_with(TEXT_TEXTURE_PREFIX));
            self.text_font_count = font_count;
        }
        self.texture_generation += 1;
        let mut layers = Vec::new();
        let prepared = scene
            .layers
            .iter()
            .try_for_each(|layer| self.prepare_layer(layer, LayerState::default(), &mut layers));
        // Evict what this frame did not use, also when it failed part way
        // (the entries it did reach are still marked as used).
        let generation = self.texture_generation;
        self.textures
            .retain(|_, cached| cached.last_used == generation);
        prepared?;
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
            label: Some("Celesta layer pass"),
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
                let texture = self.cached_texture(format!("image\0{}", asset.id), |renderer| {
                    renderer.load_image(asset).cloned()
                })?;
                output.push(PreparedLayer::new(texture, layer.transform.anchor, state));
            }
            LayerContent::Psd {
                asset,
                visible_layers,
                enabled_layers,
                disabled_layers,
            } => {
                let texture = self.cached_texture(
                    psd_key(asset, visible_layers, enabled_layers, disabled_layers),
                    |renderer| {
                        renderer
                            .load_psd(asset, visible_layers, enabled_layers, disabled_layers)
                            .cloned()
                    },
                )?;
                output.push(PreparedLayer::new(texture, layer.transform.anchor, state));
            }
            LayerContent::Video { asset, timing } => {
                let path = self.local_asset_path(asset)?;
                let decoder = self
                    .video_decoder
                    .as_mut()
                    .ok_or_else(|| GpuRenderError::MissingVideoDecoder(layer.id.clone()))?;
                let frame =
                    decoder.decode_frame_for(&layer.id, &path, timing.source_time_seconds)?;
                let image = DecodedImage::shared(frame.width, frame.height, frame.pixels)?;
                // Every frame brings new pixels, so video is never cached.
                let texture = self.upload_texture(&image);
                output.push(PreparedLayer::new(texture, layer.transform.anchor, state));
            }
            LayerContent::Text {
                text,
                style,
                max_width,
            } => {
                // `{:?}` spells out every style field and prints floats
                // exactly, so equal keys always mean equal rasterizer input.
                let key = format!("{TEXT_TEXTURE_PREFIX}{text}\0{style:?}\0{max_width:?}");
                let texture = self.cached_texture(key, |renderer| {
                    let text = renderer
                        .text_rasterizer
                        .rasterize(text, style, *max_width, 1.0)
                        .map_err(GpuRenderError::Text)?;
                    DecodedImage::new(text.width(), text.height(), text.into_pixels())
                })?;
                output.push(PreparedLayer::new(texture, layer.transform.anchor, state));
            }
            LayerContent::Rect {
                width,
                height,
                fill,
                stroke,
                corner_radius,
            } => {
                let key =
                    format!("rect\0{width:?}\0{height:?}\0{corner_radius:?}\0{fill:?}\0{stroke:?}");
                let texture = self.cached_texture(key, |_| {
                    let rect = rasterize_rect(
                        *width,
                        *height,
                        *corner_radius,
                        fill.as_ref(),
                        stroke.as_ref(),
                    )
                    .map_err(GpuRenderError::Text)?;
                    DecodedImage::new(rect.width(), rect.height(), rect.into_pixels())
                })?;
                output.push(PreparedLayer::new(texture, layer.transform.anchor, state));
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
            // Sniff the format: a downloaded file's name may lack an extension.
            let image = ImageReader::open(&path)
                .and_then(ImageReader::with_guessed_format)
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
        visible_layers: &[String],
        enabled_layers: &[String],
        disabled_layers: &[String],
    ) -> Result<&DecodedImage, GpuRenderError> {
        let key = psd_key(asset, visible_layers, enabled_layers, disabled_layers);
        if !self.images.contains_key(&key) {
            let path = self.local_asset_path(asset)?;
            let frame = celesta_renderer::rasterize_psd(
                &asset.id,
                &path,
                visible_layers,
                enabled_layers,
                disabled_layers,
            )
            .map_err(GpuRenderError::Psd)?;
            let image = DecodedImage::new(frame.width(), frame.height(), frame.pixels().to_vec())?;
            self.images.insert(key.clone(), image);
        }
        Ok(self.images.get(&key).expect("PSD image was cached"))
    }

    fn local_asset_path(&self, asset: &ResolvedAsset) -> Result<PathBuf, GpuRenderError> {
        resolve_asset_path(&self.asset_root, &asset.location).map_err(|source| {
            GpuRenderError::RemoteAsset {
                asset: asset.id.clone(),
                source,
            }
        })
    }

    /// Returns the texture cached under `key`, marking it used by the frame
    /// being prepared, or produces and uploads it on a miss.
    fn cached_texture(
        &mut self,
        key: String,
        produce: impl FnOnce(&mut Self) -> Result<DecodedImage, GpuRenderError>,
    ) -> Result<LayerTexture, GpuRenderError> {
        let generation = self.texture_generation;
        if let Some(cached) = self.textures.get_mut(&key) {
            cached.last_used = generation;
            return Ok(cached.texture.clone());
        }
        let image = produce(self)?;
        let texture = self.upload_texture(&image);
        self.textures.insert(
            key,
            CachedTexture {
                texture: texture.clone(),
                last_used: generation,
            },
        );
        Ok(texture)
    }

    fn upload_texture(&self, image: &DecodedImage) -> LayerTexture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Celesta layer texture"),
            size: wgpu::Extent3d {
                width: image.width,
                height: image.height,
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
            &image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * BYTES_PER_PIXEL),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        LayerTexture {
            _texture: texture,
            view,
            width: image.width,
            height: image.height,
        }
    }

    fn create_draw(
        &self,
        layer: &PreparedLayer,
        canvas_width: u32,
        canvas_height: u32,
    ) -> Result<GpuDraw, GpuRenderError> {
        let uniform = layer.uniform(canvas_width, canvas_height);
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Celesta layer uniform"),
                contents: &uniform,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Celesta layer bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&layer.texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        Ok(GpuDraw {
            _texture: layer.texture.clone(),
            _uniform: uniform,
            bind_group,
        })
    }
}

/// Prefix of every cached text texture's key, so a newly loaded font can drop
/// just those.
const TEXT_TEXTURE_PREFIX: &str = "text\0";

fn psd_key(
    asset: &ResolvedAsset,
    visible_layers: &[String],
    enabled_layers: &[String],
    disabled_layers: &[String],
) -> String {
    format!(
        "psd\0{}\0{}\0{}\0{}",
        asset.id,
        visible_layers.join("\0"),
        enabled_layers.join("\0"),
        disabled_layers.join("\0")
    )
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Celesta layer pipeline"),
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
    /// `Arc<Vec<u8>>` rather than `Arc<[u8]>`: the latter cannot take ownership
    /// of an existing `Vec` and copies it instead, which costs a full 8MB
    /// memcpy for every 1080p video frame the preview decodes — several
    /// milliseconds per frame for nothing, since the buffer is already owned.
    pixels: Arc<Vec<u8>>,
}

impl DecodedImage {
    fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, GpuRenderError> {
        Self::shared(width, height, Arc::new(pixels))
    }

    fn shared(width: u32, height: u32, pixels: Arc<Vec<u8>>) -> Result<Self, GpuRenderError> {
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
            pixels,
        })
    }
}

/// An uploaded layer texture. Cloning it clones the `wgpu` handles, not the
/// pixels, so a cached texture can back draws in several in-flight frames.
#[derive(Clone)]
struct LayerTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

struct CachedTexture {
    texture: LayerTexture,
    last_used: u64,
}

struct PreparedLayer {
    texture: LayerTexture,
    anchor: Point,
    state: LayerState,
}

impl PreparedLayer {
    const fn new(texture: LayerTexture, anchor: Point, state: LayerState) -> Self {
        Self {
            texture,
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
            self.texture.width as f32,
            self.texture.height as f32,
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
    _texture: LayerTexture,
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
        Self::with_bytes_per_pixel(width, height, BYTES_PER_PIXEL)
    }

    fn with_bytes_per_pixel(
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
    ) -> Result<Self, GpuRenderError> {
        let unpadded_bytes_per_row = width
            .checked_mul(bytes_per_pixel)
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

    /// Copies a mapped readback buffer into tightly packed rows. When
    /// the row width is already aligned (e.g. 1920 or 1280 pixels wide) the
    /// buffer has no padding and is copied in one go.
    fn unpad(&self, mapped: &[u8], width: u32, height: u32) -> Result<Vec<u8>, GpuRenderError> {
        let capacity = (self.unpadded_bytes_per_row as usize)
            .checked_mul(height as usize)
            .ok_or(GpuRenderError::SurfaceTooLarge { width, height })?;
        let mut pixels = Vec::with_capacity(capacity);
        self.unpad_into(mapped, &mut pixels);
        Ok(pixels)
    }

    /// Appends the tightly packed rows of `mapped`, a region of exactly
    /// `buffer_size` bytes, to `pixels`.
    fn unpad_into(&self, mapped: &[u8], pixels: &mut Vec<u8>) {
        let mapped = &mapped[..self.buffer_size as usize];
        if self.padded_bytes_per_row == self.unpadded_bytes_per_row {
            pixels.extend_from_slice(mapped);
            return;
        }
        let row = self.unpadded_bytes_per_row as usize;
        for padded in mapped.chunks_exact(self.padded_bytes_per_row as usize) {
            pixels.extend_from_slice(&padded[..row]);
        }
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
    readback: SlotReadback,
    pending: Option<(
        wgpu::SubmissionIndex,
        mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    )>,
}

/// How a slot's rendered texture reaches its mapped `buffer`.
enum SlotReadback {
    /// Copied as is, rows padded to the copy alignment.
    Rgba8(ReadbackLayout),
    /// Converted into Y, U, and V plane textures by two render passes, whose
    /// rows are then copied, padded, one plane after the other.
    Yuv420p(Box<YuvReadback>),
}

/// The plane textures one yuv420p slot converts into, and where each lands
/// in the slot's readback buffer.
struct YuvReadback {
    /// Samples the slot's RGBA texture.
    bind_group: wgpu::BindGroup,
    /// Y, U, and V, in the order they are packed.
    planes: [YuvPlane; 3],
}

struct YuvPlane {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    height: u32,
    layout: ReadbackLayout,
    /// Byte offset of this plane's padded rows in the readback buffer.
    offset: u64,
}

impl YuvReadback {
    /// Total size of the three padded planes.
    fn buffer_size(&self) -> u64 {
        let last = &self.planes[2];
        last.offset + last.layout.buffer_size
    }

    /// Packs the three padded planes of a mapped readback buffer into one
    /// tightly packed I420 frame.
    fn unpad(&self, mapped: &[u8]) -> Vec<u8> {
        let capacity = self
            .planes
            .iter()
            .map(|plane| plane.layout.unpadded_bytes_per_row as usize * plane.height as usize)
            .sum();
        let mut pixels = Vec::with_capacity(capacity);
        for plane in &self.planes {
            let start = plane.offset as usize;
            let region = &mapped[start..start + plane.layout.buffer_size as usize];
            plane.layout.unpad_into(region, &mut pixels);
        }
        pixels
    }
}

impl ReadbackSlot {
    fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        yuv_converter: Option<&YuvConverter>,
    ) -> Result<Self, GpuRenderError> {
        let layout = ReadbackLayout::new(width, height)?;
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
        if yuv_converter.is_some() {
            usage |= wgpu::TextureUsages::TEXTURE_BINDING;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Celesta pipelined offscreen frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (readback, buffer_size) = match yuv_converter {
            None => {
                let size = layout.buffer_size;
                (SlotReadback::Rgba8(layout), size)
            }
            Some(converter) => {
                let yuv = converter.readback(device, &view, width, height)?;
                let size = yuv.buffer_size();
                (SlotReadback::Yuv420p(Box::new(yuv)), size)
            }
        };
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Celesta pipelined readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Self {
            texture,
            view,
            buffer,
            width,
            height,
            readback,
            pending: None,
        })
    }

    const fn format(&self) -> ReadbackFormat {
        match self.readback {
            SlotReadback::Rgba8(_) => ReadbackFormat::Rgba8,
            SlotReadback::Yuv420p(_) => ReadbackFormat::Yuv420p,
        }
    }
}

/// The render pipelines behind [`ReadbackFormat::Yuv420p`] (`yuv420p.wgsl`).
struct YuvConverter {
    luma: wgpu::RenderPipeline,
    chroma: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl YuvConverter {
    const PLANE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

    fn new(device: &wgpu::Device) -> Self {
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
        let shader = device.create_shader_module(wgpu::include_wgsl!("yuv420p.wgsl"));
        let target = Some(wgpu::ColorTargetState {
            format: Self::PLANE_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        });
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
        Self {
            luma: pipeline(
                "Celesta yuv420p luma",
                "luma",
                std::slice::from_ref(&target),
            ),
            chroma: pipeline(
                "Celesta yuv420p chroma",
                "chroma",
                &[target.clone(), target],
            ),
            bind_group_layout,
        }
    }

    /// The plane textures and bind group converting one `width`x`height`
    /// slot whose RGBA texture is `frame`.
    fn readback(
        &self,
        device: &wgpu::Device,
        frame: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<YuvReadback, GpuRenderError> {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Celesta yuv420p bind group"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(frame),
            }],
        });
        let mut offset = 0;
        let mut plane = |label, width: u32, height: u32| -> Result<YuvPlane, GpuRenderError> {
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
                format: Self::PLANE_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let layout = ReadbackLayout::with_bytes_per_pixel(width, height, 1)?;
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
        let planes = [
            plane("Celesta yuv420p Y plane", width, height)?,
            plane("Celesta yuv420p U plane", width / 2, height / 2)?,
            plane("Celesta yuv420p V plane", width / 2, height / 2)?,
        ];
        Ok(YuvReadback { bind_group, planes })
    }

    /// Renders the slot's RGBA texture into its Y, U, and V planes.
    fn encode(&self, encoder: &mut wgpu::CommandEncoder, yuv: &YuvReadback) {
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
        let [luma, u, v] = &yuv.planes;
        for (label, pipeline, targets) in [
            (
                "Celesta yuv420p luma pass",
                &self.luma,
                vec![attachment(luma)],
            ),
            (
                "Celesta yuv420p chroma pass",
                &self.chroma,
                vec![attachment(u), attachment(v)],
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
            pass.set_bind_group(0, &yuv.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
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
    RemoteAsset {
        asset: String,
        source: RemoteAssetError,
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
    /// The GPU cannot produce frames in this [`ReadbackFormat`].
    UnsupportedReadbackFormat(ReadbackFormat),
    /// [`ReadbackFormat::Yuv420p`] subsamples chroma 2x2, so it needs even
    /// dimensions.
    OddYuv420pSize {
        width: u32,
        height: u32,
    },
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
            Self::RemoteAsset { asset, source } => {
                write!(formatter, "could not load remote asset `{asset}`: {source}")
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
            Self::UnsupportedReadbackFormat(format) => {
                write!(formatter, "this GPU cannot read frames back as {format:?}")
            }
            Self::OddYuv420pSize { width, height } => write!(
                formatter,
                "yuv420p readback requires even dimensions, got {width}x{height}"
            ),
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
            Self::RemoteAsset { source, .. } => Some(source),
            Self::Psd(error) => Some(error),
            Self::Media(error) => Some(error),
            Self::Text(error) => Some(error),
            Self::MapCallbackDropped
            | Self::InvalidImageData { .. }
            | Self::MissingVideoDecoder(_)
            | Self::UnsupportedContent { .. }
            | Self::InvalidSurfaceSize { .. }
            | Self::InvalidTargetSize { .. }
            | Self::IncompatibleSurface
            | Self::SurfaceNotConfigured
            | Self::SurfaceValidation
            | Self::SurfaceTooLarge { .. }
            | Self::NativePreview(_)
            | Self::UnsupportedReadbackFormat(_)
            | Self::OddYuv420pSize { .. } => None,
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

    use celesta_composition::{
        AssetLocation, EvaluatedTransform, Layer, LayerContent, MediaTiming, Paint, Point,
        Rational, ResolvedAsset, Scene, Stroke, TextStyle, Time,
    };
    use celesta_media::{MediaError, VideoFrame, VideoFrameDecoder};

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
    fn unpads_aligned_and_padded_readback_rows() {
        let aligned = ReadbackLayout::new(64, 2).unwrap();
        let mapped: Vec<u8> = (0..=255).cycle().take(512).collect();
        assert_eq!(aligned.unpad(&mapped, 64, 2).unwrap(), mapped);

        let padded = ReadbackLayout::new(3, 2).unwrap();
        let mut mapped = vec![0; 512];
        mapped[..12].fill(1);
        mapped[256..268].fill(2);
        let pixels = padded.unpad(&mapped, 3, 2).unwrap();
        assert_eq!(pixels.len(), 24);
        assert!(pixels[..12].iter().all(|byte| *byte == 1));
        assert!(pixels[12..].iter().all(|byte| *byte == 2));
    }

    #[test]
    fn reuses_unchanged_layer_textures_across_frames_and_evicts_unused_ones() {
        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::rgba(0, 0, 0, 255),
        }) else {
            return;
        };
        let rect = |id: &str, x: f64, color: &str| Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform {
                position: Point { x, y: 1.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            content: LayerContent::Rect {
                width: 2.0,
                height: 2.0,
                fill: Some(Paint::Solid {
                    color: color.to_owned(),
                }),
                stroke: None,
                corner_radius: 0.0,
            },
        };
        let scene = |layers: Vec<Layer>| {
            let mut scene = empty_scene(4, 2);
            scene.layers = layers;
            scene
        };
        let pixel = |frame: &GpuFrame, x: usize| frame.pixels()[x * 4..x * 4 + 4].to_vec();

        let first = scene(vec![
            rect("red", 1.0, "#ff0000"),
            rect("blue", 3.0, "#0000ff"),
        ]);
        let frame = renderer.render(&first).unwrap();
        assert_eq!(pixel(&frame, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&frame, 3), [0, 0, 255, 255]);
        assert_eq!(renderer.textures.len(), 2);

        // The same rect geometry with another fill is a different texture;
        // the blue one this frame no longer uses is dropped.
        let second = scene(vec![
            rect("red", 1.0, "#ff0000"),
            rect("blue", 3.0, "#00ff00"),
        ]);
        let frame = renderer.render(&second).unwrap();
        assert_eq!(pixel(&frame, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&frame, 3), [0, 255, 0, 255]);
        assert_eq!(renderer.textures.len(), 2);

        // Cached textures render the same frame again, including through
        // the pipelined readback path.
        let again = renderer.render(&second).unwrap();
        assert_eq!(again, frame);
        assert!(renderer.submit(&second).unwrap().is_none());
        assert_eq!(renderer.drain().unwrap(), vec![frame]);

        let empty = scene(Vec::new());
        renderer.render(&empty).unwrap();
        assert!(renderer.textures.is_empty());
    }

    fn solid_rect(id: &str, x: f64, width: f64, height: f64, color: &str) -> Layer {
        Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform {
                position: Point { x, y: height / 2.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            content: LayerContent::Rect {
                width,
                height,
                fill: Some(Paint::Solid {
                    color: color.to_owned(),
                }),
                stroke: None,
                corner_radius: 0.0,
            },
        }
    }

    #[test]
    fn converts_frames_to_the_same_yuv420p_values_as_libswscale() {
        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::rgba(0, 0, 0, 255),
        }) else {
            return;
        };
        if !renderer.supports_yuv420p_readback() {
            eprintln!("skipping yuv420p test: the GPU cannot convert to yuv420p");
            return;
        }
        renderer
            .set_readback_format(ReadbackFormat::Yuv420p)
            .unwrap();
        // Red, blue, white, and black columns, two pixels wide each.
        let mut scene = empty_scene(8, 4);
        for (index, color) in ["#ff0000", "#0000ff", "#ffffff", "#000000"]
            .into_iter()
            .enumerate()
        {
            let x = index as f64 * 2.0 + 1.0;
            scene
                .layers
                .push(solid_rect(&format!("column-{index}"), x, 2.0, 4.0, color));
        }

        assert!(renderer.submit(&scene).unwrap().is_none());
        let frame = renderer.drain().unwrap().remove(0);
        assert_eq!(frame.format(), ReadbackFormat::Yuv420p);
        assert_eq!(frame.pixels().len(), 8 * 4 * 3 / 2);
        let (luma, chroma) = frame.pixels().split_at(32);
        let (u, v) = chroma.split_at(8);
        // `ffmpeg -f rawvideo -pix_fmt rgba -i … -pix_fmt yuv420p` output
        // for the same pixels.
        for row in luma.chunks_exact(8) {
            assert_eq!(row, [81, 81, 41, 41, 235, 235, 16, 16]);
        }
        for row in u.chunks_exact(4) {
            assert_eq!(row, [90, 240, 128, 128]);
        }
        for row in v.chunks_exact(4) {
            assert_eq!(row, [240, 110, 128, 128]);
        }
    }

    #[test]
    fn pipelines_yuv420p_frames_with_padded_plane_rows() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        if !renderer.supports_yuv420p_readback() {
            eprintln!("skipping yuv420p test: the GPU cannot convert to yuv420p");
            return;
        }
        renderer
            .set_readback_format(ReadbackFormat::Yuv420p)
            .unwrap();
        // 6x2 packs into 18 bytes: 12 of luma and a 3x1 U and V plane each,
        // every plane row padded to the copy alignment on the GPU.
        let colors = [
            (Color::rgba(255, 0, 0, 255), [81, 90, 240]),
            (Color::rgba(0, 0, 255, 255), [41, 240, 110]),
            (Color::rgba(255, 255, 255, 255), [235, 128, 128]),
            (Color::rgba(0, 0, 0, 255), [16, 128, 128]),
            (Color::rgba(128, 128, 128, 255), [126, 128, 128]),
        ];
        let mut frames = Vec::new();
        for (color, _) in colors {
            renderer.options.background = color;
            frames.extend(renderer.submit(&empty_scene(6, 2)).unwrap());
        }
        frames.extend(renderer.drain().unwrap());

        assert_eq!(frames.len(), colors.len());
        for (frame, (_, [y, u, v])) in frames.iter().zip(colors) {
            let mut expected = vec![y; 12];
            expected.extend([u; 3]);
            expected.extend([v; 3]);
            assert_eq!(frame.pixels(), expected);
        }

        assert!(matches!(
            renderer.submit(&empty_scene(6, 3)),
            Err(GpuRenderError::OddYuv420pSize {
                width: 6,
                height: 3
            })
        ));

        // Switching back reads RGBA again, reusing the freed slots.
        renderer.set_readback_format(ReadbackFormat::Rgba8).unwrap();
        renderer.options.background = Color::rgba(1, 2, 3, 255);
        assert!(renderer.submit(&empty_scene(6, 2)).unwrap().is_none());
        let frame = renderer.drain().unwrap().remove(0);
        assert_eq!(frame.format(), ReadbackFormat::Rgba8);
        assert_eq!(frame.pixels(), [1, 2, 3, 255].repeat(12));
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
            label: Some("Celesta preview test target"),
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
                    pixels: vec![12, 34, 56, 255].into(),
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
                text: "Celesta".to_owned(),
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
                text: "Celesta".to_owned(),
                style: TextStyle {
                    font_size: Some(96.0),
                    fill: Some(Paint::Solid {
                        color: "#FFA13BFF".to_owned(),
                    }),
                    align: Some(celesta_composition::TextAlign::Center),
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
