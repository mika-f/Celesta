//! Plugin directories: a `celesta-plugin.json` and what it names.

use crate::locations::{PluginSource, SearchPath, search_paths, unsafe_reason};
use crate::settings::UserSettings;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The file that makes a directory a plugin.
pub const MANIFEST: &str = "celesta-plugin.json";

/// The environment variable listing where plugins are, as the platform's
/// path list (`;` on Windows, `:` elsewhere).
pub const PLUGIN_PATH_ENV: &str = "CELESTA_PLUGIN_PATH";

/// The environment variable listing, like [`PLUGIN_PATH_ENV`], the plugins
/// that wait for the user's approval. The React runtime resolves their
/// imports to stand-ins that run none of their code.
pub const WAITING_PLUGIN_PATH_ENV: &str = "CELESTA_WAITING_PLUGIN_PATH";

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub protocol: u32,
    /// The plugin process, relative to the plugin directory, by platform:
    /// `"<os>-<arch>"` (such as `windows-x86_64` or `macos-aarch64`) or the
    /// OS alone for any architecture. See [`platform`].
    #[serde(default)]
    pub executable: BTreeMap<String, String>,
    /// The JavaScript module entries import as `@celesta-plugin/<id>`.
    #[serde(default)]
    pub module: Option<String>,
    /// Its declarations, for project TypeScript support.
    #[serde(default)]
    pub types: Option<String>,
    /// Components of `module` the editor's Add menu offers.
    #[serde(default)]
    pub components: Vec<ComponentInfo>,
}

/// A component a plugin's module exports, as the editor's Add menu offers
/// it: drawn on the canvas like an Image or a Video when it takes a file as
/// `src` (`src`, `x`, `y`, `width`), else like a Rect (`x`, `y`, `width`,
/// `height`), and written with `props` after those.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComponentInfo {
    /// The export's name: a JSX component name such as `Flag`.
    pub name: String,
    /// What the Add menu says; the name when absent.
    #[serde(default)]
    pub label: Option<String>,
    /// The kind of file the component takes as `src`, chosen when it is
    /// added.
    #[serde(default)]
    pub src: Option<ComponentSource>,
    /// Props the component is added with: strings, numbers, and booleans.
    #[serde(default)]
    pub props: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ComponentSource {
    Image,
    Video,
}

impl ComponentInfo {
    /// What the Add menu says.
    pub fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }

    /// The props the editor writes from the drawn box, which `props` may not
    /// set again.
    pub fn drawn_props(&self) -> &'static [&'static str] {
        match self.src {
            Some(_) => &["src", "x", "y", "width"],
            None => &["x", "y", "width", "height"],
        }
    }

    /// Why the component cannot be offered, if it cannot.
    fn invalid_reason(&self) -> Option<String> {
        let name = &self.name;
        let mut chars = name.chars();
        if !(chars.next().is_some_and(|c| c.is_ascii_uppercase())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_'))
        {
            return Some(format!(
                "component {name:?} is not a component name (a capital letter, then letters, digits, or '_')"
            ));
        }
        for (prop, value) in &self.props {
            let mut chars = prop.chars();
            let is_name = chars
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !is_name || self.drawn_props().contains(&prop.as_str()) {
                return Some(format!("component {name:?} cannot set the prop {prop:?}"));
            }
            if !(value.is_string() || value.is_number() || value.is_boolean()) {
                return Some(format!(
                    "component {name:?}'s prop {prop:?} is not a string, number, or boolean"
                ));
            }
        }
        None
    }
}

/// This platform as manifests name it: `(os, arch)` in Rust's names, such
/// as `windows`, `macos`, or `linux` and `x86_64` or `aarch64`.
pub const fn platform() -> (&'static str, &'static str) {
    (std::env::consts::OS, std::env::consts::ARCH)
}

/// A plugin found on disk.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginEntry {
    pub manifest: Manifest,
    pub directory: PathBuf,
    /// Which search path it was found under.
    pub source: PluginSource,
}

