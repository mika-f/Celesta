use crate::error::EvaluationError;
use celesta_composition::{
    Animatable, AnimationError, Color, EvaluatedTransform, LayerEffects, LayerFilter, LayerGlow,
    LayerShadow, Point, Time, Transform, evaluate_f64, integrate_f64, keyframe_segment,
};
use celesta_project::{TimelineEffects, TimelineItem};

pub(crate) fn evaluate_optional(
    value: &Option<Animatable<f64>>,
    time: Time,
    default: f64,
) -> Result<f64, AnimationError> {
    value
        .as_ref()
        .map_or(Ok(default), |value| evaluate_f64(value, time))
}

pub(crate) fn evaluate_effects(
    item: &TimelineItem,
    time: Time,
) -> Result<LayerEffects, EvaluationError> {
    let Some(effects) = &item.effects else {
        return Ok(LayerEffects::default());
    };
    Ok(LayerEffects {
        blur: evaluate_optional(&effects.blur, time, 0.0)?.max(0.0),
        shadow: effects
            .shadow
            .as_ref()
            .map(|shadow| -> Result<_, EvaluationError> {
                Ok(Box::new(LayerShadow {
                    color: evaluate_effect_color(&shadow.color, time)?,
                    blur: evaluate_f64(&shadow.blur, time)?.max(0.0),
                    offset_x: evaluate_f64(&shadow.offset_x, time)?,
                    offset_y: evaluate_f64(&shadow.offset_y, time)?,
                }))
            })
            .transpose()?,
        glow: effects
            .glow
            .as_ref()
            .map(|glow| -> Result<_, EvaluationError> {
                Ok(Box::new(LayerGlow {
                    color: evaluate_effect_color(&glow.color, time)?,
                    blur: evaluate_f64(&glow.blur, time)?.max(0.0),
                }))
            })
            .transpose()?,
        filters: evaluate_color_filters(effects, time)?,
    })
}

/// The item's color filters at `time`, in the order the React props of the
/// same names apply, leaving out those that change nothing. An easing that
/// overshoots is held to the range validation allows.
fn evaluate_color_filters(
    effects: &TimelineEffects,
    time: Time,
) -> Result<Vec<LayerFilter>, EvaluationError> {
    let at = |value: &Option<Animatable<f64>>| -> Result<Option<f64>, EvaluationError> {
        Ok(value
            .as_ref()
            .map(|value| evaluate_f64(value, time))
            .transpose()?)
    };
    let unit = |value: f64| value.clamp(0.0, 1.0);
    let mut filters = Vec::new();
    if let Some(amount) = at(&effects.brightness)? {
        filters.push(LayerFilter::Brightness {
            amount: amount.max(0.0),
        });
    }
    if let Some(amount) = at(&effects.contrast)? {
        filters.push(LayerFilter::Contrast {
            amount: amount.max(0.0),
        });
    }
    if let Some(amount) = at(&effects.saturation)? {
        filters.push(LayerFilter::Saturation {
            amount: amount.max(0.0),
        });
    }
    if let Some(degrees) = at(&effects.hue)? {
        filters.push(LayerFilter::Hue { degrees });
    }
    if let Some(amount) = at(&effects.grayscale)? {
        filters.push(LayerFilter::Grayscale {
            amount: unit(amount),
        });
    }
    if let Some(amount) = at(&effects.invert)? {
        filters.push(LayerFilter::Invert {
            amount: unit(amount),
        });
    }
    if let Some(tint) = &effects.tint {
        filters.push(LayerFilter::Tint {
            color: evaluate_effect_color(&tint.color, time)?,
            amount: unit(at(&tint.amount)?.unwrap_or(1.0)),
        });
    }
    if let Some(vignette) = &effects.vignette {
        filters.push(LayerFilter::Vignette {
            amount: unit(evaluate_f64(&vignette.amount, time)?),
            radius: at(&vignette.radius)?.unwrap_or(0.5).max(0.0),
            softness: at(&vignette.softness)?.unwrap_or(0.5).max(0.0),
        });
    }
    filters.retain(|filter| !filter.is_identity());
    Ok(filters)
}

/// An effect color at `time`: a static one as written, an animated one
/// interpolated per channel between the two keyframes around `time`.
pub(crate) fn evaluate_effect_color(
    value: &Animatable<String>,
    time: Time,
) -> Result<String, EvaluationError> {
    match value {
        Animatable::Static(color) => Ok(color.clone()),
        Animatable::Keyframes(animation) => {
            let (from, to, eased) = keyframe_segment(animation, time)?;
            let parse = |color: &str| {
                Color::from_hex(color)
                    .ok_or_else(|| EvaluationError::InvalidEffectColor(color.to_owned()))
            };
            Ok(parse(from)?.lerp(parse(to)?, eased).to_string())
        }
    }
}

pub(crate) fn integrate_optional(
    value: &Option<Animatable<f64>>,
    time: Time,
    default: f64,
) -> Result<f64, AnimationError> {
    value.as_ref().map_or_else(
        || Ok(default * time.as_seconds()?),
        |value| integrate_f64(value, time),
    )
}

pub(crate) fn evaluate_transform(
    transform: Option<&Transform>,
    time: Time,
) -> Result<EvaluatedTransform, AnimationError> {
    let Some(transform) = transform else {
        return Ok(EvaluatedTransform::default());
    };
    let defaults = EvaluatedTransform::default();

    Ok(EvaluatedTransform {
        position: evaluate_point(transform.position.as_ref(), time, defaults.position)?,
        scale: evaluate_point(transform.scale.as_ref(), time, defaults.scale)?,
        rotation: evaluate_optional(&transform.rotation, time, defaults.rotation)?,
        anchor: evaluate_point(transform.anchor.as_ref(), time, defaults.anchor)?,
        origin: evaluate_point(transform.origin.as_ref(), time, defaults.origin)?,
        skew: evaluate_point(transform.skew.as_ref(), time, defaults.skew)?,
        rotation_x: evaluate_optional(&transform.rotation_x, time, defaults.rotation_x)?,
        rotation_y: evaluate_optional(&transform.rotation_y, time, defaults.rotation_y)?,
        perspective: transform
            .perspective
            .as_ref()
            .map(|perspective| evaluate_f64(perspective, time))
            .transpose()?,
        backface_visibility: transform.backface_visibility.unwrap_or_default(),
    })
}

pub(crate) fn evaluate_point(
    point: Option<&celesta_composition::AnimatablePoint>,
    time: Time,
    default: Point,
) -> Result<Point, AnimationError> {
    let Some(point) = point else {
        return Ok(default);
    };
    Ok(Point {
        x: evaluate_optional(&point.x, time, default.x)?,
        y: evaluate_optional(&point.y, time, default.y)?,
    })
}
