//! The MP4 and QuickTime muxers, on `AVAssetWriter`: one passthrough input
//! per track (no output settings, the track's format description as the
//! source format hint), written from the encoders' packets as they are.
//!
//! Each packet becomes one sample buffer. The writer interleaves its
//! inputs itself, and an input is not ready while it is ahead of the
//! others, so each keeps a queue of the sample buffers it was not ready
//! for: [`MuxerSession::write`] appends what the inputs take and keeps the
//! rest, and [`MuxerSession::finish`] drains every queue, marking an input
//! finished as soon as its queue is empty so the others can go on. While
//! the writer is busy writing, `write` waits for the queues to fall back to
//! [`MAX_PENDING_BYTES`] rather than hold a whole backlog in memory.
//!
//! The session starts at 0. AAC before 0 is trimmed with
//! `TrimDurationAtStart`, up to the encoder's delay and `leadingAudio`;
//! with deviation 60's two attachments on the first buffer, the writer
//! records that as an edit list and a `roll` group that FFmpeg and
//! AVFoundation both honour. PCM before 0 is cut from its packets instead
//! (deviation 61), and a short last AAC packet is trimmed at its end.

use crate::components::MuxerFormat;
use crate::formats::{AAC_FRAMES, Pcm, audio_format, int, video_format};
use block2::RcBlock;
use celesta_media::settings::{InvalidSetting, validate};
use celesta_media::{MediaError, MuxerRequest, Packet, SettingValue, Track, TrackPacket};
use celesta_plugin_host::codec_family;
use objc2::rc::Retained;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterStatus, AVFileTypeMPEG4,
    AVFileTypeQuickTimeMovie, AVMediaTypeAudio, AVMediaTypeVideo,
};
use objc2_core_audio_types::AudioStreamPacketDescription;
use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_media::{
    CMAttachmentBearer, CMAudioSampleBufferCreateReadyWithPacketDescriptions, CMBlockBuffer,
    CMFormatDescription, CMSampleBuffer, CMSampleTimingInfo, CMSetAttachment, CMTime, CMTimeFlags,
    kCMAttachmentMode_ShouldPropagate, kCMBlockBufferAssureMemoryNowFlag,
    kCMSampleAttachmentKey_NotSync, kCMSampleBufferAttachmentKey_GradualDecoderRefresh,
    kCMSampleBufferAttachmentKey_TrimDurationAtEnd,
    kCMSampleBufferAttachmentKey_TrimDurationAtStart, kCMTimeZero,
};
use objc2_foundation::{NSString, NSURL};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::{NonNull, null, null_mut};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How often [`MuxerSession::finish`], and [`MuxerSession::write`] with
/// full queues, look again at inputs that are not ready.
const POLL: Duration = Duration::from_millis(1);

/// How long [`MuxerSession::finish`] and [`MuxerSession::write`] wait for
/// the writer to take any sample before they give up on it.
const STALL: Duration = Duration::from_secs(60);

/// The packet bytes the inputs' queues hold at most once
/// [`MuxerSession::write`] returns, while the writer is busy writing: the
/// exporter streams a backlog in `writePackets` batches as fast as they are
/// answered, and a disk-bound writer (ProRes 4444 at 4K is about 150 MB/s)
/// would otherwise have the whole backlog queued here.
const MAX_PENDING_BYTES: usize = 64 << 20;

/// The attachment AVAssetReader's own passthrough AAC buffers carry beside
/// `GradualDecoderRefresh`, which has no public constant (deviation 60).
const GDR_AUTHORITATIVE: &str = "IsGradualDecoderRefreshAuthoritative";

/// What a track's packets become.
enum Kind {
    Video,
    Aac {
        /// The sample frames before 0 still to trim: the encoder's delay
        /// and `leadingAudio`, less what earlier packets trimmed.
        hidden: i64,
        /// Whether a buffer has been made: the first carries deviation
        /// 60's attachments.
        started: bool,
    },
    Pcm {
        /// The bytes of one sample frame.
        frame_bytes: usize,
    },
}

