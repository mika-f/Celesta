//! The media role of the plugin protocol (version 2): the same framing as
//! content plugins (`celesta_plugin::protocol`), with the requests of a
//! [`MediaProvider`]. A media plugin answers `hello` with the role
//! `media`; `probe`, `decodeVideo`, and `decodeAudio` name the decoder
//! component of the plugin that is to serve them. See
//! `docs/handoff/2026-11-05-codec-plugin.md` for the requests.

use crate::codec::{
    Container, EncoderId, EncoderSettings, MediaProvider, MuxRequest, Sequential, VideoRequest,
};
use crate::error::MediaError;
use crate::mix::frame_byte_len;
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
    MediaError::Codec(format!("malformed codec message: {message}"))
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
                let probe = provider.probe(&decoder, &path)?;
                Message::new(json!({ "type": "probed", "probe": probe }))
            }
            "decodeVideo" => {
                let decoder: String = field(request, "decoder")?;
                let path: PathBuf = field(request, "path")?;
                let time: f64 = field(request, "time")?;
                let sequential: Option<WireSequential> = field(request, "sequential")?;
                let frame = provider.decode_video(
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
                let audio = provider.decode_audio(
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
            "checkEncoder" => {
                provider.check_encoder(field(request, "container")?)?;
                Message::new(json!({ "type": "encoderReady" }))
            }
            "openEncoder" => {
                let settings: EncoderSettings = field(request, "settings")?;
                let encoder = provider.open_encoder(&settings)?;
                Message::new(json!({ "type": "encoderOpened", "encoder": encoder }))
            }
            "encodeFrame" => {
                provider.encode_frame(
                    field(request, "encoder")?,
                    std::mem::take(&mut request.payload),
                )?;
                Message::new(json!({ "type": "frameEncoded" }))
            }
            "finishEncoder" => {
                provider.finish_encoder(field(request, "encoder")?)?;
                Message::new(json!({ "type": "encoderFinished" }))
            }
            "abortEncoder" => {
                provider.abort_encoder(field(request, "encoder")?);
                Message::new(json!({ "type": "encoderAborted" }))
            }
            "muxAudio" => {
                let mux: MuxRequest = field(request, "request")?;
                provider.mux_audio(&mux)?;
                Message::new(json!({ "type": "muxed" }))
            }
            other => return Err(MediaError::Codec(format!("unknown request \"{other}\""))),
        }
        .into())
    })();
    answer.unwrap_or_else(|error| encode_error(&error).into())
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
        MediaError::EncoderUnavailable { encoder } => {
            json!({ "code": "encoderUnavailable", "encoder": encoder })
        }
        MediaError::Codec(_)
        | MediaError::NoCodec
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
        Some("encoderUnavailable") => {
            header["encoder"]
                .as_str()
                .map(|encoder| MediaError::EncoderUnavailable {
                    encoder: encoder.to_owned(),
                })
        }
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

pub(crate) fn check_encoder_request(container: Container) -> Message {
    Message::new(json!({ "type": "checkEncoder", "container": container }))
}

pub(crate) fn open_encoder_request(settings: &EncoderSettings) -> Result<Message, MediaError> {
    Ok(Message::new(
        json!({ "type": "openEncoder", "settings": sendable(settings)? }),
    ))
}

pub(crate) fn decode_encoder(answer: &Message) -> Result<EncoderId, MediaError> {
    field(answer, "encoder")
}

pub(crate) fn encode_frame_request(encoder: EncoderId, pixels: Vec<u8>) -> Message {
    Message {
        header: json!({ "type": "encodeFrame", "encoder": encoder }),
        payload: pixels,
    }
}

pub(crate) fn finish_encoder_request(encoder: EncoderId) -> Message {
    Message::new(json!({ "type": "finishEncoder", "encoder": encoder }))
}

pub(crate) fn abort_encoder_request(encoder: EncoderId) -> Message {
    Message::new(json!({ "type": "abortEncoder", "encoder": encoder }))
}

