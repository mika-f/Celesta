//! Where each JSX element renders across the composition (see
//! [`ReactBridge::collect_occurrences`](crate::ReactBridge::collect_occurrences)),
//! and the chapters those frames fall in.

use crate::types::{ElementKind, LayerSourceReport, ReactChapter, SourcePos};
use serde::Deserialize;

/// The frames each element renders in, by instance.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct OccurrenceIndex {
    pub elements: Vec<ElementOccurrences>,
}

/// Every instance of one element: the place it was written.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ElementOccurrences {
    /// The first entry of the source chain that is not a wrapper.
    pub element: SourcePos,
    pub instances: Vec<InstanceOccurrences>,
}

/// One way an element is reached: the same element under another component
/// call site is another instance.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceOccurrences {
    /// The source chain without its wrapper entries, innermost first.
    pub chain: Vec<SourcePos>,
    /// `[from, to)` composition frames, joined where adjacent.
    pub ranges: Vec<(u64, u64)>,
    /// Most copies of this instance in any one frame, as a `.map()` makes.
    pub max_per_frame: u32,
    /// Where the wrappers it renders through (`<Sequence>`, a
    /// `<Series.Sequence>`, ...) were written, innermost first, over every
    /// frame. Empty from runtimes that do not report them.
    #[serde(default)]
    pub wrappers: Vec<SourcePos>,
}

impl OccurrenceIndex {
    /// Takes in the index of a later chunk of frames: elements and instances
    /// it adds are appended, a range starting where one of ours ends extends
    /// it, each instance keeps the larger `max_per_frame`, and wrappers it
    /// has not listed yet are appended.
    pub fn merge(&mut self, chunk: OccurrenceIndex) {
        for element in chunk.elements {
            let Some(ours) = self
                .elements
                .iter_mut()
                .find(|ours| ours.element == element.element)
            else {
                self.elements.push(element);
                continue;
            };
            for instance in element.instances {
                let Some(mine) = ours
                    .instances
                    .iter_mut()
                    .find(|mine| mine.chain == instance.chain)
                else {
                    ours.instances.push(instance);
                    continue;
                };
                mine.max_per_frame = mine.max_per_frame.max(instance.max_per_frame);
                for wrapper in instance.wrappers {
                    if !mine.wrappers.contains(&wrapper) {
                        mine.wrappers.push(wrapper);
                    }
                }
                for (from, to) in instance.ranges {
                    match mine.ranges.iter_mut().find(|range| range.1 == from) {
                        Some(range) => range.1 = to,
                        None => mine.ranges.push((from, to)),
                    }
                }
            }
        }
    }

    /// The element and instance the layer `report` describes: its chain
    /// without wrapper entries, whose first entry is the element. `None` when
    /// the chain has no other entries or the index never saw that instance.
    pub fn instance_for(
        &self,
        report: &LayerSourceReport,
    ) -> Option<(&ElementOccurrences, &InstanceOccurrences)> {
        let chain: Vec<SourcePos> = report
            .chain
            .iter()
            .filter(|entry| entry.kind != ElementKind::Wrapper)
            .map(SourcePos::from)
            .collect();
        let element = self
            .elements
            .iter()
            .find(|element| Some(&element.element) == chain.first())?;
        let instance = element
            .instances
            .iter()
            .find(|instance| instance.chain == chain)?;
        Some((element, instance))
    }
}

/// The scenes `ranges` show: the outermost chapters (see
/// [`outermost_chapter_at`]) overlapping any range, in chapter order, each
/// once. A chapter of length 0 never overlaps.
pub fn scenes_of<'a>(ranges: &[(u64, u64)], chapters: &'a [ReactChapter]) -> Vec<&'a ReactChapter> {
    chapters
        .iter()
        .enumerate()
        .filter(|&(index, chapter)| {
            chapter.duration_in_frames > 0
                && is_outermost(chapters, index)
                && ranges
                    .iter()
                    .any(|&(from, to)| from < chapter.end() && chapter.from < to)
        })
        .map(|(_, chapter)| chapter)
        .collect()
}

/// The scene showing at `frame`: of the chapters no other chapter's span
/// holds, the latest-starting one whose span holds it. A part inside a scene
/// thus reads as that scene, where [`crate::chapter_at`] picks the part.
/// `None` in a gap, past the last chapter, and for chapters of length 0.
pub fn outermost_chapter_at(chapters: &[ReactChapter], frame: u64) -> Option<&ReactChapter> {
    chapters
        .iter()
        .enumerate()
        .filter(|&(index, chapter)| {
            is_outermost(chapters, index) && chapter.from <= frame && frame < chapter.end()
        })
        .map(|(_, chapter)| chapter)
        .max_by_key(|chapter| chapter.from)
}

