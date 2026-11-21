//! Review comments: notes a viewer leaves on a video, anchored to what the
//! composition names (a chapter id, a layer id) rather than to source
//! offsets, so they survive code edits.
//!
//! Each thread is one JSON file, `<project root>/celesta-comments/<id>.json`,
//! so two people adding comments never touch the same file in git. A
//! thread's time is in frames from its chapter's first frame (or from frame
//! 0 without a chapter); when the chapter id is gone the comment is kept and
//! reported as orphaned, with the frame it was last placed on.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[cfg(test)]
mod tests;

/// The folder, in a project's root, that holds one file per thread.
pub const COMMENTS_DIR: &str = "celesta-comments";

/// The format version this crate writes, and the newest it reads.
pub const FORMAT_VERSION: u32 = 1;

/// One review thread: the first comment, its anchor, and its replies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub version: u32,
    /// The file name without `.json`: lowercase letters, digits, and `-`.
    pub id: String,
    /// The entry the comment is on, relative to the project root with `/`
    /// separators (`film.tsx`, `scenes/intro.tsx`).
    pub entry: String,
    /// The root composition, when the entry declares several. Left out, the
    /// comment shows on whichever composition the entry opens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition: Option<String>,
    pub anchor: Anchor,
    /// Where the anchor resolved the last time someone acted on the
    /// thread; what an orphaned comment still points at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<LastSeen>,
    pub author: String,
    /// RFC 3339, UTC (`2026-11-22T10:00:00Z`).
    pub created_at: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<Resolution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replies: Vec<Reply>,
    /// Fields a later version added, kept as they were when the file is
    /// written again.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// What a comment points at. Only `from` is required.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    /// The chapter (`<Composition chapters>`, a `defineScenes` id) the
    /// frames count from. Left out, they count from the composition's start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter: Option<String>,
    /// The first frame, from the chapter's first frame.
    pub from: u64,
    /// The frame after the last, for a span; left out for one moment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<u64>,
    /// The layer the comment is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// The spot on the canvas, as fractions of its width and height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<Point>,
}

impl Anchor {
    /// Where the anchor falls among `chapters`; an orphan has no last place
    /// here (see [`Comment::place`]).
    pub fn place(&self, chapters: &[ChapterSpan<'_>]) -> Placement {
        let (start, chapter_name) = match &self.chapter {
            None => (0, None),
            Some(id) => match chapters.iter().find(|chapter| chapter.id == id) {
                Some(chapter) => (chapter.from, Some(chapter.name.to_owned())),
                None => return Placement::Orphaned { last_seen: None },
            },
        };
        Placement::Placed {
            from: start + self.from,
            to: self.to.map(|to| start + to),
            chapter_name,
        }
    }
}

/// A spot on the canvas: 0 is the left or top edge, 1 the right or bottom.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastSeen {
    /// The composition frame the anchor's `from` was on.
    pub frame: u64,
    /// The chapter's name then, to say which scene an orphan belonged to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolution {
    pub by: String,
    pub at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub id: String,
    pub author: String,
    pub created_at: String,
    pub body: String,
}

/// A chapter of the composition as it is now, to place comments against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChapterSpan<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub from: u64,
}

/// Where a comment falls in the composition as it is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    /// In composition frames: `to` is the frame after the last of a span.
    Placed {
        from: u64,
        to: Option<u64>,
        chapter_name: Option<String>,
    },
    /// Its chapter id is no longer declared. `last_seen` is where it was.
    Orphaned { last_seen: Option<LastSeen> },
}

impl Placement {
    /// The frame to seek to: the anchor's first frame, or an orphan's last.
    pub fn frame(&self) -> Option<u64> {
        match self {
            Self::Placed { from, .. } => Some(*from),
            Self::Orphaned { last_seen } => last_seen.as_ref().map(|seen| seen.frame),
        }
    }

    pub fn is_orphaned(&self) -> bool {
        matches!(self, Self::Orphaned { .. })
    }

    /// Whether `frame` is inside the span, or is the moment itself.
    pub fn covers(&self, frame: u64) -> bool {
        match self {
            Self::Placed { from, to, .. } => match to {
                Some(to) => *from <= frame && frame < *to,
                None => *from == frame,
            },
            Self::Orphaned { .. } => false,
        }
    }
}

