//! Checks read from evaluated scenes and timelines, without rendering:
//! short scenes, a first frame with nothing on it, and text whose color is
//! too close to the solid color behind it.

use std::collections::BTreeMap;

use celesta_composition::{
    Color, EvaluatedTransform, Layer, LayerContent, Paint, PathCommand, Rational, Scene, TextStyle,
};
use celesta_project::{Project, TrackKind};
use celesta_react_bridge::ReactChapter;

use super::luminance::{contrast_ratio, relative_luminance};
use super::{
    FrameSpan, LARGE_TEXT_CONTRAST, MIN_SCENE_SECONDS, Measured, QualityCode, QualityIssue,
    TEXT_CONTRAST, describe_spans, push_frame, seconds,
};

/// The export's opaque background, where nothing is drawn:
/// `GpuRenderOptions::default().background`.
pub(super) const BACKGROUND: [u8; 3] = [20, 22, 28];

/// Text at least this many pixels tall on a 1080-pixel short side is large
/// (WCAG's 18 pt), and bold text at [`LARGE_BOLD_TEXT_PX`].
const LARGE_TEXT_PX: f64 = 24.0;
const LARGE_BOLD_TEXT_PX: f64 = 18.66;
const BOLD_WEIGHT: u16 = 700;
/// The renderer's size and color for text that gives none.
const DEFAULT_FONT_SIZE: f64 = 32.0;

/// Below this, a layer counts as invisible.
const VISIBLE_OPACITY: f64 = 0.001;

/// The outermost chapters (the scenes) shorter than a second. Chapters of
/// length 0 never play and are left out, as are parts inside a scene.
pub fn short_chapters(chapters: &[ReactChapter], frame_rate: Rational) -> Vec<QualityIssue> {
    chapters
        .iter()
        .enumerate()
        .filter(|&(index, chapter)| chapter.duration_in_frames > 0 && is_outermost(chapters, index))
        .filter_map(|(_, chapter)| {
            let length = seconds(chapter.duration_in_frames, frame_rate);
            (length < MIN_SCENE_SECONDS).then(|| {
                let name = if chapter.name == chapter.id {
                    format!("{:?}", chapter.id)
                } else {
                    format!("{:?} ({})", chapter.name, chapter.id)
                };
                short_scene(
                    format!("chapter {name}"),
                    length,
                    FrameSpan::new(chapter.from, chapter.end(), frame_rate),
                )
            })
        })
        .collect()
}

/// Whether no other chapter's span holds chapter `index`'s; of chapters
/// with the same span the first is the outer one, as the bridge's
/// `outermost_chapter_at` decides.
fn is_outermost(chapters: &[ReactChapter], index: usize) -> bool {
    let chapter = &chapters[index];
    !chapters.iter().enumerate().any(|(other_index, other)| {
        other_index != index
            && other.from <= chapter.from
            && chapter.end() <= other.end()
            && (other_index < index || (other.from, other.end()) != (chapter.from, chapter.end()))
    })
}

/// A JSON project's enabled items on enabled video tracks that last less
/// than a second.
pub fn short_items(project: &Project) -> Vec<QualityIssue> {
    let frame_rate = project.settings.frame_rate;
    let fps = f64::from(frame_rate.numerator) / f64::from(frame_rate.denominator.max(1));
    let mut issues = Vec::new();
    for track in &project.tracks {
        if track.kind != TrackKind::Video || track.enabled == Some(false) {
            continue;
        }
        for item in &track.items {
            if item.enabled == Some(false) {
                continue;
            }
            let (Ok(start), Ok(length)) = (
                item.range.start.as_seconds(),
                item.range.duration.as_seconds(),
            ) else {
                continue;
            };
            if length <= 0.0 || length >= MIN_SCENE_SECONDS {
                continue;
            }
            // The frames whose time falls in `[start, end)`, as the
            // evaluator shows the item; one that falls between two frames
            // shows on none.
            let first_at = |seconds: f64| (seconds * fps - 1e-9).ceil().max(0.0) as u64;
            let (from, to) = (first_at(start), first_at(start + length));
            if to <= from {
                continue;
            }
            let name = item.name.as_deref().map_or_else(
                || format!("{:?}", item.id.to_string()),
                |name| format!("{name:?}"),
            );
            issues.push(short_scene(
                format!("item {name} on video track {:?}", track.name),
                length,
                FrameSpan::new(from, to, frame_rate),
            ));
        }
    }
    issues
}

