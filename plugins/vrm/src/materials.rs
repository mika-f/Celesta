//! A VRM's materials and the textures they sample.
//!
//! - A material with `VRMC_materials_mtoon` is MToon: every field the file
//!   has, on top of UniVRM's importer defaults (`MtoonMaterial::default()`).
//! - `KHR_materials_unlit` is unlit: the base color times its texture.
//! - Any other material is glTF PBR, which Celesta does not shade yet: it is
//!   drawn unlit with its base color and base color texture, and one `info`
//!   notice names them all.
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
use base64::Engine as _;
use celesta_plugin::{
    AlphaMode, ColorSpace, Filter, Material, MaterialKind, MtoonMaterial, Notice, NoticeLevel,
    OutlineWidthMode, Sampler, ScaledTextureSlot, SceneTexture, TextureRef, TextureSlot,
    TextureSource, TextureTransform, UnlitMaterial, Wrap,
};
use gltf::json;
use gltf::json::validation::Checked::Valid;
use image::{ImageFormat, ImageReader};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
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

/// The environment variable that moves the texture cache.
const CACHE_VARIABLE: &str = "CELESTA_VRM_TEXTURE_CACHE";

/// The image MIME types the plugin reads. `image/jpg` is not one glTF
/// allows, but exporters write it.
const READABLE: [&str; 4] = ["image/png", "image/jpeg", "image/jpg", "image/webp"];

/// A model's materials, by glTF index, and the textures they sample.
#[derive(Debug)]
// The model (Task 4) reads these.
#[allow(dead_code)]
pub(crate) struct Materials {
    pub textures: Vec<SceneTexture>,
    pub materials: Vec<Material>,
    pub notices: Vec<Notice>,
    /// By material: its base color texture's transform, which texture
    /// transform binds start from. `None` without a base color texture,
    /// or when that texture's image cannot be read.
    pub base_transforms: Vec<Option<TextureTransform>>,
}

/// Where images are written: `$CELESTA_VRM_TEXTURE_CACHE`, else
/// `celesta-plugin-vrm` in the temporary directory.
// The provider (Task 4) calls this.
#[allow(dead_code)]
pub(crate) fn texture_cache_dir() -> PathBuf {
    std::env::var_os(CACHE_VARIABLE)
        .filter(|dir| !dir.is_empty())
        .map_or_else(
            || std::env::temp_dir().join("celesta-plugin-vrm"),
            PathBuf::from,
        )
}

