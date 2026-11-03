//! A VRM's materials and the textures they sample.
//!
//! - A material with `VRMC_materials_mtoon` is MToon: every field the file
//!   has, on top of UniVRM's importer defaults (`Mtoon::default()`), drawn
//!   as a custom material on the plugin's MToon shader (see `mtoon`), the
//!   model's shader 0. A blended one keeps its `transparentWithZWrite` and
//!   `renderQueueOffsetNumber` as the blend's depth write and queue offset;
//!   opaque and masked ones drop them, as UniVRM does.
//! - `KHR_materials_unlit` is unlit: the base color times its texture.
//! - Any other material is glTF PBR, which Celesta does not shade yet: it is
//!   drawn unlit with its base color and base color texture, and the model
//!   names those its draws use in one `info` notice.
//!
//! Materials keep glTF's indices, unused ones too, so expression binds can
//! name them. Every slot samples `TEXCOORD_0` whatever its `texCoord`, as
//! UniVRM does, and only the base color texture's `KHR_texture_transform`
//! is kept: it moves every texture but the matcap and the outline width, as
//! UniVRM's single `_MainTex_ST` does.
//!
//! Images are not decoded here. Each PNG, JPEG, or WebP image a slot uses
//! is written once to a cache directory under its content's SHA-256 and
//! sent as a path, which Celesta decodes; its header gives the size that
//! counts toward [`TEXTURE_BUDGET`], once for each scene texture that
//! samples it. Images are read only from the file:
//! a buffer view or a `data:` URI, never another file or the network.

use crate::document::{Document, LoadError};
use crate::mtoon::{self, Mtoon, OutlineWidthMode, ScaledTextureSlot, TextureSlot};
use crate::notices::Warnings;
use base64::Engine as _;
use celesta_plugin::{
    AlphaMode, ColorSpace, Filter, Material, MaterialKind, Notice, Sampler, SceneShader,
    SceneTexture, TextureRef, TextureSource, TextureTransform, UnlitMaterial, Wrap,
};
use gltf::json;
use gltf::json::validation::Checked::Valid;
use image::{ImageDecoder, ImageFormat, ImageReader};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fmt::{self, Write as _};
use std::fs::{self, OpenOptions};
use std::io::{self, Cursor, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use vrm_spec::vrmc_materials_mtoon_1_0::{
    OutlineWidthMode as MtoonOutlineWidthMode, VrmcMaterialsMtoonSchema,
};

/// The most a model's scene textures may take once decoded, 4 bytes a
/// texel: Celesta decodes each scene texture's image on its own.
pub(crate) const TEXTURE_BUDGET: u64 = 1 << 30;
const MIB: u64 = 1 << 20;

/// The most one image may take decoded in its own pixel format (4 bytes a
/// texel for 8-bit RGBA, 8 for a 16-bit RGBA PNG, 3 for a JPEG): the
/// default `Limits::max_alloc` of image 0.25, which Celesta's
/// `image::open` reserves before it decodes. A larger image fails there,
/// so its slot is left empty with a warning instead.
const IMAGE_DECODE_LIMIT: u64 = 512 * MIB;

/// The environment variable that moves the texture cache.
const CACHE_VARIABLE: &str = "CELESTA_VRM_TEXTURE_CACHE";

/// The image MIME types the plugin reads. `image/jpg` is not one glTF
/// allows, but exporters write it.
const READABLE: [&str; 4] = ["image/png", "image/jpeg", "image/jpg", "image/webp"];

/// A model's materials, by glTF index, and the textures they sample.
#[derive(Debug)]
pub(crate) struct Materials {
    pub textures: Vec<SceneTexture>,
    pub materials: Vec<Material>,
    pub notices: Vec<Notice>,
    /// The glTF PBR materials drawn as unlit base colour, by index, with
    /// their names (`material {i}` without one): the model names those its
    /// draws use in one notice.
    pub pbr: Vec<(usize, String)>,
    /// By material: its base color texture's transform, which texture
    /// transform binds start from. `None` without a base color texture,
    /// or when that texture's image cannot be read.
    pub base_transforms: Vec<Option<TextureTransform>>,
    /// By material: an MToon material's parameters, which its custom
    /// material carries packed.
    pub mtoon: Vec<Option<Mtoon>>,
    /// The MToon shader when a material is MToon, else nothing.
    pub shaders: Vec<SceneShader>,
}

/// The model's shader that draws MToon materials.
const MTOON_SHADER: usize = 0;

/// What a material is drawn with, before the scene textures are known.
enum Kind {
    Unlit(UnlitMaterial),
    Mtoon(Box<Mtoon>),
}

/// The cache directory's own name.
const CACHE_NAME: &str = "celesta-plugin-vrm";

/// Where images are written: `$CELESTA_VRM_TEXTURE_CACHE`, else
/// `celesta-plugin-vrm` in the user's own cache directory
/// (`%LOCALAPPDATA%` on Windows, `~/Library/Caches` on macOS,
/// `$XDG_CACHE_HOME` or `~/.cache` elsewhere). Without one, a directory
/// named for the user in the temporary directory. [`materials`] refuses
/// any of them that is relative, or not the user's own and private.
pub(crate) fn texture_cache_dir() -> PathBuf {
    cache_dir_from(|name| std::env::var_os(name))
}

/// [`texture_cache_dir`] with the environment variables `var` gives.
fn cache_dir_from(var: impl Fn(&str) -> Option<OsString>) -> PathBuf {
    if let Some(dir) = var(CACHE_VARIABLE).filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir);
    }
    let absolute = |name: &str| {
        var(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let base = if cfg!(windows) {
        absolute("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        absolute("HOME").map(|home| home.join("Library").join("Caches"))
    } else {
        absolute("XDG_CACHE_HOME").or_else(|| absolute("HOME").map(|home| home.join(".cache")))
    };
    match base {
        Some(base) => base.join(CACHE_NAME),
        #[cfg(unix)]
        None => std::env::temp_dir().join(format!("{CACHE_NAME}-{}", user())),
        // Windows' temporary directory is the user's own.
        #[cfg(not(unix))]
        None => std::env::temp_dir().join(CACHE_NAME),
    }
}

/// The user the plugin runs as.
#[cfg(unix)]
fn user() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

/// Makes the cache directory, private to the user where it is new, and
/// refuses one that is not safe to write images to and read them back
/// from. A relative cache is refused before anything is made: Celesta
/// starts the plugin in the plugin's own folder and resolves the paths it
/// is sent against its own working directory, so the two would differ.
fn prepare_cache(cache: &Path) -> Result<(), LoadError> {
    if !cache.is_absolute() {
        return Err(cache_refusal(cache, "is not an absolute path"));
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    if let Err(error) = builder.create(cache)
        // Something already there that is not a directory: the check says
        // what.
        && fs::symlink_metadata(cache).is_err()
    {
        return Err(cache_error(cache, error));
    }
    #[cfg(unix)]
    return check_cache(cache, user());
    #[cfg(not(unix))]
    return check_cache(cache);
}

fn cache_refusal(cache: &Path, reason: &str) -> LoadError {
    LoadError::Io(io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "the texture cache {} {reason}; set CELESTA_VRM_TEXTURE_CACHE to a directory of your own",
            cache.display()
        ),
    ))
}

/// The cache is a real directory (not a link to one), and on Unix it is
/// `user`'s and no one else can write to it: otherwise another user could
/// put their own image where Celesta reads the model's.
fn check_cache(cache: &Path, #[cfg(unix)] user: u32) -> Result<(), LoadError> {
    let metadata = fs::symlink_metadata(cache).map_err(|error| cache_error(cache, error))?;
    if metadata.file_type().is_symlink() {
        return Err(cache_refusal(cache, "is a symbolic link"));
    }
    if !metadata.is_dir() {
        return Err(cache_refusal(cache, "is not a directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != user {
            return Err(cache_refusal(cache, "belongs to another user"));
        }
        if metadata.mode() & 0o022 != 0 {
            return Err(cache_refusal(
                cache,
                &format!(
                    "can be written by other users (mode {:o})",
                    metadata.mode() & 0o777
                ),
            ));
        }
    }
    Ok(())
}

/// Reads every material of `document`, and writes the images they use to
/// `cache` once the textures are known to fit [`TEXTURE_BUDGET`].
pub(crate) fn materials(document: &Document, cache: &Path) -> Result<Materials, LoadError> {
    let mut reader = Reader {
        document,
        json: document.gltf.as_json(),
        images: (0..document.gltf.as_json().images.len())
            .map(|_| None)
            .collect(),
        used: Vec::new(),
        keys: Vec::new(),
        lookup: HashMap::new(),
        warnings: Warnings::new("textures a material draws without"),
        noticed: HashSet::new(),
    };
    let mut read = Vec::new();
    let mut base_transforms = Vec::new();
    let mut pbr = Vec::new();
    for material in document.gltf.materials() {
        let index = material.index().unwrap_or(read.len());
        let at = At {
            index,
            name: material.name(),
        };
        let (alpha_mode, kind, transform) = reader.material(&material, &at)?;
        match &kind {
            // Celesta sees only the packed parameters, so the plugin checks
            // them.
            Kind::Mtoon(mtoon) => {
                if let Some(reason) = mtoon::invalid(mtoon, reader.keys.len()) {
                    return Err(LoadError::Invalid(format!("{at}: {reason}")));
                }
            }
            Kind::Unlit(_) if !material.unlit() => pbr.push((
                index,
                material
                    .name()
                    .map_or_else(|| format!("material {index}"), str::to_owned),
            )),
            Kind::Unlit(_) => {}
        }
        read.push((alpha_mode, material.double_sided(), kind));
        base_transforms.push(transform);
    }
    let mut notices = Vec::new();
    reader.warnings.finish(&mut notices);

    // Every image is sized before anything is written. Celesta decodes each
    // scene texture's file on its own, so an image two scene textures
    // sample (another color space or sampler) counts twice.
    let decoded = reader
        .keys
        .iter()
        .filter_map(|&(image, _, _)| match &reader.images[image] {
            Some(Image::Readable(readable)) => Some(readable.decoded),
            _ => None,
        })
        .fold(0u64, u64::saturating_add);
    if decoded > TEXTURE_BUDGET {
        return Err(LoadError::Invalid(format!(
            "the model's textures decode to {} MiB, more than {} MiB",
            mib(decoded),
            TEXTURE_BUDGET / MIB
        )));
    }

    let mut paths: HashMap<usize, PathBuf> = HashMap::new();
    if !reader.used.is_empty() {
        prepare_cache(cache)?;
    }
    for &image in &reader.used {
        if let Some(Image::Readable(readable)) = &reader.images[image] {
            let name = format!(
                "{}.{}",
                hex(&Sha256::digest(&readable.bytes)),
                readable.extension
            );
            paths.insert(image, write_cached(cache, &name, &readable.bytes)?);
        }
    }
    let textures: Vec<SceneTexture> = reader
        .keys
        .into_iter()
        .map(|(image, color_space, sampler)| SceneTexture {
            source: TextureSource::Path(paths.get(&image).cloned().unwrap_or_default()),
            color_space,
            sampler,
        })
        .collect();

    // MToon's parameters read the scene textures' samplers.
    let mut materials = Vec::with_capacity(read.len());
    let mut mtoons = Vec::with_capacity(read.len());
    for (alpha_mode, double_sided, kind) in read {
        let (kind, mtoon) = match kind {
            Kind::Unlit(unlit) => (MaterialKind::Unlit(unlit), None),
            Kind::Mtoon(mtoon) => (
                MaterialKind::Custom(mtoon::custom_material(&mtoon, MTOON_SHADER, &textures)),
                Some(*mtoon),
            ),
        };
        materials.push(Material {
            alpha_mode,
            double_sided,
            kind,
        });
        mtoons.push(mtoon);
    }
    let shaders = if mtoons.iter().any(Option::is_some) {
        vec![mtoon::shader()]
    } else {
        Vec::new()
    };
    Ok(Materials {
        textures,
        materials,
        notices,
        pbr,
        base_transforms,
        mtoon: mtoons,
        shaders,
    })
}

/// A material, for messages: `material "Body"`, or `material 3` without a
/// name.
struct At<'d> {
    index: usize,
    name: Option<&'d str>,
}

impl fmt::Display for At<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name {
            Some(name) => write!(f, "material {name:?}"),
            None => write!(f, "material {}", self.index),
        }
    }
}