fn short_scene(what: String, length: f64, span: FrameSpan) -> QualityIssue {
    let mut issue = QualityIssue::new(
        QualityCode::ShortScene,
        Measured::Layers,
        format!(
            "{what} lasts {length:.2} s ({}), under {MIN_SCENE_SECONDS} s; give each scene at least a second",
            describe_spans(&[span])
        ),
    );
    issue.frames = vec![span];
    issue.value = Some(round(length, 1000.0));
    issue.limit = Some(MIN_SCENE_SECONDS);
    issue
}

/// The checks that read each evaluated frame's layers: whether frame 0
/// shows anything, and each text layer's contrast against the solid colors
/// behind its anchor point. Frames may come in any selection, in order.
pub struct SceneLint {
    frame_rate: Rational,
    contrast: bool,
    first_frame: Option<QualityIssue>,
    texts: BTreeMap<String, TextRecord>,
}

/// What one text layer's frames found.
struct TextRecord {
    /// The worst measured ratio and the colors it was measured between.
    worst: f64,
    limit: f64,
    colors: ([u8; 3], [u8; 3]),
    sample: String,
    failing: Vec<(u64, u64)>,
    /// The last frame's finding, kept until the next frame says whether it
    /// was moving, and whether it was moving from the frame before.
    pending: Option<(u64, TextFinding, bool)>,
}

impl TextRecord {
    /// Counts `finding`, at `frame`, unless the text was fading or scaling
    /// in or out there.
    fn settle(&mut self, frame: u64, finding: &TextFinding, moving: bool) {
        if moving || finding.ratio >= finding.limit {
            return;
        }
        push_frame(&mut self.failing, frame);
        if finding.ratio < self.worst {
            self.worst = finding.ratio;
            self.limit = finding.limit;
            self.colors = (finding.fg, finding.bg);
        }
    }

    /// Takes `finding` at `frame`, settling the frame before.
    fn take(&mut self, frame: u64, finding: TextFinding) {
        let mut moved_in = false;
        if let Some((last, before, moved_before)) = self.pending.take() {
            let adjacent = last + 1 == frame;
            moved_in = adjacent && finding.differs(&before);
            self.settle(last, &before, moved_before || moved_in);
        }
        self.pending = Some((frame, finding, moved_in));
    }

    fn finish(&mut self) {
        if let Some((last, finding, moved)) = self.pending.take() {
            self.settle(last, &finding, moved);
        }
    }
}

impl SceneLint {
    /// A lint for frames at `frame_rate`; `contrast` false leaves the text
    /// out (for `--check`, which measures it in pixels).
    pub fn new(frame_rate: Rational, contrast: bool) -> Self {
        Self {
            frame_rate,
            contrast,
            first_frame: None,
            texts: BTreeMap::new(),
        }
    }

    /// Reads composition frame `frame`'s scene.
    pub fn observe(&mut self, frame: u64, scene: &Scene) {
        let mut walk = Walk::new(scene, self.contrast);
        walk.layers(&scene.layers, Affine::IDENTITY, 1.0, Certainty::EXACT);
        if frame == 0 && !walk.content {
            let mut issue = QualityIssue::new(
                QualityCode::BlankFirstFrame,
                Measured::Layers,
                if walk.flat_rects {
                    "frame 0 shows only solid rectangles: it is the thumbnail and the video's first frame, so put the title or the subject on it from the start".to_owned()
                } else {
                    "frame 0 draws nothing but the background: it is the thumbnail and the video's first frame, so put the title or the subject on it from the start".to_owned()
                },
            );
            issue.frames = vec![FrameSpan::new(0, 1, self.frame_rate)];
            self.first_frame = Some(issue);
        }
        // A text fading or scaling in or out is measured once it holds
        // still: a frame counts once the next one shows it did not move.
        for text in walk.texts {
            let record = self
                .texts
                .entry(text.id.clone())
                .or_insert_with(|| TextRecord {
                    worst: f64::INFINITY,
                    limit: text.limit,
                    colors: (text.fg, text.bg),
                    sample: text.sample.clone(),
                    failing: Vec::new(),
                    pending: None,
                });
            record.take(frame, text);
        }
    }

