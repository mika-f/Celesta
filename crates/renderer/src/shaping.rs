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
use celesta_composition::{LineBreak, TextAlign, TextFontRun, TextStyle};
use cosmic_text::{
    Align, Attrs, AttrsList, Buffer, BufferLine, Color as CosmicColor, Family, FontSystem,
    LayoutGlyph, LineEnding, LineIter, Metrics, PhysicalGlyph, Renderer, Shaping, SwashCache,
    SwashContent, Weight, Wrap,
};
use std::sync::Arc;

impl TextRasterizer {
    /// Shapes `text` into a laid-out buffer. `width` and `scale` are in
    /// output pixels: `scale` multiplies the style's font size and line height.
    pub(crate) fn shaped_buffer(
        &mut self,
        text: &str,
        style: &TextStyle,
        width: Option<f32>,
        scale: f32,
    ) -> Arc<Buffer> {
        let source = text;
        self.select_language(style.lang.as_deref());
        let key = (text.len() <= 100_000).then(|| {
            format!(
                "{text:?}\0{:?}\0{width:?}\0{scale:?}",
                (
                    &style.lang,
                    &style.font_family,
                    style.font_size,
                    style.font_weight,
                    style.align,
                    style.line_height,
                    style.letter_spacing,
                    style.line_break,
                    &style.font_runs,
                )
            )
        });
        if let Some(buffer) = key.as_ref().and_then(|key| self.shaped_buffers.get(key)) {
            return Arc::clone(buffer);
        }
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
        // Font runs become per-range attributes: the run's family (or the
        // text's) at the weight CSS matching picks within that family.
        let font_runs = if style.font_runs.is_empty() {
            Vec::new()
        } else {
            usable_font_runs(style, source.chars().count())
        };
        let mut run_attrs: Vec<(Attrs, u16)> = Vec::with_capacity(font_runs.len());
        for run in &font_runs {
            let family = run.font_family.as_deref().or(style.font_family.as_deref());
            let requested = run.font_weight.or(style.font_weight).unwrap_or(400);
            let weight = family
                .and_then(|family| self.matched_weight(family, requested))
                .unwrap_or(requested);
            let mut run_attr = attrs.clone().weight(Weight(weight));
            if let Some(family) = family {
                run_attr = run_attr.family(Family::Name(family));
            }
            run_attrs.push((run_attr, requested));
        }
        // Each buffer line's source text and the code point it starts at,
        // with the empty line `set_text` adds after a trailing line ending.
        let mut line_sources: Vec<(&str, usize)> = Vec::new();
        if !font_runs.is_empty() {
            let mut base = 0;
            for (range, ending) in LineIter::new(source) {
                line_sources.push((&source[range.clone()], base));
                base += source[range].chars().count() + ending.as_str().chars().count();
            }
            line_sources.push(("", base));
        }
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
        // The emoji font's weight depends on the weight asked for where an
        // emoji sits: the text's, or a font run's.
        let mut emoji_weights: Vec<(u16, u16)> = Vec::new();
        if let Some(family) = emoji_family.as_deref() {
            for requested in std::iter::once(requested_weight)
                .chain(run_attrs.iter().map(|(_, requested)| *requested))
            {
                if !emoji_weights.iter().any(|(asked, _)| *asked == requested) {
                    let matched = self.matched_weight(family, requested).unwrap_or(requested);
                    emoji_weights.push((requested, matched));
                }
            }
        }
        // Added to the lines `set_text` made rather than passed to
        // `set_rich_text`, which splits lines differently (dropping the
        // empty line after a trailing newline). Emoji and word-joiner spans
        // start from the attributes in effect where they sit, so they never
        // switch a font run back to the text's font and split its shaping.
        let add_spans = |line_i: usize, line: &mut BufferLine| {
            let runs = line_sources
                .get(line_i)
                .map(|&(original, base)| font_run_spans(line.text(), original, base, &font_runs))
                .unwrap_or_default();
            let attrs_at = |at: usize| {
                runs.iter()
                    .find(|(range, _)| range.contains(&at))
                    .map_or((&attrs, requested_weight), |(_, index)| {
                        (&run_attrs[*index].0, run_attrs[*index].1)
                    })
            };
            // cosmic-text shapes an ASCII word in one piece when every added
            // span it overlaps matches the word's first attributes, ignoring
            // parts left at the defaults: "CD" starting in a run would come
            // out in the run's font throughout. Spelling the defaults out as
            // a span over a line with runs keeps such words split.
            let mut spans: Vec<(std::ops::Range<usize>, Attrs)> = if runs.is_empty() {
                Vec::new()
            } else {
                vec![(0..line.text().len(), attrs.clone())]
            };
            spans.extend(
                runs.iter()
                    .map(|(range, index)| (range.clone(), run_attrs[*index].0.clone())),
            );
            if let Some(family) = emoji_family.as_deref() {
                for range in emoji_presentation_spans(line.text()) {
                    let (at, requested) = attrs_at(range.start);
                    let weight = emoji_weights
                        .iter()
                        .find(|(asked, _)| *asked == requested)
                        .map_or(requested, |(_, matched)| *matched);
                    spans.push((
                        range,
                        at.clone()
                            .family(Family::Name(family))
                            .weight(Weight(weight)),
                    ));
                }
            }
            // A word joiner draws nothing, so it gets no letter spacing
            // either. Letter spacing does not split a shaping run, so the
            // span leaves the shaping as it is.
            if style.letter_spacing.is_some() {
                for (start, joiner) in line.text().match_indices(WORD_JOINER) {
                    let (at, _) = attrs_at(start);
                    spans.push((start..start + joiner.len(), at.clone().letter_spacing(0.0)));
                }
            }
            if spans.is_empty() {
                return;
            }
            let mut attrs_list = line.attrs_list().clone();
            for (range, attrs) in spans {
                attrs_list.add_span(range, &attrs);
            }
            line.set_attrs_list(attrs_list);
        };
        for (line_i, line) in buffer.lines.iter_mut().enumerate() {
            add_spans(line_i, line);
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
            for (line_i, ((line, (original, segments)), mut glyphs)) in buffer
                .lines
                .iter_mut()
                .zip(&phrase_lines)
                .zip(glyphs)
                .enumerate()
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
                    add_spans(line_i, line);
                }
            }
            buffer.set_size(&mut self.font_system, Some(width), None);
        }
        buffer.shape_until_scroll(&mut self.font_system, false);
        let buffer = Arc::new(buffer);
        // Bound retained layouts; long documents do not grow the cache forever.
        if let Some(key) = key {
            if self.shaped_buffers.len() >= 32 {
                self.shaped_buffers.clear();
            }
            self.shaped_buffers.insert(key, Arc::clone(&buffer));
        }
        buffer
    }

    /// Each layout run's baseline when `style` has font runs, which do not
    /// move baselines: placed as cosmic-text places them, but from the
    /// largest ascent and descent of the glyphs outside font runs on the
    /// line, or in the whole text for a line whose glyphs are all in runs.
    /// A line with no glyphs keeps cosmic-text's. `None` without font runs,
    /// or when every glyph is in a run, so cosmic-text's baselines stand.
    pub(crate) fn run_baselines(
        &mut self,
        buffer: &Buffer,
        text: &str,
        style: &TextStyle,
    ) -> Option<Vec<f32>> {
        if style.font_runs.is_empty() {
            return None;
        }
        let runs = usable_font_runs(style, text.chars().count());
        let offsets = source_offsets(buffer, text, style);
        let in_run = |point: usize| {
            let index = runs.partition_point(|run| run.end <= point);
            runs.get(index).is_some_and(|run| run.start <= point)
        };
        let widen = |extent: Option<(f32, f32)>, (ascent, descent): (f32, f32)| {
            Some(extent.map_or((ascent, descent), |(a, d): (f32, f32)| {
                (a.max(ascent), d.max(descent))
            }))
        };
        let mut lines = Vec::new();
        let mut overall = None;
        for layout in buffer.layout_runs() {
            let mut extent = None;
            for glyph in layout.glyphs {
                if in_run(offsets[layout.line_i][glyph.start]) {
                    continue;
                }
                if let Some(glyph_extent) = self.glyph_extent(glyph) {
                    extent = widen(extent, glyph_extent);
                }
            }
            if let Some(line_extent) = extent {
                overall = widen(overall, line_extent);
            }
            let has_glyphs = !layout.glyphs.is_empty();
            lines.push((
                layout.line_top,
                layout.line_height,
                layout.line_y,
                has_glyphs,
                extent,
            ));
        }
        let overall = overall?;
        Some(
            lines
                .into_iter()
                .map(|(top, height, line_y, has_glyphs, extent)| {
                    if !has_glyphs {
                        return line_y;
                    }
                    let (ascent, descent) = extent.unwrap_or(overall);
                    top + (height - (ascent + descent)) / 2.0 + ascent
                })
                .collect(),
        )
    }

    /// A glyph's ascent and descent in pixels, from its face's metrics,
    /// computed as cosmic-text computes them for its line layout.
    pub(crate) fn glyph_extent(&mut self, glyph: &LayoutGlyph) -> Option<(f32, f32)> {
        let font = self
            .font_system
            .get_font(glyph.font_id, glyph.font_weight)?;
        let metrics = font.metrics();
        let units = f32::from(metrics.units_per_em);
        Some((
            glyph.font_size * (metrics.ascent / units),
            glyph.font_size * (-metrics.descent / units),
        ))
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
        let offsets = source_offsets(&buffer, text, style);
        let boundaries: Vec<_> = text
            .char_indices()
            .map(|(byte, _)| byte)
            .chain(std::iter::once(text.len()))
            .collect();
        let baselines = self.run_baselines(&buffer, text, style);
        let mut metrics = TextMetrics::default();
        for (index, run) in buffer.layout_runs().enumerate() {
            metrics.width = metrics.width.max(f64::from(run.line_w));
            metrics.height = metrics
                .height
                .max(f64::from(run.line_top + run.line_height));
            if metrics.lines == 0 {
                let line_y = baselines
                    .as_ref()
                    .map_or(run.line_y, |baselines| baselines[index]);
                metrics.ascent = f64::from(line_y - run.line_top);
                metrics.descent = f64::from(run.line_height) - metrics.ascent;
                metrics.line_height = f64::from(run.line_height);
            }
            let start = run
                .glyphs
                .iter()
                .map(|glyph| glyph.start)
                .min()
                .unwrap_or(0);
            metrics.line_starts.push(offsets[run.line_i][start]);
            metrics.lines += 1;
            // Cluster-wise: a ligature or combined glyph has one entry.
            for glyph in run.glyphs {
                let start = offsets[run.line_i][glyph.start];
                let end = offsets[run.line_i][glyph.end];
                let cluster = &text[boundaries[start]..boundaries[end]];
                if cluster.is_empty() {
                    continue;
                }
                metrics.glyphs.push(GlyphMetrics {
                    start: offsets[run.line_i][glyph.start],
                    end: offsets[run.line_i][glyph.end],
                    rtl: glyph.level.is_rtl(),
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
        validate_font_runs(text, style)?;
        let scale = scale.abs();
        let width = max_width.map(|width| width as f32 * scale);
        let buffer = self.shaped_buffer(text, style, width, scale);

        let measured_width = buffer
            .layout_runs()
            .fold(0.0_f32, |width, run| width.max(run.line_w));
        let measured_height = buffer.layout_runs().fold(0.0_f32, |height, run| {
            height.max(run.line_top + run.line_height)
        });
        let baselines = self.run_baselines(&buffer, text, style);
        let line_y = |index: usize, line_y: f32| {
            baselines
                .as_ref()
                .map_or(line_y, |baselines| baselines[index])
        };
        let baseline = buffer
            .layout_runs()
            .next()
            .map_or(0.0, |run| line_y(0, run.line_y));
        // With font runs, a run's glyphs can reach past the line boxes the
        // text's own glyphs fit; leave room for them above and below.
        let (overflow_top, overflow_bottom) = match &baselines {
            None => (0, 0),
            Some(baselines) => {
                let mut top = 0.0_f32;
                let mut bottom = 0.0_f32;
                for (index, run) in buffer.layout_runs().enumerate() {
                    for glyph in run.glyphs {
                        if let Some((ascent, descent)) = self.glyph_extent(glyph) {
                            top = top.max(ascent - baselines[index]);
                            bottom = bottom.max(baselines[index] + descent - measured_height);
                        }
                    }
                }
                (top.ceil() as u32, bottom.ceil() as u32)
            }
        };
        let layout_width = width.unwrap_or(measured_width).ceil().max(1.0) as u32;
        let layout_height = measured_height.ceil().max(1.0) as u32;
        // The stroke grows the glyphs by its width in every direction, which
        // reaches past the layout box at the first and last glyph (and above
        // and below tall or low glyphs), so leave that much room around it.
        let stroke_radius = style.stroke.as_ref().map_or(0, |stroke| {
            (stroke.width * f64::from(scale)).round().max(0.0) as u32
        });
        let pad = stroke_radius;
        // Rows above the layout box's top edge in the image.
        let pad_top = pad + overflow_top;
        let mask_width = layout_width + 2 * pad;
        let mask_height = layout_height + 2 * pad + overflow_top + overflow_bottom;
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
        let offsets = (!style.color_runs.is_empty() || style.visible_characters.is_some())
            .then(|| source_offsets(&buffer, text, style));
        let count = if style.color_runs.is_empty() {
            0
        } else {
            text.chars().count()
        };
        let mut colors = Vec::with_capacity(style.color_runs.len());
        let mut previous_end = 0;
        for (index, run) in style.color_runs.iter().enumerate() {
            if run.start < previous_end || run.end < run.start || run.end > count {
                return Err(RenderError::InvalidTextColorRun {
                    index,
                    start: run.start,
                    end: run.end,
                    text_length: count,
                });
            }
            colors.push(Color::from_hex(&run.color)?);
            previous_end = run.end;
        }
        let mut ink_rows: Option<(usize, usize)> = None;
        let mut renderer = GlyphPixelRenderer {
            font_system: &mut self.font_system,
            cache: &mut self.swash_cache,
            gradient: None,
            visible: true,
            callback: |x: i32, y: i32, coverage: u8, color: CosmicColor, visible: bool| {
                let pixel_x = x + pad as i32;
                let pixel_y = y + pad_top as i32;
                if pixel_x < 0
                    || pixel_y < 0
                    || pixel_x >= mask_width as i32
                    || pixel_y >= mask_height as i32
                {
                    return;
                }
                let row = pixel_y as usize;
                ink_rows =
                    Some(ink_rows.map_or((row, row), |(min, max)| (min.min(row), max.max(row))));
                if !visible {
                    return;
                }
                let offset = row * mask_width as usize + pixel_x as usize;
                mask[offset] = mask[offset].max(coverage);
                let pixel_offset = offset * 4;
                blend(
                    &mut glyph_pixels[pixel_offset..pixel_offset + 4],
                    Color::rgba(color.r(), color.g(), color.b(), color.a()),
                    1.0,
                );
            },
        };
        for (line_index, run) in buffer.layout_runs().enumerate() {
            let run_y = line_y(line_index, run.line_y);
            for glyph in run.glyphs {
                let start = offsets
                    .as_ref()
                    .map_or(0, |offsets| offsets[run.line_i][glyph.start]);
                renderer.visible = style
                    .visible_characters
                    .is_none_or(|visible| start < visible);
                let index = style.color_runs.partition_point(|run| run.end <= start);
                let override_color = style
                    .color_runs
                    .get(index)
                    .filter(|run| run.start <= start)
                    .map(|_| colors[index]);
                let color = override_color.unwrap_or(fill);
                renderer.gradient = if override_color.is_none() {
                    fill_paint.as_ref().filter(|paint| paint.solid().is_none())
                } else {
                    None
                };
                renderer.glyph(
                    glyph.physical((0.0, run_y), 1.0),
                    CosmicColor::rgba(color.red, color.green, color.blue, color.alpha),
                );
            }
        }

        // Without a stroke the glyphs are the frame as they are: drawn onto
        // nothing, `blend` copies each pixel, and every transparent glyph
        // pixel is all zeros.
        let mut frame = RgbaFrame {
            width: mask_width,
            height: mask_height,
            pixels: glyph_pixels,
        };
        if let Some(stroke) = &style.stroke
            && stroke_radius > 0
        {
            let glyph_pixels = std::mem::replace(
                &mut frame.pixels,
                vec![0; mask_width as usize * mask_height as usize * 4],
            );
            let stroke_mask = dilate_mask(&mask, mask_width, mask_height, stroke_radius);
            let stroke_paint = ResolvedPaint::from_paint(&stroke.paint)?.scaled(f64::from(scale));
            composite_mask_with(&mut frame, &stroke_mask, mask_width, |x, y| {
                stroke_paint.color_at(
                    f64::from(x) - f64::from(pad) + 0.5,
                    f64::from(y) - f64::from(pad_top) + 0.5,
                )
            });
            composite_rgba(&mut frame, &glyph_pixels, mask_width, mask_height, 0, 0);
        }
        // Single-line text keeps its advance width, so leading and trailing
        // spaces still take up room, but drops the empty rows above and below
        // its ink: `anchorY` 0.5 centers the letters, not the line box.
        let mut top = 0;
        let single_line = !text.contains('\n');
        if single_line {
            if (!style.color_runs.is_empty() || style.visible_characters.is_some())
                && let Some((min, max)) = ink_rows
            {
                let min = min.saturating_sub(stroke_radius as usize);
                let max = (max + stroke_radius as usize).min(mask_height as usize - 1);
                (frame, top) = trim_rows(frame, min, max);
            } else {
                (frame, top) = trim_transparent_rows(frame);
            }
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
                top: pad_top,
                width: layout_width,
                height: layout_height,
            }
        };
        Ok(RasterizedText {
            width: frame.width,
            height: frame.height,
            baseline: baseline + pad_top as f32 - top as f32,
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
    trim_rows(frame, min_y, max_y)
}

fn trim_rows(frame: RgbaFrame, min_y: usize, max_y: usize) -> (RgbaFrame, i32) {
    let row_bytes = frame.width as usize * 4;
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

/// Draws glyphs pixel by pixel like cosmic-text's `Buffer::draw`, but applies
/// the base color's alpha to ordinary (mask) glyphs, which `draw` drops.
/// Color glyphs (emoji) keep their own pixels. `callback` receives each
/// pixel's position, its unscaled coverage, and its color.
struct GlyphPixelRenderer<'a, F: FnMut(i32, i32, u8, CosmicColor, bool)> {
    font_system: &'a mut FontSystem,
    cache: &'a mut SwashCache,
    gradient: Option<&'a ResolvedPaint>,
    visible: bool,
    callback: F,
}

impl<F: FnMut(i32, i32, u8, CosmicColor, bool)> Renderer for GlyphPixelRenderer<'_, F> {
    fn rectangle(&mut self, x: i32, y: i32, w: u32, h: u32, color: CosmicColor) {
        for offset_y in 0..h as i32 {
            for offset_x in 0..w as i32 {
                (self.callback)(x + offset_x, y + offset_y, color.a(), color, self.visible);
            }
        }
    }

    fn glyph(&mut self, glyph: PhysicalGlyph, color: CosmicColor) {
        let Self {
            font_system,
            cache,
            gradient,
            visible,
            callback,
        } = self;
        let mask = matches!(
            cache.get_image(font_system, glyph.cache_key),
            Some(image) if matches!(image.content, SwashContent::Mask)
        );
        cache.with_pixels(font_system, glyph.cache_key, color, |x, y, pixel| {
            let coverage = pixel.a();
            // Nothing to draw: the mask keeps its larger coverage, and
            // `blend` keeps the pixel below (all zeros if transparent, as the
            // glyph buffer's transparent pixels are).
            if coverage == 0 {
                return;
            }
            let pixel = if mask {
                let alpha = (f64::from(coverage) * f64::from(color.a()) / 255.0).round() as u8;
                if let Some(gradient) = gradient {
                    let color = gradient
                        .color_at(f64::from(glyph.x + x) + 0.5, f64::from(glyph.y + y) + 0.5);
                    let alpha =
                        (f64::from(coverage) * f64::from(color.alpha) / 255.0).round() as u8;
                    CosmicColor::rgba(color.red, color.green, color.blue, alpha)
                } else {
                    CosmicColor::rgba(pixel.r(), pixel.g(), pixel.b(), alpha)
                }
            } else {
                pixel
            };
            callback(glyph.x + x, glyph.y + y, coverage, pixel, *visible);
        });
    }
}

/// Translate shaping's line-local UTF-8 offsets to full-source code points.
pub(crate) fn source_offsets(buffer: &Buffer, text: &str, style: &TextStyle) -> Vec<Vec<usize>> {
    let mut base = 0;
    buffer
        .lines
        .iter()
        .zip(LineIter::new(text).chain(std::iter::once((text.len()..text.len(), LineEnding::None))))
        .map(|(line, (range, ending))| {
            let mut offsets = vec![base; line.text().len() + 1];
            let mut point = base;
            let mut original = text[range.clone()].chars().peekable();
            for (byte, character) in line.text().char_indices() {
                offsets[byte..byte + character.len_utf8()].fill(point);
                if style.line_break != Some(LineBreak::Phrase)
                    || character != WORD_JOINER
                    || original.peek() == Some(&WORD_JOINER)
                {
                    original.next();
                    point += 1;
                }
            }
            offsets[line.text().len()] = point;
            base += text[range].chars().count() + ending.as_str().chars().count();
            offsets
        })
        .collect()
}

/// Checks `style`'s font runs against `text`: ordered, non-overlapping
/// code-point ranges within it.
pub fn validate_font_runs(text: &str, style: &TextStyle) -> Result<(), RenderError> {
    if style.font_runs.is_empty() {
        return Ok(());
    }
    let count = text.chars().count();
    let mut previous_end = 0;
    for (index, run) in style.font_runs.iter().enumerate() {
        if run.start < previous_end || run.end < run.start || run.end > count {
            return Err(RenderError::InvalidTextFontRun {
                index,
                start: run.start,
                end: run.end,
                text_length: count,
            });
        }
        previous_end = run.end;
    }
    Ok(())
}

/// Where each code point of `original` starts in `shaped`, then where the
/// line ends. `shaped` is `original` with any word joiners phrase line
/// breaking inserted, told apart from original ones as `source_offsets`
/// does. A code point starts at the first joiner inserted before it, since
/// `source_offsets` counts those joiners as its own: a run then takes the
/// joiners before its first character and leaves the ones before the
/// character after it to that character.
fn shaped_byte_offsets(shaped: &str, original: &str) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(original.len() + 1);
    let mut original = original.chars().peekable();
    let mut joiners = None;
    for (byte, character) in shaped.char_indices() {
        if character == WORD_JOINER && original.peek() != Some(&WORD_JOINER) {
            joiners.get_or_insert(byte);
            continue;
        }
        offsets.push(joiners.take().unwrap_or(byte));
        original.next();
    }
    offsets.push(joiners.unwrap_or(shaped.len()));
    offsets
}

/// The byte range of `shaped` (a buffer line holding source text `original`,
/// which starts at code point `base`) that each font run covers, with the
/// run's index in `runs`. cosmic-text applies a cluster's attributes from
/// its first byte, so a range starting inside a cluster leaves it alone.
fn font_run_spans(
    shaped: &str,
    original: &str,
    base: usize,
    runs: &[&TextFontRun],
) -> Vec<(std::ops::Range<usize>, usize)> {
    if runs.is_empty() {
        return Vec::new();
    }
    let offsets = shaped_byte_offsets(shaped, original);
    let end = base + offsets.len() - 1;
    runs.iter()
        .enumerate()
        .filter(|(_, run)| run.start < end && run.end > base)
        .map(|(index, run)| {
            let start = run.start.max(base) - base;
            let stop = run.end.min(end) - base;
            (offsets[start]..offsets[stop], index)
        })
        .collect()
}

/// The font runs of `style` that are well-formed for a text of `length`
/// code points, in order. Runs `validate_font_runs` rejects are left out,
/// so shaping never panics on them; empty runs are left out too.
pub(crate) fn usable_font_runs(style: &TextStyle, length: usize) -> Vec<&TextFontRun> {
    let mut previous_end = 0;
    style
        .font_runs
        .iter()
        .filter(|run| {
            let usable = run.start >= previous_end && run.start < run.end && run.end <= length;
            if usable {
                previous_end = run.end;
            }
            usable
        })
        .collect()
}
