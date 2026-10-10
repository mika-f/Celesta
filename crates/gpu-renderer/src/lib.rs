//! GPU rendering foundation built on `wgpu`.
//!
//! It supports offscreen image, video, and text composition with nested
//! transforms, opacity, painter ordering, and deterministic RGBA readback.
//! Unsupported content returns an error instead of silently disappearing.

mod bounds;
mod compositor;
mod draw;
mod effect;
mod encode;
mod error;
mod layer;
mod mask;
mod path;
mod pipeline;
mod plan;
mod prepare;
mod readback;
mod renderer;
mod shader;
#[cfg(test)]
mod tests;
mod text;
mod texture;
mod transform;
mod types;
mod yuv;

pub use error::GpuRenderError;
pub use renderer::GpuRenderer;
pub use transform::path_transform;
#[cfg(target_os = "macos")]
pub use types::NativePreviewFrame;
pub use types::{
    Color, GpuDriver, GpuFrame, GpuRenderOptions, GpuRenderTarget, PreviewFrame,
    PreviewFrameStatus, PreviewViewport, ReadbackFormat, RenderQuality, UnknownGpuDriver,
    UnknownRenderQuality,
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
