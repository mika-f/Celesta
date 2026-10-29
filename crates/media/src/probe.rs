use crate::color::VideoColor;
use crate::error::MediaError;
use crate::sequential::ffmpeg_path;
use crate::types::{AudioStream, MediaProbe, VideoStream};
use celesta_composition::{Rational, Time};
use ez_ffmpeg::stream_info::{StreamInfo, find_all_stream_infos};
use ez_ffmpeg::{AVRational, container_info};
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
    let streams = find_all_stream_infos(url.as_str()).map_err(MediaError::Ffmpeg)?;
    let duration = container_info::get_duration_us(url.as_str())
        .ok()
        .filter(|micros| *micros > 0)
        .map(|micros| Time::new(micros, 1_000_000).reduced());

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
            } if video.is_none() => {
                let (width, height) = displayed_size(width, height, rotate);
                video = Some(VideoStream {
                    index: index.max(0) as u32,
                    codec: normalize_codec(codec_name),
                    width,
                    height,
                    frame_rate: video_frame_rate(avg_frame_rate, r_frame_rate),
                    duration: stream_duration(duration, time_base),
                    color: VideoColor::from_raw(color_primaries, color_transfer, color_space),
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
/// `None` when the demuxer reported no usable value.
pub(crate) fn stream_duration(duration: i64, time_base: AVRational) -> Option<Time> {
    if duration <= 0 || time_base.num <= 0 || time_base.den <= 0 {
        return None;
    }
    let micros =
        i128::from(duration) * i128::from(time_base.num) * 1_000_000 / i128::from(time_base.den);
    i64::try_from(micros)
        .ok()
        .map(|micros| Time::new(micros, 1_000_000).reduced())
}
