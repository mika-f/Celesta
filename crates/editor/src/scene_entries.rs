//! A scene's end on the chapter row: which `defineScenes` entry a chapter
//! comes from, how far a drag of its end may go, and the text it writes.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::edit_history::Texts;
use crate::element_edit::{CommitError, apply_checked, reason_text};
use crate::source::source_files;
use crate::timeline_drag::snap_targets;
use crate::view::EditorView;
use celesta_react_bridge::ReactChapter;
use celesta_source_edit::{
    CompositionLength, EditTarget, EditValue, LockReason, SceneList, composition_length,
    format_number, scene_lists,
};
use gpui_kit::Context;

const DURATION: &str = "durationInFrames";

/// The `defineScenes` lists under the React entry's folder, read for the
/// reload `generation`, and how the entry writes its composition's length.
#[derive(Debug, Default)]
pub(crate) struct SceneSources {
    pub(crate) generation: u64,
    pub(crate) lists: Vec<(PathBuf, SceneList)>,
    pub(crate) length: Option<CompositionLength>,
    pub(crate) entry: PathBuf,
}

/// Reads every `.ts`/`.tsx` file under `dir` that contains `defineScenes`
/// (`source_files`' skips), and the entry's composition length.
pub(crate) fn find_scene_sources(dir: &Path, entry: &Path, generation: u64) -> SceneSources {
    let mut files = source_files(dir);
    files.sort();
    let lists = files
        .into_iter()
        .filter(|path| matches!(path.extension().and_then(OsStr::to_str), Some("ts" | "tsx")))
        .filter_map(|path| {
            let text = fs::read_to_string(&path).ok()?;
            text.contains("defineScenes").then_some((path, text))
        })
        .flat_map(|(path, text)| {
            scene_lists(&text, &path)
                .into_iter()
                .map(move |list| (path.clone(), list))
        })
        .collect();
    let length = fs::read_to_string(entry)
        .ok()
        .and_then(|text| composition_length(&text, entry));
    SceneSources {
        generation,
        lists,
        length,
        entry: entry.to_path_buf(),
    }
}

/// `path` with its `.` and `..` components taken out, without reading the
/// file system.
fn normalized(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// The files an import of `base` (a relative specifier joined to the
/// importing file's folder) may name, in the order they are tried.
fn module_files(base: &Path) -> Vec<PathBuf> {
    let with = |suffix: &str| {
        let mut path = base.as_os_str().to_owned();
        path.push(suffix);
        PathBuf::from(path)
    };
    vec![
        with(".ts"),
        with(".tsx"),
        base.join("index.ts"),
        base.join("index.tsx"),
    ]
}

/// Whether the composition's length is the list's `durationInFrames` (same
/// file, or an import resolving to it).
pub(crate) fn composition_follows(
    entry: &Path,
    length: Option<&CompositionLength>,
    list_file: &Path,
    binding: Option<&str>,
    exists: impl Fn(&Path) -> bool,
) -> bool {
    let Some(CompositionLength::ListDuration { module, name }) = length else {
        return false;
    };
    if binding != Some(name.as_str()) {
        return false;
    }
    let list_file = normalized(list_file);
    let Some(module) = module else {
        return normalized(entry) == list_file;
    };
    let relative = module.starts_with("./") || module.starts_with("../");
    let Some(dir) = entry.parent().filter(|_| relative) else {
        return false;
    };
    module_files(&normalized(&dir.join(module)))
        .into_iter()
        .find(|file| exists(file))
        .is_some_and(|file| file == list_file)
}

/// Whether a chapter's end drags, and what it writes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SceneEdge {
    Editable {
        file: PathBuf,
        duration: EditTarget,
        /// The id of its list's first entry.
        first: String,
        /// The ids after it in its list.
        later: Vec<String>,
        follows: bool,
    },
    Locked(String),
}

/// Why the end of a chapter no `defineScenes` entry (or several) has
/// doesn't drag.
fn not_found(id: &str) -> String {
    format!("Can't find scene `{id}` in the source")
}

/// Why the end of a part (a chapter inside another, such as one listed
/// with `listChapters`) that no list has doesn't drag.
const NOT_AN_ENTRY: &str = "Not a `defineScenes` entry";

