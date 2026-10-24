//! An in-memory undo history of the file writes the editor makes (props-file
//! edits and canvas edits to the source files). Each entry keeps the whole
//! text of every file one edit wrote, before and after, so undo is a plain
//! write of the old texts; it refuses when any file no longer holds what the
//! entry wrote.

use crate::actions::{Redo, Undo};
use crate::problems::Source;
use crate::source::{PropertyArgs, watched_mtime};
use crate::view::EditorView;
use celesta_react_bridge::write_temp;
use gpui_kit::{Context, Window};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The most entries kept; the oldest are dropped first.
const CAPACITY: usize = 100;

/// One write to one file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FileEdit {
    pub(crate) path: PathBuf,
    /// The text before the write; `None` when the file did not exist.
    pub(crate) before: Option<String>,
    pub(crate) after: String,
}

impl FileEdit {
    /// Whether the edit changed the file's text (an edit that wrote nothing
    /// is not worth an undo step or a reload).
    pub(crate) fn changes(&self) -> bool {
        self.before.as_deref() != Some(self.after.as_str())
    }
}

/// One undo step: every file one edit wrote.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HistoryEntry {
    pub(crate) files: Vec<FileEdit>,
}

impl HistoryEntry {
    fn touches(&self, path: &Path) -> bool {
        self.files.iter().any(|file| file.path == path)
    }
}

impl From<FileEdit> for HistoryEntry {
    fn from(file: FileEdit) -> Self {
        Self { files: vec![file] }
    }
}

/// The texts of a set of files, keyed by path (a missing file reads `""`).
pub(crate) type Texts = BTreeMap<PathBuf, String>;