impl At<'_> {
    /// `values`, refused when one is not finite (a JSON number past `f32`'s
    /// range reads as infinity).
    fn finite<const N: usize>(&self, field: &str, values: [f32; N]) -> Result<[f32; N], LoadError> {
        if values.iter().all(|value| value.is_finite()) {
            Ok(values)
        } else {
            let article = if field.starts_with(['a', 'e', 'i', 'o', 'u']) {
                "an"
            } else {
                "a"
            };
            Err(LoadError::Invalid(format!(
                "{self} has {article} {field} that is not finite: {values:?}"
            )))
        }
    }

    fn number(&self, field: &str, value: f64) -> Result<f32, LoadError> {
        Ok(self.finite(field, [value as f32])?[0])
    }

    fn color(&self, field: &str, value: [f64; 3]) -> Result<[f32; 3], LoadError> {
        self.finite(field, value.map(|value| value as f32))
    }
}

/// `material`'s `VRMC_materials_hdr_emissiveMultiplier.emissiveMultiplier`:
/// 1 without the extension or the number (VRM's default), refused when it
/// is not a finite number.
fn emissive_multiplier(material: &gltf::Material, at: &At) -> Result<f32, LoadError> {
    const FIELD: &str = "VRMC_materials_hdr_emissiveMultiplier.emissiveMultiplier";
    let value = material
        .extension_value("VRMC_materials_hdr_emissiveMultiplier")
        .and_then(|extension| extension.get("emissiveMultiplier"));
    match value {
        None | Some(serde_json::Value::Null) => Ok(1.0),
        Some(value) => match value.as_f64() {
            Some(number) => at.number(FIELD, number),
            None => Err(LoadError::Invalid(format!(
                "{at} has a {FIELD} that is not a number: {value}"
            ))),
        },
    }
}

/// An image a slot uses: its encoded bytes, or why the slot draws without
/// it (`is image/ktx2, which the VRM plugin cannot read`).
enum Image<'d> {
    Readable(Readable<'d>),
    Unreadable(String),
}

/// Where a texture's pixels are: an image the plugin may read, or only an
/// image of a MIME type it cannot (`KHR_texture_basisu`, `MSFT_texture_dds`
/// without a `source` to fall back on).
enum TextureImage {
    Image(usize),
    Unreadable(&'static str),
}

struct Readable<'d> {
    bytes: Cow<'d, [u8]>,
    /// The file extension Celesta decodes it by.
    extension: &'static str,
    /// Its size decoded, 4 bytes a texel.
    decoded: u64,
}

struct Reader<'d> {
    document: &'d Document,
    json: &'d json::Root,
    /// By image index, once a slot has used it.
    images: Vec<Option<Image<'d>>>,
    /// Readable images in first-use order: every one has a scene texture.
    used: Vec<usize>,
    /// The scene textures: image, color space, and sampler.
    keys: Vec<(usize, ColorSpace, Sampler)>,
    lookup: HashMap<(usize, ColorSpace, Sampler), usize>,
    warnings: Warnings,
    noticed: HashSet<String>,
}

impl<'d> Reader<'d> {
    /// One material's alpha mode, what it is drawn with, and its base color
    /// texture's transform.
    fn material(
        &mut self,
        material: &gltf::Material<'d>,
        at: &At,
    ) -> Result<(AlphaMode, Kind, Option<TextureTransform>), LoadError> {
        let mtoon = self.document.mtoon.get(at.index).and_then(Option::as_ref);
        let alpha_mode = match material.alpha_mode() {
            gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
            gltf::material::AlphaMode::Mask => AlphaMode::Mask {
                cutoff: at.finite("alphaCutoff", [material.alpha_cutoff().unwrap_or(0.5)])?[0],
            },
            // MToon's own queue offset and depth write; UniVRM reads them
            // for a blended material only.
            gltf::material::AlphaMode::Blend => AlphaMode::Blend {
                depth_write: mtoon
                    .and_then(|mtoon| mtoon.transparent_with_z_write)
                    .unwrap_or(false),
                // Celesta clamps it to 9 either way.
                queue_offset: mtoon
                    .and_then(|mtoon| mtoon.render_queue_offset_number)
                    .map_or(0, |value| {
                        value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
                    }),
            },
        };
        let pbr = material.pbr_metallic_roughness();
        let base_color = at.finite("baseColorFactor", pbr.base_color_factor())?;
        let base_color_texture = match pbr.base_color_texture() {
            Some(info) => {
                let transform = info
                    .texture_transform()
                    .map(|transform| {
                        let [x, y] = at.finite("KHR_texture_transform", transform.offset())?;
                        let [rotation] =
                            at.finite("KHR_texture_transform", [transform.rotation()])?;
                        let [u, v] = at.finite("KHR_texture_transform", transform.scale())?;
                        Ok::<_, LoadError>(TextureTransform {
                            offset: [x, y],
                            rotation,
                            scale: [u, v],
                        })
                    })
                    .transpose()?;
                self.slot(at, info.texture().index(), ColorSpace::Srgb)?
                    .map(|texture| TextureRef { texture, transform })
            }
            None => None,
        };
        let transform = base_color_texture
            .as_ref()
            .and_then(|texture| texture.transform);

        let kind = match mtoon {
            Some(mtoon) => Kind::Mtoon(Box::new(self.mtoon(
                material,
                mtoon,
                at,
                Mtoon {
                    base_color,
                    base_color_texture,
                    ..Mtoon::default()
                },
            )?)),
            // KHR_materials_unlit, and glTF PBR drawn as its base color.
            None => Kind::Unlit(UnlitMaterial {
                base_color,
                base_color_texture,
            }),
        };
        Ok((alpha_mode, kind, transform))
    }

