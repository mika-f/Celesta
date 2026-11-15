//! Editing `plugins.dependencies` in a project's `celesta.config.json`
//! without losing anything else in the file.

use celesta_plugin_host::PROJECT_CONFIG;
use indexmap::IndexMap;
use serde::de::{DeserializeSeed, Deserializer, MapAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::value::RawValue;
use std::cell::RefCell;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path} is not a project settings file: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path} is not a project settings file: it is not a JSON object")]
    NotAnObject { path: PathBuf },
    #[error("{path}: the key {key:?} appears twice; remove one before celesta edits this file")]
    DuplicateKey { path: PathBuf, key: String },
    #[error("could not write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

type Raw = IndexMap<String, Box<RawValue>>;

/// A project's `celesta.config.json`, with `plugins.dependencies` editable
/// and every other value kept as its original text, in its original order.
#[derive(Debug, Clone)]
pub struct ProjectConfig {
    file: Raw,
    plugins: Raw,
    dependencies: IndexMap<String, String>,
}

/// Reads an object into an ordered map, refusing a repeated key: the first
/// one found is left in `duplicate`.
struct Unique<'a, T> {
    duplicate: &'a RefCell<Option<String>>,
    value: std::marker::PhantomData<T>,
}

impl<'de, T: serde::Deserialize<'de>> DeserializeSeed<'de> for Unique<'_, T> {
    type Value = IndexMap<String, T>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        struct Collect<'a, T>(&'a RefCell<Option<String>>, std::marker::PhantomData<T>);
        impl<'de, T: serde::Deserialize<'de>> Visitor<'de> for Collect<'_, T> {
            type Value = IndexMap<String, T>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
                let mut map = IndexMap::new();
                while let Some((key, value)) = access.next_entry::<String, T>()? {
                    if map.contains_key(&key) {
                        *self.0.borrow_mut() = Some(key.clone());
                        return Err(serde::de::Error::custom(format!("duplicate key {key:?}")));
                    }
                    map.insert(key, value);
                }
                Ok(map)
            }
        }
        deserializer.deserialize_map(Collect(self.duplicate, std::marker::PhantomData))
    }
}

fn parse_unique<T: serde::de::DeserializeOwned>(
    text: &str,
    path: &Path,
) -> Result<IndexMap<String, T>, ConfigError> {
    let duplicate = RefCell::new(None);
    let seed = Unique {
        duplicate: &duplicate,
        value: std::marker::PhantomData,
    };
    seed.deserialize(&mut serde_json::Deserializer::from_str(text))
        .map_err(|source| match duplicate.take() {
            Some(key) => ConfigError::DuplicateKey {
                path: path.to_path_buf(),
                key,
            },
            None => ConfigError::Parse {
                path: path.to_path_buf(),
                source,
            },
        })
}

/// Writes `bytes` over `path` through a temporary file and a rename, keeping
/// the permissions of the file it replaces.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let folder = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(folder)?;
    temporary.write_all(bytes)?;
    if let Ok(metadata) = std::fs::metadata(path) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
    }
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn empty_object() -> Box<RawValue> {
    RawValue::from_string("{}".to_owned()).expect("an empty object is JSON")
}

impl ProjectConfig {
    /// The settings of a project without the file.
    pub fn new() -> Self {
        Self {
            file: Raw::new(),
            plugins: Raw::new(),
            dependencies: IndexMap::new(),
        }
    }

    /// `root`'s settings and the exact bytes they were read from (for
    /// [`restore_config`]), or `None` when there is no such file.
    pub fn load(root: &Path) -> Result<Option<(Self, Vec<u8>)>, ConfigError> {
        let path = root.join(PROJECT_CONFIG);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(ConfigError::Read { path, source }),
        };
        let parse = |source| ConfigError::Parse {
            path: path.clone(),
            source,
        };
        let whole: Box<RawValue> = serde_json::from_slice(&bytes).map_err(parse)?;
        if !whole.get().starts_with('{') {
            return Err(ConfigError::NotAnObject { path });
        }
        let file: Raw = parse_unique(whole.get(), &path)?;
        let plugins: Raw = match file.get("plugins") {
            Some(value) => parse_unique(value.get(), &path)?,
            None => Raw::new(),
        };
        let dependencies = match plugins.get("dependencies") {
            Some(value) => parse_unique(value.get(), &path)?,
            None => IndexMap::new(),
        };
        Ok(Some((
            Self {
                file,
                plugins,
                dependencies,
            },
            bytes,
        )))
    }

    /// `id`'s requirement as written in the file.
    pub fn dependency(&self, id: &str) -> Option<&str> {
        self.dependencies.get(id).map(String::as_str)
    }

    /// Replaces `id`'s requirement in place, else appends it.
    pub fn set_dependency(&mut self, id: &str, requirement: &str) {
        self.dependencies
            .insert(id.to_owned(), requirement.to_owned());
        // The placeholders mark where the edited objects sit when written.
        self.plugins
            .entry("dependencies".to_owned())
            .or_insert_with(empty_object);
        self.file
            .entry("plugins".to_owned())
            .or_insert_with(empty_object);
    }

    /// Drops `id`; true when it was there. `dependencies` goes when it
    /// becomes empty, and `plugins` when that leaves it empty.
    pub fn remove_dependency(&mut self, id: &str) -> bool {
        if self.dependencies.shift_remove(id).is_none() {
            return false;
        }
        if self.dependencies.is_empty() {
            self.plugins.shift_remove("dependencies");
            if self.plugins.is_empty() {
                self.file.shift_remove("plugins");
            }
        }
        true
    }

    /// Writes the file as two-space JSON with one trailing newline, through
    /// a temporary file and a rename.
    pub fn save(&self, root: &Path) -> Result<(), ConfigError> {
        let path = root.join(PROJECT_CONFIG);
        let write = |source| ConfigError::Write {
            path: path.clone(),
            source,
        };
        let mut text = Vec::new();
        let mut serializer = serde_json::Serializer::with_formatter(
            &mut text,
            serde_json::ser::PrettyFormatter::new(),
        );
        Edited {
            config: self,
            level: Level::File,
        }
        .serialize(&mut serializer)
        .map_err(|error| write(error.into()))?;
        text.push(b'\n');
        write_atomic(&path, &text).map_err(write)
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
enum Level {
    File,
    Plugins,
    Dependencies,
}

/// One level of the settings, with the edited levels written from their
/// parsed form and everything else as its original text.
struct Edited<'a> {
    config: &'a ProjectConfig,
    level: Level,
}

