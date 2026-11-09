//! The decoder, on `AVAssetReader`: probes by path, video frames as the
//! FFmpeg plugin's decoder gives them, and a file's first audio track.
//!
//! An `AVURLAsset` is made once a path (precise timing asked), its keys
//! loaded through `loadValuesAsynchronouslyForKeys:completionHandler:` and a
//! channel, so no run loop is needed. A frame is the one shown at the
//! requested time: a reader whose time range starts there gives that frame
//! first (retimed to the start), and later ones as they are, so a request
//! walks the reader forward to the last frame shown by its time. A time past
//! the last frame is clamped to it (as FFmpeg's `clamp_to_source_end`).
//! Frames are 8-bit BGRA converted by AVFoundation from BT.709 (deviation
//! 64), turned to RGBA, and turned upright by a quarter-turn
//! `preferredTransform` (as FFmpeg's autorotation). A sequential session
//! keeps its reader while its requests move forward by up to a second.
//!
//! Audio is the first audio track through `AVAssetReaderAudioMixOutput`, as
//! interleaved `f32` at the requested rate and channels; AVFoundation
//! honours the file's edit lists, so priming and trims stay hidden.

use crate::names::{codec_name, matrix_name, pcm_name, primaries_name, transfer_name};
use block2::RcBlock;
use celesta_composition::{Rational, Time};
use celesta_media::{
    AudioBuffer, AudioStream, MediaError, MediaProbe, VideoColor, VideoFrame, VideoRequest,
    VideoStream, timestamps_match, video_seconds,
};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetReader, AVAssetReaderAudioMixOutput, AVAssetReaderStatus, AVAssetReaderTrackOutput,
    AVAssetTrack, AVAsynchronousKeyValueLoading, AVMediaTypeAudio, AVMediaTypeVideo, AVURLAsset,
    AVURLAssetPreferPreciseDurationAndTimingKey, AVVideoColorPrimaries_ITU_R_709_2,
    AVVideoColorPrimariesKey, AVVideoColorPropertiesKey, AVVideoTransferFunction_ITU_R_709_2,
    AVVideoTransferFunctionKey, AVVideoYCbCrMatrix_ITU_R_709_2, AVVideoYCbCrMatrixKey,
};
use objc2_core_audio_types::{
    kAudioFormatFlagIsBigEndian, kAudioFormatFlagIsFloat, kAudioFormatLinearPCM,
};
use objc2_core_foundation::{CFRetained, CFString, CGAffineTransform};
use objc2_core_media::{
    CMAudioFormatDescriptionGetStreamBasicDescription, CMFormatDescription, CMSampleBuffer, CMTime,
    CMTimeFlags, CMTimeRange, CMVideoFormatDescriptionGetDimensions,
    kCMFormatDescriptionExtension_ColorPrimaries, kCMFormatDescriptionExtension_TransferFunction,
    kCMFormatDescriptionExtension_YCbCrMatrix, kCMTimePositiveInfinity,
};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSURL};
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::mpsc;

/// How far past the last request a sequential session's next may be while
/// its reader is walked forward to it; past it, a new reader starts there.
const MAX_FORWARD_WALK_SECONDS: f64 = 1.0;

/// How many frames a session may go unasked before it is closed, counted
/// as the FFmpeg plugin counts them: this many times the sessions open.
const IDLE_SESSION_FRAMES: u64 = 30;

/// The most sessions held open at once; past it, the one asked least
/// recently is closed.
const MAX_VIDEO_SESSIONS: usize = 16;

/// The asset keys the probe loads, and those of each track.
const ASSET_KEYS: [&str; 2] = ["tracks", "duration"];
const TRACK_KEYS: [&str; 6] = [
    "mediaType",
    "formatDescriptions",
    "preferredTransform",
    "minFrameDuration",
    "timeRange",
    "naturalTimeScale",
];

/// How a track's `preferredTransform` turns its frames upright: a display
/// pixel `(a x + c y, b x + d y)`, moved to start at 0, for each coded
/// pixel `(x, y)`. Only quarter turns and mirrors are taken; any other
/// transform shows frames as they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Turn {
    a: i64,
    b: i64,
    c: i64,
    d: i64,
}

impl Turn {
    const NONE: Self = Self {
        a: 1,
        b: 0,
        c: 0,
        d: 1,
    };

