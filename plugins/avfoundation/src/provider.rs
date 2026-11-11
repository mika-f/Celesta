//! [`AvfCodec`], the plugin's [`MediaProvider`], and the plugin as one made
//! in this process ([`registered`], [`install`]).
//!
//! It describes every component, from what this Mac's frameworks list. Its
//! encoder and muxer sessions, of every kind, are numbered from one counter;
//! the decoder's sequential sessions are named by the renderer.

use crate::audio::{self, AudioSession};
use crate::components::{self, AudioCodec, MuxerFormat, VideoCodec};
use crate::decoder::AvfDecoder;
use crate::muxer::{self, MuxerSession};
use crate::video::{self, VideoSession};
use crate::{DECODER_ID, PLUGIN_ID, apple};
use celesta_media::{
    AudioBuffer, AudioEncoder, AudioEncoderOpened, AudioEncoderRequest, ComponentDescription,
    Decoder, Encoded, MediaError, MediaProbe, MediaProvider, MediaRegistry, Muxer, MuxerRequest,
    ProviderFactory, RegisteredPlugin, SessionId, TrackPacket, VideoEncoder, VideoEncoderOpened,
    VideoEncoderRequest, VideoFrame, VideoRequest,
};
use celesta_plugin_host::{MediaManifest, MediaOrigin};
use objc2::rc::autoreleasepool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The plugin's manifest source, which declares its components.
const PLUGIN_JSON: &str = include_str!("../plugin.json");

/// A [`MediaProvider`] on VideoToolbox, AudioToolbox, and AVFoundation.
#[derive(Default)]
pub struct AvfCodec {
    decoder: AvfDecoder,
    video: HashMap<SessionId, VideoSession>,
    audio: HashMap<SessionId, AudioSession>,
    muxers: HashMap<SessionId, MuxerSession>,
    /// The last session number given, one count for every kind.
    last: u64,
}

impl AvfCodec {
    /// A [`ProviderFactory`] making an [`AvfCodec`] in this process,
    /// whatever it is for.
    pub fn factory() -> ProviderFactory {
        Arc::new(|_kill, _use| Ok(Box::new(Self::default()) as Box<dyn MediaProvider>))
    }

    fn next_session(&mut self) -> SessionId {
        self.last += 1;
        SessionId(self.last)
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

/// Runs `call` in an autorelease pool of its own. The program serves on a
/// plain thread, which has no pool, and the frameworks autorelease objects
/// (convenience constructors, `localizedDescription`, their own internals):
/// without one they would live as long as the process.
fn pooled<T>(call: impl FnOnce() -> T) -> T {
    autoreleasepool(|_| call())
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
        name: PLUGIN_ID.to_owned(),
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
        Ok(pooled(|| {
            let video = apple::video_encoder_types();
            let audio = apple::audio_encoder_formats();
            let rates = apple::aac_sample_rates();
            components::describe_from(
                video.as_deref().map_err(String::as_str),
                audio.as_deref().map_err(String::as_str),
                rates.as_deref().map_err(String::as_str),
            )
        }))
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
    fn probe(&mut self, decoder: &str, path: &Path) -> Result<MediaProbe, MediaError> {
        check_decoder(decoder)?;
        pooled(|| self.decoder.probe(path))
    }

    fn decode_video(
        &mut self,
        decoder: &str,
        request: &VideoRequest<'_>,
    ) -> Result<VideoFrame, MediaError> {
        check_decoder(decoder)?;
        pooled(|| self.decoder.decode(request))
    }

    fn decode_audio(
        &mut self,
        decoder: &str,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        check_decoder(decoder)?;
        pooled(|| self.decoder.decode_audio(path, sample_rate, channels))
    }
}

impl VideoEncoder for AvfCodec {
    fn open_video(
        &mut self,
        request: &VideoEncoderRequest<'_>,
    ) -> Result<VideoEncoderOpened, MediaError> {
        let codec = VideoCodec::from_id(request.component)
            .ok_or_else(|| no_component("video encoder", request.component))?;
        let (opened, input, alpha) = pooled(|| video::open(codec, request))?;
        let session = self.next_session();
        self.video.insert(session, opened);
        Ok(VideoEncoderOpened {
            session,
            input,
            alpha,
        })
    }

    fn encode_video(
        &mut self,
        session: SessionId,
        pts: i64,
        frame: Vec<u8>,
    ) -> Result<Encoded, MediaError> {
        let opened = self
            .video
            .get_mut(&session)
            .ok_or_else(|| no_session("video encoder", session))?;
        pooled(|| opened.encode(pts, &frame))
    }

    fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        let opened = self
            .video
            .remove(&session)
            .ok_or_else(|| no_session("video encoder", session))?;
        pooled(|| opened.finish())
    }

    /// Ends a video encoder session without completing its frames (dropping
    /// it invalidates the `VTCompressionSession`); a session of another
    /// kind is left alone.
    fn abort_video(&mut self, session: SessionId) {
        pooled(|| drop(self.video.remove(&session)));
    }
}

impl AudioEncoder for AvfCodec {
    fn open_audio(
        &mut self,
        request: &AudioEncoderRequest<'_>,
    ) -> Result<AudioEncoderOpened, MediaError> {
        AudioCodec::from_id(request.component)
            .ok_or_else(|| no_component("audio encoder", request.component))?;
        let (opened, sample_rate, channels, delay) = pooled(|| audio::open(request))?;
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
        let opened = self
            .audio
            .get_mut(&session)
            .ok_or_else(|| no_session("audio encoder", session))?;
        pooled(|| opened.encode(pts, samples))
    }

