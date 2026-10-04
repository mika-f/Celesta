use celesta_composition::{Rational, Time, TimeError};

/// Frame-accurate playhead state shared by editor controls and preview evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineClock {
    pub(crate) frame: i64,
    pub(crate) end_frame: i64,
    pub(crate) frame_rate: Rational,
}

impl TimelineClock {
    pub fn new(duration: Time, frame_rate: Rational) -> Result<Self, TimeError> {
        if !duration.is_valid() {
            return Err(TimeError::ZeroTimescale);
        }
        if !frame_rate.is_valid() {
            return Err(TimeError::InvalidFrameRate);
        }
        let numerator = i128::from(duration.value.max(0)) * i128::from(frame_rate.numerator);
        let denominator = i128::from(duration.timescale) * i128::from(frame_rate.denominator);
        let end_frame = numerator
            .checked_add(denominator - 1)
            .and_then(|value| i64::try_from(value / denominator).ok())
            .ok_or(TimeError::Overflow)?;
        Ok(Self {
            frame: 0,
            end_frame,
            frame_rate,
        })
    }

    pub const fn frame(self) -> i64 {
        self.frame
    }

    pub const fn end_frame(self) -> i64 {
        self.end_frame
    }

    pub const fn is_at_end(self) -> bool {
        self.frame >= self.end_frame
    }

    pub fn time(self) -> Result<Time, TimeError> {
        Time::frames(self.frame, self.frame_rate)
    }

    pub fn frame_for_time(self, time: Time) -> Result<i64, TimeError> {
        if !time.is_valid() {
            return Err(TimeError::ZeroTimescale);
        }
        let numerator = i128::from(time.value) * i128::from(self.frame_rate.numerator);
        let denominator = i128::from(time.timescale) * i128::from(self.frame_rate.denominator);
        let rounded = if numerator >= 0 {
            numerator.checked_add(denominator / 2)
        } else {
            numerator.checked_sub(denominator / 2)
        }
        .ok_or(TimeError::Overflow)?;
        i64::try_from(rounded / denominator).map_err(|_| TimeError::Overflow)
    }

    pub fn seek(&mut self, frame: i64) {
        self.frame = frame.clamp(0, self.end_frame);
    }

    pub fn frame_at_fraction(self, fraction: f32) -> i64 {
        let fraction = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        (fraction * self.end_frame as f32).round() as i64
    }

    pub fn seek_fraction(&mut self, fraction: f32) {
        self.frame = self.frame_at_fraction(fraction);
    }

    pub fn step(&mut self, delta: i64) {
        self.seek(self.frame.saturating_add(delta));
    }
}