    /// The turn of `transform`; [`Turn::NONE`] unless its matrix is a
    /// quarter turn or a mirror.
    fn of(transform: CGAffineTransform) -> Self {
        let unit = |value: f64| {
            let rounded = value.round();
            ((value - rounded).abs() < 1e-3 && rounded.abs() <= 1.0).then_some(rounded as i64)
        };
        let (Some(a), Some(b), Some(c), Some(d)) = (
            unit(transform.a),
            unit(transform.b),
            unit(transform.c),
            unit(transform.d),
        ) else {
            return Self::NONE;
        };
        let straight = a != 0 && d != 0 && b == 0 && c == 0;
        let sideways = a == 0 && d == 0 && b != 0 && c != 0;
        if straight || sideways {
            Self { a, b, c, d }
        } else {
            Self::NONE
        }
    }

    /// The displayed size of a `width`x`height` frame.
    fn size(self, width: u32, height: u32) -> (u32, u32) {
        if self.a == 0 {
            (height, width)
        } else {
            (width, height)
        }
    }

    /// `pixels`, a tightly packed `width`x`height` RGBA frame, turned.
    fn apply(self, pixels: Vec<u8>, width: u32, height: u32) -> Vec<u8> {
        if self == Self::NONE {
            return pixels;
        }
        let (width, height) = (i64::from(width), i64::from(height));
        let (shown_width, _) = self.size(width as u32, height as u32);
        // The display coordinates of the coded corners are at least these.
        let column0 = -(self.a.min(0) * (width - 1) + self.c.min(0) * (height - 1));
        let row0 = -(self.b.min(0) * (width - 1) + self.d.min(0) * (height - 1));
        let mut turned = vec![0; pixels.len()];
        for (index, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
            let (x, y) = (index as i64 % width, index as i64 / width);
            let column = self.a * x + self.c * y + column0;
            let row = self.b * x + self.d * y + row0;
            let at = ((row * i64::from(shown_width) + column) * 4) as usize;
            turned[at..at + 4].copy_from_slice(pixel);
        }
        turned
    }
}

/// A file's asset, its probe, and the tracks it is decoded from.
struct Source {
    asset: Retained<AVURLAsset>,
    probe: MediaProbe,
    video: Option<VideoTrack>,
    /// The first audio track.
    audio: Option<Retained<AVAssetTrack>>,
}

/// The first video track, and how its frames are shown.
struct VideoTrack {
    track: Retained<AVAssetTrack>,
    /// The track's own timescale, which reader start times are given in.
    timescale: i32,
    turn: Turn,
    /// The displayed size (the probe's).
    width: u32,
    height: u32,
}

/// A decoded sample buffer, shown from `seconds`.
struct Shown {
    seconds: f64,
    sample: Retained<CMSampleBuffer>,
    /// The frame, once converted.
    frame: Option<VideoFrame>,
}

/// An open reader of a video track, walked forward through its frames.
struct Run {
    reader: Retained<AVAssetReader>,
    output: Retained<AVAssetReaderTrackOutput>,
    /// The last frame shown by the last request.
    current: Option<Shown>,
    /// The frame read after it, not shown yet.
    next: Option<Shown>,
    /// Whether the reader has given its last frame.
    ended: bool,
}

impl Drop for Run {
    /// Stops a reader still reading (a session closed, or a single frame
    /// served), so it decodes no further.
    fn drop(&mut self) {
        // SAFETY: a reader that is reading may be cancelled.
        unsafe {
            if self.reader.status() == AVAssetReaderStatus::Reading {
                self.reader.cancelReading();
            }
        }
    }
}

/// A sequential session: its run and the last (clamped) time it served.
struct Session {
    run: Run,
    last: f64,
    /// [`AvfDecoder::requests`] when the session was last asked.
    used: u64,
}

/// A sequential session: the renderer's name for it, and the file.
type SessionKey = (String, PathBuf);

/// The decoder's assets by path and its sequential sessions.
#[derive(Default)]
pub(crate) struct AvfDecoder {
    sources: HashMap<PathBuf, Source>,
    sessions: HashMap<SessionKey, Session>,
    /// Sequential requests served: the clock idleness is measured on.
    requests: u64,
}

// SAFETY: AVFoundation's assets, tracks, readers, outputs, and the sample
// buffers they give are not bound to the thread that made them; the decoder
// reaches them only through `&mut self`, so from one thread at a time, and
// the loading blocks it hands AVFoundation hold only an `mpsc::Sender`.
unsafe impl Send for AvfDecoder {}

/// Fails with [`MediaError::NotFound`] for a missing file, which
/// AVFoundation would only say it cannot open.
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

