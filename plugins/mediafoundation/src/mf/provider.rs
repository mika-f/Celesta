//! [`MfCodec`], the plugin's [`MediaProvider`], and the plugin as one made
//! in this process ([`registered`], [`install`]).
//!
//! It describes every component, from what this system's Media Foundation
//! lists. Its encoder sessions, of every kind, are numbered from one counter.
//! Its decoder is [`MfDecoder`], on the source reader.

use crate::components::{self, AudioCodec, VideoCodec};
use crate::mf::audio::{self, AudioSession};
use crate::mf::decoder::MfDecoder;
use crate::mf::video::{self, VideoSession};
use crate::mf::{enumerate, startup::Started};
use crate::{DECODER_ID, PLUGIN_ID};
use celesta_media::{
    AudioBuffer, AudioEncoder, AudioEncoderOpened, AudioEncoderRequest, ComponentDescription,
    Decoder, Encoded, MediaError, MediaProbe, MediaProvider, MediaRegistry, ProviderFactory,
    RegisteredPlugin, SessionId, VideoEncoder, VideoEncoderOpened, VideoEncoderRequest, VideoFrame,
    VideoRequest,
};
use celesta_plugin_host::{MediaManifest, MediaOrigin};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The plugin's manifest source, which declares its components.
const PLUGIN_JSON: &str = include_str!("../../plugin.json");

/// A [`MediaProvider`] on Media Foundation.
pub struct MfCodec {
    /// The open video encoder sessions. Declared before `startup`, so they
    /// are released before Media Foundation shuts down.
    video: HashMap<SessionId, VideoSession>,
    /// The open audio encoder sessions, likewise.
    audio: HashMap<SessionId, AudioSession>,
    /// The decoder and its sequential sessions' readers, likewise.
    decoder: MfDecoder,
    /// The last session number given, one count for every kind.
    last: u64,
    /// COM and Media Foundation as started, or why they did not: `describe`
    /// says it rather than the program failing to start.
    startup: Result<Started, String>,
}

impl MfCodec {
    /// Starts COM and Media Foundation. A failure is kept for `describe`.
    pub fn new() -> Self {
        Self {
            video: HashMap::new(),
            audio: HashMap::new(),
            decoder: MfDecoder::default(),
            last: 0,
            startup: Started::new(),
        }
    }

    /// The friendly name of the transform an open video encoder `session`
    /// encodes with (`MFT_FRIENDLY_NAME_Attribute`), for tests and tools to
    /// tell a hardware encoder from Microsoft's software one.
    pub fn encoder_name(&self, session: SessionId) -> Option<&str> {
        self.video.get(&session).map(VideoSession::encoder_name)
    }

    fn next_session(&mut self) -> SessionId {
        self.last += 1;
        SessionId(self.last)
    }

