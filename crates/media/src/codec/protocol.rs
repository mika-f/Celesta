//! The codec role of the plugin protocol (version 1): the same framing as
//! content plugins (`celesta_plugin::protocol`), with the requests of a
//! [`CodecProvider`]. A codec plugin answers `hello` with the role
//! `codec`. See `docs/handoff/2026-11-05-codec-plugin.md` for the requests.

use crate::codec::{
    CodecProvider, Container, EncoderId, EncoderSettings, MuxRequest, Sequential, VideoRequest,
};
use crate::error::MediaError;
use crate::mix::frame_byte_len;
use crate::types::{AudioBuffer, MediaProbe, VideoFrame};
use celesta_composition::Rational;
use celesta_plugin::protocol::{Message, ProtocolError, serve_requests, serve_requests_on};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What a codec plugin answers `hello` with as its roles.
pub const ROLES: &[&str] = &["codec"];

/// Answers requests from `reader` on `writer` with `provider` until the
/// reader ends. `plugin` and `version` are what the plugin says it is.
pub fn serve_on(
    provider: &mut impl CodecProvider,
    plugin: &str,
    version: &str,
    reader: impl Read,
    writer: impl Write,
) -> Result<(), ProtocolError> {
    serve_requests_on(plugin, version, ROLES, reader, writer, |request| {
        answer(provider, request)
    })
}

