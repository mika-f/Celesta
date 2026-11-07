//! A scene's end on the chapter row: which `defineScenes` entry a chapter
//! comes from, how far a drag of its end may go, and the text it writes.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::edit_history::Texts;
use crate::element_edit::{CommitError, apply_checked, reason_text};
use crate::helpers::lexically_normalized;
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

/// Reads every `.ts`/`.tsx`/`.mts`/`.cts` file under `dir` that contains `defineScenes`
/// (`source_files`' skips), and the entry's composition length.
pub(crate) fn find_scene_sources(dir: &Path, entry: &Path, generation: u64) -> SceneSources {
    let mut files = source_files(dir);
    files.sort();
    let lists = files
        .into_iter()
        .filter(|path| {
            matches!(
                path.extension().and_then(OsStr::to_str),
                Some("ts" | "tsx" | "mts" | "cts")
            )
        })
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

/// The files an import of `base` (a relative specifier joined to the
/// importing file's folder) may name, in the order they are tried. With a
/// module extension that is `base` itself, then for `.js`, `.jsx`, `.mjs`
/// and `.cjs` the TypeScript file it compiles from, as TypeScript and
/// esbuild resolve it; without one, `base.ts`, `base.tsx` and the folder's
/// `index.ts` and `index.tsx`.
fn module_files(base: &Path) -> Vec<PathBuf> {
    let sources: &[&str] = match base.extension().and_then(OsStr::to_str) {
        Some("ts" | "tsx" | "mts" | "cts") => &[],
        Some("js") => &["ts", "tsx"],
        Some("jsx") => &["tsx"],
        Some("mjs") => &["mts"],
        Some("cjs") => &["cts"],
        _ => {
            let with = |suffix: &str| {
                let mut path = base.as_os_str().to_owned();
                path.push(suffix);
                PathBuf::from(path)
            };
            return vec![
                with(".ts"),
                with(".tsx"),
                base.join("index.ts"),
                base.join("index.tsx"),
            ];
        }
    };
    std::iter::once(base.to_owned())
        .chain(
            sources
                .iter()
                .map(|extension| base.with_extension(extension)),
        )
        .collect()
}

/// Whether the composition's length is `list`'s `durationInFrames`: its
/// binding in the same file, or a name it is exported under (`default`
/// for a default import) through an import resolving to its file.
pub(crate) fn composition_follows(
    entry: &Path,
    length: Option<&CompositionLength>,
    list_file: &Path,
    list: &SceneList,
    exists: impl Fn(&Path) -> bool,
) -> bool {
    let Some(CompositionLength::ListDuration { module, name }) = length else {
        return false;
    };
    let list_file = lexically_normalized(list_file);
    let Some(module) = module else {
        return list.binding.as_ref() == Some(name) && lexically_normalized(entry) == list_file;
    };
    if !list.exports.contains(name) {
        return false;
    }
    let relative = module.starts_with("./") || module.starts_with("../");
    let Some(dir) = entry.parent().filter(|_| relative) else {
        return false;
    };
    module_files(&lexically_normalized(&dir.join(module)))
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
        /// The ids of the chapters that move with its end: the entries after
        /// it in its list, then the chapters of no entry of the list that
        /// start inside one of those (parts placed in a later scene, such as
        /// with `listChapters(plan, { from: scenes[1].from })`).
        later: Vec<String>,
        follows: bool,
    },
    Locked(String),
}

/// Why the end of a chapter no `defineScenes` entry (or several) has
/// doesn't drag.
fn not_found(id: &str) -> String {
    format!("Can’t find scene `{id}` in the source")
}

/// Why the end of a part (a chapter inside another, such as one listed
/// with `listChapters`) that no list has doesn't drag.
const NOT_AN_ENTRY: &str = "Not a `defineScenes` entry";

/// Why the end of an entry read before something its list holds that
/// can't be read (a spread, a `.map()`) doesn't drag: the entries after it
/// move with it, and they aren't known.
const BEFORE_UNREAD: &str = "Entries after it in its list can’t be read (a spread or `.map()`)";

