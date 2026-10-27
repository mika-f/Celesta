//! Shared raster and SVG decoding and display-box layout for both renderers.
use celesta_composition::{ImageAnimation, ImageFit};
use image::{
    AnimationDecoder, ImageFormat, ImageReader, Rgba, RgbaImage,
    codecs::{gif::GifDecoder, png::PngDecoder, webp::WebPDecoder},
};
use resvg::{tiny_skia, usvg};
use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    io::Read,
    path::Path,
    sync::Arc,
};

#[derive(Default)]
pub struct ImageSources {
    sources: HashMap<String, Source>,
    /// The latest renders of each source, most recently used first: one per
    /// display size and fit (its latest resolution), at most
    /// [`LAYOUTS_PER_SOURCE`].
    rendered: HashMap<String, VecDeque<Render>>,
}

struct Render {
    layout: String,
    resolution: (u32, u32),
    image: DisplayImage,
}

/// How many display sizes and fits of one source stay rendered. One frame
/// can draw an image at several (a `cover` thumbnail and a `contain` hero),
/// and keeping only the last would render each again every frame.
const LAYOUTS_PER_SOURCE: usize = 4;

/// How many pixels a source's renders besides its latest may hold between
/// them (64 MB of RGBA): a few layouts of an image, not several of a huge SVG.
const OLDER_RENDER_PIXELS: u64 = 16 << 20;

/// Keeps the latest of `renders`, and the next most recent ones up to
/// `LAYOUTS_PER_SOURCE` in all while they hold `pixels` or fewer.
fn keep_recent(renders: &mut VecDeque<Render>, pixels: u64) {
    let mut held = 0;
    let older = renders
        .iter()
        .skip(1)
        .take(LAYOUTS_PER_SOURCE - 1)
        .take_while(|render| {
            held += u64::from(render.resolution.0) * u64::from(render.resolution.1);
            held <= pixels
        })
        .count();
    renders.truncate(1 + older);
}
enum Source {
    Raster(Arc<RgbaImage>),
    Animated(Animation),
    Svg(Box<usvg::Tree>),
}

impl Source {
    fn size(&self) -> (f64, f64) {
        match self {
            Source::Raster(image) => (image.width() as f64, image.height() as f64),
            Source::Animated(animation) => {
                let (width, height) = animation.frames[0].dimensions();
                (width as f64, height as f64)
            }
            Source::Svg(tree) => (tree.size().width() as f64, tree.size().height() as f64),
        }
    }

    /// The pixels of the frame showing at `animation` and its index; no
    /// pixels for an SVG.
    fn frame(&self, animation: Option<ImageAnimation>) -> (Option<&Arc<RgbaImage>>, usize) {
        match self {
            Source::Raster(pixels) => (Some(pixels), 0),
            Source::Animated(frames) => {
                let frame = animation.map_or(0, |animation| {
                    animation_frame(
                        &frames.ends,
                        animation.source_time_seconds,
                        animation.r#loop,
                    )
                });
                (Some(&frames.frames[frame]), frame)
            }
            Source::Svg(_) => (None, 0),
        }
    }

    /// How `ImageSources::render` lays out `self`, whose frame's pixels are
    /// `raster`, in a `width` x `height` box at `density`.
    fn layout(
        &self,
        raster: Option<&Arc<RgbaImage>>,
        width: Option<f64>,
        height: Option<f64>,
        fit: Option<ImageFit>,
        density: f64,
    ) -> Result<ImageLayout, image::ImageError> {
        let (sw, sh) = self.size();
        let (w, h) = display_size(sw, sh, width, height);
        if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
            return Err(invalid("image dimensions must be finite and positive"));
        }
        if fit.is_none()
            && let Some(pixels) = raster
        {
            return Ok(ImageLayout {
                width: w,
                height: h,
                pixel_width: pixels.width(),
                pixel_height: pixels.height(),
            });
        }
        // SVG rasterization follows the
        // complete parent transform, including animated scale and rotation.
        let density = match self {
            Source::Svg(_) => density.max(1.0),
            Source::Raster(_) | Source::Animated(_) => 1.0,
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
        Ok(ImageLayout {
            width: w,
            height: h,
            pixel_width: pw as u32,
            pixel_height: ph as u32,
        })
    }
}

