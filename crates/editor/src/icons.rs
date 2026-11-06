//! The icons the editor draws beyond gpui-kit's default bundle: Lucide icons
//! that gpui-kit-assets ships but its default [`Assets`] leaves out, named by
//! [`IconName`], and [`DiamondFilled`], which Lucide has no glyph for.
//!
//! [`Assets`]: gpui_kit::assets::Assets
//! [`IconName`]: gpui_kit::assets::IconName

use std::borrow::Cow;

use gpui_kit::assets::{IconNamed, icon_assets};
use gpui_kit::{AssetSource, Result, SharedString};

icon_assets!(
    LucideIcons,
    [
        SkipBack,
        SkipForward,
        Repeat,
        Scan,
        Lock,
        MousePointerClick,
        Diamond,
        Square,
        Circle,
        Type,
        Image,
        Film,
        Group,
        Shapes,
        CornerDownRight,
    ]
);

/// Lucide's `diamond` filled with the icon color: a keyframe on the frame
/// shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiamondFilled;

const DIAMOND_FILLED_PATH: &str = "celesta/icons/diamond-filled.svg";
const DIAMOND_FILLED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path fill="currentColor" d="M2.7 10.3a2.41 2.41 0 0 0 0 3.41l7.59 7.59a2.41 2.41 0 0 0 3.41 0l7.59-7.59a2.41 2.41 0 0 0 0-3.41l-7.59-7.59a2.41 2.41 0 0 0-3.41 0Z"/></svg>"#;

impl IconNamed for DiamondFilled {
    fn path(self) -> SharedString {
        DIAMOND_FILLED_PATH.into()
    }
}

/// gpui-kit's default assets, the [`LucideIcons`] the editor names beyond
/// them, and [`DiamondFilled`].
pub struct CelestaAssets;

impl AssetSource for CelestaAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == DIAMOND_FILLED_PATH {
            return Ok(Some(Cow::Borrowed(DIAMOND_FILLED.as_bytes())));
        }
        if let Some(svg) = LucideIcons.load(path)? {
            return Ok(Some(svg));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = gpui_kit::assets::Assets.list(path)?;
        assets.extend(LucideIcons.list(path)?);
        if DIAMOND_FILLED_PATH.starts_with(path) {
            assets.push(DIAMOND_FILLED_PATH.into());
        }
        Ok(assets)
    }
}

#[cfg(test)]
mod tests {
    use super::{CelestaAssets, DiamondFilled};
    use gpui_kit::AssetSource as _;
    use gpui_kit::assets::{IconName, IconNamed};

    #[test]
    fn every_icon_resolves_to_an_svg() {
        let paths = [
            IconName::SkipBack,
            IconName::SkipForward,
            IconName::Repeat,
            IconName::Scan,
            IconName::Lock,
            IconName::MousePointerClick,
            IconName::Diamond,
            IconName::Square,
            IconName::Circle,
            IconName::Type,
            IconName::Image,
            IconName::Film,
            IconName::Group,
            IconName::Shapes,
            IconName::CornerDownRight,
        ]
        .map(IconNamed::path)
        .into_iter()
        .chain([DiamondFilled.path()]);
        for path in paths {
            let svg = CelestaAssets
                .load(&path)
                .expect("icon loads")
                .unwrap_or_else(|| panic!("{path} exists"));
            let svg = std::str::from_utf8(&svg).expect("SVG is UTF-8");
            assert!(svg.trim_start().starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
        }
    }

    #[test]
    fn default_icons_still_load() {
        assert!(CelestaAssets.load("icons/play.svg").unwrap().is_some());
    }
}
