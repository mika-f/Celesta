//! celesta editor theme setup and the small set of audited raster/domain colors
//! the timeline canvas needs.
//!
//! Everything that is ordinary interface chrome (panels, text, borders,
//! buttons, inputs) reads its color from `cx.theme()` — see the gpui-kit
//! Design Guides. The constants here are the documented exception: they color
//! *data* drawn on the timeline (clip kinds, the audio waveform, level meters,
//! the playhead), where the hue is the information rather than decoration.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, rgb, rgba};
use celesta_project::TrackKind;

/// Forces the NLE-standard dark appearance. gpui-component's dark theme still
/// defines the full light palette, so `cx.theme()` stays valid in both modes;
/// the editor simply never switches to light.
pub fn init(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
}

/// celesta's brand accent (the "Celesta" wordmark, the timeline playhead). It is a
/// raster/identity color, not a semantic role — do not use it for the default
/// commit action; that is `cx.theme().primary`.
pub const ACCENT: u32 = 0xff_a1_3b;

/// Deliverable in/out range band drawn over the timeline scrubber.
pub const EXPORT_RANGE_FILL: u32 = 0xff_c4_6b_44;
pub const EXPORT_RANGE_BORDER: u32 = 0xff_c4_6b_aa;

/// Outline of the currently selected clip.
pub const CLIP_SELECTED_BORDER: u32 = 0xff_d2_9d;

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
