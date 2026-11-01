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
use crate::provider::{ContentProvider, ModelId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub const PROTOCOL_VERSION: u32 = 1;

/// The largest header or payload either side accepts.
const MAX_PART: u32 = 1 << 30;

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
    for part in [&header[..], &message.payload[..]] {
        let length = u32::try_from(part.len())
            .ok()
            .filter(|length| *length <= MAX_PART)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "message part too large"))?;
        writer.write_all(&length.to_le_bytes())?;
        writer.write_all(part)?;
    }
    writer.flush()
}

fn read_part(reader: &mut impl Read) -> Result<Vec<u8>, ProtocolError> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length);
    if length > MAX_PART {
        return Err(malformed(format!("a part of {length} bytes")));
    }
    let mut part = vec![0; length as usize];
    reader.read_exact(&mut part)?;
    Ok(part)
}

/// The next message, or `None` when the stream ends between messages.
pub fn read_message(reader: &mut impl Read) -> Result<Option<Message>, ProtocolError> {
    let header = match read_part(reader) {
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
    let payload = read_part(reader)?;
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

fn push_f32(payload: &mut Vec<u8>, values: impl IntoIterator<Item = f32>) {
    for value in values {
        payload.extend_from_slice(&value.to_le_bytes());
    }
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

    fn pairs(&mut self, count: u32) -> Result<Vec<[f32; 2]>, ProtocolError> {
        let bytes = self.take(count as usize * 8)?;
        Ok(bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|pair| {
                [
                    f32::from_le_bytes([pair[0], pair[1], pair[2], pair[3]]),
                    f32::from_le_bytes([pair[4], pair[5], pair[6], pair[7]]),
                ]
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

/// `{"type":"opened"}`: a model's header, then its meshes' UVs, their
/// indices, and inline textures' pixels.
pub fn encode_model(id: ModelId, model: &MeshModel) -> Message {
    let header = OpenedHeader {
        model: id.0,
        canvas: model.canvas,
        textures: model
            .textures
            .iter()
            .map(|texture| match texture {
                TextureSource::Path(path) => TextureHeader::Path { path: path.clone() },
                TextureSource::Rgba { width, height, .. } => TextureHeader::Rgba {
                    width: *width,
                    height: *height,
                },
            })
            .collect(),
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
        for index in &mesh.indices {
            payload.extend_from_slice(&index.to_le_bytes());
        }
    }
    for texture in &model.textures {
        if let TextureSource::Rgba { pixels, .. } = texture {
            payload.extend_from_slice(pixels);
        }
    }
    let mut header = serde_json::to_value(header).expect("serializable");
    header["type"] = json!("opened");
    Message { header, payload }
}

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
    let mut textures = Vec::new();
    for texture in header.textures {
        textures.push(match texture {
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
        });
    }
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

/// `{"type":"frame"}`: every mesh's state, then their positions in order.
pub fn encode_frame(frame: &MeshFrame) -> Message {
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
    mut reader: impl Read,
    mut writer: impl Write,
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
                    }))
                } else {
                    error_message(format!(
                        "this plugin speaks protocol {PROTOCOL_VERSION}, not {}",
                        request.header["protocol"]
                    ))
                }
            }
            "open" => match BTreeMap::<String, PathBuf>::deserialize(&request.header["assets"]) {
                Ok(assets) => match provider.open(plugin, &assets) {
                    Ok((id, model)) => encode_model(id, &model),
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
                        Ok(frame) => encode_frame(&frame),
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
        };
        write_message(&mut writer, &answer)?;
    }
    Ok(())
}

/// [`serve_on`] this process's stdin and stdout. On Unix the answers go
/// through a duplicate of stdout taken first, and stdout itself is pointed
/// at stderr: whatever else the process prints, native libraries' banners
/// and logs included, lands in Celesta's log instead of corrupting the
/// protocol.
pub fn serve(
    provider: &mut impl ContentProvider,
    plugin: &str,
    version: &str,
) -> Result<(), ProtocolError> {
    let stdin = io::stdin().lock();
    serve_on(
        provider,
        plugin,
        version,
        stdin,
        io::BufWriter::new(protocol_output()?),
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

#[cfg(not(unix))]
fn protocol_output() -> io::Result<io::StdoutLock<'static>> {
    Ok(io::stdout().lock())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::PluginError;

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
        ) -> Result<(ModelId, MeshModel), PluginError> {
            assert_eq!(plugin, "fixed");
            assert_eq!(assets["model"], PathBuf::from("C:/m.json"));
            Ok((ModelId(1), model()))
        }

        fn frame(
            &mut self,
            model: ModelId,
            time: f64,
            data: &Value,
        ) -> Result<MeshFrame, PluginError> {
            if data["fail"] == true {
                return Err(PluginError::Failed {
                    plugin: "fixed".into(),
                    message: "asked to".into(),
                });
            }
            assert_eq!(model, ModelId(1));
            Ok(frame(time))
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
}
