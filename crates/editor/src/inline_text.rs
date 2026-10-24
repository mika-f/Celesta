//! An inline text edit: the text, its selection and IME marked text, and
//! the geometry that places a caret, a selection and a click in the text's
//! layout, mapped onto the box the renderer drew the text in.
//!
//! Indices are code points, as `TextMetrics` counts them; UTF-16 offsets
//! appear only at GPUI's input handler. Layout coordinates have their
//! origin at the first line's top-left, line `i` from `i · line_height`;
//! indices past a layout's text (a layout lagging its edit) are clamped to
//! it.

use celesta_composition::{Point, TextAlign};
use celesta_gpu_renderer::{GlyphMetrics, TextLayout};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// A text being edited: what is committed, the selection, and the IME's
/// marked (composing) text.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextEdit {
    /// The committed text, without the marked text.
    text: String,
    /// Where the selection began, in `display_text`.
    anchor: usize,
    /// Where the selection ends, the caret, in `display_text`.
    head: usize,
    /// The caret at a wrapped line's start draws at the end of the line
    /// before.
    upstream: bool,
    /// The range of `text` the marked text stands in for, and the marked
    /// text.
    marked: Option<(Range<usize>, String)>,
    /// The x (layout units) Up and Down keep to across lines shorter than
    /// it; any other change or move forgets it.
    goal_x: Option<f64>,
}

impl TextEdit {
    /// The caret at the end.
    pub(crate) fn new(text: String) -> Self {
        let end = text.chars().count();
        Self {
            text,
            anchor: end,
            head: end,
            upstream: false,
            marked: None,
            goal_x: None,
        }
    }

    /// The committed text, without the marked text.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// The text with the marked text in place: what is rendered.
    pub(crate) fn display_text(&self) -> String {
        match &self.marked {
            Some((range, marked)) => splice(&self.text, range.clone(), marked),
            None => self.text.clone(),
        }
    }

