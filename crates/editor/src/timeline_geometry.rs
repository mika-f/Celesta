//! The timeline's sizes in pixels at the window's `rem`, for what has to
//! place or paint by pixels: lane offsets, label overlays, the ruler's
//! spacing and the painted ticks and playhead cap.

use celesta_editor_theme::metrics;
use gpui_kit::{Pixels, Rems, px};

/// The timeline's sizes at one `rem`. Layout reads the same `metrics`
/// tokens as `Rems`, so the element tree and the pixel math agree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TimelineGeometry {
    pub rem: Pixels,
    pub track_header_width: Pixels,
    pub ruler_height: Pixels,
    pub video_track_height: Pixels,
    pub audio_track_height: Pixels,
    pub clip_inset: Pixels,
    pub clip_label_height: Pixels,
    pub chapter_lane_height: Pixels,
    pub footer_height: Pixels,
    /// The band between the video and the audio tracks.
    pub section_divider: Pixels,
    /// The least distance between two labelled ruler marks.
    pub label_spacing: Pixels,
    /// The least distance between two unlabelled ruler ticks.
    pub minor_tick_spacing: Pixels,
    /// The length of a minor ruler tick.
    pub minor_tick: Pixels,
    /// Half the playhead cap's width.
    pub playhead_cap_half: Pixels,
    /// The playhead cap's height.
    pub playhead_cap_height: Pixels,
    /// How far above the cap's point its sides turn inward.
    pub playhead_cap_shoulder: Pixels,
}

impl TimelineGeometry {
    pub(crate) fn new(rem: Pixels) -> Self {
        let at = |rems: Rems| rem * rems.0;
        let scaled = |rems: f32| rem * rems;
        Self {
            rem,
            track_header_width: at(metrics::track_header_width()),
            ruler_height: at(metrics::ruler_height()),
            video_track_height: at(metrics::video_track_height()),
            audio_track_height: at(metrics::audio_track_height()),
            clip_inset: at(metrics::clip_inset()),
            clip_label_height: at(metrics::clip_label_height()),
            chapter_lane_height: at(metrics::chapter_lane_height()),
            footer_height: at(metrics::toolbar_row_height()),
            section_divider: scaled(0.25),
            label_spacing: scaled(12.0),
            minor_tick_spacing: scaled(0.75),
            minor_tick: scaled(0.375),
            playhead_cap_half: scaled(0.375),
            playhead_cap_height: scaled(0.75),
            playhead_cap_shoulder: scaled(0.3125),
        }
    }
}

impl Default for TimelineGeometry {
    /// At the editor's base font size, until the timeline has rendered once.
    fn default() -> Self {
        Self::new(px(metrics::BASE_FONT_SIZE))
    }
}

#[cfg(test)]
mod tests {
    use super::TimelineGeometry;
    use celesta_editor_theme::metrics;
    use gpui_kit::px;

    #[test]
    fn sizes_are_their_tokens_times_the_rem() {
        for rem in [13.0, 16.0] {
            let geometry = TimelineGeometry::new(px(rem));
            assert_eq!(geometry.rem, px(rem));
            assert_eq!(geometry.track_header_width, px(17.0 * rem));
            assert_eq!(geometry.ruler_height, px(2.25 * rem));
            assert_eq!(geometry.video_track_height, px(3.0 * rem));
            assert_eq!(geometry.audio_track_height, px(4.0 * rem));
            assert_eq!(geometry.label_spacing, px(12.0 * rem));
            assert_eq!(geometry.footer_height, px(2.0 * rem));
        }
    }

    #[test]
    fn the_default_is_the_base_font_size() {
        assert_eq!(
            TimelineGeometry::default(),
            TimelineGeometry::new(px(metrics::BASE_FONT_SIZE))
        );
        assert_eq!(TimelineGeometry::default().track_header_width, px(221.0));
    }
}
