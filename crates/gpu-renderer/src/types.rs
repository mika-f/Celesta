use crate::error::GpuRenderError;
use strum::VariantArray;

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

    pub(crate) fn as_wgpu(self) -> wgpu::Color {
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
    pub driver: GpuDriver,
}

impl Default for GpuRenderOptions {
    fn default() -> Self {
        Self {
            background: Color::rgba(20, 22, 28, 255),
            driver: GpuDriver::default(),
        }
    }
}

/// The graphics API `GpuRenderer::new` renders through.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    strum::Display,
    strum::EnumString,
    strum::IntoStaticStr,
    strum::VariantArray,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[strum(
    serialize_all = "lowercase",
    parse_err_ty = UnknownGpuDriver,
    parse_err_fn = UnknownGpuDriver::new
)]
pub enum GpuDriver {
    /// The first API with a GPU, in wgpu's order: Vulkan, Metal, DX12, then
    /// OpenGL.
    #[default]
    Auto,
    Vulkan,
    /// Direct3D 12, Windows only.
    Dx12,
    /// macOS only.
    Metal,
    /// OpenGL (ES), through WGL on Windows and EGL elsewhere.
    Gl,
}

impl GpuDriver {
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    pub(crate) fn backends(self) -> wgpu::Backends {
        match self {
            Self::Auto => wgpu::Backends::all(),
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::Dx12 => wgpu::Backends::DX12,
            Self::Metal => wgpu::Backends::METAL,
            Self::Gl => wgpu::Backends::GL,
        }
    }

    /// Whether this build of Celesta can render through this API on this
    /// platform. It may still find no GPU that supports it.
    pub fn is_built_in(self) -> bool {
        wgpu::Instance::enabled_backend_features().intersects(self.backends())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown driver '{0}' (expected {expected})", expected = names(GpuDriver::VARIANTS))]
pub struct UnknownGpuDriver(pub(crate) String);

impl UnknownGpuDriver {
    fn new(name: &str) -> Self {
        Self(name.to_owned())
    }
}

/// The pixel layout `GpuRenderer::submit`/`drain` read frames back in.
///
/// Either way the frame is composited finer than 8 bits where the GPU can
/// (half floats) and rounded to 8-bit code values once, with an 8x8 ordered
/// dither that depends on the pixel's position only, so a gentle gradient
/// does not band and the same frame always reads back the same. A value on a
/// whole code, such as a solid fill's RGBA, is kept exactly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ReadbackFormat {
    /// Tightly packed RGBA8 rows, 4 bytes per pixel.
    #[default]
    Rgba8,
    /// Tightly packed RGBA rows of 16-bit little-endian codes (FFmpeg's
    /// `rgba64le`), 8 bytes per pixel: the composited frame without
    /// rounding it to 8 bits, for an encoder that converts it to more than
    /// 8 bits itself. Not dithered; 16 bits are finer than the half floats
    /// it is composited in.
    Rgba16,
    /// [`Rgba16`](Self::Rgba16) with straight alpha, for an encoder that
    /// expects it (ProRes 4444, VP9 with alpha): the color divided by
    /// alpha, and `(0, 0, 0, 0)` where nothing is drawn. The division is
    /// done on the GPU at 16 bits, so a translucent edge keeps its color.
    Rgba16Straight,
    /// Planar I420 converted on the GPU: the full-size Y plane followed by
    /// the quarter-size U and V planes, 1.5 bytes per pixel. BT.601 limited
    /// range with 2x2-averaged chroma, the same matrix libswscale applies to
    /// untagged RGB input. Both dimensions must be even. A solid color whose
    /// Y, U or V falls between two codes alternates between them, within a
    /// code of libswscale's value.
    Yuv420p,
    /// [`Yuv420p`](Self::Yuv420p) in 10-bit codes, each a little-endian
    /// 16-bit sample (FFmpeg's `yuv420p10le`), 3 bytes per pixel: the 8-bit
    /// matrix scaled by 4, so Y spans 64-940.
    Yuv420p10,
}

impl ReadbackFormat {
    /// Whether frames come back as Y, U, and V planes, which needs even
    /// dimensions.
    pub const fn is_yuv(self) -> bool {
        matches!(self, Self::Yuv420p | Self::Yuv420p10)
    }
}

/// How much work the renderer spends on layers that are scaled or rotated.
///
/// Untransformed layers look the same in both: they are placed on whole
/// pixels and copied texel for texel. Transformed layers are always filtered
/// (bilinear, trilinear from mipmaps for cached images). The modes differ only
/// in the work that is too slow to repeat for every frame of playback.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    strum::Display,
    strum::EnumString,
    strum::IntoStaticStr,
    strum::VariantArray,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[strum(
    serialize_all = "lowercase",
    parse_err_ty = UnknownRenderQuality,
    parse_err_fn = UnknownRenderQuality::new
)]
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
    pub fn as_str(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown render quality '{0}' (expected draft or final)")]
pub struct UnknownRenderQuality(pub(crate) String);

impl UnknownRenderQuality {
    fn new(name: &str) -> Self {
        Self(name.to_owned())
    }
}

/// `names`, separated by commas.
fn names(values: &[impl Copy + Into<&'static str>]) -> String {
    let names: Vec<&str> = values.iter().map(|&value| value.into()).collect();
    names.join(", ")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: ReadbackFormat,
    pub(crate) pixels: Vec<u8>,
}

pub enum PreviewFrame {
    Cpu(GpuFrame),
    #[cfg(target_os = "macos")]
    Native(NativePreviewFrame),
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePreviewFrame(pub(crate) core_video::pixel_buffer::CVPixelBuffer);

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
