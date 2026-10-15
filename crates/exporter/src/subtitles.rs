//! The SRT and WebVTT files an export writes (`ExportOptions::subtitles`):
//! a React entry's `<Subtitles>` cues and `<Dialogue>` lines, or a project's
//! dialogue items, timed like the video. A path with `{lang}` is written
//! once per `<Subtitles lang>`.

use crate::error::ExportError;
use celesta_composition::{Time, TimeError};
use celesta_project::{Project, TimelineContent};
use celesta_react_bridge::ReactSubtitleDescriptor;
use std::collections::{BTreeSet, HashSet};
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

/// The placeholder a subtitle path names its language with.
pub const LANGUAGE_PLACEHOLDER: &str = "{lang}";

/// The format of a subtitle file, chosen by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubtitleFormat {
    Srt,
    Vtt,
}

impl SubtitleFormat {
    pub const ALL: [Self; 2] = [Self::Srt, Self::Vtt];

    /// The file extension.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Vtt => "vtt",
        }
    }

    /// The format whose extension `path` has, ignoring case.
    pub fn from_extension(path: &Path) -> Option<Self> {
        let extension = path.extension().and_then(OsStr::to_str)?;
        Self::ALL
            .into_iter()
            .find(|format| format.as_str().eq_ignore_ascii_case(extension))
    }
}

/// One subtitle of the written file, in seconds from the file's start.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Cue {
    pub(crate) start: f64,
    pub(crate) end: f64,
    pub(crate) text: String,
    /// Its `<Subtitles lang>`; `None` goes in every language's file.
    pub(crate) lang: Option<String>,
}

/// The exported span of the composition, in composition seconds: the file's
/// time 0 is `start`, and nothing after `start + duration` is shown.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Window {
    pub(crate) start: f64,
    pub(crate) duration: f64,
}

impl Window {
    pub(crate) fn new(start: Time, duration: Time) -> Result<Self, ExportError> {
        Ok(Self {
            start: start.as_seconds().map_err(ExportError::Time)?,
            duration: duration.as_seconds().map_err(ExportError::Time)?,
        })
    }

    /// `text` shown from `start` to `end` composition seconds, shifted to
    /// the file's clock and cut to the window; `None` outside it.
    fn cue(self, text: &str, start: f64, end: f64, lang: Option<&str>) -> Option<Cue> {
        let start = (start - self.start).max(0.0);
        let end = (end - self.start).min(self.duration);
        (end > start && !text.trim().is_empty()).then(|| Cue {
            start,
            end,
            text: text.to_owned(),
            lang: lang.map(str::to_owned),
        })
    }
}

/// The cues a React entry's frames reported, each once, ordered by start.
/// A subtitle is reported by every frame that shows it, with the same
/// numbers each time, so equal reports are one subtitle.
pub(crate) fn react_cues(reports: &[ReactSubtitleDescriptor], window: Window) -> Vec<Cue> {
    let mut seen = HashSet::new();
    let mut cues: Vec<Cue> = reports
        .iter()
        .filter(|report| {
            seen.insert((
                &report.lang,
                &report.text,
                report.start.to_bits(),
                report.end.to_bits(),
            ))
        })
        .filter_map(|report| {
            window.cue(
                &report.text,
                report.start,
                report.end,
                report.lang.as_deref(),
            )
        })
        .collect();
    cues.sort_by(|a, b| a.start.total_cmp(&b.start));
    cues
}

/// The project's dialogue items on enabled tracks, ordered by start.
pub(crate) fn project_cues(project: &Project, window: Window) -> Result<Vec<Cue>, TimeError> {
    let mut cues = Vec::new();
    for track in project
        .tracks
        .iter()
        .filter(|track| track.enabled != Some(false))
    {
        for item in track
            .items
            .iter()
            .filter(|item| item.enabled != Some(false))
        {
            if let TimelineContent::Dialogue { text, .. } = &item.content {
                let start = item.range.start.as_seconds()?;
                let end = item.range.end()?.as_seconds()?;
                cues.extend(window.cue(text, start, end, None));
            }
        }
    }
    cues.sort_by(|a, b| a.start.total_cmp(&b.start));
    Ok(cues)
}

