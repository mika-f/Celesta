//! The decoder, on Media Foundation's source reader
//! (`MFCreateSourceReaderFromURL`, advanced video processing on): probes by
//! path, video frames as the FFmpeg plugin's decoder gives them, and a
//! file's first audio stream. Its sequential sessions follow P5's
//! (`plugins/avfoundation/src/decoder.rs`), with P5's constants.
//!
//! - Video comes out as `MFVideoFormat_RGB32`, made opaque RGBA. The
//!   reader's video processor converts from the stream's colour and turns
//!   the frame upright by its `MF_MT_VIDEO_ROTATION` (Task 1), so frames are
//!   at the displayed size; the native type's rotation only gives the
//!   probe's size.
//! - A frame is the one shown at the requested time (`timestamps_match`),
//!   the last one past the end. A request seeks (`SetCurrentPosition`, to
//!   the key frame before) and reads forward to it; a sequential session
//!   keeps its reader while its requests move forward by up to
//!   [`MAX_FORWARD_WALK_SECONDS`].
//! - Audio comes out as `MFAudioFormat_Float` at the file's rate and
//!   channels, then is mixed to the channels asked and resampled.
//! - The reader ignores MP4 and MOV edit lists (Task 1), so the decoder
//!   applies the first video and audio tracks' itself (`edit_list`): video
//!   times are mapped into the media's, and audio drops its leading
//!   `media_time`, gains its empty edits as silence, and keeps at most its
//!   edit's duration.
//!
//! Files are not held: the edit lists are read, and the file closed, before
//! a reader opens, and every reader is released before the request is
//! answered, except a sequential session's, which goes when the session
//! closes.

use crate::edit_list::{TrackEdit, track_edits};
use crate::mf::mft::{check, sample_bytes};
use crate::names::{codec_name, matrix_name, pcm_name, primaries_name, transfer_name};
use celesta_composition::{Rational, Time};
use celesta_media::{
    AudioBuffer, AudioStream, MediaError, MediaProbe, VideoColor, VideoFrame, VideoRequest,
    VideoStream, frame_byte_len, resample_audio, timestamps_match, video_seconds,
};
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use windows::Win32::Media::MediaFoundation::{
    IMFMediaType, IMFSample, IMFSourceReader, MF_E_INVALIDSTREAMNUMBER,
    MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
    MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
    MF_MT_TRANSFER_FUNCTION, MF_MT_VIDEO_PRIMARIES, MF_MT_VIDEO_ROTATION, MF_MT_YUV_MATRIX,
    MF_PD_DURATION, MF_SOURCE_READER_ALL_STREAMS,
    MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, MF_SOURCE_READER_MEDIASOURCE,
    MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED, MF_SOURCE_READERF_ENDOFSTREAM,
    MF_SOURCE_READERF_ERROR, MFAudioFormat_Float, MFAudioFormat_PCM, MFCreateAttributes,
    MFCreateMediaType, MFCreateSourceReaderFromURL, MFMediaType_Audio, MFMediaType_Video,
    MFVideoFormat_RGB32,
};
use windows::Win32::System::Com::StructuredStorage::{
    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0, PropVariantClear,
};
use windows::Win32::System::Variant::{VT_I8, VT_UI8};
use windows_core::{GUID, HSTRING};

/// How far past the last request a sequential session's next may be while
/// its reader is walked forward to it; past it, the reader seeks.
const MAX_FORWARD_WALK_SECONDS: f64 = 1.0;

/// How many frames a session may go unasked before it is closed, counted
/// as the FFmpeg plugin counts them: this many times the sessions open.
const IDLE_SESSION_FRAMES: u64 = 30;

/// The most sessions held open at once; past it, the one asked least
/// recently is closed.
const MAX_VIDEO_SESSIONS: usize = 16;

/// Media Foundation's time unit, 100 ns, in a second.
const HNS_PER_SECOND: f64 = 10_000_000.0;

/// The tail every subtype GUID made from a FourCC or a `WAVE_FORMAT` tag
/// has: `{fourcc}-0000-0010-8000-00AA00389B71`.
const FOURCC_TAIL: (u16, u16, [u8; 8]) = (
    0x0000,
    0x0010,
    [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71],
);

