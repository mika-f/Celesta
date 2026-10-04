use crate::composite::{blend, composite_mask_with, composite_rgba};
use crate::error::RenderError;
use crate::fonts::{COLOR_EMOJI_FAMILIES, TextRasterizer};
use crate::linebreak::{
    PhraseSegment, WORD_JOINER, emoji_presentation_spans, insert_joiners, phrase_segments,
};
use crate::paint::{ResolvedPaint, resolve_paint};
use crate::rect::dilate_mask;
use crate::text::{AnchorBox, GlyphMetrics, RasterizedText, TextMetrics};
use crate::types::{Color, RgbaFrame};
use celesta_composition::{LineBreak, TextAlign, TextStyle};
use cosmic_text::{
    Align, Attrs, AttrsList, Buffer, BufferLine, Color as CosmicColor, Family, LineIter, Metrics,
    Shaping, Weight, Wrap,
};

impl TextRasterizer {
    /// Shapes `text` into a laid-out buffer. `width` and `scale` are in
    /// output pixels: `scale` multiplies the style's font size and line height.
    pub(crate) fn shaped_buffer(
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
        // `lineBreak: phrase` joins every phrase segment before the text is
        // shaped, so the joined text, as it is laid out, is shaped once. Each
        // line's original text and segments are kept to measure them below.
        // Without a width nothing wraps, so nothing is joined.
        let mut phrase_lines: Vec<(&str, Vec<PhraseSegment>)> = Vec::new();
        let joined;
        let text = if style.line_break == Some(LineBreak::Phrase) && width.is_some() {
            let mut joined_text = String::with_capacity(text.len() * 2);
            // The lines `Buffer::set_text` makes.
            for (range, ending) in LineIter::new(text) {
                let line = &text[range];
                let segments = phrase_segments(line);
                let joiners = segments.iter().flat_map(|segment| &segment.joiners);
                joined_text.push_str(&insert_joiners(line, joiners.copied()));
                joined_text.push_str(ending.as_str());
                phrase_lines.push((line, segments));
            }
            joined = joined_text;
            joined.as_str()
        } else {
            text
        };
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
        let emoji_attrs = emoji_family.as_deref().map(|family| {
            let weight = self
                .matched_weight(family, requested_weight)
                .unwrap_or(requested_weight);
            attrs
                .clone()
                .family(Family::Name(family))
                .weight(Weight(weight))
        });
        // A word joiner draws nothing, so it gets no letter spacing either.
        // Letter spacing does not split a shaping run, so the span leaves the
        // shaping as it is.
        let joiner_attrs = style
            .letter_spacing
            .map(|_| attrs.clone().letter_spacing(0.0));
        // Added to the lines `set_text` made rather than passed to
        // `set_rich_text`, which splits lines differently (dropping the
        // empty line after a trailing newline).
        let add_spans = |line: &mut BufferLine| {
            let mut spans: Vec<(std::ops::Range<usize>, &Attrs)> = Vec::new();
            if let Some(emoji_attrs) = &emoji_attrs {
                for range in emoji_presentation_spans(line.text()) {
                    spans.push((range, emoji_attrs));
                }
            }
            if let Some(joiner_attrs) = &joiner_attrs {
                for (start, joiner) in line.text().match_indices(WORD_JOINER) {
                    spans.push((start..start + joiner.len(), joiner_attrs));
                }
            }
            if spans.is_empty() {
                return;
            }
            let mut attrs_list = line.attrs_list().clone();
            for (range, attrs) in spans {
                attrs_list.add_span(range, attrs);
            }
            line.set_attrs_list(attrs_list);
        };
        for line in &mut buffer.lines {
            add_spans(line);
        }

        if let Some(width) = width
            && phrase_lines
                .iter()
                .any(|(_, segments)| !segments.is_empty())
        {
            // Lay the joined lines out unwrapped, to see how wide each
            // segment is as it is shaped: joining makes a phrase one word,
            // which can change its kerning and fallback fonts. A segment
            // wider than the line loses its joiners and wraps as `normal`
            // text does. Laying out again keeps the shaping, so only lines
            // whose joiners change are shaped again.
            buffer.set_size(&mut self.font_system, None, None);
            buffer.shape_until_scroll(&mut self.font_system, false);
            let mut glyphs = vec![Vec::new(); buffer.lines.len()];
            for run in buffer.layout_runs() {
                glyphs[run.line_i].extend(run.glyphs.iter().map(|glyph| (glyph.start, glyph.w)));
            }
            for ((line, (original, segments)), mut glyphs) in
                buffer.lines.iter_mut().zip(&phrase_lines).zip(glyphs)
            {
                if segments.is_empty() {
                    continue;
                }
                glyphs.sort_by_key(|&(start, _)| start);
                // `advances[i]` is the width of the line's first `i` glyphs.
                let advances: Vec<f32> = std::iter::once(0.0)
                    .chain(glyphs.iter().scan(0.0, |advance, &(_, width)| {
                        *advance += width;
                        Some(*advance)
                    }))
                    .collect();
                let joiners: Vec<usize> = segments
                    .iter()
                    .flat_map(|segment| segment.joiners.iter().copied())
                    .collect();
                // Where `offset` in the original line is in the joined one.
                let joined_offset = |offset: usize| {
                    offset
                        + WORD_JOINER.len_utf8()
                            * joiners.partition_point(|&joiner| joiner < offset)
                };
                let advance_to = |offset: usize| {
                    let offset = joined_offset(offset);
                    advances[glyphs.partition_point(|&(start, _)| start < offset)]
                };
                let fits = |segment: &&PhraseSegment| {
                    // Whitespace at the end of a line hangs past its width.
                    let text = &original[segment.range.clone()];
                    let end = segment.range.start + text.trim_end().len();
                    advance_to(end) - advance_to(segment.range.start) <= width
                };
                if !segments.iter().all(|segment| fits(&segment)) {
                    let joiners = segments
                        .iter()
                        .filter(fits)
                        .flat_map(|segment| segment.joiners.iter().copied());
                    let ending = line.ending();
                    line.set_text(
                        insert_joiners(original, joiners),
                        ending,
                        AttrsList::new(&attrs),
                    );
                    line.set_align(alignment);
                    add_spans(line);
                }
            }
            buffer.set_size(&mut self.font_system, Some(width), None);
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
                // The word joiners `lineBreak: phrase` adds are not text.
                let cluster = if style.line_break == Some(LineBreak::Phrase) {
                    cluster.replace(WORD_JOINER, "")
                } else {
                    cluster.to_owned()
                };
                if cluster.is_empty() {
                    continue;
                }
                metrics.glyphs.push(GlyphMetrics {
                    text: cluster,
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

/// Crops the fully transparent rows above and below the ink, then pads one
/// empty row on top when needed to keep the height even. Returns the frame and
/// how many rows its top edge moved down (negative when padded).
pub(crate) fn trim_transparent_rows(frame: RgbaFrame) -> (RgbaFrame, i32) {
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
