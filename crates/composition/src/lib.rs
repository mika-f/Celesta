//! Renderer-independent primitives shared by projects and evaluated compositions.

mod keyframe;
mod style;
mod tagged;
#[cfg(test)]
mod tests;
mod time;
mod transform;

pub use keyframe::{Animatable, Easing, Keyframe, KeyframeAnimation, KeyframeAnimationType};
pub use style::{
    FontFallbackMode, FontFamily, FontFeatures, GradientStop, LetterSpacing, LineBreak, Paint,
    Stroke, TextAlign, TextColorRun, TextFontRun, TextStroke, TextStyle,
};
pub use time::{Rational, Time, TimeError, TimeRange};
pub use transform::{AnimatablePoint, Transform};

mod animation;
mod blend;
mod color;
mod duck;
mod instances;
mod model;
mod projection;

pub use animation::{
    AnimationError, KeyframeCursor, evaluate_f64, integrate_f64, keyframe_segment,
};
pub use blend::BlendMode;
pub use color::Color;
pub use duck::{
    AudioDuck, DuckError, DuckGroups, DuckLevel, check_duck_group, duck_curve, duck_curves,
    duck_duration,
};
pub use instances::{F32Column, InstanceShape, U8Column, instance_size};
pub use model::{
    AssetLocation, AudioClip, AudioFade, AudioGraph, Clip, DEFAULT_MITER_LIMIT, EvaluatedTransform,
    GroupMask, ImageAnimation, ImageFit, Layer, LayerContent, LayerEffects, LayerFilter, LayerGlow,
    LayerShader, LayerShadow, LineCap, LineDash, LineJoin, MaskMode, MediaTiming, PathCommand,
    Point, ResolvedAsset, Scene, ShaderParam, ShaderParamType, ShaderSource, is_zero_fade,
    is_zero_pitch, zero_fade, zero_pitch,
};
pub use projection::{
    BackfaceVisibility, IDENTITY3, Matrix3, faces_away, invert3, multiply3, multiply3_flat,
    project, stretch_at,
};