/// Whether `chapter` is a part: another chapter's span holds it (of two
/// with the same span, the first is the scene).
fn is_part(chapters: &[ReactChapter], at: usize) -> bool {
    let chapter = &chapters[at];
    chapters.iter().enumerate().any(|(index, other)| {
        index != at
            && other.duration_in_frames > 0
            && other.from <= chapter.from
            && chapter.end() <= other.end()
            && (index < at || (other.from, other.end()) != (chapter.from, chapter.end()))
    })
}

/// A locked duration's reason, and for an expression the inputs it reads,
/// one a line (`BAR = 30`, or why the input is locked).
fn lock_text(reason: &LockReason, inputs: &[(String, EditTarget)]) -> String {
    let mut lines = vec![reason_text(reason)];
    lines.extend(inputs.iter().map(|(name, input)| match input {
        EditTarget::Literal {
            value: EditValue::Number(value),
            ..
        } => format!("{name} = {}", format_number(*value)),
        EditTarget::Locked(reason) => reason_text(reason),
        _ => name.clone(),
    }));
    lines.join("\n")
}

/// The edge of each chapter: editable when one entry of all the lists has
/// its id and a number written as a literal (or a `const` of the file);
/// else locked with why.
pub(crate) fn scene_edges(
    sources: &SceneSources,
    chapters: &[ReactChapter],
    exists: impl Fn(&Path) -> bool,
) -> HashMap<String, SceneEdge> {
    chapters
        .iter()
        .enumerate()
        .map(|(at, chapter)| {
            let found: Vec<(&PathBuf, &SceneList, usize)> = sources
                .lists
                .iter()
                .flat_map(|(file, list)| {
                    list.entries
                        .iter()
                        .enumerate()
                        .filter(|(_, entry)| entry.id == chapter.id)
                        .map(move |(index, _)| (file, list, index))
                })
                .collect();
            let edge = match found.as_slice() {
                [(file, list, index)] => {
                    let entry = &list.entries[*index];
                    match &entry.duration {
                        EditTarget::Literal {
                            value: EditValue::Number(_),
                            ..
                        } => SceneEdge::Editable {
                            file: (*file).clone(),
                            duration: entry.duration.clone(),
                            first: list.entries[0].id.clone(),
                            later: list.entries[index + 1..]
                                .iter()
                                .map(|entry| entry.id.clone())
                                .collect(),
                            follows: composition_follows(
                                &sources.entry,
                                sources.length.as_ref(),
                                file,
                                list.binding.as_deref(),
                                &exists,
                            ),
                        },
                        EditTarget::Locked(reason) => {
                            SceneEdge::Locked(lock_text(reason, &entry.inputs))
                        }
                        _ => SceneEdge::Locked(format!("Can't write `{DURATION}`")),
                    }
                }
                [] if is_part(chapters, at) => SceneEdge::Locked(NOT_AN_ENTRY.to_owned()),
                _ => SceneEdge::Locked(not_found(&chapter.id)),
            };
            (chapter.id.clone(), edge)
        })
        .collect()
}

/// The frames of `chapter` the composition shows: all of it, unless the
/// composition ends first.
fn visible_length(chapter: &ReactChapter, composition_end: u64) -> u64 {
    chapter
        .end()
        .min(composition_end)
        .saturating_sub(chapter.from)
}

