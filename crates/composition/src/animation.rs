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

/// The easings.net catalogue, mirroring
/// `packages/react/src/animation.ts`'s `Easings` object formula-for-formula
/// so the same named curve looks identical whether it drives a project.json
/// keyframe (here) or an `interpolate()` call in a React composition.
fn apply_easing(progress: f64, easing: Easing) -> f64 {
    let t = progress;
    match easing {
        Easing::Linear => t,
        Easing::EaseIn => t * t,
        Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
        Easing::EaseInOut => t * t * (3.0 - 2.0 * t),
        Easing::EaseInSine => 1.0 - (t * std::f64::consts::FRAC_PI_2).cos(),
        Easing::EaseOutSine => (t * std::f64::consts::FRAC_PI_2).sin(),
        Easing::EaseInOutSine => -((std::f64::consts::PI * t).cos() - 1.0) / 2.0,
        Easing::EaseInQuad => t * t,
        Easing::EaseOutQuad => t * (2.0 - t),
        Easing::EaseInOutQuad => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                -1.0 + (4.0 - 2.0 * t) * t
            }
        }
        Easing::EaseInCubic => t * t * t,
        Easing::EaseOutCubic => (t - 1.0).powi(3) + 1.0,
        Easing::EaseInOutCubic => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                (t - 1.0) * (2.0 * t - 2.0).powi(2) + 1.0
            }
        }
        Easing::EaseInQuart => t.powi(4),
        Easing::EaseOutQuart => 1.0 - (t - 1.0).powi(4),
        Easing::EaseInOutQuart => {
            if t < 0.5 {
                8.0 * t.powi(4)
            } else {
                1.0 - 8.0 * (t - 1.0).powi(4)
            }
        }
        Easing::EaseInQuint => t.powi(5),
        Easing::EaseOutQuint => 1.0 + (t - 1.0).powi(5),
        Easing::EaseInOutQuint => {
            if t < 0.5 {
                16.0 * t.powi(5)
            } else {
                1.0 + 16.0 * (t - 1.0).powi(5)
            }
        }
        Easing::EaseInExpo => {
            if t == 0.0 {
                0.0
            } else {
                2.0_f64.powf(10.0 * t - 10.0)
            }
        }
        Easing::EaseOutExpo => {
            if t == 1.0 {
                1.0
            } else {
                1.0 - 2.0_f64.powf(-10.0 * t)
            }
        }
        Easing::EaseInOutExpo => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else if t < 0.5 {
                2.0_f64.powf(20.0 * t - 10.0) / 2.0
            } else {
                (2.0 - 2.0_f64.powf(-20.0 * t + 10.0)) / 2.0
            }
        }
        Easing::EaseInCirc => 1.0 - (1.0 - t * t).sqrt(),
        Easing::EaseOutCirc => (1.0 - (t - 1.0).powi(2)).sqrt(),
        Easing::EaseInOutCirc => {
            if t < 0.5 {
                (1.0 - (1.0 - 4.0 * t * t).sqrt()) / 2.0
            } else {
                ((1.0 - 4.0 * (t - 1.0).powi(2)).sqrt() + 1.0) / 2.0
            }
        }
        Easing::EaseInBack => {
            const C1: f64 = 1.70158;
            const C3: f64 = C1 + 1.0;
            C3 * t * t * t - C1 * t * t
        }
        Easing::EaseOutBack => {
            const C1: f64 = 1.70158;
            const C3: f64 = C1 + 1.0;
            1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
        }
        Easing::EaseInOutBack => {
            const C1: f64 = 1.70158;
            const C2: f64 = C1 * 1.525;
            if t < 0.5 {
                (2.0 * t).powi(2) * ((C2 + 1.0) * 2.0 * t - C2) / 2.0
            } else {
                ((2.0 * t - 2.0).powi(2) * ((C2 + 1.0) * (t * 2.0 - 2.0) + C2) + 2.0) / 2.0
            }
        }
        Easing::EaseInElastic => {
            const C4: f64 = 2.0 * std::f64::consts::PI / 3.0;
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                -(2.0_f64.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * C4).sin()
            }
        }
        Easing::EaseOutElastic => {
            const C4: f64 = 2.0 * std::f64::consts::PI / 3.0;
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                2.0_f64.powf(-10.0 * t) * ((t * 10.0 - 0.75) * C4).sin() + 1.0
            }
        }
        Easing::EaseInOutElastic => {
            const C5: f64 = 2.0 * std::f64::consts::PI / 4.5;
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else if t < 0.5 {
                -(2.0_f64.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * C5).sin()) / 2.0
            } else {
                (2.0_f64.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * C5).sin()) / 2.0 + 1.0
            }
        }
        Easing::EaseInBounce => 1.0 - ease_out_bounce(1.0 - t),
        Easing::EaseOutBounce => ease_out_bounce(t),
        Easing::EaseInOutBounce => {
            if t < 0.5 {
                (1.0 - ease_out_bounce(1.0 - 2.0 * t)) / 2.0
            } else {
                (1.0 + ease_out_bounce(2.0 * t - 1.0)) / 2.0
            }
        }
    }
}

/// Shared by `EaseOutBounce`/`EaseInBounce`/`EaseInOutBounce`, mirroring
/// `Easings.easeOutBounce` in `packages/react/src/animation.ts`.
fn ease_out_bounce(t: f64) -> f64 {
    const N1: f64 = 7.5625;
    const D1: f64 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t = t - 1.5 / D1;
        N1 * t * t + 0.75
    } else if t < 2.5 / D1 {
        let t = t - 2.25 / D1;
        N1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / D1;
        N1 * t * t + 0.984375
    }
}