/// Loads `keys` of `object` and waits for AVFoundation to say it is done,
/// on whatever queue it says it on. A key that failed to load reads as
/// empty or invalid afterwards, which the callers check.
fn load<T>(object: &T, keys: &[&str]) -> Result<(), MediaError>
where
    T: AVAsynchronousKeyValueLoading + objc2::Message,
{
    let keys: Vec<Retained<NSString>> = keys.iter().map(|key| NSString::from_str(key)).collect();
    let keys = NSArray::from_retained_slice(&keys);
    let (sender, done) = mpsc::channel();
    let handler = RcBlock::new(move || {
        let _ = sender.send(());
    });
    // SAFETY: the keys are an array of strings; the handler only sends on a
    // channel, which may be done from any thread.
    unsafe { object.loadValuesAsynchronouslyForKeys_completionHandler(&keys, Some(&handler)) };
    done.recv()
        .map_err(|_| MediaError::Codec("AVFoundation never finished loading a file".to_owned()))
}

/// The seconds of `time`, if it is numeric.
fn seconds(time: CMTime) -> Option<f64> {
    let numeric = time.flags.contains(CMTimeFlags::Valid)
        && !time.flags.intersects(CMTimeFlags::ImpliedValueFlagsMask);
    (numeric && time.timescale > 0).then(|| time.value as f64 / f64::from(time.timescale))
}

/// A positive numeric `time` as an exact [`Time`].
fn duration(time: CMTime) -> Option<Time> {
    seconds(time)?;
    let timescale = u32::try_from(time.timescale).ok()?;
    (time.value > 0).then(|| Time::new(time.value, timescale).reduced())
}

/// The frame rate one frame of `duration` makes, exactly.
fn frame_rate(duration: CMTime) -> Option<Rational> {
    seconds(duration)?;
    let (mut numerator, mut denominator) = (
        u32::try_from(duration.timescale).ok()?,
        u32::try_from(duration.value).ok()?,
    );
    if numerator == 0 || denominator == 0 {
        return None;
    }
    let (mut left, mut right) = (numerator, denominator);
    while right != 0 {
        (left, right) = (right, left % right);
    }
    (numerator, denominator) = (numerator / left, denominator / left);
    Some(Rational::new(numerator, denominator))
}

/// The first format description of `track`, whose `formatDescriptions`
/// key was loaded.
fn format_description(track: &AVAssetTrack) -> Option<CFRetained<CMFormatDescription>> {
    // SAFETY: the key was loaded.
    let first = unsafe { track.formatDescriptions() }.firstObject()?;
    let first = NonNull::new(Retained::into_raw(first).cast::<CMFormatDescription>())?;
    // SAFETY: `formatDescriptions` holds `CMFormatDescriptionRef`s, objects
    // like any CF type; the retain `first` held moves to the result.
    Some(unsafe { CFRetained::from_raw(first) })
}

/// `key` of `format`'s extensions, if it is a string.
fn extension(format: &CMFormatDescription, key: &CFString) -> Option<String> {
    // SAFETY: a format description's extension may be read with any key.
    let value = unsafe { format.extension(key) }?;
    value.downcast_ref::<CFString>().map(ToString::to_string)
}

/// The colour tags of `format`, by FFmpeg's names.
fn video_color(format: &CMFormatDescription) -> VideoColor {
    // SAFETY (each static): an immutable CFString CoreMedia defines.
    let tag = |key: &CFString, name: fn(&str) -> Option<&'static str>| {
        extension(format, key).and_then(|value| name(&value).map(str::to_owned))
    };
    VideoColor {
        primaries: tag(
            unsafe { kCMFormatDescriptionExtension_ColorPrimaries },
            primaries_name,
        ),
        transfer: tag(
            unsafe { kCMFormatDescriptionExtension_TransferFunction },
            transfer_name,
        ),
        matrix: tag(
            unsafe { kCMFormatDescriptionExtension_YCbCrMatrix },
            matrix_name,
        ),
    }
}

