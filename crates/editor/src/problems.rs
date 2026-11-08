//! What is wrong right now, and a log of the errors, warnings, and finished
//! operations since the project opened. The Problems tab, the status bar
//! counts, and the toasts all read from here.

use chrono::{DateTime, Local};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt::Write as _;

/// The most log entries kept; the oldest are dropped first.
pub(crate) const HISTORY_LIMIT: usize = 500;

/// Where a problem comes from, in the order the Problems tab lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Source {
    Project,
    Media,
    MediaDecoders,
    Audio,
    Preview,
    Schemas,
    Export,
    TypeScript,
    Plugins,
}

impl Source {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Media => "Media",
            Self::MediaDecoders => "Project decoders",
            Self::Audio => "Audio",
            Self::Preview => "Preview",
            Self::Schemas => "Schemas",
            Self::Export => "Export",
            Self::TypeScript => "TypeScript",
            Self::Plugins => "Plugins",
        }
    }

    /// Audio, TypeScript, and plugin problems (plugins waiting or skipped,
    /// plugin settings not saved) leave the editor usable, so they are
    /// warnings; the project decoders notice is not a problem at all; the
    /// rest block what the user asked for.
    pub(crate) fn severity(self) -> Severity {
        match self {
            Self::MediaDecoders => Severity::Info,
            Self::Audio | Self::TypeScript | Self::Plugins => Severity::Warning,
            _ => Severity::Error,
        }
    }

    /// Whether this source's problems get a toast. The preview error panel
    /// already shows a preview error in place, for as long as it persists.
    fn toasts_problems(self) -> bool {
        self != Self::Preview
    }

    /// The title for a failure that has no more specific one.
    pub(crate) fn failure_title(self) -> &'static str {
        match self {
            Self::Project => "Couldn’t open project",
            Self::Media => "Media probe failed",
            Self::MediaDecoders => "Project decoders in use",
            Self::Audio => "Audio unavailable",
            Self::Preview => "Preview unavailable",
            Self::Schemas => "Couldn’t load component schemas",
            Self::Export => "Export failed",
            Self::TypeScript => "TypeScript setup failed",
            Self::Plugins => "Some plugins were skipped",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    Error,
    Warning,
    /// Something to tell the user that is not a problem, such as a plugin's
    /// note on a model's terms of use.
    Info,
    Done,
}

