//! Font stacks: `fontFamily` lists tried in order for each grapheme cluster.

use crate::fonts::{COLOR_EMOJI_FAMILIES, TextRasterizer};
use crate::linebreak::{emoji_presentation_spans, is_visible_character};
use crate::shaping::usable_font_runs;
use crate::stylesheet_fonts::{FamilyFace, NO_FACE_FAMILY};
use celesta_composition::{FontFallbackMode, FontFamily, FontFeatures, TextFontRun, TextStyle};
use cosmic_text::{Family, Font, Weight, fontdb};
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

/// A half-open code-point range of the complete text shaped in a family,
/// weight, or features other than the text's own: a `<Span>` font run, or
/// clusters a font stack draws with a family other than its primary one.
/// Omitted fields use the text's own; features are the run's, to merge over
/// the text's.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FaceRun {
    pub start: usize,
    pub end: usize,
    pub font_weight: Option<u16>,
    pub font_family: Option<Arc<str>>,
    pub font_features: Option<FontFeatures>,
}

/// The faces of one stack's families at one weight, in the order each
/// character tries them: family by family, a stylesheet family's faces the
/// last rule first.
struct StackFaces<'a> {
    family: &'a FontFamily,
    weight: u16,
    /// Each face with the family of the stack it draws.
    faces: Vec<(Arc<str>, FamilyFace)>,
    /// The family the stack is shaped in where no other face is chosen.
    primary: Arc<str>,
    /// With `fontFallback: 'loaded'`, the font list's other families' faces
    /// at this weight, in list order, tried before the system's fonts.
    fallback: Vec<FamilyFace>,
}

impl TextRasterizer {
    /// The family `family` is shaped in where no other face of it is
    /// chosen: the first of its families with a face (for a stylesheet
    /// family, the private family of that face), or when none has one, its
    /// first family, drawn with a fallback font as a missing family always
    /// was, unless a stylesheet declares it: then [`NO_FACE_FAMILY`], so no
    /// installed font of that name draws it.
    pub(crate) fn primary_family(&mut self, family: &FontFamily, weight: u16) -> Arc<str> {
        let first = family.first();
        let names = if family.is_stack() {
            family.names()
        } else {
            std::slice::from_ref(&family.names()[0])
        };
        for name in names {
            if self.is_stylesheet_family(name) {
                let drawing = self.drawing_family(name, weight);
                if &*drawing != NO_FACE_FAMILY {
                    return drawing;
                }
            } else if !family.is_stack() || self.matched_weight(name, weight).is_some() {
                return name.as_str().into();
            }
        }
        if self.is_stylesheet_family(first) {
            NO_FACE_FAMILY.into()
        } else {
            first.into()
        }
    }

    /// The face of `family` that a request for `weight` draws with, as
    /// cosmic-text picks it for the weight [`Self::matched_weight`] returns:
    /// for a stylesheet family, the face its first character tries.
    pub(crate) fn matched_face(&mut self, family: &str, weight: u16) -> Option<Arc<Font>> {
        if self.is_stylesheet_family(family) {
            return self
                .family_faces(family, weight)
                .into_iter()
                .next()
                .map(|face| face.font);
        }
        let matched = self.matched_weight(family, weight)?;
        let database = self.font_system.db();
        // A query walks every installed face, so it is looked up once.
        let id = (*self
            .matched_faces
            .entry((family.to_owned(), matched))
            .or_insert_with(|| {
                database.query(&fontdb::Query {
                    families: &[Family::Name(family)],
                    weight: Weight(matched),
                    ..fontdb::Query::default()
                })
            }))?;
        self.font_system.get_font(id, Weight(matched))
    }

