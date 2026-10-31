//! `@font-face` families: the faces a font stylesheet declares, matched by
//! their descriptors as CSS Fonts 4 §5.2 matches them, apart from the
//! installed and loaded fonts.
//!
//! Each face is added to the database as copies whose only family is a
//! private one, one copy per weight it is drawn at, so a stylesheet family
//! hides an installed family of the same name and a face draws at the
//! weight its descriptor gives. Text in a stylesheet family is shaped with
//! the private family of the face [`TextRasterizer::family_faces`] picks.

use crate::fonts::TextRasterizer;
use celesta_composition::TextStyle;
use celesta_remote::{FontFaceDescriptors, FontFaceStyle};
use cosmic_text::{Font, fontdb};
use std::collections::HashSet;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A family no face has, which text is shaped in where no face of the
/// family asked for draws it, so cosmic-text draws it with its fallback
/// fonts: a stylesheet family with no face loaded, or code points outside
/// a face's `unicode-range`.
pub(crate) const NO_FACE_FAMILY: &str = "celesta-no-face";

/// The code points a face may draw, sorted and merged, or `None` for all.
pub(crate) type UnicodeRange = Option<Arc<[RangeInclusive<u32>]>>;

/// One `@font-face` rule's face.
#[derive(Clone, Debug)]
pub(crate) struct StylesheetFace {
    /// The face as the database lists it: the installed one, or the first
    /// face of the downloaded file, which the database no longer has.
    template: fontdb::FaceInfo,
    descriptors: FontFaceDescriptors,
    /// `descriptors.unicode_range`, shared with each [`FamilyFace`].
    unicode_range: UnicodeRange,
    /// The weights the face has a copy at, with each copy's private family.
    instances: Vec<(u16, Arc<str>)>,
    /// The stylesheets that declare the face. It draws only while one of
    /// them is in the font list.
    stylesheets: Vec<PathBuf>,
}

impl StylesheetFace {
    fn is_active(&self, active: &HashSet<PathBuf>) -> bool {
        self.stylesheets
            .iter()
            .any(|stylesheet| active.contains(stylesheet))
    }
}

#[cfg(test)]
impl StylesheetFace {
    pub(crate) fn template(&self) -> &fontdb::FaceInfo {
        &self.template
    }

    pub(crate) fn instances(&self) -> &[(u16, Arc<str>)] {
        &self.instances
    }
}

/// A face that text in some family is drawn with: its private family (or
/// the family itself, for one not from a stylesheet), the face, and the
/// code points it may draw (`None` for all).
#[derive(Clone)]
pub(crate) struct FamilyFace {
    pub name: Arc<str>,
    pub font: Arc<Font>,
    pub unicode_range: UnicodeRange,
}

impl FamilyFace {
    /// Whether this face draws `character`: its `unicode-range` holds it and
    /// its font has a glyph for it.
    pub(crate) fn draws(&self, character: char) -> bool {
        in_range(self.unicode_range.as_deref(), character)
            && self.font.as_swash().charmap().map(character) != 0
    }
}

pub(crate) fn in_range(ranges: Option<&[RangeInclusive<u32>]>, character: char) -> bool {
    ranges.is_none_or(|ranges| {
        let point = u32::from(character);
        let index = ranges.partition_point(|range| *range.end() < point);
        ranges
            .get(index)
            .is_some_and(|range| range.contains(&point))
    })
}

/// What CSS font matching picks from a stylesheet family's faces for a
/// weight: the weight they draw at and their indices, the last rule first.
struct FaceMatch {
    weight: u16,
    faces: Vec<usize>,
}