    /// `VRMC_materials_mtoon` on top of `out`, which has the base color.
    /// Slots are read in `Mtoon`'s field order, which orders the scene's
    /// textures.
    fn mtoon(
        &mut self,
        material: &gltf::Material<'d>,
        mtoon: &VrmcMaterialsMtoonSchema,
        at: &At,
        mut out: Mtoon,
    ) -> Result<Mtoon, LoadError> {
        use ColorSpace::{Linear, Srgb};

        if let Some(normal) = material.normal_texture() {
            let [scale] = at.finite("normalTexture.scale", [normal.scale()])?;
            out.normal_texture = self
                .slot(at, normal.texture().index(), Linear)?
                .map(|texture| ScaledTextureSlot { texture, scale });
        }
        // UniGLTF's ImportLinearEmissiveFactor: KHR_materials_emissive_strength,
        // else VRMC_materials_hdr_emissiveMultiplier.
        let strength = match material.emissive_strength() {
            Some(strength) => strength,
            None => emissive_multiplier(material, at)?,
        };
        out.emissive_factor = at.finite(
            "emissiveFactor",
            material.emissive_factor().map(|value| value * strength),
        )?;
        if let Some(info) = material.emissive_texture() {
            out.emissive_texture = self.texture_slot(at, info.texture().index(), Srgb)?;
        }
        if let Some(value) = mtoon.shade_color_factor {
            out.shade_color_factor = at.color("shadeColorFactor", value)?;
        }
        if let Some(info) = &mtoon.shade_multiply_texture {
            out.shade_multiply_texture = self.texture_slot(at, info.index.value(), Srgb)?;
        }
        if let Some(value) = mtoon.shading_shift_factor {
            out.shading_shift_factor = at.number("shadingShiftFactor", value)?;
        }
        if let Some(info) = &mtoon.shading_shift_texture {
            let scale = at.number("shadingShiftTexture.scale", info.scale.unwrap_or(1.0))?;
            out.shading_shift_texture = self
                .slot(at, info.index.value(), Linear)?
                .map(|texture| ScaledTextureSlot { texture, scale });
        }
        if let Some(value) = mtoon.shading_toony_factor {
            out.shading_toony_factor = at.number("shadingToonyFactor", value)?;
        }
        if let Some(value) = mtoon.gi_equalization_factor {
            out.gi_equalization_factor = at.number("giEqualizationFactor", value)?;
        }
        if let Some(value) = mtoon.matcap_factor {
            out.matcap_factor = at.color("matcapFactor", value)?;
        }
        if let Some(info) = &mtoon.matcap_texture {
            out.matcap_texture = self.texture_slot(at, info.index.value(), Srgb)?;
        }
        if let Some(value) = mtoon.parametric_rim_color_factor {
            out.parametric_rim_color_factor = at.color("parametricRimColorFactor", value)?;
        }
        if let Some(info) = &mtoon.rim_multiply_texture {
            out.rim_multiply_texture = self.texture_slot(at, info.index.value(), Srgb)?;
        }
        if let Some(value) = mtoon.rim_lighting_mix_factor {
            out.rim_lighting_mix_factor = at.number("rimLightingMixFactor", value)?;
        }
        if let Some(value) = mtoon.parametric_rim_fresnel_power_factor {
            out.parametric_rim_fresnel_power_factor =
                at.number("parametricRimFresnelPowerFactor", value)?;
        }
        if let Some(value) = mtoon.parametric_rim_lift_factor {
            out.parametric_rim_lift_factor = at.number("parametricRimLiftFactor", value)?;
        }
        if let Some(mode) = mtoon.outline_width_mode {
            out.outline_width_mode = match mode {
                MtoonOutlineWidthMode::None => OutlineWidthMode::None,
                MtoonOutlineWidthMode::WorldCoordinates => OutlineWidthMode::WorldCoordinates,
                MtoonOutlineWidthMode::ScreenCoordinates => OutlineWidthMode::ScreenCoordinates,
            };
        }
        if let Some(value) = mtoon.outline_width_factor {
            out.outline_width_factor = at.number("outlineWidthFactor", value)?;
        }
        if let Some(info) = &mtoon.outline_width_multiply_texture {
            out.outline_width_multiply_texture =
                self.texture_slot(at, info.index.value(), Linear)?;
        }
        if let Some(value) = mtoon.outline_color_factor {
            out.outline_color_factor = at.color("outlineColorFactor", value)?;
        }
        if let Some(value) = mtoon.outline_lighting_mix_factor {
            out.outline_lighting_mix_factor = at.number("outlineLightingMixFactor", value)?;
        }
        if let Some(info) = &mtoon.uv_animation_mask_texture {
            out.uv_animation_mask_texture = self.texture_slot(at, info.index.value(), Linear)?;
        }
        if let Some(value) = mtoon.uv_animation_scroll_x_speed_factor {
            out.uv_animation_scroll_x_speed_factor =
                at.number("uvAnimationScrollXSpeedFactor", value)?;
        }
        if let Some(value) = mtoon.uv_animation_scroll_y_speed_factor {
            out.uv_animation_scroll_y_speed_factor =
                at.number("uvAnimationScrollYSpeedFactor", value)?;
        }
        if let Some(value) = mtoon.uv_animation_rotation_speed_factor {
            out.uv_animation_rotation_speed_factor =
                at.number("uvAnimationRotationSpeedFactor", value)?;
        }
        Ok(out)
    }

    fn texture_slot(
        &mut self,
        at: &At,
        texture: usize,
        color_space: ColorSpace,
    ) -> Result<Option<TextureSlot>, LoadError> {
        Ok(self
            .slot(at, texture, color_space)?
            .map(|texture| TextureSlot { texture }))
    }

    /// The scene texture for glTF texture `texture` in `color_space`, or
    /// `None` with a warning when its image cannot be read.
    fn slot(
        &mut self,
        at: &At,
        texture: usize,
        color_space: ColorSpace,
    ) -> Result<Option<usize>, LoadError> {
        let Some(json) = self.json.textures.get(texture) else {
            return Err(LoadError::Invalid(format!(
                "{at} uses texture {texture}, but the file has {} textures",
                self.json.textures.len()
            )));
        };
        let image = match self.texture_image(texture, json)? {
            TextureImage::Image(image) => image,
            TextureImage::Unreadable(mime) => {
                self.warn(format!(
                    "texture {texture} is {mime}, which the VRM plugin cannot read; {at} draws without it"
                ));
                return Ok(None);
            }
        };
        let sampler = match json.sampler {
            None => Sampler::default(),
            Some(index) => match self.json.samplers.get(index.value()) {
                Some(sampler) => scene_sampler(sampler),
                None => {
                    return Err(LoadError::Invalid(format!(
                        "texture {texture} uses sampler {}, but the file has {} samplers",
                        index.value(),
                        self.json.samplers.len()
                    )));
                }
            },
        };
        if self.images[image].is_none() {
            let read = self.read_image(image)?;
            if let Image::Readable(_) = read {
                self.used.push(image);
            }
            self.images[image] = Some(read);
        }
        if let Some(Image::Unreadable(reason)) = &self.images[image] {
            let message = format!("texture {texture} {reason}; {at} draws without it");
            self.warn(message);
            return Ok(None);
        }
        let key = (image, color_space, sampler);
        if let Some(&index) = self.lookup.get(&key) {
            return Ok(Some(index));
        }
        self.keys.push(key);
        self.lookup.insert(key, self.keys.len() - 1);
        Ok(Some(self.keys.len() - 1))
    }

    /// A warning, once however many slots give it (see `notices`).
    fn warn(&mut self, message: String) {
        if self.noticed.insert(message.clone()) {
            self.warnings.push(|| message);
        }
    }

    /// The image a texture shows: `EXT_texture_webp`'s when it has one,
    /// else its `source`. Without either, a `KHR_texture_basisu` (KTX2) or
    /// `MSFT_texture_dds` texture shows an image the plugin cannot read.
    fn texture_image(
        &self,
        texture: usize,
        json: &json::Texture,
    ) -> Result<TextureImage, LoadError> {
        let extension = |name: &str| {
            json.extensions
                .as_ref()
                .and_then(|extensions| extensions.others.get(name))
        };
        let image = match extension("EXT_texture_webp") {
            Some(webp) => webp
                .get("source")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| {
                    LoadError::Invalid(format!(
                        "texture {texture}'s EXT_texture_webp has no image source"
                    ))
                })?,
            // `u32::MAX` stands for no `source`.
            None if json.source.value() == u32::MAX as usize => {
                if extension("KHR_texture_basisu").is_some() {
                    return Ok(TextureImage::Unreadable("image/ktx2"));
                }
                if extension("MSFT_texture_dds").is_some() {
                    return Ok(TextureImage::Unreadable("image/vnd-ms.dds"));
                }
                return Err(LoadError::Invalid(format!(
                    "texture {texture} has no image"
                )));
            }
            None => json.source.value() as u64,
        };
        let images = self.json.images.len();
        match usize::try_from(image) {
            Ok(image) if image < images => Ok(TextureImage::Image(image)),
            _ => Err(LoadError::Invalid(format!(
                "texture {texture} uses image {image}, but the file has {images} images"
            ))),
        }
    }

    /// Image `index` (in range): its MIME type first, so an image the
    /// plugin cannot read costs nothing, then its bytes and its header.
    fn read_image(&self, index: usize) -> Result<Image<'d>, LoadError> {
        let invalid =
            |reason: fmt::Arguments| LoadError::Invalid(format!("image {index} {reason}"));
        let json = &self.json.images[index];
        let mime = json
            .mime_type
            .as_ref()
            .map(|mime| mime.0.as_str())
            .or_else(|| json.uri.as_deref().and_then(data_uri_mime));
        if let Some(mime) = mime
            && !READABLE.contains(&mime)
        {
            return Ok(Image::Unreadable(format!(
                "is {mime}, which the VRM plugin cannot read"
            )));
        }

        let bytes: Cow<'d, [u8]> = match (&json.buffer_view, &json.uri) {
            (Some(view), _) => Cow::Borrowed(self.view_bytes(index, view.value())?),
            (None, Some(uri)) => match uri.strip_prefix("data:") {
                Some(data) => Cow::Owned(data_uri_bytes(index, data)?),
                None => {
                    return Err(invalid(format_args!(
                        "is an external file; a .vrm keeps everything in itself"
                    )));
                }
            },
            (None, None) => {
                return Err(invalid(format_args!("has neither a bufferView nor a uri")));
            }
        };

        let reader = ImageReader::new(Cursor::new(&*bytes)).with_guessed_format()?;
        let (extension, format) = match reader.format() {
            Some(ImageFormat::Png) => ("png", "PNG"),
            Some(ImageFormat::Jpeg) => ("jpg", "JPEG"),
            Some(ImageFormat::WebP) => ("webp", "WebP"),
            _ => {
                return Err(invalid(format_args!(
                    "({}) is not a PNG, JPEG, or WebP image",
                    mime.unwrap_or("no MIME type")
                )));
            }
        };
        // The header alone: no pixel is decoded here. The decoder gets the
        // default limits, as Celesta's `image::open` does.
        // What Celesta's decoder reserves is in the image's own pixel format.
        let ((width, height), native) = reader
            .into_decoder()
            .map(|decoder| (decoder.dimensions(), decoder.total_bytes()))
            .map_err(|error| invalid(format_args!("cannot be read as {format}: {error}")))?;
        if native > IMAGE_DECODE_LIMIT {
            return Ok(Image::Unreadable(format!(
                "is a {width}x{height} {format} image, which decodes to {} MiB, more than the {} MiB Celesta decodes one image to",
                mib(native),
                IMAGE_DECODE_LIMIT / MIB
            )));
        }
        Ok(Image::Readable(Readable {
            bytes,
            extension,
            decoded: u64::from(width) * u64::from(height) * 4,
        }))
    }

    /// The bytes of buffer view `view`, which must lie inside its buffer.
    fn view_bytes(&self, image: usize, view: usize) -> Result<&'d [u8], LoadError> {
        let views = &self.json.buffer_views;
        let Some(json) = views.get(view) else {
            return Err(LoadError::Invalid(format!(
                "image {image} uses buffer view {view}, but the file has {} buffer views",
                views.len()
            )));
        };
        let buffer = self
            .document
            .buffers
            .get(json.buffer.value())
            .map_or(&[][..], |data| data.0.as_slice());
        let start = json.byte_offset.map_or(0, |offset| offset.0);
        let range = start
            .checked_add(json.byte_length.0)
            .filter(|&end| end <= buffer.len() as u64)
            .map(|end| start as usize..end as usize);
        match range {
            Some(range) => Ok(&buffer[range]),
            None => Err(LoadError::Invalid(format!(
                "image {image}'s buffer view {view} runs past its buffer's {} bytes",
                buffer.len()
            ))),
        }
    }
}

