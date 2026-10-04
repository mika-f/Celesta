use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Rational {
    pub numerator: u32,
    pub denominator: u32,
}

impl Rational {
    pub const fn new(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    pub const fn is_valid(self) -> bool {
        self.numerator != 0 && self.denominator != 0
    }
}

/// Exact timeline time, represented as `value / timescale` seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Time {
    pub value: i64,
    pub timescale: u32,
}

impl Time {
    pub const ZERO: Self = Self {
        value: 0,
        timescale: 1,
    };

    pub const fn new(value: i64, timescale: u32) -> Self {
        Self { value, timescale }
    }

    pub const fn samples(value: i64, sample_rate: u32) -> Self {
        Self::new(value, sample_rate)
    }

    pub fn frames(value: i64, frame_rate: Rational) -> Result<Self, TimeError> {
        if !frame_rate.is_valid() {
            return Err(TimeError::InvalidFrameRate);
        }

        let scaled = i128::from(value) * i128::from(frame_rate.denominator);
        let scaled = i64::try_from(scaled).map_err(|_| TimeError::Overflow)?;
        Ok(Self::new(scaled, frame_rate.numerator))
    }

    pub const fn is_valid(self) -> bool {
        self.timescale != 0
    }

    pub fn as_seconds(self) -> Result<f64, TimeError> {
        if !self.is_valid() {
            return Err(TimeError::ZeroTimescale);
        }
        Ok(self.value as f64 / f64::from(self.timescale))
    }

    pub fn checked_add(self, other: Self) -> Result<Self, TimeError> {
        if !self.is_valid() || !other.is_valid() {
            return Err(TimeError::ZeroTimescale);
        }

        let gcd = gcd(self.timescale, other.timescale);
        let left_factor = other.timescale / gcd;
        let right_factor = self.timescale / gcd;
        let timescale = self
            .timescale
            .checked_mul(left_factor)
            .ok_or(TimeError::Overflow)?;
        let value = i128::from(self.value) * i128::from(left_factor)
            + i128::from(other.value) * i128::from(right_factor);
        let value = i64::try_from(value).map_err(|_| TimeError::Overflow)?;

        Ok(Self::new(value, timescale).reduced())
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, TimeError> {
        let value = other.value.checked_neg().ok_or(TimeError::Overflow)?;
        self.checked_add(Self::new(value, other.timescale))
    }

    pub fn cmp_exact(self, other: Self) -> Result<Ordering, TimeError> {
        if !self.is_valid() || !other.is_valid() {
            return Err(TimeError::ZeroTimescale);
        }

        Ok((i128::from(self.value) * i128::from(other.timescale))
            .cmp(&(i128::from(other.value) * i128::from(self.timescale))))
    }

    pub fn reduced(self) -> Self {
        if !self.is_valid() || self.value == 0 {
            return if self.is_valid() { Self::ZERO } else { self };
        }

        let divisor = gcd_u64(self.value.unsigned_abs(), u64::from(self.timescale));
        Self::new(self.value / divisor as i64, self.timescale / divisor as u32)
    }
}

pub(crate) const fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

pub(crate) fn gcd_u64(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeError {
    ZeroTimescale,
    InvalidFrameRate,
    Overflow,
}

impl fmt::Display for TimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroTimescale => formatter.write_str("time timescale must not be zero"),
            Self::InvalidFrameRate => formatter.write_str("frame rate must be positive"),
            Self::Overflow => formatter.write_str("time calculation overflowed"),
        }
    }
}

impl Error for TimeError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TimeRange {
    pub start: Time,
    pub duration: Time,
}

impl TimeRange {
    pub fn end(self) -> Result<Time, TimeError> {
        self.start.checked_add(self.duration)
    }
}