/// The frames of a GIF, APNG, or animated WebP, each composited onto the
/// ones before it as its disposal and blending say, so any frame is drawn
/// without decoding the others again.
pub struct Animation {
    pub frames: Vec<Arc<RgbaImage>>,
    /// When each frame stops showing, in microseconds from the start.
    pub ends: Vec<u64>,
}

impl Animation {
    pub fn duration_seconds(&self) -> f64 {
        self.ends.last().copied().unwrap_or(0) as f64 / 1e6
    }
}

/// How long a frame with a delay of `numer / denom` milliseconds shows, in
/// microseconds. Delays of 10 ms or less show for 100 ms, as browsers show
/// them: GIFs saved with no delay expect it.
pub fn frame_duration_micros(numer: u32, denom: u32) -> u64 {
    let micros = if denom == 0 {
        0
    } else {
        (u64::from(numer) * 1000 + u64::from(denom) / 2) / u64::from(denom)
    };
    if micros <= 10_000 { 100_000 } else { micros }
}

/// The frame showing `seconds` into an animation whose frames end at `ends`
/// (microseconds). Before the start it is the first frame; past the end the
/// animation starts over with `looped` and holds its last frame otherwise.
/// `@celesta/web` picks frames the same way.
pub fn animation_frame(ends: &[u64], seconds: f64, looped: bool) -> usize {
    let total = ends.last().copied().unwrap_or(0);
    if total == 0 {
        return 0;
    }
    let micros = if seconds.is_finite() && seconds > 0.0 {
        (seconds * 1e6).round() as u64
    } else {
        0
    };
    let micros = if looped {
        micros % total
    } else if micros >= total {
        return ends.len() - 1;
    } else {
        micros
    };
    ends.partition_point(|&end| end <= micros)
        .min(ends.len() - 1)
}

/// How many bytes an animation's decoded frames may hold between them.
const ANIMATION_BYTES: u64 = 1 << 30;

/// Decodes every frame of `data` when it is an animated GIF, APNG, or WebP;
/// `None` for anything else, including those formats with a single frame.
pub fn decode_animation(data: &[u8]) -> Result<Option<Animation>, image::ImageError> {
    let webp;
    let cursor = |data| std::io::Cursor::new(data);
    let frames = match image::guess_format(data) {
        Ok(ImageFormat::Gif) => GifDecoder::new(cursor(data))?.into_frames(),
        Ok(ImageFormat::Png) => {
            let decoder = PngDecoder::new(cursor(data))?;
            if !decoder.is_apng()? {
                return Ok(None);
            }
            decoder.apng()?.into_frames()
        }
        Ok(ImageFormat::WebP) => {
            webp = with_webp_alpha(data);
            let mut decoder = WebPDecoder::new(cursor(&*webp))?;
            if !decoder.has_animation() {
                return Ok(None);
            }
            // Disposed frames clear to transparent, as browsers draw them;
            // without a color the decoder leaves them in place.
            decoder.set_background_color(Rgba([0, 0, 0, 0]))?;
            decoder.into_frames()
        }
        _ => return Ok(None),
    };
    let mut animation = Animation {
        frames: Vec::new(),
        ends: Vec::new(),
    };
    let mut bytes = 0;
    let mut end = 0;
    for frame in frames {
        let frame = frame?;
        let (numer, denom) = frame.delay().numer_denom_ms();
        let pixels = frame.into_buffer();
        bytes += pixels.as_raw().len() as u64;
        if bytes > ANIMATION_BYTES {
            return Err(invalid(
                "the animated image's frames decode to more than 1 GiB",
            ));
        }
        end += frame_duration_micros(numer, denom);
        animation.frames.push(Arc::new(pixels));
        animation.ends.push(end);
    }
    Ok((animation.frames.len() > 1).then_some(animation))
}

/// Whether `data` is an animated GIF, APNG, or WebP, read from its header
/// (and a GIF's first two frames) without decoding the whole animation.
pub fn is_animated(data: &[u8]) -> bool {
    let cursor = std::io::Cursor::new(data);
    match image::guess_format(data) {
        Ok(ImageFormat::Gif) => {
            GifDecoder::new(cursor).is_ok_and(|decoder| decoder.into_frames().take(2).count() == 2)
        }
        Ok(ImageFormat::Png) => PngDecoder::new(cursor)
            .and_then(|decoder| decoder.is_apng())
            .unwrap_or(false),
        Ok(ImageFormat::WebP) => {
            WebPDecoder::new(cursor).is_ok_and(|decoder| decoder.has_animation())
        }
        _ => false,
    }
}

