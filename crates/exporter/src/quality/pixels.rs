//! Checks measured on rendered frames: flashing and red flashing (WCAG
//! 2.3.1), shots between hard cuts shorter than a second, and a first frame
//! of one flat color.
//!
//! Each frame is reduced to a grid of cells averaged in linear light (at
//! most [`GRID_WIDTH`] across). A cell makes a *transition* when its
//! relative luminance has moved by at least 0.1 the other way from its last
//! extreme, the darker of the two below 0.8 (a red transition: `(R − G − B)
//! · 320` moved by more than 20, one side saturated red, `R / (R + G + B) ≥
//! 0.8`). WCAG counts a flash where such transitions cover a quarter of any
//! 10° field of view, which on a screen at the usual distance is a third of
//! its width by a third of its height: windows of that size slide over the
//! grid, and one that has transitions over more than a quarter of its area
//! in a frame counts one transition there. More than six in one second
//! (three flashes, each a pair of opposing transitions) is a warning.

use celesta_composition::Rational;

use super::luminance::{linear_table, luminance_of_linear};
use super::{
    FrameSpan, MAX_FLASHES_PER_SECOND, MIN_SCENE_SECONDS, Measured, QualityCode, QualityIssue,
    Region, describe_spans, frames_per_second,
};

/// Most cells across the analysis grid.
const GRID_WIDTH: u32 = 160;
/// A luminance change of this much (0–1) is a general transition.
const LUMINANCE_STEP: f32 = 0.1;
/// Both sides of a general transition at or above this are too bright to
/// flash.
const DARKER_BELOW: f32 = 0.8;
/// A change of `(R − G − B) · 320` larger than this is a red transition.
const RED_STEP: f32 = 20.0;
const SATURATED_RED: f32 = 0.8;
/// A window flashes where transitions cover more than this share of it.
const FLASH_AREA: f64 = 0.25;
/// A cell changed in a cut when one encoded channel moved this much (0–1).
const CUT_STEP: f32 = 0.2;
/// A frame is a hard cut when this share of the cells changed at once.
const CUT_AREA: f64 = 0.6;
/// A first frame is flat when fewer than this share of its pixels differ
/// from its median color by more than [`FLAT_TOLERANCE`] codes.
const FLAT_SHARE: f64 = 0.000_5;
const FLAT_TOLERANCE: i32 = 8;

/// Renders' pixel checks over consecutive frames: feed every frame of the
/// checked range in order, then [`Self::finish`].
pub struct PixelLint {
    frame_rate: Rational,
    width: u32,
    height: u32,
    grid: Grid,
    general: Vec<Swing>,
    red: Vec<Swing>,
    general_events: Windows,
    red_events: Windows,
    /// Each cell's encoded color in the frame before.
    previous: Vec<[f32; 3]>,
    first: Option<u64>,
    last: Option<u64>,
    cuts: Vec<u64>,
    blank: Option<QualityIssue>,
}

impl PixelLint {
    /// A lint for `width`×`height` RGBA frames at `frame_rate`.
    pub fn new(width: u32, height: u32, frame_rate: Rational) -> Self {
        let grid = Grid::new(width, height);
        let cells = grid.cells();
        Self {
            frame_rate,
            width,
            height,
            general_events: Windows::new(&grid),
            red_events: Windows::new(&grid),
            grid,
            general: vec![Swing::default(); cells],
            red: vec![Swing::default(); cells],
            previous: Vec::new(),
            first: None,
            last: None,
            cuts: Vec::new(),
            blank: None,
        }
    }