/// CSS Fonts 4 §5.2 for `font-style: normal` text at `requested`: the faces
/// of the nearest style (normal and oblique angles from 0deg up, then
/// italic, then negative oblique angles), then of those, the faces whose
/// weight range holds `requested`, or else the nearest weight in the
/// direction the specification gives. Faces left with the same weight are
/// one composite face, tried for each character in reverse rule order.
/// Only faces of the `active` stylesheets take part.
fn match_faces(
    faces: &[StylesheetFace],
    requested: u16,
    active: &HashSet<PathBuf>,
) -> Option<FaceMatch> {
    let style_rank = |face: &StylesheetFace| match face.descriptors.style {
        FontFaceStyle::Normal => (0, 0.0),
        FontFaceStyle::Oblique { min, max } if min <= 0.0 && max >= 0.0 => (0, 0.0),
        FontFaceStyle::Oblique { min, .. } if min > 0.0 => (1, min),
        FontFaceStyle::Italic => (2, 0.0),
        FontFaceStyle::Oblique { max, .. } => (3, -max),
    };
    let candidates: Vec<usize> = (0..faces.len())
        .filter(|&index| faces[index].is_active(active))
        .collect();
    let best = candidates
        .iter()
        .map(|&index| style_rank(&faces[index]))
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?;
    let styled: Vec<usize> = candidates
        .into_iter()
        .filter(|&index| style_rank(&faces[index]) == best)
        .collect();
    let contains = |index: usize, weight: u16| {
        let (min, max) = faces[index].descriptors.weight;
        (min..=max).contains(&weight)
    };
    let weight = if styled.iter().any(|&index| contains(index, requested)) {
        requested
    } else {
        // The nearest weight below and above the request.
        let below = styled
            .iter()
            .map(|&index| faces[index].descriptors.weight.1)
            .filter(|&weight| weight < requested)
            .max();
        let above = styled
            .iter()
            .map(|&index| faces[index].descriptors.weight.0)
            .filter(|&weight| weight > requested)
            .min();
        if (400..=500).contains(&requested) {
            above.filter(|&weight| weight <= 500).or(below).or(above)
        } else if requested < 400 {
            below.or(above)
        } else {
            above.or(below)
        }?
    };
    Some(FaceMatch {
        weight,
        faces: styled
            .into_iter()
            .rev()
            .filter(|&index| contains(index, weight))
            .collect(),
    })
}

impl TextRasterizer {
    /// Whether `family` is declared by a font stylesheet in the font list,
    /// so that only its own faces draw it.
    pub(crate) fn is_stylesheet_family(&self, family: &str) -> bool {
        self.active_stylesheet_families.contains(family)
    }

    /// The stylesheet family `family`'s faces, none when they all failed to
    /// load, while a stylesheet in the font list declares it.
    fn active_family_faces(&self, family: &str) -> Option<&[StylesheetFace]> {
        if !self.is_stylesheet_family(family) {
            return None;
        }
        Some(self.stylesheet_faces.get(family).map_or(&[], Vec::as_slice))
    }

    /// Makes the stylesheets among the font list's `paths` the ones whose
    /// families and faces draw, after a reload drops or adds one.
    pub(crate) fn activate_stylesheets(&mut self, paths: &[PathBuf]) {
        let active: HashSet<PathBuf> = paths
            .iter()
            .filter(|path| self.stylesheet_families.contains_key(*path))
            .cloned()
            .collect();
        if active == self.active_stylesheets {
            return;
        }
        self.active_stylesheet_families = active
            .iter()
            .flat_map(|path| self.stylesheet_families[path].iter().cloned())
            .collect();
        self.active_stylesheets = active;
        self.fonts_changed();
    }

    /// The stylesheet family a face's private family belongs to.
    pub(crate) fn stylesheet_family_of(&self, private: &str) -> Option<&str> {
        self.stylesheet_privates.get(private).map(String::as_str)
    }

    /// Whether `face` draws `family`: for a stylesheet family, it is a copy
    /// of one of the family's faces; for another, the family is one of its
    /// own.
    pub(crate) fn face_in_family(&self, face: &fontdb::FaceInfo, family: &str) -> bool {
        let stylesheet = self.is_stylesheet_family(family);
        face.families
            .iter()
            .any(|(name, _)| match self.stylesheet_family_of(name) {
                Some(owner) => stylesheet && owner == family,
                None => !stylesheet && name == family,
            })
    }

