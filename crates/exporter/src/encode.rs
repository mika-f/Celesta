//! An export's encoders and muxer: the media plugins' processes it starts
//! for them, the video and audio it sends, and the packets it hands the
//! muxer in decode order (the spec's Encoding and Muxing).

use crate::control::{ExportCancellation, TrackInfo};
use crate::error::ExportError;
use crate::interleave::{Interleaver, WRITE_BATCH_BYTES};
use crate::options::OutputChoice;
use crate::resolve::{OutputPlan, ResolvedOutput, resolve_output};
use celesta_composition::Rational;
use celesta_gpu_renderer::{GpuFrame, GpuRenderer, ReadbackFormat};
use celesta_media::{
    AudioBuffer, AudioEncoderRequest, Color as MediaColor, ComponentDescription, ComponentRef,
    Encoded, MediaError, MediaProvider, MediaRegistry, MuxerRequest, PixelFormat, ProviderUse,
    SessionId, Track, TrackConfig, TrackPacket, VideoEncoderOpened, VideoEncoderRequest,
    resample_audio,
};
use celesta_plugin_host::PluginKillSwitch;
use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Sample frames of audio sent to an encoder at a time.
pub(crate) const AUDIO_CHUNK_FRAMES: usize = 1024 * 16;

/// Sample frames of the mix's end a looping export encodes ahead of its
/// start, for a muxer that trims leading audio: sixteen AAC frames. The
/// encoder settles its bit allocation over its first frames: after four, a
/// loud first hit still came out worse than with no lead-in at all, and from
/// sixteen on as cleanly as the same hit later.
pub(crate) const LOOP_LEAD_IN_FRAMES: usize = 16 * 1024;

const VIDEO_STAGE: &str = "video encoding";
const AUDIO_STAGE: &str = "audio encoding";
const MUXER_STAGE: &str = "muxing";

/// The [`LOOP_LEAD_IN_FRAMES`] sample frames that precede the mix's start
/// when it plays in a loop: its end, repeated as many times as a short mix
/// needs.
pub(crate) fn loop_lead_in(audio: &AudioBuffer) -> Vec<f32> {
    let channels = usize::from(audio.channels);
    let frames = audio.frame_count();
    if frames == 0 {
        return Vec::new();
    }
    (0..LOOP_LEAD_IN_FRAMES)
        .flat_map(|frame| {
            let source = (frames - LOOP_LEAD_IN_FRAMES % frames + frame) % frames * channels;
            audio.samples[source..source + channels].iter().copied()
        })
        .collect()
}

/// `audio` with `channels` channels: mono copied to each channel, several
/// averaged to mono; the same count is a clone.
pub(crate) fn convert_channels(audio: &AudioBuffer, channels: u16) -> AudioBuffer {
    let from = usize::from(audio.channels);
    let to = usize::from(channels);
    if from == to || from == 0 || to == 0 {
        return audio.clone();
    }
    let samples = audio
        .samples
        .chunks_exact(from)
        .flat_map(|frame| {
            let mono = frame.iter().sum::<f32>() / from as f32;
            (0..to).map(move |channel| match (from, to) {
                (_, 1) => mono,
                (1, _) => frame[0],
                _ => frame[channel.min(from - 1)],
            })
        })
        .collect();
    AudioBuffer {
        sample_rate: audio.sample_rate,
        channels,
        samples,
    }
}

/// The renderer's readback that produces `format`.
pub(crate) fn readback_of(format: PixelFormat) -> Result<ReadbackFormat, ExportError> {
    match format {
        PixelFormat::Rgba8 => Ok(ReadbackFormat::Rgba8),
        PixelFormat::Rgba16 => Ok(ReadbackFormat::Rgba16Straight),
        PixelFormat::Yuv420p => Ok(ReadbackFormat::Yuv420p),
        PixelFormat::Yuv420p10 => Ok(ReadbackFormat::Yuv420p10),
        PixelFormat::Nv12 | PixelFormat::P010 => Err(ExportError::Codec {
            stage: VIDEO_STAGE,
            source: MediaError::Codec(format!(
                "the video encoder chose {format}, which Celesta does not produce yet"
            )),
        }),
    }
}

/// `files` as they are once the staged output is published at `output`.
/// A muxer that lists none wrote the output alone.
pub(crate) fn published_files(files: Vec<PathBuf>, staged: &Path, output: &Path) -> Vec<PathBuf> {
    if files.is_empty() {
        return vec![output.to_owned()];
    }
    let staged = absolute(staged);
    files
        .into_iter()
        .map(|file| {
            if absolute(&file) == staged {
                output.to_owned()
            } else {
                file
            }
        })
        .collect()
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}

/// What an export wrote, as the encoders and muxer answered.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExportedTracks {
    pub muxer: ComponentRef,
    pub video: TrackInfo,
    pub audio: Option<TrackInfo>,
    pub alpha: bool,
    /// The files the muxer wrote, as it named them (the staged output).
    pub files: Vec<PathBuf>,
}

/// The video track: its encoder session until it is finished, and what the
/// muxer's track needs.
struct VideoTrack {
    session: Option<SessionId>,
    width: u32,
    height: u32,
    frame_rate: Rational,
    alpha: bool,
    config: Option<TrackConfig>,
    frames_sent: u64,
}

/// Whether the export has an audio track, once that is decided.
enum AudioTrack {
    Undecided,
    None,
    Encoded {
        config: TrackConfig,
        sample_rate: u32,
        channels: u16,
        delay: u64,
    },
}

/// One export's encoders and muxer, through the processes it starts for
/// their plugins ([`ProviderUse::Export`]), one per plugin, kept until the
/// session ends. The video encoder opens before the first frame; the muxer
/// opens once the video's config is known and the audio is decided, and
/// until then the video packets wait in the [`Interleaver`].
pub(crate) struct ExportSession {
    registry: Arc<MediaRegistry>,
    cancellation: ExportCancellation,
    kill: PluginKillSwitch,
    processes: HashMap<usize, Box<dyn MediaProvider>>,
    resolved: Option<ResolvedOutput>,
    video: Option<VideoTrack>,
    audio: AudioTrack,
    /// The audio encoder's session while the mix is being encoded.
    audio_session: Option<SessionId>,
    /// Sample frames of the loop pre-roll the muxer hides.
    leading_audio: u64,
    staged: Option<PathBuf>,
    interleaver: Option<Interleaver>,
    /// Packet bytes written to the muxer per request at most.
    batch_bytes: usize,
    muxer: Option<SessionId>,
}