    fn finish_audio(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        let opened = self
            .audio
            .remove(&session)
            .ok_or_else(|| no_session("audio encoder", session))?;
        pooled(|| opened.finish())
    }

    /// Ends an audio encoder session without flushing it (dropping it
    /// disposes of the `AudioConverter`); a session of another kind is left
    /// alone.
    fn abort_audio(&mut self, session: SessionId) {
        pooled(|| drop(self.audio.remove(&session)));
    }
}

impl Muxer for AvfCodec {
    fn open_muxer(&mut self, request: &MuxerRequest<'_>) -> Result<SessionId, MediaError> {
        let format = MuxerFormat::from_id(request.component)
            .ok_or_else(|| no_component("muxer", request.component))?;
        let opened = pooled(|| muxer::open(format, request))?;
        let session = self.next_session();
        self.muxers.insert(session, opened);
        Ok(session)
    }

    fn write_packets(
        &mut self,
        session: SessionId,
        packets: &[TrackPacket],
    ) -> Result<(), MediaError> {
        let opened = self
            .muxers
            .get_mut(&session)
            .ok_or_else(|| no_session("muxer", session))?;
        pooled(|| opened.write(packets))
    }

    fn finish_muxer(&mut self, session: SessionId) -> Result<Vec<PathBuf>, MediaError> {
        let opened = self
            .muxers
            .remove(&session)
            .ok_or_else(|| no_session("muxer", session))?;
        pooled(|| opened.finish())
    }