#[derive(Debug)]
pub(crate) enum HistoryError {
    /// A file no longer holds what the entry expects, so no file was touched.
    // `path` names the first such file for callers that report per file.
    #[allow(dead_code)]
    ChangedOutside { path: PathBuf },
    /// The file could not be read or written.
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChangedOutside { .. } => formatter.write_str("The file changed outside Celesta"),
            Self::Io { path, source } => {
                write!(formatter, "Couldn’t write {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for HistoryError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Step {
    Undo,
    Redo,
}

/// An entry taken off its stack while its files are being written elsewhere
/// (the UI thread hands it to a background task and settles it afterwards).
#[derive(Debug)]
pub(crate) struct Pending {
    entry: HistoryEntry,
    step: Step,
    epoch: u64,
}

impl Pending {
    /// Checks every file still holds what the step expects, then writes the
    /// other side of all of them; one file that differs refuses the whole
    /// entry. Touches only the files, so it may run off the UI thread.
    pub(crate) fn apply(&self) -> Result<(), HistoryError> {
        let mut writes = Vec::with_capacity(self.entry.files.len());
        for file in &self.entry.files {
            let (expected, written) = match self.step {
                Step::Undo => (Some(file.after.as_str()), file.before.as_deref()),
                Step::Redo => (file.before.as_deref(), Some(file.after.as_str())),
            };
            if read(&file.path)?.as_deref() != expected {
                return Err(HistoryError::ChangedOutside {
                    path: file.path.clone(),
                });
            }
            writes.push((file.path.as_path(), written));
        }
        write_files(&writes)
    }
}

#[derive(Debug, Default)]
pub(crate) struct EditHistory {
    /// Oldest first.
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    /// A step or an own write is running off the UI thread; writes to one
    /// file stay serialized.
    busy: bool,
    /// Bumped by `clear`, so a task that started before it is ignored.
    epoch: u64,
}

impl EditHistory {
    /// Records a write that just happened. A new edit ends the redo chain.
    pub(crate) fn push(&mut self, entry: HistoryEntry) {
        self.redo.clear();
        self.undo.push(entry);
        if self.undo.len() > CAPACITY {
            self.undo.remove(0);
        }
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.busy
    }

    /// Takes the newest entry of `step`'s stack and marks the history busy.
    /// `None` when that stack is empty or the history is already busy.
    pub(crate) fn begin(&mut self, step: Step) -> Option<Pending> {
        if self.busy {
            return None;
        }
        let entry = match step {
            Step::Undo => self.undo.pop(),
            Step::Redo => self.redo.pop(),
        }?;
        self.busy = true;
        Some(Pending {
            entry,
            step,
            epoch: self.epoch,
        })
    }

    /// Puts the outcome of `pending.apply()` in the stacks: a success moves the
    /// entry to the other stack; a refusal drops it and every other entry on
    /// that stack touching any of its files, since they cannot apply; any
    /// other error keeps it. `None` when the history was cleared meanwhile.
    pub(crate) fn settle(
        &mut self,
        pending: Pending,
        result: Result<(), HistoryError>,
    ) -> Option<Result<(), HistoryError>> {
        if pending.epoch != self.epoch {
            return None;
        }
        self.busy = false;
        let Pending { entry, step, .. } = pending;
        let (own, other) = match step {
            Step::Undo => (&mut self.undo, &mut self.redo),
            Step::Redo => (&mut self.redo, &mut self.undo),
        };
        Some(match result {
            Ok(()) => {
                other.push(entry);
                Ok(())
            }
            Err(error @ HistoryError::ChangedOutside { .. }) => {
                own.retain(|rest| !entry.files.iter().any(|file| rest.touches(&file.path)));
                Err(error)
            }
            Err(error) => {
                own.push(entry);
                Err(error)
            }
        })
    }

    /// Marks the history busy for an own write; returns the epoch to hand to
    /// `finish_write`, or `None` when it is already busy.
    pub(crate) fn begin_write(&mut self) -> Option<u64> {
        if self.busy {
            return None;
        }
        self.busy = true;
        Some(self.epoch)
    }

    /// Ends an own write, recording its entry. `false` when the history was
    /// cleared meanwhile (the entry is then not recorded).
    pub(crate) fn finish_write(&mut self, epoch: u64, entry: Option<HistoryEntry>) -> bool {
        if epoch != self.epoch {
            return false;
        }
        self.busy = false;
        if let Some(entry) = entry {
            self.push(entry);
        }
        true
    }

    /// Undoes synchronously (the view runs the same steps in the background)
    /// and returns the paths of the entry it stepped.
    #[cfg(test)]
    fn run(&mut self, step: Step) -> Result<Option<Vec<PathBuf>>, HistoryError> {
        let Some(pending) = self.begin(step) else {
            return Ok(None);
        };
        let paths: Vec<_> = pending
            .entry
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect();
        let result = pending.apply();
        self.settle(pending, result)
            .transpose()
            .map(|stepped| stepped.map(|()| paths))
    }

    #[cfg(test)]
    pub(crate) fn undo(&mut self) -> Result<Option<Vec<PathBuf>>, HistoryError> {
        self.run(Step::Undo)
    }

    #[cfg(test)]
    fn redo(&mut self) -> Result<Option<Vec<PathBuf>>, HistoryError> {
        self.run(Step::Redo)
    }

    pub(crate) fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.busy = false;
        self.epoch += 1;
    }
}

/// The file's text, or `None` when it does not exist.
fn read(path: &Path) -> Result<Option<String>, HistoryError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(HistoryError::Io {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Writes every file of `writes` as one step, as far as the file system
/// allows: each text first goes to a temp file beside its file
/// (`celesta_react_bridge::write_temp`), and only once all of
/// them are written are they renamed over their files and the `None` files
/// removed. A failed temp write removes the temps already written and leaves
/// every file alone. A failed rename removes the temps not yet renamed, but
/// the files renamed before it stay changed, and no history entry records
/// that change.
fn write_files(writes: &[(&Path, Option<&str>)]) -> Result<(), HistoryError> {
    let failed = |path: &Path, source| HistoryError::Io {
        path: path.to_owned(),
        source,
    };
    let mut temps = Vec::with_capacity(writes.len());
    for &(path, text) in writes {
        let Some(text) = text else { continue };
        match write_temp(path, text) {
            Ok(temp) => temps.push((temp, path)),
            Err(source) => {
                for (temp, _) in &temps {
                    let _ = fs::remove_file(temp);
                }
                return Err(failed(path, source));
            }
        }
    }
    let mut temps = temps.into_iter();
    while let Some((temp, path)) = temps.next() {
        if let Err(source) = fs::rename(&temp, path) {
            for (temp, _) in std::iter::once((temp, path)).chain(temps) {
                let _ = fs::remove_file(temp);
            }
            return Err(failed(path, source));
        }
    }
    for &(path, text) in writes {
        if text.is_none() {
            fs::remove_file(path).map_err(|source| failed(path, source))?;
        }
    }
    Ok(())
}

/// Refuses a read-only file, which a rename would replace without a word.
fn refuse_read_only(path: &Path) -> Result<(), String> {
    if fs::metadata(path).is_ok_and(|metadata| metadata.permissions().readonly()) {
        return Err(format!(
            "Couldn’t write {}: the file is read-only",
            path.display()
        ));
    }
    Ok(())
}

/// Reads `path` (a missing file reads as `""`), lets `edit` compute the new
/// text, and writes it atomically; an existing file whose text `edit` left
/// as it was is not written (see [`FileEdit::changes`]).
pub(crate) fn write_recorded(
    path: &Path,
    edit: impl FnOnce(&str) -> Result<String, String>,
) -> Result<FileEdit, String> {
    let before = read(path).map_err(|error| error.to_string())?;
    let after = edit(before.as_deref().unwrap_or(""))?;
    if before.as_deref() != Some(after.as_str()) {
        refuse_read_only(path)?;
        write_files(&[(path, Some(&after))]).map_err(|error| error.to_string())?;
    }
    Ok(FileEdit {
        path: path.to_owned(),
        before,
        after,
    })
}

/// Reads every path in `paths`, lets `edit` compute the new texts from all
/// of them at once (so it can check the set together), writes every changed
/// file to a temp file first and renames them only when all temp writes
/// succeeded. Paths `edit` leaves out, or returns unchanged, are not written.
/// Returns the edits of the files written, in the order `edit` gave them.
pub(crate) fn write_all_recorded(
    paths: Vec<PathBuf>,
    edit: impl FnOnce(&Texts) -> Result<Vec<(PathBuf, String)>, String>,
) -> Result<Vec<FileEdit>, String> {
    let mut befores = BTreeMap::new();
    for path in paths {
        let before = read(&path).map_err(|error| error.to_string())?;
        befores.insert(path, before);
    }
    let texts: Texts = befores
        .iter()
        .map(|(path, before)| (path.clone(), before.clone().unwrap_or_default()))
        .collect();
    let mut files: Vec<FileEdit> = Vec::new();
    for (path, after) in edit(&texts)? {
        let Some(before) = befores.remove(&path) else {
            // Not read, or returned twice.
            return Err(format!(
                "Couldn’t write {}: the edit did not read it",
                path.display()
            ));
        };
        files.push(FileEdit {
            path,
            before,
            after,
        });
    }
    files.retain(FileEdit::changes);
    for file in &files {
        refuse_read_only(&file.path)?;
    }
    let writes: Vec<_> = files
        .iter()
        .map(|file| (file.path.as_path(), Some(file.after.as_str())))
        .collect();
    write_files(&writes).map_err(|error| error.to_string())?;
    Ok(files)
}

impl EditorView {
    /// The React entry file the view has open.
    pub(crate) fn react_entry(&self) -> Option<&Path> {
        self.react_preview
            .as_ref()
            .map(|react| react.entry.as_path())
    }

    /// Takes a write the editor made itself as seen: `watched` (the watched
    /// files' mtime sampled after the write) replaces the reload watcher's, so
    /// the write is not mistaken for an edit from elsewhere. Returns the reload
    /// order a reload requested from now on will read the write at.
    pub(crate) fn apply_own_write_sample(&mut self, watched: Option<SystemTime>) -> u64 {
        let next_reload = self.react_reloads.next();
        // The reload that reads this write keeps the selected layer, whose
        // transform targets are resolved again from the reloaded code.
        self.selection_keep.own_write();
        self.element_edit.invalidate();
        if let Some(react) = self.react_preview.as_mut()
            && watched.is_some()
        {
            react.watched_mtime = watched;
            self.watched_since = next_reload;
        }
        next_reload
    }

    /// The entry and arguments the reload watcher's mtime sample reads.
    fn watch_sample_inputs(&self) -> Option<(PathBuf, PropertyArgs)> {
        let react = self.react_preview.as_ref()?;
        Some((react.entry.clone(), react.property_args.clone()))
    }

    /// Writes `paths` as one own write off the UI thread: `edit` computes the
    /// new texts from the files' current texts (`""` when missing), the write
    /// goes through `write_all_recorded`, and the watched files are sampled
    /// once after all of them are written. Back on the UI thread the files
    /// written are pushed to the history as one entry, the sample is applied
    /// and the entry reloads; then `done` gets the result. An edit that leaves
    /// every text as it was writes nothing, records nothing and reloads
    /// nothing, and `done` gets `Ok`. When the history or a property write is
    /// busy, `done` gets an error at once.
    pub(crate) fn record_own_writes(
        &mut self,
        paths: Vec<PathBuf>,
        edit: impl FnOnce(&Texts) -> Result<Vec<(PathBuf, String)>, String> + Send + 'static,
        done: impl FnOnce(&mut Self, Result<(), String>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let (Some((entry, property_args)), false) =
            (self.watch_sample_inputs(), self.property_panel.is_writing())
        else {
            return done(self, Err(BUSY.to_owned()), cx);
        };
        let Some(epoch) = self.edit_history.begin_write() else {
            return done(self, Err(BUSY.to_owned()), cx);
        };
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let files = write_all_recorded(paths, edit)?;
                    Ok((files, watched_mtime(&entry, &property_args)))
                })
                .await;
            view.update(cx, |this, cx| {
                let outcome = match result {
                    Ok((files, watched)) => {
                        let changes = !files.is_empty();
                        let entry = changes.then_some(HistoryEntry { files });
                        if this.edit_history.finish_write(epoch, entry) && changes {
                            this.apply_own_write_sample(watched);
                            this.request_react_reload(cx);
                        }
                        Ok(())
                    }
                    Err(message) => {
                        this.edit_history.finish_write(epoch, None);
                        Err(message)
                    }
                };
                done(this, outcome, cx);
                this.drain_property_commits(cx);
                this.drain_held_rows(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn undo_action(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.step_history(Step::Undo, cx);
    }

    pub(crate) fn redo_action(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.step_history(Step::Redo, cx);
    }

    /// Undoes or redoes the newest edit off the UI thread, then reloads.
    fn step_history(&mut self, step: Step, cx: &mut Context<Self>) {
        // A property write still running has no entry yet, and writes to one
        // file must not overlap.
        if self.property_panel.is_writing() {
            return;
        }
        let Some((entry, property_args)) = self.watch_sample_inputs() else {
            return;
        };
        let Some(pending) = self.edit_history.begin(step) else {
            return;
        };
        let failure = match step {
            Step::Undo => "Couldn’t undo",
            Step::Redo => "Couldn’t redo",
        };
        cx.spawn(async move |view, cx| {
            let (pending, result, watched) = cx
                .background_executor()
                .spawn(async move {
                    let result = pending.apply();
                    // Sampled after the write, so the reload watcher does not
                    // take it for an edit from elsewhere.
                    let watched = watched_mtime(&entry, &property_args);
                    (pending, result, watched)
                })
                .await;
            view.update(cx, |this, cx| {
                match this.edit_history.settle(pending, result) {
                    Some(Ok(_)) => {
                        this.apply_own_write_sample(watched);
                        this.request_react_reload(cx);
                    }
                    Some(Err(error)) => {
                        this.problems
                            .report(Source::Project, failure, error.to_string());
                    }
                    None => {}
                }
                this.drain_property_commits(cx);
                this.drain_held_rows(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

const BUSY: &str = "Another edit is still being saved";

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_react_bridge::set_property_text;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn scratch(name: &str) -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "celesta-history-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn edit(path: &Path, before: Option<&str>, after: &str) -> FileEdit {
        FileEdit {
            path: path.to_owned(),
            before: before.map(str::to_owned),
            after: after.to_owned(),
        }
    }

    #[test]
    fn undo_restores_before_and_redo_reapplies() {
        let file = scratch("undo").join("a.txt");
        fs::write(&file, "two").unwrap();
        let mut history = EditHistory::default();
        history.push(edit(&file, Some("one"), "two").into());

        assert_eq!(history.undo().unwrap(), Some(vec![file.clone()]));
        assert_eq!(fs::read_to_string(&file).unwrap(), "one");
        assert_eq!(history.redo().unwrap(), Some(vec![file.clone()]));
        assert_eq!(fs::read_to_string(&file).unwrap(), "two");
        assert_eq!(history.redo().unwrap(), None);
    }

    #[test]
    fn undo_of_a_created_file_removes_it_and_redo_creates_it_again() {
        let file = scratch("created").join("a.txt");
        fs::write(&file, "new").unwrap();
        let mut history = EditHistory::default();
        history.push(edit(&file, None, "new").into());

        history.undo().unwrap();
        assert!(!file.exists());
        history.redo().unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
    }

    #[test]
    fn undo_refuses_when_file_changed_outside() {
        let dir = scratch("outside");
        let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
        fs::write(&a, "a1").unwrap();
        fs::write(&b, "b1").unwrap();
        let mut history = EditHistory::default();
        history.push(edit(&a, Some("a0"), "a1").into());
        history.push(edit(&b, Some("b0"), "b1").into());
        history.push(edit(&a, Some("a1"), "a2").into());
        fs::write(&a, "someone else").unwrap();

        let error = history.undo().unwrap_err();

        assert!(matches!(error, HistoryError::ChangedOutside { .. }));
        assert_eq!(error.to_string(), "The file changed outside Celesta");
        assert_eq!(fs::read_to_string(&a).unwrap(), "someone else");
        // Older entries for `a` are gone too; `b`'s entry is kept.
        assert_eq!(history.undo().unwrap(), Some(vec![b.clone()]));
        assert_eq!(fs::read_to_string(&b).unwrap(), "b0");
        assert_eq!(history.undo().unwrap(), None);
    }

    #[test]
    fn undo_refuses_when_the_created_file_was_deleted() {
        let file = scratch("deleted").join("a.txt");
        let mut history = EditHistory::default();
        history.push(edit(&file, None, "new").into());

        assert!(matches!(
            history.undo(),
            Err(HistoryError::ChangedOutside { .. })
        ));
    }

    #[test]
    fn a_read_error_keeps_the_entry() {
        let dir = scratch("readerror");
        // Reading a folder fails with something other than "not found".
        let folder = dir.join("props.json");
        fs::create_dir(&folder).unwrap();
        let mut history = EditHistory::default();
        history.push(edit(&folder, Some("a"), "b").into());

        assert!(matches!(history.undo(), Err(HistoryError::Io { .. })));
        assert!(!history.is_busy());
        fs::remove_dir(&folder).unwrap();
        fs::write(&folder, "b").unwrap();
        assert_eq!(history.undo().unwrap(), Some(vec![folder.clone()]));
        assert_eq!(fs::read_to_string(&folder).unwrap(), "a");

        // The same goes for redo.
        fs::remove_file(&folder).unwrap();
        fs::create_dir(&folder).unwrap();
        assert!(matches!(history.redo(), Err(HistoryError::Io { .. })));
        fs::remove_dir(&folder).unwrap();
        fs::write(&folder, "a").unwrap();
        assert_eq!(history.redo().unwrap(), Some(vec![folder.clone()]));
    }

    #[test]
    fn a_cleared_history_ignores_a_step_that_was_running() {
        let file = scratch("epoch").join("a.txt");
        fs::write(&file, "b").unwrap();
        let mut history = EditHistory::default();
        history.push(edit(&file, Some("a"), "b").into());
        let pending = history.begin(Step::Undo).unwrap();
        assert!(history.begin(Step::Undo).is_none());
        history.clear();

        let result = pending.apply();

        assert!(history.settle(pending, result).is_none());
        assert!(!history.is_busy());
        assert_eq!(history.undo().unwrap(), None);
    }

    #[test]
    fn push_clears_redo() {
        let file = scratch("redo").join("a.txt");
        fs::write(&file, "b").unwrap();
        let mut history = EditHistory::default();
        history.push(edit(&file, Some("a"), "b").into());
        history.undo().unwrap();
        history.push(edit(&file, Some("a"), "c").into());

        assert_eq!(history.redo().unwrap(), None);
    }

    #[test]
    fn cap_keeps_latest_100() {
        let file = scratch("cap").join("a.txt");
        let mut history = EditHistory::default();
        for index in 0..105 {
            history.push(edit(&file, Some(&index.to_string()), &(index + 1).to_string()).into());
        }
        fs::write(&file, "105").unwrap();

        let mut undone = 0;
        while history.undo().unwrap().is_some() {
            undone += 1;
        }

        assert_eq!(undone, 100);
        assert_eq!(fs::read_to_string(&file).unwrap(), "5");
    }

    #[test]
    fn undo_after_failed_reload_restores_before() {
        // The history only looks at the file, never at reload results.
        let file = scratch("reload").join("a.txt");
        fs::write(&file, "broken").unwrap();
        let mut history = EditHistory::default();
        history.push(edit(&file, Some("fine"), "broken").into());

        history.undo().unwrap();

        assert_eq!(fs::read_to_string(&file).unwrap(), "fine");
    }

    #[test]
    fn property_commit_is_recorded() {
        let file = scratch("property").join("props.json");
        fs::write(&file, "{\n  \"a\": 1\n}\n").unwrap();

        let recorded = write_recorded(&file, |text| {
            set_property_text(&file, text, "a", Some(&json!(2))).map_err(|error| error.to_string())
        })
        .unwrap();

        let after: serde_json::Value = serde_json::from_str(&recorded.after).unwrap();
        assert_eq!(after, json!({ "a": 2 }));
        assert_eq!(recorded.before.as_deref(), Some("{\n  \"a\": 1\n}\n"));
        assert_eq!(fs::read_to_string(&file).unwrap(), recorded.after);
    }

    #[test]
    fn writing_a_missing_file_records_no_before() {
        let file = scratch("missing").join("props.json");

        let recorded = write_recorded(&file, |text| {
            assert_eq!(text, "");
            Ok("{}\n".to_owned())
        })
        .unwrap();

        assert_eq!(recorded.before, None);
        assert_eq!(fs::read_to_string(&file).unwrap(), "{}\n");
    }

    #[test]
    fn an_edit_that_changes_nothing_is_not_written() {
        let file = scratch("same").join("a.txt");
        fs::write(&file, "x").unwrap();
        // Read-only: a write would fail, so success shows nothing was written.
        set_readonly(&file, true);

        let recorded = write_recorded(&file, |text| Ok(text.to_owned())).unwrap();

        assert!(!recorded.changes());
        assert!(edit(&file, Some("x"), "y").changes());
        assert!(edit(&file, None, "").changes());
        let mut history = EditHistory::default();
        let epoch = history.begin_write().unwrap();
        assert!(history.finish_write(epoch, recorded.changes().then(|| recorded.into())));
        assert_eq!(history.undo().unwrap(), None, "no history entry");
    }

    #[test]
    fn a_failed_edit_leaves_the_file_alone() {
        let file = scratch("failed").join("a.txt");
        fs::write(&file, "x").unwrap();

        let error = write_recorded(&file, |_| Err("nope".to_owned())).unwrap_err();

        assert_eq!(error, "nope");
        assert_eq!(fs::read_to_string(&file).unwrap(), "x");
    }

    fn set_readonly(path: &Path, readonly: bool) {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(readonly);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[test]
    fn a_two_file_entry_undoes_both() {
        let dir = scratch("two");
        let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
        fs::write(&a, "y").unwrap();
        fs::write(&b, "q").unwrap();
        let mut history = EditHistory::default();
        history.push(HistoryEntry {
            files: vec![edit(&a, Some("x"), "y"), edit(&b, Some("p"), "q")],
        });

        assert_eq!(history.undo().unwrap(), Some(vec![a.clone(), b.clone()]));
        assert_eq!(fs::read_to_string(&a).unwrap(), "x");
        assert_eq!(fs::read_to_string(&b).unwrap(), "p");
        assert_eq!(history.redo().unwrap(), Some(vec![a.clone(), b.clone()]));
        assert_eq!(fs::read_to_string(&a).unwrap(), "y");
        assert_eq!(fs::read_to_string(&b).unwrap(), "q");
    }

    #[test]
    fn an_outside_change_refuses_the_whole_entry() {
        let dir = scratch("outside-two");
        let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
        fs::write(&a, "y").unwrap();
        fs::write(&b, "r").unwrap();
        let mut history = EditHistory::default();
        history.push(HistoryEntry {
            files: vec![edit(&a, Some("x"), "y"), edit(&b, Some("p"), "q")],
        });

        let error = history.undo().unwrap_err();

        assert!(matches!(error, HistoryError::ChangedOutside { path } if path == b));
        assert_eq!(fs::read_to_string(&a).unwrap(), "y");
        assert_eq!(fs::read_to_string(&b).unwrap(), "r");
        assert_eq!(history.undo().unwrap(), None, "the entry is dropped");
    }

    #[test]
    fn a_refusal_drops_older_entries_touching_any_of_its_files() {
        let dir = scratch("outside-older");
        let (a, b, c) = (dir.join("a.txt"), dir.join("b.txt"), dir.join("c.txt"));
        fs::write(&a, "a1").unwrap();
        fs::write(&b, "someone else").unwrap();
        fs::write(&c, "c1").unwrap();
        let mut history = EditHistory::default();
        history.push(edit(&c, Some("c0"), "c1").into());
        history.push(edit(&b, Some("b0"), "b1").into());
        history.push(HistoryEntry {
            files: vec![edit(&a, Some("a0"), "a1"), edit(&b, Some("b1"), "b2")],
        });

        assert!(matches!(
            history.undo(),
            Err(HistoryError::ChangedOutside { .. })
        ));
        // `b`'s older entry is gone; `c`'s is kept.
        assert_eq!(history.undo().unwrap(), Some(vec![c.clone()]));
        assert_eq!(history.undo().unwrap(), None);
    }

    #[test]
    fn a_failed_temp_write_renames_nothing() {
        let dir = scratch("temp");
        let (writable, blocked) = (dir.join("a-writable"), dir.join("b-blocked"));
        fs::create_dir(&writable).unwrap();
        fs::create_dir(&blocked).unwrap();
        let (a, b) = (writable.join("a.txt"), blocked.join("b.txt"));
        fs::write(&a, "x").unwrap();
        fs::write(&b, "p").unwrap();
        // The file itself is writable, but its temp file can't be made: a
        // directory holds its name. (A read-only directory would not stop the
        // write on Windows.)
        fs::create_dir(blocked.join(format!(".b.txt.tmp-{}", std::process::id()))).unwrap();

        let result = write_all_recorded(vec![a.clone(), b.clone()], |_| {
            Ok(vec![
                (a.clone(), "y".to_owned()),
                (b.clone(), "q".to_owned()),
            ])
        });

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&a).unwrap(), "x");
        assert_eq!(fs::read_to_string(&b).unwrap(), "p");
        let left: Vec<_> = fs::read_dir(&writable)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(left, vec!["a.txt"], "a's temp file is removed");
    }

    #[test]
    fn unchanged_files_are_not_written() {
        let dir = scratch("unchanged");
        let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
        fs::write(&a, "x").unwrap();
        fs::write(&b, "p").unwrap();
        // Read-only: a write would fail, so success shows nothing was written.
        set_readonly(&b, true);

        let files = write_all_recorded(vec![a.clone(), b.clone()], |_| {
            Ok(vec![
                (a.clone(), "y".to_owned()),
                (b.clone(), "p".to_owned()),
            ])
        })
        .unwrap();

        assert_eq!(files, vec![edit(&a, Some("x"), "y")]);
        assert_eq!(fs::read_to_string(&a).unwrap(), "y");
    }

    #[test]
    fn edit_sees_every_text() {
        let dir = scratch("texts");
        let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
        fs::write(&a, "x").unwrap();
        fs::write(&b, "p").unwrap();

        let error = write_all_recorded(vec![a.clone(), b.clone()], |texts| {
            let expected: Texts = [(a.clone(), "x".to_owned()), (b.clone(), "p".to_owned())]
                .into_iter()
                .collect();
            assert_eq!(texts, &expected);
            Err("the pair does not fit".to_owned())
        })
        .unwrap_err();

        assert_eq!(error, "the pair does not fit");
        assert_eq!(fs::read_to_string(&a).unwrap(), "x");
        assert_eq!(fs::read_to_string(&b).unwrap(), "p");
    }

    #[test]
    fn the_cap_counts_entries_not_files() {
        let dir = scratch("cap-entries");
        let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
        let mut history = EditHistory::default();
        for index in 0..101 {
            let (before, after) = (index.to_string(), (index + 1).to_string());
            history.push(HistoryEntry {
                files: vec![
                    edit(&a, Some(&before), &after),
                    edit(&b, Some(&before), &after),
                ],
            });
        }
        fs::write(&a, "101").unwrap();
        fs::write(&b, "101").unwrap();

        let mut undone = 0;
        while history.undo().unwrap().is_some() {
            undone += 1;
        }

        assert_eq!(undone, 100);
        assert_eq!(fs::read_to_string(&a).unwrap(), "1");
        assert_eq!(fs::read_to_string(&b).unwrap(), "1");
    }
}
