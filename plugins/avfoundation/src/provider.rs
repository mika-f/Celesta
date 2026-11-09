//! [`AvfCodec`], the plugin's [`MediaProvider`], and the plugin as one made
//! in this process ([`registered`], [`install`]).
//!
//! It describes every component, from what this Mac's frameworks list; a
//! component it cannot open yet answers
//! [`MediaError::ComponentUnavailable`].

use crate::components::{self, AudioCodec, MuxerFormat, VideoCodec};
use crate::{DECODER_ID, PLUGIN_ID, apple};
use celesta_media::{
    AudioBuffer, AudioEncoder, AudioEncoderOpened, AudioEncoderRequest, ComponentDescription,
    Decoder, Encoded, MediaError, MediaProbe, MediaProvider, MediaRegistry, Muxer, MuxerRequest,
    ProviderFactory, RegisteredPlugin, SessionId, TrackPacket, VideoEncoder, VideoEncoderOpened,
    VideoEncoderRequest, VideoFrame, VideoRequest,
};
use celesta_plugin_host::{MediaManifest, MediaOrigin};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The plugin's manifest source, which declares its components.
const PLUGIN_JSON: &str = include_str!("../plugin.json");

/// Why a component the plugin declares cannot be opened yet.
const NOT_IMPLEMENTED: &str = "not implemented yet";

/// A [`MediaProvider`] on VideoToolbox, AudioToolbox, and AVFoundation.
#[derive(Default)]
pub struct AvfCodec {}

impl AvfCodec {
    /// A [`ProviderFactory`] making an [`AvfCodec`] in this process,
    /// whatever it is for.
    pub fn factory() -> ProviderFactory {
        Arc::new(|_kill, _use| Ok(Box::new(Self::default()) as Box<dyn MediaProvider>))
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

/// The error for `component`, which the plugin declares but cannot open yet.
fn not_implemented(component: &str) -> MediaError {
    MediaError::ComponentUnavailable {
        component: component.to_owned(),
        reason: NOT_IMPLEMENTED.to_owned(),
    }
}

/// The decoder the request names, which only [`DECODER_ID`] is; it cannot
/// read anything yet.
fn unopened_decoder<T>(decoder: &str) -> Result<T, MediaError> {
    if decoder == DECODER_ID {
        Err(not_implemented(DECODER_ID))
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
        factory: AvfCodec::factory(),
    }
}

/// Makes [`AvfCodec`], in this process, the media plugin every
/// `celesta_media::MediaBackend` uses: for tests and tools, which need no
/// plugin installed.
pub fn install() {
    celesta_media::set_registry(Some(Arc::new(MediaRegistry::new(vec![registered()]))));
}

impl MediaProvider for AvfCodec {
    fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
        Ok(components::describe_from(
            &apple::video_encoder_types(),
            &apple::audio_encoder_formats(),
            &apple::aac_sample_rates(),
        ))
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
        Some(self)
    }
}

impl Decoder for AvfCodec {
    fn probe(&mut self, decoder: &str, _path: &Path) -> Result<MediaProbe, MediaError> {
        unopened_decoder(decoder)
    }

    fn decode_video(
        &mut self,
        decoder: &str,
        _request: &VideoRequest<'_>,
    ) -> Result<VideoFrame, MediaError> {
        unopened_decoder(decoder)
    }

    fn decode_audio(
        &mut self,
        decoder: &str,
        _path: &Path,
        _sample_rate: u32,
        _channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        unopened_decoder(decoder)
    }
}

impl VideoEncoder for AvfCodec {
    fn open_video(
        &mut self,
        request: &VideoEncoderRequest<'_>,
    ) -> Result<VideoEncoderOpened, MediaError> {
        let codec = VideoCodec::from_id(request.component)
            .ok_or_else(|| no_component("video encoder", request.component))?;
        Err(not_implemented(codec.id()))
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

    /// No video encoder session is ever open, so there is none to abort.
    fn abort_video(&mut self, _session: SessionId) {}
}

impl AudioEncoder for AvfCodec {
    fn open_audio(
        &mut self,
        request: &AudioEncoderRequest<'_>,
    ) -> Result<AudioEncoderOpened, MediaError> {
        let codec = AudioCodec::from_id(request.component)
            .ok_or_else(|| no_component("audio encoder", request.component))?;
        Err(not_implemented(codec.id()))
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

    /// No audio encoder session is ever open, so there is none to abort.
    fn abort_audio(&mut self, _session: SessionId) {}
}

impl Muxer for AvfCodec {
    fn open_muxer(&mut self, request: &MuxerRequest<'_>) -> Result<SessionId, MediaError> {
        let format = MuxerFormat::from_id(request.component)
            .ok_or_else(|| no_component("muxer", request.component))?;
        Err(not_implemented(format.id()))
    }