    /// The issues every observed frame adds up to.
    pub fn finish(self) -> Vec<QualityIssue> {
        let mut issues: Vec<QualityIssue> = self.first_frame.into_iter().collect();
        for (id, mut record) in self.texts {
            record.finish();
            if record.failing.is_empty() {
                continue;
            }
            let spans: Vec<FrameSpan> = record
                .failing
                .iter()
                .map(|&(from, to)| FrameSpan::new(from, to, self.frame_rate))
                .collect();
            let mut issue = QualityIssue::new(
                QualityCode::LowContrastText,
                Measured::Layers,
                format!(
                    "text layer {id:?} ({:?}) is {} on {} behind it: {:.2}:1, below {}:1, at {}",
                    record.sample,
                    hex(record.colors.0),
                    hex(record.colors.1),
                    record.worst,
                    record.limit,
                    describe_spans(&spans)
                ),
            );
            issue.frames = spans;
            issue.layer = Some(id);
            issue.value = Some(round(record.worst, 100.0));
            issue.limit = Some(record.limit);
            issues.push(issue);
        }
        issues
    }
}

fn round(value: f64, scale: f64) -> f64 {
    (value * scale).round() / scale
}

fn hex([red, green, blue]: [u8; 3]) -> String {
    format!("#{red:02X}{green:02X}{blue:02X}")
}

/// A 2D map `x' = a x + c y + tx`, `y' = b x + d y + ty`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}

impl Affine {
    pub(super) const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// A layer's map from its own plane, relative to its anchor point, to
    /// its parent's, as the renderer draws a flat transform.
    pub(super) fn of(transform: &EvaluatedTransform) -> Self {
        let linear = transform.linear();
        let position = transform.translation(&linear);
        Self {
            a: linear[0][0],
            b: linear[1][0],
            c: linear[0][1],
            d: linear[1][1],
            tx: position.x,
            ty: position.y,
        }
    }

    fn translated(x: f64, y: f64) -> Self {
        Self {
            tx: x,
            ty: y,
            ..Self::IDENTITY
        }
    }

    pub(super) fn then(self, child: Self) -> Self {
        Self {
            a: self.a * child.a + self.c * child.b,
            b: self.b * child.a + self.d * child.b,
            c: self.a * child.c + self.c * child.d,
            d: self.b * child.c + self.d * child.d,
            tx: self.a * child.tx + self.c * child.ty + self.tx,
            ty: self.b * child.tx + self.d * child.ty + self.ty,
        }
    }

    fn apply(self, (x, y): (f64, f64)) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.tx,
            self.b * x + self.d * y + self.ty,
        )
    }

    fn determinant(self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    fn invert(self) -> Option<Self> {
        let determinant = self.determinant();
        if !determinant.is_finite() || determinant.abs() < 1e-12 {
            return None;
        }
        let (a, b, c, d) = (
            self.d / determinant,
            -self.b / determinant,
            -self.c / determinant,
            self.a / determinant,
        );
        Some(Self {
            a,
            b,
            c,
            d,
            tx: -(a * self.tx + c * self.ty),
            ty: -(b * self.tx + d * self.ty),
        })
    }

    /// How much it scales lengths, on average.
    pub(super) fn scale(self) -> f64 {
        self.determinant().abs().sqrt()
    }
}

/// How much of a layer's place in the picture the walk knows.
#[derive(Clone, Copy)]
struct Certainty {
    /// Every ancestor is flat (no skew, 3D turn, or perspective), so the
    /// layer's map is an affine one.
    geometry: bool,
    /// Every ancestor blends normally, with no mask: what the layer draws
    /// is what shows.
    exact: bool,
}

impl Certainty {
    const EXACT: Self = Self {
        geometry: true,
        exact: true,
    };
}

/// Something drawn before a text, which may be behind it.
enum Backdrop {
    /// A solid color (straight sRGB, alpha 0–1) over a box of its own plane.
    Solid {
        inverse: Affine,
        bounds: [f64; 4],
        color: [f64; 4],
    },
    /// Pixels of unknown colors over a box.
    Unknown { inverse: Affine, bounds: [f64; 4] },
    /// Pixels of unknown colors anywhere.
    Everywhere,
}

impl Backdrop {
    fn covers(&self, point: (f64, f64)) -> bool {
        match self {
            Self::Solid {
                inverse, bounds, ..
            }
            | Self::Unknown { inverse, bounds } => {
                let (x, y) = inverse.apply(point);
                bounds[0] <= x && x <= bounds[2] && bounds[1] <= y && y <= bounds[3]
            }
            Self::Everywhere => true,
        }
    }
}