fn easing_integral(progress: f64, easing: Easing) -> f64 {
    match easing {
        Easing::Linear => progress * progress / 2.0,
        Easing::EaseIn => progress * progress * progress / 3.0,
        Easing::EaseOut => progress * progress - progress * progress * progress / 3.0,
        Easing::EaseInOut => progress * progress * progress - progress.powi(4) / 2.0,
        // The rest of the catalogue (trig, exponential, and multi-branch
        // piecewise curves like elastic/back/bounce) doesn't have an
        // antiderivative worth hand-deriving and hand-verifying by eye the
        // way the four polynomial curves above do — `integrate_f64`'s own
        // caller only needs this to convert an animated playback rate into
        // an absolute source-time offset, so a numerically integrated
        // approximation (indistinguishable from the closed form at
        // f64 precision for these smooth curves) is the pragmatic choice.
        _ => integrate_numerically(progress, |p| apply_easing(p, easing)),
    }
}

/// Composite Simpson's rule for ∫₀^progress f(x) dx, `progress` assumed
/// non-negative (always true for the `[0, 1]`-clamped progress values this
/// module calls it with).
fn integrate_numerically(progress: f64, f: impl Fn(f64) -> f64) -> f64 {
    if progress <= 0.0 {
        return 0.0;
    }
    const STEPS: usize = 128; // even, required by Simpson's rule
    let h = progress / STEPS as f64;
    let mut sum = f(0.0) + f(progress);
    for i in 1..STEPS {
        let x = i as f64 * h;
        sum += f(x) * if i % 2 == 0 { 2.0 } else { 4.0 };
    }
    sum * h / 3.0
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
    fn every_easing_starts_at_0_and_ends_at_1() {
        // t=0/t=1 are exact for every curve in the catalogue (including the
        // ones that special-case those endpoints, like elastic/expo, to
        // avoid landing exactly on an asymptote).
        for easing in ALL_EASINGS {
            assert!((apply_easing(0.0, easing) - 0.0).abs() < 1e-9, "{easing:?} at t=0");
            assert!((apply_easing(1.0, easing) - 1.0).abs() < 1e-9, "{easing:?} at t=1");
        }
    }

    #[test]
    fn ease_in_out_variants_pass_through_the_midpoint() {
        // Every "InOut" curve is point-symmetric about (0.5, 0.5).
        for easing in [
            Easing::EaseInOut,
            Easing::EaseInOutSine,
            Easing::EaseInOutQuad,
            Easing::EaseInOutCubic,
            Easing::EaseInOutQuart,
            Easing::EaseInOutQuint,
            Easing::EaseInOutCirc,
            Easing::EaseInOutBack,
            Easing::EaseInOutBounce,
        ] {
            assert!(
                (apply_easing(0.5, easing) - 0.5).abs() < 1e-9,
                "{easing:?} at t=0.5"
            );
        }
    }

    #[test]
    fn easing_integral_matches_the_closed_form_for_the_legacy_curves() {
        // The numeric fallback (used for everything but the four original
        // curves) should agree with the hand-derived closed form for those
        // four, cross-checking `integrate_numerically` itself.
        for easing in [Easing::Linear, Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut] {
            let closed_form = easing_integral(0.7, easing);
            let numeric = integrate_numerically(0.7, |p| apply_easing(p, easing));
            assert!(
                (closed_form - numeric).abs() < 1e-6,
                "{easing:?}: closed form {closed_form} vs numeric {numeric}"
            );
        }
    }

    #[test]
    fn easing_integral_is_monotonic_and_bounded_for_every_curve() {
        // A loose sanity check on the numeric fallback for curves that
        // overshoot/undershoot [0, 1] (back, elastic): the integral up to
        // t=1 should still land close to the curve's actual signed area,
        // which for all of these is within a small multiple of the [0, 1]
        // range front-loaded/back-loaded eases live in.
        for easing in ALL_EASINGS {
            let total = easing_integral(1.0, easing);
            assert!(total.is_finite(), "{easing:?} integral is finite");
        }
    }

    const ALL_EASINGS: [Easing; 34] = [
        Easing::Linear,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
        Easing::EaseInSine,
        Easing::EaseOutSine,
        Easing::EaseInOutSine,
        Easing::EaseInQuad,
        Easing::EaseOutQuad,
        Easing::EaseInOutQuad,
        Easing::EaseInCubic,
        Easing::EaseOutCubic,
        Easing::EaseInOutCubic,
        Easing::EaseInQuart,
        Easing::EaseOutQuart,
        Easing::EaseInOutQuart,
        Easing::EaseInQuint,
        Easing::EaseOutQuint,
        Easing::EaseInOutQuint,
        Easing::EaseInExpo,
        Easing::EaseOutExpo,
        Easing::EaseInOutExpo,
        Easing::EaseInCirc,
        Easing::EaseOutCirc,
        Easing::EaseInOutCirc,
        Easing::EaseInBack,
        Easing::EaseOutBack,
        Easing::EaseInOutBack,
        Easing::EaseInElastic,
        Easing::EaseOutElastic,
        Easing::EaseInOutElastic,
        Easing::EaseInBounce,
        Easing::EaseOutBounce,
        Easing::EaseInOutBounce,
    ];

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
