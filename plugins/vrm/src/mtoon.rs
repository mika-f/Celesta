// Ported from UniVRM (https://github.com/vrm-c/UniVRM), under
// Packages/VRM10/: MToon10/Runtime/UnityShaderLab/Properties/MToon10Properties.cs
// and Runtime/IO/Material/BuiltInRP/Import/Materials/BuiltInVrm10MToonMaterialImporter.cs
// (the defaults), MToon10/Runtime/MToonValidator.cs (what reaches the
// shader), and the shader's parameters in MToon10/Shaders/.
// MIT License, Copyright (c) 2020 VRM Consortium, Copyright (c) 2018 Masataka
// SUMI for MToon.

//! MToon, the plugin's own scene material: VRM 1.0's toon shading
//! (`VRMC_materials_mtoon`) as UniVRM's MToon10 shader draws it, written in
//! WGSL against Celesta's material interface (`mtoon.wgsl`).
//!
//! A model that uses it sends [`shader`] once, as its shader 0, and each
//! MToon material as a custom material on it ([`custom_material`]): its
//! typed parameters ([`Mtoon`]) packed into the shader's `Params`
//! ([`params`]) and its textures in the shader's nine slots ([`slots`]). A
//! frame sends a draw's parameters again when the UV animation has moved
//! or an expression binds one of its colours ([`MtoonOverrides`]).
//!
//! Celesta sees only bytes, so the plugin checks what it packs: every
//! texture a slot names is one of the model's, and every number is finite
//! (`invalid`).

use celesta_plugin::{
    CustomMaterial, EmptyTexel, OutlinePass, SceneShader, SceneTexture, ShaderTexture, TextureRef,
    Wrap,
};

/// The bytes of `mtoon.wgsl`'s `Params`: eleven `vec4f`s.
pub const MTOON_PARAMS: usize = 176;

/// VRM 1.0's MToon parameters, as UniVRM's MToon10 shader takes them. A
/// field the file leaves out takes UniVRM's default, given on each field
/// ([`Mtoon::default`]).
///
/// Every texture but the matcap and the outline width is sampled at the
/// base color texture's coordinates: `base_color_texture`'s `transform`,
/// and a frame's texture offset and scale binds, move them all, and then
/// the UV animation does, as UniVRM's single `_MainTex_ST` does. That is
/// why the other slots are [`TextureSlot`]s without a transform. The
/// outline width texture is sampled at the raw texture coordinates, and the
/// matcap at its own, from the view-space normal.
///
/// `shading_shift_texture` is read from its red channel,
/// `outline_width_multiply_texture` from its green, and
/// `uv_animation_mask_texture` from its blue; the others' RGB is used. Each
/// texture's color space is the scene texture's: sRGB for the base color,
/// shade multiply, emissive, matcap, and rim multiply textures; linear for
/// the normal, shading shift, outline width, and UV animation mask
/// textures.
///
/// `transparentWithZWrite` and `renderQueueOffsetNumber` are not here: they
/// are the material's `AlphaMode::Blend` fields, which Celesta draws for
/// every material.
#[derive(Clone, Debug, PartialEq)]
pub struct Mtoon {
    /// Linear RGBA. Default `[1, 1, 1, 1]`.
    pub base_color: [f32; 4],
    /// Its `transform` moves every texture but the matcap and the outline
    /// width.
    pub base_color_texture: Option<TextureRef>,
    /// A tangent-space normal map; `scale` is glTF's `normalTexture.scale`.
    pub normal_texture: Option<ScaledTextureSlot>,
    /// Linear RGB, which may exceed 1: the file's
    /// `KHR_materials_emissive_strength` is multiplied in. Default
    /// `[0, 0, 0]`.
    pub emissive_factor: [f32; 3],
    pub emissive_texture: Option<TextureSlot>,
    /// The color of the shaded side, linear RGB. Default `[1, 1, 1]`.
    pub shade_color_factor: [f32; 3],
    pub shade_multiply_texture: Option<TextureSlot>,
    /// Shifts where light turns to shade. Default `-0.05`.
    pub shading_shift_factor: f32,
    /// Red channel, times `scale`, added to the shift.
    pub shading_shift_texture: Option<ScaledTextureSlot>,
    /// How sharp the light-to-shade edge is, 0 smooth to 1 hard; above 1
    /// the ramp turns around, as in UniVRM. Default `0.95`.
    pub shading_toony_factor: f32,
    /// How far ambient light is flattened to its average. Default `0.9`.
    pub gi_equalization_factor: f32,
    /// Linear RGB. Default `[0, 0, 0]`.
    pub matcap_factor: [f32; 3],
    pub matcap_texture: Option<TextureSlot>,
    /// Linear RGB. Default `[0, 0, 0]`.
    pub parametric_rim_color_factor: [f32; 3],
    pub rim_multiply_texture: Option<TextureSlot>,
    /// How far the rim is lit by the scene's light. Default `1`.
    pub rim_lighting_mix_factor: f32,
    /// Default `5`.
    pub parametric_rim_fresnel_power_factor: f32,
    /// Default `0`.
    pub parametric_rim_lift_factor: f32,
    /// Default `OutlineWidthMode::None`.
    pub outline_width_mode: OutlineWidthMode,
    /// The outline's width, in `outline_width_mode`'s units (metres for
    /// world coordinates). Default `0`.
    pub outline_width_factor: f32,
    /// Green channel, at the raw texture coordinates.
    pub outline_width_multiply_texture: Option<TextureSlot>,
    /// Linear RGB. Default `[0, 0, 0]`.
    pub outline_color_factor: [f32; 3],
    /// How far the outline is lit by the scene's light. Default `1`.
    pub outline_lighting_mix_factor: f32,
    /// Blue channel: how much the UV animation moves each texel.
    pub uv_animation_mask_texture: Option<TextureSlot>,
    /// Texture coordinate units a second. Default `0`.
    pub uv_animation_scroll_x_speed_factor: f32,
    /// Texture coordinate units a second. Default `0`.
    pub uv_animation_scroll_y_speed_factor: f32,
    /// Radians a second. Default `0`.
    pub uv_animation_rotation_speed_factor: f32,
}

