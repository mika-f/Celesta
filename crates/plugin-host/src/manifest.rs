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
}

/// Where to look for plugins: `extra` first, then `CELESTA_PLUGIN_PATH`.
pub fn plugin_paths(extra: &[PathBuf]) -> Vec<PathBuf> {
    let from_env = std::env::var_os(PLUGIN_PATH_ENV).unwrap_or_default();
    extra
        .iter()
        .cloned()
        .chain(std::env::split_paths(&from_env).filter(|path| !path.as_os_str().is_empty()))
        .collect()
}

fn read_manifest(directory: &Path) -> Result<Option<PluginEntry>, DiscoveryError> {
    let path = directory.join(MANIFEST);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(DiscoveryError::Io { path, source }),
    };
    let manifest =
        serde_json::from_str(&text).map_err(|source| DiscoveryError::Manifest { path, source })?;
    Ok(Some(PluginEntry {
        manifest,
        directory: directory.to_path_buf(),
    }))
}

/// The plugins under `paths`: each is a plugin directory or a directory of
/// them. The first plugin found with an id wins. Unreadable manifests are
/// reported, not fatal.
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
}
