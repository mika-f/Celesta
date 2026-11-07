use crate::EDITOR_DEMO_PROJECT;
use crate::plugins::FoundPlugins;
use crate::preview::{KeptSchema, ReactPreview};
use crate::problems::error_chain;
use celesta_editor_core::EditorDocument;
use celesta_plugin_host::PluginEntry;
use celesta_react_bridge::{
    PrepareDependency, PropertyInputs, ReactBridge, SpawnOptions, project_types_template,
    refresh_project_types, runtime_paths as react_runtime_paths,
};
use std::collections::{BTreeSet, HashMap};
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
    plugins: &FoundPlugins,
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
            let watched = watched_stamp(path, &property_args);
            let properties = property_args.load(path)?;
            let metadata = ReactBridge::spawn_with_options(
                &node,
                &cli_script,
                path,
                &properties,
                SpawnOptions {
                    composition_id: property_args.composition.clone(),
                    plugins: Some(plugins.installed.runtime()),
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
                    loaded: watched.clone(),
                    watched,
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
        refresh_typescript_support(document, &plugins.declared());
    }
    loaded
}

/// Keeps a project that ran File > Set Up TypeScript on this build's
/// declarations and on `plugins`'. Best effort: stale types must not block
/// opening the project.
pub(crate) fn refresh_typescript_support(document: &EditorDocument, plugins: &[PluginEntry]) {
    let Some(entry) = document.react_entry_absolute_path() else {
        return;
    };
    let (_, cli_script) = react_runtime_paths();
    if let Err(error) = refresh_project_types(&project_types_template(&cli_script), &entry, plugins)
    {
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

/// A sample of what the reload watcher watches: the path, mtime and size of
/// every source file under the entry's folder (see [`source_files`]), the
/// mtime of every folder walked (which changes when a file in it is added,
/// removed or renamed), and the `--props-file`'s. Two samples differ when
/// any of them changed, in either direction: a file restored with an older
/// mtime (`cp -p`, `rsync -a`) counts, and so does a module deleted from a
/// subfolder. `None` when the entry has no folder or its folder's metadata
/// cannot be read; a subfolder that cannot be listed is left out of the
/// sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WatchStamp {
    /// Every folder and source file walked, by path relative to the
    /// entry's folder (so an entry opened as `film.tsx` and later sampled
    /// as `/p/film.tsx` gives the same stamp), sorted by it.
    sources: Vec<WatchedSource>,
    /// The `--props-file`'s mtime and size; `Some(None)` when it cannot be
    /// read.
    props_file: Option<Option<(Option<SystemTime>, u64)>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WatchedSource {
    path: PathBuf,
    folder: bool,
    modified: Option<SystemTime>,
    len: u64,
}

impl WatchStamp {
    /// The source files, relative to the entry's folder, that differ
    /// between this sample and `newer`: changed, added or removed, sorted.
    /// A path that is a folder in both samples is left out (its files are
    /// listed), and so is the `--props-file`; a file replaced by a folder of
    /// the same name, or the other way round, is listed.
    pub(crate) fn changed_files(&self, newer: &WatchStamp) -> Vec<PathBuf> {
        let by_path = |stamp: &WatchStamp| -> HashMap<PathBuf, WatchedSource> {
            stamp
                .sources
                .iter()
                .map(|source| (source.path.clone(), source.clone()))
                .collect()
        };
        let (older, newer) = (by_path(self), by_path(newer));
        let paths: BTreeSet<&PathBuf> = older.keys().chain(newer.keys()).collect();
        paths
            .into_iter()
            .filter(|path| {
                let (old, new) = (older.get(*path), newer.get(*path));
                let file = |source: Option<&WatchedSource>| source.is_some_and(|s| !s.folder);
                old != new && (file(old) || file(new))
            })
            .cloned()
            .collect()
    }
}

pub(crate) fn watched_stamp(entry: &Path, property_args: &PropertyArgs) -> Option<WatchStamp> {
    let dir = entry.parent()?;
    let mut sources = Vec::new();
    walk_sources(dir, &mut |path, metadata| {
        sources.push(WatchedSource {
            path: path.strip_prefix(dir).unwrap_or(path).to_owned(),
            folder: metadata.is_dir(),
            modified: metadata.modified().ok(),
            len: metadata.len(),
        });
    });
    if sources.is_empty() {
        return None;
    }
    sources.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    let props_file = property_args.props_file.as_deref().map(|file| {
        std::fs::metadata(file)
            .ok()
            .map(|metadata| (metadata.modified().ok(), metadata.len()))
    });
    Some(WatchStamp {
        sources,
        props_file,
    })
}

/// The source files that differ between `loaded`, the sample the loaded
/// code was read from, and `latest`, as real paths under the entry's
/// folder: the bundler names the modules elements are written in by their
/// real paths (on Windows without the `\\?\` prefix `std::fs::canonicalize`
/// adds, as `dunce` resolves them). `None` when either sample is missing or
/// the folder cannot be resolved, so which files changed is not known.
pub(crate) fn changed_sources(
    entry: &Path,
    loaded: Option<&WatchStamp>,
    latest: Option<&WatchStamp>,
) -> Option<Vec<PathBuf>> {
    let changed = loaded?.changed_files(latest?);
    let dir = dunce::canonicalize(entry.parent()?).ok()?;
    Some(changed.into_iter().map(|file| dir.join(file)).collect())
}

/// The JS/TS/JSON/CSS/WGSL/subtitle source files under `dir`, recursively
/// to depth 8, skipping `node_modules`, `dist`, `.tmp` and `.git`.
pub(crate) fn source_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_sources(dir, &mut |path, metadata| {
        if !metadata.is_dir() {
            files.push(path.to_owned());
        }
    });
    files
}

/// Calls `visit` with `dir` and, recursively to depth 8, every folder and
/// source file under it (see [`source_files`]), with their metadata (not
/// followed through a symlink). An unreadable entry is skipped.
fn walk_sources(dir: &Path, visit: &mut dyn FnMut(&Path, &std::fs::Metadata)) {
    fn walk(dir: &Path, visit: &mut dyn FnMut(&Path, &std::fs::Metadata), depth: u32) {
        if depth > 8 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.is_dir() {
                if !matches!(
                    path.file_name().and_then(OsStr::to_str),
                    Some("node_modules" | "dist" | ".tmp" | ".git")
                ) {
                    visit(&path, &metadata);
                    walk(&path, visit, depth + 1);
                }
            } else if matches!(
                path.extension().and_then(OsStr::to_str),
                Some(
                    "tsx"
                        | "ts"
                        | "mts"
                        | "cts"
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
            ) {
                visit(&path, &metadata);
            }
        }
    }
    let Ok(metadata) = std::fs::metadata(dir) else {
        return;
    };
    visit(dir, &metadata);
    walk(dir, visit, 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

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

    /// A folder with `film.tsx` and `scenes/{a,b}.ts`. The entry's mtime is
    /// a day ahead of the clock, so it stays the newest of everything.
    fn scene_folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("celesta-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        for module in ["scenes/a.ts", "scenes/b.ts"] {
            std::fs::write(dir.join(module), "export const x = 1;").unwrap();
        }
        let entry = dir.join("film.tsx");
        std::fs::write(&entry, "export default null;").unwrap();
        let ahead = SystemTime::now() + Duration::from_secs(86_400);
        set_modified(&entry, ahead);
        dir
    }

    fn set_modified(path: &Path, time: SystemTime) {
        std::fs::File::options()
            .write(true)
            .open(path)
            .and_then(|file| file.set_modified(time))
            .unwrap();
    }

    #[test]
    fn the_watcher_notices_a_module_deleted_from_a_subfolder() {
        let dir = scene_folder("watch-delete");
        let entry = dir.join("film.tsx");
        let args = PropertyArgs::default();

        let before = watched_stamp(&entry, &args);
        // Neither the newest file nor the entry's folder changes.
        std::fs::remove_file(dir.join("scenes/a.ts")).unwrap();
        let after = watched_stamp(&entry, &args);

        assert!(before.is_some());
        assert_ne!(before, after);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_watcher_notices_a_file_restored_with_an_older_mtime() {
        let dir = scene_folder("watch-restore");
        let entry = dir.join("film.tsx");
        let module = dir.join("scenes/a.ts");
        let args = PropertyArgs::default();
        let modified = std::fs::metadata(&module).unwrap().modified().unwrap();

        let before = watched_stamp(&entry, &args);
        // What `cp -p` of an older copy does: new text, an older mtime.
        std::fs::write(&module, "export const x = 2;").unwrap();
        set_modified(&module, modified - Duration::from_secs(3_600));
        let after = watched_stamp(&entry, &args);

        assert_ne!(before, after);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_watcher_sample_is_the_same_while_nothing_changes() {
        let dir = scene_folder("watch-same");
        let entry = dir.join("film.tsx");
        let args = PropertyArgs::default();

        assert_eq!(watched_stamp(&entry, &args), watched_stamp(&entry, &args));
        // Another spelling of the same folder, such as a relative path on
        // the command line and the absolute entry later.
        let name = dir.file_name().unwrap();
        let spelled = dir.join("..").join(name).join("film.tsx");
        assert_eq!(watched_stamp(&spelled, &args), watched_stamp(&entry, &args));
        assert_eq!(watched_stamp(&dir.join("missing/film.tsx"), &args), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn two_samples_name_the_files_changed_added_and_removed() {
        let dir = scene_folder("watch-changed");
        let entry = dir.join("film.tsx");
        let args = PropertyArgs::default();
        let before = watched_stamp(&entry, &args).unwrap();

        // Saved outside Celesta with another length, so the change shows
        // whatever the filesystem's mtime granularity.
        std::fs::write(dir.join("scenes/a.ts"), "export const x = 10;").unwrap();
        std::fs::write(dir.join("scenes/c.ts"), "export const x = 1;").unwrap();
        std::fs::remove_file(dir.join("scenes/b.ts")).unwrap();
        let after = watched_stamp(&entry, &args).unwrap();

        // The `scenes` folder changed too, but its files are what is named.
        assert_eq!(
            before.changed_files(&after),
            ["scenes/a.ts", "scenes/b.ts", "scenes/c.ts"].map(PathBuf::from)
        );
        assert_eq!(after.changed_files(&after.clone()), Vec::<PathBuf>::new());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_replaced_by_a_folder_of_its_name_is_named() {
        let dir = scene_folder("watch-replaced");
        let entry = dir.join("film.tsx");
        let args = PropertyArgs::default();
        let before = watched_stamp(&entry, &args).unwrap();

        // A path that is a folder in both samples is not named, but one that
        // held a module (with choices made in it) is.
        std::fs::remove_file(dir.join("scenes/a.ts")).unwrap();
        std::fs::create_dir(dir.join("scenes/a.ts")).unwrap();
        let after = watched_stamp(&entry, &args).unwrap();

        assert_eq!(before.changed_files(&after), [PathBuf::from("scenes/a.ts")]);
        assert_eq!(after.changed_files(&before), [PathBuf::from("scenes/a.ts")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn changed_sources_are_real_paths_under_the_entry_folder() {
        let dir = scene_folder("watch-real");
        let entry = dir.join("film.tsx");
        let args = PropertyArgs::default();
        let before = watched_stamp(&entry, &args);
        std::fs::write(dir.join("scenes/a.ts"), "export const x = 10;").unwrap();
        let after = watched_stamp(&entry, &args);

        // The temporary folder is behind a symlink on macOS (`/var`); the
        // bundler, and so the elements' positions, name the real path.
        let real = dunce::canonicalize(&dir).unwrap();
        assert_eq!(
            changed_sources(&entry, before.as_ref(), after.as_ref()),
            Some(vec![real.join("scenes/a.ts")])
        );
        // Without the sample the loaded code was read from, which files
        // changed is not known.
        assert_eq!(changed_sources(&entry, None, after.as_ref()), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_props_file_change_names_no_source() {
        let dir = scene_folder("watch-props");
        let entry = dir.join("film.tsx");
        // Outside the entry's folder, so only the --props-file sample has it.
        let props =
            std::env::temp_dir().join(format!("celesta-watch-props-{}.json", std::process::id()));
        std::fs::write(&props, "{}").unwrap();
        let args = PropertyArgs::new(Some(props.clone()), None);
        let before = watched_stamp(&entry, &args).unwrap();

        std::fs::write(&props, r#"{"title":"Another"}"#).unwrap();
        let after = watched_stamp(&entry, &args).unwrap();

        // It reloads, and no element's position moved.
        assert_ne!(before, after);
        assert_eq!(before.changed_files(&after), Vec::<PathBuf>::new());
        std::fs::remove_file(&props).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_watcher_notices_props_json_deletion() {
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

        let before = watched_stamp(&entry, &args);
        tick();
        std::fs::remove_file(dir.join("props.json")).unwrap();
        let after = watched_stamp(&entry, &args);

        assert_ne!(before, after);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