    /// Reads composition frame `frame`, rendered as opaque straight RGBA8.
    /// A frame that does not follow the one before starts the transitions
    /// over.
    pub fn observe(&mut self, frame: u64, rgba: &[u8]) {
        if frame == 0 {
            self.blank = flat_first_frame(rgba, self.frame_rate);
        }
        let follows = self.last.is_some_and(|last| last + 1 == frame);
        if !follows {
            self.general.fill(Swing::default());
            self.red.fill(Swing::default());
            self.previous.clear();
        }
        self.first.get_or_insert(frame);
        self.last = Some(frame);
        let cells = self.grid.average(rgba, self.width);
        let mut general = vec![false; cells.len()];
        let mut red = vec![false; cells.len()];
        for (index, &linear) in cells.iter().enumerate() {
            let luminance = luminance_of_linear(linear.map(f64::from)) as f32;
            general[index] =
                self.general[index].step(luminance, false, LUMINANCE_STEP, |a, _, b, _| {
                    a.min(b) < DARKER_BELOW
                });
            let [r, g, b] = linear;
            let sum = r + g + b;
            let saturated = sum > 0.0 && r / sum >= SATURATED_RED;
            let redness = (r - g - b).max(0.0) * 320.0;
            red[index] = self.red[index].step(redness, saturated, RED_STEP, |_, a, _, b| a || b);
        }
        self.general_events.record(frame, &general, &self.grid);
        self.red_events.record(frame, &red, &self.grid);
        let encoded: Vec<[f32; 3]> = cells
            .iter()
            .map(|linear| linear.map(|channel| channel.max(0.0).powf(1.0 / 2.2)))
            .collect();
        if follows && !self.previous.is_empty() {
            let changed = encoded
                .iter()
                .zip(&self.previous)
                .filter(|(now, before)| {
                    now.iter()
                        .zip(before.iter())
                        .any(|(now, before)| (now - before).abs() >= CUT_STEP)
                })
                .count();
            if changed as f64 >= CUT_AREA * encoded.len() as f64 {
                self.cuts.push(frame);
            }
        }
        self.previous = encoded;
    }

    /// The issues the frames add up to. `ends_composition` says the last
    /// frame fed is the composition's last, so the shot it ends is whole.
    pub fn finish(self, ends_composition: bool) -> Vec<QualityIssue> {
        let fps = frames_per_second(self.frame_rate);
        let scale = (
            f64::from(self.width) / f64::from(self.grid.width),
            f64::from(self.height) / f64::from(self.grid.height),
        );
        let mut issues: Vec<QualityIssue> = self.blank.into_iter().collect();
        let mut flashing = Vec::new();
        for (code, windows) in [
            (QualityCode::Flashing, &self.general_events),
            (QualityCode::RedFlashing, &self.red_events),
        ] {
            for violation in windows.violations(fps) {
                flashing.push((violation.from, violation.to));
                issues.push(flash_issue(code, &violation, scale, self.frame_rate));
            }
        }
        if let (Some(first), Some(last)) = (self.first, self.last) {
            issues.extend(short_shots(
                first,
                last + 1,
                &self.cuts,
                ends_composition,
                &flashing,
                fps,
                self.frame_rate,
            ));
        }
        issues
    }
}

/// One cell's luminance (or redness) path: the extreme it last turned at,
/// and which way it is going.
#[derive(Clone, Copy, Debug, Default)]
struct Swing {
    started: bool,
    extreme: f32,
    /// Whether the extreme was saturated red.
    flag: bool,
    /// 1 rising, -1 falling, 0 not yet moved.
    direction: i8,
}

impl Swing {
    /// Takes the next value; true when it completes a transition: a move of
    /// at least `step` from the extreme against the direction so far, which
    /// `counts(extreme, flag, value, flag)` accepts.
    fn step(
        &mut self,
        value: f32,
        flag: bool,
        step: f32,
        counts: impl Fn(f32, bool, f32, bool) -> bool,
    ) -> bool {
        if !self.started {
            *self = Self {
                started: true,
                extreme: value,
                flag,
                direction: 0,
            };
            return false;
        }
        let moved = value - self.extreme;
        let extends = (self.direction > 0 && moved > 0.0) || (self.direction < 0 && moved < 0.0);
        if extends {
            self.extreme = value;
            self.flag = flag;
            return false;
        }
        if moved.abs() >= step && counts(self.extreme, self.flag, value, flag) {
            self.direction = if moved > 0.0 { 1 } else { -1 };
            self.extreme = value;
            self.flag = flag;
            return true;
        }
        false
    }
}

/// The analysis grid: which source pixels each cell averages.
struct Grid {
    width: u32,
    height: u32,
    /// Each column's source x range, and each row's y range.
    columns: Vec<(u32, u32)>,
    rows: Vec<(u32, u32)>,
    /// Source pixels skipped between those read, for speed.
    stride: u32,
}

