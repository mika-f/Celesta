//! Where rendered stills go when the caller names no output, and how they
//! come back as image content.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use celesta_exporter::{ExportedFile, StillFormat};
use celesta_renderer::resample::{Coverage, box_downscale};
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, ImageFormat, ImageReader};
use rmcp::model::ContentBlock;

/// Most frames one `render_frames` call returns as images; the rest are
/// listed in the report only.
pub const MAX_INLINE_FRAMES: usize = 6;

/// Longest side of an image returned inline; larger images are sent
/// downscaled as PNG (the file keeps its size and format), so clients'
/// per-image limits are not exceeded.
pub const MAX_INLINE_EDGE: u32 = 1568;

/// A temporary folder, removed when the server exits, with a new subfolder
/// for each call so repeated checks never collide.
pub struct OutputDirs {
    root: tempfile::TempDir,
    next: AtomicU64,
}

impl OutputDirs {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            root: tempfile::Builder::new().prefix("celesta-mcp-").tempdir()?,
            next: AtomicU64::new(1),
        })
    }

    pub fn fresh(&self) -> io::Result<PathBuf> {
        let dir = self
            .root
            .path()
            .join(format!("{:04}", self.next.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&dir)?;
        Ok(dir)
    }
}

/// The written stills as image content, each after a line naming it: the
/// first [`MAX_INLINE_FRAMES`] frames, or a contact sheet.
pub fn image_blocks(files: &[ExportedFile]) -> io::Result<Vec<ContentBlock>> {
    let mut blocks = Vec::new();
    let mut frames = 0;
    for file in files {
        let (label, path) = match file {
            ExportedFile::Frame { path, frame } => {
                if frames == MAX_INLINE_FRAMES {
                    continue;
                }
                frames += 1;
                (format!("frame {frame}: {}", path.display()), path)
            }
            ExportedFile::ContactSheet { path, frames, .. } => (
                format!("contact sheet of frames {frames:?}: {}", path.display()),
                path,
            ),
            ExportedFile::Video { .. } | ExportedFile::Subtitles { .. } => continue,
        };
        let image = inline_image(path)?;
        let label = match image.shown {
            Some((width, height)) => format!("{label} (shown at {width}×{height})"),
            None => label,
        };
        blocks.push(ContentBlock::text(label));
        blocks.push(ContentBlock::image(
            base64::engine::general_purpose::STANDARD.encode(image.bytes),
            image.mime_type,
        ));
    }
    Ok(blocks)
}

/// An image to send inline.
struct InlineImage {
    bytes: Vec<u8>,
    mime_type: &'static str,
    /// The size it shows at when it was downscaled.
    shown: Option<(u32, u32)>,
}

