//! A codec plugin's process, behind the [`CodecProvider`] interface.

use crate::codec::protocol::{self, ROLES, decode_error};
use crate::codec::{
    CodecFactory, CodecProvider, Container, EncoderId, EncoderSettings, MuxRequest, VideoRequest,
};
use crate::error::MediaError;
use crate::types::{AudioBuffer, MediaProbe, VideoFrame};
use celesta_plugin::protocol::Message;
use celesta_plugin_host::{PluginChild, RequestError};
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
const FINISH_ENCODER_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MUX_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// A running codec plugin, one request at a time (see [`PluginChild`] for
/// its window, deadlines, and stderr). Dropping it stops the process.
pub struct CodecProcess(PluginChild);

impl CodecProcess {
    /// Starts `executable` in `directory` (the plugin's own) and checks that
    /// it speaks this protocol as a codec.
    pub fn start(executable: &Path, directory: &Path) -> Result<Self, MediaError> {
        let mut child =
            PluginChild::start(executable, directory).map_err(MediaError::CodecUnavailable)?;
        let hello = child.hello().map_err(media_error)?;
        let roles = hello.header["roles"].as_array();
        if !roles.is_some_and(|roles| roles.iter().any(|role| role == ROLES[0])) {
            return Err(MediaError::CodecUnavailable(format!(
                "{} is not a codec plugin (its roles are {})",
                executable.display(),
                hello.header["roles"]
            )));
        }
        Ok(Self(child))
    }

    /// Makes a [`CodecFactory`] that starts `executable` for each backend.
    pub fn factory(executable: PathBuf, directory: PathBuf) -> CodecFactory {
        Arc::new(move || {
            Ok(Box::new(CodecProcess::start(&executable, &directory)?) as Box<dyn CodecProvider>)
        })
    }

    fn request(
        &mut self,
        request: Message,
        expected: &str,
        timeout: Duration,
    ) -> Result<Message, MediaError> {
        self.0
            .request(request, expected, timeout)
            .map_err(media_error)
    }
}

/// A codec that could not be talked to is unavailable, so a backend starts
/// another; one that refused says why, in the protocol's own error.
fn media_error(error: RequestError) -> MediaError {
    match error {
        RequestError::Unavailable(message) => MediaError::CodecUnavailable(message),
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
        error => error,
    })
}

impl CodecProvider for CodecProcess {
    fn probe(&mut self, path: &Path) -> Result<MediaProbe, MediaError> {
        let request = protocol::probe_request(&absolute(path))?;
        named(
            path,
            self.request(request, "probed", PROBE_TIMEOUT)
                .and_then(|answer| protocol::decode_probe(&answer)),
        )
    }

    fn decode_video(&mut self, request: &VideoRequest<'_>) -> Result<VideoFrame, MediaError> {
        let path = absolute(request.path);
        let message = protocol::decode_video_request(&VideoRequest {
            path: &path,
            ..*request
        })?;
        named(
            request.path,
            self.request(message, "videoFrame", DECODE_VIDEO_TIMEOUT)
                .and_then(protocol::decode_video_frame),
        )
    }

    fn decode_audio(
        &mut self,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        let request = protocol::decode_audio_request(&absolute(path), sample_rate, channels)?;
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
        let request = protocol::abort_encoder_request(encoder);
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
