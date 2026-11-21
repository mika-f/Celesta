//! The review comment tools: `list_comments`, `reply_to_comment`, and
//! `resolve_comment`, over the project's `celesta-comments/` files.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};

use celesta_composition::{Layer, LayerContent, Rational, Time};
use celesta_plugin_host::project_root;
use celesta_project::review::{ChapterSpan, Comment, CommentStore, Placement, ReviewError};
use celesta_react_bridge::{ReactBridge, ReactChapter};
use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use strum::{Display, EnumString};

use crate::args::reject_unknown;
use crate::inspect::{Stop, spawn_bridge};
use crate::server::{SharedMedia, error_result, success, usage_error};
use crate::summary::fmt;

/// Most distinct frames `list_comments` evaluates to look for the layers
/// comments name; comments past it are listed without that check.
const LAYER_CHECK_FRAMES: usize = 50;

/// Who a reply or resolution is by when the call does not say.
const AGENT: &str = "agent";

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema, EnumString, Display,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Status {
    #[default]
    Open,
    Resolved,
    All,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListCommentsArgs {
    /// The entry (`.tsx`) or `.celesta.json` project the comments are on.
    entry: PathBuf,
    /// Root composition ID; defaults to the first declared root.
    composition: Option<String>,
    /// `open` (default), `resolved`, or `all`.
    #[serde(default)]
    status: Status,
    /// Only the comments anchored to this chapter id.
    chapter: Option<String>,
    /// Load the entry to place each comment in the composition as it is
    /// now (frames, chapter names, orphans). Default true; false only reads
    /// the files.
    check: Option<bool>,
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: Map<String, Value>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReplyArgs {
    /// The entry the comment is on (any file of its project will do).
    entry: PathBuf,
    /// The comment's id from list_comments.
    id: String,
    /// The reply: say what you changed, or ask what was meant.
    body: String,
    /// Your name as it should show in the thread; default `agent`.
    author: Option<String>,
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: Map<String, Value>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolveArgs {
    /// The entry the comment is on (any file of its project will do).
    entry: PathBuf,
    /// The comment's id from list_comments.
    id: String,
    /// A last reply, added before resolving: what was changed.
    reply: Option<String>,
    /// Your name as it should show in the thread; default `agent`.
    author: Option<String>,
    /// false reopens a resolved comment. Default true.
    resolved: Option<bool>,
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: Map<String, Value>,
}

/// The composition comments are placed against: its id, rate, chapters,
/// and, for the layer check, the bridge that evaluates its frames.
struct Composition<'a> {
    id: String,
    first: bool,
    frame_rate: Rational,
    chapters: Vec<ReactChapter>,
    bridge: Option<&'a mut ReactBridge>,
}

impl Composition<'_> {
    fn spans(&self) -> Vec<ChapterSpan<'_>> {
        spans(&self.chapters)
    }
}

fn spans(chapters: &[ReactChapter]) -> Vec<ChapterSpan<'_>> {
    chapters
        .iter()
        .map(|chapter| ChapterSpan {
            id: &chapter.id,
            name: &chapter.name,
            from: chapter.from,
        })
        .collect()
}

fn is_react(entry: &Path) -> bool {
    entry
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext, "tsx" | "ts" | "jsx" | "js" | "mjs" | "mts"))
}

fn store_for(entry: &Path) -> (CommentStore, String) {
    let entry = std::path::absolute(entry).unwrap_or_else(|_| entry.to_path_buf());
    let store = CommentStore::new(project_root(&entry));
    let key = store.entry_key(&entry);
    (store, key)
}

/// The composition a comment on `composition` of an entry shows on: one
/// naming the shown composition, or none when the shown one is the first.
fn shows_on(comment: &Comment, key: &str, composition: Option<&Composition<'_>>) -> bool {
    match composition {
        None => comment.entry == key,
        Some(shown) => {
            comment.is_on(key, Some(&shown.id)) && (comment.composition.is_some() || shown.first)
        }
    }
}

