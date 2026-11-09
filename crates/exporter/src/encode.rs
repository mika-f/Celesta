//! An export's encoders and muxer: the media plugins' processes it starts
//! for them, the video and audio it sends, and the packets it hands the
//! muxer in decode order (the spec's Encoding and Muxing).

use crate::control::{ExportCancellation, TrackInfo};
use crate::error::ExportError;
use crate::interleave::{Interleaver, WRITE_REQUEST_BYTES, WRITE_REQUEST_PACKETS, packet_bytes};
use crate::options::OutputChoice;
use crate::resolve::{OutputPlan, ResolvedOutput, resolve_output};
use celesta_composition::Rational;
use celesta_gpu_renderer::{GpuFrame, GpuRenderer, ReadbackFormat};
use celesta_media::{
    Answered, AudioBuffer, AudioEncoderOpened, AudioEncoderRequest, Color as MediaColor,
    ComponentDescription, ComponentRef, Encoded, MediaError, MediaProvider, MediaRegistry,
    MuxerRequest, Packet, PixelFormat, ProviderUse, RingLayout, SessionId, Track, TrackConfig,
    TrackPacket, VideoEncoderOpened, VideoEncoderRequest, resample_audio, ring_layout,
};
use celesta_plugin_host::{MediaManifest, PluginKillSwitch};
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Sample frames of audio sent to an encoder at a time.
pub(crate) const AUDIO_CHUNK_FRAMES: usize = 1024 * 16;

/// Ready packets that make a `writePackets` request: the muxer is asked
/// once per this many (or [`WRITE_BATCH_BYTES`]), and for the rest when the
/// export finishes, so a plugin's window carries mostly frames.
pub(crate) const WRITE_BATCH_PACKETS: usize = 8;

/// Ready packet bytes that make a `writePackets` request, however few the
/// packets.
pub(crate) const WRITE_BATCH_BYTES: usize = 4 << 20;

/// The environment variable that, set to `1`, has an export print how long
/// its render and its mix took on stderr ([`timing_line`]).
pub(crate) const TIMING_VARIABLE: &str = "CELESTA_EXPORT_TIMING";

/// Sample frames of the mix's end a looping export encodes ahead of its
/// start, for a muxer that trims leading audio: sixteen AAC frames. The
/// encoder settles its bit allocation over its first frames: after four, a
/// loud first hit still came out worse than with no lead-in at all, and from
/// sixteen on as cleanly as the same hit later.
pub(crate) const LOOP_LEAD_IN_FRAMES: usize = 16 * 1024;

/// The transport a video encoder describes to take its frames through a
/// ring of shared memory.
const SHARED_MEMORY_TRANSPORT: &str = "sharedMemory";

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
    /// What the session measured, once every packet was staged and written.
    pub timing: SessionTiming,
}

/// How the video encoder's process takes the frames, and how many go at a
/// time: through the ring of shared memory it accepted and was declared at
/// the open, in each request's payload through the pipe, or in Celesta's
/// own process, one at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FramePath {
    Ring(usize),
    Pipe(usize),
    #[default]
    InProcess,
}

/// What a session measured of its staged video: the interleaver's, over
/// the whole export (finish's packets included), and how its frames went.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SessionTiming {
    pub peak_staged_bytes: usize,
    pub spilled: bool,
    pub frame_path: FramePath,
}

/// How long an export's render and mix took, and how much video waited
/// for the mix ([`TIMING_VARIABLE`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExportTiming {
    /// The render loop, from its first frame to the last one it sent to
    /// the encoder (answers still in flight are received by `finish`).
    pub render: Duration,
    /// The frames the render sent to the encoder.
    pub frames: u64,
    /// The mix, from its start to its samples; `None` without one.
    pub mix: Option<Duration>,
    /// The frames sent when the render took the mix.
    pub mix_done_at_frame: Option<u64>,
    pub peak_staged_bytes: usize,
    pub spilled: bool,
    pub frame_path: FramePath,
}

impl ExportTiming {
    /// `self` with what `session` measured.
    pub(crate) fn with(self, session: SessionTiming) -> Self {
        Self {
            peak_staged_bytes: session.peak_staged_bytes,
            spilled: session.spilled,
            frame_path: session.frame_path,
            ..self
        }
    }
}

/// `timing` as the one line the export prints on stderr; the staged peak
/// is in decimal megabytes (10^6 bytes).
pub(crate) fn timing_line(timing: &ExportTiming) -> String {
    let mix = match (timing.mix, timing.mix_done_at_frame) {
        (None, _) => "mix none".to_owned(),
        (Some(mix), Some(frame)) if frame == timing.frames => {
            format!("mix {:.2} s, done after the render", mix.as_secs_f64())
        }
        (Some(mix), Some(frame)) => {
            format!("mix {:.2} s, done at frame {frame}", mix.as_secs_f64())
        }
        (Some(mix), None) => format!("mix {:.2} s", mix.as_secs_f64()),
    };
    let (frames, depth) = match timing.frame_path {
        FramePath::Ring(depth) => ("ring", depth),
        FramePath::Pipe(depth) => ("pipe", depth),
        FramePath::InProcess => ("in process", 1),
    };
    format!(
        "celesta: timing: render {:.2} s for {} frames; {mix}; staged peak {:.1} MB; spilled {}; \
         frames {frames} {depth} deep",
        timing.render.as_secs_f64(),
        timing.frames,
        timing.peak_staged_bytes as f64 / 1e6,
        if timing.spilled { "yes" } else { "no" },
    )
}

