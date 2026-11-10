//! [`MfCodec`], the plugin's [`MediaProvider`], and the plugin as one made
//! in this process ([`registered`], [`install`]).
//!
//! It describes every component, from what this system's Media Foundation
//! lists. Its encoder sessions, of every kind, are numbered from one counter.
//! Until the encoders and the decoder are built, every open and decode
//! answers that the component is not available.

use crate::components::{self, AudioCodec, VideoCodec};
use crate::mf::{enumerate, startup::Started};
use crate::{DECODER_ID, PLUGIN_ID};
use celesta_media::{
    AudioBuffer, AudioEncoder, AudioEncoderOpened, AudioEncoderRequest, ComponentDescription,
    Decoder, Encoded, MediaError, MediaProbe, MediaProvider, MediaRegistry, ProviderFactory,
    RegisteredPlugin, SessionId, VideoEncoder, VideoEncoderOpened, VideoEncoderRequest, VideoFrame,
    VideoRequest,
};
use celesta_plugin_host::{MediaManifest, MediaOrigin};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The plugin's manifest source, which declares its components.
const PLUGIN_JSON: &str = include_str!("../../plugin.json");

/// A [`MediaProvider`] on Media Foundation.
pub struct MfCodec {
    /// COM and Media Foundation as started, or why they did not: `describe`
    /// says it rather than the program failing to start.
    startup: Result<Started, String>,
}

impl MfCodec {
    /// Starts COM and Media Foundation. A failure is kept for `describe`.
    pub fn new() -> Self {
        Self {
            startup: Started::new(),
        }
    }

    /// A [`ProviderFactory`] making an [`MfCodec`] in this process,
    /// whatever it is for.
    pub fn factory() -> ProviderFactory {
        Arc::new(|_kill, _use| Ok(Box::new(Self::new()) as Box<dyn MediaProvider>))
    }
}

impl Default for MfCodec {
    fn default() -> Self {
        Self::new()
    }
}

/// The error for a request to a session of `kind` that is not open.
fn no_session(kind: &str, session: SessionId) -> MediaError {
    MediaError::Codec(format!("no open {kind} {}", session.0))
}

/// The error for a component the plugin has no `kind` of.
fn no_component(kind: &str, component: &str) -> MediaError {
    MediaError::Codec(format!("no {kind} {component:?} in {PLUGIN_ID}"))
}

/// The answer to a request for `component`, which is built later.
fn not_implemented(component: &str) -> MediaError {
    MediaError::ComponentUnavailable {
        component: component.to_owned(),
        reason: "not implemented yet".to_owned(),
    }
}

/// `Ok` for the decoder [`DECODER_ID`], the plugin's one.
fn check_decoder(decoder: &str) -> Result<(), MediaError> {
    if decoder == DECODER_ID {
        Ok(())
    } else {
        Err(no_component("decoder", decoder))
    }
}

/// The components `plugin.json` declares.
fn manifest() -> MediaManifest {
    let plugin: serde_json::Value = serde_json::from_str(PLUGIN_JSON).expect("plugin.json is JSON");
    let (manifest, problems) = MediaManifest::from_json(plugin["media"].clone());
    assert!(problems.is_empty(), "plugin.json is valid: {problems:?}");
    manifest
}

/// The plugin as a media plugin made in this process: bundled, with the
/// components its manifest declares.
pub fn registered() -> RegisteredPlugin {
    RegisteredPlugin {
        id: PLUGIN_ID.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        origin: MediaOrigin::Bundled,
        directory: PathBuf::new(),
        executable: None,
        media: manifest(),
        factory: MfCodec::factory(),
    }
}

/// Makes [`MfCodec`], in this process, the media plugin every
/// `celesta_media::MediaBackend` uses: for tests and tools, which need no
/// plugin installed.
pub fn install() {
    celesta_media::set_registry(Some(Arc::new(MediaRegistry::new(vec![registered()]))));
}

impl MediaProvider for MfCodec {
    fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
        Ok(match &self.startup {
            Ok(_) => components::describe_from(Ok(()), &enumerate::encoders()),
            Err(error) => components::describe_from(
                Err(error.clone()),
                &components::Encoders {
                    h264: false,
                    hevc: false,
                    aac: false,
                },
            ),
        })
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
}

impl Decoder for MfCodec {
    fn probe(&mut self, decoder: &str, _path: &Path) -> Result<MediaProbe, MediaError> {
        check_decoder(decoder)?;
        Err(not_implemented(decoder))
    }

    fn decode_video(
        &mut self,
        decoder: &str,
        _request: &VideoRequest<'_>,
    ) -> Result<VideoFrame, MediaError> {
        check_decoder(decoder)?;
        Err(not_implemented(decoder))
    }

    fn decode_audio(
        &mut self,
        decoder: &str,
        _path: &Path,
        _sample_rate: u32,
        _channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        check_decoder(decoder)?;
        Err(not_implemented(decoder))
    }
}

impl VideoEncoder for MfCodec {
    fn open_video(
        &mut self,
        request: &VideoEncoderRequest<'_>,
    ) -> Result<VideoEncoderOpened, MediaError> {
        VideoCodec::from_id(request.component)
            .ok_or_else(|| no_component("video encoder", request.component))?;
        Err(not_implemented(request.component))
    }

    fn encode_video(
        &mut self,
        session: SessionId,
        _pts: i64,
        _frame: Vec<u8>,
    ) -> Result<Encoded, MediaError> {
        Err(no_session("video encoder", session))
    }

    fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        Err(no_session("video encoder", session))
    }

    /// Ends a video encoder session without completing its frames; a session
    /// of another kind is left alone. None is open yet.
    fn abort_video(&mut self, _session: SessionId) {}
}

impl AudioEncoder for MfCodec {
    fn open_audio(
        &mut self,
        request: &AudioEncoderRequest<'_>,
    ) -> Result<AudioEncoderOpened, MediaError> {
        AudioCodec::from_id(request.component)
            .ok_or_else(|| no_component("audio encoder", request.component))?;
        Err(not_implemented(request.component))
    }

    fn encode_audio(
        &mut self,
        session: SessionId,
        _pts: i64,
        _samples: &[f32],
    ) -> Result<Encoded, MediaError> {
        Err(no_session("audio encoder", session))
    }

    fn finish_audio(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        Err(no_session("audio encoder", session))
    }

    /// Ends an audio encoder session without flushing it; a session of
    /// another kind is left alone. None is open yet.
    fn abort_audio(&mut self, _session: SessionId) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_declares_the_manifest() {
        let plugin = registered();
        assert_eq!(plugin.id, PLUGIN_ID);
        assert_eq!(plugin.origin, MediaOrigin::Bundled);
        assert_ne!(plugin.media, MediaManifest::default());
        assert_eq!(plugin.media, manifest());
    }

    #[test]
    fn a_component_that_is_not_built_yet_is_unavailable() {
        let mut codec = MfCodec::new();
        let error = codec.probe(DECODER_ID, Path::new("a.mp4")).unwrap_err();
        assert!(
            matches!(&error, MediaError::ComponentUnavailable { component, .. } if component == DECODER_ID),
            "{error:?}"
        );
        assert!(matches!(
            codec.probe("avfoundation", Path::new("a.mp4")),
            Err(MediaError::Codec(_))
        ));
    }
}
