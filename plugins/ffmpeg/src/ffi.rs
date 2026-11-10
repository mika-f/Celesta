//! Owners of the libavcodec, libavutil, and libswscale objects the encoders
//! use, each freed when dropped, and FFmpeg's error codes as
//! [`MediaError`]s.

use crate::tags::AvColor;
use celesta_media::MediaError;
use ffmpeg_sys_next::{
    AV_BUFFER_FLAG_READONLY, AV_LOG_ERROR, AVAudioFifo, AVCodec, AVCodecContext, AVFrame, AVPacket,
    AVPixelFormat, AVSampleFormat, SwsContext, SwsFlags, av_audio_fifo_alloc, av_audio_fifo_free,
    av_buffer_create, av_channel_layout_default, av_frame_alloc, av_frame_free,
    av_frame_get_buffer, av_image_fill_arrays, av_log_set_level, av_packet_alloc, av_packet_free,
    av_strerror, avcodec_alloc_context3, avcodec_free_context, sws_alloc_context, sws_freeContext,
    sws_init_context, sws_scale_frame, sws_setColorspaceDetails,
};
use std::ffi::{CStr, c_char, c_int, c_uint, c_void};
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

/// A libswscale context converting frames of one size between two formats
/// on libswscale's own threads, freed when dropped.
///
/// The threads are what `main`'s `scale` filter had (libavfilter gives it
/// the graph's automatic thread count): each converts a band of the frame's
/// rows with the same filters, so the output is the single-threaded one.
pub(crate) struct Scaler {
    context: *mut SwsContext,
    source: AVPixelFormat,
    width: c_int,
    height: c_int,
}

// SAFETY: as for `CodecContext`.
unsafe impl Send for Scaler {}

impl Scaler {
    /// Converts `width`x`height` frames from `source` to `destination` with
    /// `flags` (`SWS_*`).
    fn new(
        width: u32,
        height: u32,
        source: AVPixelFormat,
        destination: AVPixelFormat,
        flags: c_int,
    ) -> Result<Self, MediaError> {
        let (width, height) = (dimension(width)?, dimension(height)?);
        // SAFETY: a context from `sws_alloc_context`, its fields set as
        // `sws_getContext` sets them (and its thread count automatic)
        // before `sws_init_context`, with no filters; it is freed on
        // failure.
        unsafe {
            let context = sws_alloc_context();
            if context.is_null() {
                return Err(MediaError::Codec(
                    "FFmpeg could not allocate a scaler".to_owned(),
                ));
            }
            let scaler = Self {
                context,
                source,
                width,
                height,
            };
            (*context).src_w = width;
            (*context).src_h = height;
            (*context).src_format = source as c_int;
            (*context).dst_w = width;
            (*context).dst_h = height;
            (*context).dst_format = destination as c_int;
            (*context).flags = flags as c_uint;
            (*context).threads = 0;
            // The explicitly initialized path, which FFmpeg 8 keeps for
            // compatibility, on purpose: it converts as `sws_getContext`
            // did, whose bytes were checked against `main`'s; a context
            // left uninitialized takes FFmpeg 8's dynamic path instead.
            if sws_init_context(context, null_mut(), null_mut()) < 0 {
                return Err(MediaError::Codec(format!(
                    "FFmpeg cannot convert {source:?} to {destination:?}"
                )));
            }
            Ok(scaler)
        }
    }

    /// Converts full-range RGB `source` to YUV `destination` with `color`'s
    /// matrix and range, bicubically filtering the chroma it halves.
    pub(crate) fn rgb_to_yuv(
        width: u32,
        height: u32,
        source: AVPixelFormat,
        destination: AVPixelFormat,
        color: &AvColor,
    ) -> Result<Self, MediaError> {
        let (table, range) = color.sws_coefficients()?;
        let scaler = Self::new(
            width,
            height,
            source,
            destination,
            SwsFlags::SWS_BICUBIC as c_int,
        )?;
        // SAFETY: the table is libswscale's own static; the details reach
        // the context's per-thread contexts too. The source table goes
        // unused for RGB input; it is the destination's, as ProRes's and
        // VP9's BT.709 conversion has always set it, so their bytes stay
        // the same.
        let result = unsafe {
            sws_setColorspaceDetails(scaler.context, table, 1, table, range, 0, 1 << 16, 1 << 16)
        };
        if result < 0 {
            return Err(av_error(result, "set the color conversion"));
        }
        Ok(scaler)
    }

