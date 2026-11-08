//! A media plugin's process, behind the [`MediaProvider`] interface.

use crate::error::MediaError;
use crate::protocol::{self, FrameLocation, ROLES, SharedSegment, decode_error};
use crate::provider::{
    Answered, AudioEncoder, AudioEncoderOpened, AudioEncoderRequest, ComponentDescription, Decoder,
    Encoded, MediaProvider, Muxer, MuxerRequest, PIPE_WINDOW, Pipeline, ProviderFactory,
    ProviderUse, SessionId, TrackPacket, VideoEncoder, VideoEncoderOpened, VideoEncoderRequest,
    VideoRequest,
};
use crate::types::{AudioBuffer, MediaProbe, VideoFrame};
use celesta_plugin::MEDIA_PROTOCOL_VERSION;
use celesta_plugin::protocol::Message;
use celesta_plugin::shared::{RingLayout, SLOT_HEADER, SharedMemory};
use celesta_plugin_host::{DeadlineFrom, PluginChild, PluginKillSwitch, RequestError};
use serde_json::Map;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// How long each request may take before the plugin is stopped as hung.
/// Generous, as a long file's audio, an encoder's flush, or muxing a long
/// export's audio takes a while, but bounded, so a hang ends.
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);
const DECODE_VIDEO_TIMEOUT: Duration = Duration::from_secs(60);
const DECODE_AUDIO_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Also protocol 2's `finishVideoEncoder` and `finishAudioEncoder`.
const FINISH_ENCODER_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Protocol 2's deadlines for describing, encoding, and muxing (the spec's
/// table). A missed one stops the process; an abort's is short, as the
/// process is killed after it anyway.
const DESCRIBE_TIMEOUT: Duration = Duration::from_secs(30);
const OPEN_TIMEOUT: Duration = Duration::from_secs(60);
const ENCODE_TIMEOUT: Duration = Duration::from_secs(60);
const FINISH_MUXER_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const ABORT_TIMEOUT: Duration = Duration::from_secs(5);

/// The bytes of the segment a codec process is offered for decoded frames:
/// a slot's header and 40 MiB, which holds an RGBA frame up to 4096x2560
/// (DCI 4K and UHD included); a larger frame goes through the pipe. Pages
/// are reserved, not used, until a frame is written.
pub const SHARED_FRAME_BYTES: usize = celesta_plugin::shared::SLOT_HEADER + (40 << 20);

/// The environment variable that, set to `0`, keeps decoded frames in the
/// pipe: no codec process is offered shared memory.
pub const SHARED_MEMORY_VARIABLE: &str = "CELESTA_CODEC_SHARED_MEMORY";

/// How a codec process's decoded frames reach Celesta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameTransport {
    /// Through the pipe, in each answer's payload.
    Pipe,
    /// Offered shared memory, which the plugin may decline (an older plugin
    /// does), and through the pipe whatever does not fit it.
    SharedMemory,
}

impl FrameTransport {
    /// Shared memory, unless [`SHARED_MEMORY_VARIABLE`] is `0`.
    pub fn from_environment() -> Self {
        match std::env::var_os(SHARED_MEMORY_VARIABLE) {
            Some(value) if value == "0" => Self::Pipe,
            _ => Self::SharedMemory,
        }
    }
}

/// A running media plugin (see [`PluginChild`] for its window, deadlines,
/// and stderr). Its encode and write requests may go several at a time
/// through its [`Pipeline`]; every other request is sent only when none is
/// in flight, and refused otherwise. Dropping it stops the process before
/// it unmaps the memory shared with it, so the process never writes into
/// memory being unmapped.
pub struct MediaProcess {
    child: PluginChild,
    /// The slot decoded frames arrive in, when the plugin accepted it.
    frames: Option<SharedSegment>,
    /// The slots video frames to encode go through, when the plugin
    /// accepted them.
    ring: Option<FrameRing>,
    /// The [`Pipeline`]'s requests in flight, oldest first, in step with
    /// the child's window.
    pending: VecDeque<Pending>,
    /// The channels of each open audio encoder session, which its
    /// `encodeAudio` counts frames by.
    audio_channels: HashMap<SessionId, u16>,
}