/// A glTF sampler as Celesta samples it.
fn scene_sampler(json: &json::texture::Sampler) -> Sampler {
    use json::texture::{MagFilter, MinFilter, WrappingMode};
    let wrap = |mode: &json::validation::Checked<WrappingMode>| match mode {
        Valid(WrappingMode::ClampToEdge) => Wrap::ClampToEdge,
        Valid(WrappingMode::MirroredRepeat) => Wrap::MirroredRepeat,
        _ => Wrap::Repeat,
    };
    let mag_filter = match json.mag_filter {
        Some(Valid(MagFilter::Nearest)) => Filter::Nearest,
        _ => Filter::Linear,
    };
    let (min_filter, mipmaps) = match json.min_filter {
        Some(Valid(MinFilter::Nearest)) => (Filter::Nearest, false),
        Some(Valid(MinFilter::Linear)) => (Filter::Linear, false),
        Some(Valid(MinFilter::NearestMipmapNearest | MinFilter::NearestMipmapLinear)) => {
            (Filter::Nearest, true)
        }
        _ => (Filter::Linear, true),
    };
    Sampler {
        wrap_s: wrap(&json.wrap_s),
        wrap_t: wrap(&json.wrap_t),
        mag_filter,
        min_filter,
        mipmaps,
    }
}

/// A `data:` URI's MIME type, if it names one.
fn data_uri_mime(uri: &str) -> Option<&str> {
    let mime = uri.strip_prefix("data:")?.split([';', ',']).next()?;
    (!mime.is_empty()).then_some(mime)
}

/// The bytes of a `data:` URI, after `data:`; glTF's are base64.
fn data_uri_bytes(image: usize, data: &str) -> Result<Vec<u8>, LoadError> {
    let (meta, payload) = data.split_once(',').unwrap_or((data, ""));
    if !meta.ends_with(";base64") {
        return Err(LoadError::Invalid(format!(
            "image {image}'s data: URI is not base64"
        )));
    }
    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|error| {
            LoadError::Invalid(format!(
                "image {image}'s data: URI is not valid base64: {error}"
            ))
        })
}

/// `bytes` in MiB, to the nearest.
fn mib(bytes: u64) -> u64 {
    bytes.saturating_add(MIB / 2) / MIB
}

fn hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn cache_error(path: &Path, error: io::Error) -> LoadError {
    LoadError::Io(io::Error::new(
        error.kind(),
        format!("writing the texture cache {}: {error}", path.display()),
    ))
}

/// How many temporary names [`write_cached`] tries before it gives up.
const TEMPORARY_ATTEMPTS: u64 = 8;

/// `bytes` at `cache/name`, written unless a file there already holds
/// them. The name is a content hash and an extension, so nothing in the
/// model chooses where it goes. The bytes go to a new temporary file
/// first (never one already there, nor through a link) and are renamed
/// into place, so a reader never sees half a file.
fn write_cached(cache: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, LoadError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    write_cached_with(cache, name, bytes, &NEXT)
}

