//! Shared raster and SVG decoding and display-box layout for both renderers.
use celesta_composition::ImageFit;
use image::{ImageReader, RgbaImage};
use resvg::{tiny_skia, usvg};
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::Arc,
};

#[derive(Default)]
pub struct ImageSources {
    sources: HashMap<String, Source>,
    rendered: HashMap<String, (String, DisplayImage)>,
}
enum Source {
    Raster(Arc<RgbaImage>),
    Svg(Box<usvg::Tree>),
}

#[derive(Clone)]
pub struct DisplayImage {
    pub pixels: Arc<RgbaImage>,
    pub width: f64,
    pub height: f64,
    /// SVG pixels are rasterized at the requested display density.
    pub is_svg: bool,
}

impl ImageSources {
    /// Adds decoded pixels, allowing callers to share an existing asset cache.
    pub fn insert_raster(&mut self, key: &str, pixels: RgbaImage) {
        self.sources
            .entry(key.to_owned())
            .or_insert(Source::Raster(Arc::new(pixels)));
    }

    pub fn render(
        &mut self,
        key: &str,
        path: &Path,
        width: Option<f64>,
        height: Option<f64>,
        fit: Option<ImageFit>,
        density: f64,
    ) -> Result<DisplayImage, image::ImageError> {
        if !self.sources.contains_key(key) {
            let data = std::fs::read(path)?;
            let svg = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
                || std::str::from_utf8(&data).is_ok_and(|s| s.contains("<svg"));
            let source = if svg {
                let text = std::str::from_utf8(&data).map_err(invalid)?;
                let text = css_fallbacks(text);
                let mut options = usvg::Options {
                    resources_dir: path.parent().map(Path::to_path_buf),
                    ..Default::default()
                };
                options.fontdb_mut().load_system_fonts();
                Source::Svg(Box::new(
                    usvg::Tree::from_str(&text, &options).map_err(invalid)?,
                ))
            } else {
                Source::Raster(Arc::new(
                    ImageReader::new(std::io::Cursor::new(data))
                        .with_guessed_format()?
                        .decode()?
                        .to_rgba8(),
                ))
            };
            self.sources.insert(key.to_owned(), source);
        }
        let source = &self.sources[key];
        let (sw, sh) = match source {
            Source::Raster(image) => (image.width() as f64, image.height() as f64),
            Source::Svg(tree) => (tree.size().width() as f64, tree.size().height() as f64),
        };
        let (w, h) = match (width, height) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, w * sh / sw),
            (None, Some(h)) => (h * sw / sh, h),
            (None, None) => (sw, sh),
        };
        if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
            return Err(invalid("image dimensions must be finite and positive"));
        }
        if fit.is_none()
            && let Source::Raster(pixels) = source
        {
            return Ok(DisplayImage {
                pixels: pixels.clone(),
                width: w,
                height: h,
                is_svg: false,
            });
        }
        // SVG rasterization follows the
        // complete parent transform, including animated scale and rotation.
        let density = match source {
            Source::Svg(_) => density.max(1.0),
            Source::Raster(_) => 1.0,
        };
        let pw = (w * density).ceil();
        let ph = (h * density).ceil();
        if !pw.is_finite()
            || !ph.is_finite()
            || pw > 16384.0
            || ph > 16384.0
            || pw * ph > 64_000_000.0
        {
            return Err(invalid("image display size exceeds rasterization limits"));
        }
        let (pw, ph) = (pw as u32, ph as u32);
        let signature = format!("{w}:{h}:{pw}:{ph}:{fit:?}");
        if let Some((previous, image)) = self.rendered.get(key)
            && previous == &signature
        {
            return Ok(image.clone());
        }
        let (sx, sy) = (pw as f64 / sw, ph as f64 / sh);
        let (sx, sy) = match fit {
            Some(ImageFit::Contain) => (sx.min(sy), sx.min(sy)),
            Some(ImageFit::Cover) => (sx.max(sy), sx.max(sy)),
            None => (sx, sy),
        };
        let (ox, oy) = ((pw as f64 - sw * sx) / 2.0, (ph as f64 - sh * sy) / 2.0);
        let pixels = match source {
            Source::Svg(tree) => {
                let mut pixmap = tiny_skia::Pixmap::new(pw, ph)
                    .ok_or_else(|| invalid("invalid SVG raster size"))?;
                resvg::render(
                    tree,
                    tiny_skia::Transform::from_row(
                        sx as f32, 0.0, 0.0, sy as f32, ox as f32, oy as f32,
                    ),
                    &mut pixmap.as_mut(),
                );
                // tiny-skia uses premultiplied alpha; Celesta stores straight RGBA.
                for pixel in pixmap.data_mut().chunks_exact_mut(4) {
                    if pixel[3] != 0 {
                        for channel in 0..3 {
                            pixel[channel] = ((pixel[channel] as u32 * 255 + pixel[3] as u32 / 2)
                                / pixel[3] as u32)
                                .min(255) as u8;
                        }
                    }
                }
                RgbaImage::from_raw(pw, ph, pixmap.take())
                    .ok_or_else(|| invalid("invalid SVG pixels"))?
            }
            Source::Raster(image) => {
                let mut output = RgbaImage::new(pw, ph);
                for (x, y, pixel) in output.enumerate_pixels_mut() {
                    let x = (x as f64 + 0.5 - ox) / sx;
                    let y = (y as f64 + 0.5 - oy) / sy;
                    if x >= 0.0 && y >= 0.0 && x < sw && y < sh {
                        *pixel = *image.get_pixel(x as u32, y as u32);
                    }
                }
                output
            }
        };
        let image = DisplayImage {
            pixels: Arc::new(pixels),
            width: w,
            height: h,
            is_svg: matches!(source, Source::Svg(_)),
        };
        // Retain only the latest resolution for each source during animation.
        self.rendered
            .insert(key.to_owned(), (signature, image.clone()));
        Ok(image)
    }
}

