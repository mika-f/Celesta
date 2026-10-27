use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::blend::BlendMode;
use crate::instances::{F32Column, InstanceShape, U8Column};
use crate::tagged::TypeTagged;
use crate::{
    Animatable, BackfaceVisibility, Paint, Rational, Stroke, TextStyle, Time, TimeError, TimeRange,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub time: Time,
    /// Project-provided fonts available to text layout, in addition to system fonts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<ResolvedAsset>,
    /// The custom shaders the scene's layers use, each once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shaders: Vec<ShaderSource>,
    /// Bottom-to-top painter's order.
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub id: String,
    pub transform: EvaluatedTransform,
    /// 1 when left out, as the React bridge leaves it out of compact frames.
    /// A `Group` below 1 fades as one picture; see `isolates_group`.
    #[serde(default = "full_opacity")]
    pub opacity: f64,
    /// How the layer's pixels combine with what is already drawn beneath
    /// it. A `Group` with a mode other than `Normal` blends as one layer;
    /// see `isolates_group`.
    #[serde(default, skip_serializing_if = "BlendMode::is_normal")]
    pub blend_mode: BlendMode,
    #[serde(default, skip_serializing_if = "LayerEffects::is_empty")]
    pub effects: LayerEffects,
    pub content: LayerContent,
}

impl Layer {
    /// Whether this `Group` is isolated: its children composite onto a
    /// transparent layer first, and that result blends with the backdrop
    /// as one layer through the group's opacity and blend mode. A group
    /// with a mask, a mode other than `Normal`, or an opacity below 1 is,
    /// so its overlapping children fade together, as under CSS `opacity`,
    /// instead of each letting the ones beneath show through. A group whose
    /// only child is one shape (not a `Group` or `Instances`) blended
    /// `Normal` is not: that child just takes the group's opacity as well.
    /// Any other layer is never isolated as a group.
    pub fn isolates_group(&self) -> bool {
        let LayerContent::Group { layers, mask, .. } = &self.content else {
            return false;
        };
        if mask.is_some() || !self.blend_mode.is_normal() {
            return true;
        }
        if self.opacity >= 1.0 {
            return false;
        }
        match layers.as_slice() {
            [] => false,
            [child] => {
                !child.blend_mode.is_normal()
                    || matches!(
                        child.content,
                        LayerContent::Group { .. } | LayerContent::Instances { .. }
                    )
            }
            _ => true,
        }
    }
}

/// Effects applied to the composited pixels of one visual layer or group.
/// Radii and offsets are measured in output pixels after transforms.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LayerEffects {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub blur: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Box<LayerShadow>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glow: Option<Box<LayerGlow>>,
    /// Custom shaders and built-in color filters, applied in order before
    /// `blur`, `shadow`, and `glow`: each filters the result of the one
    /// before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<LayerFilter>,
}

impl LayerEffects {
    pub fn is_empty(&self) -> bool {
        self.blur <= 0.0 && self.shadow.is_none() && self.glow.is_none() && self.filters.is_empty()
    }
}

