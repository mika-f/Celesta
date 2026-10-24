use crate::EDITOR_DEMO_PROJECT;
use crate::preview::{KeptSchema, ReactPreview};
use crate::problems::error_chain;
use celesta_editor_core::EditorDocument;
use celesta_react_bridge::{
    PrepareDependency, PropertyInputs, ReactBridge, SpawnOptions, project_types_template,
    refresh_project_types, runtime_paths as react_runtime_paths,
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

/// `--props`/`--props-file` and the initial `--composition` selection, kept with the opened entry
/// so every reload re-reads the file and passes the same values to preview,
/// audio, and export as `celesta-exporter` would.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PropertyArgs {
    pub(crate) props_file: Option<PathBuf>,
    pub(crate) props: Option<String>,
    pub(crate) composition: Option<String>,
}

impl PropertyArgs {
    pub(crate) fn new(props_file: Option<PathBuf>, props: Option<String>) -> Self {
        Self {
            // Absolute, so a reload reads the same file whatever the current directory.
            props_file: props_file.map(|file| std::path::absolute(&file).unwrap_or(file)),
            props,
            composition: None,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.props_file.is_none() && self.props.is_none() && self.composition.is_none()
    }

    pub(crate) fn load(&self, entry: &Path) -> Result<PropertyInputs, String> {
        PropertyInputs::load_for_entry(entry, self.props_file.as_deref(), self.props.as_deref())
            .map_err(|error| error_chain(&error))
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
        return Err(
            "--props, --props-file and --composition need a React entry to open".to_owned(),
        );
    }
    let loaded = match path {
        Some(path) if is_react_entry(path) => {
            let (node, cli_script) = react_runtime_paths();
            // Sampled before reading, so an edit made while Node starts is noticed.
            let watched_mtime = watched_mtime(path, &property_args);
            let properties = property_args.load(path)?;
            let metadata = ReactBridge::spawn_with_options(
                &node,
                &cli_script,
                path,
                &properties,
                SpawnOptions {
                    composition_id: property_args.composition.clone(),
                    ..SpawnOptions::default()
                },
            )
            .map_err(|error| error_chain(&error))?
            .metadata()
            .clone();
            // A failed first load never opens, so there is no schema to keep.
            let property_schema = KeptSchema::from_metadata(&metadata);
            let document = EditorDocument::react_preview(
                path,
                metadata.width,
                metadata.height,
                metadata.frame_rate,
                REACT_PREVIEW_SAMPLE_RATE,
                metadata.duration_in_frames,
            )
            .map_err(|error| error_chain(&error))?;
            let entry = document
                .react_entry_absolute_path()
                .unwrap_or_else(|| path.to_owned());
            Ok((
                document,
                Some(ReactPreview {
                    composition_id: metadata.composition_id,
                    compositions: metadata.compositions,
                    chapters: metadata.chapters,
                    primitive_properties: metadata.primitive_properties,
                    component_schemas: metadata.component_schemas,
                    entry,
                    watched_mtime,
                    dependencies: metadata.prepare_dependencies,
                    property_args,
                    disk_properties: properties.clone(),
                    properties,
                    property_schema,
                    property_issues: Vec::new(),
                    property_load_error: None,
                    properties_read_reload: 0,
                }),
            ))
        }
        Some(path) => EditorDocument::load(path)
            .map(|document| (document, None))
            .map_err(|error| error_chain(&error)),
        None => EditorDocument::from_json(EDITOR_DEMO_PROJECT, "examples")
            .map(|document| (document, None))
            .map_err(|error| error_chain(&error)),
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

/// The state on disk now of each path in `recorded`, in the same order.
pub(crate) fn sample_dependencies(recorded: &[PrepareDependency]) -> Vec<PrepareDependency> {
    recorded
        .iter()
        .map(|dependency| PrepareDependency::sample(&dependency.path))
        .collect()
}

/// Debounces reloads for changed preparation dependencies. A tool saving a
/// recording writes it over a moment, sometimes as several files, so a
/// change reloads only once two consecutive polls see the same state. A
/// state that already triggered a reload never triggers another, so a
/// dependency whose reported and sampled forms differ cannot loop.
#[derive(Debug, Default)]
pub(crate) struct DependencyDebounce {
    pending: Option<Vec<PrepareDependency>>,
    triggered: Option<Vec<PrepareDependency>>,
}

impl DependencyDebounce {
    /// Takes one poll's `current` sample of the `recorded` dependencies;
    /// true when the entry should reload now.
    pub(crate) fn poll(
        &mut self,
        recorded: &[PrepareDependency],
        current: Vec<PrepareDependency>,
    ) -> bool {
        if current == recorded {
            self.pending = None;
            self.triggered = None;
            return false;
        }
        if self.triggered.as_ref() == Some(&current) {
            return false;
        }
        if self.pending.as_ref() == Some(&current) {
            self.pending = None;
            self.triggered = Some(current);
            return true;
        }
        self.pending = Some(current);
        false
    }

    /// Forgets a pending change: a reload for another reason reads it anyway.
    pub(crate) fn reset(&mut self) {
        self.pending = None;
    }
}

/// Newest mtime among the entry's sources, its `--props-file` (which may live
/// outside the entry's directory), and the entry directory itself, whose mtime
/// moves when `props.json` is created or deleted.
pub(crate) fn watched_mtime(entry: &Path, property_args: &PropertyArgs) -> Option<SystemTime> {
    let sources = entry.parent().and_then(newest_source_mtime);
    let props_file = property_args.props_file.as_deref().and_then(|file| {
        std::fs::metadata(file)
            .and_then(|meta| meta.modified())
            .ok()
    });
    let folder = entry
        .parent()
        .and_then(|dir| std::fs::metadata(dir).and_then(|meta| meta.modified()).ok());
    sources.max(props_file).max(folder)
}

/// Newest mtime among the JS/TS/JSON/CSS/WGSL source files under `dir` (recursively,
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
                Some(
                    "tsx"
                        | "ts"
                        | "jsx"
                        | "js"
                        | "mjs"
                        | "cjs"
                        | "json"
                        | "css"
                        | "wgsl"
                        | "srt"
                        | "vtt"
                )
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn dependency(path: &str, mtime: &str) -> PrepareDependency {
        PrepareDependency {
            path: PathBuf::from(path),
            mtime_ns: Some(mtime.to_owned()),
            size: Some(10),
        }
    }

    #[test]
    fn a_dependency_change_reloads_once_two_polls_agree() {
        let recorded = vec![dependency("a.wav", "1")];
        let mut debounce = DependencyDebounce::default();
        assert!(!debounce.poll(&recorded, recorded.clone()));
        // Still being written: each poll sees a newer state.
        assert!(!debounce.poll(&recorded, vec![dependency("a.wav", "2")]));
        assert!(!debounce.poll(&recorded, vec![dependency("a.wav", "3")]));
        // Settled.
        assert!(debounce.poll(&recorded, vec![dependency("a.wav", "3")]));
    }

    #[test]
    fn a_state_that_already_reloaded_does_not_reload_again() {
        let recorded = vec![dependency("a.wav", "1")];
        let mut debounce = DependencyDebounce::default();
        let changed = vec![dependency("a.wav", "2")];
        assert!(!debounce.poll(&recorded, changed.clone()));
        assert!(debounce.poll(&recorded, changed.clone()));
        // The reload kept the old record (it failed, or reported another
        // form): the same state must not reload in a loop.
        assert!(!debounce.poll(&recorded, changed.clone()));
        assert!(!debounce.poll(&recorded, changed));
        // A new write does.
        assert!(!debounce.poll(&recorded, vec![dependency("a.wav", "4")]));
        assert!(debounce.poll(&recorded, vec![dependency("a.wav", "4")]));
    }

    #[test]
    fn sampled_dependencies_notice_replacement_and_deletion() {
        let dir = std::env::temp_dir().join(format!("celesta-deps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("a.wav");
        std::fs::write(&wav, b"short").unwrap();
        let recorded = vec![
            PrepareDependency::sample(&dir),
            PrepareDependency::sample(&wav),
        ];
        assert_eq!(sample_dependencies(&recorded), recorded);

        // Replaced atomically with a longer file.
        std::fs::write(dir.join("a.wav.tmp"), b"a longer take").unwrap();
        std::fs::rename(dir.join("a.wav.tmp"), &wav).unwrap();
        let replaced = sample_dependencies(&recorded);
        assert_ne!(replaced[1], recorded[1]);

        std::fs::remove_file(&wav).unwrap();
        let deleted = sample_dependencies(&recorded);
        assert_eq!(deleted[1].mtime_ns, None);
        assert_eq!(deleted[1].size, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn watched_mtime_notices_props_json_deletion() {
        let dir = std::env::temp_dir().join(format!("celesta-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("film.tsx");
        // Outlast the filesystem's mtime granularity.
        let tick = || std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(dir.join("props.json"), "{}").unwrap();
        tick();
        // The entry is newer, so deleting props.json leaves the newest source mtime alone.
        std::fs::write(&entry, "export default null;").unwrap();
        tick();
        let args = PropertyArgs::default();

        let before = watched_mtime(&entry, &args);
        tick();
        std::fs::remove_file(dir.join("props.json")).unwrap();
        let after = watched_mtime(&entry, &args);

        assert_ne!(before, after);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