/// The process of plugin `plugin`, started for the export when it is first
/// needed.
fn process<'a>(
    processes: &'a mut HashMap<usize, Box<dyn MediaProvider>>,
    registry: &MediaRegistry,
    kill: &PluginKillSwitch,
    plugin: usize,
) -> Result<&'a mut Box<dyn MediaProvider>, MediaError> {
    Ok(match processes.entry(plugin) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => entry.insert((registry.plugin(plugin).factory)(
            kill,
            ProviderUse::Export,
        )?),
    })
}

/// A plugin's process as a provider that starts only when it is asked to
/// describe, so a cached description starts nothing.
struct Lazy<'a> {
    processes: &'a mut HashMap<usize, Box<dyn MediaProvider>>,
    registry: &'a MediaRegistry,
    kill: &'a PluginKillSwitch,
    plugin: usize,
}

impl MediaProvider for Lazy<'_> {
    fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
        process(self.processes, self.registry, self.kill, self.plugin)?.describe()
    }
}

/// `plugin` has no component of this kind to serve the request.
fn missing(registry: &MediaRegistry, plugin: usize, kind: &str) -> MediaError {
    MediaError::Codec(format!("{} serves no {kind}", registry.plugin(plugin).id))
}

fn not_ready(what: &str) -> MediaError {
    MediaError::Codec(format!("the export has {what}"))
}

impl ExportSession {
    /// A session writing through `registry`'s plugins, whose processes
    /// `cancellation` kills.
    pub(crate) fn start(registry: Arc<MediaRegistry>, cancellation: &ExportCancellation) -> Self {
        Self {
            registry,
            cancellation: cancellation.clone(),
            kill: cancellation.plugins(),
            processes: HashMap::new(),
            resolved: None,
            video: None,
            audio: AudioTrack::Undecided,
            audio_session: None,
            leading_audio: 0,
            staged: None,
            interleaver: None,
            batch_bytes: WRITE_BATCH_BYTES,
            muxer: None,
        }
    }

    /// Chooses the components `choice` names for `output`
    /// ([`resolve_output`]), describing each plugin through the process
    /// this session keeps for it. Processes of plugins left unused stop.
    pub(crate) fn resolve(
        &mut self,
        choice: &OutputChoice,
        output: &Path,
    ) -> Result<OutputPlan, ExportError> {
        let registry = self.registry.clone();
        let kill = &self.kill;
        let processes = &mut self.processes;
        let resolved = resolve_output(choice, output, &registry, &mut |plugin| {
            registry.describe_with(
                plugin,
                &mut Lazy {
                    processes: &mut *processes,
                    registry: &registry,
                    kill,
                    plugin,
                },
            )
        })
        .map_err(|error| self.cancellation.settle(error))?;
        let used = [
            Some(resolved.muxer.plugin),
            Some(resolved.video.plugin),
            resolved.audio.as_ref().map(|audio| audio.plugin),
        ];
        self.processes
            .retain(|plugin, _| used.contains(&Some(*plugin)));
        let plan = resolved.plan();
        self.resolved = Some(resolved);
        Ok(plan)
    }

    /// Whether the resolved output has an audio track to encode.
    pub(crate) fn encodes_audio(&self) -> bool {
        self.resolved
            .as_ref()
            .is_some_and(|resolved| resolved.audio.is_some())
    }

    /// The formats `renderer` can read back, best first: the YUV formats
    /// when it converts on a hardware GPU, then straight-alpha 16-bit and
    /// 8-bit RGBA.
    pub(crate) fn offered_formats(renderer: &GpuRenderer) -> Vec<PixelFormat> {
        let mut offered = Vec::with_capacity(4);
        if renderer.supports_yuv420p_readback() && !renderer.is_software() {
            offered.extend([PixelFormat::Yuv420p, PixelFormat::Yuv420p10]);
        }
        offered.extend([PixelFormat::Rgba16, PixelFormat::Rgba8]);
        offered
    }

    fn error(&self, stage: &'static str, source: MediaError) -> ExportError {
        self.cancellation
            .settle(ExportError::Codec { stage, source })
    }

    fn resolved(&self) -> Result<&ResolvedOutput, MediaError> {
        self.resolved
            .as_ref()
            .ok_or_else(|| not_ready("no components chosen yet"))
    }

    /// The process of plugin `plugin`, started if it is not running.
    fn provider(&mut self, plugin: usize) -> Result<&mut Box<dyn MediaProvider>, MediaError> {
        process(&mut self.processes, &self.registry, &self.kill, plugin)
    }

    /// Opens the video encoder for `width`x`height` frames at `frame_rate`,
    /// offering `offered`; the muxer will write `staged`, and video packets
    /// that wait for it spill beside it.
    pub(crate) fn open_video(
        &mut self,
        width: u32,
        height: u32,
        frame_rate: Rational,
        offered: &[PixelFormat],
        staged: &Path,
    ) -> Result<VideoEncoderOpened, ExportError> {
        let opened = self
            .try_open_video(width, height, frame_rate, offered)
            .map_err(|source| self.error(VIDEO_STAGE, source))?;
        let spill = staged
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        self.interleaver = Some(Interleaver::new(frame_rate.numerator, spill));
        self.staged = Some(staged.to_owned());
        Ok(opened)
    }