/// A group's clip: a box of the group's children's plane, and the map to
/// it from the canvas. Corners are taken as square.
#[derive(Clone, Copy)]
struct ClipBox {
    inverse: Affine,
    bounds: [f64; 4],
}

impl ClipBox {
    fn contains(&self, point: (f64, f64)) -> bool {
        let (x, y) = self.inverse.apply(point);
        let [x0, y0, x1, y1] = self.bounds;
        x0 <= x && x <= x1 && y0 <= y && y <= y1
    }
}

/// One text layer's measurement in one frame.
#[derive(Clone, Debug)]
struct TextFinding {
    id: String,
    sample: String,
    ratio: f64,
    limit: f64,
    fg: [u8; 3],
    bg: [u8; 3],
    opacity: f64,
    scale: f64,
}

impl TextFinding {
    /// Whether the text's opacity or size changed from `other`'s.
    fn differs(&self, other: &Self) -> bool {
        (self.opacity - other.opacity).abs() > 1e-3 || (self.scale - other.scale).abs() > 1e-3
    }
}

/// One frame's walk through its layers in drawing order.
struct Walk {
    short_side: f64,
    contrast: bool,
    /// What was drawn so far, each with the group clips it was drawn
    /// through.
    backdrops: Vec<(Backdrop, Vec<ClipBox>)>,
    /// The clips of the groups the walk is in.
    clips: Vec<ClipBox>,
    texts: Vec<TextFinding>,
    /// Whether anything but solid rectangles is visible.
    content: bool,
    /// Whether a solid rectangle is.
    flat_rects: bool,
}

impl Walk {
    fn new(scene: &Scene, contrast: bool) -> Self {
        Self {
            short_side: f64::from(scene.width.min(scene.height).max(1)),
            contrast,
            backdrops: Vec::new(),
            clips: Vec::new(),
            texts: Vec::new(),
            content: false,
            flat_rects: false,
        }
    }

    fn layers(&mut self, layers: &[Layer], parent: Affine, opacity: f64, certainty: Certainty) {
        for layer in layers {
            self.layer(layer, parent, opacity, certainty);
        }
    }

