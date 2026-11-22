use crate::color::{conversion, video_color};
use crate::ffmpeg_error;
use crate::sequential::{ffmpeg_path, seconds_to_us};
use celesta_media::{
    DecodedColor, DecodedMatrix, DecodedPixelFormat, DecodedRange, MediaError, VideoFrame,
};
use ez_ffmpeg::core::context::output::VSyncMethod;
use ez_ffmpeg::core::scheduler::ffmpeg_scheduler::{FfmpegScheduler, Running};
use ez_ffmpeg::filter::frame_filter::{FrameFilter, FrameFilterError, RequestFrameMode};
use ez_ffmpeg::filter::frame_filter_context::FrameFilterContext;
use ez_ffmpeg::filter::frame_pipeline_builder::FramePipelineBuilder;
use ez_ffmpeg::util::ffmpeg_utils::frame_is_eof_marker;
use ez_ffmpeg::{AVMediaType, FfmpegContext, Frame, Input, Output};
use ffmpeg_sys_next::{
    AVChromaLocation, AVColorRange, AVColorSpace, AVFrame, AVPacketSideDataType, AVPixelFormat,
    av_dict_get, av_packet_side_data_get, avformat_close_input, avformat_find_stream_info,
    avformat_open_input,
};
use std::ffi::{CStr, CString};
use std::path::Path;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

pub(crate) fn supports_source(path: &Path, index: u32) -> bool {
    let Ok(path) = CString::new(ffmpeg_path(path)) else {
        return false;
    };
    let mut context = std::ptr::null_mut();
    // SAFETY: FFmpeg owns the opened context until close_input. Stream and
    // side-data bounds are checked before reading pointers owned by it.
    unsafe {
        if avformat_open_input(
            &mut context,
            path.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ) < 0
        {
            return false;
        }
        let supported = if avformat_find_stream_info(context, std::ptr::null_mut()) < 0
            || index >= (*context).nb_streams
        {
            false
        } else {
            let stream = &**(*context).streams.add(index as usize);
            let alpha = av_dict_get(stream.metadata, c"alpha_mode".as_ptr(), std::ptr::null(), 0);
            let codec = &*stream.codecpar;
            let side = av_packet_side_data_get(
                codec.coded_side_data,
                codec.nb_coded_side_data,
                AVPacketSideDataType::AV_PKT_DATA_DISPLAYMATRIX,
            );
            let upright = side.is_null()
                || ((*side).size >= 36
                    && !(*side).data.is_null()
                    && [65536i32, 0, 0, 0, 65536, 0, 0, 0, 1073741824]
                        .into_iter()
                        .enumerate()
                        .all(|(index, value)| {
                            std::ptr::read_unaligned((*side).data.add(index * 4).cast::<i32>())
                                == value
                        }));
            upright && (alpha.is_null() || CStr::from_ptr((*alpha).value).to_bytes() == b"0")
        };
        avformat_close_input(&mut context);
        supported
    }
}

pub(crate) struct NativeFrame {
    pub(crate) pts_us: Option<i64>,
    pub(crate) frame: Option<VideoFrame>,
}

pub(crate) struct NativeFrames {
    rx: Option<Receiver<NativeFrame>>,
    scheduler: Option<FfmpegScheduler<Running>>,
}

impl NativeFrames {
    pub(crate) fn spawn(
        path: &Path,
        stream: u32,
        start: f64,
        max_frames: Option<u64>,
        accepted: &[DecodedPixelFormat],
    ) -> Result<Self, MediaError> {
        let (tx, rx) = sync_channel(1);
        let sink = FramePipelineBuilder::new(AVMediaType::AVMEDIA_TYPE_VIDEO)
            .filter(
                "celesta_native_yuv",
                Box::new(NativeSink {
                    tx,
                    accepted: accepted.to_vec(),
                    remaining: max_frames,
                }),
            )
            .set_stream_index(0)
            .build();
        let mut output = Output::from("-")
            .set_format("null")
            .set_vsync_method(VSyncMethod::VsyncPassthrough)
            .add_stream_map("[native]")
            .add_frame_pipeline(sink);
        if let Some(max) = max_frames.and_then(|n| i64::try_from(n).ok()) {
            output = output.set_max_video_frames(Some(max));
        }
        let context = FfmpegContext::builder()
            .input(Input::from(ffmpeg_path(path)).set_start_time_us(seconds_to_us(start)))
            .filter_desc(format!("[0:{stream}]null[native]"))
            .output(output)
            .build()
            .map_err(ffmpeg_error)?;
        let scheduler = FfmpegScheduler::new(context)
            .start()
            .map_err(ffmpeg_error)?;
        Ok(Self {
            rx: Some(rx),
            scheduler: Some(scheduler),
        })
    }

    pub(crate) fn next(&mut self) -> Option<Result<NativeFrame, MediaError>> {
        if let Ok(frame) = self.rx.as_ref()?.recv() {
            return Some(Ok(frame));
        }
        self.rx = None;
        self.scheduler
            .take()?
            .wait()
            .err()
            .map(|error| Err(ffmpeg_error(error)))
    }
}

