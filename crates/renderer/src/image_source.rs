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
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

/// Bytes [`ImageSources::default`] keeps decoded and rendered, besides the
/// images the current frame uses.
pub const DEFAULT_BUDGET: u64 = 1 << 30;

/// The decoded images both renderers draw, keyed by asset id.
///
/// An entry remembers the file it was decoded from, and that file's
/// modified time and length, and is decoded again when either changes: a
/// React `<Image name>` keeps its id when its `src` changes across a reload,
/// and a file can be overwritten while a document is open. Each decode gets
/// a new [`DisplayImage::revision`], so a texture cached from the old pixels
/// is not drawn again.
///
/// The entries hold at most a byte budget between them, besides the images
/// the current frame uses (since [`Self::begin_frame`]), dropping the least
/// recently used first.
pub struct ImageSources {
    entries: HashMap<String, Entry>,
    budget: u64,
    bytes: u64,
    /// Counts uses, to tell which entry was used least recently.
    tick: u64,
    /// The tick the current frame began at; `None` before the first frame,
    /// when only the image being used is kept past the budget.
    frame_start: Option<u64>,
    /// The revisions handed out so far.
    revisions: u64,
    /// The system fonts SVG text draws with, loaded on the first SVG and
    /// shared by every SVG after it: scanning them is slow. Fonts installed
    /// after the first SVG loads are not seen until a new `ImageSources`
    /// (a reopened document, or the next export run).
    svg_fonts: Option<Arc<usvg::fontdb::Database>>,
}

struct Entry {
    source: Source,
    /// The file `source` was decoded from and its stamp when it was read;
    /// `None` for pixels given to [`ImageSources::insert_raster`], which
    /// stand for whatever file a caller names.
    file: Option<(PathBuf, FileStamp)>,
    revision: u64,
    /// The latest renders of `source`, most recently used first: one per
    /// display size and fit (its latest resolution), at most
    /// [`LAYOUTS_PER_SOURCE`].
    rendered: VecDeque<Render>,
    /// What `source` holds, estimated for an SVG from its file's length.
    source_bytes: u64,
    /// `source_bytes` and the pixels of `rendered`.
    bytes: u64,
    last_used: u64,
}

impl Entry {
    fn count_bytes(&mut self) {
        self.bytes = self.source_bytes
            + self
                .rendered
                .iter()
                .map(|render| render.image.pixels.as_raw().len() as u64)
                .sum::<u64>();
    }
}

/// When a file was last modified and how long it is, to tell when it was
/// overwritten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl FileStamp {
    fn read(path: &Path) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        Ok(Self {
            modified: metadata.modified().ok(),
            len: metadata.len(),
        })
    }
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
        let (w, h) = display_size(sw, sh, width, height).map_err(invalid)?;
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
/// `@celesta.studio/web` picks frames the same way.
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
    /// Which decode of its asset this was drawn from: a new one whenever
    /// the asset's file changes, so a cache of what was drawn from it can
    /// tell stale pixels by this alone.
    pub revision: u64,
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

impl Default for ImageSources {
    fn default() -> Self {
        Self::with_budget(DEFAULT_BUDGET)
    }
}

impl ImageSources {
    /// A cache holding at most `budget` bytes, besides the images the
    /// current frame uses.
    pub fn with_budget(budget: u64) -> Self {
        Self {
            entries: HashMap::new(),
            budget,
            bytes: 0,
            tick: 0,
            frame_start: None,
            revisions: 0,
            svg_fonts: None,
        }
    }

    /// Starts a frame: the images used from here on stay past the budget
    /// until the next frame begins, so a frame drawing more than the
    /// budget does not decode its own images again, and the ones only
    /// earlier frames used are dropped first.
    pub fn begin_frame(&mut self) {
        self.frame_start = Some(self.tick);
        self.evict(None);
    }

    /// The bytes the cached images hold between them.
    pub fn cached_bytes(&self) -> u64 {
        self.bytes
    }

    /// Adds decoded pixels, allowing callers to share an existing asset
    /// cache. They are drawn for `key` whatever file a caller names, and
    /// are not replaced by a later call for the same `key`.
    pub fn insert_raster(&mut self, key: &str, pixels: RgbaImage) {
        if self.entries.contains_key(key) {
            return;
        }
        self.tick += 1;
        let bytes = pixels.as_raw().len() as u64;
        self.insert(key, Source::Raster(Arc::new(pixels)), None, bytes);
    }

