//! The media role of the plugin protocol (version 2): the same framing as
//! content plugins (`celesta_plugin::protocol`), with the requests of a
//! [`MediaProvider`]. A media plugin answers `hello` with the role
//! `media`, and `describe` with its components; `probe`, `decodeVideo`, and
//! `decodeAudio` name the decoder component of the plugin that is to serve
//! them, and the encoding and muxing requests go to the provider's
//! [`VideoEncoder`], [`AudioEncoder`], or [`Muxer`]. A media plugin's
//! program runs [`serve`]. See the spec's Protocol 2
//! (`docs/superpowers/specs/2026-11-08-media-components-design.md`) and
//! `docs/handoff/2026-11-08-media-decoders.md` and
//! `docs/handoff/2026-11-08-media-encoders.md` for the requests.

use crate::error::MediaError;
use crate::mix::frame_byte_len;
use crate::provider::{
    AudioEncoder, AudioEncoderOpened, AudioEncoderRequest, Color, ComponentDescription, Encoded,
    MediaProvider, Muxer, MuxerRequest, Packet, PixelFormat, Sequential, SessionId, SideData,
    Track, TrackPacket, VideoEncoder, VideoEncoderOpened, VideoEncoderRequest, VideoRequest,
};
use crate::settings::Settings;
use crate::types::{AudioBuffer, MediaProbe, VideoFrame};
use celesta_composition::Rational;
use celesta_plugin::protocol::{
    Answer, Message, ProtocolError, serve_requests_with_hello, serve_requests_with_hello_on,
};
use celesta_plugin::shared::SharedMemory;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What a media plugin answers `hello` with as its roles.
pub const ROLES: &[&str] = &["media"];

/// Answers requests from `reader` on `writer` with `provider` until the
/// reader ends. `plugin` and `version` are what the plugin says it is.
pub fn serve_on(
    provider: &mut impl MediaProvider,
    plugin: &str,
    version: &str,
    reader: impl Read,
    writer: impl Write,
) -> Result<(), ProtocolError> {
    let frames = RefCell::new(None);
    serve_requests_with_hello_on(
        plugin,
        version,
        ROLES,
        reader,
        writer,
        |hello| accept_frames(hello, &frames),
        |request| answer(provider, request, &mut frames.borrow_mut()),
    )
}

/// [`serve_on`] this process's stdin and stdout, with stdout kept for the
/// protocol alone (see `celesta_plugin::protocol::serve_requests`): a media
/// plugin's main loop.
pub fn serve(
    provider: &mut impl MediaProvider,
    plugin: &str,
    version: &str,
) -> Result<(), ProtocolError> {
    let frames = RefCell::new(None);
    serve_requests_with_hello(
        plugin,
        version,
        ROLES,
        |hello| accept_frames(hello, &frames),
        |request| answer(provider, request, &mut frames.borrow_mut()),
    )
}

/// The decoded frames' slot in memory shared with Celesta, on either side,
/// and the generation of the frame last written to it.
pub(crate) struct SharedFrames {
    pub(crate) memory: SharedMemory,
    pub(crate) generation: u64,
}

/// What Celesta's `hello` offers of shared memory for decoded frames: the
/// handle the plugin maps the segment by, and its size.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SharedMemoryOffer {
    handle: u64,
    bytes: usize,
}

/// The fields of a `hello` request offering `memory`, mapped by `handle`
/// in the plugin's process, for decoded frames.
pub(crate) fn shared_frames_offer(handle: u64, memory: &SharedMemory) -> Map<String, Value> {
    let offer = SharedMemoryOffer {
        handle,
        bytes: memory.len(),
    };
    Map::from_iter([("frames".to_owned(), json!({ "sharedMemory": offer }))])
}

/// Whether a `hello` answer accepted the shared memory its request offered.
pub(crate) fn accepted_shared_frames(hello: &Message) -> bool {
    hello.header["frames"] == "sharedMemory"
}

/// Maps the shared memory `hello` offers into `frames`, and says so in the
/// answer's fields; an offer that cannot be mapped is declined (said on
/// stderr), and the frames go through the pipe.
fn accept_frames(hello: &Message, frames: &RefCell<Option<SharedFrames>>) -> Map<String, Value> {
    let offer = &hello.header["frames"]["sharedMemory"];
    if offer.is_null() {
        return Map::new();
    }
    let opened = SharedMemoryOffer::deserialize(offer)
        .map_err(|error| error.to_string())
        .and_then(|offer| {
            SharedMemory::open(offer.handle, offer.bytes).map_err(|error| error.to_string())
        });
    match opened {
        Ok(memory) => {
            *frames.borrow_mut() = Some(SharedFrames {
                memory,
                generation: 0,
            });
            Map::from_iter([("frames".to_owned(), json!("sharedMemory"))])
        }
        Err(error) => {
            eprintln!(
                "decoded frames go through the pipe: the shared memory offered could not be mapped ({error})"
            );
            Map::new()
        }
    }
}

fn malformed(message: impl std::fmt::Display) -> MediaError {
    MediaError::Codec(format!("malformed media message: {message}"))
}