    fn layer(&mut self, layer: &Layer, parent: Affine, opacity: f64, certainty: Certainty) {
        let opacity = opacity * layer.opacity.clamp(0.0, 1.0);
        let local = Affine::of(&layer.transform);
        let world = parent.then(local);
        if opacity < VISIBLE_OPACITY || world.scale() < 1e-6 {
            return;
        }
        let flat = layer.transform.is_flat() && layer.transform.perspective_distance().is_none();
        let certainty = Certainty {
            geometry: certainty.geometry && flat,
            exact: certainty.exact && layer.blend_mode.is_normal(),
        };
        let anchored = |width: f64, height: f64| {
            world.then(Affine::translated(
                -layer.transform.anchor.x * width,
                -layer.transform.anchor.y * height,
            ))
        };
        match &layer.content {
            LayerContent::Group {
                layers, mask, clip, ..
            } => {
                let certainty = Certainty {
                    exact: certainty.exact && mask.is_none(),
                    ..certainty
                };
                // Under an unknown map, the children are unknown anyway.
                let clipped = match clip {
                    Some(clip) if certainty.geometry => {
                        let Some(inverse) = world.invert() else {
                            return;
                        };
                        self.clips.push(ClipBox {
                            inverse,
                            bounds: [clip.x, clip.y, clip.x + clip.width, clip.y + clip.height],
                        });
                        true
                    }
                    _ => false,
                };
                self.layers(layers, world, opacity, certainty);
                if clipped {
                    self.clips.pop();
                }
            }
            LayerContent::Rect {
                width,
                height,
                fill,
                stroke,
                ..
            } => {
                let solid = match fill {
                    Some(Paint::Solid { color }) => Color::from_hex(color),
                    _ => None,
                };
                if (fill.is_some() && solid.is_none()) || stroke.is_some() {
                    self.content = true;
                } else if solid.is_some_and(|color| color.alpha > 0) {
                    self.flat_rects = true;
                }
                if fill.is_none() {
                    return;
                }
                let bounds = [0.0, 0.0, *width, *height];
                let filtered = !layer.effects.filters.is_empty() || layer.effects.blur > 0.0;
                let backdrop = if certainty.geometry {
                    let Some(inverse) = anchored(*width, *height).invert() else {
                        return;
                    };
                    match solid {
                        Some(color) if certainty.exact && !filtered => Backdrop::Solid {
                            inverse,
                            bounds,
                            color: [
                                f64::from(color.red),
                                f64::from(color.green),
                                f64::from(color.blue),
                                f64::from(color.alpha) / 255.0 * opacity,
                            ],
                        },
                        _ => Backdrop::Unknown { inverse, bounds },
                    }
                } else {
                    Backdrop::Everywhere
                };
                self.drawn(backdrop);
            }
            LayerContent::Image { width, height, .. }
            | LayerContent::Video { width, height, .. } => {
                self.content = true;
                let backdrop = match (width, height, certainty.geometry) {
                    (Some(width), Some(height), true) => match anchored(*width, *height).invert() {
                        Some(inverse) => Backdrop::Unknown {
                            inverse,
                            bounds: [0.0, 0.0, *width, *height],
                        },
                        None => return,
                    },
                    _ => Backdrop::Everywhere,
                };
                self.drawn(backdrop);
            }
            LayerContent::Path { commands, .. } => {
                self.content = true;
                let backdrop = match (path_bounds(commands), world.invert(), certainty.geometry) {
                    (None, _, _) => return,
                    (Some(bounds), Some(inverse), true) => Backdrop::Unknown { inverse, bounds },
                    (Some(_), None, true) => return,
                    (Some(_), _, false) => Backdrop::Everywhere,
                };
                self.drawn(backdrop);
            }
            LayerContent::Instances { count, .. } => {
                if *count > 0 {
                    self.content = true;
                    self.drawn(Backdrop::Everywhere);
                }
            }
            LayerContent::Text { text, style, .. } => {
                let shown = match style.visible_characters {
                    Some(visible) => text.chars().take(visible).collect::<String>(),
                    None => text.clone(),
                };
                if shown.trim().is_empty() {
                    return;
                }
                self.content = true;
                if self.contrast && certainty.geometry && certainty.exact {
                    self.text(layer, &shown, style, world, opacity);
                }
            }
            LayerContent::Psd { .. }
            | LayerContent::Plugin { .. }
            | LayerContent::StaticRef { .. }
            | LayerContent::MissingComponent { .. } => {
                self.content = true;
                self.drawn(Backdrop::Everywhere);
            }
        }
    }

    /// Keeps `backdrop`, drawn through the current clips.
    fn drawn(&mut self, backdrop: Backdrop) {
        self.backdrops.push((backdrop, self.clips.clone()));
    }

    /// Measures a text layer against the solid colors under its anchor
    /// point, when nothing of unknown color is there.
    fn text(&mut self, layer: &Layer, shown: &str, style: &TextStyle, world: Affine, opacity: f64) {
        if !layer.blend_mode.is_normal() || !style.color_runs.is_empty() {
            return;
        }
        let fill = match &style.fill {
            None => Color::WHITE,
            Some(Paint::Solid { color }) => match Color::from_hex(color) {
                Some(color) => color,
                None => return,
            },
            Some(_) => return,
        };
        let point = world.apply((0.0, 0.0));
        // Clipped away there, the text shows nothing to measure.
        if !self.clips.iter().all(|clip| clip.contains(point)) {
            return;
        }
        let mut behind = BACKGROUND.map(f64::from);
        for (backdrop, clips) in &self.backdrops {
            if !backdrop.covers(point) || !clips.iter().all(|clip| clip.contains(point)) {
                continue;
            }
            match backdrop {
                Backdrop::Solid { color, .. } => behind = over(behind, *color),
                Backdrop::Unknown { .. } | Backdrop::Everywhere => return,
            }
        }
        let behind = encode(behind);
        let alpha = f64::from(fill.alpha) / 255.0 * opacity;
        let fg = encode(over(
            behind.map(f64::from),
            [
                f64::from(fill.red),
                f64::from(fill.green),
                f64::from(fill.blue),
                alpha,
            ],
        ));
        let fg_luminance = relative_luminance(fg);
        let mut ratio = contrast_ratio(fg_luminance, relative_luminance(behind));
        // An outline, shadow, or glow is the text's own backdrop where it
        // touches the glyphs.
        for halo in halo_colors(layer, style) {
            ratio = ratio.max(contrast_ratio(fg_luminance, relative_luminance(halo)));
        }
        let scale = world.scale();
        let size =
            style.font_size.unwrap_or(DEFAULT_FONT_SIZE).abs() * scale * 1080.0 / self.short_side;
        let bold = style.font_weight.unwrap_or(400) >= BOLD_WEIGHT;
        let large = size >= LARGE_TEXT_PX || bold && size >= LARGE_BOLD_TEXT_PX;
        let mut sample: String = shown.chars().take(24).collect();
        if shown.chars().count() > 24 {
            sample.push('…');
        }
        self.texts.push(TextFinding {
            id: layer.id.clone(),
            sample,
            ratio,
            limit: if large {
                LARGE_TEXT_CONTRAST
            } else {
                TEXT_CONTRAST
            },
            fg,
            bg: behind,
            opacity,
            scale,
        });
    }
}

