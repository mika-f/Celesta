//! A lossless codec for Celesta's tests. [`TestCodec`] is a media plugin with
//! all four kinds of component: a video encoder that stores the frames it is
//! given (in their [`PixelFormat`], with no conversion), an audio encoder that
//! stores `f32` samples, a muxer that writes both into one file, and a decoder
//! that reads that file back; it also reads 16-bit PCM WAV, so tests keep the
//! checked-in voices. Encoded video is LZ4-compressed. It honours the
//! [`MediaProvider`] contract the exporter relies on: settings validation,
//! the audio delay and leading audio, and errors for what it does not
//! support.
//!
//! Tests use it in their own process ([`install`]), or, for a child process
//! such as `celesta-exporter`, as the [`PROGRAM`] in a temporary plugin
//! folder ([`TestCodecPlugin`]). `src/format.rs` describes the file it
//! writes; [`read`] gives a file's contents to compare byte for byte.

mod components;
mod format;
mod plugin;
mod wav;

pub use celesta_plugin_host::PLUGIN_PATH_ENV;
pub use components::{
    AUDIO_CODEC, AUDIO_ENCODER_ID, DECODER_ID, DECODER_NAME, MUXER_ID, VIDEO_CODEC,
    VIDEO_ENCODER_ID,
};
pub use format::{StoredAudio, StoredVideo, TestMedia, read, write_audio, write_rgba_video};
pub use plugin::{TestCodecPlugin, profile_dir};

use celesta_composition::Time;
use celesta_media::{
    AudioBuffer, AudioEncoder, AudioEncoderOpened, AudioEncoderRequest, AudioStream,
    ComponentDescription, Decoder, Encoded, InvalidSetting, MediaError, MediaProbe, MediaProvider,
    MediaRegistry, Muxer, MuxerRequest, Packet, PixelFormat, ProviderFactory, RegisteredPlugin,
    SessionId, SettingValue, Settings, TrackConfig, TrackPacket, VideoEncoder, VideoEncoderOpened,
    VideoEncoderRequest, VideoFrame, VideoRequest, VideoStream, settings::validate,
    timestamps_match,
};
use celesta_plugin_host::MediaOrigin;
use format::{Kind, MuxWriter, Scanned, VideoHeader, invalid};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The id the codec's plugin manifest and `hello` give.
pub const PLUGIN_ID: &str = "celesta-test-codec";

/// The name of the program that serves the codec.
pub const PROGRAM: &str = "celesta-codec-test";

/// An open video encoder.
struct VideoSession {
    video: VideoHeader,
    frame_len: usize,
    /// Whether the track's config has not been answered yet.
    config_due: bool,
}

/// An open audio encoder.
struct AudioSession {
    channels: u16,
    delay: u64,
    /// Whether no samples have come yet, so the delay's silence is due.
    first: bool,
    config_due: bool,
}

/// The lossless test codec.
#[derive(Default)]
pub struct TestCodec {
    video: HashMap<SessionId, VideoSession>,
    audio: HashMap<SessionId, AudioSession>,
    muxers: HashMap<SessionId, MuxWriter>,
    /// The last session number given, one count for every kind.
    last: u64,
}

impl TestCodec {
    pub fn new() -> Self {
        Self::default()
    }

    /// A [`ProviderFactory`] making a [`TestCodec`] in this process.
    pub fn factory() -> ProviderFactory {
        Arc::new(|_, _| Ok(Box::new(Self::new()) as Box<dyn MediaProvider>))
    }

    fn next_session(&mut self) -> SessionId {
        self.last += 1;
        SessionId(self.last)
    }
}

/// The codec as a media plugin made in this process: bundled, with its four
/// components.
pub fn registered() -> RegisteredPlugin {
    RegisteredPlugin {
        id: PLUGIN_ID.to_owned(),
        name: "Test codec".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        origin: MediaOrigin::Bundled,
        directory: PathBuf::new(),
        executable: None,
        media: components::manifest(),
        factory: TestCodec::factory(),
    }
}

/// Makes [`TestCodec`], in this process, the media plugin every
/// `celesta_media::MediaBackend` uses.
pub fn install() {
    celesta_media::set_registry(Some(Arc::new(MediaRegistry::new(vec![registered()]))));
}