/// `track` as the probe's video stream, and how it is decoded.
fn video_stream(index: u32, track: Retained<AVAssetTrack>) -> Option<(VideoStream, VideoTrack)> {
    let format = &*format_description(&track)?;
    // SAFETY: a video track's format description is a video one; the keys
    // read were loaded.
    let (dimensions, fourcc, transform, min_frame, range, timescale) = unsafe {
        (
            CMVideoFormatDescriptionGetDimensions(format),
            format.media_sub_type(),
            track.preferredTransform(),
            track.minFrameDuration(),
            track.timeRange(),
            track.naturalTimeScale(),
        )
    };
    let coded = (
        u32::try_from(dimensions.width).ok()?,
        u32::try_from(dimensions.height).ok()?,
    );
    let turn = Turn::of(transform);
    let (width, height) = turn.size(coded.0, coded.1);
    let stream = VideoStream {
        index,
        codec: Some(codec_name(fourcc)),
        width,
        height,
        frame_rate: frame_rate(min_frame),
        duration: duration(range.duration),
        color: video_color(format),
    };
    let decoded = VideoTrack {
        track,
        timescale: if timescale > 0 {
            timescale
        } else {
            Time::MICROSECONDS as i32
        },
        turn,
        width,
        height,
    };
    Some((stream, decoded))
}

/// `track` as one of the probe's audio streams.
fn audio_stream(index: u32, track: &AVAssetTrack) -> AudioStream {
    let description = format_description(track);
    let format = description.as_deref();
    // SAFETY: an audio track's format description is an audio one, whose
    // stream description lives as long as it; the key read was loaded.
    let basic = format.and_then(|format| unsafe {
        CMAudioFormatDescriptionGetStreamBasicDescription(format).as_ref()
    });
    let codec = match (format, basic) {
        (_, Some(basic)) if basic.mFormatID == kAudioFormatLinearPCM => pcm_name(
            basic.mBitsPerChannel,
            basic.mFormatFlags & kAudioFormatFlagIsFloat != 0,
            basic.mFormatFlags & kAudioFormatFlagIsBigEndian != 0,
        )
        .map(str::to_owned),
        _ => None,
    }
    // SAFETY: any format description has a media subtype.
    .or_else(|| format.map(|format| codec_name(unsafe { format.media_sub_type() })));
    // SAFETY: the key was loaded.
    let range = unsafe { track.timeRange() };
    AudioStream {
        index,
        codec,
        sample_rate: basic
            .map(|basic| basic.mSampleRate.round())
            .filter(|rate| *rate >= 1.0 && *rate <= f64::from(u32::MAX))
            .map(|rate| rate as u32),
        channels: basic
            .and_then(|basic| u16::try_from(basic.mChannelsPerFrame).ok())
            .filter(|channels| *channels > 0),
        duration: duration(range.duration),
    }
}

/// Opens `path` as an asset, its keys loaded: what it holds.
fn open_source(path: &Path) -> Result<Source, MediaError> {
    ensure_exists(path)?;
    let text = path.to_str().ok_or_else(|| {
        MediaError::Codec(format!(
            "AVFoundation cannot read {}: its path is not UTF-8",
            path.display()
        ))
    })?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(text));
    let precise = NSNumber::new_bool(true);
    // SAFETY: an immutable NSString AVFoundation defines.
    let key = unsafe { AVURLAssetPreferPreciseDurationAndTimingKey };
    let options: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_slices(&[key], &[precise.as_ref()]);
    // SAFETY: a file URL, and options of the key's documented type.
    let asset = unsafe { AVURLAsset::URLAssetWithURL_options(&url, Some(&options)) };
    load(&*asset, &ASSET_KEYS)?;
    // SAFETY: the keys were loaded (a file AVFoundation cannot open has no
    // tracks); each static is an immutable NSString AVFoundation defines.
    let (tracks, total, video_type, audio_type) = unsafe {
        (
            asset.tracks(),
            asset.duration(),
            AVMediaTypeVideo,
            AVMediaTypeAudio,
        )
    };
    let (Some(video_type), Some(audio_type)) = (video_type, audio_type) else {
        return Err(MediaError::Codec(
            "AVFoundation has no media types".to_owned(),
        ));
    };
    let mut video = None;
    let mut audio = Vec::new();
    let mut audio_track = None;
    for (index, track) in tracks.iter().enumerate() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        load(&*track, &TRACK_KEYS)?;
        // SAFETY: the key was loaded.
        let media = unsafe { track.mediaType() };
        if media.isEqualToString(video_type) {
            if video.is_none() {
                video = video_stream(index, track);
            }
        } else if media.isEqualToString(audio_type) {
            audio.push(audio_stream(index, &track));
            audio_track.get_or_insert(track);
        }
    }
    if video.is_none() && audio.is_empty() {
        return Err(unsupported(path));
    }
    let (video_stream, video) = video.unzip();
    Ok(Source {
        asset,
        probe: MediaProbe {
            duration: duration(total),
            video: video_stream,
            audio,
        },
        video,
        audio: audio_track,
    })
}