/// The file's text. SRT cues are numbered from 1; blank lines inside a cue
/// are dropped, since they would end it, and WebVTT escapes `&`, `<`, `>`.
/// `parseSubtitles()` in `@celesta/react` reads it back.
pub(crate) fn format(cues: &[Cue], format: SubtitleFormat) -> String {
    let mut output = String::new();
    if format == SubtitleFormat::Vtt {
        output.push_str("WEBVTT\n\n");
    }
    let mut number = 0;
    for cue in cues {
        let text = cue_text(&cue.text, format);
        if text.is_empty() {
            continue;
        }
        if number > 0 {
            output.push('\n');
        }
        number += 1;
        if format == SubtitleFormat::Srt {
            let _ = writeln!(output, "{number}");
        }
        let _ = writeln!(
            output,
            "{} --> {}\n{text}",
            timestamp(cue.start, format),
            timestamp(cue.end, format)
        );
    }
    output
}

fn cue_text(text: &str, format: SubtitleFormat) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect();
    let text = lines.join("\n");
    match format {
        SubtitleFormat::Srt => text,
        SubtitleFormat::Vtt => text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;"),
    }
}

/// `HH:MM:SS,mmm` (SRT) or `HH:MM:SS.mmm` (WebVTT), rounded to the millisecond.
fn timestamp(seconds: f64, format: SubtitleFormat) -> String {
    let total = (seconds * 1000.0).round().max(0.0) as u64;
    let separator = match format {
        SubtitleFormat::Srt => ',',
        SubtitleFormat::Vtt => '.',
    };
    format!(
        "{:02}:{:02}:{:02}{separator}{:03}",
        total / 3_600_000,
        total / 60_000 % 60,
        total / 1000 % 60,
        total % 1000
    )
}

/// One subtitle file to write.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SubtitleFile {
    pub(crate) path: PathBuf,
    pub(crate) format: SubtitleFormat,
    /// The language of its tagged cues; `None` when none is tagged.
    pub(crate) lang: Option<String>,
    pub(crate) cues: Vec<Cue>,
}

fn format_of(path: &Path) -> Result<SubtitleFormat, ExportError> {
    SubtitleFormat::from_extension(path)
        .ok_or_else(|| ExportError::UnsupportedSubtitles(path.to_owned()))
}

/// `path` as text when it names its language with `{lang}`.
fn language_template(path: &Path) -> Option<&str> {
    path.to_str()
        .filter(|path| path.contains(LANGUAGE_PLACEHOLDER))
}

/// Fails before anything renders when a path's extension is neither `.srt`
/// nor `.vtt`, when a path is given twice, or, without `overwrite`, when a
/// path without `{lang}` exists. [`files`] checks the rest once the
/// languages are known.
pub(crate) fn check_outputs(paths: &[PathBuf], overwrite: bool) -> Result<(), ExportError> {
    let mut seen = HashSet::new();
    for path in paths {
        format_of(path)?;
        if !seen.insert(path) {
            return Err(ExportError::DuplicateOutput(path.clone()));
        }
        if language_template(path).is_none() && !overwrite && path.exists() {
            return Err(ExportError::OutputExists(path.clone()));
        }
    }
    Ok(())
}