/// [`write_cached`], numbering its temporary files from `next`. A name
/// already taken (a file left by a process that crashed with this one's
/// id) is left alone and the next is tried. A temporary file this call
/// made is removed when the write or the rename fails.
fn write_cached_with(
    cache: &Path,
    name: &str,
    bytes: &[u8],
    next: &AtomicU64,
) -> Result<PathBuf, LoadError> {
    let path = cache.join(name);
    let holds_bytes = |path: &Path| {
        fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() == bytes.len() as u64)
            && fs::read(path).is_ok_and(|held| held == bytes)
    };
    if holds_bytes(&path) {
        return Ok(path);
    }
    let mut failure = None;
    for _ in 0..TEMPORARY_ATTEMPTS {
        let temporary = cache.join(format!(
            "{name}.{}-{}.tmp",
            std::process::id(),
            next.fetch_add(1, Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary);
        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                failure = Some(error);
                continue;
            }
            Err(error) => {
                failure = Some(error);
                break;
            }
        };
        // The file is closed before the rename, which Windows needs.
        let written = file.write_all(bytes);
        drop(file);
        match written.and_then(|()| fs::rename(&temporary, &path)) {
            Ok(()) => return Ok(path),
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                failure = Some(error);
                break;
            }
        }
    }
    // Another process may have put the same bytes there first.
    if holds_bytes(&path) {
        return Ok(path);
    }
    let error = failure.unwrap_or_else(|| io::Error::other("no temporary file could be made"));
    Err(cache_error(&path, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::synthetic::SyntheticVrm;
    use celesta_plugin::NoticeLevel;
    use image::{ExtendedColorType, ImageEncoder};
    use serde_json::{Value, json};
    use tempfile::TempDir;

    fn document(vrm: &SyntheticVrm, edit: impl FnOnce(&mut Value)) -> Document {
        Document::parse(&vrm.glb_with(edit))
            .unwrap_or_else(|error| panic!("the file did not parse: {error}"))
    }

    fn load_with(vrm: &SyntheticVrm, edit: impl FnOnce(&mut Value)) -> (Materials, TempDir) {
        let document = document(vrm, edit);
        let cache = TempDir::new().unwrap();
        match materials(&document, cache.path()) {
            Ok(materials) => (materials, cache),
            Err(error) => panic!("the materials were refused: {error}"),
        }
    }

    fn load(vrm: &SyntheticVrm) -> (Materials, TempDir) {
        load_with(vrm, |_| {})
    }

    fn refusal_of(vrm: &SyntheticVrm, edit: impl FnOnce(&mut Value)) -> String {
        let document = document(vrm, edit);
        let cache = TempDir::new().unwrap();
        match materials(&document, cache.path()) {
            Ok(_) => panic!("the materials were read"),
            Err(error) => error.to_string(),
        }
    }

    fn refusal(edit: impl FnOnce(&mut Value)) -> String {
        refusal_of(&SyntheticVrm::avatar(), edit)
    }

    fn mtoon(materials: &Materials, index: usize) -> &Mtoon {
        match &materials.mtoon[index] {
            Some(mtoon) => mtoon,
            None => panic!("material {index} is {:?}", materials.materials[index].kind),
        }
    }

    fn custom(materials: &Materials, index: usize) -> &celesta_plugin::CustomMaterial {
        match &materials.materials[index].kind {
            MaterialKind::Custom(custom) => custom,
            other => panic!("material {index} is {other:?}"),
        }
    }

    fn unlit(materials: &Materials, index: usize) -> &UnlitMaterial {
        match &materials.materials[index].kind {
            MaterialKind::Unlit(unlit) => unlit,
            other => panic!("material {index} is {other:?}"),
        }
    }

    fn path(materials: &Materials, texture: usize) -> &Path {
        match &materials.textures[texture].source {
            TextureSource::Path(path) => path,
            other => panic!("texture {texture} is {other:?}"),
        }
    }

    fn sha256(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn files(dir: &Path) -> usize {
        std::fs::read_dir(dir).unwrap().count()
    }

    fn mtoon_extension(root: &mut Value, material: usize) -> &mut Value {
        &mut root["materials"][material]["extensions"]["VRMC_materials_mtoon"]
    }

    #[test]
    fn mtoon_slots_dedupe_by_image_colour_space_and_sampler() {
        let (materials, _cache) = load(&SyntheticVrm::avatar());
        let spaces: Vec<(ColorSpace, Sampler)> = materials
            .textures
            .iter()
            .map(|texture| (texture.color_space, texture.sampler))
            .collect();
        assert_eq!(spaces.len(), 3);
        assert_eq!(spaces[0], (ColorSpace::Srgb, Sampler::default()));
        assert_eq!(spaces[1], (ColorSpace::Linear, Sampler::default()));
        assert_eq!(spaces[2].0, ColorSpace::Linear);
        assert_eq!(path(&materials, 0), path(&materials, 2), "both are image 0");
        assert_ne!(path(&materials, 0), path(&materials, 1));

        let body = mtoon(&materials, 0);
        assert_eq!(body.base_color_texture.as_ref().unwrap().texture, 0);
        assert_eq!(
            body.shade_multiply_texture,
            Some(TextureSlot { texture: 0 })
        );
        assert_eq!(body.normal_texture.unwrap().texture, 1);
        assert_eq!(body.shading_shift_texture.unwrap().texture, 2);
        assert_eq!(
            custom(&materials, 0).textures,
            [
                Some(0),
                Some(0),
                Some(2),
                Some(1),
                None,
                None,
                None,
                None,
                None
            ]
        );
    }

    #[test]
    fn mtoon_materials_share_one_shader() {
        let (materials, _cache) = load(&SyntheticVrm::avatar());
        assert_eq!(materials.shaders, [mtoon::shader()]);
        let (body, hair) = (custom(&materials, 0), custom(&materials, 1));
        assert_eq!((body.shader, hair.shader), (0, 0));
        // Only Body has a normal texture.
        assert!(body.requires_tangents);
        assert!(!hair.requires_tangents);
        assert_eq!(
            body.params,
            mtoon::params(
                mtoon(&materials, 0),
                &mtoon::MtoonOverrides::default(),
                0.0,
                &materials.textures
            )
        );
        assert_eq!(body.outline, None);
        assert_eq!(materials.mtoon[2], None);
        assert_eq!(materials.mtoon[3], None);

        // A model without MToon brings no shader.
        let (materials, _cache) = load_with(&SyntheticVrm::avatar(), |root| {
            for material in 0..2 {
                root["materials"][material]["extensions"]
                    .as_object_mut()
                    .unwrap()
                    .remove("VRMC_materials_mtoon");
            }
        });
        assert_eq!(materials.shaders, []);
        assert!(materials.mtoon.iter().all(Option::is_none));
    }

    #[test]
    fn base_colour_transform_moves_the_material() {
        let (materials, _cache) = load(&SyntheticVrm::avatar());
        let transform = TextureTransform {
            offset: [0.5, 0.0],
            rotation: 0.0,
            scale: [1.0, 1.0],
        };
        assert_eq!(
            mtoon(&materials, 0)
                .base_color_texture
                .as_ref()
                .unwrap()
                .transform,
            Some(transform)
        );
        assert_eq!(materials.base_transforms[0], Some(transform));
        assert_eq!(materials.base_transforms[1], None);
        assert_eq!(materials.base_transforms.len(), 4);
    }

    #[test]
    fn hair_keeps_blend_and_its_queue_offset() {
        let (materials, _cache) = load(&SyntheticVrm::avatar());
        assert_eq!(
            materials.materials[1].alpha_mode,
            AlphaMode::Blend {
                depth_write: false,
                queue_offset: -1
            }
        );
        assert_eq!(materials.materials[0].alpha_mode, AlphaMode::Opaque);
        assert!(!materials.materials[0].double_sided);

        // The depth write and an offset past i32, on the blended hair and
        // the opaque body: only the blend keeps them, as UniVRM does.
        let (materials, _cache) = load_with(&SyntheticVrm::avatar(), |root| {
            for material in 0..2 {
                let extension = mtoon_extension(root, material);
                extension["transparentWithZWrite"] = json!(true);
                extension["renderQueueOffsetNumber"] = json!(1_000_000_000_000_i64);
            }
        });
        assert_eq!(
            materials.materials[1].alpha_mode,
            AlphaMode::Blend {
                depth_write: true,
                queue_offset: i32::MAX
            }
        );
        assert_eq!(materials.materials[0].alpha_mode, AlphaMode::Opaque);
    }

    #[test]
    fn pbr_is_drawn_unlit_with_a_notice() {
        let (materials, _cache) = load(&SyntheticVrm::avatar());
        assert_eq!(unlit(&materials, 2).base_color, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(unlit(&materials, 3).base_color, [0.0, 1.0, 0.0, 1.0]);
        // The model, which knows which materials its draws use, names them.
        assert!(materials.notices.is_empty(), "{:?}", materials.notices);
        assert_eq!(materials.pbr, [(2, "Pbr".to_string())]);
    }

    #[test]
    fn sampler_follows_gltf() {
        let (materials, _cache) = load(&SyntheticVrm::avatar());
        assert_eq!(
            materials.textures[2].sampler,
            Sampler {
                wrap_s: Wrap::ClampToEdge,
                wrap_t: Wrap::MirroredRepeat,
                mag_filter: Filter::Nearest,
                min_filter: Filter::Nearest,
                mipmaps: false,
            }
        );
    }

    #[test]
    fn min_filters_choose_mipmaps() {
        for (min, filter, mipmaps) in [
            (9728, Filter::Nearest, false),
            (9729, Filter::Linear, false),
            (9984, Filter::Nearest, true),
            (9985, Filter::Linear, true),
            (9986, Filter::Nearest, true),
            (9987, Filter::Linear, true),
        ] {
            let (materials, _cache) = load_with(&SyntheticVrm::avatar(), |root| {
                root["samplers"][1] = json!({ "minFilter": min, "magFilter": 9729 });
            });
            let sampler = materials.textures[2].sampler;
            assert_eq!(
                (sampler.min_filter, sampler.mipmaps),
                (filter, mipmaps),
                "{min}"
            );
            assert_eq!(sampler.mag_filter, Filter::Linear);
            assert_eq!(sampler.wrap_s, Wrap::Repeat);
        }
    }

    #[test]
    fn images_are_written_once_by_content() {
        let vrm = SyntheticVrm::avatar();
        let document = document(&vrm, |_| {});
        let cache = TempDir::new().unwrap();
        let materials = materials(&document, cache.path()).unwrap();
        let file = path(&materials, 0);
        assert_eq!(file.parent(), Some(cache.path()));
        assert_eq!(
            file.file_name().unwrap().to_str().unwrap(),
            format!("{}.png", sha256(&vrm.images[0].1))
        );
        assert_eq!(std::fs::read(file).unwrap(), vrm.images[0].1);
        assert_eq!(files(cache.path()), 2);

        let again = super::materials(&document, cache.path()).unwrap();
        assert_eq!(again.textures, materials.textures);
        assert_eq!(files(cache.path()), 2);
    }

    #[test]
    fn a_stale_cache_file_is_replaced() {
        let vrm = SyntheticVrm::avatar();
        let document = document(&vrm, |_| {});
        let cache = TempDir::new().unwrap();
        let name = format!("{}.png", sha256(&vrm.images[0].1));
        std::fs::write(cache.path().join(&name), b"not the image").unwrap();
        let materials = materials(&document, cache.path()).unwrap();
        assert_eq!(std::fs::read(path(&materials, 0)).unwrap(), vrm.images[0].1);
        assert_eq!(files(cache.path()), 2);
    }

    /// An 8-bit RGBA [`png_header_of`].
    fn png_header(width: u32, height: u32) -> Vec<u8> {
        png_header_of(width, height, 8)
    }

    /// A PNG of `width × height` RGBA, `depth` bits a channel, that has a
    /// signature, an IHDR, an empty IDAT, and an IEND: a header that claims
    /// the size and no pixels. (The header is read up to the first IDAT,
    /// which must be there.)
    fn png_header_of(width: u32, height: u32, depth: u8) -> Vec<u8> {
        fn crc32(bytes: &[u8]) -> u32 {
            let mut crc = !0u32;
            for &byte in bytes {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xEDB8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
            !crc
        }
        fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
            png.extend((data.len() as u32).to_be_bytes());
            let start = png.len();
            png.extend(kind);
            png.extend(data);
            let crc = crc32(&png[start..]);
            png.extend(crc.to_be_bytes());
        }
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend(width.to_be_bytes());
        ihdr.extend(height.to_be_bytes());
        // RGBA, deflate, adaptive filters, no interlace.
        ihdr.extend([depth, 6, 0, 0, 0]);
        chunk(&mut png, b"IHDR", &ihdr);
        chunk(&mut png, b"IDAT", &[]);
        chunk(&mut png, b"IEND", &[]);
        png
    }

    /// The temporary name `write_cached_with` tries `attempt`th.
    fn temporary(cache: &Path, name: &str, attempt: u64) -> PathBuf {
        cache.join(format!("{name}.{}-{attempt}.tmp", std::process::id()))
    }

    /// A temporary file left by a process that died with this one's id is
    /// not reused, nor removed: the next name is tried.
    #[test]
    fn taken_temporary_names_are_skipped() {
        let cache = TempDir::new().unwrap();
        for attempt in 0..2 {
            std::fs::write(temporary(cache.path(), "a.png", attempt), b"left over").unwrap();
        }
        let next = AtomicU64::new(0);
        let path = write_cached_with(cache.path(), "a.png", b"image", &next).unwrap();
        assert_eq!(path, cache.path().join("a.png"));
        assert_eq!(std::fs::read(&path).unwrap(), b"image");
        assert_eq!(next.load(Ordering::Relaxed), 3);
        for attempt in 0..2 {
            assert_eq!(
                std::fs::read(temporary(cache.path(), "a.png", attempt)).unwrap(),
                b"left over"
            );
        }
        assert_eq!(files(cache.path()), 3);
    }

    #[test]
    fn every_temporary_name_taken_is_an_error() {
        let cache = TempDir::new().unwrap();
        for attempt in 0..TEMPORARY_ATTEMPTS {
            std::fs::write(temporary(cache.path(), "a.png", attempt), b"left over").unwrap();
        }
        let next = AtomicU64::new(0);
        let message = match write_cached_with(cache.path(), "a.png", b"image", &next) {
            Ok(path) => panic!("written to {path:?}"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("writing the texture cache"), "{message}");
        // The files there are left as they were, and nothing is added.
        assert_eq!(files(cache.path()), TEMPORARY_ATTEMPTS as usize);
        assert!(!cache.path().join("a.png").exists());
    }

    /// A temporary file the plugin made is removed when the rename fails.
    #[test]
    fn a_failed_write_leaves_no_temporary_file() {
        let cache = TempDir::new().unwrap();
        // A directory that is not empty cannot be renamed over.
        std::fs::create_dir_all(cache.path().join("a.png").join("inside")).unwrap();
        let next = AtomicU64::new(0);
        assert!(write_cached_with(cache.path(), "a.png", b"image", &next).is_err());
        assert_eq!(files(cache.path()), 1);
    }

    /// The model's warnings.
    fn warnings(materials: &Materials) -> Vec<&str> {
        materials
            .notices
            .iter()
            .filter(|notice| notice.level == NoticeLevel::Warning)
            .map(|notice| notice.message.as_str())
            .collect()
    }

    /// An avatar whose image 0 is a 10240×10240 RGBA PNG: 400 MiB decoded,
    /// under Celesta's 512 MiB for one image. The avatar samples it in two
    /// scene textures (the base color and shade in sRGB, the shading shift
    /// in linear through sampler 1): 800 MiB.
    fn four_hundred_mib() -> SyntheticVrm {
        let mut vrm = SyntheticVrm::avatar();
        vrm.images[0].1 = png_header(10240, 10240);
        vrm
    }

    /// A third scene texture of image 0: the matcap, sRGB through sampler 1.
    fn third_sampling(root: &mut Value) {
        mtoon_extension(root, 0)["matcapTexture"] = json!({ "index": 2 });
    }

    /// Celesta decodes each scene texture's file on its own, so an image
    /// sampled three ways counts three times.
    #[test]
    fn the_budget_counts_every_scene_texture() {
        let (materials, _cache) = load(&four_hundred_mib());
        assert_eq!(materials.textures.len(), 3);
        let message = refusal_of(&four_hundred_mib(), third_sampling);
        assert!(
            message.contains("the model's textures decode to 1200 MiB, more than 1024 MiB"),
            "{message}"
        );
    }

    #[test]
    fn a_refused_model_writes_nothing() {
        let document = document(&four_hundred_mib(), third_sampling);
        let cache = TempDir::new().unwrap();
        assert!(materials(&document, cache.path()).is_err());
        assert_eq!(files(cache.path()), 0);
    }

    /// An image larger than Celesta decodes would fail the model there; it
    /// is left out instead, and counts toward nothing.
    #[test]
    fn an_image_past_what_celesta_decodes_leaves_the_slot_empty() {
        let mut vrm = SyntheticVrm::avatar();
        vrm.images[0].1 = png_header(32768, 32768);
        let (materials, cache) = load(&vrm);
        let body = mtoon(&materials, 0);
        assert_eq!(body.base_color_texture, None);
        assert_eq!(body.shade_multiply_texture, None);
        assert_eq!(body.shading_shift_texture, None);
        assert_eq!(materials.base_transforms[0], None);
        assert_eq!(
            warnings(&materials),
            [0, 2].map(|texture| format!(
                "texture {texture} is a 32768x32768 PNG image, which decodes to 4096 MiB, \
                 more than the 512 MiB Celesta decodes one image to; material \"Body\" draws \
                 without it"
            ))
        );
        assert_eq!(materials.textures.len(), 1);
        assert_eq!(files(cache.path()), 1);
    }

    /// Celesta decodes a 16-bit PNG at 16 bits, 8 bytes an RGBA texel,
    /// before it makes 8-bit RGBA of it.
    #[test]
    fn sixteen_bit_images_count_their_own_bytes() {
        // 8192 × 8200 × 8 bytes is just over 512 MiB; at 8 bits, half that.
        let mut vrm = SyntheticVrm::avatar();
        vrm.images[0].1 = png_header_of(8192, 8200, 16);
        let (materials, _cache) = load(&vrm);
        assert_eq!(mtoon(&materials, 0).base_color_texture, None);
        assert!(
            warnings(&materials)[0].contains("decodes to 513 MiB"),
            "{:?}",
            warnings(&materials)
        );
        vrm.images[0].1 = png_header_of(8192, 8200, 8);
        let (materials, _cache) = load(&vrm);
        assert!(mtoon(&materials, 0).base_color_texture.is_some());
        assert_eq!(warnings(&materials), Vec::<&str>::new());
        // Exactly 512 MiB is what Celesta allows.
        vrm.images[0].1 = png_header_of(8192, 8192, 16);
        let (materials, _cache) = load(&vrm);
        assert!(mtoon(&materials, 0).base_color_texture.is_some());
    }

    /// `KHR_texture_basisu` and `MSFT_texture_dds` textures, as their
    /// specifications have them: no `source`, so the extensions are
    /// required.
    #[test]
    fn compressed_textures_leave_the_slot_empty() {
        let (materials, _cache) = load_with(&SyntheticVrm::avatar(), |root| {
            root["textures"][1] =
                json!({ "extensions": { "KHR_texture_basisu": { "source": 1 } } });
            root["textures"][2] =
                json!({ "sampler": 1, "extensions": { "MSFT_texture_dds": { "source": 1 } } });
            root["extensionsRequired"] = json!(["KHR_texture_basisu", "MSFT_texture_dds"]);
            for name in ["KHR_texture_basisu", "MSFT_texture_dds"] {
                root["extensionsUsed"]
                    .as_array_mut()
                    .unwrap()
                    .push(name.into());
            }
        });
        let body = mtoon(&materials, 0);
        assert_eq!(body.normal_texture, None);
        assert_eq!(body.shading_shift_texture, None);
        assert!(body.base_color_texture.is_some());
        assert_eq!(
            warnings(&materials),
            [
                "texture 1 is image/ktx2, which the VRM plugin cannot read; material \"Body\" \
                 draws without it",
                "texture 2 is image/vnd-ms.dds, which the VRM plugin cannot read; material \
                 \"Body\" draws without it",
            ]
        );
        assert_eq!(materials.textures.len(), 1);
    }

    #[test]
    fn texture_warnings_past_the_listed_are_counted() {
        let (materials, _cache) = load_with(&SyntheticVrm::avatar(), |root| {
            root["textures"][1] =
                json!({ "extensions": { "KHR_texture_basisu": { "source": 1 } } });
            root["extensionsRequired"] = json!(["KHR_texture_basisu"]);
            root["extensionsUsed"]
                .as_array_mut()
                .unwrap()
                .push("KHR_texture_basisu".into());
            let body = root["materials"][0].clone();
            let materials = root["materials"].as_array_mut().unwrap();
            for copy in 0..30 {
                let mut copy_of_body = body.clone();
                copy_of_body["name"] = json!(format!("Copy {copy}"));
                materials.push(copy_of_body);
            }
        });
        // Body and its 30 copies each draw without texture 1.
        let found = warnings(&materials);
        assert_eq!(found.len(), crate::notices::LISTED + 1, "{found:?}");
        assert!(found[0].contains("material \"Body\" draws without it"));
        assert_eq!(
            found[crate::notices::LISTED],
            format!(
                "and {} more textures a material draws without",
                31 - crate::notices::LISTED
            )
        );
    }

    /// With a PNG `source` beside it, a compressed texture draws the PNG.
    #[test]
    fn compressed_textures_fall_back_to_their_source() {
        let (materials, _cache) = load_with(&SyntheticVrm::avatar(), |root| {
            root["textures"][1]["extensions"] = json!({ "KHR_texture_basisu": { "source": 0 } });
            root["extensionsUsed"]
                .as_array_mut()
                .unwrap()
                .push("KHR_texture_basisu".into());
        });
        assert_eq!(mtoon(&materials, 0).normal_texture.unwrap().texture, 1);
        assert_eq!(warnings(&materials), Vec::<&str>::new());
        assert_eq!(materials.textures.len(), 3);
    }

    #[test]
    fn unreadable_images_leave_the_slot_empty() {
        let mut vrm = SyntheticVrm::avatar();
        vrm.images[1].0 = "image/ktx2".into();
        let (materials, cache) = load(&vrm);
        assert_eq!(mtoon(&materials, 0).normal_texture, None);
        let warnings = warnings(&materials);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("image/ktx2"), "{}", warnings[0]);
        assert!(warnings[0].contains("\"Body\""), "{}", warnings[0]);
        assert!(warnings[0].starts_with("texture 1 is image/ktx2"));
        // The KTX2 image is neither written nor counted.
        assert_eq!(materials.textures.len(), 2);
        assert_eq!(files(cache.path()), 1);
    }

    #[test]
    fn mtoon_fields_and_colour_spaces_follow_the_file() {
        let (materials, _cache) = load_with(&SyntheticVrm::avatar(), |root| {
            let extension = mtoon_extension(root, 0);
            extension["matcapTexture"] = json!({ "index": 1 });
            extension["outlineWidthMultiplyTexture"] = json!({ "index": 0 });
            extension["rimMultiplyTexture"] = json!({ "index": 1 });
            extension["uvAnimationMaskTexture"] = json!({ "index": 1 });
            extension["shadingShiftTexture"] = json!({ "index": 2, "scale": 0.5 });
            extension["outlineWidthMode"] = json!("worldCoordinates");
            extension["outlineWidthFactor"] = json!(0.01);
            extension["giEqualizationFactor"] = json!(0.25);
            extension["shadeColorFactor"] = json!([0.5, 0.25, 0.125]);
            extension["uvAnimationRotationSpeedFactor"] = json!(2.0);
            root["materials"][0]["normalTexture"]["scale"] = json!(0.75);
            root["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"] =
                json!([1.0, 0.5, 0.25, 0.75]);
        });
        let body = mtoon(&materials, 0);
        assert_eq!(body.base_color, [1.0, 0.5, 0.25, 0.75]);
        assert_eq!(
            body.normal_texture,
            Some(ScaledTextureSlot {
                texture: 1,
                scale: 0.75
            })
        );
        assert_eq!(
            body.shading_shift_texture,
            Some(ScaledTextureSlot {
                texture: 2,
                scale: 0.5
            })
        );
        // Image 1 in sRGB is new; image 0 in linear with the default
        // sampler is new; image 1 in sRGB again reuses 3; image 1 linear
        // reuses the normal map's.
        assert_eq!(body.matcap_texture, Some(TextureSlot { texture: 3 }));
        assert_eq!(body.rim_multiply_texture, Some(TextureSlot { texture: 3 }));
        assert_eq!(
            body.outline_width_multiply_texture,
            Some(TextureSlot { texture: 4 })
        );
        assert_eq!(
            body.uv_animation_mask_texture,
            Some(TextureSlot { texture: 1 })
        );
        assert_eq!(materials.textures.len(), 5);
        assert_eq!(materials.textures[3].color_space, ColorSpace::Srgb);
        assert_eq!(materials.textures[4].color_space, ColorSpace::Linear);
        assert_eq!(body.outline_width_mode, OutlineWidthMode::WorldCoordinates);
        assert_eq!(body.outline_width_factor, 0.01);
        assert_eq!(body.gi_equalization_factor, 0.25);
        assert_eq!(body.shade_color_factor, [0.5, 0.25, 0.125]);
        assert_eq!(body.uv_animation_rotation_speed_factor, 2.0);
        // What the file leaves out keeps UniVRM's default.
        assert_eq!(
            body.shading_toony_factor,
            Mtoon::default().shading_toony_factor
        );
        // The custom material names the same textures, outlines, and packs
        // the same numbers.
        let custom = custom(&materials, 0);
        assert_eq!(
            custom.textures,
            [
                Some(0),
                Some(0),
                Some(2),
                Some(1),
                None,
                Some(3),
                Some(3),
                Some(4),
                Some(1)
            ]
        );
        assert_eq!(custom.outline, Some(celesta_plugin::OutlinePass::default()));
        assert_eq!(body.emissive_texture, None);
    }

    #[test]
    fn emissive_strength_is_multiplied_in() {
        let (materials, _cache) = load_with(&SyntheticVrm::avatar(), |root| {
            let body = &mut root["materials"][0];
            body["emissiveFactor"] = json!([1.0, 0.5, 0.0]);
            body["emissiveTexture"] = json!({ "index": 0 });
            body["extensions"]["KHR_materials_emissive_strength"] =
                json!({ "emissiveStrength": 2.0 });
        });
        let body = mtoon(&materials, 0);
        assert_eq!(body.emissive_factor, [2.0, 1.0, 0.0]);
        assert_eq!(body.emissive_texture, Some(TextureSlot { texture: 0 }));
    }

    /// Body's emissive factor with `extensions` on the material.
    fn emissive_with(extensions: Value) -> [f32; 3] {
        let (materials, _cache) = load_with(&SyntheticVrm::avatar(), |root| {
            let body = &mut root["materials"][0];
            body["emissiveFactor"] = json!([1.0, 0.5, 0.0]);
            for (name, value) in extensions.as_object().unwrap() {
                body["extensions"][name] = value.clone();
            }
        });
        mtoon(&materials, 0).emissive_factor
    }

    #[test]
    fn the_vrm_emissive_multiplier_is_multiplied_in_without_the_strength() {
        // UniGLTF's ImportLinearEmissiveFactor: the strength, else VRM's
        // multiplier.
        let multiplier = json!({ "emissiveMultiplier": 4.0 });
        assert_eq!(
            emissive_with(json!({ "VRMC_materials_hdr_emissiveMultiplier": multiplier })),
            [4.0, 2.0, 0.0]
        );
        assert_eq!(
            emissive_with(json!({
                "VRMC_materials_hdr_emissiveMultiplier": multiplier,
                "KHR_materials_emissive_strength": { "emissiveStrength": 2.0 },
            })),
            [2.0, 1.0, 0.0]
        );
        // Without its number, the multiplier is VRM's default, 1.
        assert_eq!(
            emissive_with(json!({ "VRMC_materials_hdr_emissiveMultiplier": {} })),
            [1.0, 0.5, 0.0]
        );
    }

    #[test]
    fn alpha_and_sides_follow_gltf() {
        let (materials, _cache) = load_with(&SyntheticVrm::avatar(), |root| {
            root["materials"][2]["alphaMode"] = json!("MASK");
            root["materials"][2]["doubleSided"] = json!(true);
            root["materials"][3]["alphaMode"] = json!("MASK");
            root["materials"][3]["alphaCutoff"] = json!(0.25);
        });
        assert_eq!(
            materials.materials[2].alpha_mode,
            AlphaMode::Mask { cutoff: 0.5 }
        );
        assert!(materials.materials[2].double_sided);
        assert_eq!(
            materials.materials[3].alpha_mode,
            AlphaMode::Mask { cutoff: 0.25 }
        );
    }

    #[test]
    fn unlit_and_pbr_keep_their_base_colour_texture() {
        let (materials, _cache) = load_with(&SyntheticVrm::avatar(), |root| {
            root["materials"][2]["pbrMetallicRoughness"]["baseColorTexture"] = json!({
                "index": 2,
                "extensions": { "KHR_texture_transform": { "scale": [2.0, 2.0], "rotation": 0.5 } },
            });
            root["materials"][2]["normalTexture"] = json!({ "index": 1 });
            root["materials"][3]["pbrMetallicRoughness"]["baseColorTexture"] =
                json!({ "index": 0 });
        });
        let transform = TextureTransform {
            offset: [0.0, 0.0],
            rotation: 0.5,
            scale: [2.0, 2.0],
        };
        let pbr = unlit(&materials, 2).base_color_texture.as_ref().unwrap();
        // Image 0 in sRGB with sampler 1.
        assert_eq!(pbr.texture, 3);
        assert_eq!(materials.textures[3].color_space, ColorSpace::Srgb);
        assert_eq!(pbr.transform, Some(transform));
        assert_eq!(materials.base_transforms[2], Some(transform));
        let plain = unlit(&materials, 3).base_color_texture.as_ref().unwrap();
        assert_eq!((plain.texture, plain.transform), (0, None));
        // The PBR normal map is dropped: nothing new samples image 1.
        assert_eq!(materials.textures.len(), 4);
    }

    #[test]
    fn webp_images_are_read() {
        let mut vrm = SyntheticVrm::avatar();
        let mut webp = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut webp)
            .write_image(&[0, 0, 255, 255], 1, 1, ExtendedColorType::Rgba8)
            .unwrap();
        vrm.images.push(("image/webp".into(), webp.clone()));
        let (materials, _cache) = load_with(&vrm, |root| {
            // A texture with only the WebP source, and one with a fallback.
            root["textures"][1] = json!({ "extensions": { "EXT_texture_webp": { "source": 2 } } });
            root["textures"][2]["extensions"] = json!({ "EXT_texture_webp": { "source": 2 } });
            root["extensionsUsed"]
                .as_array_mut()
                .unwrap()
                .push("EXT_texture_webp".into());
        });
        let file = path(&materials, 1);
        assert_eq!(
            file.file_name().unwrap().to_str().unwrap(),
            format!("{}.webp", sha256(&webp))
        );
        assert_eq!(std::fs::read(file).unwrap(), webp);
        assert_eq!(path(&materials, 2), file);
    }

    #[test]
    fn data_uri_images_are_read() {
        let vrm = SyntheticVrm::avatar();
        let bytes = vrm.images[0].1.clone();
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let (materials, _cache) = load_with(&vrm, |root| {
            root["images"][0] = json!({ "uri": format!("data:image/png;base64,{encoded}") });
        });
        assert_eq!(std::fs::read(path(&materials, 0)).unwrap(), bytes);

        let message = refusal(|root| root["images"][0] = json!({ "uri": "data:image/png,%89PNG" }));
        assert!(
            message.contains("image 0's data: URI is not base64"),
            "{message}"
        );
        let message =
            refusal(|root| root["images"][0] = json!({ "uri": "data:image/png;base64,!!!!" }));
        assert!(
            message.contains("image 0's data: URI is not valid base64"),
            "{message}"
        );
    }

    #[test]
    fn images_without_a_mime_type_are_read_by_their_content() {
        let vrm = SyntheticVrm::avatar();
        let (materials, _cache) = load_with(&vrm, |root| {
            root["images"][0]
                .as_object_mut()
                .unwrap()
                .remove("mimeType");
        });
        assert_eq!(std::fs::read(path(&materials, 0)).unwrap(), vrm.images[0].1);
    }

    #[test]
    fn texture_indices_are_checked() {
        let message = refusal(|root| {
            mtoon_extension(root, 0)["shadeMultiplyTexture"] = json!({ "index": 99 });
        });
        assert!(
            message.contains("material \"Body\" uses texture 99, but the file has 3 textures"),
            "{message}"
        );
        let message = refusal(|root| {
            mtoon_extension(root, 0)["shadingShiftTexture"] = json!({ "index": 3 });
        });
        assert!(
            message.contains("uses texture 3, but the file has 3 textures"),
            "{message}"
        );
        let message = refusal(|root| {
            root["textures"][0]["extensions"] = json!({ "EXT_texture_webp": { "source": 99 } });
        });
        assert!(
            message.contains("texture 0 uses image 99, but the file has 2 images"),
            "{message}"
        );
        let message = refusal(|root| {
            root["textures"][0]["extensions"] = json!({ "EXT_texture_webp": { "source": "face" } });
        });
        assert!(
            message.contains("texture 0's EXT_texture_webp has no image source"),
            "{message}"
        );
        let message = refusal(|root| {
            root["textures"][0]
                .as_object_mut()
                .unwrap()
                .remove("source");
        });
        assert!(message.contains("texture 0 has no image"), "{message}");
    }

    #[test]
    fn images_from_elsewhere_are_refused() {
        let message = refusal(|root| {
            root["images"][0] = json!({ "uri": "face.png", "mimeType": "image/png" });
        });
        assert!(message.contains("image 0 is an external file"), "{message}");
        let message = refusal(|root| {
            root["images"][0] = json!({ "uri": "https://example.com/face.png" });
        });
        assert!(message.contains("image 0 is an external file"), "{message}");
        let message = refusal(|root| root["images"][0] = json!({ "mimeType": "image/png" }));
        assert!(
            message.contains("image 0 has neither a bufferView nor a uri"),
            "{message}"
        );
    }

    #[test]
    fn image_views_past_their_buffer_are_refused() {
        for (field, value) in [
            ("byteLength", json!(1u64 << 40)),
            ("byteOffset", json!(u64::MAX - 4)),
        ] {
            let message = refusal(|root| {
                let view = root["images"][0]["bufferView"].as_u64().unwrap() as usize;
                root["bufferViews"][view][field] = value;
            });
            assert!(
                message.contains("image 0's buffer view") && message.contains("runs past"),
                "{field}: {message}"
            );
        }
    }

    #[test]
    fn garbage_images_are_refused() {
        let mut vrm = SyntheticVrm::avatar();
        vrm.images[0].1 = b"not an image at all".to_vec();
        let message = refusal_of(&vrm, |_| {});
        assert!(
            message.contains("image 0 (image/png) is not a PNG, JPEG, or WebP image"),
            "{message}"
        );
        // A PNG signature and nothing after it.
        vrm.images[0].1 = b"\x89PNG\r\n\x1a\n".to_vec();
        let message = refusal_of(&vrm, |_| {});
        assert!(message.contains("image 0 cannot be read"), "{message}");
    }

    #[test]
    fn numbers_past_f32_are_refused() {
        let message = refusal(|root| {
            root["materials"][2]["pbrMetallicRoughness"]["baseColorFactor"] =
                json!([1e39, 0.0, 0.0, 1.0]);
        });
        assert!(
            message.contains("material \"Pbr\" has a baseColorFactor that is not finite"),
            "{message}"
        );
        let message = refusal(|root| {
            mtoon_extension(root, 0)["giEqualizationFactor"] = json!(1e39);
        });
        assert!(
            message.contains("material \"Body\" has a giEqualizationFactor that is not finite"),
            "{message}"
        );
        let message = refusal(|root| {
            root["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["extensions"]["KHR_texture_transform"]
                ["offset"] = json!([1e39, 0.0]);
        });
        assert!(
            message.contains("has a KHR_texture_transform that is not finite"),
            "{message}"
        );
        let message = refusal(|root| {
            let body = &mut root["materials"][0];
            // Each finite, but their product is past f32's range.
            body["emissiveFactor"] = json!([1.0, 1.0, 2.0]);
            body["extensions"]["KHR_materials_emissive_strength"] =
                json!({ "emissiveStrength": 3e38 });
        });
        assert!(
            message.contains("has an emissiveFactor that is not finite"),
            "{message}"
        );
        for multiplier in [json!(1e39), json!("bright")] {
            let message = refusal(|root| {
                root["materials"][0]["extensions"]["VRMC_materials_hdr_emissiveMultiplier"] =
                    json!({ "emissiveMultiplier": multiplier });
            });
            assert!(
                message.contains(
                    "material \"Body\" has a VRMC_materials_hdr_emissiveMultiplier.emissiveMultiplier that is not"
                ),
                "{message}"
            );
        }
        let message = refusal(|root| {
            root["materials"][1]["alphaMode"] = json!("MASK");
            root["materials"][1]["alphaCutoff"] = json!(1e39);
        });
        assert!(
            message.contains("material \"Hair\" has an alphaCutoff that is not finite"),
            "{message}"
        );
    }

    /// The cache directory `cache_dir_from` picks from these variables.
    fn cache_dir(vars: &[(&str, &str)]) -> PathBuf {
        cache_dir_from(|name| {
            vars.iter()
                .find(|(set, _)| *set == name)
                .map(|(_, value)| value.into())
        })
    }

    #[test]
    fn the_cache_is_per_user() {
        let home = if cfg!(windows) {
            "C:\\Users\\u"
        } else {
            "/home/u"
        };
        let set = if cfg!(windows) {
            "D:\\cache"
        } else {
            "/srv/cache"
        };
        assert_eq!(
            cache_dir(&[("CELESTA_VRM_TEXTURE_CACHE", set), ("HOME", home)]),
            Path::new(set)
        );
        let expected = if cfg!(windows) {
            PathBuf::from("C:\\Users\\u\\AppData\\Local\\celesta-plugin-vrm")
        } else if cfg!(target_os = "macos") {
            PathBuf::from("/home/u/Library/Caches/celesta-plugin-vrm")
        } else {
            PathBuf::from("/home/u/.cache/celesta-plugin-vrm")
        };
        let vars = [
            ("CELESTA_VRM_TEXTURE_CACHE", ""),
            ("HOME", home),
            ("LOCALAPPDATA", "C:\\Users\\u\\AppData\\Local"),
            // Relative, so ignored.
            ("XDG_CACHE_HOME", "cache"),
        ];
        assert_eq!(cache_dir(&vars), expected);
        if cfg!(all(unix, not(target_os = "macos"))) {
            assert_eq!(
                cache_dir(&[("XDG_CACHE_HOME", "/xdg"), ("HOME", home)]),
                Path::new("/xdg/celesta-plugin-vrm")
            );
        }
        // Nowhere of the user's own: a directory in the temporary one
        // named for the user, which the checks below keep private.
        let fallback = cache_dir(&[("HOME", "relative")]);
        assert_eq!(fallback.parent(), Some(std::env::temp_dir().as_path()));
        assert!(
            fallback
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("celesta-plugin-vrm"),
            "{fallback:?}"
        );
        #[cfg(unix)]
        assert_eq!(
            fallback.file_name().unwrap().to_str().unwrap(),
            format!("celesta-plugin-vrm-{}", user())
        );
    }

    #[cfg(unix)]
    fn cache_refusal(cache: &Path) -> String {
        let document = document(&SyntheticVrm::avatar(), |_| {});
        match materials(&document, cache) {
            Ok(_) => panic!("the cache was used"),
            Err(error) => error.to_string(),
        }
    }

    /// Celesta starts the plugin in its own folder and resolves a path it
    /// is sent against its own working directory, so a relative cache
    /// would name another place to each.
    #[test]
    fn a_relative_cache_is_refused_before_it_is_made() {
        let relative = Path::new("relative-texture-cache-of-a-test");
        let document = document(&SyntheticVrm::avatar(), |_| {});
        let message = match materials(&document, relative) {
            Ok(_) => panic!("the cache was used"),
            Err(error) => error.to_string(),
        };
        assert!(
            message.contains(
                "the texture cache relative-texture-cache-of-a-test is not an absolute path; \
                 set CELESTA_VRM_TEXTURE_CACHE to a directory of your own"
            ),
            "{message}"
        );
        assert!(!relative.exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_cache_is_made_private() {
        use std::os::unix::fs::PermissionsExt;
        let root = TempDir::new().unwrap();
        let cache = root.path().join("made").join("here");
        let document = document(&SyntheticVrm::avatar(), |_| {});
        materials(&document, &cache).unwrap();
        let mode = std::fs::metadata(&cache).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "{mode:o}");
        assert_eq!(files(&cache), 2);
    }

    #[cfg(unix)]
    #[test]
    fn a_cache_others_can_write_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let root = TempDir::new().unwrap();
        for mode in [0o777, 0o720, 0o702] {
            let cache = root.path().join(format!("{mode:o}"));
            std::fs::create_dir(&cache).unwrap();
            std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(mode)).unwrap();
            let message = cache_refusal(&cache);
            assert!(
                message.contains("can be written by other users"),
                "{mode:o}: {message}"
            );
            assert_eq!(files(&cache), 0);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_cache_of_another_user_is_refused() {
        let cache = TempDir::new().unwrap();
        let message = match check_cache(cache.path(), user().wrapping_add(1)) {
            Ok(()) => panic!("the cache was accepted"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("belongs to another user"), "{message}");
        check_cache(cache.path(), user()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_cache_is_refused() {
        let root = TempDir::new().unwrap();
        let real = root.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let message = cache_refusal(&link);
        assert!(message.contains("is a symbolic link"), "{message}");
        assert_eq!(files(&real), 0);

        let file = root.path().join("file");
        std::fs::write(&file, b"").unwrap();
        let message = cache_refusal(&file);
        assert!(message.contains("is not a directory"), "{message}");
    }

    /// A link planted under a cached name is replaced, not followed, even
    /// when it leads to the right bytes.
    #[cfg(unix)]
    #[test]
    fn a_symlink_in_the_cache_is_replaced() {
        let vrm = SyntheticVrm::avatar();
        let root = TempDir::new().unwrap();
        let elsewhere = root.path().join("elsewhere.png");
        std::fs::write(&elsewhere, &vrm.images[0].1).unwrap();
        let cache = root.path().join("cache");
        std::fs::create_dir(&cache).unwrap();
        let name = cache.join(format!("{}.png", sha256(&vrm.images[0].1)));
        std::os::unix::fs::symlink(&elsewhere, &name).unwrap();
        let document = document(&vrm, |_| {});
        let materials = materials(&document, &cache).unwrap();
        assert_eq!(path(&materials, 0), name);
        let metadata = std::fs::symlink_metadata(&name).unwrap();
        assert!(metadata.is_file(), "{metadata:?}");
        assert_eq!(std::fs::read(&name).unwrap(), vrm.images[0].1);
        assert_eq!(std::fs::read(&elsewhere).unwrap(), vrm.images[0].1);
    }
}