impl Drop for NativeFrames {
    fn drop(&mut self) {
        self.rx = None;
        if let Some(scheduler) = self.scheduler.take() {
            scheduler.abort();
        }
    }
}

struct NativeSink {
    tx: SyncSender<NativeFrame>,
    accepted: Vec<DecodedPixelFormat>,
    remaining: Option<u64>,
}

impl FrameFilter for NativeSink {
    fn media_type(&self) -> AVMediaType {
        AVMediaType::AVMEDIA_TYPE_VIDEO
    }
    fn request_frame_mode(&self) -> RequestFrameMode {
        RequestFrameMode::Never
    }
    fn filter_frame(
        &mut self,
        frame: Frame,
        _ctx: &mut FrameFilterContext,
    ) -> Result<Option<Frame>, FrameFilterError> {
        if frame_is_eof_marker(&frame) || self.remaining == Some(0) {
            return Ok(Some(frame));
        }
        // SAFETY: the EOF marker was excluded and the owning frame stays alive.
        let source = unsafe { &*frame.as_ptr() };
        let native = NativeFrame {
            pts_us: crate::converted::pts_us(source),
            frame: pack(source, &self.accepted)?,
        };
        if self.tx.send(native).is_ok()
            && let Some(remaining) = &mut self.remaining
        {
            *remaining -= 1;
        }
        Ok(Some(frame))
    }
}

fn pack(
    frame: &AVFrame,
    accepted: &[DecodedPixelFormat],
) -> Result<Option<VideoFrame>, FrameFilterError> {
    if frame.width <= 0 || frame.height <= 0 || frame.width % 2 != 0 || frame.height % 2 != 0 {
        return Ok(None);
    }
    let format = if frame.format == AVPixelFormat::AV_PIX_FMT_YUV420P as i32
        || frame.format == AVPixelFormat::AV_PIX_FMT_YUVJ420P as i32
    {
        DecodedPixelFormat::Yuv420p
    } else if frame.format == AVPixelFormat::AV_PIX_FMT_NV12 as i32 {
        DecodedPixelFormat::Nv12
    } else {
        return Ok(None);
    };
    if !accepted.contains(&format)
        || conversion(&video_color(
            frame.color_primaries as i32,
            frame.color_trc as i32,
            frame.colorspace as i32,
        ))
        .is_some()
    {
        return Ok(None);
    }
    let matrix = match frame.colorspace {
        AVColorSpace::AVCOL_SPC_BT709 => DecodedMatrix::Bt709,
        AVColorSpace::AVCOL_SPC_UNSPECIFIED
        | AVColorSpace::AVCOL_SPC_BT470BG
        | AVColorSpace::AVCOL_SPC_SMPTE170M => DecodedMatrix::Bt601,
        _ => return Ok(None),
    };
    if !matches!(
        frame.chroma_location,
        AVChromaLocation::AVCHROMA_LOC_LEFT | AVChromaLocation::AVCHROMA_LOC_UNSPECIFIED
    ) {
        return Ok(None);
    }
    let range = match frame.color_range {
        AVColorRange::AVCOL_RANGE_JPEG => DecodedRange::Full,
        AVColorRange::AVCOL_RANGE_MPEG | AVColorRange::AVCOL_RANGE_UNSPECIFIED => {
            if frame.format == AVPixelFormat::AV_PIX_FMT_YUVJ420P as i32 {
                DecodedRange::Full
            } else {
                DecodedRange::Limited
            }
        }
        _ => return Ok(None),
    };
    let (width, height) = (frame.width as u32, frame.height as u32);
    let mut pixels = Vec::with_capacity(
        format
            .frame_bytes(width, height)
            .map_err(|e| e.to_string())?,
    );
    let (w, h) = (width as usize, height as usize);
    let planes: &[(usize, usize, usize)] = if format == DecodedPixelFormat::Nv12 {
        &[(0, w, h), (1, w, h / 2)]
    } else {
        &[(0, w, h), (1, w / 2, h / 2), (2, w / 2, h / 2)]
    };
    for &(plane, row, rows) in planes {
        if frame.data[plane].is_null() || frame.linesize[plane].unsigned_abs() < row as u32 {
            return Err("native frame has no usable pixel plane".into());
        }
        for y in 0..rows {
            // SAFETY: FFmpeg owns the plane for this call. Signed stride handles bottom-up rows.
            let line = unsafe {
                std::slice::from_raw_parts(
                    frame.data[plane].offset(y as isize * frame.linesize[plane] as isize),
                    row,
                )
            };
            pixels.extend_from_slice(line);
        }
    }
    VideoFrame::new(
        width,
        height,
        format,
        Some(DecodedColor {
            matrix,
            range,
            chroma_location: Default::default(),
        }),
        pixels,
    )
    .map(Some)
    .map_err(|error| error.to_string().into())
}
