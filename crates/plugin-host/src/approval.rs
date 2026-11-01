//! Which plugins may run without asking the user first. Plugins from the
//! user's folder and from a project run only once the user allowed them as
//! they are now: a plugin's fingerprint covers its manifest and every file
//! the manifest names, so an update or a replaced file is asked about
//! again. Plugins named explicitly and the ones bundled with Celesta are not
//! asked about. A file downloaded from the internet is marked so the
//! question can say so (the Mark of the Web on Windows).

use crate::locations::PluginSource;
use crate::manifest::{InstalledPlugins, MANIFEST, PluginEntry};
use crate::settings::{Allowance, UserSettings};
use sha2::{Digest, Sha256};
use std::io;
use std::path::Path;

impl PluginSource {
    /// Whether plugins from here run only once the user allowed them.
    pub fn needs_allowance(self) -> bool {
        matches!(self, Self::Project | Self::User)
    }
}

/// `sha256:` and the hash of `entry`'s manifest and of every file it names
/// that exists, each with its path, in a fixed order.
pub fn fingerprint(entry: &PluginEntry) -> io::Result<String> {
    let manifest = &entry.manifest;
    let mut named: Vec<&String> = manifest
        .executable
        .values()
        .chain(&manifest.module)
        .chain(&manifest.types)
        .collect();
    named.sort();
    named.dedup();
    let mut hash = Sha256::new();
    let mut add = |name: &str, contents: &[u8]| {
        // Lengths first, so no two different lists hash alike.
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((contents.len() as u64).to_le_bytes());
        hash.update(contents);
    };
    add(MANIFEST, &std::fs::read(entry.directory.join(MANIFEST))?);
    for name in named {
        match std::fs::read(entry.directory.join(name)) {
            Ok(contents) => add(name, &contents),
            Err(error) if error.kind() == io::ErrorKind::NotFound => add(name, b""),
            Err(error) => return Err(error),
        }
    }
    Ok(format!("sha256:{:x}", hash.finalize()))
}

/// Whether `path` was downloaded from the internet, as far as the platform
/// records it: on Windows, a Mark of the Web (`Zone.Identifier`) naming the
/// internet or an untrusted zone. Elsewhere, and for files without the mark,
/// `false`.
pub fn from_the_internet(path: &Path) -> bool {
    #[cfg(windows)]
    {
        let mut stream = path.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        std::fs::read_to_string(stream).is_ok_and(|text| zone_is_internet(&text))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// Whether a `Zone.Identifier` stream's text names zone 3 (internet) or 4
/// (restricted).
pub fn zone_is_internet(text: &str) -> bool {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("ZoneId="))
        .any(|zone| matches!(zone.trim(), "3" | "4"))
}

/// A plugin that waits for the user to allow it.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingPlugin {
    pub entry: PluginEntry,
    pub fingerprint: String,
    /// Allowed once, but changed since.
    pub changed: bool,
    /// Its manifest or a file it names came from the internet.
    pub from_the_internet: bool,
}

impl PendingPlugin {
    fn new(entry: PluginEntry, fingerprint: String, changed: bool) -> Self {
        let manifest = &entry.manifest;
        let from_the_internet = std::iter::once(MANIFEST)
            .chain(manifest.executable.values().map(String::as_str))
            .chain(manifest.module.as_deref())
            .chain(manifest.types.as_deref())
            .any(|name| from_the_internet(&entry.directory.join(name)));
        Self {
            entry,
            fingerprint,
            changed,
            from_the_internet,
        }
    }
}

/// What may run of what was found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Allowed {
    /// What may run now.
    pub plugins: InstalledPlugins,
    /// What waits for the user, in search order.
    pub pending: Vec<PendingPlugin>,
    /// Plugins whose files could not be read to fingerprint them, and why.
    pub unreadable: Vec<String>,
}