/// `data`, an extended WebP, marked as having alpha if it is animated. An
/// animation's canvas is transparent where no frame has drawn yet, or where
/// one was disposed, even when no frame has alpha of its own; marked
/// without it, the decoder would return those pixels opaque black.
fn with_webp_alpha(data: &[u8]) -> Cow<'_, [u8]> {
    const VP8X_FLAGS: usize = 20;
    const ALPHA: u8 = 0x10;
    const ANIMATION: u8 = 0x02;
    match data.get(VP8X_FLAGS) {
        Some(&flags)
            if data.get(12..16) == Some(b"VP8X") && flags & (ALPHA | ANIMATION) == ANIMATION =>
        {
            let mut data = data.to_vec();
            data[VP8X_FLAGS] |= ALPHA;
            Cow::Owned(data)
        }
        _ => Cow::Borrowed(data),
    }
}

/// The size, frame count, and duration of an animated image file, or `None`
/// for anything else.
pub struct AnimationInfo {
    pub width: u32,
    pub height: u32,
    pub frame_count: usize,
    pub duration_seconds: f64,
}

pub fn probe_animation(path: &Path) -> Result<Option<AnimationInfo>, image::ImageError> {
    // Enough to tell the format, before reading what may be a long video.
    let mut header = Vec::with_capacity(16);
    std::fs::File::open(path)?
        .take(16)
        .read_to_end(&mut header)?;
    if !matches!(
        image::guess_format(&header),
        Ok(ImageFormat::Gif | ImageFormat::Png | ImageFormat::WebP)
    ) {
        return Ok(None);
    }
    let data = std::fs::read(path)?;
    Ok(decode_animation(&data)?.map(|animation| {
        let (width, height) = animation.frames[0].dimensions();
        AnimationInfo {
            width,
            height,
            frame_count: animation.frames.len(),
            duration_seconds: animation.duration_seconds(),
        }
    }))
}

#[derive(Clone)]
pub struct DisplayImage {
    pub pixels: Arc<RgbaImage>,
    pub width: f64,
    pub height: f64,
    /// SVG pixels are rasterized at the requested display density.
    pub is_svg: bool,
    /// Which frame of an animated image this is; 0 for any other image.
    pub frame: usize,
}

/// Where `ImageSources::render` lays an image out, without its pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageLayout {
    /// The display size in layer units.
    pub width: f64,
    pub height: f64,
    /// The size of the pixels `render` returns.
    pub pixel_width: u32,
    pub pixel_height: u32,
}

impl ImageSources {
    /// Adds decoded pixels, allowing callers to share an existing asset cache.
    pub fn insert_raster(&mut self, key: &str, pixels: RgbaImage) {
        self.sources
            .entry(key.to_owned())
            .or_insert(Source::Raster(Arc::new(pixels)));
    }