/// Prints [`timing_line`] on stderr when [`TIMING_VARIABLE`] is `1`.
pub(crate) fn report_timing(timing: &ExportTiming) {
    if std::env::var_os(TIMING_VARIABLE).is_some_and(|value| value == "1") {
        eprintln!("{}", timing_line(timing));
    }
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

/// A plugin's process for the export, whose encode and write requests go
/// several at a time through its [`Pipeline`](celesta_media::Pipeline). A
/// provider without one (in this process) answers each as it is sent, and
/// its answers wait in `ready` until they are received, so the session
/// treats both alike.
struct Plugin {
    provider: Box<dyn MediaProvider>,
    ready: VecDeque<Answered>,
    /// The stage of each request in flight, oldest first, which names the
    /// stage of its answer's error.
    stages: VecDeque<&'static str>,
}

impl Plugin {
    fn new(provider: Box<dyn MediaProvider>) -> Self {
        Self {
            provider,
            ready: VecDeque::new(),
            stages: VecDeque::new(),
        }
    }

    /// How many requests may be in flight: the pipeline's depth, or one.
    fn depth(&mut self) -> usize {
        self.provider
            .pipeline()
            .map_or(1, |pipeline| pipeline.depth())
    }

    /// How many requests are sent and not yet received.
    fn in_flight(&mut self) -> usize {
        match self.provider.pipeline() {
            Some(pipeline) => pipeline.in_flight(),
            None => self.ready.len(),
        }
    }

    fn send_video_frame(
        &mut self,
        session: SessionId,
        pts: i64,
        frame: Vec<u8>,
    ) -> Result<(), MediaError> {
        match self.provider.pipeline() {
            Some(pipeline) => pipeline.send_video_frame(session, pts, frame)?,
            None => {
                let encoded = self
                    .provider
                    .video_encoder()
                    .ok_or_else(|| serves_none("video encoder"))?
                    .encode_video(session, pts, frame)?;
                self.ready.push_back(Answered::VideoFrame(encoded));
            }
        }
        self.stages.push_back(VIDEO_STAGE);
        Ok(())
    }

    fn send_audio(
        &mut self,
        session: SessionId,
        pts: i64,
        samples: &[f32],
    ) -> Result<(), MediaError> {
        match self.provider.pipeline() {
            Some(pipeline) => pipeline.send_audio(session, pts, samples)?,
            None => {
                let encoded = self
                    .provider
                    .audio_encoder()
                    .ok_or_else(|| serves_none("audio encoder"))?
                    .encode_audio(session, pts, samples)?;
                self.ready.push_back(Answered::Audio(encoded));
            }
        }
        self.stages.push_back(AUDIO_STAGE);
        Ok(())
    }

    fn send_packets(
        &mut self,
        session: SessionId,
        packets: &[TrackPacket],
    ) -> Result<(), MediaError> {
        match self.provider.pipeline() {
            Some(pipeline) => pipeline.send_packets(session, packets)?,
            None => {
                self.provider
                    .muxer()
                    .ok_or_else(|| serves_none("muxer"))?
                    .write_packets(session, packets)?;
                self.ready.push_back(Answered::Packets);
            }
        }
        self.stages.push_back(MUXER_STAGE);
        Ok(())
    }

    /// The oldest request's answer, and the stage it was sent for.
    fn receive(&mut self) -> (&'static str, Result<Answered, MediaError>) {
        let answer = match self.provider.pipeline() {
            Some(pipeline) => pipeline.receive(),
            None => self
                .ready
                .pop_front()
                .ok_or_else(|| MediaError::Codec("no request is in flight".to_owned())),
        };
        (self.stages.pop_front().unwrap_or(VIDEO_STAGE), answer)
    }
}

fn serves_none(kind: &str) -> MediaError {
    MediaError::Codec(format!("the plugin serves no {kind}"))
}

/// One export's encoders and muxer, through the processes it starts for
/// their plugins ([`ProviderUse::Export`]), one per plugin, kept until the
/// session ends. The video encoder opens before the first frame; the muxer
/// opens once the video's config is known and the audio is decided, and
/// until then the video packets wait in the [`Interleaver`].
///
/// Frames, audio chunks, and batches of packets go up to each process's
/// depth at a time, answered in the order sent; every other request to a
/// process waits until its answers are all in.
pub(crate) struct ExportSession {
    registry: Arc<MediaRegistry>,
    cancellation: ExportCancellation,
    kill: PluginKillSwitch,
    processes: HashMap<usize, Plugin>,
    /// The frame size the video encoder's ring is sized for.
    frame: (u32, u32),
    resolved: Option<ResolvedOutput>,
    video: Option<VideoTrack>,
    audio: AudioTrack,
    /// The audio encoder's session while the mix is being encoded.
    audio_session: Option<SessionId>,
    /// The audio encoder's packets and first config, as they are received.
    audio_packets: Vec<Packet>,
    audio_config: Option<TrackConfig>,
    /// Sample frames of the loop pre-roll the muxer hides.
    leading_audio: u64,
    staged: Option<PathBuf>,
    interleaver: Option<Interleaver>,
    /// Packet bytes written to the muxer per request at most.
    request_bytes: usize,
    /// Ready packets not yet written, and their bytes.
    pending_write: Vec<TrackPacket>,
    pending_bytes: usize,
    muxer: Option<SessionId>,
    /// How the video encoder's process takes the frames, once it opened.
    frame_path: FramePath,
}

/// The process of plugin `plugin`, started for the export with `ring` when
/// it is first needed.
fn process<'a>(
    processes: &'a mut HashMap<usize, Plugin>,
    registry: &MediaRegistry,
    kill: &PluginKillSwitch,
    plugin: usize,
    ring: Option<RingLayout>,
) -> Result<&'a mut Plugin, MediaError> {
    Ok(match processes.entry(plugin) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => entry.insert(Plugin::new((registry.plugin(plugin).factory)(
            kill,
            ProviderUse::Export { ring },
        )?)),
    })
}

/// The formats a plugin's manifest says its video encoders take, which
/// size the ring of a process started to describe it: `None` without a
/// video encoder, and none when one does not say (an RGBA16 frame then
/// sizes it). Names Celesta does not know are left out.
fn manifest_formats(media: &MediaManifest) -> Option<Vec<PixelFormat>> {
    if media.video_encoders.is_empty() {
        return None;
    }
    let mut formats = Vec::new();
    for encoder in &media.video_encoders {
        let Some(names) = &encoder.input_formats else {
            return Some(Vec::new());
        };
        formats.extend(
            names
                .iter()
                .filter_map(|name| name.parse::<PixelFormat>().ok()),
        );
    }
    Some(formats)
}

/// A plugin's process as a provider that starts only when it is asked to
/// describe, so a cached description starts nothing.
struct Lazy<'a> {
    processes: &'a mut HashMap<usize, Plugin>,
    registry: &'a MediaRegistry,
    kill: &'a PluginKillSwitch,
    plugin: usize,
    frame: (u32, u32),
}