impl InstalledPlugins {
    /// Sorts these into the plugins `settings` allows to run and the ones
    /// that wait for the user (see [`PluginSource::needs_allowance`]). A
    /// waiting plugin does not run, and does not give way to a later plugin
    /// with its id either.
    pub fn allowed_by(self, settings: &UserSettings) -> Allowed {
        let mut allowed = Allowed::default();
        for entry in self.plugins {
            if !entry.source.needs_allowance() {
                allowed.plugins.plugins.push(entry);
                continue;
            }
            let fingerprint = match fingerprint(&entry) {
                Ok(fingerprint) => fingerprint,
                Err(error) => {
                    allowed.unreadable.push(format!(
                        "{} was skipped: its files could not be read: {error}",
                        entry.directory.display()
                    ));
                    continue;
                }
            };
            match settings.allowance(&entry.manifest.id, &entry.directory, &fingerprint) {
                Allowance::Allowed => allowed.plugins.plugins.push(entry),
                Allowance::Changed => {
                    allowed
                        .pending
                        .push(PendingPlugin::new(entry, fingerprint, true))
                }
                Allowance::New => {
                    allowed
                        .pending
                        .push(PendingPlugin::new(entry, fingerprint, false))
                }
            }
        }
        allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locations::SearchPath;
    use crate::manifest::discover_in;

    fn plugin(directory: &Path, id: &str) {
        std::fs::create_dir_all(directory.join("js")).unwrap();
        std::fs::write(
            directory.join(MANIFEST),
            serde_json::json!({ "id": id, "version": "1", "protocol": 1, "module": "js/index.js" })
                .to_string(),
        )
        .unwrap();
        std::fs::write(directory.join("js/index.js"), "export {};").unwrap();
    }

    fn found(root: &Path, source: PluginSource) -> InstalledPlugins {
        let (plugins, errors) = discover_in(&[SearchPath {
            path: root.to_path_buf(),
            source,
        }]);
        assert!(errors.is_empty(), "{errors:?}");
        InstalledPlugins { plugins }
    }

    #[test]
    fn fingerprints_change_with_the_files() {
        let root = tempfile::tempdir().unwrap();
        plugin(&root.path().join("p"), "p");
        let entry = || found(root.path(), PluginSource::User).plugins.remove(0);
        let first = fingerprint(&entry()).unwrap();
        assert!(first.starts_with("sha256:"));
        assert_eq!(fingerprint(&entry()).unwrap(), first);
        std::fs::write(root.path().join("p/js/index.js"), "export const x = 1;").unwrap();
        assert_ne!(fingerprint(&entry()).unwrap(), first);
    }

    #[test]
    fn user_and_project_plugins_wait_until_allowed() {
        let root = tempfile::tempdir().unwrap();
        plugin(&root.path().join("p"), "p");
        let mut settings = UserSettings::default();

        for source in [PluginSource::Explicit, PluginSource::Bundled] {
            let allowed = found(root.path(), source).allowed_by(&settings);
            assert_eq!(allowed.plugins.plugins.len(), 1, "{source:?}");
            assert!(allowed.pending.is_empty());
        }
        for source in [PluginSource::User, PluginSource::Project] {
            let allowed = found(root.path(), source).allowed_by(&settings);
            assert!(allowed.plugins.plugins.is_empty(), "{source:?}");
            assert_eq!(allowed.pending.len(), 1);
            assert!(!allowed.pending[0].changed);
        }

        let pending = found(root.path(), PluginSource::User)
            .allowed_by(&settings)
            .pending
            .remove(0);
        settings.allow("p", &pending.entry.directory, &pending.fingerprint);
        let allowed = found(root.path(), PluginSource::User).allowed_by(&settings);
        assert_eq!(allowed.plugins.plugins.len(), 1);

        // Changed since it was allowed: asked again, as a change.
        std::fs::write(root.path().join("p/js/index.js"), "export const x = 1;").unwrap();
        let allowed = found(root.path(), PluginSource::User).allowed_by(&settings);
        assert!(allowed.plugins.plugins.is_empty());
        assert!(allowed.pending[0].changed);
    }

    #[test]
    fn the_mark_of_the_web_names_internet_zones() {
        assert!(zone_is_internet(
            "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://example.com/\r\n"
        ));
        assert!(zone_is_internet("[ZoneTransfer]\nZoneId=4\n"));
        assert!(!zone_is_internet("[ZoneTransfer]\nZoneId=1\n"));
        assert!(!zone_is_internet("[ZoneTransfer]\nZoneId=2\n"));
        assert!(!zone_is_internet(""));
    }

    #[cfg(windows)]
    #[test]
    fn a_marked_file_is_from_the_internet() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("index.js");
        std::fs::write(&file, "").unwrap();
        assert!(!from_the_internet(&file));
        let mut stream = file.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        std::fs::write(stream, "[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
        assert!(from_the_internet(&file));
    }
}