/// `delta` within the bounds, and the note when the composition's end
/// stopped it. The dragged chapter `id` keeps a length ≥ 0, and no chapter
/// of `later` (the entries after it in its list, which move by `delta`)
/// starts before the list's `first` chapter. When the composition's length
/// doesn't follow the list, no later chapter starts at or after the
/// composition's end, and the dragged end doesn't pass it.
pub(crate) fn clamp_scene_delta(
    chapters: &[ReactChapter],
    id: &str,
    first: &str,
    later: &[String],
    delta: i64,
    composition_end: u64,
    follows: bool,
) -> (i64, Option<String>) {
    let Some(chapter) = chapters.iter().find(|chapter| chapter.id == id) else {
        return (0, None);
    };
    let later: Vec<&ReactChapter> = chapters
        .iter()
        .filter(|chapter| later.contains(&chapter.id))
        .collect();
    let frames = |frame: u64| i64::try_from(frame).unwrap_or(i64::MAX);
    let visible = frames(visible_length(chapter, composition_end));
    // The list starts where its first chapter does (its own offset taken
    // as 0, a bound at least as strict); without it, the earliest of the
    // chapters that count.
    let floor = chapters
        .iter()
        .find(|chapter| chapter.id == first)
        .or_else(|| {
            later
                .iter()
                .copied()
                .chain([chapter])
                .min_by_key(|chapter| chapter.from)
        })
        .map_or(0, |first| frames(first.from));
    // A later chapter already before the floor holds the drag back, never
    // pushes it forward.
    let lowest = later
        .iter()
        .map(|chapter| floor - frames(chapter.from))
        .fold(-visible, i64::max)
        .min(0);
    let mut delta = delta;
    let mut note = None;
    if !follows {
        let end = frames(composition_end);
        let highest = later
            .iter()
            .map(|chapter| end - 1 - frames(chapter.from))
            .fold(end - frames(chapter.from) - visible, i64::min);
        if delta > highest {
            delta = highest;
            note = Some(format!("The composition is {composition_end} frames long"));
        }
    }
    (delta.max(lowest), note)
}

/// The length a drag of `delta` writes: what the composition shows of the
/// chapter, plus `delta` (an entry cut at the composition's end would
/// otherwise change by an unseen amount).
pub(crate) fn written_length(
    chapters: &[ReactChapter],
    id: &str,
    delta: i64,
    composition_end: u64,
) -> f64 {
    let visible = chapters
        .iter()
        .find(|chapter| chapter.id == id)
        .map_or(0, |chapter| visible_length(chapter, composition_end));
    visible as f64 + delta as f64
}

/// Whether writing `written` leaves the entry `edge` writes as it is (a
/// chapter the composition cuts, dragged back to its entry's length): then
/// nothing is written and no reload comes.
pub(crate) fn writes_nothing(edge: &SceneEdge, written: f64) -> bool {
    matches!(
        edge,
        SceneEdge::Editable {
            duration: EditTarget::Literal {
                value: EditValue::Number(now),
                ..
            },
            ..
        } if *now == written
    )
}

/// `chapters` as a drag of `id`'s end by `delta` shows them: its length
/// changed, the chapters of `later` moved.
pub(crate) fn preview_chapters(
    chapters: &[ReactChapter],
    id: &str,
    later: &[String],
    delta: i64,
) -> Vec<ReactChapter> {
    let moved = |frame: u64| frame.saturating_add_signed(delta);
    chapters
        .iter()
        .map(|chapter| {
            let mut chapter = chapter.clone();
            if chapter.id == id {
                chapter.duration_in_frames = moved(chapter.duration_in_frames);
            } else if later.contains(&chapter.id) {
                chapter.from = moved(chapter.from);
            }
            chapter
        })
        .collect()
}

/// What the end of `id` snaps to: the playhead and the edges of the
/// chapters that stay where they are, its own end left out.
pub(crate) fn scene_snap_targets(
    playhead: f64,
    chapters: &[ReactChapter],
    id: &str,
    later: &[String],
) -> Vec<f64> {
    let Some(end) = chapters
        .iter()
        .find(|chapter| chapter.id == id)
        .map(|chapter| chapter.end() as f64)
    else {
        return Vec::new();
    };
    let still: Vec<ReactChapter> = chapters
        .iter()
        .filter(|chapter| !later.contains(&chapter.id))
        .cloned()
        .collect();
    snap_targets(playhead, &still, &[], (end, end))
}