impl Grid {
    fn new(width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let grid_width = width.min(GRID_WIDTH);
        let grid_height = ((u64::from(height) * u64::from(grid_width)).div_ceil(u64::from(width)))
            .clamp(1, u64::from(height)) as u32;
        let ranges = |cells: u32, size: u32| -> Vec<(u32, u32)> {
            (0..cells)
                .map(|cell| {
                    let start = (u64::from(cell) * u64::from(size) / u64::from(cells)) as u32;
                    let end = (u64::from(cell + 1) * u64::from(size) / u64::from(cells)) as u32;
                    (start, end.max(start + 1))
                })
                .collect()
        };
        Self {
            width: grid_width,
            height: grid_height,
            columns: ranges(grid_width, width),
            rows: ranges(grid_height, height),
            stride: (width / 960).max(1),
        }
    }

    fn cells(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Each cell's mean linear RGB.
    fn average(&self, rgba: &[u8], width: u32) -> Vec<[f32; 3]> {
        let table = linear_table();
        let mut cells = Vec::with_capacity(self.cells());
        for &(top, bottom) in &self.rows {
            for &(left, right) in &self.columns {
                let mut sum = [0.0_f32; 3];
                let mut count = 0_u32;
                for y in (top..bottom).step_by(self.stride as usize) {
                    let row = y as usize * width as usize * 4;
                    for x in (left..right).step_by(self.stride as usize) {
                        let at = row + x as usize * 4;
                        let Some(pixel) = rgba.get(at..at + 3) else {
                            continue;
                        };
                        for (total, &code) in sum.iter_mut().zip(pixel) {
                            *total += table[usize::from(code)];
                        }
                        count += 1;
                    }
                }
                let count = count.max(1) as f32;
                cells.push(sum.map(|total| total / count));
            }
        }
        cells
    }
}

/// The sliding windows a third of the frame each way, and the frames each
/// counted a transition in.
struct Windows {
    /// Each window's cells `[x0, x1) × [y0, y1)`.
    windows: Vec<(u32, u32, u32, u32)>,
    events: Vec<Vec<u64>>,
}

/// More than three flashes within a second, over `windows`.
struct Violation {
    from: u64,
    to: u64,
    transitions: usize,
    cells: (u32, u32, u32, u32),
}

impl Windows {
    fn new(grid: &Grid) -> Self {
        let size = |cells: u32| (cells / 3).max(1);
        let (width, height) = (size(grid.width), size(grid.height));
        let starts = |cells: u32, size: u32| {
            let step = (size / 4).max(1);
            let mut starts: Vec<u32> = (0..=cells - size).step_by(step as usize).collect();
            if starts.last() != Some(&(cells - size)) {
                starts.push(cells - size);
            }
            starts
        };
        let mut windows = Vec::new();
        for y in starts(grid.height, height) {
            for x in starts(grid.width, width) {
                windows.push((x, y, x + width, y + height));
            }
        }
        let events = vec![Vec::new(); windows.len()];
        Self { windows, events }
    }

    /// Counts a transition at `frame` in each window whose cells made
    /// transitions over more than [`FLASH_AREA`] of it.
    fn record(&mut self, frame: u64, transitions: &[bool], grid: &Grid) {
        if !transitions.contains(&true) {
            return;
        }
        // A summed-area table of the transitions.
        let (width, height) = (grid.width as usize, grid.height as usize);
        let mut table = vec![0_u32; (width + 1) * (height + 1)];
        for y in 0..height {
            let mut row = 0;
            for x in 0..width {
                row += u32::from(transitions[y * width + x]);
                table[(y + 1) * (width + 1) + x + 1] = table[y * (width + 1) + x + 1] + row;
            }
        }
        let at = |x: u32, y: u32| table[y as usize * (width + 1) + x as usize];
        for (window, events) in self.windows.iter().zip(&mut self.events) {
            let &(x0, y0, x1, y1) = window;
            let count = at(x1, y1) + at(x0, y0) - at(x1, y0) - at(x0, y1);
            let area = f64::from((x1 - x0) * (y1 - y0));
            if f64::from(count) > FLASH_AREA * area {
                events.push(frame);
            }
        }
    }