/// The solid colors that border a text's glyphs: its innermost stroke,
/// its shadow, and its glow.
pub(super) fn halo_colors(layer: &Layer, style: &TextStyle) -> Vec<[u8; 3]> {
    let mut colors = Vec::new();
    if let Some(stroke) = &style.stroke
        && let Some(innermost) = stroke
            .strokes()
            .iter()
            .filter(|stroke| stroke.width > 0.0)
            .min_by(|a, b| a.width.total_cmp(&b.width))
        && let Paint::Solid { color } = &innermost.paint
        && let Some(color) = Color::from_hex(color)
        && color.alpha >= 128
    {
        colors.push([color.red, color.green, color.blue]);
    }
    let shadow = layer.effects.shadow.as_ref().map(|shadow| &shadow.color);
    let glow = layer.effects.glow.as_ref().map(|glow| &glow.color);
    for color in shadow.into_iter().chain(glow) {
        if let Some(color) = Color::from_hex(color)
            && color.alpha >= 128
        {
            colors.push([color.red, color.green, color.blue]);
        }
    }
    colors
}

/// `color` (straight, alpha 0–1) over `behind`, in encoded sRGB as the
/// renderer blends.
fn over(behind: [f64; 3], [red, green, blue, alpha]: [f64; 4]) -> [f64; 3] {
    let alpha = alpha.clamp(0.0, 1.0);
    [
        behind[0] + (red - behind[0]) * alpha,
        behind[1] + (green - behind[1]) * alpha,
        behind[2] + (blue - behind[2]) * alpha,
    ]
}

fn encode(color: [f64; 3]) -> [u8; 3] {
    color.map(|channel| channel.round().clamp(0.0, 255.0) as u8)
}

