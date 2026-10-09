use crate::compositor::BackdropTexture;
use crate::draw::{LAYER_INSTANCE_SIZE, clip_bind_group, clip_buffer, instance_buffer};
use crate::effect::EffectProcessor;
use crate::error::GpuRenderError;
use crate::mask::MaskPipelines;
#[cfg(target_os = "macos")]
use crate::native_preview;
use crate::path::PendingPath;
use crate::pipeline::{PipelineKind, create_pipeline};
use crate::readback::{ReadbackLayout, ReadbackSlot, ReadbackWorker, SlotReadback};
use crate::text::PendingText;
use crate::texture::{CachedTexture, CanvasTexture, DecodedImage, LayerTexture, upload_texture};
use crate::transform::{CLIP_ENTRY_SIZE, ClipEntry};
#[cfg(target_os = "macos")]
use crate::types::NativePreviewFrame;
use crate::types::{
    GpuFrame, GpuRenderOptions, GpuRenderTarget, PreviewFrame, PreviewFrameStatus, ReadbackFormat,
    RenderQuality,
};
use crate::yuv::YuvConverter;
use crate::{BYTES_PER_PIXEL, PIPELINE_DEPTH};
use celesta_composition::Scene;
use celesta_media::VideoFrameDecoder;
use celesta_renderer::{FontFallback, MissingGlyphs, TextRasterizer};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc;