    /// Adds `template` to the stylesheet family `family` as a face with
    /// `descriptors` that `stylesheet` declares, unless it has that face
    /// already, with a copy at the weight 400 text draws it at.
    pub(crate) fn add_stylesheet_face(
        &mut self,
        stylesheet: &Path,
        family: &str,
        template: fontdb::FaceInfo,
        descriptors: FontFaceDescriptors,
    ) {
        let faces = self.stylesheet_faces.entry(family.to_owned()).or_default();
        if let Some(face) = faces
            .iter_mut()
            .find(|face| face.template.id == template.id && face.descriptors == descriptors)
        {
            if !face.stylesheets.iter().any(|path| path == stylesheet) {
                face.stylesheets.push(stylesheet.to_owned());
            }
            return;
        }
        let (min, max) = descriptors.weight;
        let unicode_range = descriptors.unicode_range.as_deref().map(Arc::from);
        faces.push(StylesheetFace {
            template,
            descriptors,
            unicode_range,
            instances: Vec::new(),
            stylesheets: vec![stylesheet.to_owned()],
        });
        let index = faces.len() - 1;
        self.add_instance(family, index, 400.clamp(min, max));
    }

    /// Adds a copy of face `index` of `family` drawn at `weight`.
    fn add_instance(&mut self, family: &str, index: usize, weight: u16) {
        let private: Arc<str> =
            format!("celesta-stylesheet-face-{}", self.stylesheet_privates.len()).into();
        let face = &mut self.stylesheet_faces.get_mut(family).expect("declared")[index];
        let mut copy = face.template.clone();
        copy.families = vec![(private.to_string(), fontdb::Language::English_UnitedStates)];
        copy.weight = fontdb::Weight(weight);
        copy.style = match face.descriptors.style {
            FontFaceStyle::Normal => fontdb::Style::Normal,
            FontFaceStyle::Italic => fontdb::Style::Italic,
            FontFaceStyle::Oblique { .. } => fontdb::Style::Oblique,
        };
        face.instances.push((weight, Arc::clone(&private)));
        self.stylesheet_privates
            .insert(Arc::clone(&private), family.to_owned());
        self.font_system.db_mut().push_face_info(copy);
        self.fonts_changed();
    }

    /// Adds the copies `style` needs to draw a stylesheet face at a weight
    /// in its `font-weight` range other than the one it was loaded at, for
    /// its families, its runs', and with `fontFallback: 'loaded'`, the font
    /// list's. Called before shaping, since a new face invalidates what
    /// shaping has looked up.
    pub(crate) fn prepare_stylesheet_weights(&mut self, style: &TextStyle) {
        let fallback = crate::font_stack::falls_back_to_loaded(style)
            .then(|| Arc::clone(&self.fallback_families));
        let mut missing: Vec<(String, usize, u16)> = Vec::new();
        for (family, weight) in crate::fonts::requested_faces(style) {
            let names = family.names().iter().map(String::as_str).chain(
                fallback
                    .iter()
                    .flat_map(|names| names.iter().map(|name| &**name)),
            );
            for name in names {
                let Some(faces) = self.active_family_faces(name) else {
                    continue;
                };
                let Some(matched) = match_faces(faces, weight, &self.active_stylesheets) else {
                    continue;
                };
                for index in matched.faces {
                    if !faces[index]
                        .instances
                        .iter()
                        .any(|(drawn, _)| *drawn == matched.weight)
                    {
                        missing.push((name.to_owned(), index, matched.weight));
                    }
                }
            }
        }
        missing.dedup();
        for (family, index, weight) in missing {
            if !self.stylesheet_faces[&family][index]
                .instances
                .iter()
                .any(|(drawn, _)| *drawn == weight)
            {
                self.add_instance(&family, index, weight);
            }
        }
    }