    /// Every span where some window had more than three flashes' worth of
    /// transitions in `fps` frames, joined where they overlap.
    fn violations(&self, fps: u64) -> Vec<Violation> {
        let most = 2 * MAX_FLASHES_PER_SECOND as usize;
        let mut found: Vec<Violation> = Vec::new();
        for (window, events) in self.windows.iter().zip(&self.events) {
            let mut end = 0;
            for start in 0..events.len() {
                end = end.max(start);
                while end + 1 < events.len() && events[end + 1] < events[start] + fps {
                    end += 1;
                }
                let transitions = end - start + 1;
                if transitions > most {
                    found.push(Violation {
                        from: events[start],
                        to: events[end] + 1,
                        transitions,
                        cells: *window,
                    });
                }
            }
        }
        found.sort_by_key(|violation| violation.from);
        let mut joined: Vec<Violation> = Vec::new();
        for violation in found {
            match joined.last_mut() {
                Some(last) if violation.from <= last.to => {
                    last.to = last.to.max(violation.to);
                    last.transitions = last.transitions.max(violation.transitions);
                    let (a, b) = (last.cells, violation.cells);
                    last.cells = (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3));
                }
                _ => joined.push(violation),
            }
        }
        joined
    }
}

fn flash_issue(
    code: QualityCode,
    violation: &Violation,
    scale: (f64, f64),
    frame_rate: Rational,
) -> QualityIssue {
    let span = FrameSpan::new(violation.from, violation.to, frame_rate);
    let (x0, y0, x1, y1) = violation.cells;
    let region = Region {
        x: (f64::from(x0) * scale.0).round() as u32,
        y: (f64::from(y0) * scale.1).round() as u32,
        width: (f64::from(x1 - x0) * scale.0).round() as u32,
        height: (f64::from(y1 - y0) * scale.1).round() as u32,
    };
    let flashes = violation.transitions as f64 / 2.0;
    let what = match code {
        QualityCode::RedFlashing => "saturated red flashes",
        _ => "flashes",
    };
    let mut issue = QualityIssue::new(
        code,
        Measured::Pixels,
        format!(
            "{flashes} {what} within one second, more than {MAX_FLASHES_PER_SECOND}, at {} in the region {region}; a flash that fast can trigger seizures: slow it to 3 a second or less, make it smaller, or lower its contrast",
            describe_spans(&[span])
        ),
    );
    issue.frames = vec![span];
    issue.region = Some(region);
    issue.value = Some(flashes);
    issue.limit = Some(f64::from(MAX_FLASHES_PER_SECOND));
    issue
}

/// The shots between `cuts` in frames `[first, end)` shorter than `fps`
/// frames, runs of them joined; a shot the range cuts off at either side is
/// left out, as are shots inside `flashing` spans, which are reported as
/// flashing.
fn short_shots(
    first: u64,
    end: u64,
    cuts: &[u64],
    ends_composition: bool,
    flashing: &[(u64, u64)],
    fps: u64,
    frame_rate: Rational,
) -> Vec<QualityIssue> {
    let mut bounds = vec![first];
    bounds.extend(cuts.iter().copied().filter(|&cut| first < cut && cut < end));
    bounds.push(end);
    let mut runs: Vec<Vec<(u64, u64)>> = Vec::new();
    let mut previous_short = false;
    for (index, pair) in bounds.windows(2).enumerate() {
        let (from, to) = (pair[0], pair[1]);
        let whole = (index > 0 || first == 0) && (index + 2 < bounds.len() || ends_composition);
        let flashes = flashing
            .iter()
            .any(|&(start, stop)| from < stop && start < to);
        let short = whole && !flashes && to - from < fps;
        if short {
            if previous_short {
                runs.last_mut().expect("a run is open").push((from, to));
            } else {
                runs.push(vec![(from, to)]);
            }
        }
        previous_short = short;
    }
    runs.into_iter()
        .map(|run| {
            let spans: Vec<FrameSpan> = run
                .iter()
                .map(|&(from, to)| FrameSpan::new(from, to, frame_rate))
                .collect();
            let shortest = spans
                .iter()
                .map(|span| span.end - span.start)
                .fold(f64::INFINITY, f64::min);
            let what = if spans.len() == 1 {
                format!("a shot lasts {shortest:.2} s")
            } else {
                format!(
                    "{} shots in a row last under {MIN_SCENE_SECONDS} s (the shortest {shortest:.2} s)",
                    spans.len()
                )
            };
            let mut issue = QualityIssue::new(
                QualityCode::ShortShot,
                Measured::Pixels,
                format!(
                    "between hard cuts {what}, at {}; hold each picture for at least {MIN_SCENE_SECONDS} s",
                    describe_spans(&spans)
                ),
            );
            issue.frames = spans;
            issue.value = Some((shortest * 1000.0).round() / 1000.0);
            issue.limit = Some(MIN_SCENE_SECONDS);
            issue
        })
        .collect()
}

