//! Editing `plugins.dependencies` in a project's `celesta.config.json`
//! without losing anything else in the file.

use indexmap::IndexMap;
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::value::RawValue;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const PROJECT_CONFIG: &str = "celesta.config.json";

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
    /// [`restore`]), or `None` when there is no such file.
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
        let file: Raw = serde_json::from_str(whole.get()).map_err(parse)?;
        let plugins: Raw = match file.get("plugins") {
            Some(value) => serde_json::from_str(value.get()).map_err(parse)?,
            None => Raw::new(),
        };
        let dependencies = match plugins.get("dependencies") {
            Some(value) => serde_json::from_str(value.get()).map_err(parse)?,
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
        let mut temporary = tempfile::NamedTempFile::new_in(root).map_err(write)?;
        temporary.write_all(&text).map_err(write)?;
        temporary
            .persist(&path)
            .map_err(|error| write(error.error))?;
        Ok(())
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

/// Puts `previous` back exactly, or removes the file when there was none.
pub fn restore(root: &Path, previous: Option<&[u8]>) -> io::Result<()> {
    let path = root.join(PROJECT_CONFIG);
    match previous {
        Some(bytes) => std::fs::write(path, bytes),
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
        restore(dir.path(), Some(&bytes)).unwrap();
        assert_eq!(read(dir.path()), "{ \"a\" :1 }\n");
    }

    #[test]
    fn restore_removes_a_created_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = ProjectConfig::new();
        config.set_dependency("a.b", "1");
        config.save(dir.path()).unwrap();
        restore(dir.path(), None).unwrap();
        assert!(!dir.path().join("celesta.config.json").exists());
        restore(dir.path(), None).unwrap();
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