/// What a track's edit list does, in seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Edit {
    /// Where the presentation starts in the media: `media_time / timescale`.
    media_start: f64,
    /// How late the track starts.
    start_delay: f64,
    /// How much of the media is shown, when the edit says.
    duration: Option<f64>,
    /// Where the media ends, by `mdhd`'s duration. The edit's duration is in
    /// the movie's coarser units (FFmpeg writes milliseconds), so it can run
    /// past an audio track's last sample; the audio stops at both. (A video
    /// track's media is shown shifted by its composition offsets, so its end
    /// is not this.)
    media_end: Option<f64>,
}

impl Edit {
    fn of(track: Option<&TrackEdit>) -> Self {
        track.map_or_else(Self::default, |track| {
            let timescale = f64::from(track.timescale.max(1));
            Self {
                media_start: track.media_time as f64 / timescale,
                start_delay: track.start_delay,
                duration: track.duration,
                media_end: track
                    .media_duration
                    .map(|duration| duration as f64 / timescale),
            }
        })
    }

    /// The media time shown at presentation time `time`; a time before the
    /// track starts shows its first frame.
    fn media_seconds(self, time: f64) -> f64 {
        (time - self.start_delay).max(0.0) + self.media_start
    }
}

/// The first video stream and how it is shown.
struct VideoTrack {
    /// The reader's stream index.
    stream: u32,
    edit: Edit,
}

/// The first audio stream.
struct AudioTrack {
    stream: u32,
    edit: Edit,
}

/// An open reader of a file, its probe, and the streams it is decoded from.
struct Source {
    reader: IMFSourceReader,
    probe: MediaProbe,
    video: Option<VideoTrack>,
    audio: Option<AudioTrack>,
}

/// Fails with [`MediaError::NotFound`] for a missing file, which the reader
/// would only say it cannot open.
fn ensure_exists(path: &Path) -> Result<(), MediaError> {
    match path.try_exists() {
        Ok(false) => Err(MediaError::NotFound(path.to_owned())),
        _ => Ok(()),
    }
}

fn unsupported(path: &Path) -> MediaError {
    MediaError::Unsupported {
        path: path.to_owned(),
        tried: Vec::new(),
    }
}

/// `key` of `media_type`, when it has it.
fn u32_of(media_type: &IMFMediaType, key: &GUID) -> Option<u32> {
    // SAFETY: a plain COM call on a live interface.
    unsafe { media_type.GetUINT32(key) }.ok()
}

/// `key` of `media_type` as its two halves, the upper first.
fn pair_of(media_type: &IMFMediaType, key: &GUID) -> Option<(u32, u32)> {
    // SAFETY: a plain COM call on a live interface.
    let packed = unsafe { media_type.GetUINT64(key) }.ok()?;
    Some(((packed >> 32) as u32, packed as u32))
}

fn guid_of(media_type: &IMFMediaType, key: &GUID) -> Option<GUID> {
    // SAFETY: a plain COM call on a live interface.
    unsafe { media_type.GetGUID(key) }.ok()
}

/// FFmpeg's name for the codec of `media_type`'s subtype; none for a
/// subtype not made from a FourCC.
fn subtype_codec(media_type: &IMFMediaType) -> Option<String> {
    let subtype = guid_of(media_type, &MF_MT_SUBTYPE)?;
    if (subtype.data2, subtype.data3, subtype.data4) != FOURCC_TAIL {
        return None;
    }
    if subtype == MFAudioFormat_PCM || subtype == MFAudioFormat_Float {
        let bits = u32_of(media_type, &MF_MT_AUDIO_BITS_PER_SAMPLE);
        if let Some(name) = bits.and_then(|bits| pcm_name(bits, subtype == MFAudioFormat_Float)) {
            return Some(name.to_owned());
        }
    }
    Some(codec_name(subtype.data1))
}

/// `numerator / denominator` in lowest terms; none when either is 0.
fn reduced(numerator: u32, denominator: u32) -> Option<Rational> {
    if numerator == 0 || denominator == 0 {
        return None;
    }
    let (mut left, mut right) = (numerator, denominator);
    while right != 0 {
        (left, right) = (right, left % right);
    }
    Some(Rational::new(numerator / left, denominator / left))
}

/// `seconds` as a [`Time`], to the microsecond; none unless positive.
fn seconds_time(seconds: f64) -> Option<Time> {
    let micros = (seconds * f64::from(Time::MICROSECONDS)).round();
    (micros >= 1.0 && micros < i64::MAX as f64)
        .then(|| Time::new(micros as i64, Time::MICROSECONDS).reduced())
}