/// The header's `key`, as `T`.
fn field<T: DeserializeOwned>(message: &Message, key: &str) -> Result<T, MediaError> {
    T::deserialize(&message.header[key]).map_err(|error| malformed(format!("`{key}`: {error}")))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireSequential {
    session: String,
    frame_rate: Rational,
}

fn answer(
    provider: &mut impl MediaProvider,
    mut request: Message,
    frames: &mut Option<SharedFrames>,
) -> Answer {
    let answer = (|| -> Result<Answer, MediaError> {
        let request = &mut request;
        Ok(match request.kind() {
            "probe" => {
                let decoder: String = field(request, "decoder")?;
                let path: PathBuf = field(request, "path")?;
                let probe = provider
                    .decoder()
                    .ok_or_else(|| lacks("decoder"))?
                    .probe(&decoder, &path)?;
                Message::new(json!({ "type": "probed", "probe": probe }))
            }
            "decodeVideo" => {
                let decoder: String = field(request, "decoder")?;
                let path: PathBuf = field(request, "path")?;
                let time: f64 = field(request, "time")?;
                let sequential: Option<WireSequential> = field(request, "sequential")?;
                let frame = provider
                    .decoder()
                    .ok_or_else(|| lacks("decoder"))?
                    .decode_video(
                        &decoder,
                        &VideoRequest {
                            path: &path,
                            time,
                            sequential: sequential.as_ref().map(|sequential| Sequential {
                                session: &sequential.session,
                                frame_rate: sequential.frame_rate,
                            }),
                        },
                    )?;
                // Into the shared slot when there is one and the frame fits:
                // Celesta reads it from there before it sends another
                // request, so nothing else writes the slot meanwhile.
                if let Some(frames) = frames.as_mut()
                    && frame.pixels.len() <= frames.memory.slot_bytes()
                {
                    // Even generations: the odd one between is the write.
                    let generation = frames.generation + 2;
                    frames
                        .memory
                        .write_slot(&frame.pixels, generation)
                        .map_err(|error| MediaError::Codec(error.to_string()))?;
                    frames.generation = generation;
                    return Ok(Message::new(json!({
                        "type": "videoFrame",
                        "width": frame.width,
                        "height": frame.height,
                        "slot": { "generation": generation, "bytes": frame.pixels.len() },
                    }))
                    .into());
                }
                // The codec may keep the frame for a repeat request, so it is
                // written from its share rather than copied.
                return Ok(Answer::Shared {
                    header: json!({
                        "type": "videoFrame",
                        "width": frame.width,
                        "height": frame.height,
                    }),
                    payload: frame.pixels,
                });
            }
            "decodeAudio" => {
                let decoder: String = field(request, "decoder")?;
                let path: PathBuf = field(request, "path")?;
                let audio = provider
                    .decoder()
                    .ok_or_else(|| lacks("decoder"))?
                    .decode_audio(
                        &decoder,
                        &path,
                        field(request, "sampleRate")?,
                        field(request, "channels")?,
                    )?;
                Message {
                    header: json!({
                        "type": "audio",
                        "sampleRate": audio.sample_rate,
                        "channels": audio.channels,
                    }),
                    payload: audio
                        .samples
                        .iter()
                        .flat_map(|sample| sample.to_le_bytes())
                        .collect(),
                }
            }
            "describe" => {
                let components = provider.describe()?;
                Message::new(json!({ "type": "described", "components": sendable(&components)? }))
            }
            "openVideoEncoder" | "encodeVideoFrame" | "finishVideoEncoder" => {
                let encoder = provider
                    .video_encoder()
                    .ok_or_else(|| lacks("video encoder"))?;
                answer_video(encoder, request)?
            }
            "openAudioEncoder" | "encodeAudio" | "finishAudioEncoder" => {
                let encoder = provider
                    .audio_encoder()
                    .ok_or_else(|| lacks("audio encoder"))?;
                answer_audio(encoder, request)?
            }
            "abortEncoder" => {
                // Sessions are unique across kinds, so at most one of them
                // owns it; the other ignores it.
                let session: SessionId = field(request, "session")?;
                if let Some(encoder) = provider.video_encoder() {
                    encoder.abort_video(session);
                }
                if let Some(encoder) = provider.audio_encoder() {
                    encoder.abort_audio(session);
                }
                Message::new(json!({ "type": "encoderAborted" }))
            }
            "openMuxer" | "writePackets" | "finishMuxer" | "abortMuxer" => {
                let muxer = provider.muxer().ok_or_else(|| lacks("muxer"))?;
                answer_muxer(muxer, request)?
            }
            other => return Err(MediaError::Codec(format!("unknown request \"{other}\""))),
        }
        .into())
    })();
    answer.unwrap_or_else(|error| encode_error(&error).into())
}

/// The refusal of a provider that has no component of `kind`.
fn lacks(kind: &str) -> MediaError {
    MediaError::Codec(format!("this plugin has no {kind}"))
}

fn answer_video(
    encoder: &mut dyn VideoEncoder,
    request: &mut Message,
) -> Result<Message, MediaError> {
    Ok(match request.kind() {
        "openVideoEncoder" => {
            let component: String = field(request, "component")?;
            let settings: Settings = field(request, "settings")?;
            let offered: Vec<PixelFormat> = field(request, "offered")?;
            let color: Color = field(request, "color")?;
            let opened = encoder.open_video(&VideoEncoderRequest {
                component: &component,
                settings: &settings,
                width: field(request, "width")?,
                height: field(request, "height")?,
                frame_rate: field(request, "frameRate")?,
                timescale: field(request, "timescale")?,
                offered: &offered,
                color: &color,
            })?;
            opened_answer("videoEncoderOpened", &opened)?
        }
        "encodeVideoFrame" => {
            let encoded = encoder.encode_video(
                field(request, "session")?,
                field(request, "pts")?,
                std::mem::take(&mut request.payload),
            )?;
            encoded_answer("videoFrameAccepted", &encoded)?
        }
        _ => {
            let encoded = encoder.finish_video(field(request, "session")?)?;
            encoded_answer("videoEncoderFinished", &encoded)?
        }
    })
}

fn answer_audio(
    encoder: &mut dyn AudioEncoder,
    request: &mut Message,
) -> Result<Message, MediaError> {
    Ok(match request.kind() {
        "openAudioEncoder" => {
            let component: String = field(request, "component")?;
            let settings: Settings = field(request, "settings")?;
            let opened = encoder.open_audio(&AudioEncoderRequest {
                component: &component,
                settings: &settings,
                sample_rate: field(request, "sampleRate")?,
                channels: field(request, "channels")?,
            })?;
            opened_answer("audioEncoderOpened", &opened)?
        }
        "encodeAudio" => {
            let frames: usize = field(request, "frames")?;
            let samples = f32_samples(&request.payload)?;
            let whole = match frames {
                0 => samples.is_empty(),
                frames => samples.len().is_multiple_of(frames),
            };
            if !whole {
                return Err(malformed(format!(
                    "{} samples that are not {frames} whole frames",
                    samples.len()
                )));
            }
            let encoded = encoder.encode_audio(
                field(request, "session")?,
                field(request, "pts")?,
                &samples,
            )?;
            encoded_answer("audioAccepted", &encoded)?
        }
        _ => {
            let encoded = encoder.finish_audio(field(request, "session")?)?;
            encoded_answer("audioEncoderFinished", &encoded)?
        }
    })
}

fn answer_muxer(muxer: &mut dyn Muxer, request: &mut Message) -> Result<Message, MediaError> {
    Ok(match request.kind() {
        "openMuxer" => {
            let component: String = field(request, "component")?;
            let settings: Settings = field(request, "settings")?;
            let output: PathBuf = field(request, "output")?;
            let tracks: Vec<Track> = field(request, "tracks")?;
            let leading_audio: Option<u64> = field(request, "leadingAudio")?;
            let session = muxer.open_muxer(&MuxerRequest {
                component: &component,
                settings: &settings,
                output: &output,
                tracks: &tracks,
                leading_audio: leading_audio.unwrap_or(0),
            })?;
            Message::new(json!({ "type": "muxerOpened", "session": session }))
        }
        "writePackets" => {
            let entries: Vec<Value> = field(request, "packets")?;
            let packets = unpack_track_packets(&entries, &request.payload)?;
            muxer.write_packets(field(request, "session")?, &packets)?;
            Message::new(json!({ "type": "packetsWritten" }))
        }
        "finishMuxer" => {
            let files = muxer.finish_muxer(field(request, "session")?)?;
            Message::new(json!({ "type": "muxerFinished", "files": sendable(&files)? }))
        }
        _ => {
            let files = muxer.abort_muxer(field(request, "session")?);
            Message::new(json!({ "type": "muxerAborted", "files": sendable(&files)? }))
        }
    })
}

/// An `…Opened` answer: `opened`'s fields beside the `type`.
fn opened_answer(kind: &str, opened: &impl Serialize) -> Result<Message, MediaError> {
    let mut header = sendable(opened)?;
    if let Some(header) = header.as_object_mut() {
        header.insert("type".to_owned(), json!(kind));
    }
    Ok(Message::new(header))
}

/// An encoder's answer: its packets, with their bytes as the payload, and
/// the config when it has one.
fn encoded_answer(kind: &str, encoded: &Encoded) -> Result<Message, MediaError> {
    let (packets, payload) = pack_packets(&encoded.packets);
    let mut header = json!({ "type": kind, "packets": packets });
    if let Some(config) = &encoded.config {
        header["config"] = sendable(config)?;
    }
    Ok(Message { header, payload })
}

/// A payload of `f32` LE samples.
fn f32_samples(payload: &[u8]) -> Result<Vec<f32>, MediaError> {
    let (samples, rest) = payload.as_chunks::<4>();
    if !rest.is_empty() {
        return Err(malformed("audio that is not whole `f32` samples"));
    }
    Ok(samples.iter().copied().map(f32::from_le_bytes).collect())
}

/// An `error` answer: the error's text, and, for the errors Celesta tells
/// apart, a `code` with their fields.
pub fn encode_error(error: &MediaError) -> Message {
    let details = match error {
        MediaError::NotFound(path) => json!({ "code": "notFound", "path": path }),
        MediaError::NoVideoStream(path) => json!({ "code": "noVideoStream", "path": path }),
        MediaError::Unsupported { path, .. } => json!({ "code": "unsupported", "path": path }),
        MediaError::InvalidTimestamp(time) => json!({ "code": "invalidTimestamp", "time": time }),
        MediaError::FrameTooLarge { width, height } => {
            json!({ "code": "frameTooLarge", "width": width, "height": height })
        }
        MediaError::UnexpectedFrameSize {
            width,
            height,
            actual,
            expected_width,
            expected_height,
            expected,
        } => json!({
            "code": "unexpectedFrameSize",
            "width": width,
            "height": height,
            "actual": actual,
            "expectedWidth": expected_width,
            "expectedHeight": expected_height,
            "expected": expected,
        }),
        MediaError::InvalidAudioFormat {
            sample_rate,
            channels,
        } => json!({
            "code": "invalidAudioFormat",
            "sampleRate": sample_rate,
            "channels": channels,
        }),
        MediaError::InvalidAudioSampleCount { samples, channels } => json!({
            "code": "invalidAudioSampleCount",
            "samples": samples,
            "channels": channels,
        }),
        // The spec's `key` and `message`: the message without the key, in
        // place of the error's text.
        MediaError::InvalidSettings { key, message } => {
            json!({ "code": "invalidSettings", "key": key, "message": message })
        }
        MediaError::ComponentUnavailable { component, reason } => {
            json!({ "code": "componentUnavailable", "component": component, "reason": reason })
        }
        MediaError::Codec(_)
        | MediaError::NoDecoder
        | MediaError::CodecUnavailable(_)
        | MediaError::CodecStopped(_) => json!({}),
    };
    let mut header = json!({ "type": "error", "message": error.to_string() });
    if let (Some(header), Value::Object(details)) = (header.as_object_mut(), details) {
        header.extend(details);
    }
    Message::new(header)
}

/// The error an `error` answer carries: the one its `code` names, or its
/// text as [`MediaError::Codec`].
pub fn decode_error(answer: &Message) -> MediaError {
    let header = &answer.header;
    let message = header["message"]
        .as_str()
        .unwrap_or("unknown error")
        .to_owned();
    let get = |key: &str| header.get(key).cloned().unwrap_or(Value::Null);
    let u32_of = |key: &str| u32::deserialize(get(key)).ok();
    let usize_of = |key: &str| usize::deserialize(get(key)).ok();
    let path = || PathBuf::deserialize(get("path")).ok();
    let decoded = match header["code"].as_str() {
        Some("notFound") => path().map(MediaError::NotFound),
        Some("noVideoStream") => path().map(MediaError::NoVideoStream),
        Some("unsupported") => path().map(|path| MediaError::Unsupported {
            path,
            tried: Vec::new(),
        }),
        Some("invalidTimestamp") => header["time"].as_f64().map(MediaError::InvalidTimestamp),
        Some("frameTooLarge") => u32_of("width")
            .zip(u32_of("height"))
            .map(|(width, height)| MediaError::FrameTooLarge { width, height }),
        Some("unexpectedFrameSize") => (|| {
            Some(MediaError::UnexpectedFrameSize {
                width: u32_of("width")?,
                height: u32_of("height")?,
                actual: usize_of("actual")?,
                expected_width: u32_of("expectedWidth")?,
                expected_height: u32_of("expectedHeight")?,
                expected: usize_of("expected")?,
            })
        })(),
        Some("invalidAudioFormat") => u32_of("sampleRate")
            .zip(u16::deserialize(get("channels")).ok())
            .map(|(sample_rate, channels)| MediaError::InvalidAudioFormat {
                sample_rate,
                channels,
            }),
        Some("invalidAudioSampleCount") => usize_of("samples")
            .zip(u16::deserialize(get("channels")).ok())
            .map(|(samples, channels)| MediaError::InvalidAudioSampleCount { samples, channels }),
        Some("invalidSettings") => header["key"]
            .as_str()
            .map(|key| MediaError::InvalidSettings {
                key: key.to_owned(),
                message: message.clone(),
            }),
        Some("componentUnavailable") => header["component"]
            .as_str()
            .zip(header["reason"].as_str())
            .map(|(component, reason)| MediaError::ComponentUnavailable {
                component: component.to_owned(),
                reason: reason.to_owned(),
            }),
        _ => None,
    };
    decoded.unwrap_or(MediaError::Codec(message))
}

/// `value` as JSON for a request, failing as an error rather than a panic
/// for what JSON cannot carry: a path that is not UTF-8, which a file name
/// in a legacy encoding can be.
fn sendable(value: &impl Serialize) -> Result<Value, MediaError> {
    serde_json::to_value(value)
        .map_err(|error| MediaError::Codec(format!("cannot send this to the codec: {error}")))
}

/// Celesta's side: the requests, and what their answers carry.
pub(crate) fn probe_request(decoder: &str, path: &Path) -> Result<Message, MediaError> {
    Ok(Message::new(
        json!({ "type": "probe", "decoder": decoder, "path": sendable(&path)? }),
    ))
}

pub(crate) fn decode_probe(answer: &Message) -> Result<MediaProbe, MediaError> {
    field(answer, "probe")
}

pub(crate) fn decode_video_request(
    decoder: &str,
    request: &VideoRequest<'_>,
) -> Result<Message, MediaError> {
    // JSON has no infinities or NaN: it would send `null`.
    if !request.time.is_finite() {
        return Err(MediaError::InvalidTimestamp(request.time));
    }
    Ok(Message::new(json!({
        "type": "decodeVideo",
        "decoder": decoder,
        "path": sendable(&request.path)?,
        "time": request.time,
        "sequential": request.sequential.map(|sequential| json!({
            "session": sequential.session,
            "frameRate": sequential.frame_rate,
        })),
    })))
}

/// Where a `videoFrame` answer left its pixels in shared memory.
#[derive(Deserialize)]
struct WireSlot {
    generation: u64,
    bytes: usize,
}

/// The frame of a `videoFrame` answer: its payload, or, when it names a
/// slot, the pixels copied out of `frames`. A slot that is not there, or
/// does not hold the frame under the generation named (a newer one than the
/// last), is `CodecUnavailable`: the codec has fallen out of step, and a new
/// one is started.
pub(crate) fn decode_video_frame(
    answer: Message,
    frames: Option<&mut SharedFrames>,
) -> Result<VideoFrame, MediaError> {
    let width = field(&answer, "width")?;
    let height = field(&answer, "height")?;
    let expected = frame_byte_len(width, height)?;
    if !answer.header["slot"].is_null() {
        let slot: WireSlot = field(&answer, "slot")?;
        let out_of_step = |what: &str| {
            MediaError::CodecUnavailable(format!(
                "the codec answered a {width}x{height} frame in shared memory {what}"
            ))
        };
        let Some(frames) = frames else {
            return Err(out_of_step("it was not offered"));
        };
        if slot.bytes != expected {
            return Err(out_of_step(&format!(
                "of {} bytes, not {expected}",
                slot.bytes
            )));
        }
        if slot.generation <= frames.generation {
            return Err(out_of_step("under a generation already read"));
        }
        let pixels = frames
            .memory
            .read_slot(expected, slot.generation)
            .ok_or_else(|| out_of_step("that the slot does not hold"))?;
        frames.generation = slot.generation;
        return Ok(VideoFrame {
            width,
            height,
            pixels: Arc::new(pixels),
        });
    }
    if answer.payload.len() != expected {
        return Err(malformed(format!(
            "a {width}x{height} RGBA frame of {} bytes, not {expected}",
            answer.payload.len()
        )));
    }
    Ok(VideoFrame {
        width,
        height,
        pixels: Arc::new(answer.payload),
    })
}

pub(crate) fn decode_audio_request(
    decoder: &str,
    path: &Path,
    sample_rate: u32,
    channels: u16,
) -> Result<Message, MediaError> {
    Ok(Message::new(json!({
        "type": "decodeAudio",
        "decoder": decoder,
        "path": sendable(&path)?,
        "sampleRate": sample_rate,
        "channels": channels,
    })))
}

pub(crate) fn decode_audio_buffer(answer: &Message) -> Result<AudioBuffer, MediaError> {
    let (samples, rest) = answer.payload.as_chunks::<4>();
    if !rest.is_empty() {
        return Err(malformed("audio that is not whole `f32` samples"));
    }
    Ok(AudioBuffer {
        sample_rate: field(answer, "sampleRate")?,
        channels: field(answer, "channels")?,
        samples: samples.iter().copied().map(f32::from_le_bytes).collect(),
    })
}

pub(crate) fn describe_request() -> Message {
    Message::new(json!({ "type": "describe" }))
}

pub(crate) fn decode_described(answer: &Message) -> Result<Vec<ComponentDescription>, MediaError> {
    field(answer, "components")
}

pub(crate) fn open_video_request(request: &VideoEncoderRequest<'_>) -> Result<Message, MediaError> {
    Ok(Message::new(json!({
        "type": "openVideoEncoder",
        "component": request.component,
        "settings": sendable(request.settings)?,
        "width": request.width,
        "height": request.height,
        "frameRate": request.frame_rate,
        "timescale": request.timescale,
        "offered": request.offered,
        "color": request.color,
    })))
}

pub(crate) fn decode_video_encoder_opened(
    answer: &Message,
) -> Result<VideoEncoderOpened, MediaError> {
    VideoEncoderOpened::deserialize(&answer.header).map_err(malformed)
}

/// An `encodeVideoFrame` with the frame as its payload.
pub(crate) fn encode_video_request(session: SessionId, pts: i64, frame: Vec<u8>) -> Message {
    Message {
        header: json!({ "type": "encodeVideoFrame", "session": session, "pts": pts }),
        payload: frame,
    }
}

pub(crate) fn finish_video_request(session: SessionId) -> Message {
    Message::new(json!({ "type": "finishVideoEncoder", "session": session }))
}

pub(crate) fn open_audio_request(request: &AudioEncoderRequest<'_>) -> Result<Message, MediaError> {
    Ok(Message::new(json!({
        "type": "openAudioEncoder",
        "component": request.component,
        "settings": sendable(request.settings)?,
        "sampleRate": request.sample_rate,
        "channels": request.channels,
    })))
}

pub(crate) fn decode_audio_encoder_opened(
    answer: &Message,
) -> Result<AudioEncoderOpened, MediaError> {
    AudioEncoderOpened::deserialize(&answer.header).map_err(malformed)
}

/// An `encodeAudio` of interleaved `samples` in `channels` channels, which
/// must be whole frames.
pub(crate) fn encode_audio_request(
    session: SessionId,
    pts: i64,
    channels: u16,
    samples: &[f32],
) -> Result<Message, MediaError> {
    if channels == 0 || !samples.len().is_multiple_of(usize::from(channels)) {
        return Err(MediaError::InvalidAudioSampleCount {
            samples: samples.len(),
            channels,
        });
    }
    Ok(Message {
        header: json!({
            "type": "encodeAudio",
            "session": session,
            "pts": pts,
            "frames": samples.len() / usize::from(channels),
        }),
        payload: samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect(),
    })
}

pub(crate) fn finish_audio_request(session: SessionId) -> Message {
    Message::new(json!({ "type": "finishAudioEncoder", "session": session }))
}

/// `abortEncoder` for a video or an audio encoder's session.
pub(crate) fn abort_encoder_request(session: SessionId) -> Message {
    Message::new(json!({ "type": "abortEncoder", "session": session }))
}

/// The packets of an encoder's answer.
pub(crate) fn decode_encoded(answer: &Message) -> Result<Encoded, MediaError> {
    let entries: Vec<Value> = field(answer, "packets")?;
    Ok(Encoded {
        packets: unpack_packets(&entries, &answer.payload)?,
        config: field(answer, "config")?,
    })
}

pub(crate) fn open_muxer_request(request: &MuxerRequest<'_>) -> Result<Message, MediaError> {
    let mut header = json!({
        "type": "openMuxer",
        "component": request.component,
        "settings": sendable(request.settings)?,
        "output": sendable(&request.output)?,
        "tracks": sendable(&request.tracks)?,
    });
    if request.leading_audio > 0 {
        header["leadingAudio"] = json!(request.leading_audio);
    }
    Ok(Message::new(header))
}

pub(crate) fn decode_muxer_opened(answer: &Message) -> Result<SessionId, MediaError> {
    field(answer, "session")
}

/// A `writePackets`: each packet's header with its `track`, and their bytes
/// as the payload.
pub(crate) fn write_packets_request(session: SessionId, packets: &[TrackPacket]) -> Message {
    let (mut entries, payload) = pack(packets.iter().map(|packet| &packet.packet));
    for (entry, packet) in entries.iter_mut().zip(packets) {
        entry["track"] = json!(packet.track);
    }
    Message {
        header: json!({ "type": "writePackets", "session": session, "packets": entries }),
        payload,
    }
}

pub(crate) fn finish_muxer_request(session: SessionId) -> Message {
    Message::new(json!({ "type": "finishMuxer", "session": session }))
}

pub(crate) fn abort_muxer_request(session: SessionId) -> Message {
    Message::new(json!({ "type": "abortMuxer", "session": session }))
}

/// The `files` a muxer's `muxerFinished` or `muxerAborted` lists.
pub(crate) fn decode_files(answer: &Message) -> Result<Vec<PathBuf>, MediaError> {
    field(answer, "files")
}

/// A packet's header: `bytes` and each side data's `bytes` are lengths.
#[derive(Deserialize, Serialize)]
struct WirePacket {
    pts: i64,
    dts: i64,
    duration: i64,
    key: bool,
    bytes: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    additional: Vec<WireSideData>,
}

#[derive(Deserialize, Serialize)]
struct WireSideData {
    id: u32,
    bytes: usize,
}

/// `packets`' headers, and their bytes back to back: each packet's own,
/// then its side data's in order.
pub(crate) fn pack_packets(packets: &[Packet]) -> (Vec<Value>, Vec<u8>) {
    pack(packets)
}

/// [`pack_packets`] of packets wherever they are kept.
fn pack<'a>(packets: impl IntoIterator<Item = &'a Packet>) -> (Vec<Value>, Vec<u8>) {
    let mut payload = Vec::new();
    let entries = packets
        .into_iter()
        .map(|packet| {
            payload.extend_from_slice(&packet.bytes);
            for side in &packet.additional {
                payload.extend_from_slice(&side.bytes);
            }
            json!(WirePacket {
                pts: packet.pts,
                dts: packet.dts,
                duration: packet.duration,
                key: packet.key,
                bytes: packet.bytes.len(),
                additional: packet
                    .additional
                    .iter()
                    .map(|side| WireSideData {
                        id: side.id,
                        bytes: side.bytes.len(),
                    })
                    .collect(),
            })
        })
        .collect();
    (entries, payload)
}