    fn try_open_video(
        &mut self,
        width: u32,
        height: u32,
        frame_rate: Rational,
        offered: &[PixelFormat],
    ) -> Result<VideoEncoderOpened, MediaError> {
        let video = &self.resolved()?.video;
        let (plugin, component, settings) = (
            video.plugin,
            video.reference.component.clone(),
            video.settings.clone(),
        );
        let color = MediaColor::bt709_limited();
        let request = VideoEncoderRequest {
            component: &component,
            settings: &settings,
            width,
            height,
            frame_rate,
            timescale: frame_rate.numerator,
            offered,
            color: &color,
        };
        let registry = self.registry.clone();
        let opened = self
            .provider(plugin)?
            .video_encoder()
            .ok_or_else(|| missing(&registry, plugin, "video encoder"))?
            .open_video(&request)?;
        // Opened: an abort from here on ends the session.
        self.video = Some(VideoTrack {
            session: Some(opened.session),
            width,
            height,
            frame_rate,
            alpha: opened.alpha,
            config: None,
            frames_sent: 0,
        });
        if !offered.contains(&opened.input) {
            return Err(MediaError::InvalidSettings {
                key: "input".to_owned(),
                message: format!("the encoder chose {}, which was not offered", opened.input),
            });
        }
        if opened.alpha && opened.input != PixelFormat::Rgba16 {
            return Err(MediaError::InvalidSettings {
                key: "alpha".to_owned(),
                message: format!(
                    "keeping alpha needs rgba16 as the input, not {}",
                    opened.input
                ),
            });
        }
        Ok(opened)
    }

    /// Encodes the next frame, read back in the encoder's input format.
    pub(crate) fn encode_frame(&mut self, frame: GpuFrame) -> Result<(), ExportError> {
        self.encode_pixels(frame.into_pixels())
    }

    /// Encodes the next frame's pixels, shown at `frames_sent ×
    /// denominator`, and writes what the muxer can take.
    fn encode_pixels(&mut self, pixels: Vec<u8>) -> Result<(), ExportError> {
        let encoded = self
            .try_encode_pixels(pixels)
            .map_err(|source| self.error(VIDEO_STAGE, source))?;
        self.take_video(encoded)?;
        self.try_open_muxer()?;
        self.flush()
    }

    fn try_encode_pixels(&mut self, pixels: Vec<u8>) -> Result<Encoded, MediaError> {
        let plugin = self.resolved()?.video.plugin;
        let video = self
            .video
            .as_ref()
            .ok_or_else(|| not_ready("no open video encoder"))?;
        let session = video
            .session
            .ok_or_else(|| not_ready("finished its video"))?;
        let pts = video
            .frames_sent
            .checked_mul(u64::from(video.frame_rate.denominator))
            .and_then(|pts| i64::try_from(pts).ok())
            .ok_or_else(|| MediaError::Codec("the video is too long".to_owned()))?;
        let registry = self.registry.clone();
        let encoded = self
            .provider(plugin)?
            .video_encoder()
            .ok_or_else(|| missing(&registry, plugin, "video encoder"))?
            .encode_video(session, pts, pixels)?;
        if let Some(video) = self.video.as_mut() {
            video.frames_sent += 1;
        }
        Ok(encoded)
    }

    /// Keeps the video's first config and stages its packets.
    fn take_video(&mut self, encoded: Encoded) -> Result<(), ExportError> {
        if let Some(video) = self.video.as_mut()
            && video.config.is_none()
        {
            video.config = encoded.config;
        }
        match self.interleaver.as_mut() {
            Some(interleaver) => interleaver.push_video(encoded.packets),
            None => Err(self.error(VIDEO_STAGE, not_ready("no open video encoder"))),
        }
    }

    /// Encodes the whole `mix`, or decides there is no audio when the
    /// output has no audio encoder. The encoder answers the rate and
    /// channels it takes, and the mix is converted to them. A `looping`
    /// export to a muxer that trims leading audio first sends
    /// [`loop_lead_in`] before time 0.
    pub(crate) fn encode_audio(
        &mut self,
        mix: &AudioBuffer,
        looping: bool,
    ) -> Result<(), ExportError> {
        if !self.encodes_audio() {
            return self.no_audio();
        }
        self.try_encode_audio(mix, looping)
            .map_err(|source| self.error(AUDIO_STAGE, source))?;
        self.try_open_muxer()?;
        self.flush()
    }

    fn try_encode_audio(&mut self, mix: &AudioBuffer, looping: bool) -> Result<(), MediaError> {
        if !matches!(self.audio, AudioTrack::Undecided) {
            return Err(not_ready("its audio already"));
        }
        if self.interleaver.is_none() {
            return Err(not_ready("no open video encoder"));
        }
        let resolved = self.resolved()?;
        let audio = resolved
            .audio
            .as_ref()
            .ok_or_else(|| not_ready("no audio encoder"))?;
        let (plugin, component, settings) = (
            audio.plugin,
            audio.reference.component.clone(),
            audio.settings.clone(),
        );
        let trims = resolved.muxer.entry.trims_leading_audio;
        let request = AudioEncoderRequest {
            component: &component,
            settings: &settings,
            sample_rate: mix.sample_rate,
            channels: mix.channels,
        };
        let registry = self.registry.clone();
        let encoder = process(&mut self.processes, &self.registry, &self.kill, plugin)?
            .audio_encoder()
            .ok_or_else(|| missing(&registry, plugin, "audio encoder"))?;
        let opened = encoder.open_audio(&request)?;
        // Opened: an abort from here on ends the session.
        self.audio_session = Some(opened.session);
        if !(1..=2).contains(&opened.channels) {
            return Err(MediaError::InvalidSettings {
                key: "channels".to_owned(),
                message: format!(
                    "the encoder chose {} channels; Celesta sends 1 or 2",
                    opened.channels
                ),
            });
        }

        let mix = if opened.channels == mix.channels {
            Cow::Borrowed(mix)
        } else {
            Cow::Owned(convert_channels(mix, opened.channels))
        };
        let mix = if opened.sample_rate == mix.sample_rate {
            mix
        } else {
            Cow::Owned(resample_audio(&mix, opened.sample_rate)?)
        };
        let lead_in = if looping && trims {
            loop_lead_in(&mix)
        } else {
            Vec::new()
        };
        let channels = usize::from(opened.channels);
        let lead_frames = lead_in.len() / channels;
        let chunk = AUDIO_CHUNK_FRAMES * channels;

        let mut packets = Vec::new();
        let mut config = None;
        let mut collect = |encoded: Encoded| {
            packets.extend(encoded.packets);
            if config.is_none() {
                config = encoded.config;
            }
        };
        let mut pts = -(lead_frames as i64);
        for samples in lead_in.chunks(chunk).chain(mix.samples.chunks(chunk)) {
            collect(encoder.encode_audio(opened.session, pts, samples)?);
            pts += (samples.len() / channels) as i64;
        }
        collect(encoder.finish_audio(opened.session)?);
        self.audio_session = None;
        let config = config.ok_or_else(|| {
            MediaError::Codec("the audio encoder produced no configuration".to_owned())
        })?;

        let interleaver = self
            .interleaver
            .as_mut()
            .expect("checked before the encoder opened");
        interleaver.set_audio_timescale(opened.sample_rate);
        interleaver.push_audio(packets);
        interleaver.finish_audio();
        self.leading_audio = lead_frames as u64;
        self.audio = AudioTrack::Encoded {
            config,
            sample_rate: opened.sample_rate,
            channels: opened.channels,
            delay: opened.delay,
        };
        Ok(())
    }