pub fn list(args: ListCommentsArgs, stop: &Stop, media: &SharedMedia) -> CallToolResult {
    if let Err(message) = reject_unknown(&args.unknown) {
        return usage_error(&message);
    }
    let entry = std::path::absolute(&args.entry).unwrap_or_else(|_| args.entry.clone());
    let (store, key) = store_for(&entry);
    let loaded = store.load();
    let mut warnings: Vec<String> = loaded
        .problems
        .iter()
        .map(|problem| format!("skipped {}: {}", problem.path.display(), problem.message))
        .collect();
    let mut bridge = None;
    if args.check.unwrap_or(true)
        && is_react(&entry)
        && loaded.comments.iter().any(|comment| comment.entry == key)
    {
        match spawn_bridge(&entry, args.composition.clone(), None, None, stop, media) {
            Ok(spawned) => bridge = Some(spawned),
            Err(result) => {
                let message = result
                    .structured_content
                    .as_ref()
                    .and_then(|report| report["error"]["message"].as_str())
                    .unwrap_or("the entry did not load")
                    .to_owned();
                warnings.push(format!(
                    "could not load the entry, so comments are not placed: {message}"
                ));
            }
        }
    }
    let mut composition = bridge.as_mut().map(|bridge| {
        let metadata = bridge.metadata();
        Composition {
            id: metadata.composition_id.clone(),
            first: metadata
                .compositions
                .first()
                .is_none_or(|first| first.id == metadata.composition_id),
            frame_rate: metadata.frame_rate,
            chapters: metadata.chapters.clone(),
            bridge: None,
        }
    });
    if let (Some(composition), Some(bridge)) = (composition.as_mut(), bridge.as_mut()) {
        composition.bridge = Some(bridge);
    }
    let comments: Vec<&Comment> = loaded
        .comments
        .iter()
        .filter(|comment| shows_on(comment, &key, composition.as_ref()))
        .filter(|comment| match args.status {
            Status::Open => comment.is_open(),
            Status::Resolved => !comment.is_open(),
            Status::All => true,
        })
        .filter(|comment| {
            args.chapter
                .as_ref()
                .is_none_or(|chapter| comment.anchor.chapter.as_ref() == Some(chapter))
        })
        .collect();
    let missing_layers = match composition.as_mut() {
        Some(composition) => missing_layers(composition, &comments, &mut warnings),
        None => BTreeSet::new(),
    };
    let mut text = String::new();
    let mut listed = Vec::new();
    for comment in &comments {
        let value = describe(
            comment,
            composition.as_ref(),
            missing_layers.contains(&comment.id),
            &store,
        );
        write_comment(&mut text, comment, &value);
        listed.push(value);
    }
    let open = comments.iter().filter(|comment| comment.is_open()).count();
    let orphaned = listed
        .iter()
        .filter(|value| !value["orphaned"].is_null())
        .count();
    let summary = if comments.is_empty() {
        format!("No {} comments on {key}.", args.status)
    } else {
        format!(
            "{} comment(s) on {key}: {open} open, {} resolved, {orphaned} orphaned.",
            comments.len(),
            comments.len() - open
        )
    };
    for warning in &warnings {
        let _ = writeln!(text, "warning: {warning}");
    }
    text.push_str(&summary);
    success(
        text,
        json!({
            "entry": key,
            "folder": store.dir(),
            "compositionId": composition.as_ref().map(|composition| &composition.id),
            "comments": listed,
            "warnings": warnings,
        }),
    )
}

/// The ids of comments whose layer is not in the scene at their frame.
fn missing_layers(
    composition: &mut Composition<'_>,
    comments: &[&Comment],
    warnings: &mut Vec<String>,
) -> BTreeSet<String> {
    let spans = composition.spans();
    let mut wanted: BTreeMap<u64, Vec<(&str, &str)>> = BTreeMap::new();
    for comment in comments {
        let Some(layer) = comment.anchor.layer.as_deref() else {
            continue;
        };
        if let Placement::Placed { from, .. } = comment.place(&spans) {
            wanted
                .entry(from)
                .or_default()
                .push((comment.id.as_str(), layer));
        }
    }
    let rate = composition.frame_rate;
    let Some(bridge) = composition.bridge.as_deref_mut() else {
        return BTreeSet::new();
    };
    if wanted.len() > LAYER_CHECK_FRAMES {
        warnings.push(format!(
            "layers were checked on the first {LAYER_CHECK_FRAMES} of {} frames comments name",
            wanted.len()
        ));
    }
    let mut missing = BTreeSet::new();
    for (frame, layers) in wanted.into_iter().take(LAYER_CHECK_FRAMES) {
        let Some(time) = i64::try_from(frame)
            .ok()
            .and_then(|frame| Time::frames(frame, rate).ok())
        else {
            continue;
        };
        match bridge.evaluate_at(time, None) {
            Ok(evaluation) => {
                for (id, layer) in layers {
                    if !has_layer(&evaluation.scene.layers, layer) {
                        missing.insert(id.to_owned());
                    }
                }
            }
            Err(error) => warnings.push(format!("frame {frame} did not evaluate: {error}")),
        }
    }
    missing
}