/// The second line of a chapter no list has, when a list has entries that
/// can't be read: it may be one of them.
fn unread_in(file: &Path) -> String {
    let name = file.file_name().unwrap_or(file.as_os_str());
    format!(
        "A `defineScenes` list in `{}` has entries Celesta can’t read",
        name.to_string_lossy()
    )
}

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

/// The chapters a drag of the end of `list`'s entry `index` moves: the
/// entries after it, which `defineScenes` places after its end, then the
/// chapters of no entry of the list that start inside one of them, as a
/// `from` reading a later scene's start places them. A chapter outside every
/// later scene (after the list's end, or in a gap between its scenes) stays.
/// One placed at a written frame inside a later scene moves too: its `from`
/// can't be told from one reading the scene's start, and the reload puts it
/// back.
fn moving_chapters(chapters: &[ReactChapter], list: &SceneList, index: usize) -> Vec<String> {
    let after = &list.entries[index + 1..];
    let scenes: Vec<&ReactChapter> = chapters
        .iter()
        .filter(|chapter| after.iter().any(|entry| entry.id == chapter.id))
        .collect();
    let parts = chapters.iter().filter(|chapter| {
        !list.entries.iter().any(|entry| entry.id == chapter.id)
            && scenes
                .iter()
                .any(|scene| scene.from <= chapter.from && chapter.from < scene.end())
    });
    after
        .iter()
        .map(|entry| entry.id.clone())
        .chain(parts.map(|chapter| chapter.id.clone()))
        .collect()
}

/// The edge of each chapter: editable when one entry of all the lists has
/// its id and a number written as a literal (or a `const` of the file),
/// and every entry after it in its list was read; else locked with why. A
/// top-level chapter no list has may come from a list's entries that can't
/// be read (`SceneList::unread`): the lock names the first such list's
/// file.
pub(crate) fn scene_edges(
    sources: &SceneSources,
    chapters: &[ReactChapter],
    exists: impl Fn(&Path) -> bool,
) -> HashMap<String, SceneEdge> {
    let unread = sources
        .lists
        .iter()
        .find(|(_, list)| !list.unread.is_empty())
        .map(|(file, _)| file.as_path());
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
                [(_, list, index)] if list.unread.iter().any(|&at| at > *index) => {
                    SceneEdge::Locked(BEFORE_UNREAD.to_owned())
                }
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
                            later: moving_chapters(chapters, list, *index),
                            follows: composition_follows(
                                &sources.entry,
                                sources.length.as_ref(),
                                file,
                                list,
                                &exists,
                            ),
                        },
                        EditTarget::Locked(reason) => {
                            SceneEdge::Locked(lock_text(reason, &entry.inputs))
                        }
                        _ => SceneEdge::Locked(format!("Can’t write `{DURATION}`")),
                    }
                }
                [] if is_part(chapters, at) => SceneEdge::Locked(NOT_AN_ENTRY.to_owned()),
                [] => match unread {
                    Some(file) => SceneEdge::Locked(format!(
                        "{}\n{}",
                        not_found(&chapter.id),
                        unread_in(file)
                    )),
                    None => SceneEdge::Locked(not_found(&chapter.id)),
                },
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

/// `delta` within the bounds, and the note when the composition's end, or a
/// chapter that would start after it, stopped it. The dragged chapter `id`
/// keeps a length ≥ 0, and no chapter of `later` (those moving by `delta`,
/// see `SceneEdge::Editable`) starts before the list's `first` chapter. When
/// the composition's length doesn't follow the list, no later chapter starts
/// at or after the composition's end, and the dragged end doesn't pass it.
/// When it follows, every chapter that stays, the dragged one included,
/// starts before the end of what moves (`compositionChapters` throws for a
/// chapter at or after the end).
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
    let mut lowest = later
        .iter()
        .map(|chapter| floor - frames(chapter.from))
        .fold(-visible, i64::max);
    // The list then ends no sooner than what moves, moved (the entries
    // before the dragged one taken to end at 0, a bound at least as
    // strict): a chapter that stays must start before that.
    let mut holding = None;
    if follows {
        let moving_end = later
            .iter()
            .map(|chapter| frames(chapter.end()))
            .fold(frames(chapter.from) + visible, i64::max);
        // Of a part and the dragged chapter starting together, the part
        // holds the end: it is the one the note names.
        let mut tightest: Option<(i64, &ReactChapter)> = None;
        for still in chapters
            .iter()
            .filter(|still| !later.iter().any(|moving| moving.id == still.id))
        {
            let bound = frames(still.from) + 1 - moving_end;
            let wins = tightest
                .is_none_or(|(best, held)| bound > best || (bound == best && held.id == id));
            if wins {
                tightest = Some((bound, still));
            }
        }
        if let Some((bound, still)) = tightest.filter(|(bound, _)| *bound > lowest) {
            lowest = bound;
            holding = (still.id != id).then_some(still);
        }
    }
    let lowest = lowest.min(0);
    let mut delta = delta;
    let mut note = None;
    if delta < lowest {
        note =
            holding.map(|still| format!("Chapter `{}` starts at frame {}", still.id, still.from));
    }
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