/// A track's input, and the sample buffers it was not ready for yet, each
/// with the bytes of its packet.
struct Input {
    input: Retained<AVAssetWriterInput>,
    format: CFRetained<CMFormatDescription>,
    kind: Kind,
    timescale: i32,
    pending: VecDeque<(CFRetained<CMSampleBuffer>, usize)>,
    /// The bytes of the packets in `pending`.
    pending_bytes: usize,
    finished: bool,
}

/// An open muxer.
pub(crate) struct MuxerSession {
    output: PathBuf,
    writer: Retained<AVAssetWriter>,
    inputs: Vec<Input>,
    /// [`MAX_PENDING_BYTES`], less in tests.
    max_pending: usize,
}

// SAFETY: `AVAssetWriter`, its inputs, and the CoreMedia objects queued for
// them are not bound to the thread that made them; the session is only
// reached through `&mut self` or by value, so from one thread at a time,
// and the one block it hands the writer holds only an `mpsc::Sender`.
unsafe impl Send for MuxerSession {}

impl MuxerFormat {
    /// Whether the format stores `track`'s codec: H.264 and HEVC everywhere,
    /// ProRes in QuickTime; AAC everywhere, packed PCM in QuickTime.
    fn takes(self, track: &Track) -> bool {
        match track {
            Track::Video { config, .. } => match codec_family(&config.codec).as_str() {
                "avc1" | "hvc1" => true,
                "ap4h" | "apch" => self == Self::Mov,
                _ => false,
            },
            Track::Audio { config, .. } => {
                codec_family(&config.codec) == "mp4a"
                    || (self == Self::Mov && Pcm::from_codec(&config.codec).is_some())
            }
        }
    }
}

/// The error for an `AVAssetWriter` that failed `doing`: its error's
/// description, if it has one.
fn writer_error(writer: &AVAssetWriter, doing: &str) -> MediaError {
    // SAFETY: reading a writer's error is always allowed.
    let error = unsafe { writer.error() };
    let reason = error.map_or_else(
        || "no reason given".to_owned(),
        |error| error.localizedDescription().to_string(),
    );
    MediaError::Codec(format!("AVAssetWriter could not {doing}: {reason}"))
}