    /// Moves YUV `source` samples into `destination`'s layout, of the same
    /// matrix, range, and subsampling (P010 into planar 10-bit 4:2:0), with
    /// no filtering.
    pub(crate) fn repack(
        width: u32,
        height: u32,
        source: AVPixelFormat,
        destination: AVPixelFormat,
    ) -> Result<Self, MediaError> {
        Self::new(
            width,
            height,
            source,
            destination,
            SwsFlags::SWS_POINT as c_int,
        )
    }

    /// Converts `pixels`, a tightly packed frame of the source format,
    /// into `destination`.
    ///
    /// # Safety
    ///
    /// `destination` is a writable frame of the destination format and
    /// size, and `pixels` holds a whole frame of the source.
    pub(crate) unsafe fn convert(
        &self,
        destination: *mut AVFrame,
        pixels: &[u8],
    ) -> Result<(), MediaError> {
        /// Leaves the pixels to their owner: the buffer only lends them.
        unsafe extern "C" fn lent(_: *mut c_void, _: *mut u8) {}
        // SAFETY: the source frame borrows `pixels` through a read-only
        // buffer whose free does nothing; libswscale drops its references
        // to both frames before `sws_scale_frame` returns, and the frame's
        // own is dropped with it here, while `pixels` is still borrowed.
        unsafe {
            let source = Self::frame()?;
            let raw = source.as_ptr();
            (*raw).format = self.source as c_int;
            (*raw).width = self.width;
            (*raw).height = self.height;
            let result = av_image_fill_arrays(
                (*raw).data.as_mut_ptr(),
                (*raw).linesize.as_mut_ptr(),
                pixels.as_ptr(),
                self.source,
                self.width,
                self.height,
                1,
            );
            if result < 0 {
                return Err(av_error(result, "read the frame"));
            }
            (*raw).buf[0] = av_buffer_create(
                pixels.as_ptr().cast_mut(),
                pixels.len(),
                Some(lent),
                null_mut(),
                AV_BUFFER_FLAG_READONLY,
            );
            if (*raw).buf[0].is_null() {
                return Err(MediaError::Codec(
                    "FFmpeg could not allocate a frame".to_owned(),
                ));
            }
            let result = sws_scale_frame(self.context, destination, raw);
            if result < 0 {
                return Err(av_error(result, "convert the frame"));
            }
        }
        Ok(())
    }

    /// An empty frame.
    fn frame() -> Result<Frame, MediaError> {
        // SAFETY: allocates an empty frame.
        let frame = Frame(unsafe { av_frame_alloc() });
        if frame.0.is_null() {
            Err(MediaError::Codec(
                "FFmpeg could not allocate a frame".to_owned(),
            ))
        } else {
            Ok(frame)
        }
    }
}

impl Drop for Scaler {
    fn drop(&mut self) {
        // SAFETY: the context was allocated by `sws_alloc_context`.
        unsafe { sws_freeContext(self.context) }
    }
}

