//! A decode run for streams the packed RGBA export cannot show correctly
//! (HDR or BT.2020): FFmpeg converts each frame to 16-bit R'G'B', and
//! [`ToneMapper`] maps it to 8-bit sRGB on FFmpeg's own output thread, so
//! the conversion of the next frame overlaps the rendering of this one.

use crate::color::{ColorConversion, DEFAULT_PEAK_LUMINANCE, ToneMapper};
use crate::sequential::{ffmpeg_path, seconds_to_us};
use ez_ffmpeg::core::context::output::VSyncMethod;
use ez_ffmpeg::core::scheduler::ffmpeg_scheduler::{FfmpegScheduler, Running};
use ez_ffmpeg::filter::frame_filter::{FrameFilter, FrameFilterError, RequestFrameMode};
use ez_ffmpeg::filter::frame_filter_context::FrameFilterContext;
use ez_ffmpeg::filter::frame_pipeline_builder::FramePipelineBuilder;
use ez_ffmpeg::util::ffmpeg_utils::frame_is_eof_marker;
use ez_ffmpeg::{AVMediaType, FfmpegContext, Frame, Input, Output};
use ffmpeg_sys_next::{
    AV_NOPTS_VALUE, AVFrame, AVFrameSideDataType, AVPixelFormat, AVRational,
    av_frame_get_side_data, av_rescale_q,
};
use std::path::Path;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

/// One converted frame: tightly packed RGBA8 rows.
pub(crate) struct ConvertedFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// The presentation time relative to the run's start, like
    /// `ez_ffmpeg`'s `VideoFrame::pts_us`.
    pub(crate) pts_us: Option<i64>,
    pub(crate) pixels: Vec<u8>,
}

/// Streams [`ConvertedFrame`]s out of a running FFmpeg job. Dropping it
/// drops the receiver before aborting the job, so a sink blocked on the
/// bounded channel is released first.
pub(crate) struct ConvertedFrames {
    rx: Option<Receiver<ConvertedFrame>>,
    scheduler: Option<FfmpegScheduler<Running>>,
}

impl ConvertedFrames {
    /// Starts decoding stream `stream_index` of `path` from
    /// `start_seconds`, the way `FrameExtractor` does: a container seek
    /// re-zeroes the timeline at the request and the input trim drops the
    /// lead-in before it.
    pub(crate) fn spawn(
        path: &Path,
        stream_index: u32,
        start_seconds: f64,
        max_frames: Option<u64>,
        conversion: ColorConversion,
    ) -> Result<Self, ez_ffmpeg::error::Error> {
        let (tx, rx) = sync_channel(1);
        let sink = FramePipelineBuilder::new(AVMediaType::AVMEDIA_TYPE_VIDEO)
            .filter(
                "celesta_color_conversion",
                Box::new(ConversionSink {
                    tx,
                    conversion,
                    mapper: None,
                    remaining: max_frames,
                }),
            )
            .set_stream_index(0)
            .build();
        let mut output = Output::from("-")
            .set_format("null")
            .set_vsync_method(VSyncMethod::VsyncPassthrough)
            .add_stream_map("[converted]")
            .add_frame_pipeline(sink);
        if let Some(max) = max_frames.and_then(|max| i64::try_from(max).ok()) {
            output = output.set_max_video_frames(Some(max));
        }
        // `scale` reads the frame's matrix and range tags; the transfer and
        // primaries pass through untouched for `ToneMapper` to apply.
        let filter = format!(
            "[0:{stream_index}]scale=iw:ih:flags=bicubic:in_color_matrix=auto:in_range=auto:\
             out_range=full,format=rgb48le[converted]"
        );
        let context = FfmpegContext::builder()
            .input(Input::from(ffmpeg_path(path)).set_start_time_us(seconds_to_us(start_seconds)))
            .filter_desc(filter)
            .output(output)
            .build()?;
        let scheduler = FfmpegScheduler::new(context).start()?;
        Ok(Self {
            rx: Some(rx),
            scheduler: Some(scheduler),
        })
    }
}

impl Iterator for ConvertedFrames {
    type Item = Result<ConvertedFrame, ez_ffmpeg::error::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Ok(frame) = self.rx.as_ref()?.recv() {
            return Some(Ok(frame));
        }
        // The job finished or failed: join it once and surface its error.
        self.rx = None;
        self.scheduler.take()?.wait().err().map(Err)
    }
}

impl Drop for ConvertedFrames {
    fn drop(&mut self) {
        self.rx = None;
        if let Some(scheduler) = self.scheduler.take() {
            scheduler.abort();
        }
    }
}

struct ConversionSink {
    tx: SyncSender<ConvertedFrame>,
    conversion: ColorConversion,
    /// Rebuilt when a frame declares a different peak luminance.
    mapper: Option<(u32, ToneMapper)>,
    remaining: Option<u64>,
}