impl Default for Mtoon {
    fn default() -> Self {
        Self {
            base_color: [1.0; 4],
            base_color_texture: None,
            normal_texture: None,
            emissive_factor: [0.0; 3],
            emissive_texture: None,
            shade_color_factor: [1.0; 3],
            shade_multiply_texture: None,
            shading_shift_factor: -0.05,
            shading_shift_texture: None,
            shading_toony_factor: 0.95,
            gi_equalization_factor: 0.9,
            matcap_factor: [0.0; 3],
            matcap_texture: None,
            parametric_rim_color_factor: [0.0; 3],
            rim_multiply_texture: None,
            rim_lighting_mix_factor: 1.0,
            parametric_rim_fresnel_power_factor: 5.0,
            parametric_rim_lift_factor: 0.0,
            outline_width_mode: OutlineWidthMode::None,
            outline_width_factor: 0.0,
            outline_width_multiply_texture: None,
            outline_color_factor: [0.0; 3],
            outline_lighting_mix_factor: 1.0,
            uv_animation_mask_texture: None,
            uv_animation_scroll_x_speed_factor: 0.0,
            uv_animation_scroll_y_speed_factor: 0.0,
            uv_animation_rotation_speed_factor: 0.0,
        }
    }
}

/// What an MToon outline's width is measured in, if it has one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum OutlineWidthMode {
    /// No outline is drawn.
    #[default]
    None,
    WorldCoordinates,
    ScreenCoordinates,
}

/// A scene texture in an MToon slot, sampled at the base color texture's
/// coordinates, except the matcap and the outline width (see [`Mtoon`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureSlot {
    pub texture: usize,
}

/// A [`TextureSlot`] whose values are multiplied by `scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaledTextureSlot {
    pub texture: usize,
    pub scale: f32,
}