    /// Reads and parses (an SVG) or decodes (a raster image) the file at
    /// `path` under `key`, unless `key` already holds that file as it is
    /// on disk now.
    fn load(&mut self, key: &str, path: &Path) -> Result<(), image::ImageError> {
        self.tick += 1;
        if let Some(entry) = self.entries.get_mut(key) {
            let current = match &entry.file {
                None => true,
                Some((file, stamp)) => {
                    file == path && FileStamp::read(path).is_ok_and(|now| now == *stamp)
                }
            };
            if current {
                entry.last_used = self.tick;
                return Ok(());
            }
            // Stale: dropped before the new pixels are decoded, so both are
            // never held at once.
            let stale = self.entries.remove(key).expect("the entry was just found");
            self.bytes -= stale.bytes;
        }
        // Stamped before reading: a write landing during the read changes
        // the file's stamp from this one, so the next use reads it again.
        let stamp = FileStamp::read(path)?;
        let data = std::fs::read(path)?;
        let svg = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
            || std::str::from_utf8(&data).is_ok_and(|s| s.contains("<svg"));
        let file_bytes = data.len() as u64;
        let source = if svg {
            let text = std::str::from_utf8(&data).map_err(invalid)?;
            let text = css_fallbacks(text);
            let fonts = self.svg_fonts.get_or_insert_with(|| {
                let mut fonts = usvg::fontdb::Database::new();
                fonts.load_system_fonts();
                Arc::new(fonts)
            });
            let options = usvg::Options {
                resources_dir: path.parent().map(Path::to_path_buf),
                fontdb: Arc::clone(fonts),
                ..Default::default()
            };
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
        let bytes = match &source {
            Source::Raster(pixels) => pixels.as_raw().len() as u64,
            Source::Animated(animation) => animation
                .frames
                .iter()
                .map(|frame| frame.as_raw().len() as u64)
                .sum(),
            // The parsed tree is not measured; its renders are.
            Source::Svg(_) => file_bytes,
        };
        self.insert(key, source, Some((path.to_owned(), stamp)), bytes);
        Ok(())
    }

    /// Stores `source` under `key` as a new revision, used now, and drops
    /// what no longer fits the budget.
    fn insert(
        &mut self,
        key: &str,
        source: Source,
        file: Option<(PathBuf, FileStamp)>,
        source_bytes: u64,
    ) {
        self.revisions += 1;
        let entry = Entry {
            source,
            file,
            revision: self.revisions,
            rendered: VecDeque::new(),
            source_bytes,
            bytes: source_bytes,
            last_used: self.tick,
        };
        self.bytes += entry.bytes;
        if let Some(previous) = self.entries.insert(key.to_owned(), entry) {
            self.bytes -= previous.bytes;
        }
        self.evict(Some(key));
    }

    /// Drops the least recently used entries, other than `keep` and the
    /// ones the current frame used, until the rest fit the budget.
    fn evict(&mut self, keep: Option<&str>) {
        let protected_after = self.frame_start.unwrap_or(u64::MAX);
        while self.bytes > self.budget {
            let Some(oldest) = self
                .entries
                .iter()
                .filter(|(key, entry)| {
                    Some(key.as_str()) != keep && entry.last_used <= protected_after
                })
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            let evicted = self.entries.remove(&oldest).expect("evicted entry exists");
            self.bytes -= evicted.bytes;
        }
    }

    /// The image's own size in layer units, loading it under `key` without
    /// rasterizing it.
    pub fn natural_size(
        &mut self,
        key: &str,
        path: &Path,
    ) -> Result<(f64, f64), image::ImageError> {
        self.load(key, path)?;
        Ok(self.entries[key].source.size())
    }

    /// Where `render` lays out the image at `key` (loading it from `path`
    /// unless it is loaded as it is now), without rasterizing or resampling
    /// it: for an SVG, only its size counts.
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
        let source = &self.entries[key].source;
        let (raster, _) = source.frame(animation);
        source.layout(raster, width, height, fit, density)
    }

    /// Lays out the image at `key` (loading it from `path` unless it is
    /// loaded as it is now) in a `width` x `height` box, drawing an animated
    /// image's frame at `animation`.
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
        let entry = self
            .entries
            .get_mut(key)
            .expect("the image was just loaded");
        let revision = entry.revision;
        let (source, renders) = (&entry.source, &mut entry.rendered);
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
                revision,
            });
        }
        let layout = format!("{w}:{h}:{fit:?}:{frame}");
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
            revision,
        };
        renders.push_front(Render {
            layout,
            resolution: (pw, ph),
            image: image.clone(),
        });
        keep_recent(renders, OLDER_RENDER_PIXELS);
        self.bytes -= entry.bytes;
        entry.count_bytes();
        self.bytes += entry.bytes;
        self.evict(Some(key));
        Ok(image)
    }
}

