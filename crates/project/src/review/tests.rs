use std::path::Path;

use super::*;

fn chapters() -> Vec<ChapterSpan<'static>> {
    vec![
        ChapterSpan {
            id: "intro",
            name: "Intro",
            from: 0,
        },
        ChapterSpan {
            id: "demo",
            name: "Demo",
            from: 120,
        },
    ]
}

fn on_demo(from: u64, to: Option<u64>) -> Comment {
    Comment::new(
        "film.tsx".to_owned(),
        Anchor {
            chapter: Some("demo".to_owned()),
            from,
            to,
            ..Anchor::default()
        },
        "Kanon".to_owned(),
        "Slower here".to_owned(),
    )
}

#[test]
fn frames_count_from_the_chapter() {
    let comment = on_demo(15, Some(45));
    assert_eq!(
        comment.place(&chapters()),
        Placement::Placed {
            from: 135,
            to: Some(165),
            chapter_name: Some("Demo".to_owned()),
        }
    );
}

#[test]
fn a_moved_chapter_moves_its_comments() {
    let comment = on_demo(15, None);
    let mut moved = chapters();
    moved[1].from = 300;
    assert_eq!(comment.place(&moved).frame(), Some(315));
}

#[test]
fn without_a_chapter_frames_are_absolute() {
    let mut comment = on_demo(15, None);
    comment.anchor.chapter = None;
    assert_eq!(comment.place(&[]).frame(), Some(15));
}

#[test]
fn a_gone_chapter_orphans_the_comment_at_its_last_place() {
    let mut comment = on_demo(15, None);
    comment.remember(&chapters());
    let placement = comment.place(&chapters()[..1]);
    assert!(placement.is_orphaned());
    assert_eq!(placement.frame(), Some(135));
    assert_eq!(
        placement,
        Placement::Orphaned {
            last_seen: Some(LastSeen {
                frame: 135,
                chapter_name: Some("Demo".to_owned()),
                extra: Map::new(),
            }),
        }
    );
    // Remembering while orphaned keeps the old place.
    comment.remember(&chapters()[..1]);
    assert_eq!(comment.last_seen.as_ref().map(|seen| seen.frame), Some(135));
}

#[test]
fn a_span_covers_its_frames_and_a_moment_only_itself() {
    let span = on_demo(0, Some(10)).place(&chapters());
    assert!(span.covers(120) && span.covers(129));
    assert!(!span.covers(130) && !span.covers(119));
    let moment = on_demo(5, None).place(&chapters());
    assert!(moment.covers(125) && !moment.covers(126));
}

#[test]
fn compositions_filter_only_when_both_name_one() {
    let mut comment = on_demo(0, None);
    assert!(comment.is_on("film.tsx", Some("main")));
    assert!(comment.is_on("film.tsx", None));
    assert!(!comment.is_on("other.tsx", None));
    comment.composition = Some("teaser".to_owned());
    assert!(!comment.is_on("film.tsx", Some("main")));
    assert!(comment.is_on("film.tsx", Some("teaser")));
}

#[test]
fn replies_and_resolution() {
    let mut comment = on_demo(0, None);
    let reply = comment
        .reply("claude".to_owned(), "Done".to_owned())
        .clone();
    assert!(reply.id.starts_with("r-"));
    assert!(comment.is_open());
    comment.resolve("claude".to_owned());
    let first = comment.resolved.clone();
    comment.resolve("someone".to_owned());
    assert_eq!(comment.resolved, first);
    comment.reopen();
    assert!(comment.is_open());
}