fn has_layer(layers: &[Layer], id: &str) -> bool {
    layers.iter().any(|layer| {
        layer.id == id
            || match &layer.content {
                LayerContent::Group { layers, mask, .. } => {
                    has_layer(layers, id)
                        || mask
                            .as_ref()
                            .is_some_and(|mask| has_layer(&mask.layers, id))
                }
                _ => false,
            }
    })
}

fn describe(
    comment: &Comment,
    composition: Option<&Composition<'_>>,
    layer_missing: bool,
    store: &CommentStore,
) -> Value {
    let mut value = json!({
        "id": comment.id,
        "status": if comment.is_open() { "open" } else { "resolved" },
        "author": comment.author,
        "createdAt": comment.created_at,
        "body": comment.body,
        "anchor": comment.anchor,
        "resolved": comment.resolved,
        "replies": comment.replies,
        "file": store.path(&comment.id),
        "orphaned": Value::Null,
    });
    if let Some(composition) = &comment.composition {
        value["composition"] = json!(composition);
    }
    let Some(shown) = composition else {
        return value;
    };
    let fps = f64::from(shown.frame_rate.numerator) / f64::from(shown.frame_rate.denominator);
    let placement = comment.place(&shown.spans());
    match &placement {
        Placement::Placed {
            from,
            to,
            chapter_name,
        } => {
            value["frame"] = json!(from);
            value["seconds"] = json!(*from as f64 / fps);
            if let Some(to) = to {
                value["toFrame"] = json!(to);
            }
            if let Some(name) = chapter_name {
                value["chapterName"] = json!(name);
            }
            if layer_missing {
                value["orphaned"] = json!("layer");
            }
        }
        Placement::Orphaned { last_seen } => {
            value["orphaned"] = json!("chapter");
            if let Some(seen) = last_seen {
                value["lastSeenFrame"] = json!(seen.frame);
                if let Some(name) = &seen.chapter_name {
                    value["chapterName"] = json!(name);
                }
            }
        }
    }
    value
}

fn write_comment(text: &mut String, comment: &Comment, value: &Value) {
    let anchor = &comment.anchor;
    let status = if comment.is_open() {
        "open"
    } else {
        "resolved"
    };
    let _ = write!(text, "{} [{status}]", comment.id);
    match (&anchor.chapter, value["chapterName"].as_str()) {
        (Some(id), Some(name)) if name != id => {
            let _ = write!(text, " chapter {name} ({id})");
        }
        (Some(id), _) => {
            let _ = write!(text, " chapter {id}");
        }
        (None, _) => {}
    }
    match anchor.to {
        Some(to) => {
            let _ = write!(text, " +{}–{to}", anchor.from);
        }
        None => {
            let _ = write!(text, " +{}", anchor.from);
        }
    }
    if let Some(frame) = value["frame"].as_u64() {
        let seconds = value["seconds"].as_f64().unwrap_or_default();
        match value["toFrame"].as_u64() {
            Some(to) => {
                let _ = write!(text, " = frames {frame}–{to} ({} s)", fmt(seconds));
            }
            None => {
                let _ = write!(text, " = frame {frame} ({} s)", fmt(seconds));
            }
        }
    }
    if let Some(layer) = &anchor.layer {
        let _ = write!(text, ", layer {layer}");
    }
    if let Some(point) = anchor.point {
        let _ = write!(text, ", point ({}, {})", fmt(point.x), fmt(point.y));
    }
    text.push('\n');
    match value["orphaned"].as_str() {
        Some("chapter") => {
            let _ = write!(
                text,
                "  ORPHANED: chapter \"{}\" no longer exists",
                anchor.chapter.as_deref().unwrap_or_default()
            );
            match value["lastSeenFrame"].as_u64() {
                Some(frame) => {
                    let _ = writeln!(text, "; it was at frame {frame}");
                }
                None => text.push('\n'),
            }
        }
        Some("layer") => {
            let _ = writeln!(
                text,
                "  ORPHANED: no layer \"{}\" at that frame",
                anchor.layer.as_deref().unwrap_or_default()
            );
        }
        _ => {}
    }
    let _ = writeln!(
        text,
        "  {} ({}): {}",
        comment.author, comment.created_at, comment.body
    );
    for reply in &comment.replies {
        let _ = writeln!(
            text,
            "  reply {} ({}): {}",
            reply.author, reply.created_at, reply.body
        );
    }
    if let Some(resolved) = &comment.resolved {
        let _ = writeln!(text, "  resolved by {} ({})", resolved.by, resolved.at);
    }
    text.push('\n');
}

