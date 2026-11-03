// Ported from UniVRM (https://github.com/vrm-c/UniVRM), under
// Packages/VRM10/Runtime/Components/: Vrm10RuntimeExpression.cs,
// Expression/DefaultExpressionValidator.cs, ExpressionMerger.cs,
// MorphTargetBindingMerger/MorphTargetBindingMerger.cs, and
// MaterialValueBindingMerger.cs; and Packages/VRM10/Runtime/IO/ExpressionExtensions.cs.
// MIT License, Copyright (c) 2020 VRM Consortium, Copyright (c) 2018 Masataka
// SUMI for MToon.

//! A VRM avatar's expressions, with UniVRM's rules for how they combine.
//!
//! - **Keys** go in the presets' order (`happy` … `neutral`), then custom
//!   names in byte order. A custom expression named like a preset is left
//!   out with a warning: its name asks for the preset.
//! - **Inputs** are clamped to 0..1, VRM 1.0's range. Names the avatar has
//!   no expression for are ignored, as UniVRM's `SetWeight` does.
//! - **Validation** (`DefaultExpressionValidator`): a binary expression's
//!   weight is rounded, halves to even (Unity's `Mathf.Round`). Every
//!   expression outside a group (blink, lookAt, mouth) adds its override
//!   rate to that group: `none` 0, `block` 1 once its weight is above 0,
//!   `blend` its weight. Each group's rate is clamped to 0..1, and the
//!   group's expressions are multiplied by `1 − rate`; a binary one is 0
//!   once the rate is above 0.
//! - **Binds** (`ExpressionMerger` and its mergers): a binary expression
//!   counts as 1 above 0.5 and 0 otherwise.
//!   - A morph target any expression binds is `Σ bind.weight × weight`,
//!     from 0 each frame, on every draw of the bound node; the others keep
//!     their defaults.
//!   - A material colour is `base + Σ (target − base) × weight`. UniVRM
//!     keeps `color`, `shadeColor`, `matcapColor`, `rimColor`, and
//!     `outlineColor` as sRGB-encoded material colours and takes the
//!     file's `targetValue` as it is, so these blend in sRGB (alpha stays
//!     linear); `emissionColor` blends linearly.
//!   - A base colour texture's scale and offset are
//!     `base + Σ (bind − base) × weight`, from the material's own transform.
//!   - A material's colour or transform is overridden only while an
//!     expression binding it has a weight; otherwise it is the material's
//!     own, which is what `base + 0` gives.
//! - **Indices:** a bind naming a node or material the file lacks refuses
//!   the model, as UniVRM's importer fails on it. A morph target bind on a
//!   node without a mesh, or past its mesh's targets, is ignored with a
//!   warning, as UniVRM's merger skips it.
//!
//! `VRMC_vrm_expressions_node_transform` binds are not read.

use crate::document::{Document, LoadError};
use crate::model::DrawSource;
use celesta_plugin::{
    Material, MaterialKind, MaterialOverrides, MtoonMaterial, Notice, NoticeLevel, TextureTransform,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Range;
use vrm_spec::vrmc_vrm_1_0::{
    Expression, ExpressionOverrideType, ExpressionPresetName, MaterialColorType,
};

/// VRM 1.0's preset expressions, in the order keys sort.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Preset {
    Happy,
    Angry,
    Sad,
    Relaxed,
    Surprised,
    Aa,
    Ih,
    Ou,
    Ee,
    Oh,
    Blink,
    BlinkLeft,
    BlinkRight,
    LookUp,
    LookDown,
    LookLeft,
    LookRight,
    Neutral,
}

impl Preset {
    pub const ALL: [Self; 18] = [
        Self::Happy,
        Self::Angry,
        Self::Sad,
        Self::Relaxed,
        Self::Surprised,
        Self::Aa,
        Self::Ih,
        Self::Ou,
        Self::Ee,
        Self::Oh,
        Self::Blink,
        Self::BlinkLeft,
        Self::BlinkRight,
        Self::LookUp,
        Self::LookDown,
        Self::LookLeft,
        Self::LookRight,
        Self::Neutral,
    ];

    /// Its name in VRM files and in frame data.
    pub fn name(self) -> &'static str {
        match self {
            Self::Happy => "happy",
            Self::Angry => "angry",
            Self::Sad => "sad",
            Self::Relaxed => "relaxed",
            Self::Surprised => "surprised",
            Self::Aa => "aa",
            Self::Ih => "ih",
            Self::Ou => "ou",
            Self::Ee => "ee",
            Self::Oh => "oh",
            Self::Blink => "blink",
            Self::BlinkLeft => "blinkLeft",
            Self::BlinkRight => "blinkRight",
            Self::LookUp => "lookUp",
            Self::LookDown => "lookDown",
            Self::LookLeft => "lookLeft",
            Self::LookRight => "lookRight",
            Self::Neutral => "neutral",
        }
    }

    fn from_spec(name: ExpressionPresetName) -> Self {
        use ExpressionPresetName as N;
        match name {
            N::Happy => Self::Happy,
            N::Angry => Self::Angry,
            N::Sad => Self::Sad,
            N::Relaxed => Self::Relaxed,
            N::Surprised => Self::Surprised,
            N::Aa => Self::Aa,
            N::Ih => Self::Ih,
            N::Ou => Self::Ou,
            N::Ee => Self::Ee,
            N::Oh => Self::Oh,
            N::Blink => Self::Blink,
            N::BlinkLeft => Self::BlinkLeft,
            N::BlinkRight => Self::BlinkRight,
            N::LookUp => Self::LookUp,
            N::LookDown => Self::LookDown,
            N::LookLeft => Self::LookLeft,
            N::LookRight => Self::LookRight,
            N::Neutral => Self::Neutral,
        }
    }
}

/// An expression's name: a preset, or a custom name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExpressionKey {
    Preset(Preset),
    Custom(String),
}