impl Serialize for Edited<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (map, below) = match self.level {
            Level::File => (&self.config.file, Some(("plugins", Level::Plugins))),
            Level::Plugins => (
                &self.config.plugins,
                Some(("dependencies", Level::Dependencies)),
            ),
            Level::Dependencies => return self.config.dependencies.serialize(serializer),
        };
        let mut out = serializer.serialize_map(Some(map.len()))?;
        for (key, value) in map {
            match below {
                Some((name, level)) if key == name => out.serialize_entry(
                    key,
                    &Edited {
                        config: self.config,
                        level,
                    },
                )?,
                _ => out.serialize_entry(key, value)?,
            }
        }
        out.end()
    }
}

/// Puts `previous` back as `root`'s `celesta.config.json` exactly, or
/// removes the file when there was none.
pub fn restore_config(root: &Path, previous: Option<&[u8]>) -> io::Result<()> {
    restore_file(&root.join(PROJECT_CONFIG), previous)
}

/// Puts `previous` back at `path` exactly, or removes the file when there
/// was none.
pub fn restore_file(path: &Path, previous: Option<&[u8]>) -> io::Result<()> {
    match previous {
        Some(bytes) => write_atomic(path, bytes),
        None => match std::fs::remove_file(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use serde_json::value::RawValue;
    use std::path::Path;

    type Raw = IndexMap<String, Box<RawValue>>;

    fn write(root: &Path, text: &str) {
        std::fs::write(root.join("celesta.config.json"), text).unwrap();
    }

    fn read(root: &Path) -> String {
        std::fs::read_to_string(root.join("celesta.config.json")).unwrap()
    }

    fn keys(text: &str) -> Vec<String> {
        serde_json::from_str::<Raw>(text)
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    fn edit(text: &str, change: impl FnOnce(&mut ProjectConfig)) -> String {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), text);
        let (mut config, _) = ProjectConfig::load(dir.path()).unwrap().unwrap();
        change(&mut config);
        config.save(dir.path()).unwrap();
        read(dir.path())
    }

    #[test]
    fn adding_keeps_other_keys_in_order() {
        let input = r#"{"name":"x","plugins":{"paths":["p"],"dependencies":{"b":"1"}},"zeta":{"k":[1, 2]}}"#;
        let output = edit(input, |config| config.set_dependency("a", "0.2"));
        assert_eq!(keys(&output), ["name", "plugins", "zeta"]);
        let top: Raw = serde_json::from_str(&output).unwrap();
        let plugins: Raw = serde_json::from_str(top["plugins"].get()).unwrap();
        assert_eq!(
            plugins.keys().cloned().collect::<Vec<_>>(),
            ["paths", "dependencies"]
        );
        let dependencies: IndexMap<String, String> =
            serde_json::from_str(plugins["dependencies"].get()).unwrap();
        assert_eq!(dependencies.keys().cloned().collect::<Vec<_>>(), ["b", "a"]);
        assert!(output.contains("[1, 2]"));
        assert!(output.ends_with("}\n") && !output.ends_with("\n\n"));
    }

    #[test]
    fn replacing_keeps_the_position() {
        let input = r#"{"plugins":{"dependencies":{"a":"1","b":"1","c":"1"}}}"#;
        let output = edit(input, |config| config.set_dependency("b", "2"));
        assert_eq!(
            output,
            "{\n  \"plugins\": {\n    \"dependencies\": {\n      \"a\": \"1\",\n      \"b\": \"2\",\n      \"c\": \"1\"\n    }\n  }\n}\n"
        );
    }

    #[test]
    fn adding_creates_missing_objects() {
        let output = edit("{}", |config| config.set_dependency("a", "1"));
        assert_eq!(
            output,
            "{\n  \"plugins\": {\n    \"dependencies\": {\n      \"a\": \"1\"\n    }\n  }\n}\n"
        );
    }

    #[test]
    fn removing_the_last_dependency_drops_empty_objects() {
        let output = edit(r#"{"plugins":{"dependencies":{"a":"1"}}}"#, |config| {
            assert!(config.remove_dependency("a"));
        });
        assert_eq!(output, "{}\n");
    }

    #[test]
    fn removing_keeps_plugins_with_other_keys() {
        let output = edit(
            r#"{"plugins":{"paths":["p"],"dependencies":{"a":"1"}}}"#,
            |config| assert!(config.remove_dependency("a")),
        );
        let top: Raw = serde_json::from_str(&output).unwrap();
        let plugins: Raw = serde_json::from_str(top["plugins"].get()).unwrap();
        assert_eq!(plugins.keys().cloned().collect::<Vec<_>>(), ["paths"]);
    }

    #[test]
    fn removing_an_unknown_id_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), r#"{"plugins":{"dependencies":{"a":"1"}}}"#);
        let (mut config, _) = ProjectConfig::load(dir.path()).unwrap().unwrap();
        assert!(!config.remove_dependency("b"));
    }

    #[test]
    fn invalid_json_is_refused_and_not_written() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), r#"{"plugins":"#);
        let error = ProjectConfig::load(dir.path()).unwrap_err();
        assert!(matches!(error, ConfigError::Parse { .. }), "{error}");
        assert_eq!(read(dir.path()), r#"{"plugins":"#);
    }

    #[test]
    fn a_non_string_dependency_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), r#"{"plugins":{"dependencies":{"a":1}}}"#);
        let error = ProjectConfig::load(dir.path()).unwrap_err();
        assert!(matches!(error, ConfigError::Parse { .. }), "{error}");
    }

    #[test]
    fn duplicate_keys_are_refused() {
        for text in [
            r#"{"a":1,"a":2}"#,
            r#"{"plugins":{"dependencies":{"a":"1","a":"2"}}}"#,
        ] {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), text);
            let error = ProjectConfig::load(dir.path()).unwrap_err();
            assert!(
                matches!(&error, ConfigError::DuplicateKey { key, .. } if key == "a"),
                "{error}"
            );
            assert_eq!(read(dir.path()), text);
        }
    }

    #[cfg(unix)]
    #[test]
    fn save_keeps_the_file_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "{}");
        let path = dir.path().join("celesta.config.json");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let (mut config, _) = ProjectConfig::load(dir.path()).unwrap().unwrap();
        config.set_dependency("a.b", "1");
        config.save(dir.path()).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o644);
    }

    #[test]
    fn a_failed_write_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("celesta.config.json")).unwrap();
        let mut config = ProjectConfig::new();
        config.set_dependency("a.b", "1");
        let error = config.save(dir.path()).unwrap_err();
        assert!(matches!(error, ConfigError::Write { .. }), "{error}");
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn a_non_object_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "[]");
        let error = ProjectConfig::load(dir.path()).unwrap_err();
        assert!(matches!(error, ConfigError::NotAnObject { .. }), "{error}");
    }

    #[test]
    fn no_file_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(ProjectConfig::load(dir.path()).unwrap().is_none());
    }

    #[test]
    fn load_returns_the_exact_bytes() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "{ \"a\" :1 }\r\n");
        let (_, bytes) = ProjectConfig::load(dir.path()).unwrap().unwrap();
        assert_eq!(bytes, b"{ \"a\" :1 }\r\n");
    }

    #[test]
    fn restore_puts_back_the_exact_bytes() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "{ \"a\" :1 }\n");
        let (mut config, bytes) = ProjectConfig::load(dir.path()).unwrap().unwrap();
        config.set_dependency("a.b", "1");
        config.save(dir.path()).unwrap();
        restore_config(dir.path(), Some(&bytes)).unwrap();
        assert_eq!(read(dir.path()), "{ \"a\" :1 }\n");
    }

    #[test]
    fn restore_removes_a_created_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = ProjectConfig::new();
        config.set_dependency("a.b", "1");
        config.save(dir.path()).unwrap();
        restore_config(dir.path(), None).unwrap();
        assert!(!dir.path().join("celesta.config.json").exists());
        restore_config(dir.path(), None).unwrap();
    }

    #[test]
    fn the_saved_file_is_read_by_plugin_host() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = ProjectConfig::new();
        config.set_dependency("acme.charts", "1.2.3");
        config.save(dir.path()).unwrap();
        let dependencies = celesta_plugin_host::project_dependencies(dir.path()).unwrap();
        let expected = semver_req("1.2.3");
        assert_eq!(dependencies.dependencies.len(), 1);
        assert_eq!(dependencies.dependencies["acme.charts"], expected);
    }

    fn semver_req(text: &str) -> semver::VersionReq {
        text.parse().unwrap()
    }
}