/// Reads every material of `document`, and writes the images they use to
/// `cache` once the textures are known to fit [`TEXTURE_BUDGET`].
// The model (Task 4) calls this.
#[allow(dead_code)]
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
        notices: Vec::new(),
        noticed: HashSet::new(),
    };
    let mut materials = Vec::new();
    let mut base_transforms = Vec::new();
    let mut pbr = Vec::new();
    for material in document.gltf.materials() {
        let index = material.index().unwrap_or(materials.len());
        let at = At {
            index,
            name: material.name(),
        };
        let (read, transform) = reader.material(&material, &at)?;
        let is_mtoon = matches!(read.kind, MaterialKind::Mtoon(_));
        if !is_mtoon && !material.unlit() {
            pbr.push(
                material
                    .name()
                    .map_or_else(|| format!("material {index}"), str::to_owned),
            );
        }
        materials.push(read);
        base_transforms.push(transform);
    }
    let mut notices = reader.notices;
    if !pbr.is_empty() {
        notices.push(Notice {
            level: NoticeLevel::Info,
            message: format!(
                "{} glTF PBR material(s) are drawn as unlit base colour: {}",
                pbr.len(),
                pbr.join(", ")
            ),
        });
    }

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
            decoded.saturating_add(MIB / 2) / MIB,
            TEXTURE_BUDGET / MIB
        )));
    }

    let mut paths: HashMap<usize, PathBuf> = HashMap::new();
    if !reader.used.is_empty() {
        fs::create_dir_all(cache).map_err(|error| cache_error(cache, error))?;
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
    let textures = reader
        .keys
        .into_iter()
        .map(|(image, color_space, sampler)| SceneTexture {
            source: TextureSource::Path(paths.get(&image).cloned().unwrap_or_default()),
            color_space,
            sampler,
        })
        .collect();
    Ok(Materials {
        textures,
        materials,
        notices,
        base_transforms,
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

/// An image a slot uses: its encoded bytes, or the MIME type the plugin
/// cannot read.
enum Image<'d> {
    Readable(Readable<'d>),
    Unreadable(String),
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
    notices: Vec<Notice>,
    noticed: HashSet<String>,
}

impl<'d> Reader<'d> {
    /// One material and its base color texture's transform.
    fn material(
        &mut self,
        material: &gltf::Material<'d>,
        at: &At,
    ) -> Result<(Material, Option<TextureTransform>), LoadError> {
        let alpha_mode = match material.alpha_mode() {
            gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
            gltf::material::AlphaMode::Mask => AlphaMode::Mask {
                cutoff: at.finite("alphaCutoff", [material.alpha_cutoff().unwrap_or(0.5)])?[0],
            },
            gltf::material::AlphaMode::Blend => AlphaMode::Blend,
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

        let mtoon = self.document.mtoon.get(at.index).and_then(Option::as_ref);
        let kind = match mtoon {
            Some(mtoon) => MaterialKind::Mtoon(self.mtoon(
                material,
                mtoon,
                at,
                MtoonMaterial {
                    base_color,
                    base_color_texture,
                    ..MtoonMaterial::default()
                },
            )?),
            // KHR_materials_unlit, and glTF PBR drawn as its base color.
            None => MaterialKind::Unlit(UnlitMaterial {
                base_color,
                base_color_texture,
            }),
        };
        Ok((
            Material {
                alpha_mode,
                double_sided: material.double_sided(),
                kind,
            },
            transform,
        ))
    }

    /// `VRMC_materials_mtoon` on top of `out`, which has the base color.
    /// Slots are read in `MtoonMaterial`'s field order, which orders the
    /// scene's textures.
    fn mtoon(
        &mut self,
        material: &gltf::Material<'d>,
        mtoon: &VrmcMaterialsMtoonSchema,
        at: &At,
        mut out: MtoonMaterial,
    ) -> Result<MtoonMaterial, LoadError> {
        use ColorSpace::{Linear, Srgb};

        if let Some(normal) = material.normal_texture() {
            let [scale] = at.finite("normalTexture.scale", [normal.scale()])?;
            out.normal_texture = self
                .slot(at, normal.texture().index(), Linear)?
                .map(|texture| ScaledTextureSlot { texture, scale });
        }
        // UniGLTF's ImportLinearEmissiveFactor.
        let strength = material.emissive_strength().unwrap_or(1.0);
        out.emissive_factor = at.finite(
            "emissiveFactor",
            material.emissive_factor().map(|value| value * strength),
        )?;
        if let Some(info) = material.emissive_texture() {
            out.emissive_texture = self.texture_slot(at, info.texture().index(), Srgb)?;
        }
        if let Some(value) = mtoon.transparent_with_z_write {
            out.transparent_with_z_write = value;
        }
        if let Some(value) = mtoon.render_queue_offset_number {
            // Celesta clamps it to 9 either way.
            out.render_queue_offset_number =
                value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
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
        let image = self.texture_image(texture, json)?;
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
        if let Some(Image::Unreadable(mime)) = &self.images[image] {
            let message = format!(
                "texture {texture} is {mime}, which the VRM plugin cannot read; {at} draws without it"
            );
            if self.noticed.insert(message.clone()) {
                self.notices.push(Notice {
                    level: NoticeLevel::Warning,
                    message,
                });
            }
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

    /// The image a texture shows: `EXT_texture_webp`'s when it has one,
    /// else its `source`.
    fn texture_image(&self, texture: usize, json: &json::Texture) -> Result<usize, LoadError> {
        let webp = json
            .extensions
            .as_ref()
            .and_then(|extensions| extensions.others.get("EXT_texture_webp"));
        let image = match webp {
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
                return Err(LoadError::Invalid(format!(
                    "texture {texture} has no image"
                )));
            }
            None => json.source.value() as u64,
        };
        let images = self.json.images.len();
        match usize::try_from(image) {
            Ok(image) if image < images => Ok(image),
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
            return Ok(Image::Unreadable(mime.to_owned()));
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
        // The header alone: no pixel is decoded here.
        let (width, height) = reader
            .into_dimensions()
            .map_err(|error| invalid(format_args!("cannot be read as {format}: {error}")))?;
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

/// `bytes` at `cache/name`, written unless a file there already holds
/// them. The name is a content hash and an extension, so nothing in the
/// model chooses where it goes. The bytes go to a new temporary file
/// first (never one already there, nor through a link) and are renamed
/// into place, so a reader never sees half a file.
fn write_cached(cache: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, LoadError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = cache.join(name);
    let holds_bytes = |path: &Path| {
        fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() == bytes.len() as u64)
            && fs::read(path).is_ok_and(|held| held == bytes)
    };
    if holds_bytes(&path) {
        return Ok(path);
    }
    let temporary = cache.join(format!(
        "{name}.{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .and_then(|mut file| file.write_all(bytes))
        .and_then(|()| fs::rename(&temporary, &path));
    match written {
        Ok(()) => Ok(path),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            // Another process may have put the same bytes there first.
            if holds_bytes(&path) {
                Ok(path)
            } else {
                Err(cache_error(&path, error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::synthetic::SyntheticVrm;
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

    fn mtoon(materials: &Materials, index: usize) -> &MtoonMaterial {
        match &materials.materials[index].kind {
            MaterialKind::Mtoon(mtoon) => mtoon,
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
        assert_eq!(materials.materials[1].alpha_mode, AlphaMode::Blend);
        assert_eq!(mtoon(&materials, 1).render_queue_offset_number, -1);
        assert_eq!(materials.materials[0].alpha_mode, AlphaMode::Opaque);
        assert!(!materials.materials[0].double_sided);
    }

    #[test]
    fn pbr_is_drawn_unlit_with_a_notice() {
        let (materials, _cache) = load(&SyntheticVrm::avatar());
        assert_eq!(unlit(&materials, 2).base_color, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(unlit(&materials, 3).base_color, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(materials.notices.len(), 1, "{:?}", materials.notices);
        let notice = &materials.notices[0];
        assert_eq!(notice.level, NoticeLevel::Info);
        assert!(
            notice.message.contains("drawn as unlit base colour"),
            "{}",
            notice.message
        );
        assert!(notice.message.contains("Pbr"), "{}", notice.message);
        assert!(!notice.message.contains("Unlit"), "{}", notice.message);
        assert!(notice.message.starts_with("1 glTF PBR material(s)"));
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

    /// A PNG of `width × height` that has a signature, an IHDR, an empty
    /// IDAT, and an IEND: a header that claims the size and no pixels. (The
    /// header is read up to the first IDAT, which must be there.)
    fn png_header(width: u32, height: u32) -> Vec<u8> {
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
        // 8 bits a channel, RGBA, deflate, adaptive filters, no interlace.
        ihdr.extend([8, 6, 0, 0, 0]);
        chunk(&mut png, b"IHDR", &ihdr);
        chunk(&mut png, b"IDAT", &[]);
        chunk(&mut png, b"IEND", &[]);
        png
    }

    #[test]
    fn texture_budget_is_refused() {
        let mut vrm = SyntheticVrm::avatar();
        vrm.images[0].1 = png_header(32768, 32768);
        // Image 0 in one scene texture only (the shading shift samples it
        // in linear, a second).
        let message = refusal_of(&vrm, |root| {
            mtoon_extension(root, 0)
                .as_object_mut()
                .unwrap()
                .remove("shadingShiftTexture");
        });
        assert!(
            message.contains("4096 MiB, more than 1024 MiB"),
            "{message}"
        );
    }

    /// Celesta decodes each scene texture's file on its own, so an image
    /// sampled two ways counts twice.
    #[test]
    fn the_budget_counts_every_scene_texture() {
        let mut vrm = SyntheticVrm::avatar();
        // 1,024,000,000 bytes decoded: under the budget once, over it twice.
        vrm.images[0].1 = png_header(16000, 16000);
        let once = |root: &mut Value| {
            mtoon_extension(root, 0)
                .as_object_mut()
                .unwrap()
                .remove("shadingShiftTexture");
        };
        let (materials, _cache) = load_with(&vrm, once);
        assert_eq!(materials.textures.len(), 2);
        // The same image in sRGB through sampler 1: another scene texture.
        let message = refusal_of(&vrm, |root| {
            once(root);
            mtoon_extension(root, 0)["matcapTexture"] = json!({ "index": 2 });
        });
        assert!(
            message.contains("1953 MiB, more than 1024 MiB"),
            "{message}"
        );
    }

    #[test]
    fn a_refused_model_writes_nothing() {
        let mut vrm = SyntheticVrm::avatar();
        vrm.images[0].1 = png_header(32768, 32768);
        let document = document(&vrm, |_| {});
        let cache = TempDir::new().unwrap();
        assert!(materials(&document, cache.path()).is_err());
        assert_eq!(files(cache.path()), 0);
    }

    #[test]
    fn unreadable_images_leave_the_slot_empty() {
        let mut vrm = SyntheticVrm::avatar();
        vrm.images[1].0 = "image/ktx2".into();
        let (materials, cache) = load(&vrm);
        assert_eq!(mtoon(&materials, 0).normal_texture, None);
        let warnings: Vec<&str> = materials
            .notices
            .iter()
            .filter(|notice| notice.level == NoticeLevel::Warning)
            .map(|notice| notice.message.as_str())
            .collect();
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
            extension["transparentWithZWrite"] = json!(true);
            extension["renderQueueOffsetNumber"] = json!(1_000_000_000_000_i64);
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
        assert!(body.transparent_with_z_write);
        assert_eq!(body.render_queue_offset_number, i32::MAX);
        // What the file leaves out keeps UniVRM's default.
        assert_eq!(
            body.shading_toony_factor,
            MtoonMaterial::default().shading_toony_factor
        );
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
        let message = refusal(|root| {
            root["materials"][1]["alphaMode"] = json!("MASK");
            root["materials"][1]["alphaCutoff"] = json!(1e39);
        });
        assert!(
            message.contains("material \"Hair\" has an alphaCutoff that is not finite"),
            "{message}"
        );
    }

    #[test]
    fn the_cache_is_the_environment_s_or_the_temp_directory() {
        let dir = texture_cache_dir();
        match std::env::var_os("CELESTA_VRM_TEXTURE_CACHE") {
            Some(set) if !set.is_empty() => assert_eq!(dir, Path::new(&set)),
            _ => assert_eq!(dir, std::env::temp_dir().join("celesta-plugin-vrm")),
        }
    }
}
