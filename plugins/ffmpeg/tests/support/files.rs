//! Media files made with the linked FFmpeg libraries for the FFmpeg plugins'
//! tests, and what libavformat and libavcodec read back of the files the
//! plugins write.
#![allow(dead_code)]

use ez_ffmpeg::stream_info::{StreamInfo, find_audio_stream_info};
use ez_ffmpeg::{FfmpegContext, Input, Output};
use std::path::Path;

/// Encodes the `lavfi` source to `path` with `codec`.
pub fn generate(path: &Path, lavfi: &str, codec: &str) {
    FfmpegContext::builder()
        .input(Input::from(lavfi).set_format("lavfi"))
        .output(Output::from(path.to_string_lossy().into_owned()).set_video_codec(codec))
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
}

/// Renders a synthetic `lavfi` source to a lossless MKV fixture through the
/// linked FFmpeg libraries, forcing the output frame rate so the container
/// records a real frame-duration hint.
pub fn generate_source(path: &Path, lavfi: &str, fps: (i32, i32)) {
    FfmpegContext::builder()
        .input(Input::from(lavfi).set_format("lavfi"))
        .output(
            Output::from(path.to_string_lossy().into_owned())
                .set_video_codec("ffv1")
                .set_framerate(fps.0, fps.1),
        )
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
}

/// The video stream's `(color_primaries, color_trc, color_space,
/// color_range)` as libavformat reads them from the container.
pub fn stream_color(path: &Path) -> (i32, i32, i32, i32) {
    use ffmpeg_sys_next as ff;
    let url = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
    // SAFETY: the context is opened, read, and closed here; the stream
    // pointers are valid while it is open.
    unsafe {
        let mut context = std::ptr::null_mut();
        assert!(
            ff::avformat_open_input(
                &mut context,
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut()
            ) >= 0
        );
        assert!(ff::avformat_find_stream_info(context, std::ptr::null_mut()) >= 0);
        let streams =
            std::slice::from_raw_parts((*context).streams, (*context).nb_streams as usize);
        let parameters = streams
            .iter()
            .map(|&stream| (*stream).codecpar)
            .find(|&parameters| (*parameters).codec_type == ff::AVMediaType::AVMEDIA_TYPE_VIDEO)
            .expect("a video stream");
        let colors = (
            (*parameters).color_primaries as i32,
            (*parameters).color_trc as i32,
            (*parameters).color_space as i32,
            (*parameters).color_range as i32,
        );
        ff::avformat_close_input(&mut context);
        colors
    }
}

/// The four bytes after the first `colr` in `path` (an ISO BMFF colour
/// type: `nclx` in MP4, `nclc` in QuickTime).
pub fn colr_atom(path: &Path) -> Option<[u8; 4]> {
    let bytes = std::fs::read(path).unwrap();
    let at = bytes.windows(4).position(|window| window == b"colr")?;
    bytes.get(at + 4..at + 8)?.try_into().ok()
}

pub fn audio_stream(url: &str) -> (String, i32) {
    let Some(StreamInfo::Audio {
        codec_name,
        sample_rate,
        ..
    }) = find_audio_stream_info(url).unwrap()
    else {
        panic!("exported file has no audio stream");
    };
    (codec_name, sample_rate)
}

/// `path`'s first audio stream decoded to mono `f32` samples, with the sample
/// rate the stream reports (the decoder drops the encoder's priming samples).
pub fn decoded_audio(path: &Path) -> (Vec<f32>, u32) {
    let raw = path.with_extension("f32");
    FfmpegContext::builder()
        .input(Input::from(path.to_string_lossy().into_owned()))
        .output(
            Output::from(raw.to_string_lossy().into_owned())
                .add_stream_map("0:a:0")
                .set_format("f32le")
                .set_audio_codec("pcm_f32le")
                .set_audio_channels(1),
        )
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
    let (_, sample_rate) = audio_stream(&path.to_string_lossy());
    let bytes = std::fs::read(raw).unwrap();
    let samples = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect();
    (samples, sample_rate as u32)
}

/// How many seconds of audio `path` decodes to, after the decoder drops the
/// encoder's priming samples.
pub fn decoded_audio_seconds(path: &Path) -> f64 {
    let (samples, sample_rate) = decoded_audio(path);
    samples.len() as f64 / f64::from(sample_rate)
}