impl PluginEntry {
    /// This platform's plugin process, if the plugin has one: the entry for
    /// this OS and architecture, else for this OS.
    pub fn executable(&self) -> Option<PathBuf> {
        let (os, arch) = platform();
        let executables = &self.manifest.executable;
        executables
            .get(&format!("{os}-{arch}"))
            .or_else(|| executables.get(os))
            .map(|relative| self.directory.join(relative))
    }

    pub fn module(&self) -> Option<PathBuf> {
        self.manifest
            .module
            .as_ref()
            .map(|module| self.directory.join(module))
    }

    pub fn types(&self) -> Option<PathBuf> {
        self.manifest
            .types
            .as_ref()
            .map(|types| self.directory.join(types))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a plugin manifest: {source}")]
    Manifest {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path} was skipped: {reason}")]
    Invalid { path: PathBuf, reason: String },
    #[error("plugin path {path} was skipped: it is not an absolute path")]
    RelativePath { path: PathBuf },
    #[error("the user's settings were not used: {0}")]
    Settings(String),
}

/// Whether `id` can name a plugin: lowercase ASCII letters, digits, `-`,
/// `_`, and `.`, starting with a letter or digit. That keeps
/// `@celesta-plugin/<id>` one specifier segment and `<id>` one path
/// component wherever Celesta files things under it.
pub fn is_valid_id(id: &str) -> bool {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let mut chars = id.chars();
    chars.next().is_some_and(allowed) && chars.all(|c| allowed(c) || matches!(c, '-' | '_' | '.'))
}

/// Whether a path a manifest names stays inside the plugin directory on
/// every platform: relative, with no `..` part, drive, or `:` (which would
/// also name an NTFS stream). Either separator counts, since one manifest
/// serves every platform.
pub fn is_contained_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with(['/', '\\'])
        && !path.contains([':', '\0'])
        && path.split(['/', '\\']).all(|part| part != "..")
}

/// Where to look for plugins: `extra` first, made absolute against the
/// current directory, then `CELESTA_PLUGIN_PATH`. [`discover`] skips a
/// relative entry of the latter, whose folder would depend on where Celesta
/// happens to be started, as `.` on `PATH` does.
pub fn plugin_paths(extra: &[PathBuf]) -> Vec<PathBuf> {
    let from_env = std::env::var_os(PLUGIN_PATH_ENV).unwrap_or_default();
    extra
        .iter()
        .map(|path| std::path::absolute(path).unwrap_or_else(|_| path.clone()))
        .chain(std::env::split_paths(&from_env).filter(|path| !path.as_os_str().is_empty()))
        .collect()
}

/// Why `manifest` cannot be used from `directory`, if it cannot: a bad id, a
/// component that cannot be offered (or components without a module), or a
/// path that leads outside the directory, by its text or through a link.
/// Files that do not exist yet are left to fail where they are used.
fn invalid_reason(manifest: &Manifest, directory: &Path) -> Option<String> {
    if !is_valid_id(&manifest.id) {
        return Some(format!(
            "its id {:?} may use only lowercase letters, digits, '-', '_', and '.', \
             and must start with a letter or digit",
            manifest.id
        ));
    }
    if !manifest.components.is_empty() && manifest.module.is_none() {
        return Some("it lists components but has no module to import them from".into());
    }
    if let Some(reason) = manifest
        .components
        .iter()
        .find_map(ComponentInfo::invalid_reason)
    {
        return Some(reason);
    }
    let named = manifest
        .executable
        .values()
        .chain(&manifest.module)
        .chain(&manifest.types);
    // Without the folder's own resolved path, links out of it could not be
    // told apart, so such a folder is refused rather than trusted.
    let root = match std::fs::canonicalize(directory) {
        Ok(root) => root,
        Err(error) => return Some(format!("its folder could not be resolved: {error}")),
    };
    for path in named {
        if !is_contained_path(path) {
            return Some(format!(
                "{path:?} is not a relative path inside the plugin directory"
            ));
        }
        if let Ok(resolved) = std::fs::canonicalize(directory.join(path))
            && !resolved.starts_with(&root)
        {
            return Some(format!("{path:?} leads outside the plugin directory"));
        }
    }
    None
}