/// The size `width` x `height` shrinks to, keeping its aspect ratio, so that
/// neither side exceeds `max` (a GPU's largest texture). Sizes that already
/// fit are returned unchanged.
pub fn fit_within(width: u32, height: u32, max: u32) -> (u32, u32) {
    let largest = width.max(height);
    if largest <= max {
        return (width, height);
    }
    let scale = f64::from(max) / f64::from(largest);
    let fit = |side: u32| ((f64::from(side) * scale).round() as u32).clamp(1, max);
    (fit(width), fit(height))
}

/// Resamples straight RGBA `pixels` (`width` x `height`, which they must
/// match) to `to_width` x `to_height` with a triangle filter. Colors are
/// weighted by alpha so transparent texels do not darken the edges they are
/// averaged into, and are kept in `f32` until the end so translucent texels
/// keep their hue.
///
/// Rows are resampled across first and kept only while the output rows
/// being filled read them, so a large source needs no full-size buffer
/// besides the result.
pub fn resize_rgba(
    width: u32,
    height: u32,
    pixels: &[u8],
    to_width: u32,
    to_height: u32,
) -> RgbaImage {
    assert_eq!(
        pixels.len(),
        width as usize * height as usize * 4,
        "pixels match their size"
    );
    let columns = triangle_taps(width, to_width);
    let mut rows: VecDeque<(usize, Vec<[f32; 4]>)> = VecDeque::new();
    let mut next_row = 0;
    let mut output = Vec::with_capacity(to_width as usize * to_height as usize * 4);
    for (first, weights) in triangle_taps(height, to_height) {
        while rows.front().is_some_and(|(row, _)| *row < first) {
            rows.pop_front();
        }
        next_row = next_row.max(first);
        while next_row < first + weights.len() {
            let source = &pixels[next_row * width as usize * 4..][..width as usize * 4];
            let row = columns
                .iter()
                .map(|(first, weights)| {
                    let mut sum = [0.0f32; 4];
                    for (pixel, weight) in source[first * 4..].chunks_exact(4).zip(weights) {
                        let alpha = f32::from(pixel[3]) * weight;
                        for channel in 0..3 {
                            sum[channel] += f32::from(pixel[channel]) * alpha;
                        }
                        sum[3] += alpha;
                    }
                    sum
                })
                .collect();
            rows.push_back((next_row, row));
            next_row += 1;
        }
        let offset = first - rows.front().expect("the rows read are kept").0;
        for column in 0..to_width as usize {
            let mut sum = [0.0f32; 4];
            for ((_, row), weight) in rows.iter().skip(offset).zip(&weights) {
                for channel in 0..4 {
                    sum[channel] += row[column][channel] * weight;
                }
            }
            let alpha = sum[3];
            if alpha <= 0.0 {
                output.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            for channel in &sum[..3] {
                output.push((channel / alpha).round().clamp(0.0, 255.0) as u8);
            }
            output.push(alpha.round().clamp(0.0, 255.0) as u8);
        }
    }
    RgbaImage::from_raw(to_width, to_height, output).expect("output matches its size")
}

/// For each of `to` output pixels along an axis of `from` source pixels,
/// the first source pixel it reads and the normalized triangle weights of
/// it and the ones after it. Shrinking widens the triangle to cover every
/// source pixel; enlarging interpolates between neighbors.
fn triangle_taps(from: u32, to: u32) -> Vec<(usize, Vec<f32>)> {
    let scale = f64::from(from) / f64::from(to);
    let support = scale.max(1.0);
    (0..to)
        .map(|index| {
            let center = (f64::from(index) + 0.5) * scale - 0.5;
            let first = ((center - support).floor() + 1.0).max(0.0) as usize;
            let last = ((center + support).ceil() - 1.0).min(f64::from(from) - 1.0) as usize;
            let mut weights: Vec<f32> = (first..=last.max(first))
                .map(|source| (1.0 - (source as f64 - center).abs() / support).max(0.0) as f32)
                .collect();
            let total: f32 = weights.iter().sum();
            for weight in &mut weights {
                *weight /= total;
            }
            (first, weights)
        })
        .collect()
}

fn invalid(error: impl std::fmt::Display) -> image::ImageError {
    image::ImageError::IoError(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        error.to_string(),
    ))
}