/// A string key of AVFoundation's settings from CoreVideo's.
fn cf_key(key: &CFString) -> &NSString {
    // SAFETY: CFString and NSString are toll-free bridged; the reference
    // lives as long as `key`.
    unsafe { &*(key as *const CFString).cast::<NSString>() }
}

/// A reader of `source`'s asset.
fn new_reader(source: &Source) -> Result<Retained<AVAssetReader>, MediaError> {
    // SAFETY: an asset whose keys were loaded.
    unsafe { AVAssetReader::assetReaderWithAsset_error(&source.asset) }.map_err(|error| {
        MediaError::Codec(format!(
            "AVAssetReader cannot read the file: {}",
            error.localizedDescription()
        ))
    })
}

/// The error for `reader`, which failed `doing`: its error's description,
/// if it has one.
fn reader_error(doing: &str, reader: &AVAssetReader) -> MediaError {
    // SAFETY: a reader's error may always be read.
    let reason = unsafe { reader.error() }.map_or_else(
        || "no reason given".to_owned(),
        |error| error.localizedDescription().to_string(),
    );
    MediaError::Codec(format!("AVAssetReader could not {doing}: {reason}"))
}

/// Whether a frame shown from `frame` seconds is shown by `time`.
fn shown_by(frame: f64, time: f64) -> bool {
    frame <= time || timestamps_match(frame, time)
}

impl Run {
    /// A reader of `video` from `time` seconds, started.
    fn open(source: &Source, video: &VideoTrack, time: f64) -> Result<Run, MediaError> {
        let reader = new_reader(source)?;
        // SAFETY (each static): an immutable string the frameworks define.
        let (primaries_key, primaries, transfer_key, transfer, matrix_key, matrix, color_key) = unsafe {
            (
                AVVideoColorPrimariesKey,
                AVVideoColorPrimaries_ITU_R_709_2,
                AVVideoTransferFunctionKey,
                AVVideoTransferFunction_ITU_R_709_2,
                AVVideoYCbCrMatrixKey,
                AVVideoYCbCrMatrix_ITU_R_709_2,
                AVVideoColorPropertiesKey,
            )
        };
        let missing = || MediaError::Codec("AVFoundation has no BT.709 colour keys".to_owned());
        let color: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
            &[
                primaries_key.ok_or_else(missing)?,
                transfer_key.ok_or_else(missing)?,
                matrix_key.ok_or_else(missing)?,
            ],
            &[
                primaries.ok_or_else(missing)?.as_ref(),
                transfer.ok_or_else(missing)?.as_ref(),
                matrix.ok_or_else(missing)?.as_ref(),
            ],
        );
        let bgra = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
        // SAFETY: an immutable CFString CoreVideo defines.
        let format_key = cf_key(unsafe { kCVPixelBufferPixelFormatTypeKey });
        let settings: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
            &[format_key, color_key.ok_or_else(missing)?],
            &[bgra.as_ref(), color.as_ref()],
        );
        let start = CMTime {
            value: start_ticks(time, video.timescale),
            timescale: video.timescale,
            flags: CMTimeFlags::Valid,
            epoch: 0,
        };
        // SAFETY: a video track of the reader's asset, and uncompressed
        // video settings (a pixel format and colour properties); the output
        // is added once the reader says it can be, and the time range is set
        // before reading starts.
        let output = unsafe {
            let output = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(
                &video.track,
                Some(&settings),
            );
            output.setAlwaysCopiesSampleData(false);
            if !reader.canAddOutput(&output) {
                return Err(reader_error("read the video track", &reader));
            }
            reader.addOutput(&output);
            reader.setTimeRange(CMTimeRange {
                start,
                duration: kCMTimePositiveInfinity,
            });
            if !reader.startReading() {
                return Err(reader_error("start reading", &reader));
            }
            output
        };
        Ok(Run {
            reader,
            output,
            current: None,
            next: None,
            ended: false,
        })
    }

    /// The next frame the reader gives, none past the last.
    fn read(&mut self) -> Result<Option<Shown>, MediaError> {
        loop {
            // SAFETY: the output was added and reading started; a reader
            // that ended or failed gives no more buffers.
            let sample = unsafe { self.output.copyNextSampleBuffer() };
            let Some(sample) = sample else {
                // SAFETY: a reader's status and error may always be read.
                return match unsafe { self.reader.status() } {
                    AVAssetReaderStatus::Failed => {
                        Err(reader_error("decode the video", &self.reader))
                    }
                    _ => Ok(None),
                };
            };
            // SAFETY: a sample buffer's sample count, image buffer, and
            // presentation time may always be read.
            let (count, image, time) = unsafe {
                (
                    sample.num_samples(),
                    sample.image_buffer(),
                    sample.presentation_time_stamp(),
                )
            };
            if count == 0 || image.is_none() {
                continue;
            }
            let Some(seconds) = seconds(time) else {
                continue;
            };
            return Ok(Some(Shown {
                seconds,
                sample,
                frame: None,
            }));
        }
    }

    /// The frame shown at `time` (clamped to the file), walking the reader
    /// forward to it; the first frame when the reader's first is later.
    fn frame_at(
        &mut self,
        time: f64,
        video: &VideoTrack,
        path: &Path,
    ) -> Result<VideoFrame, MediaError> {
        loop {
            if self.next.is_none() && !self.ended {
                self.next = self.read()?;
                self.ended = self.next.is_none();
            }
            match &self.next {
                Some(next) if self.current.is_none() || shown_by(next.seconds, time) => {
                    self.current = self.next.take();
                }
                _ => break,
            }
        }
        let shown = self
            .current
            .as_mut()
            .ok_or_else(|| MediaError::NoVideoStream(path.to_owned()))?;
        if let Some(frame) = &shown.frame {
            return Ok(frame.clone());
        }
        let frame = convert(&shown.sample, video)?;
        shown.frame = Some(frame.clone());
        Ok(frame)
    }
}