/// Frame 0 as one flat color: nearly every pixel within a few codes of the
/// median.
fn flat_first_frame(rgba: &[u8], frame_rate: Rational) -> Option<QualityIssue> {
    let pixels = rgba.len() / 4;
    if pixels == 0 {
        return None;
    }
    let mut histograms = [[0_u32; 256]; 3];
    let sample = (pixels / 50_000).max(1);
    let mut sampled = 0_u32;
    for pixel in rgba.as_chunks::<4>().0.iter().step_by(sample) {
        for (histogram, &code) in histograms.iter_mut().zip(pixel) {
            histogram[usize::from(code)] += 1;
        }
        sampled += 1;
    }
    let median = histograms.map(|histogram| {
        let mut seen = 0;
        histogram
            .iter()
            .position(|&count| {
                seen += count;
                seen * 2 >= sampled
            })
            .unwrap_or(0) as i32
    });
    let differ = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| {
            pixel[..3]
                .iter()
                .zip(median)
                .any(|(&code, median)| (i32::from(code) - median).abs() > FLAT_TOLERANCE)
        })
        .count();
    if differ as f64 >= FLAT_SHARE * pixels as f64 {
        return None;
    }
    let [red, green, blue] = median.map(|channel| channel as u8);
    let mut issue = QualityIssue::new(
        QualityCode::BlankFirstFrame,
        Measured::Pixels,
        format!(
            "frame 0 is one flat color (#{red:02X}{green:02X}{blue:02X}): it is the thumbnail and the video's first frame, so put the title or the subject on it from the start"
        ),
    );
    issue.frames = vec![FrameSpan::new(0, 1, frame_rate)];
    Some(issue)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 192;
    const H: u32 = 108;

    fn frame(color: [u8; 3]) -> Vec<u8> {
        let mut pixels = Vec::with_capacity((W * H * 4) as usize);
        for _ in 0..W * H {
            pixels.extend_from_slice(&[color[0], color[1], color[2], 255]);
        }
        pixels
    }

    /// Paints `[x0, x1) × [y0, y1)` of `pixels` in `color`.
    fn paint(pixels: &mut [u8], (x0, y0, x1, y1): (u32, u32, u32, u32), color: [u8; 3]) {
        for y in y0..y1 {
            for x in x0..x1 {
                let at = ((y * W + x) * 4) as usize;
                pixels[at..at + 3].copy_from_slice(&color);
            }
        }
    }

    fn run(frames: impl IntoIterator<Item = Vec<u8>>) -> Vec<QualityIssue> {
        let mut lint = PixelLint::new(W, H, Rational::new(30, 1));
        for (index, pixels) in frames.into_iter().enumerate() {
            lint.observe(index as u64 + 1, &pixels);
        }
        lint.finish(false)
    }

    fn codes(issues: &[QualityIssue]) -> Vec<QualityCode> {
        issues.iter().map(|issue| issue.code).collect()
    }

    /// Black and white alternating every `period` frames.
    fn strobe(period: usize, frames: usize) -> Vec<Vec<u8>> {
        (0..frames)
            .map(|index| {
                if (index / period).is_multiple_of(2) {
                    frame([0, 0, 0])
                } else {
                    frame([255, 255, 255])
                }
            })
            .collect()
    }

    #[test]
    fn a_full_screen_strobe_faster_than_three_a_second_flashes() {
        // 30 fps, flipping every 3 frames: 5 flashes a second.
        let issues = run(strobe(3, 60));
        assert_eq!(codes(&issues), [QualityCode::Flashing], "{issues:?}");
        let issue = &issues[0];
        assert!(issue.value.unwrap() > 3.0, "{issue:?}");
        let region = issue.region.unwrap();
        assert_eq!(
            (region.x, region.y, region.width, region.height),
            (0, 0, W, H)
        );
        assert!(
            issue.frames[0].from <= 4 && issue.frames[0].to >= 58,
            "{issue:?}"
        );
    }

    #[test]
    fn three_flashes_a_second_and_slow_fades_are_fine() {
        // Flipping every 5 frames is 3 flashes a second: allowed.
        assert!(!codes(&run(strobe(5, 90))).contains(&QualityCode::Flashing));
        // A fade from black to white and back over two seconds.
        let fade = (0..120).map(|index: u32| {
            let level = if index < 60 {
                index * 4
            } else {
                (120 - index) * 4
            };
            frame([level.min(255) as u8; 3])
        });
        assert!(run(fade).is_empty());
    }

    #[test]
    fn a_small_blinking_dot_is_not_a_flash_but_a_large_one_is() {
        let blink = |size: (u32, u32, u32, u32)| {
            (0..60).map(move |index| {
                let mut pixels = frame([0, 0, 0]);
                if (index / 2) % 2 == 1 {
                    paint(&mut pixels, size, [255, 255, 255]);
                }
                pixels
            })
        };
        assert!(run(blink((10, 10, 16, 16))).is_empty());
        let issues = run(blink((100, 50, 190, 105)));
        assert_eq!(codes(&issues), [QualityCode::Flashing], "{issues:?}");
        let region = issues[0].region.unwrap();
        assert!(region.x > 0 && region.y > 0, "{region:?}");
    }

    #[test]
    fn bright_only_changes_are_not_general_flashes_but_red_ones_count() {
        // Between 0.85 and 1.0 luminance: never below 0.8.
        let light = (0..60).map(|index| frame(if index % 4 < 2 { [240; 3] } else { [255; 3] }));
        assert!(run(light).is_empty());
        // Saturated red against dark red-free grey, every 2 frames.
        let red = (0..60).map(|index| {
            frame(if index % 4 < 2 {
                [255, 0, 0]
            } else {
                [40, 40, 40]
            })
        });
        let issues = run(red);
        assert!(
            codes(&issues).contains(&QualityCode::RedFlashing),
            "{issues:?}"
        );
    }

    #[test]
    fn short_shots_between_cuts_are_found_and_joined() {
        let colors = [[200, 30, 30], [30, 200, 30], [30, 30, 200], [220, 220, 30]];
        // Shots of 45, 10, 12, and 45 frames, frames 1–112.
        let mut frames = Vec::new();
        for (color, length) in colors.iter().zip([45, 10, 12, 45]) {
            frames.extend(std::iter::repeat_n(frame(*color), length));
        }
        let issues = run(frames);
        assert_eq!(codes(&issues), [QualityCode::ShortShot], "{issues:?}");
        let issue = &issues[0];
        assert_eq!(issue.frames.len(), 2);
        assert_eq!((issue.frames[0].from, issue.frames[0].to), (46, 56));
        assert_eq!((issue.frames[1].from, issue.frames[1].to), (56, 68));
        assert!(
            issue.message.contains("2 shots in a row"),
            "{}",
            issue.message
        );
    }

    #[test]
    fn the_first_frame_of_one_color_is_blank() {
        let mut lint = PixelLint::new(W, H, Rational::new(30, 1));
        lint.observe(0, &frame([20, 22, 28]));
        let issues = lint.finish(false);
        assert_eq!(codes(&issues), [QualityCode::BlankFirstFrame]);
        assert!(
            issues[0].message.contains("#14161C"),
            "{}",
            issues[0].message
        );

        let mut titled = frame([20, 22, 28]);
        paint(&mut titled, (60, 40, 130, 60), [255, 255, 255]);
        let mut lint = PixelLint::new(W, H, Rational::new(30, 1));
        lint.observe(0, &titled);
        assert!(lint.finish(false).is_empty());
        // A one-frame video is one shot, shorter than a second.
        let mut lint = PixelLint::new(W, H, Rational::new(30, 1));
        lint.observe(0, &titled);
        assert_eq!(codes(&lint.finish(true)), [QualityCode::ShortShot]);
    }
}