/// Whether `directory`'s path would break the path list the React runtime
/// gets (`CELESTA_PLUGIN_PATH`): it holds the list's separator (`;` on
/// Windows, `:` elsewhere), or a `"`, which Windows uses to quote such
/// entries and the runtime does not read. `join_paths` cannot be relied on
/// to say so: on Windows it quotes an entry with `;` instead of failing.
fn breaks_path_list(directory: &Path) -> bool {
    let text = directory.as_os_str().to_string_lossy();
    let separator = if cfg!(windows) { ';' } else { ':' };
    text.contains(separator) || text.contains('"')
}

/// Why a plugin from `source` cannot be used, if other users could change
/// it: `directory` or anything inside it, since a program loads libraries
/// beside it and a module imports files the manifest does not name (see
/// [`unsafe_reason`]). Only checked sources are judged, and only where
/// permissions are judged at all (Unix).
fn unsafe_plugin(directory: &Path, source: PluginSource) -> Option<String> {
    if !source.is_checked() || !cfg!(unix) {
        return None;
    }
    let mut folders = vec![directory.to_path_buf()];
    while let Some(folder) = folders.pop() {
        if let Some(reason) = unsafe_reason(&folder) {
            return Some(reason);
        }
        let entries = match std::fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) => {
                return Some(format!("{} could not be read: {error}", folder.display()));
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    return Some(format!("{} could not be read: {error}", folder.display()));
                }
            };
            let path = entry.path();
            // Links are judged where they lead (`unsafe_reason` follows
            // them) but not walked into, so a link cannot loop the walk.
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => folders.push(path),
                _ => {
                    if let Some(reason) = unsafe_reason(&path) {
                        return Some(reason);
                    }
                }
            }
        }
    }
    None
}

fn read_manifest(
    directory: &Path,
    source: PluginSource,
) -> Result<Option<PluginEntry>, DiscoveryError> {
    let path = directory.join(MANIFEST);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(DiscoveryError::Io { path, source }),
    };
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|source| DiscoveryError::Manifest {
            path: path.clone(),
            source,
        })?;
    if let Some(reason) =
        invalid_reason(&manifest, directory).or_else(|| unsafe_plugin(directory, source))
    {
        return Err(DiscoveryError::Invalid { path, reason });
    }
    // The React runtime gets each plugin directory in a path list, which a
    // directory with the list's separator in its name would split.
    if breaks_path_list(directory) {
        return Err(DiscoveryError::Invalid {
            path,
            reason: "its folder's path contains the path-list separator or a quote".into(),
        });
    }
    Ok(Some(PluginEntry {
        manifest,
        directory: directory.to_path_buf(),
        source,
    }))
}

/// The plugins under `paths`, each an absolute path to a plugin directory
/// or a directory of them, all named explicitly (see [`discover_in`]).
pub fn discover(paths: &[PathBuf]) -> (Vec<PluginEntry>, Vec<DiscoveryError>) {
    let paths: Vec<_> = paths
        .iter()
        .map(|path| SearchPath {
            path: path.clone(),
            source: PluginSource::Explicit,
        })
        .collect();
    discover_in(&paths)
}