impl MediaProvider for Lazy<'_> {
    fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
        let formats = manifest_formats(&self.registry.plugin(self.plugin).media);
        let ring = ExportSession::ring_for(formats.as_deref(), self.frame);
        process(self.processes, self.registry, self.kill, self.plugin, ring)?
            .provider
            .describe()
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
            frame: (0, 0),
            resolved: None,
            video: None,
            audio: AudioTrack::Undecided,
            audio_session: None,
            audio_packets: Vec::new(),
            audio_config: None,
            leading_audio: 0,
            staged: None,
            interleaver: None,
            request_bytes: WRITE_REQUEST_BYTES,
            pending_write: Vec::new(),
            pending_bytes: 0,
            muxer: None,
            frame_path: FramePath::default(),
        }
    }

    /// Chooses the components `choice` names for `output`
    /// ([`resolve_output`]), describing each plugin through the process
    /// this session keeps for it, whose ring is sized for `frame`-sized
    /// frames. Processes of plugins left unused stop.
    pub(crate) fn resolve(
        &mut self,
        choice: &OutputChoice,
        output: &Path,
        frame: (u32, u32),
    ) -> Result<OutputPlan, ExportError> {
        self.frame = frame;
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
                    frame,
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

    /// The ring for `frame`-sized frames of an encoder taking `formats`
    /// ([`ring_layout`]); none for a process that encodes no video.
    fn ring_for(formats: Option<&[PixelFormat]>, frame: (u32, u32)) -> Option<RingLayout> {
        formats.and_then(|formats| ring_layout(formats, frame.0, frame.1))
    }

    /// Whether the resolved output has an audio track to encode.
    pub(crate) fn encodes_audio(&self) -> bool {
        self.resolved
            .as_ref()
            .is_some_and(|resolved| resolved.audio.is_some())
    }

    /// The frames sent to the video encoder so far.
    pub(crate) fn frames_sent(&self) -> u64 {
        self.video.as_ref().map_or(0, |video| video.frames_sent)
    }

    /// What the session measured of its staged video so far; the whole
    /// export's is [`ExportedTracks::timing`].
    fn timing(&self) -> SessionTiming {
        self.interleaver
            .as_ref()
            .map_or_else(SessionTiming::default, |interleaver| SessionTiming {
                peak_staged_bytes: interleaver.peak_staged_bytes(),
                spilled: interleaver.spilled(),
                frame_path: self.frame_path,
            })
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

    /// The process of plugin `plugin`, started if it is not running: the
    /// video encoder's with a ring sized from the formats it described.
    fn plugin(&mut self, plugin: usize) -> Result<&mut Plugin, MediaError> {
        let ring = match &self.resolved {
            Some(resolved) if resolved.video.plugin == plugin => {
                Self::ring_for(Some(&resolved.video.input_formats), self.frame)
            }
            _ => None,
        };
        process(
            &mut self.processes,
            &self.registry,
            &self.kill,
            plugin,
            ring,
        )
    }

    /// Receives answers from plugin `plugin` until another request fits its
    /// window.
    fn make_room(&mut self, plugin: usize) -> Result<(), ExportError> {
        while let Some(process) = self.processes.get_mut(&plugin)
            && process.in_flight() >= process.depth()
        {
            self.drain_one(plugin)?;
        }
        Ok(())
    }

    /// Receives every answer plugin `plugin` owes, as a one-at-a-time
    /// request to it needs.
    fn drain(&mut self, plugin: usize) -> Result<(), ExportError> {
        while let Some(process) = self.processes.get_mut(&plugin)
            && process.in_flight() > 0
        {
            self.drain_one(plugin)?;
        }
        Ok(())
    }

    /// Receives plugin `plugin`'s oldest answer: a frame's packets go to the
    /// interleaver, an audio chunk's wait for the rest of the mix, and each
    /// track keeps its first config.
    fn drain_one(&mut self, plugin: usize) -> Result<(), ExportError> {
        let Some(process) = self.processes.get_mut(&plugin) else {
            return Ok(());
        };
        let (stage, answer) = process.receive();
        match answer.map_err(|source| self.error(stage, source))? {
            Answered::VideoFrame(encoded) => self.take_video(encoded)?,
            Answered::Audio(encoded) => self.take_audio(encoded),
            Answered::Packets => {}
        }
        Ok(())
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
        // A ring its process accepted carries frames only to an encoder
        // that describes taking them so.
        let shared = video
            .transports
            .iter()
            .any(|transport| transport == SHARED_MEMORY_TRANSPORT);
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
        let process = self.plugin(plugin)?;
        if !shared && let Some(pipeline) = process.provider.pipeline() {
            pipeline.decline_ring();
        }
        let opened = process
            .provider
            .video_encoder()
            .ok_or_else(|| missing(&registry, plugin, "video encoder"))?
            .open_video(&request)?;
        self.frame_path = match process.provider.pipeline() {
            Some(pipeline) if pipeline.frames_in_ring() => FramePath::Ring(pipeline.depth()),
            Some(pipeline) => FramePath::Pipe(pipeline.depth()),
            None => FramePath::InProcess,
        };
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

    /// Sends the next frame's pixels, shown at `frames_sent × denominator`,
    /// once the encoder's window has room, and writes what the muxer can
    /// take.
    fn encode_pixels(&mut self, pixels: Vec<u8>) -> Result<(), ExportError> {
        let (plugin, session, pts) = self
            .next_frame()
            .map_err(|source| self.error(VIDEO_STAGE, source))?;
        self.make_room(plugin)?;
        self.plugin(plugin)
            .and_then(|process| process.send_video_frame(session, pts, pixels))
            .map_err(|source| self.error(VIDEO_STAGE, source))?;
        if let Some(video) = self.video.as_mut() {
            video.frames_sent += 1;
        }
        self.flush(false)
    }

    /// The video encoder's plugin and session, and the next frame's pts.
    fn next_frame(&self) -> Result<(usize, SessionId, i64), MediaError> {
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
        Ok((plugin, session, pts))
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

    /// Keeps the audio's first config and its packets until the mix is
    /// encoded.
    fn take_audio(&mut self, encoded: Encoded) {
        self.audio_packets.extend(encoded.packets);
        if self.audio_config.is_none() {
            self.audio_config = encoded.config;
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
        self.try_encode_audio(mix, looping)?;
        self.flush(false)
    }

    fn try_encode_audio(&mut self, mix: &AudioBuffer, looping: bool) -> Result<(), ExportError> {
        let audio_error = |session: &Self, source| session.error(AUDIO_STAGE, source);
        let (plugin, component, settings, trims) = self
            .audio_encoder()
            .map_err(|source| audio_error(self, source))?;
        // The open waits for the answers the plugin owes, which a process
        // that also encodes the video may.
        self.drain(plugin)?;
        let request = AudioEncoderRequest {
            component: &component,
            settings: &settings,
            sample_rate: mix.sample_rate,
            channels: mix.channels,
        };
        let opened = self
            .open_audio(plugin, &request)
            .map_err(|source| audio_error(self, source))?;
        // Opened: an abort from here on ends the session.
        self.audio_session = Some(opened.session);
        if !(1..=2).contains(&opened.channels) {
            return Err(audio_error(
                self,
                MediaError::InvalidSettings {
                    key: "channels".to_owned(),
                    message: format!(
                        "the encoder chose {} channels; Celesta sends 1 or 2",
                        opened.channels
                    ),
                },
            ));
        }

        let mix = if opened.channels == mix.channels {
            Cow::Borrowed(mix)
        } else {
            Cow::Owned(convert_channels(mix, opened.channels))
        };
        let mix = if opened.sample_rate == mix.sample_rate {
            mix
        } else {
            Cow::Owned(resample_audio(&mix, opened.sample_rate).map_err(|e| audio_error(self, e))?)
        };
        let lead_in = if looping && trims {
            loop_lead_in(&mix)
        } else {
            Vec::new()
        };
        let channels = usize::from(opened.channels);
        let lead_frames = lead_in.len() / channels;
        let chunk = AUDIO_CHUNK_FRAMES * channels;

        let mut pts = -(lead_frames as i64);
        for samples in lead_in.chunks(chunk).chain(mix.samples.chunks(chunk)) {
            self.make_room(plugin)?;
            self.plugin(plugin)
                .and_then(|process| process.send_audio(opened.session, pts, samples))
                .map_err(|source| audio_error(self, source))?;
            pts += (samples.len() / channels) as i64;
        }
        self.drain(plugin)?;
        let finished = self
            .finish_audio(plugin, opened.session)
            .map_err(|source| audio_error(self, source))?;
        self.take_audio(finished);
        self.audio_session = None;
        let config = self.audio_config.take().ok_or_else(|| {
            audio_error(
                self,
                MediaError::Codec("the audio encoder produced no configuration".to_owned()),
            )
        })?;

        let packets = std::mem::take(&mut self.audio_packets);
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

    /// The audio encoder's plugin, component, and settings, and whether the
    /// muxer trims leading audio, once the audio may be encoded.
    fn audio_encoder(&self) -> Result<(usize, String, celesta_media::Settings, bool), MediaError> {
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
        Ok((
            audio.plugin,
            audio.reference.component.clone(),
            audio.settings.clone(),
            resolved.muxer.entry.trims_leading_audio,
        ))
    }

    fn open_audio(
        &mut self,
        plugin: usize,
        request: &AudioEncoderRequest<'_>,
    ) -> Result<AudioEncoderOpened, MediaError> {
        let registry = self.registry.clone();
        self.plugin(plugin)?
            .provider
            .audio_encoder()
            .ok_or_else(|| missing(&registry, plugin, "audio encoder"))?
            .open_audio(request)
    }

    fn finish_audio(&mut self, plugin: usize, session: SessionId) -> Result<Encoded, MediaError> {
        let registry = self.registry.clone();
        self.plugin(plugin)?
            .provider
            .audio_encoder()
            .ok_or_else(|| missing(&registry, plugin, "audio encoder"))?
            .finish_audio(session)
    }

    /// Decides that the export has no audio track. The answers the video
    /// encoder owes are received, so the muxer can open on the video's
    /// config now: before the first frame (a project's export) there are
    /// none, and after the last (a React entry's) the finish waits for them
    /// anyway.
    pub(crate) fn no_audio(&mut self) -> Result<(), ExportError> {
        if matches!(self.audio, AudioTrack::Undecided) {
            let Some(interleaver) = self.interleaver.as_mut() else {
                return Err(self.error(MUXER_STAGE, not_ready("no open video encoder")));
            };
            interleaver.finish_audio();
            self.audio = AudioTrack::None;
        }
        if let Some(resolved) = &self.resolved {
            self.drain(resolved.video.plugin)?;
        }
        self.flush(false)
    }

    /// Opens the muxer, once the video's config is known and the audio is
    /// decided (none, or its config known). The open waits for the answers
    /// the muxer's plugin owes, which a process that also encodes may.
    fn try_open_muxer(&mut self) -> Result<(), ExportError> {
        let due = self.muxer.is_none()
            && self
                .video
                .as_ref()
                .is_some_and(|video| video.config.is_some())
            && !matches!(self.audio, AudioTrack::Undecided);
        let Some(plugin) = self.resolved.as_ref().map(|resolved| resolved.muxer.plugin) else {
            return Ok(());
        };
        if !due {
            return Ok(());
        }
        self.drain(plugin)?;
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
            .plugin(plugin)?
            .provider
            .muxer()
            .ok_or_else(|| missing(&registry, plugin, "muxer"))?
            .open_muxer(&request)?;
        self.muxer = Some(session);
        Ok(())
    }

    /// Opens the muxer when it can, and sends what the interleaver has
    /// ready as `writePackets` requests: one per [`WRITE_BATCH_PACKETS`]
    /// packets or [`WRITE_BATCH_BYTES`] bytes, never more than
    /// `request_bytes` (but one packet, however large) or
    /// [`WRITE_REQUEST_PACKETS`] in one, and the rest too when `force`.
    /// Packets are taken from a staged backlog a request at a time, so it
    /// is never all in memory at once; a spill file that
    /// cannot be read back fails the export, and the packets taken with it
    /// are lost.
    fn flush(&mut self, force: bool) -> Result<(), ExportError> {
        self.try_open_muxer()?;
        let Some(session) = self.muxer else {
            return Ok(());
        };
        while let Some(interleaver) = self.interleaver.as_mut() {
            let ready = match interleaver.take_ready(self.request_bytes) {
                Ok(ready) => ready,
                Err(error) => return Err(self.cancellation.settle(error)),
            };
            if ready.is_empty() {
                break;
            }
            let bytes: usize = ready.iter().map(|ready| packet_bytes(&ready.packet)).sum();
            if !self.pending_write.is_empty()
                && (self.pending_bytes.saturating_add(bytes) > self.request_bytes
                    || self.pending_write.len() + ready.len() > WRITE_REQUEST_PACKETS)
            {
                self.write_pending(session)?;
            }
            self.pending_write.extend(ready);
            self.pending_bytes = self.pending_bytes.saturating_add(bytes);
            if self.pending_write.len() >= WRITE_BATCH_PACKETS
                || self.pending_bytes >= WRITE_BATCH_BYTES
            {
                self.write_pending(session)?;
            }
        }
        if force && !self.pending_write.is_empty() {
            self.write_pending(session)?;
        }
        Ok(())
    }

    /// Sends the packets waiting to be written as one `writePackets`, once
    /// the muxer's window has room.
    fn write_pending(&mut self, session: SessionId) -> Result<(), ExportError> {
        let plugin = self
            .resolved()
            .map_err(|source| self.error(MUXER_STAGE, source))?
            .muxer
            .plugin;
        self.make_room(plugin)?;
        let packets = std::mem::take(&mut self.pending_write);
        self.pending_bytes = 0;
        self.plugin(plugin)
            .and_then(|process| process.send_packets(session, &packets))
            .map_err(|source| self.error(MUXER_STAGE, source))
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
        self.plugin(plugin)?
            .provider
            .video_encoder()
            .ok_or_else(|| missing(&registry, plugin, "video encoder"))?
            .finish_video(session)
    }

    fn finish_muxer(&mut self, session: SessionId) -> Result<Vec<PathBuf>, MediaError> {
        let plugin = self.resolved()?.muxer.plugin;
        let registry = self.registry.clone();
        self.plugin(plugin)?
            .provider
            .muxer()
            .ok_or_else(|| missing(&registry, plugin, "muxer"))?
            .finish_muxer(session)
    }

    fn try_finish(&mut self) -> Result<ExportedTracks, ExportError> {
        let mut plugins: Vec<usize> = self.processes.keys().copied().collect();
        plugins.sort_unstable();
        for plugin in plugins {
            self.drain(plugin)?;
        }
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
        self.flush(true)?;
        let Some(session) = self.muxer else {
            return Err(self.error(MUXER_STAGE, not_ready("no open muxer")));
        };
        let muxer_plugin = self
            .resolved()
            .map_err(|source| self.error(MUXER_STAGE, source))?
            .muxer
            .plugin;
        self.drain(muxer_plugin)?;
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
            timing: self.timing(),
        })
    }

    /// Ends every session still open (`abortEncoder`, `abortMuxer`) and
    /// stops the processes. A process that does not answer within the
    /// abort's deadline is killed. A process with requests in flight is
    /// stopped without waiting on them, its sessions with it; the staged
    /// output and the spill file are the export's own and go either way.
    /// Nothing is started to abort.
    pub(crate) fn abort(mut self) {
        let Some(resolved) = self.resolved.take() else {
            return;
        };
        self.processes.retain(|_, process| {
            // An in-process provider's answers are already in.
            process.ready.clear();
            process.stages.clear();
            process.in_flight() == 0
        });
        if let Some(session) = self.video.as_mut().and_then(|video| video.session.take())
            && let Some(encoder) = self
                .processes
                .get_mut(&resolved.video.plugin)
                .and_then(|process| process.provider.video_encoder())
        {
            encoder.abort_video(session);
        }
        if let (Some(session), Some(audio)) = (self.audio_session.take(), &resolved.audio)
            && let Some(encoder) = self
                .processes
                .get_mut(&audio.plugin)
                .and_then(|process| process.provider.audio_encoder())
        {
            encoder.abort_audio(session);
        }
        if let Some(session) = self.muxer.take()
            && let Some(muxer) = self
                .processes
                .get_mut(&resolved.muxer.plugin)
                .and_then(|process| process.provider.muxer())
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
        AudioEncoder, ComponentKind, Muxer, Pipeline, RegisteredPlugin, VideoEncoder,
    };
    use celesta_plugin_host::{
        MediaAudioEncoderEntry, MediaMuxerEntry, MediaOrigin, MediaVideoEncoderEntry,
    };
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

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
        /// A pipelined request's answer was waited for.
        Received,
        /// The pipeline was told to send frames in payloads.
        DeclineRing,
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
        described: Vec<ComponentDescription>,
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
            Ok(self.described.clone())
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

    fn available(kind: ComponentKind, id: &str) -> ComponentDescription {
        ComponentDescription {
            available: true,
            reason: None,
            ..ComponentDescription::unavailable(kind, id, "")
        }
    }

    fn video_entry(input_formats: Option<&[&str]>) -> MediaVideoEncoderEntry {
        MediaVideoEncoderEntry {
            id: "video".to_owned(),
            name: "Video".to_owned(),
            codecs: vec!["fake".to_owned()],
            input_formats: input_formats
                .map(|formats| formats.iter().map(|&format| format.to_owned()).collect()),
        }
    }

    fn audio_entry() -> MediaAudioEncoderEntry {
        MediaAudioEncoderEntry {
            id: "pcm".to_owned(),
            name: "PCM".to_owned(),
            codecs: vec!["lpcm".to_owned()],
        }
    }

    /// The muxer `file`, writing `.fake` with `fake` video and, when
    /// `audio`, `lpcm` audio.
    fn muxer_entry(audio: bool, trims: bool) -> MediaMuxerEntry {
        MediaMuxerEntry {
            id: "file".to_owned(),
            name: "File".to_owned(),
            extensions: vec!["fake".to_owned()],
            video: vec!["fake".to_owned()],
            audio: audio.then(|| vec!["lpcm".to_owned()]),
            default_video: Some("fake".to_owned()),
            default_audio: audio.then(|| "lpcm".to_owned()),
            trims_leading_audio: trims,
            sequence: false,
        }
    }

    /// The uses each plugin's factory was started for, by plugin id.
    type Uses = Arc<Mutex<Vec<(String, ProviderUse)>>>;

    /// A bundled plugin `id` declaring `media`, each instance of it a
    /// [`Fake`] describing `described`, noting its starts in `uses`.
    fn fake_plugin(
        id: &str,
        media: MediaManifest,
        described: Vec<ComponentDescription>,
        answers: Answers,
        log: &Arc<Mutex<Vec<Asked>>>,
        uses: &Uses,
    ) -> RegisteredPlugin {
        let (log, uses, name) = (log.clone(), uses.clone(), id.to_owned());
        RegisteredPlugin {
            id: id.to_owned(),
            version: "1.0.0".to_owned(),
            origin: MediaOrigin::Bundled,
            directory: PathBuf::new(),
            executable: None,
            media,
            factory: Arc::new(move |_, purpose| {
                uses.lock().unwrap().push((name.clone(), purpose));
                Ok(Box::new(Fake {
                    answers,
                    described: described.clone(),
                    log: log.clone(),
                    last: 0,
                    video_config_due: true,
                    audio_channels: 1,
                    audio_config_due: true,
                }) as Box<dyn MediaProvider>)
            }),
        }
    }

    /// A session through the fake plugin, resolved for `out.fake`, and what
    /// the plugin is asked.
    fn fake_session(answers: Answers) -> (ExportSession, Arc<Mutex<Vec<Asked>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let plugin = fake_plugin(
            "fake",
            MediaManifest {
                decoders: Vec::new(),
                video_encoders: vec![video_entry(None)],
                audio_encoders: vec![audio_entry()],
                muxers: vec![muxer_entry(true, answers.trims)],
            },
            vec![
                available(ComponentKind::VideoEncoder, "video"),
                available(ComponentKind::AudioEncoder, "pcm"),
                available(ComponentKind::Muxer, "file"),
            ],
            answers,
            &log,
            &Uses::default(),
        );
        let registry = Arc::new(MediaRegistry::new(vec![plugin]));
        let mut session = ExportSession::start(registry, &ExportCancellation::default());
        session
            .resolve(&OutputChoice::default(), Path::new("out.fake"), (2, 2))
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
        session.request_bytes = 40;
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

    /// How the pipelined fake answers `receive`.
    #[derive(Clone, Copy, Default, PartialEq)]
    enum Receiving {
        /// Each request's answer, in order.
        #[default]
        Answers,
        /// The second receive is `Err(MediaError::Codec("boom"))`.
        FailsSecond,
        /// Any receive panics.
        Panics,
    }

    /// A video encoder whose provider pipelines three frames deep, as a
    /// plugin's process does: it notes each frame sent
    /// (`Asked::EncodeVideo`) and each answer received (`Asked::Received`),
    /// answers a frame with one 100-byte packet (and the config on the
    /// first), and refuses a one-at-a-time request while any is in flight.
    struct Piped {
        log: Arc<Mutex<Vec<Asked>>>,
        receiving: Receiving,
        /// The transports its video encoder describes.
        transports: Vec<String>,
        /// Whether it was told to send frames in payloads.
        declined: bool,
        answers: VecDeque<Answered>,
        received: usize,
        config_due: bool,
        dropped: Arc<AtomicBool>,
    }

    impl Piped {
        fn ask(&self, asked: Asked) {
            self.log.lock().unwrap().push(asked);
        }

        fn idle(&self) -> Result<(), MediaError> {
            if self.answers.is_empty() {
                Ok(())
            } else {
                Err(MediaError::Codec("requests are in flight".to_owned()))
            }
        }
    }

    impl Drop for Piped {
        fn drop(&mut self) {
            self.dropped.store(true, AtomicOrdering::SeqCst);
        }
    }

    impl MediaProvider for Piped {
        fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
            self.idle()?;
            Ok(vec![ComponentDescription {
                transports: self.transports.clone(),
                ..available(ComponentKind::VideoEncoder, "video")
            }])
        }

        fn video_encoder(&mut self) -> Option<&mut dyn VideoEncoder> {
            Some(self)
        }

        fn pipeline(&mut self) -> Option<&mut dyn Pipeline> {
            Some(self)
        }
    }

    impl VideoEncoder for Piped {
        fn open_video(
            &mut self,
            request: &VideoEncoderRequest<'_>,
        ) -> Result<VideoEncoderOpened, MediaError> {
            self.idle()?;
            self.ask(Asked::OpenVideo);
            Ok(VideoEncoderOpened {
                session: SessionId(1),
                input: request.offered[0],
                alpha: false,
            })
        }

        fn encode_video(
            &mut self,
            _: SessionId,
            _: i64,
            _: Vec<u8>,
        ) -> Result<Encoded, MediaError> {
            unreachable!("frames go through the pipeline")
        }

        fn finish_video(&mut self, _: SessionId) -> Result<Encoded, MediaError> {
            self.idle()?;
            self.ask(Asked::FinishVideo);
            Ok(Encoded::default())
        }

        fn abort_video(&mut self, session: SessionId) {
            self.ask(Asked::AbortVideo(session));
        }
    }

    impl Pipeline for Piped {
        fn depth(&self) -> usize {
            3
        }

        fn in_flight(&self) -> usize {
            self.answers.len()
        }

        fn decline_ring(&mut self) {
            self.ask(Asked::DeclineRing);
            self.declined = true;
        }

        fn frames_in_ring(&self) -> bool {
            !self.declined
        }

        fn send_video_frame(
            &mut self,
            _: SessionId,
            pts: i64,
            _: Vec<u8>,
        ) -> Result<(), MediaError> {
            self.ask(Asked::EncodeVideo { pts });
            let config = std::mem::take(&mut self.config_due).then(|| config("fake"));
            self.answers.push_back(Answered::VideoFrame(Encoded {
                packets: vec![Packet {
                    pts,
                    dts: pts,
                    duration: 1,
                    key: true,
                    bytes: vec![pts as u8; 100],
                    additional: Vec::new(),
                }],
                config,
            }));
            Ok(())
        }

        fn send_audio(&mut self, _: SessionId, _: i64, _: &[f32]) -> Result<(), MediaError> {
            unreachable!("the fake encodes no audio")
        }

        fn send_packets(&mut self, _: SessionId, _: &[TrackPacket]) -> Result<(), MediaError> {
            unreachable!("the fake muxes nothing")
        }

        fn receive(&mut self) -> Result<Answered, MediaError> {
            assert!(
                self.receiving != Receiving::Panics,
                "the fake was waited on"
            );
            self.ask(Asked::Received);
            self.received += 1;
            let answer = self
                .answers
                .pop_front()
                .ok_or_else(|| MediaError::Codec("no request is in flight".to_owned()))?;
            if self.receiving == Receiving::FailsSecond && self.received == 2 {
                return Err(MediaError::Codec("boom".to_owned()));
            }
            Ok(answer)
        }
    }

    /// A session whose video encoder is [`Piped`], in plugin `v`, and whose
    /// muxer is a [`Fake`] in plugin `m`: what both are asked, and whether
    /// the encoder's provider was dropped.
    fn piped_session(
        receiving: Receiving,
    ) -> (ExportSession, Arc<Mutex<Vec<Asked>>>, Arc<AtomicBool>) {
        piped_session_with(receiving, &["sharedMemory", "pipe"])
    }

    /// [`piped_session`] with a video encoder describing `transports`.
    fn piped_session_with(
        receiving: Receiving,
        transports: &[&str],
    ) -> (ExportSession, Arc<Mutex<Vec<Asked>>>, Arc<AtomicBool>) {
        let transports: Vec<String> = transports.iter().map(|&name| name.to_owned()).collect();
        let log = Arc::new(Mutex::new(Vec::new()));
        let dropped = Arc::new(AtomicBool::new(false));
        let (encoder_log, encoder_dropped) = (log.clone(), dropped.clone());
        let encoder = RegisteredPlugin {
            id: "v".to_owned(),
            version: "1.0.0".to_owned(),
            origin: MediaOrigin::Bundled,
            directory: PathBuf::new(),
            executable: None,
            media: MediaManifest {
                video_encoders: vec![video_entry(None)],
                ..MediaManifest::default()
            },
            factory: Arc::new(move |_, _| {
                Ok(Box::new(Piped {
                    log: encoder_log.clone(),
                    receiving,
                    transports: transports.clone(),
                    declined: false,
                    answers: VecDeque::new(),
                    received: 0,
                    config_due: true,
                    dropped: encoder_dropped.clone(),
                }) as Box<dyn MediaProvider>)
            }),
        };
        let muxer = fake_plugin(
            "m",
            MediaManifest {
                muxers: vec![muxer_entry(false, false)],
                ..MediaManifest::default()
            },
            vec![available(ComponentKind::Muxer, "file")],
            Answers::default(),
            &log,
            &Uses::default(),
        );
        let registry = Arc::new(MediaRegistry::new(vec![encoder, muxer]));
        let mut session = ExportSession::start(registry, &ExportCancellation::default());
        session
            .resolve(&OutputChoice::default(), Path::new("out.fake"), (2, 2))
            .unwrap();
        (session, log, dropped)
    }

    fn count(log: &Mutex<Vec<Asked>>, which: impl Fn(&Asked) -> bool) -> usize {
        log_of(log).iter().filter(|asked| which(asked)).count()
    }

    fn sends(log: &Mutex<Vec<Asked>>) -> usize {
        count(log, |asked| matches!(asked, Asked::EncodeVideo { .. }))
    }

    fn receives(log: &Mutex<Vec<Asked>>) -> usize {
        count(log, |asked| *asked == Asked::Received)
    }

    #[test]
    fn frames_are_sent_up_to_the_depth_before_waiting() {
        let (mut session, log, _) = piped_session(Receiving::Answers);
        open(&mut session);
        frames(&mut session, 3);
        assert_eq!((sends(&log), receives(&log)), (3, 0), "{:?}", log_of(&log));
        session.encode_pixels(vec![3; 16]).unwrap();
        let asked = log_of(&log);
        assert_eq!(
            asked[asked.len() - 2..],
            [Asked::Received, Asked::EncodeVideo { pts: 3 }],
            "{asked:?}"
        );
        assert_eq!(session.frames_sent(), 4);
    }

    #[test]
    fn an_encoder_without_shared_memory_gets_no_ring() {
        for (transports, declined) in [
            (&["sharedMemory", "pipe"][..], false),
            (&["pipe"][..], true),
            (&[][..], true),
        ] {
            let (mut session, log, _) = piped_session_with(Receiving::Answers, transports);
            open(&mut session);
            let asked = log_of(&log);
            let expected: &[Asked] = if declined {
                &[Asked::DeclineRing, Asked::OpenVideo]
            } else {
                &[Asked::OpenVideo]
            };
            assert_eq!(asked, expected, "{transports:?}");
        }
    }

    #[test]
    fn the_timing_names_how_the_frames_went() {
        for (transports, path) in [
            (&["sharedMemory", "pipe"][..], FramePath::Ring(3)),
            (&["pipe"][..], FramePath::Pipe(3)),
        ] {
            let (mut session, _, _) = piped_session_with(Receiving::Answers, transports);
            open(&mut session);
            session.no_audio().unwrap();
            frames(&mut session, 2);
            let timing = session.finish().unwrap().timing;
            assert_eq!(timing.frame_path, path, "{transports:?}");
        }
        let (mut session, _) = fake_session(Answers::default());
        open(&mut session);
        session.no_audio().unwrap();
        frames(&mut session, 2);
        let timing = session.finish().unwrap().timing;
        assert_eq!(timing.frame_path, FramePath::InProcess);
    }

    #[test]
    fn write_packets_are_batched() {
        let (mut session, log, _) = piped_session(Receiving::Answers);
        open(&mut session);
        session.no_audio().unwrap();
        frames(&mut session, 20);
        session.finish().unwrap();
        let batches: Vec<Vec<TrackPacket>> = log_of(&log)
            .into_iter()
            .filter_map(|asked| match asked {
                Asked::WritePackets(packets) => Some(packets),
                _ => None,
            })
            .collect();
        assert!(
            batches.len() <= 20 / WRITE_BATCH_PACKETS + 2,
            "{} requests",
            batches.len()
        );
        let dts: Vec<i64> = batches
            .iter()
            .flatten()
            .map(|packet| packet.packet.dts)
            .collect();
        assert_eq!(dts, (0..20).collect::<Vec<_>>());
    }

    #[test]
    fn the_staged_peak_counts_the_packets_finish_receives() {
        let (mut session, _, _) = piped_session(Receiving::Answers);
        open(&mut session);
        session.no_audio().unwrap();
        // The three frames' answers are received only by finish.
        frames(&mut session, 3);
        assert_eq!(session.timing().peak_staged_bytes, 0);
        let timing = session.finish().unwrap().timing;
        assert!(timing.peak_staged_bytes >= 300, "{timing:?}");
    }

    #[test]
    fn finish_drains_the_window_first() {
        let (mut session, log, _) = piped_session(Receiving::Answers);
        open(&mut session);
        session.no_audio().unwrap();
        frames(&mut session, 2);
        assert_eq!(receives(&log), 0);
        session.finish().unwrap();
        let asked = log_of(&log);
        let finished = asked
            .iter()
            .position(|asked| *asked == Asked::FinishVideo)
            .expect("the encoder finished");
        let received = asked[..finished]
            .iter()
            .filter(|asked| **asked == Asked::Received)
            .count();
        assert_eq!(received, 2, "{asked:?}");
    }

    #[test]
    fn abort_with_frames_in_flight_does_not_wait() {
        let (mut session, log, dropped) = piped_session(Receiving::Panics);
        open(&mut session);
        frames(&mut session, 2);
        session.abort();
        assert!(
            dropped.load(AtomicOrdering::SeqCst),
            "the encoder was dropped"
        );
        assert_eq!(receives(&log), 0);
        assert!(
            !log_of(&log)
                .iter()
                .any(|asked| matches!(asked, Asked::AbortVideo(_))),
            "nothing is sent behind the frames in flight"
        );
    }

    #[test]
    fn a_refused_frame_fails_the_export() {
        let (mut session, _, _) = piped_session(Receiving::FailsSecond);
        open(&mut session);
        // Three frames fill the window; the fourth waits on the first's
        // answer, and the fifth on the second's, the refused one.
        frames(&mut session, 4);
        let result = session.encode_pixels(vec![4; 16]);
        assert!(
            matches!(
                &result,
                Err(ExportError::Codec { stage: VIDEO_STAGE, source })
                    if source.to_string().contains("boom")
            ),
            "{result:?}"
        );
    }

    /// The `Export` uses plugin `id` was started for.
    fn exports_of(uses: &Uses, id: &str) -> Vec<ProviderUse> {
        uses.lock()
            .unwrap()
            .iter()
            .filter(|(plugin, purpose)| plugin == id && *purpose != ProviderUse::Describe)
            .map(|(_, purpose)| *purpose)
            .collect()
    }

    fn ring(slots: usize, slot_bytes: usize) -> ProviderUse {
        ProviderUse::Export {
            ring: Some(RingLayout { slots, slot_bytes }),
        }
    }

    /// A video encoder and a muxer `file` (writing no audio) in plugin `id`,
    /// with `inputFormats` `manifest` in its manifest and `described` in
    /// its description.
    fn encoder_plugin(
        id: &str,
        manifest: Option<&[&str]>,
        described: &[PixelFormat],
        log: &Arc<Mutex<Vec<Asked>>>,
        uses: &Uses,
    ) -> RegisteredPlugin {
        fake_plugin(
            id,
            MediaManifest {
                video_encoders: vec![video_entry(manifest)],
                muxers: vec![muxer_entry(false, false)],
                ..MediaManifest::default()
            },
            vec![
                ComponentDescription {
                    input_formats: described.to_vec(),
                    ..available(ComponentKind::VideoEncoder, "video")
                },
                available(ComponentKind::Muxer, "file"),
            ],
            Answers::default(),
            log,
            uses,
        )
    }

    #[test]
    fn the_ring_is_sized_from_the_plugins_formats() {
        let (log, uses) = (Arc::new(Mutex::new(Vec::new())), Uses::default());
        // The description, not the manifest (which would size for rgba16),
        // sizes the video encoder's ring.
        let video = fake_plugin(
            "v",
            MediaManifest {
                video_encoders: vec![video_entry(None)],
                muxers: vec![muxer_entry(true, false)],
                ..MediaManifest::default()
            },
            vec![
                ComponentDescription {
                    input_formats: vec![PixelFormat::Yuv420p, PixelFormat::Rgba8],
                    ..available(ComponentKind::VideoEncoder, "video")
                },
                available(ComponentKind::Muxer, "file"),
            ],
            Answers::default(),
            &log,
            &uses,
        );
        let audio = fake_plugin(
            "a",
            MediaManifest {
                audio_encoders: vec![audio_entry()],
                ..MediaManifest::default()
            },
            vec![available(ComponentKind::AudioEncoder, "pcm")],
            Answers::default(),
            &log,
            &uses,
        );
        let registry = Arc::new(MediaRegistry::new(vec![video, audio]));
        for plugin in 0..2 {
            registry.describe(plugin, &PluginKillSwitch::default());
        }
        let mut session = ExportSession::start(registry, &ExportCancellation::default());
        session
            .resolve(&OutputChoice::default(), Path::new("out.fake"), (64, 48))
            .unwrap();
        assert_eq!(
            exports_of(&uses, "v"),
            [],
            "a cached describe starts nothing"
        );
        session
            .open_video(
                64,
                48,
                RATE,
                &[PixelFormat::Rgba8],
                Path::new("staged.fake"),
            )
            .unwrap();
        assert_eq!(exports_of(&uses, "v"), [ring(3, 12_288)]);
        session.encode_audio(&mix(48_000, 100), false).unwrap();
        assert_eq!(exports_of(&uses, "a"), [ProviderUse::Export { ring: None }]);
    }

    #[test]
    fn an_uncached_describe_sizes_the_ring_from_the_manifest() {
        for (manifest, expected) in [
            (Some(&["yuv420p"][..]), ring(3, 4_608)),
            (None, ring(3, 24_576)),
        ] {
            let (log, uses) = (Arc::new(Mutex::new(Vec::new())), Uses::default());
            // The description would size for rgba8.
            let plugin = encoder_plugin("v", manifest, &[PixelFormat::Rgba8], &log, &uses);
            let registry = Arc::new(MediaRegistry::new(vec![plugin]));
            let mut session = ExportSession::start(registry, &ExportCancellation::default());
            session
                .resolve(&OutputChoice::default(), Path::new("out.fake"), (64, 48))
                .unwrap();
            assert_eq!(exports_of(&uses, "v"), [expected], "{manifest:?}");
        }
    }

    #[test]
    fn no_ring_when_the_layout_is_none() {
        let (log, uses) = (Arc::new(Mutex::new(Vec::new())), Uses::default());
        let plugin = encoder_plugin("v", Some(&["rgba16"]), &[PixelFormat::Rgba16], &log, &uses);
        let registry = Arc::new(MediaRegistry::new(vec![plugin]));
        let mut session = ExportSession::start(registry, &ExportCancellation::default());
        session
            .resolve(
                &OutputChoice::default(),
                Path::new("out.fake"),
                (8192, 4320),
            )
            .unwrap();
        assert_eq!(exports_of(&uses, "v"), [ProviderUse::Export { ring: None }]);
        session
            .open_video(
                8192,
                4320,
                RATE,
                &[PixelFormat::Rgba16],
                Path::new("staged.fake"),
            )
            .unwrap();
        frames(&mut session, 2);
        session.no_audio().unwrap();
        session.finish().unwrap();
    }

    #[test]
    fn timing_line_formats_the_measurement() {
        let timing = ExportTiming {
            render: Duration::from_millis(4_500),
            frames: 300,
            mix: Some(Duration::from_millis(1_250)),
            mix_done_at_frame: Some(87),
            peak_staged_bytes: 12_582_912,
            spilled: false,
            frame_path: FramePath::Ring(3),
        };
        assert_eq!(
            timing_line(&timing),
            "celesta: timing: render 4.50 s for 300 frames; mix 1.25 s, done at frame 87; \
             staged peak 12.6 MB; spilled no; frames ring 3 deep"
        );
        assert_eq!(
            timing_line(&ExportTiming {
                mix_done_at_frame: Some(300),
                spilled: true,
                frame_path: FramePath::Pipe(1),
                ..timing
            }),
            "celesta: timing: render 4.50 s for 300 frames; mix 1.25 s, done after the render; \
             staged peak 12.6 MB; spilled yes; frames pipe 1 deep"
        );
        assert_eq!(
            timing_line(&ExportTiming {
                mix: None,
                mix_done_at_frame: None,
                frame_path: FramePath::InProcess,
                ..timing
            }),
            "celesta: timing: render 4.50 s for 300 frames; mix none; staged peak 12.6 MB; \
             spilled no; frames in process 1 deep"
        );
    }
}
