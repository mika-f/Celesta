//! A lossless codec for Celesta's tests. [`TestCodec`] stores exactly the
//! frames an encoder is given (in their [`PixelFormat`], with no conversion)
//! and the audio muxed in, compressed with LZ4, and reads them back; it also
//! reads 16-bit PCM WAV, so tests keep the checked-in voices. It honours the
//! [`CodecProvider`] contract the exporter relies on: containers,
//! `check_encoder`, the lead-in, and errors for what it does not support.
//!
//! Tests use it in their own process ([`install`]), or, for a child process
//! such as `celesta-exporter`, as the [`PROGRAM`] in a temporary plugin
//! folder ([`TestCodecPlugin`]). `src/format.rs` describes the file it
//! writes; [`read`] gives a file's contents to compare byte for byte.

mod format;
mod plugin;
mod wav;

pub use celesta_plugin_host::PLUGIN_PATH_ENV;
pub use format::{StoredAudio, StoredVideo, TestMedia, read, write_audio, write_rgba_video};
pub use plugin::{TestCodecPlugin, profile_dir};

use celesta_composition::Time;
use celesta_media::{
    AudioBuffer, AudioStream, CodecFactory, CodecProvider, Container, EncoderId, EncoderSettings,
    MediaError, MediaProbe, MuxRequest, PixelFormat, VideoFrame, VideoRequest, VideoStream,
    timestamps_match,
};
use format::{AudioHeader, Kind, Scanned, VideoHeader, VideoWriter, invalid};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// The id the codec's plugin manifest and `hello` give.
pub const PLUGIN_ID: &str = "celesta-test-codec";

/// The name of the program that serves the codec.
pub const PROGRAM: &str = "celesta-codec-test";

/// The probe's codec name for a test codec file's video.
const VIDEO_CODEC: &str = "celesta-test";

/// The probe's codec name for a test codec file's audio.
const AUDIO_CODEC: &str = "pcm_f32le";

/// The largest H.264 constant rate factor, which the FFmpeg codec refuses
/// to go past.
const MAX_CRF: u8 = 51;

/// The lossless test codec.
#[derive(Default)]
pub struct TestCodec {
    encoders: HashMap<EncoderId, VideoWriter>,
    next: u64,
}

impl TestCodec {
    pub fn new() -> Self {
        Self::default()
    }

    /// A [`CodecFactory`] making a [`TestCodec`] in this process.
    pub fn factory() -> CodecFactory {
        Arc::new(|| Ok(Box::new(Self::new()) as Box<dyn CodecProvider>))
    }
}

/// Makes [`TestCodec`], in this process, the codec every
/// `celesta_media::MediaBackend` uses.
pub fn install() {
    celesta_media::set_codec(Some(TestCodec::factory()));
}

fn no_encoder(encoder: EncoderId) -> MediaError {
    MediaError::Codec(format!("no open encoder {}", encoder.0))
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
        PixelFormat::Yuv420p | PixelFormat::Yuv420p10 => Err(invalid(
            path,
            format_args!(
                "is {format:?} video, which the test codec does not decode; \
                 compare its frames through celesta_codec_test::read"
            ),
        )),
    }
}

impl CodecProvider for TestCodec {
    fn probe(&mut self, path: &Path) -> Result<MediaProbe, MediaError> {
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

    fn decode_video(&mut self, request: &VideoRequest<'_>) -> Result<VideoFrame, MediaError> {
        let path = request.path;
        let Source::Test(mut scanned) = Source::open(path)? else {
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
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        if sample_rate == 0 || channels == 0 {
            return Err(MediaError::InvalidAudioFormat {
                sample_rate,
                channels,
            });
        }
        let samples = match Source::open(path)?.played_audio()? {
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

    fn check_encoder(&mut self, _container: Container) -> Result<(), MediaError> {
        Ok(())
    }

    fn open_encoder(&mut self, settings: &EncoderSettings) -> Result<EncoderId, MediaError> {
        if settings.container == Container::Mp4 {
            if settings.crf > MAX_CRF {
                return Err(MediaError::Codec(format!(
                    "H.264 CRF must be between 0 and {MAX_CRF}, got {}",
                    settings.crf
                )));
            }
            if !matches!(settings.bit_depth, 8 | 10) {
                return Err(MediaError::Codec(format!(
                    "H.264 is written at a bit depth of 8 or 10, not {}",
                    settings.bit_depth
                )));
            }
        }
        let writer = VideoWriter::create(
            &settings.output,
            Some(settings.container),
            VideoHeader {
                width: settings.width,
                height: settings.height,
                frame_rate: settings.frame_rate,
                format: settings.input,
            },
        )?;
        self.next += 1;
        let id = EncoderId(self.next);
        self.encoders.insert(id, writer);
        Ok(id)
    }

    fn encode_frame(&mut self, encoder: EncoderId, pixels: Vec<u8>) -> Result<(), MediaError> {
        self.encoders
            .get_mut(&encoder)
            .ok_or_else(|| no_encoder(encoder))?
            .push(&pixels)
    }

    fn finish_encoder(&mut self, encoder: EncoderId) -> Result<(), MediaError> {
        self.encoders
            .remove(&encoder)
            .ok_or_else(|| no_encoder(encoder))?
            .finish()
    }

    fn abort_encoder(&mut self, encoder: EncoderId) {
        // Dropped at once, so the caller can remove what it wrote.
        self.encoders.remove(&encoder);
    }

    fn mux_audio(&mut self, request: &MuxRequest) -> Result<(), MediaError> {
        format::mux(
            &request.video,
            &request.audio,
            &request.output,
            request.container,
            AudioHeader {
                sample_rate: request.sample_rate,
                channels: request.channels,
                lead_in_frames: request.lead_in_frames,
            },
        )
    }
}
