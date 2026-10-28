//! The SRT or WebVTT files an export writes (`ExportOptions::subtitles`):
//! a React entry's `<Subtitles>` cues and `<Dialogue>` lines, or a project's
//! dialogue items, timed like the video. A `{lang}` in a path writes one
//! file per language.

use crate::error::ExportError;
use crate::output;
use celesta_composition::{Time, TimeError};
use celesta_project::{Project, TimelineContent};
use celesta_react_bridge::ReactSubtitleDescriptor;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use strum::VariantArray;

/// The placeholder a subtitle path names its language with.
pub const LANG_PLACEHOLDER: &str = "{lang}";

/// The language a `{lang}` path names cues with none (BCP 47 "undetermined").
const UNDETERMINED: &str = "und";

/// The format of a subtitle file, chosen by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::IntoStaticStr, strum::VariantArray)]
#[strum(serialize_all = "lowercase")]
pub enum SubtitleFormat {
    Srt,
    Vtt,
}

impl SubtitleFormat {
    /// The file extension.
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    /// The format whose extension `path` has, ignoring case.
    pub fn from_extension(path: &Path) -> Option<Self> {
        let extension = path.extension().and_then(OsStr::to_str)?;
        Self::VARIANTS
            .iter()
            .copied()
            .find(|format| format.as_str().eq_ignore_ascii_case(extension))
    }
}

/// One subtitle of the written file, in seconds from the file's start.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Cue {
    pub(crate) start: f64,
    pub(crate) end: f64,
    pub(crate) text: String,
    /// The language it is in, when the composition says.
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
    /// The span from `start` lasting `duration`, in composition time.
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
                &report.text,
                report.start.to_bits(),
                report.end.to_bits(),
                &report.lang,
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

/// Whether `path` writes one file per language.
pub(crate) fn is_per_language(path: &Path) -> bool {
    path.to_string_lossy().contains(LANG_PLACEHOLDER)
}

/// Fails before anything renders when a path's extension is neither `.srt`
/// nor `.vtt` or, without `overwrite`, when a path without `{lang}` exists.
/// A `{lang}` path's files are only known once the cues are.
pub(crate) fn check_outputs(paths: &[PathBuf], overwrite: bool) -> Result<(), ExportError> {
    for path in paths {
        SubtitleFormat::from_extension(path)
            .ok_or_else(|| ExportError::UnsupportedSubtitles(path.clone()))?;
        if !overwrite && !is_per_language(path) && path.exists() {
            return Err(ExportError::OutputExists(path.clone()));
        }
    }
    Ok(())
}

/// One subtitle file to write: its path, language, and cues.
#[derive(Debug, PartialEq)]
pub(crate) struct File {
    pub(crate) path: PathBuf,
    pub(crate) lang: Option<String>,
    pub(crate) cues: Vec<Cue>,
}

/// The files `path` stands for. A `{lang}` path is one file per language
/// the cues are in (none for no cues), with `und` for cues without one; any
/// other path holds every cue, so their languages must agree.
pub(crate) fn files(path: &Path, cues: &[Cue]) -> Result<Vec<File>, ExportError> {
    let mut languages = BTreeMap::<Option<&str>, Vec<Cue>>::new();
    for cue in cues {
        languages
            .entry(cue.lang.as_deref())
            .or_default()
            .push(cue.clone());
    }
    if is_per_language(path) {
        let path = path.to_string_lossy();
        return Ok(languages
            .into_iter()
            .map(|(lang, cues)| File {
                path: PathBuf::from(path.replace(LANG_PLACEHOLDER, &file_language(lang))),
                lang: lang.map(str::to_owned),
                cues,
            })
            .collect());
    }
    if languages.len() > 1 {
        return Err(ExportError::MixedSubtitleLanguages {
            path: path.to_owned(),
            languages: languages
                .keys()
                .map(|lang| lang.map_or_else(|| "(none)".to_owned(), str::to_owned))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        });
    }
    let lang = languages.keys().next().copied().flatten();
    Ok(vec![File {
        path: path.to_owned(),
        lang: lang.map(str::to_owned),
        cues: cues.to_vec(),
    }])
}

/// `lang` as it goes in a file name: `und` without one, and only letters,
/// digits, `-` and `_`, so it cannot name another directory.
fn file_language(lang: Option<&str>) -> String {
    match lang.map(str::trim) {
        None | Some("") => UNDETERMINED.to_owned(),
        Some(lang) => lang
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect(),
    }
}

