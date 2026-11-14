//! `celesta.lock`: the registry plugins a project resolved, at exact
//! versions, with the hashes of what was downloaded and installed, so every
//! machine that opens the project gets the same plugins. It sits beside
//! `celesta.config.json` and is committed with the project, written whole
//! and replaced atomically. It keeps keys this build does not know so that
//! a newer build's lock survives a round trip.

use crate::manifest::CELESTA_VERSION;
use celesta_registry_format::{Hash, Target};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// The lock's file name, in the project's root folder.
pub const LOCK_FILE: &str = "celesta.lock";

/// The lock format this build reads and writes.
pub const LOCK_FORMAT: u32 = 1;

/// What a project resolved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lockfile {
    /// The Celesta it was resolved with.
    pub celesta: semver::Version,
    pub plugins: BTreeMap<String, LockedPlugin>,
    /// The project's targets, sorted.
    pub targets: Vec<Target>,
    /// Keys a newer build wrote, kept as they were.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// One locked plugin: an exact version from one registry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LockedPlugin {
    /// The Celesta versions the version works with, so a minor upgrade of
    /// Celesta is seen without the registry.
    pub celesta: semver::VersionReq,
    /// The registry's base URL, such as `https://registry.celesta.studio`.
    pub registry: String,
    /// Every target the version has.
    pub targets: BTreeMap<Target, LockedArtifact>,
    pub version: semver::Version,
    /// Keys a newer build wrote, kept as they were.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// The hashes of one target's download.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedArtifact {
    /// The archive as downloaded.
    pub sha256: Hash,
    /// The installed folder (see `celesta_registry_format::tree_hash`).
    pub tree_hash: Hash,
}

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path} is not a Celesta lock: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "{path} was written in lock format {found}, which this Celesta {CELESTA_VERSION} does not read; update Celesta"
    )]
    Format { path: PathBuf, found: u64 },
    #[error("could not write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// A lock as written: `format` first, then the lock's own keys.
#[derive(Serialize)]
struct Written<'a> {
    format: u32,
    #[serde(flatten)]
    lock: &'a Lockfile,
}

impl Lockfile {
    /// Reads a lock from `bytes`, which came from `path`.
    pub fn parse(bytes: &[u8], path: &Path) -> Result<Self, LockError> {
        let parse = |source| LockError::Parse {
            path: path.to_path_buf(),
            source,
        };
        let Value::Object(mut object) = serde_json::from_slice(bytes).map_err(parse)? else {
            return Err(parse(serde::de::Error::custom(
                "the lock is not a JSON object",
            )));
        };
        // Judged before the rest, so a newer format's other shapes are not
        // reported as parse errors.
        let format = object
            .remove("format")
            .ok_or_else(|| parse(serde::de::Error::missing_field("format")))?;
        let found = format.as_u64().ok_or_else(|| {
            parse(serde::de::Error::custom(
                "format is not a non-negative whole number",
            ))
        })?;
        if found != u64::from(LOCK_FORMAT) {
            return Err(LockError::Format {
                path: path.to_path_buf(),
                found,
            });
        }
        serde_json::from_value(Value::Object(object)).map_err(parse)
    }

    /// Reads the project's lock in `root`; `None` when there is none.
    pub fn load(root: &Path) -> Result<Option<Self>, LockError> {
        let path = root.join(LOCK_FILE);
        match std::fs::read(&path) {
            Ok(bytes) => Self::parse(&bytes, &path).map(Some),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(LockError::Read { path, source }),
        }
    }