    /// Cancels a muxer session's writing: the file it leaves, for Celesta
    /// to remove; nothing for a session that is not open.
    fn abort_muxer(&mut self, session: SessionId) -> Vec<PathBuf> {
        pooled(|| {
            self.muxers
                .remove(&session)
                .map(MuxerSession::abort)
                .unwrap_or_default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_composition::Rational;
    use celesta_media::{Color, PixelFormat, Settings, Track, TrackConfig};
    use celesta_plugin_host::MediaManifest;

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

    fn audio_request<'a>(component: &'a str, settings: &'a Settings) -> AudioEncoderRequest<'a> {
        AudioEncoderRequest {
            component,
            settings,
            sample_rate: 48_000,
            channels: 2,
        }
    }

    #[test]
    fn other_components_are_refused() {
        let mut codec = AvfCodec::default();
        let settings = Settings::new();
        let color = Color::bt709_limited();
        assert!(matches!(
            codec.open_video(&video_request("libx264", &settings, &color)),
            Err(MediaError::Codec(message)) if message == r#"no video encoder "libx264" in avfoundation"#
        ));
        assert!(matches!(
            codec.open_audio(&audio_request("opus", &settings)),
            Err(MediaError::Codec(message)) if message == r#"no audio encoder "opus" in avfoundation"#
        ));
        let request = MuxerRequest {
            component: "webm",
            settings: &settings,
            output: Path::new("/nonexistent/out.webm"),
            tracks: &[],
            leading_audio: 0,
        };
        assert!(matches!(
            codec.open_muxer(&request),
            Err(MediaError::Codec(message)) if message == r#"no muxer "webm" in avfoundation"#
        ));
        // The decoder reads: a missing file is not found.
        let path = Path::new("/nonexistent/clip.mp4");
        assert!(matches!(
            codec.probe(DECODER_ID, path),
            Err(MediaError::NotFound(missing)) if missing == path
        ));
        assert!(matches!(
            codec.decode_audio(DECODER_ID, path, 48_000, 2),
            Err(MediaError::NotFound(_))
        ));
        assert!(matches!(
            codec.probe("ffmpeg", path),
            Err(MediaError::Codec(message)) if message == r#"no decoder "ffmpeg" in avfoundation"#
        ));
    }

    #[test]
    fn video_sessions_count_from_one_and_abort_alone() {
        let mut codec = AvfCodec::default();
        let settings = Settings::new();
        let color = Color::bt709_limited();
        let first = codec
            .open_video(&video_request("h264", &settings, &color))
            .unwrap();
        let second = codec
            .open_video(&video_request("h264", &settings, &color))
            .unwrap();
        assert_eq!(
            (first.session, second.session),
            (SessionId(1), SessionId(2))
        );
        assert_eq!(first.input, PixelFormat::Nv12);
        // A refused open takes no number.
        let unknown = Color {
            primaries: "smpte432".into(),
            ..Color::bt709_limited()
        };
        assert!(matches!(
            codec.open_video(&video_request("h264", &settings, &unknown)),
            Err(MediaError::Codec(message)) if message == "unknown color primaries `smpte432`"
        ));
        let bad = [(
            "profile".to_owned(),
            celesta_media::SettingValue::String("high10".into()),
        )]
        .into();
        assert!(matches!(
            codec.open_video(&video_request("h264", &bad, &color)),
            Err(MediaError::InvalidSettings { key, .. }) if key == "profile"
        ));
        let third = codec
            .open_video(&video_request("h264", &settings, &color))
            .unwrap();
        assert_eq!(third.session, SessionId(3));

        // `abortEncoder` reaches both kinds: the audio side leaves a video
        // session alone, the video side ends it.
        codec.abort_audio(first.session);
        let frame = vec![128; PixelFormat::Nv12.frame_bytes(64, 36)];
        codec.encode_video(first.session, 0, frame.clone()).unwrap();
        codec.abort_video(first.session);
        assert!(codec.encode_video(first.session, 1, frame.clone()).is_err());
        assert!(codec.finish_video(first.session).is_err());
        let finished = codec.encode_video(second.session, 0, frame).unwrap();
        let rest = codec.finish_video(second.session).unwrap();
        assert_eq!(finished.packets.len() + rest.packets.len(), 1);
        assert!(codec.finish_video(second.session).is_err());
        codec.abort_video(third.session);
        codec.abort_video(third.session);
    }

    #[test]
    fn audio_sessions_share_the_count_and_abort_alone() {
        let mut codec = AvfCodec::default();
        let settings = Settings::new();
        let color = Color::bt709_limited();
        let video = codec
            .open_video(&video_request("h264", &settings, &color))
            .unwrap();
        let audio = codec.open_audio(&audio_request("aac", &settings)).unwrap();
        assert_eq!((video.session, audio.session), (SessionId(1), SessionId(2)));
        // A refused open takes no number.
        let high = [(
            "bitrate".to_owned(),
            celesta_media::SettingValue::Number(400_000.0),
        )]
        .into();
        assert!(matches!(
            codec.open_audio(&audio_request("aac", &high)),
            Err(MediaError::InvalidSettings { key, .. }) if key == "bitrate"
        ));
        let other = codec.open_audio(&audio_request("aac", &settings)).unwrap();
        assert_eq!(other.session, SessionId(3));

        // Each side of `abortEncoder` ends only its own kind's session.
        codec.abort_video(audio.session);
        codec.encode_audio(audio.session, 0, &[0.0; 2048]).unwrap();
        codec.abort_audio(video.session);
        let frame = vec![128; PixelFormat::Nv12.frame_bytes(64, 36)];
        codec.encode_video(video.session, 0, frame).unwrap();
        codec.abort_audio(audio.session);
        assert!(matches!(
            codec.encode_audio(audio.session, 1_024, &[0.0; 2048]),
            Err(MediaError::Codec(message)) if message == "no open audio encoder 2"
        ));
        assert!(codec.finish_audio(audio.session).is_err());
        let finished = codec.finish_audio(other.session).unwrap();
        assert!(finished.config.is_some());
        assert!(codec.finish_audio(other.session).is_err());
        codec.abort_video(video.session);
    }

    #[test]
    fn muxer_sessions_share_the_count_and_abort_alone() {
        let mut codec = AvfCodec::default();
        let settings = Settings::new();
        let color = Color::bt709_limited();
        let video = codec
            .open_video(&video_request("h264", &settings, &color))
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("out.m4a");
        let tracks = [Track::Audio {
            config: TrackConfig {
                codec: "mp4a.40.2".to_owned(),
                description: Some(vec![0x11, 0x90]),
            },
            timescale: 48_000,
            sample_rate: 48_000,
            channels: 2,
            delay: 2_112,
        }];
        let request = |component, output| MuxerRequest {
            component,
            settings: &settings,
            output,
            tracks: &tracks,
            leading_audio: 0,
        };
        // A refused open takes no number.
        assert!(matches!(
            codec.open_muxer(&request("mov", Path::new("/nonexistent/out.mov"))),
            Err(MediaError::Codec(_))
        ));
        let muxer = codec.open_muxer(&request("mp4", &output)).unwrap();
        assert_eq!((video.session, muxer), (SessionId(1), SessionId(2)));

        // `abortEncoder` leaves a muxer alone, and `abortMuxer` an encoder.
        codec.abort_video(muxer);
        codec.abort_audio(muxer);
        assert!(codec.abort_muxer(video.session).is_empty());
        let frame = vec![128; PixelFormat::Nv12.frame_bytes(64, 36)];
        codec.encode_video(video.session, 0, frame).unwrap();
        assert_eq!(codec.abort_muxer(muxer), [output]);
        assert!(codec.abort_muxer(muxer).is_empty());
        assert!(matches!(
            codec.write_packets(muxer, &[]),
            Err(MediaError::Codec(message)) if message == "no open muxer 2"
        ));
        codec.abort_video(video.session);
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