/// The files `paths` name for `cues`, in order. A path with `{lang}` is one
/// file per language a cue is tagged with, the tag in place of `{lang}`,
/// holding that language's cues and the untagged ones; it needs at least
/// one tagged cue. A path without it holds every cue, which must not be in
/// two languages. Fails, without `overwrite`, when a file exists, and when
/// two files are the same.
pub(crate) fn files(
    paths: &[PathBuf],
    cues: &[Cue],
    overwrite: bool,
) -> Result<Vec<SubtitleFile>, ExportError> {
    let languages: BTreeSet<&str> = cues.iter().filter_map(|cue| cue.lang.as_deref()).collect();
    let mut files = Vec::new();
    for path in paths {
        let format = format_of(path)?;
        if let Some(template) = language_template(path) {
            if languages.is_empty() {
                return Err(ExportError::SubtitleLanguages {
                    path: path.clone(),
                    languages: Vec::new(),
                });
            }
            for &lang in &languages {
                files.push(SubtitleFile {
                    path: PathBuf::from(template.replace(LANGUAGE_PLACEHOLDER, lang)),
                    format,
                    lang: Some(lang.to_owned()),
                    cues: cues
                        .iter()
                        .filter(|cue| cue.lang.as_deref().is_none_or(|tag| tag == lang))
                        .cloned()
                        .collect(),
                });
            }
        } else {
            if languages.len() > 1 {
                return Err(ExportError::SubtitleLanguages {
                    path: path.clone(),
                    languages: languages.iter().map(|&lang| lang.to_owned()).collect(),
                });
            }
            files.push(SubtitleFile {
                path: path.clone(),
                format,
                lang: languages.first().map(|&lang| lang.to_owned()),
                cues: cues.to_vec(),
            });
        }
    }
    let mut seen = HashSet::new();
    for file in &files {
        if !seen.insert(&file.path) {
            return Err(ExportError::DuplicateOutput(file.path.clone()));
        }
        if !overwrite && file.path.exists() {
            return Err(ExportError::OutputExists(file.path.clone()));
        }
    }
    Ok(files)
}