    /// The lock as written to disk: indented by two spaces, keys in order,
    /// one trailing newline, so that the same lock gives the same bytes and
    /// version control shows only real changes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut lock = self.clone();
        lock.targets.sort();
        let written = Written {
            format: LOCK_FORMAT,
            lock: &lock,
        };
        let mut text =
            serde_json::to_string_pretty(&written).expect("a lock serializes to JSON text");
        text.push('\n');
        text.into_bytes()
    }

    /// Writes the lock to `root`, through a temporary file renamed over
    /// it, so a reader never sees half of it. Unlike the user's settings it
    /// has ordinary permissions: the project commits it.
    pub fn save(&self, root: &Path) -> Result<(), LockError> {
        let path = root.join(LOCK_FILE);
        let write = |source| LockError::Write {
            path: path.clone(),
            source,
        };
        let temporary = root.join(format!("{LOCK_FILE}.{}.tmp", std::process::id()));
        std::fs::write(&temporary, self.to_bytes()).map_err(write)?;
        std::fs::rename(&temporary, &path).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            write(error)
        })
    }

    /// The ids whose locked range leaves `celesta` out, sorted.
    pub fn unloadable(&self, celesta: &semver::Version) -> Vec<String> {
        self.plugins
            .iter()
            .filter(|(_, locked)| !locked.celesta.matches(celesta))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The locked plugin's artifact for `host`, else the one for `any`.
    pub fn artifact(&self, id: &str, host: &Target) -> Option<&LockedArtifact> {
        let targets = &self.plugins.get(id)?.targets;
        targets.get(host).or_else(|| targets.get(&Target::ANY))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: u8) -> Hash {
        Hash([byte; 32])
    }

    fn target(name: &str) -> Target {
        Target::parse(name).unwrap()
    }

    fn plugin(version: &str, range: &str, targets: &[(&str, u8)]) -> LockedPlugin {
        LockedPlugin {
            celesta: range.parse().unwrap(),
            registry: "https://registry.celesta.studio".to_owned(),
            targets: targets
                .iter()
                .map(|(name, byte)| {
                    (
                        target(name),
                        LockedArtifact {
                            sha256: hash(*byte),
                            tree_hash: hash(*byte + 1),
                        },
                    )
                })
                .collect(),
            version: version.parse().unwrap(),
            other: Map::new(),
        }
    }

    fn lock(plugins: Vec<(&str, LockedPlugin)>) -> Lockfile {
        Lockfile {
            celesta: "0.9.0".parse().unwrap(),
            targets: vec![target("linux-x86_64"), target("macos-aarch64")],
            plugins: plugins
                .into_iter()
                .map(|(id, plugin)| (id.to_owned(), plugin))
                .collect(),
            other: Map::new(),
        }
    }

    fn path() -> PathBuf {
        PathBuf::from("celesta.lock")
    }

    #[test]
    fn a_lock_round_trips() {
        let original = lock(vec![
            (
                "acme/charts",
                plugin("1.2.3", ">=0.9.0, <0.10.0", &[("any", 1)]),
            ),
            (
                "acme/video",
                plugin(
                    "0.4.0",
                    ">=0.9.0, <0.10.0",
                    &[("linux-x86_64", 10), ("macos-aarch64", 20)],
                ),
            ),
        ]);
        let bytes = original.to_bytes();
        let parsed = Lockfile::parse(&bytes, &path()).unwrap();
        assert_eq!(parsed, original);
        assert_eq!(parsed.to_bytes(), bytes);
    }

    #[test]
    fn the_bytes_are_stable() {
        let one = lock(vec![(
            "acme/charts",
            plugin("1.2.3", ">=0.9.0, <0.10.0", &[("any", 1)]),
        )]);
        let one_hash = "sha256:0101010101010101010101010101010101010101010101010101010101010101";
        let two_hash = "sha256:0202020202020202020202020202020202020202020202020202020202020202";
        let expected = format!(
            r#"{{
  "format": 1,
  "celesta": "0.9.0",
  "plugins": {{
    "acme/charts": {{
      "celesta": ">=0.9.0, <0.10.0",
      "registry": "https://registry.celesta.studio",
      "targets": {{
        "any": {{
          "sha256": "{one_hash}",
          "treeHash": "{two_hash}"
        }}
      }},
      "version": "1.2.3"
    }}
  }},
  "targets": [
    "linux-x86_64",
    "macos-aarch64"
  ]
}}
"#
        );
        assert_eq!(String::from_utf8(one.to_bytes()).unwrap(), expected);
    }

    #[test]
    fn unknown_fields_are_kept() {
        let mut text = String::from_utf8(
            lock(vec![(
                "acme/charts",
                plugin("1.2.3", ">=0.9.0, <0.10.0", &[("any", 1)]),
            )])
            .to_bytes(),
        )
        .unwrap();
        text = text.replacen(
            "\"format\": 1,",
            "\"format\": 1,\n  \"future\": {\"a\": [1, 2]},",
            1,
        );
        text = text.replacen(
            "\"registry\"",
            "\"pinnedBy\": \"someone\",\n      \"registry\"",
            1,
        );
        let parsed = Lockfile::parse(text.as_bytes(), &path()).unwrap();
        assert_eq!(parsed.other["future"], serde_json::json!({"a": [1, 2]}));
        assert!(!parsed.other.contains_key("format"));
        assert_eq!(parsed.plugins["acme/charts"].other["pinnedBy"], "someone");
        let again: Value = serde_json::from_slice(&parsed.to_bytes()).unwrap();
        assert_eq!(again["future"], serde_json::json!({"a": [1, 2]}));
        assert_eq!(again["format"], 1);
        assert_eq!(again["plugins"]["acme/charts"]["pinnedBy"], "someone");
    }

    #[test]
    fn a_newer_format_fails_naming_it() {
        let error = Lockfile::parse(br#"{"format": 2, "other": "shape"}"#, &path()).unwrap_err();
        assert!(matches!(error, LockError::Format { found: 2, .. }));
        assert!(error.to_string().contains("lock format 2"), "{error}");
    }

    #[test]
    fn a_missing_format_is_a_parse_error() {
        let error = Lockfile::parse(b"{}", &path()).unwrap_err();
        assert!(matches!(error, LockError::Parse { .. }), "{error}");
    }

    #[test]
    fn crlf_is_read() {
        let original = lock(vec![(
            "acme/charts",
            plugin("1.2.3", ">=0.9.0, <0.10.0", &[("any", 1)]),
        )]);
        let text = String::from_utf8(original.to_bytes())
            .unwrap()
            .replace('\n', "\r\n");
        assert_eq!(Lockfile::parse(text.as_bytes(), &path()).unwrap(), original);
    }

    #[test]
    fn unloadable_lists_ids_outside_their_range() {
        let locked = lock(vec![
            ("b/two", plugin("1.0.0", ">=0.9.0, <0.10.0", &[("any", 1)])),
            ("a/one", plugin("1.0.0", ">=0.9.0, <0.10.0", &[("any", 1)])),
            ("c/new", plugin("1.0.0", ">=0.10.0, <0.11.0", &[("any", 1)])),
        ]);
        assert_eq!(
            locked.unloadable(&"0.10.0".parse().unwrap()),
            ["a/one", "b/two"]
        );
        assert_eq!(locked.unloadable(&"0.9.5".parse().unwrap()), ["c/new"]);
    }

    #[test]
    fn artifact_falls_back_to_any() {
        let locked = lock(vec![
            (
                "a/native",
                plugin("1.0.0", "*", &[("linux-x86_64", 1), ("any", 5)]),
            ),
            ("a/portable", plugin("1.0.0", "*", &[("any", 9)])),
        ]);
        let host = target("linux-x86_64");
        assert_eq!(locked.artifact("a/native", &host).unwrap().sha256, hash(1));
        assert_eq!(
            locked
                .artifact("a/native", &target("macos-aarch64"))
                .unwrap()
                .sha256,
            hash(5)
        );
        assert_eq!(
            locked.artifact("a/portable", &host).unwrap().sha256,
            hash(9)
        );
        assert!(locked.artifact("a/missing", &host).is_none());
    }

    #[test]
    fn saved_locks_load_and_replace_the_old_one() {
        let root = tempfile::tempdir().unwrap();
        assert!(Lockfile::load(root.path()).unwrap().is_none());
        let first = lock(vec![]);
        first.save(root.path()).unwrap();
        assert_eq!(Lockfile::load(root.path()).unwrap(), Some(first));
        let second = lock(vec![("a/b", plugin("1.0.0", "*", &[("any", 1)]))]);
        second.save(root.path()).unwrap();
        assert_eq!(Lockfile::load(root.path()).unwrap(), Some(second));
        let names: Vec<_> = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [std::ffi::OsString::from(LOCK_FILE)]);
    }
}