/// The packets [`pack_packets`] made, from their headers and payload. A
/// payload that is not exactly the lengths listed is malformed.
pub(crate) fn unpack_packets(entries: &[Value], payload: &[u8]) -> Result<Vec<Packet>, MediaError> {
    let mut rest = payload;
    let mut take = |length: usize| {
        let Some((bytes, after)) = rest.split_at_checked(length) else {
            return Err(malformed(format!(
                "packets listing more bytes than the {} sent",
                payload.len()
            )));
        };
        rest = after;
        Ok(bytes.to_vec())
    };
    let packets = entries
        .iter()
        .map(|entry| {
            let wire = WirePacket::deserialize(entry)
                .map_err(|error| malformed(format!("`packets`: {error}")))?;
            let bytes = take(wire.bytes)?;
            let additional = wire
                .additional
                .iter()
                .map(|side| {
                    Ok(SideData {
                        id: side.id,
                        bytes: take(side.bytes)?,
                    })
                })
                .collect::<Result<_, MediaError>>()?;
            Ok(Packet {
                pts: wire.pts,
                dts: wire.dts,
                duration: wire.duration,
                key: wire.key,
                bytes,
                additional,
            })
        })
        .collect::<Result<Vec<_>, MediaError>>()?;
    if !rest.is_empty() {
        return Err(malformed(format!(
            "{} bytes past the packets listed",
            rest.len()
        )));
    }
    Ok(packets)
}

