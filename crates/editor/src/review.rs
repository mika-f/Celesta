//! The open entry's review comments: what the Comments tab lists, the
//! timeline marks and the canvas pins, and the writes they make. The files
//! are `celesta_project::review`'s; this keeps them loaded and decides what
//! shows where.

use std::path::Path;
use std::time::SystemTime;

use celesta_plugin_host::project_root;
use celesta_project::review::{
    Anchor, ChapterSpan, Comment, CommentStore, Placement, Point, ReviewError, default_author,
};
use celesta_react_bridge::{ReactChapter, chapter_at};

/// What the comments folder looked like when it was last read: each file's
/// name, length, and modification time. Another program's write (an agent
/// resolving a comment) changes it.
pub(crate) type FolderStamp = Vec<(String, u64, Option<SystemTime>)>;

/// Samples `dir` for [`FolderStamp`]; empty when it does not exist.
pub(crate) fn folder_stamp(dir: &Path) -> FolderStamp {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut stamp: FolderStamp = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let meta = entry.metadata().ok()?;
            name.ends_with(".json")
                .then(|| (name, meta.len(), meta.modified().ok()))
        })
        .collect();
    stamp.sort();
    stamp
}

/// The composition comments are shown on.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shown<'a> {
    /// The selected root composition; `None` for a `.celesta.json` project.
    pub(crate) composition: Option<&'a str>,
    /// Whether it is the entry's first root, where comments without a
    /// composition show.
    pub(crate) first: bool,
    pub(crate) chapters: &'a [ReactChapter],
}

impl<'a> Shown<'a> {
    /// The chapters as spans, borrowing the chapters rather than `self`.
    fn owned_spans(self) -> Vec<ChapterSpan<'a>> {
        self.chapters
            .iter()
            .map(|chapter| ChapterSpan {
                id: &chapter.id,
                name: &chapter.name,
                from: chapter.from,
            })
            .collect()
    }

    fn spans(&self) -> Vec<ChapterSpan<'_>> {
        self.chapters
            .iter()
            .map(|chapter| ChapterSpan {
                id: &chapter.id,
                name: &chapter.name,
                from: chapter.from,
            })
            .collect()
    }
}

/// One comment as the panel, the timeline, and the canvas show it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Listed {
    pub(crate) comment: Comment,
    /// 1-based, in creation order among the entry's comments: the label of
    /// its timeline mark and canvas pin, kept when others are resolved.
    pub(crate) number: usize,
    pub(crate) placement: Placement,
}

impl Listed {
    pub(crate) fn id(&self) -> &str {
        &self.comment.id
    }
}

/// What a new comment will point at, chosen as the composer opens.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Draft {
    pub(crate) anchor: Anchor,
    /// The composition the anchor was taken on; the comment is saved only
    /// while it is still the one shown.
    pub(crate) composition: Option<String>,
    /// A press on the canvas places the point (the composer's "Pin" button).
    pub(crate) pinning: bool,
}

/// The entry's comments and the Comments tab's state.
#[derive(Debug, Default)]
pub(crate) struct Review {
    /// `None` for the built-in demo, which has no folder to write in.
    store: Option<CommentStore>,
    entry: String,
    author: String,
    comments: Vec<Comment>,
    /// Files that did not read, as `path: message`.
    pub(crate) problems: Vec<String>,
    pub(crate) stamp: FolderStamp,
    pub(crate) selected: Option<String>,
    pub(crate) show_resolved: bool,
    /// The comment being written, while the composer is open.
    pub(crate) draft: Option<Draft>,
    /// Why the last write failed, until the next one.
    pub(crate) error: Option<String>,
}

impl Review {
    /// The comments of the project `source` belongs to, on `source`.
    pub(crate) fn open(source: Option<&Path>) -> Self {
        let Some(source) = source else {
            return Self::default();
        };
        let store = CommentStore::new(project_root(source));
        let mut review = Self {
            entry: store.entry_key(source),
            author: default_author(store.root()),
            store: Some(store),
            ..Self::default()
        };
        review.reload();
        review
    }

    /// The folder to watch for other programs' writes.
    pub(crate) fn folder(&self) -> Option<std::path::PathBuf> {
        self.store.as_ref().map(CommentStore::dir)
    }

    pub(crate) fn author(&self) -> &str {
        &self.author
    }

