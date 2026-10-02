//! Deterministic CPU reference renderer.
//!
//! This backend exists to fix scene semantics before the GPU renderer arrives.
//! Video and unresolved components remain diagnostic placeholders; images and text use
//! real decoders, font shaping, and glyph rasterization.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use celesta_composition::{
    BlendMode, Clip, Layer, LayerContent, MediaTiming, Paint, Point, ResolvedAsset, Scene, Stroke,
    TextAlign, TextStyle,
};
use celesta_media::{MediaError, VideoFrameDecoder};
use celesta_remote::{RemoteAssetError, resolve_asset_path};
use cosmic_text::fontdb;
use cosmic_text::{
    Align, Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Metrics, Shaping, SwashCache,
    Weight, Wrap,
};
use unicode_properties::{EmojiStatus, GeneralCategory, UnicodeEmoji, UnicodeGeneralCategory};
use unicode_segmentation::UnicodeSegmentation;
pub mod image_source;
mod path;
pub mod psd_source;

pub use path::{
    PathDraw, PathShape, PathTransform, RasterizedPath, rasterize_path, rasterize_paths,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
    pub const WHITE: Self = Self::rgba(255, 255, 255, 255);

    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub fn from_hex(value: &str) -> Result<Self, RenderError> {
        let hex = value
            .strip_prefix('#')
            .ok_or_else(|| RenderError::InvalidColor(value.to_owned()))?;
        if !matches!(hex.len(), 6 | 8) {
            return Err(RenderError::InvalidColor(value.to_owned()));
        }
        let byte = |offset| {
            u8::from_str_radix(&hex[offset..offset + 2], 16)
                .map_err(|_| RenderError::InvalidColor(value.to_owned()))
        };
        Ok(Self::rgba(
            byte(0)?,
            byte(2)?,
            byte(4)?,
            if hex.len() == 8 { byte(6)? } else { 255 },
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderOptions {
    pub background: Color,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            background: Color::rgba(20, 22, 28, 255),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaFrame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RgbaFrame {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn encode_png(&self) -> Result<Vec<u8>, RenderError> {
        let mut output = Vec::new();
        self.write_png_to(&mut output)?;
        Ok(output)
    }

    pub fn write_png(&self, path: impl AsRef<Path>) -> Result<(), RenderError> {
        let file = File::create(path).map_err(RenderError::Io)?;
        let mut writer = BufWriter::new(file);
        self.write_png_to(&mut writer)?;
        writer.flush().map_err(RenderError::Io)
    }

    fn write_png_to(&self, output: impl Write) -> Result<(), RenderError> {
        let mut encoder = png::Encoder::new(output, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(RenderError::Png)?;
        writer
            .write_image_data(&self.pixels)
            .map_err(RenderError::Png)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RasterizedText {
    width: u32,
    height: u32,
    /// Pixel row of the first line's baseline, from the top edge.
    baseline: f32,
    pixels: Vec<u8>,
    /// The box a layer's anchor refers to, in image pixels. For text it is the
    /// layout box (the advance width, and the line boxes or the visible rows
    /// of a single line), which a stroke can reach past, so the image may be
    /// larger. For everything else it is the whole image.
    anchor_box: AnchorBox,
}

/// A rectangle in image pixels; see [`RasterizedText::anchor_in_image`].
#[derive(Clone, Copy, Debug, PartialEq)]
struct AnchorBox {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
}

impl RasterizedText {
    /// An image whose anchor box is the whole image.
    pub(crate) fn whole(width: u32, height: u32, baseline: f32, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            baseline,
            pixels,
            anchor_box: AnchorBox {
                left: 0,
                top: 0,
                width,
                height,
            },
        }
    }

    /// Converts an anchor normalized to the anchor box (what a layer's
    /// `anchor` means) into one normalized to the whole image, which is what
    /// placing the image needs. The identity when the two are the same.
    pub fn anchor_in_image(&self, x: f64, y: f64) -> (f64, f64) {
        let AnchorBox {
            left,
            top,
            width,
            height,
        } = self.anchor_box;
        (
            (f64::from(left) + x * f64::from(width)) / f64::from(self.width),
            (f64::from(top) + y * f64::from(height)) / f64::from(self.height),
        )
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn baseline(&self) -> f32 {
        self.baseline
    }

    /// `baseline` as a fraction of `height`: the normalized anchor `y` that
    /// pins the first line's baseline.
    pub fn baseline_anchor(&self) -> f64 {
        f64::from(self.baseline) / f64::from(self.height)
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }
}

/// Size of laid-out text, in composition units.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextMetrics {
    /// Widest line's advance width.
    pub width: f64,
    /// Total height of all lines.
    pub height: f64,
    /// First line's top edge to its baseline.
    pub ascent: f64,
    /// First line's baseline to its bottom edge (`ascent + descent` is `line_height`).
    pub descent: f64,
    pub line_height: f64,
    pub lines: usize,
    pub glyphs: Vec<GlyphMetrics>,
}

/// One shaped glyph cluster: its text, left edge within its line, and advance.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphMetrics {
    pub text: String,
    pub x: f64,
    pub width: f64,
    /// Zero-based line the cluster sits on.
    pub line: usize,
}

/// A `Text` layer whose `fontFamily` has no loaded or installed face, so it
/// is drawn with a fallback font instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFallback {
    pub layer: String,
    pub family: String,
    pub weight: u16,
}

impl fmt::Display for FontFallback {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "font family \"{}\" (weight {}) is not installed or loaded; text layer \"{}\" uses a fallback font",
            self.family, self.weight, self.layer
        )
    }
}

/// Characters of a `Text` layer that its `fontFamily` has no glyph for, so
/// they are drawn with another font (or as a missing-glyph box) while the
/// rest of the text uses the family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingGlyphs {
    pub layer: String,
    pub family: String,
    pub weight: u16,
    /// Each missing character once, in the order they first appear.
    pub characters: Vec<char>,
}

impl MissingGlyphs {
    /// How many characters the message spells out before summarizing the rest.
    const LISTED: usize = 10;
}

impl fmt::Display for MissingGlyphs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let listed = self
            .characters
            .iter()
            .take(Self::LISTED)
            .collect::<String>();
        write!(
            formatter,
            "font family \"{}\" (weight {}) has no glyph for \"{listed}\"",
            self.family, self.weight
        )?;
        if self.characters.len() > Self::LISTED {
            let more = self.characters.len() - Self::LISTED;
            write!(formatter, " and {more} more characters")?;
        }
        write!(
            formatter,
            "; text layer \"{}\" draws them with a fallback font",
            self.layer
        )
    }
}

pub struct TextRasterizer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    loaded_fonts: HashSet<PathBuf>,
    /// `matched_weight` results by family and requested weight, cleared
    /// whenever a font is loaded.
    matched_weights: HashMap<(String, u16), Option<u16>>,
    /// `color_emoji_family`'s result once looked up, cleared whenever a
    /// font is loaded.
    color_emoji_family: Option<Option<String>>,
    /// `missing_characters` results by text, family, and weight, cleared
    /// whenever a font is loaded.
    missing_characters: HashMap<(String, String, u16), Vec<char>>,
}

/// Families of color emoji fonts, most preferred first: the ones macOS and
/// Windows ship, then the ones Linux distributions and apps commonly carry.
const COLOR_EMOJI_FAMILIES: &[&str] = &[
    "Apple Color Emoji",
    "Segoe UI Emoji",
    "Noto Color Emoji",
    "Twemoji Mozilla",
    "Twemoji",
    "Twitter Color Emoji",
    "JoyPixels",
    "EmojiOne Color",
];

impl TextRasterizer {
    pub fn new() -> Self {
        let font_system = FontSystem::new();
        #[cfg(target_os = "windows")]
        let font_system = {
            let mut font_system = font_system;
            load_directwrite_system_fonts(&mut font_system);
            font_system
        };

        Self {
            font_system,
            swash_cache: SwashCache::new(),
            loaded_fonts: HashSet::new(),
            matched_weights: HashMap::new(),
            color_emoji_family: None,
            missing_characters: HashMap::new(),
        }
    }

    /// How many font files have been loaded so far. It only grows, so a
    /// caller caching rasterized text can compare it across frames to tell
    /// when a newly loaded font may change how existing text lays out.
    pub fn loaded_font_count(&self) -> usize {
        self.loaded_fonts.len()
    }

    pub fn load_fonts(
        &mut self,
        fonts: &[ResolvedAsset],
        asset_root: &Path,
    ) -> Result<(), RenderError> {
        for font in fonts {
            // Keyed by the resolved file, not the id: a React `<Font>` keeps
            // its `name` when its `src` changes across a reload.
            let path = local_asset_path(asset_root, font)?;
            if self.loaded_fonts.contains(&path) {
                continue;
            }
            let data = read_asset(&font.id, &path)?;
            if !is_sfnt_or_woff(&data) && celesta_remote::is_font_stylesheet(&data) {
                // A web font stylesheet (e.g. Google Fonts): load every face
                // its `@font-face` rules point to, under their CSS family too.
                let css = String::from_utf8_lossy(&data);
                let faces = celesta_remote::stylesheet_font_faces(&css, &font.location);
                if faces.is_empty() {
                    return Err(RenderError::InvalidFont {
                        asset: font.id.clone(),
                        reason: "the stylesheet has no @font-face url()",
                    });
                }
                for face in faces {
                    let face_asset = ResolvedAsset {
                        id: font.id.clone(),
                        location: face.location,
                    };
                    let face_path = local_asset_path(asset_root, &face_asset)?;
                    if !self.loaded_fonts.contains(&face_path) {
                        let face_data = read_asset(&font.id, &face_path)?;
                        self.load_font_data(&font.id, face_data, face.family)?;
                        self.loaded_fonts.insert(face_path);
                    }
                }
            } else {
                self.load_font_data(&font.id, data, None)?;
            }
            self.loaded_fonts.insert(path);
        }
        Ok(())
    }

    /// Loads a TrueType/OpenType font, or a WOFF/WOFF2 one after unpacking it.
    /// `alias` adds a family name the faces also match, besides the ones
    /// stored in the file.
    fn load_font_data(
        &mut self,
        asset: &str,
        data: Vec<u8>,
        alias: Option<String>,
    ) -> Result<(), RenderError> {
        let invalid = |reason| RenderError::InvalidFont {
            asset: asset.to_owned(),
            reason,
        };
        let data = match data.get(..4) {
            Some(b"wOFF") => {
                wuff::decompress_woff1(&data).map_err(|_| invalid("invalid WOFF data"))?
            }
            Some(b"wOF2") => {
                wuff::decompress_woff2(&data).map_err(|_| invalid("invalid WOFF2 data"))?
            }
            _ => data,
        };
        let database = self.font_system.db_mut();
        let ids = database.load_font_source(fontdb::Source::Binary(Arc::new(data)));
        if ids.is_empty() {
            return Err(invalid("no font faces found"));
        }
        self.matched_weights.clear();
        self.color_emoji_family = None;
        self.missing_characters.clear();
        let Some(alias) = alias else {
            return Ok(());
        };
        for id in ids {
            let Some(mut face) = database.face(id).cloned() else {
                continue;
            };
            if face.families.iter().any(|(family, _)| *family == alias) {
                continue;
            }
            face.families
                .push((alias.clone(), fontdb::Language::English_UnitedStates));
            database.remove_face(id);
            database.push_face_info(face);
        }
        Ok(())
    }

    /// The weight of the face that CSS font matching (CSS Fonts §5.2) picks
    /// from `family` for `requested`: `requested` itself when the family has
    /// a face of that weight, otherwise the nearest one it has. `None` when
    /// no face of `family` is loaded or installed.
    fn matched_weight(&mut self, family: &str, requested: u16) -> Option<u16> {
        let database = self.font_system.db();
        *self
            .matched_weights
            .entry((family.to_owned(), requested))
            .or_insert_with(|| {
                let id = database.query(&fontdb::Query {
                    families: &[Family::Name(family)],
                    weight: Weight(requested),
                    ..fontdb::Query::default()
                })?;
                Some(database.face(id)?.weight.0)
            })
    }

    /// The fallback `style` is drawn with on `layer` when its `fontFamily`
    /// has no loaded or installed face; `None` when it has one or names no
    /// family.
    pub fn font_fallback(&mut self, layer: &str, style: &TextStyle) -> Option<FontFallback> {
        let family = style.font_family.as_deref()?;
        let weight = style.font_weight.unwrap_or(400);
        self.matched_weight(family, weight)
            .is_none()
            .then(|| FontFallback {
                layer: layer.to_owned(),
                family: family.to_owned(),
                weight,
            })
    }

    /// The first of [`COLOR_EMOJI_FAMILIES`] with a loaded or installed face.
    fn color_emoji_family(&mut self) -> Option<String> {
        let database = self.font_system.db();
        self.color_emoji_family
            .get_or_insert_with(|| {
                COLOR_EMOJI_FAMILIES
                    .iter()
                    .find(|family| {
                        database
                            .faces()
                            .any(|face| face.families.iter().any(|(name, _)| name == *family))
                    })
                    .map(|family| (*family).to_owned())
            })
            .clone()
    }

    /// The characters of `text` that `style`'s `fontFamily` has no glyph
    /// for, drawn on `layer` with another font instead; `None` when the
    /// family draws all of them, names no family, or has no face at all
    /// (which [`Self::font_fallback`] reports). Emoji that another font
    /// draws are left out, since they are meant to come from a color emoji
    /// font, and so are whitespace and invisible characters. Characters no
    /// font has, emoji included, are drawn as a missing-glyph box and
    /// always reported.
    pub fn missing_glyphs(
        &mut self,
        layer: &str,
        text: &str,
        style: &TextStyle,
    ) -> Option<MissingGlyphs> {
        let family = style.font_family.as_deref()?;
        let weight = style.font_weight.unwrap_or(400);
        self.matched_weight(family, weight)?;
        let key = (text.to_owned(), family.to_owned(), weight);
        let characters = match self.missing_characters.get(&key) {
            Some(characters) => characters.clone(),
            None => {
                let characters = self.missing_characters(text, style, family);
                // Text that changes every frame (a counter, a subtitle)
                // would otherwise grow this without bound.
                if self.missing_characters.len() >= 4096 {
                    self.missing_characters.clear();
                }
                self.missing_characters.insert(key, characters.clone());
                characters
            }
        };
        (!characters.is_empty()).then(|| MissingGlyphs {
            layer: layer.to_owned(),
            family: family.to_owned(),
            weight,
            characters,
        })
    }