/// A request the [`Pipeline`] sent: what its answer is, and the ring slot
/// its frame holds until then.
struct Pending {
    kind: PendingKind,
    slot: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
enum PendingKind {
    VideoFrame,
    Audio,
    Packets,
}

/// An export's ring of frame slots in shared memory, on Celesta's side:
/// which slots are free, and the generation each was last written under. A
/// slot is taken by [`Self::place`] and free again only on
/// [`Self::release`], once the plugin answered the frame placed in it.
/// Frames are written through [`celesta_plugin::shared::Ring::write_slot`]'s
/// raw pointers; nothing holds a reference into the segment.
pub(crate) struct FrameRing {
    memory: SharedMemory,
    layout: RingLayout,
    generations: Vec<u64>,
    /// The free slots; the last is taken next.
    free: Vec<usize>,
}

impl FrameRing {
    /// `memory` seen as `layout`'s slots, every one free and never written
    /// (generation 0).
    pub(crate) fn new(memory: SharedMemory, layout: RingLayout) -> Self {
        Self {
            memory,
            layout,
            generations: vec![0; layout.slots],
            free: (0..layout.slots).rev().collect(),
        }
    }

    /// Writes `frame` into a free slot under that slot's next generation
    /// (its last one + 2) and takes the slot: where the frame is. `None`,
    /// writing nothing, when no slot is free or `frame` is larger than a
    /// slot; the frame then goes in the request's payload.
    pub(crate) fn place(&mut self, frame: &[u8]) -> Option<FrameLocation> {
        if frame.len() > self.layout.slot_bytes {
            return None;
        }
        let index = *self.free.last()?;
        let generation = self.generations[index] + 2;
        self.memory
            .ring(self.layout)
            .and_then(|ring| ring.write_slot(index, frame, generation))
            .ok()?;
        self.free.pop();
        self.generations[index] = generation;
        Some(FrameLocation::Slot {
            index,
            generation,
            bytes: frame.len(),
        })
    }

    /// Frees slot `index`, whose frame was answered; a slot that is free
    /// already, or no slot, is left alone.
    pub(crate) fn release(&mut self, index: usize) {
        if index < self.layout.slots && !self.free.contains(&index) {
            self.free.push(index);
        }
    }

    /// How many slots are free; only the tests ask, as
    /// [`Self::place`] says when none is.
    #[cfg(test)]
    pub(crate) fn free_slots(&self) -> usize {
        self.free.len()
    }

    pub(crate) fn layout(&self) -> RingLayout {
        self.layout
    }
}

impl MediaProcess {
    /// Starts `executable` in `directory` (the plugin's own) and checks that
    /// it speaks this protocol as a media plugin. The process is registered with
    /// `kill` before it is asked anything, and none starts once `kill` was
    /// killed. It decodes, and its decoded frames come through shared
    /// memory unless [`SHARED_MEMORY_VARIABLE`] says otherwise (see
    /// [`Self::start_with`]).
    pub fn start(
        executable: &Path,
        directory: &Path,
        kill: &PluginKillSwitch,
    ) -> Result<Self, MediaError> {
        Self::start_with(
            executable,
            directory,
            kill,
            FrameTransport::from_environment(),
            ProviderUse::Decode,
        )
    }

