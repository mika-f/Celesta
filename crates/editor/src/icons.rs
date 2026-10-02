//! Transport and viewer icons that gpui-kit's default icon set does not ship.
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
}

const SVG_OPEN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"#;

/// `(asset path, SVG body)` for every icon, served by [`CelestaAssets`].
const ICONS: [(&str, &str); 5] = [
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
];

impl IconNamed for CelestaIcon {
    fn path(self) -> SharedString {
        let index = match self {
            Self::SkipBack => 0,
            Self::SkipForward => 1,
            Self::Repeat => 2,
            Self::Scan => 3,
            Self::Lock => 4,
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