impl FrameFilter for ConversionSink {
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
        // SAFETY: `frame_is_eof_marker` rejected a null frame, and the frame
        // stays alive (and unmodified) for the whole call.
        let converted = unsafe { self.convert(&*frame.as_ptr())? };
        // A closed receiver means the run is being dropped; keep forwarding
        // so the job winds down normally.
        if self.tx.send(converted).is_ok()
            && let Some(remaining) = &mut self.remaining
        {
            *remaining -= 1;
        }
        Ok(Some(frame))
    }
}

impl ConversionSink {
    /// # Safety
    /// `frame` must be a valid decoded video frame.
    unsafe fn convert(&mut self, frame: &AVFrame) -> Result<ConvertedFrame, FrameFilterError> {
        if frame.format != AVPixelFormat::AV_PIX_FMT_RGB48LE as i32 {
            return Err(
                format!("expected an RGB48 frame, got pixel format {}", frame.format).into(),
            );
        }
        let (width, height) = (frame.width.max(0) as usize, frame.height.max(0) as usize);
        let row = width * 6;
        let stride = frame.linesize[0];
        if stride < row as i32 || frame.data[0].is_null() {
            return Err("RGB48 frame has no usable pixel plane".into());
        }
        // SAFETY: plane 0 of a packed RGB48 frame holds `height` rows of
        // `linesize[0]` bytes, each starting with `width` pixels.
        let rows = unsafe { std::slice::from_raw_parts(frame.data[0], stride as usize * height) };
        let packed;
        let rgb48 = if stride as usize == row {
            rows
        } else {
            packed = rows
                .chunks(stride as usize)
                .flat_map(|line| &line[..row])
                .copied()
                .collect::<Vec<_>>();
            &packed
        };

        // SAFETY: `frame` is valid for reading its side data.
        let peak = unsafe { peak_luminance(frame) };
        if self.mapper.as_ref().is_none_or(|(known, _)| *known != peak) {
            self.mapper = Some((peak, ToneMapper::new(self.conversion, peak as f32)));
        }
        let mapper = &self.mapper.as_ref().expect("mapper was just built").1;
        let mut pixels = vec![0; width * height * 4];
        mapper.convert(rgb48, width, &mut pixels);
        Ok(ConvertedFrame {
            width: width as u32,
            height: height as u32,
            pts_us: pts_us(frame),
            pixels,
        })
    }
}

fn pts_us(frame: &AVFrame) -> Option<i64> {
    if frame.pts == AV_NOPTS_VALUE || frame.time_base.den == 0 {
        return None;
    }
    // SAFETY: a pure arithmetic helper.
    Some(unsafe {
        av_rescale_q(
            frame.pts,
            frame.time_base,
            AVRational {
                num: 1,
                den: 1_000_000,
            },
        )
    })
}

/// The content's peak luminance in nits, as Skia picks it: MaxCLL, else
/// the mastering display's peak, else 1000 nits.
///
/// # Safety
/// `frame` must be a valid frame.
unsafe fn peak_luminance(frame: &AVFrame) -> u32 {
    let side_data = |kind| {
        // SAFETY: FFmpeg only reads the frame's side data array.
        let data = unsafe { av_frame_get_side_data(frame, kind) };
        // SAFETY: a non-null entry points at a valid side data struct.
        (!data.is_null()).then(|| unsafe { (*data).data })
    };
    if let Some(data) = side_data(AVFrameSideDataType::AV_FRAME_DATA_CONTENT_LIGHT_LEVEL) {
        // SAFETY: this side data type holds an `AVContentLightMetadata`.
        let light = unsafe { &*data.cast::<ContentLightMetadata>() };
        if light.max_cll > 0 {
            return light.max_cll;
        }
    }
    if let Some(data) = side_data(AVFrameSideDataType::AV_FRAME_DATA_MASTERING_DISPLAY_METADATA) {
        // SAFETY: this side data type holds an `AVMasteringDisplayMetadata`.
        let display = unsafe { &*data.cast::<MasteringDisplayMetadata>() };
        let peak = display.max_luminance;
        if display.has_luminance != 0 && peak.num > 0 && peak.den > 0 {
            return (f64::from(peak.num) / f64::from(peak.den)).round() as u32;
        }
    }
    DEFAULT_PEAK_LUMINANCE as u32
}

/// libavutil's `AVContentLightMetadata`, which the bindings leave out.
#[repr(C)]
struct ContentLightMetadata {
    max_cll: u32,
    max_fall: u32,
}

/// libavutil's `AVMasteringDisplayMetadata`, which the bindings leave out.
#[repr(C)]
struct MasteringDisplayMetadata {
    display_primaries: [[AVRational; 2]; 3],
    white_point: [AVRational; 2],
    min_luminance: AVRational,
    max_luminance: AVRational,
    has_primaries: i32,
    has_luminance: i32,
}