    /// Reads the folder again. A selected comment that is gone is let go.
    pub(crate) fn reload(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        self.stamp = folder_stamp(&store.dir());
        let loaded = store.load();
        self.problems = loaded
            .problems
            .iter()
            .map(|problem| format!("{}: {}", problem.path.display(), problem.message))
            .collect();
        self.comments = loaded.comments;
        if let Some(selected) = &self.selected
            && !self.comments.iter().any(|comment| &comment.id == selected)
        {
            self.selected = None;
        }
    }

    /// Every comment on the shown composition, open ones and, with
    /// `show_resolved`, resolved ones, in creation order.
    pub(crate) fn listed(&self, shown: Shown<'_>) -> Vec<Listed> {
        self.on(shown)
            .filter(|listed| self.show_resolved || listed.comment.is_open())
            .collect()
    }

    /// How many open comments the shown composition has.
    pub(crate) fn open_count(&self, shown: Shown<'_>) -> usize {
        self.on(shown)
            .filter(|listed| listed.comment.is_open())
            .count()
    }

    fn on<'a>(&'a self, shown: Shown<'a>) -> impl Iterator<Item = Listed> + 'a {
        let spans = shown.owned_spans();
        self.comments
            .iter()
            .filter(move |comment| {
                comment.is_on(&self.entry, shown.composition)
                    && (comment.composition.is_some() || shown.first)
            })
            .enumerate()
            .map(move |(index, comment)| Listed {
                comment: comment.clone(),
                number: index + 1,
                placement: comment.place(&spans),
            })
    }

    /// Opens the composer on what is in front of the reviewer: the playhead
    /// at `frame`, or the In/Out span `marks`, in the chapter showing
    /// there, on the selected `layer`.
    pub(crate) fn begin(&mut self, at: At, layer: Option<String>, shown: Shown<'_>) {
        self.error = None;
        self.draft = Some(Draft {
            anchor: anchor_at(at, layer, shown.chapters),
            composition: shown.composition.map(str::to_owned),
            pinning: false,
        });
    }

    /// Sets the draft's canvas point, ending the pin press.
    pub(crate) fn pin(&mut self, point: Point) {
        if let Some(draft) = &mut self.draft {
            draft.anchor.point = Some(point);
            draft.pinning = false;
        }
    }

    pub(crate) fn cancel(&mut self) {
        self.draft = None;
        self.error = None;
    }

    /// Writes the draft as a new comment with `body` and selects it.
    pub(crate) fn add(
        &mut self,
        body: &str,
        shown: Shown<'_>,
        several_roots: bool,
    ) -> Result<String, ReviewError> {
        let body = body.trim();
        let (Some(store), Some(draft)) = (&self.store, &self.draft) else {
            return Err(ReviewError::NotFound(String::new()));
        };
        if draft.composition.as_deref() != shown.composition {
            // Its frames and chapter are the other composition's.
            self.draft = None;
            let refused = ReviewError::Refused(
                "the composition changed while it was written; add it again".to_owned(),
            );
            self.error = Some(refused.to_string());
            return Err(refused);
        }
        let mut comment = Comment::new(
            self.entry.clone(),
            draft.anchor.clone(),
            self.author.clone(),
            body.to_owned(),
        );
        if several_roots {
            comment.composition = shown.composition.map(str::to_owned);
        }
        comment.remember(&shown.spans());
        let written = store.save(&comment);
        self.finish(written.map(|_| comment.id.clone()), |review, id| {
            review.draft = None;
            review.selected = Some(id.clone());
        })
    }

    /// Adds `body` as a reply to `id` by the reviewer.
    pub(crate) fn reply(
        &mut self,
        id: &str,
        body: &str,
        shown: Shown<'_>,
    ) -> Result<String, ReviewError> {
        let (author, body) = (self.author.clone(), body.trim().to_owned());
        self.change(id, shown, |comment| {
            comment.reply(author, body);
        })
    }

    /// Resolves `id` (or reopens it).
    pub(crate) fn set_resolved(
        &mut self,
        id: &str,
        resolved: bool,
        shown: Shown<'_>,
    ) -> Result<String, ReviewError> {
        let by = self.author.clone();
        self.change(id, shown, |comment| {
            if resolved {
                comment.resolve(by);
            } else {
                comment.reopen();
            }
        })
    }