/// A frame dimension as FFmpeg takes it.
pub(crate) fn dimension(value: u32) -> Result<c_int, MediaError> {
    c_int::try_from(value)
        .map_err(|_| MediaError::Codec(format!("a frame dimension of {value} is too large")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ffmpeg_sys_next::{
        SWS_CS_ITU709, av_image_copy_to_buffer, av_image_get_buffer_size, sws_getCoefficients,
        sws_getContext, sws_scale,
    };

    /// `frame`'s pixels, tightly packed.
    fn packed(frame: &Frame, format: AVPixelFormat, width: c_int, height: c_int) -> Vec<u8> {
        // SAFETY: `frame` holds `width`x`height` pixels of `format`, and the
        // buffer is sized for them.
        unsafe {
            let size = av_image_get_buffer_size(format, width, height, 1);
            let mut bytes = vec![0; usize::try_from(size).unwrap()];
            let raw = frame.as_ptr();
            let written = av_image_copy_to_buffer(
                bytes.as_mut_ptr(),
                size,
                (*raw).data.as_ptr().cast(),
                (*raw).linesize.as_ptr(),
                format,
                width,
                height,
                1,
            );
            assert_eq!(written, size);
            bytes
        }
    }

    /// The conversion on libswscale's threads gives the bytes one thread
    /// gives (`sws_getContext` and `sws_scale`, as before the threads), for
    /// every conversion the video encoders make, at a height that splits
    /// into uneven bands.
    #[test]
    fn threaded_conversions_equal_single_threaded_ones() {
        use AVPixelFormat::*;
        const WIDTH: u32 = 642;
        const HEIGHT: u32 = 362;
        let (width, height) = (WIDTH as c_int, HEIGHT as c_int);
        let color = AvColor::from_color(&celesta_media::Color::bt709_limited()).unwrap();
        // Whether each converts RGB to YUV, rather than repacking YUV.
        for (source, destination, rgb) in [
            (AV_PIX_FMT_RGBA, AV_PIX_FMT_YUV420P, true),
            (AV_PIX_FMT_RGBA64LE, AV_PIX_FMT_YUV420P, true),
            (AV_PIX_FMT_RGBA, AV_PIX_FMT_YUV420P10LE, true),
            (AV_PIX_FMT_RGBA64LE, AV_PIX_FMT_YUV420P10LE, true),
            (AV_PIX_FMT_RGBA64LE, AV_PIX_FMT_YUV422P10LE, true),
            (AV_PIX_FMT_RGBA64LE, AV_PIX_FMT_YUVA444P10LE, true),
            (AV_PIX_FMT_RGBA64LE, AV_PIX_FMT_YUVA420P, true),
            (AV_PIX_FMT_P010LE, AV_PIX_FMT_YUV420P10LE, false),
        ] {
            // SAFETY: a size query on a valid format and dimensions.
            let bytes = unsafe { av_image_get_buffer_size(source, width, height, 1) };
            let mut seed = 7u32;
            let pixels: Vec<u8> = (0..bytes)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed >> 24) as u8
                })
                .collect();
            let (scaler, flags) = if rgb {
                (
                    Scaler::rgb_to_yuv(WIDTH, HEIGHT, source, destination, &color).unwrap(),
                    SwsFlags::SWS_BICUBIC as c_int,
                )
            } else {
                (
                    Scaler::repack(WIDTH, HEIGHT, source, destination).unwrap(),
                    SwsFlags::SWS_POINT as c_int,
                )
            };
            let threaded = Frame::video(destination, WIDTH, HEIGHT).unwrap();
            // SAFETY: the frame is the conversion's destination, and the
            // pixels a whole source frame.
            unsafe { scaler.convert(threaded.as_ptr(), &pixels) }.unwrap();

            let one = Frame::video(destination, WIDTH, HEIGHT).unwrap();
            // SAFETY: a single-threaded context for the same conversion,
            // fed the same tightly packed frame.
            unsafe {
                let context = sws_getContext(
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
                );
                assert!(!context.is_null());
                if rgb {
                    let table = sws_getCoefficients(SWS_CS_ITU709);
                    sws_setColorspaceDetails(context, table, 1, table, 0, 0, 1 << 16, 1 << 16);
                }
                let mut data = [null_mut::<u8>(); 4];
                let mut strides = [0 as c_int; 4];
                av_image_fill_arrays(
                    data.as_mut_ptr(),
                    strides.as_mut_ptr(),
                    pixels.as_ptr(),
                    source,
                    width,
                    height,
                    1,
                );
                let raw = one.as_ptr();
                let rows = sws_scale(
                    context,
                    data.as_ptr().cast(),
                    strides.as_ptr(),
                    0,
                    height,
                    (*raw).data.as_ptr(),
                    (*raw).linesize.as_ptr(),
                );
                assert_eq!(rows, height);
                sws_freeContext(context);
            }
            assert!(
                packed(&threaded, destination, width, height)
                    == packed(&one, destination, width, height),
                "{source:?} to {destination:?} differs on threads"
            );
        }
    }

    /// Repacking P010 keeps each sample's ten bits as they are.
    #[test]
    fn repacking_p010_keeps_its_samples() {
        use AVPixelFormat::*;
        const WIDTH: u32 = 6;
        const HEIGHT: u32 = 4;
        let luma: Vec<u16> = (0..WIDTH * HEIGHT)
            .map(|index| 64 + index as u16 * 37)
            .collect();
        let chroma: Vec<u16> = (0..WIDTH * HEIGHT / 2)
            .map(|index| 960 - index as u16 * 41)
            .collect();
        let pixels: Vec<u8> = luma
            .iter()
            .chain(&chroma)
            .flat_map(|sample| (sample << 6).to_le_bytes())
            .collect();
        let scaler =
            Scaler::repack(WIDTH, HEIGHT, AV_PIX_FMT_P010LE, AV_PIX_FMT_YUV420P10LE).unwrap();
        let frame = Frame::video(AV_PIX_FMT_YUV420P10LE, WIDTH, HEIGHT).unwrap();
        // SAFETY: as in the test above.
        unsafe { scaler.convert(frame.as_ptr(), &pixels) }.unwrap();
        let planar: Vec<u16> = packed(
            &frame,
            AV_PIX_FMT_YUV420P10LE,
            WIDTH as c_int,
            HEIGHT as c_int,
        )
        .chunks(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
        let (u, v): (Vec<u16>, Vec<u16>) = chroma.chunks(2).map(|pair| (pair[0], pair[1])).unzip();
        assert_eq!(planar, [luma, u, v].concat());
    }
}