/// One frame's values in place of an MToon material's, as expressions'
/// `materialColorBind` and `textureTransformBind` set them. By default none
/// are.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MtoonOverrides {
    /// Replaces `Mtoon::base_color`.
    pub base_color: Option<[f32; 4]>,
    /// Replaces the base color texture's transform's offset.
    pub texture_offset: Option<[f32; 2]>,
    /// Replaces the base color texture's transform's scale.
    pub texture_scale: Option<[f32; 2]>,
    /// Replaces `Mtoon::shade_color_factor`.
    pub shade_color: Option<[f32; 3]>,
    /// Replaces `Mtoon::emissive_factor`.
    pub emission_color: Option<[f32; 3]>,
    /// Replaces `Mtoon::matcap_factor`.
    pub matcap_color: Option<[f32; 3]>,
    /// Replaces `Mtoon::parametric_rim_color_factor`.
    pub rim_color: Option<[f32; 3]>,
    /// Replaces `Mtoon::outline_color_factor`.
    pub outline_color: Option<[f32; 3]>,
}

/// One of the shader's texture slots.
struct Slot {
    /// Its `VRMC_materials_mtoon` (or glTF) name.
    name: &'static str,
    /// The texel bound when it is empty: Unity's default for the property.
    empty: EmptyTexel,
    /// Whether `mtoon.wgsl` samples it at the UV-animated coordinates; the
    /// matcap, the outline width, and the mask itself have their own.
    animated: bool,
}

const fn slot(name: &'static str, empty: EmptyTexel, animated: bool) -> Slot {
    Slot {
        name,
        empty,
        animated,
    }
}

/// The shader's texture slots, in `mtoon.wgsl`'s order: slot `k` is its
/// `texture_k` and `sampler_k`. An empty slot's texel leaves its term as
/// UniVRM's keyword gates do without the texture: no shift but the
/// factor's, no matcap, the whole of the UV animation, the width factor
/// alone. An empty normal slot is sampled but has no effect: the shader
/// keeps the geometric normal.
const SLOTS: [Slot; 9] = [
    slot("baseColorTexture", EmptyTexel::White, true),
    slot("shadeMultiplyTexture", EmptyTexel::White, true),
    slot("shadingShiftTexture", EmptyTexel::Black, true),
    slot("normalTexture", EmptyTexel::White, true),
    slot("emissiveTexture", EmptyTexel::White, true),
    slot("matcapTexture", EmptyTexel::Black, false),
    slot("rimMultiplyTexture", EmptyTexel::White, true),
    slot("outlineWidthMultiplyTexture", EmptyTexel::White, false),
    slot("uvAnimationMaskTexture", EmptyTexel::White, false),
];

/// The MToon shader, of nine texture slots named as `VRMC_materials_mtoon`
/// names its textures.
pub fn shader() -> SceneShader {
    SceneShader {
        name: Some("MToon".into()),
        wgsl: include_str!("mtoon.wgsl").into(),
        textures: SLOTS
            .iter()
            .map(|slot| ShaderTexture {
                name: Some(slot.name.into()),
                empty: slot.empty,
            })
            .collect(),
    }
}

/// The scene textures `mtoon`'s slots name, in the shader's order.
pub fn slots(mtoon: &Mtoon) -> [Option<usize>; 9] {
    [
        mtoon.base_color_texture.as_ref().map(|slot| slot.texture),
        mtoon.shade_multiply_texture.map(|slot| slot.texture),
        mtoon.shading_shift_texture.map(|slot| slot.texture),
        mtoon.normal_texture.map(|slot| slot.texture),
        mtoon.emissive_texture.map(|slot| slot.texture),
        mtoon.matcap_texture.map(|slot| slot.texture),
        mtoon.rim_multiply_texture.map(|slot| slot.texture),
        mtoon
            .outline_width_multiply_texture
            .map(|slot| slot.texture),
        mtoon.uv_animation_mask_texture.map(|slot| slot.texture),
    ]
}

/// `mtoon` as a custom material on the model's MToon shader, `shader`, its
/// parameters packed at time 0 without binds: an outline pass (Unity's
/// `Cull Front` and `Offset 1, 1`) unless its outline width mode is `none`,
/// whatever its width, as UniVRM draws it; tangents when it has a normal
/// texture. `textures` are the model's.
pub fn custom_material(mtoon: &Mtoon, shader: usize, textures: &[SceneTexture]) -> CustomMaterial {
    CustomMaterial {
        shader,
        params: params(mtoon, &MtoonOverrides::default(), 0.0, textures).to_vec(),
        textures: slots(mtoon).to_vec(),
        outline: (mtoon.outline_width_mode != OutlineWidthMode::None).then(OutlinePass::default),
        requires_tangents: mtoon.normal_texture.is_some(),
    }
}