impl Severity {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::Warning => "Warning",
            Self::Info => "Info",
            Self::Done => "Done",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Problem {
    /// What happened: `Export failed`.
    pub(crate) title: String,
    /// The whole message, with its causes.
    pub(crate) detail: String,
}

impl Problem {
    pub(crate) fn new(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ActiveProblem {
    /// When this problem was first set.
    pub(crate) time: DateTime<Local>,
    pub(crate) problem: Problem,
}

#[derive(Clone, Debug)]
pub(crate) struct LogEntry {
    /// Unique within a `Problems`, increasing; keys the interface's rows.
    pub(crate) id: u64,
    pub(crate) time: DateTime<Local>,
    pub(crate) severity: Severity,
    pub(crate) source: Source,
    pub(crate) problem: Problem,
    /// Whether announcing this entry shows a toast. Preview errors and
    /// cancelled exports do not.
    pub(crate) toast: bool,
}

impl LogEntry {
    /// `12:03:41 Error · Export · Export failed`, then the detail, if any.
    pub(crate) fn to_text(&self) -> String {
        let heading = format!(
            "{} {} · {} · {}",
            self.time.format("%H:%M:%S"),
            self.severity.label(),
            self.source.label(),
            self.problem.title
        );
        if self.problem.detail.is_empty() {
            heading
        } else {
            format!("{heading}\n{}", self.problem.detail)
        }
    }
}

#[derive(Default)]
pub(crate) struct Problems {
    active: BTreeMap<Source, ActiveProblem>,
    preview_warnings: Vec<String>,
    history: VecDeque<LogEntry>,
    /// How many of the newest history entries have not been announced.
    unannounced: usize,
    next_id: u64,
}

impl Problems {
    /// Replaces `source`'s active problem. A problem that differs from the
    /// current one is logged; setting the same one again is not. A preview
    /// problem is logged without a toast.
    pub(crate) fn set(&mut self, source: Source, problem: Option<Problem>) {
        let Some(problem) = problem else {
            self.active.remove(&source);
            return;
        };
        if self
            .active
            .get(&source)
            .is_some_and(|active| active.problem == problem)
        {
            return;
        }
        let time = Local::now();
        self.push(
            time,
            source.severity(),
            source,
            problem.clone(),
            source.toasts_problems(),
        );
        self.active.insert(source, ActiveProblem { time, problem });
    }

    pub(crate) fn report(
        &mut self,
        source: Source,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.set(source, Some(Problem::new(title, detail)));
    }

    /// Reports a failure under `source`'s default title.
    pub(crate) fn fail(&mut self, source: Source, detail: impl Into<String>) {
        self.report(source, source.failure_title(), detail);
    }

    pub(crate) fn clear(&mut self, source: Source) {
        self.set(source, None);
    }

    /// Logs a finished operation, such as an export.
    pub(crate) fn info(
        &mut self,
        source: Source,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.push(
            Local::now(),
            Severity::Done,
            source,
            Problem::new(title, detail),
            true,
        );
    }

    /// Logs a finished operation the user already sees ended, such as a
    /// cancelled export, without a toast.
    pub(crate) fn info_without_toast(
        &mut self,
        source: Source,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.push(
            Local::now(),
            Severity::Done,
            source,
            Problem::new(title, detail),
            false,
        );
    }

    /// Logs what an export reported, if anything, and empties `notices`
    /// for the next export. It is logged without a toast: the export's own
    /// entry, logged next, announces how it ended.
    pub(crate) fn log_export_notices(&mut self, notices: &mut ExportNotices) {
        if let Some((severity, problem)) = notices.take() {
            self.push(Local::now(), severity, Source::Export, problem, false);
        }
    }

    /// Replaces the preview's warnings. They are recomputed on every preview
    /// reload, so they are counted but not logged.
    pub(crate) fn set_preview_warnings(&mut self, warnings: Vec<String>) {
        self.preview_warnings = warnings;
    }

    pub(crate) fn active(&self, source: Source) -> Option<&ActiveProblem> {
        self.active.get(&source)
    }

    pub(crate) fn active_problems(&self) -> impl Iterator<Item = (Source, &ActiveProblem)> {
        self.active.iter().map(|(source, active)| (*source, active))
    }

    pub(crate) fn preview_warnings(&self) -> &[String] {
        &self.preview_warnings
    }

    /// Log entries, oldest first.
    pub(crate) fn history(&self) -> impl DoubleEndedIterator<Item = &LogEntry> {
        self.history.iter()
    }

    /// The whole log as plain text, oldest first, one block per entry.
    pub(crate) fn history_text(&self) -> String {
        self.history
            .iter()
            .map(LogEntry::to_text)
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub(crate) fn clear_history(&mut self) {
        self.history.clear();
        self.unannounced = 0;
    }

    /// The entries logged since the last call that get a toast, oldest
    /// first.
    pub(crate) fn take_unannounced(&mut self) -> Vec<LogEntry> {
        let start = self.history.len() - self.unannounced;
        self.unannounced = 0;
        self.history
            .range(start..)
            .filter(|entry| entry.toast)
            .cloned()
            .collect()
    }

    pub(crate) fn error_count(&self) -> usize {
        self.count(Severity::Error)
    }

    pub(crate) fn warning_count(&self) -> usize {
        self.count(Severity::Warning) + self.preview_warnings.len()
    }

    fn count(&self, severity: Severity) -> usize {
        self.active
            .keys()
            .filter(|source| source.severity() == severity)
            .count()
    }

    fn push(
        &mut self,
        time: DateTime<Local>,
        severity: Severity,
        source: Source,
        problem: Problem,
        toast: bool,
    ) {
        self.history.push_back(LogEntry {
            id: self.next_id,
            time,
            severity,
            source,
            problem,
            toast,
        });
        self.next_id += 1;
        self.unannounced += 1;
        while self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
        self.unannounced = self.unannounced.min(self.history.len());
    }
}

/// The warnings and notes one export reported, in the order they arrived,
/// each once. The exporter reports each once per export already; a notice
/// repeated on many frames still appears once here.
#[derive(Debug, Default)]
pub(crate) struct ExportNotices {
    entries: Vec<ExportNotice>,
    /// `entries`, for finding a repeat without scanning them.
    seen: HashSet<ExportNotice>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ExportNotice {
    Warning(String),
    /// Says what it is (`note: …`), as the exporter prints it.
    Note(String),
}

impl ExportNotice {
    /// The line the exporter prints for it.
    fn line(&self) -> Cow<'_, str> {
        match self {
            Self::Warning(message) => Cow::Owned(format!("warning: {message}")),
            Self::Note(message) => Cow::Borrowed(message),
        }
    }
}

impl ExportNotices {
    pub(crate) fn warning(&mut self, message: String) {
        self.add(ExportNotice::Warning(message));
    }

    pub(crate) fn note(&mut self, message: String) {
        self.add(ExportNotice::Note(message));
    }

    fn add(&mut self, notice: ExportNotice) {
        if self.seen.insert(notice.clone()) {
            self.entries.push(notice);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.seen.clear();
    }

    /// One log entry for everything reported, oldest first, one line each,
    /// leaving this empty: a warning if any was reported, information if
    /// only notes were.
    fn take(&mut self) -> Option<(Severity, Problem)> {
        if self.entries.is_empty() {
            return None;
        }
        let warnings = self
            .entries
            .iter()
            .filter(|notice| matches!(notice, ExportNotice::Warning(_)))
            .count();
        let notes = self.entries.len() - warnings;
        let counted = |count: usize, noun: &str| {
            format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
        };
        let (severity, title) = match (warnings, notes) {
            (0, notes) => (
                Severity::Info,
                format!("Export reported {}", counted(notes, "note")),
            ),
            (warnings, 0) => (
                Severity::Warning,
                format!("Export reported {}", counted(warnings, "warning")),
            ),
            (warnings, notes) => (
                Severity::Warning,
                format!(
                    "Export reported {} and {}",
                    counted(warnings, "warning"),
                    counted(notes, "note")
                ),
            ),
        };
        self.seen.clear();
        let detail = self
            .entries
            .drain(..)
            .map(|notice| notice.line().into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        Some((severity, Problem::new(title, detail)))
    }
}

/// `detail` as a Markdown code block that shows it verbatim: the fence is
/// longer than any run of backticks inside it.
pub(crate) fn fenced(detail: &str) -> String {
    let longest = detail
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}text\n{detail}\n{fence}")
}

/// The first line of `detail`, for a toast.
pub(crate) fn first_line(detail: &str) -> &str {
    detail.lines().next().unwrap_or_default()
}

/// `text` as one flowing paragraph, for a summary that wraps it to the
/// available width: each line break, with the whitespace around it, becomes
/// one space, and the ends are trimmed. Text without a line break is
/// borrowed, not copied.
pub(crate) fn flow_line_breaks(text: &str) -> Cow<'_, str> {
    if !text.contains(['\n', '\r']) {
        return Cow::Borrowed(text.trim());
    }
    let mut flowed = String::with_capacity(text.len());
    for line in text.split(['\n', '\r']).map(str::trim) {
        if line.is_empty() {
            continue;
        }
        if !flowed.is_empty() {
            flowed.push(' ');
        }
        flowed.push_str(line);
    }
    Cow::Owned(flowed)
}

/// How many preview warnings a summary lists before folding the rest into
/// one "N more" line.
pub(crate) const SUMMARY_WARNING_LIMIT: usize = 3;

/// What a summary of the preview warnings shows. Each warning is flowed
/// into two lines at most, so the summary can always cut something off and
/// always points to Problems for the full text.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct WarningsSummary<'a> {
    /// The warnings listed.
    pub(crate) shown: &'a [String],
    /// How many warnings the summary leaves out.
    pub(crate) hidden: usize,
}

pub(crate) fn summarize_warnings(warnings: &[String]) -> WarningsSummary<'_> {
    let shown = &warnings[..warnings.len().min(SUMMARY_WARNING_LIMIT)];
    WarningsSummary {
        shown,
        hidden: warnings.len() - shown.len(),
    }
}

/// `error` and the errors that caused it, outermost first, one per line.
/// A cause whose text the message already contains is left out.
pub(crate) fn error_chain(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut cause = error.source();
    while let Some(error) = cause {
        let message = error.to_string();
        if !text.contains(&message) {
            write!(text, "\ncaused by: {message}").expect("writing to a String cannot fail");
        }
        cause = error.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;

    fn problem(title: &str, detail: &str) -> Problem {
        Problem::new(title, detail)
    }

    fn warnings(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|text| (*text).to_owned()).collect()
    }

    #[test]
    fn flowing_joins_lines_with_one_space_and_drops_indentation_and_blank_lines() {
        let text = "React preview unavailable: exited\nIts last output:\n\n    Error: Cannot find module '@celesta.studio/react'\n      at require  \n";
        assert_eq!(
            flow_line_breaks(text),
            "React preview unavailable: exited Its last output: Error: Cannot find module '@celesta.studio/react' at require"
        );
    }

    #[test]
    fn flowing_treats_crlf_as_a_line_break() {
        assert_eq!(flow_line_breaks("first\r\nsecond\r\n"), "first second");
    }

    #[test]
    fn flowing_leaves_a_single_line_as_it_is_without_copying() {
        let flowed = flow_line_breaks("font fallback: using  Noto Sans");
        assert_eq!(flowed, "font fallback: using  Noto Sans");
        assert!(matches!(flowed, Cow::Borrowed(_)));
    }

    #[test]
    fn flowing_an_empty_string_gives_an_empty_string() {
        assert_eq!(flow_line_breaks(""), "");
        assert_eq!(flow_line_breaks("\n  \r\n"), "");
    }

    #[test]
    fn a_few_one_line_warnings_are_summarized_in_full() {
        let warnings = warnings(&["a", "b", "c"]);
        let summary = summarize_warnings(&warnings);
        assert_eq!(summary.shown, &warnings[..]);
        assert_eq!(summary.hidden, 0);
    }

    #[test]
    fn no_warnings_summarize_to_nothing() {
        let summary = summarize_warnings(&[]);
        assert!(summary.shown.is_empty());
        assert_eq!(summary.hidden, 0);
    }

    #[test]
    fn warnings_past_the_limit_are_counted() {
        let warnings = warnings(&["a", "b", "c", "d", "e"]);
        let summary = summarize_warnings(&warnings);
        assert_eq!(summary.shown, &warnings[..3]);
        assert_eq!(summary.hidden, 2);
    }

    #[test]
    fn setting_a_new_problem_logs_one_entry() {
        let mut problems = Problems::default();
        problems.set(Source::Export, Some(problem("Export failed", "disk full")));
        assert_eq!(problems.history().count(), 1);
        let entry = problems.history().next().unwrap();
        assert_eq!(entry.severity, Severity::Error);
        assert_eq!(entry.source, Source::Export);
        assert_eq!(entry.problem, problem("Export failed", "disk full"));
        assert_eq!(
            problems
                .active(Source::Export)
                .map(|active| &active.problem),
            Some(&problem("Export failed", "disk full"))
        );
    }

    #[test]
    fn setting_the_same_problem_again_logs_nothing() {
        let mut problems = Problems::default();
        for _ in 0..3 {
            problems.fail(Source::Export, "disk full");
        }
        assert_eq!(problems.history().count(), 1);
        assert_eq!(problems.take_unannounced().len(), 1);
    }

    #[test]
    fn changing_a_problem_logs_another_entry() {
        let mut problems = Problems::default();
        problems.fail(Source::Preview, "first");
        problems.fail(Source::Preview, "second");
        let details: Vec<&str> = problems
            .history()
            .map(|entry| entry.problem.detail.as_str())
            .collect();
        assert_eq!(details, ["first", "second"]);
    }

    #[test]
    fn clearing_keeps_the_history_entry() {
        let mut problems = Problems::default();
        problems.fail(Source::Audio, "no output device");
        problems.clear(Source::Audio);
        assert!(problems.active(Source::Audio).is_none());
        assert_eq!(problems.history().count(), 1);
        assert_eq!(
            problems.history().next().unwrap().severity,
            Severity::Warning
        );
    }

    #[test]
    fn a_problem_that_recurs_after_clearing_is_logged_again() {
        let mut problems = Problems::default();
        problems.fail(Source::Audio, "no output device");
        problems.clear(Source::Audio);
        problems.fail(Source::Audio, "no output device");
        assert_eq!(problems.history().count(), 2);
    }

    #[test]
    fn info_logs_a_done_entry_without_active_state() {
        let mut problems = Problems::default();
        problems.info(Source::Export, "Export finished", "/tmp/out.mp4");
        assert!(problems.active(Source::Export).is_none());
        let entry = problems.history().next().unwrap();
        assert_eq!(entry.severity, Severity::Done);
        assert_eq!(entry.problem, problem("Export finished", "/tmp/out.mp4"));
    }

    #[test]
    fn counts_errors_and_warnings() {
        let mut problems = Problems::default();
        problems.fail(Source::Export, "disk full");
        problems.fail(Source::Preview, "node exited");
        problems.fail(Source::Audio, "no output device");
        problems.set_preview_warnings(vec!["font fallback".to_owned(), "missing glyph".to_owned()]);
        problems.info(Source::TypeScript, "TypeScript set up in film", "");
        assert_eq!(problems.error_count(), 2);
        assert_eq!(problems.warning_count(), 3);
    }

    #[test]
    fn preview_warnings_are_not_logged() {
        let mut problems = Problems::default();
        problems.set_preview_warnings(vec!["font fallback".to_owned()]);
        assert_eq!(problems.preview_warnings(), ["font fallback".to_owned()]);
        assert_eq!(problems.history().count(), 0);
    }

    #[test]
    fn export_notices_are_logged_once_in_arrival_order() {
        let mut problems = Problems::default();
        let mut notices = ExportNotices::default();
        notices.warning("fallback font".to_owned());
        notices.note("note: flag (layer hero): terms of use".to_owned());
        notices.warning("missing glyph".to_owned());
        notices.note("note: flag (layer hero): terms of use".to_owned());
        notices.warning("fallback font".to_owned());
        problems.log_export_notices(&mut notices);
        assert_eq!(problems.history().count(), 1);
        let entry = problems.history().next().unwrap();
        assert_eq!(entry.severity, Severity::Warning);
        assert_eq!(entry.source, Source::Export);
        assert_eq!(
            entry.problem,
            problem(
                "Export reported 2 warnings and 1 note",
                "warning: fallback font\nnote: flag (layer hero): terms of use\nwarning: missing glyph"
            )
        );
        assert!(problems.active(Source::Export).is_none());
        assert_eq!(problems.warning_count(), 0);
        assert!(problems.take_unannounced().is_empty());
    }

    #[test]
    fn export_notes_alone_are_logged_as_information() {
        let mut problems = Problems::default();
        let mut notices = ExportNotices::default();
        notices.note("note: flag (layer hero): terms of use".to_owned());
        problems.log_export_notices(&mut notices);
        let entry = problems.history().next().unwrap();
        assert_eq!(entry.severity, Severity::Info);
        assert_eq!(entry.problem.title, "Export reported 1 note");
    }

    #[test]
    fn export_notices_are_emptied_once_logged() {
        let mut problems = Problems::default();
        let mut notices = ExportNotices::default();
        notices.warning("fallback font".to_owned());
        problems.log_export_notices(&mut notices);
        problems.log_export_notices(&mut notices);
        assert_eq!(problems.history().count(), 1);
        assert_eq!(
            problems.history().next().unwrap().problem.title,
            "Export reported 1 warning"
        );
        notices.warning("fallback font".to_owned());
        problems.log_export_notices(&mut notices);
        assert_eq!(problems.history().count(), 2);
    }

    #[test]
    fn an_export_without_notices_logs_nothing() {
        let mut problems = Problems::default();
        problems.log_export_notices(&mut ExportNotices::default());
        assert_eq!(problems.history().count(), 0);
    }

    #[test]
    fn history_is_capped_and_drops_the_oldest() {
        let mut problems = Problems::default();
        for index in 0..HISTORY_LIMIT + 10 {
            problems.info(Source::Export, "Export finished", index.to_string());
        }
        assert_eq!(problems.history().count(), HISTORY_LIMIT);
        assert_eq!(problems.history().next().unwrap().problem.detail, "10");
    }

    #[test]
    fn trimming_keeps_unannounced_within_history() {
        let mut problems = Problems::default();
        for index in 0..HISTORY_LIMIT + 10 {
            problems.info(Source::Export, "Export finished", index.to_string());
        }
        let announced = problems.take_unannounced();
        assert_eq!(announced.len(), HISTORY_LIMIT);
        assert_eq!(announced.first().unwrap().problem.detail, "10");
    }

    #[test]
    fn take_unannounced_returns_each_entry_once() {
        let mut problems = Problems::default();
        problems.fail(Source::Export, "disk full");
        problems.info(Source::TypeScript, "TypeScript set up in film", "");
        let first: Vec<Source> = problems
            .take_unannounced()
            .iter()
            .map(|entry| entry.source)
            .collect();
        assert_eq!(first, [Source::Export, Source::TypeScript]);
        assert!(problems.take_unannounced().is_empty());
        problems.fail(Source::Project, "bad json");
        assert_eq!(problems.take_unannounced().len(), 1);
    }

    #[test]
    fn a_preview_error_gets_no_toast() {
        let mut problems = Problems::default();
        problems.fail(Source::Preview, "node exited");
        assert!(problems.take_unannounced().is_empty());
    }

    #[test]
    fn an_export_error_gets_a_toast() {
        let mut problems = Problems::default();
        problems.fail(Source::Export, "disk full");
        let announced = problems.take_unannounced();
        assert_eq!(announced.len(), 1);
        assert_eq!(announced[0].source, Source::Export);
    }

    #[test]
    fn a_preview_error_is_still_logged_and_counted() {
        let mut problems = Problems::default();
        problems.fail(Source::Preview, "node exited");
        assert_eq!(problems.history().count(), 1);
        assert_eq!(problems.history().next().unwrap().severity, Severity::Error);
        assert!(problems.active(Source::Preview).is_some());
        assert_eq!(problems.error_count(), 1);
    }

    #[test]
    fn info_without_toast_is_logged_but_not_announced() {
        let mut problems = Problems::default();
        problems.fail(Source::Export, "disk full");
        problems.info_without_toast(Source::Export, "Export cancelled", "");
        problems.info(Source::TypeScript, "TypeScript set up in film", "");
        let logged: Vec<&str> = problems
            .history()
            .map(|entry| entry.problem.title.as_str())
            .collect();
        assert_eq!(
            logged,
            [
                "Export failed",
                "Export cancelled",
                "TypeScript set up in film"
            ]
        );
        let announced: Vec<String> = problems
            .take_unannounced()
            .into_iter()
            .map(|entry| entry.problem.title)
            .collect();
        assert_eq!(announced, ["Export failed", "TypeScript set up in film"]);
        assert_eq!(problems.history().next().unwrap().severity, Severity::Error);
        assert_eq!(problems.history().nth(1).unwrap().severity, Severity::Done);
    }

    #[test]
    fn clear_history_empties_the_log_but_not_active_problems() {
        let mut problems = Problems::default();
        problems.fail(Source::Export, "disk full");
        problems.clear_history();
        assert_eq!(problems.history().count(), 0);
        assert!(problems.take_unannounced().is_empty());
        assert!(problems.active(Source::Export).is_some());
    }

    #[test]
    fn entry_ids_are_unique_and_increasing() {
        let mut problems = Problems::default();
        problems.fail(Source::Export, "a");
        problems.fail(Source::Export, "b");
        let ids: Vec<u64> = problems.history().map(|entry| entry.id).collect();
        assert!(ids[0] < ids[1]);
    }

    #[test]
    fn active_problems_are_listed_in_source_order() {
        let mut problems = Problems::default();
        problems.fail(Source::Export, "disk full");
        problems.fail(Source::Project, "bad json");
        let sources: Vec<Source> = problems
            .active_problems()
            .map(|(source, _)| source)
            .collect();
        assert_eq!(sources, [Source::Project, Source::Export]);
    }

    #[test]
    fn log_entry_text_names_time_severity_source_title_and_detail() {
        let entry = LogEntry {
            id: 0,
            time: Local.with_ymd_and_hms(2026, 10, 14, 12, 3, 41).unwrap(),
            severity: Severity::Error,
            source: Source::Export,
            problem: problem("Export failed", "ffmpeg exited\ncaused by: disk full"),
            toast: true,
        };
        assert_eq!(
            entry.to_text(),
            "12:03:41 Error · Export · Export failed\nffmpeg exited\ncaused by: disk full"
        );
    }

    #[test]
    fn history_text_is_chronological_and_blank_line_separated() {
        let mut problems = Problems::default();
        problems.fail(Source::Export, "disk full");
        problems.info(Source::Export, "Export finished", "");
        let text = problems.history_text();
        let blocks: Vec<&str> = text.split("\n\n").collect();
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("Export failed"));
        assert!(blocks[1].ends_with("Done · Export · Export finished"));
    }

