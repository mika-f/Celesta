use crate::error::ExportError;
use crate::render::{frame_count, frame_floor};
use celesta_composition::{AudioGraph, Rational, Time, TimeError};

/// A composition-time span to export. `start` is inclusive, `end` exclusive;
/// both are clamped into `[0, composition duration]` and `start` is snapped
/// down to a frame boundary. `end: None` means "to the end of the
/// composition".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportRange {
    pub start: Time,
    pub end: Option<Time>,
}

impl ExportRange {
    /// `[start, end]` with an explicit end.
    pub fn new(start: Time, end: Time) -> Self {
        Self {
            start,
            end: Some(end),
        }
    }

    /// `[start, composition end)`.
    pub fn from(start: Time) -> Self {
        Self { start, end: None }
    }

    /// `[0, end)`.
    pub fn until(end: Time) -> Self {
        Self {
            start: Time::ZERO,
            end: Some(end),
        }
    }
}

/// The frame-snapped result of resolving an [`ExportRange`] against a concrete
/// composition duration and frame rate.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ExportWindow {
    /// Snapped composition-time start (the amount video/audio are shifted by).
    pub(crate) start: Time,
    /// First composition frame index to render.
    pub(crate) start_frame: i64,
    /// Number of frames to render.
    pub(crate) frames: u64,
    /// `frames` expressed as a `Time` at the composition frame rate — the
    /// span the audio mixdown covers.
    pub(crate) duration: Time,
}

/// Resolves `range` against the composition's full `duration`/`frame_rate`:
/// clamps both ends into `[0, duration]`, snaps `start` down to a frame
/// boundary, and clamps the frame count so it never runs past the
/// composition. An empty span is [`ExportError::EmptyRange`].
pub(crate) fn resolve_window(
    range: &ExportRange,
    duration: Time,
    frame_rate: Rational,
) -> Result<ExportWindow, ExportError> {
    let total_frames = frame_count(duration, frame_rate)?;
    let clamp = |time: Time| -> Result<Time, ExportError> {
        if time
            .cmp_exact(Time::ZERO)
            .map_err(ExportError::Time)?
            .is_lt()
        {
            return Ok(Time::ZERO);
        }
        if time.cmp_exact(duration).map_err(ExportError::Time)?.is_gt() {
            return Ok(duration);
        }
        Ok(time)
    };

    let start = clamp(range.start)?;
    let end = clamp(range.end.unwrap_or(duration))?;

    let start_frame = frame_floor(start, frame_rate)?.min(total_frames as i64);
    let snapped_start = Time::frames(start_frame, frame_rate).map_err(ExportError::Time)?;
    let span = end.checked_sub(snapped_start).map_err(ExportError::Time)?;
    if span
        .cmp_exact(Time::ZERO)
        .map_err(ExportError::Time)?
        .is_le()
    {
        return Err(ExportError::EmptyRange);
    }
    let frames = frame_count(span, frame_rate)?.min(total_frames - start_frame as u64);
    if frames == 0 {
        return Err(ExportError::EmptyRange);
    }

    Ok(ExportWindow {
        start: snapped_start,
        start_frame,
        frames,
        duration: Time::frames(frames as i64, frame_rate).map_err(ExportError::Time)?,
    })
}

/// Clones `graph`, shifting every clip's `range.start` earlier by `offset` so
/// a mix over `[0, window]` renders the composition span `[offset, offset +
/// window]`. Clips that began before the window get a negative `range.start`;
/// the mixer's `local_time = project_time - clip.range.start` then keeps
/// advancing them from the correct source position.
pub(crate) fn shifted_audio_graph(
    graph: &AudioGraph,
    offset: Time,
) -> Result<AudioGraph, TimeError> {
    let mut shifted = graph.clone();
    for clip in &mut shifted.clips {
        clip.range.start = clip.range.start.checked_sub(offset)?;
    }
    Ok(shifted)
}