/// The packets of a `writePackets`, each with its `track`.
fn unpack_track_packets(entries: &[Value], payload: &[u8]) -> Result<Vec<TrackPacket>, MediaError> {
    let packets = unpack_packets(entries, payload)?;
    entries
        .iter()
        .zip(packets)
        .map(|(entry, packet)| {
            let track = usize::deserialize(&entry["track"])
                .map_err(|error| malformed(format!("`track`: {error}")))?;
            Ok(TrackPacket { track, packet })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VideoColor;
    use crate::provider::{ComponentKind, Decoder, SampleRates, TrackConfig};
    use crate::settings::{SettingField, SettingValue};
    use crate::types::{AudioStream, VideoStream};
    use celesta_composition::Time;
    use celesta_plugin::protocol::{read_message, write_message};

    /// Answers every request with made-up media, and records what it was
    /// asked.
    #[derive(Default)]
    struct Fake {
        asked: Vec<String>,
    }

    fn probe() -> MediaProbe {
        MediaProbe {
            duration: Some(Time::new(2, 1)),
            video: Some(VideoStream {
                index: 0,
                codec: Some("h264".into()),
                width: 2,
                height: 1,
                frame_rate: Some(Rational::new(30, 1)),
                duration: Some(Time::new(3, 2)),
                color: VideoColor {
                    primaries: Some("bt709".into()),
                    transfer: None,
                    matrix: None,
                },
            }),
            audio: vec![AudioStream {
                index: 1,
                codec: Some("aac".into()),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration: None,
            }],
        }
    }

    impl MediaProvider for Fake {
        fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
            Ok(Vec::new())
        }

        fn decoder(&mut self) -> Option<&mut dyn Decoder> {
            Some(self)
        }
    }

    impl Decoder for Fake {
        fn probe(&mut self, _: &str, path: &Path) -> Result<MediaProbe, MediaError> {
            self.asked.push(format!("probe {}", path.display()));
            if path == Path::new("/missing") {
                return Err(MediaError::NotFound(path.to_owned()));
            }
            Ok(probe())
        }

        fn decode_video(
            &mut self,
            _: &str,
            request: &VideoRequest<'_>,
        ) -> Result<VideoFrame, MediaError> {
            self.asked.push(format!(
                "video {} {} {:?}",
                request.path.display(),
                request.time,
                request.sequential.map(|sequential| sequential.session)
            ));
            Ok(VideoFrame {
                width: 2,
                height: 1,
                pixels: Arc::new(vec![1, 2, 3, 4, 5, 6, 7, 8]),
            })
        }

        fn decode_audio(
            &mut self,
            _: &str,
            _: &Path,
            sample_rate: u32,
            channels: u16,
        ) -> Result<AudioBuffer, MediaError> {
            Ok(AudioBuffer {
                sample_rate,
                channels,
                samples: vec![0.5, -0.25],
            })
        }
    }

    /// Serves `requests` with `codec`, returning its answers.
    fn serve_all(codec: &mut impl MediaProvider, requests: &[Message]) -> Vec<Message> {
        let mut input = Vec::new();
        for request in requests {
            write_message(&mut input, request).unwrap();
        }
        let mut output = Vec::new();
        serve_on(codec, "fake", "1.0.0", &input[..], &mut output).unwrap();
        let mut reader = &output[..];
        std::iter::from_fn(|| read_message(&mut reader).unwrap()).collect()
    }

    #[test]
    fn serves_every_request_of_the_media_role() {
        let mut codec = Fake::default();
        let answers = serve_all(
            &mut codec,
            &[
                Message::new(json!({ "type": "hello", "protocol": 2 })),
                probe_request("ffmpeg", Path::new("/a.mp4")).unwrap(),
                probe_request("ffmpeg", Path::new("/missing")).unwrap(),
                decode_video_request(
                    "ffmpeg",
                    &VideoRequest {
                        path: Path::new("/a.mp4"),
                        time: 0.5,
                        sequential: Some(Sequential {
                            session: "layer",
                            frame_rate: Rational::new(30, 1),
                        }),
                    },
                )
                .unwrap(),
                decode_audio_request("ffmpeg", Path::new("/a.mp4"), 48_000, 2).unwrap(),
                Message::new(json!({ "type": "draw" })),
            ],
        );
        assert_eq!(answers[0].header["roles"], json!(["media"]));
        assert_eq!(decode_probe(&answers[1]).unwrap(), probe());
        assert!(matches!(
            decode_error(&answers[2]),
            MediaError::NotFound(path) if path == Path::new("/missing")
        ));
        let frame = decode_video_frame(answers[3].clone(), None).unwrap();
        assert_eq!((frame.width, frame.height), (2, 1));
        assert_eq!(*frame.pixels, [1, 2, 3, 4, 5, 6, 7, 8]);
        let audio = decode_audio_buffer(&answers[4]).unwrap();
        assert_eq!((audio.sample_rate, audio.channels), (48_000, 2));
        assert_eq!(audio.samples, [0.5, -0.25]);
        assert_eq!(answers[5].kind(), "error");
        assert_eq!(
            codec.asked,
            [
                "probe /a.mp4",
                "probe /missing",
                "video /a.mp4 0.5 Some(\"layer\")",
            ]
        );
    }

    /// Serves the frame it keeps, as a plugin keeps the last frame it decoded
    /// for a repeat request.
    struct Keeper {
        kept: Arc<Vec<u8>>,
    }

    impl MediaProvider for Keeper {
        fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
            Ok(Vec::new())
        }

        fn decoder(&mut self) -> Option<&mut dyn Decoder> {
            Some(self)
        }
    }

    impl Decoder for Keeper {
        fn probe(&mut self, _: &str, _: &Path) -> Result<MediaProbe, MediaError> {
            Ok(probe())
        }

        fn decode_video(
            &mut self,
            _: &str,
            _: &VideoRequest<'_>,
        ) -> Result<VideoFrame, MediaError> {
            Ok(VideoFrame {
                width: 2,
                height: 1,
                pixels: Arc::clone(&self.kept),
            })
        }

        fn decode_audio(
            &mut self,
            _: &str,
            _: &Path,
            _: u32,
            _: u16,
        ) -> Result<AudioBuffer, MediaError> {
            unreachable!()
        }
    }

    #[test]
    fn a_frame_the_plugin_keeps_is_sent_whole_and_stays_its_own() {
        let kept = Arc::new(vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let mut codec = Keeper {
            kept: Arc::clone(&kept),
        };
        let request = decode_video_request(
            "ffmpeg",
            &VideoRequest {
                path: Path::new("/a.mp4"),
                time: 0.0,
                sequential: None,
            },
        )
        .unwrap();
        let mut input = Vec::new();
        for _ in 0..2 {
            write_message(&mut input, &request).unwrap();
        }
        let mut output = Vec::new();
        serve_on(&mut codec, "keeper", "1.0.0", &input[..], &mut output).unwrap();
        let mut reader = &output[..];
        for _ in 0..2 {
            let frame =
                decode_video_frame(read_message(&mut reader).unwrap().unwrap(), None).unwrap();
            assert_eq!(*frame.pixels, *kept);
        }
        assert!(read_message(&mut reader).unwrap().is_none());
        // The codec's share and this one: no answer holds the frame.
        assert_eq!(Arc::strong_count(&kept), 2);
    }

    #[test]
    fn errors_keep_their_kind_across_the_protocol() {
        let errors = [
            MediaError::NotFound("/a.mp4".into()),
            MediaError::NoVideoStream("/a.wav".into()),
            MediaError::InvalidTimestamp(-1.0),
            MediaError::FrameTooLarge {
                width: 1,
                height: 2,
            },
            MediaError::UnexpectedFrameSize {
                width: 1,
                height: 2,
                actual: 3,
                expected_width: 4,
                expected_height: 5,
                expected: 6,
            },
            MediaError::InvalidAudioFormat {
                sample_rate: 0,
                channels: 2,
            },
            MediaError::InvalidAudioSampleCount {
                samples: 3,
                channels: 2,
            },
            MediaError::Codec("FFmpeg operation failed: oops".into()),
        ];
        for error in errors {
            let decoded = decode_error(&encode_error(&error));
            assert_eq!(
                std::mem::discriminant(&decoded),
                std::mem::discriminant(&error)
            );
            assert_eq!(decoded.to_string(), error.to_string());
        }
    }

    #[test]
    fn what_json_cannot_carry_is_an_error_not_a_panic() {
        let infinite = decode_video_request(
            "ffmpeg",
            &VideoRequest {
                path: Path::new("/a.mp4"),
                time: f64::INFINITY,
                sequential: None,
            },
        );
        assert!(matches!(infinite, Err(MediaError::InvalidTimestamp(time)) if time.is_infinite()));
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let legacy = Path::new(std::ffi::OsStr::from_bytes(b"/clip-\xff.mp4"));
            assert!(matches!(
                probe_request("ffmpeg", legacy),
                Err(MediaError::Codec(_))
            ));
            assert!(matches!(
                decode_audio_request("ffmpeg", legacy, 48_000, 2),
                Err(MediaError::Codec(_))
            ));
        }
    }

    #[test]
    fn a_frame_whose_pixels_do_not_fit_its_size_is_refused() {
        let frame = |payload: Vec<u8>| Message {
            header: json!({ "type": "videoFrame", "width": 2, "height": 1 }),
            payload,
        };
        assert!(decode_video_frame(frame(vec![0; 8]), None).is_ok());
        assert!(matches!(
            decode_video_frame(frame(vec![0; 7]), None),
            Err(MediaError::Codec(_))
        ));
    }

    /// A `hello` request offering `memory` (by a second handle, which the
    /// codec in this process maps as a plugin process would) for frames.
    fn offering_hello(memory: &SharedMemory) -> Message {
        let mut header = shared_frames_offer(memory.duplicate_handle().unwrap(), memory);
        header.insert("type".to_owned(), json!("hello"));
        header.insert("protocol".to_owned(), json!(2));
        Message::new(Value::Object(header))
    }

    fn video_request() -> Message {
        decode_video_request(
            "ffmpeg",
            &VideoRequest {
                path: Path::new("/a.mp4"),
                time: 0.5,
                sequential: None,
            },
        )
        .unwrap()
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn frames_come_through_shared_memory_once_hello_accepts_it() {
        let memory = SharedMemory::create(celesta_plugin::shared::SLOT_HEADER + 64).unwrap();
        let mut codec = Fake::default();
        // Both requests are answered before either answer is read, which
        // Celesta never does: the second frame takes the slot over.
        let answers = serve_all(
            &mut codec,
            &[offering_hello(&memory), video_request(), video_request()],
        );
        assert!(accepted_shared_frames(&answers[0]));
        for (answer, generation) in answers[1..].iter().zip([2, 4]) {
            assert_eq!(answer.header["slot"]["generation"], generation);
            assert!(
                answer.payload.is_empty(),
                "the pixels went through the pipe"
            );
        }
        let mut frames = SharedFrames {
            memory,
            generation: 0,
        };
        assert!(
            matches!(
                decode_video_frame(answers[1].clone(), Some(&mut frames)),
                Err(MediaError::CodecUnavailable(_))
            ),
            "the first frame was read from a slot written over"
        );
        let frame = decode_video_frame(answers[2].clone(), Some(&mut frames)).unwrap();
        assert_eq!(*frame.pixels, [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn a_frame_larger_than_the_slot_goes_through_the_pipe() {
        // The fake's frame is 8 bytes; the slot holds 4.
        let memory = SharedMemory::create(celesta_plugin::shared::SLOT_HEADER + 4).unwrap();
        let answers = serve_all(
            &mut Fake::default(),
            &[offering_hello(&memory), video_request()],
        );
        assert!(accepted_shared_frames(&answers[0]));
        assert!(answers[1].header["slot"].is_null());
        let mut frames = SharedFrames {
            memory,
            generation: 0,
        };
        let frame = decode_video_frame(answers[1].clone(), Some(&mut frames)).unwrap();
        assert_eq!(*frame.pixels, [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn hello_without_an_offer_or_with_one_that_cannot_be_mapped_keeps_the_pipe() {
        let unmappable = Message::new(json!({
            "type": "hello",
            "protocol": 2,
            "frames": { "sharedMemory": { "handle": 1_000_000, "bytes": 4096 } },
        }));
        let answers = serve_all(
            &mut Fake::default(),
            &[
                Message::new(json!({ "type": "hello", "protocol": 2 })),
                unmappable,
                video_request(),
            ],
        );
        assert!(!accepted_shared_frames(&answers[0]));
        assert!(!accepted_shared_frames(&answers[1]));
        assert_eq!(answers[1].header["roles"], json!(["media"]));
        assert_eq!(answers[2].payload, [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn a_slot_answer_out_of_step_means_the_plugin_is_unavailable() {
        let memory = SharedMemory::create(celesta_plugin::shared::SLOT_HEADER + 64).unwrap();
        memory.write_slot(&[7; 8], 4).unwrap();
        let answer = |generation: u64, bytes: usize| {
            Message::new(json!({
                "type": "videoFrame",
                "width": 2,
                "height": 1,
                "slot": { "generation": generation, "bytes": bytes },
            }))
        };
        let mut frames = SharedFrames {
            memory,
            generation: 0,
        };
        let unavailable = |result: Result<VideoFrame, MediaError>| {
            matches!(result, Err(MediaError::CodecUnavailable(_)))
        };
        assert!(
            unavailable(decode_video_frame(answer(4, 8), None)),
            "never offered"
        );
        assert!(
            unavailable(decode_video_frame(answer(4, 4), Some(&mut frames))),
            "not the frame's size"
        );
        assert!(
            unavailable(decode_video_frame(answer(2, 8), Some(&mut frames))),
            "not the slot's generation"
        );
        assert!(
            unavailable(decode_video_frame(answer(3, 8), Some(&mut frames))),
            "an odd generation, a write"
        );
        assert_eq!(
            *decode_video_frame(answer(4, 8), Some(&mut frames))
                .unwrap()
                .pixels,
            [7; 8]
        );
        assert!(
            unavailable(decode_video_frame(answer(4, 8), Some(&mut frames))),
            "read already"
        );
    }

    #[test]
    fn requests_name_their_decoder() {
        let path = Path::new("/a.mp4");
        let video = VideoRequest {
            path,
            time: 0.0,
            sequential: None,
        };
        for request in [
            probe_request("ffmpeg", path).unwrap(),
            decode_video_request("ffmpeg", &video).unwrap(),
            decode_audio_request("ffmpeg", path, 48_000, 2).unwrap(),
        ] {
            assert_eq!(request.header["decoder"], "ffmpeg", "{}", request.kind());
        }
    }

    /// Records the decoder each request names.
    #[derive(Default)]
    struct Recorder {
        decoders: Vec<String>,
    }

    impl MediaProvider for Recorder {
        fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
            Ok(Vec::new())
        }

        fn decoder(&mut self) -> Option<&mut dyn Decoder> {
            Some(self)
        }
    }

    impl Decoder for Recorder {
        fn probe(&mut self, decoder: &str, _: &Path) -> Result<MediaProbe, MediaError> {
            self.decoders.push(decoder.to_owned());
            Ok(probe())
        }

        fn decode_video(
            &mut self,
            decoder: &str,
            _: &VideoRequest<'_>,
        ) -> Result<VideoFrame, MediaError> {
            self.decoders.push(decoder.to_owned());
            Ok(VideoFrame {
                width: 2,
                height: 1,
                pixels: Arc::new(vec![0; 8]),
            })
        }

        fn decode_audio(
            &mut self,
            decoder: &str,
            _: &Path,
            sample_rate: u32,
            channels: u16,
        ) -> Result<AudioBuffer, MediaError> {
            self.decoders.push(decoder.to_owned());
            Ok(AudioBuffer {
                sample_rate,
                channels,
                samples: Vec::new(),
            })
        }
    }

    #[test]
    fn serve_passes_the_decoder_on() {
        let path = Path::new("/a.mp4");
        let video = VideoRequest {
            path,
            time: 0.0,
            sequential: None,
        };
        let mut input = Vec::new();
        for request in [
            Message::new(json!({ "type": "hello", "protocol": 2 })),
            probe_request("x", path).unwrap(),
            decode_video_request("y", &video).unwrap(),
            decode_audio_request("z", path, 48_000, 2).unwrap(),
        ] {
            write_message(&mut input, &request).unwrap();
        }
        let mut recorder = Recorder::default();
        let mut output = Vec::new();
        serve_on(&mut recorder, "fake", "1.0.0", &input[..], &mut output).unwrap();
        assert_eq!(recorder.decoders, ["x", "y", "z"]);
        let mut reader = &output[..];
        let hello = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(hello.header["protocol"], 2);
    }

    #[test]
    fn unsupported_round_trips() {
        let path = PathBuf::from("/a.xyz");
        let error = MediaError::Unsupported {
            path: path.clone(),
            tried: vec!["ffmpeg".to_owned()],
        };
        assert!(matches!(
            decode_error(&encode_error(&error)),
            MediaError::Unsupported { path: decoded, tried } if decoded == path && tried.is_empty()
        ));
    }

    /// Implements the four component traits: records what it is asked and
    /// answers canned values. Its sessions come from one counter, as a
    /// plugin's must.
    struct Components {
        has_muxer: bool,
        next: u64,
        /// The requests it was given, as `{request:?}`.
        asked: Vec<String>,
        video_sessions: Vec<SessionId>,
        audio_sessions: Vec<(SessionId, u16)>,
        /// Each frame: its pts and bytes.
        frames: Vec<(i64, Vec<u8>)>,
        /// Each audio chunk: its pts and sample frames.
        chunks: Vec<(i64, usize)>,
        written: Vec<TrackPacket>,
        output: PathBuf,
    }

    impl Components {
        fn new(next: u64) -> Self {
            Self {
                has_muxer: true,
                next,
                asked: Vec::new(),
                video_sessions: Vec::new(),
                audio_sessions: Vec::new(),
                frames: Vec::new(),
                chunks: Vec::new(),
                written: Vec::new(),
                output: PathBuf::new(),
            }
        }

        fn session(&mut self) -> SessionId {
            let session = SessionId(self.next);
            self.next += 1;
            session
        }
    }

    fn settings_schema() -> Vec<SettingField> {
        serde_json::from_value(json!([
            { "key": "crf", "type": "number", "integer": true, "min": 0, "max": 51,
              "default": 23, "label": "CRF" },
            { "key": "preset", "type": "select", "default": "medium", "label": "Preset",
              "options": [{ "value": "fast", "label": "Fast" },
                          { "value": "medium", "label": "Medium" }] },
        ]))
        .unwrap()
    }

    fn descriptions() -> Vec<ComponentDescription> {
        vec![
            ComponentDescription {
                kind: ComponentKind::VideoEncoder,
                id: "x264".into(),
                available: true,
                reason: None,
                settings: settings_schema(),
                input_formats: vec![PixelFormat::Yuv420p, PixelFormat::Rgba8],
                transports: vec!["pipe".into()],
                sample_rates: None,
                channels: Vec::new(),
            },
            ComponentDescription {
                kind: ComponentKind::Muxer,
                id: "mkv".into(),
                available: false,
                reason: Some("this build has no Matroska muxer".into()),
                settings: Vec::new(),
                input_formats: Vec::new(),
                transports: Vec::new(),
                sample_rates: None,
                channels: Vec::new(),
            },
        ]
    }

    fn encoded_packets() -> Vec<Packet> {
        vec![
            Packet {
                pts: 0,
                dts: -1001,
                duration: 1001,
                key: true,
                bytes: vec![1, 2, 3, 4],
                additional: vec![SideData {
                    id: 1,
                    bytes: vec![9, 8, 7],
                }],
            },
            Packet {
                pts: 3003,
                dts: 0,
                duration: 1001,
                key: false,
                bytes: vec![5, 6],
                additional: Vec::new(),
            },
        ]
    }

    fn avc_config() -> TrackConfig {
        TrackConfig {
            codec: "avc1.640028".into(),
            description: Some(vec![1, 0x64, 0, 0x28, 0xff, 0xe1, 0]),
        }
    }

    impl Decoder for Components {
        fn probe(&mut self, _: &str, _: &Path) -> Result<MediaProbe, MediaError> {
            Ok(probe())
        }

        fn decode_video(
            &mut self,
            _: &str,
            _: &VideoRequest<'_>,
        ) -> Result<VideoFrame, MediaError> {
            unreachable!()
        }

        fn decode_audio(
            &mut self,
            _: &str,
            _: &Path,
            _: u32,
            _: u16,
        ) -> Result<AudioBuffer, MediaError> {
            unreachable!()
        }
    }

    impl VideoEncoder for Components {
        fn open_video(
            &mut self,
            request: &VideoEncoderRequest<'_>,
        ) -> Result<VideoEncoderOpened, MediaError> {
            self.asked.push(format!("{request:?}"));
            let session = self.session();
            self.video_sessions.push(session);
            Ok(VideoEncoderOpened {
                session,
                input: request.offered[0],
                alpha: false,
            })
        }

        fn encode_video(
            &mut self,
            session: SessionId,
            pts: i64,
            frame: Vec<u8>,
        ) -> Result<Encoded, MediaError> {
            assert!(self.video_sessions.contains(&session));
            self.frames.push((pts, frame));
            Ok(Encoded {
                packets: encoded_packets(),
                config: Some(avc_config()),
            })
        }

        fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
            self.video_sessions.retain(|open| *open != session);
            Ok(Encoded {
                packets: encoded_packets()[1..].to_vec(),
                config: None,
            })
        }

        fn abort_video(&mut self, session: SessionId) {
            self.video_sessions.retain(|open| *open != session);
        }
    }

    impl AudioEncoder for Components {
        fn open_audio(
            &mut self,
            request: &AudioEncoderRequest<'_>,
        ) -> Result<AudioEncoderOpened, MediaError> {
            self.asked.push(format!("{request:?}"));
            let session = self.session();
            self.audio_sessions.push((session, request.channels));
            Ok(AudioEncoderOpened {
                session,
                sample_rate: request.sample_rate,
                channels: request.channels,
                delay: 1024,
            })
        }

        fn encode_audio(
            &mut self,
            session: SessionId,
            pts: i64,
            samples: &[f32],
        ) -> Result<Encoded, MediaError> {
            let (_, channels) = self
                .audio_sessions
                .iter()
                .find(|(open, _)| *open == session)
                .copied()
                .unwrap();
            self.chunks
                .push((pts, samples.len() / usize::from(channels)));
            Ok(Encoded::default())
        }

        fn finish_audio(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
            self.audio_sessions.retain(|(open, _)| *open != session);
            Ok(Encoded::default())
        }

        fn abort_audio(&mut self, session: SessionId) {
            self.audio_sessions.retain(|(open, _)| *open != session);
        }
    }

    impl Muxer for Components {
        fn open_muxer(&mut self, request: &MuxerRequest<'_>) -> Result<SessionId, MediaError> {
            self.asked.push(format!("{request:?}"));
            self.output = request.output.to_owned();
            Ok(self.session())
        }

        fn write_packets(
            &mut self,
            _: SessionId,
            packets: &[TrackPacket],
        ) -> Result<(), MediaError> {
            self.written.extend_from_slice(packets);
            Ok(())
        }

        fn finish_muxer(&mut self, _: SessionId) -> Result<Vec<PathBuf>, MediaError> {
            Ok(vec![self.output.clone()])
        }

        fn abort_muxer(&mut self, _: SessionId) -> Vec<PathBuf> {
            vec![self.output.clone(), self.output.with_extension("part")]
        }
    }

    impl MediaProvider for Components {
        fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
            Ok(descriptions())
        }

        fn decoder(&mut self) -> Option<&mut dyn Decoder> {
            Some(self)
        }

        fn video_encoder(&mut self) -> Option<&mut dyn VideoEncoder> {
            Some(self)
        }

        fn audio_encoder(&mut self) -> Option<&mut dyn AudioEncoder> {
            Some(self)
        }

        fn muxer(&mut self) -> Option<&mut dyn Muxer> {
            if self.has_muxer { Some(self) } else { None }
        }
    }

    fn hello() -> Message {
        Message::new(json!({ "type": "hello", "protocol": 2 }))
    }

    #[test]
    fn describe_round_trips() {
        let answers = serve_all(&mut Components::new(1), &[hello(), describe_request()]);
        assert_eq!(answers[1].kind(), "described");
        assert_eq!(answers[1].header["components"][0]["kind"], "videoEncoder");
        assert_eq!(
            answers[1].header["components"][0]["inputFormats"],
            json!(["yuv420p", "rgba8"])
        );
        assert_eq!(decode_described(&answers[1]).unwrap(), descriptions());
    }

    #[test]
    fn sample_rates_are_any_or_a_list() {
        let wire = |rates: SampleRates| serde_json::to_value(rates).unwrap();
        assert_eq!(wire(SampleRates::Any), json!("any"));
        assert_eq!(wire(SampleRates::Only(vec![48_000])), json!([48_000]));
        for rates in [SampleRates::Any, SampleRates::Only(vec![44_100, 48_000])] {
            let decoded: SampleRates = serde_json::from_value(wire(rates.clone())).unwrap();
            assert_eq!(decoded, rates);
        }
        assert!(serde_json::from_value::<SampleRates>(json!("some")).is_err());
    }

    #[test]
    fn pixel_formats_read_and_print_their_wire_names() {
        let names = ["rgba8", "rgba16", "yuv420p", "yuv420p10", "nv12", "p010"];
        let formats = [
            PixelFormat::Rgba8,
            PixelFormat::Rgba16,
            PixelFormat::Yuv420p,
            PixelFormat::Yuv420p10,
            PixelFormat::Nv12,
            PixelFormat::P010,
        ];
        for (name, format) in names.into_iter().zip(formats) {
            assert_eq!(format.to_string(), name);
            assert_eq!(name.parse::<PixelFormat>().unwrap(), format);
            assert_eq!(serde_json::to_value(format).unwrap(), name);
        }
        assert!("rgb24".parse::<PixelFormat>().is_err());
    }

    #[test]
    fn video_encoding_round_trips() {
        let mut fake = Components::new(7);
        let settings: Settings = [("crf".to_owned(), SettingValue::Number(18.0))].into();
        let color = Color::bt709_limited();
        let request = VideoEncoderRequest {
            component: "x264",
            settings: &settings,
            width: 4,
            height: 2,
            frame_rate: Rational::new(30_000, 1001),
            timescale: 30_000,
            offered: &[PixelFormat::Yuv420p, PixelFormat::Rgba8],
            color: &color,
        };
        let open = open_video_request(&request).unwrap();
        assert_eq!(open.header["offered"], json!(["yuv420p", "rgba8"]));
        assert_eq!(open.header["color"]["matrix"], "bt709");
        assert_eq!(open.header["settings"], json!({ "crf": 18 }));
        let frame: Vec<u8> = (0..16).collect();
        let answers = serve_all(
            &mut fake,
            &[
                hello(),
                open,
                encode_video_request(SessionId(7), 3003, frame.clone()),
                finish_video_request(SessionId(7)),
            ],
        );
        assert_eq!(fake.asked, [format!("{request:?}")]);
        assert_eq!(
            decode_video_encoder_opened(&answers[1]).unwrap(),
            VideoEncoderOpened {
                session: SessionId(7),
                input: PixelFormat::Yuv420p,
                alpha: false,
            }
        );
        assert_eq!(fake.frames, [(3003, frame)]);
        assert_eq!(answers[2].kind(), "videoFrameAccepted");
        assert_eq!(
            decode_encoded(&answers[2]).unwrap(),
            Encoded {
                packets: encoded_packets(),
                config: Some(avc_config()),
            }
        );
        assert_eq!(answers[3].kind(), "videoEncoderFinished");
        assert_eq!(
            decode_encoded(&answers[3]).unwrap(),
            Encoded {
                packets: encoded_packets()[1..].to_vec(),
                config: None,
            }
        );
    }

    #[test]
    fn audio_encoding_round_trips() {
        let mut fake = Components::new(1);
        let settings = Settings::new();
        let request = AudioEncoderRequest {
            component: "aac",
            settings: &settings,
            sample_rate: 48_000,
            channels: 2,
        };
        let open = open_audio_request(&request).unwrap();
        let encode = encode_audio_request(SessionId(1), -16_384, 2, &[0.25; 2048]).unwrap();
        assert_eq!(encode.header["frames"], 1024);
        assert_eq!(encode.payload.len(), 2048 * 4);
        let answers = serve_all(
            &mut fake,
            &[hello(), open, encode, finish_audio_request(SessionId(1))],
        );
        assert_eq!(fake.asked, [format!("{request:?}")]);
        assert_eq!(
            decode_audio_encoder_opened(&answers[1]).unwrap(),
            AudioEncoderOpened {
                session: SessionId(1),
                sample_rate: 48_000,
                channels: 2,
                delay: 1024,
            }
        );
        assert_eq!(fake.chunks, [(-16_384, 1024)]);
        assert_eq!(answers[2].kind(), "audioAccepted");
        assert_eq!(decode_encoded(&answers[2]).unwrap(), Encoded::default());
        assert_eq!(answers[3].kind(), "audioEncoderFinished");
        assert!(matches!(
            encode_audio_request(SessionId(1), 0, 2, &[0.0; 3]),
            Err(MediaError::InvalidAudioSampleCount {
                samples: 3,
                channels: 2
            })
        ));
    }

    #[test]
    fn muxing_round_trips() {
        let mut fake = Components::new(1);
        let settings: Settings = [("faststart".to_owned(), SettingValue::Boolean(true))].into();
        let output = Path::new("/out/film.mp4");
        let tracks = [
            Track::Video {
                config: avc_config(),
                timescale: 30_000,
                width: 4,
                height: 2,
                frame_rate: Rational::new(30_000, 1001),
                color: Color::bt709_limited(),
                alpha: false,
            },
            Track::Audio {
                config: TrackConfig {
                    codec: "mp4a.40.2".into(),
                    description: Some(vec![0x12, 0x10]),
                },
                timescale: 48_000,
                sample_rate: 48_000,
                channels: 2,
                delay: 1024,
            },
        ];
        let request = MuxerRequest {
            component: "mp4",
            settings: &settings,
            output,
            tracks: &tracks,
            leading_audio: 16_384,
        };
        let open = open_muxer_request(&request).unwrap();
        assert_eq!(open.header["tracks"][0]["kind"], "video");
        assert_eq!(
            open.header["tracks"][0]["frameRate"],
            json!(Rational::new(30_000, 1001))
        );
        assert_eq!(open.header["tracks"][1]["kind"], "audio");
        assert_eq!(open.header["tracks"][1]["delay"], 1024);
        assert_eq!(open.header["tracks"][1]["config"]["description"], "EhA=");
        assert_eq!(open.header["leadingAudio"], 16_384);
        let packets = [
            TrackPacket {
                track: 1,
                packet: Packet {
                    pts: -1024,
                    dts: -1024,
                    duration: 1024,
                    key: true,
                    bytes: vec![1, 2, 3],
                    additional: Vec::new(),
                },
            },
            TrackPacket {
                track: 0,
                packet: encoded_packets()[0].clone(),
            },
        ];
        let write = write_packets_request(SessionId(1), &packets);
        assert_eq!(write.header["packets"][0]["track"], 1);
        let answers = serve_all(
            &mut fake,
            &[
                hello(),
                open,
                write,
                finish_muxer_request(SessionId(1)),
                abort_muxer_request(SessionId(1)),
            ],
        );
        assert_eq!(fake.asked, [format!("{request:?}")]);
        assert_eq!(decode_muxer_opened(&answers[1]).unwrap(), SessionId(1));
        assert_eq!(answers[2].kind(), "packetsWritten");
        assert_eq!(fake.written, packets);
        assert_eq!(answers[3].kind(), "muxerFinished");
        assert_eq!(decode_files(&answers[3]).unwrap(), [output]);
        assert_eq!(answers[4].kind(), "muxerAborted");
        assert_eq!(
            decode_files(&answers[4]).unwrap(),
            [output.to_owned(), output.with_extension("part")]
        );
    }

    #[test]
    fn a_full_write_packets_batch_fits_the_header_limit() {
        // The exporter's `WRITE_BATCH_PACKETS` (16 × 1024) packets of the
        // widest header there is: every time at its longest, empty bytes.
        let packet = TrackPacket {
            track: 1,
            packet: Packet {
                pts: i64::MIN,
                dts: i64::MIN,
                duration: i64::MIN,
                key: false,
                bytes: Vec::new(),
                additional: Vec::new(),
            },
        };
        let write = write_packets_request(SessionId(u64::MAX), &vec![packet; 16 * 1024]);
        let header = serde_json::to_vec(&write.header).unwrap();
        assert!(header.len() < 4 << 20, "{} header bytes", header.len());
        write_message(&mut Vec::new(), &write).unwrap();
    }

    #[test]
    fn a_provider_without_the_kind_refuses() {
        let mut fake = Components::new(1);
        fake.has_muxer = false;
        let request = MuxerRequest {
            component: "mp4",
            settings: &Settings::new(),
            output: Path::new("/out/film.mp4"),
            tracks: &[],
            leading_audio: 0,
        };
        let answers = serve_all(&mut fake, &[hello(), open_muxer_request(&request).unwrap()]);
        assert_eq!(answers[1].kind(), "error");
        assert_eq!(answers[1].header["message"], "this plugin has no muxer");
        assert!(fake.asked.is_empty());
    }

    #[test]
    fn packets_pack_back_to_back() {
        let packets = vec![
            Packet {
                pts: 0,
                dts: 0,
                duration: 1,
                key: true,
                bytes: vec![1, 2, 3],
                additional: vec![SideData {
                    id: 1,
                    bytes: vec![4, 5, 6, 7],
                }],
            },
            Packet {
                pts: 1,
                dts: 1,
                duration: 1,
                key: false,
                bytes: vec![8, 9],
                additional: Vec::new(),
            },
        ];
        let (header, payload) = pack_packets(&packets);
        assert_eq!(payload, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(
            header[0],
            json!({ "pts": 0, "dts": 0, "duration": 1, "key": true, "bytes": 3,
                    "additional": [{ "id": 1, "bytes": 4 }] })
        );
        assert!(header[1].get("additional").is_none());
        assert_eq!(unpack_packets(&header, &payload).unwrap(), packets);
        assert!(matches!(
            unpack_packets(&header, &payload[..8]),
            Err(MediaError::Codec(message)) if message.starts_with("malformed")
        ));
    }

    #[test]
    fn new_error_codes_round_trip() {
        let invalid = MediaError::InvalidSettings {
            key: "crf".into(),
            message: "must be at most 51".into(),
        };
        assert_eq!(invalid.to_string(), "crf: must be at most 51");
        assert!(matches!(
            decode_error(&encode_error(&invalid)),
            MediaError::InvalidSettings { key, message }
                if key == "crf" && message == "must be at most 51"
        ));
        let unavailable = MediaError::ComponentUnavailable {
            component: "x264".into(),
            reason: "this FFmpeg build has no libx264".into(),
        };
        assert_eq!(
            unavailable.to_string(),
            "x264 is unavailable: this FFmpeg build has no libx264"
        );
        assert!(matches!(
            decode_error(&encode_error(&unavailable)),
            MediaError::ComponentUnavailable { component, reason }
                if component == "x264" && reason == "this FFmpeg build has no libx264"
        ));
    }

    #[test]
    fn an_abort_reaches_whichever_kind_owns_the_session() {
        let mut fake = Components::new(1);
        let settings = Settings::new();
        let color = Color::bt709_limited();
        let video = VideoEncoderRequest {
            component: "x264",
            settings: &settings,
            width: 4,
            height: 2,
            frame_rate: Rational::new(30, 1),
            timescale: 30,
            offered: &[PixelFormat::Yuv420p],
            color: &color,
        };
        let audio = AudioEncoderRequest {
            component: "aac",
            settings: &settings,
            sample_rate: 48_000,
            channels: 2,
        };
        let answers = serve_all(
            &mut fake,
            &[
                hello(),
                open_video_request(&video).unwrap(),
                open_audio_request(&audio).unwrap(),
                abort_encoder_request(SessionId(2)),
            ],
        );
        assert_eq!(
            decode_video_encoder_opened(&answers[1]).unwrap().session,
            SessionId(1)
        );
        assert_eq!(
            decode_audio_encoder_opened(&answers[2]).unwrap().session,
            SessionId(2)
        );
        assert_eq!(answers[3].kind(), "encoderAborted");
        assert!(fake.audio_sessions.is_empty());
        assert_eq!(fake.video_sessions, [SessionId(1)]);
    }

    #[test]
    fn renamed_errors_say_media() {
        assert_eq!(
            MediaError::CodecUnavailable("x".into()).to_string(),
            "the media plugin is unavailable: x"
        );
        assert_eq!(
            MediaError::CodecStopped("x".into()).to_string(),
            "the media plugin was stopped: x"
        );
        let entries = [json!({ "pts": 0, "dts": 0, "duration": 1, "key": true, "size": 3 })];
        let Err(MediaError::Codec(message)) = unpack_packets(&entries, &[]) else {
            panic!("a packet without its payload was accepted");
        };
        assert!(
            message.starts_with("malformed media message: "),
            "{message}"
        );
    }
}