/// One filter in a layer's `filters`. The built-in color filters work on
/// the straight (unpremultiplied) sRGB-encoded color, keep alpha, and clamp
/// their result to 0–1, as CSS `filter` functions do. The stylize effects
/// and the chroma key measure in output pixels and degrees; their grids and
/// centres follow the content box, the box of the layers' shapes.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LayerFilter {
    /// A custom WGSL shader.
    Shader(LayerShader),
    /// Multiplies the color by `amount`: 1 keeps it, 0 is black.
    Brightness { amount: f64 },
    /// Scales the color's distance from mid-gray by `amount`: 1 keeps it,
    /// 0 is mid-gray.
    Contrast { amount: f64 },
    /// 1 keeps the color, 0 is its luma (Rec. 709), above 1 saturates it.
    Saturation { amount: f64 },
    /// Rotates the hue by `degrees`, as CSS `hue-rotate()`.
    Hue { degrees: f64 },
    /// Mixes the color toward its luma by `amount`, 0–1.
    Grayscale { amount: f64 },
    /// Mixes the color toward its inverse by `amount`, 0–1.
    Invert { amount: f64 },
    /// Mixes the color toward `color` (`#RRGGBB` or `#RRGGBBAA`) by
    /// `amount` times `color`'s alpha, 0–1.
    Tint { color: String, amount: f64 },
    /// Darkens toward the content box's edges by up to `amount`, 0–1.
    /// Darkening starts `radius` of the way from the box's centre to its
    /// corners and reaches `amount` `softness` further.
    Vignette {
        amount: f64,
        radius: f64,
        softness: f64,
    },
    /// Averages cells of `size` output pixels, from the content box's
    /// top-left corner. 1 or less keeps the pixels.
    Pixelate { size: f64 },
    /// Dots of each cell's color on a grid of `size` pixels turned by
    /// `angle` degrees about the content box's top-left corner, each dot's
    /// area its cell's luma; transparent between them. 0 keeps the pixels.
    Halftone { size: f64, angle: f64 },
    /// Moves red `offset` pixels toward `angle` degrees and blue as far the
    /// other way.
    ChromaticAberration { offset: f64, angle: f64 },
    /// Sways the pixels `amplitude` pixels toward `angle` degrees, along a
    /// sine of `wavelength` pixels across that direction, shifted by
    /// `phase` degrees.
    Wave {
        amplitude: f64,
        wavelength: f64,
        phase: f64,
        angle: f64,
    },
    /// Adds grain of up to `amount` to the color, from a hash of the scene
    /// pixel and `seed`: the same for every channel when `monochrome`.
    Noise {
        amount: f64,
        seed: u32,
        monochrome: bool,
    },
    /// Darkens by up to `amount` the horizontal bands `thickness` of each
    /// `spacing` pixels thick, from `offset` pixels below the content box's
    /// top.
    Scanlines {
        amount: f64,
        spacing: f64,
        thickness: f64,
        offset: f64,
    },
    /// Blurs toward `center_x`, `center_y` of the content box (0–1), over
    /// `amount` of each pixel's distance to it, 0–1.
    ZoomBlur {
        amount: f64,
        center_x: f64,
        center_y: f64,
    },
    /// Makes the colors near `color` (`#RRGGBB`) transparent: those within
    /// `tolerance` of its chroma (BT.709 CbCr distance) fully, those within
    /// `softness` further partly. `spill`, 0–1, takes the key's chroma out
    /// of what remains.
    ChromaKey {
        color: String,
        tolerance: f64,
        softness: f64,
        spill: f64,
    },
}

impl LayerFilter {
    /// Whether the filter leaves every pixel as it was: a built-in filter
    /// at the value that changes nothing. A custom shader and a chroma key
    /// never are.
    pub fn is_identity(&self) -> bool {
        match self {
            Self::Shader(_) | Self::ChromaKey { .. } => false,
            Self::Pixelate { size } => *size <= 1.0,
            Self::Halftone { size, .. } => *size == 0.0,
            Self::ChromaticAberration { offset, .. } => *offset == 0.0,
            Self::Wave { amplitude, .. } => *amplitude == 0.0,
            Self::Noise { amount, .. }
            | Self::Scanlines { amount, .. }
            | Self::ZoomBlur { amount, .. } => *amount == 0.0,
            Self::Brightness { amount }
            | Self::Contrast { amount }
            | Self::Saturation { amount } => *amount == 1.0,
            Self::Hue { degrees } => degrees % 360.0 == 0.0,
            Self::Grayscale { amount } | Self::Invert { amount } => *amount == 0.0,
            // `#RRGGBB00` lays a transparent color.
            Self::Tint { color, amount } => {
                *amount == 0.0 || (color.len() == 9 && color.ends_with("00"))
            }
            Self::Vignette { amount, .. } => *amount == 0.0,
        }
    }
}

impl<'de> Deserialize<'de> for LayerFilter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        LayerFilterDef::deserialize(TypeTagged(deserializer))
    }
}