/// The image at `path` as written, in the format its extension names; or,
/// when a side exceeds [`MAX_INLINE_EDGE`], a downscaled PNG of it.
fn inline_image(path: &Path) -> io::Result<InlineImage> {
    let (format, mime_type) = match StillFormat::from_extension(path) {
        Some(StillFormat::Png) => (ImageFormat::Png, "image/png"),
        Some(StillFormat::Jpeg) => (ImageFormat::Jpeg, "image/jpeg"),
        Some(StillFormat::Webp) => (ImageFormat::WebP, "image/webp"),
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is not a PNG, JPEG or WebP image", path.display()),
            ));
        }
    };
    let bytes = std::fs::read(path)?;
    let invalid = |error: image::ImageError| io::Error::new(io::ErrorKind::InvalidData, error);
    let reader = || ImageReader::with_format(io::Cursor::new(&bytes), format);
    let (width, height) = reader().into_dimensions().map_err(invalid)?;
    if width.max(height) <= MAX_INLINE_EDGE {
        return Ok(InlineImage {
            bytes,
            mime_type,
            shown: None,
        });
    }
    let pixels = reader().decode().map_err(invalid)?.into_rgba8().into_raw();
    let longest = width.max(height);
    let scaled = |side: u32| {
        ((u64::from(side) * u64::from(MAX_INLINE_EDGE) + u64::from(longest) / 2)
            / u64::from(longest))
        .max(1) as u32
    };
    let (small_width, small_height) = (scaled(width), scaled(height));
    // Straight averages, as before the box filter was shared: frames are
    // normally opaque, where this matches `Coverage::Alpha` byte for byte.
    let small = box_downscale(
        width,
        height,
        &pixels,
        small_width,
        small_height,
        Coverage::Straight,
    );
    let mut encoded = Vec::new();
    PngEncoder::new(&mut encoded)
        .write_image(&small, small_width, small_height, ExtendedColorType::Rgba8)
        .map_err(io::Error::other)?;
    Ok(InlineImage {
        bytes: encoded,
        mime_type: "image/png",
        shown: Some((small_width, small_height)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &std::path::Path, width: u32, height: u32) {
        image::RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([((y * width + x) % 256) as u8, 0x40, 0x80, 0xff])
        })
        .save(path)
        .unwrap();
    }

    /// A JPEG or WebP (by `path`'s extension) of the same pattern as
    /// [`write_png`]; JPEG drops the alpha channel.
    fn write_lossy(path: &std::path::Path, width: u32, height: u32) {
        let image = image::RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([((y * width + x) % 256) as u8, 0x40, 0x80, 0xff])
        });
        let image = image::DynamicImage::ImageRgba8(image);
        match StillFormat::from_extension(path) {
            Some(StillFormat::Jpeg) => image.to_rgb8().save(path).unwrap(),
            _ => image.save(path).unwrap(),
        }
    }

    fn decoded_size(block: &ContentBlock) -> (u32, u32) {
        assert_eq!(block.as_image().unwrap().mime_type, "image/png");
        let image = block.as_image().expect("an image block");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&image.data)
            .unwrap();
        ImageReader::with_format(std::io::Cursor::new(bytes), ImageFormat::Png)
            .into_dimensions()
            .unwrap()
    }

    #[test]
    fn large_frames_are_downscaled_inline_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frame.png");
        write_png(&path, 3200, 1800);
        let blocks = image_blocks(&[ExportedFile::Frame {
            path: path.clone(),
            frame: 0,
        }])
        .unwrap();
        let (width, height) = decoded_size(&blocks[1]);
        assert!(
            width <= MAX_INLINE_EDGE && height <= MAX_INLINE_EDGE,
            "{width}×{height}"
        );
        assert_eq!((width, height), (1568, 882));
        assert_eq!(image::image_dimensions(&path).unwrap(), (3200, 1800));
    }

    #[test]
    fn full_hd_frames_fit_the_inline_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frame.png");
        write_png(&path, 1080, 1920);
        let blocks = image_blocks(&[ExportedFile::Frame { path, frame: 0 }]).unwrap();
        assert_eq!(decoded_size(&blocks[1]), (882, 1568));
        assert!(
            blocks[0]
                .as_text()
                .unwrap()
                .text
                .ends_with("(shown at 882×1568)")
        );
    }

    #[test]
    fn small_frames_are_sent_as_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frame.png");
        write_png(&path, 1280, 720);
        let written = std::fs::read(&path).unwrap();
        let blocks = image_blocks(&[ExportedFile::Frame { path, frame: 0 }]).unwrap();
        let image = blocks[1].as_image().unwrap();
        assert_eq!(image.mime_type, "image/png");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&image.data)
                .unwrap(),
            written
        );
    }

    #[test]
    fn small_jpeg_and_webp_are_sent_as_written_with_their_type() {
        let dir = tempfile::tempdir().unwrap();
        for (name, mime_type) in [
            ("frame.jpeg", "image/jpeg"),
            ("frame.JPG", "image/jpeg"),
            ("frame.webp", "image/webp"),
        ] {
            let path = dir.path().join(name);
            write_lossy(&path, 160, 90);
            let written = std::fs::read(&path).unwrap();
            let blocks = image_blocks(&[ExportedFile::Frame { path, frame: 0 }]).unwrap();
            assert!(blocks[0].as_text().unwrap().text.ends_with(name));
            let image = blocks[1].as_image().unwrap();
            assert_eq!(image.mime_type, mime_type, "{name}");
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(&image.data)
                    .unwrap(),
                written,
                "{name}"
            );
        }
    }

    #[test]
    fn large_jpeg_and_webp_are_downscaled_to_png_inline() {
        let dir = tempfile::tempdir().unwrap();
        for (name, size, shown) in [
            ("sheet.jpg", (3200, 1800), (1568, 882)),
            ("sheet.webp", (2000, 100), (1568, 78)),
        ] {
            let path = dir.path().join(name);
            write_lossy(&path, size.0, size.1);
            let blocks = image_blocks(&[ExportedFile::ContactSheet {
                path: path.clone(),
                frames: vec![0],
                columns: 1,
            }])
            .unwrap();
            assert_eq!(decoded_size(&blocks[1]), shown, "{name}");
            assert!(
                blocks[0]
                    .as_text()
                    .unwrap()
                    .text
                    .ends_with(&format!("(shown at {}×{})", shown.0, shown.1))
            );
            let on_disk = image::image_dimensions(&path).unwrap();
            assert_eq!(on_disk, size, "{name}");
        }
    }

    #[test]
    fn other_extensions_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frame.gif");
        std::fs::write(&path, b"GIF89a").unwrap();
        let error = image_blocks(&[ExportedFile::Frame { path, frame: 0 }]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