    /// The selection in `display_text`, ordered.
    pub(crate) fn selection(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    /// The caret in `display_text`, and whether it draws upstream.
    pub(crate) fn head(&self) -> (usize, bool) {
        (self.head, self.upstream)
    }

    /// Replaces the selection; `\r\n` and `\r` become `\n`.
    pub(crate) fn insert(&mut self, text: &str) {
        self.unmark();
        self.replace(self.selection(), text);
    }

    /// Removes the selection, or the grapheme before the caret.
    pub(crate) fn backspace(&mut self) {
        self.unmark();
        let selection = self.selection();
        if selection.is_empty() {
            let before = step(&self.text, self.head, false);
            self.replace(before..self.head, "");
        } else {
            self.replace(selection, "");
        }
    }

    /// Removes the selection, or the grapheme after the caret.
    pub(crate) fn delete(&mut self) {
        self.unmark();
        let selection = self.selection();
        if selection.is_empty() {
            let after = step(&self.text, self.head, true);
            self.replace(self.head..after, "");
        } else {
            self.replace(selection, "");
        }
    }

    /// One grapheme on; without `extend`, a selection collapses to its side
    /// instead.
    pub(crate) fn move_horizontal(&mut self, forward: bool, extend: bool) {
        self.unmark();
        let selection = self.selection();
        let index = match (extend, selection.is_empty(), forward) {
            (false, false, true) => selection.end,
            (false, false, false) => selection.start,
            _ => step(&self.text, self.head, forward),
        };
        self.place(index, false, extend);
    }

    /// One line on at the goal column (the caret's x when the last move
    /// was not vertical), or the text's start (end) from its first (last)
    /// line.
    pub(crate) fn move_vertical(&mut self, layout: &TextLayout, down: bool, extend: bool) {
        self.unmark();
        let (from, upstream) = self.moving_from(down, extend);
        let lines = Lines::new(layout);
        let from = lines.clamp(from);
        let line = lines.line_of(from, upstream);
        let x = self.goal_x.unwrap_or_else(|| lines.caret_x(line, from));
        let (index, upstream) = match (down, line) {
            (false, 0) => (0, false),
            (true, line) if line + 1 >= lines.count() => (self.text.chars().count(), false),
            (down, line) => {
                let target = if down { line + 1 } else { line - 1 };
                lines.index_at(target, x)
            }
        };
        self.place(index, upstream, extend);
        self.goal_x = Some(x);
    }

    /// The start or end of the caret's visual line; the end of a wrapped
    /// line draws upstream.
    pub(crate) fn move_line_edge(&mut self, layout: &TextLayout, end: bool, extend: bool) {
        self.unmark();
        let (from, upstream) = self.moving_from(end, extend);
        let lines = Lines::new(layout);
        let line = lines.line_of(lines.clamp(from), upstream);
        let (index, upstream) = if end {
            lines.content_end(line)
        } else {
            (lines.start(line), false)
        };
        self.place(index, upstream, extend);
    }

    /// Where a move begins: the caret, or without `extend` the side of the
    /// selection it moves towards.
    fn moving_from(&self, forward: bool, extend: bool) -> (usize, bool) {
        let selection = self.selection();
        if extend || selection.is_empty() {
            (self.head, self.upstream)
        } else if forward {
            (selection.end, false)
        } else {
            (selection.start, false)
        }
    }

    pub(crate) fn select_all(&mut self) {
        self.unmark();
        self.anchor = 0;
        self.head = self.text.chars().count();
        self.upstream = false;
        self.goal_x = None;
    }

    pub(crate) fn selected_text(&self) -> String {
        let selection = self.selection();
        self.display_text()
            .chars()
            .skip(selection.start)
            .take(selection.len())
            .collect()
    }

    /// Puts the caret at `index`; `extend` keeps the selection's anchor.
    pub(crate) fn place(&mut self, index: usize, upstream: bool, extend: bool) {
        self.unmark();
        self.head = index.min(self.text.chars().count());
        if !extend {
            self.anchor = self.head;
        }
        self.upstream = upstream;
        self.goal_x = None;
    }

    /// Replaces `range` of `display_text` (UTF-16; the marked text, else
    /// the selection, when `None`), accepting any marked text, and puts the
    /// caret after it.
    pub(crate) fn replace_utf16(&mut self, range: Option<Range<usize>>, text: &str) {
        let range = self.range_or_marked(range);
        self.unmark();
        self.replace(range, text);
    }

    /// Marks `text` in place of `range` of `display_text` (UTF-16; the
    /// marked text, else the selection, when `None`), selecting `selected`
    /// of it (UTF-16 within `text`; its end when `None`). The committed
    /// text it stands in for stays until it is accepted; an empty `text`
    /// ends the composition, bringing that text back selected.
    pub(crate) fn mark_utf16(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
    ) {
        let range = self.range_or_marked(range);
        let replaced = match self.marked.take() {
            // A step of the composition: it still stands in for the same text.
            Some((replaced, marked))
                if range == (replaced.start..replaced.start + count(&marked)) =>
            {
                replaced
            }
            marked => {
                self.marked = marked;
                self.unmark();
                range
            }
        };
        let text = normalize_line_breaks(text);
        if text.is_empty() {
            self.anchor = replaced.start;
            self.head = replaced.end;
        } else {
            let selected = selected.map_or(count(&text)..count(&text), |selected| {
                from_utf16(&text, selected.start)..from_utf16(&text, selected.end)
            });
            self.anchor = replaced.start + selected.start;
            self.head = replaced.start + selected.end;
            self.marked = Some((replaced, text));
        }
        self.upstream = false;
        self.goal_x = None;
    }

    /// Accepts the marked text as it is shown (NSTextInputClient's
    /// `unmarkText`).
    pub(crate) fn unmark(&mut self) {
        if let Some((range, marked)) = self.marked.take() {
            self.text = splice(&self.text, range, &marked);
        }
    }

    /// The marked text's range in `display_text`.
    pub(crate) fn marked(&self) -> Option<Range<usize>> {
        let (range, marked) = self.marked.as_ref()?;
        Some(range.start..range.start + count(marked))
    }

    /// The marked text's range in `display_text`, UTF-16.
    pub(crate) fn marked_utf16(&self) -> Option<Range<usize>> {
        let (range, marked) = self.marked.as_ref()?;
        let display = self.display_text();
        Some(to_utf16(&display, range.start)..to_utf16(&display, range.start + count(marked)))
    }

    /// The selection in `display_text`, UTF-16.
    pub(crate) fn selection_utf16(&self) -> Range<usize> {
        let display = self.display_text();
        let selection = self.selection();
        to_utf16(&display, selection.start)..to_utf16(&display, selection.end)
    }

    /// `range` (UTF-16 in `display_text`) in code points, or the marked
    /// text's range, else the selection.
    fn range_or_marked(&self, range: Option<Range<usize>>) -> Range<usize> {
        match (range, &self.marked) {
            (Some(range), _) => {
                let display = self.display_text();
                let (start, end) = (
                    from_utf16(&display, range.start),
                    from_utf16(&display, range.end),
                );
                start.min(end)..start.max(end)
            }
            (None, Some((replaced, marked))) => replaced.start..replaced.start + count(marked),
            (None, None) => self.selection(),
        }
    }

    /// Replaces `range` of the (unmarked) text with `text`, its line breaks
    /// normalized, and puts the caret after it.
    fn replace(&mut self, range: Range<usize>, text: &str) {
        let text = normalize_line_breaks(text);
        self.text = splice(&self.text, range.clone(), &text);
        self.place(range.start + count(&text), false, false);
    }
}

fn count(text: &str) -> usize {
    text.chars().count()
}

/// Byte offset of code point `index` of `text`; its length past the end.
fn byte_at(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// `text` with code points `range` replaced by `with`.
fn splice(text: &str, range: Range<usize>, with: &str) -> String {
    let (start, end) = (byte_at(text, range.start), byte_at(text, range.end));
    [&text[..start], with, &text[end..]].concat()
}

fn normalize_line_breaks(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Code point `index` of `text` as a UTF-16 offset.
pub(crate) fn to_utf16(text: &str, index: usize) -> usize {
    text.chars().take(index).map(char::len_utf16).sum()
}

/// UTF-16 offset `offset` of `text` as a code point; inside a surrogate
/// pair, the code point after it.
pub(crate) fn from_utf16(text: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    for (index, c) in text.chars().enumerate() {
        if utf16 >= offset {
            return index;
        }
        utf16 += c.len_utf16();
    }
    count(text)
}

/// `text`'s grapheme boundaries as code points, 0 and its end included.
fn grapheme_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = vec![0];
    for grapheme in text.graphemes(true) {
        boundaries.push(boundaries[boundaries.len() - 1] + count(grapheme));
    }
    boundaries
}

/// The grapheme boundary after (`forward`) or before `index` in `text`,
/// stopping at its ends.
fn step(text: &str, index: usize, forward: bool) -> usize {
    let boundaries = grapheme_boundaries(text);
    if forward {
        boundaries
            .into_iter()
            .find(|&b| b > index)
            .unwrap_or(count(text))
    } else {
        boundaries
            .into_iter()
            .rev()
            .find(|&b| b < index)
            .unwrap_or(0)
    }
}

/// The caret at `index` as a segment from its line's top to its bottom.
pub(crate) fn caret(layout: &TextLayout, index: usize, upstream: bool) -> [Point; 2] {
    let lines = Lines::new(layout);
    let index = lines.clamp(index);
    let line = lines.line_of(index, upstream);
    let x = lines.caret_x(line, index);
    let top = lines.top(line);
    [
        Point { x, y: top },
        Point {
            x,
            y: top + layout.metrics.line_height,
        },
    ]
}

/// One rectangle per line the range covers, corners clockwise from the
/// top-left.
pub(crate) fn selection_quads(layout: &TextLayout, range: Range<usize>) -> Vec<[Point; 4]> {
    let lines = Lines::new(layout);
    let (start, end) = (lines.clamp(range.start), lines.clamp(range.end));
    let (start, end) = (start.min(end), start.max(end));
    (0..lines.count())
        .filter_map(|line| {
            let (from, to) = (start.max(lines.start(line)), end.min(lines.span_end(line)));
            if from >= to {
                return None;
            }
            // The covered clusters' edges, for a line mixing directions.
            let edges = lines
                .glyphs(line)
                .filter(|glyph| glyph.start >= from && glyph.end <= to)
                .flat_map(|glyph| [glyph.x, glyph.x + glyph.width]);
            let xs = [lines.caret_x(line, from), lines.caret_x(line, to)];
            let (left, right) = xs
                .into_iter()
                .chain(edges)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(left, right), x| {
                    (left.min(x), right.max(x))
                });
            let (top, bottom) = (lines.top(line), lines.top(line + 1));
            Some([
                Point { x: left, y: top },
                Point { x: right, y: top },
                Point {
                    x: right,
                    y: bottom,
                },
                Point { x: left, y: bottom },
            ])
        })
        .collect()
}

/// The grapheme boundary nearest to `point`: (index, upstream).
pub(crate) fn index_at(layout: &TextLayout, point: Point) -> (usize, bool) {
    let lines = Lines::new(layout);
    let line_height = layout.metrics.line_height;
    let line = if line_height > 0.0 {
        (point.y / line_height).floor()
    } else {
        0.0
    };
    // `as` saturates a negative or NaN line to 0.
    let line = (line as usize).min(lines.count() - 1);
    lines.index_at(line, point.x)
}

/// A layout's text in code points with its grapheme boundaries, for the
/// geometry above.
struct Lines<'a> {
    layout: &'a TextLayout,
    chars: Vec<char>,
    boundaries: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn new(layout: &'a TextLayout) -> Self {
        Self {
            layout,
            chars: layout.text.chars().collect(),
            boundaries: grapheme_boundaries(&layout.text),
        }
    }

