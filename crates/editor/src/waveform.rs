use crate::media_probe::MediaAssetInfo;
use celesta_composition::{Animatable, AudioClip, Time, evaluate_f64, integrate_f64};
use celesta_media::{AudioBuffer, DynamicRange, MediaProbe};

pub(crate) fn waveform_peaks(buffer: &AudioBuffer, requested_buckets: usize) -> Vec<f32> {
    let frame_count = buffer.frame_count();
    if frame_count == 0 || requested_buckets == 0 || buffer.channels == 0 {
        return Vec::new();
    }
    let bucket_count = requested_buckets.min(frame_count);
    let channels = usize::from(buffer.channels);
    let mut peaks = vec![0.0_f32; bucket_count];
    for (frame_index, frame) in buffer.samples.chunks_exact(channels).enumerate() {
        let bucket = frame_index * bucket_count / frame_count;
        let amplitude = frame
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        peaks[bucket] = peaks[bucket].max(amplitude);
    }
    peaks
}

pub(crate) fn media_asset_info(probe: &MediaProbe) -> MediaAssetInfo {
    let duration = probe
        .duration
        .or_else(|| probe.video.as_ref().and_then(|video| video.duration))
        .or_else(|| probe.audio.iter().find_map(|audio| audio.duration));
    MediaAssetInfo {
        duration,
        video_size: probe
            .video
            .as_ref()
            .map(|video| (video.width, video.height)),
        dynamic_range: probe
            .video
            .as_ref()
            .map_or(DynamicRange::Sdr, |video| video.color.dynamic_range()),
        has_audio: !probe.audio.is_empty(),
    }
}

pub(crate) fn format_media_asset_info(info: &MediaAssetInfo) -> String {
    let mut parts = Vec::new();
    if let Some((width, height)) = info.video_size {
        parts.push(format!("{width}×{height}"));
    }
    match info.dynamic_range {
        DynamicRange::Sdr => {}
        DynamicRange::Pq => parts.push("HDR (PQ)".to_owned()),
        DynamicRange::Hlg => parts.push("HDR (HLG)".to_owned()),
    }
    if let Some(duration) = info
        .duration
        .and_then(|duration| duration.as_seconds().ok())
    {
        parts.push(format!("{duration:.3}s"));
    }
    if info.has_audio && info.video_size.is_some() {
        parts.push("audio".to_owned());
    }
    if parts.is_empty() {
        "No media streams".to_owned()
    } else {
        parts.join(" · ")
    }
}

pub(crate) fn map_clip_waveform(
    peaks: &[f32],
    source_frames: usize,
    sample_rate: u32,
    clip: &AudioClip,
    requested_buckets: usize,
) -> Vec<f32> {
    if peaks.is_empty() || source_frames == 0 || sample_rate == 0 || requested_buckets == 0 {
        return Vec::new();
    }
    let Ok(source_start) = clip.source_start.as_seconds() else {
        return Vec::new();
    };
    let source_limit = match clip.source_duration {
        Some(duration) => {
            let Ok(duration) = duration.as_seconds() else {
                return Vec::new();
            };
            Some(source_start + duration)
        }
        None => None,
    };
    let source_duration = source_frames as f64 / f64::from(sample_rate);
    let bucket_count = requested_buckets.min(peaks.len().max(1));
    (0..bucket_count)
        .map(|bucket| {
            let Some(local_start) = time_fraction(clip.range.duration, bucket, bucket_count) else {
                return 0.0;
            };
            let Some(local_end) = time_fraction(clip.range.duration, bucket + 1, bucket_count)
            else {
                return 0.0;
            };
            let Ok(mapped_start) = integrate_f64(&clip.playback_rate, local_start) else {
                return 0.0;
            };
            let Ok(mapped_end) = integrate_f64(&clip.playback_rate, local_end) else {
                return 0.0;
            };
            let start = source_start + mapped_start;
            let mut end = (source_start + mapped_end).max(start);
            if let Some(limit) = source_limit {
                if start >= limit {
                    return 0.0;
                }
                end = end.min(limit);
            }
            if !clip.r#loop {
                return source_peak(peaks, source_duration, start.max(0.0), end);
            }
            // A looping clip starts the source over past its end, so a
            // bucket across the end shows both sides of it.
            let length = (end - start).min(source_duration);
            let start = start.rem_euclid(source_duration);
            source_peak(peaks, source_duration, start, start + length).max(source_peak(
                peaks,
                source_duration,
                0.0,
                start + length - source_duration,
            ))
        })
        .collect()
}

