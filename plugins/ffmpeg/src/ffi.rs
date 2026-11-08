//! Owners of the libavcodec, libavutil, and libswscale objects the encoders
//! use, each freed when dropped, and FFmpeg's error codes as
//! [`MediaError`]s.

use celesta_media::MediaError;
use ffmpeg_sys_next::{
    AV_LOG_ERROR, AVAudioFifo, AVCodec, AVCodecContext, AVFrame, AVPacket, AVPixelFormat,
    AVSampleFormat, SwsContext, av_audio_fifo_alloc, av_audio_fifo_free, av_channel_layout_default,
    av_frame_alloc, av_frame_free, av_frame_get_buffer, av_log_set_level, av_packet_alloc,
    av_packet_free, av_strerror, avcodec_alloc_context3, avcodec_free_context, sws_freeContext,
    sws_getContext,
};
use std::ffi::{CStr, c_char, c_int};
use std::ptr::null_mut;
use std::sync::Once;

/// Keeps FFmpeg's own log to errors, as `main`'s encoders (ez-ffmpeg, which
/// sends it to the `log` crate) did: an encoder's statistics would otherwise
/// fill the plugin's stderr at every export.
pub(crate) fn quiet_logs() {
    static QUIET: Once = Once::new();
    // SAFETY: sets libavutil's global log level.
    QUIET.call_once(|| unsafe { av_log_set_level(AV_LOG_ERROR) });
}