/// [`serve_on`] this process's stdin and stdout, with stdout kept for the
/// protocol alone (see `celesta_plugin::protocol::serve_requests`): a codec
/// plugin's main loop.
pub fn serve(
    provider: &mut impl CodecProvider,
    plugin: &str,
    version: &str,
) -> Result<(), ProtocolError> {
    serve_requests(plugin, version, ROLES, |request| answer(provider, request))
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

fn answer(provider: &mut impl CodecProvider, mut request: Message) -> Message {
    let answer = (|| -> Result<Message, MediaError> {
        let request = &mut request;
        Ok(match request.kind() {
            "probe" => {
                let path: PathBuf = field(request, "path")?;
                let probe = provider.probe(&path)?;
                Message::new(json!({ "type": "probed", "probe": probe }))
            }
            "decodeVideo" => {
                let path: PathBuf = field(request, "path")?;
                let time: f64 = field(request, "time")?;
                let sequential: Option<WireSequential> = field(request, "sequential")?;
                let frame = provider.decode_video(&VideoRequest {
                    path: &path,
                    time,
                    sequential: sequential.as_ref().map(|sequential| Sequential {
                        session: &sequential.session,
                        frame_rate: sequential.frame_rate,
                    }),
                })?;
                Message {
                    header: json!({
                        "type": "videoFrame",
                        "width": frame.width,
                        "height": frame.height,
                    }),
                    payload: Arc::try_unwrap(frame.pixels)
                        .unwrap_or_else(|shared| (*shared).clone()),
                }
            }
            "decodeAudio" => {
                let path: PathBuf = field(request, "path")?;
                let audio = provider.decode_audio(
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
        })
    })();
    answer.unwrap_or_else(|error| encode_error(&error))
}

/// An `error` answer: the error's text, and, for the errors Celesta tells
/// apart, a `code` with their fields.
pub fn encode_error(error: &MediaError) -> Message {
    let details = match error {
        MediaError::NotFound(path) => json!({ "code": "notFound", "path": path }),
        MediaError::NoVideoStream(path) => json!({ "code": "noVideoStream", "path": path }),
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
pub(crate) fn probe_request(path: &Path) -> Result<Message, MediaError> {
    Ok(Message::new(
        json!({ "type": "probe", "path": sendable(&path)? }),
    ))
}

pub(crate) fn decode_probe(answer: &Message) -> Result<MediaProbe, MediaError> {
    field(answer, "probe")
}

pub(crate) fn decode_video_request(request: &VideoRequest<'_>) -> Result<Message, MediaError> {
    // JSON has no infinities or NaN: it would send `null`.
    if !request.time.is_finite() {
        return Err(MediaError::InvalidTimestamp(request.time));
    }
    Ok(Message::new(json!({
        "type": "decodeVideo",
        "path": sendable(&request.path)?,
        "time": request.time,
        "sequential": request.sequential.map(|sequential| json!({
            "session": sequential.session,
            "frameRate": sequential.frame_rate,
        })),
    })))
}

pub(crate) fn decode_video_frame(answer: Message) -> Result<VideoFrame, MediaError> {
    let width = field(&answer, "width")?;
    let height = field(&answer, "height")?;
    let expected = frame_byte_len(width, height)?;
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
    path: &Path,
    sample_rate: u32,
    channels: u16,
) -> Result<Message, MediaError> {
    Ok(Message::new(json!({
        "type": "decodeAudio",
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

    impl CodecProvider for Fake {
        fn probe(&mut self, path: &Path) -> Result<MediaProbe, MediaError> {
            self.asked.push(format!("probe {}", path.display()));
            if path == Path::new("/missing") {
                return Err(MediaError::NotFound(path.to_owned()));
            }
            Ok(probe())
        }

        fn decode_video(&mut self, request: &VideoRequest<'_>) -> Result<VideoFrame, MediaError> {
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
                Message::new(json!({ "type": "hello", "protocol": 1 })),
                probe_request(Path::new("/a.mp4")).unwrap(),
                probe_request(Path::new("/missing")).unwrap(),
                decode_video_request(&VideoRequest {
                    path: Path::new("/a.mp4"),
                    time: 0.5,
                    sequential: Some(Sequential {
                        session: "layer",
                        frame_rate: Rational::new(30, 1),
                    }),
                })
                .unwrap(),
                decode_audio_request(Path::new("/a.mp4"), 48_000, 2).unwrap(),
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
        assert_eq!(answers[0].header["roles"], json!(["codec"]));
        assert_eq!(decode_probe(&answers[1]).unwrap(), probe());
        assert!(matches!(
            decode_error(&answers[2]),
            MediaError::NotFound(path) if path == Path::new("/missing")
        ));
        let frame = decode_video_frame(answers[3].clone()).unwrap();
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

    impl CodecProvider for Dying {
        fn probe(&mut self, _: &Path) -> Result<MediaProbe, MediaError> {
            if self.failed {
                return Ok(probe());
            }
            self.failed = true;
            Err(MediaError::CodecUnavailable("exited".into()))
        }
        fn decode_video(&mut self, _: &VideoRequest<'_>) -> Result<VideoFrame, MediaError> {
            unreachable!()
        }
        fn decode_audio(&mut self, _: &Path, _: u32, _: u16) -> Result<AudioBuffer, MediaError> {
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
            Ok(Box::new(Dying { failed }) as Box<dyn CodecProvider>)
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

    impl CodecProvider for Stopped {
        fn probe(&mut self, _: &Path) -> Result<MediaProbe, MediaError> {
            Err(MediaError::CodecStopped("did not answer in time".into()))
        }
        fn decode_video(&mut self, _: &VideoRequest<'_>) -> Result<VideoFrame, MediaError> {
            unreachable!()
        }
        fn decode_audio(&mut self, _: &Path, _: u32, _: u16) -> Result<AudioBuffer, MediaError> {
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
            Ok(Box::new(Stopped) as Box<dyn CodecProvider>)
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
    fn a_backend_whose_switch_is_killed_opens_no_codec() {
        let opened = Arc::new(AtomicUsize::new(0));
        let counter = opened.clone();
        let kill = celesta_plugin_host::PluginKillSwitch::new();
        let mut backend = MediaBackend::with_codec(Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Fake::default()) as Box<dyn CodecProvider>)
        }))
        .with_kill_switch(kill.clone());
        kill.kill();
        assert!(matches!(
            backend.probe("/a.mp4"),
            Err(MediaError::CodecStopped(_))
        ));
        assert_eq!(opened.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_backend_gives_its_codec_its_kill_switch() {
        let kill = celesta_plugin_host::PluginKillSwitch::new();
        let given = Arc::new(Mutex::new(None));
        let seen = given.clone();
        let mut backend = MediaBackend::with_codec(Arc::new(move |kill| {
            *seen.lock().unwrap() = Some(kill.clone());
            Ok(Box::new(Fake::default()) as Box<dyn CodecProvider>)
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
        let infinite = decode_video_request(&VideoRequest {
            path: Path::new("/a.mp4"),
            time: f64::INFINITY,
            sequential: None,
        });
        assert!(matches!(infinite, Err(MediaError::InvalidTimestamp(time)) if time.is_infinite()));
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let legacy = Path::new(std::ffi::OsStr::from_bytes(b"/clip-\xff.mp4"));
            assert!(matches!(probe_request(legacy), Err(MediaError::Codec(_))));
            assert!(matches!(
                decode_audio_request(legacy, 48_000, 2),
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
        assert!(decode_video_frame(frame(vec![0; 8])).is_ok());
        assert!(matches!(
            decode_video_frame(frame(vec![0; 7])),
            Err(MediaError::Codec(_))
        ));
    }

    #[test]
    fn a_backend_asks_only_for_mono_or_stereo() {
        let mut backend = MediaBackend::with_codec(Arc::new(|_| {
            Ok(Box::new(Fake::default()) as Box<dyn CodecProvider>)
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
}