/// The plugins under `paths`, in order: each is an absolute path to a
/// plugin directory or a directory of them. The first plugin found with an
/// id wins. Relative paths, folders and manifests that cannot be read,
/// manifests that name a bad id or a file outside their directory, and,
/// from a checked source, a folder or plugin other users could change are
/// reported, not fatal; a missing folder is skipped quietly.
pub fn discover_in(paths: &[SearchPath]) -> (Vec<PluginEntry>, Vec<DiscoveryError>) {
    let mut found: Vec<PluginEntry> = Vec::new();
    let mut errors = Vec::new();
    let mut add = |entry: Result<Option<PluginEntry>, DiscoveryError>,
                   found: &mut Vec<PluginEntry>| match entry {
        Ok(Some(entry))
            if !found
                .iter()
                .any(|other| other.manifest.id == entry.manifest.id) =>
        {
            found.push(entry)
        }
        Ok(_) => {}
        Err(error) => errors.push(error),
    };
    for SearchPath { path, source } in paths {
        if !path.is_absolute() {
            add(
                Err(DiscoveryError::RelativePath { path: path.clone() }),
                &mut found,
            );
            continue;
        }
        if !path.exists() {
            if source.reports_missing() {
                add(
                    Err(DiscoveryError::Io {
                        path: path.clone(),
                        source: std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "the folder does not exist",
                        ),
                    }),
                    &mut found,
                );
            }
            continue;
        }
        if source.is_checked()
            && let Some(reason) = unsafe_reason(path)
        {
            add(
                Err(DiscoveryError::Invalid {
                    path: path.clone(),
                    reason,
                }),
                &mut found,
            );
            continue;
        }
        if path.join(MANIFEST).is_file() {
            add(read_manifest(path, *source), &mut found);
            continue;
        }
        let children = match std::fs::read_dir(path) {
            Ok(children) => children,
            Err(source) => {
                add(
                    Err(DiscoveryError::Io {
                        path: path.clone(),
                        source,
                    }),
                    &mut found,
                );
                continue;
            }
        };
        let mut children: Vec<PathBuf> = children
            .filter_map(|child| child.ok().map(|child| child.path()))
            .filter(|child| child.is_dir())
            .collect();
        children.sort();
        for child in children {
            add(read_manifest(&child, *source), &mut found);
        }
    }
    (found, errors)
}

/// The plugins found in every location Celesta searches ([`search_paths`]
/// with `explicit` first), as one set every part of Celesta shares: the
/// renderer's [`crate::PluginHost`] and the React runtime's
/// `CELESTA_PLUGIN_PATH` ([`InstalledPlugins::runtime`]) are made from the
/// same discovery, so they agree on what is installed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InstalledPlugins {
    /// What may run.
    pub plugins: Vec<PluginEntry>,
    /// What waits for the user's approval (the app's
    /// [`InstalledPlugins::allowed_by`]): never run, but the React runtime
    /// stands in for their imports so an entry using one still opens.
    pub waiting: Vec<PluginEntry>,
}

/// What a React runtime's process is told about plugins, as environment
/// variables: give it explicitly (as `Command::env` does) rather than
/// through Celesta's own environment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimePlugins {
    /// [`PLUGIN_PATH_ENV`]: the plugins it may load.
    pub path: std::ffi::OsString,
    /// [`WAITING_PLUGIN_PATH_ENV`]: the plugins it stands in for.
    pub waiting: std::ffi::OsString,
}

impl RuntimePlugins {
    /// The variables to set, by name.
    pub fn env(&self) -> [(&'static str, &std::ffi::OsStr); 2] {
        [
            (PLUGIN_PATH_ENV, &self.path),
            (WAITING_PLUGIN_PATH_ENV, &self.waiting),
        ]
    }
}

/// `entries`' directories as a path list, in order. Discovery skips
/// directories that would break the list (see `read_manifest`); a set made
/// by hand leaves them out here, so joining what remains cannot fail.
fn path_list(entries: &[PluginEntry]) -> std::ffi::OsString {
    std::env::join_paths(
        entries
            .iter()
            .map(|entry| &entry.directory)
            .filter(|directory| !breaks_path_list(directory)),
    )
    .expect("paths without the separator or a quote join")
}

impl InstalledPlugins {
    /// Discovers them, with the plugins that were skipped and why: the
    /// folders the user added in their settings are searched too, and the
    /// plugins they turned off are left out. This is what command-line tools
    /// run, without asking about anything (see
    /// [`InstalledPlugins::allowed_by`] for the app's questions).
    pub fn find(explicit: &[PathBuf]) -> (Self, Vec<DiscoveryError>) {
        let (settings, settings_error) = UserSettings::current();
        let (mut found, mut errors) = Self::find_with(explicit, &[], &settings);
        if let Some(error) = settings_error {
            errors.insert(0, DiscoveryError::Settings(error.to_string()));
        }
        found
            .plugins
            .retain(|entry| !settings.is_disabled(&entry.manifest.id, &entry.directory));
        (found, errors)
    }

