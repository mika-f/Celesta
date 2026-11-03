//! Plugin directories: a `celesta-plugin.json` and what it names.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The file that makes a directory a plugin.
pub const MANIFEST: &str = "celesta-plugin.json";

/// The environment variable listing where plugins are, as the platform's
/// path list (`;` on Windows, `:` elsewhere).
pub const PLUGIN_PATH_ENV: &str = "CELESTA_PLUGIN_PATH";

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

/// Why `manifest` cannot be used from `directory`, if it cannot: a bad id,
/// or a path that leads outside the directory, by its text or through a
/// link. Files that do not exist yet are left to fail where they are used.
fn invalid_reason(manifest: &Manifest, directory: &Path) -> Option<String> {
    if !is_valid_id(&manifest.id) {
        return Some(format!(
            "its id {:?} may use only lowercase letters, digits, '-', '_', and '.', \
             and must start with a letter or digit",
            manifest.id
        ));
    }
    let named = manifest
        .executable
        .values()
        .chain(&manifest.module)
        .chain(&manifest.types);
    let root = std::fs::canonicalize(directory).ok();
    for path in named {
        if !is_contained_path(path) {
            return Some(format!(
                "{path:?} is not a relative path inside the plugin directory"
            ));
        }
        let resolved = std::fs::canonicalize(directory.join(path)).ok();
        if let (Some(root), Some(resolved)) = (&root, resolved)
            && !resolved.starts_with(root)
        {
            return Some(format!("{path:?} leads outside the plugin directory"));
        }
    }
    None
}

fn read_manifest(directory: &Path) -> Result<Option<PluginEntry>, DiscoveryError> {
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
    if let Some(reason) = invalid_reason(&manifest, directory) {
        return Err(DiscoveryError::Invalid { path, reason });
    }
    Ok(Some(PluginEntry {
        manifest,
        directory: directory.to_path_buf(),
    }))
}

/// The plugins under `paths`: each is an absolute path to a plugin
/// directory or a directory of them. The first plugin found with an id
/// wins. Relative paths, unreadable manifests, and manifests that name a bad
/// id or a file outside their directory are reported, not fatal.
pub fn discover(paths: &[PathBuf]) -> (Vec<PluginEntry>, Vec<DiscoveryError>) {
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
    for path in paths {
        if !path.is_absolute() {
            add(
                Err(DiscoveryError::RelativePath { path: path.clone() }),
                &mut found,
            );
            continue;
        }
        if path.join(MANIFEST).is_file() {
            add(read_manifest(path), &mut found);
            continue;
        }
        let Ok(children) = std::fs::read_dir(path) else {
            continue;
        };
        let mut children: Vec<PathBuf> = children
            .filter_map(|child| child.ok().map(|child| child.path()))
            .filter(|child| child.is_dir())
            .collect();
        children.sort();
        for child in children {
            add(read_manifest(&child), &mut found);
        }
    }
    (found, errors)
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
            },
            directory: PathBuf::from("plugin"),
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
    fn makes_extra_paths_absolute() {
        let paths = plugin_paths(&[PathBuf::from("plugins")]);
        assert_eq!(paths[0], std::env::current_dir().unwrap().join("plugins"));
    }
}
