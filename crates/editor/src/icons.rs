//! Transport, viewer and layer icons that gpui-kit's default icon set does
//! not ship.
//!
//! The shapes are Lucide icons (ISC license), the family the default set is
//! drawn from, so they sit beside `IconName` glyphs without a style break.

use std::borrow::Cow;

use gpui_kit::component::IconNamed;
use gpui_kit::{AssetSource, Result, SharedString};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CelestaIcon {
    SkipBack,
    SkipForward,
    Repeat,
    Scan,
    Lock,
    MousePointerClick,
    Diamond,
    /// `Diamond` filled with the icon color.
    DiamondFilled,
    Square,
    Circle,
    Type,
    Image,
    Film,
    Group,
    Shapes,
    CornerDownRight,
}

const SVG_OPEN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"#;

/// `(asset path, SVG body)` for every icon, served by [`CelestaAssets`].
const ICONS: [(&str, &str); 16] = [
    (
        "celesta/icons/skip-back.svg",
        r#"<polygon points="19 20 9 12 19 4 19 20"/><line x1="5" x2="5" y1="19" y2="5"/>"#,
    ),
    (
        "celesta/icons/skip-forward.svg",
        r#"<polygon points="5 4 15 12 5 20 5 4"/><line x1="19" x2="19" y1="5" y2="19"/>"#,
    ),
    (
        "celesta/icons/repeat.svg",
        r#"<path d="m17 2 4 4-4 4"/><path d="M3 11v-1a4 4 0 0 1 4-4h14"/><path d="m7 22-4-4 4-4"/><path d="M21 13v1a4 4 0 0 1-4 4H3"/>"#,
    ),
    (
        "celesta/icons/scan.svg",
        r#"<path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/><path d="M7 21H5a2 2 0 0 1-2-2v-2"/>"#,
    ),
    (
        "celesta/icons/lock.svg",
        r#"<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>"#,
    ),
    (
        "celesta/icons/mouse-pointer-click.svg",
        r#"<path d="M14 4.1 12 6"/><path d="m5.1 8-2.9-.8"/><path d="m6 12-1.9 2"/><path d="M7.2 2.2 8 5.1"/><path d="M9.037 9.69a.498.498 0 0 1 .653-.653l11 4.5a.5.5 0 0 1-.074.949l-4.349 1.041a1 1 0 0 0-.74.739l-1.04 4.35a.5.5 0 0 1-.95.074z"/>"#,
    ),
    ("celesta/icons/diamond.svg", DIAMOND),
    ("celesta/icons/diamond-filled.svg", DIAMOND_FILLED),
    (
        "celesta/icons/square.svg",
        r#"<rect width="18" height="18" x="3" y="3" rx="2"/>"#,
    ),
    (
        "celesta/icons/circle.svg",
        r#"<circle cx="12" cy="12" r="10"/>"#,
    ),
    (
        "celesta/icons/type.svg",
        r#"<path d="M12 4v16"/><path d="M4 7V5a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v2"/><path d="M9 20h6"/>"#,
    ),
    (
        "celesta/icons/image.svg",
        r#"<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>"#,
    ),
    (
        "celesta/icons/film.svg",
        r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M7 3v18"/><path d="M3 7.5h4"/><path d="M3 12h18"/><path d="M3 16.5h4"/><path d="M17 3v18"/><path d="M17 7.5h4"/><path d="M17 16.5h4"/>"#,
    ),
    (
        "celesta/icons/group.svg",
        r#"<path d="M3 7V5c0-1.1.9-2 2-2h2"/><path d="M17 3h2c1.1 0 2 .9 2 2v2"/><path d="M21 17v2c0 1.1-.9 2-2 2h-2"/><path d="M7 21H5c-1.1 0-2-.9-2-2v-2"/><rect width="7" height="5" x="7" y="7" rx="1"/><rect width="7" height="5" x="10" y="12" rx="1"/>"#,
    ),
    (
        "celesta/icons/shapes.svg",
        r#"<path d="M8.3 10a.7.7 0 0 1-.626-1.079L11.4 3a.7.7 0 0 1 1.198-.043L16.3 8.9a.7.7 0 0 1-.572 1.1Z"/><rect x="3" y="14" width="7" height="7" rx="1"/><circle cx="17.5" cy="17.5" r="3.5"/>"#,
    ),
    (
        "celesta/icons/corner-down-right.svg",
        r#"<path d="m15 10 5 5-5 5"/><path d="M4 4v7a4 4 0 0 0 4 4h12"/>"#,
    ),
];