    /// [`Self::start`] for `purpose`, offering the process shared memory
    /// when `transport` asks for it and `purpose` has a use for it: a slot
    /// of [`SHARED_FRAME_BYTES`] for decoded frames to [`ProviderUse::Decode`],
    /// a segment of the ring's slots for frames to encode to
    /// [`ProviderUse::Export`] with a ring; none to
    /// [`ProviderUse::Describe`] or an export without a ring. A segment
    /// that cannot be made or passed on, or a plugin that declines it,
    /// leaves the frames in the pipe.
    pub fn start_with(
        executable: &Path,
        directory: &Path,
        kill: &PluginKillSwitch,
        transport: FrameTransport,
        purpose: ProviderUse,
    ) -> Result<Self, MediaError> {
        if kill.is_killed() {
            return Err(MediaError::CodecStopped(
                "it was not started, as its plugins were stopped".to_owned(),
            ));
        }
        let ring = match purpose {
            ProviderUse::Export { ring } => ring,
            ProviderUse::Decode | ProviderUse::Describe => None,
        };
        let segment_bytes = match (transport, purpose) {
            (FrameTransport::Pipe, _) => None,
            (FrameTransport::SharedMemory, ProviderUse::Decode) => Some(SHARED_FRAME_BYTES),
            (FrameTransport::SharedMemory, _) => ring.and_then(|layout| {
                // Checked: a caller's layout may be any size.
                layout
                    .slot_bytes
                    .checked_add(SLOT_HEADER)?
                    .checked_mul(layout.slots)
            }),
        };
        let memory = segment_bytes
            .and_then(|bytes| SharedMemory::create(bytes).ok())
            .filter(|memory| ring.is_none_or(|layout| memory.ring(layout).is_ok()));
        let (mut child, handle) =
            PluginChild::start_sharing(executable, directory, memory.as_ref())
                .map_err(MediaError::CodecUnavailable)?;
        kill.register(child.kill_handle());
        let offer = match (&memory, handle) {
            (Some(memory), Some(handle)) => protocol::shared_frames_offer(handle, memory),
            _ => Map::new(),
        };
        let hello = child
            .hello_as(MEDIA_PROTOCOL_VERSION, offer)
            .map_err(media_error)?;
        let roles = hello.header["roles"].as_array();
        if !roles.is_some_and(|roles| roles.iter().any(|role| role == ROLES[0])) {
            return Err(MediaError::CodecUnavailable(format!(
                "{} is not a media plugin (its roles are {})",
                executable.display(),
                hello.header["roles"]
            )));
        }
        let accepted =
            memory.filter(|_| handle.is_some() && protocol::accepted_shared_frames(&hello));
        let (frames, ring) = match (accepted, ring) {
            (Some(memory), Some(layout)) => (None, Some(FrameRing::new(memory, layout))),
            (Some(memory), None) => (
                Some(SharedSegment {
                    memory,
                    generation: 0,
                    ring: None,
                }),
                None,
            ),
            (None, _) => (None, None),
        };
        Ok(Self {
            child,
            frames,
            ring,
            pending: VecDeque::new(),
            audio_channels: HashMap::new(),
        })
    }

    /// Makes a [`ProviderFactory`] that starts `executable` in `directory`
    /// for the use it is given, with shared memory as [`Self::start_with`]
    /// offers it unless [`SHARED_MEMORY_VARIABLE`] says otherwise.
    pub fn factory(executable: PathBuf, directory: PathBuf) -> ProviderFactory {
        Arc::new(move |kill, purpose| {
            Ok(Box::new(MediaProcess::start_with(
                &executable,
                &directory,
                kill,
                FrameTransport::from_environment(),
                purpose,
            )?) as Box<dyn MediaProvider>)
        })
    }

    /// Whether frames go through shared memory: the plugin accepted a
    /// decode slot or an export's ring.
    pub fn shares_frames(&self) -> bool {
        self.frames.is_some() || self.ring.is_some()
    }

    /// The ring of slots video frames to encode go through, when the plugin
    /// accepted one.
    pub fn ring(&self) -> Option<RingLayout> {
        self.ring.as_ref().map(FrameRing::layout)
    }

    /// Sends `request` and waits for its answer, only when the
    /// [`Pipeline`] has nothing in flight: the answer taken would otherwise
    /// be the oldest pipelined request's, not this one's.
    fn request(
        &mut self,
        request: Message,
        expected: &str,
        timeout: Duration,
    ) -> Result<Message, MediaError> {
        self.idle(request.kind())?;
        self.child
            .request(request, expected, timeout)
            .map_err(media_error)
    }

    /// Refuses `kind` while pipelined requests are in flight.
    fn idle(&self, kind: &str) -> Result<(), MediaError> {
        match self.pending.len() {
            0 => Ok(()),
            in_flight => Err(MediaError::Codec(format!(
                "`{kind}` was asked with {in_flight} pipelined requests in flight; they are received first"
            ))),
        }
    }

    /// Sends a pipelined `request`, whose answer is `expected`, holding
    /// `slot` until it is received; a request that could not be sent frees
    /// its slot at once.
    fn send(
        &mut self,
        request: Message,
        expected: &str,
        kind: PendingKind,
        slot: Option<usize>,
    ) -> Result<(), MediaError> {
        match self.child.send(
            request,
            expected,
            ENCODE_TIMEOUT,
            DeadlineFrom::PreviousAnswer,
        ) {
            Ok(()) => {
                self.pending.push_back(Pending { kind, slot });
                Ok(())
            }
            Err(error) => {
                self.release(slot);
                Err(media_error(error))
            }
        }
    }