/// Every number of `mtoon`, its texture slots' scales and base color
/// texture transform included.
fn numbers(mtoon: &Mtoon) -> Vec<f32> {
    let mut numbers = mtoon.base_color.to_vec();
    numbers.extend(mtoon.emissive_factor);
    numbers.extend(mtoon.shade_color_factor);
    numbers.extend(mtoon.matcap_factor);
    numbers.extend(mtoon.parametric_rim_color_factor);
    numbers.extend(mtoon.outline_color_factor);
    numbers.extend([
        mtoon.shading_shift_factor,
        mtoon.shading_toony_factor,
        mtoon.gi_equalization_factor,
        mtoon.rim_lighting_mix_factor,
        mtoon.parametric_rim_fresnel_power_factor,
        mtoon.parametric_rim_lift_factor,
        mtoon.outline_width_factor,
        mtoon.outline_lighting_mix_factor,
        mtoon.uv_animation_scroll_x_speed_factor,
        mtoon.uv_animation_scroll_y_speed_factor,
        mtoon.uv_animation_rotation_speed_factor,
    ]);
    numbers.extend(mtoon.normal_texture.map(|slot| slot.scale));
    numbers.extend(mtoon.shading_shift_texture.map(|slot| slot.scale));
    if let Some(transform) = mtoon
        .base_color_texture
        .as_ref()
        .and_then(|texture| texture.transform)
    {
        numbers.extend(transform.offset);
        numbers.extend(transform.scale);
        numbers.push(transform.rotation);
    }
    numbers
}

/// Why `mtoon` cannot be packed for a model of `textures` scene textures: a
/// slot naming a texture past them, or a number that is not finite.
/// Celesta sees only the packed bytes, so it cannot tell.
pub(crate) fn invalid(mtoon: &Mtoon, textures: usize) -> Option<String> {
    for (texture, slot) in slots(mtoon).into_iter().zip(&SLOTS) {
        if let Some(texture) = texture
            && texture >= textures
        {
            return Some(format!(
                "{} uses texture {texture}, of {textures}",
                slot.name
            ));
        }
    }
    if !numbers(mtoon).iter().all(|number| number.is_finite()) {
        return Some("its MToon parameters are not finite".into());
    }
    None
}

/// The rows of `texture`'s transform, which scales, then rotates, then
/// offsets coordinates as `KHR_texture_transform`'s sample code does, with
/// the offset and scale `overrides` names in place of the texture's. A
/// rotation turns `(1, 0)` toward `(0, 1)`.
fn texture_rows(texture: Option<&TextureRef>, overrides: &MtoonOverrides) -> ([f32; 4], [f32; 4]) {
    let transform = texture.and_then(|texture| texture.transform);
    let (offset, rotation, scale) = transform.map_or(([0.0; 2], 0.0, [1.0; 2]), |transform| {
        (transform.offset, transform.rotation, transform.scale)
    });
    let offset = overrides.texture_offset.unwrap_or(offset);
    let scale = overrides.texture_scale.unwrap_or(scale);
    let (sin, cos) = rotation.sin_cos();
    (
        [cos * scale[0], -sin * scale[1], offset[0], 0.0],
        [sin * scale[0], cos * scale[1], offset[1], 0.0],
    )
}

