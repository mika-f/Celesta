//! An export's progress as the status bar shows it: a bar and a line such
//! as `42%  252/600  31.4 fps  0:11 left`.

use celesta_exporter::ExportProgress;
use std::time::{Duration, Instant};

/// How long rendering runs before its speed and time left show, so the
/// first frames do not give wild numbers.
const SETTLE: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Before the first frame renders.
    Starting,
    /// `frame` of `total` is rendering: events mark frame starts.
    Rendering {
        frame: u64,
        total: u64,
    },
    MixingAudio,
    Muxing,
}

/// The running export's stage and when its rendering began.
#[derive(Debug)]
pub(crate) struct ExportStatus {
    stage: Stage,
    rendering_started: Option<Instant>,
}

/// What the status bar shows for an export.
#[derive(Debug, PartialEq)]
pub(crate) struct StatusLine {
    /// How much is done, 0–100; `None` for a stage with no measure, which
    /// shows as an indeterminate bar.
    pub(crate) percent: Option<f32>,
    pub(crate) text: String,
}

impl ExportStatus {
    /// An export that has not rendered a frame yet.
    pub(crate) fn new() -> Self {
        Self {
            stage: Stage::Starting,
            rendering_started: None,
        }
    }

    /// Takes in `progress` reported at `now`. Events that are not a stage
    /// (warnings, notes, the plan, the written file) change nothing.
    pub(crate) fn update(&mut self, progress: &ExportProgress, now: Instant) {
        self.stage = match *progress {
            ExportProgress::Rendering { frame, total } if total > 0 => {
                self.rendering_started.get_or_insert(now);
                Stage::Rendering { frame, total }
            }
            ExportProgress::MixingAudio => Stage::MixingAudio,
            ExportProgress::Muxing => Stage::Muxing,
            _ => return,
        };
    }

    /// The bar and the line at `now`; `cancelling` replaces the line with
    /// `Cancelling…` and keeps the bar.
    pub(crate) fn line(&self, now: Instant, cancelling: bool) -> StatusLine {
        let (percent, text) = match self.stage {
            Stage::Starting => (None, "Starting export…".to_owned()),
            Stage::Rendering { frame, total } => {
                let done = frame.saturating_sub(1).min(total);
                let percent = (done as f64 * 100.0 / total as f64) as f32;
                let mut text = format!("{}%  {done}/{total}", percent.floor());
                if let Some((fps, left)) = self.speed(done, total, now) {
                    text.push_str(&format!("  {fps:.1} fps  {} left", time_left_label(left)));
                }
                (Some(percent), text)
            }
            Stage::MixingAudio => (None, "Mixing audio…".to_owned()),
            Stage::Muxing => (None, "Muxing…".to_owned()),
        };
        let text = if cancelling {
            "Cancelling…".to_owned()
        } else {
            text
        };
        StatusLine { percent, text }
    }

    /// Frames per second since rendering began and the time the frames
    /// left take at that rate, once rendering has run for [`SETTLE`] and
    /// a frame is done.
    fn speed(&self, done: u64, total: u64, now: Instant) -> Option<(f64, Duration)> {
        let elapsed = now.checked_duration_since(self.rendering_started?)?;
        if elapsed < SETTLE || done == 0 {
            return None;
        }
        let fps = done as f64 / elapsed.as_secs_f64();
        let left = Duration::try_from_secs_f64(total.saturating_sub(done) as f64 / fps).ok()?;
        Some((fps, left))
    }
}