/// The decoder the request names, which only [`DECODER_ID`] is.
fn check_decoder(decoder: &str) -> Result<(), MediaError> {
    if decoder == DECODER_ID {
        Ok(())
    } else {
        Err(MediaError::Codec(format!(
            "no decoder {decoder:?} in {PLUGIN_ID}"
        )))
    }
}

/// `component` as the one of its kind the plugin has.
fn check_component(kind: &str, component: &str, id: &str) -> Result<(), MediaError> {
    if component == id {
        Ok(())
    } else {
        Err(MediaError::Codec(format!(
            "no {kind} {component:?} in {PLUGIN_ID}"
        )))
    }
}

fn no_session(session: SessionId) -> MediaError {
    MediaError::Codec(format!("no open session {}", session.0))
}

/// `given` checked against `schema`, with the defaults filled in.
fn settings_for(
    schema: &[celesta_media::SettingField],
    given: &Settings,
) -> Result<Settings, MediaError> {
    validate(schema, given)
        .map_err(|InvalidSetting { key, message }| MediaError::InvalidSettings { key, message })
}

fn invalid_setting(key: &str, message: impl Into<String>) -> MediaError {
    MediaError::InvalidSettings {
        key: key.to_owned(),
        message: message.into(),
    }
}

/// A file the codec reads.
enum Source {
    Test(Scanned),
    Wav(wav::Wav),
}

impl Source {
    fn open(path: &Path) -> Result<Self, MediaError> {
        Ok(match format::sniff(path)? {
            (Kind::Test, file) => Self::Test(Scanned::new(path, file)?),
            (Kind::Wav, file) => Self::Wav(wav::read(path, file)?),
        })
    }

    /// [`Self::open`] for a decode: `unsupported` answers only a probe, so a
    /// file the codec does not read is a plain error there.
    fn open_to_decode(path: &Path) -> Result<Self, MediaError> {
        Self::open(path).map_err(|error| match error {
            MediaError::Unsupported { path, .. } => MediaError::Codec(format!(
                "{} is neither a test codec file nor a WAV file, the only files the test codec reads",
                path.display()
            )),
            error => error,
        })
    }

    /// The audio from the video's start on: its sample rate, channels, and
    /// interleaved samples.
    fn played_audio(self) -> Result<Option<(u32, u16, Vec<f32>)>, MediaError> {
        Ok(match self {
            Self::Test(mut scanned) => scanned.audio()?.map(|audio| {
                let played = audio.played().to_vec();
                (audio.sample_rate, audio.channels, played)
            }),
            Self::Wav(wav) => Some((wav.sample_rate, wav.channels, wav.samples)),
        })
    }
}

/// The longer of two durations.
fn longer(a: Option<Time>, b: Option<Time>) -> Option<Time> {
    match (a, b) {
        (Some(a), Some(b)) => Some(match a.cmp_exact(b) {
            Ok(Ordering::Less) => b,
            _ => a,
        }),
        (a, b) => a.or(b),
    }
}

/// The index of the first frame at or after `time` seconds in a stream of
/// `fps` frames a second, the frame FFmpeg's decoder serves: a frame counts as
/// at the request when it is within [`timestamps_match`]'s tolerance, the one
/// the FFmpeg plugin uses too, so a time a rounding error past a frame still
/// lands on that frame.
fn first_frame_at_or_after(time: f64, fps: f64) -> usize {
    let after = (time * fps).ceil() as usize;
    match after.checked_sub(1) {
        Some(before) if timestamps_match(before as f64 / fps, time) => before,
        _ => after,
    }
}

fn audio_stream(
    index: u32,
    codec: &str,
    sample_rate: u32,
    channels: u16,
    frames: usize,
) -> AudioStream {
    AudioStream {
        index,
        codec: Some(codec.to_owned()),
        sample_rate: Some(sample_rate),
        channels: Some(channels),
        duration: Some(Time::samples(frames as i64, sample_rate)),
    }
}