/// The native type of `reader`'s stream `stream`: `Ok(None)` past the last
/// stream, `Err(())` for a stream without one.
fn native_type(reader: &IMFSourceReader, stream: u32) -> Result<Option<IMFMediaType>, ()> {
    // SAFETY: a plain COM call on a live interface.
    match unsafe { reader.GetNativeMediaType(stream, 0) } {
        Ok(native) => Ok(Some(native)),
        Err(error) if error.code() == MF_E_INVALIDSTREAMNUMBER => Ok(None),
        Err(_) => Err(()),
    }
}

/// The video stream `stream` of the reader, of type `native`, as the probe
/// gives it: its size the displayed one.
fn video_stream(stream: u32, native: &IMFMediaType, edit: Edit) -> Option<VideoStream> {
    let (width, height) = pair_of(native, &MF_MT_FRAME_SIZE)?;
    let rotation = u32_of(native, &MF_MT_VIDEO_ROTATION).unwrap_or(0);
    let (width, height) = if rotation % 180 == 90 {
        (height, width)
    } else {
        (width, height)
    };
    let name = |key: &GUID, name: fn(u32) -> Option<&'static str>| {
        u32_of(native, key).and_then(name).map(str::to_owned)
    };
    Some(VideoStream {
        index: stream,
        codec: subtype_codec(native),
        width,
        height,
        frame_rate: pair_of(native, &MF_MT_FRAME_RATE)
            .and_then(|(numerator, denominator)| reduced(numerator, denominator)),
        duration: edit.duration.and_then(seconds_time),
        color: VideoColor {
            primaries: name(&MF_MT_VIDEO_PRIMARIES, primaries_name),
            transfer: name(&MF_MT_TRANSFER_FUNCTION, transfer_name),
            matrix: name(&MF_MT_YUV_MATRIX, matrix_name),
        },
    })
}

/// The audio stream `stream` of the reader, of type `native`.
fn audio_stream(stream: u32, native: &IMFMediaType, edit: Edit) -> AudioStream {
    AudioStream {
        index: stream,
        codec: subtype_codec(native),
        sample_rate: u32_of(native, &MF_MT_AUDIO_SAMPLES_PER_SECOND).filter(|rate| *rate > 0),
        channels: u32_of(native, &MF_MT_AUDIO_NUM_CHANNELS)
            .and_then(|channels| u16::try_from(channels).ok())
            .filter(|channels| *channels > 0),
        duration: edit.duration.and_then(seconds_time),
    }
}

/// The presentation's duration (`MF_PD_DURATION`).
fn presentation_duration(reader: &IMFSourceReader) -> Option<Time> {
    // SAFETY: plain COM calls on a live interface; a `VT_UI8` value is read
    // as one, and the variant is cleared after.
    unsafe {
        let mut value = reader
            .GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
            .ok()?;
        let inner = &value.Anonymous.Anonymous;
        let hns = (inner.vt == VT_UI8).then(|| inner.Anonymous.uhVal);
        let _ = PropVariantClear(&mut value);
        let hns = i64::try_from(hns?).ok().filter(|hns| *hns > 0)?;
        Some(Time::new(hns, 10_000_000).reduced())
    }
}