impl ExpressionKey {
    /// The key a name in a file or in frame data stands for: a preset's
    /// exact name is that preset, anything else a custom name.
    pub fn parse(name: &str) -> Self {
        Preset::ALL
            .into_iter()
            .find(|preset| preset.name() == name)
            .map_or_else(|| Self::Custom(name.to_string()), Self::Preset)
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Preset(preset) => preset.name(),
            Self::Custom(name) => name,
        }
    }

    /// `blink`, `blinkLeft`, or `blinkRight`.
    pub fn is_blink(&self) -> bool {
        matches!(
            self,
            Self::Preset(Preset::Blink | Preset::BlinkLeft | Preset::BlinkRight)
        )
    }

    /// `lookUp`, `lookDown`, `lookLeft`, or `lookRight`.
    pub fn is_look_at(&self) -> bool {
        matches!(
            self,
            Self::Preset(Preset::LookUp | Preset::LookDown | Preset::LookLeft | Preset::LookRight)
        )
    }

    /// `aa`, `ih`, `ou`, `ee`, or `oh`.
    pub fn is_mouth(&self) -> bool {
        matches!(
            self,
            Self::Preset(Preset::Aa | Preset::Ih | Preset::Ou | Preset::Ee | Preset::Oh)
        )
    }
}

impl fmt::Display for ExpressionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// How an expression overrides a group (`ExpressionOverrideType`).
#[derive(Clone, Copy, Debug)]
enum Override {
    None,
    Block,
    Blend,
}

impl Override {
    fn from_spec(value: Option<ExpressionOverrideType>) -> Self {
        match value {
            None | Some(ExpressionOverrideType::None) => Self::None,
            Some(ExpressionOverrideType::Block) => Self::Block,
            Some(ExpressionOverrideType::Blend) => Self::Blend,
        }
    }

    /// `DefaultExpressionValidator.GetOverrideRate`.
    fn rate(self, weight: f32) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Block if weight > 0.0 => 1.0,
            Self::Block => 0.0,
            Self::Blend => weight,
        }
    }
}

/// A material colour an expression binds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Colour {
    Base,
    Emission,
    Shade,
    Matcap,
    Rim,
    Outline,
}

impl Colour {
    fn from_spec(kind: MaterialColorType) -> Self {
        match kind {
            MaterialColorType::Color => Self::Base,
            MaterialColorType::EmissionColor => Self::Emission,
            MaterialColorType::ShadeColor => Self::Shade,
            MaterialColorType::MatcapColor => Self::Matcap,
            MaterialColorType::RimColor => Self::Rim,
            MaterialColorType::OutlineColor => Self::Outline,
        }
    }

    /// Whether UniVRM keeps it sRGB-encoded, so it blends in sRGB.
    fn srgb(self) -> bool {
        self != Self::Emission
    }

    /// The material's own value, linear RGBA (alpha 1 for RGB colours).
    /// MToon-only colours on an unlit material are MToon's defaults; the
    /// renderer ignores them there.
    fn of(self, material: Option<&Material>) -> [f32; 4] {
        let defaults = MtoonMaterial::default();
        let mtoon = match material.map(|material| &material.kind) {
            Some(MaterialKind::Mtoon(mtoon)) => mtoon,
            Some(MaterialKind::Unlit(unlit)) if self == Self::Base => return unlit.base_color,
            _ => &defaults,
        };
        let [r, g, b] = match self {
            Self::Base => return mtoon.base_color,
            Self::Emission => mtoon.emissive_factor,
            Self::Shade => mtoon.shade_color_factor,
            Self::Matcap => mtoon.matcap_factor,
            Self::Rim => mtoon.parametric_rim_color_factor,
            Self::Outline => mtoon.outline_color_factor,
        };
        [r, g, b, 1.0]
    }

    /// Writes `value` (linear RGBA) into its override field.
    fn write(self, value: [f32; 4], overrides: &mut MaterialOverrides) {
        let rgb = Some([value[0], value[1], value[2]]);
        match self {
            Self::Base => overrides.base_color = Some(value),
            Self::Emission => overrides.emission_color = rgb,
            Self::Shade => overrides.shade_color = rgb,
            Self::Matcap => overrides.matcap_color = rgb,
            Self::Rim => overrides.rim_color = rgb,
            Self::Outline => overrides.outline_color = rgb,
        }
    }
}