/// `samples` with `from` channels as `to` channels: mono doubled to stereo,
/// stereo averaged to mono.
fn convert_channels(
    path: &Path,
    samples: Vec<f32>,
    from: u16,
    to: u16,
) -> Result<Vec<f32>, MediaError> {
    Ok(match (from, to) {
        _ if from == to => samples,
        (1, 2) => samples
            .iter()
            .flat_map(|&sample| [sample, sample])
            .collect(),
        (2, 1) => samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[left, right]| (left + right) * 0.5)
            .collect(),
        _ => {
            return Err(invalid(
                path,
                format_args!("has {from} channels, which the test codec does not make {to}"),
            ));
        }
    })
}

/// A stored frame as straight RGBA8: RGBA8 as it is, RGBA16 rounded to the
/// nearest 8-bit code.
fn rgba8(path: &Path, format: PixelFormat, frame: Vec<u8>) -> Result<Vec<u8>, MediaError> {
    match format {
        PixelFormat::Rgba8 => Ok(frame),
        PixelFormat::Rgba16 => Ok(frame
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&channel| {
                let value = u32::from(u16::from_le_bytes(channel));
                ((value * 255 + 32767) / 65535) as u8
            })
            .collect()),
        PixelFormat::Yuv420p | PixelFormat::Yuv420p10 | PixelFormat::Nv12 | PixelFormat::P010 => {
            Err(invalid(
                path,
                format_args!(
                    "is {format:?} video, which the test codec does not decode; \
                 compare its frames through celesta_codec_test::read"
                ),
            ))
        }
    }
}

impl MediaProvider for TestCodec {
    fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
        Ok(components::describe())
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

impl Decoder for TestCodec {
    fn probe(&mut self, decoder: &str, path: &Path) -> Result<MediaProbe, MediaError> {
        check_decoder(decoder)?;
        match Source::open(path)? {
            Source::Test(mut scanned) => {
                let video = scanned.header.video.map(|video| VideoStream {
                    index: 0,
                    codec: Some(VIDEO_CODEC.to_owned()),
                    width: video.width,
                    height: video.height,
                    frame_rate: Some(video.frame_rate),
                    duration: Time::frames(scanned.frames.len() as i64, video.frame_rate).ok(),
                    color: Default::default(),
                });
                let audio = match (scanned.header.audio, scanned.audio_frames()?) {
                    (Some(header), Some(frames)) => vec![audio_stream(
                        u32::from(video.is_some()),
                        AUDIO_CODEC,
                        header.sample_rate,
                        header.channels,
                        frames - header.lead_in_frames,
                    )],
                    _ => Vec::new(),
                };
                let duration = longer(
                    video.as_ref().and_then(|video| video.duration),
                    audio.first().and_then(|audio| audio.duration),
                );
                Ok(MediaProbe {
                    duration,
                    video,
                    audio,
                })
            }
            Source::Wav(wav) => {
                let stream =
                    audio_stream(0, wav::CODEC, wav.sample_rate, wav.channels, wav.frames());
                Ok(MediaProbe {
                    duration: stream.duration,
                    video: None,
                    audio: vec![stream],
                })
            }
        }
    }

    fn decode_video(
        &mut self,
        decoder: &str,
        request: &VideoRequest<'_>,
    ) -> Result<VideoFrame, MediaError> {
        check_decoder(decoder)?;
        let path = request.path;
        let Source::Test(mut scanned) = Source::open_to_decode(path)? else {
            return Err(MediaError::NoVideoStream(path.to_owned()));
        };
        let Some(video) = scanned.header.video else {
            return Err(MediaError::NoVideoStream(path.to_owned()));
        };
        if !request.time.is_finite() || request.time < 0.0 {
            return Err(MediaError::InvalidTimestamp(request.time));
        }
        let Some(last) = scanned.frames.len().checked_sub(1) else {
            return Err(invalid(path, "has no frames"));
        };
        let fps = f64::from(video.frame_rate.numerator) / f64::from(video.frame_rate.denominator);
        // The first frame at or after the request, and an overrun holds the
        // last frame, as FFmpeg's decoder does.
        let index = first_frame_at_or_after(request.time, fps).min(last);
        let frame = scanned.frame(index)?;
        Ok(VideoFrame {
            width: video.width,
            height: video.height,
            pixels: Arc::new(rgba8(path, video.format, frame)?),
        })
    }

