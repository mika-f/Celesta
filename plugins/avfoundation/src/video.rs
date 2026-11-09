//! The H.264, HEVC, and ProRes encoders, on VideoToolbox's
//! `VTCompressionSession`.
//!
//! A session takes frames of the input [`components::choose_input`] chose,
//! copied row by row into pixel buffers from the session's pool and tagged
//! BT.709 (limited range is the `420v` / `x420` formats', and the matrix the
//! session converts `rgba16` with). VideoToolbox gives each sample buffer
//! to an output handler on a thread of its own, which reads it into a
//! shared queue; [`VideoSession::encode`] and [`VideoSession::finish`]
//! drain the queue into packets. The first sample's format description
//! gives the track's config, and the stream's reorder depth, by which every
//! decode timestamp is shifted back so that none follows its presentation
//! (as libx264's do).

use crate::color::check_color;
use crate::components::{
    self, VideoCodec, VtSetting, checked_settings, choose_input, prores_hq, vt_settings,
};
use crate::{apple, params};
use block2::RcBlock;
use celesta_media::{Encoded, MediaError, Packet, PixelFormat, TrackConfig, VideoEncoderRequest};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFData, CFDictionary, CFNumber, CFRetained, CFString, CFType,
};
use objc2_core_media::{
    CMSampleBuffer, CMTime, CMTimeFlags,
    kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms, kCMSampleAttachmentKey_NotSync,
    kCMTimeInvalid,
};
use objc2_core_video::{
    CVAttachmentMode, CVPixelBuffer, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBaseAddressOfPlane, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferGetHeight, CVPixelBufferGetHeightOfPlane,
    CVPixelBufferGetPixelFormatType, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferPool, CVPixelBufferUnlockBaseAddress, kCVImageBufferColorPrimaries_ITU_R_709_2,
    kCVImageBufferColorPrimariesKey, kCVImageBufferTransferFunction_ITU_R_709_2,
    kCVImageBufferTransferFunctionKey, kCVImageBufferYCbCrMatrix_ITU_R_709_2,
    kCVImageBufferYCbCrMatrixKey, kCVPixelBufferHeightKey, kCVPixelBufferIOSurfacePropertiesKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey, kCVPixelFormatType_64RGBALE,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
};
use objc2_video_toolbox::{
    VTCompressionSession, VTEncodeInfoFlags, VTSessionSetProperty,
    kVTCompressionPropertyKey_AllowFrameReordering, kVTCompressionPropertyKey_AverageBitRate,
    kVTCompressionPropertyKey_ColorPrimaries, kVTCompressionPropertyKey_ExpectedFrameRate,
    kVTCompressionPropertyKey_MaxKeyFrameInterval, kVTCompressionPropertyKey_ProfileLevel,
    kVTCompressionPropertyKey_Quality, kVTCompressionPropertyKey_RealTime,
    kVTCompressionPropertyKey_TransferFunction, kVTCompressionPropertyKey_YCbCrMatrix,
    kVTProfileLevel_H264_Baseline_AutoLevel, kVTProfileLevel_H264_High_AutoLevel,
    kVTProfileLevel_H264_Main_AutoLevel, kVTProfileLevel_HEVC_Main_AutoLevel,
    kVTProfileLevel_HEVC_Main10_AutoLevel,
};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr::{NonNull, null_mut};
use std::sync::{Arc, Mutex, PoisonError};

/// The output handler's type, as `VTCompressionOutputHandler` points at it.
type Handler = RcBlock<dyn Fn(i32, VTEncodeInfoFlags, *mut CMSampleBuffer)>;

/// The ProRes 422 HQ codec type; 4444's is [`VideoCodec::codec_type`].
const PRORES_HQ: u32 = u32::from_be_bytes(*b"apch");

/// One compressed frame as the output handler read it, times in the
/// session's timescale.
struct Sample {
    pts: Option<i64>,
    /// None when VideoToolbox gave no decode time: a stream that does not
    /// reorder.
    dts: Option<i64>,
    duration: Option<i64>,
    key: bool,
    /// Length-prefixed NAL units, or a ProRes frame.
    bytes: Vec<u8>,
    /// The `avcC` or `hvcC` of the sample's format description: read for the
    /// first sample only.
    description: Option<Vec<u8>>,
}

