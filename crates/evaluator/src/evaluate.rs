use crate::error::EvaluationError;
use celesta_composition::{
    Animatable, AnimationError, EvaluatedTransform, Keyframe, KeyframeAnimation, LayerEffects,
    LayerGlow, LayerShadow, Point, Time, Transform, evaluate_f64, integrate_f64,
};
use celesta_project::TimelineItem;

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
                Ok(LayerShadow {
                    color: evaluate_effect_color(&shadow.color, time)?,
                    blur: evaluate_f64(&shadow.blur, time)?.max(0.0),
                    offset_x: evaluate_f64(&shadow.offset_x, time)?,
                    offset_y: evaluate_f64(&shadow.offset_y, time)?,
                })
            })
            .transpose()?,
        glow: effects
            .glow
            .as_ref()
            .map(|glow| -> Result<_, EvaluationError> {
                Ok(LayerGlow {
                    color: evaluate_effect_color(&glow.color, time)?,
                    blur: evaluate_f64(&glow.blur, time)?.max(0.0),
                })
            })
            .transpose()?,
    })
}

pub(crate) fn evaluate_effect_color(
    value: &Animatable<String>,
    time: Time,
) -> Result<String, EvaluationError> {
    match value {
        Animatable::Static(color) => Ok(color.clone()),
        Animatable::Keyframes(animation) => {
            let colors: Vec<[u8; 4]> = animation
                .keyframes
                .iter()
                .map(|keyframe| parse_effect_color(&keyframe.value))
                .collect::<Result<_, _>>()?;
            let mut channels = [0_u8; 4];
            for (index, channel) in channels.iter_mut().enumerate() {
                let values = Animatable::Keyframes(KeyframeAnimation {
                    kind: animation.kind,
                    keyframes: animation
                        .keyframes
                        .iter()
                        .zip(&colors)
                        .map(|(keyframe, color)| Keyframe {
                            time: keyframe.time,
                            value: f64::from(color[index]),
                            easing: keyframe.easing,
                        })
                        .collect(),
                });
                *channel = evaluate_f64(&values, time)?.round().clamp(0.0, 255.0) as u8;
            }
            Ok(format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                channels[0], channels[1], channels[2], channels[3]
            ))
        }
    }
}

pub(crate) fn parse_effect_color(color: &str) -> Result<[u8; 4], EvaluationError> {
    let hex = color
        .strip_prefix('#')
        .ok_or_else(|| EvaluationError::InvalidEffectColor(color.to_owned()))?;
    let byte = |offset| {
        u8::from_str_radix(&hex[offset..offset + 2], 16)
            .map_err(|_| EvaluationError::InvalidEffectColor(color.to_owned()))
    };
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|digit| digit.is_ascii_hexdigit()) {
        return Err(EvaluationError::InvalidEffectColor(color.to_owned()));
    }
    Ok([
        byte(0)?,
        byte(2)?,
        byte(4)?,
        if hex.len() == 8 { byte(6)? } else { 255 },
    ])
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