/// `LayerFilter` without the `type` tag, which [`TypeTagged`] reads instead.
#[derive(Deserialize)]
#[serde(
    remote = "LayerFilter",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum LayerFilterDef {
    Shader(LayerShader),
    Brightness {
        amount: f64,
    },
    Contrast {
        amount: f64,
    },
    Saturation {
        amount: f64,
    },
    Hue {
        degrees: f64,
    },
    Grayscale {
        amount: f64,
    },
    Invert {
        amount: f64,
    },
    Tint {
        color: String,
        amount: f64,
    },
    Vignette {
        amount: f64,
        radius: f64,
        softness: f64,
    },
    Pixelate {
        size: f64,
    },
    Halftone {
        size: f64,
        angle: f64,
    },
    ChromaticAberration {
        offset: f64,
        angle: f64,
    },
    Wave {
        amplitude: f64,
        wavelength: f64,
        phase: f64,
        angle: f64,
    },
    Noise {
        amount: f64,
        seed: u32,
        monochrome: bool,
    },
    Scanlines {
        amount: f64,
        spacing: f64,
        thickness: f64,
        offset: f64,
    },
    ZoomBlur {
        amount: f64,
        center_x: f64,
        center_y: f64,
    },
    ChromaKey {
        color: String,
        tolerance: f64,
        softness: f64,
        spill: f64,
    },
}

/// One use of a custom shader on a layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LayerShader {
    /// A `ShaderSource::id` in the scene's `shaders`.
    pub id: String,
    /// Every parameter's components in declared order: 1 for `f32`, and 2,
    /// 3, or 4 for the vectors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<f64>,
    /// Output pixels the shader may write beyond the content box.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub padding: f64,
}

/// A custom shader's WGSL source and parameters, shared by every layer
/// that uses it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ShaderSource {
    /// Identifies the shader across frames.
    pub id: String,
    /// Names the shader in error messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The author's code: the `effect` function and its helpers, without
    /// the prelude.
    pub wgsl: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<ShaderParam>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ShaderParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: ShaderParamType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ShaderParamType {
    F32,
    Vec2,
    Vec3,
    Vec4,
}

impl ShaderParamType {
    /// How many numbers a value of this type has.
    pub const fn components(self) -> usize {
        match self {
            Self::F32 => 1,
            Self::Vec2 => 2,
            Self::Vec3 => 3,
            Self::Vec4 => 4,
        }
    }
}