/// Writes `cues` to `path` as its extension says, all at once, and returns
/// the format it wrote.
pub(crate) fn write(
    path: &Path,
    cues: &[Cue],
    overwrite: bool,
) -> Result<SubtitleFormat, ExportError> {
    let format = SubtitleFormat::from_extension(path)
        .ok_or_else(|| ExportError::UnsupportedSubtitles(path.to_owned()))?;
    if !overwrite && path.exists() {
        return Err(ExportError::OutputExists(path.to_owned()));
    }
    let mut file = output::stage(
        path,
        &format!(".{}", format.as_str()),
        "create temporary subtitles",
    )?;
    file.as_file_mut()
        .write_all(self::format(cues, format).as_bytes())
        .map_err(|source| ExportError::Io {
            operation: "write subtitles",
            source,
        })?;
    output::publish(file, path, overwrite, "publish subtitles")?;
    Ok(format)
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

    fn in_lang(lang: &str, mut cue: Cue) -> Cue {
        cue.lang = Some(lang.to_owned());
        cue
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
            check_outputs(&[PathBuf::from("captions.txt")], false),
            Err(ExportError::UnsupportedSubtitles(_))
        ));
        assert!(matches!(
            check_outputs(&[PathBuf::from("captions.{lang}.txt")], false),
            Err(ExportError::UnsupportedSubtitles(_))
        ));
    }

    #[test]
    fn reports_in_different_languages_are_different_cues() {
        let mut english = report("Hi", 0.0, 1.0);
        english.lang = Some("en".into());
        let reports = [report("Hi", 0.0, 1.0), english.clone(), english];
        assert_eq!(
            react_cues(&reports, WHOLE),
            [cue("Hi", 0.0, 1.0), in_lang("en", cue("Hi", 0.0, 1.0))]
        );
    }

    #[test]
    fn a_lang_path_writes_one_file_per_language() {
        let cues = [
            in_lang("ja", cue("こんにちは", 0.0, 1.0)),
            in_lang("en", cue("Hello", 0.0, 1.0)),
            cue("Narration", 1.0, 2.0),
            in_lang("en", cue("World", 1.0, 2.0)),
            in_lang("../x", cue("Escape", 2.0, 3.0)),
        ];
        let files = files(Path::new("out/film.{lang}.vtt"), &cues).unwrap();
        let summary: Vec<_> = files
            .iter()
            .map(|file| {
                (
                    file.path.to_string_lossy().into_owned(),
                    file.lang.clone(),
                    file.cues
                        .iter()
                        .map(|cue| cue.text.as_str())
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("out/film.und.vtt".into(), None, vec!["Narration"]),
                (
                    "out/film.___x.vtt".into(),
                    Some("../x".into()),
                    vec!["Escape"]
                ),
                (
                    "out/film.en.vtt".into(),
                    Some("en".into()),
                    vec!["Hello", "World"]
                ),
                (
                    "out/film.ja.vtt".into(),
                    Some("ja".into()),
                    vec!["こんにちは"]
                ),
            ]
        );
        assert!(files_of(Path::new("film.{lang}.srt"), &[]).is_empty());
    }

    fn files_of(path: &Path, cues: &[Cue]) -> Vec<File> {
        files(path, cues).unwrap()
    }

    #[test]
    fn a_plain_path_takes_one_language() {
        let english = [
            in_lang("en", cue("Hello", 0.0, 1.0)),
            in_lang("en", cue("World", 1.0, 2.0)),
        ];
        assert_eq!(
            files_of(Path::new("film.srt"), &english),
            [File {
                path: "film.srt".into(),
                lang: Some("en".into()),
                cues: english.to_vec(),
            }]
        );
        assert_eq!(
            files_of(Path::new("film.srt"), &[]),
            [File {
                path: "film.srt".into(),
                lang: None,
                cues: Vec::new(),
            }]
        );
        let mixed = [english[0].clone(), cue("Narration", 1.0, 2.0)];
        match files(Path::new("film.srt"), &mixed) {
            Err(ExportError::MixedSubtitleLanguages { path, languages }) => {
                assert_eq!(path, Path::new("film.srt"));
                assert_eq!(languages, ["(none)", "en"]);
            }
            other => panic!("expected mixed languages, got {other:?}"),
        }
    }

    #[test]
    fn write_keeps_an_existing_file_without_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/captions.srt");
        write(&path, &[cue("one", 0.0, 1.0)], false).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "1\n00:00:00,000 --> 00:00:01,000\none\n"
        );
        assert!(matches!(
            write(&path, &[], false),
            Err(ExportError::OutputExists(_))
        ));
        write(&path, &[], true).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    }
}