    /// `index` within the text, on the grapheme boundary at or before it.
    fn clamp(&self, index: usize) -> usize {
        let index = index.min(self.chars.len());
        self.boundaries
            .iter()
            .rev()
            .find(|&&b| b <= index)
            .copied()
            .unwrap_or(0)
    }

    fn count(&self) -> usize {
        self.layout.metrics.line_starts.len().max(1)
    }

    fn top(&self, line: usize) -> f64 {
        line as f64 * self.layout.metrics.line_height
    }

    fn start(&self, line: usize) -> usize {
        let start = self.layout.metrics.line_starts.get(line).copied();
        start.unwrap_or(0).min(self.chars.len())
    }

    /// Where line `line`'s span ends: the next line's start, or the text's
    /// end.
    fn span_end(&self, line: usize) -> usize {
        if line + 1 < self.count() {
            self.start(line + 1)
        } else {
            self.chars.len()
        }
    }

    /// Where line `line`'s content ends, and whether the caret there draws
    /// upstream: before the line break that ends it, or at the next line's
    /// start after a soft wrap.
    fn content_end(&self, line: usize) -> (usize, bool) {
        let end = self.span_end(line);
        if line + 1 >= self.count() {
            return (end, false);
        }
        let before = |back: usize| end.checked_sub(back).map(|index| self.chars[index]);
        let start = self.start(line);
        match (before(2), before(1)) {
            (Some('\r'), Some('\n')) => ((end - 2).max(start), false),
            (_, Some('\n' | '\r')) => ((end - 1).max(start), false),
            _ => (end, end > start),
        }
    }

    /// The visual line `index` draws on: the one its span starts on, or
    /// the one before for an upstream caret at a soft wrap.
    fn line_of(&self, index: usize, upstream: bool) -> usize {
        let line = (0..self.count())
            .rev()
            .find(|&line| self.start(line) <= index)
            .unwrap_or(0);
        if upstream && line > 0 && self.content_end(line - 1) == (index, true) {
            line - 1
        } else {
            line
        }
    }

    fn glyphs(&self, line: usize) -> impl Iterator<Item = &'a GlyphMetrics> + Clone + use<'a> {
        let glyphs = &self.layout.metrics.glyphs;
        glyphs.iter().filter(move |glyph| glyph.line == line)
    }

    /// The caret's x for `index` on line `line`: a cluster's leading edge
    /// where it starts, its trailing edge where it ends, and an even share
    /// of its advance per grapheme inside it (a ligature). A line without
    /// clusters (an empty line) puts it where its alignment would.
    fn caret_x(&self, line: usize, index: usize) -> f64 {
        let leading = |glyph: &GlyphMetrics| glyph.x + if glyph.rtl { glyph.width } else { 0.0 };
        let trailing = |glyph: &GlyphMetrics| glyph.x + if glyph.rtl { 0.0 } else { glyph.width };
        let glyphs = self.glyphs(line);
        if let Some(glyph) = glyphs.clone().find(|glyph| glyph.start == index) {
            return leading(glyph);
        }
        if let Some(glyph) = glyphs.clone().find(|glyph| glyph.end == index) {
            return trailing(glyph);
        }
        if let Some(glyph) = glyphs
            .clone()
            .find(|glyph| glyph.start < index && index < glyph.end)
        {
            let inside = index - glyph.start;
            let (mut passed, mut graphemes, mut offset) = (0, 0, 0);
            for grapheme in glyph.text.graphemes(true) {
                offset += count(grapheme);
                graphemes += 1;
                if offset <= inside {
                    passed += 1;
                }
            }
            let share = passed as f64 / graphemes.max(1) as f64;
            let share = if glyph.rtl { 1.0 - share } else { share };
            return glyph.x + glyph.width * share;
        }
        // No cluster here: after the last one before it, else before the
        // first.
        if let Some(glyph) = glyphs
            .clone()
            .filter(|glyph| glyph.end <= index)
            .max_by_key(|glyph| glyph.end)
        {
            return trailing(glyph);
        }
        if let Some(glyph) = glyphs.min_by_key(|glyph| glyph.start) {
            return leading(glyph);
        }
        let width = self.layout.max_width.unwrap_or(self.layout.metrics.width);
        match self.layout.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => width / 2.0,
            TextAlign::Right => width,
        }
    }

    /// The grapheme boundary on line `line` whose caret is nearest to `x`.
    fn index_at(&self, line: usize, x: f64) -> (usize, bool) {
        let start = self.start(line);
        let (end, wraps) = self.content_end(line);
        let mut nearest = (start, false);
        let mut distance = f64::INFINITY;
        for &index in self.boundaries.iter().filter(|&&b| start <= b && b <= end) {
            let candidate = (index, wraps && index == end);
            let gap = (self.caret_x(line, index) - x).abs();
            if gap < distance {
                (nearest, distance) = (candidate, gap);
            }
        }
        nearest
    }
}