fn review_error(error: &ReviewError) -> CallToolResult {
    let code = match error {
        ReviewError::NotFound(_) => "comment_not_found",
        ReviewError::BadId(_) => "usage",
        ReviewError::Invalid { .. } => "invalid_comment",
        ReviewError::Io { .. } => "io",
    };
    error_result(code, error.to_string())
}

fn author(author: Option<String>) -> String {
    author
        .map(|author| author.trim().to_owned())
        .filter(|author| !author.is_empty())
        .unwrap_or_else(|| AGENT.to_owned())
}

fn changed(comment: &Comment, store: &CommentStore, what: &str) -> CallToolResult {
    let status = if comment.is_open() {
        "open"
    } else {
        "resolved"
    };
    success(
        format!("{what} {} ({status}).", comment.id),
        json!({
            "id": comment.id,
            "status": status,
            "resolved": comment.resolved,
            "replies": comment.replies,
            "file": store.path(&comment.id),
        }),
    )
}

pub fn reply(args: ReplyArgs) -> CallToolResult {
    if let Err(message) = reject_unknown(&args.unknown) {
        return usage_error(&message);
    }
    let body = args.body.trim().to_owned();
    if body.is_empty() {
        return usage_error("body is empty");
    }
    let (store, _) = store_for(&args.entry);
    let by = author(args.author);
    match store.update(&args.id, |comment| {
        comment.reply(by, body);
    }) {
        Ok(comment) => changed(&comment, &store, "Replied to"),
        Err(error) => review_error(&error),
    }
}

pub fn resolve(args: ResolveArgs) -> CallToolResult {
    if let Err(message) = reject_unknown(&args.unknown) {
        return usage_error(&message);
    }
    let reply = args
        .reply
        .map(|reply| reply.trim().to_owned())
        .filter(|reply| !reply.is_empty());
    let resolved = args.resolved.unwrap_or(true);
    let (store, _) = store_for(&args.entry);
    let by = author(args.author);
    match store.update(&args.id, |comment| {
        if let Some(reply) = reply {
            comment.reply(by.clone(), reply);
        }
        if resolved {
            comment.resolve(by);
        } else {
            comment.reopen();
        }
    }) {
        Ok(comment) => changed(
            &comment,
            &store,
            if resolved { "Resolved" } else { "Reopened" },
        ),
        Err(error) => review_error(&error),
    }
}