const fn full_opacity() -> f64 {
    1.0
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// The default for [`AudioClip::pitch`]: no transposition.
pub fn zero_pitch() -> Animatable<f64> {
    Animatable::Static(0.0)
}

/// Whether `pitch` is the default, so [`AudioClip`] can leave it out.
pub fn is_zero_pitch(pitch: &Animatable<f64>) -> bool {
    matches!(pitch, Animatable::Static(value) if *value == 0.0)
}

/// The default for [`AudioClip::fade_in`] and [`AudioClip::fade_out`]: no
/// fade.
pub fn zero_fade() -> Time {
    Time::ZERO
}

/// Whether a fade is the default, so [`AudioClip`] can leave it out.
pub fn is_zero_fade(fade: &Time) -> bool {
    fade.value == 0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LayerShadow {
    pub color: String,
    pub blur: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LayerGlow {
    pub color: String,
    pub blur: f64,
}

// Deserialized through `LayerContentDef`, which keeps the same serde field attributes.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LayerContent {
    Video {
        asset: ResolvedAsset,
        timing: MediaTiming,
        /// Display size, as on `Image`: both set, both apply; one set, the
        /// other follows the frame's aspect; neither, the frame's own size.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f64>,
    },
    Image {
        asset: ResolvedAsset,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fit: Option<ImageFit>,
        /// Where an animated image (GIF, APNG, animated WebP) is in its own
        /// timeline. Absent, it draws its first frame; a still image ignores
        /// it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        animation: Option<ImageAnimation>,
    },
    Psd {
        asset: ResolvedAsset,
        /// Full paths of the layers and folders a portrait preset makes
        /// visible (PSDTool "all layer" semantics). When non-empty the PSD's
        /// own saved visibility is ignored and exactly these layers compose;
        /// when empty the PSD renders from its saved visibility state.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        visible_layers: Vec<String>,
        /// Layers forced visible on top of the resolved set — the selected
        /// lip-sync mouth and eyes for the current frame, and an expression's
        /// layers when there is no preset.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        enabled_layers: Vec<String>,
        /// Layers forced hidden — the other lip-sync mouth shapes. A folder
        /// path hides every layer inside it except the enabled ones.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        disabled_layers: Vec<String>,
    },
    Text {
        text: String,
        style: Box<TextStyle>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_width: Option<f64>,
        /// Anchors the layer vertically on its first line's baseline instead
        /// of `transform.anchor.y`, so separate `Text` layers placed at the
        /// same `y` share a baseline whatever their glyphs or sizes.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        baseline_anchor: bool,
    },
    Group {
        layers: Vec<Layer>,
        /// Draws the children only inside this region, in the group's own
        /// coordinate space (the space the children's positions are given
        /// in), so it moves, scales, and rotates with the group.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clip: Option<Clip>,
        /// Shows the children only where this subtree is drawn; see
        /// `GroupMask`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mask: Option<GroupMask>,
        /// Each child is one sample of the same subtree at another moment
        /// while the shutter is open. Each draws onto a transparent canvas of
        /// its own, and the group shows their average with equal weights,
        /// taken on premultiplied, sRGB-encoded values with no
        /// linearization. The group is always isolated: the average then
        /// goes through the mask, the effects, the opacity, and the blend
        /// mode as the children of any isolated group do. The clip applies
        /// to every sample.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        motion_blur: bool,
    },
    /// A flat-shaded rectangle, optionally rounded and/or stroked. Has no
    /// natural size the way `Image`/`Video` do, so `width`/`height` are
    /// explicit.
    Rect {
        width: f64,
        height: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<Paint>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<Stroke>,
        #[serde(default)]
        corner_radius: f64,
    },
    /// Lines and curves through points in the layer's own coordinates,
    /// filled and/or stroked as one shape: overlapping parts of a
    /// translucent stroke are painted once. The layer's position is the
    /// origin of those coordinates; like `Group`, a path has no box for
    /// `transform.anchor` to pick a point in, so the anchor is ignored.
    Path {
        commands: Vec<PathCommand>,
        /// Fills the area inside each subpath (non-zero rule); an open
        /// subpath is closed with a straight line for the fill only.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<Paint>,
        /// Painted over the fill, centered on the path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<Stroke>,
        #[serde(default, skip_serializing_if = "LineCap::is_butt")]
        line_cap: LineCap,
        #[serde(default, skip_serializing_if = "LineJoin::is_miter")]
        line_join: LineJoin,
        /// The longest a miter join may be, from its inner corner to its
        /// tip, as a multiple of the stroke width, before it falls back to a
        /// bevel. Defaults to 4, like SVG's `stroke-miterlimit`.
        #[serde(
            default = "default_miter_limit",
            skip_serializing_if = "is_default_miter_limit"
        )]
        miter_limit: f64,
        /// Breaks the stroke into dashes; `None` strokes a solid line.
        /// Boxed, as few paths are dashed and every layer stays small.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line_dash: Option<Box<LineDash>>,
    },
    /// `count` particles of one shape, drawn in order: particle `i` is the
    /// shape at `widths[i]` x `heights[i]` (or the shared size), centred on
    /// `(x[i], y[i])` in the layer's own coordinates, rotated `rotation[i]`
    /// degrees about its centre, and coloured `colors[i]` (straight RGBA).
    /// A rect or circle is filled with that colour; an image's texels are
    /// multiplied by it. Every column holds `count` values, `colors`
    /// `4 * count`. Like `Group`, the layer has no box, so the anchor is
    /// ignored.
    Instances {
        count: u32,
        shape: Box<InstanceShape>,
        /// Every particle's width when `widths` is absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        /// Every particle's height when `heights` is absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f64>,
        /// Every particle's colour (hex) when `colors` is absent; white when
        /// both are.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        #[cfg_attr(feature = "codegen", ts(type = "string"))]
        x: F32Column,
        #[cfg_attr(feature = "codegen", ts(type = "string"))]
        y: F32Column,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(type = "string"))]
        widths: Option<F32Column>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(type = "string"))]
        heights: Option<F32Column>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(type = "string"))]
        rotation: Option<F32Column>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(type = "string"))]
        colors: Option<U8Column>,
    },
    /// A `<Static>` subtree the React bridge already sent under `key`. It
    /// only travels inside the bridge, which replaces it with a `Group` of
    /// that subtree's layers before handing the scene on; renderers refuse
    /// it.
    StaticRef { key: u32 },
    MissingComponent {
        component: String,
        props: BTreeMap<String, Value>,
    },
}