/// Opens `path` with a source reader: what it holds. The edit lists are
/// read first, and the file closed, before the reader opens it.
fn open_source(path: &Path) -> Result<Source, MediaError> {
    ensure_exists(path)?;
    let edits = std::fs::File::open(path)
        .ok()
        .and_then(|mut file| track_edits(&mut file).ok())
        .unwrap_or_default();
    let of_handler = |handler: &[u8; 4]| -> Vec<Edit> {
        edits
            .iter()
            .filter(|edit| &edit.handler == handler)
            .map(|edit| Edit::of(Some(edit)))
            .collect()
    };
    let (video_edits, audio_edits) = (of_handler(b"vide"), of_handler(b"soun"));
    let full = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    // SAFETY: plain COM calls on live interfaces; the attribute store is
    // made before it is set and given.
    let reader = unsafe {
        let mut attributes = None;
        check("MFCreateAttributes", MFCreateAttributes(&mut attributes, 1))?;
        let attributes = attributes
            .ok_or_else(|| MediaError::Codec("MFCreateAttributes gave nothing".to_owned()))?;
        check(
            "SetUINT32(MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING)",
            attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1),
        )?;
        MFCreateSourceReaderFromURL(&HSTRING::from(full.as_path()), &attributes)
    }
    .map_err(|_| unsupported(path))?;

    let mut probe = MediaProbe {
        duration: presentation_duration(&reader),
        video: None,
        audio: Vec::new(),
    };
    let (mut video, mut audio) = (None, None);
    let mut stream = 0;
    loop {
        let native = match native_type(&reader, stream) {
            Ok(Some(native)) => native,
            Ok(None) => break,
            Err(()) => {
                stream += 1;
                continue;
            }
        };
        let major = guid_of(&native, &MF_MT_MAJOR_TYPE);
        if major == Some(MFMediaType_Video) {
            let edit = video_edits.first().copied().unwrap_or_default();
            if video.is_none()
                && let Some(found) = video_stream(stream, &native, edit)
            {
                probe.video = Some(found);
                video = Some(VideoTrack { stream, edit });
            }
        } else if major == Some(MFMediaType_Audio) {
            let edit = audio_edits
                .get(probe.audio.len())
                .copied()
                .unwrap_or_default();
            probe.audio.push(audio_stream(stream, &native, edit));
            audio.get_or_insert(AudioTrack { stream, edit });
        }
        stream += 1;
    }
    if probe.video.is_none() && probe.audio.is_empty() {
        return Err(unsupported(path));
    }
    Ok(Source {
        reader,
        probe,
        video,
        audio,
    })
}

/// Selects only `stream` of `reader` and sets its output to `subtype` of
/// `major`: the type it then gives.
fn select_output(
    reader: &IMFSourceReader,
    stream: u32,
    major: GUID,
    subtype: GUID,
) -> Result<IMFMediaType, MediaError> {
    // SAFETY: plain COM calls on live interfaces; the partial type is made
    // before it is given.
    unsafe {
        check(
            "IMFSourceReader::SetStreamSelection",
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false),
        )?;
        check(
            "IMFSourceReader::SetStreamSelection",
            reader.SetStreamSelection(stream, true),
        )?;
        let partial = check("MFCreateMediaType", MFCreateMediaType())?;
        check(
            "SetGUID(MF_MT_MAJOR_TYPE)",
            partial.SetGUID(&MF_MT_MAJOR_TYPE, &major),
        )?;
        check(
            "SetGUID(MF_MT_SUBTYPE)",
            partial.SetGUID(&MF_MT_SUBTYPE, &subtype),
        )?;
        check(
            "IMFSourceReader::SetCurrentMediaType",
            reader.SetCurrentMediaType(stream, None, &partial),
        )?;
        check(
            "IMFSourceReader::GetCurrentMediaType",
            reader.GetCurrentMediaType(stream),
        )
    }
}

/// One `ReadSample` of `stream`: the sample (none for a gap) at its time in
/// hns, and whether the stream ended; whether the output type changed.
struct Read {
    sample: Option<(IMFSample, i64)>,
    ended: bool,
    changed: bool,
}

fn read_sample(reader: &IMFSourceReader, stream: u32, doing: &str) -> Result<Read, MediaError> {
    let mut flags = 0u32;
    let mut time = 0i64;
    let mut sample = None;
    // SAFETY: each out pointer is a live local of the type the call writes.
    check(&format!("IMFSourceReader::ReadSample ({doing})"), unsafe {
        reader.ReadSample(
            stream,
            0,
            None,
            Some(&mut flags),
            Some(&mut time),
            Some(&mut sample),
        )
    })?;
    let flag = |bit: i32| flags & bit as u32 != 0;
    if flag(MF_SOURCE_READERF_ERROR.0) {
        return Err(MediaError::Codec(format!(
            "the source reader failed to {doing}"
        )));
    }
    Ok(Read {
        sample: sample.map(|sample| (sample, time)),
        ended: flag(MF_SOURCE_READERF_ENDOFSTREAM.0),
        changed: flag(MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0),
    })
}

