//! A media plugin's process, behind the [`MediaProvider`] interface.

use crate::error::MediaError;
use crate::protocol::{self, ROLES, SharedFrames, decode_error};
use crate::provider::{
    AudioEncoder, AudioEncoderOpened, AudioEncoderRequest, ComponentDescription, Container,
    Encoded, EncoderId, EncoderSettings, MediaProvider, MuxRequest, Muxer, MuxerRequest,
    ProviderFactory, SessionId, TrackPacket, VideoEncoder, VideoEncoderOpened, VideoEncoderRequest,
    VideoRequest,
};
use crate::types::{AudioBuffer, MediaProbe, VideoFrame};
use celesta_plugin::MEDIA_PROTOCOL_VERSION;
use celesta_plugin::protocol::Message;
use celesta_plugin::shared::SharedMemory;
use celesta_plugin_host::{PluginChild, PluginKillSwitch, RequestError};
use serde_json::Map;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// How long each request may take before the plugin is stopped as hung.
/// Generous, as a long file's audio, an encoder's flush, or muxing a long
/// export's audio takes a while, but bounded, so a hang ends.
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);
const DECODE_VIDEO_TIMEOUT: Duration = Duration::from_secs(60);
const DECODE_AUDIO_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const ENCODER_TIMEOUT: Duration = Duration::from_secs(60);
/// Also protocol 2's `finishVideoEncoder` and `finishAudioEncoder`.
const FINISH_ENCODER_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MUX_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Protocol 2's deadlines for describing, encoding, and muxing (the spec's
/// table). A missed one stops the process; an abort's is short, as the
/// process is killed after it anyway.
const DESCRIBE_TIMEOUT: Duration = Duration::from_secs(30);
const OPEN_TIMEOUT: Duration = Duration::from_secs(60);
const ENCODE_TIMEOUT: Duration = Duration::from_secs(60);
const FINISH_MUXER_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const ABORT_TIMEOUT: Duration = Duration::from_secs(5);

/// The bytes of the segment a codec process is offered for decoded frames:
/// a slot's header and 40 MiB, which holds an RGBA frame up to 4096x2560
/// (DCI 4K and UHD included); a larger frame goes through the pipe. Pages
/// are reserved, not used, until a frame is written.
pub const SHARED_FRAME_BYTES: usize = celesta_plugin::shared::SLOT_HEADER + (40 << 20);

/// The environment variable that, set to `0`, keeps decoded frames in the
/// pipe: no codec process is offered shared memory.
pub const SHARED_MEMORY_VARIABLE: &str = "CELESTA_CODEC_SHARED_MEMORY";

/// How a codec process's decoded frames reach Celesta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameTransport {
    /// Through the pipe, in each answer's payload.
    Pipe,
    /// Offered shared memory, which the plugin may decline (an older plugin
    /// does), and through the pipe whatever does not fit it.
    SharedMemory,
}

impl FrameTransport {
    /// Shared memory, unless [`SHARED_MEMORY_VARIABLE`] is `0`.
    pub fn from_environment() -> Self {
        match std::env::var_os(SHARED_MEMORY_VARIABLE) {
            Some(value) if value == "0" => Self::Pipe,
            _ => Self::SharedMemory,
        }
    }
}

/// A running media plugin, one request at a time (see [`PluginChild`] for
/// its window, deadlines, and stderr). Dropping it stops the process before
/// it unmaps the memory shared with it, so the process never writes into
/// memory being unmapped.
pub struct MediaProcess {
    child: PluginChild,
    /// The slot decoded frames arrive in, when the plugin accepted it.
    frames: Option<SharedFrames>,
    /// The channels of each open audio encoder session, which its
    /// `encodeAudio` counts frames by.
    audio_channels: HashMap<SessionId, u16>,
}

impl MediaProcess {
    /// Starts `executable` in `directory` (the plugin's own) and checks that
    /// it speaks this protocol as a media plugin. The process is registered with
    /// `kill` before it is asked anything, and none starts once `kill` was
    /// killed. Its decoded frames come through shared memory unless
    /// [`SHARED_MEMORY_VARIABLE`] says otherwise (see [`Self::start_with`]).
    pub fn start(
        executable: &Path,
        directory: &Path,
        kill: &PluginKillSwitch,
    ) -> Result<Self, MediaError> {
        Self::start_with(
            executable,
            directory,
            kill,
            FrameTransport::from_environment(),
        )
    }

