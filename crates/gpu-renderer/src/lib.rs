//! GPU rendering foundation built on `wgpu`.
//!
//! It supports offscreen image, video, and text composition with nested
//! transforms, opacity, painter ordering, and deterministic RGBA readback.
//! Unsupported content returns an error instead of silently disappearing.

use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;

use celesta_composition::{
    BlendMode, Clip, EvaluatedTransform, Layer, LayerContent, LayerEffects, Point, ResolvedAsset,
    Scene, TextStyle,
};
use celesta_media::{MediaError, VideoFrameDecoder};
use celesta_remote::{RemoteAssetError, resolve_asset_path};
use rayon::prelude::*;
use celesta_renderer::{
    Color as CpuColor, FlattenedPath, FontFallback, LineSegment, MissingGlyphs, PathShape,
    PathTransform, RectPaint, RenderError, ResolvedPaint, TextRasterizer, flatten_path,
    image_source::{fit_within, resize_rgba},
    resolve_rect_paint,
};

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

/// Evaluates an unparented Path layer's transform using the GPU renderer's
/// layer evaluator and precision. Path commands use local coordinates;
/// the layer's anchor does not offset them.
pub fn path_transform(transform: &EvaluatedTransform) -> PathTransform {
    LayerState::default().then(transform, 1.0).transform.into()
}

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

/// How much work the renderer spends on layers that are scaled or rotated.
///
/// Untransformed layers look the same in both: they are placed on whole
/// pixels and copied texel for texel. Transformed layers are always filtered
/// (bilinear, trilinear from mipmaps for cached images). The modes differ only
/// in the work that is too slow to repeat for every frame of playback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderQuality {
    /// For interactive playback: text is rasterized once at scale 1 and
    /// filtered when it is scaled, and video frames get no mipmaps.
    Draft,
    /// What exports use: text is rasterized at the scale it is drawn at
    /// (rounded up to the next eighth of an octave, so an animated scale reuses
    /// a handful of textures), and shrunk video frames get mipmaps.
    #[default]
    Final,
}

impl RenderQuality {
    pub const ALL: [Self; 2] = [Self::Draft, Self::Final];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Final => "final",
        }
    }
}

impl fmt::Display for RenderQuality {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for RenderQuality {
    type Err = UnknownRenderQuality;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|quality| quality.as_str() == name)
            .ok_or_else(|| UnknownRenderQuality(name.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownRenderQuality(String);

impl fmt::Display for UnknownRenderQuality {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown render quality '{}' (expected draft or final)",
            self.0
        )
    }
}

impl Error for UnknownRenderQuality {}

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
    /// Layout of `PipelineKind::Blend` pipelines: the layer texture, then
    /// the backdrop.
    blend_pipeline_layout: wgpu::PipelineLayout,
    pipelines: HashMap<(wgpu::TextureFormat, PipelineKind), wgpu::RenderPipeline>,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    backdrop_bind_group_layout: wgpu::BindGroupLayout,
    render_quality: RenderQuality,
    /// The scene-sized texture a frame that uses blend modes or effects
    /// composites in. Isolated groups and effects draw onto smaller canvases
    /// from `effects`' pool. Reused across frames of the same size.
    canvas: Option<CanvasTexture>,
    /// What a blended draw reads its backdrop from: a copy of the canvas it
    /// draws onto, taken just before the draw.
    backdrop: Option<BackdropTexture>,
    effects: EffectProcessor,
    /// Every layer's `LayerInstance` for the frame being prepared, reused
    /// (and grown when a frame needs more) across frames.
    instances: wgpu::Buffer,
    /// Layout of the bind group that exposes `clips` to the fragment shader.
    clip_bind_group_layout: wgpu::BindGroupLayout,
    /// Every clip of the frame being prepared, as `CLIP_ENTRY_FLOATS` floats
    /// each, reused (and grown when a frame needs more) like `instances`.
    clips: wgpu::Buffer,
    clip_bind_group: wgpu::BindGroup,
    /// The clips the frame being prepared has entered so far.
    clip_entries: Vec<ClipEntry>,
    /// Every gradient a rect of the frame being prepared is painted with,
    /// laid out as `layer.wgsl`'s `paints` reads them; shares the clips'
    /// bind group and is reused like them.
    paints: wgpu::Buffer,
    paint_entries: Vec<[f32; 4]>,
    /// The edges of every path of the frame being prepared, laid out as
    /// `layer.wgsl`'s `paths` reads them (see `ShadedPath`); shares the
    /// clips' bind group and is reused like them.
    paths: wgpu::Buffer,
    path_entries: Vec<[f32; 4]>,
    /// The size of the scene being prepared, which paths are cut to.
    scene_size: (u32, u32),
    /// Bound for draws that shade their content (rects) instead of sampling.
    placeholder_texture: LayerTexture,
    asset_root: PathBuf,
    psd_sources: celesta_renderer::psd_source::PsdSources,
    image_sources: celesta_renderer::image_source::ImageSources,
    /// The device's largest 2D texture side. Layer content larger than this
    /// is shrunk before upload and enlarged again by the draw's filtering.
    max_texture_dimension: u32,
    video_decoder: Option<Box<dyn VideoFrameDecoder>>,
    text_rasterizer: TextRasterizer,
    /// Forks of `text_rasterizer` that rasterize a frame's new text in
    /// parallel, rebuilt whenever it loads another font.
    text_workers: Vec<TextRasterizer>,
    /// This frame's text layers waiting for `resolve_pending_texts`.
    pending_texts: Vec<PendingText>,
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
    /// the next (images, PSD composites, and text), keyed by what
    /// produced them. A cache hit skips re-rasterizing the text on the
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
    /// The last prepared frame's text layers that use a fallback font, one
    /// per family and weight.
    font_fallbacks: Vec<FontFallback>,
    /// The last prepared frame's text layers with characters their family
    /// has no glyph for, one per family, weight, and set of characters.
    missing_glyphs: Vec<MissingGlyphs>,
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

    fn prepare_draws(&mut self, scene: &Scene) -> Result<PreparedDraws, GpuRenderError> {
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
            self.text_workers.clear();
            self.text_font_count = font_count;
        }
        self.pending_texts.clear();
        self.texture_generation += 1;
        self.clip_entries.clear();
        self.paint_entries.clear();
        self.path_entries.clear();
        self.scene_size = (scene.width, scene.height);
        self.font_fallbacks.clear();
        self.missing_glyphs.clear();
        let mut items = Vec::new();
        let prepared = scene
            .layers
            .iter()
            .try_for_each(|layer| self.prepare_layer(layer, LayerState::default(), &mut items));
        // Evict what this frame did not use, also when it failed part way
        // (the entries it did reach are still marked as used).
        let generation = self.texture_generation;
        self.textures
            .retain(|_, cached| cached.last_used == generation);
        prepared?;
        self.resolve_pending_texts(&mut items)?;

        // A frame that blends anything but source-over composites through
        // scene-sized canvases, so its layers need one more instance: the
        // one that copies the finished root canvas onto the target.
        let composited = items.iter().any(
            |item| !matches!(item, PreparedItem::Layer(layer) if layer.blend_mode.is_normal()),
        );
        let instance_count = items
            .iter()
            .filter(|item| !matches!(item, PreparedItem::BeginGroup))
            .count()
            + items
                .iter()
                .filter(|item| matches!(item, PreparedItem::EndEffect(..)))
                .count()
            + usize::from(composited);

        // One buffer write for every layer, and one instanced draw for each
        // run of layers that sample the same texture, rather than a buffer,
        // a bind group, and a draw per layer: a dense scene has thousands of
        // layers, and consecutive rects all share the placeholder texture.
        let size = instance_count as u64 * LAYER_INSTANCE_SIZE;
        if size > self.device.limits().max_buffer_size || u32::try_from(instance_count).is_err() {
            return Err(GpuRenderError::TooManyLayers(instance_count));
        }
        let root = CanvasRegion::scene(scene.width, scene.height);
        let mut groups = plan_groups(&items, root).into_iter();
        let mut instances = Vec::with_capacity(size as usize);
        let mut steps: Vec<GpuStep> = Vec::new();
        let mut index = 0_u32;
        // The canvas each open group draws onto, the root canvas first.
        let mut open = vec![GroupPlan {
            canvas: root,
            content: None,
            drawn: true,
        }];
        for item in items {
            let target = open.last().expect("the root canvas is always open").canvas;
            let layer = match item {
                PreparedItem::Layer(layer) => layer,
                PreparedItem::PendingText => unreachable!("pending text is resolved"),
                PreparedItem::BeginGroup => {
                    let group = groups.next().expect("plan_groups plans every group");
                    steps.push(GpuStep::BeginGroup {
                        canvas: group.canvas,
                    });
                    open.push(group);
                    continue;
                }
                PreparedItem::EndGroup(layer) => {
                    let group = open.pop().expect("every group was begun");
                    let target = open.last().expect("the root canvas is always open").canvas;
                    layer.write_instance(target, group.canvas, &mut instances);
                    steps.push(GpuStep::EndGroup {
                        instance: index,
                        blend_mode: layer.blend_mode,
                        area: group
                            .drawn
                            .then(|| target.local_area(group.canvas.bounds()))
                            .flatten(),
                    });
                    index += 1;
                    continue;
                }
                PreparedItem::EndEffect(layer, effects) => {
                    let group = open.pop().expect("every group was begun");
                    let target = open.last().expect("the root canvas is always open").canvas;
                    PreparedLayer::canvas(LayerState::default(), BlendMode::Normal, true)
                        .write_instance(group.canvas, group.canvas, &mut instances);
                    layer.write_instance(target, group.canvas, &mut instances);
                    steps.push(GpuStep::EndEffect {
                        inner_instance: index,
                        final_instance: index + 1,
                        blend_mode: layer.blend_mode,
                        effects,
                        content: group
                            .content
                            .filter(|_| group.drawn)
                            .map(|content| group.canvas.local(content)),
                        area: target.local_area(group.canvas.bounds()),
                    });
                    index += 2;
                    continue;
                }
            };
            layer.write_instance(target, target, &mut instances);
            let blend_mode = layer.blend_mode;
            let area = (!blend_mode.is_normal())
                .then(|| target.local_area(layer.bounds()))
                .flatten();
            let vertices = match &layer.content {
                PreparedContent::Path(path) => 0..path.tiles * 6,
                _ => 0..6,
            };
            let texture = match layer.content {
                PreparedContent::Texture(texture) => texture,
                PreparedContent::Rect(_) | PreparedContent::Path(_) => {
                    self.placeholder_texture.clone()
                }
                PreparedContent::Canvas { .. } => unreachable!("only a group's end draws a canvas"),
            };
            let draw = GpuDraw {
                texture,
                vertices,
                instances: index..index + 1,
            };
            if !blend_mode.is_normal() {
                steps.push(GpuStep::Blend {
                    draw,
                    blend_mode,
                    area,
                });
            } else {
                match steps.last_mut() {
                    Some(GpuStep::Draw(last))
                        if last.texture.bind_group == draw.texture.bind_group
                            && last.vertices == (0..6)
                            && draw.vertices == (0..6) =>
                    {
                        last.instances.end = index + 1;
                    }
                    _ => steps.push(GpuStep::Draw(draw)),
                }
            }
            index += 1;
        }
        let composite =
            composited.then(|| {
                PreparedLayer::canvas(LayerState::default(), BlendMode::Normal, false)
                    .write_instance(root, root, &mut instances);
                CompositePlan {
                    blit_instance: index,
                }
            });
        if size > self.instances.size() {
            // In-flight frames keep the old buffer alive until they finish.
            self.instances = instance_buffer(&self.device, size.next_power_of_two());
        }
        if !instances.is_empty() {
            // Safe while earlier frames that read this buffer are still in
            // flight: queued writes land after previously submitted work.
            self.queue.write_buffer(&self.instances, 0, &instances);
        }