/// Seeks `reader` to `seconds` (to the key frame at or before it), or to
/// just inside `duration`, the presentation's: the reader refuses a
/// position past it (`MF_E_INVALID_POSITION`), and a track's media can run
/// past it by its edit's `media_time`. Frames past the position are then
/// read forward to.
fn seek(reader: &IMFSourceReader, seconds: f64, duration: Option<Time>) -> Result<(), MediaError> {
    // The tick at or before it, or the one a rounding error past it.
    let tolerance = 1e-7_f64.max(seconds.abs() * 1e-9);
    let mut hns = ((seconds + tolerance) * HNS_PER_SECOND).floor().max(0.0) as i64;
    if let Some(end) = duration.and_then(|duration| duration.as_seconds().ok()) {
        let last = (end * HNS_PER_SECOND).floor() as i64 - 1;
        hns = hns.min(last.max(0));
    }
    let position = PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_I8,
                Anonymous: PROPVARIANT_0_0_0 { hVal: hns },
                ..Default::default()
            }),
        },
    };
    // SAFETY: a `VT_I8` variant in hns, the time format `GUID_NULL` asks;
    // it holds nothing to clear.
    check("IMFSourceReader::SetCurrentPosition", unsafe {
        reader.SetCurrentPosition(&GUID::zeroed(), &position)
    })
}

/// A decoded frame, shown from `seconds` of media time.
struct Shown {
    seconds: f64,
    sample: IMFSample,
    /// The frame, once converted.
    frame: Option<VideoFrame>,
}

/// The size and row order of the frames a reader gives.
#[derive(Clone, Copy)]
struct Layout {
    width: u32,
    height: u32,
    bottom_up: bool,
}

impl Layout {
    fn of(current: &IMFMediaType) -> Result<Self, MediaError> {
        let (width, height) = pair_of(current, &MF_MT_FRAME_SIZE).ok_or_else(|| {
            MediaError::Codec("the source reader's RGB32 type has no frame size".to_owned())
        })?;
        let stride = u32_of(current, &MF_MT_DEFAULT_STRIDE).map(|stride| stride as i32);
        Ok(Self {
            width,
            height,
            bottom_up: stride.is_some_and(|stride| stride < 0),
        })
    }
}

/// An open reader of a video stream, walked forward through its frames.
struct Run {
    source: Source,
    stream: u32,
    edit: Edit,
    layout: Layout,
    /// The last frame shown by the last request.
    current: Option<Shown>,
    /// The frame read after it, not shown yet.
    next: Option<Shown>,
    /// Whether the reader has given its last frame.
    ended: bool,
}

impl Run {
    /// A reader of `path`'s video, its output set to RGB32.
    fn open(path: &Path) -> Result<Self, MediaError> {
        let source = open_source(path)?;
        let Some(video) = &source.video else {
            return Err(MediaError::NoVideoStream(path.to_owned()));
        };
        let (stream, edit) = (video.stream, video.edit);
        let current = select_output(
            &source.reader,
            stream,
            MFMediaType_Video,
            MFVideoFormat_RGB32,
        )?;
        let layout = Layout::of(&current)?;
        Ok(Self {
            source,
            stream,
            edit,
            layout,
            current: None,
            next: None,
            ended: false,
        })
    }

    /// Seeks to `media` seconds of media time, forgetting what was read.
    fn seek(&mut self, media: f64) -> Result<(), MediaError> {
        seek(&self.source.reader, media, self.source.probe.duration)?;
        self.current = None;
        self.next = None;
        self.ended = false;
        Ok(())
    }

    /// The next frame the reader gives, none past the last.
    fn read(&mut self) -> Result<Option<Shown>, MediaError> {
        while !self.ended {
            let read = read_sample(&self.source.reader, self.stream, "decode the video")?;
            self.ended = read.ended;
            if read.changed {
                // SAFETY: a plain COM call on a live interface.
                let current = check("IMFSourceReader::GetCurrentMediaType", unsafe {
                    self.source.reader.GetCurrentMediaType(self.stream)
                })?;
                self.layout = Layout::of(&current)?;
            }
            if let Some((sample, time)) = read.sample {
                return Ok(Some(Shown {
                    seconds: time as f64 / HNS_PER_SECOND,
                    sample,
                    frame: None,
                }));
            }
        }
        Ok(None)
    }

