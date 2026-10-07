//! Renderer-independent primitives shared by projects and evaluated compositions.

mod keyframe;
mod style;
#[cfg(test)]
mod tests;
mod time;
mod transform;

pub use keyframe::{Animatable, Easing, Keyframe, KeyframeAnimation, KeyframeAnimationType};
pub use style::{GradientStop, LineBreak, Paint, Stroke, TextAlign, TextColorRun, TextStyle};
pub use time::{Rational, Time, TimeError, TimeRange};
pub use transform::{AnimatablePoint, Transform};

mod animation;
mod model;

pub use animation::{AnimationError, evaluate_f64, integrate_f64};
pub use model::{
    AssetLocation, AudioClip, AudioGraph, BlendMode, Clip, DEFAULT_MITER_LIMIT, EvaluatedTransform,
    ImageFit, Layer, LayerContent, LayerEffects, LayerGlow, LayerShadow, LineCap, LineJoin,
    MediaTiming, PathCommand, Point, ResolvedAsset, Scene,
};