impl<'de> Deserialize<'de> for LayerContent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let content = LayerContentDef::deserialize(TypeTagged(deserializer))?;
        if let Some(error) = content
            .instance_column_error()
            .or_else(|| content.line_dash_error())
        {
            return Err(serde::de::Error::custom(error));
        }
        Ok(content)
    }
}

impl LayerContent {
    /// Why a `Path` layer's dash pattern cannot be drawn: a negative or
    /// non-finite length, or a non-finite offset.
    fn line_dash_error(&self) -> Option<String> {
        let LayerContent::Path {
            line_dash: Some(dash),
            ..
        } = self
        else {
            return None;
        };
        let LineDash { pattern, offset } = dash.as_ref();
        if pattern
            .iter()
            .any(|length| !length.is_finite() || *length < 0.0)
        {
            return Some(format!(
                "path lineDash pattern lengths must be finite and at least 0, got {pattern:?}"
            ));
        }
        if !offset.is_finite() {
            return Some(format!("path lineDash offset must be finite, got {offset}"));
        }
        None
    }

    /// Why an `Instances` layer's columns do not fit its `count`, so the
    /// renderers can index every column by particle without checks.
    fn instance_column_error(&self) -> Option<String> {
        let LayerContent::Instances {
            count,
            x,
            y,
            widths,
            heights,
            rotation,
            colors,
            ..
        } = self
        else {
            return None;
        };
        let count = *count as usize;
        let floats = [
            ("x", Some(x)),
            ("y", Some(y)),
            ("widths", widths.as_ref()),
            ("heights", heights.as_ref()),
            ("rotation", rotation.as_ref()),
        ];
        let lengths = floats
            .into_iter()
            .filter_map(|(name, column)| column.map(|column| (name, column.len(), count)))
            .chain(
                colors
                    .as_ref()
                    .map(|colors| ("colors", colors.len(), 4 * count)),
            );
        for (name, len, expected) in lengths {
            if len != expected {
                return Some(format!(
                    "instances column `{name}` has {len} values; count {count} needs {expected}"
                ));
            }
        }
        None
    }
}

/// `LayerContent` without the `type` tag, which [`TypeTagged`] reads instead.
#[derive(Deserialize)]
#[serde(
    remote = "LayerContent",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum LayerContentDef {
    Video {
        asset: ResolvedAsset,
        timing: MediaTiming,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
    },
    Image {
        asset: ResolvedAsset,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
        #[serde(default)]
        fit: Option<ImageFit>,
        #[serde(default)]
        animation: Option<ImageAnimation>,
    },
    Psd {
        asset: ResolvedAsset,
        #[serde(default)]
        visible_layers: Vec<String>,
        #[serde(default)]
        enabled_layers: Vec<String>,
        #[serde(default)]
        disabled_layers: Vec<String>,
    },
    Text {
        text: String,
        style: Box<TextStyle>,
        #[serde(default)]
        max_width: Option<f64>,
        #[serde(default)]
        baseline_anchor: bool,
    },
    Group {
        layers: Vec<Layer>,
        #[serde(default)]
        clip: Option<Clip>,
        #[serde(default)]
        mask: Option<GroupMask>,
        #[serde(default)]
        motion_blur: bool,
    },
    Rect {
        width: f64,
        height: f64,
        #[serde(default)]
        fill: Option<Paint>,
        #[serde(default)]
        stroke: Option<Stroke>,
        #[serde(default)]
        corner_radius: f64,
    },
    Path {
        commands: Vec<PathCommand>,
        #[serde(default)]
        fill: Option<Paint>,
        #[serde(default)]
        stroke: Option<Stroke>,
        #[serde(default)]
        line_cap: LineCap,
        #[serde(default)]
        line_join: LineJoin,
        #[serde(default = "default_miter_limit")]
        miter_limit: f64,
        #[serde(default)]
        line_dash: Option<Box<LineDash>>,
    },
    Instances {
        count: u32,
        shape: Box<InstanceShape>,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
        #[serde(default)]
        color: Option<String>,
        x: F32Column,
        y: F32Column,
        #[serde(default)]
        widths: Option<F32Column>,
        #[serde(default)]
        heights: Option<F32Column>,
        #[serde(default)]
        rotation: Option<F32Column>,
        #[serde(default)]
        colors: Option<U8Column>,
    },
    StaticRef {
        key: u32,
    },
    MissingComponent {
        component: String,
        props: BTreeMap<String, Value>,
    },
}