/// What the output handler was given for one frame.
enum Delivered {
    Sample(Sample),
    /// The status of a frame VideoToolbox could not compress.
    Failed(i32),
    /// A frame VideoToolbox dropped, or a sample buffer without what a
    /// packet needs (named).
    Missing(&'static str),
}

/// The queue the output handler fills and the session drains.
#[derive(Default)]
struct Shared {
    delivered: VecDeque<Delivered>,
    /// Whether a sample's description has been read.
    described: bool,
}

/// An open VideoToolbox compression session.
pub(crate) struct VideoSession {
    session: CFRetained<VTCompressionSession>,
    shared: Arc<Mutex<Shared>>,
    handler: Handler,
    codec: VideoCodec,
    /// The codec string ProRes tracks get (`ap4h` or `apch`).
    prores_family: &'static str,
    input: PixelFormat,
    width: u32,
    height: u32,
    timescale: i32,
    /// A frame's duration in the timescale.
    frame: i64,
    /// Whether the encoder may reorder frames (`bFrames`).
    reorders: bool,
    /// How many frames decode timestamps are shifted back by, once the first
    /// sample's parameter sets told.
    depth: Option<i64>,
}

// SAFETY: a `VTCompressionSession` and the `CFRetained` references to it
// may be used from any thread (VideoToolbox serialises its own work, and CF
// reference counting is atomic); the session object is only reached through
// `&mut self`, so never from two threads at once. The handler block's
// reference count is atomic too, and its closure holds only an
// `Arc<Mutex<Shared>>` and plain values, which are `Send + Sync`.
unsafe impl Send for VideoSession {}

/// The error for a framework call that answered `status`.
fn failed(call: &str, status: i32) -> MediaError {
    MediaError::Codec(format!("{call} failed: {status}"))
}

/// `status` as `Ok` when it is `noErr`.
fn check(call: &str, status: i32) -> Result<(), MediaError> {
    if status == 0 {
        Ok(())
    } else {
        Err(failed(call, status))
    }
}

/// `time` in ticks of `timescale`, rounded to the nearest; none when it is
/// not a numeric time.
fn ticks(time: CMTime, timescale: i32) -> Option<i64> {
    let numeric = time.flags.contains(CMTimeFlags::Valid)
        && !time.flags.intersects(CMTimeFlags::ImpliedValueFlagsMask);
    if !numeric || time.timescale <= 0 {
        return None;
    }
    if time.timescale == timescale {
        return Some(time.value);
    }
    let (value, from, to) = (
        i128::from(time.value),
        i128::from(time.timescale),
        i128::from(timescale),
    );
    i64::try_from((value * to * 2 + from).div_euclid(from * 2)).ok()
}

/// The CoreVideo pixel format VideoToolbox is given `input` in.
fn pixel_format(input: PixelFormat) -> u32 {
    match input {
        PixelFormat::Nv12 => kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
        PixelFormat::P010 => kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
        _ => kCVPixelFormatType_64RGBALE,
    }
}

/// The bytes a row of each of `input`'s planes takes at `width`, and its
/// rows at `height`: luma then interleaved chroma for the 4:2:0 formats,
/// one plane of RGBA otherwise.
fn planes(input: PixelFormat, width: usize, height: usize) -> Vec<(usize, usize)> {
    match input {
        PixelFormat::Nv12 => vec![(width, height), (width.div_ceil(2) * 2, height.div_ceil(2))],
        PixelFormat::P010 => vec![
            (width * 2, height),
            (width.div_ceil(2) * 4, height.div_ceil(2)),
        ],
        _ => vec![(width * 8, height)],
    }
}

/// `kVTProfileLevel_…` named by its suffix.
fn profile_level(name: &str) -> Result<&'static CFString, MediaError> {
    // SAFETY: immutable static CFStrings VideoToolbox defines.
    unsafe {
        Ok(match name {
            "H264_High_AutoLevel" => kVTProfileLevel_H264_High_AutoLevel,
            "H264_Main_AutoLevel" => kVTProfileLevel_H264_Main_AutoLevel,
            "H264_Baseline_AutoLevel" => kVTProfileLevel_H264_Baseline_AutoLevel,
            "HEVC_Main_AutoLevel" => kVTProfileLevel_HEVC_Main_AutoLevel,
            "HEVC_Main10_AutoLevel" => kVTProfileLevel_HEVC_Main10_AutoLevel,
            other => return Err(MediaError::Codec(format!("no profile level {other}"))),
        })
    }
}