    fn missing_characters(&mut self, text: &str, style: &TextStyle, family: &str) -> Vec<char> {
        // The font each character is drawn with does not depend on the
        // wrap width or the size, so shape on one unwrapped line at scale 1.
        let buffer = self.shaped_buffer(text, style, None, 1.0);
        let database = self.font_system.db();
        let mut characters = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                // Glyph 0 is `.notdef`: no font had the character, and the
                // missing-glyph box is drawn.
                let drawn = glyph.glyph_id != 0;
                let from_family = drawn
                    && database
                        .face(glyph.font_id)
                        .is_some_and(|face| face.families.iter().any(|(name, _)| name == family));
                let Some(cluster) = run.text.get(glyph.start..glyph.end) else {
                    continue;
                };
                // An emoji drawn from a color emoji font is expected; one no
                // font has is a box like any other missing character.
                if from_family || (drawn && is_emoji_cluster(cluster)) {
                    continue;
                }
                for character in cluster.chars().filter(|&c| is_visible_character(c)) {
                    if !characters.contains(&character) {
                        characters.push(character);
                    }
                }
            }
        }
        characters
    }

    /// Shapes `text` into a laid-out buffer. `width` and `scale` are in
    /// output pixels: `scale` multiplies the style's font size and line height.
    fn shaped_buffer(
        &mut self,
        text: &str,
        style: &TextStyle,
        width: Option<f32>,
        scale: f32,
    ) -> Buffer {
        let requested_weight = style.font_weight.unwrap_or(400);
        // cosmic-text only picks the requested family's face when its weight
        // is exactly the requested one, and otherwise falls back to another
        // family. Ask for the weight CSS matching picks within the family
        // instead, so a family loaded only in Bold still draws a 400 request.
        let weight = style
            .font_family
            .as_deref()
            .and_then(|family| self.matched_weight(family, requested_weight))
            .unwrap_or(requested_weight);
        let font_size = style.font_size.unwrap_or(32.0) as f32 * scale;
        let line_height = style
            .line_height
            .map(|line_height| line_height as f32 * scale)
            .unwrap_or(font_size * 1.2);
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
        buffer.set_size(&mut self.font_system, width, None);
        buffer.set_wrap(&mut self.font_system, Wrap::Word);

        let mut attrs = Attrs::new().weight(Weight(weight));
        if let Some(family) = style.font_family.as_deref() {
            attrs = attrs.family(Family::Name(family));
        }
        if let Some(letter_spacing) = style.letter_spacing
            && style.font_size.unwrap_or(32.0) > 0.0
        {
            // cosmic-text takes tracking in em.
            attrs = attrs.letter_spacing((letter_spacing / style.font_size.unwrap_or(32.0)) as f32);
        }
        let alignment = style.align.map(|align| match align {
            TextAlign::Left => Align::Left,
            TextAlign::Center => Align::Center,
            TextAlign::Right => Align::Right,
        });
        // cosmic-text falls back per character to the first font with a
        // glyph, trying the color emoji font only after text fonts, so an
        // emoji that a text font also has (❤️, a keycap, a flag's letters)
        // came out as a plain glyph. Ask for the color emoji font first for
        // the graphemes meant to look like emoji, unless the family asked
        // for is a color emoji font itself.
        let emoji_family = if style
            .font_family
            .as_deref()
            .is_some_and(|family| COLOR_EMOJI_FAMILIES.contains(&family))
            || emoji_presentation_spans(text).is_empty()
        {
            None
        } else {
            self.color_emoji_family()
        };
        buffer.set_text(
            &mut self.font_system,
            text,
            &attrs,
            Shaping::Advanced,
            alignment,
        );
        if let Some(emoji_family) = emoji_family {
            let emoji_weight = self
                .matched_weight(&emoji_family, requested_weight)
                .unwrap_or(requested_weight);
            let emoji_attrs = attrs
                .clone()
                .family(Family::Name(&emoji_family))
                .weight(Weight(emoji_weight));
            // Added to the lines `set_text` made rather than passed to
            // `set_rich_text`, which splits lines differently (dropping
            // the empty line after a trailing newline).
            for line in &mut buffer.lines {
                let spans = emoji_presentation_spans(line.text());
                if spans.is_empty() {
                    continue;
                }
                let mut attrs_list = line.attrs_list().clone();
                for range in spans {
                    attrs_list.add_span(range, &emoji_attrs);
                }
                line.set_attrs_list(attrs_list);
            }
        }
        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer
    }

    /// Measures `text` in composition units (scale 1) without drawing it,
    /// laid out exactly as [`Self::rasterize`] would.
    pub fn measure(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
    ) -> TextMetrics {
        let buffer = self.shaped_buffer(text, style, max_width.map(|w| w as f32), 1.0);
        let mut metrics = TextMetrics::default();
        for run in buffer.layout_runs() {
            metrics.width = metrics.width.max(f64::from(run.line_w));
            metrics.height = metrics
                .height
                .max(f64::from(run.line_top + run.line_height));
            if metrics.lines == 0 {
                metrics.ascent = f64::from(run.line_y - run.line_top);
                metrics.descent = f64::from(run.line_height) - metrics.ascent;
                metrics.line_height = f64::from(run.line_height);
            }
            metrics.lines += 1;
            // Cluster-wise: a ligature or combined glyph has one entry.
            for glyph in run.glyphs {
                let Some(cluster) = run.text.get(glyph.start..glyph.end) else {
                    continue;
                };
                metrics.glyphs.push(GlyphMetrics {
                    text: cluster.to_owned(),
                    x: f64::from(glyph.x),
                    width: f64::from(glyph.w),
                    line: metrics.lines - 1,
                });
            }
        }
        metrics
    }

    pub fn rasterize(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        scale: f32,
    ) -> Result<RasterizedText, RenderError> {
        let scale = scale.abs();
        let width = max_width.map(|width| width as f32 * scale);
        let buffer = self.shaped_buffer(text, style, width, scale);

        let measured_width = buffer
            .layout_runs()
            .fold(0.0_f32, |width, run| width.max(run.line_w));
        let measured_height = buffer.layout_runs().fold(0.0_f32, |height, run| {
            height.max(run.line_top + run.line_height)
        });
        let baseline = buffer.layout_runs().next().map_or(0.0, |run| run.line_y);
        let layout_width = width.unwrap_or(measured_width).ceil().max(1.0) as u32;
        let layout_height = measured_height.ceil().max(1.0) as u32;
        // The stroke grows the glyphs by its width in every direction, which
        // reaches past the layout box at the first and last glyph (and above
        // and below tall or low glyphs), so leave that much room around it.
        let stroke_radius = style.stroke.as_ref().map_or(0, |stroke| {
            (stroke.width * f64::from(scale)).round().max(0.0) as u32
        });
        let pad = stroke_radius;
        let mask_width = layout_width + 2 * pad;
        let mask_height = layout_height + 2 * pad;
        let fill_paint =
            resolve_paint(style.fill.as_ref())?.map(|paint| paint.scaled(f64::from(scale)));
        // A gradient is painted over white glyphs below.
        let fill = fill_paint
            .as_ref()
            .map_or(Some(Color::WHITE), ResolvedPaint::solid)
            .unwrap_or(Color::WHITE);
        // A coverage-only alpha mask, used for the stroke's dilation below —
        // meaningful for both ordinary glyphs and color glyphs (an emoji's
        // silhouette dilates the same way plain text would).
        let mut mask = vec![0_u8; mask_width as usize * mask_height as usize];
        // The actual per-pixel glyph color. cosmic-text/swash already decode
        // color glyphs (COLR, sbix, CBDT/CBLC — how system emoji fonts like
        // Apple Color Emoji store their glyphs) into real per-pixel RGBA
        // here, not just a coverage mask; passing `fill` as the base color
        // below means an ordinary (non-color) glyph's pixels come back as
        // `fill` scaled by coverage, so accumulating directly into this
        // buffer reproduces flat-fill text exactly while also capturing
        // multi-color emoji glyphs, which a single mask+solid-fill
        // composite cannot represent.
        let mut glyph_pixels = vec![0_u8; mask_width as usize * mask_height as usize * 4];
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            CosmicColor::rgba(fill.red, fill.green, fill.blue, fill.alpha),
            |x, y, width, height, color| {
                for offset_y in 0..height as i32 {
                    for offset_x in 0..width as i32 {
                        let pixel_x = x + offset_x + pad as i32;
                        let pixel_y = y + offset_y + pad as i32;
                        if pixel_x < 0
                            || pixel_y < 0
                            || pixel_x >= mask_width as i32
                            || pixel_y >= mask_height as i32
                        {
                            continue;
                        }
                        let offset = pixel_y as usize * mask_width as usize + pixel_x as usize;
                        mask[offset] = mask[offset].max(color.a());
                        let pixel_offset = offset * 4;
                        blend(
                            &mut glyph_pixels[pixel_offset..pixel_offset + 4],
                            Color::rgba(color.r(), color.g(), color.b(), color.a()),
                            1.0,
                        );
                    }
                }
            },
        );

        if let Some(gradient) = fill_paint.as_ref().filter(|paint| paint.solid().is_none()) {
            // Plain glyph pixels come back as opaque-white scaled by coverage;
            // anything with color (an emoji) keeps its own.
            for (index, pixel) in glyph_pixels.chunks_exact_mut(4).enumerate() {
                if pixel[3] == 0 || pixel[..3] != [255, 255, 255] {
                    continue;
                }
                // Gradient coordinates are relative to the layout box.
                let x = f64::from((index % mask_width as usize) as u32) - f64::from(pad) + 0.5;
                let y = f64::from((index / mask_width as usize) as u32) - f64::from(pad) + 0.5;
                let color = gradient.color_at(x, y);
                pixel[..3].copy_from_slice(&[color.red, color.green, color.blue]);
                pixel[3] = (f64::from(pixel[3]) * f64::from(color.alpha) / 255.0).round() as u8;
            }
        }

        let pixel_count = mask_width as usize * mask_height as usize * 4;
        let mut frame = RgbaFrame {
            width: mask_width,
            height: mask_height,
            pixels: vec![0; pixel_count],
        };
        if let Some(stroke) = &style.stroke
            && stroke_radius > 0
        {
            let stroke_mask = dilate_mask(&mask, mask_width, mask_height, stroke_radius);
            let stroke_paint = ResolvedPaint::from_paint(&stroke.paint)?.scaled(f64::from(scale));
            composite_mask_with(&mut frame, &stroke_mask, mask_width, |x, y| {
                stroke_paint.color_at(
                    f64::from(x) - f64::from(pad) + 0.5,
                    f64::from(y) - f64::from(pad) + 0.5,
                )
            });
        }
        composite_rgba(&mut frame, &glyph_pixels, mask_width, mask_height, 0, 0);
        // Single-line text keeps its advance width, so leading and trailing
        // spaces still take up room, but drops the empty rows above and below
        // its ink: `anchorY` 0.5 centers the letters, not the line box.
        let mut top = 0;
        let single_line = !text.contains('\n');
        if single_line {
            (frame, top) = trim_transparent_rows(frame);
        }
        // A single line is anchored by its visible rows, stroke included;
        // several lines by their line boxes.
        let anchor_box = if single_line {
            AnchorBox {
                left: pad,
                top: 0,
                width: layout_width,
                height: frame.height,
            }
        } else {
            AnchorBox {
                left: pad,
                top: pad,
                width: layout_width,
                height: layout_height,
            }
        };
        Ok(RasterizedText {
            width: frame.width,
            height: frame.height,
            baseline: baseline + pad as f32 - top as f32,
            pixels: frame.pixels,
            anchor_box,
        })
    }
}

#[cfg(target_os = "windows")]
fn load_directwrite_system_fonts(font_system: &mut FontSystem) {
    use cosmic_text::fontdb::Source;

    let mut loaded_paths = font_system
        .db()
        .faces()
        .filter_map(|face| match &face.source {
            Source::File(path) | Source::SharedFile(path, _) => Some(path.clone()),
            Source::Binary(_) => None,
        })
        .collect::<HashSet<_>>();

    for family in dwrote::FontCollection::get_system(true).families_iter() {
        for index in 0..family.get_font_count() {
            let Ok(font) = family.font(index) else {
                continue;
            };
            let Ok(files) = font.create_font_face().files() else {
                continue;
            };

            for file in files {
                if let Ok(path) = file.font_file_path() {
                    if loaded_paths.insert(path.clone()) {
                        let _ = font_system.db_mut().load_font_file(path);
                    }
                } else if let Ok(bytes) = file.font_file_bytes() {
                    font_system.db_mut().load_font_data(bytes);
                }
            }
        }
    }

    // Adobe keeps synced fonts outside DirectWrite's system collection. Validate
    // its extensionless cache files with DirectWrite before adding them to fontdb.
    let Some(app_data) = std::env::var_os("APPDATA") else {
        return;
    };
    let directory = PathBuf::from(app_data).join("Adobe/CoreSync/plugins/livetype/r");
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if loaded_paths.insert(path.clone()) && dwrote::FontFile::new_from_path(&path).is_some() {
            let _ = font_system.db_mut().load_font_file(path);
        }
    }
}

/// Crops the fully transparent rows above and below the ink, then pads one
/// empty row on top when needed to keep the height even. Returns the frame and
/// how many rows its top edge moved down (negative when padded).
fn trim_transparent_rows(frame: RgbaFrame) -> (RgbaFrame, i32) {
    let row_bytes = frame.width as usize * 4;
    let visible = |row: &[u8]| row.chunks_exact(4).any(|pixel| pixel[3] != 0);
    let rows = || frame.pixels.chunks_exact(row_bytes);
    let (Some(min_y), Some(from_bottom)) =
        (rows().position(visible), rows().rev().position(visible))
    else {
        return (frame, 0);
    };
    let max_y = frame.height as usize - 1 - from_bottom;
    let height = max_y - min_y + 1;
    let pad_top = height % 2;
    if height == frame.height as usize && pad_top == 0 {
        return (frame, 0);
    }
    let mut pixels = vec![0; pad_top * row_bytes];
    pixels.extend_from_slice(&frame.pixels[min_y * row_bytes..(max_y + 1) * row_bytes]);
    (
        RgbaFrame {
            width: frame.width,
            height: (height + pad_top) as u32,
            pixels,
        },
        min_y as i32 - pad_top as i32,
    )
}

impl Default for TextRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

/// The byte ranges of `text`'s graphemes that are meant to be drawn as
/// emoji, with runs of adjacent ones merged: an emoji that is one by default
/// (Emoji_Presentation, which covers flags' regional indicators), or any
/// character followed by the emoji presentation selector U+FE0F (❤️, 1️⃣).
/// The text presentation selector U+FE0E keeps a grapheme text.
fn emoji_presentation_spans(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut spans: Vec<std::ops::Range<usize>> = Vec::new();
    for (start, grapheme) in text.grapheme_indices(true) {
        // Any character of the grapheme, not only the first: one may start
        // with a prepended character (U+0600 before an emoji, say).
        let emoji = !grapheme.contains('\u{FE0E}')
            && (grapheme.contains('\u{FE0F}')
                || grapheme.chars().any(|character| {
                    matches!(
                        character.emoji_status(),
                        EmojiStatus::EmojiPresentation
                            | EmojiStatus::EmojiPresentationAndModifierBase
                            | EmojiStatus::EmojiPresentationAndEmojiComponent
                            | EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
                    )
                }));
        if !emoji {
            continue;
        }
        let end = start + grapheme.len();
        match spans.last_mut() {
            Some(span) if span.end == start => span.end = end,
            _ => spans.push(start..end),
        }
    }
    spans
}

/// Whether a shaped cluster is (part of) an emoji, which is meant to be
/// drawn with a color emoji font rather than the layer's family.
fn is_emoji_cluster(cluster: &str) -> bool {
    cluster.chars().any(|character| {
        matches!(
            character,
            // Zero width joiner, combining keycap, emoji presentation
            // selector, and emoji tag characters.
            '\u{200D}' | '\u{20E3}' | '\u{FE0F}' | '\u{E0020}'..='\u{E007F}'
        ) || matches!(
            character.emoji_status(),
            EmojiStatus::EmojiPresentation
                | EmojiStatus::EmojiPresentationAndModifierBase
                | EmojiStatus::EmojiPresentationAndEmojiComponent
                | EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
        )
    })
}