/// The box around a path's points (control points included), in its own
/// plane.
fn path_bounds(commands: &[PathCommand]) -> Option<[f64; 4]> {
    let mut bounds: Option<[f64; 4]> = None;
    let mut add = |x: f64, y: f64| {
        let b = bounds.get_or_insert([x, y, x, y]);
        *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    };
    for command in commands {
        match *command {
            PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => add(x, y),
            PathCommand::QuadTo { x1, y1, x, y } => {
                add(x1, y1);
                add(x, y);
            }
            PathCommand::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                add(x1, y1);
                add(x2, y2);
                add(x, y);
            }
            PathCommand::Close => {}
        }
    }
    bounds
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scene(layers: serde_json::Value) -> Scene {
        serde_json::from_value(json!({
            "width": 1920, "height": 1080,
            "frameRate": { "numerator": 30, "denominator": 1 },
            "time": { "value": 0, "timescale": 30 },
            "layers": layers,
        }))
        .unwrap()
    }

    fn rect(id: &str, color: &str) -> serde_json::Value {
        json!({
            "id": id,
            "transform": { "position": { "x": 0, "y": 0 }, "anchor": { "x": 0, "y": 0 } },
            "content": { "type": "rect", "width": 1920, "height": 1080, "fill": { "type": "solid", "color": color } },
        })
    }

    fn text(id: &str, color: &str, size: f64, opacity: f64) -> serde_json::Value {
        json!({
            "id": id,
            "opacity": opacity,
            "transform": { "position": { "x": 960, "y": 540 } },
            "content": { "type": "text", "text": "Hello, Celesta", "style": { "fontSize": size, "fill": { "type": "solid", "color": color } } },
        })
    }

    fn lint(frames: &[(u64, Scene)]) -> Vec<QualityIssue> {
        let mut lint = SceneLint::new(Rational::new(30, 1), true);
        for (frame, scene) in frames {
            lint.observe(*frame, scene);
        }
        lint.finish()
    }

    #[test]
    fn a_first_frame_of_only_background_is_blank() {
        let issues = lint(&[(
            0,
            scene(json!([
                rect("bg", "#101018"),
                text("t", "#ffffff", 64.0, 0.0)
            ])),
        )]);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].code, QualityCode::BlankFirstFrame);
        assert!(issues[0].message.contains("only solid rectangles"));

        let issues = lint(&[(0, scene(json!([])))]);
        assert!(issues[0].message.contains("draws nothing"));

        // The title is there from the start; a later empty frame is fine.
        let issues = lint(&[
            (
                0,
                scene(json!([
                    rect("bg", "#101018"),
                    text("t", "#ffffff", 64.0, 1.0)
                ])),
            ),
            (5, scene(json!([]))),
        ]);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn dark_text_on_a_dark_ground_is_low_contrast_with_its_frames() {
        let frames: Vec<(u64, Scene)> = (0..3)
            .map(|frame| {
                (
                    frame,
                    scene(json!([
                        rect("bg", "#0F2A3D"),
                        text("title", "#1F4A6D", 20.0, 1.0)
                    ])),
                )
            })
            .collect();
        let issues = lint(&frames);
        assert_eq!(issues.len(), 1, "{issues:?}");
        let issue = &issues[0];
        assert_eq!(issue.code, QualityCode::LowContrastText);
        assert_eq!(issue.layer.as_deref(), Some("title"));
        assert_eq!(issue.limit, Some(TEXT_CONTRAST));
        assert_eq!((issue.frames[0].from, issue.frames[0].to), (0, 3));
        assert!(issue.value.unwrap() < 2.0, "{issue:?}");
        assert!(
            issue.message.contains("#1F4A6D on #0F2A3D"),
            "{}",
            issue.message
        );
    }

    #[test]
    fn large_text_needs_3_to_1_and_light_text_passes() {
        // #767676 on black is about 4.7:1; #5A5A5A about 3.0:1.
        let issues = lint(&[(
            1,
            scene(json!([
                rect("bg", "#000000"),
                text("big", "#5C5C5C", 96.0, 1.0)
            ])),
        )]);
        assert!(issues.is_empty(), "{issues:?}");
        let issues = lint(&[(
            1,
            scene(json!([
                rect("bg", "#000000"),
                text("small", "#5C5C5C", 16.0, 1.0)
            ])),
        )]);
        assert_eq!(issues[0].limit, Some(TEXT_CONTRAST), "{issues:?}");
        let issues = lint(&[(
            1,
            scene(json!([
                rect("bg", "#0F2A3D"),
                text("t", "#EAF4F2", 20.0, 1.0)
            ])),
        )]);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn fading_text_is_measured_once_it_holds_still() {
        let frames: Vec<(u64, Scene)> = [0.1, 0.4, 0.7, 1.0, 1.0]
            .iter()
            .enumerate()
            .map(|(frame, &opacity)| {
                (
                    frame as u64 + 1,
                    scene(json!([
                        rect("bg", "#000000"),
                        text("t", "#FFFFFF", 40.0, opacity)
                    ])),
                )
            })
            .collect();
        assert!(lint(&frames).is_empty());
        // Held at 20 % it stays low contrast.
        let frames: Vec<(u64, Scene)> = (1..4)
            .map(|frame| {
                (
                    frame,
                    scene(json!([
                        rect("bg", "#000000"),
                        text("t", "#FFFFFF", 40.0, 0.2)
                    ])),
                )
            })
            .collect();
        assert_eq!(lint(&frames)[0].frames[0].from, 1);
    }

    #[test]
    fn unknown_backdrops_and_outlines_are_not_guessed() {
        let image = json!({
            "id": "photo",
            "transform": { "position": { "x": 960, "y": 540 } },
            "content": { "type": "image", "asset": { "id": "a", "location": { "type": "file", "path": "a.png" } }, "width": 1920, "height": 1080 },
        });
        let issues = lint(&[(
            1,
            scene(json!([
                rect("bg", "#FFFFFF"),
                image,
                text("t", "#FFFFFF", 40.0, 1.0)
            ])),
        )]);
        assert!(issues.is_empty(), "{issues:?}");

        // White on white with a black outline reads.
        let mut outlined = text("t", "#FFFFFF", 40.0, 1.0);
        outlined["content"]["style"]["stroke"] =
            json!({ "paint": { "type": "solid", "color": "#000000" }, "width": 4 });
        let issues = lint(&[(1, scene(json!([rect("bg", "#FFFFFF"), outlined])))]);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn a_rect_away_from_the_text_is_not_behind_it() {
        let mut card = rect("card", "#FFFFFF");
        card["content"]["width"] = json!(200);
        card["content"]["height"] = json!(100);
        // The card sits at the top left; the white text is over the black ground.
        let issues = lint(&[(
            1,
            scene(json!([
                rect("bg", "#000000"),
                card,
                text("t", "#FFFFFF", 40.0, 1.0)
            ])),
        )]);
        assert!(issues.is_empty(), "{issues:?}");
        // Moved under the text, it hides it.
        let mut card = rect("card", "#FFFFFF");
        card["transform"]["position"] = json!({ "x": 900, "y": 500 });
        card["content"]["width"] = json!(200);
        card["content"]["height"] = json!(100);
        let issues = lint(&[(
            1,
            scene(json!([
                rect("bg", "#000000"),
                card,
                text("t", "#FFFFFF", 40.0, 1.0)
            ])),
        )]);
        assert_eq!(issues.len(), 1, "{issues:?}");
    }

    #[test]
    fn a_backdrop_clipped_away_is_not_behind_the_text() {
        // A white full-canvas rect in a group clipped to the left half; the
        // white text stands on the black right half.
        let clipped = json!({
            "id": "left",
            "transform": { "position": { "x": 0, "y": 0 } },
            "content": { "type": "group", "clip": { "x": 0, "y": 0, "width": 960, "height": 1080 }, "layers": [rect("white", "#FFFFFF")] },
        });
        let mut title = text("t", "#FFFFFF", 40.0, 1.0);
        title["transform"]["position"] = json!({ "x": 1400, "y": 540 });
        let issues = lint(&[(
            1,
            scene(json!([rect("bg", "#000000"), clipped.clone(), title])),
        )]);
        assert!(issues.is_empty(), "{issues:?}");
        // On the left half the white rect is behind it.
        let mut title = text("t", "#FFFFFF", 40.0, 1.0);
        title["transform"]["position"] = json!({ "x": 400, "y": 540 });
        let issues = lint(&[(1, scene(json!([rect("bg", "#000000"), clipped, title])))]);
        assert_eq!(issues.len(), 1, "{issues:?}");
    }

    fn chapter(id: &str, from: u64, duration_in_frames: u64) -> ReactChapter {
        ReactChapter {
            id: id.into(),
            name: id.into(),
            from,
            duration_in_frames,
            desc: None,
            status: Default::default(),
        }
    }

    #[test]
    fn outermost_chapters_under_a_second_are_short_scenes() {
        let chapters = [
            chapter("intro", 0, 15),
            chapter("body", 15, 90),
            chapter("beat", 20, 5),
            chapter("never", 105, 0),
            chapter("outro", 105, 30),
        ];
        let issues = short_chapters(&chapters, Rational::new(30, 1));
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].code, QualityCode::ShortScene);
        assert_eq!(issues[0].value, Some(0.5));
        assert_eq!((issues[0].frames[0].from, issues[0].frames[0].to), (0, 15));
        assert!(
            issues[0]
                .message
                .starts_with("chapter \"intro\" lasts 0.50 s (frames 0–14"),
            "{}",
            issues[0].message
        );
    }

    #[test]
    fn short_video_track_items_are_short_scenes() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/editor-demo.celesta.json");
        let mut project = Project::load(path).unwrap();
        // Its one track is an overlay, whose items are not scenes.
        project.tracks[0].items[0].range.duration = celesta_composition::Time::new(1, 2);
        assert!(short_items(&project).is_empty());
        project.tracks[0].kind = TrackKind::Video;
        let issues = short_items(&project);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].value, Some(0.5));
        // At 60 fps an item ending at 30.25 frames shows on frames 0–30.
        project.tracks[0].items[0].range.duration = celesta_composition::Time::new(121, 240);
        let issues = short_items(&project);
        assert_eq!((issues[0].frames[0].from, issues[0].frames[0].to), (0, 31));
        // One between two frames shows on none.
        project.tracks[0].items[0].range = celesta_composition::TimeRange {
            start: celesta_composition::Time::new(1, 240),
            duration: celesta_composition::Time::new(1, 480),
        };
        assert!(short_items(&project).is_empty());
    }
}