/// Sets `key` to `value` on `session`, naming `name` when it is refused.
fn set_property(
    session: &VTCompressionSession,
    name: &str,
    key: &CFString,
    value: &CFType,
) -> Result<(), MediaError> {
    // SAFETY: `session` is a VTSession, and `value` is of the type the
    // property `key` takes (each caller passes the documented one).
    let status = unsafe { VTSessionSetProperty(session, key, Some(value)) };
    check(&format!("VTSessionSetProperty({name})"), status)
}

/// Sets the properties `settings` give, BT.709 colour, offline encoding,
/// and the frame rate `rate` (frames a second) on `session`.
fn configure(
    session: &VTCompressionSession,
    settings: &[VtSetting],
    rate: f64,
) -> Result<(), MediaError> {
    // SAFETY (each static): an immutable CFString VideoToolbox or CoreVideo
    // defines.
    for setting in settings {
        match *setting {
            VtSetting::ProfileLevel(name) => set_property(
                session,
                "ProfileLevel",
                unsafe { kVTCompressionPropertyKey_ProfileLevel },
                profile_level(name)?,
            )?,
            VtSetting::Quality(quality) => set_property(
                session,
                "Quality",
                unsafe { kVTCompressionPropertyKey_Quality },
                &CFNumber::new_f32(quality),
            )?,
            VtSetting::AverageBitRate(bitrate) => set_property(
                session,
                "AverageBitRate",
                unsafe { kVTCompressionPropertyKey_AverageBitRate },
                &CFNumber::new_i64(bitrate),
            )?,
            VtSetting::MaxKeyFrameInterval(frames) => set_property(
                session,
                "MaxKeyFrameInterval",
                unsafe { kVTCompressionPropertyKey_MaxKeyFrameInterval },
                &CFNumber::new_i32(frames),
            )?,
            VtSetting::AllowFrameReordering(allowed) => set_property(
                session,
                "AllowFrameReordering",
                unsafe { kVTCompressionPropertyKey_AllowFrameReordering },
                CFBoolean::new(allowed),
            )?,
        }
    }
    let colors = unsafe {
        [
            (
                "ColorPrimaries",
                kVTCompressionPropertyKey_ColorPrimaries,
                kCVImageBufferColorPrimaries_ITU_R_709_2,
            ),
            (
                "TransferFunction",
                kVTCompressionPropertyKey_TransferFunction,
                kCVImageBufferTransferFunction_ITU_R_709_2,
            ),
            (
                "YCbCrMatrix",
                kVTCompressionPropertyKey_YCbCrMatrix,
                kCVImageBufferYCbCrMatrix_ITU_R_709_2,
            ),
        ]
    };
    for (name, key, value) in colors {
        set_property(session, name, key, value)?;
    }
    set_property(
        session,
        "RealTime",
        unsafe { kVTCompressionPropertyKey_RealTime },
        CFBoolean::new(false),
    )?;
    set_property(
        session,
        "ExpectedFrameRate",
        unsafe { kVTCompressionPropertyKey_ExpectedFrameRate },
        &CFNumber::new_f64(rate),
    )
}

/// The `atom` (`avcC` or `hvcC`) of `sample`'s format description's
/// `SampleDescriptionExtensionAtoms`.
///
/// # Safety
///
/// `sample` is a valid sample buffer.
unsafe fn description_atom(sample: &CMSampleBuffer, atom: &str) -> Option<Vec<u8>> {
    // SAFETY: `sample` is valid; the extension is a dictionary of atoms
    // keyed by their four-character names, each a CFData.
    unsafe {
        let format = sample.format_description()?;
        let atoms = format
            .extension(kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms)?
            .downcast::<CFDictionary>()
            .ok()?;
        let atoms = CFRetained::cast_unchecked::<CFDictionary<CFString, CFType>>(atoms);
        let record = atoms.get(&CFString::from_str(atom))?;
        Some(record.downcast::<CFData>().ok()?.to_vec())
    }
}

