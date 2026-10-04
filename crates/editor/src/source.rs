use crate::EDITOR_DEMO_PROJECT;
use crate::preview::ReactPreview;
use celesta_editor_core::EditorDocument;
use celesta_react_bridge::{
    ReactBridge, project_types_template, refresh_project_types,
    runtime_paths as react_runtime_paths,
};
use std::ffi::OsStr;
use std::path::Path;
use std::time::SystemTime;

/// True when `path` should open as a standalone React composition rather than
/// a `project.json`. A `*.celesta.json` is always a project; a JS/TS module
/// extension is a React entry.
pub(crate) fn is_react_entry(path: &Path) -> bool {
    if path
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.ends_with(".celesta.json"))
    {
        return false;
    }
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some("tsx" | "ts" | "jsx" | "js" | "mjs" | "cjs")
    )
}

/// Loads what the viewer shows for `path`: a `project.json`, a standalone React
/// entry (whose composition facts come from a blocking Node handshake), or the
/// built-in demo when `path` is `None`. Free of GPUI state so it can run on a
/// background thread when a file is opened from the menu.
pub(crate) fn load_source(
    path: Option<&Path>,
) -> Result<(EditorDocument, Option<ReactPreview>), String> {
    let loaded = match path {
        Some(path) if is_react_entry(path) => {
            let (node, cli_script) = react_runtime_paths();
            let metadata = ReactBridge::spawn(&node, &cli_script, path)
                .map_err(|error| error.to_string())?
                .metadata()
                .clone();
            let document = EditorDocument::react_preview(
                path,
                metadata.width,
                metadata.height,
                metadata.frame_rate,
                REACT_PREVIEW_SAMPLE_RATE,
                metadata.duration_in_frames,
            )
            .map_err(|error| error.to_string())?;
            let entry = document
                .react_entry_absolute_path()
                .unwrap_or_else(|| path.to_owned());
            let watched_mtime = entry.parent().and_then(newest_source_mtime);
            Ok((
                document,
                Some(ReactPreview {
                    entry,
                    watched_mtime,
                }),
            ))
        }
        Some(path) => EditorDocument::load(path)
            .map(|document| (document, None))
            .map_err(|error| error.to_string()),
        None => EditorDocument::from_json(EDITOR_DEMO_PROJECT, "examples")
            .map(|document| (document, None))
            .map_err(|error| error.to_string()),
    };
    if let Ok((document, _)) = &loaded {
        refresh_typescript_support(document);
    }
    loaded
}

/// Keeps a project that ran File > Set Up TypeScript on this build's
/// declarations. Best effort: stale types must not block opening the project.
pub(crate) fn refresh_typescript_support(document: &EditorDocument) {
    let Some(entry) = document.react_entry_absolute_path() else {
        return;
    };
    let (_, cli_script) = react_runtime_paths();
    if let Err(error) = refresh_project_types(&project_types_template(&cli_script), &entry) {
        eprintln!("Celesta: couldn’t refresh TypeScript support: {error}");
    }
}

/// Default sample rate for a standalone React entry's audio graph — the entry
/// has no `project.json` to source one from. Matches `celesta-exporter`'s
/// `DEFAULT_REACT_AUDIO_SAMPLE_RATE` and every checked-in example project.
pub(crate) const REACT_PREVIEW_SAMPLE_RATE: u32 = 48_000;

/// Newest mtime among the JS/TS/JSON source files under `dir` (recursively,
/// skipping `node_modules`, `dist`, and `.tmp`). Used to notice when a React
/// composition's code — the entry or any module it bundles — changed on disk.
pub(crate) fn newest_source_mtime(dir: &Path) -> Option<SystemTime> {
    fn walk(dir: &Path, newest: &mut Option<SystemTime>, depth: u32) {
        if depth > 8 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if !matches!(
                    path.file_name().and_then(OsStr::to_str),
                    Some("node_modules" | "dist" | ".tmp" | ".git")
                ) {
                    walk(&path, newest, depth + 1);
                }
            } else if matches!(
                path.extension().and_then(OsStr::to_str),
                Some("tsx" | "ts" | "jsx" | "js" | "mjs" | "cjs" | "json" | "css")
            ) && let Ok(modified) = entry.metadata().and_then(|meta| meta.modified())
            {
                *newest = Some(newest.map_or(modified, |current| current.max(modified)));
            }
        }
    }
    let mut newest = None;
    walk(dir, &mut newest, 0);
    newest
}