    fn release(&mut self, slot: Option<usize>) {
        if let (Some(slot), Some(ring)) = (slot, self.ring.as_mut()) {
            ring.release(slot);
        }
    }
}

/// The answer to a one-at-a-time encode or write, sent through the
/// [`Pipeline`] with nothing else in flight, was another kind's.
fn out_of_step() -> MediaError {
    MediaError::Codec("the plugin answered out of step".to_owned())
}

impl Pipeline for MediaProcess {
    fn depth(&self) -> usize {
        self.ring
            .as_ref()
            .map_or(PIPE_WINDOW, |ring| ring.layout().slots)
    }

    fn in_flight(&self) -> usize {
        self.pending.len()
    }

    /// The frame goes in a free slot of the ring, or in the payload when
    /// there is no ring, no slot is free, or it does not fit one.
    fn send_video_frame(
        &mut self,
        session: SessionId,
        pts: i64,
        frame: Vec<u8>,
    ) -> Result<(), MediaError> {
        let placed = self.ring.as_mut().and_then(|ring| ring.place(&frame));
        let slot = match placed {
            Some(FrameLocation::Slot { index, .. }) => Some(index),
            _ => None,
        };
        let location = placed.unwrap_or(FrameLocation::Payload(frame));
        let request = protocol::encode_video_request(session, pts, location);
        self.send(request, "videoFrameAccepted", PendingKind::VideoFrame, slot)
    }

    fn send_audio(
        &mut self,
        session: SessionId,
        pts: i64,
        samples: &[f32],
    ) -> Result<(), MediaError> {
        let channels = *self.audio_channels.get(&session).ok_or_else(|| {
            MediaError::Codec(format!("no open audio encoder session {}", session.0))
        })?;
        let request = protocol::encode_audio_request(session, pts, channels, samples)?;
        self.send(request, "audioAccepted", PendingKind::Audio, None)
    }

    fn send_packets(
        &mut self,
        session: SessionId,
        packets: &[TrackPacket],
    ) -> Result<(), MediaError> {
        let request = protocol::write_packets_request(session, packets);
        self.send(request, "packetsWritten", PendingKind::Packets, None)
    }

    /// The oldest request's slot is freed whether it was answered or not:
    /// an `error` answer means the plugin is done with the frame, and any
    /// other failure stopped the process.
    fn receive(&mut self) -> Result<Answered, MediaError> {
        let Some(Pending { kind, slot }) = self.pending.pop_front() else {
            return Err(MediaError::Codec("no request is in flight".to_owned()));
        };
        let answer = self.child.receive();
        self.release(slot);
        let answer = answer.map_err(media_error)?;
        match kind {
            PendingKind::VideoFrame => protocol::decode_encoded(&answer).map(Answered::VideoFrame),
            PendingKind::Audio => protocol::decode_encoded(&answer).map(Answered::Audio),
            PendingKind::Packets => Ok(Answered::Packets),
        }
    }
}

/// A codec that could not be talked to is unavailable, so a backend starts
/// another; one that was stopped (its deadline, or its kill switch) is not
/// asked again; one that refused says why, in the protocol's own error.
fn media_error(error: RequestError) -> MediaError {
    match error {
        RequestError::Unavailable(message) => MediaError::CodecUnavailable(message),
        RequestError::Stopped(message) => MediaError::CodecStopped(message),
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
        MediaError::Unsupported { tried, .. } => MediaError::Unsupported {
            path: path.to_owned(),
            tried,
        },
        error => error,
    })
}

impl MediaProvider for MediaProcess {
    fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
        let answer = self.request(protocol::describe_request(), "described", DESCRIBE_TIMEOUT)?;
        protocol::decode_described(&answer)
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

    fn pipeline(&mut self) -> Option<&mut dyn Pipeline> {
        Some(self)
    }
}

impl Decoder for MediaProcess {
    fn probe(&mut self, decoder: &str, path: &Path) -> Result<MediaProbe, MediaError> {
        let request = protocol::probe_request(decoder, &absolute(path))?;
        named(
            path,
            self.request(request, "probed", PROBE_TIMEOUT)
                .and_then(|answer| protocol::decode_probe(&answer)),
        )
    }

    fn decode_video(
        &mut self,
        decoder: &str,
        request: &VideoRequest<'_>,
    ) -> Result<VideoFrame, MediaError> {
        let path = absolute(request.path);
        let message = protocol::decode_video_request(
            decoder,
            &VideoRequest {
                path: &path,
                ..*request
            },
        )?;
        let answer = self.request(message, "videoFrame", DECODE_VIDEO_TIMEOUT);
        named(
            request.path,
            answer.and_then(|answer| protocol::decode_video_frame(answer, self.frames.as_mut())),
        )
    }