impl Comment {
    /// A new open thread by `author`, with a fresh id and the current time.
    pub fn new(entry: String, anchor: Anchor, author: String, body: String) -> Self {
        Self {
            version: FORMAT_VERSION,
            id: new_id("c"),
            entry,
            composition: None,
            anchor,
            last_seen: None,
            author,
            created_at: now_utc(),
            body,
            resolved: None,
            replies: Vec::new(),
            extra: Map::new(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.resolved.is_none()
    }

    /// Whether the comment belongs on `entry`'s composition `composition`.
    /// One without a composition shows on every composition of its entry.
    pub fn is_on(&self, entry: &str, composition: Option<&str>) -> bool {
        self.entry == entry
            && match (&self.composition, composition) {
                (Some(own), Some(shown)) => own == shown,
                _ => true,
            }
    }

    /// Where the anchor falls among `chapters`, the composition's chapters
    /// now.
    pub fn place(&self, chapters: &[ChapterSpan<'_>]) -> Placement {
        match self.anchor.place(chapters) {
            Placement::Orphaned { .. } => Placement::Orphaned {
                last_seen: self.last_seen.clone(),
            },
            placed => placed,
        }
    }

    /// Records where the anchor is now, so an orphan keeps a place to show.
    /// An orphan keeps the place it had.
    pub fn remember(&mut self, chapters: &[ChapterSpan<'_>]) {
        if let Placement::Placed {
            from, chapter_name, ..
        } = self.place(chapters)
        {
            self.last_seen = Some(LastSeen {
                frame: from,
                chapter_name,
            });
        }
    }

    /// Adds a reply by `author` and returns it.
    pub fn reply(&mut self, author: String, body: String) -> &Reply {
        self.replies.push(Reply {
            id: new_id("r"),
            author,
            created_at: now_utc(),
            body,
        });
        self.replies.last().expect("just pushed")
    }

    /// Marks the thread resolved by `by`, now. Resolving it again keeps the
    /// first resolution.
    pub fn resolve(&mut self, by: String) {
        if self.resolved.is_none() {
            self.resolved = Some(Resolution { by, at: now_utc() });
        }
    }

    pub fn reopen(&mut self) {
        self.resolved = None;
    }

    /// Checks what serde cannot: the version, the id, and the anchor's
    /// ranges.
    pub fn validate(&self) -> Result<(), String> {
        if self.version == 0 || self.version > FORMAT_VERSION {
            return Err(format!(
                "format version {} is not one this Celesta reads (it reads {FORMAT_VERSION}); update Celesta",
                self.version
            ));
        }
        check_id(&self.id)?;
        if self.entry.is_empty() {
            return Err("entry is empty".to_owned());
        }
        if let Some(to) = self.anchor.to
            && to <= self.anchor.from
        {
            return Err(format!(
                "anchor.to ({to}) must be after anchor.from ({})",
                self.anchor.from
            ));
        }
        if let Some(point) = self.anchor.point
            && !((0.0..=1.0).contains(&point.x) && (0.0..=1.0).contains(&point.y))
        {
            return Err(format!(
                "anchor.point ({}, {}) must be within 0 and 1",
                point.x, point.y
            ));
        }
        Ok(())
    }
}

/// Why a comment file could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{}: {message}", path.display())]
    Invalid { path: PathBuf, message: String },
    #[error("no comment \"{0}\" in this project")]
    NotFound(String),
    #[error("\"{0}\" is not a comment id (lowercase letters, digits, and -)")]
    BadId(String),
}

/// A comment file that could not be read, among others that could.
#[derive(Debug)]
pub struct LoadProblem {
    pub path: PathBuf,
    pub message: String,
}

/// Every comment of a project, and the files that failed.
#[derive(Debug, Default)]
pub struct Loaded {
    /// Oldest first; ties by id.
    pub comments: Vec<Comment>,
    pub problems: Vec<LoadProblem>,
}

/// The comment files of one project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentStore {
    root: PathBuf,
}

impl CommentStore {
    /// The store of the project whose root folder is `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn dir(&self) -> PathBuf {
        self.root.join(COMMENTS_DIR)
    }

    /// `entry` as comments name it: relative to the root, with `/`. An
    /// entry outside the root is named by its file name.
    pub fn entry_key(&self, entry: &Path) -> String {
        let entry = std::path::absolute(entry).unwrap_or_else(|_| entry.to_path_buf());
        let root = std::path::absolute(&self.root).unwrap_or_else(|_| self.root.clone());
        match entry.strip_prefix(&root) {
            Ok(relative) => relative
                .components()
                .filter_map(|part| match part {
                    Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/"),
            Err(_) => entry
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }

    /// Every comment file. No folder is no comments; a file that does not
    /// parse is a [`LoadProblem`] and the others still load.
    pub fn load(&self) -> Loaded {
        let mut loaded = Loaded::default();
        let dir = self.dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return loaded,
            Err(error) => {
                loaded.problems.push(LoadProblem {
                    path: dir,
                    message: error.to_string(),
                });
                return loaded;
            }
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        paths.sort();
        for path in paths {
            match read(&path) {
                Ok(comment) => loaded.comments.push(comment),
                Err(error) => loaded.problems.push(LoadProblem {
                    message: match error {
                        ReviewError::Invalid { message, .. } => message,
                        other => other.to_string(),
                    },
                    path,
                }),
            }
        }
        loaded
            .comments
            .sort_by(|a, b| (&a.created_at, &a.id).cmp(&(&b.created_at, &b.id)));
        loaded
    }

    /// The comment `id`.
    pub fn get(&self, id: &str) -> Result<Comment, ReviewError> {
        check_id(id).map_err(|_| ReviewError::BadId(id.to_owned()))?;
        let path = self.path(id);
        if !path.exists() {
            return Err(ReviewError::NotFound(id.to_owned()));
        }
        read(&path)
    }

    /// Writes `comment` to its file, replacing it whole: written beside it
    /// first, then renamed over it.
    pub fn save(&self, comment: &Comment) -> Result<PathBuf, ReviewError> {
        let path = self.path(&comment.id);
        comment.validate().map_err(|message| ReviewError::Invalid {
            path: path.clone(),
            message,
        })?;
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| ReviewError::Io { path, source }
        };
        std::fs::create_dir_all(self.dir()).map_err(io(&self.dir()))?;
        let mut text = serde_json::to_string_pretty(comment).expect("comments serialize");
        text.push('\n');
        let staged = self.dir().join(format!(".{}.json.tmp", comment.id));
        std::fs::write(&staged, text).map_err(io(&staged))?;
        std::fs::rename(&staged, &path).map_err(|source| {
            let _ = std::fs::remove_file(&staged);
            ReviewError::Io {
                path: path.clone(),
                source,
            }
        })?;
        Ok(path)
    }

    /// Loads `id`, applies `change`, and saves it.
    pub fn update(
        &self,
        id: &str,
        change: impl FnOnce(&mut Comment),
    ) -> Result<Comment, ReviewError> {
        let mut comment = self.get(id)?;
        change(&mut comment);
        self.save(&comment)?;
        Ok(comment)
    }

    pub fn path(&self, id: &str) -> PathBuf {
        self.dir().join(format!("{id}.json"))
    }
}

fn read(path: &Path) -> Result<Comment, ReviewError> {
    let text = std::fs::read_to_string(path).map_err(|source| ReviewError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let invalid = |message: String| ReviewError::Invalid {
        path: path.to_path_buf(),
        message,
    };
    let comment: Comment =
        serde_json::from_str(&text).map_err(|error| invalid(error.to_string()))?;
    comment.validate().map_err(invalid)?;
    let stem = path.file_stem().map(|stem| stem.to_string_lossy());
    if stem.as_deref() != Some(comment.id.as_str()) {
        return Err(invalid(format!(
            "id \"{}\" does not match the file name",
            comment.id
        )));
    }
    Ok(comment)
}

fn check_id(id: &str) -> Result<(), String> {
    let valid = !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('-')
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(format!(
            "id \"{id}\" must be lowercase letters, digits, and -"
        ))
    }
}

/// A fresh id: `prefix`, `-`, and 10 hexadecimal digits.
pub fn new_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos()),
    );
    hasher.write_u32(std::process::id());
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    format!("{prefix}-{:010x}", hasher.finish() & 0xff_ffff_ffff)
}

/// The current time, RFC 3339 in UTC to the second.
pub fn now_utc() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    format_utc(seconds)
}

/// `seconds` since the Unix epoch as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn format_utc(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(i64::MAX);
    let rest = seconds % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// Who a comment written here is by: `CELESTA_AUTHOR`, else `git config
/// user.name` in `root`, else the login name, else `Anonymous`.
pub fn default_author(root: &Path) -> String {
    let named = |name: String| {
        let name = name.trim().to_owned();
        (!name.is_empty()).then_some(name)
    };
    std::env::var("CELESTA_AUTHOR")
        .ok()
        .and_then(named)
        .or_else(|| {
            Command::new("git")
                .arg("-C")
                .arg(root)
                .args(["config", "user.name"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| named(String::from_utf8_lossy(&output.stdout).into_owned()))
        })
        .or_else(|| std::env::var("USER").ok().and_then(named))
        .or_else(|| std::env::var("USERNAME").ok().and_then(named))
        .unwrap_or_else(|| "Anonymous".to_owned())
}