/// The entry's new text: found again in `texts`, at the same place,
/// `durationInFrames` set to `written`.
pub(crate) fn scene_files(
    texts: &Texts,
    id: &str,
    edge: &SceneEdge,
    written: f64,
) -> Result<Vec<(PathBuf, String)>, CommitError> {
    let SceneEdge::Editable { file, duration, .. } = edge else {
        return Err(CommitError::NotEditable(DURATION.to_owned()));
    };
    let text = texts.get(file).ok_or(CommitError::SourceChanged)?;
    let found: Vec<EditTarget> = scene_lists(text, file)
        .into_iter()
        .flat_map(|list| list.entries)
        .filter(|entry| entry.id == id)
        .map(|entry| entry.duration)
        .collect();
    let same = match (found.as_slice(), duration) {
        ([EditTarget::Literal { span: now, .. }], EditTarget::Literal { span: was, .. }) => {
            now == was
        }
        _ => false,
    };
    if !same {
        return Err(CommitError::SourceChanged);
    }
    if !written.is_finite() {
        return Err(CommitError::NotANumber);
    }
    let edit = duration
        .to_edit(DURATION, &EditValue::Number(written))
        .ok_or_else(|| CommitError::NotEditable(DURATION.to_owned()))?;
    let written = apply_checked(file, text, &[edit])?;
    Ok(vec![(file.clone(), written)])
}

/// Why a chapter's end doesn't drag while the search for this reload runs.
const READING: &str = "Reading source…";