    /// The weight text at `requested` in the stylesheet family `family`
    /// draws at, or `None` when it has no face.
    pub(crate) fn stylesheet_weight(&self, family: &str, requested: u16) -> Option<u16> {
        match_faces(
            self.active_family_faces(family)?,
            requested,
            &self.active_stylesheets,
        )
        .map(|matched| matched.weight)
    }

    /// The faces text in `family` at `weight` is drawn with, in the order
    /// each character tries them: for a stylesheet family, the faces CSS
    /// matching picks, the last rule first; for another family, the face
    /// its nearest weight matches. Empty when it has none.
    pub(crate) fn family_faces(&mut self, family: &str, weight: u16) -> Vec<FamilyFace> {
        let Some(faces) = self.active_family_faces(family) else {
            return self
                .matched_face(family, weight)
                .map(|font| FamilyFace {
                    name: family.into(),
                    font,
                    unicode_range: None,
                })
                .into_iter()
                .collect();
        };
        self.stylesheet_copies(faces, weight)
            .into_iter()
            .filter_map(|(name, unicode_range)| {
                let font = self.matched_face(&name, weight)?;
                Some(FamilyFace {
                    name,
                    font,
                    unicode_range,
                })
            })
            .collect()
    }

    /// The private families and `unicode-range`s of the copies the
    /// stylesheet family's `faces` draw text at `weight` with, the last rule
    /// first. [`Self::prepare_stylesheet_weights`] adds the copy at the
    /// matched weight before shaping; elsewhere the nearest copy stands in.
    fn stylesheet_copies(
        &self,
        faces: &[StylesheetFace],
        weight: u16,
    ) -> Vec<(Arc<str>, UnicodeRange)> {
        let Some(matched) = match_faces(faces, weight, &self.active_stylesheets) else {
            return Vec::new();
        };
        matched
            .faces
            .iter()
            .filter_map(|&index| {
                let face = &faces[index];
                let (_, private) = face
                    .instances
                    .iter()
                    .min_by_key(|(drawn, _)| drawn.abs_diff(matched.weight))?;
                Some((Arc::clone(private), face.unicode_range.clone()))
            })
            .collect()
    }

    /// The family text in `family` at `weight` is shaped in where no other
    /// face is chosen: the private family of a stylesheet family's first
    /// face, or [`NO_FACE_FAMILY`] when it has none; another family itself.
    pub(crate) fn drawing_family(&self, family: &str, weight: u16) -> Arc<str> {
        let Some(faces) = self.active_family_faces(family) else {
            return family.into();
        };
        self.stylesheet_copies(faces, weight)
            .into_iter()
            .next()
            .map_or_else(|| NO_FACE_FAMILY.into(), |(name, _)| name)
    }