    /// [`Self::start`], offering the process shared memory for its decoded
    /// frames when `transport` asks for it. A segment that cannot be made or
    /// passed on, or a plugin that declines it, leaves the frames in the
    /// pipe.
    pub fn start_with(
        executable: &Path,
        directory: &Path,
        kill: &PluginKillSwitch,
        transport: FrameTransport,
    ) -> Result<Self, MediaError> {
        if kill.is_killed() {
            return Err(MediaError::CodecStopped(
                "it was not started, as its plugins were stopped".to_owned(),
            ));
        }
        let memory = match transport {
            FrameTransport::SharedMemory => SharedMemory::create(SHARED_FRAME_BYTES).ok(),
            FrameTransport::Pipe => None,
        };
        let (mut child, handle) =
            PluginChild::start_sharing(executable, directory, memory.as_ref())
                .map_err(MediaError::CodecUnavailable)?;
        kill.register(child.kill_handle());
        let offer = match (&memory, handle) {
            (Some(memory), Some(handle)) => protocol::shared_frames_offer(handle, memory),
            _ => Map::new(),
        };
        let hello = child
            .hello_as(MEDIA_PROTOCOL_VERSION, offer)
            .map_err(media_error)?;
        let roles = hello.header["roles"].as_array();
        if !roles.is_some_and(|roles| roles.iter().any(|role| role == ROLES[0])) {
            return Err(MediaError::CodecUnavailable(format!(
                "{} is not a media plugin (its roles are {})",
                executable.display(),
                hello.header["roles"]
            )));
        }
        let frames = memory
            .filter(|_| handle.is_some() && protocol::accepted_shared_frames(&hello))
            .map(|memory| SharedFrames {
                memory,
                generation: 0,
            });
        Ok(Self {
            child,
            frames,
            audio_channels: HashMap::new(),
        })
    }

    /// Makes a [`ProviderFactory`] that starts `executable` for each backend.
    pub fn factory(executable: PathBuf, directory: PathBuf) -> ProviderFactory {
        Arc::new(move |kill| {
            Ok(
                Box::new(MediaProcess::start(&executable, &directory, kill)?)
                    as Box<dyn MediaProvider>,
            )
        })
    }

    /// Whether decoded frames come through shared memory: the plugin
    /// accepted it.
    pub fn shares_frames(&self) -> bool {
        self.frames.is_some()
    }

    fn request(
        &mut self,
        request: Message,
        expected: &str,
        timeout: Duration,
    ) -> Result<Message, MediaError> {
        self.child
            .request(request, expected, timeout)
            .map_err(media_error)
    }
}

/// A codec that could not be talked to is unavailable, so a backend starts
/// another; one that was stopped (its deadline, or its kill switch) is not
/// asked again; one that refused says why, in the protocol's own error.
fn media_error(error: RequestError) -> MediaError {
    match error {
        RequestError::Unavailable(message) => MediaError::CodecUnavailable(message),
        RequestError::Stopped(message) => MediaError::CodecStopped(message),
        RequestError::Refused(answer) => decode_error(&answer),
    }
}

/// `path` made absolute against Celesta's working directory: the plugin's
/// process runs in its own folder, so a relative path would name another
/// file there.
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}

/// `result` with the errors that name the file naming `path` as the caller
/// gave it, rather than as it was sent.
fn named<T>(path: &Path, result: Result<T, MediaError>) -> Result<T, MediaError> {
    result.map_err(|error| match error {
        MediaError::NotFound(_) => MediaError::NotFound(path.to_owned()),
        MediaError::NoVideoStream(_) => MediaError::NoVideoStream(path.to_owned()),
        MediaError::Unsupported { tried, .. } => MediaError::Unsupported {
            path: path.to_owned(),
            tried,
        },
        error => error,
    })
}

