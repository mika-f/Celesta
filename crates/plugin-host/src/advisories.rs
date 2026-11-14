//! The advisory list the registry client cached: a verified record of
//! plugin versions that must not run. Discovery reads it without the
//! network, so a plugin under an advisory stays stopped offline, and after
//! the key that signed the list has expired.

use crate::locations::{registry_cache_dir, unsafe_reason};
use crate::manifest::PluginEntry;
use celesta_registry_format::{Advisory, AdvisoryList};
use std::io;
use std::path::Path;
use time::OffsetDateTime;

/// The cached list's file name, in [`registry_cache_dir`].
pub const ADVISORIES_CACHE: &str = "advisories.json";

/// How old a list may be before the user is told it is stale.
const STALE_AFTER: time::Duration = time::Duration::days(7);

/// The verified list the client cached at `path`; `None` when there is
/// none. On Unix a file other users can change is refused, since it decides
/// what may run.
pub fn load_cached_advisories(path: &Path) -> Result<Option<AdvisoryList>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("could not read {}: {error}", path.display())),
    };
    if let Some(reason) = unsafe_reason(path) {
        return Err(reason);
    }
    AdvisoryList::parse(&bytes)
        .map(Some)
        .map_err(|error| format!("{} is not an advisory list: {error}", path.display()))
}

/// The cached list at its standard place, with why it could not be used
/// when it could not.
pub fn cached_advisories() -> (Option<AdvisoryList>, Option<String>) {
    let Some(folder) = registry_cache_dir() else {
        return (None, None);
    };
    match load_cached_advisories(&folder.join(ADVISORIES_CACHE)) {
        Ok(list) => (list, None),
        Err(error) => (None, Some(error)),
    }
}

/// How old `list` is when it was generated more than seven days before
/// `now`: a list that old may miss newer advisories.
pub fn advisories_stale(list: &AdvisoryList, now: OffsetDateTime) -> Option<time::Duration> {
    let age = now - list.generated;
    (age > STALE_AFTER).then_some(age)
}