impl EditorView {
    /// Starts the scene search for the current reload on the background
    /// executor; its edges replace the last ones when it lands, unless a
    /// newer reload (or another file) came meanwhile.
    pub(crate) fn refresh_scene_sources(&mut self, cx: &mut Context<Self>) {
        let Some(react) = self.react_preview.as_ref() else {
            self.scene_edges = None;
            return;
        };
        let entry = react.entry.clone();
        let Some(dir) = entry.parent().map(Path::to_path_buf) else {
            return;
        };
        let chapters = react.chapters.clone();
        let generation = self.react_reload_generation;
        let session = self.session;
        cx.spawn(async move |view, cx| {
            let (generation, edges) = cx
                .background_executor()
                .spawn(async move {
                    let sources = find_scene_sources(&dir, &entry, generation);
                    let edges = scene_edges(&sources, &chapters, Path::is_file);
                    (sources.generation, edges)
                })
                .await;
            view.update(cx, |this, cx| {
                if this.session == session && this.react_reload_generation == generation {
                    this.scene_edges = Some((generation, edges));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The end of chapter `id`, as the search for the current reload found
    /// it.
    pub(crate) fn scene_edge(&self, id: &str) -> SceneEdge {
        match &self.scene_edges {
            Some((generation, edges)) if *generation == self.react_reload_generation => edges
                .get(id)
                .cloned()
                .unwrap_or_else(|| SceneEdge::Locked(not_found(id))),
            _ => SceneEdge::Locked(READING.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(id: &str, from: u64, duration_in_frames: u64) -> ReactChapter {
        ReactChapter {
            id: id.to_owned(),
            name: id.to_owned(),
            from,
            duration_in_frames,
        }
    }

    const TIMELINE: &str = "import { defineScenes } from '@celesta/react';\n\nexport const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: 90 },\n  { id: 'main', durationInFrames: 120 },\n  { id: 'outro', durationInFrames: 90 },\n]);\n";

    fn sources(lists: &[(&str, &str)], film: &str) -> SceneSources {
        let entry = PathBuf::from("/p/film.tsx");
        SceneSources {
            generation: 1,
            lists: lists
                .iter()
                .flat_map(|(file, text)| {
                    let path = PathBuf::from(file);
                    scene_lists(text, &path)
                        .into_iter()
                        .map(move |list| (path.clone(), list))
                })
                .collect(),
            length: composition_length(film, &entry),
            entry,
        }
    }

    fn film(import: &str, duration: &str) -> String {
        format!(
            "import {{ Composition }} from '@celesta/react';\n{import}\nexport const Film = () => (\n  <Composition width={{1920}} height={{1080}} fps={{30}} durationInFrames={duration}>\n  </Composition>\n);\n"
        )
    }

    fn three() -> Vec<ReactChapter> {
        vec![
            chapter("intro", 0, 90),
            chapter("main", 90, 120),
            chapter("outro", 210, 90),
        ]
    }

    fn ids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    fn only_timeline(path: &Path) -> bool {
        path == Path::new("/p/timeline.tsx")
    }

    #[test]
    fn an_entry_found_once_is_editable() {
        let found = sources(
            &[("/p/timeline.tsx", TIMELINE)],
            &film(
                "import { SCENES } from './timeline';",
                "{SCENES.durationInFrames}",
            ),
        );
        let edges = scene_edges(&found, &three(), only_timeline);
        let Some(SceneEdge::Editable {
            file,
            duration,
            first,
            later,
            follows,
        }) = edges.get("main")
        else {
            panic!("main is not editable: {edges:?}");
        };
        assert_eq!(file, Path::new("/p/timeline.tsx"));
        assert!(matches!(
            duration,
            EditTarget::Literal {
                value: EditValue::Number(120.0),
                ..
            }
        ));
        assert_eq!(first, "intro");
        assert_eq!(later, &ids(&["outro"]));
        assert!(follows);
        assert!(matches!(
            edges.get("outro"),
            Some(SceneEdge::Editable { later, .. }) if later.is_empty()
        ));
    }

    #[test]
    fn zero_or_two_entries_lock_with_the_reason() {
        let other =
            "export const MORE = defineScenes([\n  { id: 'main', durationInFrames: 30 },\n]);\n";
        let found = sources(
            &[("/p/timeline.tsx", TIMELINE), ("/p/more.ts", other)],
            &film("", "{300}"),
        );
        let mut chapters = three();
        chapters.push(chapter("credits", 300, 30));
        let edges = scene_edges(&found, &chapters, only_timeline);
        assert_eq!(
            edges.get("main"),
            Some(&SceneEdge::Locked(
                "Can't find scene `main` in the source".to_owned()
            ))
        );
        assert_eq!(
            edges.get("credits"),
            Some(&SceneEdge::Locked(
                "Can't find scene `credits` in the source".to_owned()
            ))
        );
        assert!(matches!(
            edges.get("intro"),
            Some(SceneEdge::Editable { follows: false, .. })
        ));
    }

    #[test]
    fn a_part_no_list_has_is_not_an_entry() {
        let found = sources(&[("/p/timeline.tsx", TIMELINE)], &film("", "{300}"));
        let mut chapters = three();
        // Listed with `listChapters` inside Main, and one as long as Outro.
        chapters.push(chapter("main-hook", 100, 20));
        chapters.push(chapter("outro-all", 210, 90));
        // At the top level, a chapter no list has may be a scene of a list
        // Celesta skipped.
        chapters.push(chapter("credits", 300, 30));
        let edges = scene_edges(&found, &chapters, only_timeline);
        let locked = |text: &str| Some(SceneEdge::Locked(text.to_owned()));
        assert_eq!(
            edges.get("main-hook").cloned(),
            locked("Not a `defineScenes` entry")
        );
        assert_eq!(
            edges.get("outro-all").cloned(),
            locked("Not a `defineScenes` entry")
        );
        assert_eq!(
            edges.get("credits").cloned(),
            locked("Can't find scene `credits` in the source")
        );
        assert!(matches!(
            edges.get("outro"),
            Some(SceneEdge::Editable { .. })
        ));
    }

    #[test]
    fn an_expression_duration_locks_with_its_inputs() {
        let text = "const BAR = 30;\nexport const S = { words: 40 };\nexport const END = 30;\nexport const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: BAR * 3 },\n  { id: 'main', durationInFrames: S.words - 1 },\n  { id: 'outro' },\n  { id: 'end', durationInFrames: END },\n]);\n";
        let found = sources(&[("/p/timeline.ts", text)], &film("", "{300}"));
        let chapters = vec![
            chapter("intro", 0, 90),
            chapter("main", 90, 39),
            chapter("outro", 129, 0),
            chapter("end", 129, 30),
        ];
        let edges = scene_edges(&found, &chapters, only_timeline);
        let locked = |id: &str| match edges.get(id) {
            Some(SceneEdge::Locked(text)) => text.clone(),
            other => panic!("{id} is not locked: {other:?}"),
        };
        assert_eq!(
            locked("intro"),
            "Written as an expression: BAR * 3\nBAR = 30"
        );
        assert_eq!(
            locked("main"),
            "Written as an expression: S.words - 1\nComes from S, which can't be traced"
        );
        assert_eq!(locked("outro"), "Scene `outro` has no durationInFrames");
        assert_eq!(locked("end"), "Exported, so other files may read it");
    }

    #[test]
    fn composition_follows_an_imported_scene_list() {
        let entry = Path::new("/p/film.tsx");
        let timeline = Path::new("/p/timeline.tsx");
        let follows = |film: &str, list: &Path, binding: &str, exists: &dyn Fn(&Path) -> bool| {
            composition_follows(
                entry,
                composition_length(film, entry).as_ref(),
                list,
                Some(binding),
                exists,
            )
        };
        let duration = "{SCENES.durationInFrames}";
        let import = film("import { SCENES } from './timeline';", duration);
        assert!(follows(&import, timeline, "SCENES", &only_timeline));
        let alias = film(
            "import { SCENES as S } from './timeline';",
            "{S.durationInFrames}",
        );
        assert!(follows(&alias, timeline, "SCENES", &only_timeline));
        // Another binding of the same file.
        assert!(!follows(&import, timeline, "MORE", &only_timeline));
        // `./timeline` resolves to another file's list.
        let index = |path: &Path| path == Path::new("/p/timeline/index.ts");
        assert!(!follows(&import, timeline, "SCENES", &index));
        assert!(follows(
            &import,
            Path::new("/p/timeline/index.ts"),
            "SCENES",
            &index
        ));
        // `.ts` before `.tsx`.
        let both = |path: &Path| {
            path == Path::new("/p/timeline.ts") || path == Path::new("/p/timeline.tsx")
        };
        assert!(!follows(&import, timeline, "SCENES", &both));
        // A path through `..`.
        let nested = film("import { SCENES } from '../p/./timeline';", duration);
        assert!(follows(&nested, timeline, "SCENES", &only_timeline));
        // A literal.
        assert!(!follows(
            &film("", "{300}"),
            timeline,
            "SCENES",
            &only_timeline
        ));
        // The list in the entry itself.
        let local = film(
            "const SCENES = defineScenes([{ id: 'a', durationInFrames: 30 }]);",
            duration,
        );
        assert!(follows(&local, entry, "SCENES", &only_timeline));
        assert!(!follows(&local, timeline, "SCENES", &only_timeline));
        // A package.
        let package = film("import { SCENES } from 'scenes';", duration);
        assert!(!follows(&package, timeline, "SCENES", &|_: &Path| true));
    }

    #[test]
    fn the_clamp_keeps_chapters_inside_a_literal_length() {
        let later = ids(&["main", "outro"]);
        let clamp = |delta, follows| {
            clamp_scene_delta(&three(), "intro", "intro", &later, delta, 300, follows)
        };
        assert_eq!(
            clamp(100, false),
            (89, Some("The composition is 300 frames long".to_owned()))
        );
        assert_eq!(clamp(100, true), (100, None));
        assert_eq!(clamp(50, false), (50, None));
        // The last chapter's end stops at the composition's end.
        assert_eq!(
            clamp_scene_delta(&three(), "outro", "intro", &[], 20, 300, false),
            (0, Some("The composition is 300 frames long".to_owned()))
        );
        assert_eq!(
            clamp_scene_delta(&three(), "outro", "intro", &[], 20, 300, true),
            (20, None)
        );
    }

    #[test]
    fn the_clamp_keeps_lengths_and_starts_non_negative() {
        let later = ids(&["main", "outro"]);
        assert_eq!(
            clamp_scene_delta(&three(), "intro", "intro", &later, -120, 300, false),
            (-90, None)
        );
        // `outro` overlaps back to 60 (`offset: -150`): it can't start
        // before the list's first chapter.
        let chapters = vec![
            chapter("intro", 0, 90),
            chapter("outro", 60, 90),
            chapter("main", 90, 120),
        ];
        assert_eq!(
            clamp_scene_delta(
                &chapters,
                "main",
                "intro",
                &ids(&["outro"]),
                -100,
                300,
                false
            ),
            (-60, None)
        );
        // A list placed later: its first chapter starts at 100.
        let placed = vec![chapter("a", 100, 30), chapter("b", 110, 30)];
        assert_eq!(
            clamp_scene_delta(&placed, "a", "a", &ids(&["b"]), -30, 300, false),
            (-10, None)
        );
        // `b` already starts before the list's first chapter: the floor
        // holds the drag back but never pushes it forward.
        let below = vec![chapter("a", 100, 30), chapter("b", 90, 30)];
        let clamp = |delta| clamp_scene_delta(&below, "a", "a", &ids(&["b"]), delta, 300, true);
        assert_eq!(clamp(0), (0, None));
        assert_eq!(clamp(-5), (0, None));
        assert_eq!(clamp(5), (5, None));
    }

    #[test]
    fn only_later_entries_of_the_same_list_count() {
        // `credits`, of another list, starts after `outro`: it neither moves
        // nor bounds the drag.
        let mut chapters = three();
        chapters.push(chapter("credits", 280, 20));
        assert_eq!(
            clamp_scene_delta(
                &chapters,
                "intro",
                "intro",
                &ids(&["main", "outro"]),
                80,
                300,
                false
            ),
            (80, None)
        );
    }

    #[test]
    fn a_clipped_chapter_writes_its_visible_length() {
        // The entry is 200 long; the composition ends 150 frames in.
        let chapters = vec![chapter("intro", 0, 100), chapter("outro", 100, 150)];
        assert_eq!(written_length(&chapters, "outro", -10, 250), 140.0);
        // The runtime's chapter is already cut; one that isn't is cut too.
        let uncut = vec![chapter("intro", 0, 100), chapter("outro", 100, 200)];
        assert_eq!(written_length(&uncut, "outro", -10, 250), 140.0);
        assert_eq!(written_length(&uncut, "intro", 15, 250), 115.0);

        // Dragged back to the entry's own length, it writes nothing.
        let text = "export const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: 100 },\n  { id: 'outro', durationInFrames: 200 },\n]);\n";
        let found = sources(&[("/p/timeline.tsx", text)], &film("", "{250}"));
        let edges = scene_edges(&found, &chapters, only_timeline);
        let outro = &edges["outro"];
        assert!(writes_nothing(
            outro,
            written_length(&chapters, "outro", 50, 250)
        ));
        assert!(!writes_nothing(
            outro,
            written_length(&chapters, "outro", -10, 250)
        ));
        assert!(!writes_nothing(&SceneEdge::Locked(String::new()), 200.0));
    }

    fn spans(chapters: &[ReactChapter]) -> Vec<(&str, u64, u64)> {
        chapters
            .iter()
            .map(|chapter| (chapter.id.as_str(), chapter.from, chapter.end()))
            .collect()
    }

    #[test]
    fn a_drag_moves_the_end_and_the_later_chapters() {
        let mut chapters = three();
        chapters.push(chapter("credits", 250, 20));
        let later = ids(&["outro"]);
        assert_eq!(
            spans(&preview_chapters(&chapters, "main", &later, 15)),
            [
                ("intro", 0, 90),
                ("main", 90, 225),
                ("outro", 225, 315),
                ("credits", 250, 270)
            ]
        );
        assert_eq!(
            spans(&preview_chapters(&chapters, "main", &later, -40)),
            [
                ("intro", 0, 90),
                ("main", 90, 170),
                ("outro", 170, 260),
                ("credits", 250, 270)
            ]
        );
    }

    #[test]
    fn scene_snap_targets_leave_out_the_end_and_what_moves() {
        let mut chapters = three();
        chapters.push(chapter("credits", 250, 20));
        let targets = scene_snap_targets(45.0, &chapters, "main", &ids(&["outro"]));
        for edge in [45.0, 0.0, 90.0, 250.0, 270.0] {
            assert!(targets.contains(&edge), "{edge} in {targets:?}");
        }
        // The dragged end, and `outro`'s edges, which move with it.
        assert!(!targets.contains(&210.0));
        assert!(!targets.contains(&300.0));
    }

    fn edge_in(text: &str, path: &str, id: &str) -> SceneEdge {
        let found = sources(&[(path, text)], &film("", "{300}"));
        scene_edges(&found, &three(), only_timeline)
            .remove(id)
            .expect("the chapter has an edge")
    }

    #[test]
    fn scene_files_writes_the_entry() {
        // `.ts`: `<number>v` is a cast, not JSX.
        let text = format!("const v: unknown = 1;\nconst n = <number>v;\n{TIMELINE}");
        let path = "/p/timeline.ts";
        let edge = edge_in(&text, path, "intro");
        let texts = Texts::from([(PathBuf::from(path), text.clone())]);
        let files = scene_files(&texts, "intro", &edge, 100.0).expect("the entry is written");
        assert_eq!(
            files,
            [(
                PathBuf::from(path),
                text.replace(
                    "'intro', durationInFrames: 90",
                    "'intro', durationInFrames: 100"
                )
            )]
        );

        // A const of the file.
        let text = "const INTRO = 90;\nexport const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: INTRO },\n]);\n";
        let path = "/p/timeline.tsx";
        let edge = edge_in(text, path, "intro");
        let texts = Texts::from([(PathBuf::from(path), text.to_owned())]);
        let files = scene_files(&texts, "intro", &edge, 75.0).expect("the const is written");
        assert_eq!(files[0].1, text.replace("= 90", "= 75"));

        // A locked edge writes nothing.
        let locked = SceneEdge::Locked("Exported, so other files may read it".to_owned());
        assert!(scene_files(&texts, "intro", &locked, 75.0).is_err());
    }

    #[test]
    fn scene_files_refuses_a_changed_source() {
        let path = "/p/timeline.tsx";
        let edge = edge_in(TIMELINE, path, "main");
        // The entry moved.
        let moved = TIMELINE.replace("{ id: 'intro', durationInFrames: 90 },\n", "");
        let texts = Texts::from([(PathBuf::from(path), moved)]);
        assert_eq!(
            scene_files(&texts, "main", &edge, 100.0),
            Err(CommitError::SourceChanged)
        );
        // The entry is gone, or twice in the file.
        let gone = TIMELINE.replace("'main'", "'body'");
        let texts = Texts::from([(PathBuf::from(path), gone)]);
        assert_eq!(
            scene_files(&texts, "main", &edge, 100.0),
            Err(CommitError::SourceChanged)
        );
        let twice = format!(
            "{TIMELINE}export const MORE = defineScenes([{{ id: 'main', durationInFrames: 1 }}]);\n"
        );
        let texts = Texts::from([(PathBuf::from(path), twice)]);
        assert_eq!(
            scene_files(&texts, "main", &edge, 100.0),
            Err(CommitError::SourceChanged)
        );
        // Not read.
        assert_eq!(
            scene_files(&Texts::new(), "main", &edge, 100.0),
            Err(CommitError::SourceChanged)
        );
        // Not a number.
        let texts = Texts::from([(PathBuf::from(path), TIMELINE.to_owned())]);
        assert_eq!(
            scene_files(&texts, "main", &edge, f64::NAN),
            Err(CommitError::NotANumber)
        );
    }

    #[test]
    fn the_search_skips_ignored_folders_and_files_without_define_scenes() {
        let dir = std::env::temp_dir().join(format!("celesta-scene-search-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        let list = |id: &str| {
            format!("export const S = defineScenes([{{ id: '{id}', durationInFrames: 30 }}]);\n")
        };
        for (file, text) in [
            ("timeline.ts", list("root")),
            ("scenes/more.tsx", list("nested")),
            ("node_modules/x.ts", list("module")),
            ("dist/y.ts", list("built")),
            (".tmp/z.ts", list("temporary")),
            ("other.js", list("script")),
            // Without the text: not read for lists.
            ("plain.ts", "export const A = 1;\n".to_owned()),
            (
                "film.tsx",
                film("import { S } from './timeline';", "{S.durationInFrames}"),
            ),
        ] {
            let path = dir.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
        }
        let entry = dir.join("film.tsx");
        let found = find_scene_sources(&dir, &entry, 7);
        let mut found_ids: Vec<(PathBuf, String)> = found
            .lists
            .iter()
            .map(|(file, list)| (file.clone(), list.entries[0].id.clone()))
            .collect();
        found_ids.sort();
        assert_eq!(
            found_ids,
            [
                (dir.join("scenes/more.tsx"), "nested".to_owned()),
                (dir.join("timeline.ts"), "root".to_owned()),
            ]
        );
        assert_eq!(found.generation, 7);
        assert_eq!(found.entry, entry);
        assert_eq!(
            found.length,
            Some(CompositionLength::ListDuration {
                module: Some("./timeline".to_owned()),
                name: "S".to_owned(),
            })
        );
        fs::remove_dir_all(&dir).ok();
    }
}
