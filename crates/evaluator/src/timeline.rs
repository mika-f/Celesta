use celesta_composition::{Time, TimeError};
use celesta_project::{LipSyncCue, MouthShape, SourceRange, TimelineContent, TimelineItem};

pub(crate) fn mouth_shape_at(
    cues: &[LipSyncCue],
    local_time: Time,
) -> Result<Option<MouthShape>, TimeError> {
    let mut shape = None;
    for cue in cues {
        if cue.time.cmp_exact(local_time)?.is_gt() {
            break;
        }
        shape = Some(cue.shape);
    }
    Ok(shape)
}

pub(crate) fn item_has_audio(item: &TimelineItem) -> bool {
    matches!(
        &item.content,
        TimelineContent::Video { .. }
            | TimelineContent::Audio { .. }
            | TimelineContent::Dialogue { audio: Some(_), .. }
    )
}

pub(crate) fn is_active(item: &TimelineItem, time: Time) -> Result<bool, TimeError> {
    Ok(time.cmp_exact(item.range.start)?.is_ge() && time.cmp_exact(item.range.end()?)?.is_lt())
}

pub(crate) fn source_start(range: Option<SourceRange>) -> Time {
    range.map_or(Time::ZERO, |range| range.start)
}

pub(crate) fn source_duration(range: Option<SourceRange>) -> Option<Time> {
    range.and_then(|range| range.duration)
}
