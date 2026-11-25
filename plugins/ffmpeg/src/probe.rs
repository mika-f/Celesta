use crate::color::video_color;
use crate::ffi::av_error;
use crate::ffmpeg_error;
use crate::sequential::ffmpeg_path;
use celesta_composition::{Rational, Time};
use celesta_media::{AudioStream, MediaError, MediaProbe, VideoStream};
use ez_ffmpeg::AVRational;
use ez_ffmpeg::error::{Error, OpenInputError};
use ez_ffmpeg::stream_info::{StreamInfo, find_all_stream_infos};
use ffmpeg_sys_next::{
    AV_DISPOSITION_ATTACHED_PIC, av_dict_free, av_dict_set, avformat_close_input,
    avformat_find_stream_info, avformat_open_input,
};
use std::ffi::CString;
use std::path::Path;

/// Fails with [`MediaError::NotFound`] for a missing file, whose FFmpeg
/// error ("File or stream not found") would not name it.
pub(crate) fn ensure_exists(path: &Path) -> Result<(), MediaError> {
    match path.try_exists() {
        Ok(false) => Err(MediaError::NotFound(path.to_owned())),
        _ => Ok(()),
    }
}

pub(crate) fn probe_path(path: &Path) -> Result<MediaProbe, MediaError> {
    ensure_exists(path)?;
    let url = ffmpeg_path(path);
    let streams = find_all_stream_infos(url.as_str()).map_err(|error| match error {
        // FFmpeg found no container it knows in the file: not a file this
        // decoder reads, so another may.
        Error::OpenInputStream(OpenInputError::InvalidData) => MediaError::Unsupported {
            path: path.to_owned(),
            tried: Vec::new(),
        },
        error => ffmpeg_error(error),
    })?;
    let (duration, attached_pictures) = container_details(&url)?;

    let mut video = None;
    let mut audio = Vec::new();
    for stream in streams {
        match stream {
            StreamInfo::Video {
                index,
                codec_name,
                width,
                height,
                avg_frame_rate,
                r_frame_rate,
                duration,
                time_base,
                rotate,
                color_primaries,
                color_transfer,
                color_space,
                ..
            } if video.is_none() && !attached_pictures.contains(&index) => {
                let (width, height) = displayed_size(width, height, rotate);
                video = Some(VideoStream {
                    index: index.max(0) as u32,
                    codec: normalize_codec(codec_name),
                    width,
                    height,
                    frame_rate: video_frame_rate(avg_frame_rate, r_frame_rate),
                    duration: stream_duration(duration, time_base),
                    color: video_color(color_primaries, color_transfer, color_space),
                });
            }
            StreamInfo::Audio {
                index,
                codec_name,
                sample_rate,
                nb_channels,
                duration,
                time_base,
                ..
            } => {
                audio.push(AudioStream {
                    index: index.max(0) as u32,
                    codec: normalize_codec(codec_name),
                    sample_rate: (sample_rate > 0).then_some(sample_rate as u32),
                    channels: (nb_channels > 0).then_some(nb_channels as u16),
                    duration: stream_duration(duration, time_base),
                });
            }
            _ => {}
        }
    }
    Ok(MediaProbe {
        duration,
        video,
        audio,
    })
}

fn container_details(url: &str) -> Result<(Option<Time>, Vec<i32>), MediaError> {
    let url = CString::new(url).map_err(|error| MediaError::Codec(error.to_string()))?;
    let mut context = std::ptr::null_mut();
    // SAFETY: The options dictionary is freed regardless of the open result.
    // A successfully opened context is closed after reading its streams.
    unsafe {
        let mut options = std::ptr::null_mut();
        let result = av_dict_set(&mut options, c"scan_all_pmts".as_ptr(), c"1".as_ptr(), 0);
        if result < 0 {
            av_dict_free(&mut options);
            return Err(av_error(result, "set media probe options"));
        }
        let result = avformat_open_input(
            &mut context,
            url.as_ptr(),
            std::ptr::null_mut(),
            &mut options,
        );
        av_dict_free(&mut options);
        if result < 0 {
            return Err(MediaError::Codec(
                "Couldn’t read media stream flags".to_owned(),
            ));
        }
        let result = if avformat_find_stream_info(context, std::ptr::null_mut()) < 0 {
            Err(MediaError::Codec(
                "Couldn’t read media stream flags".to_owned(),
            ))
        } else {
            let duration = (*context).duration;
            let duration = (duration > 0).then(|| Time::new(duration, 1_000_000).reduced());
            let attached_pictures = (0..(*context).nb_streams)
                .filter_map(|index| {
                    let stream = &**(*context).streams.add(index as usize);
                    (stream.disposition & AV_DISPOSITION_ATTACHED_PIC != 0).then_some(stream.index)
                })
                .collect();
            Ok((duration, attached_pictures))
        };
        avformat_close_input(&mut context);
        result
    }
}

/// A video's size as displayed — what a browser reports as
/// `videoWidth`/`videoHeight`, and what FFmpeg's input autorotation decodes
/// it to: the coded size, turned on its side by a quarter-turn display
/// rotation (every portrait phone recording).
pub(crate) fn displayed_size(width: i32, height: i32, rotate: i32) -> (u32, u32) {
    let (width, height) = (width.max(0) as u32, height.max(0) as u32);
    if rotate.rem_euclid(180) == 90 {
        (height, width)
    } else {
        (width, height)
    }
}

pub(crate) fn normalize_codec(name: String) -> Option<String> {
    (!name.is_empty() && name != "Unknown codec").then_some(name)
}

pub(crate) fn rational_from_av(rate: AVRational) -> Option<Rational> {
    (rate.num > 0 && rate.den > 0).then(|| Rational::new(rate.num as u32, rate.den as u32))
}

/// A video stream's frame rate: `avg_frame_rate` when the demuxer computed
/// one, otherwise `r_frame_rate` — but only when the latter is a plausible
/// capture/render rate. FFmpeg reports the container time base as
/// `r_frame_rate` (Matroska's `1000/1`, MPEG-TS's `90000/1`, …) when a stream
/// carries no real frame-duration hint, and that artifact must not be
/// mistaken for a real rate (`clamp_to_source_end` reads it).
pub(crate) fn video_frame_rate(avg: AVRational, raw: AVRational) -> Option<Rational> {
    rational_from_av(avg).or_else(|| rational_from_av(raw).filter(is_plausible_frame_rate))
}

pub(crate) fn is_plausible_frame_rate(rate: &Rational) -> bool {
    f64::from(rate.numerator) / f64::from(rate.denominator) <= 480.0
}

/// A stream's own `duration` (in `time_base` units) as exact [`Time`], or
/// `None` when the demuxer reported no usable value or it overflows.
pub(crate) fn stream_duration(duration: i64, time_base: AVRational) -> Option<Time> {
    if duration <= 0 || time_base.num <= 0 || time_base.den <= 0 {
        return None;
    }
    // Cancel common factors first, so a product that only overflows before
    // reduction still fits.
    let (mut duration, mut num, mut den) =
        (duration, i64::from(time_base.num), i64::from(time_base.den));
    let common = gcd(num, den);
    (num, den) = (num / common, den / common);
    let common = gcd(duration, den);
    (duration, den) = (duration / common, den / common);
    let value = duration.checked_mul(num)?;
    let timescale = u32::try_from(den).ok()?;
    Some(Time::new(value, timescale).reduced())
}

/// The greatest common divisor of two positive numbers.
fn gcd(mut left: i64, mut right: i64) -> i64 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}