        // The clips and paints the layers above point into, uploaded like
        // the instances.
        let clip_size = self.clip_entries.len() as u64 * CLIP_ENTRY_SIZE;
        let paint_size = self.paint_entries.len() as u64 * 16;
        let path_size = self.path_entries.len() as u64 * 16;
        // The largest path buffer the device can create and bind, in whole
        // entries.
        let limits = self.device.limits();
        let path_limit = limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size)
            & !15;
        if path_size > path_limit {
            return Err(GpuRenderError::PathsTooComplex(self.path_entries.len()));
        }
        if clip_size > self.clips.size()
            || paint_size > self.paints.size()
            || path_size > self.paths.size()
        {
            if clip_size > self.clips.size() {
                self.clips = clip_buffer(&self.device, clip_size.next_power_of_two());
            }
            if paint_size > self.paints.size() {
                self.paints = clip_buffer(&self.device, paint_size.next_power_of_two());
            }
            if path_size > self.paths.size() {
                self.paths =
                    clip_buffer(&self.device, path_size.next_power_of_two().min(path_limit));
            }
            self.clip_bind_group = clip_bind_group(
                &self.device,
                &self.clip_bind_group_layout,
                &self.clips,
                &self.paints,
                &self.paths,
            );
        }
        if !self.path_entries.is_empty() {
            self.queue
                .write_buffer(&self.paths, 0, bytemuck::cast_slice(&self.path_entries));
        }
        if !self.paint_entries.is_empty() {
            let bytes: Vec<u8> = self
                .paint_entries
                .iter()
                .flatten()
                .flat_map(|value| value.to_ne_bytes())
                .collect();
            self.queue.write_buffer(&self.paints, 0, &bytes);
        }
        if !self.clip_entries.is_empty() {
            let bytes: Vec<u8> = self
                .clip_entries
                .iter()
                .flat_map(ClipEntry::floats)
                .flat_map(f32::to_ne_bytes)
                .collect();
            self.queue.write_buffer(&self.clips, 0, &bytes);
        }
        Ok(PreparedDraws {
            instances: self.instances.clone(),
            clips: self.clip_bind_group.clone(),
            steps,
            composite,
        })
    }

    fn encode_draws(
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

    /// Draws a frame that uses blend modes, isolated groups or effects onto
    /// the scene-sized root canvas (each group through a canvas of its own),
    /// leaving the result in `self.canvas` for `encode_draws` to copy onto
    /// the target. Canvases hold premultiplied alpha, which is what
    /// source-over blending onto a cleared texture produces.
    fn encode_composited(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        draws: &PreparedDraws,
    ) {
        const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
        self.prepare_canvases(scene.width, scene.height);
        self.ensure_pipeline(FORMAT, PipelineKind::Layer);
        self.ensure_pipeline(FORMAT, PipelineKind::Blend);
        // Each effect filters through at most two passes per shadow, glow
        // and blur.
        let effects = draws
            .steps
            .iter()
            .filter(|step| matches!(step, GpuStep::EndEffect { .. }))
            .count();
        self.effects.begin_frame(&self.device, effects * 6);
        let root = self.canvas.clone().expect("prepare_canvases creates it");
        let mut compositor = Compositor {
            device: &self.device,
            texture_layout: &self.texture_bind_group_layout,
            layer_pipeline: &self.pipelines[&(FORMAT, PipelineKind::Layer)],
            blend_pipeline: &self.pipelines[&(FORMAT, PipelineKind::Blend)],
            backdrop: self.backdrop.as_ref().expect("prepare_canvases creates it"),
            draws,
            effects: &mut self.effects,
        };
        compositor.draw_canvas(
            encoder,
            &root,
            self.options.background.as_wgpu(),
            &draws.steps,
        );
        self.effects.end_frame(&self.queue);
    }

    /// Makes sure the root canvas and the backdrop exist at `width`x`height`.
    fn prepare_canvases(&mut self, width: u32, height: u32) {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        if self
            .canvas
            .as_ref()
            .is_none_or(|canvas| canvas.texture.size() != size)
        {
            self.canvas = Some(canvas_texture(
                &self.device,
                &self.texture_bind_group_layout,
                width,
                height,
            ));
        }
        if self
            .backdrop
            .as_ref()
            .is_none_or(|backdrop| backdrop.texture.size() != size)
        {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Celesta backdrop"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Celesta backdrop bind group"),
                layout: &self.backdrop_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            });
            self.backdrop = Some(BackdropTexture {
                texture,
                bind_group,
            });
        }
    }

    fn ensure_pipeline(&mut self, format: wgpu::TextureFormat, kind: PipelineKind) {
        self.pipelines.entry((format, kind)).or_insert_with(|| {
            let layout = match kind {
                PipelineKind::Blend => &self.blend_pipeline_layout,
                PipelineKind::Layer | PipelineKind::Blit => &self.pipeline_layout,
            };
            create_pipeline(&self.device, layout, &self.shader, format, kind)
        });
    }

    fn prepare_layer(
        &mut self,
        layer: &Layer,
        parent: LayerState,
        output: &mut Vec<PreparedItem>,
    ) -> Result<(), GpuRenderError> {
        let state = parent.then(&layer.transform, layer.opacity);
        if state.opacity == 0.0 || state.transform.is_degenerate() {
            return Ok(());
        }
        let blend_mode = layer.blend_mode;
        if !layer.effects.is_empty() {
            let effects = EffectSpec::parse(&layer.effects)?;
            output.push(PreparedItem::BeginGroup);
            let mut inner = layer.clone();
            inner.opacity = 1.0;
            inner.blend_mode = BlendMode::Normal;
            inner.effects = LayerEffects::default();
            self.prepare_layer(
                &inner,
                LayerState {
                    opacity: 1.0,
                    ..parent
                },
                output,
            )?;
            output.push(PreparedItem::EndEffect(
                PreparedLayer::canvas(
                    LayerState {
                        transform: Affine::IDENTITY,
                        opacity: state.opacity,
                        clip: parent.clip,
                    },
                    blend_mode,
                    true,
                ),
                effects,
            ));
            return Ok(());
        }
        match &layer.content {
            LayerContent::Group { layers, clip } => {
                let mut child_state = state;
                if let Some(clip) = clip {
                    if clip.is_empty() {
                        return Ok(());
                    }
                    child_state.clip = Some(self.push_clip(clip, state)?);
                }
                if blend_mode.is_normal() {
                    for child in layers {
                        self.prepare_layer(child, child_state, output)?;
                    }
                    return Ok(());
                }
                // Isolated: the children draw onto a transparent canvas of
                // their own, which then blends onto its parent as one layer.
                // The children carry the clip; the canvas itself does not.
                output.push(PreparedItem::BeginGroup);
                let inner = LayerState {
                    opacity: 1.0,
                    ..child_state
                };
                for child in layers {
                    self.prepare_layer(child, inner, output)?;
                }
                output.push(PreparedItem::EndGroup(PreparedLayer::canvas(
                    LayerState {
                        transform: Affine::IDENTITY,
                        opacity: state.opacity,
                        clip: None,
                    },
                    blend_mode,
                    true,
                )));
            }
            LayerContent::Image {
                asset,
                width,
                height,
                fit,
            } => {
                // Largest singular value also covers shear from rotated,
                // non-uniformly scaled parent groups.
                let m = state.transform;
                let x = (m.a as f64).powi(2) + (m.b as f64).powi(2);
                let y = (m.c as f64).powi(2) + (m.d as f64).powi(2);
                let dot = m.a as f64 * m.c as f64 + m.b as f64 * m.d as f64;
                let density = ((x + y + ((x - y).powi(2) + 4.0 * dot * dot).sqrt()) / 2.0).sqrt();
                let path = self.local_asset_path(asset)?;
                let display = self
                    .image_sources
                    .render(&asset.id, &path, *width, *height, *fit, density)
                    .map_err(|source| GpuRenderError::ImageDecode {
                        asset: asset.id.clone(),
                        source,
                    })?;
                let (texture_width, texture_height) = fit_within(
                    display.pixels.width(),
                    display.pixels.height(),
                    self.max_texture_dimension,
                );
                let mut state = state;
                state.transform.a *= (display.width / f64::from(texture_width)) as f32;
                state.transform.b *= (display.width / f64::from(texture_width)) as f32;
                state.transform.c *= (display.height / f64::from(texture_height)) as f32;
                state.transform.d *= (display.height / f64::from(texture_height)) as f32;
                let texture = self.cached_texture(
                    // Both sizes: SVG rasterized at different densities can
                    // shrink to the same texture size from different pixels.
                    format!(
                        "image\0{}\0{:?}\0{:?}\0{:?}\0{}x{}\0{}x{}",
                        asset.id,
                        width,
                        height,
                        fit,
                        display.pixels.width(),
                        display.pixels.height(),
                        texture_width,
                        texture_height
                    ),
                    // SVG is already rasterized for this draw; mipmaps blur
                    // its downscaled edges relative to the CPU renderer.
                    !display.is_svg,
                    |_| {
                        let pixels =
                            if (texture_width, texture_height) == display.pixels.dimensions() {
                                display.pixels.as_ref().clone()
                            } else {
                                resize_rgba(
                                    display.pixels.width(),
                                    display.pixels.height(),
                                    display.pixels.as_raw(),
                                    texture_width,
                                    texture_height,
                                )
                            };
                        DecodedImage::new(texture_width, texture_height, pixels.into_raw())
                    },
                )?;
                output.push(PreparedItem::Layer(PreparedLayer::new(
                    texture,
                    layer.transform.anchor,
                    state,
                    blend_mode,
                )));
            }
            LayerContent::Psd {
                asset,
                visible_layers,
                enabled_layers,
                disabled_layers,
            } => {
                let path = self.local_asset_path(asset)?;
                let density = f64::from(state.transform.stretch().0);
                let image = self
                    .psd_sources
                    .render_within(
                        &asset.id,
                        &path,
                        visible_layers,
                        enabled_layers,
                        disabled_layers,
                        density,
                        self.max_texture_dimension,
                    )
                    .map_err(GpuRenderError::Psd)?;
                let mut state = state;
                let x = (f64::from(image.canvas_width) / f64::from(image.width)) as f32;
                let y = (f64::from(image.canvas_height) / f64::from(image.height)) as f32;
                state.transform.a *= x;
                state.transform.b *= x;
                state.transform.c *= y;
                state.transform.d *= y;
                let texture = self.cached_texture(
                    // Keyed like the composite: a composite shrunk to the
                    // limit can match another level's size with other pixels.
                    format!(
                        "{}\0{}\0{}\0{}x{}",
                        psd_key(asset, visible_layers, enabled_layers, disabled_layers),
                        celesta_renderer::psd_source::level_for(density),
                        self.max_texture_dimension,
                        image.width,
                        image.height
                    ),
                    true,
                    |_| DecodedImage::shared(image.width, image.height, image.pixels.clone()),
                )?;
                output.push(PreparedItem::Layer(PreparedLayer::new(
                    texture,
                    layer.transform.anchor,
                    state,
                    blend_mode,
                )));
            }
            LayerContent::Video { asset, timing } => {
                let path = self.local_asset_path(asset)?;
                let decoder = self
                    .video_decoder
                    .as_mut()
                    .ok_or_else(|| GpuRenderError::MissingVideoDecoder(layer.id.clone()))?;
                let frame =
                    decoder.decode_frame_for(&layer.id, &path, timing.source_time_seconds)?;
                let (width, height) =
                    fit_within(frame.width, frame.height, self.max_texture_dimension);
                // Checked against its size before anything reads it.
                let mut image = DecodedImage::shared(frame.width, frame.height, frame.pixels)?;
                if (width, height) != (frame.width, frame.height) {
                    let pixels =
                        resize_rgba(frame.width, frame.height, &image.pixels, width, height);
                    image = DecodedImage::new(width, height, pixels.into_raw())?;
                }
                let mut state = state;
                let x = (f64::from(frame.width) / f64::from(width)) as f32;
                let y = (f64::from(frame.height) / f64::from(height)) as f32;
                state.transform.a *= x;
                state.transform.b *= x;
                state.transform.c *= y;
                state.transform.d *= y;
                // Every frame brings new pixels, so video is never cached,
                // and its mipmaps would be rebuilt every frame: only a final
                // render of a frame shrunk to half size or less pays for them.
                let mipmaps = self.render_quality == RenderQuality::Final
                    && state.transform.texels_per_pixel(1.0) >= 2.0;
                let texture = self.upload_texture(&image, mipmaps);
                output.push(PreparedItem::Layer(PreparedLayer::new(
                    texture,
                    layer.transform.anchor,
                    state,
                    blend_mode,
                )));
            }
            LayerContent::Text {
                text,
                style,
                max_width,
                baseline_anchor,
            } => {
                if let Some(fallback) = self.text_rasterizer.font_fallback(&layer.id, style) {
                    if !self.font_fallbacks.iter().any(|reported| {
                        reported.family == fallback.family && reported.weight == fallback.weight
                    }) {
                        self.font_fallbacks.push(fallback);
                    }
                } else if let Some(missing) =
                    self.text_rasterizer.missing_glyphs(&layer.id, text, style)
                    && !self.missing_glyphs.iter().any(|reported| {
                        reported.family == missing.family
                            && reported.weight == missing.weight
                            && reported.characters == missing.characters
                    })
                {
                    self.missing_glyphs.push(missing);
                }
                let raster_scale = match self.render_quality {
                    RenderQuality::Draft => 1.0,
                    RenderQuality::Final => text_raster_scale(state.transform),
                };
                // `{:?}` spells out every style field and prints floats
                // exactly, so equal keys always mean equal rasterizer input.
                let key = format!(
                    "{TEXT_TEXTURE_PREFIX}{text}\0{style:?}\0{max_width:?}\0{raster_scale:?}"
                );
                let generation = self.texture_generation;
                if let Some(cached) = self.textures.get_mut(&key) {
                    cached.last_used = generation;
                    output.push(PreparedItem::Layer(text_layer(
                        cached.texture.clone(),
                        layer.transform.anchor,
                        *baseline_anchor,
                        state,
                        blend_mode,
                    )));
                } else {
                    // Rasterized with the frame's other new text once every
                    // layer is prepared; see `resolve_pending_texts`.
                    self.pending_texts.push(PendingText {
                        item: output.len(),
                        key,
                        text: text.clone(),
                        style: style.clone(),
                        max_width: *max_width,
                        raster_scale,
                        anchor: layer.transform.anchor,
                        baseline_anchor: *baseline_anchor,
                        state,
                        blend_mode,
                    });
                    output.push(PreparedItem::PendingText);
                }
            }
            LayerContent::Rect {
                width,
                height,
                fill,
                stroke,
                corner_radius,
            } => {
                // Shaded on the GPU rather than rasterized into a texture:
                // animated rects change size (and gradients colors) every
                // frame, and a texture per rect per frame dominates the cost
                // of dense geometry and of full-screen gradients alike.
                let paint = resolve_rect_paint(fill.as_ref(), stroke.as_ref())
                    .map_err(GpuRenderError::Text)?;
                output.push(PreparedItem::Layer(PreparedLayer {
                    content: PreparedContent::Rect(RectShape::new(
                        *width,
                        *height,
                        *corner_radius,
                        paint,
                        &mut self.paint_entries,
                    )),
                    anchor: layer.transform.anchor,
                    state,
                    blend_mode,
                    raster_scale: 1.0,
                }));
            }
            LayerContent::Path {
                commands,
                fill,
                stroke,
                line_cap,
                line_join,
                miter_limit,
            } => {
                // Outlined and flattened on the CPU, with the whole
                // transform applied to the geometry exactly as the CPU
                // renderer's rasterizer does; the coverage of every output
                // pixel is then shaded by `layer.wgsl`, so an animated path
                // costs neither a rasterization nor an upload per frame.
                let shape = PathShape {
                    commands,
                    fill: fill.as_ref(),
                    stroke: stroke.as_ref(),
                    line_cap: *line_cap,
                    line_join: *line_join,
                    miter_limit: *miter_limit,
                };
                let (width, height) = self.scene_size;
                let Some(path) = flatten_path(&shape, state.transform.into(), width, height)
                    .map_err(GpuRenderError::Text)?
                else {
                    return Ok(());
                };
                let (left, top) = (path.left as f32, path.top as f32);
                let shaded =
                    ShadedPath::new(&path, &mut self.paint_entries, &mut self.path_entries);
                if shaded.tiles == 0 {
                    return Ok(());
                }
                output.push(PreparedItem::Layer(PreparedLayer {
                    content: PreparedContent::Path(shaded),
                    anchor: Point { x: 0.0, y: 0.0 },
                    state: LayerState {
                        transform: Affine {
                            tx: left,
                            ty: top,
                            ..Affine::IDENTITY
                        },
                        ..state
                    },
                    blend_mode,
                    raster_scale: 1.0,
                }));
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

    /// Enters `clip`, defined in the frame `state` describes (the group's
    /// own), inside the clip `state` is already drawn through, and returns
    /// its index for the layers below it.
    fn push_clip(&mut self, clip: &Clip, state: LayerState) -> Result<u32, GpuRenderError> {
        let depth = state
            .clip
            .map_or(1, |parent| self.clip_entries[parent as usize].depth + 1);
        if depth > MAX_CLIP_DEPTH {
            return Err(GpuRenderError::ClipsNestedTooDeep(depth));
        }
        let index = u32::try_from(self.clip_entries.len())
            .map_err(|_| GpuRenderError::TooManyLayers(self.clip_entries.len()))?;
        self.clip_entries
            .push(ClipEntry::new(clip, state.transform, state.clip, depth));
        Ok(index)
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
    /// being prepared, or produces and uploads it on a miss. Cached textures
    /// are reused for many frames, so they get mipmaps when `mipmaps` is set.
    fn cached_texture(
        &mut self,
        key: String,
        mipmaps: bool,
        produce: impl FnOnce(&mut Self) -> Result<DecodedImage, GpuRenderError>,
    ) -> Result<LayerTexture, GpuRenderError> {
        let generation = self.texture_generation;
        if let Some(cached) = self.textures.get_mut(&key) {
            cached.last_used = generation;
            return Ok(cached.texture.clone());
        }
        let image = produce(self)?;
        let texture = self.upload_texture(&image, mipmaps);
        self.textures.insert(
            key,
            CachedTexture {
                texture: texture.clone(),
                last_used: generation,
            },
        );
        Ok(texture)
    }

    /// Rasterizes the text `prepare_layer` found missing from the cache and
    /// puts its layers in place of their `PendingText` items. A frame whose
    /// labels or scale change each frame misses on dozens of texts, and each
    /// rasterization is independent, so they spread over rayon's threads,
    /// each with its own fork of the text rasterizer.
    fn resolve_pending_texts(&mut self, items: &mut [PreparedItem]) -> Result<(), GpuRenderError> {
        let pending = std::mem::take(&mut self.pending_texts);
        if pending.is_empty() {
            return Ok(());
        }
        let mut seen = HashSet::new();
        let jobs: Vec<&PendingText> = pending
            .iter()
            .filter(|text| seen.insert(text.key.as_str()))
            .collect();
        let limit = self.max_texture_dimension;
        let images: Vec<_> = if jobs.len() == 1 {
            vec![rasterize_text(&mut self.text_rasterizer, jobs[0], limit)]
        } else {
            let workers = jobs.len().min(rayon::current_num_threads());
            while self.text_workers.len() < workers {
                self.text_workers.push(self.text_rasterizer.fork());
            }
            let chunk = jobs.len().div_ceil(workers);
            self.text_workers
                .par_iter_mut()
                .zip(jobs.par_chunks(chunk))
                .flat_map_iter(|(rasterizer, jobs)| {
                    jobs.iter()
                        .map(|job| rasterize_text(rasterizer, job, limit))
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        let generation = self.texture_generation;
        for (job, image) in jobs.iter().zip(images) {
            let texture = self.upload_texture(&image?, false);
            self.textures.insert(
                job.key.clone(),
                CachedTexture {
                    texture,
                    last_used: generation,
                },
            );
        }
        for text in pending {
            let texture = self.textures[&text.key].texture.clone();
            items[text.item] = PreparedItem::Layer(text_layer(
                texture,
                text.anchor,
                text.baseline_anchor,
                text.state,
                text.blend_mode,
            ));
        }
        Ok(())
    }

    fn upload_texture(&self, image: &DecodedImage, mipmaps: bool) -> LayerTexture {
        upload_texture(
            &self.device,
            &self.queue,
            &self.texture_bind_group_layout,
            image,
            mipmaps,
        )
    }
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    image: &DecodedImage,
    mipmaps: bool,
) -> LayerTexture {
    let mut levels = vec![(image.width, image.height, Arc::clone(&image.pixels))];
    while mipmaps {
        let (width, height, pixels) = levels.last().expect("level 0 exists");
        if *width == 1 && *height == 1 {
            break;
        }
        let next = downsample(*width, *height, pixels);
        levels.push((next.0, next.1, Arc::new(next.2)));
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Celesta layer texture"),
        size: wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, (width, height, pixels)) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * BYTES_PER_PIXEL),
                rows_per_image: Some(*height),
            },
            wgpu::Extent3d {
                width: *width,
                height: *height,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Celesta layer texture bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    LayerTexture {
        _texture: texture,
        bind_group,
        width: image.width,
        height: image.height,
        baseline_anchor: image.baseline_anchor,
        anchor_origin: image.anchor_origin,
        anchor_span: image.anchor_span,
        raster_scale: image.raster_scale,
    }
}

/// The next mip level of straight-alpha RGBA pixels: each texel averages the
/// 2x2 block above it (the last row or column of an odd size folds into its
/// neighbour), weighting color by alpha so transparent texels do not darken
/// the edges of what they surround. Sizes halve rounding down, as wgpu
/// expects of mip levels.
fn downsample(width: u32, height: u32, pixels: &[u8]) -> (u32, u32, Vec<u8>) {
    let next_width = (width / 2).max(1);
    let next_height = (height / 2).max(1);
    let mut next = vec![0; (next_width * next_height * BYTES_PER_PIXEL) as usize];
    let span = |index: u32, size: u32, next_size: u32| {
        let start = index * size / next_size;
        let end = ((index + 1) * size / next_size).max(start + 1);
        start..end
    };
    for y in 0..next_height {
        for x in 0..next_width {
            let mut color = [0_u32; 3];
            let mut alpha = 0_u32;
            let mut count = 0_u32;
            for source_y in span(y, height, next_height) {
                for source_x in span(x, width, next_width) {
                    let offset = ((source_y * width + source_x) * BYTES_PER_PIXEL) as usize;
                    let texel = &pixels[offset..offset + 4];
                    let weight = u32::from(texel[3]);
                    for channel in 0..3 {
                        color[channel] += u32::from(texel[channel]) * weight;
                    }
                    alpha += weight;
                    count += 1;
                }
            }
            let offset = ((y * next_width + x) * BYTES_PER_PIXEL) as usize;
            // A fully transparent block keeps a transparent black texel.
            for channel in 0..3 {
                if let Some(average) = (color[channel] + alpha / 2).checked_div(alpha) {
                    next[offset + channel] = average as u8;
                }
            }
            next[offset + 3] = ((alpha + count / 2) / count) as u8;
        }
    }
    (next_width, next_height, next)
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

/// How a pipeline writes its fragments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PipelineKind {
    /// Source-over onto the target.
    Layer,
    /// Replaces the target: copies a finished canvas onto it.
    Blit,
    /// Blends with a backdrop copy of the target (`fs_blend`), replacing it.
    Blend,
}

fn create_pipeline(
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

#[derive(Clone)]
struct DecodedImage {
    width: u32,
    height: u32,
    /// `Arc<Vec<u8>>` rather than `Arc<[u8]>`: the latter cannot take ownership
    /// of an existing `Vec` and copies it instead, which costs a full 8MB
    /// memcpy for every 1080p video frame the preview decodes — several
    /// milliseconds per frame for nothing, since the buffer is already owned.
    pixels: Arc<Vec<u8>>,
    /// Normalized anchor `y` of the first text baseline; 0 for non-text images.
    baseline_anchor: f64,
    /// Maps a layer anchor onto the image: `origin + anchor * span`, both
    /// normalized. Text can be larger than the box its anchor refers to (a
    /// stroke reaches past it); everything else is `(0, 0)` and `(1, 1)`.
    anchor_origin: (f64, f64),
    anchor_span: (f64, f64),
    /// Texels per layer unit: the scale text was rasterized at, else 1.
    raster_scale: f32,
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
            baseline_anchor: 0.0,
            anchor_origin: (0.0, 0.0),
            anchor_span: (1.0, 1.0),
            raster_scale: 1.0,
        })
    }
}

/// An uploaded layer texture. Cloning it clones the `wgpu` handles, not the
/// pixels, so a cached texture can back draws in several in-flight frames.
#[derive(Clone)]
struct LayerTexture {
    _texture: wgpu::Texture,
    /// The texture, bound as group 0.
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    /// See `DecodedImage::baseline_anchor`.
    baseline_anchor: f64,
    /// See `DecodedImage::anchor_origin`.
    anchor_origin: (f64, f64),
    anchor_span: (f64, f64),
    /// See `DecodedImage::raster_scale`.
    raster_scale: f32,
}

struct CachedTexture {
    texture: LayerTexture,
    last_used: u64,
}

#[derive(Clone)]
struct CanvasTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Samples the canvas as a layer texture (group 0).
    bind_group: wgpu::BindGroup,
}

fn canvas_texture(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    width: u32,
    height: u32,
) -> CanvasTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Celesta canvas"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        // Copied from when a blended draw takes its backdrop.
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Celesta canvas bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    CanvasTexture {
        texture,
        view,
        bind_group,
    }
}

/// An axis-aligned rectangle in canvas pixels: `[left, top, right, bottom]`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PixelBounds([f32; 4]);

impl PixelBounds {
    fn union(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        match (a, b) {
            (Some(Self(a)), Some(Self(b))) => Some(Self([
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ])),
            (a, None) => a,
            (None, b) => b,
        }
    }

    fn expand(self, x: f32, y: f32) -> Self {
        let [left, top, right, bottom] = self.0;
        Self([left - x, top - y, right + x, bottom + y])
    }

    fn offset(self, [x, y]: [f32; 2]) -> Self {
        let [left, top, right, bottom] = self.0;
        Self([left + x, top + y, right + x, bottom + y])
    }

    /// The whole pixels the rectangle touches, clipped to `size`, as a
    /// scissor rect; `None` when nothing of it is on the canvas.
    fn scissor(self, size: wgpu::Extent3d) -> Option<[u32; 4]> {
        let [left, top, right, bottom] = self.0;
        let clamp = |value: f32, limit: u32| value.clamp(0.0, limit as f32);
        let left = clamp(left.floor(), size.width) as u32;
        let top = clamp(top.floor(), size.height) as u32;
        let right = clamp(right.ceil(), size.width) as u32;
        let bottom = clamp(bottom.ceil(), size.height) as u32;
        (right > left && bottom > top).then(|| [left, top, right - left, bottom - top])
    }
}

/// The part of the scene a canvas covers, in whole scene pixels: its
/// top-left corner and the size of its texture.
///
/// The root canvas covers the whole scene. An isolated group or an effect
/// draws onto a canvas that only covers what its layers can touch, so a
/// small glow composites through a small texture rather than several
/// scene-sized ones (each render pass loads and stores its whole target).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CanvasRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl CanvasRegion {
    /// Group canvases are sized in steps of this many pixels, so a region
    /// that moves or grows a little from frame to frame reuses its texture.
    const STEP: u32 = 128;

    const fn scene(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// A canvas inside the `scene` that covers `bounds` (clipped to the
    /// scene), or `None` when nothing of `bounds` is on it. The canvas never
    /// reaches past the scene, so what is drawn onto it is exactly what a
    /// scene-sized canvas would hold there.
    fn covering(bounds: Option<PixelBounds>, scene: Self) -> Option<Self> {
        let [left, top, width, height] = bounds?.scissor(wgpu::Extent3d {
            width: scene.width,
            height: scene.height,
            depth_or_array_layers: 1,
        })?;
        let step = |extent: u32, limit: u32| (extent.div_ceil(Self::STEP) * Self::STEP).min(limit);
        let width = step(width, scene.width);
        let height = step(height, scene.height);
        Some(Self {
            x: left.min(scene.width - width),
            y: top.min(scene.height - height),
            width,
            height,
        })
    }

    /// `bounds`, in scene pixels, in this canvas's own pixels.
    fn local(self, bounds: PixelBounds) -> PixelBounds {
        bounds.offset([-(self.x as f32), -(self.y as f32)])
    }

    /// `bounds`, in scene pixels, in this canvas's pixels and clipped to
    /// it, as `[x, y, width, height]`; `None` when it misses the canvas.
    fn local_area(self, bounds: PixelBounds) -> Option<[u32; 4]> {
        self.local(bounds).scissor(self.extent())
    }

    const fn extent(self) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: self.width,
            height: self.height,
            depth_or_array_layers: 1,
        }
    }

    fn bounds(self) -> PixelBounds {
        PixelBounds([
            self.x as f32,
            self.y as f32,
            (self.x + self.width) as f32,
            (self.y + self.height) as f32,
        ])
    }
}

/// How far, in pixels, a blur of `radius` (the Gaussian's sigma in
/// `effect.wgsl`) can carry a pixel: its kernel's extent, plus one pixel for
/// the bilinear tap a fractional shadow offset reads.
fn blur_reach(radius: f32) -> f32 {
    let sigma = radius.clamp(0.0, 64.0);
    (sigma * 3.0).ceil() + 1.0
}

struct EffectProcessor {
    params_layout: wgpu::BindGroupLayout,
    /// The bilinear sampler `effect.wgsl` pairs taps through, bound beside
    /// the parameters.
    sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    /// Canvases groups draw onto and effects filter through, kept across
    /// effects and frames instead of allocating several per effect every
    /// frame, each with the frame it was last taken in. A canvas goes back
    /// to the pool once the commands that read it are encoded; the queue
    /// runs them in order, so reusing it later is safe.
    pool: Vec<(CanvasTexture, u64)>,
    /// Counts composited frames; see `end_frame`.
    frame: u64,
    /// Every filter pass's `Params` of the frame being encoded, one per
    /// `params_stride` bytes, written to `params` once the frame is encoded
    /// and read with a dynamic offset: a dense frame has hundreds of passes,
    /// and a buffer and a bind group each would dominate their cost.
    params: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    params_data: Vec<u8>,
    params_stride: u32,
}

/// Bytes of `Params` in `effect.wgsl`: three `vec4<f32>`s.
const EFFECT_PARAMS_SIZE: u64 = 3 * 4 * 4;