    /// Whether text in the stylesheet family `family` needs a face chosen
    /// for each character: it has several faces, or one limited to a
    /// `unicode-range`.
    pub(crate) fn splits_stylesheet_family(&self, family: &str) -> bool {
        self.active_family_faces(family).is_some_and(|faces| {
            let mut active = faces
                .iter()
                .filter(|face| face.is_active(&self.active_stylesheets));
            active.clone().count() > 1 || active.any(|face| face.unicode_range.is_some())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(weight: (u16, u16), style: FontFaceStyle) -> StylesheetFace {
        StylesheetFace {
            template: fontdb::FaceInfo {
                id: fontdb::ID::dummy(),
                source: fontdb::Source::Binary(Arc::new(Vec::<u8>::new())),
                index: 0,
                families: Vec::new(),
                post_script_name: String::new(),
                style: fontdb::Style::Normal,
                weight: fontdb::Weight(400),
                stretch: fontdb::Stretch::Normal,
                monospaced: false,
            },
            descriptors: FontFaceDescriptors {
                weight,
                style,
                unicode_range: None,
            },
            unicode_range: None,
            instances: Vec::new(),
            stylesheets: vec![PathBuf::from("a.css")],
        }
    }

    fn active() -> HashSet<PathBuf> {
        HashSet::from([PathBuf::from("a.css")])
    }

    fn matched(faces: &[StylesheetFace], requested: u16) -> (u16, Vec<usize>) {
        let matched = match_faces(faces, requested, &active()).unwrap();
        (matched.weight, matched.faces)
    }

    #[test]
    fn matches_weights_as_css_fonts_4_does() {
        use FontFaceStyle::Normal;
        // The specification's example family: 300, 400-600 (variable),
        // 700, and 900.
        let faces = [
            face((300, 300), Normal),
            face((400, 600), Normal),
            face((700, 700), Normal),
            face((900, 900), Normal),
        ];
        assert_eq!(matched(&faces, 450), (450, vec![1]));
        assert_eq!(matched(&faces, 650), (700, vec![2]));
        assert_eq!(matched(&faces, 800), (900, vec![3]));
        assert_eq!(matched(&faces, 950), (900, vec![3]));
        assert_eq!(matched(&faces, 200), (300, vec![0]));
        assert_eq!(matched(&faces, 350), (300, vec![0]));
        // Between 400 and 500: up to 500 first, then down, then up.
        let faces = [face((300, 300), Normal), face((600, 600), Normal)];
        assert_eq!(matched(&faces, 450), (300, vec![0]));
        let faces = [face((300, 300), Normal), face((500, 500), Normal)];
        assert_eq!(matched(&faces, 400), (500, vec![1]));
        let faces = [face((600, 600), Normal)];
        assert_eq!(matched(&faces, 400), (600, vec![0]));
    }

    #[test]
    fn prefers_upright_faces_and_tries_the_last_rule_first() {
        use FontFaceStyle::{Italic, Normal, Oblique};
        let faces = [
            face((700, 700), Italic),
            face((400, 400), Normal),
            face((400, 400), Normal),
        ];
        // Same descriptors: one composite face, the last rule first.
        assert_eq!(matched(&faces, 700), (400, vec![2, 1]));
        // Without an upright face, the smallest positive oblique angle,
        // then italic, then the negative angle nearest 0deg.
        let faces = [
            face(
                (400, 400),
                Oblique {
                    min: -20.0,
                    max: -10.0,
                },
            ),
            face((400, 400), Italic),
            face(
                (400, 400),
                Oblique {
                    min: 20.0,
                    max: 30.0,
                },
            ),
            face(
                (400, 400),
                Oblique {
                    min: 10.0,
                    max: 10.0,
                },
            ),
        ];
        assert_eq!(matched(&faces, 400), (400, vec![3]));
        assert_eq!(matched(&faces[..3], 400), (400, vec![2]));
        assert_eq!(matched(&faces[..2], 400), (400, vec![1]));
        assert_eq!(matched(&faces[..1], 400), (400, vec![0]));
        // An oblique range through 0deg is upright.
        let faces = [
            face((400, 400), Normal),
            face(
                (400, 400),
                Oblique {
                    min: -5.0,
                    max: 5.0,
                },
            ),
        ];
        assert_eq!(matched(&faces, 400), (400, vec![1, 0]));
        assert!(match_faces(&[], 400, &active()).is_none());
        // Faces of a stylesheet that left the font list take no part.
        let mut faces = [face((400, 400), Normal), face((700, 700), Normal)];
        faces[0].stylesheets = vec![PathBuf::from("gone.css")];
        assert_eq!(matched(&faces, 400), (700, vec![1]));
        assert!(match_faces(&faces[..1], 400, &active()).is_none());
    }

    #[test]
    fn unicode_ranges_hold_their_code_points() {
        let ranges = [0x30..=0x39, 0x3000..=0x30FF];
        for (character, inside) in [
            ('0', true),
            ('9', true),
            ('A', false),
            ('あ', true),
            ('一', false),
        ] {
            assert_eq!(in_range(Some(&ranges), character), inside, "{character}");
        }
        assert!(in_range(None, 'A'));
    }
}
