//! Dimension tokens for the editor's chrome, on the `rem` scale.
//!
//! The editor sets gpui-kit's base font size to [`BASE_FONT_SIZE`], which
//! becomes the window's `rem`. Every token here is in `rem`s, so the chrome
//! keeps its proportions if that base changes. Spacing uses GPUI's own `rem`
//! helpers (`gap_2`, `p_3` and so on); these are the sizes they do not cover.

use gpui_kit::{Pixels, Rems, px, rems};

/// gpui-kit's `font_size`, in pixels: body text, and the window's `rem`.
pub const BASE_FONT_SIZE: f32 = 13.0;

/// Height of a panel's header band (the editor's `PanelHeader` and the right dock's tabs).
pub fn panel_header_height() -> Rems {
    rems(1.875)
}

/// Height of one row in a list or tree.
pub fn list_row_height() -> Rems {
    rems(1.75)
}

/// Height of a transport row, a toolbar, and the status bar.
pub fn toolbar_row_height() -> Rems {
    rems(2.0)
}

/// A size that was designed in pixels against gpui-kit's default 16px `rem`,
/// in `rem`s, so it keeps its proportion to the text at the editor's base.
pub fn scaled(design_px: f32) -> Rems {
    rems(design_px / 16.0)
}

/// [`scaled`] in pixels at [`BASE_FONT_SIZE`], for APIs that take only
/// `Pixels` (dialog widths, resizable panel sizes).
pub fn scaled_px(design_px: f32) -> Pixels {
    px(design_px * BASE_FONT_SIZE / 16.0)
}

/// Leading and trailing padding inside a panel.
pub fn panel_padding() -> Rems {
    rems(0.75)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{Rems, rems};

    #[test]
    fn scaled_keeps_a_16px_design_in_proportion() {
        assert_eq!(scaled(16.0), rems(1.0));
        assert_eq!(scaled(44.0), rems(2.75));
    }

    #[test]
    fn scaled_px_is_scaled_at_the_base_font_size() {
        assert_eq!(scaled_px(16.0), px(BASE_FONT_SIZE));
        assert_eq!(scaled_px(520.0), px(422.5));
    }

    #[test]
    fn tokens_keep_their_order() {
        let px = |r: Rems| r.0 * BASE_FONT_SIZE;
        assert!(px(panel_header_height()) >= px(list_row_height()));
        assert!(px(toolbar_row_height()) >= px(list_row_height()));
        assert!(px(panel_padding()) < px(list_row_height()));
    }
}