    fn decode_audio(
        &mut self,
        decoder: &str,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        check_decoder(decoder)?;
        if sample_rate == 0 || channels == 0 {
            return Err(MediaError::InvalidAudioFormat {
                sample_rate,
                channels,
            });
        }
        let samples = match Source::open_to_decode(path)?.played_audio()? {
            None => Vec::new(),
            Some((stored_rate, _, _)) if stored_rate != sample_rate => {
                return Err(MediaError::Codec(format!(
                    "the test codec does not resample: {} is {stored_rate} Hz, not {sample_rate} Hz",
                    path.display()
                )));
            }
            Some((_, stored_channels, samples)) => {
                convert_channels(path, samples, stored_channels, channels)?
            }
        };
        Ok(AudioBuffer {
            sample_rate,
            channels,
            samples,
        })
    }
}

impl VideoEncoder for TestCodec {
    fn open_video(
        &mut self,
        request: &VideoEncoderRequest<'_>,
    ) -> Result<VideoEncoderOpened, MediaError> {
        check_component("video encoder", request.component, VIDEO_ENCODER_ID)?;
        let settings = settings_for(&components::video_settings(), request.settings)?;
        let input = match settings.get("input") {
            Some(SettingValue::String(text)) if text != "auto" => text
                .parse::<PixelFormat>()
                .map_err(|_| invalid_setting("input", format!("{text:?} is not a pixel format")))?,
            _ => *request
                .offered
                .first()
                .ok_or_else(|| invalid_setting("input", "no pixel format is offered"))?,
        };
        if !request.offered.contains(&input) {
            return Err(invalid_setting(
                "input",
                format!("{input} is not among the formats offered"),
            ));
        }
        let alpha = settings.get("alpha") == Some(&SettingValue::Boolean(true));
        if alpha && input != PixelFormat::Rgba16 {
            return Err(invalid_setting("alpha", "needs rgba16 as the input"));
        }
        let video = VideoHeader {
            width: request.width,
            height: request.height,
            frame_rate: request.frame_rate,
            format: input,
        };
        let frame_len = format::checked_video(Path::new("the video"), &video)?;
        let session = self.next_session();
        self.video.insert(
            session,
            VideoSession {
                video,
                frame_len,
                config_due: true,
            },
        );
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
        let state = self
            .video
            .get_mut(&session)
            .ok_or_else(|| no_session(session))?;
        if frame.len() != state.frame_len {
            let VideoHeader {
                width,
                height,
                format,
                ..
            } = state.video;
            return Err(MediaError::Codec(format!(
                "a {width}x{height} {format} frame has {} bytes, not {}",
                state.frame_len,
                frame.len()
            )));
        }
        let packet = Packet {
            pts,
            dts: pts,
            duration: i64::from(state.video.frame_rate.denominator),
            key: true,
            bytes: lz4_flex::block::compress_prepend_size(&frame),
            additional: Vec::new(),
        };
        Ok(Encoded {
            packets: vec![packet],
            config: video_config(state),
        })
    }

    fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        let mut state = self
            .video
            .remove(&session)
            .ok_or_else(|| no_session(session))?;
        Ok(Encoded {
            packets: Vec::new(),
            config: video_config(&mut state),
        })
    }

    fn abort_video(&mut self, session: SessionId) {
        self.video.remove(&session);
    }
}

/// The video track's config, the first time it is asked for: its
/// description is the name of the pixel format.
fn video_config(state: &mut VideoSession) -> Option<TrackConfig> {
    std::mem::take(&mut state.config_due).then(|| TrackConfig {
        codec: VIDEO_CODEC.to_owned(),
        description: Some(state.video.format.to_string().into_bytes()),
    })
}

fn audio_config(state: &mut AudioSession) -> Option<TrackConfig> {
    std::mem::take(&mut state.config_due).then(|| TrackConfig {
        codec: AUDIO_CODEC.to_owned(),
        description: None,
    })
}