    /// Decides that the export has no audio track.
    pub(crate) fn no_audio(&mut self) -> Result<(), ExportError> {
        if matches!(self.audio, AudioTrack::Undecided) {
            let Some(interleaver) = self.interleaver.as_mut() else {
                return Err(self.error(MUXER_STAGE, not_ready("no open video encoder")));
            };
            interleaver.finish_audio();
            self.audio = AudioTrack::None;
        }
        self.try_open_muxer()?;
        self.flush()
    }

    /// Opens the muxer, once the video's config is known and the audio is
    /// decided (none, or its config known).
    fn try_open_muxer(&mut self) -> Result<(), ExportError> {
        self.try_open_muxer_inner()
            .map_err(|source| self.error(MUXER_STAGE, source))
    }

    fn try_open_muxer_inner(&mut self) -> Result<(), MediaError> {
        if self.muxer.is_some() {
            return Ok(());
        }
        let Some(video) = &self.video else {
            return Ok(());
        };
        let Some(config) = &video.config else {
            return Ok(());
        };
        let audio = match &self.audio {
            AudioTrack::Undecided => return Ok(()),
            AudioTrack::None => None,
            AudioTrack::Encoded {
                config,
                sample_rate,
                channels,
                delay,
            } => Some(Track::Audio {
                config: config.clone(),
                timescale: *sample_rate,
                sample_rate: *sample_rate,
                channels: *channels,
                delay: *delay,
            }),
        };
        let mut tracks = vec![Track::Video {
            config: config.clone(),
            timescale: video.frame_rate.numerator,
            width: video.width,
            height: video.height,
            frame_rate: video.frame_rate,
            color: MediaColor::bt709_limited(),
            alpha: video.alpha,
        }];
        tracks.extend(audio);
        let muxer = &self.resolved()?.muxer;
        let (plugin, component, settings) = (
            muxer.plugin,
            muxer.reference.component.clone(),
            muxer.settings.clone(),
        );
        let staged = self
            .staged
            .clone()
            .ok_or_else(|| not_ready("no output to write"))?;
        let request = MuxerRequest {
            component: &component,
            settings: &settings,
            output: &staged,
            tracks: &tracks,
            leading_audio: self.leading_audio,
        };
        let registry = self.registry.clone();
        let session = self
            .provider(plugin)?
            .muxer()
            .ok_or_else(|| missing(&registry, plugin, "muxer"))?
            .open_muxer(&request)?;
        self.muxer = Some(session);
        Ok(())
    }

    /// Writes every packet the interleaver has ready, a batch of at most
    /// [`WRITE_BATCH_BYTES`] per request, so a staged backlog is never all
    /// in memory at once. A spill file that cannot be read back fails the
    /// export: the packets taken in that batch are lost with it.
    fn flush(&mut self) -> Result<(), ExportError> {
        let Some(session) = self.muxer else {
            return Ok(());
        };
        loop {
            let Some(interleaver) = self.interleaver.as_mut() else {
                return Ok(());
            };
            let ready = match interleaver.take_ready(self.batch_bytes) {
                Ok(ready) => ready,
                Err(error) => return Err(self.cancellation.settle(error)),
            };
            if ready.is_empty() {
                return Ok(());
            }
            self.write_packets(session, &ready)
                .map_err(|source| self.error(MUXER_STAGE, source))?;
        }
    }

    fn write_packets(
        &mut self,
        session: SessionId,
        ready: &[TrackPacket],
    ) -> Result<(), MediaError> {
        let plugin = self.resolved()?.muxer.plugin;
        let registry = self.registry.clone();
        self.provider(plugin)?
            .muxer()
            .ok_or_else(|| missing(&registry, plugin, "muxer"))?
            .write_packets(session, ready)
    }

    /// Flushes the video encoder, writes what is left, and finishes the
    /// muxer: what was written. A failure aborts what is still open.
    pub(crate) fn finish(mut self) -> Result<ExportedTracks, ExportError> {
        match self.try_finish() {
            Ok(tracks) => Ok(tracks),
            Err(error) => {
                self.abort();
                Err(error)
            }
        }
    }

    fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        let plugin = self.resolved()?.video.plugin;
        let registry = self.registry.clone();
        self.provider(plugin)?
            .video_encoder()
            .ok_or_else(|| missing(&registry, plugin, "video encoder"))?
            .finish_video(session)
    }

    fn finish_muxer(&mut self, session: SessionId) -> Result<Vec<PathBuf>, MediaError> {
        let plugin = self.resolved()?.muxer.plugin;
        let registry = self.registry.clone();
        self.provider(plugin)?
            .muxer()
            .ok_or_else(|| missing(&registry, plugin, "muxer"))?
            .finish_muxer(session)
    }

    fn try_finish(&mut self) -> Result<ExportedTracks, ExportError> {
        if let Some(session) = self.video.as_mut().and_then(|video| video.session.take()) {
            let encoded = self
                .finish_video(session)
                .map_err(|source| self.error(VIDEO_STAGE, source))?;
            self.take_video(encoded)?;
        }
        if let Some(interleaver) = self.interleaver.as_mut() {
            interleaver.finish_video();
        }
        let Some((codec, alpha)) = self
            .video
            .as_ref()
            .and_then(|video| Some((video.config.as_ref()?.codec.clone(), video.alpha)))
        else {
            return Err(self.error(
                VIDEO_STAGE,
                MediaError::Codec("the encoder produced no configuration".to_owned()),
            ));
        };
        if matches!(self.audio, AudioTrack::Undecided) {
            return Err(self.error(MUXER_STAGE, not_ready("no audio decided")));
        }
        self.try_open_muxer()?;
        self.flush()?;
        let Some(session) = self.muxer else {
            return Err(self.error(MUXER_STAGE, not_ready("no open muxer")));
        };
        let files = self
            .finish_muxer(session)
            .map_err(|source| self.error(MUXER_STAGE, source))?;
        self.muxer = None;
        let resolved = self
            .resolved()
            .map_err(|source| self.error(MUXER_STAGE, source))?;
        let audio = match (&self.audio, &resolved.audio) {
            (AudioTrack::Encoded { config, .. }, Some(encoder)) => Some(TrackInfo {
                component: encoder.reference.clone(),
                codec: config.codec.clone(),
            }),
            _ => None,
        };
        Ok(ExportedTracks {
            muxer: resolved.muxer.reference.clone(),
            video: TrackInfo {
                component: resolved.video.reference.clone(),
                codec,
            },
            audio,
            alpha,
            files,
        })
    }

    /// Ends every session still open (`abortEncoder`, `abortMuxer`) and
    /// stops the processes. A process that does not answer within the
    /// abort's deadline is killed. Nothing is started to abort.
    pub(crate) fn abort(mut self) {
        let Some(resolved) = self.resolved.take() else {
            return;
        };
        if let Some(session) = self.video.as_mut().and_then(|video| video.session.take())
            && let Some(encoder) = self
                .processes
                .get_mut(&resolved.video.plugin)
                .and_then(|provider| provider.video_encoder())
        {
            encoder.abort_video(session);
        }
        if let (Some(session), Some(audio)) = (self.audio_session.take(), &resolved.audio)
            && let Some(encoder) = self
                .processes
                .get_mut(&audio.plugin)
                .and_then(|provider| provider.audio_encoder())
        {
            encoder.abort_audio(session);
        }
        if let Some(session) = self.muxer.take()
            && let Some(muxer) = self
                .processes
                .get_mut(&resolved.muxer.plugin)
                .and_then(|provider| provider.muxer())
        {
            // The staged output goes with its file, whatever the muxer left.
            muxer.abort_muxer(session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interleave::{AUDIO_TRACK, VIDEO_TRACK};
    use celesta_media::{
        AudioEncoder, AudioEncoderOpened, ComponentKind, Muxer, Packet, RegisteredPlugin,
        VideoEncoder,
    };
    use celesta_plugin_host::{
        MediaAudioEncoderEntry, MediaManifest, MediaMuxerEntry, MediaOrigin, MediaVideoEncoderEntry,
    };
    use std::sync::Mutex;

    /// What the fake plugin was asked, in order.
    #[derive(Clone, Debug, PartialEq)]
    enum Asked {
        OpenVideo,
        EncodeVideo {
            pts: i64,
        },
        FinishVideo,
        AbortVideo(SessionId),
        OpenAudio {
            sample_rate: u32,
            channels: u16,
        },
        EncodeAudio {
            pts: i64,
            samples: Vec<f32>,
        },
        FinishAudio,
        AbortAudio(SessionId),
        OpenMuxer {
            tracks: Vec<Track>,
            leading_audio: u64,
        },
        WritePackets(Vec<TrackPacket>),
        FinishMuxer,
        AbortMuxer(SessionId),
    }

    /// How the fake answers.
    #[derive(Clone, Copy, Default)]
    struct Answers {
        /// The video encoder's input, else the first offered.
        input: Option<PixelFormat>,
        alpha: bool,
        sample_rate: Option<u32>,
        channels: Option<u16>,
        trims: bool,
    }

    const DELAY: u64 = 1024;

    struct Fake {
        answers: Answers,
        log: Arc<Mutex<Vec<Asked>>>,
        last: u64,
        video_config_due: bool,
        audio_channels: usize,
        audio_config_due: bool,
    }

    impl Fake {
        fn ask(&self, asked: Asked) {
            self.log.lock().unwrap().push(asked);
        }

        fn session(&mut self) -> SessionId {
            self.last += 1;
            SessionId(self.last)
        }
    }

    fn config(codec: &str) -> TrackConfig {
        TrackConfig {
            codec: codec.to_owned(),
            description: None,
        }
    }

    impl MediaProvider for Fake {
        fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
            let available = |kind, id: &str| ComponentDescription {
                available: true,
                reason: None,
                ..ComponentDescription::unavailable(kind, id, "")
            };
            Ok(vec![
                available(ComponentKind::VideoEncoder, "video"),
                available(ComponentKind::AudioEncoder, "pcm"),
                available(ComponentKind::Muxer, "file"),
            ])
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

    impl VideoEncoder for Fake {
        fn open_video(
            &mut self,
            request: &VideoEncoderRequest<'_>,
        ) -> Result<VideoEncoderOpened, MediaError> {
            self.ask(Asked::OpenVideo);
            Ok(VideoEncoderOpened {
                session: self.session(),
                input: self.answers.input.unwrap_or(request.offered[0]),
                alpha: self.answers.alpha,
            })
        }

        fn encode_video(
            &mut self,
            _: SessionId,
            pts: i64,
            frame: Vec<u8>,
        ) -> Result<Encoded, MediaError> {
            self.ask(Asked::EncodeVideo { pts });
            Ok(Encoded {
                packets: vec![Packet {
                    pts,
                    dts: pts,
                    duration: 1,
                    key: true,
                    bytes: frame,
                    additional: Vec::new(),
                }],
                config: std::mem::take(&mut self.video_config_due).then(|| config("fake")),
            })
        }

        fn finish_video(&mut self, _: SessionId) -> Result<Encoded, MediaError> {
            self.ask(Asked::FinishVideo);
            Ok(Encoded::default())
        }

        fn abort_video(&mut self, session: SessionId) {
            self.ask(Asked::AbortVideo(session));
        }
    }

    impl AudioEncoder for Fake {
        fn open_audio(
            &mut self,
            request: &AudioEncoderRequest<'_>,
        ) -> Result<AudioEncoderOpened, MediaError> {
            self.ask(Asked::OpenAudio {
                sample_rate: request.sample_rate,
                channels: request.channels,
            });
            let channels = self.answers.channels.unwrap_or(request.channels);
            self.audio_channels = usize::from(channels);
            Ok(AudioEncoderOpened {
                session: self.session(),
                sample_rate: self.answers.sample_rate.unwrap_or(request.sample_rate),
                channels,
                delay: DELAY,
            })
        }

        fn encode_audio(
            &mut self,
            _: SessionId,
            pts: i64,
            samples: &[f32],
        ) -> Result<Encoded, MediaError> {
            self.ask(Asked::EncodeAudio {
                pts,
                samples: samples.to_vec(),
            });
            let at = pts - DELAY as i64;
            Ok(Encoded {
                packets: vec![Packet {
                    pts: at,
                    dts: at,
                    duration: (samples.len() / self.audio_channels) as i64,
                    key: true,
                    bytes: Vec::new(),
                    additional: Vec::new(),
                }],
                config: std::mem::take(&mut self.audio_config_due).then(|| config("lpcm.f32le")),
            })
        }

        fn finish_audio(&mut self, _: SessionId) -> Result<Encoded, MediaError> {
            self.ask(Asked::FinishAudio);
            Ok(Encoded {
                packets: Vec::new(),
                config: std::mem::take(&mut self.audio_config_due).then(|| config("lpcm.f32le")),
            })
        }

        fn abort_audio(&mut self, session: SessionId) {
            self.ask(Asked::AbortAudio(session));
        }
    }

    impl Muxer for Fake {
        fn open_muxer(&mut self, request: &MuxerRequest<'_>) -> Result<SessionId, MediaError> {
            self.ask(Asked::OpenMuxer {
                tracks: request.tracks.to_vec(),
                leading_audio: request.leading_audio,
            });
            Ok(self.session())
        }

        fn write_packets(
            &mut self,
            _: SessionId,
            packets: &[TrackPacket],
        ) -> Result<(), MediaError> {
            self.ask(Asked::WritePackets(packets.to_vec()));
            Ok(())
        }

        fn finish_muxer(&mut self, _: SessionId) -> Result<Vec<PathBuf>, MediaError> {
            self.ask(Asked::FinishMuxer);
            Ok(vec![PathBuf::from("staged.fake")])
        }

        fn abort_muxer(&mut self, session: SessionId) -> Vec<PathBuf> {
            self.ask(Asked::AbortMuxer(session));
            Vec::new()
        }
    }

    /// A session through the fake plugin, resolved for `out.fake`, and what
    /// the plugin is asked.
    fn fake_session(answers: Answers) -> (ExportSession, Arc<Mutex<Vec<Asked>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let factory_log = log.clone();
        let plugin = RegisteredPlugin {
            id: "fake".to_owned(),
            version: "1.0.0".to_owned(),
            origin: MediaOrigin::Bundled,
            directory: PathBuf::new(),
            executable: None,
            media: MediaManifest {
                decoders: Vec::new(),
                video_encoders: vec![MediaVideoEncoderEntry {
                    id: "video".to_owned(),
                    name: "Video".to_owned(),
                    codecs: vec!["fake".to_owned()],
                    input_formats: None,
                }],
                audio_encoders: vec![MediaAudioEncoderEntry {
                    id: "pcm".to_owned(),
                    name: "PCM".to_owned(),
                    codecs: vec!["lpcm".to_owned()],
                }],
                muxers: vec![MediaMuxerEntry {
                    id: "file".to_owned(),
                    name: "File".to_owned(),
                    extensions: vec!["fake".to_owned()],
                    video: vec!["fake".to_owned()],
                    audio: Some(vec!["lpcm".to_owned()]),
                    default_video: Some("fake".to_owned()),
                    default_audio: Some("lpcm".to_owned()),
                    trims_leading_audio: answers.trims,
                    sequence: false,
                }],
            },
            factory: Arc::new(move |_, _| {
                Ok(Box::new(Fake {
                    answers,
                    log: factory_log.clone(),
                    last: 0,
                    video_config_due: true,
                    audio_channels: 1,
                    audio_config_due: true,
                }) as Box<dyn MediaProvider>)
            }),
        };
        let registry = Arc::new(MediaRegistry::new(vec![plugin]));
        let mut session = ExportSession::start(registry, &ExportCancellation::default());
        session
            .resolve(&OutputChoice::default(), Path::new("out.fake"))
            .unwrap();
        (session, log)
    }

    const RATE: Rational = Rational::new(30, 1);

    fn open(session: &mut ExportSession) -> VideoEncoderOpened {
        session
            .open_video(
                2,
                2,
                RATE,
                &[PixelFormat::Rgba16, PixelFormat::Rgba8],
                Path::new("staged.fake"),
            )
            .unwrap()
    }

    fn frames(session: &mut ExportSession, count: u8) {
        for index in 0..count {
            session.encode_pixels(vec![index; 16]).unwrap();
        }
    }

    /// `frames` of stereo at `sample_rate`, each frame `(n, -n)`.
    fn mix(sample_rate: u32, frames: usize) -> AudioBuffer {
        AudioBuffer {
            sample_rate,
            channels: 2,
            samples: (0..frames)
                .flat_map(|frame| [frame as f32, -(frame as f32) / 2.0])
                .collect(),
        }
    }

    fn log_of(log: &Mutex<Vec<Asked>>) -> Vec<Asked> {
        log.lock().unwrap().clone()
    }

    fn opened_muxer(log: &Mutex<Vec<Asked>>) -> Option<(Vec<Track>, u64)> {
        log_of(log).into_iter().find_map(|asked| match asked {
            Asked::OpenMuxer {
                tracks,
                leading_audio,
            } => Some((tracks, leading_audio)),
            _ => None,
        })
    }

    fn sent_audio(log: &Mutex<Vec<Asked>>) -> Vec<(i64, Vec<f32>)> {
        log_of(log)
            .into_iter()
            .filter_map(|asked| match asked {
                Asked::EncodeAudio { pts, samples } => Some((pts, samples)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_muxer_opens_once_both_configs_are_known() {
        let (mut session, log) = fake_session(Answers::default());
        open(&mut session);
        frames(&mut session, 3);
        assert_eq!(opened_muxer(&log), None, "the audio is not known yet");
        session.encode_audio(&mix(48_000, 100), false).unwrap();

        let asked = log_of(&log);
        let opened = asked
            .iter()
            .position(|asked| matches!(asked, Asked::OpenMuxer { .. }))
            .expect("the muxer opened");
        let finished = asked
            .iter()
            .position(|asked| *asked == Asked::FinishAudio)
            .unwrap();
        assert!(opened > finished, "{asked:?}");
        let (tracks, leading_audio) = opened_muxer(&log).unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(leading_audio, 0);
        assert!(
            matches!(
                tracks[1],
                Track::Audio {
                    delay: DELAY,
                    sample_rate: 48_000,
                    channels: 2,
                    ..
                }
            ),
            "{tracks:?}"
        );
    }

    #[test]
    fn frames_before_the_mix_are_staged_then_muxed_in_order() {
        let (mut session, log) = fake_session(Answers::default());
        open(&mut session);
        frames(&mut session, 5);
        session.encode_audio(&mix(48_000, 10_000), false).unwrap();
        let tracks = session.finish().unwrap();
        assert_eq!(tracks.video.codec, "fake");
        assert_eq!(tracks.audio.as_ref().unwrap().codec, "lpcm.f32le");

        let written: Vec<TrackPacket> = log_of(&log)
            .into_iter()
            .filter_map(|asked| match asked {
                Asked::WritePackets(packets) => Some(packets),
                _ => None,
            })
            .flatten()
            .collect();
        let seconds = |packet: &TrackPacket| {
            let timescale = if packet.track == VIDEO_TRACK {
                f64::from(RATE.numerator)
            } else {
                48_000.0
            };
            packet.packet.dts as f64 / timescale
        };
        assert!(
            written
                .windows(2)
                .all(|pair| seconds(&pair[0]) <= seconds(&pair[1])),
            "{:?}",
            written.iter().map(seconds).collect::<Vec<_>>()
        );
        let video: Vec<Vec<u8>> = written
            .iter()
            .filter(|packet| packet.track == VIDEO_TRACK)
            .map(|packet| packet.packet.bytes.clone())
            .collect();
        assert_eq!(
            video,
            (0..5).map(|index| vec![index; 16]).collect::<Vec<_>>()
        );
        assert!(written.iter().any(|packet| packet.track == AUDIO_TRACK));
        assert_eq!(log_of(&log).last(), Some(&Asked::FinishMuxer));
    }

    #[test]
    fn no_audio_opens_a_video_only_muxer() {
        let (mut session, log) = fake_session(Answers::default());
        open(&mut session);
        frames(&mut session, 1);
        session.no_audio().unwrap();
        let (tracks, _) = opened_muxer(&log).unwrap();
        assert!(matches!(tracks[..], [Track::Video { .. }]), "{tracks:?}");
        let tracks = session.finish().unwrap();
        assert_eq!(tracks.audio, None);
        assert_eq!(tracks.files, [PathBuf::from("staged.fake")]);
    }

    #[test]
    fn a_looping_export_sends_the_pre_roll_only_to_trimming_muxers() {
        for (trims, leading) in [(true, LOOP_LEAD_IN_FRAMES as i64), (false, 0)] {
            let (mut session, log) = fake_session(Answers {
                trims,
                ..Answers::default()
            });
            open(&mut session);
            frames(&mut session, 1);
            session.encode_audio(&mix(48_000, 1_000), true).unwrap();
            let (_, leading_audio) = opened_muxer(&log).unwrap();
            assert_eq!(leading_audio, leading as u64, "trims: {trims}");
            let sent = sent_audio(&log);
            assert_eq!(sent[0].0, -leading, "trims: {trims}");
            let frames: usize = sent.iter().map(|(_, samples)| samples.len() / 2).sum();
            assert_eq!(frames, 1_000 + leading as usize, "trims: {trims}");
        }
    }

    #[test]
    fn a_refused_rate_is_resampled() {
        let (mut session, log) = fake_session(Answers {
            sample_rate: Some(48_000),
            ..Answers::default()
        });
        open(&mut session);
        frames(&mut session, 1);
        let count = 44_100;
        session.encode_audio(&mix(44_100, count), false).unwrap();
        assert!(log_of(&log).contains(&Asked::OpenAudio {
            sample_rate: 44_100,
            channels: 2
        }));
        let received: usize = sent_audio(&log)
            .iter()
            .map(|(_, samples)| samples.len() / 2)
            .sum();
        let expected = (count as f64 * 48_000.0 / 44_100.0).round() as usize;
        assert!(received.abs_diff(expected) <= 1, "{received} vs {expected}");
        let (tracks, _) = opened_muxer(&log).unwrap();
        assert!(matches!(
            tracks[1],
            Track::Audio {
                sample_rate: 48_000,
                timescale: 48_000,
                ..
            }
        ));
    }

    #[test]
    fn a_refused_channel_count_is_converted() {
        // Stereo to an encoder that takes mono: each frame averaged.
        let (mut session, log) = fake_session(Answers {
            channels: Some(1),
            ..Answers::default()
        });
        open(&mut session);
        let stereo = mix(48_000, 100);
        session.encode_audio(&stereo, false).unwrap();
        let received: Vec<f32> = sent_audio(&log).into_iter().flat_map(|(_, s)| s).collect();
        let averaged: Vec<f32> = stereo
            .samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[left, right]| (left + right) / 2.0)
            .collect();
        assert_eq!(received, averaged);

        // Mono to one that takes stereo: each sample twice.
        let (mut session, log) = fake_session(Answers {
            channels: Some(2),
            ..Answers::default()
        });
        open(&mut session);
        let mono = AudioBuffer {
            sample_rate: 48_000,
            channels: 1,
            samples: (0..100).map(|sample| sample as f32).collect(),
        };
        session.encode_audio(&mono, false).unwrap();
        let received: Vec<f32> = sent_audio(&log).into_iter().flat_map(|(_, s)| s).collect();
        let doubled: Vec<f32> = mono.samples.iter().flat_map(|&s| [s, s]).collect();
        assert_eq!(received, doubled);
    }

    #[test]
    fn more_than_two_channels_are_refused_and_aborted() {
        let (mut session, log) = fake_session(Answers {
            channels: Some(3),
            ..Answers::default()
        });
        open(&mut session);
        let result = session.encode_audio(&mix(48_000, 100), false);
        assert!(
            matches!(
                &result,
                Err(ExportError::Codec {
                    stage: AUDIO_STAGE,
                    source: MediaError::InvalidSettings { key, .. },
                }) if key == "channels"
            ),
            "{result:?}"
        );
        assert!(sent_audio(&log).is_empty());
        session.abort();
        assert!(
            log_of(&log)
                .iter()
                .any(|asked| matches!(asked, Asked::AbortAudio(_))),
            "{:?}",
            log_of(&log)
        );
    }

    #[test]
    fn alpha_needs_rgba16_input() {
        let (mut session, _) = fake_session(Answers {
            input: Some(PixelFormat::Yuv420p),
            alpha: true,
            ..Answers::default()
        });
        let result = session.open_video(
            2,
            2,
            RATE,
            &[PixelFormat::Yuv420p, PixelFormat::Rgba16],
            Path::new("staged.fake"),
        );
        assert!(
            matches!(
                &result,
                Err(ExportError::Codec {
                    stage: VIDEO_STAGE,
                    source: MediaError::InvalidSettings { key, .. },
                }) if key == "alpha"
            ),
            "{result:?}"
        );
    }

    #[test]
    fn abort_ends_every_open_session() {
        // Two frames, no audio yet: the muxer never opened.
        let (mut session, log) = fake_session(Answers::default());
        let video = open(&mut session).session;
        frames(&mut session, 2);
        session.abort();
        let asked = log_of(&log);
        assert!(asked.contains(&Asked::AbortVideo(video)), "{asked:?}");
        assert!(
            !asked
                .iter()
                .any(|asked| matches!(asked, Asked::AbortMuxer(_))),
            "{asked:?}"
        );

        // With the audio encoded the muxer is open; the audio encoder
        // already finished.
        let (mut session, log) = fake_session(Answers::default());
        let video = open(&mut session).session;
        frames(&mut session, 2);
        session.encode_audio(&mix(48_000, 100), false).unwrap();
        session.abort();
        let asked = log_of(&log);
        assert!(asked.contains(&Asked::AbortVideo(video)), "{asked:?}");
        assert!(
            asked
                .iter()
                .any(|asked| matches!(asked, Asked::AbortMuxer(_))),
            "{asked:?}"
        );
        assert!(
            !asked
                .iter()
                .any(|asked| matches!(asked, Asked::AbortAudio(_))),
            "{asked:?}"
        );
    }

    #[test]
    fn channels_convert_both_ways() {
        let stereo = AudioBuffer {
            sample_rate: 8_000,
            channels: 2,
            samples: vec![1.0, 3.0, -2.0, 0.0],
        };
        assert_eq!(convert_channels(&stereo, 1).samples, [2.0, -1.0]);
        assert_eq!(convert_channels(&stereo, 2), stereo);
        let mono = convert_channels(&stereo, 1);
        assert_eq!(convert_channels(&mono, 2).samples, [2.0, 2.0, -1.0, -1.0]);
    }

    #[test]
    fn staged_files_are_named_by_their_published_path() {
        let files = published_files(
            vec![
                PathBuf::from("/out/.celesta-x.mp4"),
                PathBuf::from("/out/other"),
            ],
            Path::new("/out/.celesta-x.mp4"),
            Path::new("/out/film.mp4"),
        );
        assert_eq!(
            files,
            [PathBuf::from("/out/film.mp4"), PathBuf::from("/out/other")]
        );
        assert_eq!(
            published_files(
                Vec::new(),
                Path::new("/out/.celesta-x.mp4"),
                Path::new("/out/film.mp4"),
            ),
            [PathBuf::from("/out/film.mp4")]
        );
    }

    #[test]
    fn a_staged_backlog_is_written_in_bounded_batches() {
        let directory = tempfile::tempdir().unwrap();
        let (mut session, log) = fake_session(Answers::default());
        session
            .open_video(
                2,
                2,
                RATE,
                &[PixelFormat::Rgba8],
                &directory.path().join("staged.fake"),
            )
            .unwrap();
        // Two frames in memory, the rest spilled, written 40 bytes at most at
        // a time once the audio is decided.
        session.interleaver = Some(session.interleaver.take().unwrap().with_spill_limit(32));
        session.batch_bytes = 40;
        frames(&mut session, 10);
        assert_eq!(files_in(directory.path()).len(), 1, "a spill file");
        session.no_audio().unwrap();
        let tracks = session.finish().unwrap();
        assert_eq!(tracks.audio, None);

        let batches: Vec<Vec<TrackPacket>> = log_of(&log)
            .into_iter()
            .filter_map(|asked| match asked {
                Asked::WritePackets(packets) => Some(packets),
                _ => None,
            })
            .collect();
        assert!(batches.len() >= 4, "{} batches", batches.len());
        assert!(
            batches
                .iter()
                .all(|batch| { batch.iter().map(|p| p.packet.bytes.len()).sum::<usize>() <= 40 })
        );
        let video: Vec<Vec<u8>> = batches
            .into_iter()
            .flatten()
            .map(|packet| packet.packet.bytes)
            .collect();
        assert_eq!(
            video,
            (0..10).map(|index| vec![index; 16]).collect::<Vec<_>>()
        );
        assert!(
            files_in(directory.path()).is_empty(),
            "the spill file is gone"
        );
    }

    fn files_in(directory: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect()
    }
}