pub(crate) fn mux_audio_request(request: &MuxRequest) -> Result<Message, MediaError> {
    Ok(Message::new(
        json!({ "type": "muxAudio", "request": sendable(request)? }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VideoColor;
    use crate::backend::MediaBackend;
    use crate::codec::{CodecFactory, PixelFormat};
    use crate::decoder::{AudioDecoder, VideoFrameDecoder};
    use crate::types::{AudioStream, VideoStream};
    use celesta_composition::Time;
    use celesta_plugin::protocol::{read_message, write_message};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

        fn check_encoder(&mut self, container: Container) -> Result<(), MediaError> {
            match container {
                Container::Webm => Err(MediaError::EncoderUnavailable {
                    encoder: "libvpx-vp9".into(),
                }),
                _ => Ok(()),
            }
        }

        fn open_encoder(&mut self, settings: &EncoderSettings) -> Result<EncoderId, MediaError> {
            self.asked.push(format!("open {:?}", settings.input));
            Ok(EncoderId(7))
        }

        fn encode_frame(&mut self, encoder: EncoderId, pixels: Vec<u8>) -> Result<(), MediaError> {
            self.asked.push(format!("frame {} {:?}", encoder.0, pixels));
            Ok(())
        }

        fn finish_encoder(&mut self, encoder: EncoderId) -> Result<(), MediaError> {
            self.asked.push(format!("finish {}", encoder.0));
            Ok(())
        }

        fn abort_encoder(&mut self, encoder: EncoderId) {
            self.asked.push(format!("abort {}", encoder.0));
        }

        fn mux_audio(&mut self, request: &MuxRequest) -> Result<(), MediaError> {
            self.asked.push(format!(
                "mux {} {}",
                request.lead_in_frames,
                request.output.display()
            ));
            Ok(())
        }
    }

    /// Serves `requests` with `codec`, returning its answers.
    fn serve_all(codec: &mut Fake, requests: &[Message]) -> Vec<Message> {
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
    fn serves_every_request_of_the_codec_role() {
        let mut codec = Fake::default();
        let settings = EncoderSettings {
            output: "/out/video.mp4".into(),
            width: 2,
            height: 1,
            frame_rate: Rational::new(30, 1),
            input: PixelFormat::Yuv420p10,
            container: Container::Mp4,
            preset: "medium".into(),
            crf: 18,
            bit_depth: 10,
        };
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
                check_encoder_request(Container::Webm),
                open_encoder_request(&settings).unwrap(),
                encode_frame_request(EncoderId(7), vec![9, 9]),
                finish_encoder_request(EncoderId(7)),
                mux_audio_request(&MuxRequest {
                    video: "/out/video.mp4".into(),
                    audio: "/out/mix.pcm".into(),
                    sample_rate: 48_000,
                    channels: 2,
                    output: "/out/film.mp4".into(),
                    container: Container::Mp4,
                    lead_in_frames: 16,
                })
                .unwrap(),
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
        assert!(matches!(
            decode_error(&answers[5]),
            MediaError::EncoderUnavailable { encoder } if encoder == "libvpx-vp9"
        ));
        assert_eq!(decode_encoder(&answers[6]).unwrap(), EncoderId(7));
        assert_eq!(answers[7].kind(), "frameEncoded");
        assert_eq!(answers[8].kind(), "encoderFinished");
        assert_eq!(answers[9].kind(), "muxed");
        assert_eq!(answers[10].kind(), "error");
        assert_eq!(
            codec.asked,
            [
                "probe /a.mp4",
                "probe /missing",
                "video /a.mp4 0.5 Some(\"layer\")",
                "open Yuv420p10",
                "frame 7 [9, 9]",
                "finish 7",
                "mux 16 /out/film.mp4",
            ]
        );
    }

    /// Serves the frame it keeps, as a codec keeps the last frame it decoded
    /// for a repeat request.
    struct Keeper {
        kept: Arc<Vec<u8>>,
    }

    impl MediaProvider for Keeper {
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

        fn check_encoder(&mut self, _: Container) -> Result<(), MediaError> {
            unreachable!()
        }

        fn open_encoder(&mut self, _: &EncoderSettings) -> Result<EncoderId, MediaError> {
            unreachable!()
        }

        fn encode_frame(&mut self, _: EncoderId, _: Vec<u8>) -> Result<(), MediaError> {
            unreachable!()
        }

        fn finish_encoder(&mut self, _: EncoderId) -> Result<(), MediaError> {
            unreachable!()
        }

        fn abort_encoder(&mut self, _: EncoderId) {}

        fn mux_audio(&mut self, _: &MuxRequest) -> Result<(), MediaError> {
            unreachable!()
        }
    }

    #[test]
    fn a_frame_the_codec_keeps_is_sent_whole_and_stays_its_own() {
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
            MediaError::EncoderUnavailable {
                encoder: "aac".into(),
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

    /// Fails its first request as a process that died would.
    struct Dying {
        failed: bool,
    }

    impl MediaProvider for Dying {
        fn probe(&mut self, _: &str, _: &Path) -> Result<MediaProbe, MediaError> {
            if self.failed {
                return Ok(probe());
            }
            self.failed = true;
            Err(MediaError::CodecUnavailable("exited".into()))
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
            Ok(AudioBuffer {
                sample_rate: 44_100,
                channels: 2,
                samples: Vec::new(),
            })
        }
        fn check_encoder(&mut self, _: Container) -> Result<(), MediaError> {
            unreachable!()
        }
        fn open_encoder(&mut self, _: &EncoderSettings) -> Result<EncoderId, MediaError> {
            unreachable!()
        }
        fn encode_frame(&mut self, _: EncoderId, _: Vec<u8>) -> Result<(), MediaError> {
            unreachable!()
        }
        fn finish_encoder(&mut self, _: EncoderId) -> Result<(), MediaError> {
            unreachable!()
        }
        fn abort_encoder(&mut self, _: EncoderId) {}
        fn mux_audio(&mut self, _: &MuxRequest) -> Result<(), MediaError> {
            unreachable!()
        }
    }

    #[test]
    fn a_backend_opens_its_codec_again_once_when_it_stops_answering() {
        let opened = Arc::new(AtomicUsize::new(0));
        let counter = opened.clone();
        // The first codec dies on its first request; the second answers.
        let dead = Mutex::new(false);
        let factory: CodecFactory = Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            let mut dead = dead.lock().unwrap();
            let failed = *dead;
            *dead = true;
            Ok(Box::new(Dying { failed }) as Box<dyn MediaProvider>)
        });
        let mut backend = MediaBackend::with_codec(factory);
        assert_eq!(backend.probe("/a.mp4").unwrap(), &probe());
        assert_eq!(opened.load(Ordering::SeqCst), 2);
        // Probes are cached, and a duration comes from the probe.
        assert_eq!(
            backend.video_duration(Path::new("/a.mp4")).unwrap(),
            Some(1.5)
        );
        assert_eq!(opened.load(Ordering::SeqCst), 2);
        // Audio of another format than asked for is refused.
        assert!(matches!(
            backend.decode_audio(Path::new("/a.mp4"), 48_000, 2),
            Err(MediaError::Codec(_))
        ));
    }

    /// Fails every request as a codec stopped at its deadline would.
    struct Stopped;

    impl MediaProvider for Stopped {
        fn probe(&mut self, _: &str, _: &Path) -> Result<MediaProbe, MediaError> {
            Err(MediaError::CodecStopped("did not answer in time".into()))
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
        fn check_encoder(&mut self, _: Container) -> Result<(), MediaError> {
            unreachable!()
        }
        fn open_encoder(&mut self, _: &EncoderSettings) -> Result<EncoderId, MediaError> {
            unreachable!()
        }
        fn encode_frame(&mut self, _: EncoderId, _: Vec<u8>) -> Result<(), MediaError> {
            unreachable!()
        }
        fn finish_encoder(&mut self, _: EncoderId) -> Result<(), MediaError> {
            unreachable!()
        }
        fn abort_encoder(&mut self, _: EncoderId) {}
        fn mux_audio(&mut self, _: &MuxRequest) -> Result<(), MediaError> {
            unreachable!()
        }
    }

    #[test]
    fn a_stopped_codec_is_not_asked_again_but_replaced_on_the_next_request() {
        let opened = Arc::new(AtomicUsize::new(0));
        let counter = opened.clone();
        let mut backend = MediaBackend::with_codec(Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Stopped) as Box<dyn MediaProvider>)
        }));
        assert!(matches!(
            backend.probe("/a.mp4"),
            Err(MediaError::CodecStopped(_))
        ));
        assert_eq!(opened.load(Ordering::SeqCst), 1, "not sent again");
        assert!(backend.probe("/a.mp4").is_err());
        assert_eq!(opened.load(Ordering::SeqCst), 2, "a new codec next time");
    }

    #[test]
    fn a_codec_in_this_process_opens_after_its_switch_is_killed() {
        // It has no process to stop, so it answers as before: an export's
        // Cancel stops it between frames, as it always did.
        let kill = celesta_plugin_host::PluginKillSwitch::new();
        let mut backend = MediaBackend::with_codec(Arc::new(|_| {
            Ok(Box::new(Fake::default()) as Box<dyn MediaProvider>)
        }))
        .with_kill_switch(kill.clone());
        kill.kill();
        assert!(backend.decode_audio(Path::new("/a.mp4"), 48_000, 1).is_ok());
    }

    #[test]
    fn a_backend_gives_its_codec_its_kill_switch() {
        let kill = celesta_plugin_host::PluginKillSwitch::new();
        let given = Arc::new(Mutex::new(None));
        let seen = given.clone();
        let mut backend = MediaBackend::with_codec(Arc::new(move |kill| {
            *seen.lock().unwrap() = Some(kill.clone());
            Ok(Box::new(Fake::default()) as Box<dyn MediaProvider>)
        }))
        .with_kill_switch(kill.clone());
        let _ = backend.decode_audio(Path::new("/a.mp4"), 48_000, 1);
        let given = given.lock().unwrap().take().expect("the codec was opened");
        assert!(!given.is_killed());
        kill.kill();
        assert!(given.is_killed(), "the codec got the backend's switch");
    }

    #[test]
    fn a_backend_without_a_codec_says_so() {
        let mut backend = MediaBackend::with_factory_for_test(None);
        assert!(matches!(backend.probe("/a.mp4"), Err(MediaError::NoCodec)));
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
    fn a_slot_answer_out_of_step_means_the_codec_is_unavailable() {
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
    fn a_backend_asks_only_for_mono_or_stereo() {
        let mut backend = MediaBackend::with_codec(Arc::new(|_| {
            Ok(Box::new(Fake::default()) as Box<dyn MediaProvider>)
        }));
        assert!(matches!(
            backend.decode_audio(Path::new("/a.mp4"), 48_000, 3),
            Err(MediaError::InvalidAudioFormat { channels: 3, .. })
        ));
        assert_eq!(
            backend
                .decode_audio(Path::new("/a.mp4"), 48_000, 1)
                .unwrap()
                .channels,
            1
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

        fn check_encoder(&mut self, _: Container) -> Result<(), MediaError> {
            unreachable!()
        }

        fn open_encoder(&mut self, _: &EncoderSettings) -> Result<EncoderId, MediaError> {
            unreachable!()
        }

        fn encode_frame(&mut self, _: EncoderId, _: Vec<u8>) -> Result<(), MediaError> {
            unreachable!()
        }

        fn finish_encoder(&mut self, _: EncoderId) -> Result<(), MediaError> {
            unreachable!()
        }

        fn abort_encoder(&mut self, _: EncoderId) {}

        fn mux_audio(&mut self, _: &MuxRequest) -> Result<(), MediaError> {
            unreachable!()
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
}