/// The ticks of `timescale` a reader starts at for `time`: the tick at or
/// before it, or the one a rounding error past it.
fn start_ticks(time: f64, timescale: i32) -> i64 {
    let tolerance = 1e-7_f64.max(time.abs() * 1e-9);
    ((time + tolerance) * f64::from(timescale)).floor() as i64
}

/// `sample`'s BGRA image as tightly packed RGBA, turned upright.
fn convert(sample: &CMSampleBuffer, video: &VideoTrack) -> Result<VideoFrame, MediaError> {
    // SAFETY: a sample buffer's image buffer may always be read.
    let image = unsafe { sample.image_buffer() }
        .ok_or_else(|| MediaError::Codec("AVAssetReader gave a frame without an image".into()))?;
    let buffer: &CVPixelBuffer = &image;
    let format = CVPixelBufferGetPixelFormatType(buffer);
    if format != kCVPixelFormatType_32BGRA {
        return Err(MediaError::Codec(format!(
            "AVAssetReader gave frames of {:?}, not BGRA",
            format.to_be_bytes().map(char::from)
        )));
    }
    let lock = CVPixelBufferLockFlags::ReadOnly;
    // SAFETY: `buffer` is a valid pixel buffer.
    let status = unsafe { CVPixelBufferLockBaseAddress(buffer, lock) };
    if status != 0 {
        return Err(MediaError::Codec(format!(
            "CVPixelBufferLockBaseAddress failed: {status}"
        )));
    }
    let (width, height) = (
        CVPixelBufferGetWidth(buffer),
        CVPixelBufferGetHeight(buffer),
    );
    let (base, stride) = (
        CVPixelBufferGetBaseAddress(buffer),
        CVPixelBufferGetBytesPerRow(buffer),
    );
    let pixels = if base.is_null() || stride < width * 4 {
        Err(MediaError::Codec(format!(
            "AVAssetReader's frame has rows of {stride} bytes for {width} pixels"
        )))
    } else {
        let mut pixels = Vec::with_capacity(width * height * 4);
        for row in 0..height {
            // SAFETY: the base address is locked, and the buffer has
            // `height` rows of `stride >= 4 × width` bytes.
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    base.cast::<u8>().add(row * stride).cast_const(),
                    width * 4,
                )
            };
            pixels.extend(
                bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|&[b, g, r, a]| [r, g, b, a]),
            );
        }
        Ok(pixels)
    };
    // SAFETY: the base address was locked with these flags above.
    let status = unsafe { CVPixelBufferUnlockBaseAddress(buffer, lock) };
    let pixels = pixels?;
    if status != 0 {
        return Err(MediaError::Codec(format!(
            "CVPixelBufferUnlockBaseAddress failed: {status}"
        )));
    }
    let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
        return Err(MediaError::FrameTooLarge {
            width: u32::MAX,
            height: u32::MAX,
        });
    };
    let (shown_width, shown_height) = video.turn.size(width, height);
    if (shown_width, shown_height) != (video.width, video.height) {
        return Err(MediaError::UnexpectedFrameSize {
            width: shown_width,
            height: shown_height,
            actual: pixels.len(),
            expected_width: video.width,
            expected_height: video.height,
            expected: video.width as usize * video.height as usize * 4,
        });
    }
    Ok(VideoFrame {
        width: shown_width,
        height: shown_height,
        pixels: Arc::new(video.turn.apply(pixels, width, height)),
    })
}