    fn decode_audio(
        &mut self,
        decoder: &str,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        let request =
            protocol::decode_audio_request(decoder, &absolute(path), sample_rate, channels)?;
        named(
            path,
            self.request(request, "audio", DECODE_AUDIO_TIMEOUT)
                .and_then(|answer| protocol::decode_audio_buffer(&answer)),
        )
    }
}

impl VideoEncoder for MediaProcess {
    fn open_video(
        &mut self,
        request: &VideoEncoderRequest<'_>,
    ) -> Result<VideoEncoderOpened, MediaError> {
        let request = protocol::open_video_request(request, self.ring())?;
        let answer = self.request(request, "videoEncoderOpened", OPEN_TIMEOUT)?;
        protocol::decode_video_encoder_opened(&answer)
    }

    /// [`Pipeline::send_video_frame`], then its answer; refused while
    /// pipelined requests are in flight.
    fn encode_video(
        &mut self,
        session: SessionId,
        pts: i64,
        frame: Vec<u8>,
    ) -> Result<Encoded, MediaError> {
        self.idle("encodeVideoFrame")?;
        self.send_video_frame(session, pts, frame)?;
        match Pipeline::receive(self)? {
            Answered::VideoFrame(encoded) => Ok(encoded),
            _ => Err(out_of_step()),
        }
    }

    fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        let request = protocol::finish_video_request(session);
        let answer = self.request(request, "videoEncoderFinished", FINISH_ENCODER_TIMEOUT)?;
        protocol::decode_encoded(&answer)
    }

    fn abort_video(&mut self, session: SessionId) {
        let request = protocol::abort_encoder_request(session);
        let _ = self.request(request, "encoderAborted", ABORT_TIMEOUT);
    }
}

impl AudioEncoder for MediaProcess {
    fn open_audio(
        &mut self,
        request: &AudioEncoderRequest<'_>,
    ) -> Result<AudioEncoderOpened, MediaError> {
        let request = protocol::open_audio_request(request)?;
        let answer = self.request(request, "audioEncoderOpened", OPEN_TIMEOUT)?;
        let opened = protocol::decode_audio_encoder_opened(&answer)?;
        self.audio_channels.insert(opened.session, opened.channels);
        Ok(opened)
    }

    /// [`Pipeline::send_audio`], then its answer; refused while pipelined
    /// requests are in flight.
    fn encode_audio(
        &mut self,
        session: SessionId,
        pts: i64,
        samples: &[f32],
    ) -> Result<Encoded, MediaError> {
        self.idle("encodeAudio")?;
        self.send_audio(session, pts, samples)?;
        match Pipeline::receive(self)? {
            Answered::Audio(encoded) => Ok(encoded),
            _ => Err(out_of_step()),
        }
    }

    fn finish_audio(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        self.audio_channels.remove(&session);
        let request = protocol::finish_audio_request(session);
        let answer = self.request(request, "audioEncoderFinished", FINISH_ENCODER_TIMEOUT)?;
        protocol::decode_encoded(&answer)
    }

    fn abort_audio(&mut self, session: SessionId) {
        self.audio_channels.remove(&session);
        let request = protocol::abort_encoder_request(session);
        let _ = self.request(request, "encoderAborted", ABORT_TIMEOUT);
    }
}

impl Muxer for MediaProcess {
    fn open_muxer(&mut self, request: &MuxerRequest<'_>) -> Result<SessionId, MediaError> {
        let output = absolute(request.output);
        let request = protocol::open_muxer_request(&MuxerRequest {
            output: &output,
            ..*request
        })?;
        let answer = self.request(request, "muxerOpened", OPEN_TIMEOUT)?;
        protocol::decode_muxer_opened(&answer)
    }

    /// [`Pipeline::send_packets`], then its answer; refused while
    /// pipelined requests are in flight.
    fn write_packets(
        &mut self,
        session: SessionId,
        packets: &[TrackPacket],
    ) -> Result<(), MediaError> {
        self.idle("writePackets")?;
        self.send_packets(session, packets)?;
        match Pipeline::receive(self)? {
            Answered::Packets => Ok(()),
            _ => Err(out_of_step()),
        }
    }

