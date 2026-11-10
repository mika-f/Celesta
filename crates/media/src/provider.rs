//! Reading and writing media files through a media plugin.
//!
//! Celesta links no codec library. Probing, decoding, encoding, and muxing
//! go through a [`MediaProvider`]: a media plugin's process
//! ([`MediaProcess`](crate::MediaProcess), speaking
//! [`protocol`](crate::protocol)), or, in tests, a provider in the same
//! process. The application lists the media plugins once with
//! [`set_registry`](crate::set_registry); each
//! [`MediaBackend`](crate::MediaBackend) starts its own instance of a plugin
//! when it first needs one, and routes each file to one of the plugins'
//! decoders. See the media components spec
//! (`docs/superpowers/specs/2026-11-08-media-components-design.md`) and
//! `docs/handoff/2026-11-08-media-encoders.md`.

use crate::error::MediaError;
use crate::settings::{SettingField, Settings};
use crate::types::{AudioBuffer, MediaProbe, VideoFrame};
use celesta_composition::Rational;
use celesta_plugin::shared::RingLayout;
use celesta_plugin_host::PluginKillSwitch;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What a media plugin does. Each request is answered before the next is
/// asked, except encode and write requests sent through its
/// [`Self::pipeline`], several in flight at once. Every path is absolute
/// and local.
///
/// A plugin answers [`Self::describe`], and serves each kind of component
/// it has through the accessor of that kind ([`Self::video_encoder`], …);
/// the others stay `None`.
pub trait MediaProvider: Send {
    /// Every component the plugin has, whether it is available, and its
    /// settings.
    fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError>;

    fn decoder(&mut self) -> Option<&mut dyn Decoder> {
        None
    }

    fn video_encoder(&mut self) -> Option<&mut dyn VideoEncoder> {
        None
    }

    fn audio_encoder(&mut self) -> Option<&mut dyn AudioEncoder> {
        None
    }

    fn muxer(&mut self) -> Option<&mut dyn Muxer> {
        None
    }

    /// Its encoders and muxers with several requests in flight, when it can
    /// take them so; `None` asks one request at a time.
    fn pipeline(&mut self) -> Option<&mut dyn Pipeline> {
        None
    }
}

/// How deep a [`Pipeline`] without a ring of frame slots goes: frames then
/// travel in each request's payload. Fewer for frames so large that this
/// many would exceed the ring's ceiling ([`crate::pipe_depth`]).
pub const PIPE_WINDOW: usize = 3;

/// A provider's encode and write requests sent without waiting, each
/// answered by [`Self::receive`] in the order sent (the oldest first).
///
/// A caller keeps at most [`Self::depth`] requests in flight, and drains
/// them all before anything else is asked of the provider (`describe`, an
/// `open`, a `finish`, an `abort`, or one of the encoders' and muxers'
/// one-at-a-time methods), which is refused while any is in flight.
pub trait Pipeline {
    /// How many requests may be in flight: the ring's slots, or
    /// [`PIPE_WINDOW`] without a ring ([`crate::pipe_depth`] for large
    /// frames).
    fn depth(&self) -> usize;

    /// How many requests are sent and not yet received.
    fn in_flight(&self) -> usize;

    /// Sends every video frame in its request's payload, never through the
    /// ring of shared memory: the video encoder about to open does not
    /// describe the `sharedMemory` transport. Asked before
    /// [`VideoEncoder::open_video`], which then declares no ring; a
    /// pipeline without a ring has nothing to decline.
    fn decline_ring(&mut self) {}

    /// Whether video frames go through a ring of shared memory: the plugin
    /// accepted one, [`VideoEncoder::open_video`] declared it, and the
    /// opened input's frame fits a slot.
    fn frames_in_ring(&self) -> bool {
        false
    }

    /// Sends [`VideoEncoder::encode_video`]'s request; its answer is an
    /// [`Answered::VideoFrame`].
    fn send_video_frame(
        &mut self,
        session: SessionId,
        pts: i64,
        frame: Vec<u8>,
    ) -> Result<(), MediaError>;

    /// Sends [`AudioEncoder::encode_audio`]'s request; its answer is an
    /// [`Answered::Audio`].
    fn send_audio(
        &mut self,
        session: SessionId,
        pts: i64,
        samples: &[f32],
    ) -> Result<(), MediaError>;

    /// Sends [`Muxer::write_packets`]'s request; its answer is
    /// [`Answered::Packets`].
    fn send_packets(
        &mut self,
        session: SessionId,
        packets: &[TrackPacket],
    ) -> Result<(), MediaError>;

    /// Waits for the answer to the oldest request in flight;
    /// `MediaError::Codec("no request is in flight")` when there is none.
    fn receive(&mut self) -> Result<Answered, MediaError>;
}

