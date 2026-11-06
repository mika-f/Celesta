//! The plugin protocol, version 1: length-prefixed messages over a plugin
//! process's stdin and stdout. Each message is a JSON header and a binary
//! payload of little-endian arrays the header describes:
//!
//! ```text
//! u32 LE header length, header (UTF-8 JSON object),
//! u32 LE payload length, payload
//! ```
//!
//! See `docs/handoff/2026-11-01-plugin-system.md` for the requests.

use crate::mesh::{MeshBlend, MeshFrame, MeshInfo, MeshMask, MeshModel, MeshState, TextureSource};
use crate::provider::{ContentFrame, ContentModel, ContentProvider, ModelId, Notice, Opened};
use crate::scene::{
    Camera, ColorSpace, DrawInfo, DrawState, Geometry, Lights, Material, MaterialKind,
    MaterialOverrides, MorphTarget, Sampler, SceneFrame, SceneModel, SceneShader, SceneTexture,
    Skin,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub const PROTOCOL_VERSION: u32 = 1;

/// What a content plugin answers `hello` with as its roles.
pub const ROLES: &[&str] = &["content"];

/// Which kind of model an `opened` answer carries, as its `kind` says. A
/// model's frames are of the same kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentKind {
    /// 2D meshes, which an `opened` answer without `kind` carries too.
    #[default]
    Meshes,
    Scene,
}

impl ContentModel {
    /// The kind this model is sent as.
    pub fn kind(&self) -> ContentKind {
        match self {
            Self::Meshes(_) => ContentKind::Meshes,
            Self::Scene(_) => ContentKind::Scene,
        }
    }
}

/// The largest payload either side accepts.
const MAX_PAYLOAD: u32 = 1 << 30;

/// The largest JSON header either side accepts: headers describe the
/// payload's arrays rather than hold them.
const MAX_HEADER: u32 = 16 << 20;

/// What a part's buffer starts with before its bytes arrive; it grows as
/// they do, so a bogus length costs no more than the bytes really sent.
const PREALLOCATED: u32 = 1 << 20;

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("malformed message: {0}")]
    Malformed(String),
}

fn malformed(message: impl ToString) -> ProtocolError {
    ProtocolError::Malformed(message.to_string())
}

/// One message: its JSON header and binary payload.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub header: Value,
    pub payload: Vec<u8>,
}

impl Message {
    pub fn new(header: Value) -> Self {
        Self {
            header,
            payload: Vec::new(),
        }
    }

    /// The header's `type`, or "" without one.
    pub fn kind(&self) -> &str {
        self.header["type"].as_str().unwrap_or("")
    }
}

pub fn write_message(writer: &mut impl Write, message: &Message) -> io::Result<()> {
    let header = serde_json::to_vec(&message.header)?;
    for (part, max) in [
        (&header[..], MAX_HEADER),
        (&message.payload[..], MAX_PAYLOAD),
    ] {
        let length = u32::try_from(part.len())
            .ok()
            .filter(|length| *length <= max)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "message part too large"))?;
        writer.write_all(&length.to_le_bytes())?;
        writer.write_all(part)?;
    }
    writer.flush()
}

/// A part of at most `max` bytes. Its buffer grows as its bytes arrive, so
/// a length that is not one (such as text a plugin printed into the stream)
/// is not allocated up front.
fn read_part(reader: &mut impl Read, max: u32, what: &str) -> Result<Vec<u8>, ProtocolError> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length);
    if length > max {
        return Err(malformed(format!(
            "a {what} of {length} bytes, more than {max}"
        )));
    }
    let mut part = Vec::with_capacity(length.min(PREALLOCATED) as usize);
    reader.take(u64::from(length)).read_to_end(&mut part)?;
    if part.len() != length as usize {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!(
                "the stream ended {} bytes into a {length}-byte {what}",
                part.len()
            ),
        )
        .into());
    }
    Ok(part)
}