/// The line `inspect` prints about the comments on the inspected
/// composition, and its structured form; `None` without any.
pub fn summary(
    entry: &Path,
    composition_id: &str,
    first: bool,
    chapters: &[ReactChapter],
) -> Option<(String, Value)> {
    let (store, key) = store_for(entry);
    let loaded = store.load();
    let shown = Composition {
        id: composition_id.to_owned(),
        first,
        frame_rate: Rational {
            numerator: 1,
            denominator: 1,
        },
        chapters: chapters.to_vec(),
        bridge: None,
    };
    let spans = shown.spans();
    let mut open = 0;
    let mut resolved = 0;
    let mut orphaned = 0;
    for comment in &loaded.comments {
        if !shows_on(comment, &key, Some(&shown)) {
            continue;
        }
        if comment.is_open() {
            open += 1;
            if comment.place(&spans).is_orphaned() {
                orphaned += 1;
            }
        } else {
            resolved += 1;
        }
    }
    if open + resolved == 0 {
        return None;
    }
    let mut line = format!("comments: {open} open");
    if orphaned > 0 {
        let _ = write!(line, " ({orphaned} orphaned)");
    }
    let _ = write!(line, ", {resolved} resolved");
    if open > 0 {
        line.push_str("; read them with list_comments");
    }
    Some((
        line,
        json!({ "open": open, "resolved": resolved, "orphaned": orphaned }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_project::review::Anchor;

    fn project() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a temporary folder");
        std::fs::write(dir.path().join("celesta.config.json"), "{}").expect("written");
        let entry = dir.path().join("film.tsx");
        std::fs::write(&entry, "").expect("written");
        (dir, entry)
    }

    fn comment(chapter: Option<&str>) -> Comment {
        Comment::new(
            "film.tsx".to_owned(),
            Anchor {
                chapter: chapter.map(str::to_owned),
                from: 10,
                ..Anchor::default()
            },
            "Kanon".to_owned(),
            "Slower here".to_owned(),
        )
    }

    fn chapter(id: &str, from: u64) -> ReactChapter {
        ReactChapter {
            id: id.to_owned(),
            name: id.to_uppercase(),
            from,
            duration_in_frames: 60,
        }
    }

    #[test]
    fn reply_and_resolve_write_the_file() {
        let (_dir, entry) = project();
        let (store, _) = store_for(&entry);
        let saved = comment(None);
        store.save(&saved).expect("saved");
        let result = reply(ReplyArgs {
            entry: entry.clone(),
            id: saved.id.clone(),
            body: " Made it slower ".to_owned(),
            author: None,
            unknown: Map::new(),
        });
        assert_ne!(result.is_error, Some(true));
        let result = resolve(ResolveArgs {
            entry: entry.clone(),
            id: saved.id.clone(),
            reply: Some("Checked frame 10".to_owned()),
            author: Some("claude".to_owned()),
            resolved: None,
            unknown: Map::new(),
        });
        assert_ne!(result.is_error, Some(true));
        let read = store.get(&saved.id).expect("read");
        assert_eq!(read.replies.len(), 2);
        assert_eq!(read.replies[0].author, "agent");
        assert_eq!(read.replies[0].body, "Made it slower");
        assert_eq!(
            read.resolved.as_ref().map(|r| r.by.as_str()),
            Some("claude")
        );
        let result = resolve(ResolveArgs {
            entry,
            id: saved.id.clone(),
            reply: None,
            author: None,
            resolved: Some(false),
            unknown: Map::new(),
        });
        assert_ne!(result.is_error, Some(true));
        assert!(store.get(&saved.id).expect("read").is_open());
    }

    #[test]
    fn unknown_ids_and_empty_replies_are_errors() {
        let (_dir, entry) = project();
        let missing = reply(ReplyArgs {
            entry: entry.clone(),
            id: "c-none".to_owned(),
            body: "x".to_owned(),
            author: None,
            unknown: Map::new(),
        });
        assert_eq!(missing.is_error, Some(true));
        assert_eq!(
            missing.structured_content.expect("a report")["error"]["code"],
            "comment_not_found"
        );
        let empty = reply(ReplyArgs {
            entry,
            id: "c-none".to_owned(),
            body: "  ".to_owned(),
            author: None,
            unknown: Map::new(),
        });
        assert_eq!(empty.is_error, Some(true));
    }

    #[test]
    fn the_summary_counts_open_orphaned_and_resolved() {
        let (_dir, entry) = project();
        let (store, _) = store_for(&entry);
        assert!(summary(&entry, "main", true, &[]).is_none());
        store.save(&comment(Some("intro"))).expect("saved");
        store.save(&comment(Some("gone"))).expect("saved");
        let mut done = comment(None);
        done.resolve("claude".to_owned());
        store.save(&done).expect("saved");
        let mut elsewhere = comment(None);
        elsewhere.composition = Some("teaser".to_owned());
        store.save(&elsewhere).expect("saved");
        let (line, value) =
            summary(&entry, "main", true, &[chapter("intro", 0)]).expect("a summary");
        assert_eq!(
            line,
            "comments: 2 open (1 orphaned), 1 resolved; read them with list_comments"
        );
        assert_eq!(value["orphaned"], 1);
    }

    #[test]
    fn an_unnamed_comment_shows_only_on_the_first_composition() {
        let unnamed = comment(None);
        let shown = |first| Composition {
            id: "teaser".to_owned(),
            first,
            frame_rate: Rational {
                numerator: 30,
                denominator: 1,
            },
            chapters: Vec::new(),
            bridge: None,
        };
        assert!(shows_on(&unnamed, "film.tsx", Some(&shown(true))));
        assert!(!shows_on(&unnamed, "film.tsx", Some(&shown(false))));
        assert!(shows_on(&unnamed, "film.tsx", None));
    }

    #[test]
    fn a_listing_without_the_entry_reads_the_files() {
        let (_dir, entry) = project();
        let (store, _) = store_for(&entry);
        let saved = comment(Some("intro"));
        store.save(&saved).expect("saved");
        let shown = Composition {
            id: "main".to_owned(),
            first: true,
            frame_rate: Rational {
                numerator: 30,
                denominator: 1,
            },
            chapters: vec![chapter("intro", 60)],
            bridge: None,
        };
        let value = describe(&saved, Some(&shown), false, &store);
        assert_eq!(value["frame"], 70);
        assert_eq!(value["chapterName"], "INTRO");
        assert!(value["orphaned"].is_null());
        let mut text = String::new();
        write_comment(&mut text, &saved, &value);
        assert!(
            text.contains("chapter INTRO (intro) +10 = frame 70"),
            "{text}"
        );
        let orphan = describe(
            &saved,
            Some(&Composition {
                chapters: Vec::new(),
                ..shown
            }),
            false,
            &store,
        );
        assert_eq!(orphan["orphaned"], "chapter");
    }
}