    /// Discovers them with a trusted project's own plugin folders (see
    /// [`crate::project_plugin_paths`]) and the folders `settings` adds,
    /// leaving nothing out.
    pub fn find_with(
        explicit: &[PathBuf],
        project: &[PathBuf],
        settings: &UserSettings,
    ) -> (Self, Vec<DiscoveryError>) {
        let paths = search_paths(explicit, project, &settings.plugin_folders);
        let (plugins, errors) = discover_in(&paths);
        (
            Self {
                plugins,
                waiting: Vec::new(),
            },
            errors,
        )
    }

    /// Discovers them, reporting skipped plugins on stderr, for tools
    /// without a better place to show them.
    pub fn find_reporting(explicit: &[PathBuf]) -> Self {
        let (plugins, errors) = Self::find(explicit);
        for error in errors {
            eprintln!("celesta: skipped a plugin: {error}");
        }
        plugins
    }

    /// The value of `CELESTA_PLUGIN_PATH` for the React runtime: each plugin
    /// directory in search order, so the runtime resolves
    /// `@celesta-plugin/<id>` to the same plugins, without searching any
    /// folder itself.
    pub fn runtime_path(&self) -> std::ffi::OsString {
        path_list(&self.plugins)
    }

    /// Everything the React runtime is told: [`Self::runtime_path`], and
    /// the waiting plugins it stands in for.
    pub fn runtime(&self) -> RuntimePlugins {
        RuntimePlugins {
            path: self.runtime_path(),
            waiting: path_list(&self.waiting),
        }
    }

