//! The master audio meter: per-channel peak levels of the mixed preview
//! audio, read at the playhead.
//!
//! Levels come from the same buffer the preview plays, so the meter shows
//! what is heard (before the monitor volume, like a mixer's bus meter) and
//! reads the same while playing, scrubbing, or paused.

use celesta_media::AudioBuffer;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, Bounds, Hsla, IntoElement, ParentElement as _, Pixels, Styled as _, canvas, div, fill,
    point, px, relative, size,
};

use celesta_editor_theme as theme;

/// Bottom of the meter scale.
pub const FLOOR_DB: f32 = -60.0;

/// Peak buckets per second of audio.
const BUCKETS_PER_SECOND: f64 = 100.0;
/// The bar shows the loudest peak over this many buckets before the playhead.
const PEAK_WINDOW: usize = 5;
/// The hold line shows the loudest peak over this many buckets.
const HOLD_WINDOW: usize = 150;

/// Scale marks drawn beside the bars, in dBFS.
const SCALE_MARKS: [f32; 8] = [0.0, -6.0, -12.0, -18.0, -24.0, -36.0, -48.0, -60.0];

/// Above this the bar turns warm, above [`HOT_DB`] it turns hot.
const WARM_DB: f32 = -12.0;
const HOT_DB: f32 = -3.0;

/// Left/right peak envelope of a mixed buffer. Mono plays on both sides;
/// channels past the second are not metered.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MasterLevels {
    /// Buckets per second as actually cut: a whole number of sample frames
    /// each, so not exactly [`BUCKETS_PER_SECOND`] at every sample rate.
    buckets_per_second: f64,
    peaks: Vec<[f32; 2]>,
}

/// Linear peak amplitudes (0..) at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeterReading {
    pub peak: [f32; 2],
    pub hold: [f32; 2],
}

impl MasterLevels {
    pub fn from_buffer(buffer: &AudioBuffer) -> Self {
        let channels = usize::from(buffer.channels);
        if channels == 0 || buffer.sample_rate == 0 {
            return Self::default();
        }
        let frames_per_bucket =
            ((f64::from(buffer.sample_rate) / BUCKETS_PER_SECOND).round() as usize).max(1);
        let peaks = buffer
            .samples
            .chunks(channels * frames_per_bucket)
            .map(|bucket| {
                bucket
                    .chunks_exact(channels)
                    .fold([0.0_f32; 2], |[left, right], frame| {
                        let l = frame[0].abs();
                        let r = frame.get(1).map_or(l, |sample| sample.abs());
                        [left.max(l), right.max(r)]
                    })
            })
            .collect();
        Self {
            buckets_per_second: f64::from(buffer.sample_rate) / frames_per_bucket as f64,
            peaks,
        }
    }

    pub fn reading(&self, seconds: f64) -> MeterReading {
        if self.peaks.is_empty() || !seconds.is_finite() || seconds < 0.0 {
            return MeterReading::default();
        }
        let now = (seconds * self.buckets_per_second) as usize;
        if now >= self.peaks.len() {
            return MeterReading::default();
        }
        let loudest = |window: usize| {
            self.peaks[now.saturating_sub(window - 1)..=now]
                .iter()
                .fold([0.0_f32; 2], |[left, right], [l, r]| {
                    [left.max(*l), right.max(*r)]
                })
        };
        MeterReading {
            peak: loudest(PEAK_WINDOW),
            hold: loudest(HOLD_WINDOW),
        }
    }
}

/// dBFS for a linear amplitude, floored at [`FLOOR_DB`].
pub fn amplitude_db(amplitude: f32) -> f32 {
    if amplitude <= 0.0 || !amplitude.is_finite() {
        return FLOOR_DB;
    }
    (20.0 * amplitude.log10()).max(FLOOR_DB)
}

/// Height fraction (0 at the floor, 1 at 0 dBFS) of a dB value on the scale.
pub fn scale_position(db: f32) -> f32 {
    ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
}

/// Peak readout under a bar: `-12.3`, `−∞` below the floor, `+1.5` when clipping.
pub fn format_peak(amplitude: f32) -> String {
    let db = amplitude_db(amplitude);
    if db <= FLOOR_DB {
        "−∞".to_owned()
    } else if db > 0.05 {
        format!("+{db:.1}")
    } else {
        format!("{db:.1}")
    }
}