/// The answer to a request a [`Pipeline`] sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answered {
    /// To [`Pipeline::send_video_frame`].
    VideoFrame(Encoded),
    /// To [`Pipeline::send_audio`].
    Audio(Encoded),
    /// To [`Pipeline::send_packets`].
    Packets,
}

/// A plugin's decoder components: `decoder` is the id of the one that is to
/// serve the request. Every path is absolute and local.
pub trait Decoder {
    fn probe(&mut self, decoder: &str, path: &Path) -> Result<MediaProbe, MediaError>;

    /// The video frame shown at `request.time` seconds into the file, as
    /// tightly packed straight-alpha RGBA8 at the probed (displayed) size.
    fn decode_video(
        &mut self,
        decoder: &str,
        request: &VideoRequest<'_>,
    ) -> Result<VideoFrame, MediaError>;

    /// The file's first audio stream, whole, as interleaved `f32` samples
    /// at `sample_rate` with `channels` channels (1 or 2); no samples when
    /// it has no audio.
    fn decode_audio(
        &mut self,
        decoder: &str,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError>;
}

/// A plugin's video encoder components, by session.
///
/// A plugin's [`SessionId`]s are unique within its process across every
/// kind: video encoders, audio encoders, and muxers count from one counter.
/// `abortEncoder` names only a session, and reaches both
/// [`VideoEncoder::abort_video`] and [`AudioEncoder::abort_audio`], so each
/// must ignore a session it does not own.
pub trait VideoEncoder {
    /// Opens `request.component` for these settings, choosing one of
    /// `request.offered` as its input.
    fn open_video(
        &mut self,
        request: &VideoEncoderRequest<'_>,
    ) -> Result<VideoEncoderOpened, MediaError>;

    /// Encodes one frame of the session's input format, shown at `pts` in
    /// the session's timescale, and gives back the packets that are ready.
    fn encode_video(
        &mut self,
        session: SessionId,
        pts: i64,
        frame: Vec<u8>,
    ) -> Result<Encoded, MediaError>;

    /// Flushes the encoder: its last packets. The session ends.
    fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError>;

    /// Ends the session, if it is one of this kind's, with no more packets.
    fn abort_video(&mut self, session: SessionId);
}

/// A plugin's audio encoder components, by session; its sessions share the
/// plugin's one counter (see [`VideoEncoder`]).
pub trait AudioEncoder {
    /// Opens `request.component`, which answers the sample rate and channel
    /// count it takes (Celesta converts the mix to them) and its delay.
    fn open_audio(
        &mut self,
        request: &AudioEncoderRequest<'_>,
    ) -> Result<AudioEncoderOpened, MediaError>;

    /// Encodes interleaved samples in the session's format, the first of
    /// them at `pts` in sample frames; `samples.len() / channels` frames.
    fn encode_audio(
        &mut self,
        session: SessionId,
        pts: i64,
        samples: &[f32],
    ) -> Result<Encoded, MediaError>;

    /// Flushes the encoder: its last packets. The session ends.
    fn finish_audio(&mut self, session: SessionId) -> Result<Encoded, MediaError>;

    /// Ends the session, if it is one of this kind's, with no more packets.
    fn abort_audio(&mut self, session: SessionId);
}

/// A plugin's muxer components, by session; its sessions share the
/// plugin's one counter (see [`VideoEncoder`]).
pub trait Muxer {
    /// Starts writing `request.output` with `request.tracks`.
    fn open_muxer(&mut self, request: &MuxerRequest<'_>) -> Result<SessionId, MediaError>;

    /// Writes packets, in the order given, each to its track.
    fn write_packets(
        &mut self,
        session: SessionId,
        packets: &[TrackPacket],
    ) -> Result<(), MediaError>;

    /// Finishes the output: the files written.
    fn finish_muxer(&mut self, session: SessionId) -> Result<Vec<PathBuf>, MediaError>;

    /// Stops writing: the files it left, for Celesta to remove.
    fn abort_muxer(&mut self, session: SessionId) -> Vec<PathBuf>;
}

/// A frame request. With `sequential`, requests of the same session are
/// expected to move forward at about its frame rate, and the codec may keep
/// a decode run open between them. A session that stops asking may be
/// closed at any time; a later request for it then seeks again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoRequest<'a> {
    pub path: &'a Path,
    pub time: f64,
    pub sequential: Option<Sequential<'a>>,
}

/// Whether two times, in seconds, are the same: the tolerance codecs use to
/// match a decoded frame to a [`VideoRequest::time`], 0.1 µs or a part in
/// 10⁹ of the larger, so a time a rounding error past a frame still lands on
/// that frame.
pub fn timestamps_match(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-7_f64.max(left.abs().max(right.abs()) * 1e-9)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sequential<'a> {
    /// Which run of requests this is: a renderer asks for each layer
    /// showing the file on its own.
    pub session: &'a str,
    /// The spacing expected between requests until the session has shown
    /// its own.
    pub frame_rate: Rational,
}

/// The layout of the frames pushed to an encoder. On the wire, and through
/// `FromStr` and `Display`, its lowercase name: `rgba8`, `rgba16`,
/// `yuv420p`, `yuv420p10`, `nv12`, `p010`.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumString,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum PixelFormat {
    /// RGBA, 8 bits a channel.
    Rgba8,
    /// RGBA, 16-bit little-endian channels.
    Rgba16,
    /// Planar BT.709 limited-range Y, U, V with chroma halved both ways.
    Yuv420p,
    /// [`PixelFormat::Yuv420p`] with 10 bits in 16-bit little-endian samples.
    Yuv420p10,
    /// A BT.709 limited-range Y plane, then U and V interleaved in one
    /// plane, chroma halved both ways.
    Nv12,
    /// [`PixelFormat::Nv12`] with 10 bits in the high bits of 16-bit
    /// little-endian samples.
    P010,
}