    /// `style`'s font runs, and the ranges its font stacks (its own and its
    /// runs') draw with a family other than the text's primary one, in
    /// order, including ranges drawn with a loaded font when `style` falls
    /// back to them. Without a stack, split stylesheet, or loaded fallback
    /// these are `style`'s usable font runs as they are.
    pub(crate) fn face_runs(&mut self, text: &str, style: &TextStyle) -> Arc<[FaceRun]> {
        let spans = if style.font_runs.is_empty() {
            Vec::new()
        } else {
            usable_font_runs(style, text.chars().count())
        };
        let needs_selection = |family: &FontFamily| {
            family.is_stack()
                || family
                    .names()
                    .iter()
                    .any(|name| self.splits_stylesheet_family(name))
        };
        let falls_back = falls_back_to_loaded(style) && !self.fallback_families.is_empty();
        let stacked = style.font_family.as_ref().is_some_and(needs_selection)
            || spans
                .iter()
                .any(|run| run.font_family.as_ref().is_some_and(needs_selection))
            || (falls_back
                && (style.font_family.is_some()
                    || spans.iter().any(|run| run.font_family.is_some())));
        if !stacked {
            let mut runs = Vec::with_capacity(spans.len());
            for run in &spans {
                let weight = run.font_weight.or(style.font_weight).unwrap_or(400);
                runs.push(FaceRun {
                    start: run.start,
                    end: run.end,
                    font_weight: run.font_weight,
                    font_family: run
                        .font_family
                        .as_ref()
                        .map(|family| self.primary_family(family, weight)),
                    font_features: run.font_features.clone(),
                });
            }
            return runs.into();
        }
        // Measuring and drawing ask for the same text's runs again and
        // again (each frame of a typing animation, say), and choosing a
        // family looks at every cluster.
        let key = (text.len() <= 100_000).then(|| {
            format!(
                "{text:?}\0{:?}",
                (
                    &style.font_family,
                    style.font_weight,
                    &style.font_runs,
                    style.font_fallback
                )
            )
        });
        if let Some(runs) = key.as_ref().and_then(|key| self.stacked_runs.get(key)) {
            return Arc::clone(runs);
        }
        let runs: Arc<[FaceRun]> = self.resolve_stacks(text, style, &spans, falls_back).into();
        if let Some(key) = key {
            if self.stacked_runs.len() >= 64 {
                self.stacked_runs.clear();
            }
            self.stacked_runs.insert(key, Arc::clone(&runs));
        }
        runs
    }

    /// [`Self::face_runs`] for text with a font stack or loaded fallback:
    /// each grapheme cluster's family, from its span's stack or the text's,
    /// or from the font list when `falls_back`.
    fn resolve_stacks(
        &mut self,
        text: &str,
        style: &TextStyle,
        spans: &[&TextFontRun],
        falls_back: bool,
    ) -> Vec<FaceRun> {
        let requested = style.font_weight.unwrap_or(400);
        let primary = style
            .font_family
            .as_ref()
            .map(|family| self.primary_family(family, requested));
        let emoji = emoji_presentation_spans(text);
        let mut next_emoji = 0;
        let mut stacks: Vec<StackFaces> = Vec::new();
        let mut runs: Vec<FaceRun> = Vec::new();
        let mut point = 0;
        let mut next_span = 0;
        for (byte, cluster) in text.grapheme_indices(true) {
            let start = point;
            point += cluster.chars().count();
            // A cluster takes the run at its first code point.
            while next_span < spans.len() && spans[next_span].end <= start {
                next_span += 1;
            }
            let span = spans.get(next_span).filter(|run| run.start <= start);
            let font_weight = span.and_then(|run| run.font_weight);
            let font_features = span.and_then(|run| run.font_features.as_ref());
            let family = span
                .and_then(|run| run.font_family.as_ref())
                .or(style.font_family.as_ref());
            let font_family = match family {
                Some(family) => {
                    let weight = font_weight.or(style.font_weight).unwrap_or(400);
                    let index = match stacks
                        .iter()
                        .position(|stack| stack.family == family && stack.weight == weight)
                    {
                        Some(index) => index,
                        None => {
                            let faces = self.stack_faces(family, weight);
                            let fallback = if falls_back {
                                self.fallback_faces(family, weight)
                            } else {
                                Vec::new()
                            };
                            let primary = self.primary_family(family, weight);
                            stacks.push(StackFaces {
                                family,
                                weight,
                                faces,
                                primary,
                                fallback,
                            });
                            stacks.len() - 1
                        }
                    };
                    while next_emoji < emoji.len() && emoji[next_emoji].end <= byte {
                        next_emoji += 1;
                    }
                    let is_emoji = emoji
                        .get(next_emoji)
                        .is_some_and(|range| range.start <= byte);
                    Some(cluster_family(&stacks[index], cluster, is_emoji))
                }
                None => None,
            };
            // Outside spans, a cluster in the primary family needs no run.
            if span.is_none() && font_family == primary.as_deref() {
                continue;
            }
            match runs.last_mut() {
                Some(last)
                    if last.end == start
                        && last.font_family.as_deref() == font_family
                        && last.font_weight == font_weight
                        && last.font_features.as_ref() == font_features =>
                {
                    last.end = point;
                }
                _ => runs.push(FaceRun {
                    start,
                    end: point,
                    font_weight,
                    font_family: font_family.map(Into::into),
                    font_features: font_features.cloned(),
                }),
            }
        }
        runs
    }

