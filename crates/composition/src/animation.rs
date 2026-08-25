use std::error::Error;
use std::fmt;

use crate::{Animatable, Easing, Time, TimeError};

/// Evaluates a numeric property at item-local `time`.
///
/// Easing belongs to the destination keyframe for the segment ending there.
/// Values before or after the animation are clamped to its first or last value.
pub fn evaluate_f64(value: &Animatable<f64>, time: Time) -> Result<f64, AnimationError> {
    let Animatable::Keyframes(animation) = value else {
        let Animatable::Static(value) = value else {
            unreachable!()
        };
        return Ok(*value);
    };

    let first = animation
        .keyframes
        .first()
        .ok_or(AnimationError::EmptyKeyframes)?;
    if time.cmp_exact(first.time)?.is_le() {
        return Ok(first.value);
    }

    for pair in animation.keyframes.windows(2) {
        let [from, to] = pair else { unreachable!() };
        if time.cmp_exact(to.time)?.is_le() {
            let elapsed = time.checked_sub(from.time)?.as_seconds()?;
            let duration = to.time.checked_sub(from.time)?.as_seconds()?;
            if duration <= 0.0 {
                return Ok(to.value);
            }
            let progress = (elapsed / duration).clamp(0.0, 1.0);
            let eased = apply_easing(progress, to.easing.unwrap_or(Easing::Linear));
            return Ok(from.value + (to.value - from.value) * eased);
        }
    }

    Ok(animation
        .keyframes
        .last()
        .expect("first keyframe exists")
        .value)
}

/// Integrates a numeric animation from item-local zero through `time`.
///
/// This is used to turn animated playback rate into an absolute source time.
pub fn integrate_f64(value: &Animatable<f64>, time: Time) -> Result<f64, AnimationError> {
    let end = time.as_seconds()?.max(0.0);
    let Animatable::Keyframes(animation) = value else {
        let Animatable::Static(value) = value else {
            unreachable!()
        };
        return Ok(*value * end);
    };
    let first = animation
        .keyframes
        .first()
        .ok_or(AnimationError::EmptyKeyframes)?;
    let first_time = first.time.as_seconds()?.max(0.0);
    let mut integral = first.value * end.min(first_time);
    if end <= first_time {
        return Ok(integral);
    }

    for pair in animation.keyframes.windows(2) {
        let [from, to] = pair else { unreachable!() };
        let start = from.time.as_seconds()?.max(0.0);
        let finish = to.time.as_seconds()?.max(start);
        let duration = finish - start;
        if duration == 0.0 {
            continue;
        }
        let progress = ((end - start) / duration).clamp(0.0, 1.0);
        let easing = to.easing.unwrap_or(Easing::Linear);
        integral += duration
            * (from.value * progress + (to.value - from.value) * easing_integral(progress, easing));
        if end <= finish {
            return Ok(integral);
        }
    }

    let last = animation.keyframes.last().expect("first keyframe exists");
    let last_time = last.time.as_seconds()?.max(0.0);
    integral += last.value * (end - last_time).max(0.0);
    Ok(integral)
}

fn apply_easing(progress: f64, easing: Easing) -> f64 {
    match easing {
        Easing::Linear => progress,
        Easing::EaseIn => progress * progress,
        Easing::EaseOut => 1.0 - (1.0 - progress) * (1.0 - progress),
        Easing::EaseInOut => progress * progress * (3.0 - 2.0 * progress),
    }
}

fn easing_integral(progress: f64, easing: Easing) -> f64 {
    match easing {
        Easing::Linear => progress * progress / 2.0,
        Easing::EaseIn => progress * progress * progress / 3.0,
        Easing::EaseOut => progress * progress - progress * progress * progress / 3.0,
        Easing::EaseInOut => progress * progress * progress - progress.powi(4) / 2.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationError {
    EmptyKeyframes,
    Time(TimeError),
}

impl fmt::Display for AnimationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyKeyframes => formatter.write_str("animation has no keyframes"),
            Self::Time(error) => write!(formatter, "could not evaluate animation time: {error}"),
        }
    }
}

impl Error for AnimationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::EmptyKeyframes => None,
            Self::Time(error) => Some(error),
        }
    }
}

impl From<TimeError> for AnimationError {
    fn from(error: TimeError) -> Self {
        Self::Time(error)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Easing, Keyframe, KeyframeAnimation, KeyframeAnimationType};

    use super::*;

    #[test]
    fn interpolates_using_item_local_time() {
        let animation = Animatable::Keyframes(KeyframeAnimation {
            kind: KeyframeAnimationType::Keyframes,
            keyframes: vec![
                Keyframe {
                    time: Time::ZERO,
                    value: 0.0,
                    easing: None,
                },
                Keyframe {
                    time: Time::new(1, 1),
                    value: 100.0,
                    easing: Some(Easing::Linear),
                },
            ],
        });

        assert_eq!(evaluate_f64(&animation, Time::new(1, 2)).unwrap(), 50.0);
    }

    #[test]
    fn clamps_outside_the_keyframe_range() {
        let animation = Animatable::Keyframes(KeyframeAnimation {
            kind: KeyframeAnimationType::Keyframes,
            keyframes: vec![Keyframe {
                time: Time::new(1, 1),
                value: 42.0,
                easing: None,
            }],
        });

        assert_eq!(evaluate_f64(&animation, Time::ZERO).unwrap(), 42.0);
        assert_eq!(evaluate_f64(&animation, Time::new(2, 1)).unwrap(), 42.0);
    }

    #[test]
    fn integrates_playback_rate_across_keyframes() {
        let animation = Animatable::Keyframes(KeyframeAnimation {
            kind: KeyframeAnimationType::Keyframes,
            keyframes: vec![
                Keyframe {
                    time: Time::ZERO,
                    value: 1.0,
                    easing: None,
                },
                Keyframe {
                    time: Time::new(2, 1),
                    value: 3.0,
                    easing: Some(Easing::Linear),
                },
            ],
        });

        assert_eq!(integrate_f64(&animation, Time::new(2, 1)).unwrap(), 4.0);
        assert_eq!(integrate_f64(&animation, Time::new(3, 1)).unwrap(), 7.0);
    }
}
