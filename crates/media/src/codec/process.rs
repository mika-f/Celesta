//! A codec plugin's process, behind the [`CodecProvider`] interface.

use crate::codec::protocol::{self, ROLES, decode_error};
use crate::codec::{
    CodecFactory, CodecProvider, Container, EncoderId, EncoderSettings, MuxRequest, VideoRequest,
};
use crate::error::MediaError;
use crate::types::{AudioBuffer, MediaProbe, VideoFrame};
use celesta_plugin::protocol::{
    Message, PROTOCOL_VERSION, ProtocolError, read_message, write_message,
};
use serde_json::json;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;

/// A running codec plugin, one request at a time. Dropping it stops the
/// process.
pub struct CodecProcess {
    child: Child,
    input: BufWriter<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl CodecProcess {
    /// Starts `executable` in `directory` (the plugin's own) and checks that
    /// it speaks this protocol as a codec.
    pub fn start(executable: &Path, directory: &Path) -> Result<Self, MediaError> {
        let mut child = Command::new(executable)
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The plugin's own log goes where Celesta's does.
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                MediaError::CodecUnavailable(format!(
                    "could not start {}: {error}",
                    executable.display()
                ))
            })?;
        let input = BufWriter::new(child.stdin.take().expect("piped"));
        let output = BufReader::new(child.stdout.take().expect("piped"));
        let mut process = Self {
            child,
            input,
            output,
        };
        let hello = process.request(&Message::new(
            json!({ "type": "hello", "protocol": PROTOCOL_VERSION }),
        ))?;
        let roles = hello.header["roles"].as_array();
        if !roles.is_some_and(|roles| roles.iter().any(|role| role == ROLES[0])) {
            return Err(MediaError::CodecUnavailable(format!(
                "{} is not a codec plugin (its roles are {})",
                executable.display(),
                hello.header["roles"]
            )));
        }
        Ok(process)
    }

    /// Makes a [`CodecFactory`] that starts `executable` for each backend.
    pub fn factory(executable: PathBuf, directory: PathBuf) -> CodecFactory {
        Arc::new(move || {
            Ok(Box::new(CodecProcess::start(&executable, &directory)?) as Box<dyn CodecProvider>)
        })
    }

    fn request(&mut self, request: &Message) -> Result<Message, MediaError> {
        let unavailable =
            |error: &dyn std::fmt::Display| MediaError::CodecUnavailable(error.to_string());
        write_message(&mut self.input, request).map_err(|error| unavailable(&error))?;
        let answer = match read_message(&mut self.output) {
            Ok(Some(answer)) => answer,
            Ok(None) => return Err(unavailable(&"the codec plugin's process exited")),
            Err(ProtocolError::Io(error)) => return Err(unavailable(&error)),
            Err(error) => return Err(unavailable(&error)),
        };
        if answer.kind() == "error" {
            return Err(decode_error(&answer));
        }
        Ok(answer)
    }
}

impl Drop for CodecProcess {
    fn drop(&mut self) {
        // Closing stdin asks the plugin to exit; it is killed if it has not.
        let _ = self.child.kill();
        let _ = self.child.wait();
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
        let request = protocol::probe_request(&absolute(path));
        named(
            path,
            self.request(&request)
                .and_then(|answer| protocol::decode_probe(&answer)),
        )
    }

    fn decode_video(&mut self, request: &VideoRequest<'_>) -> Result<VideoFrame, MediaError> {
        let path = absolute(request.path);
        let message = protocol::decode_video_request(&VideoRequest {
            path: &path,
            ..*request
        });
        named(
            request.path,
            self.request(&message)
                .and_then(protocol::decode_video_frame),
        )
    }

    fn decode_audio(
        &mut self,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        let request = protocol::decode_audio_request(&absolute(path), sample_rate, channels);
        named(
            path,
            self.request(&request)
                .and_then(|answer| protocol::decode_audio_buffer(&answer)),
        )
    }

    fn check_encoder(&mut self, container: Container) -> Result<(), MediaError> {
        self.request(&protocol::check_encoder_request(container))
            .map(drop)
    }

    fn open_encoder(&mut self, settings: &EncoderSettings) -> Result<EncoderId, MediaError> {
        let settings = EncoderSettings {
            output: absolute(&settings.output),
            ..settings.clone()
        };
        protocol::decode_encoder(&self.request(&protocol::open_encoder_request(&settings))?)
    }

    fn encode_frame(&mut self, encoder: EncoderId, pixels: Vec<u8>) -> Result<(), MediaError> {
        self.request(&protocol::encode_frame_request(encoder, pixels))
            .map(drop)
    }

    fn finish_encoder(&mut self, encoder: EncoderId) -> Result<(), MediaError> {
        self.request(&protocol::finish_encoder_request(encoder))
            .map(drop)
    }

    fn abort_encoder(&mut self, encoder: EncoderId) {
        let _ = self.request(&protocol::abort_encoder_request(encoder));
    }

    fn mux_audio(&mut self, request: &MuxRequest) -> Result<(), MediaError> {
        let request = MuxRequest {
            video: absolute(&request.video),
            audio: absolute(&request.audio),
            output: absolute(&request.output),
            ..request.clone()
        };
        self.request(&protocol::mux_audio_request(&request))
            .map(drop)
    }
}