/// sRGB's encoding of a linear value (Unity's `Mathf.LinearToGammaSpace`).
fn encode_srgb(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// sRGB's decoding to a linear value (Unity's `Mathf.GammaToLinearSpace`).
fn decode_srgb(encoded: f32) -> f32 {
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// One expression, its binds pointing at slots.
struct Definition {
    key: ExpressionKey,
    binary: bool,
    blink: Override,
    look_at: Override,
    mouth: Override,
    /// (morph slot, bind weight).
    morphs: Vec<(usize, f32)>,
    /// (colour slot, target in the slot's blend space).
    colours: Vec<(usize, [f32; 4])>,
    /// (UV slot, scale, offset).
    uvs: Vec<(usize, [f32; 2], [f32; 2])>,
}

/// A bound node's draws and its morph slots.
struct MorphNode {
    draws: Range<usize>,
    /// Into `Expressions::morph_targets`: the node's bound targets.
    slots: Range<usize>,
}

/// A bound (material, colour), and the material's own value in the
/// colour's blend space.
struct ColourSlot {
    material: usize,
    colour: Colour,
    base: [f32; 4],
}

/// A bound material's base colour texture scale and offset.
struct UvSlot {
    material: usize,
    scale: [f32; 2],
    offset: [f32; 2],
}

/// The avatar's expressions, in key order, with their binds read.
pub(crate) struct Expressions {
    definitions: Vec<Definition>,
    /// By morph slot: its morph target. One node's slots are together, in
    /// ascending target order.
    morph_targets: Vec<usize>,
    morph_nodes: Vec<MorphNode>,
    colours: Vec<ColourSlot>,
    uvs: Vec<UvSlot>,
}

/// An expression as the file has it, checked: binds name (node, target),
/// (material, colour), and material rather than slots.
struct ReadDefinition {
    key: ExpressionKey,
    binary: bool,
    blink: Override,
    look_at: Override,
    mouth: Override,
    morphs: Vec<(usize, usize, f32)>,
    colours: Vec<(usize, Colour, [f32; 4])>,
    uvs: Vec<(usize, [f32; 2], [f32; 2])>,
}

/// The expressions as the file has them, checked; their bases come from
/// the materials (see [`Expressions::new`]).
pub(crate) struct ExpressionBinds {
    definitions: Vec<ReadDefinition>,
    /// By node: its draws.
    node_draws: BTreeMap<usize, Range<usize>>,
}

/// Every weight an expression ends a frame with.
pub(crate) struct Applied {
    /// By key, every expression the avatar has: its actual weight.
    pub weights: BTreeMap<ExpressionKey, f32>,
    /// How much the lookAt is overridden, 0..1.
    // The lookAt (Task 6) scales the gaze by 1 − this.
    #[allow(dead_code)]
    pub look_at_rate: f32,
}

/// `values` as f32, each finite, or `None`.
fn finite<const N: usize>(values: &[f64]) -> Option<[f32; N]> {
    let values: [f64; N] = values.try_into().ok()?;
    let values = values.map(|value| value as f32);
    values
        .iter()
        .all(|value| value.is_finite())
        .then_some(values)
}

/// Reads and checks the file's expressions: every bind's node and
/// material is the file's, and every number is finite. Run before the
/// materials, so a refused model writes no texture.
pub(crate) fn read_expressions(
    document: &Document,
    draws: &[DrawSource],
    notices: &mut Vec<Notice>,
) -> Result<ExpressionBinds, LoadError> {
    let gltf = &document.gltf;
    let nodes: Vec<gltf::Node> = gltf.nodes().collect();
    let material_count = gltf.materials().len();
    // By mesh: its targets, the most any primitive has.
    let mesh_targets: Vec<usize> = gltf
        .meshes()
        .map(|mesh| {
            mesh.primitives()
                .map(|primitive| primitive.morph_targets().len())
                .max()
                .unwrap_or(0)
        })
        .collect();

    let mut keyed: BTreeMap<ExpressionKey, &Expression> = BTreeMap::new();
    if let Some(expressions) = &document.vrm.expressions {
        if let Some(preset) = &expressions.preset {
            for (name, expression) in &preset.0 {
                keyed.insert(ExpressionKey::Preset(Preset::from_spec(*name)), expression);
            }
        }
        if let Some(custom) = &expressions.custom {
            let custom: BTreeMap<&String, &Expression> = custom.iter().collect();
            for (name, expression) in custom {
                match ExpressionKey::parse(name) {
                    key @ ExpressionKey::Custom(_) => {
                        keyed.insert(key, expression);
                    }
                    ExpressionKey::Preset(_) => notices.push(Notice {
                        level: NoticeLevel::Warning,
                        message: format!(
                            "custom expression \"{name}\" has a preset's name, so it is left out"
                        ),
                    }),
                }
            }
        }
    }

    let mut definitions = Vec::with_capacity(keyed.len());
    for (key, expression) in keyed {
        let material = |index: usize| {
            if index < material_count {
                Ok(index)
            } else {
                Err(LoadError::Invalid(format!(
                    "expression \"{key}\" binds material {index}, but the file has {material_count} materials"
                )))
            }
        };
        let mut binds = ReadDefinition {
            key: key.clone(),
            binary: expression.is_binary.unwrap_or(false),
            blink: Override::from_spec(expression.override_blink),
            look_at: Override::from_spec(expression.override_look_at),
            mouth: Override::from_spec(expression.override_mouth),
            morphs: Vec::new(),
            colours: Vec::new(),
            uvs: Vec::new(),
        };
        for bind in expression.morph_target_binds.iter().flatten() {
            let index = bind.node.value();
            let Some(node) = nodes.get(index) else {
                return Err(LoadError::Invalid(format!(
                    "expression \"{key}\" binds node {index}, but the file has {} nodes",
                    nodes.len()
                )));
            };
            let weight = bind.weight as f32;
            if !weight.is_finite() {
                return Err(LoadError::Invalid(format!(
                    "expression \"{key}\" has a morph target bind whose weight is not finite: {}",
                    bind.weight
                )));
            }
            let target = bind.index;
            let ignored = match node.mesh() {
                None => Some(format!(
                    "expression \"{key}\" binds morph target {target} of node {index}, which has no mesh; the bind is ignored"
                )),
                Some(mesh) => {
                    let targets = mesh_targets.get(mesh.index()).copied().unwrap_or(0);
                    (target >= targets).then(|| format!(
                        "expression \"{key}\" binds morph target {target} of node {index}, whose mesh has {targets}; the bind is ignored"
                    ))
                }
            };
            match ignored {
                Some(message) => notices.push(Notice {
                    level: NoticeLevel::Warning,
                    message,
                }),
                None => binds.morphs.push((index, target, weight)),
            }
        }
        for bind in expression.material_color_binds.iter().flatten() {
            let index = material(bind.material.value())?;
            let Some(target) = finite::<4>(&bind.target_value) else {
                let problem = if bind.target_value.len() == 4 {
                    "is not finite"
                } else {
                    "is not four numbers"
                };
                return Err(LoadError::Invalid(format!(
                    "expression \"{key}\" has a materialColorBind whose targetValue {problem}: {:?}",
                    bind.target_value
                )));
            };
            binds
                .colours
                .push((index, Colour::from_spec(bind.r#type), target));
        }
        for bind in expression.texture_transform_binds.iter().flatten() {
            let index = material(bind.material.value())?;
            let pair = |name: &str, values: &Option<Vec<f64>>, default: [f32; 2]| {
                let Some(values) = values else {
                    return Ok(default);
                };
                finite::<2>(values).ok_or_else(|| {
                    let problem = if values.len() == 2 {
                        "is not finite"
                    } else {
                        "is not two numbers"
                    };
                    LoadError::Invalid(format!(
                        "expression \"{key}\" has a textureTransformBind whose {name} {problem}: {values:?}"
                    ))
                })
            };
            let scale = pair("scale", &bind.scale, [1.0, 1.0])?;
            let offset = pair("offset", &bind.offset, [0.0, 0.0])?;
            binds.uvs.push((index, scale, offset));
        }
        definitions.push(binds);
    }

    // One node's draws are together (`read_draws` makes them node by node).
    let mut node_draws: BTreeMap<usize, Range<usize>> = BTreeMap::new();
    for (index, draw) in draws.iter().enumerate() {
        let range = node_draws.entry(draw.node).or_insert(index..index);
        debug_assert_eq!(range.end, index, "node {}'s draws are apart", draw.node);
        range.end = index + 1;
    }
    Ok(ExpressionBinds {
        definitions,
        node_draws,
    })
}

impl Expressions {
    /// The expressions of `binds`, blending from `materials` (the scene's,
    /// by index) and `base_transforms` (by glTF material).
    pub(crate) fn new(
        binds: ExpressionBinds,
        materials: &[Material],
        base_transforms: &[Option<TextureTransform>],
    ) -> Self {
        // Slots in a fixed order: by node and target, by material and
        // colour, by material.
        let morph_keys: BTreeSet<(usize, usize)> = binds
            .definitions
            .iter()
            .flat_map(|read| read.morphs.iter().map(|&(node, target, _)| (node, target)))
            .collect();
        let colour_keys: BTreeSet<(usize, Colour)> = binds
            .definitions
            .iter()
            .flat_map(|read| {
                read.colours
                    .iter()
                    .map(|&(material, colour, _)| (material, colour))
            })
            .collect();
        let uv_keys: BTreeSet<usize> = binds
            .definitions
            .iter()
            .flat_map(|read| read.uvs.iter().map(|&(material, _, _)| material))
            .collect();

        let mut morph_slots = BTreeMap::new();
        let mut morph_targets = Vec::with_capacity(morph_keys.len());
        let mut morph_nodes: Vec<MorphNode> = Vec::new();
        let mut last_node = None;
        for (node, target) in morph_keys {
            let slot = morph_targets.len();
            morph_slots.insert((node, target), slot);
            morph_targets.push(target);
            match morph_nodes.last_mut() {
                Some(last) if last_node == Some(node) => last.slots.end = slot + 1,
                _ => {
                    last_node = Some(node);
                    morph_nodes.push(MorphNode {
                        draws: binds.node_draws.get(&node).cloned().unwrap_or(0..0),
                        slots: slot..slot + 1,
                    });
                }
            }
        }

        let mut colour_slots = BTreeMap::new();
        let colours: Vec<ColourSlot> = colour_keys
            .into_iter()
            .enumerate()
            .map(|(slot, (material, colour))| {
                colour_slots.insert((material, colour), slot);
                let mut base = colour.of(materials.get(material));
                if colour.srgb() {
                    for channel in &mut base[..3] {
                        *channel = encode_srgb(*channel);
                    }
                }
                ColourSlot {
                    material,
                    colour,
                    base,
                }
            })
            .collect();

        let mut uv_slots = BTreeMap::new();
        let uvs: Vec<UvSlot> = uv_keys
            .into_iter()
            .enumerate()
            .map(|(slot, material)| {
                uv_slots.insert(material, slot);
                let transform = base_transforms.get(material).copied().flatten();
                UvSlot {
                    material,
                    scale: transform.map_or([1.0, 1.0], |transform| transform.scale),
                    offset: transform.map_or([0.0, 0.0], |transform| transform.offset),
                }
            })
            .collect();

        let definitions = binds
            .definitions
            .into_iter()
            .map(|read| Definition {
                binary: read.binary,
                blink: read.blink,
                look_at: read.look_at,
                mouth: read.mouth,
                morphs: read
                    .morphs
                    .iter()
                    .map(|&(node, target, weight)| (morph_slots[&(node, target)], weight))
                    .collect(),
                colours: read
                    .colours
                    .iter()
                    .map(|&(material, colour, target)| (colour_slots[&(material, colour)], target))
                    .collect(),
                uvs: read
                    .uvs
                    .iter()
                    .map(|&(material, scale, offset)| (uv_slots[&material], scale, offset))
                    .collect(),
                key: read.key,
            })
            .collect();

        Self {
            definitions,
            morph_targets,
            morph_nodes,
            colours,
            uvs,
        }
    }

    /// `DefaultExpressionValidator.Validate`: each expression's actual
    /// weight from the weights asked for, and the lookAt's override rate.
    /// `inputs` must be finite.
    pub(crate) fn validate(&self, inputs: &BTreeMap<ExpressionKey, f32>) -> Applied {
        let mut weights = BTreeMap::new();
        let (mut blink, mut look_at, mut mouth) = (0.0f32, 0.0f32, 0.0f32);
        // 1. Weights, and the override rates of every other expression.
        for definition in &self.definitions {
            let key = &definition.key;
            let input = inputs.get(key).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            let weight = if definition.binary {
                input.round_ties_even()
            } else {
                input
            };
            weights.insert(key.clone(), weight);
            if !key.is_blink() {
                blink += definition.blink.rate(weight);
            }
            if !key.is_look_at() {
                look_at += definition.look_at.rate(weight);
            }
            if !key.is_mouth() {
                mouth += definition.mouth.rate(weight);
            }
        }
        // 2. Saturated rates.
        let (blink, look_at, mouth) = (
            blink.clamp(0.0, 1.0),
            look_at.clamp(0.0, 1.0),
            mouth.clamp(0.0, 1.0),
        );
        // 3. Each group scaled by what the others leave it.
        for definition in &self.definitions {
            let key = &definition.key;
            let multiplier = if key.is_blink() {
                1.0 - blink
            } else if key.is_look_at() {
                1.0 - look_at
            } else if key.is_mouth() {
                1.0 - mouth
            } else {
                continue;
            };
            if let Some(weight) = weights.get_mut(key) {
                if definition.binary {
                    if multiplier < 1.0 {
                        *weight = 0.0;
                    }
                } else {
                    *weight *= multiplier;
                }
            }
        }
        Applied {
            weights,
            look_at_rate: look_at,
        }
    }

    /// `ExpressionMerger.SetValues`: writes the binds of `actual` (by key;
    /// missing keys are 0) into `morph` (by draw) and `overrides` (by
    /// material).
    pub(crate) fn apply(
        &self,
        actual: &BTreeMap<ExpressionKey, f32>,
        morph: &mut [Vec<f32>],
        overrides: &mut [MaterialOverrides],
    ) {
        let mut morphs = vec![0.0f32; self.morph_targets.len()];
        // Σ (target − base) × weight, once an expression with a weight
        // binds the slot.
        let mut colours: Vec<Option<[f32; 4]>> = vec![None; self.colours.len()];
        let mut uvs: Vec<Option<([f32; 2], [f32; 2])>> = vec![None; self.uvs.len()];

        for definition in &self.definitions {
            let weight = actual.get(&definition.key).copied().unwrap_or(0.0);
            let weight = if definition.binary {
                if weight > 0.5 { 1.0 } else { 0.0 }
            } else {
                weight
            };
            for &(slot, bind) in &definition.morphs {
                if let Some(value) = morphs.get_mut(slot) {
                    *value += bind * weight;
                }
            }
            if weight == 0.0 {
                continue;
            }
            for &(slot, target) in &definition.colours {
                let (Some(base), Some(sum)) = (self.colours.get(slot), colours.get_mut(slot))
                else {
                    continue;
                };
                let sum = sum.get_or_insert([0.0; 4]);
                for ((sum, target), base) in sum.iter_mut().zip(target).zip(base.base) {
                    *sum += (target - base) * weight;
                }
            }
            for &(slot, scale, offset) in &definition.uvs {
                let (Some(base), Some(sum)) = (self.uvs.get(slot), uvs.get_mut(slot)) else {
                    continue;
                };
                let (scales, offsets) = sum.get_or_insert(([0.0; 2], [0.0; 2]));
                for axis in 0..2 {
                    scales[axis] += (scale[axis] - base.scale[axis]) * weight;
                    offsets[axis] += (offset[axis] - base.offset[axis]) * weight;
                }
            }
        }

        // Each draw takes the node's slots below its own target count.
        for node in &self.morph_nodes {
            let (Some(targets), Some(values)) = (
                self.morph_targets.get(node.slots.clone()),
                morphs.get(node.slots.clone()),
            ) else {
                continue;
            };
            for weights in morph.get_mut(node.draws.clone()).into_iter().flatten() {
                let count = targets.partition_point(|&target| target < weights.len());
                for (&target, &value) in targets[..count].iter().zip(values) {
                    if let Some(weight) = weights.get_mut(target) {
                        *weight = value;
                    }
                }
            }
        }

        for (slot, sum) in self.colours.iter().zip(&colours) {
            let (Some(sum), Some(overrides)) = (sum, overrides.get_mut(slot.material)) else {
                continue;
            };
            let mut value: [f32; 4] =
                std::array::from_fn(|channel| slot.base[channel] + sum[channel]);
            if slot.colour.srgb() {
                for channel in &mut value[..3] {
                    *channel = decode_srgb(*channel);
                }
            }
            slot.colour.write(value, overrides);
        }

        for (slot, sum) in self.uvs.iter().zip(&uvs) {
            let (Some((scale, offset)), Some(overrides)) = (sum, overrides.get_mut(slot.material))
            else {
                continue;
            };
            overrides.texture_scale = Some([slot.scale[0] + scale[0], slot.scale[1] + scale[1]]);
            overrides.texture_offset =
                Some([slot.offset[0] + offset[0], slot.offset[1] + offset[1]]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VrmModel;
    use crate::provider::PerformanceData;
    use crate::synthetic::{SyntheticVrm, node};
    use celesta_plugin::{MaterialOverrides, NoticeLevel, SceneFrame};
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn load(vrm: &SyntheticVrm) -> VrmModel {
        let cache = TempDir::new().unwrap();
        VrmModel::load_slice(&vrm.glb(), cache.path()).unwrap()
    }

    fn avatar() -> VrmModel {
        load(&SyntheticVrm::avatar())
    }

    /// The avatar with its `VRMC_vrm.expressions` changed by `edit`.
    fn with_expressions(edit: impl FnOnce(&mut Value)) -> SyntheticVrm {
        let mut vrm = SyntheticVrm::avatar();
        edit(&mut vrm.expressions);
        vrm
    }

    fn refusal(vrm: &SyntheticVrm) -> String {
        let cache = TempDir::new().unwrap();
        match VrmModel::load_slice(&vrm.glb(), cache.path()) {
            Ok(_) => panic!("the model opened"),
            Err(error) => error.to_string(),
        }
    }

    fn data(inputs: &[(&str, f32)]) -> PerformanceData {
        PerformanceData {
            expressions: inputs
                .iter()
                .map(|&(name, weight)| (name.to_string(), weight))
                .collect(),
        }
    }

    fn frame(model: &mut VrmModel, inputs: &[(&str, f32)]) -> SceneFrame {
        model.frame(0.0, &data(inputs)).unwrap()
    }

    /// [`frame`], for tests that name a local `frame`.
    fn frame_of(model: &mut VrmModel, inputs: &[(&str, f32)]) -> SceneFrame {
        frame(model, inputs)
    }

    /// Morph target `target` of draw 0 (Body's box).
    fn target(model: &mut VrmModel, inputs: &[(&str, f32)], target: usize) -> f32 {
        frame(model, inputs).draws[0].morph_weights[target]
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 1e-5
    }

    fn close_all(actual: &[f32], expected: &[f32]) -> bool {
        actual.len() == expected.len() && actual.iter().zip(expected).all(|(a, e)| close(*a, *e))
    }

    /// Draw 0's overrides, which are material 0's.
    fn overrides(model: &mut VrmModel, inputs: &[(&str, f32)]) -> MaterialOverrides {
        frame(model, inputs).draws[0].overrides
    }

    #[test]
    fn blink_alone_moves_its_target() {
        let mut model = avatar();
        let frame = frame(&mut model, &[("blink", 1.0)]);
        assert_eq!(frame.draws[0].morph_weights[0], 0.5);
        // Every draw of the bound node: the hair is Body's too.
        assert_eq!(frame.draws[1].morph_weights[0], 0.5);
    }

    #[test]
    fn block_override_turns_blink_off() {
        let mut model = avatar();
        assert!(close(
            target(&mut model, &[("happy", 1.0), ("blink", 1.0)], 0),
            1.0
        ));
        assert!(close(
            target(&mut model, &[("happy", 0.3), ("blink", 1.0)], 0),
            0.3
        ));
        // Block is all or nothing: any weight above 0 blocks.
        assert!(close(
            target(&mut model, &[("happy", 0.01), ("blink", 1.0)], 0),
            0.01
        ));
    }

    #[test]
    fn blend_override_scales_blink() {
        let mut model = load(&with_expressions(|e| {
            e["preset"]["happy"]["overrideBlink"] = json!("blend");
        }));
        let weight = target(&mut model, &[("happy", 0.3), ("blink", 1.0)], 0);
        assert!(close(weight, 0.3 + 0.5 * 0.7), "{weight}");
    }

    #[test]
    fn binary_expressions_round_half_to_even() {
        let mut model = avatar();
        assert_eq!(target(&mut model, &[("wink", 0.5)], 0), 0.0);
        assert_eq!(target(&mut model, &[("wink", 0.51)], 0), 1.0);
        assert_eq!(target(&mut model, &[("wink", 0.49)], 0), 0.0);

        // A binary blink is off once anything overrides it at all, where a
        // blended one would be scaled (0.3 + 0.5 × 0.7).
        let mut model = load(&with_expressions(|e| {
            e["preset"]["blink"]["isBinary"] = json!(true);
        }));
        assert!(close(
            target(&mut model, &[("happy", 0.3), ("blink", 1.0)], 0),
            0.3
        ));
        let mut model = load(&with_expressions(|e| {
            e["preset"]["blink"]["isBinary"] = json!(true);
            e["preset"]["happy"]["overrideBlink"] = json!("blend");
        }));
        assert!(close(
            target(&mut model, &[("happy", 0.3), ("blink", 1.0)], 0),
            0.3
        ));
        assert!(close(target(&mut model, &[("blink", 1.0)], 0), 0.5));
    }

    #[test]
    fn mouth_override_scales_vowels() {
        let mut model = avatar();
        let overrides = overrides(&mut model, &[("aa", 1.0), ("happy", 0.4)]);
        // aa at 0.6: 0.5 + (0.25 − 0.5) × 0.6.
        assert!(close_all(&overrides.texture_offset.unwrap(), &[0.35, 0.0]));
    }

    #[test]
    fn inputs_are_clamped() {
        let mut model = avatar();
        assert_eq!(
            frame(&mut model, &[("happy", 2.0)]),
            frame(&mut model, &[("happy", 1.0)])
        );
        assert_eq!(
            frame(&mut model, &[("happy", -1.0)]),
            frame(&mut model, &[])
        );
        // Names the avatar does not have, presets included, change nothing.
        assert_eq!(frame(&mut model, &[("nope", 1.0)]), frame(&mut model, &[]));
        assert_eq!(frame(&mut model, &[("sad", 1.0)]), frame(&mut model, &[]));
        assert_eq!(frame(&mut model, &[("Happy", 1.0)]), frame(&mut model, &[]));
        for value in [f32::NAN, f32::INFINITY] {
            let message = match model.frame(0.0, &data(&[("happy", value)])) {
                Ok(_) => panic!("the frame was made"),
                Err(error) => error.to_string(),
            };
            assert!(message.contains("happy"), "{message}");
            assert!(message.contains("not finite"), "{message}");
        }
    }

    /// sRGB 0.5 decoded to linear.
    const HALF_SRGB: f32 = 0.2140411;

    #[test]
    fn colour_binds_blend_in_srgb() {
        let mut model = avatar();
        let overrides = overrides(&mut model, &[("happy", 0.5)]);
        assert!(close_all(
            &overrides.base_color.unwrap(),
            &[1.0, HALF_SRGB, HALF_SRGB, 1.0]
        ));
    }

    #[test]
    fn emission_binds_blend_linearly() {
        let mut model = avatar();
        let overrides = overrides(&mut model, &[("happy", 0.5)]);
        assert!(close_all(
            &overrides.emission_color.unwrap(),
            &[0.5, 0.5, 0.0]
        ));
    }

    #[test]
    fn every_colour_type_reaches_its_field() {
        // MToon's defaults are the base: every factor's target is sRGB 0.5,
        // reached in full at weight 1.
        let binds: Vec<Value> = ["shadeColor", "matcapColor", "rimColor", "outlineColor"]
            .iter()
            .map(|kind| json!({ "material": 0, "type": kind, "targetValue": [0.5, 0.5, 0.5, 0.0] }))
            .collect();
        let mut model = load(&with_expressions(|e| {
            e["preset"]["sad"] = json!({ "materialColorBinds": binds });
        }));
        let overrides = overrides(&mut model, &[("sad", 1.0)]);
        let half = [HALF_SRGB; 3];
        assert!(close_all(&overrides.shade_color.unwrap(), &half));
        assert!(close_all(&overrides.matcap_color.unwrap(), &half));
        assert!(close_all(&overrides.rim_color.unwrap(), &half));
        assert!(close_all(&overrides.outline_color.unwrap(), &half));
        assert_eq!(overrides.base_color, None);
        assert_eq!(overrides.emission_color, None);
        assert_eq!(overrides.texture_offset, None);
    }

    #[test]
    fn colour_binds_of_several_expressions_add_up() {
        // happy and sad both move the base colour's red from sRGB 1.
        let mut model = load(&with_expressions(|e| {
            e["preset"]["sad"] = json!({
                "materialColorBinds": [
                    { "material": 0, "type": "color", "targetValue": [0.0, 1.0, 1.0, 0.0] },
                ],
            });
        }));
        let colour = overrides(&mut model, &[("happy", 0.5), ("sad", 0.5)])
            .base_color
            .unwrap();
        // sRGB: red 1 − 0.5, green and blue 1 − 0.5, alpha 1 − 0.5.
        assert!(close_all(&colour, &[HALF_SRGB, HALF_SRGB, HALF_SRGB, 0.5]));
    }

    #[test]
    fn uv_binds_blend_from_the_material_transform() {
        let mut model = avatar();
        let overrides = overrides(&mut model, &[("aa", 0.5)]);
        assert!(close_all(&overrides.texture_offset.unwrap(), &[0.375, 0.0]));
        assert!(close_all(&overrides.texture_scale.unwrap(), &[1.0, 1.0]));
        assert_eq!(overrides.base_color, None);

        let frame = frame(&mut model, &[]);
        for draw in &frame.draws {
            assert_eq!(draw.overrides, MaterialOverrides::default());
        }
        // Expressions at 0 leave the materials alone too.
        let frame = frame_of(&mut model, &[("happy", 0.0), ("aa", 0.0)]);
        for draw in &frame.draws {
            assert_eq!(draw.overrides, MaterialOverrides::default());
        }
    }

    #[test]
    fn uv_binds_without_a_transform_start_from_the_identity() {
        // Hair (material 1) has no base colour texture; its scale and
        // offset start from (1, 1) and (0, 0). A bind without a scale
        // keeps (1, 1).
        let mut model = load(&with_expressions(|e| {
            e["preset"]["ih"] = json!({
                "textureTransformBinds": [{ "material": 1, "offset": [0.5, 0.25] }],
            });
            e["preset"]["ou"] = json!({
                "textureTransformBinds": [{ "material": 1, "scale": [3.0, 1.0] }],
            });
        }));
        let frame = frame(&mut model, &[("ih", 0.5), ("ou", 0.5)]);
        let hair = frame.draws[1].overrides;
        assert!(close_all(&hair.texture_offset.unwrap(), &[0.25, 0.125]));
        assert!(close_all(&hair.texture_scale.unwrap(), &[2.0, 1.0]));
        assert_eq!(frame.draws[0].overrides, MaterialOverrides::default());
    }

    #[test]
    fn overrides_reach_only_draws_of_the_material() {
        let mut model = avatar();
        let frame = frame(&mut model, &[("happy", 0.5)]);
        assert!(frame.draws[0].overrides.base_color.is_some());
        assert_eq!(frame.draws[1].overrides, MaterialOverrides::default());
        assert_eq!(frame.draws[2].overrides, MaterialOverrides::default());

        // With the hair on material 0 too, both get them.
        let vrm = SyntheticVrm::avatar();
        let cache = TempDir::new().unwrap();
        let glb = vrm.glb_with(|root| root["meshes"][0]["primitives"][1]["material"] = json!(0));
        let mut model = VrmModel::load_slice(&glb, cache.path()).unwrap();
        let frame = frame_of(&mut model, &[("happy", 0.5)]);
        assert_eq!(frame.draws[0].overrides, frame.draws[1].overrides);
        assert!(frame.draws[1].overrides.base_color.is_some());
    }

    #[test]
    fn unbound_targets_keep_their_default() {
        let mut model = avatar();
        let frame = frame(&mut model, &[("happy", 1.0)]);
        assert_eq!(frame.draws[0].morph_weights, [1.0, 0.25, 0.0]);
    }

    #[test]
    fn bound_targets_start_from_zero() {
        // The node's weights give target 0 a default of 1; an expression
        // binds it, so it is the expressions' sum, 0 with none.
        let vrm = SyntheticVrm::avatar();
        let cache = TempDir::new().unwrap();
        let glb =
            vrm.glb_with(|root| root["nodes"][node::BODY]["weights"] = json!([1.0, 1.0, 1.0]));
        let mut model = VrmModel::load_slice(&glb, cache.path()).unwrap();
        assert_eq!(
            frame(&mut model, &[]).draws[0].morph_weights,
            [0.0, 1.0, 0.0]
        );
        assert_eq!(
            frame(&mut model, &[("blink", 1.0)]).draws[0].morph_weights,
            [0.5, 1.0, 0.0]
        );
    }

    #[test]
    fn look_at_override_scales_look_keys() {
        let mut model = load(&with_expressions(|e| {
            e["preset"]["happy"]["overrideLookAt"] = json!("blend");
        }));
        assert!(close(
            target(&mut model, &[("happy", 0.5), ("lookLeft", 1.0)], 2),
            0.5
        ));
        let inputs: BTreeMap<ExpressionKey, f32> = [
            (ExpressionKey::parse("happy"), 0.5),
            (ExpressionKey::parse("lookLeft"), 1.0),
        ]
        .into();
        let applied = model.expressions().validate(&inputs);
        assert!(close(applied.look_at_rate, 0.5));
        // Every key the avatar has, 0 when not asked for.
        let keys: Vec<ExpressionKey> = applied.weights.keys().cloned().collect();
        assert_eq!(
            keys,
            ["happy", "aa", "blink", "lookLeft", "wink"].map(ExpressionKey::parse)
        );
        assert_eq!(applied.weights[&ExpressionKey::parse("blink")], 0.0);
        assert!(close(
            applied.weights[&ExpressionKey::parse("lookLeft")],
            0.5
        ));
    }

    #[test]
    fn override_rates_saturate() {
        // Two blends at 0.6 block blink fully, not past it.
        let mut model = load(&with_expressions(|e| {
            e["preset"]["happy"]["overrideBlink"] = json!("blend");
            e["preset"]["sad"] = json!({ "overrideBlink": "blend" });
        }));
        let inputs: BTreeMap<ExpressionKey, f32> = [
            (ExpressionKey::parse("happy"), 0.6),
            (ExpressionKey::parse("sad"), 0.6),
            (ExpressionKey::parse("blink"), 1.0),
        ]
        .into();
        let applied = model.expressions().validate(&inputs);
        assert_eq!(applied.weights[&ExpressionKey::parse("blink")], 0.0);
        assert!(close(
            target(
                &mut model,
                &[("happy", 0.6), ("sad", 0.6), ("blink", 1.0)],
                0
            ),
            0.6
        ));
    }

    #[test]
    fn a_group_does_not_override_itself() {
        // blinkLeft blocking blinks does not block itself or blink.
        let mut model = load(&with_expressions(|e| {
            e["preset"]["blinkLeft"] = json!({
                "overrideBlink": "block",
                "morphTargetBinds": [{ "node": node::BODY, "index": 1, "weight": 1.0 }],
            });
        }));
        let weights = frame(&mut model, &[("blinkLeft", 1.0), ("blink", 1.0)]).draws[0]
            .morph_weights
            .clone();
        assert_eq!(weights, [0.5, 1.0, 0.0]);
    }

    #[test]
    fn keys_follow_the_preset_order_then_custom_names() {
        let names = [
            "happy",
            "angry",
            "sad",
            "relaxed",
            "surprised",
            "aa",
            "ih",
            "ou",
            "ee",
            "oh",
            "blink",
            "blinkLeft",
            "blinkRight",
            "lookUp",
            "lookDown",
            "lookLeft",
            "lookRight",
            "neutral",
            "Zed",
            "alpha",
            "beta",
        ];
        let keys: Vec<ExpressionKey> = names
            .iter()
            .map(|name| ExpressionKey::parse(name))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(sorted, keys);
        for (index, key) in keys.iter().enumerate() {
            assert_eq!(key.name(), names[index]);
            assert_eq!(
                matches!(key, ExpressionKey::Custom(_)),
                index >= 18,
                "{key:?}"
            );
            assert_eq!(key.is_blink(), (10..13).contains(&index), "{key:?}");
            assert_eq!(key.is_look_at(), (13..17).contains(&index), "{key:?}");
            assert_eq!(key.is_mouth(), (5..10).contains(&index), "{key:?}");
        }
        assert_eq!(
            ExpressionKey::parse("Happy"),
            ExpressionKey::Custom("Happy".into())
        );
    }

    #[test]
    fn a_custom_expression_named_like_a_preset_is_left_out() {
        let mut model = load(&with_expressions(|e| {
            e["custom"]["happy"] = json!({
                "morphTargetBinds": [{ "node": node::BODY, "index": 1, "weight": 1.0 }],
            });
            e["custom"]["sad"] = json!({
                "morphTargetBinds": [{ "node": node::BODY, "index": 1, "weight": 1.0 }],
            });
        }));
        assert_eq!(target(&mut model, &[("happy", 1.0), ("sad", 1.0)], 1), 0.25);
        let warnings: Vec<&str> = model
            .notices()
            .iter()
            .filter(|notice| notice.level == NoticeLevel::Warning)
            .map(|notice| notice.message.as_str())
            .collect();
        assert_eq!(
            warnings,
            [
                "custom expression \"happy\" has a preset's name, so it is left out",
                "custom expression \"sad\" has a preset's name, so it is left out",
            ]
        );
    }

    #[test]
    fn bind_indices_are_checked() {
        let message = refusal(&with_expressions(|e| {
            e["preset"]["blink"]["morphTargetBinds"][0]["node"] = json!(99);
        }));
        assert!(
            message.contains("expression \"blink\" binds node 99, but the file has 25 nodes"),
            "{message}"
        );
        let message = refusal(&with_expressions(|e| {
            e["preset"]["happy"]["materialColorBinds"][1]["material"] = json!(4);
        }));
        assert!(
            message.contains("expression \"happy\" binds material 4, but the file has 4 materials"),
            "{message}"
        );
        let message = refusal(&with_expressions(|e| {
            e["custom"]["wink"]["textureTransformBinds"] =
                json!([{ "material": 9, "offset": [0.0, 0.0] }]);
        }));
        assert!(
            message.contains("expression \"wink\" binds material 9, but the file has 4 materials"),
            "{message}"
        );
    }

    #[test]
    fn binds_to_targets_a_mesh_lacks_are_ignored() {
        let mut model = load(&with_expressions(|e| {
            e["preset"]["blink"]["morphTargetBinds"] = json!([
                { "node": node::BODY, "index": 3, "weight": 1.0 },
                { "node": node::HEAD, "index": 0, "weight": 1.0 },
                { "node": node::BODY, "index": 1, "weight": 1.0 },
            ]);
        }));
        let frame = frame(&mut model, &[("blink", 1.0)]);
        assert_eq!(frame.draws[0].morph_weights, [0.0, 1.0, 0.0]);
        let warnings: Vec<&str> = model
            .notices()
            .iter()
            .filter(|notice| notice.level == NoticeLevel::Warning)
            .map(|notice| notice.message.as_str())
            .collect();
        assert_eq!(
            warnings,
            [
                "expression \"blink\" binds morph target 3 of node 23, whose mesh has 3; the bind is ignored",
                "expression \"blink\" binds morph target 0 of node 5, which has no mesh; the bind is ignored",
            ]
        );
    }

    #[test]
    fn draws_with_fewer_targets_skip_the_bind() {
        // The hair (draw 1) has no targets of its own; the box keeps three.
        let vrm = SyntheticVrm::avatar();
        let cache = TempDir::new().unwrap();
        let glb = vrm.glb_with(|root| {
            root["meshes"][0]["primitives"][1]
                .as_object_mut()
                .unwrap()
                .remove("targets");
        });
        let mut model = VrmModel::load_slice(&glb, cache.path()).unwrap();
        let frame = frame(&mut model, &[("blink", 1.0), ("lookLeft", 1.0)]);
        assert_eq!(frame.draws[0].morph_weights, [0.5, 0.25, 1.0]);
        assert!(frame.draws[1].morph_weights.is_empty());
    }

    #[test]
    fn bind_values_are_checked() {
        type Edit = fn(&mut Value);
        let cases: [(Edit, &str); 5] = [
            (
                |e| e["preset"]["blink"]["morphTargetBinds"][0]["weight"] = json!(1e39),
                "expression \"blink\" has a morph target bind whose weight is not finite",
            ),
            (
                |e| {
                    e["preset"]["happy"]["materialColorBinds"][0]["targetValue"] =
                        json!([1.0, 0.0, 0.0])
                },
                "expression \"happy\" has a materialColorBind whose targetValue is not four numbers: [1.0, 0.0, 0.0]",
            ),
            (
                |e| {
                    e["preset"]["happy"]["materialColorBinds"][0]["targetValue"] =
                        json!([1e39, 0.0, 0.0, 1.0])
                },
                "expression \"happy\" has a materialColorBind whose targetValue is not finite",
            ),
            (
                |e| e["preset"]["aa"]["textureTransformBinds"][0]["offset"] = json!([0.25]),
                "expression \"aa\" has a textureTransformBind whose offset is not two numbers: [0.25]",
            ),
            (
                |e| e["preset"]["aa"]["textureTransformBinds"][0]["scale"] = json!([1e39, 1.0]),
                "expression \"aa\" has a textureTransformBind whose scale is not finite",
            ),
        ];
        for (edit, expected) in cases {
            let message = refusal(&with_expressions(edit));
            assert!(message.contains(expected), "{message}");
        }
    }

    #[test]
    fn sums_past_f32_are_a_frame_error() {
        let mut model = load(&with_expressions(|e| {
            e["preset"]["blink"]["morphTargetBinds"] = json!([
                { "node": node::BODY, "index": 0, "weight": 3e38 },
                { "node": node::BODY, "index": 0, "weight": 3e38 },
            ]);
            e["preset"]["sad"] = json!({
                "textureTransformBinds": [{ "material": 0, "offset": [3e38, 0.0] }],
            });
            e["preset"]["angry"] = json!({
                "textureTransformBinds": [{ "material": 0, "offset": [3e38, 0.0] }],
            });
        }));
        frame(&mut model, &[("blink", 0.5)]);
        for inputs in [&[("blink", 1.0)][..], &[("sad", 1.0), ("angry", 1.0)]] {
            let message = match model.frame(0.0, &data(inputs)) {
                Ok(_) => panic!("the frame was made"),
                Err(error) => error.to_string(),
            };
            assert!(message.contains("not finite"), "{message}");
        }
    }

    #[test]
    fn expression_frames_repeat_exactly() {
        let mut model = avatar();
        let inputs = [("happy", 0.4), ("aa", 0.7), ("blink", 1.0), ("wink", 0.8)];
        assert_eq!(frame(&mut model, &inputs), frame(&mut model, &inputs));
    }
}