/// Celesta stages its output as an empty file before the muxer opens, and
/// `AVAssetWriter` refuses to write over a file: an empty regular file at
/// `output` is removed, anything else is refused.
fn clear_staged(output: &Path) -> Result<(), MediaError> {
    match std::fs::symlink_metadata(output) {
        Ok(metadata) if metadata.is_file() && metadata.len() == 0 => std::fs::remove_file(output)
            .map_err(|error| {
                MediaError::Codec(format!("cannot replace {}: {error}", output.display()))
            }),
        Ok(_) => Err(MediaError::Codec(format!(
            "{} already exists",
            output.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(MediaError::Codec(format!(
            "cannot read {}: {error}",
            output.display()
        ))),
    }
}

/// A track's input and how its packets are written, before the writer has
/// it.
fn input_for(format: MuxerFormat, track: &Track, hidden: u64) -> Result<Input, MediaError> {
    let muxer = format.id();
    let (config, timescale) = match track {
        Track::Video {
            config, timescale, ..
        }
        | Track::Audio {
            config, timescale, ..
        } => (config, *timescale),
    };
    if !format.takes(track) {
        return Err(MediaError::Codec(format!(
            "{muxer} does not take {}",
            config.codec
        )));
    }
    let timescale = int(timescale, "timescale")?;
    if timescale <= 0 {
        return Err(MediaError::Codec("a track has a timescale of 0".to_owned()));
    }
    // SAFETY (each static): an immutable NSString AVFoundation defines.
    let (description, kind, media) = match track {
        Track::Video { .. } => (video_format(track)?, Kind::Video, unsafe {
            AVMediaTypeVideo
        }),
        Track::Audio {
            sample_rate,
            channels,
            ..
        } => {
            if *channels == 0 {
                return Err(MediaError::Codec(
                    "an audio track has no channels".to_owned(),
                ));
            }
            // Audio times are in sample frames: the trims below count both.
            if i64::from(timescale) != i64::from(*sample_rate) {
                return Err(MediaError::Codec(format!(
                    "{muxer} takes audio timed in sample frames, not a timescale of \
                     {timescale} for {sample_rate} Hz"
                )));
            }
            let kind = match Pcm::from_codec(&config.codec) {
                Some(pcm) => Kind::Pcm {
                    frame_bytes: pcm.frame_bytes(*channels) as usize,
                },
                None => Kind::Aac {
                    hidden: i64::try_from(hidden).map_err(|_| {
                        MediaError::Codec(format!("{hidden} frames before 0 are too many"))
                    })?,
                    started: false,
                },
            };
            (audio_format(track)?, kind, unsafe { AVMediaTypeAudio })
        }
    };
    let media = media.ok_or_else(|| {
        MediaError::Codec("AVFoundation has no media type for a track".to_owned())
    })?;
    // SAFETY: no output settings (a passthrough input), and a source format
    // hint of the media type given, so the initializer does not throw.
    let input = unsafe {
        AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings_sourceFormatHint(
            media,
            None,
            Some(&description),
        )
    };
    // SAFETY: writing has not started; a media time scale is set on video
    // inputs only (setting one on audio throws).
    unsafe {
        input.setExpectsMediaDataInRealTime(false);
        if matches!(kind, Kind::Video) {
            input.setMediaTimeScale(timescale);
        }
    }
    Ok(Input {
        input,
        format: description,
        kind,
        timescale,
        pending: VecDeque::new(),
        pending_bytes: 0,
        finished: false,
    })
}

/// Opens `format` for `request`: the tracks' inputs, the file, and the
/// writing session from 0.
pub(crate) fn open(
    format: MuxerFormat,
    request: &MuxerRequest<'_>,
) -> Result<MuxerSession, MediaError> {
    let settings = validate(&format.settings(), request.settings)
        .map_err(|InvalidSetting { key, message }| MediaError::InvalidSettings { key, message })?;
    let faststart = settings.get("faststart") == Some(&SettingValue::Boolean(true));
    let inputs = request
        .tracks
        .iter()
        .map(|track| {
            let hidden = match track {
                Track::Audio { delay, .. } => delay.saturating_add(request.leading_audio),
                Track::Video { .. } => 0,
            };
            input_for(format, track, hidden)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let output = request.output;
    let path = output.to_str().ok_or_else(|| {
        MediaError::Codec(format!(
            "AVAssetWriter cannot write {}: its path is not UTF-8",
            output.display()
        ))
    })?;
    clear_staged(output)?;

    // SAFETY (each static): an immutable NSString AVFoundation defines.
    let file_type = match format {
        MuxerFormat::Mp4 => unsafe { AVFileTypeMPEG4 },
        MuxerFormat::Mov => unsafe { AVFileTypeQuickTimeMovie },
    }
    .ok_or_else(|| MediaError::Codec(format!("AVFoundation has no {} file type", format.id())))?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    // SAFETY: a file URL and a file type AVMediaFormat.h declares.
    let writer = unsafe { AVAssetWriter::assetWriterWithURL_fileType_error(&url, file_type) }
        .map_err(|error| {
            MediaError::Codec(format!(
                "AVAssetWriter cannot write {}: {}",
                output.display(),
                error.localizedDescription()
            ))
        })?;
    // The movie's timescale is the first video track's, else the first track's,
    // so its edit lists end where that track does (at the default of 600 a
    // 29.97 fps track's would lose part of its last frame).
    let movie_timescale = inputs
        .iter()
        .find(|input| matches!(input.kind, Kind::Video))
        .or_else(|| inputs.first())
        .map(|input| input.timescale);
    // SAFETY: writing has not started; each input is added only when the
    // writer says it can be, so `addInput` does not throw.
    unsafe {
        writer.setShouldOptimizeForNetworkUse(faststart);
        if let Some(timescale) = movie_timescale {
            writer.setMovieTimeScale(timescale);
        }
        for (index, input) in inputs.iter().enumerate() {
            if !writer.canAddInput(&input.input) {
                return Err(MediaError::Codec(format!(
                    "AVAssetWriter cannot add track {index} to a {} file",
                    format.id()
                )));
            }
            writer.addInput(&input.input);
        }
    }
    let session = MuxerSession {
        output: output.to_owned(),
        writer,
        inputs,
        max_pending: MAX_PENDING_BYTES,
    };
    // SAFETY: the writer has its inputs; the session starts once, after
    // writing has started.
    unsafe {
        if !session.writer.startWriting() {
            return Err(writer_error(&session.writer, "start writing"));
        }
        session.writer.startSessionAtSourceTime(kCMTimeZero);
    }
    Ok(session)
}

/// A block buffer holding a copy of `bytes`.
fn block_buffer(bytes: &[u8]) -> Result<CFRetained<CMBlockBuffer>, MediaError> {
    if bytes.is_empty() {
        return Err(MediaError::Codec("a packet has no bytes".to_owned()));
    }
    let mut buffer: *mut CMBlockBuffer = null_mut();
    // SAFETY: no memory is given, so CoreMedia allocates `bytes.len()`
    // bytes with the default allocator, now; `buffer` is a valid place for
    // the new buffer.
    let status = unsafe {
        CMBlockBuffer::create_with_memory_block(
            None,
            null_mut(),
            bytes.len(),
            None,
            null(),
            0,
            bytes.len(),
            kCMBlockBufferAssureMemoryNowFlag,
            NonNull::from(&mut buffer),
        )
    };
    if status != 0 {
        return Err(MediaError::Codec(format!(
            "CMBlockBufferCreateWithMemoryBlock failed: {status}"
        )));
    }
    let buffer = NonNull::new(buffer).ok_or_else(|| {
        MediaError::Codec("CMBlockBufferCreateWithMemoryBlock gave no buffer".to_owned())
    })?;
    // SAFETY: the buffer is the caller's to release (the Create rule).
    let buffer = unsafe { CFRetained::from_raw(buffer) };
    // SAFETY: the buffer's memory is `bytes.len()` bytes long, and `bytes`
    // is not in it.
    let status = unsafe {
        CMBlockBuffer::replace_data_bytes(
            NonNull::from(bytes).cast::<c_void>(),
            &buffer,
            0,
            bytes.len(),
        )
    };
    if status != 0 {
        return Err(MediaError::Codec(format!(
            "CMBlockBufferReplaceDataBytes failed: {status}"
        )));
    }
    Ok(buffer)
}

/// The sample buffer a `…Create` call made, which the caller owns, or why
/// there is none.
fn created(
    call: &str,
    status: i32,
    sample: *mut CMSampleBuffer,
) -> Result<CFRetained<CMSampleBuffer>, MediaError> {
    if status != 0 {
        return Err(MediaError::Codec(format!("{call} failed: {status}")));
    }
    let sample = NonNull::new(sample)
        .ok_or_else(|| MediaError::Codec(format!("{call} gave no sample buffer")))?;
    // SAFETY: the buffer is the caller's to release (the Create rule).
    Ok(unsafe { CFRetained::from_raw(sample) })
}

/// Sets `key` to `value` on `sample`, to propagate (deviation 60: without
/// it the writer ignores the gradual decoder refresh).
fn attach(sample: &CMSampleBuffer, key: &CFString, value: &CFType) {
    let bearer: &CMAttachmentBearer = sample.as_ref();
    // SAFETY: a sample buffer bears attachments; each caller's value is of
    // the type its key takes.
    unsafe { CMSetAttachment(bearer, key, Some(value), kCMAttachmentMode_ShouldPropagate) };
}

/// `value` ticks of `timescale` as a numeric `CMTime` (`CMTimeMake`).
fn time(value: i64, timescale: i32) -> CMTime {
    CMTime {
        value,
        timescale,
        flags: CMTimeFlags::Valid,
        epoch: 0,
    }
}

/// A `CMTime` of `value` ticks of `timescale` as the dictionary sample
/// buffer attachments take.
fn time_dictionary(value: i64, timescale: i32) -> Result<CFRetained<CFDictionary>, MediaError> {
    // SAFETY: any numeric time can be copied as a dictionary.
    unsafe { time(value, timescale).as_dictionary(None) }
        .ok_or_else(|| MediaError::Codec("CMTimeCopyAsDictionary gave nothing".to_owned()))
}

impl Input {
    /// The sample buffer of `packet`, none when nothing of it is shown (PCM
    /// before 0).
    fn sample(
        &mut self,
        packet: &Packet,
    ) -> Result<Option<CFRetained<CMSampleBuffer>>, MediaError> {
        match self.kind {
            Kind::Video => self.video_sample(packet).map(Some),
            Kind::Aac { .. } => self.aac_sample(packet).map(Some),
            Kind::Pcm { frame_bytes } => self.pcm_sample(packet, frame_bytes),
        }
    }

    /// One video frame: its times, and `NotSync` unless it is a keyframe.
    fn video_sample(&self, packet: &Packet) -> Result<CFRetained<CMSampleBuffer>, MediaError> {
        let data = block_buffer(&packet.bytes)?;
        let timing = CMSampleTimingInfo {
            duration: time(packet.duration, self.timescale),
            presentationTimeStamp: time(packet.pts, self.timescale),
            decodeTimeStamp: time(packet.dts, self.timescale),
        };
        let size = packet.bytes.len();
        let mut sample: *mut CMSampleBuffer = null_mut();
        // SAFETY: one sample of `size` bytes in `data`, of the track's
        // format, with one timing entry; `sample` is a valid place for the
        // new buffer.
        let status = unsafe {
            CMSampleBuffer::create_ready(
                None,
                Some(&data),
                Some(&self.format),
                1,
                1,
                &timing,
                1,
                &size,
                NonNull::from(&mut sample),
            )
        };
        let sample = created("CMSampleBufferCreateReady", status, sample)?;
        if !packet.key {
            // SAFETY: the buffer is valid; with one sample its attachments
            // array holds one mutable dictionary keyed by strings.
            unsafe {
                let attachments = sample.sample_attachments_array(true).ok_or_else(|| {
                    MediaError::Codec("a sample buffer has no attachments array".to_owned())
                })?;
                let attachments = CFRetained::cast_unchecked::<
                    objc2_core_foundation::CFArray<CFDictionary>,
                >(attachments);
                let first = attachments.get(0).ok_or_else(|| {
                    MediaError::Codec("a sample buffer has no sample attachments".to_owned())
                })?;
                let first = CFRetained::cast_unchecked::<
                    objc2_core_foundation::CFMutableDictionary<CFString, CFType>,
                >(first);
                first.set(kCMSampleAttachmentKey_NotSync, CFBoolean::new(true));
            }
        }
        Ok(sample)
    }

    /// One AAC packet at its `pts`: trimmed at the start while it is before
    /// 0 and frames remain to hide, at the end when it is short; the first
    /// carries deviation 60's attachments.
    fn aac_sample(&mut self, packet: &Packet) -> Result<CFRetained<CMSampleBuffer>, MediaError> {
        let data = block_buffer(&packet.bytes)?;
        let description = AudioStreamPacketDescription {
            mStartOffset: 0,
            mVariableFramesInPacket: 0,
            mDataByteSize: u32::try_from(packet.bytes.len())
                .map_err(|_| MediaError::Codec("an AAC packet is too large".to_owned()))?,
        };
        let mut sample: *mut CMSampleBuffer = null_mut();
        // SAFETY: one packet of the track's format, all of `data`, as the
        // description says; `sample` is a valid place for the new buffer.
        let status = unsafe {
            CMAudioSampleBufferCreateReadyWithPacketDescriptions(
                None,
                &data,
                &self.format,
                1,
                time(packet.pts, self.timescale),
                &description,
                NonNull::from(&mut sample),
            )
        };
        let sample = created(
            "CMAudioSampleBufferCreateReadyWithPacketDescriptions",
            status,
            sample,
        )?;
        let Kind::Aac { hidden, started } = &mut self.kind else {
            unreachable!("an AAC input");
        };
        let duration = packet.duration.clamp(0, AAC_FRAMES);
        let start = if packet.pts < 0 {
            duration.min(-packet.pts).min(*hidden)
        } else {
            0
        };
        // SAFETY (each static): an immutable CFString CoreMedia defines.
        if start > 0 {
            *hidden -= start;
            let trim = time_dictionary(start, self.timescale)?;
            attach(
                &sample,
                unsafe { kCMSampleBufferAttachmentKey_TrimDurationAtStart },
                trim.as_ref(),
            );
        }
        if duration < AAC_FRAMES {
            let trim = time_dictionary(AAC_FRAMES - duration, self.timescale)?;
            attach(
                &sample,
                unsafe { kCMSampleBufferAttachmentKey_TrimDurationAtEnd },
                trim.as_ref(),
            );
        }
        if !*started {
            *started = true;
            attach(
                &sample,
                unsafe { kCMSampleBufferAttachmentKey_GradualDecoderRefresh },
                CFNumber::new_i32(1).as_ref(),
            );
            attach(
                &sample,
                &CFString::from_str(GDR_AUTHORITATIVE),
                CFBoolean::new(true).as_ref(),
            );
        }
        Ok(sample)
    }

    /// The sample frames of a PCM packet from 0 on, as one buffer; none when
    /// the packet ends before 0.
    fn pcm_sample(
        &self,
        packet: &Packet,
        frame_bytes: usize,
    ) -> Result<Option<CFRetained<CMSampleBuffer>>, MediaError> {
        if !packet.bytes.len().is_multiple_of(frame_bytes) {
            return Err(MediaError::Codec(format!(
                "a PCM packet of {} bytes is not whole frames of {frame_bytes}",
                packet.bytes.len()
            )));
        }
        let frames = (packet.bytes.len() / frame_bytes) as i64;
        let cut = (-packet.pts).clamp(0, frames);
        if cut == frames {
            return Ok(None);
        }
        let data = block_buffer(&packet.bytes[cut as usize * frame_bytes..])?;
        let mut sample: *mut CMSampleBuffer = null_mut();
        // SAFETY: `frames - cut` whole frames of the track's constant
        // format in `data`, which needs no packet descriptions; `sample` is
        // a valid place for the new buffer.
        let status = unsafe {
            CMAudioSampleBufferCreateReadyWithPacketDescriptions(
                None,
                &data,
                &self.format,
                (frames - cut) as isize,
                time(packet.pts + cut, self.timescale),
                null(),
                NonNull::from(&mut sample),
            )
        };
        created(
            "CMAudioSampleBufferCreateReadyWithPacketDescriptions",
            status,
            sample,
        )
        .map(Some)
    }

    /// Appends queued buffers while the input is ready: whether it took any.
    fn append_ready(&mut self, writer: &AVAssetWriter) -> Result<bool, MediaError> {
        let mut took = false;
        // SAFETY: writing has started and the session is open; a buffer is
        // appended only while the input is ready for it, and each is of the
        // input's media type.
        unsafe {
            while let Some((sample, bytes)) = self.pending.front() {
                if !self.input.isReadyForMoreMediaData() {
                    break;
                }
                if !self.input.appendSampleBuffer(sample) {
                    return Err(writer_error(writer, "append a sample"));
                }
                self.pending_bytes -= bytes;
                self.pending.pop_front();
                took = true;
            }
        }
        Ok(took)
    }

    /// Whether the writer would take a buffer for this input now: one that
    /// is not finished and is ready.
    fn wants_more(&self) -> bool {
        // SAFETY: an input's readiness may always be read.
        !self.finished && unsafe { self.input.isReadyForMoreMediaData() }
    }
}

/// The error for a writer that took no sample for [`STALL`].
fn stalled() -> MediaError {
    MediaError::Codec(format!(
        "AVAssetWriter took no sample for {} s",
        STALL.as_secs()
    ))
}

impl MuxerSession {
    /// Queues `packets` for their tracks' inputs in the order given, and
    /// appends what the inputs are ready for; then, while the queues hold
    /// more than [`MAX_PENDING_BYTES`] and the writer is busy writing,
    /// waits for it.
    pub(crate) fn write(&mut self, packets: &[TrackPacket]) -> Result<(), MediaError> {
        self.check_writing()?;
        for given in packets {
            let count = self.inputs.len();
            let input = self.inputs.get_mut(given.track).ok_or_else(|| {
                MediaError::Codec(format!("a packet for track {} of {count}", given.track))
            })?;
            if let Some(sample) = input.sample(&given.packet)? {
                let bytes = given.packet.bytes.len();
                input.pending.push_back((sample, bytes));
                input.pending_bytes += bytes;
            }
            self.append_ready()?;
        }
        self.wait_while_full()
    }

    /// The bytes of the packets every input's queue holds.
    fn pending_bytes(&self) -> usize {
        self.inputs.iter().map(|input| input.pending_bytes).sum()
    }

    /// Appends what the inputs take, polling as `finish` does, while the
    /// queues hold more than `max_pending` bytes and no input is ready: the
    /// writer is then bound by its output. An input that is ready has an
    /// empty queue (`append_ready` would have taken from it), so the writer
    /// is waiting for that track's packets, which come only once `write`
    /// returns; a long run of one track (all the video, then the audio) is
    /// still queued whole.
    fn wait_while_full(&mut self) -> Result<(), MediaError> {
        let mut last = Instant::now();
        loop {
            if self.append_ready()? {
                last = Instant::now();
            }
            if self.pending_bytes() <= self.max_pending || self.inputs.iter().any(Input::wants_more)
            {
                return Ok(());
            }
            if last.elapsed() > STALL {
                return Err(stalled());
            }
            std::thread::sleep(POLL);
        }
    }

    /// `Ok` while the writer is writing; its error once it has failed (an
    /// input of a failed writer is never ready again, so what is queued for
    /// it would only pile up).
    fn check_writing(&self) -> Result<(), MediaError> {
        // SAFETY: reading a writer's status is always allowed.
        if unsafe { self.writer.status() } == AVAssetWriterStatus::Writing {
            Ok(())
        } else {
            Err(writer_error(&self.writer, "write the file"))
        }
    }

    /// Appends to every input what it is ready for, until none takes more:
    /// whether any took a buffer; the writer's error once it has failed.
    fn append_ready(&mut self) -> Result<bool, MediaError> {
        self.check_writing()?;
        let mut took = false;
        loop {
            let mut again = false;
            for input in &mut self.inputs {
                again |= input.append_ready(&self.writer)?;
            }
            if !again {
                return Ok(took);
            }
            took = true;
        }
    }

    /// Writes every queued buffer, finishes each input as its queue empties,
    /// then the file: the files written.
    pub(crate) fn finish(mut self) -> Result<Vec<PathBuf>, MediaError> {
        let mut last = Instant::now();
        loop {
            if self.append_ready()? {
                last = Instant::now();
            }
            for input in &mut self.inputs {
                if input.pending.is_empty() && !input.finished {
                    // SAFETY: writing has started; nothing is appended to
                    // the input after this.
                    unsafe { input.input.markAsFinished() };
                    input.finished = true;
                }
            }
            if self.inputs.iter().all(|input| input.finished) {
                break;
            }
            if last.elapsed() > STALL {
                return Err(stalled());
            }
            std::thread::sleep(POLL);
        }
        let (sender, done) = mpsc::channel();
        let handler = RcBlock::new(move || {
            let _ = sender.send(());
        });
        // SAFETY: every input is finished; the handler only sends on a
        // channel, which may be done from any thread.
        unsafe { self.writer.finishWritingWithCompletionHandler(&handler) };
        done.recv()
            .map_err(|_| MediaError::Codec("AVAssetWriter never said it finished".to_owned()))?;
        // SAFETY: reading a writer's status is always allowed.
        if unsafe { self.writer.status() } != AVAssetWriterStatus::Completed {
            return Err(writer_error(&self.writer, "finish the file"));
        }
        Ok(vec![self.output.clone()])
    }

    /// Stops writing: the files left.
    pub(crate) fn abort(self) -> Vec<PathBuf> {
        vec![self.output.clone()]
    }
}

impl Drop for MuxerSession {
    /// Cancels a file still being written (an abort, or a session dropped
    /// unfinished).
    fn drop(&mut self) {
        // SAFETY: a writer that is writing may be cancelled.
        unsafe {
            if self.writer.status() == AVAssetWriterStatus::Writing {
                self.writer.cancelWriting();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_media::{Settings, TrackConfig};

    /// A 48 kHz stereo `lpcm.s16le` track.
    fn pcm_track() -> Track {
        Track::Audio {
            config: TrackConfig {
                codec: "lpcm.s16le".to_owned(),
                description: None,
            },
            timescale: 48_000,
            sample_rate: 48_000,
            channels: 2,
            delay: 0,
        }
    }

    /// A `mov` session of `tracks` writing `output`.
    fn open_mov(output: &Path, tracks: &[Track]) -> MuxerSession {
        open(
            MuxerFormat::Mov,
            &MuxerRequest {
                component: "mov",
                settings: &Settings::new(),
                output,
                tracks,
                leading_audio: 0,
            },
        )
        .unwrap()
    }

    /// `frames` sample frames of 16-bit stereo PCM from `pts`.
    fn pcm_packet(pts: i64, frames: usize) -> TrackPacket {
        TrackPacket {
            track: 0,
            packet: Packet {
                pts,
                dts: pts,
                duration: frames as i64,
                key: true,
                bytes: (0..frames * 4).map(|index| index as u8).collect(),
                additional: Vec::new(),
            },
        }
    }

    #[test]
    fn a_busy_writer_holds_writes_back() {
        // A small cap, so a few batches fill it: 4 MiB batches of one
        // track (there is no other track to wait for), 64 MiB in all.
        const CAP: usize = 1 << 20;
        const FRAMES: usize = 1 << 20;
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("pcm.mov");
        let tracks = [pcm_track()];
        let mut session = open_mov(&output, &tracks);
        session.max_pending = CAP;
        let mut most = 0;
        for batch in 0..16 {
            let packets: Vec<TrackPacket> = (0..4)
                .map(|index| pcm_packet((batch * 4 + index) * FRAMES as i64 / 4, FRAMES / 4))
                .collect();
            let batch_bytes: usize = packets.iter().map(|given| given.packet.bytes.len()).sum();
            session.write(&packets).unwrap();
            let pending = session.pending_bytes();
            assert!(
                pending <= CAP + batch_bytes,
                "batch {batch}: {pending} bytes queued, over {CAP} and a batch of {batch_bytes}"
            );
            most = most.max(pending);
        }
        println!("at most {most} bytes queued after a write");
        assert_eq!(session.finish().unwrap(), std::slice::from_ref(&output));
        assert!(std::fs::metadata(&output).unwrap().len() >= 64 << 20);
    }
}