/// Two vertical bars with a dBFS scale. `reading` is `None` while there is no
/// mixed audio to meter.
pub fn audio_meter(reading: Option<MeterReading>, cx: &App) -> impl IntoElement {
    let reading = reading.unwrap_or_default();
    let track = cx.theme().background;
    let hold_color = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let mono = cx.theme().mono_font_family.clone();
    let bars = canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            paint_bars(bounds, reading, track, hold_color, window);
        },
    )
    .size_full();
    let scale = div()
        .relative()
        .flex_none()
        .w(px(22.0))
        .h_full()
        .children(SCALE_MARKS.iter().map(|db| {
            div()
                .absolute()
                .right_0()
                .top(relative(1.0 - scale_position(*db)))
                .mt(px(-6.0))
                .text_size(px(9.0))
                .line_height(px(12.0))
                .text_color(muted)
                .child(format!("{}", db.abs() as i32))
        }));
    div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_1()
        .min_h_0()
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .gap_1()
                // Keep the 0 and -60 labels inside the column.
                .py(px(6.0))
                .child(scale)
                .child(div().flex_1().h_full().child(bars)),
        )
        .child(
            div()
                .flex()
                .gap_1()
                .pl(px(26.0))
                .font_family(mono)
                .text_size(px(9.0))
                .children(reading.hold.iter().map(|hold| {
                    div()
                        .flex_1()
                        .text_center()
                        .text_color(if amplitude_db(*hold) > HOT_DB {
                            theme::meter(1.0)
                        } else {
                            muted
                        })
                        .child(format_peak(*hold))
                })),
        )
}

fn paint_bars(
    bounds: Bounds<Pixels>,
    reading: MeterReading,
    track: Hsla,
    hold_color: Hsla,
    window: &mut gpui_kit::Window,
) {
    let gap = px(3.0);
    let width = (bounds.size.width - gap) / 2.0;
    let height = bounds.size.height;
    let zones = [
        (FLOOR_DB, WARM_DB, theme::meter(0.0)),
        (WARM_DB, HOT_DB, theme::meter(0.7)),
        (HOT_DB, 0.0, theme::meter(1.0)),
    ];
    for (channel, (peak, hold)) in reading.peak.iter().zip(reading.hold).enumerate() {
        let left = bounds.origin.x + (width + gap) * channel as f32;
        window.paint_quad(fill(
            Bounds::new(point(left, bounds.origin.y), size(width, height)),
            track,
        ));
        let level = scale_position(amplitude_db(*peak));
        for (from, to, color) in zones {
            let bottom = scale_position(from);
            let top = scale_position(to).min(level);
            if top <= bottom {
                continue;
            }
            let y = bounds.origin.y + height * (1.0 - top);
            window.paint_quad(fill(
                Bounds::new(point(left, y), size(width, height * (top - bottom))),
                color,
            ));
        }
        if hold > 0.0 {
            let y = bounds.origin.y + height * (1.0 - scale_position(amplitude_db(hold)));
            window.paint_quad(fill(
                Bounds::new(point(left, y), size(width, px(1.0))),
                hold_color.opacity(0.8),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FLOOR_DB, MasterLevels, amplitude_db, format_peak, scale_position};
    use celesta_media::AudioBuffer;

    fn stereo(sample_rate: u32, frames: &[[f32; 2]]) -> AudioBuffer {
        AudioBuffer {
            sample_rate,
            channels: 2,
            samples: frames.iter().flatten().copied().collect(),
        }
    }

    #[test]
    fn readings_follow_each_channel_at_the_playhead() {
        // 100 Hz sample rate: one frame per bucket.
        let mut frames = vec![[0.0, 0.0]; 300];
        frames[100] = [0.5, -0.25];
        let levels = MasterLevels::from_buffer(&stereo(100, &frames));
        let at_peak = levels.reading(1.0);
        assert_eq!(at_peak.peak, [0.5, 0.25]);
        // The bar falls back after the peak window; the hold line stays.
        let later = levels.reading(1.5);
        assert_eq!(later.peak, [0.0, 0.0]);
        assert_eq!(later.hold, [0.5, 0.25]);
        // Before the peak nothing shows.
        assert_eq!(levels.reading(0.5).hold, [0.0, 0.0]);
        // Past the end of the mix the meter is silent.
        assert_eq!(levels.reading(10.0).hold, [0.0, 0.0]);
    }

    #[test]
    fn readings_stay_in_step_when_buckets_do_not_divide_the_rate() {
        // 22,050 Hz cuts 221-frame buckets, slightly over 10 ms each.
        let mut frames = vec![[0.0, 0.0]; 22_050 * 60];
        frames[22_050 * 59] = [1.0, 1.0];
        let levels = MasterLevels::from_buffer(&stereo(22_050, &frames));
        assert_eq!(levels.reading(59.0).peak, [1.0, 1.0]);
        assert_eq!(levels.reading(59.9).hold, [1.0, 1.0]);
    }

    #[test]
    fn mono_meters_on_both_sides() {
        let buffer = AudioBuffer {
            sample_rate: 100,
            channels: 1,
            samples: vec![0.0, -0.8, 0.0],
        };
        assert_eq!(
            MasterLevels::from_buffer(&buffer).reading(0.01).peak,
            [0.8, 0.8]
        );
    }

    #[test]
    fn decibels_map_onto_the_scale() {
        assert_eq!(amplitude_db(0.0), FLOOR_DB);
        assert!((amplitude_db(0.5) + 6.02).abs() < 0.01);
        assert_eq!(scale_position(0.0), 1.0);
        assert_eq!(scale_position(FLOOR_DB), 0.0);
        assert_eq!(scale_position(-30.0), 0.5);
        assert_eq!(format_peak(0.0), "−∞");
        assert_eq!(format_peak(0.5), "-6.0");
        assert_eq!(format_peak(1.2), "+1.6");
    }
}