/// How far `mtoon`'s UV animation has scrolled at `time`, before its mask
/// scales it: the time times the scroll speed, formed in f64 so that a
/// long video keeps the fraction of a texture f32 would round away. Whole
/// textures are dropped, two at a time for a mirrored repeat's period,
/// along an axis where they cannot show: where every texture the animated
/// coordinates sample repeats, and only when the animation neither turns
/// (a turn that is not a quarter one takes a whole-texture step off the
/// texel grid) nor has a mask (the mask scales the dropped textures by a
/// fraction). A slot naming a texture `textures` lacks counts as clamped.
fn uv_scroll(mtoon: &Mtoon, textures: &[SceneTexture], time: f64) -> [f32; 2] {
    let animated = slots(mtoon)
        .into_iter()
        .zip(&SLOTS)
        .filter_map(|(texture, slot)| texture.filter(|_| slot.animated));
    let still = mtoon.uv_animation_rotation_speed_factor == 0.0
        && mtoon.uv_animation_mask_texture.is_none();
    let scroll = |speed: f32, wrap: fn(&celesta_plugin::Sampler) -> Wrap| {
        let offset = time * f64::from(speed);
        let repeats = animated.clone().all(|texture| {
            textures
                .get(texture)
                .is_some_and(|texture| wrap(&texture.sampler) != Wrap::ClampToEdge)
        });
        let offset = if still && repeats {
            offset.rem_euclid(2.0)
        } else {
            offset
        };
        // A still scroll at a negative time is −0.0; `+ 0.0` makes it +0.0,
        // so the bytes match the material's own and are not sent.
        (offset + 0.0) as f32
    };
    [
        scroll(mtoon.uv_animation_scroll_x_speed_factor, |sampler| {
            sampler.wrap_s
        }),
        scroll(mtoon.uv_animation_scroll_y_speed_factor, |sampler| {
            sampler.wrap_t
        }),
    ]
}