    fn stack_faces(&mut self, family: &FontFamily, weight: u16) -> Vec<(Arc<str>, FamilyFace)> {
        let mut faces = Vec::new();
        for name in family.names() {
            let name: Arc<str> = name.as_str().into();
            for face in self.family_faces(&name, weight) {
                faces.push((Arc::clone(&name), face));
            }
        }
        faces
    }

    /// The faces of the font list's families at `weight` other than
    /// `family`'s own, in list order.
    fn fallback_faces(&mut self, family: &FontFamily, weight: u16) -> Vec<FamilyFace> {
        let families = Arc::clone(&self.fallback_families);
        let mut fallback = Vec::new();
        for name in families.iter() {
            if !family.names().iter().any(|own| **own == **name) {
                fallback.extend(self.family_faces(name, weight));
            }
        }
        fallback
    }
}

/// Whether `style` falls back to the font list's loaded fonts before the
/// system's.
pub(crate) fn falls_back_to_loaded(style: &TextStyle) -> bool {
    style.font_fallback == Some(FontFallbackMode::Loaded)
}

/// The family of `stack` a grapheme `cluster` is drawn with: the first face
/// that draws every visible character of it (holds it in its
/// `unicode-range` and has a glyph for it), so combining marks and
/// variation selectors stay with their base; failing that, the first loaded
/// fallback face that draws every one; failing that, the first face that
/// draws its first character; failing that, the primary family, where the
/// system's fallback fonts draw it, or, when the primary face's
/// `unicode-range` leaves the cluster out, a family with no face, so that
/// face does not draw it. An emoji takes a color emoji font of the stack
/// first, if one has it.
fn cluster_family<'a>(stack: &'a StackFaces<'_>, cluster: &str, is_emoji: bool) -> &'a str {
    let primary = &*stack.primary;
    let unlimited = |face: &FamilyFace| face.unicode_range.is_none();
    if stack.faces.len() <= 1
        && stack.fallback.is_empty()
        && stack.faces.iter().all(|(_, face)| unlimited(face))
    {
        return primary;
    }
    let covers = |face: &FamilyFace| {
        cluster
            .chars()
            .filter(|&character| is_visible_character(character))
            .all(|character| face.draws(character))
    };
    if is_emoji
        && let Some((_, face)) = stack
            .faces
            .iter()
            .find(|(family, face)| COLOR_EMOJI_FAMILIES.contains(&family.as_ref()) && covers(face))
    {
        return &face.name;
    }
    if let Some(face) = stack
        .faces
        .iter()
        .map(|(_, face)| face)
        .chain(&stack.fallback)
        .find(|face| covers(face))
    {
        return &face.name;
    }
    if let Some((_, face)) = cluster
        .chars()
        .next()
        .and_then(|first| stack.faces.iter().find(|(_, face)| face.draws(first)))
    {
        return &face.name;
    }
    match stack.faces.first() {
        Some((_, face)) if face.name == stack.primary && !unlimited(face) => NO_FACE_FAMILY,
        _ => primary,
    }
}