/// Clamps `time` to just inside the video's last frame, as the FFmpeg
/// plugin's `clamp_to_source_end`: a reader started past the last frame
/// gives none. A video without a frame rate or duration is left alone.
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

impl AvfDecoder {
    /// The source at `path`, opened once.
    fn source(&mut self, path: &Path) -> Result<&Source, MediaError> {
        if !self.sources.contains_key(path) {
            let source = open_source(path)?;
            self.sources.insert(path.to_owned(), source);
        }
        Ok(&self.sources[path])
    }

    pub(crate) fn probe(&mut self, path: &Path) -> Result<MediaProbe, MediaError> {
        Ok(self.source(path)?.probe.clone())
    }

    /// The frame shown at `request.time`; through its session's reader
    /// when the request is sequential.
    pub(crate) fn decode(&mut self, request: &VideoRequest<'_>) -> Result<VideoFrame, MediaError> {
        let path = request.path;
        let time = request.time;
        if !time.is_finite() || time < 0.0 {
            return Err(MediaError::InvalidTimestamp(time));
        }
        let source = self.source(path)?;
        let (Some(video), Some(stream)) = (&source.video, &source.probe.video) else {
            return Err(MediaError::NoVideoStream(path.to_owned()));
        };
        let time = clamp_to_source_end(time, stream, source.probe.duration);
        let Some(sequential) = request.sequential else {
            return Run::open(source, video, time)?.frame_at(time, video, path);
        };

        self.requests = self.requests.saturating_add(1);
        let key = (sequential.session.to_owned(), path.to_owned());
        let source = &self.sources[path];
        let video = source.video.as_ref().expect("the source has video");
        let reusable = self.sessions.get(&key).is_some_and(|session| {
            timestamps_match(session.last, time)
                || (time > session.last && time - session.last <= MAX_FORWARD_WALK_SECONDS)
        });
        if !reusable {
            self.sessions.remove(&key);
            let run = Run::open(source, video, time)?;
            self.sessions.insert(
                key.clone(),
                Session {
                    run,
                    last: time,
                    used: 0,
                },
            );
        }
        let session = self.sessions.get_mut(&key).expect("the session is open");
        session.used = self.requests;
        let frame = session.run.frame_at(time, video, path);
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

    /// The first audio track of `path`, whole, as interleaved `f32` at
    /// `sample_rate` with `channels` channels; no samples without one.
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
        let source = self.source(path)?;
        let empty = AudioBuffer {
            sample_rate,
            channels,
            samples: Vec::new(),
        };
        let Some(track) = &source.audio else {
            return Ok(empty);
        };
        // AVAudioSettings.h's keys, whose values are their names (AVFAudio
        // defines them, which this crate does not bind).
        let keys = [
            "AVFormatIDKey",
            "AVSampleRateKey",
            "AVNumberOfChannelsKey",
            "AVLinearPCMBitDepthKey",
            "AVLinearPCMIsFloatKey",
            "AVLinearPCMIsBigEndianKey",
            "AVLinearPCMIsNonInterleaved",
        ]
        .map(NSString::from_str);
        let values = [
            NSNumber::new_u32(kAudioFormatLinearPCM),
            NSNumber::new_f64(f64::from(sample_rate)),
            NSNumber::new_u16(channels),
            NSNumber::new_i32(32),
            NSNumber::new_bool(true),
            NSNumber::new_bool(false),
            NSNumber::new_bool(false),
        ];
        let keys: Vec<&NSString> = keys.iter().map(|key| &**key).collect();
        let values: Vec<&AnyObject> = values.iter().map(|value| value.as_ref()).collect();
        let settings: Retained<NSDictionary<NSString, AnyObject>> =
            NSDictionary::from_slices(&keys, &values);
        let tracks = NSArray::from_slice(&[&**track]);
        let reader = new_reader(source)?;
        let mut samples = Vec::new();
        // SAFETY: an audio track of the reader's asset and complete
        // interleaved float LPCM settings; the output is added once the
        // reader says it can be, before reading starts; each buffer it gives
        // is LPCM of those settings, whose data is `length` bytes.
        unsafe {
            let output =
                AVAssetReaderAudioMixOutput::assetReaderAudioMixOutputWithAudioTracks_audioSettings(
                    &tracks,
                    Some(&settings),
                );
            output.setAlwaysCopiesSampleData(false);
            if !reader.canAddOutput(&output) {
                return Err(reader_error("read the audio track", &reader));
            }
            reader.addOutput(&output);
            if !reader.startReading() {
                return Err(reader_error("start reading", &reader));
            }
            while let Some(sample) = output.copyNextSampleBuffer() {
                let Some(data) = sample.data_buffer() else {
                    continue;
                };
                let length = data.data_length();
                let mut bytes = vec![0_u8; length];
                if length > 0 {
                    let destination = NonNull::from(&mut bytes[..]).cast::<c_void>();
                    let status = data.copy_data_bytes(0, length, destination);
                    if status != 0 {
                        return Err(MediaError::Codec(format!(
                            "CMBlockBufferCopyDataBytes failed: {status}"
                        )));
                    }
                }
                samples.extend(
                    bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|&bytes| f32::from_le_bytes(bytes)),
                );
            }
            if reader.status() != AVAssetReaderStatus::Completed {
                return Err(reader_error("decode the audio", &reader));
            }
        }
        if samples.len() % usize::from(channels) != 0 {
            return Err(MediaError::InvalidAudioSampleCount {
                samples: samples.len(),
                channels,
            });
        }
        Ok(AudioBuffer { samples, ..empty })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transform(a: f64, b: f64, c: f64, d: f64) -> CGAffineTransform {
        CGAffineTransform {
            a,
            b,
            c,
            d,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// A 3x2 frame whose pixels are numbered in their red channel.
    fn numbered() -> Vec<u8> {
        (0..6).flat_map(|index| [index, 0, 0, 255]).collect()
    }

    fn reds(pixels: &[u8]) -> Vec<u8> {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[0])
            .collect()
    }

    #[test]
    fn quarter_turns_and_mirrors_are_taken() {
        assert_eq!(Turn::of(transform(1.0, 0.0, 0.0, 1.0)), Turn::NONE);
        // A half turn of a few degrees off is not a quarter turn.
        assert_eq!(Turn::of(transform(0.9, 0.1, -0.1, 0.9)), Turn::NONE);
        assert_eq!(Turn::of(transform(2.0, 0.0, 0.0, 2.0)), Turn::NONE);
        let clockwise = Turn::of(transform(0.0, 1.0, -1.0, 0.0));
        assert_eq!(clockwise.size(3, 2), (2, 3));
        // 0 1 2      3 0
        // 3 4 5  ->  4 1
        //            5 2
        assert_eq!(reds(&clockwise.apply(numbered(), 3, 2)), [3, 0, 4, 1, 5, 2]);
        let counter = Turn::of(transform(0.0, -1.0, 1.0, 0.0));
        assert_eq!(reds(&counter.apply(numbered(), 3, 2)), [2, 5, 1, 4, 0, 3]);
        let half = Turn::of(transform(-1.0, 0.0, 0.0, -1.0));
        assert_eq!(half.size(3, 2), (3, 2));
        assert_eq!(reds(&half.apply(numbered(), 3, 2)), [5, 4, 3, 2, 1, 0]);
        let mirror = Turn::of(transform(-1.0, 0.0, 0.0, 1.0));
        assert_eq!(reds(&mirror.apply(numbered(), 3, 2)), [2, 1, 0, 5, 4, 3]);
    }

    #[test]
    fn readers_start_at_or_just_before_the_time() {
        assert_eq!(start_ticks(0.0, 10), 0);
        assert_eq!(start_ticks(0.299, 10), 2);
        assert_eq!(start_ticks(0.3, 10), 3);
        // A rounding error before a frame lands on it.
        assert_eq!(start_ticks(1001.0 / 30_000.0 * 3.0 - 1e-12, 30_000), 3003);
        assert_eq!(start_ticks(0.15, 10_240), 1536);
    }

    #[test]
    fn frame_rates_are_inverted_exactly() {
        let time = |value, timescale| CMTime {
            value,
            timescale,
            flags: CMTimeFlags::Valid,
            epoch: 0,
        };
        assert_eq!(
            frame_rate(time(1001, 30_000)),
            Some(Rational::new(30_000, 1001))
        );
        assert_eq!(frame_rate(time(1024, 10_240)), Some(Rational::new(10, 1)));
        assert_eq!(frame_rate(time(0, 600)), None);
        assert_eq!(
            frame_rate(CMTime {
                flags: CMTimeFlags::empty(),
                ..time(1, 30)
            }),
            None
        );
    }
}