/// Writes `file` all at once.
pub(crate) fn write(file: &SubtitleFile, overwrite: bool) -> Result<(), ExportError> {
    let SubtitleFile {
        path, format, cues, ..
    } = file;
    let (path, format) = (path.as_path(), *format);
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let io_error = |operation| move |source| ExportError::Io { operation, source };
    std::fs::create_dir_all(parent).map_err(io_error("create subtitle directory"))?;
    let mut file = tempfile::Builder::new()
        .prefix(".celesta-export-")
        .suffix(&format!(".{}", format.as_str()))
        .tempfile_in(parent)
        .map_err(io_error("create temporary subtitles"))?;
    file.write_all(self::format(cues, format).as_bytes())
        .map_err(io_error("write subtitles"))?;
    if overwrite {
        file.persist(path)
    } else {
        file.persist_noclobber(path)
    }
    .map_err(|error| {
        if error.error.kind() == io::ErrorKind::AlreadyExists {
            ExportError::OutputExists(path.to_owned())
        } else {
            ExportError::Io {
                operation: "publish subtitles",
                source: error.error,
            }
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(text: &str, start: f64, end: f64) -> ReactSubtitleDescriptor {
        ReactSubtitleDescriptor {
            text: text.to_owned(),
            start,
            end,
            lang: None,
        }
    }

    fn cue(text: &str, start: f64, end: f64) -> Cue {
        Cue {
            start,
            end,
            text: text.to_owned(),
            lang: None,
        }
    }

    fn tagged(lang: &str, mut cue: Cue) -> Cue {
        cue.lang = Some(lang.to_owned());
        cue
    }

    fn paths(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    const WHOLE: Window = Window {
        start: 0.0,
        duration: f64::INFINITY,
    };

    #[test]
    fn repeated_reports_are_one_cue_in_start_order() {
        let reports = [
            report("second", 2.0, 3.0),
            report("first", 0.5, 1.5),
            report("second", 2.0, 3.0),
            report("first", 0.5, 1.5),
            report("  ", 0.0, 1.0),
        ];
        assert_eq!(
            react_cues(&reports, WHOLE),
            [cue("first", 0.5, 1.5), cue("second", 2.0, 3.0)]
        );
    }

    #[test]
    fn a_range_shifts_cues_and_cuts_them_to_the_exported_span() {
        let reports = [
            report("before", 0.0, 1.0),
            report("across start", 1.5, 2.5),
            report("inside", 3.0, 4.0),
            report("across end", 4.5, 6.0),
            report("after", 5.0, 6.0),
        ];
        let window = Window {
            start: 2.0,
            duration: 3.0,
        };
        assert_eq!(
            react_cues(&reports, window),
            [
                cue("across start", 0.0, 0.5),
                cue("inside", 1.0, 2.0),
                cue("across end", 2.5, 3.0),
            ]
        );
    }

    #[test]
    fn project_dialogue_items_on_enabled_tracks_are_cues() {
        let item = |id: &str, start: i64, text: &str, enabled: &str| {
            format!(
                r#"{{"id": "{id}", {enabled} "range": {{"start": {{"value": {start}, "timescale": 2}}, "duration": {{"value": 1, "timescale": 1}}}},
                "content": {{"type": "dialogue", "character": "akane", "text": "{text}"}}}}"#
            )
        };
        let json = format!(
            r#"{{
                "version": 0,
                "settings": {{"width": 64, "height": 64, "frameRate": {{"numerator": 30, "denominator": 1}}, "sampleRate": 48000}},
                "assets": {{}},
                "characters": {{"akane": {{"name": "Akane"}}}},
                "tracks": [
                    {{"id": "a", "name": "A", "kind": "dialogue", "items": [{}, {}, {}]}},
                    {{"id": "b", "name": "B", "kind": "dialogue", "enabled": false, "items": [{}]}}
                ],
                "properties": {{}}
            }}"#,
            item("late", 4, "Later", ""),
            item("early", 1, "First", ""),
            item("off", 0, "Disabled item", r#""enabled": false,"#),
            item("hidden", 0, "Disabled track", ""),
        );
        let project = Project::from_json(&json).unwrap();
        let window = Window {
            start: 1.0,
            duration: 2.0,
        };
        assert_eq!(
            project_cues(&project, window).unwrap(),
            [cue("First", 0.0, 0.5), cue("Later", 1.0, 2.0)]
        );
    }

    #[test]
    fn srt_numbers_cues_and_drops_blank_lines() {
        let cues = [
            cue("Hello\n\nworld & <you>", 0.0, 1.5),
            cue("\n", 1.5, 2.0),
            cue("An hour in", 3600.0, 3600.0334),
        ];
        assert_eq!(
            format(&cues, SubtitleFormat::Srt),
            "1\n00:00:00,000 --> 00:00:01,500\nHello\nworld & <you>\n\n\
             2\n01:00:00,000 --> 01:00:00,033\nAn hour in\n"
        );
    }

    #[test]
    fn webvtt_has_a_header_and_escapes_markup() {
        let cues = [cue("a < b && c > d", 61.25, 62.0)];
        assert_eq!(
            format(&cues, SubtitleFormat::Vtt),
            "WEBVTT\n\n00:01:01.250 --> 00:01:02.000\na &lt; b &amp;&amp; c &gt; d\n"
        );
        assert_eq!(format(&[], SubtitleFormat::Vtt), "WEBVTT\n\n");
        assert_eq!(format(&[], SubtitleFormat::Srt), "");
    }

    #[test]
    fn the_extension_picks_the_format() {
        assert_eq!(
            SubtitleFormat::from_extension(Path::new("a.SRT")),
            Some(SubtitleFormat::Srt)
        );
        assert_eq!(
            SubtitleFormat::from_extension(Path::new("a.vtt")),
            Some(SubtitleFormat::Vtt)
        );
        assert_eq!(SubtitleFormat::from_extension(Path::new("a.ass")), None);
        assert!(matches!(
            check_outputs(&paths(&["a.srt", "captions.txt"]), false),
            Err(ExportError::UnsupportedSubtitles(path)) if path == Path::new("captions.txt")
        ));
        assert!(matches!(
            check_outputs(&paths(&["a.{lang}"]), false),
            Err(ExportError::UnsupportedSubtitles(_))
        ));
    }

    #[test]
    fn a_reported_language_keeps_equal_lines_apart() {
        let mut english = report("OK", 0.0, 1.0);
        english.lang = Some("en".to_owned());
        let reports = [report("OK", 0.0, 1.0), english.clone(), english];
        assert_eq!(
            react_cues(&reports, WHOLE),
            [cue("OK", 0.0, 1.0), tagged("en", cue("OK", 0.0, 1.0))]
        );
    }

    #[test]
    fn check_outputs_rejects_a_repeated_path_and_an_existing_plain_one() {
        assert!(matches!(
            check_outputs(&paths(&["a.srt", "a.vtt", "a.srt"]), true),
            Err(ExportError::DuplicateOutput(path)) if path == Path::new("a.srt")
        ));
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("film.srt");
        std::fs::write(&existing, "").unwrap();
        let template = dir.path().join("film.{lang}.srt");
        assert!(matches!(
            check_outputs(&[existing.clone()], false),
            Err(ExportError::OutputExists(_))
        ));
        check_outputs(&[existing, template], true).unwrap();
    }

    #[test]
    fn one_language_per_file_with_the_untagged_cues_in_each() {
        let cues = [
            tagged("ja", cue("こんにちは", 0.0, 1.0)),
            tagged("en", cue("Hello", 0.0, 1.0)),
            cue("♪", 1.0, 2.0),
        ];
        let files = files(
            &paths(&["out/film.{lang}.srt", "out/film.{lang}.vtt"]),
            &cues,
            false,
        )
        .unwrap();
        let summary: Vec<_> = files
            .iter()
            .map(|file| {
                let texts: Vec<_> = file.cues.iter().map(|cue| cue.text.as_str()).collect();
                (
                    file.path.to_str().unwrap(),
                    file.format,
                    file.lang.as_deref(),
                    texts,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "out/film.en.srt",
                    SubtitleFormat::Srt,
                    Some("en"),
                    vec!["Hello", "♪"]
                ),
                (
                    "out/film.ja.srt",
                    SubtitleFormat::Srt,
                    Some("ja"),
                    vec!["こんにちは", "♪"]
                ),
                (
                    "out/film.en.vtt",
                    SubtitleFormat::Vtt,
                    Some("en"),
                    vec!["Hello", "♪"]
                ),
                (
                    "out/film.ja.vtt",
                    SubtitleFormat::Vtt,
                    Some("ja"),
                    vec!["こんにちは", "♪"]
                ),
            ]
        );
    }

    #[test]
    fn a_plain_path_holds_every_cue_of_at_most_one_language() {
        let untagged = [cue("one", 0.0, 1.0)];
        let one = files(&paths(&["a.srt", "a.vtt"]), &untagged, false).unwrap();
        assert_eq!(one.len(), 2);
        assert!(
            one.iter()
                .all(|file| file.lang.is_none() && file.cues == untagged)
        );

        let english = [tagged("en", cue("one", 0.0, 1.0)), cue("two", 1.0, 2.0)];
        let [file] = &files(&paths(&["a.srt"]), &english, false).unwrap()[..] else {
            panic!("expected one file");
        };
        assert_eq!(file.lang.as_deref(), Some("en"));
        assert_eq!(file.cues, english);

        let mixed = [
            tagged("ja", cue("一", 0.0, 1.0)),
            tagged("en", cue("one", 0.0, 1.0)),
        ];
        assert!(matches!(
            files(&paths(&["a.srt"]), &mixed, false),
            Err(ExportError::SubtitleLanguages { languages, .. }) if languages == ["en", "ja"]
        ));
    }

    #[test]
    fn a_language_template_needs_a_tagged_cue() {
        assert!(matches!(
            files(&paths(&["a.{lang}.srt"]), &[cue("one", 0.0, 1.0)], false),
            Err(ExportError::SubtitleLanguages { languages, .. }) if languages.is_empty()
        ));
    }

    #[test]
    fn files_reject_a_clash_and_an_existing_language_file() {
        let cues = [tagged("en", cue("one", 0.0, 1.0))];
        assert!(matches!(
            files(&paths(&["a.{lang}.srt", "a.en.srt"]), &cues, true),
            Err(ExportError::DuplicateOutput(path)) if path == Path::new("a.en.srt")
        ));
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.en.srt"), "").unwrap();
        let template = [dir.path().join("a.{lang}.srt")];
        assert!(matches!(
            files(&template, &cues, false),
            Err(ExportError::OutputExists(path)) if path == dir.path().join("a.en.srt")
        ));
        assert_eq!(files(&template, &cues, true).unwrap().len(), 1);
    }

    #[test]
    fn write_keeps_an_existing_file_without_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let file = |cues: Vec<Cue>| SubtitleFile {
            path: dir.path().join("nested/captions.srt"),
            format: SubtitleFormat::Srt,
            lang: None,
            cues,
        };
        let path = dir.path().join("nested/captions.srt");
        write(&file(vec![cue("one", 0.0, 1.0)]), false).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "1\n00:00:00,000 --> 00:00:01,000\none\n"
        );
        assert!(matches!(
            write(&file(Vec::new()), false),
            Err(ExportError::OutputExists(_))
        ));
        write(&file(Vec::new()), true).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    }
}