/// One step of a `LayerContent::Path`, in the layer's own pixels, like SVG's
/// absolute path commands. A command other than `MoveTo` that starts the
/// path, or follows `Close`, begins its subpath at `(0, 0)` or the closed
/// subpath's start, as in SVG.
// Deserialized through `PathCommandDef`, which keeps the same serde field attributes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PathCommand {
    /// Starts a new subpath at `(x, y)`.
    MoveTo { x: f64, y: f64 },
    /// A straight line to `(x, y)`.
    LineTo { x: f64, y: f64 },
    /// A quadratic Bézier curve to `(x, y)` with control point `(x1, y1)`.
    QuadTo { x1: f64, y1: f64, x: f64, y: f64 },
    /// A cubic Bézier curve to `(x, y)` with control points `(x1, y1)` and
    /// `(x2, y2)`.
    CubicTo {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    /// Closes the subpath with a straight line back to its start and joins
    /// the two ends, so a closed outline has no seam.
    Close,
}

impl<'de> Deserialize<'de> for PathCommand {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PathCommandDef::deserialize(TypeTagged(deserializer))
    }
}

/// `PathCommand` without the `type` tag, which [`TypeTagged`] reads instead.
#[derive(Deserialize)]
#[serde(
    remote = "PathCommand",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum PathCommandDef {
    MoveTo {
        x: f64,
        y: f64,
    },
    LineTo {
        x: f64,
        y: f64,
    },
    QuadTo {
        x1: f64,
        y1: f64,
        x: f64,
        y: f64,
    },
    CubicTo {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    Close,
}

/// How a stroke ends at the open ends of a path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum LineCap {
    /// Ends exactly at the end point.
    #[default]
    Butt,
    /// Adds a half disc around the end point.
    Round,
    /// Adds half a square around the end point.
    Square,
}

impl LineCap {
    pub fn is_butt(&self) -> bool {
        *self == Self::Butt
    }
}

/// How a stroke turns the corners between segments.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum LineJoin {
    /// A sharp corner, cut to a bevel past `miter_limit`.
    #[default]
    Miter,
    /// A circular arc around the corner point.
    Round,
    /// A straight cut across the corner.
    Bevel,
}

impl LineJoin {
    pub fn is_miter(&self) -> bool {
        *self == Self::Miter
    }
}

/// The dashes a path's stroke is broken into, like SVG's `stroke-dasharray`
/// and `stroke-dashoffset`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LineDash {
    /// Lengths of the dashes and the gaps between them, in turn, in the
    /// layer's own units, starting over at each subpath. An odd count is
    /// repeated to make an even one; empty, or all zero, strokes a solid
    /// line. A zero-length dash still gets its round or square caps, a dot.
    pub pattern: Vec<f64>,
    /// How far into `pattern` each subpath starts: animating it makes the
    /// dashes flow backwards along the path.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: f64,
}

pub const DEFAULT_MITER_LIMIT: f64 = 4.0;

const fn default_miter_limit() -> f64 {
    DEFAULT_MITER_LIMIT
}

fn is_default_miter_limit(value: &f64) -> bool {
    *value == DEFAULT_MITER_LIMIT
}

/// A rectangle, optionally with rounded corners, that limits where a group's
/// children are drawn. Coordinates are in the group's own space, in pixels:
/// `x`/`y` is the top-left corner. Pixels straddling the edge are
/// anti-aliased, and nested clips intersect.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Corner radius, limited to half the shorter side. Defaults to 0.
    #[serde(default)]
    pub corner_radius: f64,
}

impl Clip {
    /// Whether the region has no area, so nothing inside it is visible.
    /// Also true when a value is not finite.
    pub fn is_empty(&self) -> bool {
        let finite = [self.x, self.y, self.width, self.height]
            .into_iter()
            .all(f64::is_finite);
        !finite || self.width <= 0.0 || self.height <= 0.0
    }

    /// The corner radius as drawn: at least 0, at most half the shorter side.
    pub fn effective_corner_radius(&self) -> f64 {
        let radius = if self.corner_radius.is_finite() {
            self.corner_radius
        } else {
            0.0
        };
        radius.max(0.0).min(self.width.min(self.height) / 2.0)
    }
}