/// The advisory that stops `entry`, matched by its manifest's id and its
/// version read as semver. A version that cannot be read fails closed: the
/// first advisory for the id stops it, as it may be any version.
pub fn blocking_advisory<'a>(list: &'a AdvisoryList, entry: &PluginEntry) -> Option<&'a Advisory> {
    let id = &entry.manifest.id;
    match semver::Version::parse(&entry.manifest.version) {
        Ok(version) => list.blocking(id, &version),
        Err(_) => list
            .advisories
            .iter()
            .find(|advisory| &advisory.plugin == id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locations::PluginSource;
    use crate::locations::SearchPath;
    use crate::manifest::{DiscoveryError, tests::COMPATIBLE};
    use crate::manifest::{MANIFEST, discover_in_with};
    use std::path::PathBuf;

    fn list(generated: &str, ranges: &[(&str, &str)]) -> AdvisoryList {
        let advisories: Vec<_> = ranges
            .iter()
            .enumerate()
            .map(|(n, (plugin, versions))| {
                serde_json::json!({
                    "id": format!("CEL-2026-{n:04}"),
                    "plugin": plugin,
                    "versions": versions,
                    "title": "Reads files outside the project",
                    "description": "Details.",
                    "fixed": "1.1.0",
                    "published": "2026-11-01T00:00:00Z",
                })
            })
            .collect();
        let text =
            serde_json::json!({ "format": 1, "generated": generated, "advisories": advisories });
        AdvisoryList::parse(text.to_string().as_bytes()).unwrap()
    }

    fn write_plugin(folder: &Path, id: &str, version: &str) {
        std::fs::create_dir_all(folder).unwrap();
        std::fs::write(
            folder.join(MANIFEST),
            serde_json::json!({
                "id": id,
                "version": version,
                "protocol": celesta_plugin::PROTOCOL_VERSION,
                "celesta": COMPATIBLE,
                "roles": ["content"],
            })
            .to_string(),
        )
        .unwrap();
    }

    fn search(root: &Path, source: PluginSource) -> Vec<SearchPath> {
        vec![SearchPath {
            path: root.to_path_buf(),
            source,
        }]
    }

    fn versions(found: &[PluginEntry]) -> Vec<&str> {
        found
            .iter()
            .map(|entry| entry.manifest.version.as_str())
            .collect()
    }

    #[test]
    fn a_blocked_plugin_gives_way() {
        let root = tempfile::tempdir().unwrap();
        write_plugin(&root.path().join("a"), "p", "1.0.0");
        write_plugin(&root.path().join("b"), "p", "1.1.0");
        let advisories = list("2026-11-10T00:00:00Z", &[("p", "<1.1.0")]);
        let (found, errors) = discover_in_with(
            &search(root.path(), PluginSource::User),
            |_| false,
            Some(&advisories),
        );
        assert_eq!(versions(&found), ["1.1.0"]);
        let [
            DiscoveryError::Blocked {
                id,
                version,
                advisory,
                fixed,
                ..
            },
        ] = &errors[..]
        else {
            panic!("{errors:?}");
        };
        assert_eq!((id.as_str(), version.as_str()), ("p", "1.0.0"));
        assert_eq!(advisory, "CEL-2026-0000");
        assert_eq!(fixed.as_deref(), Some("1.1.0"));
        let message = errors[0].to_string();
        assert!(message.contains("was stopped: the plugin p 1.0.0 has a security advisory (CEL-2026-0000: Reads files outside the project); 1.1.0 fixes it"), "{message}");
    }

    #[test]
    fn bundled_and_explicit_plugins_are_blocked_too() {
        let advisories = list("2026-11-10T00:00:00Z", &[("p", "<1.1.0")]);
        for source in [PluginSource::Bundled, PluginSource::Explicit] {
            let root = tempfile::tempdir().unwrap();
            write_plugin(&root.path().join("p"), "p", "1.0.0");
            let (found, errors) =
                discover_in_with(&search(root.path(), source), |_| false, Some(&advisories));
            assert!(found.is_empty(), "{source:?}");
            assert!(
                matches!(errors[..], [DiscoveryError::Blocked { .. }]),
                "{errors:?}"
            );
        }
    }

    #[test]
    fn an_unreadable_version_with_an_advisory_is_blocked() {
        let root = tempfile::tempdir().unwrap();
        write_plugin(&root.path().join("p"), "p", "1");
        write_plugin(&root.path().join("q"), "q", "1");
        let advisories = list("2026-11-10T00:00:00Z", &[("p", ">=2.0.0")]);
        let (found, errors) = discover_in_with(
            &search(root.path(), PluginSource::User),
            |_| false,
            Some(&advisories),
        );
        // `q` has no advisory, so it stays.
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].manifest.id, "q");
        assert!(
            matches!(errors[..], [DiscoveryError::Blocked { .. }]),
            "{errors:?}"
        );
    }

    #[test]
    fn stale_lists_are_noticed() {
        let now = OffsetDateTime::parse(
            "2026-11-14T00:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();
        let eight = list("2026-11-06T00:00:00Z", &[]);
        let six = list("2026-11-08T00:00:00Z", &[]);
        assert_eq!(advisories_stale(&eight, now), Some(time::Duration::days(8)));
        assert_eq!(advisories_stale(&six, now), None);
    }

    #[test]
    fn a_cache_is_read_and_a_missing_one_is_none() {
        let root = tempfile::tempdir().unwrap();
        let path: PathBuf = root.path().join(ADVISORIES_CACHE);
        assert_eq!(load_cached_advisories(&path), Ok(None));
        std::fs::write(
            &path,
            r#"{"format":1,"generated":"2026-11-10T00:00:00Z","advisories":[]}"#,
        )
        .unwrap();
        assert!(load_cached_advisories(&path).unwrap().is_some());
        std::fs::write(&path, "{").unwrap();
        assert!(load_cached_advisories(&path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_cache_others_can_write_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(ADVISORIES_CACHE);
        std::fs::write(
            &path,
            r#"{"format":1,"generated":"2026-11-10T00:00:00Z","advisories":[]}"#,
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        let error = load_cached_advisories(&path).unwrap_err();
        assert!(error.contains("other users"), "{error}");
    }
}