    fn write_packets(
        &mut self,
        session: SessionId,
        _packets: &[TrackPacket],
    ) -> Result<(), MediaError> {
        Err(no_session("muxer", session))
    }

    fn finish_muxer(&mut self, session: SessionId) -> Result<Vec<PathBuf>, MediaError> {
        Err(no_session("muxer", session))
    }

    /// No muxer session is ever open, so it leaves no files.
    fn abort_muxer(&mut self, _session: SessionId) -> Vec<PathBuf> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_composition::Rational;
    use celesta_media::{Color, PixelFormat, Settings};
    use celesta_plugin_host::MediaManifest;

    fn unavailable<T: std::fmt::Debug>(result: Result<T, MediaError>) -> (String, String) {
        match result {
            Err(MediaError::ComponentUnavailable { component, reason }) => (component, reason),
            other => panic!("expected componentUnavailable, got {other:?}"),
        }
    }

    #[test]
    fn registered_declares_the_manifest() {
        let plugin = registered();
        assert_eq!(plugin.id, PLUGIN_ID);
        assert_eq!(plugin.origin, MediaOrigin::Bundled);
        assert_ne!(plugin.media, MediaManifest::default());
        assert_eq!(plugin.media, manifest());
    }

    #[test]
    fn components_are_not_opened_yet() {
        let mut codec = AvfCodec::default();
        let settings = Settings::new();
        let color = Color::bt709_limited();
        let video = |component| VideoEncoderRequest {
            component,
            settings: &settings,
            width: 64,
            height: 36,
            frame_rate: Rational::new(30, 1),
            timescale: 30,
            offered: &[PixelFormat::Rgba16],
            color: &color,
        };
        for codec_id in ["h264", "hevc", "prores"] {
            assert_eq!(
                unavailable(codec.open_video(&video(codec_id))),
                (codec_id.to_owned(), NOT_IMPLEMENTED.to_owned())
            );
        }
        assert!(matches!(
            codec.open_video(&video("libx264")),
            Err(MediaError::Codec(message)) if message == r#"no video encoder "libx264" in avfoundation"#
        ));
        let audio = |component| AudioEncoderRequest {
            component,
            settings: &settings,
            sample_rate: 48_000,
            channels: 2,
        };
        assert_eq!(
            unavailable(codec.open_audio(&audio("aac"))),
            ("aac".to_owned(), NOT_IMPLEMENTED.to_owned())
        );
        assert!(matches!(
            codec.open_audio(&audio("opus")),
            Err(MediaError::Codec(_))
        ));
        let output = Path::new("/nonexistent/out.mp4");
        for format in ["mp4", "mov"] {
            let request = MuxerRequest {
                component: format,
                settings: &settings,
                output,
                tracks: &[],
                leading_audio: 0,
            };
            assert_eq!(
                unavailable(codec.open_muxer(&request)),
                (format.to_owned(), NOT_IMPLEMENTED.to_owned())
            );
        }
        let path = Path::new("/nonexistent/clip.mp4");
        assert_eq!(
            unavailable(codec.probe(DECODER_ID, path)),
            (DECODER_ID.to_owned(), NOT_IMPLEMENTED.to_owned())
        );
        assert_eq!(
            unavailable(codec.decode_audio(DECODER_ID, path, 48_000, 2)),
            (DECODER_ID.to_owned(), NOT_IMPLEMENTED.to_owned())
        );
        assert!(matches!(
            codec.probe("ffmpeg", path),
            Err(MediaError::Codec(_))
        ));
    }

    #[test]
    fn no_session_is_open() {
        let mut codec = AvfCodec::default();
        let session = SessionId(1);
        assert!(matches!(
            codec.encode_video(session, 0, Vec::new()),
            Err(MediaError::Codec(message)) if message == "no open video encoder 1"
        ));
        assert!(codec.finish_video(session).is_err());
        assert!(codec.encode_audio(session, 0, &[]).is_err());
        assert!(codec.finish_audio(session).is_err());
        assert!(codec.write_packets(session, &[]).is_err());
        assert!(codec.finish_muxer(session).is_err());
        codec.abort_video(session);
        codec.abort_audio(session);
        assert!(codec.abort_muxer(session).is_empty());
    }
}
