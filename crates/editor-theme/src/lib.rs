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
use gpui_kit::component::{Theme, ThemeColor, ThemeMode};
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
        apply_palette(&mut theme.colors);
    });
}

/// One neutral grey family, darkest first: the window ground (also the
/// viewer's surround and input fields), panel bodies, header and toolbar
/// bands, then popovers. `primary` is a restrained blue for selection, focus
/// and the primary action; the brand orange ([`ACCENT`]) is not a UI color.
fn apply_palette(colors: &mut ThemeColor) {
    let ground: Hsla = rgb(0x161616).into();
    let panel: Hsla = rgb(0x1c1c1c).into();
    let band: Hsla = rgb(0x232323).into();
    let band_hover: Hsla = rgb(0x2c2c2c).into();
    let band_pressed: Hsla = rgb(0x1a1a1a).into();
    let popover: Hsla = rgb(0x2a2a2a).into();
    let border: Hsla = rgb(0x2e2e2e).into();
    let text: Hsla = rgb(0xe6e6e6).into();
    let muted_text: Hsla = rgb(0x9a9a9a).into();
    let primary: Hsla = rgb(0x4a7fd6).into();

    colors.background = ground;

    colors.sidebar = panel;
    colors.tab_active = panel;
    colors.list = panel;
    colors.table = panel;
    colors.group_box = panel;

    colors.title_bar = band;
    colors.tab_bar = band;
    colors.tab = band;
    colors.status_bar = band;
    colors.secondary = band;
    colors.muted = band;
    colors.list_head = band;
    colors.table_head = band;
    colors.secondary_hover = band_hover;
    colors.list_hover = band_hover;
    colors.secondary_active = band_pressed;
    colors.button_active = band_pressed;

    colors.popover = popover;
    // Hover on ghost buttons (at half strength) and menu items, and the
    // keyboard-selected menu item: a step above the popover.
    colors.accent = rgb(0x3a3a3a).into();
    colors.accent_foreground = text;

    colors.border = border;
    colors.input = border;
    colors.title_bar_border = border;
    colors.status_bar_border = border;
    colors.sidebar_border = border;
    colors.table_row_border = border;
    colors.window_border = border;

    colors.foreground = text;
    colors.popover_foreground = text;
    colors.sidebar_foreground = text;
    colors.tab_active_foreground = text;
    colors.secondary_foreground = text;
    colors.muted_foreground = muted_text;
    colors.tab_foreground = muted_text;

    colors.primary = primary;
    colors.primary_hover = rgb(0x5a8be0).into();
    colors.primary_active = rgb(0x3f72c4).into();
    colors.primary_foreground = rgb(0xffffff).into();
    colors.ring = primary;
    colors.list_active_border = primary;
    colors.table_active_border = primary;
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

    #[gpui_kit::test]
    fn surfaces_step_from_ground_to_popover(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            init(cx);
            let theme = Theme::global(cx);
            let l = |c: Hsla| c.l;
            assert!(l(theme.background) < l(theme.sidebar));
            assert!(l(theme.sidebar) < l(theme.title_bar));
            assert!(l(theme.title_bar) < l(theme.popover));
            assert_eq!(theme.tab_active, theme.sidebar);
            assert_eq!(theme.tab_bar, theme.title_bar);
            assert_eq!(theme.status_bar, theme.title_bar);
            // Hover and the keyboard-selected menu item use `accent`: lighter
            // than the popover it highlights, and visible at half strength
            // over the band a ghost button sits on.
            assert!(l(theme.accent) > l(theme.popover));
            assert!(l(theme.accent) - l(theme.title_bar) >= 0.06);
            // One neutral family: no hue drift between steps.
            for c in [
                theme.background,
                theme.sidebar,
                theme.title_bar,
                theme.popover,
            ] {
                assert!(c.s < 0.02, "{c:?} is not neutral");
            }
        });
    }
}