impl PixelFormat {
    /// The formats Celesta can produce, which size an export's ring.
    pub const OFFERABLE: [PixelFormat; 6] = [
        PixelFormat::Yuv420p,
        PixelFormat::Yuv420p10,
        PixelFormat::Nv12,
        PixelFormat::P010,
        PixelFormat::Rgba16,
        PixelFormat::Rgba8,
    ];

    /// The bytes of a `width` x `height` frame in this format: the planes
    /// back to back, chroma rounded up to whole samples. `usize::MAX` when
    /// they do not fit a `usize` (a size no ring or window can hold): the
    /// arithmetic saturates, as the size may come from a project before
    /// its dimensions are validated.
    pub fn frame_bytes(self, width: u32, height: u32) -> usize {
        let (width, height) = (width as usize, height as usize);
        let luma = width.saturating_mul(height);
        let chroma = width
            .div_ceil(2)
            .saturating_mul(height.div_ceil(2))
            .saturating_mul(2);
        match self {
            Self::Rgba8 => luma.saturating_mul(4),
            Self::Rgba16 => luma.saturating_mul(8),
            Self::Yuv420p | Self::Nv12 => luma.saturating_add(chroma),
            Self::Yuv420p10 | Self::P010 => luma.saturating_add(chroma).saturating_mul(2),
        }
    }
}

/// An open encoder or muxer, by the plugin's own number: unique within its
/// process across every kind (see [`VideoEncoder`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u64);

/// A video's colour, as `openVideoEncoder` and a video [`Track`] carry it:
/// the names of its primaries, transfer, matrix, and range.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Color {
    pub primaries: String,
    pub transfer: String,
    pub matrix: String,
    pub range: String,
}

impl Color {
    /// BT.709 primaries, transfer, and matrix in limited range: what every
    /// export converts with and tags.
    pub fn bt709_limited() -> Self {
        Self {
            primaries: "bt709".to_owned(),
            transfer: "bt709".to_owned(),
            matrix: "bt709".to_owned(),
            range: "limited".to_owned(),
        }
    }
}

/// What a muxer needs to know of an encoded track: its codec string
/// (`avc1.640028`, as WebCodecs names it) and decoder configuration (avcC,
/// AudioSpecificConfig, OpusHead, …), base64 on the wire.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackConfig {
    pub codec: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "base64_bytes"
    )]
    pub description: Option<Vec<u8>>,
}

/// Optional bytes as a base64 string.
mod base64_bytes {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub(super) fn serialize<S: Serializer>(
        bytes: &Option<Vec<u8>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(bytes) => serializer.serialize_str(&STANDARD.encode(bytes)),
            None => serializer.serialize_none(),
        }
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Vec<u8>>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|text| STANDARD.decode(text).map_err(D::Error::custom))
            .transpose()
    }
}

/// Side data a muxer stores beside a packet, as Matroska's BlockAdditions:
/// VP9's alpha plane is `id` 1.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SideData {
    pub id: u32,
    pub bytes: Vec<u8>,
}

/// One encoded packet; times in its track's timescale.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Packet {
    pub pts: i64,
    pub dts: i64,
    pub duration: i64,
    pub key: bool,
    pub bytes: Vec<u8>,
    pub additional: Vec<SideData>,
}

/// What an encoder answered: the packets that are ready, which may be none,
/// and the track's config once it is known.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Encoded {
    pub packets: Vec<Packet>,
    pub config: Option<TrackConfig>,
}