/// Whether `character` draws something by itself: not whitespace, a control
/// or format character, or a variation selector.
fn is_visible_character(character: char) -> bool {
    !character.is_whitespace()
        && !character.is_control()
        && character.general_category() != GeneralCategory::Format
        && !matches!(character, '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
}

pub struct CpuRenderer {
    options: RenderOptions,
    asset_root: PathBuf,
    text_rasterizer: TextRasterizer,
    psd_sources: psd_source::PsdSources,
    image_sources: image_source::ImageSources,
    video_decoder: Option<Box<dyn VideoFrameDecoder>>,
}

impl CpuRenderer {
    pub fn new(options: RenderOptions) -> Self {
        Self {
            options,
            asset_root: PathBuf::from("."),
            text_rasterizer: TextRasterizer::new(),
            psd_sources: Default::default(),
            image_sources: Default::default(),
            video_decoder: None,
        }
    }

    pub fn with_asset_root(mut self, asset_root: impl Into<PathBuf>) -> Self {
        self.asset_root = asset_root.into();
        self
    }

    pub fn with_video_decoder(mut self, decoder: impl VideoFrameDecoder + 'static) -> Self {
        self.video_decoder = Some(Box::new(decoder));
        self
    }

    pub const fn options(&self) -> RenderOptions {
        self.options
    }

    pub fn render(&mut self, scene: &Scene) -> Result<RgbaFrame, RenderError> {
        self.text_rasterizer
            .load_fonts(&scene.fonts, &self.asset_root)?;
        let pixel_count = u64::from(scene.width)
            .checked_mul(u64::from(scene.height))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or(RenderError::SurfaceTooLarge {
                width: scene.width,
                height: scene.height,
            })?;
        let mut frame = RgbaFrame {
            width: scene.width,
            height: scene.height,
            pixels: vec![0; pixel_count],
        };
        for pixel in frame.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[
                self.options.background.red,
                self.options.background.green,
                self.options.background.blue,
                self.options.background.alpha,
            ]);
        }

        for layer in &scene.layers {
            self.render_layer(&mut frame, layer, ParentState::default())?;
        }
        Ok(frame)
    }

    fn render_layer(
        &mut self,
        frame: &mut RgbaFrame,
        layer: &Layer,
        parent: ParentState,
    ) -> Result<(), RenderError> {
        if layer.transform.rotation != 0.0 {
            return Err(RenderError::UnsupportedRotation {
                layer: layer.id.clone(),
                degrees: layer.transform.rotation,
            });
        }
        let state = ParentState {
            position: Point {
                x: parent.position.x + layer.transform.position.x * parent.scale.x,
                y: parent.position.y + layer.transform.position.y * parent.scale.y,
            },
            scale: Point {
                x: parent.scale.x * layer.transform.scale.x,
                y: parent.scale.y * layer.transform.scale.y,
            },
            opacity: (parent.opacity * layer.opacity).clamp(0.0, 1.0),
            clip: parent.clip.clone(),
            blend_mode: layer.blend_mode,
        };
        if state.opacity == 0.0 || state.scale.x == 0.0 || state.scale.y == 0.0 {
            return Ok(());
        }

        if !layer.effects.is_empty() {
            return self.render_effect_layer(frame, layer, parent, state);
        }

        match &layer.content {
            LayerContent::Text {
                text,
                style,
                max_width,
                baseline_anchor,
            } => self.render_text(
                frame,
                text,
                style,
                *max_width,
                layer.transform.anchor,
                *baseline_anchor,
                &state,
            )?,
            LayerContent::Group { layers, clip } => {
                let mut child_state = state.clone();
                if let Some(clip) = clip {
                    if clip.is_empty() {
                        return Ok(());
                    }
                    child_state.clip = Some(Arc::new(ClipNode {
                        region: ClipRegion::new(clip, state.position, state.scale),
                        parent: state.clip.clone(),
                    }));
                }
                if !layer.blend_mode.is_normal() {
                    // Isolated: the children composite onto a transparent layer
                    // of their own, which then blends with the backdrop as one.
                    // The children carry the clip; the layer itself does not.
                    let mut isolated = RgbaFrame {
                        width: frame.width,
                        height: frame.height,
                        pixels: vec![0; frame.pixels.len()],
                    };
                    let inner = ParentState {
                        opacity: 1.0,
                        blend_mode: BlendMode::Normal,
                        ..child_state
                    };
                    for child in layers {
                        self.render_layer(&mut isolated, child, inner.clone())?;
                    }
                    for (destination, source) in frame
                        .pixels
                        .chunks_exact_mut(4)
                        .zip(isolated.pixels.chunks_exact(4))
                    {
                        let source = Color::rgba(source[0], source[1], source[2], source[3]);
                        blend_with_mode(destination, source, state.opacity, state.blend_mode);
                    }
                    return Ok(());
                }
                // Not isolated: each child composites straight onto the
                // backdrop, through its own blend mode.
                for child in layers {
                    self.render_layer(frame, child, child_state.clone())?;
                }
            }
            LayerContent::Video { asset, timing } => {
                if self.video_decoder.is_some() {
                    let image = self.decode_video_frame(&layer.id, asset, timing)?;
                    render_image(frame, &image, layer.transform.anchor, &state);
                } else {
                    render_placeholder(
                        frame,
                        layer.transform.anchor,
                        &state,
                        Color::rgba(54, 98, 176, 255),
                        320.0,
                        180.0,
                    );
                }
            }
            LayerContent::Image {
                asset,
                width,
                height,
                fit,
            } => {
                let path = self.local_asset_path(asset)?;
                let display = self
                    .image_sources
                    .render(
                        &asset.id,
                        &path,
                        *width,
                        *height,
                        *fit,
                        state.scale.x.abs().max(state.scale.y.abs()),
                    )
                    .map_err(|source| RenderError::ImageDecode {
                        asset: asset.id.clone(),
                        source,
                    })?;
                let mut state = state.clone();
                state.scale.x *= display.width / display.pixels.width() as f64;
                state.scale.y *= display.height / display.pixels.height() as f64;
                render_image_pixels(
                    frame,
                    display.pixels.width(),
                    display.pixels.height(),
                    display.pixels.as_raw(),
                    layer.transform.anchor,
                    &state,
                );
            }
            LayerContent::Psd {
                asset,
                visible_layers,
                enabled_layers,
                disabled_layers,
            } => {
                let path = self.local_asset_path(asset)?;
                let image = self.psd_sources.render(
                    &asset.id,
                    &path,
                    visible_layers,
                    enabled_layers,
                    disabled_layers,
                    state.scale.x.abs().max(state.scale.y.abs()),
                )?;
                let mut state = state.clone();
                state.scale.x *= f64::from(image.canvas_width) / f64::from(image.width);
                state.scale.y *= f64::from(image.canvas_height) / f64::from(image.height);
                render_image_pixels(
                    frame,
                    image.width,
                    image.height,
                    &image.pixels,
                    layer.transform.anchor,
                    &state,
                );
            }
            LayerContent::Rect {
                width,
                height,
                fill,
                stroke,
                corner_radius,
            } => {
                let image = rasterize_rect(
                    *width,
                    *height,
                    *corner_radius,
                    fill.as_ref(),
                    stroke.as_ref(),
                )?;
                render_image(
                    frame,
                    &DecodedImage {
                        width: image.width(),
                        height: image.height(),
                        pixels: image.into_pixels(),
                    },
                    layer.transform.anchor,
                    &state,
                );
            }
            LayerContent::Path {
                commands,
                fill,
                stroke,
                line_cap,
                line_join,
                miter_limit,
            } => {
                let shape = PathShape {
                    commands,
                    fill: fill.as_ref(),
                    stroke: stroke.as_ref(),
                    line_cap: *line_cap,
                    line_join: *line_join,
                    miter_limit: *miter_limit,
                };
                let transform = PathTransform::scale_translate(
                    state.scale.x,
                    state.scale.y,
                    state.position.x,
                    state.position.y,
                );
                if let Some(path) = rasterize_path(&shape, transform, frame.width, frame.height)? {
                    // Already in output pixels: drawn unscaled at its corner.
                    let state = ParentState {
                        position: Point {
                            x: f64::from(path.left),
                            y: f64::from(path.top),
                        },
                        scale: Point { x: 1.0, y: 1.0 },
                        ..state
                    };
                    render_image_pixels(
                        frame,
                        path.image.width,
                        path.image.height,
                        &path.image.pixels,
                        Point { x: 0.0, y: 0.0 },
                        &state,
                    );
                }
            }
            LayerContent::MissingComponent { .. } => render_placeholder(
                frame,
                layer.transform.anchor,
                &state,
                Color::rgba(220, 125, 42, 255),
                240.0,
                120.0,
            ),
        }
        Ok(())
    }

    fn render_effect_layer(
        &mut self,
        frame: &mut RgbaFrame,
        layer: &Layer,
        parent: ParentState,
        state: ParentState,
    ) -> Result<(), RenderError> {
        let mut source = RgbaFrame {
            width: frame.width,
            height: frame.height,
            pixels: vec![0; frame.pixels.len()],
        };
        let mut inner = layer.clone();
        inner.opacity = 1.0;
        inner.blend_mode = BlendMode::Normal;
        inner.effects = Default::default();
        self.render_layer(
            &mut source,
            &inner,
            ParentState {
                opacity: 1.0,
                ..parent.clone()
            },
        )?;

        let mut result = RgbaFrame {
            width: frame.width,
            height: frame.height,
            pixels: vec![0; frame.pixels.len()],
        };
        if let Some(shadow) = &layer.effects.shadow {
            render_effect_shadow(
                &mut result,
                &source,
                &shadow.color,
                shadow.blur,
                shadow.offset_x,
                shadow.offset_y,
            )?;
        }
        if let Some(glow) = &layer.effects.glow {
            render_effect_shadow(&mut result, &source, &glow.color, glow.blur, 0.0, 0.0)?;
        }
        let source_pixels = if layer.effects.blur > 0.0 {
            blur_pixels(&source, layer.effects.blur)
        } else {
            premultiply_pixels(&source.pixels)
        };
        for (destination, source) in result
            .pixels
            .chunks_exact_mut(4)
            .zip(source_pixels.chunks_exact(4))
        {
            blend(destination, unpremultiply_color(source), 1.0);
        }
        for (index, (destination, source)) in frame
            .pixels
            .chunks_exact_mut(4)
            .zip(result.pixels.chunks_exact(4))
            .enumerate()
        {
            let x = (index as u32 % frame.width) as i32;
            let y = (index as u32 / frame.width) as i32;
            let coverage = clip_coverage(&parent.clip, x, y);
            blend_with_mode(
                destination,
                Color::rgba(source[0], source[1], source[2], source[3]),
                state.opacity * coverage,
                state.blend_mode,
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn render_text(
        &mut self,
        frame: &mut RgbaFrame,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        mut anchor: Point,
        baseline_anchor: bool,
        state: &ParentState,
    ) -> Result<(), RenderError> {
        if (state.scale.x.abs() - state.scale.y.abs()).abs() > f64::EPSILON {
            return Err(RenderError::UnsupportedNonUniformTextScale {
                x: state.scale.x,
                y: state.scale.y,
            });
        }
        let scale = state.scale.y.abs() as f32;
        let text = self
            .text_rasterizer
            .rasterize(text, style, max_width, scale)?;
        let (anchor_x, anchor_y) = text.anchor_in_image(anchor.x, anchor.y);
        anchor.x = anchor_x;
        anchor.y = if baseline_anchor {
            text.baseline_anchor()
        } else {
            anchor_y
        };
        let image = DecodedImage {
            width: text.width,
            height: text.height,
            pixels: text.pixels,
        };
        render_image(
            frame,
            &image,
            anchor,
            &ParentState {
                scale: Point { x: 1.0, y: 1.0 },
                ..state.clone()
            },
        );
        Ok(())
    }

    fn decode_video_frame(
        &mut self,
        request_id: &str,
        asset: &ResolvedAsset,
        timing: &MediaTiming,
    ) -> Result<DecodedImage, RenderError> {
        let path = self.local_asset_path(asset)?;
        let frame = self
            .video_decoder
            .as_mut()
            .expect("video decoder presence was checked")
            .decode_frame_for(request_id, &path, timing.source_time_seconds)?;
        Ok(DecodedImage {
            width: frame.width,
            height: frame.height,
            pixels: Arc::unwrap_or_clone(frame.pixels),
        })
    }

    fn local_asset_path(&self, asset: &ResolvedAsset) -> Result<PathBuf, RenderError> {
        local_asset_path(&self.asset_root, asset)
    }
}

fn read_asset(asset: &str, path: &Path) -> Result<Vec<u8>, RenderError> {
    fs::read(path).map_err(|source| RenderError::AssetIo {
        asset: asset.to_owned(),
        source,
    })
}

/// TrueType, OpenType, collection, WOFF, or WOFF2 magic.
fn is_sfnt_or_woff(data: &[u8]) -> bool {
    matches!(
        data.get(..4),
        Some(b"\0\x01\0\0" | b"OTTO" | b"true" | b"ttcf" | b"wOFF" | b"wOF2")
    )
}

fn local_asset_path(asset_root: &Path, asset: &ResolvedAsset) -> Result<PathBuf, RenderError> {
    resolve_asset_path(asset_root, &asset.location).map_err(|source| RenderError::RemoteAsset {
        asset: asset.id.clone(),
        source,
    })
}

/// Rasterizes a PSD portrait into a full-canvas RGBA frame, with layer
/// visibility resolved as [`psd_source::PsdSources::render`] describes.
pub fn rasterize_psd(
    asset: &str,
    path: &Path,
    visible_layers: &[String],
    enabled_layers: &[String],
    disabled_layers: &[String],
) -> Result<RgbaFrame, RenderError> {
    let image = psd_source::PsdSources::default().render(
        asset,
        path,
        visible_layers,
        enabled_layers,
        disabled_layers,
        1.0,
    )?;
    Ok(RgbaFrame {
        width: image.width,
        height: image.height,
        pixels: Arc::unwrap_or_clone(image.pixels),
    })
}

impl Default for CpuRenderer {
    fn default() -> Self {
        Self::new(RenderOptions::default())
    }
}

#[derive(Clone, Debug)]
struct ParentState {
    position: Point,
    scale: Point,
    opacity: f64,
    /// The innermost clip the layer is drawn through.
    clip: Option<Arc<ClipNode>>,
    /// The blend mode of the layer being drawn (not inherited by children).
    blend_mode: BlendMode,
}

impl Default for ParentState {
    fn default() -> Self {
        Self {
            position: Point { x: 0.0, y: 0.0 },
            scale: Point { x: 1.0, y: 1.0 },
            opacity: 1.0,
            clip: None,
            blend_mode: BlendMode::Normal,
        }
    }
}

/// One clip in the chain of clips a layer sits inside; a pixel is drawn
/// through all of them, so nested clips intersect.
#[derive(Debug)]
struct ClipNode {
    region: ClipRegion,
    parent: Option<Arc<ClipNode>>,
}

/// A group's clip rectangle with the group's frame it is defined in.
#[derive(Clone, Copy, Debug)]
struct ClipRegion {
    /// Canvas position of the group's origin.
    origin: Point,
    /// The group's accumulated scale.
    scale: Point,
    center: Point,
    half: Point,
    radius: f64,
    /// Local distance to canvas pixels, `sqrt(|scale.x * scale.y|)`: exact for
    /// a uniform scale, an approximation otherwise. `celesta-gpu-renderer`
    /// uses the same factor.
    distance_scale: f64,
}

impl ClipRegion {
    fn new(clip: &Clip, position: Point, scale: Point) -> Self {
        Self {
            origin: position,
            scale,
            center: Point {
                x: clip.x + clip.width / 2.0,
                y: clip.y + clip.height / 2.0,
            },
            half: Point {
                x: clip.width / 2.0,
                y: clip.height / 2.0,
            },
            radius: clip.effective_corner_radius(),
            distance_scale: (scale.x * scale.y).abs().sqrt(),
        }
    }

    /// How much of the canvas pixel `(x, y)` lies inside the region, 0 to 1,
    /// anti-aliased over the edge like `rasterize_rect`.
    fn coverage(&self, x: i32, y: i32) -> f64 {
        let local_x = (f64::from(x) + 0.5 - self.origin.x) / self.scale.x - self.center.x;
        let local_y = (f64::from(y) + 0.5 - self.origin.y) / self.scale.y - self.center.y;
        let distance =
            signed_distance_rounded_box(local_x, local_y, self.half.x, self.half.y, self.radius);
        (0.5 - distance * self.distance_scale).clamp(0.0, 1.0)
    }
}

/// How much of the canvas pixel `(x, y)` is inside every clip in the chain.
fn clip_coverage(clip: &Option<Arc<ClipNode>>, x: i32, y: i32) -> f64 {
    let mut coverage = 1.0;
    let mut node = clip.as_deref();
    while let Some(current) = node {
        coverage *= current.region.coverage(x, y);
        if coverage == 0.0 {
            return 0.0;
        }
        node = current.parent.as_deref();
    }
    coverage
}

fn render_placeholder(
    frame: &mut RgbaFrame,
    anchor: Point,
    state: &ParentState,
    color: Color,
    width: f64,
    height: f64,
) {
    let width = (width * state.scale.x.abs()).round() as i32;
    let height = (height * state.scale.y.abs()).round() as i32;
    let left = (state.position.x - f64::from(width) * anchor.x).round() as i32;
    let top = (state.position.y - f64::from(height) * anchor.y).round() as i32;
    let clip = &state.clip;
    let mode = state.blend_mode;
    fill_rect(
        frame,
        left,
        top,
        width,
        height,
        color,
        state.opacity,
        mode,
        clip,
    );

    let border = Color::rgba(255, 255, 255, 180);
    fill_rect(
        frame,
        left,
        top,
        width,
        2,
        border,
        state.opacity,
        mode,
        clip,
    );
    fill_rect(
        frame,
        left,
        top + height - 2,
        width,
        2,
        border,
        state.opacity,
        mode,
        clip,
    );
    fill_rect(
        frame,
        left,
        top,
        2,
        height,
        border,
        state.opacity,
        mode,
        clip,
    );
    fill_rect(
        frame,
        left + width - 2,
        top,
        2,
        height,
        border,
        state.opacity,
        mode,
        clip,
    );
}

#[derive(Clone, Debug)]
struct DecodedImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

fn render_image(frame: &mut RgbaFrame, image: &DecodedImage, anchor: Point, state: &ParentState) {
    render_image_pixels(
        frame,
        image.width,
        image.height,
        &image.pixels,
        anchor,
        state,
    );
}

fn render_image_pixels(
    frame: &mut RgbaFrame,
    image_width: u32,
    image_height: u32,
    pixels: &[u8],
    anchor: Point,
    state: &ParentState,
) {
    let width = (f64::from(image_width) * state.scale.x.abs())
        .round()
        .max(1.0) as u32;
    let height = (f64::from(image_height) * state.scale.y.abs())
        .round()
        .max(1.0) as u32;
    let left = (state.position.x - f64::from(width) * anchor.x).round() as i32;
    let top = (state.position.y - f64::from(height) * anchor.y).round() as i32;
    for destination_y in 0..height {
        for destination_x in 0..width {
            let source_x = destination_x * image_width / width;
            let source_y = destination_y * image_height / height;
            let source_offset = ((source_y * image_width + source_x) * 4) as usize;
            let x = left + destination_x as i32;
            let y = top + destination_y as i32;
            if x < 0 || y < 0 || x >= frame.width as i32 || y >= frame.height as i32 {
                continue;
            }
            let coverage = clip_coverage(&state.clip, x, y);
            if coverage == 0.0 {
                continue;
            }
            let destination_offset = ((y as u32 * frame.width + x as u32) * 4) as usize;
            blend_with_mode(
                &mut frame.pixels[destination_offset..destination_offset + 4],
                Color::rgba(
                    pixels[source_offset],
                    pixels[source_offset + 1],
                    pixels[source_offset + 2],
                    pixels[source_offset + 3],
                ),
                state.opacity * coverage,
                state.blend_mode,
            );
        }
    }
}

/// Rasterizes a flat-shaded, optionally rounded and stroked rectangle into an
/// RGBA buffer, anti-aliased by signed distance. Shares `RasterizedText`'s
/// shape (width/height/pixels) so it composites through the exact same
/// `render_image` path text does. Takes the raw `Paint`/`Stroke` composition
/// types (like `TextRasterizer::rasterize` takes `&TextStyle`) so callers,
/// including `celesta-gpu-renderer`, never need their own color parsing.
pub fn rasterize_rect(
    width: f64,
    height: f64,
    corner_radius: f64,
    fill: Option<&Paint>,
    stroke: Option<&Stroke>,
) -> Result<RasterizedText, RenderError> {
    let RectPaint { fill, stroke } = resolve_rect_paint(fill, stroke)?;
    Ok(rasterize_rect_pixels(
        width,
        height,
        corner_radius,
        fill,
        stroke,
    ))
}

/// The colors [`rasterize_rect`] paints a rect with.
#[derive(Clone, Debug, PartialEq)]
pub struct RectPaint {
    pub fill: Option<ResolvedPaint>,
    /// The stroke paint and width.
    pub stroke: Option<(ResolvedPaint, f64)>,
}

/// Parses a rect's fill and stroke into the colors [`rasterize_rect`] paints
/// with. A renderer that draws rects itself (`celesta-gpu-renderer` shades
/// them on the GPU) uses this so it resolves paint exactly like the
/// rasterizer.
pub fn resolve_rect_paint(
    fill: Option<&Paint>,
    stroke: Option<&Stroke>,
) -> Result<RectPaint, RenderError> {
    let fill = resolve_paint(fill)?;
    let stroke = stroke
        .map(|stroke| {
            Ok::<_, RenderError>((ResolvedPaint::from_paint(&stroke.paint)?, stroke.width))
        })
        .transpose()?;
    Ok(RectPaint { fill, stroke })
}

fn rasterize_rect_pixels(
    width: f64,
    height: f64,
    corner_radius: f64,
    fill: Option<ResolvedPaint>,
    stroke: Option<(ResolvedPaint, f64)>,
) -> RasterizedText {
    let pixel_width = width.max(0.0).ceil().max(1.0) as u32;
    let pixel_height = height.max(0.0).ceil().max(1.0) as u32;
    let mut pixels = vec![0_u8; pixel_width as usize * pixel_height as usize * 4];

    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let radius = corner_radius.max(0.0).min(half_width.min(half_height));
    let stroke = stroke.filter(|(_, stroke_width)| *stroke_width > 0.0);

    for y in 0..pixel_height {
        for x in 0..pixel_width {
            let px = x as f64 + 0.5 - half_width;
            let py = y as f64 + 0.5 - half_height;
            let outer_distance =
                signed_distance_rounded_box(px, py, half_width, half_height, radius);
            let outer_alpha = (0.5 - outer_distance).clamp(0.0, 1.0);
            if outer_alpha <= 0.0 {
                continue;
            }

            let (sample_x, sample_y) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let mut color = fill
                .as_ref()
                .map_or(Color::TRANSPARENT, |fill| fill.color_at(sample_x, sample_y));
            if let Some((stroke_paint, stroke_width)) = &stroke {
                let (stroke_color, stroke_width) =
                    (stroke_paint.color_at(sample_x, sample_y), *stroke_width);
                let inner_half_width = (half_width - stroke_width).max(0.0);
                let inner_half_height = (half_height - stroke_width).max(0.0);
                let inner_radius = (radius - stroke_width).max(0.0);
                let inner_distance = signed_distance_rounded_box(
                    px,
                    py,
                    inner_half_width,
                    inner_half_height,
                    inner_radius,
                );
                let inner_alpha = (0.5 - inner_distance).clamp(0.0, 1.0);
                color = Color::rgba(
                    lerp(stroke_color.red, color.red, inner_alpha),
                    lerp(stroke_color.green, color.green, inner_alpha),
                    lerp(stroke_color.blue, color.blue, inner_alpha),
                    lerp(stroke_color.alpha, color.alpha, inner_alpha),
                );
            }

            let offset = (y * pixel_width + x) as usize * 4;
            pixels[offset] = color.red;
            pixels[offset + 1] = color.green;
            pixels[offset + 2] = color.blue;
            pixels[offset + 3] = (f64::from(color.alpha) * outer_alpha)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }

    RasterizedText::whole(pixel_width, pixel_height, 0.0, pixels)
}

/// Inigo Quilez's rounded-box signed distance function: negative inside the
/// shape, zero at the edge, positive outside, in the same pixel units as
/// `half_width`/`half_height`/`radius`.
fn signed_distance_rounded_box(
    px: f64,
    py: f64,
    half_width: f64,
    half_height: f64,
    radius: f64,
) -> f64 {
    let qx = px.abs() - half_width + radius;
    let qy = py.abs() - half_height + radius;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

fn lerp(a: u8, b: u8, t: f64) -> u8 {
    (f64::from(a) + (f64::from(b) - f64::from(a)) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn dilate_mask(mask: &[u8], width: u32, height: u32, radius: u32) -> Vec<u8> {
    let mut output = vec![0; mask.len()];
    let radius = radius as i32;
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let alpha = mask[y as usize * width as usize + x as usize];
            if alpha == 0 {
                continue;
            }
            for offset_y in -radius..=radius {
                for offset_x in -radius..=radius {
                    let target_x = x + offset_x;
                    let target_y = y + offset_y;
                    if target_x < 0
                        || target_y < 0
                        || target_x >= width as i32
                        || target_y >= height as i32
                    {
                        continue;
                    }
                    let offset = target_y as usize * width as usize + target_x as usize;
                    output[offset] = output[offset].max(alpha);
                }
            }
        }
    }
    output
}

/// Composites a same-size coverage `mask` onto `frame`, coloring each pixel
/// with `color_at(x, y)`.
fn composite_mask_with(
    frame: &mut RgbaFrame,
    mask: &[u8],
    width: u32,
    color_at: impl Fn(u32, u32) -> Color,
) {
    for (index, &alpha) in mask.iter().enumerate() {
        if alpha == 0 {
            continue;
        }
        let (x, y) = (index as u32 % width, index as u32 / width);
        let offset = index * 4;
        blend(
            &mut frame.pixels[offset..offset + 4],
            color_at(x, y),
            f64::from(alpha) / 255.0,
        );
    }
}

/// Like `composite_mask`, but `source` already carries its own per-pixel RGBA
/// (used for glyph rendering, where a color emoji glyph's pixels vary in hue
/// across the glyph, not just coverage).
fn composite_rgba(
    frame: &mut RgbaFrame,
    source: &[u8],
    width: u32,
    height: u32,
    left: i32,
    top: i32,
) {
    for y in 0..height {
        for x in 0..width {
            let destination_x = left + x as i32;
            let destination_y = top + y as i32;
            if destination_x < 0
                || destination_y < 0
                || destination_x >= frame.width as i32
                || destination_y >= frame.height as i32
            {
                continue;
            }
            let source_offset = ((y * width + x) * 4) as usize;
            let alpha = source[source_offset + 3];
            if alpha == 0 {
                continue;
            }
            let destination_offset =
                ((destination_y as u32 * frame.width + destination_x as u32) * 4) as usize;
            let color = Color::rgba(
                source[source_offset],
                source[source_offset + 1],
                source[source_offset + 2],
                alpha,
            );
            blend(
                &mut frame.pixels[destination_offset..destination_offset + 4],
                color,
                1.0,
            );
        }
    }
}

/// A [`Paint`] with its colors parsed, ready to be sampled per pixel.
#[derive(Clone, Debug, PartialEq)]
pub enum ResolvedPaint {
    Solid(Color),
    Linear {
        start: (f64, f64),
        end: (f64, f64),
        stops: Vec<GradientStop>,
    },
    Radial {
        center: (f64, f64),
        radius: f64,
        stops: Vec<GradientStop>,
    },
}

/// A parsed gradient stop with premultiplied `[r, g, b, a]` channels in 0..=255.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    offset: f64,
    premultiplied: [f64; 4],
}

impl ResolvedPaint {
    pub fn from_paint(paint: &Paint) -> Result<Self, RenderError> {
        Ok(match paint {
            Paint::Solid { color } => Self::Solid(Color::from_hex(color)?),
            Paint::Linear { start, end, stops } => Self::Linear {
                start: (start.x, start.y),
                end: (end.x, end.y),
                stops: resolve_stops(stops)?,
            },
            Paint::Radial {
                center,
                radius,
                stops,
            } => Self::Radial {
                center: (center.x, center.y),
                radius: *radius,
                stops: resolve_stops(stops)?,
            },
        })
    }

    /// The flat color, or `None` for a gradient.
    pub const fn solid(&self) -> Option<Color> {
        match self {
            Self::Solid(color) => Some(*color),
            _ => None,
        }
    }

    /// Scales gradient geometry, for text rasterized at a device scale.
    fn scaled(mut self, scale: f64) -> Self {
        match &mut self {
            Self::Solid(_) => {}
            Self::Linear { start, end, .. } => {
                *start = (start.0 * scale, start.1 * scale);
                *end = (end.0 * scale, end.1 * scale);
            }
            Self::Radial { center, radius, .. } => {
                *center = (center.0 * scale, center.1 * scale);
                *radius *= scale;
            }
        }
        self
    }

    /// The color at a point in the painted layer's local pixels.
    pub fn color_at(&self, x: f64, y: f64) -> Color {
        match self {
            Self::Solid(color) => *color,
            Self::Linear { start, end, stops } => {
                let (dx, dy) = (end.0 - start.0, end.1 - start.1);
                let length_squared = dx * dx + dy * dy;
                let t = if length_squared > 0.0 {
                    ((x - start.0) * dx + (y - start.1) * dy) / length_squared
                } else {
                    0.0
                };
                sample_stops(stops, t)
            }
            Self::Radial {
                center,
                radius,
                stops,
            } => {
                let distance = ((x - center.0).powi(2) + (y - center.1).powi(2)).sqrt();
                sample_stops(
                    stops,
                    if *radius > 0.0 {
                        distance / radius
                    } else {
                        0.0
                    },
                )
            }
        }
    }
}

fn resolve_stops(
    stops: &[celesta_composition::GradientStop],
) -> Result<Vec<GradientStop>, RenderError> {
    let mut resolved = stops
        .iter()
        .map(|stop| {
            let color = Color::from_hex(&stop.color)?;
            let alpha = f64::from(color.alpha) / 255.0;
            Ok(GradientStop {
                offset: stop.offset,
                premultiplied: [
                    f64::from(color.red) * alpha,
                    f64::from(color.green) * alpha,
                    f64::from(color.blue) * alpha,
                    f64::from(color.alpha),
                ],
            })
        })
        .collect::<Result<Vec<_>, RenderError>>()?;
    // Stable, so stops sharing an offset keep their order (a hard edge).
    resolved.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    Ok(resolved)
}

/// Interpolates premultiplied, like CSS, so fading to `#00000000` doesn't
/// darken the color on the way.
fn sample_stops(stops: &[GradientStop], t: f64) -> Color {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return Color::TRANSPARENT;
    };
    let value = if t <= first.offset {
        first.premultiplied
    } else if t >= last.offset {
        last.premultiplied
    } else {
        let next = stops.iter().position(|stop| stop.offset > t).unwrap_or(0);
        let (from, to) = (stops[next - 1], stops[next]);
        let span = to.offset - from.offset;
        let f = if span > 0.0 {
            (t - from.offset) / span
        } else {
            1.0
        };
        std::array::from_fn(|i| {
            from.premultiplied[i] + (to.premultiplied[i] - from.premultiplied[i]) * f
        })
    };
    let alpha = value[3];
    if alpha <= 0.0 {
        return Color::TRANSPARENT;
    }
    let unpremultiply = |channel: f64| (channel * 255.0 / alpha).round().clamp(0.0, 255.0) as u8;
    Color::rgba(
        unpremultiply(value[0]),
        unpremultiply(value[1]),
        unpremultiply(value[2]),
        alpha.round().clamp(0.0, 255.0) as u8,
    )
}

fn resolve_paint(paint: Option<&Paint>) -> Result<Option<ResolvedPaint>, RenderError> {
    paint.map(ResolvedPaint::from_paint).transpose()
}

#[allow(clippy::too_many_arguments)]
fn fill_rect(
    frame: &mut RgbaFrame,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    color: Color,
    opacity: f64,
    blend_mode: BlendMode,
    clip: &Option<Arc<ClipNode>>,
) {
    let right = (left + width).clamp(0, frame.width as i32);
    let bottom = (top + height).clamp(0, frame.height as i32);
    let left = left.clamp(0, frame.width as i32);
    let top = top.clamp(0, frame.height as i32);
    for y in top..bottom {
        for x in left..right {
            let coverage = clip_coverage(clip, x, y);
            if coverage == 0.0 {
                continue;
            }
            let offset = ((y as u32 * frame.width + x as u32) * 4) as usize;
            blend_with_mode(
                &mut frame.pixels[offset..offset + 4],
                color,
                opacity * coverage,
                blend_mode,
            );
        }
    }
}

fn premultiply_pixels(pixels: &[u8]) -> Vec<u8> {
    pixels
        .chunks_exact(4)
        .flat_map(|pixel| {
            let alpha = f64::from(pixel[3]) / 255.0;
            [
                (f64::from(pixel[0]) * alpha).round() as u8,
                (f64::from(pixel[1]) * alpha).round() as u8,
                (f64::from(pixel[2]) * alpha).round() as u8,
                pixel[3],
            ]
        })
        .collect()
}

fn unpremultiply_color(pixel: &[u8]) -> Color {
    if pixel[3] == 0 {
        return Color::TRANSPARENT;
    }
    let alpha = f64::from(pixel[3]);
    Color::rgba(
        (f64::from(pixel[0]) * 255.0 / alpha)
            .round()
            .clamp(0.0, 255.0) as u8,
        (f64::from(pixel[1]) * 255.0 / alpha)
            .round()
            .clamp(0.0, 255.0) as u8,
        (f64::from(pixel[2]) * 255.0 / alpha)
            .round()
            .clamp(0.0, 255.0) as u8,
        pixel[3],
    )
}

fn blur_pixels(source: &RgbaFrame, radius: f64) -> Vec<u8> {
    let sigma = radius.clamp(0.0, 64.0);
    let input = premultiply_pixels(&source.pixels);
    if sigma == 0.0 {
        return input;
    }
    let extent = (sigma * 3.0).ceil() as i32;
    let weights: Vec<f64> = (-extent..=extent)
        .map(|i| (-f64::from(i * i) / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    let pass = |input: &[u8], horizontal: bool| {
        let mut output = vec![0; input.len()];
        for y in 0..source.height as i32 {
            for x in 0..source.width as i32 {
                let mut value = [0.0; 4];
                for (index, weight) in weights.iter().enumerate() {
                    let shift = index as i32 - extent;
                    let (sx, sy) = if horizontal {
                        (x + shift, y)
                    } else {
                        (x, y + shift)
                    };
                    if sx < 0 || sy < 0 || sx >= source.width as i32 || sy >= source.height as i32 {
                        continue;
                    }
                    let offset = ((sy as u32 * source.width + sx as u32) * 4) as usize;
                    for channel in 0..4 {
                        value[channel] += f64::from(input[offset + channel]) * weight / total;
                    }
                }
                let offset = ((y as u32 * source.width + x as u32) * 4) as usize;
                for channel in 0..4 {
                    output[offset + channel] = value[channel].round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        output
    };
    pass(&pass(&input, true), false)
}

fn render_effect_shadow(
    destination: &mut RgbaFrame,
    source: &RgbaFrame,
    color: &str,
    radius: f64,
    offset_x: f64,
    offset_y: f64,
) -> Result<(), RenderError> {
    let tint = Color::from_hex(color)?;
    let blurred = blur_pixels(source, radius);
    for y in 0..source.height as i32 {
        for x in 0..source.width as i32 {
            let destination_offset = ((y as u32 * source.width + x as u32) * 4) as usize;
            let alpha = sample_effect_alpha(
                &blurred,
                source.width,
                source.height,
                f64::from(x) - offset_x,
                f64::from(y) - offset_y,
            );
            blend(
                &mut destination.pixels[destination_offset..destination_offset + 4],
                tint,
                alpha / 255.0,
            );
        }
    }
    Ok(())
}

fn sample_effect_alpha(pixels: &[u8], width: u32, height: u32, x: f64, y: f64) -> f64 {
    if x <= -1.0 || y <= -1.0 || x >= f64::from(width) || y >= f64::from(height) {
        return 0.0;
    }
    let left = x.floor() as i32;
    let top = y.floor() as i32;
    let fx = x - f64::from(left);
    let fy = y - f64::from(top);
    let alpha = |x: i32, y: i32| {
        if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
            0.0
        } else {
            f64::from(pixels[((y as u32 * width + x as u32) * 4 + 3) as usize])
        }
    };
    let upper = alpha(left, top) * (1.0 - fx) + alpha(left + 1, top) * fx;
    let lower = alpha(left, top + 1) * (1.0 - fx) + alpha(left + 1, top + 1) * fx;
    upper * (1.0 - fy) + lower * fy
}

/// Composites `source` onto `destination` (both non-premultiplied) through
/// `mode`: the source color becomes `(1 - ab) * Cs + ab * B(Cb, Cs)`, which
/// then composites source-over, as in the W3C Compositing and Blending spec.
fn blend_with_mode(destination: &mut [u8], source: Color, opacity: f64, mode: BlendMode) {
    if mode.is_normal() {
        return blend(destination, source, opacity);
    }
    blend_mixed(destination, source, opacity, |backdrop, source| {
        mode.blend_channel(backdrop, source)
    });
}

/// Source-over compositing with a separable mixing function `mix(backdrop,
/// source)` per channel, all in 0–1 (W3C Compositing, "simple alpha
/// compositing" with blending).
fn blend_mixed(destination: &mut [u8], source: Color, opacity: f64, mix: impl Fn(f64, f64) -> f64) {
    let source_alpha = (f64::from(source.alpha) / 255.0) * opacity.clamp(0.0, 1.0);
    if source_alpha == 0.0 {
        return;
    }
    let backdrop_alpha = f64::from(destination[3]) / 255.0;
    let output_alpha = source_alpha + backdrop_alpha * (1.0 - source_alpha);
    for channel in 0..3 {
        let source_value = f64::from([source.red, source.green, source.blue][channel]) / 255.0;
        let backdrop_value = f64::from(destination[channel]) / 255.0;
        let mixed = (1.0 - backdrop_alpha) * source_value
            + backdrop_alpha * mix(backdrop_value, source_value);
        let output = (source_alpha * mixed
            + backdrop_alpha * backdrop_value * (1.0 - source_alpha))
            / output_alpha;
        destination[channel] = (output * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    destination[3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn blend(destination: &mut [u8], source: Color, opacity: f64) {
    let source_alpha = (f64::from(source.alpha) / 255.0) * opacity.clamp(0.0, 1.0);
    let destination_alpha = f64::from(destination[3]) / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if output_alpha == 0.0 {
        destination.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    for channel in 0..3 {
        let source_value = f64::from([source.red, source.green, source.blue][channel]);
        let destination_value = f64::from(destination[channel]);
        let output = (source_value * source_alpha
            + destination_value * destination_alpha * (1.0 - source_alpha))
            / output_alpha;
        destination[channel] = output.round().clamp(0.0, 255.0) as u8;
    }
    destination[3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// The per-channel mixing function `B(backdrop, source)` of a PSD layer's
/// blend mode, all in 0–1, for the modes that blend each channel on its own.
/// `None` for normal, and for the modes drawn as normal because they mix
/// whole colors (hue, saturation, color, luminosity, darker and lighter
/// color) or noise (dissolve).
fn psd_blend_channel(mode: &str) -> Option<fn(f64, f64) -> f64> {
    fn screen(b: f64, s: f64) -> f64 {
        b + s - b * s
    }
    fn color_burn(b: f64, s: f64) -> f64 {
        if b >= 1.0 {
            1.0
        } else if s <= 0.0 {
            0.0
        } else {
            1.0 - ((1.0 - b) / s).min(1.0)
        }
    }
    fn color_dodge(b: f64, s: f64) -> f64 {
        if b <= 0.0 {
            0.0
        } else if s >= 1.0 {
            1.0
        } else {
            (b / (1.0 - s)).min(1.0)
        }
    }
    fn hard_light(b: f64, s: f64) -> f64 {
        if s <= 0.5 {
            b * 2.0 * s
        } else {
            screen(b, 2.0 * s - 1.0)
        }
    }
    fn soft_light(b: f64, s: f64) -> f64 {
        if s <= 0.5 {
            b - (1.0 - 2.0 * s) * b * (1.0 - b)
        } else {
            let d = if b <= 0.25 {
                ((16.0 * b - 12.0) * b + 4.0) * b
            } else {
                b.sqrt()
            };
            b + (2.0 * s - 1.0) * (d - b)
        }
    }
    fn vivid_light(b: f64, s: f64) -> f64 {
        if s <= 0.5 {
            color_burn(b, 2.0 * s)
        } else {
            color_dodge(b, 2.0 * s - 1.0)
        }
    }
    fn pin_light(b: f64, s: f64) -> f64 {
        if s <= 0.5 {
            b.min(2.0 * s)
        } else {
            b.max(2.0 * s - 1.0)
        }
    }

    Some(match mode {
        "Darken" => f64::min,
        "Multiply" => |b, s| b * s,
        "ColorBurn" => color_burn,
        "LinearBurn" => |b, s| (b + s - 1.0).max(0.0),
        "Lighten" => f64::max,
        "Screen" => screen,
        "ColorDodge" => color_dodge,
        "LinearDodge" => |b, s| (b + s).min(1.0),
        "Overlay" => |b, s| hard_light(s, b),
        "SoftLight" => soft_light,
        "HardLight" => hard_light,
        "VividLight" => vivid_light,
        "LinearLight" => |b, s| (b + 2.0 * s - 1.0).clamp(0.0, 1.0),
        "PinLight" => pin_light,
        "HardMix" => |b, s| if b + s >= 1.0 { 1.0 } else { 0.0 },
        "Difference" => |b, s| (b - s).abs(),
        "Exclusion" => |b, s| b + s - 2.0 * b * s,
        "Subtract" => |b, s| (b - s).max(0.0),
        "Divide" => |b, s| {
            if s <= 0.0 {
                if b <= 0.0 { 0.0 } else { 1.0 }
            } else {
                (b / s).min(1.0)
            }
        },
        // PassThrough, Normal, Dissolve, DarkerColor, LighterColor, Hue,
        // Saturation, Color, Luminosity.
        _ => return None,
    })
}

#[derive(Debug)]
pub enum RenderError {
    SurfaceTooLarge {
        width: u32,
        height: u32,
    },
    InvalidColor(String),
    UnsupportedRotation {
        layer: String,
        degrees: f64,
    },
    UnsupportedNonUniformTextScale {
        x: f64,
        y: f64,
    },
    RemoteAsset {
        asset: String,
        source: RemoteAssetError,
    },
    AssetIo {
        asset: String,
        source: io::Error,
    },
    ImageDecode {
        asset: String,
        source: image::ImageError,
    },
    PsdDecode {
        asset: String,
        source: psd::PsdError,
    },
    InvalidFont {
        asset: String,
        reason: &'static str,
    },
    MissingPsdLayer {
        asset: String,
        layer: String,
    },
    Media(MediaError),
    Time(celesta_composition::TimeError),
    Io(io::Error),
    Png(png::EncodingError),
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SurfaceTooLarge { width, height } => {
                write!(formatter, "surface {width}x{height} is too large")
            }
            Self::InvalidColor(color) => write!(formatter, "invalid color `{color}`"),
            Self::UnsupportedRotation { layer, degrees } => write!(
                formatter,
                "CPU reference renderer does not support {degrees} degree rotation on layer `{layer}`"
            ),
            Self::UnsupportedNonUniformTextScale { x, y } => write!(
                formatter,
                "CPU reference renderer does not support non-uniform text scale ({x}, {y})"
            ),
            Self::RemoteAsset { asset, source } => {
                write!(formatter, "could not load remote asset `{asset}`: {source}")
            }
            Self::AssetIo { asset, source } => {
                write!(formatter, "could not read asset `{asset}`: {source}")
            }
            Self::ImageDecode { asset, source } => {
                write!(
                    formatter,
                    "could not decode image asset `{asset}`: {source}"
                )
            }
            Self::PsdDecode { asset, source } => {
                write!(formatter, "could not decode PSD asset `{asset}`: {source}")
            }
            Self::MissingPsdLayer { asset, layer } => {
                write!(formatter, "PSD asset `{asset}` has no layer `{layer}`")
            }
            Self::InvalidFont { asset, reason } => {
                write!(formatter, "could not load font `{asset}`: {reason}")
            }
            Self::Media(error) => write!(formatter, "could not decode video frame: {error}"),
            Self::Time(error) => write!(formatter, "could not calculate video time: {error}"),
            Self::Io(error) => write!(formatter, "image I/O failed: {error}"),
            Self::Png(error) => write!(formatter, "PNG encoding failed: {error}"),
        }
    }
}

impl Error for RenderError {}

impl From<MediaError> for RenderError {
    fn from(error: MediaError) -> Self {
        Self::Media(error)
    }
}

impl From<celesta_composition::TimeError> for RenderError {
    fn from(error: celesta_composition::TimeError) -> Self {
        Self::Time(error)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use celesta_composition::{
        AssetLocation, BlendMode, Clip, EvaluatedTransform, Layer, LayerContent, MediaTiming,
        Paint, Rational, ResolvedAsset, Scene, TextStyle, Time,
    };
    use celesta_media::{MediaError, VideoFrame, VideoFrameDecoder};

    use super::*;

    #[test]
    fn measure_reports_line_metrics_and_per_glyph_advances() {
        let style = TextStyle {
            font_size: Some(40.0),
            ..TextStyle::default()
        };
        let mut rasterizer = TextRasterizer::new();
        let metrics = rasterizer.measure("ab\ncd", &style, None);

        assert_eq!(metrics.lines, 2);
        assert_eq!(metrics.glyphs.len(), 4);
        assert_eq!(metrics.line_height, 48.0);
        assert_eq!(metrics.height, 96.0);
        assert!((metrics.ascent + metrics.descent - metrics.line_height).abs() < 1e-6);
        let first_line = &metrics.glyphs[..2];
        assert_eq!(first_line[0].x, 0.0);
        assert!((first_line[1].x - first_line[0].width).abs() < 1e-6);
        assert!(metrics.width >= first_line[1].x + first_line[1].width - 1e-6);
        assert_eq!(metrics.glyphs[2].line, 1);
    }

    #[test]
    fn rasterizes_color_emoji_glyphs_when_a_color_font_is_available() {
        let mut rasterizer = TextRasterizer::new();
        let rasterized = rasterizer
            .rasterize(
                "\u{1F525}", // fire emoji: multi-colored (red/orange/yellow) on
                // any real color-emoji font, unlike a flat single-fill glyph.
                &TextStyle {
                    font_size: Some(64.0),
                    fill: Some(Paint::Solid {
                        color: "#FFFFFFFF".to_owned(),
                    }),
                    ..TextStyle::default()
                },
                None,
                1.0,
            )
            .unwrap();

        let distinct_colors: std::collections::HashSet<[u8; 3]> = rasterized
            .pixels()
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 0)
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect();

        // A regression back to alpha-only mask compositing would flatten
        // every opaque pixel to the single fill color (white here), so this
        // is the direct check that color glyph data survives rasterization.
        // Environments without any color-emoji font (uncommon, but possible
        // outside macOS) fall back to a monochrome glyph outline instead of
        // failing — not a regression this test can detect there.
        if distinct_colors.len() <= 1 {
            eprintln!(
                "skipping color emoji assertion: no color-emoji font available in this environment"
            );
            return;
        }
        assert!(distinct_colors.len() > 1);
    }

    #[test]
    fn renders_text_to_a_png() {
        let scene = Scene {
            width: 320,
            height: 180,
            frame_rate: Rational::new(60, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![Layer {
                id: "hello".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 160.0, y: 90.0 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Text {
                    text: "Hello, Celesta!".to_owned(),
                    style: TextStyle {
                        font_size: Some(24.0),
                        ..TextStyle::default()
                    },
                    max_width: None,
                    baseline_anchor: false,
                },
            }],
        };
        let mut renderer = CpuRenderer::default();
        let frame = renderer.render(&scene).unwrap();
        let png = frame.encode_png().unwrap();

        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert!(
            frame
                .pixels()
                .chunks_exact(4)
                .any(|pixel| pixel == [255, 255, 255, 255])
        );
    }

    fn clip_scene(layers: Vec<Layer>) -> Scene {
        Scene {
            width: 40,
            height: 40,
            frame_rate: Rational::new(30, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers,
        }
    }

    /// A 40x40 red rect at the group's origin.
    fn red_square() -> Layer {
        Layer {
            id: "red".to_owned(),
            transform: EvaluatedTransform {
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Rect {
                width: 40.0,
                height: 40.0,
                fill: Some(Paint::Solid {
                    color: "#FF0000FF".to_owned(),
                }),
                stroke: None,
                corner_radius: 0.0,
            },
        }
    }

    fn clipped_group(transform: EvaluatedTransform, clip: Clip, children: Vec<Layer>) -> Layer {
        Layer {
            id: "clipped".to_owned(),
            transform,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Group {
                layers: children,
                clip: Some(clip),
            },
        }
    }

    fn pixel(frame: &RgbaFrame, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * frame.width() + x) * 4) as usize;
        frame.pixels()[offset..offset + 4].try_into().unwrap()
    }

    fn is_red(frame: &RgbaFrame, x: u32, y: u32) -> bool {
        pixel(frame, x, y) == [255, 0, 0, 255]
    }

    /// The renderer's default background, which nothing has drawn over.
    fn background() -> [u8; 4] {
        let color = RenderOptions::default().background;
        [color.red, color.green, color.blue, color.alpha]
    }

    fn is_untouched(frame: &RgbaFrame, x: u32, y: u32) -> bool {
        pixel(frame, x, y) == background()
    }

    #[test]
    fn clips_a_groups_children_to_its_rectangle() {
        let clip = Clip {
            x: 10.0,
            y: 5.0,
            width: 20.0,
            height: 10.0,
            corner_radius: 0.0,
        };
        let scene = clip_scene(vec![clipped_group(
            EvaluatedTransform::default(),
            clip,
            vec![red_square()],
        )]);
        let frame = CpuRenderer::default().render(&scene).unwrap();

        // Inside, right at each edge, and one pixel beyond it.
        assert!(is_red(&frame, 10, 5));
        assert!(is_red(&frame, 29, 14));
        assert!(is_red(&frame, 20, 10));
        for (x, y) in [(9, 5), (30, 10), (20, 4), (20, 15), (0, 0), (39, 39)] {
            assert!(is_untouched(&frame, x, y), "({x}, {y}) leaked");
        }
    }

    #[test]
    fn a_clip_follows_the_groups_position_and_scale() {
        // The clip covers the group's 0..10 by 0..10, which the group's
        // position and 2x scale put at canvas 8..28 by 4..24.
        let clip = Clip {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            corner_radius: 0.0,
        };
        let scene = clip_scene(vec![clipped_group(
            EvaluatedTransform {
                position: Point { x: 8.0, y: 4.0 },
                scale: Point { x: 2.0, y: 2.0 },
                ..EvaluatedTransform::default()
            },
            clip,
            vec![red_square()],
        )]);
        let frame = CpuRenderer::default().render(&scene).unwrap();

        assert!(is_red(&frame, 8, 4));
        assert!(is_red(&frame, 27, 23));
        for (x, y) in [(7, 4), (28, 10), (10, 3), (10, 24)] {
            assert!(is_untouched(&frame, x, y), "({x}, {y}) leaked");
        }
    }

    #[test]
    fn rounds_the_corners_of_a_clip() {
        let clip = Clip {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 40.0,
            corner_radius: 10.0,
        };
        let scene = clip_scene(vec![clipped_group(
            EvaluatedTransform::default(),
            clip,
            vec![red_square()],
        )]);
        let frame = CpuRenderer::default().render(&scene).unwrap();

        assert!(is_red(&frame, 20, 20));
        assert!(is_red(&frame, 20, 0));
        assert!(is_untouched(&frame, 0, 0));
        // On the rounded edge the coverage is partial, so the pixel is a
        // blend of the red and the background rather than either.
        let edge = pixel(&frame, 2, 3);
        assert!(edge[0] > background()[0] && edge[0] < 255, "edge {edge:?}");
    }

    #[test]
    fn nested_clips_intersect() {
        let outer = Clip {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 40.0,
            corner_radius: 0.0,
        };
        let inner = Clip {
            x: 10.0,
            y: 0.0,
            width: 30.0,
            height: 10.0,
            corner_radius: 0.0,
        };
        let scene = clip_scene(vec![clipped_group(
            EvaluatedTransform::default(),
            outer,
            vec![clipped_group(
                EvaluatedTransform::default(),
                inner,
                vec![red_square()],
            )],
        )]);
        let frame = CpuRenderer::default().render(&scene).unwrap();

        // Only 10..20 by 0..10 is inside both.
        assert!(is_red(&frame, 10, 0));
        assert!(is_red(&frame, 19, 9));
        for (x, y) in [(9, 5), (20, 5), (15, 10)] {
            assert!(is_untouched(&frame, x, y), "({x}, {y}) leaked");
        }
    }

    #[test]
    fn a_clip_without_area_hides_its_children() {
        let clip = Clip {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 40.0,
            corner_radius: 0.0,
        };
        let scene = clip_scene(vec![clipped_group(
            EvaluatedTransform::default(),
            clip,
            vec![red_square()],
        )]);
        let frame = CpuRenderer::default().render(&scene).unwrap();

        assert!(
            frame
                .pixels()
                .chunks_exact(4)
                .all(|pixel| pixel == background())
        );
    }

    #[test]
    fn does_not_clip_layers_outside_the_group() {
        let clip = Clip {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            corner_radius: 0.0,
        };
        let after = Layer {
            id: "after".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 30.0, y: 30.0 },
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Rect {
                width: 10.0,
                height: 10.0,
                fill: Some(Paint::Solid {
                    color: "#00FF00FF".to_owned(),
                }),
                stroke: None,
                corner_radius: 0.0,
            },
        };
        let scene = clip_scene(vec![
            clipped_group(EvaluatedTransform::default(), clip, vec![red_square()]),
            after,
        ]);
        let frame = CpuRenderer::default().render(&scene).unwrap();

        assert!(is_red(&frame, 9, 9));
        assert!(is_untouched(&frame, 10, 10));
        assert_eq!(pixel(&frame, 30, 30), [0, 255, 0, 255]);
        assert_eq!(pixel(&frame, 39, 39), [0, 255, 0, 255]);
    }

    #[test]
    fn clips_text_to_the_rectangle() {
        let text = Layer {
            id: "text".to_owned(),
            transform: EvaluatedTransform {
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Text {
                text: "MMMM".to_owned(),
                style: TextStyle {
                    font_size: Some(30.0),
                    fill: Some(Paint::Solid {
                        color: "#FFFFFFFF".to_owned(),
                    }),
                    ..TextStyle::default()
                },
                max_width: None,
                baseline_anchor: false,
            },
        };
        let clip = Clip {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 15.0,
            corner_radius: 0.0,
        };
        let plain = CpuRenderer::default()
            .render(&clip_scene(vec![text.clone()]))
            .unwrap();
        let clipped = CpuRenderer::default()
            .render(&clip_scene(vec![clipped_group(
                EvaluatedTransform::default(),
                clip,
                vec![text],
            )]))
            .unwrap();

        // Inside the clip the text is untouched; below it nothing is drawn.
        for y in 0..40 {
            for x in 0..40 {
                if y < 15 {
                    assert_eq!(pixel(&clipped, x, y), pixel(&plain, x, y), "({x}, {y})");
                } else {
                    assert!(is_untouched(&clipped, x, y), "({x}, {y}) leaked");
                }
            }
        }
    }

    #[test]
    fn renders_a_filled_rounded_rect_with_a_stroke() {
        let scene = Scene {
            width: 200,
            height: 120,
            frame_rate: Rational::new(60, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![Layer {
                id: "card".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 100.0, y: 60.0 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Rect {
                    width: 100.0,
                    height: 60.0,
                    fill: Some(Paint::Solid {
                        color: "#3366CCFF".to_owned(),
                    }),
                    stroke: Some(celesta_composition::Stroke {
                        paint: Paint::Solid {
                            color: "#FFFFFFFF".to_owned(),
                        },
                        width: 4.0,
                    }),
                    corner_radius: 12.0,
                },
            }],
        };
        let mut renderer = CpuRenderer::default();
        let frame = renderer.render(&scene).unwrap();

        // Center of the rect is inside the fill, away from the stroke band.
        let center_offset = ((60 * frame.width() + 100) * 4) as usize;
        assert_eq!(
            &frame.pixels()[center_offset..center_offset + 4],
            &[0x33, 0x66, 0xCC, 0xFF]
        );

        // A pixel just outside the corner radius stays background (transparent
        // over the render's own background, so at least distinct from the fill
        // and stroke colors).
        let corner_offset = ((32 * frame.width() + 52) * 4) as usize;
        let corner_pixel = &frame.pixels()[corner_offset..corner_offset + 4];
        assert_ne!(corner_pixel, [0x33, 0x66, 0xCC, 0xFF]);
        assert_ne!(corner_pixel, [0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn centers_visible_single_line_text_on_its_transform() {
        let scene = Scene {
            width: 1280,
            height: 720,
            frame_rate: Rational::new(60, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![Layer {
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
            }],
        };
        let mut renderer = CpuRenderer::default();
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
        let center_x = f64::from(min_x + max_x) / 2.0;
        let center_y = f64::from(min_y + max_y) / 2.0;
        // Horizontally the advance box is centered, so uneven side bearings
        // leave the ink a few pixels off.
        assert!((center_x - 640.0).abs() <= 4.0, "center x was {center_x}");
        assert!((center_y - 360.0).abs() <= 0.5, "center y was {center_y}");
    }

    #[test]
    fn a_text_stroke_reaches_past_both_ends_of_the_line() {
        let plain_style = TextStyle {
            font_size: Some(96.0),
            ..TextStyle::default()
        };
        let stroked_style = TextStyle {
            stroke: Some(celesta_composition::Stroke {
                paint: Paint::Solid {
                    color: "#000000FF".to_owned(),
                },
                width: 16.0,
            }),
            ..plain_style.clone()
        };
        let mut rasterizer = TextRasterizer::new();
        let plain = rasterizer.rasterize("MW", &plain_style, None, 1.0).unwrap();
        let stroked = rasterizer
            .rasterize("MW", &stroked_style, None, 1.0)
            .unwrap();

        assert_eq!(stroked.width(), plain.width() + 32);
        let column_has_ink = |x: u32| {
            (0..stroked.height())
                .any(|y| stroked.pixels()[((y * stroked.width() + x) * 4 + 3) as usize] > 0)
        };
        // Before, the stroke was cut off at the advance box on both sides.
        assert!((0..16).any(column_has_ink), "no stroke left of the line");
        assert!(
            (stroked.width() - 16..stroked.width()).any(column_has_ink),
            "no stroke right of the line"
        );
        // Anchors still refer to the advance box, not the padded image.
        let (left, _) = stroked.anchor_in_image(0.0, 0.0);
        let (right, _) = stroked.anchor_in_image(1.0, 0.0);
        assert!((left - 16.0 / f64::from(stroked.width())).abs() < 1e-9);
        assert!(
            (right - f64::from(stroked.width() - 16) / f64::from(stroked.width())).abs() < 1e-9
        );
        assert_eq!(plain.anchor_in_image(0.25, 0.75), (0.25, 0.75));
    }

    #[test]
    fn a_text_stroke_does_not_move_the_text() {
        let ink_center = |stroke: Option<celesta_composition::Stroke>| {
            let scene = Scene {
                width: 640,
                height: 360,
                frame_rate: Rational::new(30, 1),
                time: Time::ZERO,
                fonts: Vec::new(),
                layers: vec![Layer {
                    id: "title".to_owned(),
                    transform: EvaluatedTransform {
                        position: Point { x: 320.0, y: 180.0 },
                        ..EvaluatedTransform::default()
                    },
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    effects: Default::default(),
                    content: LayerContent::Text {
                        text: "Celesta".to_owned(),
                        style: TextStyle {
                            font_size: Some(72.0),
                            fill: Some(Paint::Solid {
                                color: "#FFFFFFFF".to_owned(),
                            }),
                            stroke,
                            ..TextStyle::default()
                        },
                        max_width: None,
                        baseline_anchor: false,
                    },
                }],
            };
            let frame = CpuRenderer::default().render(&scene).unwrap();
            // The white fill only: the stroke is black.
            let (mut min_x, mut max_x) = (u32::MAX, 0);
            for (index, pixel) in frame.pixels().chunks_exact(4).enumerate() {
                if pixel[0] > 200 && pixel[1] > 200 {
                    let x = index as u32 % frame.width();
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                }
            }
            f64::from(min_x + max_x) / 2.0
        };
        let plain = ink_center(None);
        let stroked = ink_center(Some(celesta_composition::Stroke {
            paint: Paint::Solid {
                color: "#000000FF".to_owned(),
            },
            width: 12.0,
        }));
        assert!(
            (plain - stroked).abs() <= 1.0,
            "plain {plain}, stroked {stroked}"
        );
    }

    #[test]
    fn single_line_text_keeps_the_width_of_its_spaces() {
        let mut rasterizer = TextRasterizer::new();
        let style = TextStyle {
            font_size: Some(48.0),
            ..TextStyle::default()
        };
        let mut rasterize = |text| rasterizer.rasterize(text, &style, None, 1.0).unwrap();
        let (equals, spaced, space) = (rasterize("="), rasterize(" = "), rasterize(" "));
        assert!(space.width() > 1, "a space was {} px wide", space.width());
        // Each width is rounded up to whole pixels on its own.
        assert!(
            spaced.width().abs_diff(equals.width() + space.width() * 2) <= 2,
            "\" = \" was {} px wide, \"=\" {} px, \" \" {} px",
            spaced.width(),
            equals.width(),
            space.width()
        );
        assert_eq!(spaced.height(), equals.height());
    }

    fn stop(offset: f64, color: &str) -> celesta_composition::GradientStop {
        celesta_composition::GradientStop {
            offset,
            color: color.to_owned(),
        }
    }

    #[test]
    fn linear_gradient_fills_a_rect_and_fades_to_transparent() {
        let fill = Paint::Linear {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 100.0, y: 0.0 },
            stops: vec![stop(0.0, "#ff0000"), stop(1.0, "#00000000")],
        };
        let rect = rasterize_rect(100.0, 10.0, 0.0, Some(&fill), None).unwrap();
        let pixels = rect.pixels();
        let at = |x: usize| &pixels[(5 * 100 + x) * 4..(5 * 100 + x) * 4 + 4];
        assert_eq!(at(0)[..3], [255, 0, 0]);
        assert!(at(0)[3] > 245, "start is opaque: {:?}", at(0));
        // Premultiplied blending keeps the hue while fading out.
        assert!(
            at(50)[0] > 250 && (120..135).contains(&at(50)[3]),
            "{:?}",
            at(50)
        );
        assert!(at(99)[3] < 6, "end is transparent: {:?}", at(99));
    }

    #[test]
    fn radial_gradient_and_solid_stroke_paint_a_rect() {
        let fill = Paint::Radial {
            center: Point { x: 20.0, y: 20.0 },
            radius: 20.0,
            stops: vec![stop(0.0, "#ffffff"), stop(1.0, "#000000")],
        };
        let rect = rasterize_rect(40.0, 40.0, 0.0, Some(&fill), None).unwrap();
        let pixels = rect.pixels();
        let red = |x: usize, y: usize| pixels[(y * 40 + x) * 4];
        assert!(red(20, 20) > 240);
        assert!(red(2, 20) < 40);
    }

    #[test]
    fn gradient_text_varies_across_the_glyphs() {
        let mut rasterizer = TextRasterizer::new();
        let style = TextStyle {
            font_size: Some(64.0),
            fill: Some(Paint::Linear {
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 200.0, y: 0.0 },
                stops: vec![stop(0.0, "#ff0000"), stop(1.0, "#0000ff")],
            }),
            ..TextStyle::default()
        };
        let text = rasterizer.rasterize("MMMM", &style, None, 1.0).unwrap();
        let (width, pixels) = (text.width() as usize, text.pixels());
        let opaque = |from: usize, to: usize| {
            (0..text.height() as usize)
                .flat_map(|y| (from..to).map(move |x| (x, y)))
                .map(|(x, y)| &pixels[(y * width + x) * 4..(y * width + x) * 4 + 4])
                .find(|pixel| pixel[3] > 250)
                .map(|pixel| (pixel[0], pixel[2]))
                .unwrap()
        };
        let (left_red, left_blue) = opaque(0, width / 4);
        let (right_red, right_blue) = opaque(width * 3 / 4, width);
        assert!(left_red > right_red && right_blue > left_blue);
    }

    #[test]
    fn letter_spacing_widens_and_tightens_text() {
        let mut rasterizer = TextRasterizer::new();
        let mut width = |letter_spacing| {
            let style = TextStyle {
                font_size: Some(48.0),
                letter_spacing,
                ..TextStyle::default()
            };
            rasterizer
                .rasterize("ABCD", &style, None, 1.0)
                .unwrap()
                .width()
        };
        let (base, wide, tight) = (width(None), width(Some(10.0)), width(Some(-4.0)));
        assert!(wide > base + 30, "wide {wide}, base {base}");
        assert!(tight < base - 10, "tight {tight}, base {base}");
    }

    #[test]
    fn baseline_anchored_text_layers_share_a_baseline() {
        const BASELINE: u32 = 200;
        let text_layer = |id: &str, text: &str, x: f64, font_size: f64| Layer {
            id: id.to_owned(),
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
        };
        let scene = Scene {
            width: 400,
            height: 300,
            frame_rate: Rational::new(30, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![
                text_layer("small", "x", 20.0, 32.0),
                text_layer("tall", "H", 110.0, 96.0),
                text_layer("round", "o", 230.0, 64.0),
                text_layer("descender", "y", 320.0, 64.0),
            ],
        };
        let frame = CpuRenderer::default().render(&scene).unwrap();
        // The lowest inked row between `left` and `right`, ignoring faint
        // antialiasing at the glyph's bottom edge.
        let ink_bottom = |left: u32, right: u32| {
            (0..frame.height())
                .rev()
                .find(|&y| {
                    (left..right).any(|x| {
                        let offset = ((y * frame.width() + x) * 4) as usize;
                        frame.pixels()[offset] > 128
                    })
                })
                .unwrap()
        };
        // Glyphs without descenders rest on the baseline; `y` hangs below it.
        for (name, left, right) in [("x", 20, 100), ("H", 110, 220), ("o", 230, 310)] {
            let bottom = ink_bottom(left, right);
            assert!(
                bottom.abs_diff(BASELINE - 1) <= 2,
                "{name} ends on row {bottom}, not above the baseline at {BASELINE}"
            );
        }
        assert!(ink_bottom(320, 400) > BASELINE + 5);
    }

    #[test]
    fn alpha_composites_in_painter_order() {
        let mut destination = [0, 0, 0, 255];
        blend(&mut destination, Color::rgba(200, 100, 0, 255), 0.5);
        assert_eq!(destination, [100, 50, 0, 255]);
    }

    #[test]
    fn blends_each_mode_with_the_backdrop() {
        let blended = |mode: BlendMode, opacity: f64| {
            let mut destination = [200, 100, 50, 255];
            blend_with_mode(
                &mut destination,
                Color::rgba(100, 200, 250, 255),
                opacity,
                mode,
            );
            destination
        };
        assert_eq!(blended(BlendMode::Normal, 1.0), [100, 200, 250, 255]);
        assert_eq!(blended(BlendMode::Multiply, 1.0), [78, 78, 49, 255]);
        assert_eq!(blended(BlendMode::Screen, 1.0), [222, 222, 251, 255]);
        assert_eq!(blended(BlendMode::Overlay, 1.0), [188, 157, 98, 255]);
        assert_eq!(blended(BlendMode::Add, 1.0), [255, 255, 255, 255]);
        assert_eq!(blended(BlendMode::Difference, 1.0), [100, 100, 200, 255]);
        // Opacity fades between the backdrop and the blended color.
        assert_eq!(blended(BlendMode::Difference, 0.5), [150, 100, 125, 255]);

        // Over a transparent backdrop the source composites unchanged.
        let mut destination = [0, 0, 0, 0];
        blend_with_mode(
            &mut destination,
            Color::rgba(100, 200, 250, 128),
            1.0,
            BlendMode::Difference,
        );
        assert_eq!(destination, [100, 200, 250, 128]);
    }

    fn rect_layer(id: &str, x: f64, width: f64, color: &str, blend_mode: BlendMode) -> Layer {
        Layer {
            id: id.to_owned(),
            transform: EvaluatedTransform {
                position: Point { x, y: 0.0 },
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode,
            effects: Default::default(),
            content: LayerContent::Rect {
                width,
                height: 1.0,
                fill: Some(Paint::Solid {
                    color: color.to_owned(),
                }),
                stroke: None,
                corner_radius: 0.0,
            },
        }
    }

    #[test]
    fn blends_a_layer_with_everything_beneath_it() {
        let scene = Scene {
            width: 4,
            height: 1,
            frame_rate: Rational::new(30, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![
                rect_layer("light", 2.0, 2.0, "#ffffff", BlendMode::Normal),
                // Inside a plain group, a child still blends with what the
                // group's siblings drew.
                Layer {
                    id: "group".to_owned(),
                    transform: EvaluatedTransform {
                        anchor: Point { x: 0.0, y: 0.0 },
                        ..EvaluatedTransform::default()
                    },
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    effects: Default::default(),
                    content: LayerContent::Group {
                        layers: vec![rect_layer(
                            "hud",
                            1.0,
                            2.0,
                            "#e0e0e0",
                            BlendMode::Difference,
                        )],
                        clip: None,
                    },
                },
            ],
        };
        let frame = CpuRenderer::new(RenderOptions {
            background: Color::rgba(0, 0, 0, 255),
        })
        .render(&scene)
        .unwrap();
        let pixel = |x: usize| frame.pixels()[x * 4..x * 4 + 4].to_vec();
        assert_eq!(pixel(0), [0, 0, 0, 255]);
        // Light over the dark background, dark over the light rect.
        assert_eq!(pixel(1), [224, 224, 224, 255]);
        assert_eq!(pixel(2), [31, 31, 31, 255]);
        assert_eq!(pixel(3), [255, 255, 255, 255]);
    }

    #[test]
    fn blends_an_isolated_group_as_one_layer() {
        let group = |opacity: f64| Layer {
            id: "group".to_owned(),
            transform: EvaluatedTransform {
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            opacity,
            blend_mode: BlendMode::Difference,
            effects: Default::default(),
            content: LayerContent::Group {
                layers: vec![
                    rect_layer("white", 0.0, 3.0, "#ffffff", BlendMode::Normal),
                    // Composites onto the group's own layer, not the scene.
                    rect_layer("red", 1.0, 1.0, "#ff0000", BlendMode::Multiply),
                ],
                clip: None,
            },
        };
        let render = |opacity: f64| {
            let scene = Scene {
                width: 4,
                height: 1,
                frame_rate: Rational::new(30, 1),
                time: Time::ZERO,
                fonts: Vec::new(),
                layers: vec![
                    rect_layer("gray", 0.0, 4.0, "#808080", BlendMode::Normal),
                    group(opacity),
                ],
            };
            CpuRenderer::new(RenderOptions {
                background: Color::rgba(0, 0, 0, 255),
            })
            .render(&scene)
            .unwrap()
        };

        let frame = render(1.0);
        let pixel = |x: usize| frame.pixels()[x * 4..x * 4 + 4].to_vec();
        assert_eq!(pixel(0), [127, 127, 127, 255]);
        // White multiplied by red is red, which then differs from gray.
        assert_eq!(pixel(1), [127, 128, 128, 255]);
        assert_eq!(pixel(2), [127, 127, 127, 255]);
        assert_eq!(pixel(3), [128, 128, 128, 255]);

        // The group's opacity fades the blended result as a whole.
        let frame = render(0.5);
        assert_eq!(frame.pixels()[0..4], [128, 128, 128, 255]);
        assert_eq!(frame.pixels()[4..8], [128, 128, 128, 255]);
    }

    #[test]
    fn decodes_and_draws_a_real_image() {
        let scene = Scene {
            width: 2,
            height: 2,
            frame_rate: Rational::new(60, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![Layer {
                id: "checker".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 1.0, y: 1.0 },
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
                        id: "checker".to_owned(),
                        location: AssetLocation::File {
                            path: "tests/assets/checker.ppm".to_owned(),
                        },
                    },
                },
            }],
        };
        let mut renderer = CpuRenderer::default().with_asset_root(env!("CARGO_MANIFEST_DIR"));
        let frame = renderer.render(&scene).unwrap();

        assert_eq!(
            frame.pixels(),
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ]
        );
    }

    #[test]
    fn renders_a_video_frame_from_the_injected_decoder() {
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

        let scene = Scene {
            width: 1,
            height: 1,
            frame_rate: Rational::new(60, 1),
            time: Time::new(3, 2),
            fonts: Vec::new(),
            layers: vec![Layer {
                id: "video".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 0.5, y: 0.5 },
                    ..EvaluatedTransform::default()
                },
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
                        local_time: Time::new(1, 2),
                        source_start: Time::new(1, 1),
                        source_time_seconds: 2.0,
                        playback_rate: 2.0,
                    },
                },
            }],
        };
        let mut renderer = CpuRenderer::default().with_video_decoder(Decoder);
        let frame = renderer.render(&scene).unwrap();
        assert_eq!(frame.pixels(), &[12, 34, 56, 255]);
    }

    // `examples/assets/lipsync-fixture.psd` mimics a real "tachie" PSD: a
    // 240x320 canvas with every folder saved hidden and a `face/mouth`
    // group of six small vowel shapes at their true positions. Regenerate it
    // with `packages/react/scripts/make-lipsync-fixture.mjs`.
    fn lipsync_fixture_psd() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/lipsync-fixture.psd")
    }

    fn pixel_at(frame: &RgbaFrame, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * frame.width + x) * 4) as usize;
        frame.pixels[index..index + 4].try_into().unwrap()
    }

    #[test]
    fn rasterize_psd_without_a_preset_renders_nothing_when_every_folder_is_hidden() {
        let frame = rasterize_psd("fixture", &lipsync_fixture_psd(), &[], &[], &[]).unwrap();
        assert_eq!((frame.width, frame.height), (240, 320));
        assert!(
            frame.pixels.chunks_exact(4).all(|pixel| pixel[3] == 0),
            "a saved-all-hidden PSD with no preset should compose to nothing"
        );
    }

    #[test]
    fn rasterize_psd_composes_the_preset_and_the_selected_mouth_at_real_coordinates() {
        let preset = [
            "body".to_owned(),
            "body/base".to_owned(),
            "body/outfit-navy".to_owned(),
            "face".to_owned(),
            "face/eyes".to_owned(),
            "face/eyes/open".to_owned(),
        ];
        let frame = rasterize_psd(
            "fixture",
            &lipsync_fixture_psd(),
            &preset,
            &["face/mouth/a".to_owned()],
            &["face/mouth/o".to_owned()],
        )
        .unwrap();

        // The navy outfit rect covers (56,176)..(184,296).
        assert_eq!(pixel_at(&frame, 120, 220), [40, 60, 130, 255]);
        // The "a" mouth is a 24x20 rect at (108,142) — force-enabled even
        // though its layer and every ancestor folder are saved hidden. Its
        // red channel dominates, unlike the skin behind it.
        let mouth = pixel_at(&frame, 120, 150);
        assert_eq!(mouth[3], 255);
        assert!(mouth[0] > 150 && mouth[0] > mouth[1] + 40 && mouth[0] > mouth[2] + 40);
        // A point clear of every visible layer stays transparent.
        assert_eq!(pixel_at(&frame, 5, 5), [0, 0, 0, 0]);
    }

    #[test]
    fn rasterize_psd_applies_each_layer_blend_mode() {
        // 3×1: an opaque rgb(200, 100, 50) base; over it, one pixel each of a
        // multiply layer of rgb(128, 128, 255), a screen layer of
        // rgb(128, 128, 128), and a normal layer of rgb(10, 20, 30).
        // Written with ag-psd, whose layers the `psd` crate reads as hidden,
        // so they are listed as the visible set.
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blend-modes.psd");
        let layers = ["base", "multiply", "screen", "normal"].map(str::to_owned);
        let frame = rasterize_psd("fixture", &fixture, &layers, &[], &[]).unwrap();
        let close = |actual: [u8; 4], expected: [u8; 4]| {
            actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 1)
        };
        let multiply = pixel_at(&frame, 0, 0);
        assert!(
            close(multiply, [100, 50, 50, 255]),
            "multiply: {multiply:?}"
        );
        let screen = pixel_at(&frame, 1, 0);
        assert!(close(screen, [228, 178, 153, 255]), "screen: {screen:?}");
        assert_eq!(pixel_at(&frame, 2, 0), [10, 20, 30, 255]);
    }

    #[test]
    fn psd_blend_modes_mix_like_photoshop() {
        let mix = |mode| psd_blend_channel(mode).unwrap();
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(psd_blend_channel("Normal").is_none());
        assert!(psd_blend_channel("Hue").is_none());
        assert!(close(mix("Multiply")(0.5, 0.5), 0.25));
        assert!(close(mix("Screen")(0.5, 0.5), 0.75));
        assert!(close(mix("Overlay")(0.25, 0.5), 0.25));
        assert!(close(mix("Darken")(0.3, 0.6), 0.3));
        assert!(close(mix("Lighten")(0.3, 0.6), 0.6));
        assert!(close(mix("LinearDodge")(0.7, 0.6), 1.0));
        assert!(close(mix("Subtract")(0.3, 0.6), 0.0));
        assert!(close(mix("Difference")(0.3, 0.8), 0.5));
        assert!(close(mix("ColorDodge")(0.25, 0.5), 0.5));
        assert!(close(mix("ColorBurn")(0.75, 0.5), 0.5));
        // White and black are neutral where Photoshop says they are.
        for mode in ["Multiply", "ColorBurn", "LinearBurn"] {
            assert!(close(mix(mode)(0.4, 1.0), 0.4), "{mode:?} with white");
        }
        for mode in ["Screen", "ColorDodge", "LinearDodge"] {
            assert!(close(mix(mode)(0.4, 0.0), 0.4), "{mode:?} with black");
        }
    }

    #[test]
    fn rasterize_psd_disabled_layers_win_over_enabled() {
        let mouth = "face/mouth/a".to_owned();
        let with = rasterize_psd(
            "fixture",
            &lipsync_fixture_psd(),
            &["body".to_owned(), "body/base".to_owned()],
            std::slice::from_ref(&mouth),
            &[],
        )
        .unwrap();
        let without = rasterize_psd(
            "fixture",
            &lipsync_fixture_psd(),
            &["body".to_owned(), "body/base".to_owned()],
            std::slice::from_ref(&mouth),
            std::slice::from_ref(&mouth),
        )
        .unwrap();
        assert_ne!(pixel_at(&with, 120, 150), pixel_at(&without, 120, 150));
        // With the mouth suppressed the pixel is the bare skin base.
        assert_eq!(pixel_at(&without, 120, 150), [250, 224, 205, 255]);
    }
}

#[cfg(test)]
mod font_tests {
    use celesta_composition::{AssetLocation, ResolvedAsset};

    use super::*;

    const FAMILY: &str = "Celesta Web Font Test";

    /// A minimal sfnt holding only a `name` table: enough for fontdb to list
    /// a face under `FAMILY`.
    fn sfnt() -> Vec<u8> {
        let utf16 =
            |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(u16::to_be_bytes).collect() };
        let family = utf16(FAMILY);
        let post_script = utf16("CelestaWebFontTest");
        let mut name = Vec::new();
        for value in [0_u16, 2, 6 + 2 * 12] {
            name.extend(value.to_be_bytes());
        }
        // Windows / Unicode BMP / en-US records for family (1) and PostScript (6) names.
        for (name_id, length, offset) in [
            (1_u16, family.len(), 0),
            (6, post_script.len(), family.len()),
        ] {
            for value in [3_u16, 1, 0x409, name_id, length as u16, offset as u16] {
                name.extend(value.to_be_bytes());
            }
        }
        name.extend(&family);
        name.extend(&post_script);

        let mut font = Vec::new();
        font.extend(0x0001_0000_u32.to_be_bytes());
        for value in [1_u16, 16, 0, 0] {
            font.extend(value.to_be_bytes());
        }
        font.extend(b"name");
        font.extend(0_u32.to_be_bytes());
        font.extend(28_u32.to_be_bytes());
        font.extend((name.len() as u32).to_be_bytes());
        font.extend(&name);
        font.resize(font.len().next_multiple_of(4), 0);
        font
    }

    /// The `name` table bytes of `sfnt()`.
    fn name_table() -> Vec<u8> {
        let font = sfnt();
        let length = u32::from_be_bytes(font[24..28].try_into().unwrap()) as usize;
        font[28..28 + length].to_vec()
    }

    /// `sfnt()` as an uncompressed WOFF 1.0 file.
    fn woff1() -> Vec<u8> {
        let table = name_table();
        let padded = table.len().next_multiple_of(4);
        let mut woff = Vec::new();
        woff.extend(b"wOFF");
        woff.extend(0x0001_0000_u32.to_be_bytes());
        woff.extend(((44 + 20 + padded) as u32).to_be_bytes());
        woff.extend(1_u16.to_be_bytes());
        woff.extend(0_u16.to_be_bytes());
        woff.extend(((12 + 16 + padded) as u32).to_be_bytes());
        woff.extend(1_u16.to_be_bytes());
        woff.extend(0_u16.to_be_bytes());
        woff.extend([0; 20]);
        woff.extend(b"name");
        woff.extend(64_u32.to_be_bytes());
        woff.extend((table.len() as u32).to_be_bytes());
        woff.extend((table.len() as u32).to_be_bytes());
        woff.extend(0_u32.to_be_bytes());
        woff.extend(&table);
        woff.resize(44 + 20 + padded, 0);
        woff
    }

    /// `sfnt()` as a WOFF 2.0 file whose Brotli stream is one uncompressed
    /// meta-block.
    fn woff2() -> Vec<u8> {
        let table = name_table();
        // WBITS=16 (0), ISLAST=0, MNIBBLES=4 (00), MLEN-1 (16 bits),
        // ISUNCOMPRESSED=1, then byte-aligned raw bytes and an empty last block.
        let bits = ((table.len() as u32 - 1) << 4) | (1 << 20);
        let mut brotli = bits.to_le_bytes()[..3].to_vec();
        brotli.extend(&table);
        brotli.push(0b11);
        let mut woff = Vec::new();
        woff.extend(b"wOF2");
        woff.extend(0x0001_0000_u32.to_be_bytes());
        let length_at = woff.len();
        woff.extend(0_u32.to_be_bytes());
        woff.extend(1_u16.to_be_bytes());
        woff.extend(0_u16.to_be_bytes());
        woff.extend(((12 + 16 + table.len().next_multiple_of(4)) as u32).to_be_bytes());
        woff.extend((brotli.len() as u32).to_be_bytes());
        woff.extend(1_u16.to_be_bytes());
        woff.extend(0_u16.to_be_bytes());
        woff.extend([0; 20]);
        // Arbitrary tag (63) with the null transform, then UIntBase128 length.
        woff.push(63);
        woff.extend(b"name");
        assert!(table.len() < 128, "one UIntBase128 byte");
        woff.push(table.len() as u8);
        woff.extend(&brotli);
        woff.resize(woff.len().next_multiple_of(4), 0);
        let total = woff.len() as u32;
        woff[length_at..length_at + 4].copy_from_slice(&total.to_be_bytes());
        woff
    }

    fn has_family(rasterizer: &TextRasterizer, name: &str) -> bool {
        rasterizer
            .font_system
            .db()
            .faces()
            .any(|face| face.families.iter().any(|(family, _)| family == name))
    }

    fn file_font(path: &str) -> ResolvedAsset {
        ResolvedAsset {
            id: "brand".to_owned(),
            location: AssetLocation::File {
                path: path.to_owned(),
            },
        }
    }

    #[test]
    fn loads_truetype_woff_and_woff2_fonts() {
        for (name, data) in [("a.ttf", sfnt()), ("a.woff", woff1()), ("a.woff2", woff2())] {
            let directory = tempfile::tempdir().unwrap();
            fs::write(directory.path().join(name), data).unwrap();
            let mut rasterizer = TextRasterizer::new();
            rasterizer
                .load_fonts(&[file_font(name)], directory.path())
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(has_family(&rasterizer, FAMILY), "{name} was not loaded");
        }
    }

    #[test]
    fn loads_stylesheet_faces_under_their_css_family() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("css")).unwrap();
        fs::create_dir(directory.path().join("files")).unwrap();
        fs::write(directory.path().join("files/brand.woff2"), woff2()).unwrap();
        fs::write(
            directory.path().join("css/brand.css"),
            "@font-face {\n  font-family: 'Brand';\n  src: local('Brand'), url('../files/brand.woff2?v=1') format('woff2');\n}\n",
        )
        .unwrap();
        let mut rasterizer = TextRasterizer::new();
        rasterizer
            .load_fonts(&[file_font("css/brand.css")], directory.path())
            .unwrap();
        // Under the family stored in the file and the stylesheet's CSS name.
        assert!(has_family(&rasterizer, FAMILY));
        assert!(has_family(&rasterizer, "Brand"));
    }

    /// A rasterizer with `Bebas Neue`, whose only face is Regular (400).
    fn regular_only_rasterizer() -> TextRasterizer {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/prism");
        let mut rasterizer = TextRasterizer::new();
        rasterizer
            .load_fonts(
                &[file_font("assets/fonts/BebasNeue-Regular.ttf")],
                &examples,
            )
            .unwrap();
        rasterizer
    }

    #[test]
    fn matches_the_nearest_weight_within_a_loaded_family() {
        let mut rasterizer = regular_only_rasterizer();
        let style = |weight| TextStyle {
            font_family: Some("Bebas Neue".to_owned()),
            font_size: Some(48.0),
            font_weight: weight,
            ..TextStyle::default()
        };
        let regular = rasterizer
            .rasterize("CELESTA", &style(Some(400)), None, 1.0)
            .unwrap();
        for weight in [None, Some(100), Some(700), Some(900)] {
            assert_eq!(
                rasterizer.matched_weight("Bebas Neue", weight.unwrap_or(400)),
                Some(400)
            );
            let text = rasterizer
                .rasterize("CELESTA", &style(weight), None, 1.0)
                .unwrap();
            assert!(
                text == regular,
                "weight {weight:?} did not draw with Bebas Neue"
            );
            assert_eq!(rasterizer.font_fallback("title", &style(weight)), None);
        }
    }

    #[test]
    fn reports_a_family_with_no_face() {
        let mut rasterizer = regular_only_rasterizer();
        let style = TextStyle {
            font_family: Some("Celesta Missing Family".to_owned()),
            font_weight: Some(700),
            ..TextStyle::default()
        };
        let fallback = rasterizer.font_fallback("title", &style).unwrap();
        assert_eq!(
            fallback,
            FontFallback {
                layer: "title".to_owned(),
                family: "Celesta Missing Family".to_owned(),
                weight: 700,
            }
        );
        assert_eq!(
            fallback.to_string(),
            "font family \"Celesta Missing Family\" (weight 700) is not installed or loaded; text layer \"title\" uses a fallback font"
        );
        // Still drawn, with a fallback font.
        assert!(rasterizer.rasterize("A", &style, None, 1.0).is_ok());
        // No family means the default font, not a fallback.
        assert_eq!(
            rasterizer.font_fallback("title", &TextStyle::default()),
            None
        );
    }

    fn bebas_style() -> TextStyle {
        TextStyle {
            font_family: Some("Bebas Neue".to_owned()),
            font_size: Some(48.0),
            ..TextStyle::default()
        }
    }

    #[test]
    fn reports_characters_the_family_has_no_glyph_for() {
        let mut rasterizer = regular_only_rasterizer();
        // Bebas Neue has Latin glyphs only: the kana and kanji come from
        // another font, once each and in order.
        let missing = rasterizer
            .missing_glyphs("title", "CELESTA ずんだもん 2026 だ", &bebas_style())
            .unwrap();
        assert_eq!(
            missing,
            MissingGlyphs {
                layer: "title".to_owned(),
                family: "Bebas Neue".to_owned(),
                weight: 400,
                characters: "ずんだも".chars().collect(),
            }
        );
        assert_eq!(
            missing.to_string(),
            "font family \"Bebas Neue\" (weight 400) has no glyph for \"ずんだも\"; text layer \"title\" draws them with a fallback font"
        );

        // A long list is cut short.
        let missing = rasterizer
            .missing_glyphs("title", "あいうえおかきくけこさしすせそ", &bebas_style())
            .unwrap();
        assert_eq!(missing.characters.len(), 15);
        assert_eq!(
            missing.to_string(),
            "font family \"Bebas Neue\" (weight 400) has no glyph for \"あいうえおかきくけこ\" and 5 more characters; text layer \"title\" draws them with a fallback font"
        );
    }

    #[test]
    fn does_not_report_glyphs_the_family_has_or_emoji() {
        let mut rasterizer = regular_only_rasterizer();
        for text in [
            "CELESTA 2026",
            // Whitespace and control characters draw nothing.
            "CELESTA\n\tSTUDIO\u{3000}",
            // Emoji are meant to come from a color emoji font.
            "🎉",
            "CELESTA 🎉👍🏽",
            "❤\u{FE0F}",
            "1\u{FE0F}\u{20E3}",
            "👩\u{200D}💻",
            "🇯🇵",
        ] {
            assert_eq!(
                rasterizer.missing_glyphs("title", text, &bebas_style()),
                None,
                "{text:?}"
            );
        }
        // Not when the text names no family, either.
        assert_eq!(
            rasterizer.missing_glyphs("title", "ずんだもん", &TextStyle::default()),
            None
        );
    }

    #[test]
    fn reports_emoji_no_font_has_a_glyph_for() {
        // Only Bebas Neue, without the system's fonts: there is no color
        // emoji font to fall back to, so the emoji is drawn as a missing
        // glyph box and is reported like any other character.
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/prism");
        let mut rasterizer = TextRasterizer::new();
        rasterizer.font_system =
            FontSystem::new_with_locale_and_db("en-US".to_owned(), fontdb::Database::new());
        rasterizer
            .load_fonts(
                &[file_font("assets/fonts/BebasNeue-Regular.ttf")],
                &examples,
            )
            .unwrap();
        let missing = rasterizer
            .missing_glyphs("title", "CELESTA 🎉 ず ❤\u{FE0F}", &bebas_style())
            .unwrap();
        assert_eq!(missing.characters, ['🎉', 'ず', '❤']);
    }

    #[test]
    fn leaves_a_family_with_no_face_to_the_font_fallback() {
        let mut rasterizer = regular_only_rasterizer();
        let style = TextStyle {
            font_family: Some("Celesta Missing Family".to_owned()),
            ..TextStyle::default()
        };
        assert!(rasterizer.font_fallback("title", &style).is_some());
        assert_eq!(
            rasterizer.missing_glyphs("title", "ずんだもん", &style),
            None
        );
    }

    #[test]
    fn marks_graphemes_meant_as_emoji() {
        let spans = |text: &str| {
            emoji_presentation_spans(text)
                .into_iter()
                .map(|range| text[range].to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(spans("CELESTA ずんだもん 2026"), Vec::<String>::new());
        // Emoji by default, with a skin tone, and ZWJ sequences.
        assert_eq!(spans("A🎉B👍🏽C👩\u{200D}💻"), ["🎉", "👍🏽", "👩\u{200D}💻"]);
        // Text by default, emoji with U+FE0F: a heart and a keycap.
        assert_eq!(
            spans("❤ ❤\u{FE0F} 1 1\u{FE0F}\u{20E3}"),
            ["❤\u{FE0F}", "1\u{FE0F}\u{20E3}"]
        );
        // U+FE0E keeps an emoji-by-default character text.
        assert_eq!(spans("☔\u{FE0E}"), Vec::<String>::new());
        // An emoji in a grapheme that starts with a prepended character.
        assert_eq!(spans("\u{600}🎉"), ["\u{600}🎉"]);
        // A flag's regional indicators, and adjacent emoji, share one span.
        assert_eq!(spans("🇯🇵🎉 x"), ["🇯🇵🎉"]);
    }

    /// The family of the face each glyph cluster of `text` is drawn with.
    fn cluster_families(
        rasterizer: &mut TextRasterizer,
        text: &str,
        style: &TextStyle,
    ) -> Vec<(String, String)> {
        let buffer = rasterizer.shaped_buffer(text, style, None, 1.0);
        let database = rasterizer.font_system.db();
        let mut clusters = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let family = database
                    .face(glyph.font_id)
                    .and_then(|face| face.families.first())
                    .map_or_else(String::new, |(family, _)| family.clone());
                clusters.push((run.text[glyph.start..glyph.end].to_owned(), family));
            }
        }
        clusters
    }

    #[test]
    fn draws_emoji_presentation_with_the_color_emoji_font() {
        let mut rasterizer = regular_only_rasterizer();
        let Some(emoji_family) = rasterizer.color_emoji_family() else {
            eprintln!("skipping: no color emoji font is installed");
            return;
        };
        for weight in [None, Some(700)] {
            let style = TextStyle {
                font_family: Some("Bebas Neue".to_owned()),
                font_size: Some(48.0),
                font_weight: weight,
                ..TextStyle::default()
            };
            // Text fonts have a plain heart, keycap, and regional indicator
            // letters too; these still come from the color emoji font.
            for emoji in ["❤\u{FE0F}", "1\u{FE0F}\u{20E3}", "🇯🇵", "🎉"] {
                let text = format!("A{emoji}B");
                let clusters = cluster_families(&mut rasterizer, &text, &style);
                let families = clusters
                    .iter()
                    .map(|(_, family)| family.as_str())
                    .collect::<Vec<_>>();
                assert_eq!(families.first(), Some(&"Bebas Neue"), "{clusters:?}");
                assert_eq!(families.last(), Some(&"Bebas Neue"), "{clusters:?}");
                assert!(
                    families[1..families.len() - 1]
                        .iter()
                        .all(|family| *family == emoji_family),
                    "{weight:?} {emoji:?} was not drawn with {emoji_family}: {clusters:?}"
                );
            }
        }
    }

    #[test]
    fn keeps_the_line_structure_of_text_with_emoji() {
        let mut rasterizer = regular_only_rasterizer();
        if rasterizer.color_emoji_family().is_none() {
            eprintln!("skipping: no color emoji font is installed");
            return;
        }
        let style = TextStyle {
            font_family: Some("Bebas Neue".to_owned()),
            font_size: Some(48.0),
            line_height: Some(60.0),
            ..TextStyle::default()
        };
        // A trailing newline adds an empty last line, emoji or not.
        for (plain, emoji) in [
            ("A\n", "A❤\u{FE0F}\n"),
            ("A\nB", "A🎉\nB"),
            ("A\r\nB\r\n", "A🎉\r\nB🇯🇵\r\n"),
        ] {
            let plain_metrics = rasterizer.measure(plain, &style, None);
            let emoji_metrics = rasterizer.measure(emoji, &style, None);
            assert_eq!(
                (emoji_metrics.lines, emoji_metrics.height),
                (plain_metrics.lines, plain_metrics.height),
                "{emoji:?}"
            );
        }
    }

    #[test]
    fn rejects_files_that_hold_no_font() {
        let directory = tempfile::tempdir().unwrap();
        let cases: [(&str, &[u8], &str); 3] = [
            ("broken.woff2", b"wOF2 not really", "invalid WOFF2 data"),
            ("notes.txt", b"not a font", "no font faces found"),
            (
                "empty.css",
                b"@font-face { font-family: X }",
                "no @font-face url()",
            ),
        ];
        for (name, data, reason) in cases {
            fs::write(directory.path().join(name), data).unwrap();
            let error = TextRasterizer::new()
                .load_fonts(&[file_font(name)], directory.path())
                .unwrap_err();
            assert!(error.to_string().contains(reason), "{name}: {error}");
        }
    }
}