impl EffectProcessor {
    /// Reads its source through `texture_layout` (`layer.wgsl`'s), so a
    /// canvas's own bind group serves both shaders.
    fn new(device: &wgpu::Device, texture_layout: &wgpu::BindGroupLayout) -> Self {
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta effect parameters bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(EFFECT_PARAMS_SIZE),
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Celesta effect sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Celesta effect pipeline layout"),
            bind_group_layouts: &[Some(texture_layout), Some(&params_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("effect.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Celesta Gaussian effect pipeline"),
            layout: Some(&pipeline_layout),
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
                entry_point: Some("fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let params_stride = device
            .limits()
            .min_uniform_buffer_offset_alignment
            .max(EFFECT_PARAMS_SIZE as u32);
        let (params, params_bind_group) = Self::params_buffer(
            device,
            &params_layout,
            &sampler,
            u64::from(params_stride) * 64,
        );
        Self {
            params_layout,
            sampler,
            pipeline,
            pool: Vec::new(),
            frame: 0,
            params,
            params_bind_group,
            params_data: Vec::new(),
            params_stride,
        }
    }

    fn params_buffer(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        size: u64,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Celesta effect parameters"),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Celesta effect parameters"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(EFFECT_PARAMS_SIZE),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        (buffer, bind_group)
    }

    /// Starts a frame that runs at most `passes` filter passes.
    fn begin_frame(&mut self, device: &wgpu::Device, passes: usize) {
        self.frame += 1;
        self.params_data.clear();
        let size = u64::from(self.params_stride) * passes as u64;
        if size > self.params.size() {
            // In-flight frames keep the old buffer alive until they finish.
            (self.params, self.params_bind_group) = Self::params_buffer(
                device,
                &self.params_layout,
                &self.sampler,
                size.next_power_of_two(),
            );
        }
    }

    /// Uploads the frame's filter parameters, and drops the canvases neither
    /// this frame nor the one before took, so sizes a scene stopped using do
    /// not pile up. Queued writes land after previously submitted work, so
    /// frames still in flight keep reading their own parameters.
    fn end_frame(&mut self, queue: &wgpu::Queue) {
        if !self.params_data.is_empty() {
            queue.write_buffer(&self.params, 0, &self.params_data);
        }
        let frame = self.frame;
        self.pool.retain(|(_, used)| used + 1 >= frame);
    }

    /// A transparent-or-stale `width`x`height` canvas from the pool, or a new
    /// one. Every user clears it before drawing.
    fn take_canvas(
        &mut self,
        device: &wgpu::Device,
        layer_layout: &wgpu::BindGroupLayout,
        width: u32,
        height: u32,
    ) -> CanvasTexture {
        let fits = |(canvas, _): &(CanvasTexture, u64)| {
            let size = canvas.texture.size();
            size.width == width && size.height == height
        };
        match self.pool.iter().rposition(fits) {
            Some(index) => self.pool.swap_remove(index).0,
            None => canvas_texture(device, layer_layout, width, height),
        }
    }

    fn recycle(&mut self, canvas: CanvasTexture) {
        self.pool.push((canvas, self.frame));
    }

    /// Blurs (and, with `color`, shifts and tints) `source`, whose layers
    /// cover `content`. Each pass only shades the pixels its result can be
    /// non-transparent at, so a small glow costs a small area, not the canvas.
    #[allow(clippy::too_many_arguments)]
    fn apply(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &CanvasTexture,
        layer_layout: &wgpu::BindGroupLayout,
        content: PixelBounds,
        radius: f32,
        offset: [f32; 2],
        color: Option<[f32; 4]>,
    ) -> CanvasTexture {
        let size = source.texture.size();
        let reach = blur_reach(radius);
        let horizontal =
            (radius > 0.0).then(|| self.take_canvas(device, layer_layout, size.width, size.height));
        let vertical = self.take_canvas(device, layer_layout, size.width, size.height);
        if let Some(horizontal) = &horizontal {
            self.pass(
                encoder,
                source,
                horizontal,
                content.expand(reach, 0.0),
                [1.0, 0.0],
                radius,
                [0.0, 0.0],
                None,
            );
        }
        self.pass(
            encoder,
            horizontal.as_ref().unwrap_or(source),
            &vertical,
            content.expand(reach, reach).offset(offset),
            [0.0, 1.0],
            radius,
            offset,
            color,
        );
        if let Some(horizontal) = horizontal {
            self.recycle(horizontal);
        }
        vertical
    }

    #[allow(clippy::too_many_arguments)]
    fn pass(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        source: &CanvasTexture,
        target: &CanvasTexture,
        region: PixelBounds,
        direction: [f32; 2],
        radius: f32,
        offset: [f32; 2],
        color: Option<[f32; 4]>,
    ) {
        let tint = color.unwrap_or([0.0; 4]);
        let params = [
            direction[0],
            direction[1],
            radius,
            f32::from(color.is_some()),
            offset[0],
            offset[1],
            0.0,
            0.0,
            tint[0],
            tint[1],
            tint[2],
            tint[3],
        ];
        let at = self.params_data.len();
        assert!(
            (at + self.params_stride as usize) as u64 <= self.params.size(),
            "begin_frame reserves every pass's parameters"
        );
        self.params_data
            .extend(params.into_iter().flat_map(f32::to_ne_bytes));
        self.params_data.resize(at + self.params_stride as usize, 0);
        let mut pass = begin_pass(
            encoder,
            &target.view,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        // Outside `region` the result is transparent, which the clear wrote.
        let Some([x, y, width, height]) = region.scissor(target.texture.size()) else {
            return;
        };
        pass.set_scissor_rect(x, y, width, height);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &source.bind_group, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[at as u32]);
        pass.draw(0..3, 0..1);
    }
}

/// Encodes the steps of a frame that composites through canvases.
struct Compositor<'a> {
    device: &'a wgpu::Device,
    texture_layout: &'a wgpu::BindGroupLayout,
    layer_pipeline: &'a wgpu::RenderPipeline,
    blend_pipeline: &'a wgpu::RenderPipeline,
    backdrop: &'a BackdropTexture,
    draws: &'a PreparedDraws,
    effects: &'a mut EffectProcessor,
}

/// One draw onto a canvas, in painter's order.
struct CanvasDraw<'s> {
    /// The texture drawn: a layer's, or (`Err`) the finished group canvas
    /// at that index of `draw_canvas`'s `groups`.
    texture: Result<&'s wgpu::BindGroup, usize>,
    vertices: std::ops::Range<u32>,
    instances: std::ops::Range<u32>,
    blend_mode: BlendMode,
    /// For blended draws, the canvas pixels the draw can change.
    area: Option<[u32; 4]>,
}

impl Compositor<'_> {
    /// Clears `canvas` to `clear` and draws `steps` onto it. The groups
    /// among the steps are composited onto canvases of their own first, so
    /// that `canvas` itself is drawn in as few render passes as possible: a
    /// pass loads and stores its whole target, and the root canvas is
    /// scene-sized. Only a blended draw, which reads a copy of what is
    /// beneath it, ends a pass.
    fn draw_canvas(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        canvas: &CanvasTexture,
        clear: wgpu::Color,
        steps: &[GpuStep],
    ) {
        let mut groups: Vec<CanvasTexture> = Vec::new();
        let mut list: Vec<CanvasDraw<'_>> = Vec::new();
        let mut index = 0;
        while index < steps.len() {
            match &steps[index] {
                GpuStep::Draw(draw) => list.push(CanvasDraw {
                    texture: Ok(&draw.texture.bind_group),
                    vertices: draw.vertices.clone(),
                    instances: draw.instances.clone(),
                    blend_mode: BlendMode::Normal,
                    area: None,
                }),
                GpuStep::Blend {
                    draw,
                    blend_mode,
                    area,
                } => {
                    if area.is_some() {
                        list.push(CanvasDraw {
                            texture: Ok(&draw.texture.bind_group),
                            vertices: draw.vertices.clone(),
                            instances: draw.instances.clone(),
                            blend_mode: *blend_mode,
                            area: *area,
                        });
                    }
                }
                GpuStep::BeginGroup { .. } => {
                    let end = group_end(steps, index);
                    let (instance, blend_mode, area) = match &steps[end] {
                        GpuStep::EndGroup {
                            instance,
                            blend_mode,
                            area,
                        } => (*instance, *blend_mode, *area),
                        GpuStep::EndEffect {
                            final_instance,
                            blend_mode,
                            area,
                            ..
                        } => (*final_instance, *blend_mode, *area),
                        _ => unreachable!("group_end returns a group's end"),
                    };
                    match self.draw_group(encoder, &steps[index..=end]) {
                        Some(group) if area.is_some() => {
                            list.push(CanvasDraw {
                                texture: Err(groups.len()),
                                vertices: 0..6,
                                instances: instance..instance + 1,
                                blend_mode,
                                area,
                            });
                            groups.push(group);
                        }
                        Some(group) => self.effects.recycle(group),
                        None => {}
                    }
                    index = end;
                }
                GpuStep::EndGroup { .. } | GpuStep::EndEffect { .. } => {
                    unreachable!("draw_group consumes every group's end")
                }
            }
            index += 1;
        }

        let mut load = wgpu::LoadOp::Clear(clear);
        let texture = |draw: &CanvasDraw<'_>| -> wgpu::BindGroup {
            match draw.texture {
                Ok(texture) => texture.clone(),
                Err(group) => groups[group].bind_group.clone(),
            }
        };
        let mut draws = list.iter().peekable();
        while let Some(draw) = draws.next() {
            if draw.blend_mode.is_normal() {
                // One pass for the whole run of source-over draws.
                let mut pass = begin_pass(encoder, &canvas.view, load);
                pass.set_pipeline(self.layer_pipeline);
                pass.set_vertex_buffer(0, self.draws.instances.slice(..));
                pass.set_bind_group(2, &self.draws.clips, &[]);
                pass.set_bind_group(0, &texture(draw), &[]);
                pass.draw(draw.vertices.clone(), draw.instances.clone());
                while let Some(next) = draws.next_if(|next| next.blend_mode.is_normal()) {
                    pass.set_bind_group(0, &texture(next), &[]);
                    pass.draw(next.vertices.clone(), next.instances.clone());
                }
            } else {
                if let wgpu::LoadOp::Clear(_) = load {
                    drop(begin_pass(encoder, &canvas.view, load));
                }
                let [x, y, width, height] = draw.area.expect("blended draws have an area");
                // `fs_blend` reads the backdrop only under the draw, so only
                // that part of the canvas needs copying.
                let origin = wgpu::Origin3d { x, y, z: 0 };
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        origin,
                        ..canvas.texture.as_image_copy()
                    },
                    wgpu::TexelCopyTextureInfo {
                        origin,
                        ..self.backdrop.texture.as_image_copy()
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
                let mut pass = begin_pass(encoder, &canvas.view, wgpu::LoadOp::Load);
                pass.set_pipeline(self.blend_pipeline);
                pass.set_vertex_buffer(0, self.draws.instances.slice(..));
                pass.set_bind_group(0, &texture(draw), &[]);
                pass.set_bind_group(1, &self.backdrop.bind_group, &[]);
                pass.set_bind_group(2, &self.draws.clips, &[]);
                pass.draw(draw.vertices.clone(), draw.instances.clone());
            }
            load = wgpu::LoadOp::Load;
        }
        if let wgpu::LoadOp::Clear(_) = load {
            drop(begin_pass(encoder, &canvas.view, load));
        }
        for group in groups {
            self.effects.recycle(group);
        }
    }

    /// Composites the group `steps` (from its `BeginGroup` to its end) onto
    /// a canvas of its own and applies its effect, returning the canvas to
    /// draw onto the group's parent, or `None` when it holds nothing.
    fn draw_group(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        steps: &[GpuStep],
    ) -> Option<CanvasTexture> {
        let (Some(GpuStep::BeginGroup { canvas: region }), Some(end)) =
            (steps.first(), steps.last())
        else {
            unreachable!("a group runs from its BeginGroup to its end");
        };
        let canvas = self.effects.take_canvas(
            self.device,
            self.texture_layout,
            region.width,
            region.height,
        );
        self.draw_canvas(
            encoder,
            &canvas,
            wgpu::Color::TRANSPARENT,
            &steps[1..steps.len() - 1],
        );
        let GpuStep::EndEffect {
            inner_instance,
            effects,
            content,
            ..
        } = end
        else {
            return Some(canvas);
        };
        // Layers that drew nothing leave nothing to filter.
        let Some(content) = *content else {
            self.effects.recycle(canvas);
            return None;
        };
        let result = self.effects.take_canvas(
            self.device,
            self.texture_layout,
            region.width,
            region.height,
        );
        let mut filtered = Vec::new();
        for shadow in [effects.shadow, effects.glow].into_iter().flatten() {
            filtered.push(self.effects.apply(
                self.device,
                encoder,
                &canvas,
                self.texture_layout,
                content,
                shadow.blur,
                shadow.offset,
                Some(shadow.color),
            ));
        }
        let blurred = (effects.blur > 0.0).then(|| {
            self.effects.apply(
                self.device,
                encoder,
                &canvas,
                self.texture_layout,
                content,
                effects.blur,
                [0.0, 0.0],
                None,
            )
        });
        // Shadows, then the glow, then the (blurred) layers on top.
        let mut pass = begin_pass(
            encoder,
            &result.view,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        pass.set_pipeline(self.layer_pipeline);
        pass.set_vertex_buffer(0, self.draws.instances.slice(..));
        pass.set_bind_group(2, &self.draws.clips, &[]);
        for texture in filtered.iter().chain([blurred.as_ref().unwrap_or(&canvas)]) {
            pass.set_bind_group(0, &texture.bind_group, &[]);
            pass.draw(0..6, *inner_instance..*inner_instance + 1);
        }
        drop(pass);
        for texture in filtered.into_iter().chain(blurred).chain([canvas]) {
            self.effects.recycle(texture);
        }
        Some(result)
    }
}

/// The index of the step that ends the group `steps[begin]` begins.
fn group_end(steps: &[GpuStep], begin: usize) -> usize {
    let mut depth = 0;
    for (index, step) in steps.iter().enumerate().skip(begin) {
        match step {
            GpuStep::BeginGroup { .. } => depth += 1,
            GpuStep::EndGroup { .. } | GpuStep::EndEffect { .. } => {
                depth -= 1;
                if depth == 0 {
                    return index;
                }
            }
            GpuStep::Draw(_) | GpuStep::Blend { .. } => {}
        }
    }
    unreachable!("every group has an end")
}

struct BackdropTexture {
    texture: wgpu::Texture,
    /// Binds the backdrop as group 1 of a `PipelineKind::Blend` pipeline.
    bind_group: wgpu::BindGroup,
}

fn begin_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Celesta layer pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// Bytes of `LayerInstance` in `layer.wgsl`: nine `vec4<f32>`s.
const LAYER_INSTANCE_SIZE: u64 = 9 * 4 * 4;

const LAYER_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
    0 => Float32x4,
    1 => Float32x4,
    2 => Float32x4,
    3 => Float32x4,
    4 => Float32x4,
    5 => Float32x4,
    6 => Float32x4,
    7 => Float32x4,
    8 => Float32x4,
];

fn instance_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Celesta layer instances"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// A storage buffer for the frame's clips or paints.
fn clip_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Celesta layer clips"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn clip_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    clips: &wgpu::Buffer,
    paints: &wgpu::Buffer,
    paths: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Celesta layer clip bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: clips.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: paints.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: paths.as_entire_binding(),
            },
        ],
    })
}

/// A rect drawn by `layer.wgsl`'s `rect_color`, which reproduces
/// `celesta_renderer::rasterize_rect` texel for texel.
struct RectShape {
    /// The size of the texture `rasterize_rect` would produce: the rect's
    /// size rounded up to whole pixels, at least 1x1.
    pixel_width: u32,
    pixel_height: u32,
    half_width: f32,
    half_height: f32,
    radius: f32,
    /// 0 without a stroke.
    stroke_width: f32,
    /// See `encode_paint`.
    fill: [f32; 4],
    stroke: [f32; 4],
}

impl RectShape {
    /// The shape of a rect painted with `paint`, whose gradients are
    /// appended to `paints`.
    fn new(
        width: f64,
        height: f64,
        corner_radius: f64,
        paint: RectPaint,
        paints: &mut Vec<[f32; 4]>,
    ) -> Self {
        let RectPaint { fill, stroke } = paint;
        let (stroke, stroke_width) = match stroke {
            Some((stroke, width)) if width > 0.0 => (Some(stroke), width),
            _ => (None, 0.0),
        };
        let half_width = width / 2.0;
        let half_height = height / 2.0;
        Self {
            pixel_width: width.max(0.0).ceil().max(1.0) as u32,
            pixel_height: height.max(0.0).ceil().max(1.0) as u32,
            half_width: half_width as f32,
            half_height: half_height as f32,
            radius: corner_radius.max(0.0).min(half_width.min(half_height)) as f32,
            stroke_width: stroke_width as f32,
            fill: encode_paint(fill.as_ref(), paints),
            stroke: encode_paint(stroke.as_ref(), paints),
        }
    }
}

/// A rect's fill or stroke as `layer.wgsl`'s `paint_color` reads it: a flat
/// color is its straight-alpha RGBA in 0-255 code values (transparent
/// without a paint); a gradient is `(index, 0, 0, -1)`, where `index` is
/// the first of its entries in `paints`:
///
/// - kind (1 linear, 2 radial), stop count, unused, unused
/// - linear: start x, start y, end x, end y; radial: center x, center y,
///   radius, unused (the rect's local pixels)
/// - per stop: offset, unused, unused, unused; then its premultiplied RGBA
///   in 0-255
///
/// Shading a gradient instead of rasterizing it keeps a gradient whose
/// colors change every frame as cheap as a flat one.
fn encode_paint(paint: Option<&ResolvedPaint>, paints: &mut Vec<[f32; 4]>) -> [f32; 4] {
    let index = paints.len() as f32;
    let (kind, geometry, stops) = match paint {
        None => return [0.0; 4],
        Some(ResolvedPaint::Solid(color)) => {
            return [color.red, color.green, color.blue, color.alpha].map(f32::from);
        }
        Some(ResolvedPaint::Linear { start, end, stops }) => {
            (1.0, [start.0, start.1, end.0, end.1], stops)
        }
        Some(ResolvedPaint::Radial {
            center,
            radius,
            stops,
        }) => (2.0, [center.0, center.1, *radius, 0.0], stops),
    };
    paints.push([kind, stops.len() as f32, 0.0, 0.0]);
    paints.push(geometry.map(|value| value as f32));
    for stop in stops {
        paints.push([stop.offset() as f32, 0.0, 0.0, 0.0]);
        paints.push(stop.premultiplied().map(|value| value as f32));
    }
    [index, 0.0, 0.0, -1.0]
}

/// The pixels of a path's region that share one list of edges in `paths`.
/// Mirrors `PATH_TILE_COLUMNS` and `PATH_TILE_ROWS` in `layer.wgsl`.
const PATH_TILE_COLUMNS: u32 = 8;
const PATH_TILE_ROWS: u32 = 8;

/// A path drawn by `layer.wgsl`'s `path_texel`, which shades the coverage of
/// its outlines' edges (`celesta_renderer::flatten_path`) into the pixels
/// `celesta_renderer::rasterize_path` would have produced: texel for texel,
/// as a quad per `PATH_TILE_COLUMNS`x`PATH_TILE_ROWS` tile of its region
/// that either outline covers any of.
struct ShadedPath {
    width: u32,
    height: u32,
    /// The index of its first entry in `paths`:
    ///
    /// - the inverse transform's a, b, c, d (region pixels to layer
    ///   coordinates, for gradients)
    /// - the inverse transform's tx, ty; the number of tile columns; how many
    ///   entries each tile drawn has, one per outline
    /// - which of a tile's entries is the fill's, then the stroke's (-1
    ///   without one); the index of the first tile's; unused
    ///
    /// then the tiles drawn, each outline's entry for each: the tile's index
    /// in the region (row by row), the index and count of the edges the
    /// outline has there, and its backdrop (see `bin_tiles`). The edges, x0,
    /// y0, x1, y1 in region pixels, follow. Indices are whole numbers in f32,
    /// exact below 2^24 entries, which the storage buffer binding limit keeps
    /// them under.
    base: f32,
    /// How many tiles it draws.
    tiles: u32,
    /// See `encode_paint`.
    fill: [f32; 4],
    stroke: [f32; 4],
}

impl ShadedPath {
    fn new(path: &FlattenedPath, paints: &mut Vec<[f32; 4]>, entries: &mut Vec<[f32; 4]>) -> Self {
        let base = entries.len();
        let inverse = path.inverse;
        let columns = path.width.div_ceil(PATH_TILE_COLUMNS);
        let outlines: Vec<_> = [&path.fill, &path.stroke]
            .into_iter()
            .map(|outline| {
                outline
                    .as_ref()
                    .map(|(_, edges)| bin_tiles(edges, path.width, path.height))
            })
            .collect();
        // Each tile's entries: the fill's, then the stroke's, if it has them.
        let mut stride = 0;
        let mut slots = [-1.0; 2];
        for (slot, outline) in slots.iter_mut().zip(&outlines) {
            if outline.is_some() {
                *slot = stride as f32;
                stride += 1;
            }
        }
        // Every tile either outline covers, in order.
        let mut tiles: Vec<u32> = outlines
            .iter()
            .flatten()
            .flat_map(|bins| bins.tiles.iter().map(|tile| tile.index))
            .collect();
        tiles.sort_unstable();
        tiles.dedup();
        let first_tile = base + 3;
        entries.push([inverse.a, inverse.b, inverse.c, inverse.d].map(|value| value as f32));
        entries.push([
            inverse.tx as f32,
            inverse.ty as f32,
            columns as f32,
            stride as f32,
        ]);
        entries.push([slots[0], slots[1], first_tile as f32, 0.0]);
        entries.resize(first_tile + tiles.len() * stride, [0.0; 4]);
        for (slot, bins) in outlines.iter().flatten().enumerate() {
            let edges = entries.len();
            let mut own = bins.tiles.iter().peekable();
            for (position, &index) in tiles.iter().enumerate() {
                let entry = &mut entries[first_tile + position * stride + slot];
                *entry = [index as f32, 0.0, 0.0, 0.0];
                if let Some(tile) = own.next_if(|tile| tile.index == index) {
                    *entry = [
                        index as f32,
                        (edges + tile.edges.start) as f32,
                        tile.edges.len() as f32,
                        tile.backdrop as f32,
                    ];
                }
            }
            entries.extend_from_slice(&bins.edges);
        }
        Self {
            width: path.width,
            height: path.height,
            base: base as f32,
            tiles: tiles.len() as u32,
            fill: encode_paint(path.fill.as_ref().map(|(paint, _)| paint), paints),
            stroke: encode_paint(path.stroke.as_ref().map(|(paint, _)| paint), paints),
        }
    }
}

/// An outline's edges sorted into the tiles of its region that it covers
/// any of: those its edges reach, and those inside it.
struct TileBins {
    /// In order of their index.
    tiles: Vec<CoveredTile>,
    edges: Vec<[f32; 4]>,
}

struct CoveredTile {
    /// Row by row.
    index: u32,
    /// The winding just left of the tile's top-left corner.
    backdrop: i32,
    /// What it needs of `TileBins::edges`.
    edges: std::ops::Range<usize>,
}