impl MediaProvider for MediaProcess {
    fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
        let answer = self.request(protocol::describe_request(), "described", DESCRIBE_TIMEOUT)?;
        protocol::decode_described(&answer)
    }

    fn video_encoder(&mut self) -> Option<&mut dyn VideoEncoder> {
        Some(self)
    }

    fn audio_encoder(&mut self) -> Option<&mut dyn AudioEncoder> {
        Some(self)
    }

    fn muxer(&mut self) -> Option<&mut dyn Muxer> {
        Some(self)
    }

    fn probe(&mut self, decoder: &str, path: &Path) -> Result<MediaProbe, MediaError> {
        let request = protocol::probe_request(decoder, &absolute(path))?;
        named(
            path,
            self.request(request, "probed", PROBE_TIMEOUT)
                .and_then(|answer| protocol::decode_probe(&answer)),
        )
    }

    fn decode_video(
        &mut self,
        decoder: &str,
        request: &VideoRequest<'_>,
    ) -> Result<VideoFrame, MediaError> {
        let path = absolute(request.path);
        let message = protocol::decode_video_request(
            decoder,
            &VideoRequest {
                path: &path,
                ..*request
            },
        )?;
        let answer = self.request(message, "videoFrame", DECODE_VIDEO_TIMEOUT);
        named(
            request.path,
            answer.and_then(|answer| protocol::decode_video_frame(answer, self.frames.as_mut())),
        )
    }

    fn decode_audio(
        &mut self,
        decoder: &str,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        let request =
            protocol::decode_audio_request(decoder, &absolute(path), sample_rate, channels)?;
        named(
            path,
            self.request(request, "audio", DECODE_AUDIO_TIMEOUT)
                .and_then(|answer| protocol::decode_audio_buffer(&answer)),
        )
    }

    fn check_encoder(&mut self, container: Container) -> Result<(), MediaError> {
        let request = protocol::check_encoder_request(container);
        self.request(request, "encoderReady", ENCODER_TIMEOUT)
            .map(drop)
    }

    fn open_encoder(&mut self, settings: &EncoderSettings) -> Result<EncoderId, MediaError> {
        let settings = EncoderSettings {
            output: absolute(&settings.output),
            ..settings.clone()
        };
        let request = protocol::open_encoder_request(&settings)?;
        protocol::decode_encoder(&self.request(request, "encoderOpened", ENCODER_TIMEOUT)?)
    }

    fn encode_frame(&mut self, encoder: EncoderId, pixels: Vec<u8>) -> Result<(), MediaError> {
        let request = protocol::encode_frame_request(encoder, pixels);
        self.request(request, "frameEncoded", ENCODER_TIMEOUT)
            .map(drop)
    }

    fn finish_encoder(&mut self, encoder: EncoderId) -> Result<(), MediaError> {
        let request = protocol::finish_encoder_request(encoder);
        self.request(request, "encoderFinished", FINISH_ENCODER_TIMEOUT)
            .map(drop)
    }

    fn abort_encoder(&mut self, encoder: EncoderId) {
        let request = protocol::legacy_abort_encoder_request(encoder);
        let _ = self.request(request, "encoderAborted", ENCODER_TIMEOUT);
    }

    fn mux_audio(&mut self, request: &MuxRequest) -> Result<(), MediaError> {
        let request = MuxRequest {
            video: absolute(&request.video),
            audio: absolute(&request.audio),
            output: absolute(&request.output),
            ..request.clone()
        };
        self.request(protocol::mux_audio_request(&request)?, "muxed", MUX_TIMEOUT)
            .map(drop)
    }
}

impl VideoEncoder for MediaProcess {
    fn open_video(
        &mut self,
        request: &VideoEncoderRequest<'_>,
    ) -> Result<VideoEncoderOpened, MediaError> {
        let request = protocol::open_video_request(request)?;
        let answer = self.request(request, "videoEncoderOpened", OPEN_TIMEOUT)?;
        protocol::decode_video_encoder_opened(&answer)
    }

    fn encode_video(
        &mut self,
        session: SessionId,
        pts: i64,
        frame: Vec<u8>,
    ) -> Result<Encoded, MediaError> {
        let request = protocol::encode_video_request(session, pts, frame);
        let answer = self.request(request, "videoFrameAccepted", ENCODE_TIMEOUT)?;
        protocol::decode_encoded(&answer)
    }

    fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        let request = protocol::finish_video_request(session);
        let answer = self.request(request, "videoEncoderFinished", FINISH_ENCODER_TIMEOUT)?;
        protocol::decode_encoded(&answer)
    }

    fn abort_video(&mut self, session: SessionId) {
        let request = protocol::abort_encoder_request(session);
        let _ = self.request(request, "encoderAborted", ABORT_TIMEOUT);
    }
}

impl AudioEncoder for MediaProcess {
    fn open_audio(
        &mut self,
        request: &AudioEncoderRequest<'_>,
    ) -> Result<AudioEncoderOpened, MediaError> {
        let request = protocol::open_audio_request(request)?;
        let answer = self.request(request, "audioEncoderOpened", OPEN_TIMEOUT)?;
        let opened = protocol::decode_audio_encoder_opened(&answer)?;
        self.audio_channels.insert(opened.session, opened.channels);
        Ok(opened)
    }

    fn encode_audio(
        &mut self,
        session: SessionId,
        pts: i64,
        samples: &[f32],
    ) -> Result<Encoded, MediaError> {
        let channels = *self.audio_channels.get(&session).ok_or_else(|| {
            MediaError::Codec(format!("no open audio encoder session {}", session.0))
        })?;
        let request = protocol::encode_audio_request(session, pts, channels, samples)?;
        let answer = self.request(request, "audioAccepted", ENCODE_TIMEOUT)?;
        protocol::decode_encoded(&answer)
    }

    fn finish_audio(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        self.audio_channels.remove(&session);
        let request = protocol::finish_audio_request(session);
        let answer = self.request(request, "audioEncoderFinished", FINISH_ENCODER_TIMEOUT)?;
        protocol::decode_encoded(&answer)
    }

    fn abort_audio(&mut self, session: SessionId) {
        self.audio_channels.remove(&session);
        let request = protocol::abort_encoder_request(session);
        let _ = self.request(request, "encoderAborted", ABORT_TIMEOUT);
    }
}

impl Muxer for MediaProcess {
    fn open_muxer(&mut self, request: &MuxerRequest<'_>) -> Result<SessionId, MediaError> {
        let output = absolute(request.output);
        let request = protocol::open_muxer_request(&MuxerRequest {
            output: &output,
            ..*request
        })?;
        let answer = self.request(request, "muxerOpened", OPEN_TIMEOUT)?;
        protocol::decode_muxer_opened(&answer)
    }

    fn write_packets(
        &mut self,
        session: SessionId,
        packets: &[TrackPacket],
    ) -> Result<(), MediaError> {
        let request = protocol::write_packets_request(session, packets);
        self.request(request, "packetsWritten", ENCODE_TIMEOUT)
            .map(drop)
    }

    fn finish_muxer(&mut self, session: SessionId) -> Result<Vec<PathBuf>, MediaError> {
        let request = protocol::finish_muxer_request(session);
        let answer = self.request(request, "muxerFinished", FINISH_MUXER_TIMEOUT)?;
        protocol::decode_files(&answer)
    }

    fn abort_muxer(&mut self, session: SessionId) -> Vec<PathBuf> {
        let request = protocol::abort_muxer_request(session);
        self.request(request, "muxerAborted", ABORT_TIMEOUT)
            .and_then(|answer| protocol::decode_files(&answer))
            .unwrap_or_default()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use celesta_plugin::protocol::write_message;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_media_process_needs_the_media_role() {
        let directory = tempfile::tempdir().unwrap();
        // A plugin that answers `hello` in protocol 2, with the retired role.
        let answer = directory.path().join("answer.bin");
        let mut bytes = Vec::new();
        let hello = json!({ "type": "hello", "protocol": 2, "roles": ["codec"] });
        write_message(&mut bytes, &Message::new(hello)).unwrap();
        std::fs::write(&answer, bytes).unwrap();
        let program = directory.path().join("plugin.sh");
        std::fs::write(
            &program,
            format!("#!/bin/sh\ncat '{}'\nexec sleep 30\n", answer.display()),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let result = MediaProcess::start_with(
            &program,
            directory.path(),
            &PluginKillSwitch::default(),
            FrameTransport::Pipe,
        );
        let Err(MediaError::CodecUnavailable(message)) = result else {
            panic!("a codec plugin was accepted as a media plugin");
        };
        assert!(message.contains("is not a media plugin"), "{message}");
    }
}
