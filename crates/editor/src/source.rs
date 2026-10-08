use crate::EDITOR_DEMO_PROJECT;
use crate::preview::ReactPreview;
use celesta_editor_core::EditorDocument;
use celesta_react_bridge::{
    PropertyInputs, ReactBridge, project_types_template, refresh_project_types,
    runtime_paths as react_runtime_paths,
};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
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

/// `--props`/`--props-file` from the command line, kept with the opened entry
/// so every reload re-reads the file and passes the same values to preview,
/// audio, and export as `celesta-exporter` would.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PropertyArgs {
    pub(crate) props_file: Option<PathBuf>,
    pub(crate) props: Option<String>,
}

impl PropertyArgs {
    pub(crate) fn new(props_file: Option<PathBuf>, props: Option<String>) -> Self {
        Self {
            // Absolute, so a reload reads the same file whatever the current directory.
            props_file: props_file.map(|file| std::path::absolute(&file).unwrap_or(file)),
            props,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.props_file.is_none() && self.props.is_none()
    }

    pub(crate) fn load(&self) -> Result<PropertyInputs, String> {
        PropertyInputs::load(self.props_file.as_deref(), self.props.as_deref())
            .map_err(|error| error.to_string())
    }
}

/// Loads what the viewer shows for `path`: a `project.json`, a standalone React
/// entry (whose composition facts come from a blocking Node handshake), or the
/// built-in demo when `path` is `None`. Free of GPUI state so it can run on a
/// background thread when a file is opened from the menu.
pub(crate) fn load_source(
    path: Option<&Path>,
    property_args: PropertyArgs,
) -> Result<(EditorDocument, Option<ReactPreview>), String> {
    if !property_args.is_empty() && !path.is_some_and(is_react_entry) {
        return Err("--props and --props-file need a React entry (.tsx) to open".to_owned());
    }
    let loaded = match path {
        Some(path) if is_react_entry(path) => {
            let (node, cli_script) = react_runtime_paths();
            let properties = property_args.load()?;
            let metadata =
                ReactBridge::spawn_with_properties(&node, &cli_script, path, &properties)
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
            let watched_mtime = watched_mtime(&entry, &property_args);
            Ok((
                document,
                Some(ReactPreview {
                    entry,
                    watched_mtime,
                    property_args,
                    properties,
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

/// Newest mtime among the entry's sources and its `--props-file`, which may
/// live outside the entry's directory.
pub(crate) fn watched_mtime(entry: &Path, property_args: &PropertyArgs) -> Option<SystemTime> {
    let sources = entry.parent().and_then(newest_source_mtime);
    let props_file = property_args.props_file.as_deref().and_then(|file| {
        std::fs::metadata(file)
            .and_then(|meta| meta.modified())
            .ok()
    });
    sources.max(props_file)
}

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