/// The loudest peak between `start` and `end` seconds into a source.
fn source_peak(peaks: &[f32], source_duration: f64, start: f64, end: f64) -> f32 {
    if start >= source_duration || end <= start {
        return 0.0;
    }
    let start_fraction = (start / source_duration).clamp(0.0, 1.0) as f32;
    let duration_fraction =
        ((end.min(source_duration) - start) / source_duration).clamp(0.0, 1.0) as f32;
    waveform_segment(peaks, start_fraction, duration_fraction, 1)
        .into_iter()
        .next()
        .unwrap_or(0.0)
}

/// `waveform` scaled by the clip's gain over time: its volume, its fades,
/// and `duck`, its ducking curve on the timeline, as the mixer applies them.
pub(crate) fn clip_level_envelope(
    waveform: &[f32],
    clip: &AudioClip,
    duck: Option<&Animatable<f64>>,
) -> Vec<f32> {
    let bucket_count = waveform.len();
    let Ok(fade) = clip.fade() else {
        return vec![0.0; bucket_count];
    };
    waveform
        .iter()
        .enumerate()
        .map(|(bucket, peak)| {
            let Some(local_time) = time_fraction(clip.range.duration, bucket, bucket_count) else {
                return 0.0;
            };
            let ducked = match duck {
                Some(curve) => clip
                    .range
                    .start
                    .checked_add(local_time)
                    .ok()
                    .and_then(|time| evaluate_f64(curve, time).ok()),
                None => Some(1.0),
            };
            let (Ok(volume), Ok(seconds), Some(ducked)) = (
                evaluate_f64(&clip.volume, local_time),
                local_time.as_seconds(),
                ducked,
            ) else {
                return 0.0;
            };
            let gain = volume.clamp(0.0, 16.0) * fade.gain(seconds) * ducked;
            (*peak * gain as f32).clamp(0.0, 1.0)
        })
        .collect()
}

/// The volume after a drag of `delta_pixels` across a slider
/// `slider_width` pixels wide, which spans 0–200 %.
pub(crate) fn master_volume_from_drag(
    start_volume: f64,
    delta_pixels: f64,
    slider_width: f64,
) -> f64 {
    ((start_volume + delta_pixels / slider_width * 2.0).clamp(0.0, 2.0) * 100.0).round() / 100.0
}

pub(crate) fn level_at_time(
    levels: &[f32],
    clip: &celesta_editor_core::ClipSummary,
    time: Time,
) -> f32 {
    if levels.is_empty() {
        return 0.0;
    }
    let (Ok(now), Ok(start), Ok(duration)) = (
        time.as_seconds(),
        clip.start.as_seconds(),
        clip.duration.as_seconds(),
    ) else {
        return 0.0;
    };
    if duration <= 0.0 || now < start || now >= start + duration {
        return 0.0;
    }
    let index = (((now - start) / duration) * levels.len() as f64).floor() as usize;
    levels[index.min(levels.len() - 1)]
}

pub(crate) fn clip_local_time(time: Time, clip: &celesta_editor_core::ClipSummary) -> Time {
    if time
        .cmp_exact(clip.start)
        .is_ok_and(|ordering| !ordering.is_gt())
    {
        return Time::ZERO;
    }
    let local = time.checked_sub(clip.start).unwrap_or(Time::ZERO);
    if local
        .cmp_exact(clip.duration)
        .is_ok_and(|ordering| ordering.is_gt())
    {
        clip.duration
    } else {
        local
    }
}

pub(crate) fn time_fraction(duration: Time, numerator: usize, denominator: usize) -> Option<Time> {
    let value = i128::from(duration.value).checked_mul(i128::try_from(numerator).ok()?)?;
    let timescale = u64::from(duration.timescale).checked_mul(u64::try_from(denominator).ok()?)?;
    Some(Time::new(
        i64::try_from(value).ok()?,
        u32::try_from(timescale).ok()?,
    ))
}

pub(crate) fn waveform_segment(
    peaks: &[f32],
    start_fraction: f32,
    duration_fraction: f32,
    max_bars: usize,
) -> Vec<f32> {
    if peaks.is_empty() || duration_fraction <= 0.0 || max_bars == 0 {
        return Vec::new();
    }
    let start = (start_fraction.clamp(0.0, 1.0) * peaks.len() as f32).floor() as usize;
    let end =
        ((start_fraction + duration_fraction).clamp(0.0, 1.0) * peaks.len() as f32).ceil() as usize;
    let values = &peaks[start.min(peaks.len())..end.max(start + 1).min(peaks.len())];
    let bar_count = values.len().min(max_bars);
    (0..bar_count)
        .map(|bar| {
            let from = bar * values.len() / bar_count;
            let to = ((bar + 1) * values.len()).div_ceil(bar_count);
            values[from..to].iter().copied().fold(0.0_f32, f32::max)
        })
        .collect()
}