    /// `Ok` when Media Foundation started; otherwise `component` is
    /// unavailable, as `describe` says.
    fn started(&self, component: &str) -> Result<(), MediaError> {
        match &self.startup {
            Ok(_) => Ok(()),
            Err(error) => Err(MediaError::ComponentUnavailable {
                component: component.to_owned(),
                reason: components::not_started_reason(error),
            }),
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
    fn probe(&mut self, decoder: &str, path: &Path) -> Result<MediaProbe, MediaError> {
        check_decoder(decoder)?;
        self.started(decoder)?;
        self.decoder.probe(path)
    }

    fn decode_video(
        &mut self,
        decoder: &str,
        request: &VideoRequest<'_>,
    ) -> Result<VideoFrame, MediaError> {
        check_decoder(decoder)?;
        self.started(decoder)?;
        self.decoder.decode(request)
    }

    fn decode_audio(
        &mut self,
        decoder: &str,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        check_decoder(decoder)?;
        self.started(decoder)?;
        self.decoder.decode_audio(path, sample_rate, channels)
    }
}

impl VideoEncoder for MfCodec {
    fn open_video(
        &mut self,
        request: &VideoEncoderRequest<'_>,
    ) -> Result<VideoEncoderOpened, MediaError> {
        let codec = VideoCodec::from_id(request.component)
            .ok_or_else(|| no_component("video encoder", request.component))?;
        self.started(request.component)?;
        let (opened, input) = video::open(codec, request)?;
        let session = self.next_session();
        self.video.insert(session, opened);
        Ok(VideoEncoderOpened {
            session,
            input,
            alpha: false,
        })
    }

    fn encode_video(
        &mut self,
        session: SessionId,
        pts: i64,
        frame: Vec<u8>,
    ) -> Result<Encoded, MediaError> {
        self.video
            .get_mut(&session)
            .ok_or_else(|| no_session("video encoder", session))?
            .encode(pts, frame)
    }

    fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        self.video
            .remove(&session)
            .ok_or_else(|| no_session("video encoder", session))?
            .finish()
    }

    /// Ends a video encoder session without completing its frames (dropping
    /// it shuts the transform down); a session of another kind is left
    /// alone.
    fn abort_video(&mut self, session: SessionId) {
        drop(self.video.remove(&session));
    }
}

impl AudioEncoder for MfCodec {
    fn open_audio(
        &mut self,
        request: &AudioEncoderRequest<'_>,
    ) -> Result<AudioEncoderOpened, MediaError> {
        AudioCodec::from_id(request.component)
            .ok_or_else(|| no_component("audio encoder", request.component))?;
        self.started(request.component)?;
        let (opened, sample_rate, channels, delay) = audio::open(request)?;
        let session = self.next_session();
        self.audio.insert(session, opened);
        Ok(AudioEncoderOpened {
            session,
            sample_rate,
            channels,
            delay,
        })
    }

    fn encode_audio(
        &mut self,
        session: SessionId,
        pts: i64,
        samples: &[f32],
    ) -> Result<Encoded, MediaError> {
        self.audio
            .get_mut(&session)
            .ok_or_else(|| no_session("audio encoder", session))?
            .encode(pts, samples)
    }

    fn finish_audio(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        self.audio
            .remove(&session)
            .ok_or_else(|| no_session("audio encoder", session))?
            .finish()
    }

    /// Ends an audio encoder session without flushing it (dropping it shuts
    /// the transform down); a session of another kind, such as a video
    /// encoder's, is left alone.
    fn abort_audio(&mut self, session: SessionId) {
        drop(self.audio.remove(&session));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mf::gate;
    use celesta_composition::Rational;
    use celesta_media::{Color, PixelFormat, SettingValue, Settings};

    #[test]
    fn registered_declares_the_manifest() {
        let plugin = registered();
        assert_eq!(plugin.id, PLUGIN_ID);
        assert_eq!(plugin.origin, MediaOrigin::Bundled);
        assert_ne!(plugin.media, MediaManifest::default());
        assert_eq!(plugin.media, manifest());
    }

    fn video_request<'a>(
        component: &'a str,
        settings: &'a Settings,
        color: &'a Color,
    ) -> VideoEncoderRequest<'a> {
        VideoEncoderRequest {
            component,
            settings,
            width: 64,
            height: 36,
            frame_rate: Rational::new(30, 1),
            timescale: 30,
            offered: &[PixelFormat::Nv12, PixelFormat::Rgba16],
            color,
        }
    }

    #[test]
    fn other_components_are_refused() {
        let mut codec = MfCodec::new();
        let settings = Settings::new();
        let color = Color::bt709_limited();
        assert!(matches!(
            codec.open_video(&video_request("libx264", &settings, &color)),
            Err(MediaError::Codec(message)) if message == r#"no video encoder "libx264" in mediafoundation"#
        ));
    }

