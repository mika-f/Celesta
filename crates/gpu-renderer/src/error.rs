use crate::transform::MAX_CLIP_DEPTH;
use crate::types::{GpuDriver, ReadbackFormat};
use celesta_media::MediaError;
use celesta_remote::RemoteAssetError;
use celesta_renderer::RenderError;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum GpuRenderError {
    /// This build cannot render through the driver on this platform.
    #[error("the {0} driver is not available on this platform")]
    DriverUnavailable(GpuDriver),
    #[error("could not select a GPU: {0}")]
    RequestAdapter(#[source] wgpu::RequestAdapterError),
    #[error("could not open the GPU: {0}")]
    RequestDevice(#[source] wgpu::RequestDeviceError),
    #[error("could not wait for GPU work: {0}")]
    Poll(#[source] wgpu::PollError),
    #[error("could not read the GPU frame: {0}")]
    Map(#[source] wgpu::BufferAsyncError),
    #[error("could not access the mapped GPU frame: {0}")]
    MapRange(#[source] wgpu::MapRangeError),
    #[error("GPU readback callback was dropped")]
    MapCallbackDropped,
    /// The thread that reads pipelined frames back could not be started.
    #[error("could not start the GPU readback thread: {0}")]
    ReadbackThread(#[source] io::Error),
    #[error("could not read GPU asset {asset}: {source}")]
    AssetIo { asset: String, source: io::Error },
    #[error("could not decode GPU image {asset}: {source}")]
    ImageDecode {
        asset: String,
        source: image::ImageError,
    },
    #[error("could not rasterize GPU PSD: {0}")]
    Psd(#[source] RenderError),
    #[error("could not decode GPU video frame: {0}")]
    Media(#[from] MediaError),
    #[error("could not rasterize GPU text: {0}")]
    Text(#[source] RenderError),
    #[error("invalid GPU layer effect: {0}")]
    Effects(#[source] RenderError),
    /// A custom shader that does not compile, or a layer's use of one that
    /// does not match it. `shader` is its name, or its id when unnamed.
    #[error("invalid custom shader {shader}: {message}")]
    Shader { shader: String, message: String },
    /// A layer's built-in filter with a value the renderer cannot use.
    /// `filter` is its type, such as `brightness` or `chromaKey`.
    #[error("invalid {filter} filter: {message}")]
    BuiltinFilter {
        filter: &'static str,
        message: String,
    },
    #[error("invalid {width}x{height} RGBA image: expected {expected} bytes, got {actual}")]
    InvalidImageData {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    #[error("could not load remote asset `{asset}`: {source}")]
    RemoteAsset {
        asset: String,
        source: RemoteAssetError,
    },
    #[error("GPU video layer {0} requires a video decoder")]
    MissingVideoDecoder(String),
    #[error("GPU layer {layer} uses unsupported {content} content")]
    UnsupportedContent {
        layer: String,
        content: &'static str,
    },
    #[error("GPU frame size must be non-zero, got {width}x{height}")]
    InvalidSurfaceSize { width: u32, height: u32 },
    #[error("GPU target size must be non-zero, got {width}x{height}")]
    InvalidTargetSize { width: u32, height: u32 },
    #[error("GPU adapter is incompatible with the preview surface")]
    IncompatibleSurface,
    #[error("preview surface has not been configured")]
    SurfaceNotConfigured,
    #[error("preview surface reported a validation error")]
    SurfaceValidation,
    #[error("GPU frame is too large: {width}x{height}")]
    SurfaceTooLarge { width: u32, height: u32 },
    #[error("could not create native preview surface ({0})")]
    NativePreview(i32),
    /// The GPU cannot produce frames in this [`ReadbackFormat`].
    #[error("this GPU cannot read frames back as {0:?}")]
    UnsupportedReadbackFormat(ReadbackFormat),
    /// [`ReadbackFormat::Yuv420p`] and [`ReadbackFormat::Yuv420p10`]
    /// subsample chroma 2x2, so they need even dimensions.
    #[error("yuv420p readback requires even dimensions, got {width}x{height}")]
    OddYuv420pSize { width: u32, height: u32 },
    /// A frame has more layers than one GPU buffer can hold.
    #[error("frame has too many layers for the GPU: {0}")]
    TooManyLayers(usize),
    /// Clipped groups are nested deeper than the shader walks.
    #[error(
        "clipped groups are nested {0} deep, more than the GPU renderer's limit of {limit}",
        limit = MAX_CLIP_DEPTH
    )]
    ClipsNestedTooDeep(u32),
    /// A frame's paths have more edges than one GPU buffer can hold.
    #[error("frame's paths exceed the GPU storage buffer: {0} tile and edge entries")]
    PathsTooComplex(usize),
}

impl GpuRenderError {
    /// Whether the error comes from the scene being drawn (an asset, text,
    /// a shader or filter, too many layers or paths) rather than from the
    /// GPU, its readback, or the surface. After a scene error, the next
    /// scene can still be rendered; after any other, it would most likely
    /// fail the same way.
    pub fn is_scene_error(&self) -> bool {
        match self {
            Self::AssetIo { .. }
            | Self::ImageDecode { .. }
            | Self::Psd(_)
            | Self::Media(_)
            | Self::Text(_)
            | Self::Effects(_)
            | Self::Shader { .. }
            | Self::BuiltinFilter { .. }
            | Self::InvalidImageData { .. }
            | Self::RemoteAsset { .. }
            | Self::MissingVideoDecoder(_)
            | Self::UnsupportedContent { .. }
            | Self::TooManyLayers(_)
            | Self::ClipsNestedTooDeep(_)
            | Self::PathsTooComplex(_) => true,
            Self::DriverUnavailable(_)
            | Self::RequestAdapter(_)
            | Self::RequestDevice(_)
            | Self::Poll(_)
            | Self::Map(_)
            | Self::MapRange(_)
            | Self::MapCallbackDropped
            | Self::ReadbackThread(_)
            | Self::InvalidSurfaceSize { .. }
            | Self::InvalidTargetSize { .. }
            | Self::IncompatibleSurface
            | Self::SurfaceNotConfigured
            | Self::SurfaceValidation
            | Self::SurfaceTooLarge { .. }
            | Self::NativePreview(_)
            | Self::UnsupportedReadbackFormat(_)
            | Self::OddYuv420pSize { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_errors_are_told_apart_from_gpu_failures() {
        let color = RenderError::InvalidColor("ink".to_owned());
        assert!(GpuRenderError::Text(color).is_scene_error());
        assert!(GpuRenderError::PathsTooComplex(1 << 24).is_scene_error());
        assert!(!GpuRenderError::Poll(wgpu::PollError::Timeout).is_scene_error());
        assert!(!GpuRenderError::MapCallbackDropped.is_scene_error());
        assert!(!GpuRenderError::ReadbackThread(io::Error::other("no threads")).is_scene_error());
    }
}