/// Sorts `edges` into the `PATH_TILE_COLUMNS`x`PATH_TILE_ROWS` tiles of a
/// `width`x`height` region.
///
/// A tile gets only the parts of `edges` at or right of its left side, cut
/// to its rows. What lies left of it is summed up by its backdrop, plus a
/// vertical edge down the tile's left side from wherever an edge crosses
/// it, since the winding along that side only changes there. A tile no edge
/// reaches is uniformly covered or not, as its backdrop says, and costs the
/// shader no edges at all.
fn bin_tiles(edges: &[LineSegment], width: u32, height: u32) -> TileBins {
    let columns = width.div_ceil(PATH_TILE_COLUMNS) as usize;
    let rows = height.div_ceil(PATH_TILE_ROWS) as usize;
    let tile_width = PATH_TILE_COLUMNS as f32;
    let tile_height = PATH_TILE_ROWS as f32;
    let column_of = |x: f32| ((x / tile_width).floor().max(0.0) as usize).min(columns - 1);
    let mut pieces: Vec<(usize, [f32; 4])> = Vec::with_capacity(edges.len() * 2);
    // Where along a tile row the backdrop changes, and by how much.
    let mut backdrops: Vec<(usize, i32)> = Vec::new();
    for edge in edges {
        let LineSegment { x0, y0, x1, y1 } = *edge;
        if y0 == y1 {
            // Its crossings of tile sides, where it does not sit on a tile
            // row's top (whose backdrop already counts what it connects).
            let row = (y0 / tile_height).floor() as usize;
            if row >= rows || y0 == row as f32 * tile_height {
                continue;
            }
            let bottom = (row + 1) as f32 * tile_height;
            let direction = if x0 > x1 { [y0, bottom] } else { [bottom, y0] };
            let (low, high) = (x0.min(x1), x0.max(x1));
            for column in column_of(low) + 1..=column_of(high) {
                let side = column as f32 * tile_width;
                if low < side && side <= high {
                    pieces.push((
                        row * columns + column,
                        [side, direction[0], side, direction[1]],
                    ));
                }
            }
            continue;
        }
        let first_row = ((y0.min(y1) / tile_height).floor() as usize).min(rows - 1);
        let last_row = ((y0.max(y1) / tile_height).ceil() as usize).clamp(first_row + 1, rows);
        for row in first_row..last_row {
            let (top, bottom) = (row as f32 * tile_height, (row + 1) as f32 * tile_height);
            // The edge within the row, in its own direction.
            let at = |y: f32| x0 + (x1 - x0) * ((y - y0) / (y1 - y0));
            let clip = |x: f32, y: f32| {
                if y < top {
                    (at(top), top)
                } else if y > bottom {
                    (at(bottom), bottom)
                } else {
                    (x, y)
                }
            };
            let (ax, ay) = clip(x0, y0);
            let (bx, by) = clip(x1, y1);
            if ay == by {
                continue;
            }
            let (top_x, low, high) = if ay < by { (ax, ax, bx) } else { (bx, bx, ax) };
            let (low, high) = (low.min(high), low.max(high));
            let direction = if by > ay { 1 } else { -1 };
            let tiles = row * columns..(row + 1) * columns;
            // Where the edge starts on the row's top, the tiles whose left
            // side is right of it count it in their backdrop.
            if ay.min(by) == top {
                let first = (top_x / tile_width).floor() as usize + 1;
                if first < columns {
                    backdrops.push((tiles.start + first, direction));
                }
            }
            for column in column_of(low)..=column_of(high) {
                let side = column as f32 * tile_width;
                let tile = tiles.start + column;
                if low >= side {
                    pieces.push((tile, [ax, ay, bx, by]));
                    continue;
                }
                // Cut where it crosses the tile's left side: the part left
                // of it is the backdrop's business, plus a vertical edge
                // from the crossing down the side.
                let t = (side - ax) / (bx - ax);
                let cross = if t <= 0.0 {
                    ay
                } else if t >= 1.0 {
                    by
                } else {
                    ay + (by - ay) * t
                };
                let piece = if ax < side {
                    [side, cross, bx, by]
                } else {
                    [ax, ay, side, cross]
                };
                if piece[1] != piece[3] {
                    pieces.push((tile, piece));
                }
                if cross < bottom {
                    // Leaving the left side downwards (left part above) takes
                    // the edge's winding away below the crossing; entering
                    // it adds it.
                    let leaves = (ax < side) == (ay < by);
                    let sign = if leaves { -direction } else { direction };
                    let side_edge = if sign > 0 {
                        [side, cross, side, bottom]
                    } else {
                        [side, bottom, side, cross]
                    };
                    pieces.push((tile, side_edge));
                }
            }
        }
    }
    pieces.sort_unstable_by_key(|(tile, _)| *tile);
    backdrops.sort_unstable_by_key(|(tile, _)| *tile);
    let mut bins = TileBins {
        tiles: Vec::new(),
        edges: Vec::with_capacity(pieces.len()),
    };
    let mut pieces = pieces.into_iter().peekable();
    let mut backdrops = backdrops.into_iter().peekable();
    for row in 0..rows {
        let end = (row + 1) * columns;
        let mut backdrop = 0;
        let mut tile = row * columns;
        while tile < end {
            while let Some((_, change)) = backdrops.next_if(|(index, _)| *index == tile) {
                backdrop += change;
            }
            let start = bins.edges.len();
            while let Some((_, piece)) = pieces.next_if(|(index, _)| *index == tile) {
                bins.edges.push(piece);
            }
            let edges = start..bins.edges.len();
            if backdrop != 0 || !edges.is_empty() {
                bins.tiles.push(CoveredTile {
                    index: tile as u32,
                    backdrop,
                    edges,
                });
            }
            tile += 1;
            if backdrop == 0 {
                // Nothing to draw up to the next tile with edges or a change.
                let next_piece = pieces.peek().map_or(end, |(index, _)| *index);
                let next_change = backdrops.peek().map_or(end, |(index, _)| *index);
                tile = tile.max(next_piece.min(next_change).min(end));
            }
        }
    }
    bins
}

enum PreparedContent {
    Texture(LayerTexture),
    Rect(RectShape),
    Path(ShadedPath),
    /// A whole scene-sized canvas: a finished isolated group, or the root
    /// canvas being copied onto the target.
    Canvas {
        /// Whether the canvas holds premultiplied alpha (an isolated group)
        /// that the shader must unpremultiply.
        premultiplied: bool,
    },
}

/// What `prepare_layer` produces, in painter's order.
enum PreparedItem {
    Layer(PreparedLayer),
    /// The items up to the matching `EndGroup` draw onto a transparent canvas
    /// of their own.
    BeginGroup,
    /// Draws the finished group's canvas onto its parent.
    EndGroup(PreparedLayer),
    EndEffect(PreparedLayer, EffectSpec),
    /// A text layer `resolve_pending_texts` replaces with a `Layer` before
    /// anything else reads the items.
    PendingText,
}

struct PreparedLayer {
    content: PreparedContent,
    anchor: Point,
    state: LayerState,
    blend_mode: BlendMode,
    /// Texels per layer unit: the scale text was rasterized at, 1 for
    /// everything else. The quad keeps the layer's size in layer units.
    raster_scale: f32,
}

impl PreparedLayer {
    fn new(texture: LayerTexture, anchor: Point, state: LayerState, blend_mode: BlendMode) -> Self {
        let raster_scale = texture.raster_scale;
        Self {
            content: PreparedContent::Texture(texture),
            anchor,
            state,
            blend_mode,
            raster_scale,
        }
    }

    const fn canvas(state: LayerState, blend_mode: BlendMode, premultiplied: bool) -> Self {
        Self {
            content: PreparedContent::Canvas { premultiplied },
            anchor: Point { x: 0.0, y: 0.0 },
            state,
            blend_mode,
            raster_scale: 1.0,
        }
    }

    /// Appends this layer's `LayerInstance` (`LAYER_INSTANCE_SIZE` bytes),
    /// drawn onto the canvas covering `target`. A `Canvas` layer draws the
    /// canvas covering `canvas`; other layers ignore it.
    fn write_instance(&self, target: CanvasRegion, canvas: CanvasRegion, output: &mut Vec<u8>) {
        let (texel_width, texel_height, kind) = match &self.content {
            PreparedContent::Texture(texture) => (texture.width, texture.height, 0.0),
            PreparedContent::Rect(rect) => (rect.pixel_width, rect.pixel_height, 1.0),
            PreparedContent::Path(path) => (path.width, path.height, 2.0),
            PreparedContent::Canvas { .. } => (canvas.width, canvas.height, 0.0),
        };
        let premultiplied = matches!(
            self.content,
            PreparedContent::Canvas {
                premultiplied: true
            }
        );
        let (rect, fill, stroke) = match &self.content {
            PreparedContent::Texture(_) | PreparedContent::Canvas { .. } => {
                ([0.0; 4], [0.0; 4], [0.0; 4])
            }
            PreparedContent::Rect(rect) => (
                [
                    rect.half_width,
                    rect.half_height,
                    rect.radius,
                    rect.stroke_width,
                ],
                rect.fill,
                rect.stroke,
            ),
            PreparedContent::Path(path) => ([path.base, 0.0, 0.0, 0.0], path.fill, path.stroke),
        };
        let mut transform = self.state.transform;
        if let PreparedContent::Canvas { .. } = self.content {
            // A canvas is drawn untransformed, where it sits in the scene.
            transform.tx += canvas.x as f32;
            transform.ty += canvas.y as f32;
        }
        // A layer whose texels land one to one on canvas pixels is copied
        // exactly; anything scaled or rotated is filtered.
        let exact = transform.is_uniform_scale(self.raster_scale);
        let (tx, ty) = if exact {
            self.pixel_aligned_translation(transform, texel_width, texel_height)
        } else {
            (transform.tx, transform.ty)
        };
        let level_of_detail = if exact {
            0.0
        } else {
            transform
                .texels_per_pixel(self.raster_scale)
                .max(1.0)
                .log2()
        };
        let values = [
            transform.a,
            transform.b,
            transform.c,
            transform.d,
            tx,
            ty,
            texel_width as f32 / self.raster_scale,
            texel_height as f32 / self.raster_scale,
            self.anchor.x as f32,
            self.anchor.y as f32,
            self.state.opacity,
            kind,
            target.width as f32,
            target.height as f32,
            blend_mode_index(self.blend_mode),
            f32::from(u8::from(premultiplied)),
        ]
        .into_iter()
        .chain(rect)
        .chain(fill)
        .chain(stroke)
        // The innermost clip the layer is drawn through (-1 without one),
        // whether to filter, the mip level to filter at, and texels per
        // layer unit.
        .chain([
            self.state.clip.map_or(-1.0, |index| index as f32),
            f32::from(u8::from(!exact)),
            level_of_detail,
            self.raster_scale,
        ])
        .chain([target.x as f32, target.y as f32, 0.0, 0.0]);
        output.extend(values.flat_map(f32::to_ne_bytes));
    }

    /// Scene pixels the layer's quad can touch: the corners of the quad
    /// `vs_main` draws (a filtered layer's reaches one texel past its edge),
    /// grown by two pixels for the exact-copy rounding.
    fn bounds(&self) -> PixelBounds {
        let (texel_width, texel_height) = match &self.content {
            PreparedContent::Texture(texture) => (texture.width, texture.height),
            PreparedContent::Rect(rect) => (rect.pixel_width, rect.pixel_height),
            PreparedContent::Path(path) => (path.width, path.height),
            PreparedContent::Canvas { .. } => unreachable!("only a group's end draws a canvas"),
        };
        let width = texel_width as f32 / self.raster_scale;
        let height = texel_height as f32 / self.raster_scale;
        let transform = self.state.transform;
        let anchor = [self.anchor.x as f32, self.anchor.y as f32];
        let (margin_u, margin_v) = if transform.is_uniform_scale(self.raster_scale) {
            (0.0, 0.0)
        } else {
            (1.0 / texel_width as f32, 1.0 / texel_height as f32)
        };
        let (low_u, high_u) = (-margin_u, 1.0 + margin_u);
        let (low_v, high_v) = (-margin_v, 1.0 + margin_v);
        let corners = [
            [low_u, low_v],
            [high_u, low_v],
            [low_u, high_v],
            [high_u, high_v],
        ]
        .map(|[u, v]| {
            let x = (u - anchor[0]) * width;
            let y = (v - anchor[1]) * height;
            (
                transform.a * x + transform.c * y + transform.tx,
                transform.b * x + transform.d * y + transform.ty,
            )
        });
        let xs = corners.map(|(x, _)| x);
        let ys = corners.map(|(_, y)| y);
        let min = |values: [f32; 4]| values.into_iter().fold(f32::INFINITY, f32::min);
        let max = |values: [f32; 4]| values.into_iter().fold(f32::NEG_INFINITY, f32::max);
        PixelBounds([min(xs), min(ys), max(xs), max(ys)]).expand(2.0, 2.0)
    }

    /// The translation of a layer drawn texel for texel, moved so its
    /// top-left corner lands on a whole canvas pixel, as
    /// `celesta_renderer::render_image` places it. On a half pixel every pixel
    /// centre would sit exactly on a texel boundary, and f32 rounding would
    /// pick the left or right texel per column, notching glyph stems.
    fn pixel_aligned_translation(
        &self,
        transform: Affine,
        texel_width: u32,
        texel_height: u32,
    ) -> (f32, f32) {
        let align = |translation: f32, texels: u32, anchor: f64| {
            let offset = f64::from(texels) * anchor;
            ((f64::from(translation) - offset).round() + offset) as f32
        };
        (
            align(transform.tx, texel_width, self.anchor.x),
            align(transform.ty, texel_height, self.anchor.y),
        )
    }
}

/// A text layer whose texture was not cached when `prepare_layer` reached
/// it, holding what is needed to rasterize it and to replace `item`.
struct PendingText {
    /// Index of its `PreparedItem::PendingText` in the frame's items.
    item: usize,
    key: String,
    text: String,
    style: TextStyle,
    max_width: Option<f64>,
    raster_scale: f32,
    anchor: Point,
    baseline_anchor: bool,
    state: LayerState,
    blend_mode: BlendMode,
}

/// Rasterizes `job` at its scale, or smaller when that would exceed `limit`
/// on either side (the draw's filtering then enlarges it).
fn rasterize_text(
    rasterizer: &mut TextRasterizer,
    job: &PendingText,
    limit: u32,
) -> Result<DecodedImage, GpuRenderError> {
    let mut scale = job.raster_scale;
    let text = loop {
        let text = rasterizer
            .rasterize(&job.text, &job.style, job.max_width, scale)
            .map_err(GpuRenderError::Text)?;
        let largest = text.width().max(text.height());
        if largest <= limit || scale <= 1.0 {
            break text;
        }
        scale = (scale * limit as f32 / largest as f32 * 0.99).max(1.0);
    };
    let baseline = text.baseline_anchor();
    let origin = text.anchor_in_image(0.0, 0.0);
    let end = text.anchor_in_image(1.0, 1.0);
    let mut image = DecodedImage::new(text.width(), text.height(), text.into_pixels())?;
    image.baseline_anchor = baseline;
    image.anchor_origin = origin;
    image.anchor_span = (end.0 - origin.0, end.1 - origin.1);
    image.raster_scale = scale;
    Ok(image)
}

/// The layer drawing a text `texture`, its normalized `anchor` mapped onto
/// the texture's text box (or its first baseline).
fn text_layer(
    texture: LayerTexture,
    anchor: Point,
    baseline_anchor: bool,
    state: LayerState,
    blend_mode: BlendMode,
) -> PreparedLayer {
    let anchor = Point {
        x: texture.anchor_origin.0 + anchor.x * texture.anchor_span.0,
        y: if baseline_anchor {
            texture.baseline_anchor
        } else {
            texture.anchor_origin.1 + anchor.y * texture.anchor_span.1
        },
    };
    PreparedLayer::new(texture, anchor, state, blend_mode)
}

/// The scale to rasterize text at so it is drawn at most slightly shrunk:
/// the largest factor `transform` stretches it by, rounded up to the next
/// eighth of an octave. An animated scale then reuses a few textures (about
/// eight per doubling) instead of rasterizing every frame, and the texture
/// is at most 9% larger than drawn. Exactly 1 for unscaled text.
fn text_raster_scale(transform: Affine) -> f32 {
    const STEPS_PER_OCTAVE: f32 = 8.0;
    let (largest, _) = transform.stretch();
    let steps = largest.log2() * STEPS_PER_OCTAVE;
    let nearest = steps.round();
    // A scale that already is a step (2, 0.5) should not round up past it.
    let steps = if (steps - nearest).abs() < 1e-3 {
        nearest
    } else {
        steps.ceil()
    };
    // Keep absurd scales finite; the texture size is limited separately.
    (steps.clamp(-6.0 * STEPS_PER_OCTAVE, 6.0 * STEPS_PER_OCTAVE) / STEPS_PER_OCTAVE).exp2()
}

/// `fs_blend`'s index for `mode` in `layer.wgsl`.
fn blend_mode_index(mode: BlendMode) -> f32 {
    match mode {
        BlendMode::Normal => 0.0,
        BlendMode::Multiply => 1.0,
        BlendMode::Screen => 2.0,
        BlendMode::Overlay => 3.0,
        BlendMode::Add => 4.0,
        BlendMode::Difference => 5.0,
    }
}

/// A frame's draws, ready to encode: the buffer holding every layer's
/// instance, and what to draw with them in painter's order.
struct PreparedDraws {
    instances: wgpu::Buffer,
    /// The bind group exposing the frame's clips to the fragment shader.
    clips: wgpu::BindGroup,
    /// Only `GpuStep::Draw`s unless `composite` is set.
    steps: Vec<GpuStep>,
    composite: Option<CompositePlan>,
}

/// How a frame that uses blend modes composites through canvases.
struct CompositePlan {
    /// The instance that copies the root canvas onto the target.
    blit_instance: u32,
}

/// The canvas an isolated group or an effect draws its layers onto.
#[derive(Clone, Copy)]
struct GroupPlan {
    canvas: CanvasRegion,
    /// What the group's layers cover, in scene pixels.
    content: Option<PixelBounds>,
    /// Whether anything of the group (or its effect) reaches the scene.
    /// When not, its layers still draw onto `canvas`, a single pixel none
    /// of them can touch, and the group draws nothing onto its parent.
    drawn: bool,
}

/// Plans the canvas of every isolated group and effect in `items`, in the
/// order they begin, from the bounds of what their layers draw.
fn plan_groups(items: &[PreparedItem], scene: CanvasRegion) -> Vec<GroupPlan> {
    let mut plans: Vec<GroupPlan> = Vec::new();
    // Each open group's index in `plans` and what its layers cover so far.
    let mut open: Vec<(usize, Option<PixelBounds>)> = Vec::new();
    let cover = |open: &mut Vec<(usize, Option<PixelBounds>)>, bounds: Option<PixelBounds>| {
        if let Some((_, covered)) = open.last_mut() {
            *covered = PixelBounds::union(*covered, bounds);
        }
    };
    let plan = |content: Option<PixelBounds>, output: Option<PixelBounds>| {
        let canvas = CanvasRegion::covering(output, scene);
        GroupPlan {
            canvas: canvas.unwrap_or(CanvasRegion {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }),
            content,
            drawn: canvas.is_some(),
        }
    };
    for item in items {
        match item {
            PreparedItem::Layer(layer) => cover(&mut open, Some(layer.bounds())),
            PreparedItem::PendingText => unreachable!("pending text is resolved"),
            PreparedItem::BeginGroup => {
                open.push((plans.len(), None));
                plans.push(plan(None, None));
            }
            PreparedItem::EndGroup(_) => {
                let (index, content) = open.pop().expect("every group was begun");
                plans[index] = plan(content, content);
                cover(&mut open, content);
            }
            PreparedItem::EndEffect(_, effects) => {
                let (index, content) = open.pop().expect("every group was begun");
                let output = content.map(|content| effects.output_bounds(content));
                plans[index] = plan(content, output);
                cover(&mut open, output);
            }
        }
    }
    plans
}

enum GpuStep {
    /// A run of instances that sample the same texture, drawn source-over.
    Draw(GpuDraw),
    /// One instance blended onto the current canvas through `blend_mode`.
    Blend {
        draw: GpuDraw,
        blend_mode: BlendMode,
        /// The canvas pixels the draw can change, `[x, y, width, height]`;
        /// `None` when it misses the canvas.
        area: Option<[u32; 4]>,
    },
    /// Starts drawing onto a fresh transparent canvas covering `canvas`.
    BeginGroup { canvas: CanvasRegion },
    /// Draws the finished group canvas onto its parent with `instance`.
    EndGroup {
        instance: u32,
        blend_mode: BlendMode,
        /// The parent canvas pixels the group's canvas covers; `None` when
        /// it draws nothing there.
        area: Option<[u32; 4]>,
    },
    EndEffect {
        inner_instance: u32,
        final_instance: u32,
        blend_mode: BlendMode,
        effects: EffectSpec,
        /// The part of the effect's canvas its layers drew on, in that
        /// canvas's pixels; `None` when they drew nothing.
        content: Option<PixelBounds>,
        /// As for `EndGroup`.
        area: Option<[u32; 4]>,
    },
}

#[derive(Clone, Copy)]
struct EffectShadow {
    color: [f32; 4],
    blur: f32,
    offset: [f32; 2],
}

#[derive(Clone, Copy)]
struct EffectSpec {
    blur: f32,
    shadow: Option<EffectShadow>,
    glow: Option<EffectShadow>,
}

impl EffectSpec {
    /// The canvas pixels the filtered result of layers covering `content`
    /// can touch: the content itself, its blur, and each shifted shadow.
    fn output_bounds(&self, content: PixelBounds) -> PixelBounds {
        let mut bounds = content.expand(blur_reach(self.blur), blur_reach(self.blur));
        for shadow in [self.shadow, self.glow].into_iter().flatten() {
            let reach = blur_reach(shadow.blur);
            bounds = PixelBounds::union(
                Some(bounds),
                Some(content.expand(reach, reach).offset(shadow.offset)),
            )
            .expect("both are some");
        }
        bounds
    }

    fn parse(effects: &LayerEffects) -> Result<Self, GpuRenderError> {
        let parse_color = |color: &str| -> Result<[f32; 4], GpuRenderError> {
            let color = CpuColor::from_hex(color).map_err(GpuRenderError::Effects)?;
            Ok([color.red, color.green, color.blue, color.alpha]
                .map(|value| f32::from(value) / 255.0))
        };
        Ok(Self {
            blur: effects.blur.clamp(0.0, 64.0) as f32,
            shadow: effects
                .shadow
                .as_ref()
                .map(|shadow| -> Result<_, GpuRenderError> {
                    Ok(EffectShadow {
                        color: parse_color(&shadow.color)?,
                        blur: shadow.blur.clamp(0.0, 64.0) as f32,
                        offset: [shadow.offset_x as f32, shadow.offset_y as f32],
                    })
                })
                .transpose()?,
            glow: effects
                .glow
                .as_ref()
                .map(|glow| -> Result<_, GpuRenderError> {
                    Ok(EffectShadow {
                        color: parse_color(&glow.color)?,
                        blur: glow.blur.clamp(0.0, 64.0) as f32,
                        offset: [0.0, 0.0],
                    })
                })
                .transpose()?,
        })
    }
}

struct GpuDraw {
    texture: LayerTexture,
    /// `0..6` (one quad) except for a path, which draws a quad per tile it
    /// covers.
    vertices: std::ops::Range<u32>,
    instances: std::ops::Range<u32>,
}

#[derive(Clone, Copy)]
struct LayerState {
    transform: Affine,
    opacity: f32,
    /// Index into `GpuRenderer::clip_entries` of the innermost clip the
    /// layer is drawn through.
    clip: Option<u32>,
}

impl LayerState {
    fn then(self, transform: &EvaluatedTransform, opacity: f64) -> Self {
        Self {
            transform: self.transform.multiply(Affine::from_transform(transform)),
            opacity: (self.opacity * opacity.clamp(0.0, 1.0) as f32).clamp(0.0, 1.0),
            clip: self.clip,
        }
    }
}

impl Default for LayerState {
    fn default() -> Self {
        Self {
            transform: Affine::IDENTITY,
            opacity: 1.0,
            clip: None,
        }
    }
}

/// The deepest nesting of clipped groups `layer.wgsl` walks (`MAX_CLIP_DEPTH`
/// there).
const MAX_CLIP_DEPTH: u32 = 8;

/// Floats of one clip in the storage buffer `layer.wgsl` reads: three
/// `vec4<f32>`s.
const CLIP_ENTRY_FLOATS: usize = 12;
const CLIP_ENTRY_SIZE: u64 = CLIP_ENTRY_FLOATS as u64 * 4;

/// One clipped group, in the form the fragment shader evaluates: the canvas
/// position of a pixel maps into the group's frame (relative to the clip
/// rectangle's centre), where a rounded-box distance gives the coverage.
struct ClipEntry {
    /// The inverse of the group's transform, `a`..`d`, then the translation
    /// that also moves the clip rectangle's centre to the origin.
    inverse: [f32; 6],
    half: [f32; 2],
    radius: f32,
    /// Local distance to canvas pixels, `sqrt(|det|)`; the same factor
    /// `celesta-renderer` uses.
    distance_scale: f32,
    /// The clip this one is nested in.
    parent: Option<u32>,
    /// 1 for a clip that is not nested.
    depth: u32,
}