/// Another drawn subtree that decides, per pixel, how much of a group shows.
/// Its layers are positioned in the group's own coordinate space, like the
/// group's children, and drawn at opacity 1 with normal blending and no clip;
/// they are never shown themselves. Where they draw nothing, nothing of the
/// group shows (everything does when inverted).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GroupMask {
    pub layers: Vec<Layer>,
    #[serde(default, skip_serializing_if = "MaskMode::is_alpha")]
    pub mode: MaskMode,
    /// Shows the group where the mask is not drawn instead: `1 - m`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invert: bool,
}

/// Which value of a mask pixel shows the group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum MaskMode {
    /// The mask's alpha.
    #[default]
    Alpha,
    /// The Rec. 709 luma of the mask's sRGB-encoded color times its alpha:
    /// `(0.2126 R + 0.7152 G + 0.0722 B) × A`, with no linearization.
    Luminance,
}

impl MaskMode {
    pub fn is_alpha(&self) -> bool {
        *self == Self::Alpha
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
// Missing fields take their `Default` values, so the React bridge can leave
// out unchanged ones; a NEBULA frame has 1,500 identity-scaled particles.
#[serde(rename_all = "camelCase", default)]
pub struct EvaluatedTransform {
    pub position: Point,
    pub scale: Point,
    /// Clockwise rotation in degrees.
    pub rotation: f64,
    /// Normalized coordinates where `(0.5, 0.5)` is the center.
    pub anchor: Point,
    /// The point the layer turns, scales, and skews about, which is also the
    /// vanishing point of its perspective: an offset in the layer's own
    /// pixels from its anchor (a `Group`'s, `Path`'s, or `Instances`'s own
    /// origin, as they have no box). The layer stays where `position` puts
    /// it until it is transformed; default (0, 0).
    #[serde(skip_serializing_if = "is_zero_point")]
    #[cfg_attr(feature = "codegen", ts(as = "Option<Point>", optional))]
    pub origin: Point,
    /// Shear angles in degrees along x and y.
    #[serde(skip_serializing_if = "is_zero_point")]
    #[cfg_attr(feature = "codegen", ts(as = "Option<Point>", optional))]
    pub skew: Point,
    /// Rotation in degrees about the horizontal axis, as CSS `rotateX()`.
    #[serde(skip_serializing_if = "is_zero")]
    #[cfg_attr(feature = "codegen", ts(as = "Option<f64>", optional))]
    pub rotation_x: f64,
    /// Rotation in degrees about the vertical axis, as CSS `rotateY()`.
    #[serde(skip_serializing_if = "is_zero")]
    #[cfg_attr(feature = "codegen", ts(as = "Option<f64>", optional))]
    pub rotation_y: f64,
    /// Distance from the viewer to the layer plane; anything that is not
    /// finite and positive means no perspective.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub perspective: Option<f64>,
    #[serde(skip_serializing_if = "BackfaceVisibility::is_visible")]
    #[cfg_attr(feature = "codegen", ts(as = "Option<BackfaceVisibility>", optional))]
    pub backface_visibility: BackfaceVisibility,
}

fn is_zero_point(point: &Point) -> bool {
    point.x == 0.0 && point.y == 0.0
}

impl Default for EvaluatedTransform {
    fn default() -> Self {
        Self {
            position: Point { x: 0.0, y: 0.0 },
            scale: Point { x: 1.0, y: 1.0 },
            rotation: 0.0,
            anchor: Point { x: 0.5, y: 0.5 },
            origin: Point { x: 0.0, y: 0.0 },
            skew: Point { x: 0.0, y: 0.0 },
            rotation_x: 0.0,
            rotation_y: 0.0,
            perspective: None,
            backface_visibility: BackfaceVisibility::Visible,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ResolvedAsset {
    pub id: String,
    pub location: AssetLocation,
}

// Deserialized through `AssetLocationDef`, which keeps the same serde field attributes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AssetLocation {
    File { path: String },
    Url { url: String },
}

impl<'de> Deserialize<'de> for AssetLocation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        AssetLocationDef::deserialize(TypeTagged(deserializer))
    }
}

/// `AssetLocation` without the `type` tag, which [`TypeTagged`] reads instead.
#[derive(Deserialize)]
#[serde(
    remote = "AssetLocation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum AssetLocationDef {
    File { path: String },
    Url { url: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MediaTiming {
    pub local_time: Time,
    pub source_start: Time,
    pub source_time_seconds: f64,
    pub playback_rate: f64,
    /// Past the end of the video, start over from its first frame instead
    /// of holding the last one.
    #[serde(default, skip_serializing_if = "is_false")]
    pub r#loop: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AudioGraph {
    pub sample_rate: u32,
    #[serde(default = "default_audio_master_volume")]
    pub master_volume: f64,
    pub clips: Vec<AudioClip>,
}

const fn default_audio_master_volume() -> f64 {
    1.0
}

impl Default for AudioGraph {
    fn default() -> Self {
        Self {
            sample_rate: 0,
            master_volume: 1.0,
            clips: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AudioClip {
    pub id: String,
    pub asset: ResolvedAsset,
    pub range: TimeRange,
    pub source_start: Time,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_duration: Option<Time>,
    pub playback_rate: Animatable<f64>,
    pub volume: Animatable<f64>,
    pub muted: bool,
    /// Past the end of the source, start over from its beginning instead of
    /// falling silent.
    #[serde(default, skip_serializing_if = "is_false")]
    pub r#loop: bool,
    /// Transposition in semitones; fractions are cents. Animated over
    /// clip-local seconds like `playback_rate`.
    #[serde(default = "zero_pitch", skip_serializing_if = "is_zero_pitch")]
    pub pitch: Animatable<f64>,
    /// When true, `playback_rate` changes only how long the clip takes, and the
    /// pitch is set by `pitch` alone. When false, speed and pitch move
    /// together.
    #[serde(default, skip_serializing_if = "is_false")]
    pub preserve_pitch: bool,
    /// How long the clip takes to rise from silence at the start of `range`.
    /// See [`AudioFade`].
    #[serde(default = "zero_fade", skip_serializing_if = "is_zero_fade")]
    pub fade_in: Time,
    /// How long the clip takes to fall to silence at the end of `range`. See
    /// [`AudioFade`].
    #[serde(default = "zero_fade", skip_serializing_if = "is_zero_fade")]
    pub fade_out: Time,
    /// The group this clip's sound belongs to, for other clips' `duck.under`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duck_group: Option<String>,
    /// Turns this clip down while clips of other groups play. See
    /// [`crate::duck_curve`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duck: Option<crate::AudioDuck>,
}

impl AudioClip {
    /// The clip's fades in seconds, for a mixer to apply sample by sample.
    pub fn fade(&self) -> Result<AudioFade, TimeError> {
        Ok(AudioFade {
            fade_in: self.fade_in.as_seconds()?,
            fade_out: self.fade_out.as_seconds()?,
            duration: self.range.duration.as_seconds()?,
        })
    }
}

/// An [`AudioClip`]'s fade-in and fade-out, a gain the mixer multiplies into
/// `volume`.
///
/// Each fade is a linear ramp between silence at an edge of the clip's
/// `range` and full gain, timed on the timeline: the source's position,
/// `playback_rate`, `loop`, and `pitch` do not move it. When the fades
/// overlap, their gains multiply, so the clip never reaches full gain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioFade {
    fade_in: f64,
    fade_out: f64,
    duration: f64,
}

impl AudioFade {
    /// The gain at `local_seconds` into the clip's range, from 0 to 1. A fade
    /// of 0 or less leaves the gain alone.
    pub fn gain(&self, local_seconds: f64) -> f64 {
        let mut gain = 1.0;
        if self.fade_in > 0.0 {
            gain *= (local_seconds / self.fade_in).clamp(0.0, 1.0);
        }
        if self.fade_out > 0.0 {
            gain *= ((self.duration - local_seconds) / self.fade_out).clamp(0.0, 1.0);
        }
        gain
    }
}

/// The frame of an animated image to draw.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ImageAnimation {
    /// Seconds into the animation; the frame showing at that time is drawn.
    pub source_time_seconds: f64,
    /// Past the end, start over from the first frame instead of holding the
    /// last one.
    #[serde(default)]
    pub r#loop: bool,
}

/// Aspect-ratio policy inside an image's display box.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ImageFit {
    Contain,
    Cover,
}