/// Whether no other chapter's `[from, end)` holds chapter `index`'s. Of
/// chapters with the same span, the first counts as the outer one.
fn is_outermost(chapters: &[ReactChapter], index: usize) -> bool {
    let chapter = &chapters[index];
    !chapters.iter().enumerate().any(|(other_index, other)| {
        other_index != index
            && other.from <= chapter.from
            && chapter.end() <= other.end()
            && (other_index < index || (other.from, other.end()) != (chapter.from, chapter.end()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{LayerSource, ReceivedProps};

    fn pos(line: u32) -> SourcePos {
        SourcePos {
            file: "/a/title.tsx".into(),
            line,
            column: 4,
        }
    }

    fn index(
        chain: Vec<SourcePos>,
        ranges: Vec<(u64, u64)>,
        max_per_frame: u32,
    ) -> OccurrenceIndex {
        OccurrenceIndex {
            elements: vec![ElementOccurrences {
                element: chain[0].clone(),
                instances: vec![InstanceOccurrences {
                    chain,
                    ranges,
                    max_per_frame,
                    wrappers: vec![],
                }],
            }],
        }
    }

    fn chapter(id: &str, from: u64, duration_in_frames: u64) -> ReactChapter {
        ReactChapter {
            id: id.to_owned(),
            name: id.to_owned(),
            from,
            duration_in_frames,
        }
    }

    fn ids(chapters: Vec<&ReactChapter>) -> Vec<&str> {
        chapters.iter().map(|chapter| chapter.id.as_str()).collect()
    }

    #[test]
    fn merge_joins_ranges_across_chunks() {
        let mut merged = index(vec![pos(3), pos(8)], vec![(0, 240)], 1);
        merged.merge(index(vec![pos(3), pos(8)], vec![(240, 300)], 3));
        assert_eq!(merged, index(vec![pos(3), pos(8)], vec![(0, 300)], 3));
    }

    #[test]
    fn instance_for_skips_wrapper_entries() {
        let entry = |name: &str, line, kind| LayerSource {
            name: name.to_owned(),
            file: "/a/title.tsx".into(),
            line,
            column: 4,
            kind,
            layer_id: None,
        };
        let report = LayerSourceReport {
            chain: vec![
                entry("Rect", 3, ElementKind::Primitive),
                entry("Sequence", 2, ElementKind::Wrapper),
                entry("Card", 8, ElementKind::Component),
            ],
            local_frame: 0.0,
            candidate_frames: vec![],
            props: ReceivedProps::default(),
            render_count: 1,
            windows: vec![],
            span: None,
        };
        let mut occurrences = index(vec![pos(3)], vec![(0, 10)], 1);
        occurrences.merge(index(vec![pos(3), pos(8)], vec![(30, 60)], 1));
        let (element, instance) = occurrences.instance_for(&report).expect("found");
        assert_eq!(element.element, pos(3));
        assert_eq!(instance.chain, vec![pos(3), pos(8)]);
        assert_eq!(instance.ranges, vec![(30, 60)]);

        // A `defineScenes` scene: `<Scenes>` is a wrapper too, so a Rect
        // written in the scene function is the element alone.
        let in_scene = LayerSourceReport {
            chain: vec![
                entry("Rect", 3, ElementKind::Primitive),
                entry("Scenes", 12, ElementKind::Wrapper),
                entry("Composition", 1, ElementKind::Wrapper),
            ],
            ..report
        };
        let (_, instance) = occurrences.instance_for(&in_scene).expect("found");
        assert_eq!(instance.chain, vec![pos(3)]);
    }

    #[test]
    fn scenes_of_counts_a_part_as_its_scene() {
        let chapters = [chapter("price", 30, 60), chapter("price-intro", 30, 15)];
        assert_eq!(ids(scenes_of(&[(32, 40)], &chapters)), vec!["price"]);
    }

    #[test]
    fn scenes_of_ignores_touching_edges() {
        let chapters = [chapter("a", 0, 30), chapter("b", 30, 30)];
        assert_eq!(ids(scenes_of(&[(0, 30)], &chapters)), vec!["a"]);
    }

    #[test]
    fn scenes_of_never_matches_a_mark() {
        let chapters = [chapter("a", 0, 30), chapter("mark", 50, 0)];
        assert!(scenes_of(&[(40, 60)], &chapters).is_empty());
    }

    #[test]
    fn outermost_chapter_at_prefers_the_scene() {
        let chapters = [chapter("price", 30, 60), chapter("price-intro", 30, 15)];
        assert_eq!(
            outermost_chapter_at(&chapters, 35).map(|chapter| chapter.id.as_str()),
            Some("price")
        );
    }
}