/// The message for a text box a homography cannot be fitted to.
pub(crate) const EDGE_ON: &str = "The text is edge-on to the view";

/// A layout's rectangle: its layout width (`maxWidth`, else the measured
/// width) by `lines · line_height`, as the renderer records the text box;
/// an empty text is one line.
pub(crate) fn layout_size(layout: &TextLayout) -> (f64, f64) {
    let width = layout.max_width.unwrap_or(layout.metrics.width);
    let height = layout.metrics.lines.max(1) as f64 * layout.metrics.line_height;
    (width, height)
}

/// The homography taking a text's layout rectangle (layout width by
/// `lines · line_height`) onto `HitRegion::text_box`, and back.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextMap {
    /// Row-major: a scene point's `[x, y, w]` is this times a layout
    /// point's `[x, y, 1]`.
    forward: [f64; 9],
    inverse: [f64; 9],
    /// The layout rectangle it was fitted for.
    size: (f64, f64),
}

impl TextMap {
    /// Fails for a box of zero area or not convex (a bow-tie): the text is
    /// seen edge-on. A mirrored box, its corners in reverse, is convex.
    pub(crate) fn fit(layout: &TextLayout, text_box: [Point; 4]) -> Result<Self, String> {
        let (width, height) = layout_size(layout);
        let sized = |side: f64| side.is_finite() && side > 0.0;
        if !sized(width) || !sized(height) || !convex(text_box) {
            return Err(EDGE_ON.to_owned());
        }
        let scale = [1.0 / width, 0.0, 0.0, 0.0, 1.0 / height, 0.0, 0.0, 0.0, 1.0];
        let forward = multiply(square_to(text_box), scale);
        let inverse = invert(forward).ok_or_else(|| EDGE_ON.to_owned())?;
        Ok(Self {
            forward,
            inverse,
            size: (width, height),
        })
    }

    /// Whether `layout_point` is inside the layout rectangle the map was
    /// fitted for, edges included.
    pub(crate) fn contains(&self, layout_point: Point) -> bool {
        let (width, height) = self.size;
        (0.0..=width).contains(&layout_point.x) && (0.0..=height).contains(&layout_point.y)
    }

    pub(crate) fn to_scene(&self, layout_point: Point) -> Point {
        let [x, y, w] = apply(&self.forward, layout_point);
        Point { x: x / w, y: y / w }
    }

    /// `None` for a point no point of the text's plane is drawn at: one
    /// beyond its horizon.
    pub(crate) fn to_layout(&self, scene: Point) -> Option<Point> {
        let [x, y, w] = apply(&self.inverse, scene);
        let point = Point { x: x / w, y: y / w };
        // The plane is in front of the viewer where `w` is positive.
        let [_, _, depth] = apply(&self.forward, point);
        (point.x.is_finite() && point.y.is_finite() && depth > 0.0).then_some(point)
    }
}

/// Whether `quad` turns the same way at every corner with some area: not
/// a line, a point or a bow-tie.
pub(crate) fn convex(quad: [Point; 4]) -> bool {
    let turns: [f64; 4] = std::array::from_fn(|i| {
        let [a, b, c] = [quad[i], quad[(i + 1) % 4], quad[(i + 2) % 4]];
        (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x)
    });
    let size = quad
        .iter()
        .flat_map(|p| [p.x.abs(), p.y.abs()])
        .fold(1.0, f64::max);
    let tolerance = size * size * 1e-12;
    turns.iter().all(|&turn| turn > tolerance) || turns.iter().all(|&turn| turn < -tolerance)
}

/// The homography taking the unit square's corners (0, 0), (1, 0), (1, 1),
/// (0, 1) to `quad`'s (Heckbert, "Fundamentals of Texture Mapping").
fn square_to(quad: [Point; 4]) -> [f64; 9] {
    let [p0, p1, p2, p3] = quad;
    let (sx, sy) = (p0.x - p1.x + p2.x - p3.x, p0.y - p1.y + p2.y - p3.y);
    let (dx1, dx2) = (p1.x - p2.x, p3.x - p2.x);
    let (dy1, dy2) = (p1.y - p2.y, p3.y - p2.y);
    let det = dx1 * dy2 - dx2 * dy1;
    // A parallelogram is affine.
    let (g, h) = if sx == 0.0 && sy == 0.0 {
        (0.0, 0.0)
    } else {
        ((sx * dy2 - dx2 * sy) / det, (dx1 * sy - sx * dy1) / det)
    };
    [
        p1.x - p0.x + g * p1.x,
        p3.x - p0.x + h * p3.x,
        p0.x,
        p1.y - p0.y + g * p1.y,
        p3.y - p0.y + h * p3.y,
        p0.y,
        g,
        h,
        1.0,
    ]
}

fn multiply(a: [f64; 9], b: [f64; 9]) -> [f64; 9] {
    std::array::from_fn(|i| {
        let (row, column) = (i / 3, i % 3);
        (0..3).map(|k| a[row * 3 + k] * b[k * 3 + column]).sum()
    })
}