    /// Reads and parses (an SVG) or decodes (a raster image) the file at
    /// `path` under `key`, once.
    fn load(&mut self, key: &str, path: &Path) -> Result<(), image::ImageError> {
        if self.sources.contains_key(key) {
            return Ok(());
        }
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
        } else if let Some(animation) = decode_animation(&data)? {
            Source::Animated(animation)
        } else {
            Source::Raster(Arc::new(
                ImageReader::new(std::io::Cursor::new(data))
                    .with_guessed_format()?
                    .decode()?
                    .to_rgba8(),
            ))
        };
        self.sources.insert(key.to_owned(), source);
        Ok(())
    }

    /// The image's own size in layer units, loading it under `key` without
    /// rasterizing it.
    pub fn natural_size(
        &mut self,
        key: &str,
        path: &Path,
    ) -> Result<(f64, f64), image::ImageError> {
        self.load(key, path)?;
        Ok(self.sources[key].size())
    }

    /// Where `render` lays out the image at `key` (loading it from `path`
    /// once), without rasterizing or resampling it: for an SVG, only its
    /// size counts.
    #[allow(clippy::too_many_arguments)]
    pub fn layout(
        &mut self,
        key: &str,
        path: &Path,
        width: Option<f64>,
        height: Option<f64>,
        fit: Option<ImageFit>,
        animation: Option<ImageAnimation>,
        density: f64,
    ) -> Result<ImageLayout, image::ImageError> {
        self.load(key, path)?;
        let source = &self.sources[key];
        let (raster, _) = source.frame(animation);
        source.layout(raster, width, height, fit, density)
    }

    /// Lays out the image at `key` (loading it from `path` once) in a
    /// `width` x `height` box, drawing an animated image's frame at
    /// `animation`.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        key: &str,
        path: &Path,
        width: Option<f64>,
        height: Option<f64>,
        fit: Option<ImageFit>,
        animation: Option<ImageAnimation>,
        density: f64,
    ) -> Result<DisplayImage, image::ImageError> {
        self.load(key, path)?;
        let source = &self.sources[key];
        let (sw, sh) = source.size();
        let (raster, frame) = source.frame(animation);
        let ImageLayout {
            width: w,
            height: h,
            pixel_width: pw,
            pixel_height: ph,
        } = source.layout(raster, width, height, fit, density)?;
        if fit.is_none()
            && let Some(pixels) = raster
        {
            return Ok(DisplayImage {
                pixels: pixels.clone(),
                width: w,
                height: h,
                is_svg: false,
                frame,
            });
        }
        let layout = format!("{w}:{h}:{fit:?}:{frame}");
        let renders = self.rendered.entry(key.to_owned()).or_default();
        // An animated SVG replaces its layout's render at a new resolution
        // instead of keeping one per resolution.
        if let Some(index) = renders.iter().position(|render| render.layout == layout) {
            let render = renders.remove(index).expect("the index was just found");
            if render.resolution == (pw, ph) {
                let image = render.image.clone();
                renders.push_front(render);
                return Ok(image);
            }
        }
        let (sx, sy) = (pw as f64 / sw, ph as f64 / sh);
        let (sx, sy) = match fit {
            Some(ImageFit::Contain) => (sx.min(sy), sx.min(sy)),
            Some(ImageFit::Cover) => (sx.max(sy), sx.max(sy)),
            None => (sx, sy),
        };
        let (ox, oy) = ((pw as f64 - sw * sx) / 2.0, (ph as f64 - sh * sy) / 2.0);
        let pixels = match (source, raster) {
            (_, Some(image)) => {
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
            (Source::Svg(tree), None) => {
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
                for pixel in pixmap.data_mut().as_chunks_mut::<4>().0 {
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
            (Source::Raster(_) | Source::Animated(_), None) => {
                unreachable!("raster sources have pixels")
            }
        };
        let image = DisplayImage {
            pixels: Arc::new(pixels),
            width: w,
            height: h,
            is_svg: matches!(source, Source::Svg(_)),
            frame,
        };
        renders.push_front(Render {
            layout,
            resolution: (pw, ph),
            image: image.clone(),
        });
        keep_recent(renders, OLDER_RENDER_PIXELS);
        Ok(image)
    }
}

/// The size an `Image` or a `Video` frame of `natural_width` x
/// `natural_height` displays at: `width` and `height` when both are set, the
/// natural aspect ratio from the one that is, and the natural size when
/// neither is.
pub fn display_size(
    natural_width: f64,
    natural_height: f64,
    width: Option<f64>,
    height: Option<f64>,
) -> (f64, f64) {
    match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, w * natural_height / natural_width),
        (None, Some(h)) => (h * natural_width / natural_height, h),
        (None, None) => (natural_width, natural_height),
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
                    for (pixel, weight) in
                        source[first * 4..].as_chunks::<4>().0.iter().zip(weights)
                    {
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
    fn keeps_the_renders_of_several_layouts_of_one_source() {
        let mut cache = ImageSources::default();
        cache.insert_raster("photo", RgbaImage::new(4, 2));
        let path = Path::new("not-read.png");
        let mut render = |fit| {
            cache
                .render("photo", path, Some(8.0), Some(8.0), Some(fit), None, 1.0)
                .unwrap()
                .pixels
        };
        // One frame draws the image both ways, the next frame again.
        let cover = render(ImageFit::Cover);
        let contain = render(ImageFit::Contain);
        assert!(Arc::ptr_eq(&cover, &render(ImageFit::Cover)));
        assert!(Arc::ptr_eq(&contain, &render(ImageFit::Contain)));
    }

    #[test]
    fn keeps_older_renders_only_within_their_pixel_budget() {
        let render = |layout: &str, width: u32, height: u32| Render {
            layout: layout.to_owned(),
            resolution: (width, height),
            image: DisplayImage {
                pixels: Arc::new(RgbaImage::new(1, 1)),
                width: 1.0,
                height: 1.0,
                is_svg: true,
                frame: 0,
            },
        };
        let layouts = |renders: &VecDeque<Render>| {
            renders
                .iter()
                .map(|render| render.layout.clone())
                .collect::<Vec<_>>()
        };
        // The latest stays whatever its size; the older ones while they fit.
        let mut renders: VecDeque<_> = [
            render("huge", 100, 100),
            render("a", 4, 4),
            render("b", 4, 4),
            render("c", 2, 2),
        ]
        .into();
        keep_recent(&mut renders, 32);
        assert_eq!(layouts(&renders), ["huge", "a", "b"]);
        let mut renders: VecDeque<_> = [render("new", 1, 1), render("huge", 100, 100)].into();
        keep_recent(&mut renders, 32);
        assert_eq!(layouts(&renders), ["new"]);
        let mut renders: VecDeque<_> = (0..6).map(|i| render(&i.to_string(), 1, 1)).collect();
        keep_recent(&mut renders, 32);
        assert_eq!(renders.len(), LAYOUTS_PER_SOURCE);
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
            .render("photo", path, Some(8.0), None, None, None, 1.0)
            .unwrap();
        assert_eq!((sized.width, sized.height), (8.0, 4.0));
        let contain = cache
            .render(
                "photo",
                path,
                Some(4.0),
                Some(4.0),
                Some(ImageFit::Contain),
                None,
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
                None,
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
                None,
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
                .render("missing", &path, None, None, None, None, 1.0)
                .is_err()
        );
        std::fs::write(&path, "invalid SVG").unwrap();
        assert!(
            cache
                .render("invalid", &path, None, None, None, None, 1.0)
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
                        None,
                        1.0
                    )
                    .is_err()
            );
        }
    }
    const RED: [u8; 4] = [255, 0, 0, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    fn at(seconds: f64) -> Option<ImageAnimation> {
        Some(ImageAnimation {
            source_time_seconds: seconds,
            r#loop: true,
        })
    }

    /// The pixels of `path`'s frame `seconds` in, a row at a time.
    fn frame_pixels(path: &Path, seconds: f64) -> Vec<[u8; 4]> {
        let image = ImageSources::default()
            .render("animation", path, None, None, None, at(seconds), 1.0)
            .unwrap();
        image.pixels.pixels().map(|pixel| pixel.0).collect()
    }

    #[test]
    fn picks_the_frame_showing_at_a_time() {
        let ends = [100_000, 300_000, 400_000];
        let frame = |seconds, looped| animation_frame(&ends, seconds, looped);
        assert_eq!(frame(0.0, false), 0);
        assert_eq!(frame(0.0999, false), 0);
        assert_eq!(frame(0.1, false), 1);
        assert_eq!(frame(0.35, false), 2);
        // Past the end: start over, or hold the last frame.
        assert_eq!(frame(0.4, true), 0);
        assert_eq!(frame(0.45, true), 0);
        assert_eq!(frame(0.75, true), 2);
        assert_eq!(frame(0.4, false), 2);
        assert_eq!(frame(10.0, false), 2);
        for before in [-1.0, f64::NAN, f64::NEG_INFINITY] {
            assert_eq!(frame(before, true), 0);
        }
        assert_eq!(animation_frame(&[], 1.0, true), 0);
        assert_eq!(animation_frame(&[0, 0], 1.0, true), 0);
        assert_eq!(animation_frame(&[0, 0], 1.0, false), 0);
    }

    #[test]
    fn short_delays_show_as_browsers_show_them() {
        assert_eq!(frame_duration_micros(0, 1), 100_000);
        assert_eq!(frame_duration_micros(10, 1), 100_000);
        assert_eq!(frame_duration_micros(20, 1), 20_000);
        // An APNG frame of 1/30 s.
        assert_eq!(frame_duration_micros(100, 3), 33_333);
        assert_eq!(frame_duration_micros(1, 0), 100_000);
    }

    /// A 3x1 GIF exercising every disposal method.
    fn write_gif(path: &Path) {
        let palette = [255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0];
        let mut file = std::fs::File::create(path).unwrap();
        let mut encoder = gif::Encoder::new(&mut file, 3, 1, &palette).unwrap();
        let frames: [(u16, &[u8], gif::DisposalMethod, u16); 4] = [
            (0, &[0, 0, 0], gif::DisposalMethod::Keep, 5),
            (1, &[1], gif::DisposalMethod::Previous, 5),
            (2, &[2], gif::DisposalMethod::Background, 5),
            // A transparent pixel, saved with no delay.
            (0, &[3], gif::DisposalMethod::Keep, 0),
        ];
        for (left, buffer, dispose, delay) in frames {
            encoder
                .write_frame(&gif::Frame {
                    left,
                    width: buffer.len() as u16,
                    height: 1,
                    buffer: buffer.into(),
                    dispose,
                    delay,
                    transparent: Some(3),
                    ..gif::Frame::default()
                })
                .unwrap();
        }
    }

    #[test]
    fn composites_gif_frames_through_their_disposal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dispose.gif");
        write_gif(&path);
        let animation = decode_animation(&std::fs::read(&path).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(animation.ends, [50_000, 100_000, 150_000, 250_000]);
        assert_eq!(frame_pixels(&path, 0.0), [RED, RED, RED]);
        // Drawn over the first frame, then put back.
        assert_eq!(frame_pixels(&path, 0.05), [RED, GREEN, RED]);
        assert_eq!(frame_pixels(&path, 0.1), [RED, RED, BLUE]);
        // The blue pixel cleared; the transparent one shows what is below.
        assert_eq!(frame_pixels(&path, 0.2), [RED, RED, CLEAR]);
        // Scrubbing back gives the same frame again.
        assert_eq!(frame_pixels(&path, 0.25 + 0.05), [RED, GREEN, RED]);
    }

    #[test]
    fn composites_apng_frames_through_their_disposal_and_blending() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blend.png");
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = png::Encoder::new(file, 2, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_animated(2, 0).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.set_frame_delay(1, 10).unwrap();
        writer.set_dispose_op(png::DisposeOp::Background).unwrap();
        writer.set_blend_op(png::BlendOp::Source).unwrap();
        writer.write_image_data(&[RED, CLEAR].concat()).unwrap();
        writer.set_frame_dimension(1, 1).unwrap();
        writer.set_frame_position(1, 0).unwrap();
        writer.set_frame_delay(1, 4).unwrap();
        writer.set_dispose_op(png::DisposeOp::None).unwrap();
        writer.set_blend_op(png::BlendOp::Over).unwrap();
        writer.write_image_data(&GREEN).unwrap();
        writer.finish().unwrap();
        assert_eq!(frame_pixels(&path, 0.0), [RED, CLEAR]);
        assert_eq!(frame_pixels(&path, 0.1), [CLEAR, GREEN]);
        // 350 ms long: 0.4 s loops back into the first frame.
        assert_eq!(frame_pixels(&path, 0.4), [RED, CLEAR]);
    }

    #[test]
    fn disposes_animated_webp_frames_to_transparent() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two-frames.webp");
        let red = [RED, RED, CLEAR, CLEAR];
        assert_eq!(frame_pixels(&path, 0.0), [red, red].concat());
        // The first frame was disposed to the background, which the file
        // gives as opaque white; browsers clear it to transparent. The blue
        // frame blends over that as libwebp blends it, a level darker.
        let blue = [CLEAR, CLEAR, [0, 0, 254, 255], [0, 0, 254, 255]];
        assert_eq!(frame_pixels(&path, 0.1), [blue, blue].concat());
        let info = probe_animation(&path).unwrap().unwrap();
        assert_eq!(
            (
                info.width,
                info.height,
                info.frame_count,
                info.duration_seconds
            ),
            (4, 2, 2, 0.35)
        );
    }

    #[test]
    fn holds_the_last_frame_without_looping_and_draws_the_first_without_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dispose.gif");
        write_gif(&path);
        let mut cache = ImageSources::default();
        let mut frame = |animation| {
            cache
                .render("gif", &path, Some(6.0), None, None, animation, 1.0)
                .unwrap()
                .frame
        };
        assert_eq!(frame(None), 0);
        assert_eq!(frame(at(0.3)), 1);
        let hold = ImageAnimation {
            source_time_seconds: 0.3,
            r#loop: false,
        };
        assert_eq!(frame(Some(hold)), 3);
    }

    #[test]
    fn fits_each_frame_of_an_animation_into_its_box() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dispose.gif");
        write_gif(&path);
        let mut cache = ImageSources::default();
        let mut render = |seconds| {
            cache
                .render(
                    "gif",
                    &path,
                    Some(1.0),
                    Some(1.0),
                    Some(ImageFit::Cover),
                    at(seconds),
                    1.0,
                )
                .unwrap()
                .pixels
                .get_pixel(0, 0)
                .0
        };
        // Cover crops the 3x1 frame to its middle pixel.
        assert_eq!(render(0.0), RED);
        assert_eq!(render(0.05), GREEN);
        assert_eq!(render(0.0), RED);
    }

    #[test]
    fn tells_animated_images_from_still_ones() {
        let dir = tempfile::tempdir().unwrap();
        let gif = dir.path().join("dispose.gif");
        write_gif(&gif);
        let webp = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two-frames.webp");
        let read = |path: &Path| std::fs::read(path).unwrap();
        assert!(is_animated(&read(&gif)));
        assert!(is_animated(&read(&webp)));
        let mut png = Vec::new();
        RgbaImage::new(1, 1)
            .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        assert!(!is_animated(&png));
        assert!(!is_animated(b"<svg/>"));
    }

    #[test]
    fn loads_a_single_frame_gif_as_a_still_image() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("still.gif");
        let mut file = std::fs::File::create(&path).unwrap();
        let mut encoder = gif::Encoder::new(&mut file, 1, 1, &[0, 0, 255]).unwrap();
        encoder
            .write_frame(&gif::Frame {
                width: 1,
                height: 1,
                buffer: [0].as_slice().into(),
                ..gif::Frame::default()
            })
            .unwrap();
        drop(encoder);
        assert!(probe_animation(&path).unwrap().is_none());
        assert_eq!(frame_pixels(&path, 1.0), [BLUE]);
    }

    #[test]
    fn resolves_nested_css_fallbacks() {
        assert_eq!(
            css_fallbacks("fill:var(--a, var(--b, rgb(255, 0, 0)))"),
            "fill:rgb(255, 0, 0)"
        );
    }
    #[test]
    fn lays_out_images_as_they_render_without_rasterizing_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logo.svg");
        std::fs::write(&path, r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="white"/></svg>"#).unwrap();
        let mut sources = ImageSources::default();
        sources.insert_raster("photo", RgbaImage::new(4, 2));
        let cases = [
            ("logo", Some(20.5), None, None, 1.0),
            ("logo", Some(40.0), Some(7.3), Some(ImageFit::Contain), 2.5),
            ("logo", None, None, None, 0.25),
            ("photo", Some(9.0), None, None, 3.0),
            ("photo", Some(4.0), Some(4.0), Some(ImageFit::Cover), 3.0),
        ];
        for (key, width, height, fit, density) in cases {
            let layout = sources
                .layout(key, &path, width, height, fit, None, density)
                .unwrap();
            assert!(!sources.rendered.contains_key(key), "{key} rendered");
            let image = sources
                .render(key, &path, width, height, fit, None, density)
                .unwrap();
            let rendered = ImageLayout {
                width: image.width,
                height: image.height,
                pixel_width: image.pixels.width(),
                pixel_height: image.pixels.height(),
            };
            assert_eq!(
                layout, rendered,
                "{key} {width:?} {height:?} {fit:?} {density}"
            );
            sources.rendered.clear();
        }
    }

    #[test]
    fn svg_uses_display_size_and_scale_and_straight_alpha() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logo.svg");
        std::fs::write(&path, r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="var(--fill, white)" opacity="0.5"/></svg>"#).unwrap();
        let result = ImageSources::default()
            .render("logo", &path, Some(40.0), None, None, None, 2.0)
            .unwrap();
        assert_eq!(
            (result.width, result.height, result.pixels.dimensions()),
            (40.0, 20.0, (80, 40))
        );
        assert_eq!(result.pixels.get_pixel(40, 20).0, [255, 255, 255, 128]);
    }
}
