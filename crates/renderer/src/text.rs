use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct RasterizedText {
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Pixel row of the first line's baseline, from the top edge.
    pub(crate) baseline: f32,
    pub(crate) pixels: Vec<u8>,
    /// The box a layer's anchor refers to, in image pixels. For text it is the
    /// layout box (the advance width, and the line boxes or the visible rows
    /// of a single line), which a stroke can reach past, so the image may be
    /// larger. For everything else it is the whole image.
    pub(crate) anchor_box: AnchorBox,
}

/// A rectangle in image pixels; see [`RasterizedText::anchor_in_image`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AnchorBox {
    pub(crate) left: u32,
    pub(crate) top: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
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
    /// Half-open Unicode code-point range in the complete source text.
    pub start: usize,
    pub end: usize,
    pub rtl: bool,
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
    pub(crate) const LISTED: usize = 10;
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
