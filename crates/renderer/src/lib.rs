//! Deterministic CPU reference renderer.
//!
//! This backend exists to fix scene semantics before the GPU renderer arrives.
//! Video and unresolved components remain diagnostic placeholders; images and text use
//! real decoders, font shaping, and glyph rasterization.

mod assets;
mod clip;
mod composite;
mod effects;
mod error;
#[cfg(test)]
mod font_tests;
mod fonts;
mod images;
mod layer;
mod linebreak;
#[cfg(test)]
mod mask_tests;
mod paint;
mod rect;
mod renderer;
mod shaping;
#[cfg(test)]
mod tests;
mod text;
mod types;

pub use assets::rasterize_psd;
pub use error::RenderError;
pub use fonts::TextRasterizer;
pub use paint::{GradientStop, ResolvedPaint};
pub use rect::{RectPaint, rasterize_rect, rasterize_rect_transformed, resolve_rect_paint};
pub use renderer::CpuRenderer;
pub use text::{FontFallback, GlyphMetrics, MissingGlyphs, RasterizedText, TextMetrics};
pub use types::{Color, RenderOptions, RgbaFrame};

pub mod image_source;
mod path;
pub mod psd_source;

pub use path::{
    FlattenedPath, LineSegment, PathDraw, PathShape, PathTransform, RasterizedPath, flatten_path,
    rasterize_path, rasterize_paths,
};