    /// Walks the reader forward to the last frame shown by `media` seconds
    /// of media time; the first frame when the reader's first is later.
    fn walk(&mut self, media: f64) -> Result<(), MediaError> {
        loop {
            if self.next.is_none() && !self.ended {
                self.next = self.read()?;
            }
            match &self.next {
                Some(next) if self.current.is_none() || shown_by(next.seconds, media) => {
                    self.current = self.next.take();
                }
                _ => return Ok(()),
            }
        }
    }

    /// The frame shown at presentation time `time` (clamped to the file).
    fn frame_at(&mut self, time: f64, path: &Path) -> Result<VideoFrame, MediaError> {
        let media = self.edit.media_seconds(time);
        self.walk(media)?;
        if self.current.is_none() {
            // A seek past the last frame gives none: read the file through,
            // to its last.
            self.seek(0.0)?;
            self.walk(media)?;
        }
        let layout = self.layout;
        let shown = self
            .current
            .as_mut()
            .ok_or_else(|| MediaError::NoVideoStream(path.to_owned()))?;
        if let Some(frame) = &shown.frame {
            return Ok(frame.clone());
        }
        let frame = convert(&shown.sample, layout)?;
        shown.frame = Some(frame.clone());
        Ok(frame)
    }
}

/// Whether a frame shown from `frame` seconds is shown by `time`.
fn shown_by(frame: f64, time: f64) -> bool {
    frame <= time || timestamps_match(frame, time)
}

/// `sample`'s RGB32 image (B, G, R, X) as tightly packed opaque RGBA, top
/// row first.
fn convert(sample: &IMFSample, layout: Layout) -> Result<VideoFrame, MediaError> {
    let Layout {
        width,
        height,
        bottom_up,
    } = layout;
    let bytes = sample_bytes(sample)?;
    let expected = frame_byte_len(width, height)?;
    let row = width as usize * 4;
    // A buffer may pad its rows; each is at least a row of pixels.
    let stride = if height == 0 {
        0
    } else {
        bytes.len() / height as usize
    };
    if stride < row || height == 0 {
        return Err(MediaError::UnexpectedFrameSize {
            width,
            height,
            actual: bytes.len(),
            expected_width: width,
            expected_height: height,
            expected,
        });
    }
    let mut pixels = Vec::with_capacity(expected);
    for index in 0..height as usize {
        let index = if bottom_up {
            height as usize - 1 - index
        } else {
            index
        };
        let start = index * stride;
        pixels.extend(
            bytes[start..start + row]
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|&[b, g, r, _]| [r, g, b, 255]),
        );
    }
    Ok(VideoFrame {
        width,
        height,
        pixels: Arc::new(pixels),
    })
}

/// Clamps `time` to just inside the video's last frame, as the FFmpeg
/// plugin's `clamp_to_source_end`. A video without a frame rate or duration
/// is left alone.
fn clamp_to_source_end(time: f64, video: &VideoStream, total: Option<Time>) -> f64 {
    let Some(rate) = video.frame_rate.filter(|rate| rate.is_valid()) else {
        return time;
    };
    let Some(seconds) = video_seconds(video, total) else {
        return time;
    };
    let step = f64::from(rate.denominator) / f64::from(rate.numerator);
    time.min((seconds - step).max(0.0))
}

/// A sequential session: its run and the last (clamped) time it served.
struct Session {
    run: Run,
    last: f64,
    /// [`MfDecoder::requests`] when the session was last asked.
    used: u64,
}

/// A sequential session: the renderer's name for it, and the file.
type SessionKey = (String, PathBuf);

/// The decoder's sequential sessions; nothing else keeps a reader.
#[derive(Default)]
pub(crate) struct MfDecoder {
    sessions: HashMap<SessionKey, Session>,
    /// Sequential requests served: the clock idleness is measured on.
    requests: u64,
}

// SAFETY: the readers are Media Foundation objects of the multithreaded
// apartment `MfCodec::new` joins, which may be called on any of its
// threads; the decoder reaches them only through `&mut self`, so from one
// thread at a time.
unsafe impl Send for MfDecoder {}

/// The time `time` clamped to `source`'s video: what a request shows.
fn clamped(time: f64, source: &Source) -> f64 {
    match &source.probe.video {
        Some(video) => clamp_to_source_end(time, video, source.probe.duration),
        None => time,
    }
}

impl MfDecoder {
    pub(crate) fn probe(&mut self, path: &Path) -> Result<MediaProbe, MediaError> {
        Ok(open_source(path)?.probe)
    }

