//! celesta editor theme setup and the small set of audited raster/domain colors
//! the timeline canvas needs.
//!
//! Everything that is ordinary interface chrome (panels, text, borders,
//! buttons, inputs) reads its color from `cx.theme()` — see the gpui-kit
//! Design Guides. The constants here are the documented exception: they color
//! *data* drawn on the timeline (clip kinds, the audio waveform, level meters,
//! the playhead), where the hue is the information rather than decoration.

pub mod metrics;

use celesta_project::TrackKind;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{Anchor, App, Hsla, px, rgb, rgba};

/// Forces the NLE-standard dark appearance. gpui-component's dark theme still
/// defines the full light palette, so `cx.theme()` stays valid in both modes;
/// the editor simply never switches to light.
pub fn init(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    apply(cx);
}

/// Puts the editor's scale and palette on the global theme.
///
/// `Theme::change` reloads the dark theme's config, which resets everything
/// set here; call this again after any reload.
pub fn apply(cx: &mut App) {
    Theme::update(cx, |theme| {
        theme.font_size = px(metrics::BASE_FONT_SIZE);
        theme.radius = px(0.);
        theme.radius_lg = px(0.);
        // A thin inner line instead of a ring outside the control.
        theme.focus_ring = false;
        // Toasts sit just above the status bar (a toolbar row tall) instead
        // of over the panel headers at the top right.
        theme.notification.placement = Anchor::BottomRight;
        theme.notification.margins.bottom = px(metrics::BASE_FONT_SIZE * (2.0 + 0.75));
    });
}

/// celesta's brand accent (the "Celesta" wordmark, the timeline playhead). It is a
/// raster/identity color, not a semantic role — do not use it for the default
/// commit action; that is `cx.theme().primary`.
pub const ACCENT: u32 = 0xff_a1_3b;

/// Deliverable in/out range band drawn over the timeline scrubber.
pub const EXPORT_RANGE_FILL: u32 = 0xff_c4_6b_44;
pub const EXPORT_RANGE_BORDER: u32 = 0xff_c4_6b_aa;

/// Outline of the currently selected clip. White reads against every clip
/// fill color.
pub const CLIP_SELECTED_BORDER: u32 = 0xff_ff_ff;

/// Audio waveform bars, painted at low opacity inside a clip.
pub const WAVEFORM: u32 = 0xff_ff_ff;

/// Fill color for a timeline clip, by the kind of track it sits on.
pub const fn clip_fill(kind: TrackKind) -> u32 {
    match kind {
        TrackKind::Video => 0x4b_7b_ec,
        TrackKind::Audio => 0x26_a2_69,
        TrackKind::Overlay => 0x9b_59_b6,
        TrackKind::Dialogue => 0xe5_8e_26,
    }
}

/// Level-meter color for a normalized 0..1 audio level: green until it runs
/// warm, amber as it approaches unity, red at the top.
pub fn meter(level: f32) -> Hsla {
    let hex = if level >= 0.9 {
        0xe0_5d_5d
    } else if level >= 0.7 {
        0xe4_b3_4c
    } else {
        0x70_d9_9a
    };
    rgb(hex).into()
}

pub fn accent() -> Hsla {
    rgb(ACCENT).into()
}

pub fn waveform() -> Hsla {
    rgb(WAVEFORM).into()
}

pub fn clip_selected_border() -> Hsla {
    rgb(CLIP_SELECTED_BORDER).into()
}

pub fn export_range_fill() -> Hsla {
    rgba(EXPORT_RANGE_FILL).into()
}

pub fn export_range_border() -> Hsla {
    rgba(EXPORT_RANGE_BORDER).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn init_sets_the_editor_scale(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            init(cx);
            let theme = Theme::global(cx);
            assert_eq!(theme.font_size, px(metrics::BASE_FONT_SIZE));
            assert_eq!(theme.radius, px(0.));
            assert_eq!(theme.radius_lg, px(0.));
            assert!(!theme.focus_ring);
            assert_eq!(theme.notification.placement, Anchor::BottomRight);
            assert!(theme.is_dark());
        });
    }

    #[gpui_kit::test]
    fn apply_restores_the_scale_after_a_reload(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            apply(cx);
            assert_eq!(Theme::global(cx).font_size, px(metrics::BASE_FONT_SIZE));
            assert_eq!(Theme::global(cx).radius, px(0.));
        });
    }
}