/// A display size whose width or height is not finite and positive.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
#[error("display size {width}x{height} must be finite and positive")]
pub struct InvalidDisplaySize {
    pub width: f64,
    pub height: f64,
}

/// The size an `Image` or a `Video` frame of `natural_width` x
/// `natural_height` displays at: `width` and `height` when both are set, the
/// natural aspect ratio from the one that is, and the natural size when
/// neither is. Both renderers draw neither unless that size is finite and
/// positive.
pub fn display_size(
    natural_width: f64,
    natural_height: f64,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<(f64, f64), InvalidDisplaySize> {
    let (width, height) = match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, w * natural_height / natural_width),
        (None, Some(h)) => (h * natural_width / natural_height, h),
        (None, None) => (natural_width, natural_height),
    };
    let valid = |side: f64| side.is_finite() && side > 0.0;
    if valid(width) && valid(height) {
        Ok((width, height))
    } else {
        Err(InvalidDisplaySize { width, height })
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
    fn display_size_must_be_finite_and_positive() {
        assert_eq!(
            display_size(320.0, 180.0, Some(160.0), None),
            Ok((160.0, 90.0))
        );
        assert_eq!(display_size(320.0, 180.0, None, None), Ok((320.0, 180.0)));
        for (width, height) in [
            (Some(0.0), None),
            (Some(-160.0), None),
            (None, Some(f64::INFINITY)),
            (Some(160.0), Some(-90.0)),
        ] {
            assert!(
                display_size(320.0, 180.0, width, height).is_err(),
                "{width:?} x {height:?}"
            );
        }
        assert!(display_size(320.0, 180.0, Some(f64::NAN), None).is_err());
        // A side derived from an empty natural size is not finite either.
        assert!(display_size(0.0, 0.0, Some(160.0), None).is_err());
    }

    #[test]
    fn fits_within_the_largest_texture_keeping_aspect_ratio() {
        assert_eq!(fit_within(4832, 9488, 8192), (4172, 8192));
        assert_eq!(fit_within(9488, 4832, 8192), (8192, 4172));
        assert_eq!(fit_within(8192, 100, 8192), (8192, 100));
        assert_eq!(fit_within(20000, 1, 8192), (8192, 1));
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
                revision: 0,
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
    /// Writes a 2x2 PNG of `rgba` to `path`, stamped `seconds` from now so
    /// an overwrite is seen whatever the file system's timestamp resolution.
    fn write_png(path: &Path, rgba: [u8; 4], seconds: u64) {
        RgbaImage::from_pixel(2, 2, image::Rgba(rgba))
            .save(path)
            .unwrap();
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::now() + std::time::Duration::from_secs(seconds))
            .unwrap();
    }

    fn drawn(cache: &mut ImageSources, key: &str, path: &Path) -> DisplayImage {
        cache
            .render(key, path, None, None, None, None, 1.0)
            .unwrap()
    }

    #[test]
    fn decodes_an_ids_new_file_and_its_file_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
        write_png(&a, RED, 0);
        write_png(&b, BLUE, 0);
        let mut cache = ImageSources::default();
        let first = drawn(&mut cache, "logo", &a);
        assert_eq!(first.pixels.get_pixel(0, 0).0, RED);
        // Unchanged: the same decode.
        let again = drawn(&mut cache, "logo", &a);
        assert!(Arc::ptr_eq(&first.pixels, &again.pixels));
        assert_eq!(again.revision, first.revision);
        // A React `<Image name="logo">` keeps its id when its `src` changes.
        let moved = drawn(&mut cache, "logo", &b);
        assert_eq!(moved.pixels.get_pixel(0, 0).0, BLUE);
        assert_ne!(moved.revision, first.revision);
        // The old file's pixels are not kept beside the new ones.
        assert_eq!(cache.cached_bytes(), 16);
        write_png(&b, GREEN, 10);
        let overwritten = drawn(&mut cache, "logo", &b);
        assert_eq!(overwritten.pixels.get_pixel(0, 0).0, GREEN);
        assert_ne!(overwritten.revision, moved.revision);
        // A render for a fit is drawn from the new pixels too.
        let fitted = cache
            .render(
                "logo",
                &b,
                Some(4.0),
                None,
                Some(ImageFit::Cover),
                None,
                1.0,
            )
            .unwrap();
        write_png(&b, RED, 20);
        let refitted = cache
            .render(
                "logo",
                &b,
                Some(4.0),
                None,
                Some(ImageFit::Cover),
                None,
                1.0,
            )
            .unwrap();
        assert_eq!(fitted.pixels.get_pixel(0, 0).0, GREEN);
        assert_eq!(refitted.pixels.get_pixel(0, 0).0, RED);
    }

    #[test]
    fn evicts_the_least_recently_used_images_past_the_budget() {
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<_> = ["a", "b", "c"]
            .iter()
            .map(|name| {
                let path = dir.path().join(format!("{name}.png"));
                write_png(&path, RED, 0);
                path
            })
            .collect();
        // Two 2x2 images fit.
        let mut cache = ImageSources::with_budget(32);
        cache.begin_frame();
        let a = drawn(&mut cache, "a", &paths[0]).revision;
        drawn(&mut cache, "b", &paths[1]);
        cache.begin_frame();
        // `a` used again, so `b` is the one only an earlier frame used.
        assert_eq!(drawn(&mut cache, "a", &paths[0]).revision, a);
        drawn(&mut cache, "c", &paths[2]);
        assert_eq!(cache.cached_bytes(), 32);
        assert_eq!(drawn(&mut cache, "a", &paths[0]).revision, a);
        assert!(!cache.entries.contains_key("b"));
    }

    #[test]
    fn keeps_the_images_the_current_frame_uses_past_the_budget() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = ImageSources::with_budget(16);
        cache.begin_frame();
        let mut revisions = Vec::new();
        for name in ["a", "b", "c"] {
            let path = dir.path().join(format!("{name}.png"));
            write_png(&path, RED, 0);
            revisions.push((name, path.clone(), drawn(&mut cache, name, &path).revision));
        }
        // Drawn again in the same frame: none was decoded again.
        for (name, path, revision) in &revisions {
            assert_eq!(drawn(&mut cache, name, path).revision, *revision);
        }
        assert_eq!(cache.cached_bytes(), 48);
        // The next frame drops what no longer fits, least recently used first.
        cache.begin_frame();
        assert_eq!(cache.cached_bytes(), 16);
        assert!(cache.entries.contains_key("c"));
    }

    #[test]
    fn counts_renders_toward_the_budget() {
        let mut cache = ImageSources::with_budget(u64::MAX);
        cache.insert_raster("photo", RgbaImage::new(2, 2));
        cache
            .render(
                "photo",
                Path::new("not-read.png"),
                Some(4.0),
                Some(4.0),
                Some(ImageFit::Cover),
                None,
                1.0,
            )
            .unwrap();
        // The 2x2 source and its 4x4 render.
        assert_eq!(cache.cached_bytes(), 16 + 64);
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
            assert!(sources.entries[key].rendered.is_empty(), "{key} rendered");
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
            sources
                .entries
                .get_mut(key)
                .expect("the image is loaded")
                .rendered
                .clear();
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

    #[test]
    fn svgs_share_one_font_database() {
        let dir = tempfile::tempdir().unwrap();
        let mut sources = ImageSources::default();
        for name in ["a", "b"] {
            let path = dir.path().join(format!("{name}.svg"));
            std::fs::write(
                &path,
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"/>"#,
            )
            .unwrap();
            sources.natural_size(name, &path).unwrap();
        }
        let fonts = |key: &str| match &sources.entries[key].source {
            Source::Svg(tree) => Arc::clone(tree.fontdb()),
            _ => panic!("{key} is not an SVG"),
        };
        assert!(Arc::ptr_eq(&fonts("a"), &fonts("b")));
    }

    #[test]
    fn svg_text_draws_with_a_system_font() {
        let dir = tempfile::tempdir().unwrap();
        let mut sources = ImageSources::default();
        // The text loads after another SVG, so it draws with the fonts that
        // SVG loaded.
        let blank = dir.path().join("blank.svg");
        std::fs::write(
            &blank,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"/>"#,
        )
        .unwrap();
        sources.natural_size("blank", &blank).unwrap();
        let path = dir.path().join("text.svg");
        std::fs::write(&path, r#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><text x="4" y="32" font-size="32" font-family="DejaVu Sans, Arial, Helvetica, sans-serif" fill="white">Hi</text></svg>"#).unwrap();
        let image = sources
            .render("text", &path, None, None, None, None, 1.0)
            .unwrap();
        let drawn = image
            .pixels
            .pixels()
            .filter(|pixel| pixel[3] == 255)
            .count();
        assert!(drawn > 20, "the text drew {drawn} opaque pixels");
    }
}