    #[test]
    fn fenced_detail_uses_a_longer_fence_than_the_detail() {
        assert_eq!(fenced("plain"), "```text\nplain\n```");
        assert_eq!(
            fenced("before\n```js\nthrow 1\n```\nafter"),
            "````text\nbefore\n```js\nthrow 1\n```\nafter\n````"
        );
    }

    #[test]
    fn first_line_of_detail() {
        assert_eq!(first_line("one\ntwo"), "one");
        assert_eq!(first_line(""), "");
    }

    #[test]
    fn sources_have_labels_severities_and_failure_titles() {
        assert_eq!(Source::Schemas.label(), "Schemas");
        assert_eq!(Source::TypeScript.severity(), Severity::Warning);
        assert_eq!(Source::Audio.severity(), Severity::Warning);
        assert_eq!(Source::Project.severity(), Severity::Error);
        assert_eq!(Source::Export.failure_title(), "Export failed");
    }

    #[derive(Debug)]
    struct Outer {
        message: &'static str,
        cause: Option<Inner>,
    }

    #[derive(Debug)]
    struct Inner(&'static str);

    impl std::fmt::Display for Outer {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.message)
        }
    }

    impl std::fmt::Display for Inner {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.0)
        }
    }

    impl std::error::Error for Outer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.cause.as_ref().map(|cause| cause as _)
        }
    }

    impl std::error::Error for Inner {}

    #[test]
    fn error_chain_lists_causes_one_per_line() {
        let error = Outer {
            message: "export failed",
            cause: Some(Inner("disk full")),
        };
        assert_eq!(error_chain(&error), "export failed\ncaused by: disk full");
    }

    #[test]
    fn error_chain_skips_causes_already_in_the_message() {
        let error = Outer {
            message: "could not run node: No such file or directory",
            cause: Some(Inner("No such file or directory")),
        };
        assert_eq!(
            error_chain(&error),
            "could not run node: No such file or directory"
        );
    }

    #[test]
    fn error_chain_without_causes_is_the_message() {
        let error = Outer {
            message: "bad json",
            cause: None,
        };
        assert_eq!(error_chain(&error), "bad json");
    }
}