    #[test]
    fn no_software_encoder_asks_for_hardware() {
        let mut codec = MfCodec::new();
        if !gate::available(
            &mut codec,
            &[DECODER_ID],
            "no_software_encoder_asks_for_hardware",
        ) {
            return;
        }
        let color = Color::bt709_limited();
        // No software MFT takes P010 (Task 1); a system whose does opens it.
        let ten_bit: Settings = [
            ("bitDepth".to_owned(), SettingValue::String("10".into())),
            ("hardware".to_owned(), SettingValue::Boolean(false)),
        ]
        .into();
        let request = VideoEncoderRequest {
            offered: &[PixelFormat::P010],
            ..video_request("hevc", &ten_bit, &color)
        };
        match codec.open_video(&request) {
            Ok(opened) => codec.abort_video(opened.session),
            Err(MediaError::ComponentUnavailable { component, reason }) => {
                assert_eq!(component, "hevc");
                assert_eq!(reason, "no software HEVC encoder; turn Hardware on");
            }
            Err(error) => panic!("{error:?}"),
        }
    }

    #[test]
    fn sessions_share_one_count_and_abort_alone() {
        let mut codec = MfCodec::new();
        if !gate::available(
            &mut codec,
            &["h264"],
            "sessions_share_one_count_and_abort_alone",
        ) {
            return;
        }
        let software: Settings = [("hardware".to_owned(), SettingValue::Boolean(false))].into();
        let color = Color::bt709_limited();
        let first = codec
            .open_video(&video_request("h264", &software, &color))
            .unwrap();
        let second = codec
            .open_video(&video_request("h264", &software, &color))
            .unwrap();
        assert_eq!(
            (first.session, second.session),
            (SessionId(1), SessionId(2))
        );
        assert_eq!(first.input, PixelFormat::Nv12);
        assert!(!first.alpha);
        assert_eq!(codec.encoder_name(first.session), Some("H264 Encoder MFT"));
        // A refused open takes no number.
        let unknown = Color {
            primaries: "smpte432".into(),
            ..Color::bt709_limited()
        };
        assert!(matches!(
            codec.open_video(&video_request("h264", &software, &unknown)),
            Err(MediaError::Codec(message)) if message == "unknown color primaries `smpte432`"
        ));
        let bad: Settings = [("profile".to_owned(), SettingValue::String("high10".into()))].into();
        assert!(matches!(
            codec.open_video(&video_request("h264", &bad, &color)),
            Err(MediaError::InvalidSettings { key, .. }) if key == "profile"
        ));
        let third = codec
            .open_video(&video_request("h264", &software, &color))
            .unwrap();
        assert_eq!(third.session, SessionId(3));

        // `abortEncoder` reaches both kinds: the audio side leaves a video
        // session alone, the video side ends it.
        codec.abort_audio(first.session);
        let frame = vec![128; PixelFormat::Nv12.frame_bytes(64, 36)];
        codec.encode_video(first.session, 0, frame.clone()).unwrap();
        codec.abort_video(first.session);
        assert!(matches!(
            codec.encode_video(first.session, 1, frame.clone()),
            Err(MediaError::Codec(message)) if message == "no open video encoder 1"
        ));
        assert!(codec.finish_video(first.session).is_err());
        assert_eq!(codec.encoder_name(first.session), None);
        let answered = codec.encode_video(second.session, 0, frame).unwrap();
        let rest = codec.finish_video(second.session).unwrap();
        assert_eq!(answered.packets.len() + rest.packets.len(), 1);
        assert!(codec.finish_video(second.session).is_err());
        codec.abort_video(third.session);
        codec.abort_video(third.session);
    }

    #[test]
    fn the_decoder_answers_for_its_own_id() {
        let mut codec = MfCodec::new();
        if !gate::available(
            &mut codec,
            &[DECODER_ID],
            "the_decoder_answers_for_its_own_id",
        ) {
            return;
        }
        let missing = Path::new("no such file.mp4");
        assert!(matches!(
            codec.probe(DECODER_ID, missing),
            Err(MediaError::NotFound(path)) if path == missing
        ));
        assert!(matches!(
            codec.probe("avfoundation", missing),
            Err(MediaError::Codec(message)) if message == r#"no decoder "avfoundation" in mediafoundation"#
        ));
    }
}