pub struct GpuRenderer {
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) adapter_info: wgpu::AdapterInfo,
    pub(crate) options: GpuRenderOptions,
    pub(crate) shader: wgpu::ShaderModule,
    pub(crate) pipeline_layout: wgpu::PipelineLayout,
    /// Layout of `PipelineKind::Blend` pipelines: the layer texture, then
    /// the backdrop.
    pub(crate) blend_pipeline_layout: wgpu::PipelineLayout,
    pub(crate) pipelines: HashMap<(wgpu::TextureFormat, PipelineKind), wgpu::RenderPipeline>,
    pub(crate) texture_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) backdrop_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) render_quality: RenderQuality,
    /// The scene-sized texture a frame that uses blend modes, isolated
    /// groups, effects or masks composites in. Isolated groups, effects and
    /// masks draw onto smaller canvases from `effects`' pool. Reused across
    /// frames of the same size.
    pub(crate) canvas: Option<CanvasTexture>,
    /// What a blended draw reads its backdrop from: a copy of the canvas it
    /// draws onto, taken just before the draw.
    pub(crate) backdrop: Option<BackdropTexture>,
    pub(crate) effects: EffectProcessor,
    /// Shows masked groups' children through their masks.
    pub(crate) masks: MaskPipelines,
    /// Every layer's `LayerInstance` for the frame being prepared, reused
    /// (and grown when a frame needs more) across frames.
    pub(crate) instances: wgpu::Buffer,
    /// Layout of the bind group that exposes `clips` to the fragment shader.
    pub(crate) clip_bind_group_layout: wgpu::BindGroupLayout,
    /// Every clip of the frame being prepared, as `CLIP_ENTRY_FLOATS` floats
    /// each, reused (and grown when a frame needs more) like `instances`.
    pub(crate) clips: wgpu::Buffer,
    pub(crate) clip_bind_group: wgpu::BindGroup,
    /// The clips the frame being prepared has entered so far.
    pub(crate) clip_entries: Vec<ClipEntry>,
    /// Every gradient a rect of the frame being prepared is painted with,
    /// laid out as `layer.wgsl`'s `paints` reads them; shares the clips'
    /// bind group and is reused like them.
    pub(crate) paints: wgpu::Buffer,
    pub(crate) paint_entries: Vec<[f32; 4]>,
    /// The edges of every path of the frame being prepared, laid out as
    /// `layer.wgsl`'s `paths` reads them (see `ShadedPath`); shares the
    /// clips' bind group and is reused like them.
    pub(crate) paths: wgpu::Buffer,
    pub(crate) path_entries: Vec<[f32; 4]>,
    /// The size of the scene being prepared, which paths are cut to.
    pub(crate) scene_size: (u32, u32),
    /// Bound for draws that shade their content (rects) instead of sampling.
    pub(crate) placeholder_texture: LayerTexture,
    pub(crate) asset_root: PathBuf,
    pub(crate) psd_sources: celesta_renderer::psd_source::PsdSources,
    pub(crate) image_sources: celesta_renderer::image_source::ImageSources,
    /// The device's largest 2D texture side. Layer content larger than this
    /// is shrunk before upload and enlarged again by the draw's filtering.
    pub(crate) max_texture_dimension: u32,
    pub(crate) video_decoder: Option<Box<dyn VideoFrameDecoder>>,
    pub(crate) text_rasterizer: TextRasterizer,
    /// Forks of `text_rasterizer` that rasterize a frame's new text in
    /// parallel, rebuilt whenever it loads another font.
    pub(crate) text_workers: Vec<TextRasterizer>,
    /// This frame's text layers waiting for `rasterize_texts`.
    pub(crate) pending_texts: Vec<PendingText>,
    /// The paths `prepare_layer` met in the frame being prepared, for
    /// `outline_paths`.
    pub(crate) pending_paths: Vec<PendingPath>,
    #[cfg(target_os = "macos")]
    pub(crate) native_preview: Option<native_preview::NativePreviewBridge>,
    /// Ring of reusable offscreen texture/readback-buffer pairs behind
    /// `submit`/`drain`. Indices not currently rendering or awaiting readback
    /// sit in `readback_free`; in-flight ones are queued in `readback_order`
    /// (submission order, so `drain`/reclaim always returns frames in order).
    pub(crate) readback_slots: Vec<ReadbackSlot>,
    pub(crate) readback_order: VecDeque<usize>,
    pub(crate) readback_free: Vec<usize>,
    /// Waits for and copies out the in-flight frames, started by the first
    /// `submit`.
    pub(crate) readback_worker: Option<ReadbackWorker>,
    /// The layout newly submitted frames are read back in.
    pub(crate) readback_format: ReadbackFormat,
    /// Created by the first switch to [`ReadbackFormat::Yuv420p`].
    pub(crate) yuv_converter: Option<YuvConverter>,
    /// GPU textures for layer content that is identical from one frame to
    /// the next (images, PSD composites, and text), keyed by what
    /// produced them. A cache hit skips re-rasterizing the text on the
    /// CPU and re-uploading the pixels, which otherwise dominates the cost of
    /// a mostly static frame. Entries a frame does not use are dropped at
    /// the end of that frame's `prepare_draws`.
    pub(crate) textures: HashMap<String, CachedTexture>,
    /// Incremented by every `prepare_draws`; a cache entry's `last_used`
    /// equals it when the frame being prepared uses that entry.
    pub(crate) texture_generation: u64,
    /// `TextRasterizer::loaded_font_count` when the cached text textures
    /// were rasterized; a newly loaded font can change their layout.
    pub(crate) text_font_count: usize,
    /// The last prepared frame's text layers that use a fallback font, one
    /// per family and weight.
    pub(crate) font_fallbacks: Vec<FontFallback>,
    /// The last prepared frame's text layers with characters their family
    /// has no glyph for, one per family, weight, and set of characters.
    pub(crate) missing_glyphs: Vec<MissingGlyphs>,
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
        if !options.driver.is_built_in() {
            return Err(GpuRenderError::DriverUnavailable(options.driver));
        }
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = options.driver.backends();
        descriptor.backend_options.dx12.shader_compiler = dx12_shader_compiler();
        let instance = wgpu::Instance::new(descriptor);
        Self::request_compatible(options, &instance, None).await
    }

    /// `options.driver` is ignored: the adapter comes from `instance`, which
    /// must be the one `surface` was created from.
    pub async fn request_for_surface(
        options: GpuRenderOptions,
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<Self, GpuRenderError> {
        Self::request_compatible(options, instance, Some(surface)).await
    }

    pub(crate) async fn request_compatible(
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
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Celesta layer texture bind group layout"),
                entries: &[
                    // `layer.wgsl` reads with `textureLoad` and filters by
                    // hand, so it can treat texels outside the layer as
                    // transparent and interpolate premultiplied colors.
                    // Filterable for `effect.wgsl`, whose blur lets the
                    // sampler interpolate between taps.
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
                ],
            });
        let backdrop_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Celesta backdrop bind group layout"),
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
        let clip_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Celesta layer clip bind group layout"),
                // The clips, the gradient paints, then the path edges.
                // `vs_main` reads a path's tiles to place them.
                entries: &[0, 1, 2].map(|binding| wgpu::BindGroupLayoutEntry {
                    binding,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }),
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Celesta layer pipeline layout"),
            // Group 1 is the backdrop, which only `fs_blend` reads; group 2
            // holds the clips every pipeline reads.
            bind_group_layouts: &[
                Some(&texture_bind_group_layout),
                None,
                Some(&clip_bind_group_layout),
            ],
            immediate_size: 0,
        });
        let blend_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Celesta blend pipeline layout"),
                bind_group_layouts: &[
                    Some(&texture_bind_group_layout),
                    Some(&backdrop_bind_group_layout),
                    Some(&clip_bind_group_layout),
                ],
                immediate_size: 0,
            });
        let shader = device.create_shader_module(wgpu::include_wgsl!("layer.wgsl"));
        let effects = EffectProcessor::new(&device, &texture_bind_group_layout);
        let masks = MaskPipelines::new(&device, &texture_bind_group_layout);
        let pipeline = create_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            wgpu::TextureFormat::Rgba8Unorm,
            PipelineKind::Layer,
        );
        let mut pipelines = HashMap::new();
        pipelines.insert(
            (wgpu::TextureFormat::Rgba8Unorm, PipelineKind::Layer),
            pipeline,
        );
        let instances = instance_buffer(&device, 1024 * LAYER_INSTANCE_SIZE);
        let clips = clip_buffer(&device, 64 * CLIP_ENTRY_SIZE);
        let paints = clip_buffer(&device, 64 * 16);
        let paths = clip_buffer(&device, 1024 * 16);
        let clip_bind_group =
            clip_bind_group(&device, &clip_bind_group_layout, &clips, &paints, &paths);
        let placeholder_texture = upload_texture(
            &device,
            &queue,
            &texture_bind_group_layout,
            &DecodedImage::new(1, 1, vec![0; BYTES_PER_PIXEL as usize])?,
            false,
        );
        #[cfg(target_os = "macos")]
        let native_preview = native_preview::NativePreviewBridge::new(&device).ok();
        let max_texture_dimension = device.limits().max_texture_dimension_2d;
        Ok(Self {
            adapter,
            device,
            queue,
            adapter_info,
            options,
            shader,
            pipeline_layout,
            blend_pipeline_layout,
            pipelines,
            texture_bind_group_layout,
            backdrop_bind_group_layout,
            render_quality: RenderQuality::default(),
            canvas: None,
            backdrop: None,
            effects,
            masks,
            instances,
            clip_bind_group_layout,
            clips,
            clip_bind_group,
            clip_entries: Vec::new(),
            paints,
            paint_entries: Vec::new(),
            paths,
            path_entries: Vec::new(),
            scene_size: (0, 0),
            placeholder_texture,
            asset_root: PathBuf::from("."),
            psd_sources: Default::default(),
            image_sources: Default::default(),
            max_texture_dimension,
            video_decoder: None,
            text_rasterizer: TextRasterizer::new(),
            text_workers: Vec::new(),
            pending_texts: Vec::new(),
            pending_paths: Vec::new(),
            #[cfg(target_os = "macos")]
            native_preview,
            readback_slots: Vec::new(),
            readback_order: VecDeque::new(),
            readback_free: Vec::new(),
            readback_worker: None,
            readback_format: ReadbackFormat::Rgba8,
            yuv_converter: None,
            textures: HashMap::new(),
            texture_generation: 0,
            text_font_count: 0,
            font_fallbacks: Vec::new(),
            missing_glyphs: Vec::new(),
        })
    }

    /// Text layers in the most recently rendered or submitted frame whose
    /// `fontFamily` has no loaded or installed face, so they use a fallback
    /// font. Each family and weight is listed once, with its first layer.
    pub fn font_fallbacks(&self) -> &[FontFallback] {
        &self.font_fallbacks
    }

    /// Text layers in the most recently rendered or submitted frame with
    /// characters their `fontFamily` has no glyph for, so those characters
    /// use a fallback font. Layers missing the same characters of the same
    /// family and weight are listed once, with the first of them.
    pub fn missing_glyphs(&self) -> &[MissingGlyphs] {
        &self.missing_glyphs
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

    /// [`RenderQuality::Final`] unless set otherwise.
    pub const fn render_quality(&self) -> RenderQuality {
        self.render_quality
    }

    pub const fn with_render_quality(mut self, quality: RenderQuality) -> Self {
        self.render_quality = quality;
        self
    }

    /// Takes effect from the next frame. Scaled text is rasterized again
    /// after a switch, since each quality caches it at a different scale.
    pub const fn set_render_quality(&mut self, quality: RenderQuality) {
        self.render_quality = quality;
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
        if self.readback_worker.is_none() {
            self.readback_worker = Some(ReadbackWorker::new(self.device.clone())?);
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

        let readback = self.readback_slots[slot_index].readback(submission);
        self.readback_worker
            .as_ref()
            .expect("created before the frame was prepared")
            .send(readback);
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

    /// Waits for the oldest in-flight slot's frame from the readback
    /// worker, frees the slot for reuse (also when reading it back failed:
    /// the worker unmaps it either way), and returns the frame.
    pub(crate) fn reclaim_oldest(&mut self) -> Result<GpuFrame, GpuRenderError> {
        let slot_index = self
            .readback_order
            .pop_front()
            .expect("reclaim_oldest called with no in-flight frame");
        let frame = self
            .readback_worker
            .as_ref()
            .expect("submit starts the worker before a frame is in flight")
            .receive();
        self.readback_free.push(slot_index);
        frame
    }

    pub(crate) fn readback_slot(
        &self,
        width: u32,
        height: u32,
    ) -> Result<ReadbackSlot, GpuRenderError> {
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
}

/// DXC when `dxcompiler.dll` sits next to the executable, FXC otherwise.
/// wgpu's own default also searches `PATH`, where an older DXC (from the
/// Windows SDK or another app) fails to load instead of falling back to FXC.
/// `WGPU_DX12_COMPILER=fxc|dxc` overrides the choice.
fn dx12_shader_compiler() -> wgpu::Dx12Compiler {
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("dxcompiler.dll")))
        .filter(|dll| dll.is_file());
    match bundled {
        Some(dll) => wgpu::Dx12Compiler::DynamicDxc {
            dxc_path: dll.to_string_lossy().into_owned(),
        },
        None => wgpu::Dx12Compiler::Fxc,
    }
    .with_env()
}