/// What the output handler reads of `sample`, its description too when
/// `atom` names one.
///
/// # Safety
///
/// `sample` is a valid sample buffer of one compressed frame.
unsafe fn read_sample(sample: &CMSampleBuffer, timescale: i32, atom: Option<&str>) -> Delivered {
    // SAFETY: `sample` is valid; its attachments array, when it has one, is
    // of dictionaries keyed by strings; `bytes` has room for the data
    // buffer's whole length.
    unsafe {
        let Some(data) = sample.data_buffer() else {
            return Delivered::Missing("a sample buffer without data");
        };
        let length = data.data_length();
        let mut bytes = vec![0_u8; length];
        if length > 0 {
            let destination = NonNull::from(&mut bytes[..]).cast::<c_void>();
            if data.copy_data_bytes(0, length, destination) != 0 {
                return Delivered::Missing("a sample buffer whose data cannot be read");
            }
        }
        let not_sync = sample
            .sample_attachments_array(false)
            .map(|array| {
                CFRetained::cast_unchecked::<CFArray<CFDictionary<CFString, CFType>>>(array)
            })
            .and_then(|array| array.iter().next())
            .and_then(|attachments| attachments.get(kCMSampleAttachmentKey_NotSync))
            .and_then(|value| value.downcast::<CFBoolean>().ok())
            .is_some_and(|value| value.as_bool());
        Delivered::Sample(Sample {
            pts: ticks(sample.presentation_time_stamp(), timescale),
            dts: ticks(sample.decode_time_stamp(), timescale),
            duration: ticks(sample.duration(), timescale),
            key: !not_sync,
            bytes,
            description: atom.and_then(|atom| description_atom(sample, atom)),
        })
    }
}

/// The output handler: reads each sample buffer into `shared`.
fn handler(shared: Arc<Mutex<Shared>>, timescale: i32, atom: Option<&'static str>) -> Handler {
    RcBlock::new(
        move |status: i32, _flags: VTEncodeInfoFlags, sample: *mut CMSampleBuffer| {
            let mut shared = shared.lock().unwrap_or_else(PoisonError::into_inner);
            let delivered = if status != 0 {
                Delivered::Failed(status)
            } else if let Some(sample) = NonNull::new(sample) {
                let atom = atom.filter(|_| !shared.described);
                // SAFETY: VideoToolbox passes a valid sample buffer, which
                // it keeps alive for the handler's call.
                let delivered = unsafe { read_sample(sample.as_ref(), timescale, atom) };
                if let Delivered::Sample(Sample {
                    description: Some(_),
                    ..
                }) = delivered
                {
                    shared.described = true;
                }
                delivered
            } else {
                Delivered::Missing("a dropped frame")
            };
            shared.delivered.push_back(delivered);
        },
    )
}

/// The attributes the session's pixel buffers have: `format` at `width` x
/// `height`, on IOSurfaces.
fn buffer_attributes(format: u32, width: i32, height: i32) -> CFRetained<CFDictionary> {
    let surface = CFDictionary::<CFString, CFType>::empty();
    let (format, width, height) = (
        CFNumber::new_i32(format as i32),
        CFNumber::new_i32(width),
        CFNumber::new_i32(height),
    );
    // SAFETY (each static): an immutable CFString CoreVideo defines.
    let keys = unsafe {
        [
            kCVPixelBufferPixelFormatTypeKey,
            kCVPixelBufferWidthKey,
            kCVPixelBufferHeightKey,
            kCVPixelBufferIOSurfacePropertiesKey,
        ]
    };
    let values: [&CFType; 4] = [&format, &width, &height, &surface];
    let attributes = CFDictionary::<CFString, CFType>::from_slices(&keys, &values);
    // SAFETY: a dictionary is a dictionary whatever its keys and values.
    unsafe { CFRetained::cast_unchecked::<CFDictionary>(attributes) }
}