// Resolve design-tool var(--name, fallback) expressions, including nested
// functions. The original SVG's background and other geometry are preserved.
fn css_fallbacks(input: &str) -> String {
    let mut output = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("var(") {
        output.push_str(&rest[..start]);
        let expression = &rest[start + 4..];
        let mut depth = 1;
        let mut comma = None;
        let mut end = None;
        for (index, ch) in expression.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(index);
                        break;
                    }
                }
                ',' if depth == 1 && comma.is_none() => comma = Some(index),
                _ => {}
            }
        }
        let Some(end) = end else {
            output.push_str(&rest[start..]);
            return output;
        };
        if let Some(comma) = comma {
            output.push_str(&css_fallbacks(expression[comma + 1..end].trim()));
        } else {
            output.push_str(&rest[start..start + 4 + end + 1]);
        }
        rest = &expression[end + 1..];
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fits_within_the_largest_texture_keeping_aspect_ratio() {
        assert_eq!(fit_within(4832, 9488, 8192), (4172, 8192));
        assert_eq!(fit_within(9488, 4832, 8192), (8192, 4172));
        assert_eq!(fit_within(8192, 100, 8192), (8192, 100));
        assert_eq!(fit_within(20000, 1, 8192), (8192, 1));
    }

    #[test]
    fn resizing_does_not_darken_edges_next_to_transparency() {
        // Opaque white beside fully transparent black.
        let pixels = [[255, 255, 255, 255], [0, 0, 0, 0]].repeat(2).concat();
        let resized = resize_rgba(2, 2, &pixels, 1, 1);
        let pixel = resized.get_pixel(0, 0).0;
        assert_eq!(&pixel[..3], &[255, 255, 255]);
        assert!((100..=155).contains(&pixel[3]));
    }

    #[test]
    fn resizing_keeps_the_hue_of_translucent_pixels() {
        let pixels = [64, 128, 192, 1].repeat(16);
        let resized = resize_rgba(4, 4, &pixels, 3, 2);
        assert!(resized.pixels().all(|pixel| pixel.0 == [64, 128, 192, 1]));
    }

    #[test]
    fn resizing_averages_what_each_output_pixel_covers() {
        // A 4-pixel ramp halved: each output pixel weighs its two source
        // pixels 0.75 and the next one inward 0.25; the one past the edge
        // is missing, so the weights are renormalized over 1.75.
        let pixels = [0u8, 60, 120, 180]
            .iter()
            .flat_map(|&value| [value, value, value, 255])
            .collect::<Vec<_>>();
        let resized = resize_rgba(4, 1, &pixels, 2, 1);
        assert_eq!(resized.get_pixel(0, 0).0, [43, 43, 43, 255]);
        assert_eq!(resized.get_pixel(1, 0).0, [137, 137, 137, 255]);
    }

    #[test]
    fn raster_fit_preserves_aspect_ratio_and_centers_or_crops() {
        let mut cache = ImageSources::default();
        cache.insert_raster(
            "photo",
            RgbaImage::from_fn(4, 2, |x, _| image::Rgba([x as u8 * 60, 0, 0, 255])),
        );
        let path = Path::new("not-read.png");
        let sized = cache
            .render("photo", path, Some(8.0), None, None, 1.0)
            .unwrap();
        assert_eq!((sized.width, sized.height), (8.0, 4.0));
        let contain = cache
            .render(
                "photo",
                path,
                Some(4.0),
                Some(4.0),
                Some(ImageFit::Contain),
                1.0,
            )
            .unwrap();
        assert_eq!(contain.pixels.get_pixel(0, 0).0[3], 0);
        assert_eq!(contain.pixels.get_pixel(0, 1).0[3], 255);
        let cover = cache
            .render(
                "photo",
                path,
                Some(2.0),
                Some(2.0),
                Some(ImageFit::Cover),
                1.0,
            )
            .unwrap();
        assert_eq!(cover.pixels.get_pixel(0, 0).0, [60, 0, 0, 255]);
        assert_eq!(cover.pixels.get_pixel(1, 1).0, [120, 0, 0, 255]);
        let again = cache
            .render(
                "photo",
                path,
                Some(2.0),
                Some(2.0),
                Some(ImageFit::Cover),
                1.0,
            )
            .unwrap();
        assert!(Arc::ptr_eq(&cover.pixels, &again.pixels));
    }
    #[test]
    fn missing_invalid_and_oversized_images_return_asset_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.svg");
        let mut cache = ImageSources::default();
        assert!(
            cache
                .render("missing", &path, None, None, None, 1.0)
                .is_err()
        );
        std::fs::write(&path, "invalid SVG").unwrap();
        assert!(
            cache
                .render("invalid", &path, None, None, None, 1.0)
                .is_err()
        );
        cache.insert_raster("size", RgbaImage::new(1, 1));
        for width in [0.0, -1.0, f64::NAN, 20000.0] {
            assert!(
                cache
                    .render(
                        "size",
                        &path,
                        Some(width),
                        None,
                        Some(ImageFit::Contain),
                        1.0
                    )
                    .is_err()
            );
        }
    }
    #[test]
    fn resolves_nested_css_fallbacks() {
        assert_eq!(
            css_fallbacks("fill:var(--a, var(--b, rgb(255, 0, 0)))"),
            "fill:rgb(255, 0, 0)"
        );
    }
    #[test]
    fn svg_uses_display_size_and_scale_and_straight_alpha() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logo.svg");
        std::fs::write(&path, r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="var(--fill, white)" opacity="0.5"/></svg>"#).unwrap();
        let result = ImageSources::default()
            .render("logo", &path, Some(40.0), None, None, 2.0)
            .unwrap();
        assert_eq!(
            (result.width, result.height, result.pixels.dimensions()),
            (40.0, 20.0, (80, 40))
        );
        assert_eq!(result.pixels.get_pixel(40, 20).0, [255, 255, 255, 128]);
    }
}
