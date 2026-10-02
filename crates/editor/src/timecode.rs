//! SMPTE-style timecode labels and timeline ruler spacing.
//!
//! Timecode is non-drop-frame and counts at the nominal (rounded) frame rate,
//! as editing applications show it: 29.97 fps counts 30 frames per second.

use celesta_composition::Rational;

/// Frames counted per timecode second.
pub fn nominal_fps(frame_rate: Rational) -> i64 {
    if frame_rate.denominator == 0 {
        return 1;
    }
    let fps = f64::from(frame_rate.numerator) / f64::from(frame_rate.denominator);
    (fps.round() as i64).max(1)
}

/// `HH:MM:SS:FF` for a composition frame.
pub fn format_timecode(frame: i64, frame_rate: Rational) -> String {
    let fps = nominal_fps(frame_rate);
    let sign = if frame < 0 { "-" } else { "" };
    let frame = frame.unsigned_abs();
    let fps = fps.unsigned_abs();
    let seconds = frame / fps;
    format!(
        "{sign}{:02}:{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        frame % fps
    )
}

/// Ruler spacing in frames: a labelled `major` mark and unlabelled `minor`
/// ticks between them. `minor == major` means no subdivision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RulerScale {
    pub major: i64,
    pub minor: i64,
}

/// Picks the finest ruler whose labels stay at least `min_label_px` apart
/// and whose ticks stay at least `min_tick_px` apart.
pub fn ruler_scale(
    pixels_per_frame: f64,
    fps: i64,
    min_label_px: f64,
    min_tick_px: f64,
) -> RulerScale {
    let fps = fps.max(1);
    let mut steps: Vec<i64> = [1, 2, 5, 10]
        .into_iter()
        .filter(|step| *step < fps)
        .collect();
    if fps % 2 == 0 && fps > 10 {
        steps.push(fps / 2);
    }
    steps.extend(
        [
            1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 10800, 21600, 43200,
            86400,
        ]
        .into_iter()
        .map(|seconds| seconds * fps),
    );
    steps.sort_unstable();
    steps.dedup();
    let pixels_per_frame = if pixels_per_frame.is_finite() && pixels_per_frame > 0.0 {
        pixels_per_frame
    } else {
        f64::MIN_POSITIVE
    };
    // Past a day, keep doubling until the labels fit.
    while let Some(&last) = steps.last()
        && (last as f64) * pixels_per_frame < min_label_px
        && let Some(next) = last.checked_mul(2)
    {
        steps.push(next);
    }
    let major = steps
        .iter()
        .copied()
        .find(|step| *step as f64 * pixels_per_frame >= min_label_px)
        .unwrap_or(*steps.last().expect("ruler steps are never empty"));
    let minor = steps
        .iter()
        .copied()
        .take_while(|step| *step < major)
        .find(|step| major % step == 0 && *step as f64 * pixels_per_frame >= min_tick_px)
        .unwrap_or(major);
    RulerScale { major, minor }
}

#[cfg(test)]
mod tests {
    use super::{RulerScale, format_timecode, nominal_fps, ruler_scale};
    use celesta_composition::Rational;

    fn rate(numerator: u32, denominator: u32) -> Rational {
        Rational {
            numerator,
            denominator,
        }
    }

    #[test]
    fn timecode_counts_frames_within_each_second() {
        assert_eq!(format_timecode(0, rate(30, 1)), "00:00:00:00");
        assert_eq!(format_timecode(61, rate(30, 1)), "00:00:02:01");
        assert_eq!(
            format_timecode(60 * 60 * 61 + 5, rate(60, 1)),
            "01:01:00:05"
        );
        assert_eq!(format_timecode(-1, rate(24, 1)), "-00:00:00:01");
    }

    #[test]
    fn fractional_rates_count_at_the_nominal_rate() {
        assert_eq!(nominal_fps(rate(30_000, 1001)), 30);
        assert_eq!(nominal_fps(rate(24_000, 1001)), 24);
        assert_eq!(format_timecode(30, rate(30_000, 1001)), "00:00:01:00");
    }

    #[test]
    fn ruler_labels_whole_seconds_when_zoomed_out() {
        // 10 s at 60 fps across 1200 px: 2 px per frame.
        let scale = ruler_scale(2.0, 60, 100.0, 8.0);
        assert_eq!(
            scale,
            RulerScale {
                major: 60,
                minor: 5
            }
        );
        let scale = ruler_scale(2.0, 60, 100.0, 24.0);
        assert_eq!(
            scale,
            RulerScale {
                major: 60,
                minor: 30
            }
        );
    }

    #[test]
    fn ruler_reaches_single_frames_when_zoomed_in() {
        let scale = ruler_scale(40.0, 30, 100.0, 8.0);
        assert_eq!(scale, RulerScale { major: 5, minor: 1 });
    }

    #[test]
    fn ruler_labels_long_compositions_without_overlap() {
        // 24 hours at 30 fps across 1000 px: hourly labels would be 42 px apart.
        let pixels_per_frame = 1000.0 / (24.0 * 3600.0 * 30.0);
        let scale = ruler_scale(pixels_per_frame, 30, 100.0, 8.0);
        assert_eq!(scale.major, 3 * 3600 * 30);
        assert!(scale.major as f64 * pixels_per_frame >= 100.0);
        // Degenerate scales stop at the largest representable step.
        assert!(ruler_scale(1e-30, 30, 100.0, 8.0).major > 0);
    }
}