#[test]
fn utc_times_format_as_rfc_3339() {
    assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
    assert_eq!(format_utc(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(format_utc(1_795_341_600), "2026-11-22T10:00:00Z");
}

#[test]
fn ids_are_file_names() {
    let id = new_id("c");
    assert_eq!(id.len(), 12);
    assert!(check_id(&id).is_ok());
    assert_ne!(id, new_id("c"));
    for bad in ["", "../x", "A1", "-x", "a b", "a/b"] {
        assert!(check_id(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn validation_rejects_bad_anchors_and_newer_versions() {
    let mut comment = on_demo(10, Some(10));
    assert!(comment.validate().is_err());
    comment.anchor.to = None;
    comment.anchor.point = Some(Point::new(1.5, 0.0));
    assert!(comment.validate().is_err());
    comment.anchor.point = Some(Point::new(1.0, 0.0));
    assert!(comment.validate().is_ok());
    comment.version = FORMAT_VERSION + 1;
    assert!(comment.validate().unwrap_err().contains("update Celesta"));
}

fn store() -> (tempfile::TempDir, CommentStore) {
    let dir = tempfile::tempdir().expect("a temporary folder");
    let store = CommentStore::new(dir.path());
    (dir, store)
}

#[test]
fn a_project_without_the_folder_has_no_comments() {
    let (_dir, store) = store();
    let loaded = store.load();
    assert!(loaded.comments.is_empty() && loaded.problems.is_empty());
}

#[test]
fn comments_round_trip_through_their_files() {
    let (_dir, store) = store();
    let mut first = on_demo(15, Some(45));
    first.created_at = "2026-11-22T10:00:00Z".to_owned();
    first.anchor.layer = Some("title".to_owned());
    first.anchor.point = Some(Point::new(0.25, 0.5));
    let mut second = on_demo(0, None);
    second.created_at = "2026-11-22T09:00:00Z".to_owned();
    let path = store.save(&first).expect("saved");
    store.save(&second).expect("saved");
    assert_eq!(path, store.dir().join(format!("{}.json", first.id)));
    let text = std::fs::read_to_string(&path).expect("written");
    assert!(text.ends_with("}\n"));
    assert!(text.contains("\"createdAt\""));
    assert!(!text.contains("\"replies\""), "empty lists are left out");
    let loaded = store.load();
    assert!(loaded.problems.is_empty());
    assert_eq!(loaded.comments, vec![second, first.clone()]);
    assert_eq!(store.get(&first.id).expect("found"), first);
}

#[test]
fn unknown_fields_survive_a_rewrite() {
    let (_dir, store) = store();
    let comment = on_demo(0, None);
    store.save(&comment).expect("saved");
    let path = store.path(&comment.id);
    let mut value: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    value["reactions"] = serde_json::json!({ "+1": ["kanon"] });
    std::fs::write(&path, value.to_string()).expect("written");
    store
        .update(&comment.id, |comment| comment.resolve("claude".to_owned()))
        .expect("updated");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("\"reactions\""));
    assert!(text.contains("\"resolved\""));
}

#[test]
fn a_broken_file_is_a_problem_and_the_rest_load() {
    let (_dir, store) = store();
    let comment = on_demo(0, None);
    store.save(&comment).expect("saved");
    std::fs::write(store.dir().join("c-broken.json"), "{ not json").expect("written");
    std::fs::write(store.dir().join("notes.txt"), "ignored").expect("written");
    let mut renamed = comment.clone();
    renamed.id = "c-other".to_owned();
    std::fs::write(
        store.dir().join("c-mismatch.json"),
        serde_json::to_string(&renamed).expect("json"),
    )
    .expect("written");
    let loaded = store.load();
    assert_eq!(loaded.comments, vec![comment]);
    assert_eq!(loaded.problems.len(), 2);
    assert!(
        loaded
            .problems
            .iter()
            .any(|problem| problem.message.contains("does not match the file name"))
    );
}

#[test]
fn get_refuses_paths_and_reports_missing_ids() {
    let (_dir, store) = store();
    assert!(matches!(store.get("../secret"), Err(ReviewError::BadId(_))));
    assert!(matches!(store.get("c-none"), Err(ReviewError::NotFound(_))));
}

#[test]
fn entries_are_named_from_the_root() {
    let store = CommentStore::new(Path::new("/projects/film"));
    assert_eq!(
        store.entry_key(Path::new("/projects/film/scenes/intro.tsx")),
        "scenes/intro.tsx"
    );
    assert_eq!(store.entry_key(Path::new("/elsewhere/a.tsx")), "a.tsx");
}

#[test]
fn the_author_can_be_set_by_environment() {
    // Only checks that some name comes back; the variable itself is not
    // set here, since tests run in parallel in one process.
    assert!(!default_author(Path::new(".")).is_empty());
}

#[test]
fn the_example_comments_load() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scene-timing");
    if !root.exists() {
        return; // A packaged crate has no examples.
    }
    let loaded = CommentStore::new(&root).load();
    assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
    assert_eq!(loaded.comments.len(), 4);
    let chapters = [
        ChapterSpan {
            id: "intro",
            name: "Intro",
            from: 0,
        },
        ChapterSpan {
            id: "main",
            name: "Main",
            from: 90,
        },
        ChapterSpan {
            id: "outro",
            name: "Outro",
            from: 210,
        },
    ];
    let orphans: Vec<&str> = loaded
        .comments
        .iter()
        .filter(|comment| comment.place(&chapters).is_orphaned())
        .map(|comment| comment.anchor.chapter.as_deref().unwrap_or_default())
        .collect();
    assert_eq!(orphans, ["credits"]);
    // Each comment's lastSeen matches where it is placed now.
    for comment in &loaded.comments {
        if let Placement::Placed { from, .. } = comment.place(&chapters) {
            assert_eq!(
                comment.last_seen.as_ref().map(|seen| seen.frame),
                Some(from),
                "{}",
                comment.id
            );
        }
    }
}

#[test]
fn unknown_nested_fields_survive_a_rewrite() {
    let (_dir, store) = store();
    let mut comment = on_demo(0, Some(10));
    comment.anchor.point = Some(Point::new(0.5, 0.5));
    comment.reply("claude".to_owned(), "x".to_owned());
    comment.remember(&chapters());
    store.save(&comment).expect("saved");
    let path = store.path(&comment.id);
    let mut value: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    value["anchor"]["region"] = serde_json::json!([0, 0, 10, 10]);
    value["anchor"]["point"]["z"] = serde_json::json!(1);
    value["lastSeen"]["timecode"] = serde_json::json!("00:00:04:00");
    value["replies"][0]["edited"] = serde_json::json!(true);
    std::fs::write(&path, value.to_string()).expect("written");
    store
        .update(&comment.id, |comment| {
            comment.resolve("claude".to_owned());
            comment.remember(&chapters());
        })
        .expect("updated");
    let value: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    assert_eq!(value["anchor"]["region"], serde_json::json!([0, 0, 10, 10]));
    assert_eq!(value["anchor"]["point"]["z"], 1);
    assert_eq!(value["lastSeen"]["timecode"], "00:00:04:00");
    assert_eq!(value["replies"][0]["edited"], true);
    assert_eq!(value["resolved"]["by"], "claude");
}

#[test]
fn concurrent_updates_keep_every_reply() {
    let (_dir, store) = store();
    let comment = on_demo(0, None);
    store.save(&comment).expect("saved");
    let threads: Vec<_> = (0..8)
        .map(|n| {
            let store = store.clone();
            let id = comment.id.clone();
            std::thread::spawn(move || {
                store
                    .update(&id, |comment| {
                        comment.reply(format!("writer {n}"), "x".to_owned());
                    })
                    .expect("updated");
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("joined");
    }
    let read = store.get(&comment.id).expect("read");
    assert_eq!(read.replies.len(), 8);
    let leftovers: Vec<_> = std::fs::read_dir(store.dir())
        .expect("listed")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with('.'))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[cfg(unix)]
#[test]
fn a_planted_link_is_not_written_through() {
    let (dir, store) = store();
    let comment = on_demo(0, None);
    store.save(&comment).expect("saved");
    let outside = dir.path().join("outside.txt");
    std::fs::write(&outside, "keep").expect("written");
    // A link where earlier builds staged their writes, then one at the
    // comment's own file.
    std::os::unix::fs::symlink(
        &outside,
        store.dir().join(format!(".{}.json.tmp", comment.id)),
    )
    .expect("linked");
    store
        .update(&comment.id, |comment| comment.resolve("claude".to_owned()))
        .expect("updated");
    assert_eq!(std::fs::read_to_string(&outside).expect("read"), "keep");
    let link = store.path(&comment.id);
    std::fs::remove_file(&link).expect("removed");
    std::os::unix::fs::symlink(&outside, &link).expect("linked");
    store.save(&comment).expect("saved over the link");
    assert_eq!(std::fs::read_to_string(&outside).expect("read"), "keep");
    assert!(!std::fs::symlink_metadata(&link).expect("meta").is_symlink());
}

#[cfg(unix)]
#[test]
fn a_stale_lock_that_cannot_be_removed_is_an_error() {
    use std::os::unix::fs::PermissionsExt as _;
    let (_dir, store) = store();
    let comment = on_demo(0, None);
    store.save(&comment).expect("saved");
    let lock = store.dir().join(format!(".{}.lock", comment.id));
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(120);
    std::fs::File::create(&lock)
        .and_then(|file| file.set_modified(old))
        .expect("an old lock");
    let read_only = std::fs::Permissions::from_mode(0o555);
    std::fs::set_permissions(store.dir(), read_only).expect("read-only");
    // Root may still remove it; then the update simply goes through.
    let removable = std::fs::File::create(store.dir().join("probe")).is_ok();
    let started = std::time::Instant::now();
    let result = store.update(&comment.id, |comment| comment.resolve("claude".to_owned()));
    std::fs::set_permissions(store.dir(), std::fs::Permissions::from_mode(0o755))
        .expect("writable");
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    if !removable {
        assert!(matches!(result, Err(ReviewError::Io { .. })), "{result:?}");
    }
}

#[test]
fn updating_a_missing_comment_is_not_found() {
    let (_dir, store) = store();
    assert!(matches!(
        store.update("c-none", |_| {}),
        Err(ReviewError::NotFound(_))
    ));
}