/// Opens `codec` for `request`: the session, the input it chose, and
/// whether it keeps alpha.
pub(crate) fn open(
    codec: VideoCodec,
    request: &VideoEncoderRequest<'_>,
) -> Result<(VideoSession, PixelFormat, bool), MediaError> {
    let settings = checked_settings(codec, request.settings)?;
    let hq = codec == VideoCodec::ProRes && prores_hq(&settings);
    let codec_type = if hq { PRORES_HQ } else { codec.codec_type() };
    let encoders = apple::video_encoder_types();
    if !encoders.contains(&codec_type) {
        let reason = if hq {
            "VideoToolbox has no ProRes 422 HQ encoder on this Mac".to_owned()
        } else {
            components::video_availability(codec, &encoders)
                .err()
                .unwrap_or_default()
        };
        return Err(MediaError::ComponentUnavailable {
            component: codec.id().to_owned(),
            reason,
        });
    }
    let (input, alpha) = choose_input(
        codec,
        &settings,
        request.width,
        request.height,
        request.offered,
    )?;
    let properties = vt_settings(codec, &settings)?;
    check_color(request.color)?;
    let rate = request.frame_rate;
    let (Ok(width), Ok(height), Ok(timescale)) = (
        i32::try_from(request.width),
        i32::try_from(request.height),
        i32::try_from(request.timescale),
    ) else {
        return Err(MediaError::Codec(format!(
            "a {}x{} video at a timescale of {} is too large for VideoToolbox",
            request.width, request.height, request.timescale
        )));
    };
    if width == 0 || height == 0 || timescale == 0 || rate.numerator == 0 || rate.denominator == 0 {
        return Err(MediaError::Codec(format!(
            "a {}x{} video at {}/{} fps in a timescale of {} has no frames to encode",
            request.width, request.height, rate.numerator, rate.denominator, request.timescale
        )));
    }

    let attributes = buffer_attributes(pixel_format(input), width, height);
    let mut session: *mut VTCompressionSession = null_mut();
    // SAFETY: the attributes are a dictionary of the documented keys and
    // types; no callback is given (frames are encoded with an output
    // handler); `session` is a valid place for the new session.
    let status = unsafe {
        VTCompressionSession::create(
            None,
            width,
            height,
            codec_type,
            None,
            Some(&attributes),
            None,
            None,
            null_mut(),
            NonNull::from(&mut session),
        )
    };
    check("VTCompressionSessionCreate", status)?;
    let session = NonNull::new(session)
        .ok_or_else(|| MediaError::Codec("VTCompressionSessionCreate gave no session".into()))?;
    // SAFETY: the session is the caller's to release (the Create rule).
    let session = unsafe { CFRetained::from_raw(session) };
    let atom = match codec {
        VideoCodec::H264 => Some("avcC"),
        VideoCodec::Hevc => Some("hvcC"),
        VideoCodec::ProRes => None,
    };
    let shared = Arc::new(Mutex::new(Shared::default()));
    let opened = VideoSession {
        handler: handler(shared.clone(), timescale, atom),
        shared,
        codec,
        prores_family: if hq { "apch" } else { "ap4h" },
        input,
        width: request.width,
        height: request.height,
        timescale,
        frame: i64::from(rate.denominator),
        reorders: properties.contains(&VtSetting::AllowFrameReordering(true)),
        depth: None,
        session,
    };
    // A failure here drops `opened`, which invalidates the session.
    configure(
        &opened.session,
        &properties,
        f64::from(rate.numerator) / f64::from(rate.denominator),
    )?;
    Ok((opened, input, alpha))
}