/// The next message, or `None` when the stream ends between messages.
pub fn read_message(reader: &mut impl Read) -> Result<Option<Message>, ProtocolError> {
    let header = match read_part(reader, MAX_HEADER, "header") {
        Ok(header) => header,
        Err(ProtocolError::Io(error)) if error.kind() == io::ErrorKind::UnexpectedEof => {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let header: Value = serde_json::from_slice(&header).map_err(malformed)?;
    if !header.is_object() {
        return Err(malformed("the header is not a JSON object"));
    }
    let payload = read_part(reader, MAX_PAYLOAD, "payload")?;
    Ok(Some(Message { header, payload }))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenedHeader {
    model: u64,
    canvas: [f32; 2],
    textures: Vec<TextureHeader>,
    meshes: Vec<MeshHeader>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
enum TextureHeader {
    Path {
        path: PathBuf,
    },
    /// Pixels follow the meshes' arrays in the payload, in texture order.
    Rgba {
        width: u32,
        height: u32,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeshHeader {
    texture: usize,
    #[serde(default)]
    blend: MeshBlend,
    #[serde(default)]
    double_sided: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mask: Option<MeshMask>,
    vertices: u32,
    indices: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameHeader {
    meshes: Vec<StateHeader>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateHeader {
    index: usize,
    visible: bool,
    opacity: f32,
    multiply: [f32; 3],
    screen: [f32; 3],
    /// How many positions this mesh has in the payload.
    vertices: u32,
}

/// What every `opened` header has beside its kind's own fields.
#[derive(Deserialize)]
struct OpenedCommon {
    #[serde(default)]
    kind: ContentKind,
    #[serde(default)]
    notices: Vec<Notice>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SceneOpenedHeader {
    model: u64,
    canvas: [f32; 2],
    textures: Vec<SceneTextureHeader>,
    geometries: Vec<GeometryHeader>,
    skins: Vec<Skin>,
    #[serde(default)]
    shaders: Vec<SceneShader>,
    materials: Vec<Material>,
    /// How many bytes of parameters each material has in the payload, after
    /// the inline pixels, or none when no material has any.
    #[serde(default)]
    material_params: Vec<u32>,
    draws: Vec<DrawInfo>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SceneTextureHeader {
    #[serde(flatten)]
    source: TextureHeader,
    color_space: ColorSpace,
    sampler: Sampler,
}

/// A geometry's counts and which optional streams the payload has for it.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeometryHeader {
    vertices: u32,
    indices: u32,
    #[serde(default)]
    tangents: bool,
    /// Joints and weights follow the tangents.
    #[serde(default)]
    skinned: bool,
    #[serde(default)]
    targets: u32,
    /// Every target has normals after its positions. When only some of a
    /// geometry's targets change normals, the others send zeros, which
    /// leave the normals as they are just as no normals do.
    #[serde(default)]
    target_normals: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SceneFrameHeader {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    canvas: Option<[f32; 2]>,
    camera: Camera,
    lights: Lights,
    time: f64,
    draws: Vec<DrawStateHeader>,
    skins: Vec<Skin>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DrawStateHeader {
    index: usize,
    visible: bool,
    world: [f32; 16],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    center: Option<[f32; 3]>,
    #[serde(default)]
    overrides: MaterialOverrides,
    /// How many morph weights this draw has in the payload.
    #[serde(default)]
    weights: u32,
    /// How many bytes of parameters follow this draw's morph weights, when
    /// it has any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    params: Option<u32>,
}

fn push_f32(payload: &mut Vec<u8>, values: impl IntoIterator<Item = f32>) {
    for value in values {
        payload.extend_from_slice(&value.to_le_bytes());
    }
}

fn push_u32(payload: &mut Vec<u8>, values: &[u32]) {
    for value in values {
        payload.extend_from_slice(&value.to_le_bytes());
    }
}

fn texture_header(texture: &TextureSource) -> TextureHeader {
    match texture {
        TextureSource::Path(path) => TextureHeader::Path { path: path.clone() },
        TextureSource::Rgba { width, height, .. } => TextureHeader::Rgba {
            width: *width,
            height: *height,
        },
    }
}

/// Appends inline textures' pixels, in texture order.
fn push_pixels<'a>(payload: &mut Vec<u8>, textures: impl IntoIterator<Item = &'a TextureSource>) {
    for texture in textures {
        if let TextureSource::Rgba { pixels, .. } = texture {
            payload.extend_from_slice(pixels);
        }
    }
}

/// The texture `header` describes, taking its pixels from `cursor` when
/// they are inline.
fn texture_source(
    header: TextureHeader,
    cursor: &mut Cursor<'_>,
) -> Result<TextureSource, ProtocolError> {
    Ok(match header {
        TextureHeader::Path { path } => TextureSource::Path(path),
        TextureHeader::Rgba { width, height } => {
            let length = (width as usize)
                .checked_mul(height as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or_else(|| malformed("a texture too large"))?;
            TextureSource::Rgba {
                width,
                height,
                pixels: Arc::new(cursor.take(length)?.to_vec()),
            }
        }
    })
}

/// Reads the payload front to back.
struct Cursor<'a> {
    bytes: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], ProtocolError> {
        if self.bytes.len() < length {
            return Err(malformed("the payload is shorter than its header says"));
        }
        let (taken, rest) = self.bytes.split_at(length);
        self.bytes = rest;
        Ok(taken)
    }

    /// `count` groups of `N` floats.
    fn floats<const N: usize>(&mut self, count: u32) -> Result<Vec<[f32; N]>, ProtocolError> {
        let bytes = self.take(count as usize * N * 4)?;
        Ok(bytes
            .chunks_exact(N * 4)
            .map(|group| {
                std::array::from_fn(|i| {
                    f32::from_le_bytes(group[i * 4..i * 4 + 4].try_into().expect("four bytes"))
                })
            })
            .collect())
    }

    fn f32s(&mut self, count: u32) -> Result<Vec<f32>, ProtocolError> {
        Ok(self.floats::<1>(count)?.into_iter().flatten().collect())
    }

    fn pairs(&mut self, count: u32) -> Result<Vec<[f32; 2]>, ProtocolError> {
        self.floats(count)
    }

    fn triples(&mut self, count: u32) -> Result<Vec<[f32; 3]>, ProtocolError> {
        self.floats(count)
    }

    fn quads(&mut self, count: u32) -> Result<Vec<[f32; 4]>, ProtocolError> {
        self.floats(count)
    }

    fn matrices(&mut self, count: u32) -> Result<Vec<[f32; 16]>, ProtocolError> {
        self.floats(count)
    }

    fn u16x4(&mut self, count: u32) -> Result<Vec<[u16; 4]>, ProtocolError> {
        let bytes = self.take(count as usize * 8)?;
        Ok(bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|group| {
                std::array::from_fn(|i| u16::from_le_bytes([group[i * 2], group[i * 2 + 1]]))
            })
            .collect())
    }

    fn u32s(&mut self, count: u32) -> Result<Vec<u32>, ProtocolError> {
        let bytes = self.take(count as usize * 4)?;
        Ok(bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|value| u32::from_le_bytes(*value))
            .collect())
    }

    fn finish(self) -> Result<(), ProtocolError> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(malformed("the payload is longer than its header says"))
        }
    }
}

/// `{"type":"opened"}` for 2D meshes, without `kind` or `notices`: a
/// model's header, then its meshes' UVs, their indices, and inline
/// textures' pixels.
fn encode_model(id: ModelId, model: &MeshModel) -> Message {
    let header = OpenedHeader {
        model: id.0,
        canvas: model.canvas,
        textures: model.textures.iter().map(texture_header).collect(),
        meshes: model
            .meshes
            .iter()
            .map(|mesh| MeshHeader {
                texture: mesh.texture,
                blend: mesh.blend,
                double_sided: mesh.double_sided,
                mask: mesh.mask.clone(),
                vertices: mesh.uvs.len() as u32,
                indices: mesh.indices.len() as u32,
            })
            .collect(),
    };
    let mut payload = Vec::new();
    for mesh in &model.meshes {
        push_f32(&mut payload, mesh.uvs.iter().flatten().copied());
    }
    for mesh in &model.meshes {
        push_u32(&mut payload, &mesh.indices);
    }
    push_pixels(&mut payload, &model.textures);
    let mut header = serde_json::to_value(header).expect("serializable");
    header["type"] = json!("opened");
    Message { header, payload }
}

/// The 2D meshes an `opened` answer carries, ignoring its `kind`.
pub fn decode_model(message: &Message) -> Result<(ModelId, MeshModel), ProtocolError> {
    let header = OpenedHeader::deserialize(&message.header).map_err(malformed)?;
    let mut cursor = Cursor {
        bytes: &message.payload,
    };
    let uvs: Vec<_> = header
        .meshes
        .iter()
        .map(|mesh| cursor.pairs(mesh.vertices))
        .collect::<Result<_, _>>()?;
    let indices: Vec<_> = header
        .meshes
        .iter()
        .map(|mesh| cursor.u32s(mesh.indices))
        .collect::<Result<_, _>>()?;
    let textures: Vec<_> = header
        .textures
        .into_iter()
        .map(|texture| texture_source(texture, &mut cursor))
        .collect::<Result<_, _>>()?;
    cursor.finish()?;
    let meshes = header
        .meshes
        .into_iter()
        .zip(uvs.into_iter().zip(indices))
        .map(|(mesh, (uvs, indices))| MeshInfo {
            texture: mesh.texture,
            blend: mesh.blend,
            double_sided: mesh.double_sided,
            mask: mesh.mask,
            uvs,
            indices,
        })
        .collect();
    Ok((
        ModelId(header.model),
        MeshModel {
            canvas: header.canvas,
            textures,
            meshes,
        },
    ))
}

/// `{"type":"frame"}` for 2D meshes: every mesh's state, then their
/// positions in order.
fn encode_frame(frame: &MeshFrame) -> Message {
    let header = FrameHeader {
        meshes: frame
            .meshes
            .iter()
            .map(|mesh| StateHeader {
                index: mesh.index,
                visible: mesh.visible,
                opacity: mesh.opacity,
                multiply: mesh.multiply,
                screen: mesh.screen,
                vertices: mesh.positions.len() as u32,
            })
            .collect(),
    };
    let mut payload = Vec::new();
    for mesh in &frame.meshes {
        push_f32(&mut payload, mesh.positions.iter().flatten().copied());
    }
    let mut header = serde_json::to_value(header).expect("serializable");
    header["type"] = json!("frame");
    Message { header, payload }
}

/// A 2D meshes model's frame.
pub fn decode_frame(message: &Message) -> Result<MeshFrame, ProtocolError> {
    let header = FrameHeader::deserialize(&message.header).map_err(malformed)?;
    let mut cursor = Cursor {
        bytes: &message.payload,
    };
    let meshes = header
        .meshes
        .into_iter()
        .map(|mesh| {
            Ok(MeshState {
                index: mesh.index,
                visible: mesh.visible,
                opacity: mesh.opacity,
                multiply: mesh.multiply,
                screen: mesh.screen,
                positions: cursor.pairs(mesh.vertices)?,
            })
        })
        .collect::<Result<_, ProtocolError>>()?;
    cursor.finish()?;
    Ok(MeshFrame { meshes })
}

/// `{"type":"opened"}` for a scene, without `kind` or `notices`: its
/// header, then each geometry's vertex streams and morph targets, every
/// geometry's indices, inline textures' pixels, and each custom material's
/// parameters.
fn encode_scene_model(id: ModelId, model: &SceneModel) -> Message {
    let header = SceneOpenedHeader {
        model: id.0,
        canvas: model.canvas,
        textures: model
            .textures
            .iter()
            .map(|texture| SceneTextureHeader {
                source: texture_header(&texture.source),
                color_space: texture.color_space,
                sampler: texture.sampler,
            })
            .collect(),
        geometries: model
            .geometries
            .iter()
            .map(|geometry| GeometryHeader {
                vertices: geometry.positions.len() as u32,
                indices: geometry.indices.len() as u32,
                tangents: geometry.tangents.is_some(),
                skinned: geometry.joints.is_some() && geometry.weights.is_some(),
                targets: geometry.targets.len() as u32,
                target_normals: geometry
                    .targets
                    .iter()
                    .any(|target| !target.normals.is_empty()),
            })
            .collect(),
        skins: model.skins.clone(),
        shaders: model.shaders.clone(),
        materials: model.materials.clone(),
        material_params: if model
            .materials
            .iter()
            .all(|material| material_params(material).is_empty())
        {
            Vec::new()
        } else {
            model
                .materials
                .iter()
                .map(|material| material_params(material).len() as u32)
                .collect()
        },
        draws: model.draws.clone(),
    };
    let mut payload = Vec::new();
    for (geometry, info) in model.geometries.iter().zip(&header.geometries) {
        push_f32(&mut payload, geometry.positions.iter().flatten().copied());
        push_f32(&mut payload, geometry.normals.iter().flatten().copied());
        push_f32(&mut payload, geometry.uvs.iter().flatten().copied());
        if let Some(tangents) = &geometry.tangents {
            push_f32(&mut payload, tangents.iter().flatten().copied());
        }
        if let (Some(joints), Some(weights)) = (&geometry.joints, &geometry.weights) {
            for value in joints.iter().flatten() {
                payload.extend_from_slice(&value.to_le_bytes());
            }
            push_f32(&mut payload, weights.iter().flatten().copied());
        }
        for target in &geometry.targets {
            push_f32(&mut payload, target.positions.iter().flatten().copied());
            if info.target_normals {
                if target.normals.is_empty() {
                    push_f32(
                        &mut payload,
                        std::iter::repeat_n(0.0, geometry.positions.len() * 3),
                    );
                } else {
                    push_f32(&mut payload, target.normals.iter().flatten().copied());
                }
            }
        }
    }
    for geometry in &model.geometries {
        push_u32(&mut payload, &geometry.indices);
    }
    push_pixels(
        &mut payload,
        model.textures.iter().map(|texture| &texture.source),
    );
    for material in &model.materials {
        payload.extend_from_slice(material_params(material));
    }
    let mut header = serde_json::to_value(header).expect("serializable");
    header["type"] = json!("opened");
    Message { header, payload }
}

/// The parameter bytes `material` sends in the payload: a custom
/// material's, and none for the others.
fn material_params(material: &Material) -> &[u8] {
    match &material.kind {
        MaterialKind::Custom(custom) => &custom.params,
        MaterialKind::Unlit(_) => &[],
    }
}

fn decode_scene_model(message: &Message) -> Result<(ModelId, SceneModel), ProtocolError> {
    let header = SceneOpenedHeader::deserialize(&message.header).map_err(malformed)?;
    let mut materials = header.materials;
    let params = &header.material_params;
    if !(params.is_empty() || params.len() == materials.len()) {
        return Err(malformed(format!(
            "{} materialParams for {} materials",
            params.len(),
            materials.len()
        )));
    }
    for (index, (material, &bytes)) in materials.iter().zip(params).enumerate() {
        if bytes > 0 && !matches!(material.kind, MaterialKind::Custom(_)) {
            return Err(malformed(format!(
                "material {index} is not custom but has {bytes} parameter bytes"
            )));
        }
    }
    let mut cursor = Cursor {
        bytes: &message.payload,
    };
    let mut geometries = Vec::with_capacity(header.geometries.len());
    for info in &header.geometries {
        let count = info.vertices;
        let positions = cursor.triples(count)?;
        let normals = cursor.triples(count)?;
        let uvs = cursor.pairs(count)?;
        let tangents = if info.tangents {
            Some(cursor.quads(count)?)
        } else {
            None
        };
        let (joints, weights) = if info.skinned {
            (Some(cursor.u16x4(count)?), Some(cursor.quads(count)?))
        } else {
            (None, None)
        };
        // A target's size, not the header's count, bounds how many are made:
        // targets without bytes, or more than the payload holds, are refused.
        let target_bytes = count as usize * 12 * if info.target_normals { 2 } else { 1 };
        if info.targets > 0
            && (target_bytes == 0
                || (info.targets as usize)
                    .checked_mul(target_bytes)
                    .is_none_or(|bytes| bytes > cursor.bytes.len()))
        {
            return Err(malformed(format!(
                "{} morph targets of {count} vertices",
                info.targets
            )));
        }
        let targets = (0..info.targets)
            .map(|_| {
                Ok(MorphTarget {
                    positions: cursor.triples(count)?,
                    normals: if info.target_normals {
                        cursor.triples(count)?
                    } else {
                        Vec::new()
                    },
                })
            })
            .collect::<Result<_, ProtocolError>>()?;
        geometries.push(Geometry {
            positions,
            normals,
            uvs,
            tangents,
            joints,
            weights,
            targets,
            indices: Vec::new(),
        });
    }
    for (geometry, info) in geometries.iter_mut().zip(&header.geometries) {
        geometry.indices = cursor.u32s(info.indices)?;
    }
    let textures = header
        .textures
        .into_iter()
        .map(|texture| {
            Ok(SceneTexture {
                source: texture_source(texture.source, &mut cursor)?,
                color_space: texture.color_space,
                sampler: texture.sampler,
            })
        })
        .collect::<Result<_, ProtocolError>>()?;
    for (material, &bytes) in materials.iter_mut().zip(params) {
        if let MaterialKind::Custom(custom) = &mut material.kind {
            custom.params = cursor.take(bytes as usize)?.to_vec();
        }
    }
    cursor.finish()?;
    Ok((
        ModelId(header.model),
        SceneModel {
            canvas: header.canvas,
            textures,
            geometries,
            skins: header.skins,
            shaders: header.shaders,
            materials,
            draws: header.draws,
        },
    ))
}

/// `{"type":"frame"}` for a scene: the frame's own canvas when it has one,
/// the camera, lights, and every draw's state, then every skin's skinning matrices and every draw's morph
/// weights, each followed by the draw's parameters when it has any, in
/// order.
fn encode_scene_frame(frame: &SceneFrame) -> Message {
    let header = SceneFrameHeader {
        canvas: frame.canvas,
        camera: frame.camera,
        lights: frame.lights.clone(),
        time: frame.time,
        draws: frame
            .draws
            .iter()
            .map(|draw| DrawStateHeader {
                index: draw.index,
                visible: draw.visible,
                world: draw.world,
                center: draw.center,
                overrides: draw.overrides,
                weights: draw.morph_weights.len() as u32,
                params: draw.params.as_ref().map(|params| params.len() as u32),
            })
            .collect(),
        skins: frame
            .skins
            .iter()
            .map(|matrices| Skin {
                joints: matrices.len() as u32,
            })
            .collect(),
    };
    let mut payload = Vec::new();
    for matrices in &frame.skins {
        push_f32(&mut payload, matrices.iter().flatten().copied());
    }
    for draw in &frame.draws {
        push_f32(&mut payload, draw.morph_weights.iter().copied());
        if let Some(params) = &draw.params {
            payload.extend_from_slice(params);
        }
    }
    let mut header = serde_json::to_value(header).expect("serializable");
    header["type"] = json!("frame");
    Message { header, payload }
}

fn decode_scene_frame(message: &Message) -> Result<SceneFrame, ProtocolError> {
    let header = SceneFrameHeader::deserialize(&message.header).map_err(malformed)?;
    let mut cursor = Cursor {
        bytes: &message.payload,
    };
    let skins = header
        .skins
        .iter()
        .map(|skin| cursor.matrices(skin.joints))
        .collect::<Result<_, _>>()?;
    let draws = header
        .draws
        .into_iter()
        .map(|draw| {
            let morph_weights = cursor.f32s(draw.weights)?;
            let params = draw
                .params
                .map(|bytes| cursor.take(bytes as usize).map(<[u8]>::to_vec))
                .transpose()?;
            Ok(DrawState {
                index: draw.index,
                visible: draw.visible,
                world: draw.world,
                morph_weights,
                center: draw.center,
                overrides: draw.overrides,
                params,
            })
        })
        .collect::<Result<_, ProtocolError>>()?;
    cursor.finish()?;
    Ok(SceneFrame {
        canvas: header.canvas,
        camera: header.camera,
        lights: header.lights,
        time: header.time,
        draws,
        skins,
    })
}

/// `{"type":"opened"}`: the model's header with its `kind` and any
/// `notices`, and its payload.
pub fn encode_opened(opened: &Opened) -> Message {
    let mut message = match &opened.content {
        ContentModel::Meshes(model) => encode_model(opened.model, model),
        ContentModel::Scene(model) => encode_scene_model(opened.model, model),
    };
    message.header["kind"] = json!(opened.content.kind());
    if !opened.notices.is_empty() {
        message.header["notices"] = json!(opened.notices);
    }
    message
}

/// The model an `opened` answer carries, of the kind its `kind` names.
pub fn decode_opened(message: &Message) -> Result<Opened, ProtocolError> {
    let common = OpenedCommon::deserialize(&message.header).map_err(malformed)?;
    let (model, content) = match common.kind {
        ContentKind::Meshes => {
            let (model, content) = decode_model(message)?;
            (model, content.into())
        }
        ContentKind::Scene => {
            let (model, content) = decode_scene_model(message)?;
            (model, content.into())
        }
    };
    Ok(Opened {
        model,
        content,
        notices: common.notices,
    })
}

/// `{"type":"frame"}`: a frame of either kind.
pub fn encode_content_frame(frame: &ContentFrame) -> Message {
    match frame {
        ContentFrame::Meshes(frame) => encode_frame(frame),
        ContentFrame::Scene(frame) => encode_scene_frame(frame),
    }
}

/// A frame of a model opened as `kind`: a frame's header does not say its
/// kind.
pub fn decode_content_frame(
    message: &Message,
    kind: ContentKind,
) -> Result<ContentFrame, ProtocolError> {
    match kind {
        ContentKind::Meshes => decode_frame(message).map(ContentFrame::Meshes),
        ContentKind::Scene => decode_scene_frame(message).map(ContentFrame::Scene),
    }
}

pub fn error_message(message: impl ToString) -> Message {
    Message::new(json!({ "type": "error", "message": message.to_string() }))
}

/// Answers requests from `reader` on `writer` with `provider` until the
/// reader ends: a plugin process's main loop, on its stdin and stdout.
/// `plugin` and `version` are what the plugin says it is.
pub fn serve_on(
    provider: &mut impl ContentProvider,
    plugin: &str,
    version: &str,
    reader: impl Read,
    writer: impl Write,
) -> Result<(), ProtocolError> {
    serve_requests_on(plugin, version, ROLES, reader, writer, |request| {
        content_answer(provider, plugin, request)
    })
}

/// What a content plugin answers a request other than `hello` with.
fn content_answer(provider: &mut impl ContentProvider, plugin: &str, request: Message) -> Message {
    match request.kind() {
        "open" => match BTreeMap::<String, PathBuf>::deserialize(&request.header["assets"]) {
            Ok(assets) => match provider.open(plugin, &assets) {
                Ok(opened) => encode_opened(&opened),
                Err(error) => error_message(error),
            },
            Err(error) => error_message(format!("invalid assets: {error}")),
        },
        "frame" => match (
            request.header["model"].as_u64(),
            request.header["time"].as_f64(),
        ) {
            (Some(model), Some(time)) => {
                match provider.frame(ModelId(model), time, &request.header["data"]) {
                    Ok(frame) => encode_content_frame(&frame),
                    Err(error) => error_message(error),
                }
            }
            _ => error_message("a frame request needs `model` and `time`"),
        },
        "close" => match request.header["model"].as_u64() {
            Some(model) => {
                provider.close(ModelId(model));
                Message::new(json!({ "type": "closed" }))
            }
            None => error_message("a close request needs `model`"),
        },
        other => error_message(format!("unknown request \"{other}\"")),
    }
}

/// Answers requests from `reader` on `writer` until the reader ends: the
/// `hello` handshake here, naming `plugin`, `version`, and `roles`, and every
/// other request with `answer`. A plugin of a role other than content (a
/// codec, say) runs its loop through this.
pub fn serve_requests_on(
    plugin: &str,
    version: &str,
    roles: &[&str],
    mut reader: impl Read,
    mut writer: impl Write,
    mut answer: impl FnMut(Message) -> Message,
) -> Result<(), ProtocolError> {
    while let Some(request) = read_message(&mut reader)? {
        let answer = match request.kind() {
            "hello" => {
                if request.header["protocol"].as_u64() == Some(u64::from(PROTOCOL_VERSION)) {
                    Message::new(json!({
                        "type": "hello",
                        "protocol": PROTOCOL_VERSION,
                        "plugin": plugin,
                        "version": version,
                        "roles": roles,
                    }))
                } else {
                    error_message(format!(
                        "this plugin speaks protocol {PROTOCOL_VERSION}, not {}",
                        request.header["protocol"]
                    ))
                }
            }
            _ => answer(request),
        };
        write_message(&mut writer, &answer)?;
    }
    Ok(())
}

/// [`serve_on`] this process's stdin and stdout (see [`serve_requests`]).
pub fn serve(
    provider: &mut impl ContentProvider,
    plugin: &str,
    version: &str,
) -> Result<(), ProtocolError> {
    serve_requests(plugin, version, ROLES, |request| {
        content_answer(provider, plugin, request)
    })
}

/// [`serve_requests_on`] this process's stdin and stdout. The answers go
/// through a duplicate of stdout taken first, and stdout itself is pointed
/// at stderr: whatever else the process prints, `println!` and native
/// libraries' banners and logs included, lands in Celesta's log instead of
/// corrupting the protocol.
pub fn serve_requests(
    plugin: &str,
    version: &str,
    roles: &[&str],
    answer: impl FnMut(Message) -> Message,
) -> Result<(), ProtocolError> {
    let stdin = io::stdin().lock();
    serve_requests_on(
        plugin,
        version,
        roles,
        stdin,
        io::BufWriter::new(protocol_output()?),
        answer,
    )
}

#[cfg(unix)]
fn protocol_output() -> io::Result<std::fs::File> {
    use std::os::fd::AsFd;
    let protocol = io::stdout().as_fd().try_clone_to_owned()?;
    // SAFETY: `dup2` on the process's own standard descriptors.
    if unsafe { libc::dup2(libc::STDERR_FILENO, libc::STDOUT_FILENO) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(std::fs::File::from(protocol))
}

/// As on Unix, for the C runtime's descriptors and the process's standard
/// handles: Rust's `io::stdout()` writes to the `STD_OUTPUT_HANDLE` of the
/// moment, and C's `printf` to descriptor 1.
#[cfg(windows)]
fn protocol_output() -> io::Result<std::fs::File> {
    use std::os::windows::io::{AsHandle, RawHandle};

    /// `STD_OUTPUT_HANDLE`, `(DWORD)-11`.
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    /// `INVALID_HANDLE_VALUE`, which `_get_osfhandle` also answers for a
    /// descriptor that is not open.
    const INVALID_HANDLE_VALUE: isize = -1;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetStdHandle(std_handle: u32, handle: RawHandle) -> i32;
    }

    // A handle of its own: `_dup2` closes descriptor 1's.
    let protocol = io::stdout().as_handle().try_clone_to_owned()?;
    // SAFETY: `_dup2` and `_get_osfhandle` on the C runtime's own standard
    // descriptors; `SetStdHandle` with the handle descriptor 1 now owns,
    // which the C runtime keeps open for the life of the process.
    unsafe {
        // The C runtime reports through `errno`, not `GetLastError`.
        if libc::dup2(2, 1) < 0 {
            return Err(io::Error::other("stdout could not be pointed at stderr"));
        }
        let stdout = libc::get_osfhandle(1);
        if stdout == INVALID_HANDLE_VALUE {
            return Err(io::Error::other("descriptor 1 has no handle"));
        }
        if SetStdHandle(STD_OUTPUT_HANDLE, stdout as RawHandle) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(std::fs::File::from(protocol))
}

#[cfg(not(any(unix, windows)))]
fn protocol_output() -> io::Result<io::StdoutLock<'static>> {
    Ok(io::stdout().lock())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{NoticeLevel, PluginError};
    use crate::scene::{
        AlphaMode, CustomMaterial, DirectionalLight, EmptyTexel, Filter, MaterialKind, OutlinePass,
        Projection, SceneShader, ShaderTexture, TextureRef, TextureTransform, UnlitMaterial, Wrap,
    };

    fn model() -> MeshModel {
        MeshModel {
            canvas: [64.0, 32.0],
            textures: vec![
                TextureSource::Path("C:/a.png".into()),
                TextureSource::Rgba {
                    width: 1,
                    height: 2,
                    pixels: Arc::new(vec![1, 2, 3, 4, 5, 6, 7, 8]),
                },
            ],
            meshes: vec![
                MeshInfo {
                    texture: 1,
                    blend: MeshBlend::Additive,
                    double_sided: true,
                    mask: None,
                    uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
                    indices: vec![0, 1, 2],
                },
                MeshInfo {
                    texture: 0,
                    blend: MeshBlend::Normal,
                    double_sided: false,
                    mask: Some(MeshMask {
                        meshes: vec![0],
                        inverted: true,
                    }),
                    uvs: vec![[0.25, 0.75]],
                    indices: vec![],
                },
            ],
        }
    }

    fn frame(time: f64) -> MeshFrame {
        MeshFrame {
            meshes: vec![MeshState {
                index: 0,
                visible: true,
                opacity: 0.5,
                multiply: [1.0, 0.5, 0.25],
                screen: [0.0; 3],
                positions: vec![[time as f32, 1.0], [2.0, 3.0], [4.0, 5.0]],
            }],
        }
    }

    #[test]
    fn models_and_frames_survive_the_wire() {
        let mut bytes = Vec::new();
        write_message(&mut bytes, &encode_model(ModelId(3), &model())).unwrap();
        write_message(&mut bytes, &encode_frame(&frame(9.0))).unwrap();
        let mut reader = &bytes[..];
        let opened = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(decode_model(&opened).unwrap(), (ModelId(3), model()));
        let framed = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(decode_frame(&framed).unwrap(), frame(9.0));
        assert!(read_message(&mut reader).unwrap().is_none());
    }

    #[test]
    fn a_short_payload_is_malformed() {
        let mut message = encode_frame(&frame(0.0));
        message.payload.pop();
        assert!(matches!(
            decode_frame(&message),
            Err(ProtocolError::Malformed(_))
        ));
    }

    struct Fixed;

    impl ContentProvider for Fixed {
        fn open(
            &mut self,
            plugin: &str,
            assets: &BTreeMap<String, PathBuf>,
        ) -> Result<Opened, PluginError> {
            assert_eq!(plugin, "fixed");
            assert_eq!(assets["model"], PathBuf::from("C:/m.json"));
            Ok(Opened {
                model: ModelId(1),
                content: model().into(),
                notices: vec![],
            })
        }

        fn frame(
            &mut self,
            model: ModelId,
            time: f64,
            data: &Value,
        ) -> Result<ContentFrame, PluginError> {
            if data["fail"] == true {
                return Err(PluginError::Failed {
                    plugin: "fixed".into(),
                    message: "asked to".into(),
                });
            }
            assert_eq!(model, ModelId(1));
            Ok(frame(time).into())
        }
    }

    #[test]
    fn serves_a_session() {
        let mut requests = Vec::new();
        for header in [
            json!({ "type": "hello", "protocol": 1 }),
            json!({ "type": "open", "assets": { "model": "C:/m.json" } }),
            json!({ "type": "frame", "model": 1, "time": 2.5 }),
            json!({ "type": "frame", "model": 1, "time": 2.5, "data": { "fail": true } }),
            json!({ "type": "close", "model": 1 }),
        ] {
            write_message(&mut requests, &Message::new(header)).unwrap();
        }
        let mut answers = Vec::new();
        serve_on(&mut Fixed, "fixed", "1.0.0", &requests[..], &mut answers).unwrap();
        let mut reader = &answers[..];
        let mut next = || read_message(&mut reader).unwrap().unwrap();
        let hello = next();
        assert_eq!(hello.header["plugin"], "fixed");
        assert_eq!(decode_model(&next()).unwrap().1, model());
        assert_eq!(decode_frame(&next()).unwrap(), frame(2.5));
        let failed = next();
        assert_eq!(failed.kind(), "error");
        assert!(
            failed.header["message"]
                .as_str()
                .unwrap()
                .contains("asked to")
        );
        assert_eq!(next().kind(), "closed");
    }

    #[test]
    fn hello_names_roles() {
        let mut requests = Vec::new();
        write_message(
            &mut requests,
            &Message::new(json!({ "type": "hello", "protocol": 1 })),
        )
        .unwrap();
        let mut answers = Vec::new();
        serve_on(&mut Fixed, "fixed", "1.0.0", &requests[..], &mut answers).unwrap();
        let hello = read_message(&mut &answers[..]).unwrap().unwrap();
        assert_eq!(hello.header["roles"], json!(["content"]));
    }

    #[test]
    fn opened_without_kind_is_meshes() {
        let mut payload = Vec::new();
        push_f32(&mut payload, [0.5, 0.25]);
        let message = Message {
            header: json!({
                "type": "opened",
                "model": 2,
                "canvas": [4.0, 2.0],
                "textures": [{ "path": "C:/a.png" }],
                "meshes": [{ "texture": 0, "vertices": 1, "indices": 0 }],
            }),
            payload,
        };
        assert_eq!(
            decode_opened(&message).unwrap(),
            Opened {
                model: ModelId(2),
                content: MeshModel {
                    canvas: [4.0, 2.0],
                    textures: vec![TextureSource::Path("C:/a.png".into())],
                    meshes: vec![MeshInfo {
                        texture: 0,
                        blend: MeshBlend::Normal,
                        double_sided: false,
                        mask: None,
                        uvs: vec![[0.5, 0.25]],
                        indices: vec![],
                    }],
                }
                .into(),
                notices: vec![],
            }
        );
    }

    #[test]
    fn meshes_opened_adds_only_kind_and_notices() {
        let opened = Opened {
            model: ModelId(3),
            content: model().into(),
            notices: vec![],
        };
        let message = encode_opened(&opened);
        let mut header = message.header.clone();
        assert_eq!(header["kind"], "meshes");
        header.as_object_mut().unwrap().remove("kind");
        assert_eq!(
            Message {
                header,
                payload: message.payload
            },
            encode_model(ModelId(3), &model())
        );
    }

    #[test]
    fn notices_round_trip() {
        let opened = Opened {
            model: ModelId(3),
            content: model().into(),
            notices: vec![
                Notice {
                    level: NoticeLevel::Info,
                    message: "made by someone".into(),
                },
                Notice {
                    level: NoticeLevel::Warning,
                    message: "no redistribution".into(),
                },
            ],
        };
        let message = encode_opened(&opened);
        assert_eq!(message.header["notices"][1]["level"], "warning");
        assert_eq!(decode_opened(&message).unwrap(), opened);
    }

    fn unlit(alpha_mode: AlphaMode, texture: Option<TextureRef>) -> Material {
        Material {
            alpha_mode,
            double_sided: false,
            kind: MaterialKind::Unlit(UnlitMaterial {
                base_color: [1.0, 0.5, 0.25, 0.75],
                base_color_texture: texture,
            }),
        }
    }

    fn scene_model() -> SceneModel {
        SceneModel {
            canvas: [640.0, 360.0],
            textures: vec![
                SceneTexture {
                    source: TextureSource::Rgba {
                        width: 2,
                        height: 1,
                        pixels: Arc::new(vec![1, 2, 3, 4, 5, 6, 7, 8]),
                    },
                    color_space: ColorSpace::Srgb,
                    sampler: Sampler::default(),
                },
                SceneTexture {
                    source: TextureSource::Path("C:/normal.png".into()),
                    color_space: ColorSpace::Linear,
                    sampler: Sampler {
                        wrap_s: Wrap::ClampToEdge,
                        wrap_t: Wrap::MirroredRepeat,
                        mag_filter: Filter::Nearest,
                        min_filter: Filter::Nearest,
                        mipmaps: false,
                    },
                },
            ],
            geometries: vec![
                Geometry {
                    positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                    normals: vec![[0.0, 0.0, 1.0]; 3],
                    uvs: vec![[0.0, 1.0], [1.0, 1.0], [0.0, 0.0]],
                    tangents: Some(vec![[1.0, 0.0, 0.0, -1.0]; 3]),
                    joints: Some(vec![[0, 1, 0, 0], [1, 0, 0, 0], [65535, 2, 1, 0]]),
                    weights: Some(vec![
                        [0.5, 0.5, 0.0, 0.0],
                        [1.0, 0.0, 0.0, 0.0],
                        [0.25, 0.25, 0.5, 0.0],
                    ]),
                    targets: vec![
                        MorphTarget {
                            positions: vec![[0.0, 0.1, 0.0]; 3],
                            normals: vec![[0.0, 0.0, -0.1]; 3],
                        },
                        MorphTarget {
                            positions: vec![[0.2, 0.0, 0.0]; 3],
                            normals: vec![[0.1, 0.0, 0.0]; 3],
                        },
                    ],
                    indices: vec![0, 1, 2],
                },
                Geometry {
                    positions: vec![[0.0, 0.0, 1.0], [1.0, 1.0, 1.0]],
                    normals: vec![[0.0, 1.0, 0.0]; 2],
                    uvs: vec![[0.5, 0.5]; 2],
                    tangents: None,
                    joints: None,
                    weights: None,
                    targets: vec![],
                    indices: vec![1, 0, 1],
                },
            ],
            skins: vec![Skin { joints: 3 }],
            shaders: Vec::new(),
            materials: vec![
                unlit(
                    AlphaMode::Mask { cutoff: 0.5 },
                    Some(TextureRef {
                        texture: 0,
                        transform: Some(TextureTransform {
                            offset: [0.1, 0.2],
                            rotation: 0.3,
                            scale: [2.0, 0.5],
                        }),
                    }),
                ),
                unlit(
                    AlphaMode::BLEND,
                    Some(TextureRef {
                        texture: 1,
                        transform: None,
                    }),
                ),
            ],
            draws: vec![
                DrawInfo {
                    geometry: 0,
                    material: 0,
                    skin: Some(0),
                },
                DrawInfo {
                    geometry: 1,
                    material: 1,
                    skin: None,
                },
            ],
        }
    }

    fn scene_frame(time: f64) -> SceneFrame {
        let mut ambient = [[0.0; 3]; 9];
        ambient[0] = [0.25, 0.5, 0.75];
        ambient[8] = [-0.125, 0.0, 0.125];
        let mut world = [0.0; 16];
        for (column, value) in world.iter_mut().enumerate().step_by(5) {
            *value = 1.0 + column as f32;
        }
        SceneFrame {
            canvas: None,
            camera: Camera {
                view: world,
                projection: Projection::Perspective {
                    yfov: 0.8,
                    near: 0.1,
                    far: 100.0,
                },
            },
            lights: Lights {
                directional: vec![DirectionalLight {
                    direction: [0.0, -1.0, -0.5],
                    color: [1.0, 0.9, 0.8],
                    intensity: 1.5,
                }],
                ambient,
            },
            time,
            draws: vec![
                DrawState {
                    index: 0,
                    visible: true,
                    world,
                    morph_weights: vec![0.25, time as f32],
                    center: None,
                    overrides: MaterialOverrides {
                        base_color: Some([1.0, 0.0, 0.0, 1.0]),
                        texture_offset: Some([0.5, 0.0]),
                        ..MaterialOverrides::default()
                    },
                    params: None,
                },
                DrawState {
                    index: 1,
                    visible: false,
                    world,
                    morph_weights: vec![0.75, 0.5, 0.125],
                    center: Some([0.0, 1.5, -2.0]),
                    overrides: MaterialOverrides::default(),
                    params: None,
                },
            ],
            skins: vec![vec![world, [2.0; 16], [-0.5; 16]]],
        }
    }

    fn scene_opened() -> Opened {
        Opened {
            model: ModelId(7),
            content: scene_model().into(),
            notices: vec![Notice {
                level: NoticeLevel::Info,
                message: "a scene".into(),
            }],
        }
    }

    #[test]
    fn scene_model_round_trips() {
        let mut bytes = Vec::new();
        write_message(&mut bytes, &encode_opened(&scene_opened())).unwrap();
        let message = read_message(&mut &bytes[..]).unwrap().unwrap();
        assert_eq!(message.header["kind"], "scene");
        // Every geometry's streams, then every geometry's indices, then the
        // inline pixels.
        let (rest, pixels) = message.payload.split_at(message.payload.len() - 8);
        assert_eq!(pixels, [1, 2, 3, 4, 5, 6, 7, 8]);
        let mut indices = Vec::new();
        push_u32(&mut indices, &[0, 1, 2, 1, 0, 1]);
        assert!(rest.ends_with(&indices));
        assert_eq!(decode_opened(&message).unwrap(), scene_opened());
    }

    #[test]
    fn scene_frame_round_trips() {
        let frame = ContentFrame::Scene(scene_frame(1.25));
        let mut bytes = Vec::new();
        write_message(&mut bytes, &encode_content_frame(&frame)).unwrap();
        let message = read_message(&mut &bytes[..]).unwrap().unwrap();
        assert_eq!(message.kind(), "frame");
        // The skins' matrices, then the draws' weights in draw order.
        let mut weights = Vec::new();
        push_f32(&mut weights, [0.25, 1.25, 0.75, 0.5, 0.125]);
        assert_eq!(message.payload.len(), 3 * 64 + weights.len());
        assert!(message.payload.ends_with(&weights));
        assert_eq!(
            decode_content_frame(&message, ContentKind::Scene).unwrap(),
            frame
        );
    }

    #[test]
    fn scene_frame_canvas_round_trips() {
        for canvas in [Some([1080.0, 1920.0]), None] {
            let frame = ContentFrame::Scene(SceneFrame {
                canvas,
                ..scene_frame(0.5)
            });
            let message = encode_content_frame(&frame);
            assert_eq!(
                message.header.get("canvas").is_some(),
                canvas.is_some(),
                "{canvas:?}"
            );
            assert_eq!(
                decode_content_frame(&message, ContentKind::Scene).unwrap(),
                frame
            );
        }
    }

    fn custom(shader: usize, params: Vec<u8>) -> Material {
        Material {
            alpha_mode: AlphaMode::Opaque,
            double_sided: false,
            kind: MaterialKind::Custom(CustomMaterial {
                shader,
                params,
                textures: vec![Some(0), None],
                outline: Some(OutlinePass::default()),
                requires_tangents: shader == 1,
            }),
        }
    }

    #[test]
    fn custom_scene_model_round_trips() {
        let mut model = scene_model();
        model.shaders = vec![
            SceneShader {
                name: Some("toon".into()),
                wgsl: "struct Params { color: vec4f }".into(),
                textures: vec![
                    ShaderTexture {
                        name: Some("base".into()),
                        empty: EmptyTexel::White,
                    },
                    ShaderTexture {
                        name: None,
                        empty: EmptyTexel::Black,
                    },
                ],
            },
            SceneShader {
                name: None,
                wgsl: "struct Params { a: vec4f, b: vec4f }".into(),
                textures: Vec::new(),
            },
        ];
        model.materials = vec![
            custom(0, (0..16).collect()),
            custom(1, (100..132).collect()),
            unlit(AlphaMode::Opaque, None),
        ];
        let opened = Opened {
            model: ModelId(9),
            content: model.into(),
            notices: vec![],
        };
        let mut bytes = Vec::new();
        write_message(&mut bytes, &encode_opened(&opened)).unwrap();
        let message = read_message(&mut &bytes[..]).unwrap().unwrap();
        assert_eq!(message.header["materialParams"], json!([16, 32, 0]));
        assert_eq!(
            message.header["shaders"][0]["textures"][1]["empty"],
            "black"
        );
        assert!(message.header["materials"][0].get("params").is_none());
        // The inline pixels, then each material's parameters.
        let params: Vec<u8> = (0..16).chain(100..132).collect();
        assert!(message.payload.ends_with(&params));
        let pixels = &message.payload[..message.payload.len() - params.len()];
        assert!(pixels.ends_with(&[1, 2, 3, 4, 5, 6, 7, 8]));
        assert_eq!(decode_opened(&message).unwrap(), opened);
    }

    #[test]
    fn scene_model_without_parameters_sends_no_material_params() {
        let message = encode_opened(&scene_opened());
        assert_eq!(message.header["materialParams"], json!([]));
        assert_eq!(message.header["shaders"], json!([]));
    }

    #[test]
    fn draw_params_round_trip() {
        let mut frame = scene_frame(0.5);
        let params: Vec<u8> = (0..32).collect();
        frame.draws[0].params = Some(params.clone());
        let frame = ContentFrame::Scene(frame);
        let mut bytes = Vec::new();
        write_message(&mut bytes, &encode_content_frame(&frame)).unwrap();
        let message = read_message(&mut &bytes[..]).unwrap().unwrap();
        assert_eq!(message.header["draws"][0]["params"], 32);
        assert!(message.header["draws"][1].get("params").is_none());
        // The skins' matrices, then draw 0's weights and parameters, then
        // draw 1's weights.
        let mut draws = Vec::new();
        push_f32(&mut draws, [0.25, 0.5]);
        draws.extend_from_slice(&params);
        push_f32(&mut draws, [0.75, 0.5, 0.125]);
        assert_eq!(message.payload.len(), 3 * 64 + draws.len());
        assert!(message.payload.ends_with(&draws));
        assert_eq!(
            decode_content_frame(&message, ContentKind::Scene).unwrap(),
            frame
        );
    }

    #[test]
    fn material_params_that_do_not_fit_are_malformed() {
        let refused = |message: &Message, expected: &str| match decode_opened(message) {
            Err(ProtocolError::Malformed(error)) => {
                assert!(error.contains(expected), "{expected} in {error}");
            }
            other => panic!("{other:?}"),
        };

        // Two materials, three entries.
        let mut message = encode_opened(&scene_opened());
        message.header["materialParams"] = json!([0, 0, 4]);
        message.payload.extend_from_slice(&[0; 4]);
        refused(&message, "3 materialParams for 2 materials");

        // Bytes for an unlit material.
        let mut model = scene_model();
        model.materials.truncate(1);
        let mut message = encode_opened(&Opened {
            model: ModelId(1),
            content: model.into(),
            notices: vec![],
        });
        message.header["materialParams"] = json!([4]);
        message.payload.extend_from_slice(&[0; 4]);
        refused(
            &message,
            "material 0 is not custom but has 4 parameter bytes",
        );
    }

    #[test]
    fn parameter_bytes_past_the_payload_are_malformed() {
        // A count far past the payload is refused, not allocated.
        let mut model = scene_model();
        model.materials = vec![custom(0, vec![0; 16])];
        let mut message = encode_opened(&Opened {
            model: ModelId(1),
            content: model.into(),
            notices: vec![],
        });
        message.header["materialParams"] = json!([u32::MAX]);
        assert!(matches!(
            decode_opened(&message),
            Err(ProtocolError::Malformed(_))
        ));

        let mut frame = encode_content_frame(&scene_frame(0.0).into());
        frame.header["draws"][1]["params"] = json!(u32::MAX);
        assert!(matches!(
            decode_content_frame(&frame, ContentKind::Scene),
            Err(ProtocolError::Malformed(_))
        ));
    }

    #[test]
    fn scene_payload_too_short_is_malformed() {
        let mut opened = encode_opened(&scene_opened());
        opened.payload.pop();
        assert!(matches!(
            decode_opened(&opened),
            Err(ProtocolError::Malformed(_))
        ));
        let mut frame = encode_content_frame(&scene_frame(0.0).into());
        frame.payload.pop();
        assert!(matches!(
            decode_content_frame(&frame, ContentKind::Scene),
            Err(ProtocolError::Malformed(_))
        ));
    }

    #[test]
    fn zero_vertex_targets_are_malformed() {
        // Targets of a geometry without vertices take no payload, so their
        // count alone must not decide how many are made.
        for targets in [1, u32::MAX] {
            let message = Message::new(json!({
                "type": "opened",
                "kind": "scene",
                "model": 1,
                "canvas": [1.0, 1.0],
                "textures": [],
                "geometries": [{ "vertices": 0, "indices": 0, "targets": targets }],
                "skins": [],
                "materials": [],
                "draws": [],
            }));
            assert!(
                matches!(decode_opened(&message), Err(ProtocolError::Malformed(_))),
                "{targets} targets"
            );
        }
    }

    struct SceneFixed;

    impl ContentProvider for SceneFixed {
        fn open(
            &mut self,
            _plugin: &str,
            _assets: &BTreeMap<String, PathBuf>,
        ) -> Result<Opened, PluginError> {
            Ok(scene_opened())
        }

        fn frame(
            &mut self,
            model: ModelId,
            time: f64,
            _data: &Value,
        ) -> Result<ContentFrame, PluginError> {
            assert_eq!(model, ModelId(7));
            Ok(scene_frame(time).into())
        }
    }

    #[test]
    fn serves_a_scene_session() {
        let mut requests = Vec::new();
        for header in [
            json!({ "type": "hello", "protocol": 1 }),
            json!({ "type": "open", "assets": { "model": "C:/m.glb" } }),
            json!({ "type": "frame", "model": 7, "time": 0.5 }),
            json!({ "type": "close", "model": 7 }),
        ] {
            write_message(&mut requests, &Message::new(header)).unwrap();
        }
        let mut answers = Vec::new();
        serve_on(
            &mut SceneFixed,
            "scenes",
            "0.1.0",
            &requests[..],
            &mut answers,
        )
        .unwrap();
        let mut reader = &answers[..];
        let mut next = || read_message(&mut reader).unwrap().unwrap();
        assert_eq!(next().kind(), "hello");
        assert_eq!(decode_opened(&next()).unwrap(), scene_opened());
        assert_eq!(
            decode_content_frame(&next(), ContentKind::Scene).unwrap(),
            scene_frame(0.5).into()
        );
        assert_eq!(next().kind(), "closed");
    }

    #[test]
    fn printed_text_is_refused_as_a_header_at_once() {
        // "OK: " read as a length is about 540 MB.
        let error = read_message(&mut &b"OK: x"[..]).unwrap_err();
        assert!(
            matches!(&error, ProtocolError::Malformed(message) if message.contains("header of 540691279 bytes")),
            "{error}"
        );
    }

    #[test]
    fn a_part_longer_than_the_stream_ends_it() {
        let mut bytes = Vec::new();
        write_message(&mut bytes, &Message::new(json!({ "type": "frame" }))).unwrap();
        // The payload claims 1 GiB and brings three bytes.
        let payload = bytes.len() - 4;
        bytes[payload..].copy_from_slice(&MAX_PAYLOAD.to_le_bytes());
        bytes.extend_from_slice(b"abc");
        let error = read_message(&mut &bytes[..]).unwrap_err();
        assert!(
            matches!(&error, ProtocolError::Io(io) if io.kind() == io::ErrorKind::UnexpectedEof),
            "{error}"
        );
        assert!(error.to_string().contains("3 bytes into"), "{error}");
        // More than the largest payload is refused before it is read.
        bytes[payload..payload + 4].copy_from_slice(&(MAX_PAYLOAD + 1).to_le_bytes());
        assert!(matches!(
            read_message(&mut &bytes[..]),
            Err(ProtocolError::Malformed(_))
        ));
    }

    #[test]
    fn a_header_too_large_is_not_sent() {
        let text = "x".repeat(MAX_HEADER as usize);
        let error =
            write_message(&mut Vec::new(), &Message::new(json!({ "type": text }))).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