    /// A host that runs these plugins for a renderer.
    pub fn host(&self) -> crate::PluginHost {
        crate::PluginHost::new(self.plugins.iter().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_plugin(directory: &Path, id: &str, version: &str) {
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(
            directory.join(MANIFEST),
            serde_json::json!({
                "id": id,
                "version": version,
                "protocol": 1,
                "executable": { "windows": "bin/p.exe", "linux": "bin/p", "macos": "bin/p", "plan9-mips": "bin/other" },
                "module": "js/index.js",
            })
            .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn prefers_the_executable_for_this_architecture() {
        let (os, arch) = platform();
        let entry = |keys: &[(String, &str)]| PluginEntry {
            manifest: Manifest {
                id: "p".into(),
                version: "1".into(),
                protocol: 1,
                executable: keys
                    .iter()
                    .map(|(key, value)| (key.clone(), (*value).to_owned()))
                    .collect(),
                module: None,
                types: None,
                components: Vec::new(),
            },
            directory: PathBuf::from("plugin"),
            source: PluginSource::Explicit,
        };
        let any = entry(&[(os.to_owned(), "any")]);
        assert_eq!(any.executable(), Some(PathBuf::from("plugin/any")));
        let both = entry(&[(os.to_owned(), "any"), (format!("{os}-{arch}"), "this")]);
        assert_eq!(both.executable(), Some(PathBuf::from("plugin/this")));
        let other = entry(&[(format!("{os}-not-{arch}"), "other")]);
        assert_eq!(other.executable(), None);
    }

    #[test]
    fn finds_plugin_directories_and_directories_of_them() {
        let root = tempfile::tempdir().unwrap();
        let single = root.path().join("single");
        write_plugin(&single, "one", "1.0.0");
        let collection = root.path().join("collection");
        write_plugin(&collection.join("b"), "two", "1.0.0");
        write_plugin(&collection.join("a"), "one", "2.0.0");
        std::fs::create_dir_all(collection.join("c")).unwrap();
        std::fs::write(collection.join("c").join(MANIFEST), "{").unwrap();

        let (found, errors) = discover(&[
            single.clone(),
            collection.clone(),
            root.path().join("missing"),
        ]);
        // The first "one" wins over the collection's.
        let ids: Vec<_> = found
            .iter()
            .map(|entry| (entry.manifest.id.as_str(), entry.manifest.version.as_str()))
            .collect();
        assert_eq!(ids, [("one", "1.0.0"), ("two", "1.0.0")]);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(matches!(errors[0], DiscoveryError::Manifest { .. }));

        let entry = &found[0];
        let executable = entry.executable().unwrap();
        assert!(
            executable.starts_with(&single)
                && executable.ends_with(if cfg!(windows) { "bin/p.exe" } else { "bin/p" })
        );
        assert_eq!(entry.module(), Some(single.join("js/index.js")));
        assert_eq!(entry.types(), None);
    }

    #[test]
    fn ids_are_one_path_component() {
        for id in ["flag", "beep-voice", "live2d", "a.b_c", "0"] {
            assert!(is_valid_id(id), "{id}");
        }
        for id in ["", "..", ".hidden", "-x", "Flag", "a/b", "a\\b", "c:", "ü"] {
            assert!(!is_valid_id(id), "{id}");
        }
    }

    #[test]
    fn manifest_paths_stay_relative_and_inside() {
        for path in ["bin/p.exe", "./js/index.js", "a..b/c", "js\\index.js"] {
            assert!(is_contained_path(path), "{path}");
        }
        for path in [
            "",
            "../p",
            "bin/../../p",
            "bin\\..\\p",
            "/usr/bin/p",
            "\\\\server\\share\\p",
            "C:\\p.exe",
            "C:p.exe",
            "p.exe:stream",
        ] {
            assert!(!is_contained_path(path), "{path}");
        }
    }

    fn write_manifest(directory: &Path, manifest: serde_json::Value) {
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(directory.join(MANIFEST), manifest.to_string()).unwrap();
    }

    #[test]
    fn components_are_offered_only_as_written_safely() {
        let root = tempfile::tempdir().unwrap();
        let write = |name: &str, components: serde_json::Value| {
            write_manifest(
                &root.path().join(name),
                serde_json::json!({
                    "id": name, "version": "1", "protocol": 1,
                    "module": "js/index.js", "components": components,
                }),
            );
        };
        write(
            "good",
            serde_json::json!([
                { "name": "Flag", "label": "Waving flag", "src": "image", "props": { "amplitude": 0.1, "loop": true, "tone": "warm" } },
                { "name": "Box_2" },
            ]),
        );
        write("lowercase", serde_json::json!([{ "name": "flag" }]));
        write("markup", serde_json::json!([{ "name": "Flag x={1}" }]));
        write(
            "geometry",
            serde_json::json!([{ "name": "Flag", "src": "video", "props": { "width": 3 } }]),
        );
        write(
            "attribute",
            serde_json::json!([{ "name": "Flag", "props": { "on:click": 1 } }]),
        );
        write(
            "object",
            serde_json::json!([{ "name": "Flag", "props": { "style": { "a": 1 } } }]),
        );
        write_manifest(
            &root.path().join("moduleless"),
            serde_json::json!({ "id": "moduleless", "version": "1", "protocol": 1, "components": [{ "name": "Flag" }] }),
        );

        let (found, errors) = discover(&[root.path().to_path_buf()]);
        assert_eq!(found.len(), 1, "{errors:?}");
        assert_eq!(errors.len(), 6, "{errors:?}");
        let components = &found[0].manifest.components;
        assert_eq!(components[0].label(), "Waving flag");
        assert_eq!(components[0].src, Some(ComponentSource::Image));
        assert_eq!(components[0].drawn_props(), ["src", "x", "y", "width"]);
        assert_eq!(components[1].label(), "Box_2");
        assert_eq!(components[1].drawn_props(), ["x", "y", "width", "height"]);
    }

    #[test]
    fn skips_manifests_that_reach_outside_their_directory() {
        let root = tempfile::tempdir().unwrap();
        let collection = root.path().join("collection");
        let manifest = |id: &str, field: &str, path: &str| {
            let mut manifest = serde_json::json!({ "id": id, "version": "1", "protocol": 1 });
            if field == "executable" {
                manifest["executable"] = serde_json::json!({ platform().0: path });
            } else {
                manifest[field] = path.into();
            }
            manifest
        };
        write_manifest(
            &collection.join("a"),
            manifest("../escape", "module", "js/index.js"),
        );
        write_manifest(
            &collection.join("b"),
            manifest("b", "executable", "../../p"),
        );
        write_manifest(
            &collection.join("c"),
            manifest("c", "module", "/etc/index.js"),
        );
        write_manifest(
            &collection.join("d"),
            manifest("d", "types", "C:\\index.d.ts"),
        );
        write_manifest(
            &collection.join("e"),
            manifest("e", "module", "js/index.js"),
        );

        let (found, errors) = discover(&[PathBuf::from("relative"), collection]);
        let ids: Vec<_> = found
            .iter()
            .map(|entry| entry.manifest.id.as_str())
            .collect();
        assert_eq!(ids, ["e"]);
        assert!(
            matches!(errors[0], DiscoveryError::RelativePath { .. }),
            "{errors:?}"
        );
        assert_eq!(errors.len(), 5, "{errors:?}");
        assert!(
            errors[1..]
                .iter()
                .all(|error| matches!(error, DiscoveryError::Invalid { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn skips_manifests_whose_files_link_outside() {
        let root = tempfile::tempdir().unwrap();
        let outside = root.path().join("outside.js");
        std::fs::write(&outside, "").unwrap();
        let plugin = root.path().join("plugin");
        write_manifest(
            &plugin,
            serde_json::json!({ "id": "p", "version": "1", "protocol": 1, "module": "index.js" }),
        );
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, plugin.join("index.js"));
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&outside, plugin.join("index.js"));
        if linked.is_err() {
            // Windows creates symbolic links only in developer mode or as an
            // administrator.
            eprintln!("skipped: could not create a symbolic link");
            return;
        }

        let (found, errors) = discover(std::slice::from_ref(&plugin));
        assert!(found.is_empty());
        assert!(
            matches!(&errors[..], [DiscoveryError::Invalid { reason, .. }] if reason.contains("leads outside")),
            "{errors:?}"
        );
    }

    #[test]
    fn only_a_folder_the_user_added_is_missed_aloud() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("gone");
        let search = |source| {
            discover_in(&[SearchPath {
                path: missing.clone(),
                source,
            }])
            .1
        };
        for quiet in [
            PluginSource::Explicit,
            PluginSource::User,
            PluginSource::Bundled,
        ] {
            assert!(search(quiet).is_empty(), "{quiet:?}");
        }
        assert!(matches!(
            &search(PluginSource::Folder)[..],
            [DiscoveryError::Io { .. }]
        ));
    }

    #[test]
    fn the_runtime_gets_each_plugin_directory_in_order() {
        let root = tempfile::tempdir().unwrap();
        write_plugin(&root.path().join("b"), "two", "1");
        write_plugin(&root.path().join("a"), "one", "1");
        let (found, errors) = discover_in(&[SearchPath {
            path: root.path().to_path_buf(),
            source: PluginSource::User,
        }]);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(found.iter().all(|entry| entry.source == PluginSource::User));
        let installed = InstalledPlugins {
            plugins: found.clone(),
            waiting: found[1..].to_vec(),
        };
        let path: Vec<_> = std::env::split_paths(&installed.runtime_path()).collect();
        assert_eq!(path, [root.path().join("a"), root.path().join("b")]);
        let runtime = installed.runtime();
        assert_eq!(runtime.path, installed.runtime_path());
        assert_eq!(runtime.waiting, root.path().join("b").into_os_string());
        assert_eq!(
            runtime.env().map(|(name, _)| name),
            [PLUGIN_PATH_ENV, WAITING_PLUGIN_PATH_ENV]
        );
        assert_eq!(
            InstalledPlugins::default().runtime(),
            RuntimePlugins::default()
        );
    }

    #[cfg(unix)]
    #[test]
    fn checked_sources_skip_what_other_users_can_change() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let open = root.path().join("open");
        write_plugin(&open, "open", "1");
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
        let shut = root.path().join("shut");
        write_plugin(&shut, "shut", "1");
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(shut.join(MANIFEST), std::fs::Permissions::from_mode(0o644))
            .unwrap();
        let path = |source| SearchPath {
            path: root.path().to_path_buf(),
            source,
        };
        let ids = |found: &[PluginEntry]| -> Vec<String> {
            found
                .iter()
                .map(|entry| entry.manifest.id.clone())
                .collect()
        };

        let (found, errors) = discover_in(&[path(PluginSource::User)]);
        assert_eq!(ids(&found), ["shut"]);
        assert!(
            matches!(&errors[..], [DiscoveryError::Invalid { reason, .. }] if reason.contains("other users")),
            "{errors:?}"
        );
        // Folders named explicitly are the caller's choice.
        let (found, errors) = discover_in(&[path(PluginSource::Explicit)]);
        assert_eq!(ids(&found), ["open", "shut"]);
        assert!(errors.is_empty(), "{errors:?}");

        // A whole checked folder others can change is skipped at once.
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        let (found, errors) = discover_in(&[path(PluginSource::Bundled)]);
        assert!(found.is_empty());
        assert_eq!(errors.len(), 1, "{errors:?}");
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_file_the_manifest_does_not_name_is_judged_too() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let plugin = root.path().join("p");
        write_plugin(&plugin, "p", "1");
        let mode = |path: &Path, mode| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap()
        };
        for path in [root.path(), plugin.as_path()] {
            mode(path, 0o755);
        }
        mode(&plugin.join(MANIFEST), 0o644);
        // A library beside the program, which the manifest does not name.
        std::fs::create_dir_all(plugin.join("bin")).unwrap();
        mode(&plugin.join("bin"), 0o755);
        std::fs::write(plugin.join("bin/libhelper.so"), "").unwrap();
        mode(&plugin.join("bin/libhelper.so"), 0o644);
        let search = [SearchPath {
            path: root.path().to_path_buf(),
            source: PluginSource::User,
        }];
        assert_eq!(discover_in(&search).0.len(), 1);

        mode(&plugin.join("bin/libhelper.so"), 0o666);
        let (found, errors) = discover_in(&search);
        assert!(found.is_empty());
        assert!(
            matches!(&errors[..], [DiscoveryError::Invalid { reason, .. }] if reason.contains("libhelper")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_folder_that_would_break_the_runtime_path_is_skipped() {
        let root = tempfile::tempdir().unwrap();
        let separator = if cfg!(windows) { ';' } else { ':' };
        write_plugin(&root.path().join(format!("a{separator}b")), "split", "1");
        write_plugin(&root.path().join("plain"), "plain", "1");
        let (found, errors) = discover(&[root.path().to_path_buf()]);
        let ids: Vec<_> = found
            .iter()
            .map(|entry| entry.manifest.id.as_str())
            .collect();
        assert_eq!(ids, ["plain"]);
        assert!(
            matches!(&errors[..], [DiscoveryError::Invalid { reason, .. }] if reason.contains("separator")),
            "{errors:?}"
        );

        // A set made by hand with such a folder leaves it out of the path.
        let mut by_hand = InstalledPlugins {
            plugins: found,
            ..InstalledPlugins::default()
        };
        let mut split = by_hand.plugins[0].clone();
        split.directory = root.path().join(format!("a{separator}b"));
        by_hand.plugins.push(split);
        let path: Vec<_> = std::env::split_paths(&by_hand.runtime_path()).collect();
        assert_eq!(path, [root.path().join("plain")]);
    }

    #[test]
    fn makes_extra_paths_absolute() {
        let paths = plugin_paths(&[PathBuf::from("plugins")]);
        assert_eq!(paths[0], std::env::current_dir().unwrap().join("plugins"));
    }
}