/// What FFmpeg's error `code` says, for the step `what` that failed.
pub(crate) fn av_error(code: c_int, what: &str) -> MediaError {
    let mut message = [0 as c_char; 256];
    // SAFETY: `message` is writable for its length, and `av_strerror`
    // always ends what it writes with a NUL.
    let text = unsafe {
        if av_strerror(code, message.as_mut_ptr(), message.len()) < 0 {
            format!("error {code}")
        } else {
            CStr::from_ptr(message.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    };
    MediaError::Codec(format!("FFmpeg could not {what}: {text}"))
}

/// An `AVCodecContext`, freed when dropped.
pub(crate) struct CodecContext(*mut AVCodecContext);

// SAFETY: the context is owned here alone and used from one thread at a
// time; libavcodec keeps no thread-bound state in it.
unsafe impl Send for CodecContext {}

impl CodecContext {
    pub(crate) fn new(codec: *const AVCodec) -> Result<Self, MediaError> {
        // SAFETY: `codec` is a codec libavcodec gave, or null.
        let context = unsafe { avcodec_alloc_context3(codec) };
        if context.is_null() {
            Err(MediaError::Codec(
                "FFmpeg could not allocate an encoder".to_owned(),
            ))
        } else {
            Ok(Self(context))
        }
    }

    pub(crate) fn as_ptr(&self) -> *mut AVCodecContext {
        self.0
    }
}

impl Drop for CodecContext {
    fn drop(&mut self) {
        // SAFETY: the context was allocated by `avcodec_alloc_context3`.
        unsafe { avcodec_free_context(&mut self.0) }
    }
}

/// An `AVFrame`, freed when dropped.
pub(crate) struct Frame(*mut AVFrame);

// SAFETY: as for `CodecContext`.
unsafe impl Send for Frame {}

impl Frame {
    /// A frame with buffers for `width`x`height` pixels of `format`.
    pub(crate) fn video(
        format: AVPixelFormat,
        width: u32,
        height: u32,
    ) -> Result<Self, MediaError> {
        // SAFETY: a frame from `av_frame_alloc`, whose fields are set before
        // `av_frame_get_buffer` allocates for them.
        unsafe {
            let frame = Self(av_frame_alloc());
            if frame.0.is_null() {
                return Err(MediaError::Codec(
                    "FFmpeg could not allocate a frame".to_owned(),
                ));
            }
            (*frame.0).format = format as c_int;
            (*frame.0).width = dimension(width)?;
            (*frame.0).height = dimension(height)?;
            let result = av_frame_get_buffer(frame.0, 0);
            if result < 0 {
                return Err(av_error(result, "allocate a frame's buffers"));
            }
            Ok(frame)
        }
    }

    /// A frame with buffers for `samples` sample frames of `format` in
    /// `channels` channels (FFmpeg's default layout for the count) at
    /// `sample_rate`.
    pub(crate) fn audio(
        format: AVSampleFormat,
        channels: u16,
        sample_rate: c_int,
        samples: c_int,
    ) -> Result<Self, MediaError> {
        // SAFETY: as for `Frame::video`; the layout is a native one, which
        // owns no memory.
        unsafe {
            let frame = Self(av_frame_alloc());
            if frame.0.is_null() {
                return Err(MediaError::Codec(
                    "FFmpeg could not allocate a frame".to_owned(),
                ));
            }
            (*frame.0).format = format as c_int;
            av_channel_layout_default(&mut (*frame.0).ch_layout, c_int::from(channels));
            (*frame.0).sample_rate = sample_rate;
            (*frame.0).nb_samples = samples;
            let result = av_frame_get_buffer(frame.0, 0);
            if result < 0 {
                return Err(av_error(result, "allocate a frame's buffers"));
            }
            Ok(frame)
        }
    }

    pub(crate) fn as_ptr(&self) -> *mut AVFrame {
        self.0
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // SAFETY: the frame was allocated by `av_frame_alloc`.
        unsafe { av_frame_free(&mut self.0) }
    }
}

/// An `AVPacket`, freed when dropped.
pub(crate) struct PacketBuf(*mut AVPacket);

// SAFETY: as for `CodecContext`.
unsafe impl Send for PacketBuf {}

impl PacketBuf {
    pub(crate) fn new() -> Result<Self, MediaError> {
        // SAFETY: allocates an empty packet.
        let packet = unsafe { av_packet_alloc() };
        if packet.is_null() {
            Err(MediaError::Codec(
                "FFmpeg could not allocate a packet".to_owned(),
            ))
        } else {
            Ok(Self(packet))
        }
    }

    pub(crate) fn as_ptr(&self) -> *mut AVPacket {
        self.0
    }
}

impl Drop for PacketBuf {
    fn drop(&mut self) {
        // SAFETY: the packet was allocated by `av_packet_alloc`.
        unsafe { av_packet_free(&mut self.0) }
    }
}

/// An `AVAudioFifo`, freed when dropped.
pub(crate) struct AudioFifo(*mut AVAudioFifo);

// SAFETY: as for `CodecContext`.
unsafe impl Send for AudioFifo {}

impl AudioFifo {
    pub(crate) fn new(format: AVSampleFormat, channels: u16) -> Result<Self, MediaError> {
        // SAFETY: allocates an empty FIFO, which grows as it is written.
        let fifo = unsafe { av_audio_fifo_alloc(format, c_int::from(channels), 1) };
        if fifo.is_null() {
            Err(MediaError::Codec(
                "FFmpeg could not allocate an audio buffer".to_owned(),
            ))
        } else {
            Ok(Self(fifo))
        }
    }

    pub(crate) fn as_ptr(&self) -> *mut AVAudioFifo {
        self.0
    }
}

impl Drop for AudioFifo {
    fn drop(&mut self) {
        // SAFETY: the FIFO was allocated by `av_audio_fifo_alloc`.
        unsafe { av_audio_fifo_free(self.0) }
    }
}

/// An opened encoder's extradata, empty when it has none.
///
/// # Safety
///
/// `context` is an opened `AVCodecContext`.
pub(crate) unsafe fn extradata(context: *const AVCodecContext) -> Vec<u8> {
    // SAFETY: an opened encoder's extradata is `extradata_size` bytes, or
    // null.
    unsafe {
        match usize::try_from((*context).extradata_size) {
            Ok(size) if !(*context).extradata.is_null() => {
                std::slice::from_raw_parts((*context).extradata, size).to_vec()
            }
            _ => Vec::new(),
        }
    }
}

/// A libswscale context converting frames of one size between two formats,
/// freed when dropped.
pub(crate) struct Scaler(*mut SwsContext);

// SAFETY: as for `CodecContext`.
unsafe impl Send for Scaler {}

impl Scaler {
    /// Converts `width`x`height` frames from `source` to `destination` with
    /// `flags` (`SWS_*`).
    pub(crate) fn new(
        width: u32,
        height: u32,
        source: AVPixelFormat,
        destination: AVPixelFormat,
        flags: c_int,
    ) -> Result<Self, MediaError> {
        let (width, height) = (dimension(width)?, dimension(height)?);
        // SAFETY: no filters and no parameters, which libswscale allows.
        let context = unsafe {
            sws_getContext(
                width,
                height,
                source,
                width,
                height,
                destination,
                flags,
                null_mut(),
                null_mut(),
                std::ptr::null(),
            )
        };
        if context.is_null() {
            Err(MediaError::Codec(format!(
                "FFmpeg cannot convert {source:?} to {destination:?}"
            )))
        } else {
            Ok(Self(context))
        }
    }

    pub(crate) fn as_ptr(&self) -> *mut SwsContext {
        self.0
    }
}

impl Drop for Scaler {
    fn drop(&mut self) {
        // SAFETY: the context was allocated by `sws_getContext`.
        unsafe { sws_freeContext(self.0) }
    }
}

/// A frame dimension as FFmpeg takes it.
pub(crate) fn dimension(value: u32) -> Result<c_int, MediaError> {
    c_int::try_from(value)
        .map_err(|_| MediaError::Codec(format!("a frame dimension of {value} is too large")))
}
