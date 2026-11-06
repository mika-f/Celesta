use crate::error::ReactBridgeError;
use crate::types::ReactAudioClipDescriptor;
use celesta_composition::{
    AssetLocation, AudioClip, AudioDuck, ResolvedAsset, Time, TimeRange, check_duck_group,
};
use celesta_remote::is_remote_url;
use std::path::Path;

/// A seconds-valued `<Audio>` field as a time, to the microsecond, as
/// `packages/react/src/render.ts`'s `SECONDS_TIMESCALE` converts it.
pub(crate) fn react_seconds_to_time(seconds: f64) -> Time {
    Time::from_seconds(seconds, Time::MICROSECONDS)
}

/// Collapses per-frame `<Audio>` reports into one entry per distinct clip,
/// keeping first-seen order and how many reports it collapsed. Identical
/// declarations collapse too, so two identical `<Audio>` siblings play once,
/// as in the browser export; the count is reports across frames and copies,
/// not the number of copies.
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

/// Ids of the clips [`react_audio_clips`] builds that play a `<Video>`'s
/// soundtrack, in the same order.
pub fn react_video_clip_ids(clips: &[ReactAudioClipDescriptor]) -> Vec<String> {
    merge_react_audio_clips(clips)
        .into_iter()
        .enumerate()
        .filter(|(_, (clip, _))| clip.video)
        .map(|(index, _)| format!("react-audio:{index}"))
        .collect()
}

/// Turns merged `<Audio>` reports into `AudioClip`s, resolving relative `src`
/// paths against `entry_dir` and keeping `http`/`https` URLs as URLs.
/// Generated ids are `react-audio:{n}` in first-seen order. Fails when a
/// clip's ducking breaks a rule.
pub fn react_audio_clips(
    clips: &[ReactAudioClipDescriptor],
    entry_dir: &Path,
) -> Result<Vec<AudioClip>, ReactBridgeError> {
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
            let (duck_group, duck) = react_ducking(&clip)?;
            Ok(AudioClip {
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
                r#loop: clip.r#loop,
                pitch: clip.pitch,
                preserve_pitch: clip.preserve_pitch,
                fade_in: react_seconds_to_time(clip.fade_in),
                fade_out: react_seconds_to_time(clip.fade_out),
                duck_group,
                duck,
            })
        })
        .collect()
}

/// A reported clip's `duckGroup` and `duck`, with the defaults applied and
/// every rule checked, as for project items.
pub fn react_ducking(
    clip: &ReactAudioClipDescriptor,
) -> Result<(Option<String>, Option<AudioDuck>), ReactBridgeError> {
    let invalid = |error| ReactBridgeError::InvalidDuck {
        src: clip.src.clone(),
        error,
    };
    if let Some(group) = &clip.duck_group {
        check_duck_group(group).map_err(invalid)?;
    }
    let duck = clip
        .duck
        .as_ref()
        .map(|duck| {
            AudioDuck::resolve(
                &duck.under,
                duck.to,
                duck.attack.map(react_seconds_to_time),
                duck.release.map(react_seconds_to_time),
                duck.hold_gap.map(react_seconds_to_time),
            )
        })
        .transpose()
        .map_err(invalid)?;
    Ok((clip.duck_group.clone(), duck))
}
