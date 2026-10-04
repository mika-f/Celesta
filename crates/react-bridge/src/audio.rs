use crate::types::ReactAudioClipDescriptor;
use celesta_composition::{AssetLocation, AudioClip, ResolvedAsset, Time, TimeRange};
use celesta_remote::is_remote_url;
use std::path::Path;

/// Timescale seconds-valued `<Audio>` fields are converted at, matching
/// `packages/react/src/render.ts`'s `SECONDS_TIMESCALE`.
pub(crate) const REACT_AUDIO_SECONDS_TIMESCALE: u32 = 1_000_000;

pub(crate) fn react_seconds_to_time(seconds: f64) -> Time {
    Time::new(
        (seconds * f64::from(REACT_AUDIO_SECONDS_TIMESCALE)).round() as i64,
        REACT_AUDIO_SECONDS_TIMESCALE,
    )
}

/// Collapses per-frame `<Audio>` reports into one entry per distinct clip,
/// keeping first-seen order and how many frames reported it (its
/// multiplicity — two identical clips playing at once must stay two clips).
pub fn merge_react_audio_clips(
    clips: &[ReactAudioClipDescriptor],
) -> Vec<(ReactAudioClipDescriptor, usize)> {
    let mut merged: Vec<(ReactAudioClipDescriptor, usize)> = Vec::new();
    for clip in clips {
        match merged.iter_mut().find(|(known, _)| known == clip) {
            Some((_, count)) => *count += 1,
            None => merged.push((clip.clone(), 1)),
        }
    }
    merged
}

/// Turns merged `<Audio>` reports into `AudioClip`s, resolving relative `src`
/// paths against `entry_dir` and keeping `http`/`https` URLs as URLs.
/// Generated ids are `react-audio:{n}` in first-seen order.
pub fn react_audio_clips(clips: &[ReactAudioClipDescriptor], entry_dir: &Path) -> Vec<AudioClip> {
    merge_react_audio_clips(clips)
        .into_iter()
        .enumerate()
        .map(|(index, (clip, _))| {
            let location = if is_remote_url(&clip.src) {
                AssetLocation::Url {
                    url: clip.src.clone(),
                }
            } else if Path::new(&clip.src).is_relative() {
                AssetLocation::File {
                    path: entry_dir.join(&clip.src).to_string_lossy().into_owned(),
                }
            } else {
                AssetLocation::File {
                    path: clip.src.clone(),
                }
            };
            AudioClip {
                id: format!("react-audio:{index}"),
                asset: ResolvedAsset {
                    id: clip.src.clone(),
                    location,
                },
                range: TimeRange {
                    start: react_seconds_to_time(clip.start),
                    duration: react_seconds_to_time(clip.duration),
                },
                source_start: react_seconds_to_time(clip.source_start),
                source_duration: None,
                playback_rate: clip.playback_rate,
                volume: clip.volume,
                muted: clip.muted,
            }
        })
        .collect()
}