/// What the end of `id` snaps to: the playhead, the edges of the chapters
/// that stay where they are, and those of the selected element's `bands`
/// that end by its end (a band after it may move with the later
/// chapters); its own end left out.
pub(crate) fn scene_snap_targets(
    playhead: f64,
    chapters: &[ReactChapter],
    id: &str,
    later: &[String],
    bands: &[(f64, f64)],
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
    let bands: Vec<(f64, f64)> = bands.iter().copied().filter(|&(_, to)| to <= end).collect();
    snap_targets(playhead, &still, &bands, (end, end))
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

/// Why a chapter's end doesn't drag before the first search lands, and why
/// a drag released while the search for the current reload runs is not
/// written.
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

    /// The end of chapter `id` as the chapter row shows it and a press
    /// starts a drag of it: as the search for the current reload found it,
    /// or while that search runs, as the last one did (a reload rarely
    /// changes a scene list, and the release checks the edge again with
    /// [`Self::current_scene_edge`]). "Reading source…" before the first.
    pub(crate) fn scene_edge(&self, id: &str) -> SceneEdge {
        match &self.scene_edges {
            Some((_, edges)) => edges
                .get(id)
                .cloned()
                .unwrap_or_else(|| SceneEdge::Locked(not_found(id))),
            None => SceneEdge::Locked(READING.to_owned()),
        }
    }

    /// The end of chapter `id` as the search for the current reload found
    /// it; "Reading source…" while it runs.
    pub(crate) fn current_scene_edge(&self, id: &str) -> SceneEdge {
        match &self.scene_edges {
            Some((generation, _)) if *generation == self.react_reload_generation => {
                self.scene_edge(id)
            }
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

    /// `defineScenes` counts only as `@celesta.studio/react`'s.
    const IMPORT: &str = "import { defineScenes } from '@celesta.studio/react';\n";

    const TIMELINE: &str = "import { defineScenes } from '@celesta.studio/react';\n\nexport const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: 90 },\n  { id: 'main', durationInFrames: 120 },\n  { id: 'outro', durationInFrames: 90 },\n]);\n";

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
            "import {{ Composition }} from '@celesta.studio/react';\n{import}\nexport const Film = () => (\n  <Composition width={{1920}} height={{1080}} fps={{30}} durationInFrames={duration}>\n  </Composition>\n);\n"
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
        let other = format!(
            "{IMPORT}export const MORE = defineScenes([\n  {{ id: 'main', durationInFrames: 30 }},\n]);\n"
        );
        let found = sources(
            &[("/p/timeline.tsx", TIMELINE), ("/p/more.ts", &other)],
            &film("", "{300}"),
        );
        let mut chapters = three();
        chapters.push(chapter("credits", 300, 30));
        let edges = scene_edges(&found, &chapters, only_timeline);
        assert_eq!(
            edges.get("main"),
            Some(&SceneEdge::Locked(
                "Can’t find scene `main` in the source".to_owned()
            ))
        );
        assert_eq!(
            edges.get("credits"),
            Some(&SceneEdge::Locked(
                "Can’t find scene `credits` in the source".to_owned()
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
            locked("Can’t find scene `credits` in the source")
        );
        assert!(matches!(
            edges.get("outro"),
            Some(SceneEdge::Editable { .. })
        ));
    }

    #[test]
    fn a_list_with_entries_it_can_t_read() {
        // As `examples/delta/v0.8.0/timeline.ts` writes it.
        let text = format!(
            "{IMPORT}export const SCENES = defineScenes([\n  {{ id: 'open', durationInFrames: 30 }},\n  ...CHAPTERS.map((length, i) => ({{ id: `chapter-${{i}}`, durationInFrames: length }})),\n  {{ id: 'outro', durationInFrames: 60 }},\n]);\n"
        );
        let found = sources(&[("/p/timeline.ts", &text)], &film("", "{300}"));
        let chapters = vec![
            chapter("open", 0, 30),
            chapter("chapter-0", 30, 80),
            chapter("chapter-1", 110, 80),
            chapter("outro", 190, 60),
            // Listed with `listChapters` inside a chapter, and at the top.
            chapter("open-title", 10, 10),
            chapter("credits", 250, 50),
        ];
        let edges = scene_edges(&found, &chapters, only_timeline);
        let locked = |id: &str| match edges.get(id) {
            Some(SceneEdge::Locked(text)) => text.clone(),
            other => panic!("{id} is not locked: {other:?}"),
        };
        // Before the spread: what moves with it isn't known.
        assert_eq!(
            locked("open"),
            "Entries after it in its list can’t be read (a spread or `.map()`)"
        );
        // After it: every later entry is known.
        assert!(matches!(
            edges.get("outro"),
            Some(SceneEdge::Editable { first, later, .. }) if first == "open" && later.is_empty()
        ));
        // A top-level chapter no list has may be one of the entries.
        for id in ["chapter-0", "credits"] {
            assert_eq!(
                locked(id),
                format!(
                    "Can’t find scene `{id}` in the source\nA `defineScenes` list in `timeline.ts` has entries Celesta can’t read"
                )
            );
        }
        assert_eq!(locked("open-title"), "Not a `defineScenes` entry");

        // A list whose argument isn't an array literal.
        let text = format!("{IMPORT}export const SCENES = defineScenes(ENTRIES);\n");
        let found = sources(&[("/p/scenes.ts", &text)], &film("", "{300}"));
        assert_eq!(
            scene_edges(&found, &three(), only_timeline)["intro"],
            SceneEdge::Locked(
                "Can’t find scene `intro` in the source\nA `defineScenes` list in `scenes.ts` has entries Celesta can’t read"
                    .to_owned()
            )
        );
    }

    #[test]
    fn an_expression_duration_locks_with_its_inputs() {
        let text = format!(
            "{IMPORT}const BAR = 30;\nexport const S = {{ words: 40 }};\nexport const END = 30;\nexport const SCENES = defineScenes([\n  {{ id: 'intro', durationInFrames: BAR * 3 }},\n  {{ id: 'main', durationInFrames: S.words - 1 }},\n  {{ id: 'outro' }},\n  {{ id: 'end', durationInFrames: END }},\n]);\n"
        );
        let found = sources(&[("/p/timeline.ts", &text)], &film("", "{300}"));
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
            "Written as an expression: S.words - 1\nComes from S, which can’t be traced"
        );
        assert_eq!(locked("outro"), "Scene `outro` has no durationInFrames");
        assert_eq!(locked("end"), "Exported, so other files may read it");
    }

    #[test]
    fn composition_follows_an_imported_scene_list() {
        let entry = Path::new("/p/film.tsx");
        let timeline = Path::new("/p/timeline.tsx");
        // A list bound to `binding` and exported under that name.
        let follows = |film: &str, list: &Path, binding: &str, exists: &dyn Fn(&Path) -> bool| {
            let exported = SceneList {
                binding: Some(binding.to_owned()),
                exports: vec![binding.to_owned()],
                entries: Vec::new(),
                unread: Vec::new(),
            };
            composition_follows(
                entry,
                composition_length(film, entry).as_ref(),
                list,
                &exported,
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
        // With its extension, as the examples import.
        let explicit = film("import { SCENES } from './timeline.tsx';", duration);
        assert!(follows(&explicit, timeline, "SCENES", &only_timeline));
        // `.js` names the `.ts` or `.tsx` file it compiles to, `.mjs` the `.mts`.
        let compiled = film("import { SCENES } from './timeline.js';", duration);
        assert!(follows(&compiled, timeline, "SCENES", &only_timeline));
        let module = Path::new("/p/timeline.mts");
        let mjs = film("import { SCENES } from './timeline.mjs';", duration);
        assert!(follows(&mjs, module, "SCENES", &|path: &Path| path == module));
        // A `.js` file of that name comes first.
        let js = Path::new("/p/timeline.js");
        let both = |path: &Path| path == js || only_timeline(path);
        assert!(!follows(&compiled, timeline, "SCENES", &both));
        assert!(follows(&compiled, js, "SCENES", &both));
    }

    #[test]
    fn composition_follows_a_list_by_the_name_it_is_exported_under() {
        let entry = Path::new("/p/film.tsx");
        let timeline = Path::new("/p/timeline.tsx");
        let follows = |film: &str, list: &str| {
            let list = &scene_lists(list, timeline)[0];
            composition_follows(
                entry,
                composition_length(film, entry).as_ref(),
                timeline,
                list,
                only_timeline,
            )
        };
        let list = "defineScenes([{ id: 'a', durationInFrames: 30 }])";
        let default_import = film(
            "import SCENES from './timeline';",
            "{SCENES.durationInFrames}",
        );
        assert!(follows(
            &default_import,
            &format!("{IMPORT}export default {list};\n")
        ));
        assert!(follows(
            &default_import,
            &format!("{IMPORT}const S = {list};\nexport default S;\n")
        ));
        // Renamed by the export.
        let renamed = film(
            "import { SCENES } from './timeline';",
            "{SCENES.durationInFrames}",
        );
        assert!(follows(
            &renamed,
            &format!("{IMPORT}const S = {list};\nexport {{ S as SCENES }};\n")
        ));
        // A binding of that name that isn't exported, or isn't the default.
        assert!(!follows(
            &renamed,
            &format!("{IMPORT}const SCENES = {list};\n")
        ));
        assert!(!follows(
            &default_import,
            &format!("{IMPORT}export const SCENES = {list};\n")
        ));
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

    /// `three()` with parts: `main-hook` inside Main, `outro-hook` and
    /// `outro-end` inside Outro (listed with `listChapters(plan, { from:
    /// scenes[k].from })`).
    fn with_parts() -> Vec<ReactChapter> {
        let mut chapters = three();
        chapters.extend([
            chapter("main-hook", 100, 20),
            chapter("outro-hook", 210, 30),
            chapter("outro-end", 280, 10),
        ]);
        chapters
    }

    #[test]
    fn chapters_starting_inside_a_later_scene_move_with_the_end() {
        // `outro-end` is also an entry of another list: it still moves.
        let plan = format!(
            "{IMPORT}export const PLAN = defineScenes([\n  {{ id: 'outro-end', durationInFrames: 10 }},\n]);\n"
        );
        let found = sources(
            &[("/p/timeline.tsx", TIMELINE), ("/p/plan.ts", &plan)],
            &film("", "{330}"),
        );
        // `credits` is placed at frame 300 of a 330-frame composition.
        let mut chapters = with_parts();
        chapters.push(chapter("credits", 300, 30));
        let edges = scene_edges(&found, &chapters, only_timeline);
        let later = |id: &str| match edges.get(id) {
            Some(SceneEdge::Editable { later, .. }) => later.clone(),
            other => panic!("{id} is not editable: {other:?}"),
        };
        assert_eq!(
            later("intro"),
            ids(&["main", "outro", "main-hook", "outro-hook", "outro-end"])
        );
        // `main-hook` is inside the dragged scene, which keeps its start.
        assert_eq!(later("main"), ids(&["outro", "outro-hook", "outro-end"]));
        // `credits` starts after every scene of the list.
        assert_eq!(later("outro"), ids(&[]));
        // The other list's own scenes don't take this list's chapters.
        assert_eq!(later("outro-end"), ids(&[]));
    }

    #[test]
    fn moving_parts_bound_a_literal_length() {
        // `outro-end` moved to 300 would start at the composition's end.
        let later = ids(&["outro", "outro-hook", "outro-end"]);
        let clamp =
            |delta| clamp_scene_delta(&with_parts(), "main", "intro", &later, delta, 300, false);
        assert_eq!(
            clamp(40),
            (19, Some("The composition is 300 frames long".to_owned()))
        );
        assert_eq!(clamp(10), (10, None));
    }

    #[test]
    fn a_following_composition_ends_after_every_chapter_that_stays() {
        let chapters = with_parts();
        let clamp = |id, later: &[&str], delta| {
            clamp_scene_delta(&chapters, id, "intro", &ids(later), delta, 300, true)
        };
        // Shortening Outro: `outro-end` would start at or after the end.
        assert_eq!(
            clamp("outro", &[], -100),
            (
                -19,
                Some("Chapter `outro-end` starts at frame 280".to_owned())
            )
        );
        assert_eq!(clamp("outro", &[], -10), (-10, None));
        assert_eq!(clamp("outro", &[], 40), (40, None));
        // Without parts, the last scene keeps a frame: a scene starting at
        // the end throws too.
        assert_eq!(
            clamp_scene_delta(&three(), "outro", "intro", &[], -100, 300, true),
            (-89, None)
        );
        // A part starting with the dragged scene holds the same frame, and
        // the note names it.
        let mut together = three();
        together.push(chapter("outro-hook", 210, 30));
        assert_eq!(
            clamp_scene_delta(&together, "outro", "intro", &[], -100, 300, true),
            (
                -89,
                Some("Chapter `outro-hook` starts at frame 210".to_owned())
            )
        );
        // What moves keeps the end ahead of what stays: Main shrinks to 0.
        assert_eq!(
            clamp("main", &["outro", "outro-hook", "outro-end"], -200),
            (-120, None)
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
        let text = format!(
            "{IMPORT}export const SCENES = defineScenes([\n  {{ id: 'intro', durationInFrames: 100 }},\n  {{ id: 'outro', durationInFrames: 200 }},\n]);\n"
        );
        let found = sources(&[("/p/timeline.tsx", &text)], &film("", "{250}"));
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
    fn a_drag_moves_the_parts_inside_later_scenes() {
        let later = ids(&["outro", "outro-hook", "outro-end"]);
        assert_eq!(
            spans(&preview_chapters(&with_parts(), "main", &later, -40)),
            [
                ("intro", 0, 90),
                ("main", 90, 170),
                ("outro", 170, 260),
                ("main-hook", 100, 120),
                ("outro-hook", 170, 200),
                ("outro-end", 240, 250)
            ]
        );
        let targets = scene_snap_targets(0.0, &with_parts(), "main", &later);
        for moving in [240.0, 280.0, 290.0] {
            assert!(!targets.contains(&moving), "{moving} in {targets:?}");
        }
        assert!(targets.contains(&120.0));
    }

    #[test]
    fn scene_snap_targets_leave_out_the_end_and_what_moves() {
        let mut chapters = three();
        chapters.push(chapter("credits", 250, 20));
        // The selection's bands: one inside `main`, one ending at its end,
        // one inside `outro`.
        let bands = [(120.0, 150.0), (180.0, 210.0), (220.0, 240.0)];
        let targets = scene_snap_targets(45.0, &chapters, "main", &ids(&["outro"]), &bands);
        for edge in [45.0, 0.0, 90.0, 250.0, 270.0, 120.0, 150.0, 180.0] {
            assert!(targets.contains(&edge), "{edge} in {targets:?}");
        }
        // The dragged end, `outro`'s edges, and the band inside it, which
        // move with it.
        for edge in [210.0, 300.0, 220.0, 240.0] {
            assert!(!targets.contains(&edge), "{edge} not in {targets:?}");
        }
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
        let text = &format!(
            "{IMPORT}const INTRO = 90;\nexport const SCENES = defineScenes([\n  {{ id: 'intro', durationInFrames: INTRO }},\n]);\n"
        );
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
            format!(
                "{IMPORT}export const S = defineScenes([{{ id: '{id}', durationInFrames: 30 }}]);\n"
            )
        };
        for (file, text) in [
            ("timeline.ts", list("root")),
            ("scenes/more.tsx", list("nested")),
            ("node_modules/x.ts", list("module")),
            ("dist/y.ts", list("built")),
            (".tmp/z.ts", list("temporary")),
            ("other.js", list("script")),
            ("module.mts", list("module")),
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
                (dir.join("module.mts"), "module".to_owned()),
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