/// What [`VideoEncoder::open_video`] opens: `timescale` is the frame rate's
/// numerator, so a frame lasts its denominator. `offered` is best first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoEncoderRequest<'a> {
    pub component: &'a str,
    pub settings: &'a Settings,
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub timescale: u32,
    pub offered: &'a [PixelFormat],
    pub color: &'a Color,
}

/// A video encoder's session, the input format it chose of those offered,
/// and whether the encoded video keeps the frame's alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoEncoderOpened {
    pub session: SessionId,
    pub input: PixelFormat,
    pub alpha: bool,
}

/// What [`AudioEncoder::open_audio`] opens: the mix's rate and channels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioEncoderRequest<'a> {
    pub component: &'a str,
    pub settings: &'a Settings,
    pub sample_rate: u32,
    pub channels: u16,
}

/// An audio encoder's session, the rate and channel count it takes, and
/// its priming delay in sample frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioEncoderOpened {
    pub session: SessionId,
    pub sample_rate: u32,
    pub channels: u16,
    pub delay: u64,
}

/// One track a muxer writes, with the config its encoder gave.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Track {
    Video {
        config: TrackConfig,
        timescale: u32,
        width: u32,
        height: u32,
        frame_rate: Rational,
        color: Color,
        alpha: bool,
    },
    Audio {
        config: TrackConfig,
        timescale: u32,
        sample_rate: u32,
        channels: u16,
        /// The encoder's delay in sample frames, which the muxer hides.
        delay: u64,
    },
}

/// What [`Muxer::open_muxer`] writes. `leading_audio` counts sample frames
/// before 0 to hide beyond the audio encoder's delay (the loop pre-roll).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MuxerRequest<'a> {
    pub component: &'a str,
    pub settings: &'a Settings,
    pub output: &'a Path,
    pub tracks: &'a [Track],
    pub leading_audio: u64,
}

/// A packet for a muxer, with the index of its track in
/// [`MuxerRequest::tracks`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackPacket {
    pub track: usize,
    pub packet: Packet,
}

/// What a component is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentKind {
    Decoder,
    VideoEncoder,
    AudioEncoder,
    Muxer,
}

/// The sample rates an audio encoder takes: on the wire `"any"` or a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SampleRates {
    Any,
    Only(Vec<u32>),
}

impl Serialize for SampleRates {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Any => serializer.serialize_str("any"),
            Self::Only(rates) => rates.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for SampleRates {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Text(String),
            Only(Vec<u32>),
        }
        match Wire::deserialize(deserializer)? {
            Wire::Text(text) if text == "any" => Ok(Self::Any),
            Wire::Text(text) => Err(serde::de::Error::custom(format!(
                "sample rates are \"any\" or a list, not {text:?}"
            ))),
            Wire::Only(rates) => Ok(Self::Only(rates)),
        }
    }
}

/// What `describe` says of one component. One that is not `available` has
/// a `reason` and nothing else; the format, transport, rate, and channel
/// lists are those of video and audio encoders.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentDescription {
    pub kind: ComponentKind,
    pub id: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<SettingField>,
    /// Every format it can take under any settings, best first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_formats: Vec<PixelFormat>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transports: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rates: Option<SampleRates>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<u16>,
}

impl ComponentDescription {
    /// `kind` `id` as unavailable for `reason`, with nothing else.
    pub fn unavailable(kind: ComponentKind, id: &str, reason: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.to_owned(),
            available: false,
            reason: Some(reason.into()),
            settings: Vec::new(),
            input_formats: Vec::new(),
            transports: Vec::new(),
            sample_rates: None,
            channels: Vec::new(),
        }
    }
}

/// What an instance of a media plugin is started for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderUse {
    /// A [`crate::MediaBackend`]'s probes and decodes.
    Decode,
    /// Only to answer `describe`, then stopped
    /// ([`crate::MediaRegistry::describe`]).
    Describe,
    /// An export's encoders and muxers, with the ring of slots its video
    /// frames are to go through ([`crate::ring_layout`]); `None` sends them
    /// in each request's payload.
    Export { ring: Option<RingLayout> },
}

/// Starts an instance of a media plugin for a [`ProviderUse`]: each time a
/// [`crate::MediaBackend`] first needs that plugin, to describe it, or for
/// an export. A plugin that runs in a process of its own registers it with
/// the switch it is given, before it says `hello`, so killing the switch
/// also ends a slow start; a provider in this process ignores it.
pub type ProviderFactory = Arc<
    dyn Fn(&PluginKillSwitch, ProviderUse) -> Result<Box<dyn MediaProvider>, MediaError>
        + Send
        + Sync,
>;