    /// The frame shown at `request.time`; through its session's reader
    /// when the request is sequential.
    pub(crate) fn decode(&mut self, request: &VideoRequest<'_>) -> Result<VideoFrame, MediaError> {
        let path = request.path;
        let time = request.time;
        if !time.is_finite() || time < 0.0 {
            return Err(MediaError::InvalidTimestamp(time));
        }
        let Some(sequential) = request.sequential else {
            let mut run = Run::open(path)?;
            let time = clamped(time, &run.source);
            let media = run.edit.media_seconds(time);
            if media > 0.0 {
                run.seek(media)?;
            }
            return run.frame_at(time, path);
        };

        self.requests = self.requests.saturating_add(1);
        let key = (sequential.session.to_owned(), path.to_owned());
        let reusable = self.sessions.get(&key).and_then(|session| {
            let time = clamped(time, &session.run.source);
            (timestamps_match(session.last, time)
                || (time > session.last && time - session.last <= MAX_FORWARD_WALK_SECONDS))
                .then_some(time)
        });
        let time = match reusable {
            Some(time) => time,
            None => {
                self.sessions.remove(&key);
                let mut run = Run::open(path)?;
                let time = clamped(time, &run.source);
                let media = run.edit.media_seconds(time);
                if media > 0.0 {
                    run.seek(media)?;
                }
                self.sessions.insert(
                    key.clone(),
                    Session {
                        run,
                        last: time,
                        used: 0,
                    },
                );
                time
            }
        };
        let session = self.sessions.get_mut(&key).expect("the session is open");
        session.used = self.requests;
        let frame = session.run.frame_at(time, path);
        match &frame {
            Ok(_) => session.last = time,
            Err(_) => {
                self.sessions.remove(&key);
            }
        }
        self.close_idle_sessions();
        frame
    }