/// `mtoon.wgsl`'s `Params` for `mtoon` at `time` seconds, little-endian:
/// its base color and texture transform, its factors, the values
/// `overrides` names in place of the material's, and the UV animation's
/// scroll, which reads `textures`' samplers.
pub fn params(
    mtoon: &Mtoon,
    overrides: &MtoonOverrides,
    time: f64,
    textures: &[SceneTexture],
) -> [u8; MTOON_PARAMS] {
    let with = |[r, g, b]: [f32; 3], w: f32| [r, g, b, w];
    let (uv_u, uv_v) = texture_rows(mtoon.base_color_texture.as_ref(), overrides);
    let scroll = uv_scroll(mtoon, textures, time);
    let rows: [[f32; 4]; MTOON_PARAMS / 16] = [
        overrides.base_color.unwrap_or(mtoon.base_color),
        uv_u,
        uv_v,
        // The shade color, then the shading shift factor.
        with(
            overrides.shade_color.unwrap_or(mtoon.shade_color_factor),
            mtoon.shading_shift_factor,
        ),
        // The emissive factor, then the shading shift texture's scale.
        with(
            overrides.emission_color.unwrap_or(mtoon.emissive_factor),
            mtoon.shading_shift_texture.map_or(0.0, |slot| slot.scale),
        ),
        // The matcap factor, then the shading toony factor.
        with(
            overrides.matcap_color.unwrap_or(mtoon.matcap_factor),
            mtoon.shading_toony_factor,
        ),
        // The parametric rim color, then the rim lighting mix.
        with(
            overrides
                .rim_color
                .unwrap_or(mtoon.parametric_rim_color_factor),
            mtoon.rim_lighting_mix_factor,
        ),
        // The rim's fresnel power and lift, the GI equalization factor,
        // and the normal texture's scale.
        [
            mtoon.parametric_rim_fresnel_power_factor,
            mtoon.parametric_rim_lift_factor,
            mtoon.gi_equalization_factor,
            mtoon.normal_texture.map_or(0.0, |slot| slot.scale),
        ],
        // The outline color, then the outline lighting mix.
        with(
            overrides
                .outline_color
                .unwrap_or(mtoon.outline_color_factor),
            mtoon.outline_lighting_mix_factor,
        ),
        // The outline width factor and mode.
        [
            mtoon.outline_width_factor,
            match mtoon.outline_width_mode {
                OutlineWidthMode::None => 0.0,
                OutlineWidthMode::WorldCoordinates => 1.0,
                OutlineWidthMode::ScreenCoordinates => 2.0,
            },
            0.0,
            0.0,
        ],
        // The scroll, the rotation in turns a second, and whether there is
        // a normal texture.
        [
            scroll[0],
            scroll[1],
            mtoon.uv_animation_rotation_speed_factor / std::f32::consts::TAU,
            if mtoon.normal_texture.is_some() {
                1.0
            } else {
                0.0
            },
        ],
    ];
    let mut bytes = [0; MTOON_PARAMS];
    for (chunk, value) in bytes
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(rows.as_flattened())
    {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_plugin::{Filter, Sampler, TextureSource, TextureTransform};
    use std::sync::Arc;

    /// The f32 at byte `offset` of `bytes`.
    fn float(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn floats(bytes: &[u8], offset: usize, count: usize) -> Vec<f32> {
        (0..count)
            .map(|index| float(bytes, offset + 4 * index))
            .collect()
    }

    /// A 1x1 white texture sampled with `wrap` both ways.
    fn wrapped(wrap: Wrap) -> SceneTexture {
        SceneTexture {
            source: TextureSource::Rgba {
                width: 1,
                height: 1,
                pixels: Arc::new(vec![255; 4]),
            },
            color_space: celesta_plugin::ColorSpace::Srgb,
            sampler: Sampler {
                wrap_s: wrap,
                wrap_t: wrap,
                mag_filter: Filter::Nearest,
                min_filter: Filter::Nearest,
                mipmaps: false,
            },
        }
    }

    #[test]
    fn mtoon_params_pack_univrm_defaults() {
        let bytes = params(&Mtoon::default(), &MtoonOverrides::default(), 0.0, &[]);
        assert_eq!(floats(&bytes, 0, 4), [1.0; 4]);
        // The identity transform's rows.
        assert_eq!(
            floats(&bytes, 16, 8),
            [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]
        );
        assert_eq!(floats(&bytes, 48, 4), [1.0, 1.0, 1.0, -0.05]);
        // No matcap colour, and no rim lift.
        assert_eq!(floats(&bytes, 80, 3), [0.0; 3]);
        assert_eq!(float(&bytes, 92), 0.95);
        assert_eq!(float(&bytes, 112), 5.0);
        assert_eq!(float(&bytes, 116), 0.0);
        assert_eq!(float(&bytes, 120), 0.9);
        // Rim and outline lighting mixes 1, no outline, no animation.
        assert_eq!(float(&bytes, 108), 1.0);
        assert_eq!(float(&bytes, 140), 1.0);
        assert_eq!(floats(&bytes, 144, 4), [0.0; 4]);
        assert_eq!(floats(&bytes, 160, 4), [0.0; 4]);
    }

    #[test]
    fn params_take_the_overrides_in_place_of_the_material() {
        let mtoon = Mtoon {
            base_color_texture: Some(TextureRef {
                texture: 0,
                transform: Some(TextureTransform {
                    offset: [0.5, 0.25],
                    rotation: 0.0,
                    scale: [2.0, 2.0],
                }),
            }),
            outline_width_mode: OutlineWidthMode::ScreenCoordinates,
            normal_texture: Some(ScaledTextureSlot {
                texture: 0,
                scale: 0.5,
            }),
            uv_animation_rotation_speed_factor: std::f32::consts::PI,
            ..Mtoon::default()
        };
        let overrides = MtoonOverrides {
            base_color: Some([0.1, 0.2, 0.3, 0.4]),
            texture_offset: Some([0.75, 0.0]),
            texture_scale: None,
            shade_color: Some([0.5; 3]),
            emission_color: Some([0.25; 3]),
            matcap_color: Some([0.125; 3]),
            rim_color: Some([0.0625; 3]),
            outline_color: Some([0.03125; 3]),
        };
        let bytes = params(&mtoon, &overrides, 0.0, &[wrapped(Wrap::Repeat)]);
        assert_eq!(floats(&bytes, 0, 4), [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(
            floats(&bytes, 16, 8),
            [2.0, 0.0, 0.75, 0.0, 0.0, 2.0, 0.0, 0.0]
        );
        assert_eq!(floats(&bytes, 48, 3), [0.5; 3]);
        assert_eq!(floats(&bytes, 64, 3), [0.25; 3]);
        assert_eq!(floats(&bytes, 80, 3), [0.125; 3]);
        assert_eq!(floats(&bytes, 96, 3), [0.0625; 3]);
        assert_eq!(floats(&bytes, 128, 3), [0.03125; 3]);
        assert_eq!(float(&bytes, 124), 0.5);
        assert_eq!(float(&bytes, 148), 2.0);
        assert_eq!(floats(&bytes, 168, 2), [0.5, 1.0]);
    }

    /// The scroll packed at bytes 160 and 164.
    fn scroll(mtoon: &Mtoon, textures: &[SceneTexture], time: f64) -> [f32; 2] {
        let bytes = params(mtoon, &MtoonOverrides::default(), time, textures);
        [float(&bytes, 160), float(&bytes, 164)]
    }

    /// Half a texture a second in u, on texture 0.
    fn scrolling(edit: impl FnOnce(&mut Mtoon)) -> Mtoon {
        let mut mtoon = Mtoon {
            base_color_texture: Some(TextureRef {
                texture: 0,
                transform: None,
            }),
            uv_animation_scroll_x_speed_factor: 0.5,
            ..Mtoon::default()
        };
        edit(&mut mtoon);
        mtoon
    }

    #[test]
    fn uv_scroll_keeps_its_precision_over_long_videos() {
        // On a repeated texture an odd number of seconds moves it half a
        // texture. Past 2^25 s, f32 rounds the time to an even number of
        // seconds, and the scroll formed in f32 would be whole textures.
        let repeated = [wrapped(Wrap::Repeat)];
        for time in [1.0, 3601.0, 33_554_433.0] {
            assert_eq!(
                scroll(&scrolling(|_| {}), &repeated, time),
                [0.5, 0.0],
                "{time}"
            );
        }
        // A mirrored repeat keeps two textures, its period.
        let mirrored = [wrapped(Wrap::MirroredRepeat)];
        assert_eq!(scroll(&scrolling(|_| {}), &mirrored, 3.0), [1.5, 0.0]);
    }

    #[test]
    fn a_still_scroll_packs_the_same_bytes_at_any_time() {
        // 0 × a negative time is −0.0, whose bytes are not +0.0's.
        let still = params(&Mtoon::default(), &MtoonOverrides::default(), 0.0, &[]);
        for time in [-1.0, -0.5, 3.0] {
            let bytes = params(&Mtoon::default(), &MtoonOverrides::default(), time, &[]);
            assert_eq!(bytes, still, "{time}");
        }
    }

    #[test]
    fn uv_scroll_wraps_only_where_whole_textures_vanish() {
        let time = 4.0;
        // Clamped, the texture stays at its edge: the whole 2 is kept.
        assert_eq!(
            scroll(&scrolling(|_| {}), &[wrapped(Wrap::ClampToEdge)], time),
            [2.0, 0.0]
        );
        let repeated = [wrapped(Wrap::Repeat), wrapped(Wrap::Repeat)];
        assert_eq!(scroll(&scrolling(|_| {}), &repeated, time), [0.0, 0.0]);
        // A mask scales the dropped textures by a fraction.
        let masked = scrolling(|mtoon| {
            mtoon.uv_animation_mask_texture = Some(TextureSlot { texture: 1 });
        });
        assert_eq!(scroll(&masked, &repeated, time), [2.0, 0.0]);
        // A turn moves whole textures off the texel grid.
        let turning = scrolling(|mtoon| {
            mtoon.uv_animation_rotation_speed_factor = std::f32::consts::PI / 24.0;
        });
        assert_eq!(scroll(&turning, &repeated, time), [2.0, 0.0]);
        // Any animated slot that clamps keeps them, here the shade
        // multiply's; the matcap samples its own coordinates.
        let shaded = scrolling(|mtoon| {
            mtoon.shade_multiply_texture = Some(TextureSlot { texture: 1 });
        });
        let mixed = [wrapped(Wrap::Repeat), wrapped(Wrap::ClampToEdge)];
        assert_eq!(scroll(&shaded, &mixed, time), [2.0, 0.0]);
        let matcap = scrolling(|mtoon| {
            mtoon.matcap_texture = Some(TextureSlot { texture: 1 });
        });
        assert_eq!(scroll(&matcap, &mixed, time), [0.0, 0.0]);
    }

    #[test]
    fn mtoon_slots_and_numbers_are_checked() {
        assert_eq!(invalid(&Mtoon::default(), 0), None);

        let slot = Some(TextureSlot { texture: 1 });
        let scaled = Some(ScaledTextureSlot {
            texture: 1,
            scale: 1.0,
        });
        type Set = fn(&mut Mtoon, Option<TextureSlot>, Option<ScaledTextureSlot>);
        let cases: [(&str, Set); 9] = [
            ("baseColorTexture", |m, _, _| {
                m.base_color_texture = Some(TextureRef {
                    texture: 1,
                    transform: None,
                })
            }),
            ("shadeMultiplyTexture", |m, slot, _| {
                m.shade_multiply_texture = slot
            }),
            ("shadingShiftTexture", |m, _, scaled| {
                m.shading_shift_texture = scaled
            }),
            ("normalTexture", |m, _, scaled| m.normal_texture = scaled),
            ("emissiveTexture", |m, slot, _| m.emissive_texture = slot),
            ("matcapTexture", |m, slot, _| m.matcap_texture = slot),
            ("rimMultiplyTexture", |m, slot, _| {
                m.rim_multiply_texture = slot
            }),
            ("outlineWidthMultiplyTexture", |m, slot, _| {
                m.outline_width_multiply_texture = slot
            }),
            ("uvAnimationMaskTexture", |m, slot, _| {
                m.uv_animation_mask_texture = slot
            }),
        ];
        for (name, set) in cases {
            let mut mtoon = Mtoon::default();
            set(&mut mtoon, slot, scaled);
            assert_eq!(
                invalid(&mtoon, 1),
                Some(format!("{name} uses texture 1, of 1"))
            );
            assert_eq!(invalid(&mtoon, 2), None, "{name}");
        }

        let not_finite: [fn(&mut Mtoon); 5] = [
            |m| m.shading_toony_factor = f32::NAN,
            |m| m.base_color[3] = f32::INFINITY,
            |m| m.uv_animation_rotation_speed_factor = f32::NEG_INFINITY,
            |m| {
                m.normal_texture = Some(ScaledTextureSlot {
                    texture: 0,
                    scale: f32::NAN,
                })
            },
            |m| {
                m.base_color_texture = Some(TextureRef {
                    texture: 0,
                    transform: Some(TextureTransform {
                        offset: [0.0; 2],
                        rotation: f32::NAN,
                        scale: [1.0; 2],
                    }),
                })
            },
        ];
        for set in not_finite {
            let mut mtoon = Mtoon::default();
            set(&mut mtoon);
            assert_eq!(
                invalid(&mtoon, 1).as_deref(),
                Some("its MToon parameters are not finite")
            );
        }
    }

    #[test]
    fn the_shader_has_nine_named_slots() {
        let shader = shader();
        assert_eq!(shader.name.as_deref(), Some("MToon"));
        let names: Vec<&str> = shader
            .textures
            .iter()
            .map(|slot| slot.name.as_deref().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "baseColorTexture",
                "shadeMultiplyTexture",
                "shadingShiftTexture",
                "normalTexture",
                "emissiveTexture",
                "matcapTexture",
                "rimMultiplyTexture",
                "outlineWidthMultiplyTexture",
                "uvAnimationMaskTexture",
            ]
        );
        let black: Vec<usize> = (0..9)
            .filter(|&k| shader.textures[k].empty == EmptyTexel::Black)
            .collect();
        assert_eq!(black, [2, 5]);
        assert!(shader.wgsl.contains("struct Params"));
    }

    #[test]
    fn custom_materials_outline_unless_none_and_need_tangents_for_normals() {
        let plain = custom_material(&Mtoon::default(), 0, &[]);
        assert_eq!(plain.shader, 0);
        assert_eq!(plain.params.len(), MTOON_PARAMS);
        assert_eq!(plain.textures, [None; 9]);
        assert_eq!(plain.outline, None);
        assert!(!plain.requires_tangents);
        let outlined = custom_material(
            &Mtoon {
                outline_width_mode: OutlineWidthMode::WorldCoordinates,
                normal_texture: Some(ScaledTextureSlot {
                    texture: 0,
                    scale: 1.0,
                }),
                ..Mtoon::default()
            },
            3,
            &[],
        );
        assert_eq!(outlined.shader, 3);
        assert_eq!(outlined.outline, Some(OutlinePass::default()));
        assert!(outlined.requires_tangents);
        assert_eq!(outlined.textures[3], Some(0));
    }
}