impl VideoSession {
    /// Encodes `frame`, a tightly packed frame of the session's input, shown
    /// at `pts`: the packets that are ready.
    pub(crate) fn encode(&mut self, pts: i64, frame: &[u8]) -> Result<Encoded, MediaError> {
        let expected = self.input.frame_bytes(self.width, self.height);
        if frame.len() != expected {
            return Err(MediaError::Codec(format!(
                "a {}x{} {} frame is {expected} bytes, not {}",
                self.width,
                self.height,
                self.input,
                frame.len()
            )));
        }
        let buffer = self.pixel_buffer()?;
        self.fill(&buffer, frame)?;
        // SAFETY (each static): an immutable CFString CoreVideo defines.
        let colors = unsafe {
            [
                (
                    kCVImageBufferColorPrimariesKey,
                    kCVImageBufferColorPrimaries_ITU_R_709_2,
                ),
                (
                    kCVImageBufferTransferFunctionKey,
                    kCVImageBufferTransferFunction_ITU_R_709_2,
                ),
                (
                    kCVImageBufferYCbCrMatrixKey,
                    kCVImageBufferYCbCrMatrix_ITU_R_709_2,
                ),
            ]
        };
        for (key, value) in colors {
            // SAFETY: each value is the CFString its key takes.
            unsafe { buffer.set_attachment(key, value, CVAttachmentMode::ShouldPropagate) };
        }
        // SAFETY: `buffer` is a filled pixel buffer the session's pool made;
        // the times are numeric; the handler is a valid block, which
        // VideoToolbox copies for as long as it needs it.
        let status = unsafe {
            self.session.encode_frame_with_output_handler(
                &buffer,
                CMTime::new(pts, self.timescale),
                CMTime::new(self.frame, self.timescale),
                None,
                null_mut(),
                RcBlock::as_ptr(&self.handler),
            )
        };
        check("VTCompressionSessionEncodeFrame", status)?;
        self.drain()
    }

    /// Completes every frame given: the last packets. The session ends.
    pub(crate) fn finish(mut self) -> Result<Encoded, MediaError> {
        // SAFETY: an invalid time completes every pending frame.
        let status = unsafe { self.session.complete_frames(kCMTimeInvalid) };
        check("VTCompressionSessionCompleteFrames", status)?;
        self.drain()
    }

    /// A pixel buffer from the session's pool.
    fn pixel_buffer(&self) -> Result<CFRetained<CVPixelBuffer>, MediaError> {
        // SAFETY: the session is valid until it is dropped.
        let pool = unsafe { self.session.pixel_buffer_pool() }.ok_or_else(|| {
            MediaError::Codec("VTCompressionSessionGetPixelBufferPool gave no pool".into())
        })?;
        let mut buffer: *mut CVPixelBuffer = null_mut();
        // SAFETY: `buffer` is a valid place for the new pixel buffer.
        let status = unsafe {
            CVPixelBufferPool::create_pixel_buffer(None, &pool, NonNull::from(&mut buffer))
        };
        check("CVPixelBufferPoolCreatePixelBuffer", status)?;
        let buffer = NonNull::new(buffer).ok_or_else(|| {
            MediaError::Codec("CVPixelBufferPoolCreatePixelBuffer gave no buffer".into())
        })?;
        // SAFETY: the buffer is the caller's to release (the Create rule).
        Ok(unsafe { CFRetained::from_raw(buffer) })
    }