    fn finish_muxer(&mut self, session: SessionId) -> Result<Vec<PathBuf>, MediaError> {
        let request = protocol::finish_muxer_request(session);
        let answer = self.request(request, "muxerFinished", FINISH_MUXER_TIMEOUT)?;
        protocol::decode_files(&answer)
    }

    fn abort_muxer(&mut self, session: SessionId) -> Vec<PathBuf> {
        let request = protocol::abort_muxer_request(session);
        self.request(request, "muxerAborted", ABORT_TIMEOUT)
            .and_then(|answer| protocol::decode_files(&answer))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(unix, windows))]
    const TWO_SLOTS: RingLayout = RingLayout {
        slots: 2,
        slot_bytes: 16,
    };

    #[cfg(any(unix, windows))]
    fn slot(index: usize, generation: u64, bytes: usize) -> Option<FrameLocation> {
        Some(FrameLocation::Slot {
            index,
            generation,
            bytes,
        })
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn a_ring_hands_out_each_slot_once_until_released() {
        let memory = SharedMemory::create(TWO_SLOTS.segment_bytes()).unwrap();
        let mut ring = FrameRing::new(memory, TWO_SLOTS);
        assert_eq!(ring.free_slots(), 2);
        assert_eq!(ring.place(&[1; 8]), slot(0, 2, 8));
        assert_eq!(ring.place(&[2; 8]), slot(1, 2, 8));
        assert_eq!(ring.place(&[3; 8]), None);
        ring.release(0);
        assert_eq!(ring.free_slots(), 1);
        assert_eq!(ring.place(&[4; 8]), slot(0, 4, 8));
        assert_eq!(ring.free_slots(), 0);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn a_frame_larger_than_a_slot_is_not_placed() {
        let memory = SharedMemory::create(TWO_SLOTS.segment_bytes()).unwrap();
        let mut ring = FrameRing::new(memory, TWO_SLOTS);
        assert_eq!(ring.place(&[1; 17]), None);
        assert_eq!(ring.free_slots(), 2);
        assert_eq!(ring.place(&[1; 16]), slot(0, 2, 16));
        assert_eq!(ring.free_slots(), 1);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn placed_frames_read_back_under_their_generation() {
        let memory = SharedMemory::create(TWO_SLOTS.segment_bytes()).unwrap();
        let plugin = SharedMemory::open(memory.duplicate_handle().unwrap(), memory.len()).unwrap();
        let mut ring = FrameRing::new(memory, TWO_SLOTS);
        let frames = [vec![0xA; 16], vec![0xB; 5], vec![0xC; 8]];
        for (number, frame) in frames.iter().enumerate() {
            let Some(FrameLocation::Slot {
                index,
                generation,
                bytes,
            }) = ring.place(frame)
            else {
                panic!("frame {number} was not placed");
            };
            let read = plugin.ring(TWO_SLOTS).unwrap();
            assert_eq!(
                read.read_slot(index, bytes, generation).as_ref(),
                Some(frame)
            );
            assert_eq!((index, generation), (0, 2 * (number as u64 + 1)));
            if number > 0 {
                // Slot 0's earlier frame was written over.
                assert_eq!(read.read_slot(index, bytes, generation - 2), None);
            }
            // Released at once, so the next frame reuses slot 0.
            ring.release(index);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_media_process_needs_the_media_role() {
        use celesta_plugin::protocol::write_message;
        use serde_json::json;
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        // A plugin that answers `hello` in protocol 2, with the retired role.
        let answer = directory.path().join("answer.bin");
        let mut bytes = Vec::new();
        let hello = json!({ "type": "hello", "protocol": 2, "roles": ["codec"] });
        write_message(&mut bytes, &Message::new(hello)).unwrap();
        std::fs::write(&answer, bytes).unwrap();
        let program = directory.path().join("plugin.sh");
        std::fs::write(
            &program,
            format!("#!/bin/sh\ncat '{}'\nexec sleep 30\n", answer.display()),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let result = MediaProcess::start_with(
            &program,
            directory.path(),
            &PluginKillSwitch::default(),
            FrameTransport::Pipe,
            ProviderUse::Decode,
        );
        let Err(MediaError::CodecUnavailable(message)) = result else {
            panic!("a plugin with the retired codec role was accepted as a media plugin");
        };
        assert!(message.contains("is not a media plugin"), "{message}");
    }
}