    fn change(
        &mut self,
        id: &str,
        shown: Shown<'_>,
        change: impl FnOnce(&mut Comment),
    ) -> Result<String, ReviewError> {
        let Some(store) = &self.store else {
            return Err(ReviewError::NotFound(id.to_owned()));
        };
        let spans = shown.spans();
        let written = store
            .update(id, |comment| {
                change(comment);
                comment.remember(&spans);
            })
            .map(|comment| comment.id);
        self.finish(written, |_, _| {})
    }

    /// Reloads after a write, or keeps its error for the panel.
    fn finish(
        &mut self,
        written: Result<String, ReviewError>,
        then: impl FnOnce(&mut Self, &String),
    ) -> Result<String, ReviewError> {
        match &written {
            Ok(id) => {
                self.error = None;
                self.reload();
                then(self, id);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        written
    }
}

/// Where a new comment goes: the playhead, the In/Out marks, and the
/// composition's length in frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct At {
    pub(crate) frame: u64,
    pub(crate) marks: Option<(u64, u64)>,
    pub(crate) frames: u64,
}

/// The anchor of a comment made at `at.frame`, or on the span `at.marks`:
/// frames from the start of the chapter showing at its first frame, or from
/// the composition's start where no chapter shows. A playhead at the end
/// (Go to End puts it one past the last frame) comments on the last frame,
/// and a span ends at the end at the latest. A span wholly past the end
/// (the composition got shorter) is ignored, like marks out of order.
pub(crate) fn anchor_at(at: At, layer: Option<String>, chapters: &[ReactChapter]) -> Anchor {
    let last = at.frames.saturating_sub(1);
    let (from, to) = match at.marks {
        Some((start, end)) if start < end.min(at.frames) => (start, Some(end.min(at.frames))),
        _ => (at.frame.min(last), None),
    };
    let chapter = chapter_at(chapters, from);
    let start = chapter.map_or(0, |chapter| chapter.from);
    Anchor {
        chapter: chapter.map(|chapter| chapter.id.clone()),
        from: from - start,
        to: to.map(|to| to - start),
        layer,
        ..Anchor::default()
    }
}

/// Where `listed`'s mark sits on a composition of `end_frame` frames: its
/// start and end as fractions of it. `None` for an orphan.
pub(crate) fn mark_span(listed: &Listed, end_frame: u64) -> Option<(f64, f64)> {
    let Placement::Placed { from, to, .. } = listed.placement else {
        return None;
    };
    let span = end_frame.max(1) as f64;
    let start = (from as f64 / span).clamp(0.0, 1.0);
    Some((
        start,
        to.map_or(start, |to| (to as f64 / span).clamp(0.0, 1.0)),
    ))
}

/// The comments whose pin shows on the canvas at `frame`: the selected
/// one wherever the playhead is, and any other whose span covers it (or
/// whose moment it is).
pub(crate) fn pins_at<'a>(
    listed: &'a [Listed],
    selected: Option<&str>,
    frame: u64,
) -> impl Iterator<Item = (&'a Listed, Point)> {
    listed.iter().filter_map(move |listed| {
        let point = listed.comment.anchor.point.clone()?;
        let shows = Some(listed.id()) == selected
            || (listed.comment.is_open() && listed.placement.covers(frame));
        shows.then_some((listed, point))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(id: &str, from: u64, length: u64) -> ReactChapter {
        ReactChapter {
            id: id.to_owned(),
            name: id.to_uppercase(),
            from,
            duration_in_frames: length,
        }
    }

    /// The playhead at `frame` (and `marks`) in a 300-frame composition.
    fn at(frame: u64, marks: Option<(u64, u64)>) -> At {
        At {
            frame,
            marks,
            frames: 300,
        }
    }

    fn chapters() -> Vec<ReactChapter> {
        vec![chapter("intro", 0, 90), chapter("demo", 90, 120)]
    }

    fn review() -> (std::path::PathBuf, Review) {
        let dir = crate::scratch::scratch_dir("review");
        std::fs::write(dir.join("celesta.config.json"), "{}").expect("written");
        let entry = dir.join("film.tsx");
        std::fs::write(&entry, "").expect("written");
        (dir, Review::open(Some(&entry)))
    }

    fn shown(chapters: &[ReactChapter]) -> Shown<'_> {
        Shown {
            composition: Some("main"),
            first: true,
            chapters,
        }
    }

    #[test]
    fn a_comment_at_the_playhead_counts_from_its_chapter() {
        let anchor = anchor_at(at(100, None), Some("title".to_owned()), &chapters());
        assert_eq!(anchor.chapter.as_deref(), Some("demo"));
        assert_eq!((anchor.from, anchor.to), (10, None));
        assert_eq!(anchor.layer.as_deref(), Some("title"));
    }

    #[test]
    fn marks_make_a_span_from_the_chapter_at_its_start() {
        let anchor = anchor_at(at(0, Some((95, 150))), None, &chapters());
        assert_eq!(anchor.chapter.as_deref(), Some("demo"));
        assert_eq!((anchor.from, anchor.to), (5, Some(60)));
        // Marks out of order are ignored.
        let anchor = anchor_at(at(30, Some((150, 95))), None, &chapters());
        assert_eq!(
            (anchor.chapter.as_deref(), anchor.from),
            (Some("intro"), 30)
        );
    }

    #[test]
    fn the_end_of_the_timeline_comments_on_the_last_frame() {
        let anchor = anchor_at(at(300, None), None, &[]);
        assert_eq!((anchor.from, anchor.to), (299, None));
        let anchor = anchor_at(at(0, Some((250, 400))), None, &[]);
        assert_eq!((anchor.from, anchor.to), (250, Some(300)));
        // Marks left past a shortened composition fall back to the playhead.
        let anchor = anchor_at(
            At {
                frames: 200,
                ..at(40, Some((250, 280)))
            },
            None,
            &[],
        );
        assert_eq!((anchor.from, anchor.to), (40, None));
    }

    #[test]
    fn a_draft_from_another_composition_is_not_saved() {
        let (_dir, mut review) = review();
        review.begin(at(0, None), None, shown(&[]));
        let teaser = Shown {
            composition: Some("teaser"),
            first: false,
            chapters: &[],
        };
        assert!(matches!(
            review.add("x", teaser, true),
            Err(ReviewError::Refused(_))
        ));
        assert!(review.draft.is_none());
        assert!(review.error.is_some());
        assert!(review.listed(shown(&[])).is_empty());
    }

    #[test]
    fn past_the_chapters_frames_are_absolute() {
        let anchor = anchor_at(at(250, None), None, &chapters());
        assert_eq!((anchor.chapter, anchor.from), (None, 250));
    }

    #[test]
    fn adding_writes_a_file_and_selects_it() {
        let (_dir, mut review) = review();
        let chapters = chapters();
        review.begin(at(100, None), None, shown(&chapters));
        review.pin(Point::new(0.5, 0.25));
        let id = review
            .add("  Slower here ", shown(&chapters), false)
            .expect("written");
        assert!(review.draft.is_none());
        assert_eq!(review.selected.as_deref(), Some(id.as_str()));
        let listed = review.listed(shown(&chapters));
        assert_eq!(listed.len(), 1);
        let comment = &listed[0].comment;
        assert_eq!(comment.body, "Slower here");
        assert_eq!(comment.composition, None, "one root: no composition");
        assert_eq!(comment.last_seen.as_ref().map(|seen| seen.frame), Some(100));
        assert_eq!(listed[0].placement.frame(), Some(100));
        assert!(
            review
                .folder()
                .expect("a folder")
                .join(format!("{id}.json"))
                .exists()
        );
    }

    #[test]
    fn several_roots_name_the_composition() {
        let (_dir, mut review) = review();
        review.begin(at(0, None), None, shown(&[]));
        let id = review.add("x", shown(&[]), true).expect("written");
        let teaser = Shown {
            composition: Some("teaser"),
            first: false,
            chapters: &[],
        };
        assert!(review.listed(teaser).is_empty());
        assert_eq!(
            review.listed(shown(&[]))[0].comment.composition.as_deref(),
            Some("main")
        );
        assert_eq!(review.listed(shown(&[]))[0].id(), id);
    }

    #[test]
    fn resolved_comments_hide_until_asked_for_and_keep_their_numbers() {
        let (_dir, mut review) = review();
        let chapters = chapters();
        let mut ids = Vec::new();
        for frame in [10, 20] {
            review.begin(at(frame, None), None, shown(&chapters));
            ids.push(review.add("x", shown(&chapters), false).expect("written"));
        }
        // Two comments made in one second are ordered by id: compare the
        // number each had before.
        let number = |review: &Review, id: &str| {
            review
                .listed(shown(&chapters))
                .iter()
                .find(|listed| listed.id() == id)
                .map(|listed| listed.number)
        };
        let before = number(&review, &ids[1]);
        review
            .set_resolved(&ids[0], true, shown(&chapters))
            .expect("written");
        let listed = review.listed(shown(&chapters));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id(), ids[1]);
        assert_eq!(number(&review, &ids[1]), before);
        assert_eq!(review.open_count(shown(&chapters)), 1);
        review.show_resolved = true;
        assert_eq!(review.listed(shown(&chapters)).len(), 2);
        review
            .set_resolved(&ids[0], false, shown(&chapters))
            .expect("written");
        assert_eq!(review.open_count(shown(&chapters)), 2);
    }

    #[test]
    fn replies_are_by_the_reviewer() {
        let (_dir, mut review) = review();
        review.begin(at(0, None), None, shown(&[]));
        let id = review.add("x", shown(&[]), false).expect("written");
        review.reply(&id, "Done", shown(&[])).expect("written");
        let listed = review.listed(shown(&[]));
        let reply = &listed[0].comment.replies[0];
        assert_eq!(
            (reply.author.as_str(), reply.body.as_str()),
            (review.author(), "Done")
        );
    }

    #[test]
    fn a_removed_chapter_orphans_its_comments() {
        let (_dir, mut review) = review();
        let chapters = chapters();
        review.begin(at(100, None), None, shown(&chapters));
        review.add("x", shown(&chapters), false).expect("written");
        let listed = review.listed(shown(&chapters[..1]));
        assert!(listed[0].placement.is_orphaned());
        assert_eq!(listed[0].placement.frame(), Some(100));
        assert_eq!(mark_span(&listed[0], 300), None);
    }

    #[test]
    fn another_programs_write_changes_the_stamp() {
        let (_dir, mut review) = review();
        review.begin(at(0, None), None, shown(&[]));
        let id = review.add("x", shown(&[]), false).expect("written");
        let folder = review.folder().expect("a folder");
        assert_eq!(folder_stamp(&folder), review.stamp);
        let store = CommentStore::new(folder.parent().expect("the root"));
        store
            .update(&id, |comment| comment.resolve("agent".to_owned()))
            .expect("resolved");
        assert_ne!(folder_stamp(&folder), review.stamp);
        review.reload();
        assert_eq!(review.open_count(shown(&[])), 0);
    }

    #[test]
    fn marks_and_pins() {
        let (_dir, mut review) = review();
        let chapters = chapters();
        review.begin(at(0, Some((90, 120))), None, shown(&chapters));
        review.pin(Point::new(0.1, 0.2));
        let span = review
            .add("span", shown(&chapters), false)
            .expect("written");
        review.begin(at(150, None), None, shown(&chapters));
        review.pin(Point::new(0.3, 0.4));
        let moment = review
            .add("moment", shown(&chapters), false)
            .expect("written");
        let listed = review.listed(shown(&chapters));
        let by_id = |id: &str| {
            listed
                .iter()
                .find(|listed| listed.id() == id)
                .expect("listed")
        };
        assert_eq!(mark_span(by_id(&span), 300), Some((0.3, 0.4)));
        assert_eq!(mark_span(by_id(&moment), 300), Some((0.5, 0.5)));
        let at = |frame, selected: Option<&str>| -> Vec<String> {
            pins_at(&listed, selected, frame)
                .map(|(listed, _)| listed.id().to_owned())
                .collect()
        };
        assert_eq!(at(100, None), vec![span.clone()]);
        assert_eq!(at(150, None), vec![moment.clone()]);
        assert!(at(200, None).is_empty());
        assert_eq!(at(200, Some(&moment)), vec![moment]);
    }

    #[test]
    fn the_demo_has_no_comments_to_write() {
        let mut review = Review::open(None);
        assert!(review.folder().is_none());
        review.begin(at(0, None), None, shown(&[]));
        assert!(review.add("x", shown(&[]), false).is_err());
    }
}