impl AudioEncoder for TestCodec {
    fn open_audio(
        &mut self,
        request: &AudioEncoderRequest<'_>,
    ) -> Result<AudioEncoderOpened, MediaError> {
        check_component("audio encoder", request.component, AUDIO_ENCODER_ID)?;
        let settings = settings_for(&components::audio_settings(), request.settings)?;
        if request.sample_rate == 0 || request.channels == 0 {
            return Err(MediaError::InvalidAudioFormat {
                sample_rate: request.sample_rate,
                channels: request.channels,
            });
        }
        let delay = match settings.get("delay") {
            Some(SettingValue::Number(delay)) => *delay as u64,
            _ => 0,
        };
        let sample_rate = match settings.get("sampleRate") {
            Some(SettingValue::String(rate)) if rate != "any" => rate
                .parse()
                .map_err(|_| invalid_setting("sampleRate", format!("{rate:?} is not a rate")))?,
            _ => request.sample_rate,
        };
        let session = self.next_session();
        self.audio.insert(
            session,
            AudioSession {
                channels: request.channels,
                delay,
                first: true,
                config_due: true,
            },
        );
        Ok(AudioEncoderOpened {
            session,
            sample_rate,
            channels: request.channels,
            delay,
        })
    }

    fn encode_audio(
        &mut self,
        session: SessionId,
        pts: i64,
        samples: &[f32],
    ) -> Result<Encoded, MediaError> {
        let state = self
            .audio
            .get_mut(&session)
            .ok_or_else(|| no_session(session))?;
        let channels = usize::from(state.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(MediaError::InvalidAudioSampleCount {
                samples: samples.len(),
                channels: state.channels,
            });
        }
        let delay = i64::try_from(state.delay).expect("a delay is at most 65536");
        let mut packets = Vec::new();
        // The encoder's priming silence goes out first, ahead of the first
        // samples, which the muxer hides with the delay.
        if std::mem::take(&mut state.first) && state.delay > 0 {
            packets.push(Packet {
                pts: pts - delay,
                dts: pts - delay,
                duration: delay,
                key: true,
                bytes: vec![0; state.delay as usize * channels * 4],
                additional: Vec::new(),
            });
        }
        packets.push(Packet {
            pts: pts - delay,
            dts: pts - delay,
            duration: (samples.len() / channels) as i64,
            key: true,
            bytes: format::samples_bytes(samples),
            additional: Vec::new(),
        });
        Ok(Encoded {
            packets,
            config: audio_config(state),
        })
    }

    fn finish_audio(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        let mut state = self
            .audio
            .remove(&session)
            .ok_or_else(|| no_session(session))?;
        Ok(Encoded {
            packets: Vec::new(),
            config: audio_config(&mut state),
        })
    }

    fn abort_audio(&mut self, session: SessionId) {
        self.audio.remove(&session);
    }
}

impl Muxer for TestCodec {
    fn open_muxer(&mut self, request: &MuxerRequest<'_>) -> Result<SessionId, MediaError> {
        check_component("muxer", request.component, MUXER_ID)?;
        settings_for(&[], request.settings)?;
        let writer = MuxWriter::create(request.output, request.tracks, request.leading_audio)?;
        let session = self.next_session();
        self.muxers.insert(session, writer);
        Ok(session)
    }

    fn write_packets(
        &mut self,
        session: SessionId,
        packets: &[TrackPacket],
    ) -> Result<(), MediaError> {
        let writer = self
            .muxers
            .get_mut(&session)
            .ok_or_else(|| no_session(session))?;
        packets.iter().try_for_each(|packet| writer.write(packet))
    }

    fn finish_muxer(&mut self, session: SessionId) -> Result<Vec<PathBuf>, MediaError> {
        // The session stays until finishing succeeds, so a failed finish
        // can still be aborted and its partial file removed.
        let output = self
            .muxers
            .get_mut(&session)
            .ok_or_else(|| no_session(session))?
            .finish()?;
        self.muxers.remove(&session);
        Ok(vec![output])
    }

    fn abort_muxer(&mut self, session: SessionId) -> Vec<PathBuf> {
        if let Some(writer) = self.muxers.remove(&session) {
            writer.abort();
        }
        Vec::new()
    }
}