impl ClipEntry {
    fn new(clip: &Clip, transform: Affine, parent: Option<u32>, depth: u32) -> Self {
        let determinant = transform.a * transform.d - transform.b * transform.c;
        let a = transform.d / determinant;
        let b = -transform.b / determinant;
        let c = -transform.c / determinant;
        let d = transform.a / determinant;
        let tx = -(a * transform.tx + c * transform.ty);
        let ty = -(b * transform.tx + d * transform.ty);
        let center_x = (clip.x + clip.width / 2.0) as f32;
        let center_y = (clip.y + clip.height / 2.0) as f32;
        Self {
            inverse: [a, b, c, d, tx - center_x, ty - center_y],
            half: [(clip.width / 2.0) as f32, (clip.height / 2.0) as f32],
            radius: clip.effective_corner_radius() as f32,
            distance_scale: determinant.abs().sqrt(),
            parent,
            depth,
        }
    }

    fn floats(&self) -> [f32; CLIP_ENTRY_FLOATS] {
        let [a, b, c, d, tx, ty] = self.inverse;
        [
            a,
            b,
            c,
            d,
            tx,
            ty,
            self.half[0],
            self.half[1],
            self.radius,
            self.distance_scale,
            self.parent.map_or(-1.0, |parent| parent as f32),
            0.0,
        ]
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

impl From<Affine> for PathTransform {
    fn from(transform: Affine) -> Self {
        Self {
            a: f64::from(transform.a),
            b: f64::from(transform.b),
            c: f64::from(transform.c),
            d: f64::from(transform.d),
            tx: f64::from(transform.tx),
            ty: f64::from(transform.ty),
        }
    }
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

    /// The largest and smallest factors the transform stretches a length by
    /// (its singular values).
    fn stretch(self) -> (f32, f32) {
        let sum = self.a * self.a + self.b * self.b + self.c * self.c + self.d * self.d;
        let determinant = (self.a * self.d - self.b * self.c).abs();
        let root = (sum * sum - 4.0 * determinant * determinant)
            .max(0.0)
            .sqrt();
        (
            ((sum + root) / 2.0).sqrt(),
            ((sum - root) / 2.0).max(0.0).sqrt(),
        )
    }

    /// How many texels of a texture with `raster_scale` texels per layer unit
    /// fall across one canvas pixel along the most shrunk direction.
    fn texels_per_pixel(self, raster_scale: f32) -> f32 {
        raster_scale / self.stretch().1.max(f32::MIN_POSITIVE)
    }

    /// Whether the transform only scales by `scale`, without rotating or
    /// mirroring, up to f32 rounding (a 360° rotation counts).
    fn is_uniform_scale(self, scale: f32) -> bool {
        let tolerance = 1e-5 * scale;
        (self.a - scale).abs() <= tolerance
            && self.b.abs() <= tolerance
            && self.c.abs() <= tolerance
            && (self.d - scale).abs() <= tolerance
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
    Effects(RenderError),
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
    /// A frame has more layers than one GPU buffer can hold.
    TooManyLayers(usize),
    /// Clipped groups are nested deeper than the shader walks.
    ClipsNestedTooDeep(u32),
    /// A frame's paths have more edges than one GPU buffer can hold.
    PathsTooComplex(usize),
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
            Self::Effects(error) => write!(formatter, "invalid GPU layer effect: {error}"),
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
            Self::TooManyLayers(layers) => {
                write!(formatter, "frame has too many layers for the GPU: {layers}")
            }
            Self::ClipsNestedTooDeep(depth) => write!(
                formatter,
                "clipped groups are nested {depth} deep, more than the GPU renderer's limit of {MAX_CLIP_DEPTH}"
            ),
            Self::PathsTooComplex(entries) => write!(
                formatter,
                "frame's paths have too many edges for the GPU: {entries} entries"
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
            Self::Effects(error) => Some(error),
            Self::MapCallbackDropped
            | Self::TooManyLayers(_)
            | Self::ClipsNestedTooDeep(_)
            | Self::PathsTooComplex(_)
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
        AssetLocation, BlendMode, Clip, EvaluatedTransform, Layer, LayerContent, MediaTiming,
        Paint, Point, Rational, ResolvedAsset, Scene, Stroke, TextStyle, Time,
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

    /// A 2x2 image layer whose pixels are seeded straight into the renderer's
    /// decoded-image map, so no file is read.
    fn seeded_image(renderer: &mut GpuRenderer, id: &str, x: f64, rgba: [u8; 4]) -> Layer {
        renderer.image_sources.insert_raster(
            id,
            image::RgbaImage::from_raw(2, 2, rgba.repeat(4)).unwrap(),
        );
        Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform {
                position: Point { x, y: 1.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Image {
                width: None,
                height: None,
                fit: None,
                asset: ResolvedAsset {
                    id: id.to_owned(),
                    location: AssetLocation::File {
                        path: format!("{id}.png"),
                    },
                },
            },
        }
    }

    #[test]
    fn rasterizes_a_frames_new_text_in_parallel_like_one_at_a_time() {
        let text_layer = |index: usize, text: &str| Layer {
            id: format!("label-{index}"),
            transform: EvaluatedTransform {
                position: Point {
                    x: 8.0,
                    y: 4.0 + index as f64 * 32.0,
                },
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Text {
                text: text.to_owned(),
                style: TextStyle {
                    font_size: Some(16.0),
                    fill: Some(Paint::Solid {
                        color: "#ffffff".to_owned(),
                    }),
                    ..TextStyle::default()
                },
                max_width: None,
                baseline_anchor: false,
            },
        };
        // The repeat shares a texture with the first label.
        let labels = ["CH00 +1.000", "CH01 -2.500", "CH02 +3.750", "CH00 +1.000"];
        let scene = |labels: &[(usize, &str)]| {
            let mut scene = empty_scene(160, 136);
            scene.layers = labels
                .iter()
                .map(|&(index, text)| text_layer(index, text))
                .collect();
            scene
        };
        let all: Vec<_> = labels.iter().copied().enumerate().collect();
        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::rgba(0, 0, 0, 255),
        }) else {
            return;
        };

        // Every label is new here, so they rasterize on forks in parallel.
        let together = renderer.render(&scene(&all)).unwrap();
        // White labels on black that do not overlap: drawn one per frame,
        // each by the renderer's own rasterizer, the brightest of the frames
        // is the frame with all of them.
        let mut one_at_a_time = vec![0_u8; together.pixels().len()];
        for label in &all {
            // An empty frame evicts every cached text texture first.
            renderer.render(&scene(&[])).unwrap();
            let frame = renderer.render(&scene(std::slice::from_ref(label))).unwrap();
            for (merged, &value) in one_at_a_time.iter_mut().zip(frame.pixels()) {
                *merged = (*merged).max(value);
            }
        }

        assert!(together.pixels().contains(&255));
        assert!(together.pixels() == one_at_a_time.as_slice());
    }

    #[test]
    fn reuses_unchanged_layer_textures_across_frames_and_evicts_unused_ones() {
        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::rgba(0, 0, 0, 255),
        }) else {
            return;
        };
        let scene = |layers: Vec<Layer>| {
            let mut scene = empty_scene(4, 2);
            scene.layers = layers;
            scene
        };
        let pixel = |frame: &GpuFrame, x: usize| frame.pixels()[x * 4..x * 4 + 4].to_vec();

        let first = scene(vec![
            seeded_image(&mut renderer, "red", 1.0, [255, 0, 0, 255]),
            seeded_image(&mut renderer, "blue", 3.0, [0, 0, 255, 255]),
        ]);
        let frame = renderer.render(&first).unwrap();
        assert_eq!(pixel(&frame, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&frame, 3), [0, 0, 255, 255]);
        assert_eq!(renderer.textures.len(), 2);

        // Another image in the same place is a different texture; the blue
        // one this frame no longer uses is dropped.
        let second = scene(vec![
            seeded_image(&mut renderer, "red", 1.0, [255, 0, 0, 255]),
            seeded_image(&mut renderer, "green", 3.0, [0, 255, 0, 255]),
        ]);
        let frame = renderer.render(&second).unwrap();
        assert_eq!(pixel(&frame, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&frame, 3), [0, 255, 0, 255]);
        assert_eq!(renderer.textures.len(), 2);
        assert!(
            !renderer
                .textures
                .keys()
                .any(|key| key.starts_with("image\0blue\0"))
        );

        // Cached textures render the same frame again, including through
        // the pipelined readback path.
        let again = renderer.render(&second).unwrap();
        assert_eq!(again, frame);
        assert!(renderer.submit(&second).unwrap().is_none());
        assert_eq!(renderer.drain().unwrap(), vec![frame]);

        // Rects are shaded on the GPU and never occupy a texture.
        let rects = scene(vec![
            solid_rect("red", 1.0, 2.0, 2.0, "#ff0000"),
            solid_rect("blue", 3.0, 2.0, 2.0, "#0000ff"),
        ]);
        let frame = renderer.render(&rects).unwrap();
        assert_eq!(pixel(&frame, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&frame, 3), [0, 0, 255, 255]);
        assert!(renderer.textures.is_empty());
    }

    #[test]
    fn batches_consecutive_rects_into_one_draw_in_painter_order() {
        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::rgba(0, 0, 0, 255),
        }) else {
            return;
        };
        let mut scene = empty_scene(4, 2);
        scene.layers = vec![
            solid_rect("red", 2.0, 4.0, 2.0, "#ff0000"),
            solid_rect("green", 2.5, 3.0, 2.0, "#00ff00"),
            // Over both rects' right half, then covered again on the right.
            seeded_image(&mut renderer, "white", 2.0, [255, 255, 255, 255]),
            solid_rect("blue", 3.5, 1.0, 2.0, "#0000ff"),
            solid_rect("half", 3.5, 1.0, 2.0, "#ff000080"),
        ];

        let draws = renderer.prepare_draws(&scene).unwrap();
        assert!(draws.composite.is_none());
        let runs: Vec<_> = draws
            .steps
            .iter()
            .map(|step| match step {
                GpuStep::Draw(draw) => draw.instances.clone(),
                _ => unreachable!("a frame without blend modes only draws"),
            })
            .collect();
        assert_eq!(runs, [0..2, 2..3, 3..5]);

        let frame = renderer.render(&scene).unwrap();
        let pixel = |x: usize| frame.pixels()[x * 4..x * 4 + 4].to_vec();
        assert_eq!(pixel(0), [255, 0, 0, 255]);
        assert_eq!(pixel(1), [255, 255, 255, 255]);
        assert_eq!(pixel(2), [255, 255, 255, 255]);
        assert_eq!(pixel(3), [128, 0, 127, 255]);
    }

