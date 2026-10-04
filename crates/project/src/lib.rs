//! Celesta project format v0.

use serde_json::Value;

mod asset;
mod character;
mod document;
mod error;
#[cfg(test)]
mod tests;
mod timeline;

pub use asset::{Asset, AssetKind, AssetSource};
pub use character::{
    Character, LipSyncCue, LipSyncDefinition, MouthShape, PortraitDefinition, SubtitleDefinition,
};
pub use document::{Project, ProjectSettings, ProjectVersion};
pub use error::LoadError;
pub use timeline::{
    SourceRange, TimelineContent, TimelineEffects, TimelineGlow, TimelineItem, TimelineShadow,
    Track, TrackKind,
};

mod validation;

pub use validation::{ValidationError, ValidationErrors};

pub use celesta_composition::{
    Animatable, AnimatablePoint, BlendMode, Easing, GradientStop, Keyframe, KeyframeAnimation,
    Paint, Rational, Stroke, TextAlign, TextStyle, Time, TimeError, TimeRange, Transform,
};

pub type AssetId = String;
pub type CharacterId = String;
pub type TrackId = String;
pub type ItemId = String;
pub type PropertyValue = Value;
