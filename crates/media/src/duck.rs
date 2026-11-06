//! Where marked clips sound, for [`celesta_composition::duck_curves`].

use crate::error::AudioMixError;
use crate::types::AudioBuffer;
use celesta_composition::{Animatable, AudioClip, AudioGraph, Time, duck_curves, integrate_f64};

/// Timeline seconds at which `clip` stops sounding: the end of its range, or
/// earlier, unless it loops, where its source position reaches the end of
/// `source` (or of its `source_duration`).
pub fn audible_end(clip: &AudioClip, source: &AudioBuffer) -> Result<f64, AudioMixError> {
    let start = clip.range.start.as_seconds()?;
    let end = clip.range.end()?.as_seconds()?;
    if clip.r#loop {
        return Ok(end);
    }
    let source_start = clip.source_start.as_seconds()?;
    let mut source_end = source.frame_count() as f64 / f64::from(source.sample_rate);
    if let Some(duration) = clip.source_duration {
        source_end = source_end.min(source_start + duration.as_seconds()?);
    }
    let remaining = source_end - source_start;
    if remaining <= 0.0 {
        return Ok(start);
    }
    let played = |local: f64| -> Result<f64, AudioMixError> {
        let time = Time::from_seconds(local, Time::MICROSECONDS);
        Ok(integrate_f64(&clip.playback_rate, time)?)
    };
    let local = match clip.playback_rate {
        Animatable::Static(rate) if rate > 0.0 => remaining / rate,
        Animatable::Static(_) => return Ok(end),
        Animatable::Keyframes(_) => {
            let span = end - start;
            if played(span)? < remaining {
                return Ok(end);
            }
            let (mut low, mut high) = (0.0, span);
            for _ in 0..64 {
                let middle = (low + high) / 2.0;
                if played(middle)? < remaining {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            high
        }
    };
    Ok((start + local).min(end))
}

/// The ducking curve of each clip of a graph, in order (`None` for a clip
/// without `duck`).
pub type DuckCurves = Vec<Option<Animatable<f64>>>;

/// The ducking curve of every clip in `graph`, asking `audible` for the
/// audible end of each marked, unmuted clip some clip ducks under (by its
/// index in `graph.clips`). Graphs without ducking cost nothing.
pub(crate) fn curves_with(
    graph: &AudioGraph,
    mut audible: impl FnMut(usize, &AudioClip) -> Result<f64, AudioMixError>,
) -> Result<DuckCurves, AudioMixError> {
    if graph.clips.iter().all(|clip| clip.duck.is_none()) {
        return Ok(vec![None; graph.clips.len()]);
    }
    let counts = |clip: &AudioClip| {
        !clip.muted
            && clip.duck_group.as_ref().is_some_and(|group| {
                graph.clips.iter().any(|other| {
                    other
                        .duck
                        .as_ref()
                        .is_some_and(|duck| duck.under.contains(group))
                })
            })
    };
    let mut ends = Vec::with_capacity(graph.clips.len());
    for (index, clip) in graph.clips.iter().enumerate() {
        ends.push(if counts(clip) {
            audible(index, clip)?
        } else {
            clip.range.end()?.as_seconds()?
        });
    }
    Ok(duck_curves(&graph.clips, &ends)?)
}