const DIAMOND: &str = r#"<path d="M2.7 10.3a2.41 2.41 0 0 0 0 3.41l7.59 7.59a2.41 2.41 0 0 0 3.41 0l7.59-7.59a2.41 2.41 0 0 0 0-3.41l-7.59-7.59a2.41 2.41 0 0 0-3.41 0Z"/>"#;
const DIAMOND_FILLED: &str = r#"<path fill="currentColor" d="M2.7 10.3a2.41 2.41 0 0 0 0 3.41l7.59 7.59a2.41 2.41 0 0 0 3.41 0l7.59-7.59a2.41 2.41 0 0 0 0-3.41l-7.59-7.59a2.41 2.41 0 0 0-3.41 0Z"/>"#;

impl IconNamed for CelestaIcon {
    fn path(self) -> SharedString {
        let index = match self {
            Self::SkipBack => 0,
            Self::SkipForward => 1,
            Self::Repeat => 2,
            Self::Scan => 3,
            Self::Lock => 4,
            Self::MousePointerClick => 5,
            Self::Diamond => 6,
            Self::DiamondFilled => 7,
            Self::Square => 8,
            Self::Circle => 9,
            Self::Type => 10,
            Self::Image => 11,
            Self::Film => 12,
            Self::Group => 13,
            Self::Shapes => 14,
            Self::CornerDownRight => 15,
        };
        ICONS[index].0.into()
    }
}

/// gpui-kit's embedded assets plus [`CelestaIcon`]'s SVGs.
pub struct CelestaAssets;

impl AssetSource for CelestaAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, body)) = ICONS.iter().find(|(icon, _)| *icon == path) {
            return Ok(Some(Cow::Owned(
                format!("{SVG_OPEN}{body}</svg>").into_bytes(),
            )));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = gpui_kit::assets::Assets.list(path)?;
        assets.extend(
            ICONS
                .iter()
                .filter(|(icon, _)| icon.starts_with(path))
                .map(|(icon, _)| SharedString::from(*icon)),
        );
        Ok(assets)
    }
}

#[cfg(test)]
mod tests {
    use super::{CelestaAssets, CelestaIcon};
    use gpui_kit::AssetSource as _;
    use gpui_kit::component::IconNamed as _;

    #[test]
    fn every_icon_resolves_to_an_svg() {
        for icon in [
            CelestaIcon::SkipBack,
            CelestaIcon::SkipForward,
            CelestaIcon::Repeat,
            CelestaIcon::Scan,
            CelestaIcon::Lock,
            CelestaIcon::MousePointerClick,
            CelestaIcon::Diamond,
            CelestaIcon::DiamondFilled,
            CelestaIcon::Square,
            CelestaIcon::Circle,
            CelestaIcon::Type,
            CelestaIcon::Image,
            CelestaIcon::Film,
            CelestaIcon::Group,
            CelestaIcon::Shapes,
            CelestaIcon::CornerDownRight,
        ] {
            let svg = CelestaAssets
                .load(&icon.path())
                .expect("icon loads")
                .expect("icon exists");
            let svg = std::str::from_utf8(&svg).expect("SVG is UTF-8");
            assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
        }
    }

    #[test]
    fn default_icons_still_load() {
        assert!(CelestaAssets.load("icons/play.svg").unwrap().is_some());
    }
}