fn invert(m: [f64; 9]) -> Option<[f64; 9]> {
    let [a, b, c, d, e, f, g, h, i] = m;
    let (ea, eb, ec) = (e * i - f * h, f * g - d * i, d * h - e * g);
    let det = a * ea + b * eb + c * ec;
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    let adjugate = [
        ea,
        c * h - b * i,
        b * f - c * e,
        eb,
        a * i - c * g,
        c * d - a * f,
        ec,
        b * g - a * h,
        a * e - b * d,
    ];
    Some(adjugate.map(|value| value / det))
}

fn apply(m: &[f64; 9], p: Point) -> [f64; 3] {
    [
        m[0] * p.x + m[1] * p.y + m[2],
        m[3] * p.x + m[4] * p.y + m[5],
        m[6] * p.x + m[7] * p.y + m[8],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_composition::TextAlign;
    use celesta_gpu_renderer::{GlyphMetrics, TextMetrics};

    const LINE_HEIGHT: f64 = 20.0;

    fn at(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    fn assert_near(actual: Point, expected: Point) {
        assert!(
            (actual.x - expected.x).abs() < 1e-9 && (actual.y - expected.y).abs() < 1e-9,
            "{actual:?} != {expected:?}"
        );
    }

    fn edit_at(text: &str, index: usize) -> TextEdit {
        let mut edit = TextEdit::new(text.to_owned());
        edit.place(index, false, false);
        edit
    }

    fn cluster(start: usize, text: &str, x: f64, width: f64, line: usize) -> GlyphMetrics {
        GlyphMetrics {
            start,
            end: start + text.chars().count(),
            rtl: false,
            text: text.to_owned(),
            x,
            width,
            line,
            ink_ascent: 0.0,
            ink_descent: 0.0,
        }
    }

    fn laid_out(text: &str, line_starts: Vec<usize>, glyphs: Vec<GlyphMetrics>) -> TextLayout {
        let lines = line_starts.len();
        let width = glyphs.iter().map(|g| g.x + g.width).fold(0.0, f64::max);
        TextLayout {
            text: text.to_owned(),
            metrics: TextMetrics {
                line_starts,
                width,
                height: lines as f64 * LINE_HEIGHT,
                ascent: 16.0,
                descent: 4.0,
                line_height: LINE_HEIGHT,
                lines,
                glyphs,
                ..TextMetrics::default()
            },
            max_width: None,
            align: TextAlign::Left,
        }
    }

    /// A left-aligned layout of `lines` (each with its own line break, if
    /// it has one), every code point but a line break a 10-wide cluster.
    fn grid(lines: &[&str]) -> TextLayout {
        let mut starts = Vec::new();
        let mut glyphs = Vec::new();
        let mut start = 0;
        for (line, text) in lines.iter().enumerate() {
            starts.push(start);
            for (column, c) in text.chars().enumerate() {
                if c != '\n' {
                    let x = 10.0 * column as f64;
                    glyphs.push(cluster(start + column, &c.to_string(), x, 10.0, line));
                }
            }
            start += text.chars().count();
        }
        laid_out(&lines.concat(), starts, glyphs)
    }

    fn middle([top, bottom]: [Point; 2]) -> Point {
        at((top.x + bottom.x) / 2.0, (top.y + bottom.y) / 2.0)
    }

    #[test]
    fn insert_replaces_the_selection() {
        let mut edit = TextEdit::new("hello".to_owned());
        assert_eq!(edit.selection(), 5..5);
        edit.place(1, false, false);
        edit.place(4, false, true);
        assert_eq!(edit.selected_text(), "ell");
        edit.insert("EY");
        assert_eq!(edit.text(), "hEYo");
        assert_eq!(edit.selection(), 3..3);
    }

    #[test]
    fn backspace_removes_one_grapheme() {
        let mut edit = edit_at("ae\u{301}b", 3);
        edit.backspace();
        assert_eq!(edit.text(), "ab");
        assert_eq!(edit.selection(), 1..1);

        let mut edit = edit_at("xe\u{301}", 1);
        edit.delete();
        assert_eq!(edit.text(), "x");
        assert_eq!(edit.selection(), 1..1);

        // A selection goes whole.
        let mut edit = edit_at("abc", 0);
        edit.place(2, false, true);
        edit.backspace();
        assert_eq!(edit.text(), "c");
        assert_eq!(edit.selection(), 0..0);
    }

    #[test]
    fn shift_moves_extend_the_selection() {
        let mut edit = edit_at("abc", 1);
        edit.move_horizontal(true, true);
        edit.move_horizontal(true, true);
        assert_eq!(edit.selection(), 1..3);
        assert_eq!(edit.selected_text(), "bc");
        edit.move_horizontal(false, true);
        assert_eq!(edit.selection(), 1..2);
        // An unshifted move collapses the selection to its side.
        edit.move_horizontal(false, false);
        assert_eq!(edit.selection(), 1..1);
        // Back past the anchor.
        edit.move_horizontal(false, true);
        assert_eq!(edit.selection(), 0..1);
        assert_eq!(edit.head(), (0, false));

        edit.select_all();
        assert_eq!(edit.selected_text(), "abc");
        edit.move_horizontal(true, false);
        assert_eq!(edit.selection(), 3..3);
    }

    #[test]
    fn paste_normalizes_line_breaks() {
        let mut edit = TextEdit::new(String::new());
        edit.insert("a\r\nb\rc\n");
        assert_eq!(edit.text(), "a\nb\nc\n");
        assert_eq!(edit.selection(), 6..6);
    }

    #[test]
    fn marked_text_shows_in_display_text_and_unmark_keeps_text_unchanged() {
        let mut edit = edit_at("ab", 1);
        edit.mark_utf16(None, "に", Some(1..1));
        assert_eq!(edit.text(), "ab");
        assert_eq!(edit.display_text(), "aにb");
        assert_eq!(edit.marked_utf16(), Some(1..2));
        assert_eq!(edit.selection_utf16(), 2..2);

        // The next composition step replaces the marked text.
        edit.mark_utf16(None, "日本", Some(2..2));
        assert_eq!(edit.text(), "ab");
        assert_eq!(edit.display_text(), "a日本b");
        assert_eq!(edit.marked_utf16(), Some(1..3));
        assert_eq!(edit.selection_utf16(), 3..3);

        // Unmarking accepts the composition as shown (NSTextInputClient's
        // `unmarkText`): what is displayed stays.
        edit.unmark();
        assert_eq!(edit.display_text(), "a日本b");
        assert_eq!(edit.text(), "a日本b");
        assert_eq!(edit.marked_utf16(), None);
        assert_eq!(edit.selection_utf16(), 3..3);

        // A composition over a selection keeps the selected text until it
        // is accepted, and an emptied composition brings it back.
        let mut edit = TextEdit::new("abc".to_owned());
        edit.select_all();
        edit.mark_utf16(None, "に", None);
        assert_eq!(edit.text(), "abc");
        assert_eq!(edit.display_text(), "に");
        assert_eq!(edit.selection_utf16(), 1..1);
        edit.mark_utf16(None, "", None);
        assert_eq!(edit.display_text(), "abc");
        assert_eq!(edit.marked_utf16(), None);
        assert_eq!(edit.selection(), 0..3);
    }

    #[test]
    fn utf16_ranges_round_trip_through_astral_and_japanese_text() {
        let mut edit = TextEdit::new("日本🎌語".to_owned());
        assert_eq!(edit.selection_utf16(), 5..5);
        edit.place(3, false, false);
        assert_eq!(edit.selection_utf16(), 4..4);
        edit.place(2, false, true);
        assert_eq!(edit.selection_utf16(), 2..4);
        assert_eq!(edit.selected_text(), "🎌");

        edit.replace_utf16(Some(2..4), "X");
        assert_eq!(edit.text(), "日本X語");
        assert_eq!(edit.selection_utf16(), 3..3);

        edit.mark_utf16(Some(0..1), "🎌", Some(2..2));
        assert_eq!(edit.text(), "日本X語");
        assert_eq!(edit.display_text(), "🎌本X語");
        assert_eq!(edit.marked_utf16(), Some(0..2));
        assert_eq!(edit.selection_utf16(), 2..2);
        assert_eq!(edit.selection(), 1..1);

        // No range: the marked text is what is replaced.
        edit.replace_utf16(None, "日");
        assert_eq!(edit.text(), "日本X語");
        assert_eq!(edit.display_text(), "日本X語");
        assert_eq!(edit.marked_utf16(), None);
        assert_eq!(edit.selection_utf16(), 1..1);
    }

    #[test]
    fn escape_after_marking_restores() {
        let original = "Sale".to_owned();
        let start = TextEdit::new(original.clone());
        let mut edit = start.clone();
        edit.insert("!");
        edit.mark_utf16(None, "に", None);
        assert_eq!(edit.display_text(), "Sale!に");

        // Escape drops the edit, marked text and all.
        let restored = TextEdit::new(original);
        assert_eq!(restored, start);
        assert_eq!(restored.display_text(), "Sale");
        assert_eq!(restored.marked_utf16(), None);
    }

    #[test]
    fn caret_on_a_wrapped_line_start_and_upstream_end() {
        let layout = grid(&["ab ", "cd"]);
        assert_eq!(caret(&layout, 3, false), [at(0.0, 20.0), at(0.0, 40.0)]);
        assert_eq!(caret(&layout, 3, true), [at(30.0, 0.0), at(30.0, 20.0)]);
    }

    #[test]
    fn caret_in_a_multi_line_text_steps_by_line_height() {
        let layout = grid(&["ab\n", "cd\n", "ef"]);
        assert_eq!(caret(&layout, 0, false), [at(0.0, 0.0), at(0.0, 20.0)]);
        assert_eq!(caret(&layout, 2, false), [at(20.0, 0.0), at(20.0, 20.0)]);
        assert_eq!(caret(&layout, 3, false), [at(0.0, 20.0), at(0.0, 40.0)]);
        assert_eq!(caret(&layout, 6, false), [at(0.0, 40.0), at(0.0, 60.0)]);
        assert_eq!(caret(&layout, 8, false), [at(20.0, 40.0), at(20.0, 60.0)]);
        // After a line break there is no line end to stick to.
        assert_eq!(caret(&layout, 3, true), caret(&layout, 3, false));
    }

    #[test]
    fn caret_before_an_rtl_cluster_is_at_its_right_edge() {
        let mut aleph = cluster(0, "א", 10.0, 10.0, 0);
        let mut bet = cluster(1, "ב", 0.0, 10.0, 0);
        aleph.rtl = true;
        bet.rtl = true;
        let layout = laid_out("אב", vec![0], vec![aleph, bet]);
        assert_eq!(caret(&layout, 0, false)[0].x, 20.0);
        assert_eq!(caret(&layout, 1, false)[0].x, 10.0);
        // The end of the last cluster is its left edge.
        assert_eq!(caret(&layout, 2, false)[0].x, 0.0);
        assert_eq!(index_at(&layout, at(18.0, 10.0)), (0, false));
        assert_eq!(index_at(&layout, at(2.0, 10.0)), (2, false));
    }

    #[test]
    fn caret_inside_a_ligature_interpolates() {
        let layout = laid_out("ffi", vec![0], vec![cluster(0, "ffi", 0.0, 30.0, 0)]);
        assert_eq!(caret(&layout, 1, false)[0].x, 10.0);
        assert_eq!(caret(&layout, 2, false)[0].x, 20.0);
        assert_eq!(caret(&layout, 3, false)[0].x, 30.0);
        assert_eq!(index_at(&layout, at(14.0, 10.0)), (1, false));
    }

    #[test]
    fn caret_and_index_at_snap_to_graphemes_in_a_combining_cluster() {
        let combined = laid_out(
            "e\u{301}x",
            vec![0],
            vec![
                cluster(0, "e\u{301}", 0.0, 10.0, 0),
                cluster(2, "x", 10.0, 8.0, 0),
            ],
        );
        // The accent shaped apart from its letter.
        let apart = laid_out(
            "e\u{301}x",
            vec![0],
            vec![
                cluster(0, "e", 0.0, 10.0, 0),
                cluster(1, "\u{301}", 10.0, 0.0, 0),
                cluster(2, "x", 10.0, 8.0, 0),
            ],
        );
        for layout in [combined, apart] {
            assert_eq!(caret(&layout, 1, false), caret(&layout, 0, false));
            assert_eq!(caret(&layout, 2, false)[0].x, 10.0);
            assert_eq!(index_at(&layout, at(4.0, 10.0)), (0, false));
            assert_eq!(index_at(&layout, at(6.0, 10.0)), (2, false));
            for step in 0..=40 {
                let (index, _) = index_at(&layout, at(f64::from(step) / 2.0, 10.0));
                assert_ne!(index, 1, "at x {}", f64::from(step) / 2.0);
            }
        }
    }

    #[test]
    fn an_empty_line_s_caret_follows_alignment() {
        let mut layout = grid(&["ab\n", "\n", "cd"]);
        layout.max_width = Some(100.0);
        for (align, x) in [
            (TextAlign::Left, 0.0),
            (TextAlign::Center, 50.0),
            (TextAlign::Right, 100.0),
        ] {
            layout.align = align;
            assert_eq!(caret(&layout, 3, false), [at(x, 20.0), at(x, 40.0)]);
        }
    }

    #[test]
    fn an_empty_text_s_caret_is_one_line_tall() {
        // What the renderer measures for "": one empty line.
        let mut layout = laid_out("", vec![0], Vec::new());
        assert_eq!(caret(&layout, 0, false), [at(0.0, 0.0), at(0.0, 20.0)]);
        assert_eq!(index_at(&layout, at(5.0, 5.0)), (0, false));
        assert!(selection_quads(&layout, 0..0).is_empty());

        layout.max_width = Some(80.0);
        layout.align = TextAlign::Center;
        assert_eq!(caret(&layout, 0, false), [at(40.0, 0.0), at(40.0, 20.0)]);
        let text_box = [at(0.0, 0.0), at(80.0, 0.0), at(80.0, 20.0), at(0.0, 20.0)];
        let map = TextMap::fit(&layout, text_box).unwrap();
        assert_near(map.to_scene(at(40.0, 20.0)), at(40.0, 20.0));
    }

    #[test]
    fn selection_over_two_lines_gives_two_quads() {
        let layout = grid(&["ab ", "cd"]);
        assert_eq!(
            selection_quads(&layout, 1..4),
            [
                [at(10.0, 0.0), at(30.0, 0.0), at(30.0, 20.0), at(10.0, 20.0)],
                [at(0.0, 20.0), at(10.0, 20.0), at(10.0, 40.0), at(0.0, 40.0)],
            ]
        );
    }

    #[test]
    fn index_at_round_trips_caret() {
        let layout = grid(&["ab ", "cd"]);
        for (index, upstream) in [
            (0, false),
            (1, false),
            (2, false),
            (3, true),
            (3, false),
            (4, false),
            (5, false),
        ] {
            let point = middle(caret(&layout, index, upstream));
            assert_eq!(index_at(&layout, point), (index, upstream), "{point:?}");
        }
    }

    #[test]
    fn indices_past_a_lagging_layout_clamp() {
        let layout = grid(&["ab"]);
        assert_eq!(caret(&layout, 10, false), caret(&layout, 2, false));
        assert_eq!(
            selection_quads(&layout, 1..10),
            selection_quads(&layout, 1..2)
        );
        assert_eq!(index_at(&layout, at(500.0, 500.0)), (2, false));

        // An edit ahead of its layout moves by the layout it has.
        let mut edit = TextEdit::new("abcdef".to_owned());
        edit.move_line_edge(&layout, false, false);
        assert_eq!(edit.selection(), 0..0);
        edit.move_vertical(&layout, true, false);
        assert_eq!(edit.selection(), 6..6);
    }

    #[test]
    fn vertical_moves_keep_x() {
        let layout = grid(&["abc\n", "de\n", "fgh"]);
        let mut edit = edit_at("abc\nde\nfgh", 2);
        edit.move_vertical(&layout, true, false);
        assert_eq!(edit.selection(), 6..6);
        edit.move_vertical(&layout, true, false);
        assert_eq!(edit.selection(), 9..9);
        edit.move_vertical(&layout, false, false);
        assert_eq!(edit.selection(), 6..6);
        edit.move_vertical(&layout, false, false);
        assert_eq!(edit.selection(), 2..2);

        edit.move_vertical(&layout, true, true);
        assert_eq!(edit.selection(), 2..6);

        // Past the first or last line: the text's start or end.
        let mut edit = edit_at("abc\nde\nfgh", 2);
        edit.move_vertical(&layout, false, false);
        assert_eq!(edit.selection(), 0..0);
        let mut edit = edit_at("abc\nde\nfgh", 8);
        edit.move_vertical(&layout, true, false);
        assert_eq!(edit.selection(), 10..10);
    }

    #[test]
    fn vertical_moves_keep_a_goal_column() {
        let layout = grid(&["abc\n", "de\n", "fgh"]);
        // From the end of "abc", through the shorter "de", back to x 30.
        let mut edit = edit_at("abc\nde\nfgh", 3);
        edit.move_vertical(&layout, true, false);
        assert_eq!(edit.selection(), 6..6);
        edit.move_vertical(&layout, true, false);
        assert_eq!(edit.selection(), 10..10);
        edit.move_vertical(&layout, false, false);
        edit.move_vertical(&layout, false, false);
        assert_eq!(edit.selection(), 3..3);

        // Any other move forgets it.
        let mut edit = edit_at("abc\nde\nfgh", 3);
        edit.move_vertical(&layout, true, false);
        edit.move_horizontal(false, false);
        edit.move_vertical(&layout, true, false);
        assert_eq!(edit.selection(), 8..8);
    }

    #[test]
    fn home_and_end_use_visual_lines() {
        let layout = grid(&["ab ", "cd\n", "ef"]);
        let mut edit = edit_at("ab cd\nef", 1);
        edit.move_line_edge(&layout, true, false);
        assert_eq!(edit.head(), (3, true));
        assert_eq!(caret(&layout, 3, true)[0], at(30.0, 0.0));
        edit.move_line_edge(&layout, false, false);
        assert_eq!(edit.head(), (0, false));

        // The wrapped line's start is the next line's.
        let mut edit = edit_at("ab cd\nef", 3);
        edit.move_line_edge(&layout, true, false);
        assert_eq!(edit.head(), (5, false));
        edit.move_line_edge(&layout, false, false);
        assert_eq!(edit.head(), (3, false));

        let mut edit = edit_at("ab cd\nef", 4);
        edit.move_line_edge(&layout, true, true);
        assert_eq!(edit.selection(), 4..5);
    }

    /// `p` rotated 30° and scaled 2 about the origin, then moved by (100, 50).
    fn rotated(p: Point) -> Point {
        let (sin, cos) = 30f64.to_radians().sin_cos();
        at(
            100.0 + 2.0 * (p.x * cos - p.y * sin),
            50.0 + 2.0 * (p.x * sin + p.y * cos),
        )
    }

    fn rectangle(width: f64, height: f64) -> [Point; 4] {
        [
            at(0.0, 0.0),
            at(width, 0.0),
            at(width, height),
            at(0.0, height),
        ]
    }

    #[test]
    fn a_map_fitted_to_a_rotated_scaled_box_is_its_affine_map() {
        let layout = grid(&["abcd"]);
        let map = TextMap::fit(&layout, rectangle(40.0, 20.0).map(rotated)).unwrap();
        let [top, bottom] = caret(&layout, 2, false).map(|p| map.to_scene(p));
        // (20, 0) and (20, 20), turned by 30° and doubled: 40·(cos, sin)
        // and 40·(cos − sin, sin + cos), cos 30° = √3/2.
        let root3 = 3f64.sqrt();
        assert_near(top, at(100.0 + 20.0 * root3, 70.0));
        assert_near(
            bottom,
            at(100.0 + 20.0 * root3 - 20.0, 50.0 + 20.0 + 20.0 * root3),
        );
    }

    #[test]
    fn a_map_fitted_to_a_mirrored_box_maps() {
        let layout = grid(&["abcd"]);
        // `scaleX` −1 about x = 100: the layout's top-left is drawn at the
        // box's top-right, so its corners come in reverse.
        let text_box = [
            at(100.0, 0.0),
            at(60.0, 0.0),
            at(60.0, 20.0),
            at(100.0, 20.0),
        ];
        let map = TextMap::fit(&layout, text_box).unwrap();
        for (corner, scene) in rectangle(40.0, 20.0).into_iter().zip(text_box) {
            assert_near(map.to_scene(corner), scene);
        }
        let [top, bottom] = caret(&layout, 1, false).map(|p| map.to_scene(p));
        assert_near(top, at(90.0, 0.0));
        assert_near(bottom, at(90.0, 20.0));
    }

    /// A 40 by 20 rectangle seen tilted back: its top edge shorter.
    fn trapezoid() -> [Point; 4] {
        [at(10.0, 0.0), at(30.0, 0.0), at(40.0, 20.0), at(0.0, 20.0)]
    }

    #[test]
    fn a_map_fitted_to_a_projected_box_follows_the_perspective() {
        let layout = grid(&["abcd"]);
        let map = TextMap::fit(&layout, trapezoid()).unwrap();
        for (corner, scene) in rectangle(40.0, 20.0).into_iter().zip(trapezoid()) {
            assert_near(map.to_scene(corner), scene);
        }
        // The diagonals (10, 0)–(40, 20) and (30, 0)–(0, 20) cross a third
        // of the way down, not halfway as an affine map would put it.
        assert_near(map.to_scene(at(20.0, 10.0)), at(20.0, 20.0 / 3.0));
    }

    #[test]
    fn to_layout_inverts_to_scene() {
        let layout = grid(&["abcd"]);
        for text_box in [trapezoid(), rectangle(40.0, 20.0).map(rotated)] {
            let map = TextMap::fit(&layout, text_box).unwrap();
            for point in [at(0.0, 0.0), at(40.0, 20.0), at(13.0, 7.0), at(25.0, -3.0)] {
                assert_near(map.to_layout(map.to_scene(point)).unwrap(), point);
            }
        }
        // The trapezoid's sides meet at (20, −20): beyond that horizon the
        // plane is behind the viewer.
        let map = TextMap::fit(&layout, trapezoid()).unwrap();
        assert!(map.to_layout(at(20.0, -10.0)).is_some());
        assert_eq!(map.to_layout(at(20.0, -40.0)), None);
    }

    #[test]
    fn fit_refuses_a_degenerate_box() {
        let layout = grid(&["abcd"]);
        let edge_on = Err("The text is edge-on to the view".to_owned());
        let flat = [at(0.0, 0.0), at(40.0, 0.0), at(40.0, 0.0), at(0.0, 0.0)];
        assert_eq!(TextMap::fit(&layout, flat), edge_on);
        let line = [at(0.0, 0.0), at(10.0, 0.0), at(20.0, 0.0), at(30.0, 0.0)];
        assert_eq!(TextMap::fit(&layout, line), edge_on);
        let bow_tie = [at(0.0, 0.0), at(40.0, 0.0), at(0.0, 20.0), at(40.0, 20.0)];
        assert_eq!(TextMap::fit(&layout, bow_tie), edge_on);
    }
}
