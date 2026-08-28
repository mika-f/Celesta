//! Renderer-independent primitives shared by projects and evaluated compositions.

mod animation;
mod model;

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

pub use animation::{AnimationError, evaluate_f64, integrate_f64};
pub use model::{
    AssetLocation, AudioClip, AudioGraph, EvaluatedTransform, Layer, LayerContent, MediaTiming,
    Point, ResolvedAsset, Scene,
};

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

const fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn gcd_u64(mut left: u64, mut right: u64) -> u64 {
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(untagged)]
pub enum Animatable<T> {
    Static(T),
    Keyframes(KeyframeAnimation<T>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct KeyframeAnimation<T> {
    #[serde(rename = "type")]
    pub kind: KeyframeAnimationType,
    pub keyframes: Vec<Keyframe<T>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum KeyframeAnimationType {
    Keyframes,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Keyframe<T> {
    pub time: Time,
    pub value: T,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub easing: Option<Easing>,
}

/// Mirrors the easings.net catalogue (`EaseIn`/`EaseOut`/`EaseInOut` are the
/// pre-existing quadratic shorthand, kept for backward compatibility with
/// project files that already reference them — they compute the same curve
/// as `EaseInQuad`/`EaseOutQuad`/`EaseInOutQuad`). See `animation.rs`'s
/// `apply_easing`/`easing_integral` for the formulas, which mirror
/// `packages/react/src/animation.ts`'s `Easings` object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    EaseInSine,
    EaseOutSine,
    EaseInOutSine,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseInQuart,
    EaseOutQuart,
    EaseInOutQuart,
    EaseInQuint,
    EaseOutQuint,
    EaseInOutQuint,
    EaseInExpo,
    EaseOutExpo,
    EaseInOutExpo,
    EaseInCirc,
    EaseOutCirc,
    EaseInOutCirc,
    EaseInBack,
    EaseOutBack,
    EaseInOutBack,
    EaseInElastic,
    EaseOutElastic,
    EaseInOutElastic,
    EaseInBounce,
    EaseOutBounce,
    EaseInOutBounce,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<AnimatablePoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<AnimatablePoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Animatable<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<AnimatablePoint>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AnimatablePoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<Animatable<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<Animatable<f64>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Paint {
    Solid { color: String },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TextStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Paint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Stroke {
    pub paint: Paint,
    pub width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_times_without_floating_point() {
        let one_second = Time::new(48_000, 48_000);
        let sixty_frames = Time::frames(60, Rational::new(60, 1)).unwrap();
        assert_eq!(one_second.cmp_exact(sixty_frames), Ok(Ordering::Equal));
    }

    #[test]
    fn adds_and_reduces_times() {
        let sum = Time::new(1, 2).checked_add(Time::new(1, 3)).unwrap();
        assert_eq!(sum, Time::new(5, 6));
    }
}