    /// Closes the sessions left unasked for a while, then the least
    /// recently asked past [`MAX_VIDEO_SESSIONS`].
    fn close_idle_sessions(&mut self) {
        let now = self.requests;
        let window = IDLE_SESSION_FRAMES.saturating_mul(self.sessions.len() as u64);
        self.sessions
            .retain(|_, session| now - session.used <= window);
        while self.sessions.len() > MAX_VIDEO_SESSIONS {
            let Some(oldest) = self
                .sessions
                .iter()
                .min_by_key(|(_, session)| session.used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.sessions.remove(&oldest);
        }
    }

    /// The first audio stream of `path`, whole, as interleaved `f32` at
    /// `sample_rate` with `channels` channels, its edit list applied; no
    /// samples without one.
    pub(crate) fn decode_audio(
        &mut self,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        if sample_rate == 0 || !(1..=2).contains(&channels) {
            return Err(MediaError::InvalidAudioFormat {
                sample_rate,
                channels,
            });
        }
        let empty = AudioBuffer {
            sample_rate,
            channels,
            samples: Vec::new(),
        };
        let source = open_source(path)?;
        let Some(audio) = &source.audio else {
            return Ok(empty);
        };
        let (stream, edit) = (audio.stream, audio.edit);
        let current = select_output(
            &source.reader,
            stream,
            MFMediaType_Audio,
            MFAudioFormat_Float,
        )?;
        let file_rate = u32_of(&current, &MF_MT_AUDIO_SAMPLES_PER_SECOND).unwrap_or(0);
        let file_channels = u32_of(&current, &MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(0) as usize;
        if file_rate == 0 || file_channels == 0 {
            return Err(MediaError::Codec(
                "the source reader's float audio type has no rate or channels".to_owned(),
            ));
        }
        let mut samples = Vec::new();
        loop {
            let read = read_sample(&source.reader, stream, "decode the audio")?;
            if let Some((sample, _)) = read.sample {
                samples.extend(
                    sample_bytes(&sample)?
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|&bytes| f32::from_le_bytes(bytes)),
                );
            }
            if read.ended {
                break;
            }
        }
        // The file is not held while the samples are worked on.
        drop(current);
        drop(source);
        if samples.len() % file_channels != 0 {
            return Err(MediaError::InvalidAudioSampleCount {
                samples: samples.len(),
                channels: file_channels as u16,
            });
        }
        let samples = apply_edit(samples, file_channels, file_rate, edit);
        let samples = mix_channels(&samples, file_channels, usize::from(channels));
        resample_audio(
            &AudioBuffer {
                sample_rate: file_rate,
                channels,
                samples,
            },
            sample_rate,
        )
    }
}

/// `samples` of `channels` at `rate` with `edit` applied: the leading
/// `media_start` dropped, at most `duration` kept (and nothing past the
/// media's end), then `start_delay` of silence before. Without an edit's
/// duration, the samples are kept to the last.
fn apply_edit(samples: Vec<f32>, channels: usize, rate: u32, edit: Edit) -> Vec<f32> {
    let frames = |seconds: f64| (seconds * f64::from(rate)).round().max(0.0) as usize;
    let total = samples.len() / channels;
    let skip = frames(edit.media_start).min(total);
    let keep = edit.duration.map_or(total - skip, |duration| {
        let rest = edit
            .media_end
            .map_or(usize::MAX, |end| frames(end).saturating_sub(skip));
        frames(duration).min(rest).min(total - skip)
    });
    let mut edited = vec![0.0; frames(edit.start_delay) * channels];
    edited.extend_from_slice(&samples[skip * channels..(skip + keep) * channels]);
    edited
}

/// Interleaved `samples` of `from` channels as `to` (1 or 2): mono is the
/// channels' mean, stereo from mono duplicates it, and stereo from more
/// keeps the first two (left and right).
fn mix_channels(samples: &[f32], from: usize, to: usize) -> Vec<f32> {
    if from == to {
        return samples.to_vec();
    }
    samples
        .chunks_exact(from)
        .flat_map(|frame| match (to, from) {
            (1, _) => vec![frame.iter().sum::<f32>() / from as f32],
            (_, 1) => vec![frame[0]; to],
            _ => frame[..to].to_vec(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_map_times_into_the_media() {
        let edit = Edit {
            media_start: 2_002.0 / 30_000.0,
            start_delay: 0.5,
            ..Edit::default()
        };
        assert_eq!(edit.media_seconds(0.0), edit.media_start);
        assert_eq!(edit.media_seconds(0.25), edit.media_start);
        assert!((edit.media_seconds(1.5) - (1.0 + edit.media_start)).abs() < 1e-12);
        assert_eq!(Edit::default().media_seconds(0.3), 0.3);
    }

    #[test]
    fn audio_edits_trim_cap_and_delay() {
        // Ten stereo frames numbered 0 to 9 at 10 Hz.
        let samples: Vec<f32> = (0..10).flat_map(|frame| [frame as f32; 2]).collect();
        let edit = Edit {
            media_start: 0.2,
            start_delay: 0.1,
            duration: Some(0.5),
            media_end: None,
        };
        assert_eq!(
            apply_edit(samples.clone(), 2, 10, edit),
            [0.0, 0.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 5.0, 5.0, 6.0, 6.0]
        );
        // The media ends before the edit's duration does.
        let short = Edit {
            media_end: Some(0.5),
            ..edit
        };
        assert_eq!(
            apply_edit(samples.clone(), 2, 10, short),
            [0.0, 0.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0]
        );
        // A duration past the media keeps what there is.
        let long = Edit {
            duration: Some(5.0),
            ..Edit::default()
        };
        assert_eq!(apply_edit(samples.clone(), 2, 10, long), samples);
        assert_eq!(apply_edit(samples.clone(), 2, 10, Edit::default()), samples);
    }

    #[test]
    fn channels_are_mixed_as_asked() {
        assert_eq!(mix_channels(&[1.0, 0.0, 0.5, 0.5], 2, 1), [0.5, 0.5]);
        assert_eq!(mix_channels(&[0.25, 0.5], 1, 2), [0.25, 0.25, 0.5, 0.5]);
        assert_eq!(
            mix_channels(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3, 2),
            [1.0, 2.0, 4.0, 5.0]
        );
        assert_eq!(mix_channels(&[1.0, 2.0, 3.0], 3, 1), [2.0]);
    }

    #[test]
    fn frame_rates_are_reduced() {
        assert_eq!(reduced(30_000, 1_001), Some(Rational::new(30_000, 1_001)));
        assert_eq!(reduced(60, 2), Some(Rational::new(30, 1)));
        assert_eq!(reduced(0, 1), None);
        assert_eq!(reduced(30, 0), None);
    }
}