    fn blend_rect(
        id: &str,
        x: f64,
        y: f64,
        size: f64,
        color: &str,
        blend_mode: BlendMode,
    ) -> Layer {
        Layer {
            blend_mode,
            transform: EvaluatedTransform {
                position: Point { x, y },
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            ..solid_rect(id, 0.0, size, size, color)
        }
    }

    /// A blended draw copies only the part of its canvas under it to the
    /// backdrop, so that part must cover every pixel its quad shades: a
    /// filtered (scaled or rotated) layer's quad reaches one source texel
    /// past its edge, which is several pixels once it is magnified. A
    /// renderer whose backdrop still holds an earlier frame must draw the
    /// same frame as a fresh one.
    #[test]
    fn blends_a_magnified_layer_against_its_whole_backdrop() {
        let options = GpuRenderOptions {
            background: Color::rgba(10, 20, 30, 255),
        };
        let (Some(mut fresh), Some(mut reused)) = (renderer(options), renderer(options)) else {
            return;
        };
        let mut earlier = empty_scene(64, 64);
        earlier.layers = vec![blend_rect(
            "white",
            0.0,
            0.0,
            64.0,
            "#ffffff",
            BlendMode::Screen,
        )];
        reused.render(&earlier).unwrap();

        let mut scene = empty_scene(64, 64);
        let mut magnified =
            blend_rect("magnified", 0.0, 0.0, 4.0, "#40a0ff", BlendMode::Difference);
        magnified.transform = EvaluatedTransform {
            position: Point { x: 32.0, y: 32.0 },
            anchor: Point { x: 0.5, y: 0.5 },
            rotation: 20.0,
            scale: Point { x: 6.0, y: 6.0 },
        };
        scene.layers = vec![
            blend_rect("left", 0.0, 0.0, 32.0, "#203040", BlendMode::Normal),
            blend_rect("right", 32.0, 0.0, 32.0, "#c08040", BlendMode::Normal),
            blend_rect("bottom", 0.0, 32.0, 64.0, "#608060", BlendMode::Normal),
            magnified,
        ];
        let expected = fresh.render(&scene).unwrap();
        let frame = reused.render(&scene).unwrap();
        let difference = frame
            .pixels()
            .iter()
            .zip(expected.pixels())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert_eq!(difference, 0, "the earlier frame's backdrop shows through");
    }

    fn blend_group(opacity: f64, blend_mode: BlendMode, layers: Vec<Layer>) -> Layer {
        Layer {
            id: "group".to_owned(),
            transform: EvaluatedTransform {
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            opacity,
            blend_mode,
            effects: Default::default(),
            content: LayerContent::Group { layers, clip: None },
        }
    }

    /// Every blend mode, at partial opacity and with translucent fills, on
    /// its own and inside nested isolated groups.
    fn blend_scene() -> Scene {
        let modes = [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Add,
            BlendMode::Difference,
        ];
        let mut scene = empty_scene(24, 16);
        scene.layers = vec![
            blend_rect("dark", 0.0, 0.0, 12.0, "#202830", BlendMode::Normal),
            blend_rect("light", 12.0, 0.0, 12.0, "#e0d0c0", BlendMode::Normal),
            blend_rect("warm", 6.0, 4.0, 12.0, "#ff4d1f80", BlendMode::Normal),
        ];
        for (index, mode) in modes.into_iter().enumerate() {
            let x = index as f64 * 4.0;
            let mut layer = blend_rect("mode", x, 2.0, 5.0, "#6ab04cc0", mode);
            layer.opacity = 0.8;
            scene.layers.push(layer);
            scene.layers.push(blend_group(
                0.7,
                mode,
                vec![
                    blend_rect("under", x, 9.0, 4.0, "#3c6382", BlendMode::Normal),
                    blend_rect("over", x + 1.0, 10.0, 4.0, "#f8c291a0", BlendMode::Screen),
                    blend_group(
                        1.0,
                        BlendMode::Difference,
                        vec![blend_rect(
                            "nested",
                            x + 2.0,
                            12.0,
                            3.0,
                            "#ffffff",
                            BlendMode::Normal,
                        )],
                    ),
                ],
            ));
        }
        scene
    }

    #[test]
    fn blends_layers_and_isolated_groups_like_the_cpu_renderer() {
        let background = Color::rgba(10, 20, 30, 255);
        let Some(mut renderer) = renderer(GpuRenderOptions { background }) else {
            return;
        };
        let scene = blend_scene();
        let draws = renderer.prepare_draws(&scene).unwrap();
        assert!(draws.composite.is_some());

        let expected = celesta_renderer::CpuRenderer::new(celesta_renderer::RenderOptions {
            background: celesta_renderer::Color::rgba(10, 20, 30, 255),
        })
        .render(&scene)
        .unwrap();
        // Twice: the second frame reuses the canvases the first created.
        for _ in 0..2 {
            let frame = renderer.render(&scene).unwrap();
            for (index, (gpu, cpu)) in frame
                .pixels()
                .chunks_exact(4)
                .zip(expected.pixels().chunks_exact(4))
                .enumerate()
            {
                let close = gpu
                    .iter()
                    .zip(cpu)
                    .all(|(gpu, cpu)| gpu.abs_diff(*cpu) <= 2);
                assert!(
                    close,
                    "pixel ({}, {}): GPU {gpu:?}, CPU {cpu:?}",
                    index % 24,
                    index / 24,
                );
            }
        }

        // A preview target of another size and format gets the same frame,
        // scaled into its viewport.
        let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 48,
                height: 32,
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
                &scene,
                GpuRenderTarget {
                    view: &view,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    width: 48,
                    height: 32,
                },
            )
            .unwrap();
    }

    #[test]
    fn shades_rects_like_their_rasterized_texture() {
        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::rgba(10, 20, 30, 255),
        }) else {
            return;
        };
        let stroke = |color: &str, width: f64| Stroke {
            paint: Paint::Solid {
                color: color.to_owned(),
            },
            width,
        };
        let fill = |color: &str| {
            Some(Paint::Solid {
                color: color.to_owned(),
            })
        };
        let stops = |stops: &[(f64, &str)]| -> Vec<celesta_composition::GradientStop> {
            stops
                .iter()
                .map(|(offset, color)| celesta_composition::GradientStop {
                    offset: *offset,
                    color: (*color).to_owned(),
                })
                .collect()
        };
        let linear = |(x1, y1, x2, y2): (f64, f64, f64, f64), list: &[(f64, &str)]| {
            Some(Paint::Linear {
                start: Point { x: x1, y: y1 },
                end: Point { x: x2, y: y2 },
                stops: stops(list),
            })
        };
        let radial = |(x, y, radius): (f64, f64, f64), list: &[(f64, &str)]| {
            Some(Paint::Radial {
                center: Point { x, y },
                radius,
                stops: stops(list),
            })
        };
        // (width, height, corner radius, fill, stroke, rotation, scale, opacity)
        let cases = [
            (12.0, 7.0, 0.0, fill("#ff8000"), None, 0.0, 1.0, 1.0),
            (10.3, 5.6, 0.0, fill("#ff800080"), None, 0.0, 1.0, 0.6),
            (31.7, 1.8, 0.0, fill("#EF402B"), None, 23.5, 1.0, 0.8),
            (40.0, 24.0, 9.0, fill("#2060ff"), None, -61.0, 1.0, 1.0),
            (
                36.4,
                20.2,
                6.5,
                fill("#ffffff"),
                Some(stroke("#ff0000c0", 2.5)),
                12.0,
                1.7,
                0.9,
            ),
            (
                28.0,
                18.0,
                4.0,
                None,
                Some(stroke("#00ff00", 1.5)),
                0.0,
                1.0,
                1.0,
            ),
            (0.4, 9.0, 0.0, fill("#ffff00"), None, 45.0, 3.0, 1.0),
            // Gradients, shaded from the stops rather than rasterized.
            (
                40.0,
                24.0,
                6.0,
                linear(
                    (0.0, 0.0, 0.0, 24.0),
                    &[(0.0, "#102040"), (0.6, "#c07090"), (1.0, "#f0c090")],
                ),
                None,
                0.0,
                1.0,
                1.0,
            ),
            (
                30.0,
                30.0,
                15.0,
                radial(
                    (12.0, 10.0, 18.0),
                    &[(0.0, "#ffe8c0ff"), (0.3, "#ffb07880"), (1.0, "#ffb07800")],
                ),
                None,
                30.0,
                1.5,
                0.8,
            ),
            (
                36.0,
                20.0,
                4.0,
                linear(
                    (0.0, 0.0, 36.0, 20.0),
                    &[(0.0, "#2060ff"), (1.0, "#ff6020")],
                ),
                Some(Stroke {
                    paint: linear(
                        (36.0, 0.0, 0.0, 0.0),
                        &[(0.0, "#ffffffc0"), (1.0, "#00ff8040")],
                    )
                    .unwrap(),
                    width: 3.0,
                }),
                -12.0,
                1.0,
                0.9,
            ),
            (
                // A hard edge: two stops at one offset, and stops out of order.
                24.0,
                12.0,
                0.0,
                linear(
                    (0.0, 0.0, 24.0, 0.0),
                    &[
                        (1.0, "#0000ff"),
                        (0.5, "#00ff00"),
                        (0.5, "#ff0000"),
                        (0.0, "#000000"),
                    ],
                ),
                None,
                0.0,
                1.0,
                1.0,
            ),
        ];
        for (index, (width, height, corner_radius, fill, stroke, rotation, scale, opacity)) in
            cases.into_iter().enumerate()
        {
            let transform = EvaluatedTransform {
                position: Point { x: 32.3, y: 29.6 },
                rotation,
                scale: Point { x: scale, y: scale },
                ..EvaluatedTransform::default()
            };
            let rect = Layer {
                id: "rect".to_owned(),
                transform,
                opacity,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Rect {
                    width,
                    height,
                    fill: fill.clone(),
                    stroke: stroke.clone(),
                    corner_radius,
                },
            };
            let rasterized = celesta_renderer::rasterize_rect(
                width,
                height,
                corner_radius,
                fill.as_ref(),
                stroke.as_ref(),
            )
            .unwrap();
            let id = format!("rasterized-{index}");
            renderer.image_sources.insert_raster(
                &id,
                image::RgbaImage::from_raw(
                    rasterized.width(),
                    rasterized.height(),
                    rasterized.into_pixels(),
                )
                .unwrap(),
            );
            let image = Layer {
                id: id.clone(),
                transform,
                opacity,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Image {
                    width: None,
                    height: None,
                    fit: None,
                    asset: ResolvedAsset {
                        id: id.clone(),
                        location: AssetLocation::File { path: id },
                    },
                },
            };

            let mut scene = empty_scene(64, 60);
            scene.layers = vec![rect];
            let shaded = renderer.render(&scene).unwrap();
            scene.layers = vec![image];
            let sampled = renderer.render(&scene).unwrap();
            // The shader works in f32 and the rasterizer in f64, so a
            // stroke's blend of the two colors can land on a rounding tie in
            // one and just miss it in the other: one code value at most.
            let max_difference = shaded
                .pixels()
                .iter()
                .zip(sampled.pixels())
                .map(|(shaded, sampled)| shaded.abs_diff(*sampled))
                .max()
                .unwrap();
            assert!(
                max_difference <= 1,
                "case {index}: channels differ by up to {max_difference}"
            );
        }
    }

    /// A rect at `(x, y)` in its parent, top-left anchored.
    fn corner_rect(id: &str, x: f64, y: f64, width: f64, height: f64, color: &str) -> Layer {
        Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform {
                position: Point { x, y },
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
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

    fn clipped_group(transform: EvaluatedTransform, clip: Clip, layers: Vec<Layer>) -> Layer {
        Layer {
            id: "clipped".to_owned(),
            transform,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Group {
                layers,
                clip: Some(clip),
            },
        }
    }

    fn pixel_at(frame: &GpuFrame, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * frame.width() + x) * 4) as usize;
        frame.pixels()[offset..offset + 4].try_into().unwrap()
    }

    fn max_channel_difference(gpu: &GpuFrame, cpu: &celesta_renderer::RgbaFrame) -> u8 {
        assert_eq!(gpu.pixels().len(), cpu.pixels().len());
        gpu.pixels()
            .iter()
            .zip(cpu.pixels())
            .map(|(gpu, cpu)| gpu.abs_diff(*cpu))
            .max()
            .unwrap()
    }

    #[test]
    fn blurred_group_with_shadow_and_glow_matches_cpu() {
        use celesta_composition::{LayerEffects, LayerGlow, LayerShadow};

        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let mut scene = empty_scene(64, 64);
        scene.layers = vec![Layer {
            id: "fx".to_owned(),
            transform: EvaluatedTransform::default(),
            opacity: 0.8,
            blend_mode: BlendMode::Screen,
            effects: LayerEffects {
                blur: 1.5,
                shadow: Some(LayerShadow {
                    color: "#0000ffb0".to_owned(),
                    blur: 2.0,
                    offset_x: 5.5,
                    offset_y: 3.25,
                }),
                glow: Some(LayerGlow {
                    color: "#ff000080".to_owned(),
                    blur: 3.0,
                }),
            },
            content: LayerContent::Group {
                layers: vec![corner_rect("child", 20.0, 20.0, 16.0, 12.0, "#ffe080")],
                clip: None,
            },
        }];
        let gpu = renderer.render(&scene).unwrap();
        let cpu = celesta_renderer::CpuRenderer::default()
            .render(&scene)
            .unwrap();
        let difference = max_channel_difference(&gpu, &cpu);
        assert!(
            difference <= 5,
            "effect channels differ by up to {difference}"
        );
        assert_ne!(pixel_at(&gpu, 25, 25), pixel_at(&gpu, 0, 0));

        scene.layers[0].effects.blur = 0.0;
        scene.layers[0].effects.glow = None;
        scene.layers[0].effects.shadow.as_mut().unwrap().blur = 0.0;
        let gpu = renderer.render(&scene).unwrap();
        let cpu = celesta_renderer::CpuRenderer::default()
            .render(&scene)
            .unwrap();
        let difference = max_channel_difference(&gpu, &cpu);
        assert!(
            difference <= 5,
            "zero-radius shadow differs by up to {difference}"
        );
    }

    /// Effects filter only around what their layers cover; small, nested,
    /// empty, and edge-crossing effects must still match the CPU
    /// renderer, frame after frame as the pooled canvases are reused.
    #[test]
    fn effects_on_small_layers_match_cpu() {
        use celesta_composition::{LayerEffects, LayerGlow, LayerShadow};

        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let effect = |id: &str, effects: LayerEffects, layers: Vec<Layer>| Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects,
            content: LayerContent::Group { layers, clip: None },
        };
        let glow = |color: &str, blur: f64| LayerEffects {
            glow: Some(LayerGlow {
                color: color.to_owned(),
                blur,
            }),
            ..LayerEffects::default()
        };
        let mut hidden = corner_rect("hidden", 40.0, 40.0, 8.0, 8.0, "#ffffff");
        hidden.opacity = 0.0;
        let mut nested = effect(
            "nested",
            LayerEffects::default(),
            vec![corner_rect("inner", 84.0, 6.0, 8.0, 10.0, "#ff60c0")],
        );
        nested.blend_mode = BlendMode::Screen;
        let mut scene = empty_scene(96, 64);
        scene.layers = vec![
            corner_rect("backdrop", 0.0, 0.0, 96.0, 64.0, "#203040"),
            effect(
                "small",
                glow("#ffd060c0", 4.0),
                vec![corner_rect("small", 18.0, 30.0, 10.0, 6.0, "#80ffa0")],
            ),
            effect(
                "edge",
                LayerEffects {
                    blur: 1.0,
                    shadow: Some(LayerShadow {
                        color: "#000000a0".to_owned(),
                        blur: 3.0,
                        offset_x: 7.5,
                        offset_y: -4.25,
                    }),
                    glow: None,
                },
                vec![nested],
            ),
            effect("empty", glow("#ff0000ff", 6.0), vec![hidden]),
            effect(
                "second",
                glow("#60a0ffff", 2.5),
                vec![corner_rect("dot", 50.0, 50.0, 4.0, 4.0, "#ffffff")],
            ),
        ];
        let cpu = celesta_renderer::CpuRenderer::default()
            .render(&scene)
            .unwrap();
        for frame in 0..3 {
            let gpu = renderer.render(&scene).unwrap();
            let difference = max_channel_difference(&gpu, &cpu);
            assert!(
                difference <= 5,
                "frame {frame}: effect channels differ by up to {difference}"
            );
        }
    }

    /// Large blurs pair their taps into bilinear fetches; the result must
    /// stay within the CPU renderer's exact Gaussian, including where the
    /// kernel runs off the canvas and under a fractional shadow offset.
    #[test]
    fn large_blurs_match_cpu() {
        use celesta_composition::{LayerEffects, LayerGlow, LayerShadow};

        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let blob = |id: &str, effects: LayerEffects, layers: Vec<Layer>| Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform::default(),
            opacity: 0.55,
            blend_mode: BlendMode::Screen,
            effects,
            content: LayerContent::Group { layers, clip: None },
        };
        let mut scene = empty_scene(160, 112);
        scene.layers = vec![
            corner_rect("backdrop", 0.0, 0.0, 160.0, 112.0, "#101828"),
            blob(
                "blur",
                LayerEffects {
                    blur: 24.0,
                    ..LayerEffects::default()
                },
                vec![corner_rect("blur", 30.0, 20.0, 70.0, 50.0, "#ff40a0")],
            ),
            blob(
                "edge",
                LayerEffects {
                    blur: 13.5,
                    shadow: Some(LayerShadow {
                        color: "#40c0ffd0".to_owned(),
                        blur: 17.0,
                        offset_x: -9.5,
                        offset_y: 6.75,
                    }),
                    glow: Some(LayerGlow {
                        color: "#ffe060ff".to_owned(),
                        blur: 64.0,
                    }),
                },
                vec![corner_rect("edge", 120.0, 70.0, 40.0, 42.0, "#a0ff60")],
            ),
        ];
        let gpu = renderer.render(&scene).unwrap();
        let cpu = celesta_renderer::CpuRenderer::default()
            .render(&scene)
            .unwrap();
        let difference = max_channel_difference(&gpu, &cpu);
        assert!(
            difference <= 5,
            "large blur channels differ by up to {difference}"
        );

        // Tiny radii, where the Gaussian's tail underflows.
        for radius in [0.05, 0.3] {
            for layer in &mut scene.layers[1..] {
                layer.effects.blur = radius;
                if let Some(shadow) = &mut layer.effects.shadow {
                    shadow.blur = radius;
                }
                if let Some(glow) = &mut layer.effects.glow {
                    glow.blur = radius;
                }
            }
            let gpu = renderer.render(&scene).unwrap();
            let cpu = celesta_renderer::CpuRenderer::default()
                .render(&scene)
                .unwrap();
            let difference = max_channel_difference(&gpu, &cpu);
            assert!(
                difference <= 5,
                "radius {radius}: channels differ by up to {difference}"
            );
        }
    }

    #[test]
    fn clips_groups_like_the_cpu_renderer() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let scenes = [
            // A rounded clip in a translated group, with a translucent layer
            // straddling its edge.
            vec![clipped_group(
                EvaluatedTransform {
                    position: Point { x: 4.0, y: 4.0 },
                    ..EvaluatedTransform::default()
                },
                Clip {
                    x: 8.0,
                    y: 8.0,
                    width: 40.0,
                    height: 30.0,
                    corner_radius: 10.0,
                },
                vec![
                    corner_rect("red", 0.0, 0.0, 60.0, 60.0, "#FF0000FF"),
                    Layer {
                        opacity: 0.5,
                        ..corner_rect("blue", 30.0, 20.0, 30.0, 30.0, "#0000FFFF")
                    },
                ],
            )],
            // Nested clips inside a scaled group. The rect overhangs both
            // clips: the GPU filters a scaled layer's own edges where the CPU
            // renderer repeats texels, so only clip edges are compared.
            vec![clipped_group(
                EvaluatedTransform {
                    scale: Point { x: 2.0, y: 2.0 },
                    ..EvaluatedTransform::default()
                },
                Clip {
                    x: 0.0,
                    y: 0.0,
                    width: 24.0,
                    height: 24.0,
                    corner_radius: 4.0,
                },
                vec![clipped_group(
                    EvaluatedTransform {
                        position: Point { x: 10.0, y: 0.0 },
                        ..EvaluatedTransform::default()
                    },
                    Clip {
                        x: 0.0,
                        y: 0.0,
                        width: 30.0,
                        height: 10.0,
                        corner_radius: 0.0,
                    },
                    vec![corner_rect("red", -4.0, -4.0, 40.0, 40.0, "#FF0000FF")],
                )],
            )],
        ];
        for (index, layers) in scenes.into_iter().enumerate() {
            let mut scene = empty_scene(64, 64);
            scene.layers = layers;
            let gpu = renderer.render(&scene).unwrap();
            let cpu = celesta_renderer::CpuRenderer::default()
                .render(&scene)
                .unwrap();
            let difference = max_channel_difference(&gpu, &cpu);
            assert!(
                difference <= 2,
                "scene {index}: channels differ by up to {difference}"
            );
        }
    }

    #[test]
    fn benchmark_path_transform_matches_prepared_layer() {
        use celesta_composition::{LineCap, LineJoin, PathCommand};

        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let layer = Layer {
            id: "transformed-path".to_owned(),
            transform: EvaluatedTransform {
                position: Point {
                    x: 10.123,
                    y: 24.456,
                },
                rotation: 37.123,
                scale: Point { x: 2.5, y: -0.75 },
                anchor: Point { x: 8.0, y: 12.0 },
            },
            opacity: 0.7,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Path {
                commands: vec![
                    PathCommand::MoveTo { x: 0.0, y: 0.0 },
                    PathCommand::LineTo { x: 16.0, y: 2.0 },
                ],
                fill: None,
                stroke: Some(Stroke {
                    paint: Paint::Solid {
                        color: "#FFFFFF".to_owned(),
                    },
                    width: 1.5,
                }),
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                miter_limit: 4.0,
            },
        };
        renderer.scene_size = (64, 64);
        let mut items = Vec::new();
        renderer
            .prepare_layer(&layer, LayerState::default(), &mut items)
            .unwrap();
        let [
            PreparedItem::Layer(PreparedLayer {
                content: PreparedContent::Path(_),
                ..
            }),
        ] = items.as_slice()
        else {
            panic!("expected one path layer");
        };
        let LayerContent::Path {
            commands,
            fill,
            stroke,
            line_cap,
            line_join,
            miter_limit,
        } = &layer.content
        else {
            unreachable!("the layer is a path");
        };
        let shape = PathShape {
            commands,
            fill: fill.as_ref(),
            stroke: stroke.as_ref(),
            line_cap: *line_cap,
            line_join: *line_join,
            miter_limit: *miter_limit,
        };
        let path = flatten_path(&shape, path_transform(&layer.transform), 64, 64)
            .unwrap()
            .expect("the path is visible");
        let mut entries = Vec::new();
        ShadedPath::new(&path, &mut Vec::new(), &mut entries);
        assert_eq!(renderer.path_entries, entries);
    }

    /// How far a path shaded on the GPU may stray from the CPU renderer's.
    /// Both sample four scanlines per pixel row, but the CPU renderer rounds
    /// where they cross edges to quarter pixels and flattens curves more
    /// coarsely, so a pixel on an edge can be off by a few samples of 1/16
    /// of its coverage. On the high-contrast, sub-pixel strokes and curves
    /// below that is up to `PATH_MAX_DIFFERENCE` in a channel (59 at most
    /// when measured), while across a frame the channels differ by under
    /// `PATH_MEAN_DIFFERENCE` on average (0.43) and at most `PATH_FAR_SHARE`
    /// of them by more than `PATH_CLOSE_DIFFERENCE` (0.59%). The GPU's are
    /// the closer to exact: against an 8x supersampled CPU rendering, the
    /// curves case differs by up to 28 (0.18 on average) on the GPU and 52
    /// (0.54) on the CPU.
    const PATH_MAX_DIFFERENCE: u8 = 64;
    const PATH_MEAN_DIFFERENCE: f64 = 0.5;
    const PATH_CLOSE_DIFFERENCE: u8 = 16;
    const PATH_FAR_SHARE: f64 = 0.01;

    fn assert_paths_match(case: &str, gpu: &[u8], cpu: &[u8]) {
        assert_eq!(gpu.len(), cpu.len());
        let differences: Vec<u8> = gpu.iter().zip(cpu).map(|(a, b)| a.abs_diff(*b)).collect();
        let max = differences.iter().copied().max().unwrap_or(0);
        let mean =
            differences.iter().map(|&d| f64::from(d)).sum::<f64>() / differences.len() as f64;
        let far = differences
            .iter()
            .filter(|&&d| d > PATH_CLOSE_DIFFERENCE)
            .count() as f64
            / differences.len() as f64;
        assert!(
            max <= PATH_MAX_DIFFERENCE && mean <= PATH_MEAN_DIFFERENCE && far <= PATH_FAR_SHARE,
            "{case}: channels differ by up to {max}, {mean:.3} on average, \
             {:.3}% by more than {PATH_CLOSE_DIFFERENCE}",
            far * 100.0
        );
    }

    fn assert_paths_match_cpu(renderer: &mut GpuRenderer, case: &str, layers: Vec<Layer>) {
        let mut scene = empty_scene(96, 64);
        scene.layers = layers;
        let gpu = renderer.render(&scene).unwrap();
        let cpu = celesta_renderer::CpuRenderer::default()
            .render(&scene)
            .unwrap();
        assert_paths_match(case, gpu.pixels(), cpu.pixels());
    }

    fn polyline(points: &[(f64, f64)], closed: bool) -> Vec<celesta_composition::PathCommand> {
        use celesta_composition::PathCommand;

        let mut commands: Vec<_> = points
            .iter()
            .enumerate()
            .map(|(index, &(x, y))| match index {
                0 => PathCommand::MoveTo { x, y },
                _ => PathCommand::LineTo { x, y },
            })
            .collect();
        if closed {
            commands.push(PathCommand::Close);
        }
        commands
    }

    fn solid(color: &str) -> Paint {
        Paint::Solid {
            color: color.to_owned(),
        }
    }

    fn path_layer(
        id: &str,
        commands: Vec<celesta_composition::PathCommand>,
        fill: Option<Paint>,
        stroke: Option<(Paint, f64)>,
    ) -> Layer {
        Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Path {
                commands,
                fill,
                stroke: stroke.map(|(paint, width)| Stroke { paint, width }),
                line_cap: celesta_composition::LineCap::Butt,
                line_join: celesta_composition::LineJoin::Miter,
                miter_limit: celesta_composition::DEFAULT_MITER_LIMIT,
            },
        }
    }

    fn styled(
        mut layer: Layer,
        cap: celesta_composition::LineCap,
        join: celesta_composition::LineJoin,
        limit: f64,
    ) -> Layer {
        if let LayerContent::Path {
            line_cap,
            line_join,
            miter_limit,
            ..
        } = &mut layer.content
        {
            (*line_cap, *line_join, *miter_limit) = (cap, join, limit);
        }
        layer
    }

    fn group(transform: EvaluatedTransform, layers: Vec<Layer>) -> Layer {
        Layer {
            id: "group".to_owned(),
            transform,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Group { layers, clip: None },
        }
    }

    #[test]
    fn shades_curves_and_closed_paths_like_the_cpu_renderer() {
        use celesta_composition::{LineCap, LineJoin, PathCommand};

        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let blob = vec![
            PathCommand::MoveTo { x: 8.0, y: 32.0 },
            PathCommand::CubicTo {
                x1: 8.0,
                y1: 4.0,
                x2: 50.0,
                y2: 2.0,
                x: 56.0,
                y: 28.0,
            },
            PathCommand::QuadTo {
                x1: 62.0,
                y1: 60.0,
                x: 30.0,
                y: 58.0,
            },
            PathCommand::Close,
        ];
        assert_paths_match_cpu(
            &mut renderer,
            "curves",
            vec![
                styled(
                    path_layer(
                        "blob",
                        blob,
                        Some(solid("#3366FFAA")),
                        Some((solid("#FFCC00"), 2.5)),
                    ),
                    LineCap::Butt,
                    LineJoin::Round,
                    4.0,
                ),
                // A closed square's seam joins like its other corners.
                path_layer(
                    "square",
                    polyline(
                        &[(66.0, 8.0), (90.0, 8.0), (90.0, 32.0), (66.0, 32.0)],
                        true,
                    ),
                    None,
                    Some((solid("#66FF99"), 3.0)),
                ),
                // The same square left open shows its butt ends instead.
                path_layer(
                    "open",
                    polyline(
                        &[
                            (66.0, 40.0),
                            (90.0, 40.0),
                            (90.0, 60.0),
                            (66.0, 60.0),
                            (66.0, 40.0),
                        ],
                        false,
                    ),
                    None,
                    Some((solid("#FF6699"), 3.0)),
                ),
            ],
        );
    }

    #[test]
    fn shades_joins_caps_and_zero_length_segments_like_the_cpu_renderer() {
        use celesta_composition::{LineCap, LineJoin};

        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let spike = |id: &str, y: f64, limit: f64| {
            styled(
                path_layer(
                    id,
                    polyline(&[(4.0, y), (60.0, y + 6.0), (4.0, y + 12.0)], false),
                    None,
                    Some((solid("#FF4040"), 3.0)),
                ),
                LineCap::Butt,
                LineJoin::Miter,
                limit,
            )
        };
        let dot = |id: &str, x: f64, cap: LineCap| {
            styled(
                path_layer(
                    id,
                    polyline(&[(x, 54.0), (x, 54.0)], false),
                    None,
                    Some((solid("#40C0FF"), 6.0)),
                ),
                cap,
                LineJoin::Miter,
                4.0,
            )
        };
        assert_paths_match_cpu(
            &mut renderer,
            "joins and caps",
            vec![
                // Past the miter limit, an acute join is beveled; within a
                // high one, it comes to a point.
                spike("beveled", 2.0, 4.0),
                spike("mitered", 18.0, 40.0),
                // A repeated point in the middle of a polyline.
                styled(
                    path_layer(
                        "repeated",
                        polyline(
                            &[(66.0, 4.0), (80.0, 20.0), (80.0, 20.0), (92.0, 6.0)],
                            false,
                        ),
                        None,
                        Some((solid("#C0FF40"), 4.0)),
                    ),
                    LineCap::Square,
                    LineJoin::Round,
                    4.0,
                ),
                // Zero-length segments: round and square caps draw a dot,
                // butt caps nothing.
                dot("round", 70.0, LineCap::Round),
                dot("square", 80.0, LineCap::Square),
                dot("butt", 90.0, LineCap::Butt),
            ],
        );
    }

    #[test]
    fn shades_thin_lines_and_scaled_strokes_like_the_cpu_renderer() {
        use celesta_composition::{LineCap, LineJoin, PathCommand};

        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let line = |id: &str, from: (f64, f64), to: (f64, f64), width: f64| {
            path_layer(
                id,
                polyline(&[from, to], false),
                None,
                Some((solid("#FFFFFF"), width)),
            )
        };
        assert_paths_match_cpu(
            &mut renderer,
            "thin and scaled",
            vec![
                line("shallow", (2.0, 4.0), (60.0, 14.0), 0.5),
                line("diagonal", (2.0, 16.0), (40.0, 60.0), 1.0),
                line("steep", (50.0, 20.0), (56.0, 62.0), 0.75),
                styled(
                    path_layer(
                        "hair",
                        vec![
                            PathCommand::MoveTo { x: 2.0, y: 62.0 },
                            PathCommand::CubicTo {
                                x1: 20.0,
                                y1: 20.0,
                                x2: 40.0,
                                y2: 70.0,
                                x: 62.0,
                                y: 30.0,
                            },
                        ],
                        None,
                        Some((solid("#FFFFFF"), 0.4)),
                    ),
                    LineCap::Round,
                    LineJoin::Bevel,
                    4.0,
                ),
                // A non-uniform scale stretches the stroke with the layer,
                // and a negative one mirrors it.
                group(
                    EvaluatedTransform {
                        position: Point { x: 64.5, y: 10.25 },
                        scale: Point { x: 2.5, y: 0.75 },
                        ..EvaluatedTransform::default()
                    },
                    vec![path_layer(
                        "stretched",
                        polyline(&[(0.0, 0.0), (10.0, 4.0), (6.0, 20.0), (1.0, 12.0)], true),
                        None,
                        Some((solid("#33CC66"), 1.5)),
                    )],
                ),
                group(
                    EvaluatedTransform {
                        position: Point { x: 92.0, y: 40.0 },
                        scale: Point { x: -1.0, y: 1.5 },
                        ..EvaluatedTransform::default()
                    },
                    vec![path_layer(
                        "mirrored",
                        polyline(&[(0.0, 0.0), (20.0, 4.0), (4.0, 14.0)], false),
                        None,
                        Some((solid("#FF9933"), 1.25)),
                    )],
                ),
            ],
        );
    }

    #[test]
    fn composites_translucent_strokes_and_fills_once_like_the_cpu_renderer() {
        use celesta_composition::{LineCap, LineJoin};

        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let mut eight = styled(
            path_layer(
                "eight",
                polyline(&[(4.0, 4.0), (40.0, 40.0), (40.0, 4.0), (4.0, 40.0)], true),
                None,
                Some((solid("#FF000080"), 6.0)),
            ),
            LineCap::Butt,
            LineJoin::Round,
            4.0,
        );
        eight.opacity = 0.7;
        let mut badge = path_layer(
            "badge",
            polyline(
                &[(52.0, 8.0), (90.0, 8.0), (90.0, 56.0), (52.0, 56.0)],
                true,
            ),
            Some(solid("#2060FFC0")),
            Some((solid("#FFFFFF80"), 8.0)),
        );
        badge.opacity = 0.6;
        let layers = vec![eight, badge];
        assert_paths_match_cpu(&mut renderer, "translucent", layers.clone());

        // The overlaps are drawn once: where the strokes cross, and where
        // the stroke covers the fill, the color is what one layer gives.
        let mut scene = empty_scene(96, 64);
        scene.layers = layers;
        let gpu = renderer.render(&scene).unwrap();
        let mut single = empty_scene(96, 64);
        single.layers = vec![path_layer(
            "plain",
            polyline(&[(4.0, 4.0), (8.0, 4.0), (8.0, 8.0), (4.0, 8.0)], true),
            Some(solid("#FF000080")),
            None,
        )];
        single.layers[0].opacity = 0.7;
        let plain = renderer.render(&single).unwrap();
        // The diagonals cross at (22, 22).
        assert_eq!(pixel_at(&gpu, 22, 22), pixel_at(&plain, 6, 6));
    }

    #[test]
    fn paints_gradients_in_local_coordinates_like_the_cpu_renderer() {
        use celesta_composition::{GradientStop, PathCommand};

        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let stops = |from: &str, to: &str| {
            vec![
                GradientStop {
                    offset: 0.0,
                    color: from.to_owned(),
                },
                GradientStop {
                    offset: 1.0,
                    color: to.to_owned(),
                },
            ]
        };
        let ring = vec![
            PathCommand::MoveTo { x: 20.0, y: 10.0 },
            PathCommand::CubicTo {
                x1: 20.0,
                y1: 16.0,
                x2: 16.0,
                y2: 20.0,
                x: 10.0,
                y: 20.0,
            },
            PathCommand::CubicTo {
                x1: 4.0,
                y1: 20.0,
                x2: 0.0,
                y2: 16.0,
                x: 0.0,
                y: 10.0,
            },
            PathCommand::CubicTo {
                x1: 0.0,
                y1: 4.0,
                x2: 4.0,
                y2: 0.0,
                x: 10.0,
                y: 0.0,
            },
            PathCommand::CubicTo {
                x1: 16.0,
                y1: 0.0,
                x2: 20.0,
                y2: 4.0,
                x: 20.0,
                y: 10.0,
            },
            PathCommand::Close,
        ];
        assert_paths_match_cpu(
            &mut renderer,
            "gradients",
            vec![
                group(
                    EvaluatedTransform {
                        position: Point { x: 4.0, y: 6.0 },
                        scale: Point { x: 2.0, y: 2.5 },
                        ..EvaluatedTransform::default()
                    },
                    vec![path_layer(
                        "linear",
                        ring.clone(),
                        Some(Paint::Linear {
                            start: Point { x: 0.0, y: 0.0 },
                            end: Point { x: 20.0, y: 20.0 },
                            stops: stops("#FF0000", "#0000FF80"),
                        }),
                        Some((
                            Paint::Radial {
                                center: Point { x: 10.0, y: 10.0 },
                                radius: 12.0,
                                stops: stops("#FFFFFF", "#00FF00"),
                            },
                            2.0,
                        )),
                    )],
                ),
                group(
                    EvaluatedTransform {
                        position: Point { x: 56.0, y: 10.0 },
                        scale: Point { x: 1.5, y: 1.5 },
                        ..EvaluatedTransform::default()
                    },
                    vec![path_layer(
                        "radial",
                        ring,
                        Some(Paint::Radial {
                            center: Point { x: 6.0, y: 6.0 },
                            radius: 16.0,
                            stops: stops("#FFE080", "#8000FF"),
                        }),
                        None,
                    )],
                ),
            ],
        );
    }

    #[test]
    fn clips_blends_and_filters_paths_like_the_cpu_renderer() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let star = |id: &str| {
            path_layer(
                id,
                polyline(
                    &[
                        (16.0, 2.0),
                        (21.0, 12.0),
                        (32.0, 13.0),
                        (24.0, 21.0),
                        (26.0, 32.0),
                        (16.0, 27.0),
                        (6.0, 32.0),
                        (8.0, 21.0),
                        (0.0, 13.0),
                        (11.0, 12.0),
                    ],
                    true,
                ),
                Some(solid("#FFCC33")),
                Some((solid("#C04020"), 1.5)),
            )
        };
        let mut multiplied = star("multiplied");
        multiplied.transform.position = Point { x: 32.0, y: 4.0 };
        multiplied.blend_mode = BlendMode::Multiply;
        let mut blurred = group(
            EvaluatedTransform {
                position: Point { x: 62.0, y: 28.0 },
                ..EvaluatedTransform::default()
            },
            vec![star("blurred")],
        );
        blurred.effects.blur = 1.5;
        assert_paths_match_cpu(
            &mut renderer,
            "clips, blends and effects",
            vec![
                corner_rect("backdrop", 30.0, 0.0, 40.0, 40.0, "#40A0FF"),
                clipped_group(
                    EvaluatedTransform {
                        position: Point { x: 2.0, y: 2.0 },
                        ..EvaluatedTransform::default()
                    },
                    Clip {
                        x: 4.0,
                        y: 4.0,
                        width: 22.0,
                        height: 20.0,
                        corner_radius: 6.0,
                    },
                    vec![star("clipped")],
                ),
                multiplied,
                blurred,
            ],
        );
    }

    /// The CPU renderer draws no rotated layers, so rotated paths are
    /// compared with its rasterizer's pixels composited by hand.
    #[test]
    fn shades_rotated_paths_like_the_cpu_rasterizer() {
        use celesta_composition::{LineCap, LineJoin, PathCommand};
        use celesta_renderer::{PathDraw, PathShape, rasterize_paths};

        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let ellipse = |rx: f64, ry: f64| {
            let k = 0.5523;
            vec![
                PathCommand::MoveTo { x: rx, y: 0.0 },
                PathCommand::CubicTo {
                    x1: rx,
                    y1: ry * k,
                    x2: rx * k,
                    y2: ry,
                    x: 0.0,
                    y: ry,
                },
                PathCommand::CubicTo {
                    x1: -rx * k,
                    y1: ry,
                    x2: -rx,
                    y2: ry * k,
                    x: -rx,
                    y: 0.0,
                },
                PathCommand::CubicTo {
                    x1: -rx,
                    y1: -ry * k,
                    x2: -rx * k,
                    y2: -ry,
                    x: 0.0,
                    y: -ry,
                },
                PathCommand::CubicTo {
                    x1: rx * k,
                    y1: -ry,
                    x2: rx,
                    y2: -ry * k,
                    x: rx,
                    y: 0.0,
                },
                PathCommand::Close,
            ]
        };
        let mut scene = empty_scene(320, 180);
        // Rings like NEBULA's, overhanging the frame on every side.
        scene.layers = (0..8)
            .map(|k| {
                let mut ring = path_layer(
                    &format!("ring-{k}"),
                    ellipse(40.0 + f64::from(k) * 22.0, 15.0 + f64::from(k) * 9.0),
                    (k == 0).then(|| solid("#20408080")),
                    Some((solid("#8FB8FF"), 1.5)),
                );
                ring.transform = EvaluatedTransform {
                    position: Point { x: 160.0, y: 90.0 },
                    rotation: f64::from(k) * 23.0 + 7.0,
                    ..EvaluatedTransform::default()
                };
                ring.opacity = 0.3 + 0.08 * f64::from(k);
                styled(ring, LineCap::Butt, LineJoin::Miter, 4.0)
            })
            .collect();
        let gpu = renderer.render(&scene).unwrap();

        let draws: Vec<_> = scene
            .layers
            .iter()
            .map(|layer| {
                let LayerContent::Path {
                    commands,
                    fill,
                    stroke,
                    line_cap,
                    line_join,
                    miter_limit,
                } = &layer.content
                else {
                    unreachable!("the scene is paths");
                };
                PathDraw {
                    shape: PathShape {
                        commands,
                        fill: fill.as_ref(),
                        stroke: stroke.as_ref(),
                        line_cap: *line_cap,
                        line_join: *line_join,
                        miter_limit: *miter_limit,
                    },
                    transform: path_transform(&layer.transform),
                    opacity: layer.opacity,
                }
            })
            .collect();
        let rasterized = rasterize_paths(&draws, scene.width, scene.height)
            .unwrap()
            .unwrap();
        let background = GpuRenderOptions::default().background;
        let mut cpu: Vec<u8> = (0..scene.width * scene.height)
            .flat_map(|_| [background.red, background.green, background.blue, 255])
            .collect();
        let image = &rasterized.image;
        for (index, texel) in image.pixels().chunks_exact(4).enumerate() {
            let x = rasterized.left + (index as u32 % image.width()) as i32;
            let y = rasterized.top + (index as u32 / image.width()) as i32;
            let pixel = &mut cpu[(y as usize * scene.width as usize + x as usize) * 4..][..3];
            let alpha = f64::from(texel[3]) / 255.0;
            for (channel, value) in pixel.iter_mut().zip(texel) {
                *channel =
                    (f64::from(*value) * alpha + f64::from(*channel) * (1.0 - alpha)).round() as u8;
            }
        }
        assert_paths_match("rotated", gpu.pixels(), &cpu);
    }

    #[test]
    fn exports_and_previews_paths_with_the_same_pixels() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let mut scene = empty_scene(96, 64);
        let mut ring = path_layer(
            "ring",
            polyline(
                &[(10.0, 10.0), (80.0, 20.0), (60.0, 56.0), (14.0, 44.0)],
                true,
            ),
            Some(solid("#3060FF80")),
            Some((solid("#FFE080"), 2.0)),
        );
        ring.opacity = 0.75;
        ring.transform.rotation = 9.0;
        scene.layers = vec![ring];
        let rendered = renderer.render(&scene).unwrap();
        // Pipelined, as an export renders.
        assert!(renderer.submit(&scene).unwrap().is_none());
        let exported = renderer.drain().unwrap();
        assert_eq!(exported[0].pixels(), rendered.pixels());
        // Onto a BGRA target, as the preview renders.
        let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Celesta preview test target"),
            size: wgpu::Extent3d {
                width: scene.width,
                height: scene.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        renderer
            .render_to_target(
                &scene,
                GpuRenderTarget {
                    view: &view,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    width: scene.width,
                    height: scene.height,
                },
            )
            .unwrap();
        let layout = ReadbackLayout::new(scene.width, scene.height).unwrap();
        let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Celesta preview test readback"),
            size: layout.buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(layout.padded_bytes_per_row),
                    rows_per_image: Some(scene.height),
                },
            },
            texture.size(),
        );
        renderer.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let mapped = buffer.slice(..).get_mapped_range().unwrap();
        let mut previewed = layout.unpad(&mapped, scene.width, scene.height).unwrap();
        for pixel in previewed.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        assert_eq!(previewed, rendered.pixels());
    }

    #[test]
    fn shades_paths_alongside_changing_filtered_and_unfiltered_images() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        for (index, rgba) in [[0, 255, 0, 255], [0, 0, 255, 255]].into_iter().enumerate() {
            let image = seeded_image(&mut renderer, &format!("image-{index}"), 1.0, rgba);
            let mut filtered = seeded_image(&mut renderer, &format!("filtered-{index}"), 6.0, rgba);
            filtered.transform.position.y = 2.0;
            filtered.transform.scale = Point { x: 2.0, y: 2.0 };
            let mut scene = empty_scene(16, 8);
            scene.layers = vec![
                image,
                filtered,
                path_layer(
                    "triangle",
                    polyline(&[(9.0, 1.0), (15.0, 1.0), (9.0, 7.0)], true),
                    Some(solid("#FF0000")),
                    None,
                ),
            ];
            let frame = renderer.render(&scene).unwrap();
            assert_eq!(
                [
                    pixel_at(&frame, 0, 0),
                    pixel_at(&frame, 6, 2),
                    pixel_at(&frame, 10, 2)
                ],
                [rgba, rgba, [255, 0, 0, 255]],
                "frame {index} must shade paths and both image sampling modes"
            );
        }
    }

    /// Many edges can cross one pixel on one scanline; the shader steps
    /// through every crossing, winding by winding.
    #[test]
    fn measures_pixels_crossed_by_many_edges_exactly() {
        use celesta_composition::PathCommand;

        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::TRANSPARENT,
        }) else {
            return;
        };
        let rect = |commands: &mut Vec<PathCommand>, left: f64, right: f64, clockwise: bool| {
            let mut corners = [(left, 4.0), (right, 4.0), (right, 28.0), (left, 28.0)];
            if !clockwise {
                corners.reverse();
            }
            commands.extend(polyline(&corners, true));
        };
        let mut commands = Vec::new();
        // Five slivers 0.06 px wide inside pixel column 10: ten crossings,
        // covering 0.3 of each pixel.
        for sliver in 0..5 {
            let left = 10.05 + f64::from(sliver) * 0.18;
            rect(&mut commands, left, left + 0.06, true);
        }
        // The same square five times each way inside column 20: twenty
        // crossings whose windings cancel.
        for _ in 0..5 {
            rect(&mut commands, 20.1, 20.9, true);
            rect(&mut commands, 20.1, 20.9, false);
        }
        let mut scene = empty_scene(32, 32);
        scene.layers = vec![path_layer("crowded", commands, Some(solid("#FFFFFF")), None)];
        let frame = renderer.render(&scene).unwrap();
        let alpha = |x| pixel_at(&frame, x, 16)[3];
        assert!(alpha(10).abs_diff(77) <= 2, "slivers cover {}", alpha(10));
        assert_eq!(alpha(20), 0);
    }

    /// An animated path changes shape every frame; what the GPU holds for
    /// paths is sized by the largest frame, not by how many were drawn.
    #[test]
    fn keeps_path_buffers_bounded_while_shapes_animate() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let frame = |frame: u32| {
            let mut scene = empty_scene(96, 64);
            let t = f64::from(frame) * 0.37;
            scene.layers = vec![path_layer(
                "wobble",
                polyline(
                    &(0..24)
                        .map(|i| {
                            let angle = f64::from(i) / 24.0 * std::f64::consts::TAU;
                            let radius = 20.0 + 8.0 * (angle * 3.0 + t).sin();
                            (48.0 + radius * angle.cos(), 32.0 + radius * angle.sin())
                        })
                        .collect::<Vec<_>>(),
                    true,
                ),
                Some(solid("#80C0FF")),
                Some((solid("#FFFFFF"), 1.0 + (t * 0.5).sin().abs())),
            )];
            scene
        };
        let mut sizes = Vec::new();
        for index in 0..200 {
            renderer.submit(&frame(index)).unwrap();
            sizes.push(renderer.paths.size());
        }
        renderer.drain().unwrap();
        let largest = renderer.path_entries.capacity() as u64 * 16;
        assert!(sizes.iter().all(|&size| size == sizes[20]), "{sizes:?}");
        assert!(sizes[20] <= (largest * 2).next_power_of_two(), "{sizes:?}");
    }

    #[test]
    fn clips_groups_that_blend_like_the_cpu_renderer() {
        let background = Color::rgba(10, 20, 30, 255);
        let Some(mut renderer) = renderer(GpuRenderOptions { background }) else {
            return;
        };
        let clip = Clip {
            x: 6.0,
            y: 6.0,
            width: 30.0,
            height: 24.0,
            corner_radius: 8.0,
        };
        let mut scene = empty_scene(48, 40);
        scene.layers = vec![
            blend_rect("light", 0.0, 0.0, 48.0, "#e0d0c0", BlendMode::Normal),
            // An isolated group blends as one layer; only its clipped part shows.
            Layer {
                blend_mode: BlendMode::Difference,
                opacity: 0.8,
                ..clipped_group(
                    EvaluatedTransform::default(),
                    clip.clone(),
                    vec![
                        blend_rect("a", 0.0, 0.0, 30.0, "#3c6382", BlendMode::Normal),
                        blend_rect("b", 20.0, 10.0, 30.0, "#f8c291c0", BlendMode::Screen),
                    ],
                )
            },
            // A group that is not isolated clips each child's own blend.
            clipped_group(
                EvaluatedTransform {
                    position: Point { x: 10.0, y: 20.0 },
                    ..EvaluatedTransform::default()
                },
                clip,
                vec![blend_rect(
                    "c",
                    0.0,
                    0.0,
                    40.0,
                    "#6ab04c",
                    BlendMode::Multiply,
                )],
            ),
        ];
        let expected = celesta_renderer::CpuRenderer::new(celesta_renderer::RenderOptions {
            background: celesta_renderer::Color::rgba(10, 20, 30, 255),
        })
        .render(&scene)
        .unwrap();
        let frame = renderer.render(&scene).unwrap();
        let difference = max_channel_difference(&frame, &expected);
        assert!(difference <= 2, "channels differ by up to {difference}");
    }

    #[test]
    fn a_clip_rotates_with_its_group() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        // Turned 90 degrees clockwise about (20, 20), the group's x axis
        // points down the canvas and its y axis points left, so the clip's
        // 20x10 rectangle lands on canvas x 10..20, y 20..40.
        let mut scene = empty_scene(40, 40);
        scene.layers = vec![clipped_group(
            EvaluatedTransform {
                position: Point { x: 20.0, y: 20.0 },
                rotation: 90.0,
                ..EvaluatedTransform::default()
            },
            Clip {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 10.0,
                corner_radius: 0.0,
            },
            vec![corner_rect("red", -40.0, -40.0, 80.0, 80.0, "#FF0000FF")],
        )];
        let frame = renderer.render(&scene).unwrap();

        assert_eq!(pixel_at(&frame, 15, 30), [255, 0, 0, 255]);
        assert_eq!(pixel_at(&frame, 10, 20), [255, 0, 0, 255]);
        assert_eq!(pixel_at(&frame, 19, 39), [255, 0, 0, 255]);
        let background = renderer.options().background;
        let background = [
            background.red,
            background.green,
            background.blue,
            background.alpha,
        ];
        for (x, y) in [(9, 30), (20, 30), (15, 19)] {
            assert_eq!(pixel_at(&frame, x, y), background, "({x}, {y}) leaked");
        }
    }

    #[test]
    fn a_frame_can_hold_more_clips_than_the_buffer_starts_with() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        // 100 one-pixel clips, one per column, each over a whole-column rect.
        let mut scene = empty_scene(100, 4);
        scene.layers = (0..100)
            .map(|column| {
                clipped_group(
                    EvaluatedTransform::default(),
                    Clip {
                        x: f64::from(column),
                        y: 0.0,
                        width: 1.0,
                        height: 2.0,
                        corner_radius: 0.0,
                    },
                    vec![corner_rect(
                        "column",
                        f64::from(column),
                        0.0,
                        1.0,
                        4.0,
                        "#FF0000FF",
                    )],
                )
            })
            .collect();
        let frame = renderer.render(&scene).unwrap();
        let background = renderer.options().background;

        for column in [0, 1, 63, 64, 65, 99] {
            assert_eq!(pixel_at(&frame, column, 0), [255, 0, 0, 255]);
            assert_eq!(pixel_at(&frame, column, 1), [255, 0, 0, 255]);
            assert_eq!(
                pixel_at(&frame, column, 3),
                [
                    background.red,
                    background.green,
                    background.blue,
                    background.alpha
                ]
            );
        }
    }

    #[test]
    fn refuses_clips_nested_deeper_than_the_shader_walks() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let clip = Clip {
            x: 0.0,
            y: 0.0,
            width: 8.0,
            height: 8.0,
            corner_radius: 0.0,
        };
        let nested = |depth: u32| {
            (0..depth).fold(
                vec![corner_rect("red", 0.0, 0.0, 8.0, 8.0, "#FF0000FF")],
                |layers, _| vec![clipped_group(EvaluatedTransform::default(), clip, layers)],
            )
        };

        let mut scene = empty_scene(8, 8);
        scene.layers = nested(MAX_CLIP_DEPTH);
        let frame = renderer.render(&scene).unwrap();
        assert_eq!(pixel_at(&frame, 4, 4), [255, 0, 0, 255]);

        scene.layers = nested(MAX_CLIP_DEPTH + 1);
        assert!(matches!(
            renderer.render(&scene),
            Err(GpuRenderError::ClipsNestedTooDeep(depth)) if depth == MAX_CLIP_DEPTH + 1
        ));
    }

    #[test]
    fn renders_a_gradient_filled_rect() {
        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::rgba(0, 0, 0, 255),
        }) else {
            return;
        };
        let mut scene = empty_scene(8, 1);
        let mut layer = solid_rect("gradient", 4.0, 8.0, 1.0, "#000000");
        layer.content = LayerContent::Rect {
            width: 8.0,
            height: 1.0,
            fill: Some(Paint::Linear {
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 8.0, y: 0.0 },
                stops: vec![
                    celesta_composition::GradientStop {
                        offset: 0.0,
                        color: "#ff0000".to_owned(),
                    },
                    celesta_composition::GradientStop {
                        offset: 1.0,
                        color: "#0000ff".to_owned(),
                    },
                ],
            }),
            stroke: None,
            corner_radius: 0.0,
        };
        scene.layers = vec![layer];
        let frame = renderer.render(&scene).unwrap();
        let pixel = |x: usize| frame.pixels()[x * 4..x * 4 + 4].to_vec();
        assert!(pixel(0)[0] > 220 && pixel(0)[2] < 40, "{:?}", pixel(0));
        assert!(pixel(7)[2] > 220 && pixel(7)[0] < 40, "{:?}", pixel(7));
    }

    fn solid_rect(id: &str, x: f64, width: f64, height: f64, color: &str) -> Layer {
        Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform {
                position: Point { x, y: height / 2.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
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
    fn svg_display_size_and_animated_scale_match_cpu() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logo.svg");
        std::fs::write(&path, r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="var(--fill, white)"/></svg>"#).unwrap();
        let mut scene = empty_scene(100, 100);
        scene.layers.push(Layer {
            id: "logo".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 50.0, y: 50.0 },
                ..Default::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Image {
                asset: ResolvedAsset {
                    id: "logo".to_owned(),
                    location: AssetLocation::File {
                        path: path.to_string_lossy().into_owned(),
                    },
                },
                width: Some(20.0),
                height: None,
                fit: None,
            },
        });
        let mut cpu = celesta_renderer::CpuRenderer::default();
        for scale in [0.8, 1.0, 2.0] {
            scene.layers[0].transform.scale = Point { x: scale, y: scale };
            let actual = renderer.render(&scene).unwrap();
            let expected = cpu.render(&scene).unwrap();
            let difference = max_channel_difference(&actual, &expected);
            assert!(difference <= 1, "scale {scale}: difference {difference}");
            assert_eq!(pixel_at(&actual, 50, 50), [255, 255, 255, 255]);
        }
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
                    blend_mode: BlendMode::Normal,
                    effects: Default::default(),
                    content: LayerContent::Text {
                        text: format!("Dialogue preview frame {index}"),
                        style: TextStyle {
                            font_size: Some(48.0),
                            ..TextStyle::default()
                        },
                        max_width: Some(1000.0),
                        baseline_anchor: false,
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
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Group {
                layers: vec![Layer {
                    id: "checker".to_owned(),
                    transform: EvaluatedTransform::default(),
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    effects: Default::default(),
                    content: LayerContent::Image {
                        width: None,
                        height: None,
                        fit: None,
                        asset: ResolvedAsset {
                            id: "checker".to_owned(),
                            location: AssetLocation::File {
                                path: "tests/assets/checker.ppm".to_owned(),
                            },
                        },
                    },
                }],
                clip: None,
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
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
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

    /// 8x4 pixels: red on the left half, blue on the right.
    fn split_pixels() -> Vec<u8> {
        (0..4)
            .flat_map(|_| {
                [[255, 0, 0, 255]; 4]
                    .into_iter()
                    .chain([[0, 0, 255, 255]; 4])
            })
            .flatten()
            .collect()
    }

    /// Drawn over the whole 8x4 frame from a texture half that size, each
    /// half keeps its color. Shrinking and enlarging blur the seam a little,
    /// and the outermost pixels soften into the transparent surroundings as
    /// any enlarged layer's do, so inner pixels are read for their hue.
    fn assert_split(frame: &GpuFrame) {
        let pixel = |x: usize, y: usize| &frame.pixels()[(y * 8 + x) * 4..][..4];
        let red = |p: &[u8]| p[0] > 200 && p[1] == 0 && p[2] < 50 && p[3] == 255;
        let blue = |p: &[u8]| p[0] < 50 && p[1] == 0 && p[2] > 200 && p[3] == 255;
        for y in 1..3 {
            for x in [1, 2] {
                assert!(red(pixel(x, y)), "({x}, {y}): {:?}", pixel(x, y));
            }
            for x in [5, 6] {
                assert!(blue(pixel(x, y)), "({x}, {y}): {:?}", pixel(x, y));
            }
        }
    }

    #[test]
    fn shrinks_images_and_video_frames_larger_than_the_texture_limit() {
        struct Decoder;

        impl VideoFrameDecoder for Decoder {
            fn decode_frame(&mut self, _: &Path, _: f64) -> Result<VideoFrame, MediaError> {
                Ok(VideoFrame {
                    width: 8,
                    height: 4,
                    pixels: split_pixels().into(),
                })
            }
        }

        let Some(renderer) = renderer(GpuRenderOptions {
            background: Color::rgba(0, 0, 0, 255),
        }) else {
            return;
        };
        let mut renderer = renderer.with_video_decoder(Decoder);
        renderer.max_texture_dimension = 4;
        let transform = EvaluatedTransform {
            position: Point { x: 4.0, y: 2.0 },
            ..EvaluatedTransform::default()
        };
        let asset = |id: &str, path: &str| ResolvedAsset {
            id: id.to_owned(),
            location: AssetLocation::File {
                path: path.to_owned(),
            },
        };

        renderer.image_sources.insert_raster(
            "split",
            image::RgbaImage::from_raw(8, 4, split_pixels()).unwrap(),
        );
        let mut scene = empty_scene(8, 4);
        scene.layers.push(Layer {
            id: "image".to_owned(),
            transform,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Image {
                width: None,
                height: None,
                fit: None,
                asset: asset("split", "split.png"),
            },
        });
        // Uploaded as is, the 8x4 image would exceed the 4-texel limit.
        assert_split(&renderer.render(&scene).unwrap());
        let texture = &renderer.textures.values().next().unwrap().texture;
        assert_eq!((texture.width, texture.height), (4, 2));

        scene.layers[0].content = LayerContent::Video {
            asset: asset("clip", "clip.mp4"),
            timing: MediaTiming {
                local_time: Time::ZERO,
                source_start: Time::ZERO,
                source_time_seconds: 0.0,
                playback_rate: 1.0,
            },
        };
        assert_split(&renderer.render(&scene).unwrap());
    }

    #[test]
    fn rejects_a_malformed_video_frame_larger_than_the_texture_limit() {
        struct Decoder;

        impl VideoFrameDecoder for Decoder {
            fn decode_frame(&mut self, _: &Path, _: f64) -> Result<VideoFrame, MediaError> {
                // One pixel short of 8x4.
                Ok(VideoFrame {
                    width: 8,
                    height: 4,
                    pixels: vec![0; 31 * 4].into(),
                })
            }
        }

        let Some(renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let mut renderer = renderer.with_video_decoder(Decoder);
        renderer.max_texture_dimension = 4;
        let mut scene = empty_scene(8, 4);
        scene.layers.push(Layer {
            id: "video".to_owned(),
            transform: EvaluatedTransform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Video {
                asset: ResolvedAsset {
                    id: "clip".to_owned(),
                    location: AssetLocation::File {
                        path: "clip.mp4".to_owned(),
                    },
                },
                timing: MediaTiming {
                    local_time: Time::ZERO,
                    source_start: Time::ZERO,
                    source_time_seconds: 0.0,
                    playback_rate: 1.0,
                },
            },
        });
        assert!(matches!(
            renderer.render(&scene),
            Err(GpuRenderError::InvalidImageData { .. })
        ));
    }

    #[test]
    fn keeps_psd_composites_of_different_levels_apart_at_the_same_size() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/lipsync-fixture.psd");
        renderer.max_texture_dimension = 80;
        let portrait = |id: &str, scale: f64| Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 120.0, y: 160.0 },
                scale: Point { x: scale, y: scale },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Psd {
                asset: ResolvedAsset {
                    id: "fixture".to_owned(),
                    location: AssetLocation::File {
                        path: fixture.to_string_lossy().into_owned(),
                    },
                },
                visible_layers: vec!["body".to_owned(), "body/base".to_owned()],
                enabled_layers: Vec::new(),
                disabled_layers: Vec::new(),
            },
        };
        // At full size the 240x320 canvas is composited at 120x160 and
        // shrunk to 60x80; at a quarter it is composited at 60x80 directly.
        let mut scene = empty_scene(240, 320);
        scene.layers = vec![portrait("full", 1.0), portrait("quarter", 0.25)];
        renderer.render(&scene).unwrap();
        assert_eq!(renderer.textures.len(), 2);
        assert!(
            renderer
                .textures
                .values()
                .all(|cached| (cached.texture.width, cached.texture.height) == (60, 80))
        );
    }

    #[test]
    fn composites_a_psd_larger_than_the_texture_limit_at_the_limit() {
        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::TRANSPARENT,
        }) else {
            return;
        };
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/lipsync-fixture.psd");
        renderer.max_texture_dimension = 100;
        let preset: Vec<String> = ["body", "body/base", "body/outfit-navy"]
            .map(str::to_owned)
            .to_vec();
        let mut scene = empty_scene(240, 320);
        scene.layers.push(Layer {
            id: "portrait".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 120.0, y: 160.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Psd {
                asset: ResolvedAsset {
                    id: "fixture".to_owned(),
                    location: AssetLocation::File {
                        path: fixture.to_string_lossy().into_owned(),
                    },
                },
                visible_layers: preset.clone(),
                enabled_layers: Vec::new(),
                disabled_layers: Vec::new(),
            },
        });
        let frame = renderer.render(&scene).unwrap();
        let texture = &renderer.textures.values().next().unwrap().texture;
        assert_eq!((texture.width, texture.height), (75, 100));

        // Drawn at the PSD's full size: the shrunk composite is enlarged
        // back over the whole canvas, so its solid areas match it.
        let full = celesta_renderer::psd_source::PsdSources::default()
            .render("fixture", &fixture, &preset, &[], &[], 1.0)
            .unwrap();
        for (x, y) in [(120, 236), (70, 190), (170, 280), (10, 10)] {
            let index = (y * 240 + x) * 4;
            assert_eq!(
                frame.pixels()[index..index + 4],
                full.pixels[index..index + 4],
                "pixel ({x}, {y})"
            );
        }
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
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
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
                baseline_anchor: false,
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
    fn lists_text_layers_whose_family_has_no_face() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let missing = |id: &str, weight| {
            let mut layer = text_layer(id, Point { x: 0.0, y: 0.0 }, 1.0, 24.0);
            if let LayerContent::Text { style, .. } = &mut layer.content {
                style.font_family = Some("Celesta Missing Family".to_owned());
                style.font_weight = weight;
            }
            layer
        };
        let mut scene = empty_scene(320, 80);
        scene.layers = vec![
            missing("title", None),
            // Same family and weight: listed once, with the first layer.
            missing("subtitle", Some(400)),
            missing("caption", Some(700)),
            text_layer("default-font", Point { x: 0.0, y: 40.0 }, 1.0, 24.0),
        ];
        renderer.render(&scene).unwrap();
        let listed = renderer
            .font_fallbacks()
            .iter()
            .map(|fallback| (fallback.layer.as_str(), fallback.weight))
            .collect::<Vec<_>>();
        assert_eq!(listed, [("title", 400), ("caption", 700)]);

        // A cached text texture still reports its fallback on later frames.
        renderer.render(&scene).unwrap();
        assert_eq!(renderer.font_fallbacks().len(), 2);

        scene.layers.clear();
        renderer.render(&scene).unwrap();
        assert!(renderer.font_fallbacks().is_empty());
    }

    #[test]
    fn lists_text_layers_with_characters_their_family_has_no_glyph_for() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        renderer.set_asset_root(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/prism"));
        let text = |id: &str, family: &str, value: &str| {
            let mut layer = text_layer(id, Point { x: 0.0, y: 0.0 }, 1.0, 24.0);
            if let LayerContent::Text { text, style, .. } = &mut layer.content {
                *text = value.to_owned();
                style.font_family = Some(family.to_owned());
            }
            layer
        };
        let mut scene = empty_scene(320, 80);
        scene.fonts = vec![ResolvedAsset {
            id: "bebas".to_owned(),
            location: AssetLocation::File {
                path: "assets/fonts/BebasNeue-Regular.ttf".to_owned(),
            },
        }];
        scene.layers = vec![
            text("title", "Bebas Neue", "CELESTA ずんだもん"),
            // The same characters again: listed once, with the first layer.
            text("subtitle", "Bebas Neue", "ずんだもん"),
            text("caption", "Bebas Neue", "めたん"),
            text("complete", "Bebas Neue", "CELESTA 2026"),
            text("emoji", "Bebas Neue", "CELESTA 🎉"),
            // A family with no face gets only the font fallback warning.
            text("missing", "Celesta Missing Family", "ずんだもん"),
        ];
        renderer.render(&scene).unwrap();
        let listed = renderer
            .missing_glyphs()
            .iter()
            .map(|missing| {
                (
                    missing.layer.as_str(),
                    missing.characters.iter().collect::<String>(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            listed,
            [
                ("title", "ずんだも".to_owned()),
                ("caption", "めたん".to_owned()),
            ]
        );
        assert_eq!(renderer.font_fallbacks().len(), 1);

        // A cached text texture still reports its missing glyphs.
        renderer.render(&scene).unwrap();
        assert_eq!(renderer.missing_glyphs().len(), 2);

        scene.layers.clear();
        renderer.render(&scene).unwrap();
        assert!(renderer.missing_glyphs().is_empty());
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
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
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
                baseline_anchor: false,
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
        // Horizontally the advance box is centered, so uneven side bearings
        // leave the ink a few pixels off.
        assert!((center_x - 640.0).abs() <= 4.0, "center x was {center_x}");
        assert!(
            (center_y - 360.0).abs() <= 0.5,
            "center y was {center_y} ({min_y}..{max_y})"
        );
    }

    #[test]
    fn places_layers_on_half_pixel_positions_like_the_cpu_renderer() {
        // Nearest sampling picks a texel per pixel centre. With the quad's
        // left edge on a half pixel, every centre lands exactly on a texel
        // boundary, and f32 rounding chose a different neighbour per column:
        // glyph stems came out notched and shifted. Full HD, because the
        // error comes from the clip-space round trip at real canvas sizes.
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let text = |x: f64, anchor_x: f64, size: f64| Layer {
            id: format!("text-{x}"),
            transform: EvaluatedTransform {
                position: Point { x, y: 300.0 },
                anchor: Point {
                    x: anchor_x,
                    y: 0.0,
                },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Text {
                text: "0 Hello, 15,000".to_owned(),
                style: TextStyle {
                    font_size: Some(size),
                    font_weight: Some(900),
                    fill: Some(Paint::Solid {
                        color: "#ffffff".to_owned(),
                    }),
                    ..TextStyle::default()
                },
                max_width: None,
                baseline_anchor: false,
            },
        };
        let layers = [
            text(400.5, 0.0, 104.0),
            text(1001.5, 0.0, 104.0),
            text(37.5, 0.0, 30.0),
            text(960.0, 0.5, 104.0),
            text(961.0, 0.5, 104.0),
            corner_rect("odd-rect", 400.5, 700.0, 9.0, 40.0, "#ffe080"),
            corner_rect("fractional-rect", 1203.5, 700.0, 10.3, 40.0, "#40c0ff"),
        ];
        for layer in layers {
            let mut scene = empty_scene(1920, 1080);
            let id = layer.id.clone();
            scene.layers = vec![layer];
            let gpu = renderer.render(&scene).unwrap();
            let cpu = celesta_renderer::CpuRenderer::default()
                .render(&scene)
                .unwrap();
            let difference = max_channel_difference(&gpu, &cpu);
            assert!(
                difference <= 1,
                "{id}: channels differ by up to {difference}"
            );
        }
    }

    fn text_layer(id: &str, position: Point, scale: f64, font_size: f64) -> Layer {
        Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform {
                position,
                scale: Point { x: scale, y: scale },
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Text {
                text: "Celesta 15,000".to_owned(),
                style: TextStyle {
                    font_size: Some(font_size),
                    fill: Some(Paint::Solid {
                        color: "#ffffff".to_owned(),
                    }),
                    ..TextStyle::default()
                },
                max_width: None,
                baseline_anchor: false,
            },
        }
    }

    /// An image layer drawing `image`, registered with `renderer` as `id`.
    fn image_layer(
        renderer: &mut GpuRenderer,
        id: &str,
        image: DecodedImage,
        transform: EvaluatedTransform,
    ) -> Layer {
        renderer.image_sources.insert_raster(
            id,
            image::RgbaImage::from_raw(image.width, image.height, image.pixels.as_ref().clone())
                .unwrap(),
        );
        Layer {
            id: id.to_owned(),
            transform,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Image {
                asset: ResolvedAsset {
                    id: id.to_owned(),
                    location: AssetLocation::File {
                        path: id.to_owned(),
                    },
                },
                width: None,
                height: None,
                fit: None,
            },
        }
    }

    fn max_frame_difference(first: &GpuFrame, second: &GpuFrame) -> u8 {
        first
            .pixels()
            .iter()
            .zip(second.pixels())
            .map(|(first, second)| first.abs_diff(*second))
            .max()
            .unwrap()
    }

    #[test]
    fn final_quality_rasterizes_scaled_text_at_its_drawn_size() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let position = Point { x: 40.0, y: 60.0 };
        let mut scene = empty_scene(640, 200);
        scene.layers = vec![text_layer("big", position, 1.0, 64.0)];
        let reference = renderer.render(&scene).unwrap();
        scene.layers = vec![text_layer("scaled", position, 2.0, 32.0)];

        assert_eq!(renderer.render_quality(), RenderQuality::Final);
        let scaled = renderer.render(&scene).unwrap();
        let difference = max_frame_difference(&scaled, &reference);
        assert!(difference <= 1, "final differs by up to {difference}");

        // Draft enlarges the scale-1 texture instead: visibly softer.
        renderer.set_render_quality(RenderQuality::Draft);
        let draft = renderer.render(&scene).unwrap();
        assert!(max_frame_difference(&draft, &reference) > 64);
    }

    #[test]
    fn filters_enlarged_images_instead_of_repeating_texels() {
        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::TRANSPARENT,
        }) else {
            return;
        };
        // A horizontal ramp. Nearest sampling at 1.28x repeats some columns,
        // which shows as stems of uneven width in enlarged artwork.
        let width = 40;
        let pixels = (0..width)
            .flat_map(|x| {
                let value = (x * 6) as u8;
                [value, value, value, 255]
            })
            .collect();
        let ramp = DecodedImage::new(width, 1, pixels).unwrap();
        let transform = EvaluatedTransform {
            position: Point { x: 4.0, y: 4.0 },
            scale: Point { x: 1.28, y: 8.0 },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        };
        let mut scene = empty_scene(64, 16);
        scene.layers = vec![image_layer(&mut renderer, "ramp", ramp, transform)];
        let frame = renderer.render(&scene).unwrap();
        // Row 8 is well inside the 8-pixel-tall layer; columns away from the
        // anti-aliased ends must rise strictly.
        let row: Vec<u8> = (6..50).map(|x| pixel_at(&frame, x, 8)[0]).collect();
        assert!(
            row.windows(2).all(|pair| pair[1] > pair[0]),
            "repeated or falling columns: {row:?}"
        );
    }

    #[test]
    fn shrinks_cached_images_through_mipmaps() {
        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::TRANSPARENT,
        }) else {
            return;
        };
        // A one-texel checkerboard averages to mid grey. Without mipmaps a
        // draw at under a quarter size picks a few texels per pixel and
        // aliases.
        let size = 128;
        let pixels = (0..size * size)
            .flat_map(|index| {
                let value = if (index % size + index / size) % 2 == 0 {
                    255
                } else {
                    0
                };
                [value, value, value, 255]
            })
            .collect();
        let checkerboard = DecodedImage::new(size, size, pixels).unwrap();
        // An uneven scale and offset, so bilinear taps do not happen to
        // straddle a black and a white texel evenly.
        let transform = EvaluatedTransform {
            position: Point { x: 0.37, y: 0.61 },
            scale: Point { x: 0.23, y: 0.23 },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        };
        let mut scene = empty_scene(32, 32);
        scene.layers = vec![image_layer(
            &mut renderer,
            "checkerboard",
            checkerboard,
            transform,
        )];
        let frame = renderer.render(&scene).unwrap();
        for y in 2..27 {
            for x in 2..27 {
                let value = pixel_at(&frame, x, y)[0];
                assert!(
                    value.abs_diff(128) <= 16,
                    "pixel ({x}, {y}) is {value}, not mid grey"
                );
            }
        }
    }

    #[test]
    fn anti_aliases_the_edges_of_rotated_images() {
        let Some(mut renderer) = renderer(GpuRenderOptions {
            background: Color::TRANSPARENT,
        }) else {
            return;
        };
        let square = DecodedImage::new(20, 20, vec![255; 20 * 20 * 4]).unwrap();
        let transform = EvaluatedTransform {
            position: Point { x: 32.0, y: 32.0 },
            rotation: 30.0,
            ..EvaluatedTransform::default()
        };
        let mut scene = empty_scene(64, 64);
        scene.layers = vec![image_layer(&mut renderer, "square", square, transform)];
        let frame = renderer.render(&scene).unwrap();
        let partial = frame
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0 && pixel[3] < 255)
            .count();
        // About the perimeter (80 pixels) of partially covered edge pixels.
        assert!(partial >= 40, "only {partial} edge pixels are partial");
    }

    #[test]
    fn rounds_text_raster_scales_up_to_eighths_of_an_octave() {
        let scaled = |scale: f32| {
            text_raster_scale(Affine {
                a: scale,
                d: scale,
                ..Affine::IDENTITY
            })
        };
        assert_eq!(scaled(1.0), 1.0);
        assert_eq!(scaled(2.0), 2.0);
        assert_eq!(scaled(0.5), 0.5);
        assert!((scaled(1.5) - 2_f32.powf(5.0 / 8.0)).abs() < 1e-6);
        assert!(scaled(1.01) >= 1.01);
        // Rotation does not change the size text is drawn at.
        let rotated = Affine::from_transform(&EvaluatedTransform {
            rotation: 33.0,
            scale: Point { x: 2.0, y: 2.0 },
            ..EvaluatedTransform::default()
        });
        assert!((text_raster_scale(rotated) - 2.0).abs() < 1e-5);
    }

    #[test]
    fn mipmaps_average_color_by_coverage() {
        // An opaque white texel next to a transparent black one: the color
        // stays white rather than greying toward the transparent texel.
        let (width, height, pixels) = downsample(2, 1, &[255, 255, 255, 255, 0, 0, 0, 0]);
        assert_eq!((width, height), (1, 1));
        assert_eq!(pixels, [255, 255, 255, 128]);
        // Odd sizes fold the last column into its neighbour.
        let (width, _, pixels) = downsample(3, 1, &[30, 30, 30, 255].repeat(3));
        assert_eq!(width, 1);
        assert_eq!(pixels, [30, 30, 30, 255]);
    }

    #[test]
    fn render_qualities_round_trip_through_their_names() {
        for quality in RenderQuality::ALL {
            assert_eq!(quality.as_str().parse::<RenderQuality>(), Ok(quality));
        }
        assert!("best".parse::<RenderQuality>().is_err());
    }

    #[test]
    fn a_stroked_text_layer_keeps_its_place_and_its_whole_stroke() {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        // Columns that differ from the black background, and the columns of
        // the white fill on its own.
        let mut extent = |stroke: Option<Stroke>| {
            let mut scene = empty_scene(640, 240);
            scene.layers.push(Layer {
                id: "title".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 320.0, y: 120.0 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Text {
                    text: "MW".to_owned(),
                    style: TextStyle {
                        font_size: Some(96.0),
                        fill: Some(Paint::Solid {
                            color: "#FFFFFFFF".to_owned(),
                        }),
                        stroke,
                        ..TextStyle::default()
                    },
                    max_width: None,
                    baseline_anchor: false,
                },
            });
            let frame = renderer.render(&scene).unwrap();
            let (mut ink, mut fill) = ((u32::MAX, 0), (u32::MAX, 0));
            for (index, pixel) in frame.pixels().chunks_exact(4).enumerate() {
                let x = index as u32 % frame.width();
                if pixel[0] > 40 || pixel[1] > 40 {
                    ink = (ink.0.min(x), ink.1.max(x));
                }
                if pixel[0] > 200 && pixel[2] > 200 {
                    fill = (fill.0.min(x), fill.1.max(x));
                }
            }
            (ink, fill)
        };
        let (plain_ink, plain_fill) = extent(None);
        let (stroked_ink, stroked_fill) = extent(Some(Stroke {
            paint: Paint::Solid {
                color: "#FF0000FF".to_owned(),
            },
            width: 16.0,
        }));
        // The fill sits where it did without a stroke...
        assert!(
            plain_fill.0.abs_diff(stroked_fill.0) <= 1,
            "{plain_fill:?} {stroked_fill:?}"
        );
        assert!(
            plain_fill.1.abs_diff(stroked_fill.1) <= 1,
            "{plain_fill:?} {stroked_fill:?}"
        );
        // ...and the stroke reaches its full width past both ends of it.
        assert!(
            plain_ink.0 - stroked_ink.0 >= 14,
            "{plain_ink:?} {stroked_ink:?}"
        );
        assert!(
            stroked_ink.1 - plain_ink.1 >= 14,
            "{plain_ink:?} {stroked_ink:?}"
        );
    }

    #[test]
    fn baseline_anchored_text_layers_share_a_baseline() {
        const BASELINE: u32 = 200;
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        let mut scene = empty_scene(400, 300);
        for (text, x, font_size) in [("x", 20.0, 32.0), ("H", 110.0, 96.0), ("o", 230.0, 64.0)] {
            scene.layers.push(Layer {
                id: text.to_owned(),
                transform: EvaluatedTransform {
                    position: Point {
                        x,
                        y: f64::from(BASELINE),
                    },
                    anchor: Point { x: 0.0, y: 0.5 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Text {
                    text: text.to_owned(),
                    style: TextStyle {
                        font_size: Some(font_size),
                        ..TextStyle::default()
                    },
                    max_width: None,
                    baseline_anchor: true,
                },
            });
        }
        let frame = renderer.render(&scene).unwrap();
        for (name, left, right) in [("x", 20, 100), ("H", 110, 220), ("o", 230, 310)] {
            let bottom = (0..frame.height())
                .rev()
                .find(|&y| {
                    (left..right).any(|x| {
                        let offset = ((y * frame.width() + x) * 4) as usize;
                        frame.pixels()[offset] > 128
                    })
                })
                .unwrap();
            assert!(
                bottom.abs_diff(BASELINE - 1) <= 2,
                "{name} ends on row {bottom}, not above the baseline at {BASELINE}"
            );
        }
    }
}