/// `0:11`, or `1:01:05` from one hour.
pub(crate) fn time_left_label(left: Duration) -> String {
    let seconds = left.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendering(frame: u64, total: u64) -> ExportProgress {
        ExportProgress::Rendering { frame, total }
    }

    fn status_after(events: &[(ExportProgress, Instant)]) -> ExportStatus {
        let mut status = ExportStatus::new();
        for (progress, at) in events {
            status.update(progress, *at);
        }
        status
    }

    #[test]
    fn a_new_export_is_starting_with_an_indeterminate_bar() {
        let line = ExportStatus::new().line(Instant::now(), false);
        assert_eq!(line.percent, None);
        assert_eq!(line.text, "Starting export…");
    }

    #[test]
    fn events_before_the_first_frame_keep_it_starting() {
        let now = Instant::now();
        let mut status = ExportStatus::new();
        // The editor's own placeholder before the worker reports.
        status.update(&rendering(0, 0), now);
        status.update(&ExportProgress::Warning("font".to_owned()), now);
        let line = status.line(now, false);
        assert_eq!(line.percent, None);
        assert_eq!(line.text, "Starting export…");
    }

    #[test]
    fn rendering_shows_frames_done_without_speed_for_the_first_second() {
        let start = Instant::now();
        // Events mark frame starts: frame 4 starting means 3 are done.
        let status = status_after(&[
            (rendering(1, 600), start),
            (rendering(4, 600), start + Duration::from_millis(500)),
        ]);
        let line = status.line(start + Duration::from_millis(900), false);
        assert_eq!(line.percent, Some(0.5));
        assert_eq!(line.text, "0%  3/600");
    }

    #[test]
    fn rendering_shows_speed_and_time_left_after_a_second() {
        let start = Instant::now();
        let status = status_after(&[
            (rendering(1, 600), start),
            (rendering(253, 600), start + Duration::from_secs(8)),
        ]);
        // 252 frames in 8 s is 31.5 fps; 348 frames left take 11.05 s.
        let line = status.line(start + Duration::from_secs(8), false);
        assert_eq!(line.percent, Some(42.0));
        assert_eq!(line.text, "42%  252/600  31.5 fps  0:11 left");
    }

    #[test]
    fn speed_waits_for_a_finished_frame() {
        let start = Instant::now();
        let status = status_after(&[(rendering(1, 600), start)]);
        let line = status.line(start + Duration::from_secs(5), false);
        assert_eq!(line.text, "0%  0/600");
    }

    #[test]
    fn a_slow_export_counts_hours() {
        let start = Instant::now();
        let status = status_after(&[(rendering(11, 36_010), start)]);
        // 10 frames in 10 s: 36 000 left take ten hours.
        let line = status.line(start + Duration::from_secs(10), false);
        assert_eq!(line.text, "0%  10/36010  1.0 fps  10:00:00 left");
    }

    #[test]
    fn mixing_and_muxing_show_indeterminate_bars() {
        let start = Instant::now();
        let mut status = status_after(&[(rendering(600, 600), start)]);
        status.update(&ExportProgress::MixingAudio, start);
        let mixing = status.line(start, false);
        assert_eq!(mixing.percent, None);
        assert_eq!(mixing.text, "Mixing audio…");
        status.update(&ExportProgress::Muxing, start);
        let muxing = status.line(start, false);
        assert_eq!(muxing.percent, None);
        assert_eq!(muxing.text, "Muxing…");
    }

    #[test]
    fn warnings_and_plans_leave_the_line_as_it_was() {
        let start = Instant::now();
        let mut status = status_after(&[(rendering(101, 200), start)]);
        let before = status.line(start, false);
        status.update(&ExportProgress::Note("note: terms".to_owned()), start);
        status.update(&ExportProgress::Warning("font".to_owned()), start);
        assert_eq!(status.line(start, false), before);
    }

    #[test]
    fn cancelling_keeps_the_bar_and_says_so() {
        let start = Instant::now();
        let status = status_after(&[(rendering(101, 200), start)]);
        let line = status.line(start, true);
        assert_eq!(line.percent, Some(50.0));
        assert_eq!(line.text, "Cancelling…");
        let starting = ExportStatus::new().line(start, true);
        assert_eq!(starting.percent, None);
        assert_eq!(starting.text, "Cancelling…");
    }

    #[test]
    fn time_left_reads_minutes_or_hours() {
        assert_eq!(time_left_label(Duration::from_secs(0)), "0:00");
        assert_eq!(time_left_label(Duration::from_millis(11_900)), "0:11");
        assert_eq!(time_left_label(Duration::from_secs(59 * 60 + 59)), "59:59");
        assert_eq!(time_left_label(Duration::from_secs(3600)), "1:00:00");
        assert_eq!(time_left_label(Duration::from_secs(3600 + 65)), "1:01:05");
    }
}