    /// Copies `frame` into `buffer`, row by row: the pool's rows may be
    /// padded.
    fn fill(&self, buffer: &CVPixelBuffer, frame: &[u8]) -> Result<(), MediaError> {
        let format = CVPixelBufferGetPixelFormatType(buffer);
        if format != pixel_format(self.input) {
            return Err(MediaError::Codec(format!(
                "VideoToolbox's pool gave pixel buffers of {:?}, not {}",
                format.to_be_bytes().map(char::from),
                self.input
            )));
        }
        let lock = CVPixelBufferLockFlags(0);
        // SAFETY: `buffer` is a valid pixel buffer.
        let status = unsafe { CVPixelBufferLockBaseAddress(buffer, lock) };
        check("CVPixelBufferLockBaseAddress", status)?;
        let planes = planes(self.input, self.width as usize, self.height as usize);
        let planar = planes.len() > 1;
        let mut source = frame;
        let mut result = Ok(());
        for (plane, (row_bytes, rows)) in planes.into_iter().enumerate() {
            let (base, stride, buffer_rows) = if planar {
                (
                    CVPixelBufferGetBaseAddressOfPlane(buffer, plane),
                    CVPixelBufferGetBytesPerRowOfPlane(buffer, plane),
                    CVPixelBufferGetHeightOfPlane(buffer, plane),
                )
            } else {
                (
                    CVPixelBufferGetBaseAddress(buffer),
                    CVPixelBufferGetBytesPerRow(buffer),
                    CVPixelBufferGetHeight(buffer),
                )
            };
            if base.is_null() || stride < row_bytes || buffer_rows < rows {
                result = Err(MediaError::Codec(format!(
                    "VideoToolbox's pixel buffer has {buffer_rows} rows of {stride} bytes in \
                     plane {plane}, not {rows} of {row_bytes}"
                )));
                break;
            }
            let (data, rest) = source.split_at(row_bytes * rows);
            source = rest;
            for (row, bytes) in data.chunks_exact(row_bytes).enumerate() {
                // SAFETY: the base address is locked, and the plane has
                // `buffer_rows >= rows` rows of `stride >= row_bytes` bytes;
                // `bytes` is `row_bytes` long and not in the buffer.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        bytes.as_ptr(),
                        base.cast::<u8>().add(row * stride),
                        row_bytes,
                    );
                }
            }
        }
        // SAFETY: the base address was locked with these flags above.
        let status = unsafe { CVPixelBufferUnlockBaseAddress(buffer, lock) };
        result?;
        check("CVPixelBufferUnlockBaseAddress", status)
    }

    /// The packets of every sample the handler has read, the track's config
    /// with the first.
    fn drain(&mut self) -> Result<Encoded, MediaError> {
        let delivered = std::mem::take(
            &mut self
                .shared
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .delivered,
        );
        let mut encoded = Encoded::default();
        for delivered in delivered {
            let sample = match delivered {
                Delivered::Sample(sample) => sample,
                Delivered::Failed(status) => {
                    return Err(failed("VTCompressionSessionEncodeFrame", status));
                }
                Delivered::Missing(what) => {
                    return Err(MediaError::Codec(format!("VideoToolbox gave {what}")));
                }
            };
            if self.depth.is_none() {
                let (config, depth) = self.config(sample.description.as_deref())?;
                encoded.config = Some(config);
                self.depth = Some(depth);
            }
            encoded.packets.push(self.packet(sample)?);
        }
        Ok(encoded)
    }

    /// The track's config and reorder depth, from the first sample's
    /// `description`.
    fn config(&self, description: Option<&[u8]>) -> Result<(TrackConfig, i64), MediaError> {
        let record = |name: &str| {
            description.map(<[u8]>::to_vec).ok_or_else(|| {
                MediaError::Codec(format!("VideoToolbox gave no {name} with its first frame"))
            })
        };
        let (codec, description, depth) = match self.codec {
            VideoCodec::H264 => {
                let avcc = record("avcC")?;
                let depth = params::avc_reorder_depth(&avcc)?;
                (params::avc_codec_string(&avcc)?, Some(avcc), depth)
            }
            VideoCodec::Hevc => {
                let hvcc = record("hvcC")?;
                let depth = params::hevc_reorder_depth(&hvcc)?;
                (params::hevc_codec_string(&hvcc)?, Some(hvcc), depth)
            }
            VideoCodec::ProRes => (self.prores_family.to_owned(), None, Some(0)),
        };
        let depth = if self.reorders {
            depth.unwrap_or(params::UNKNOWN_REORDER_DEPTH)
        } else {
            0
        };
        Ok((TrackConfig { codec, description }, i64::from(depth)))
    }

    /// `sample` as a packet, its decode time shifted back by the reorder
    /// depth.
    fn packet(&self, sample: Sample) -> Result<Packet, MediaError> {
        let pts = sample.pts.ok_or_else(|| {
            MediaError::Codec("VideoToolbox gave a frame without a presentation time".into())
        })?;
        let depth = self.depth.unwrap_or(0);
        let dts = sample.dts.unwrap_or(pts) - depth * self.frame;
        if dts > pts {
            return Err(MediaError::Codec(format!(
                "VideoToolbox gave a frame shown at {pts} that decodes at {dts}, after it is \
                 shown, even with a reorder depth of {depth} frames"
            )));
        }
        Ok(Packet {
            pts,
            dts,
            duration: sample.duration.unwrap_or(self.frame),
            key: sample.key,
            bytes: sample.bytes,
            additional: Vec::new(),
        })
    }
}

impl Drop for VideoSession {
    /// Tears the session down; frames it has not completed are discarded
    /// (an abort), and the handler is called no more.
    fn drop(&mut self) {
        // SAFETY: the session is valid; invalidating one twice is harmless.
        unsafe { self.session.invalidate() };
    }
}
