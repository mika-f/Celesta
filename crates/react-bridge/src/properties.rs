use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::{fmt, fs, io};

/// Where one layer of [`PropertyInputs`] came from. The entry checks
/// `--props`/`--props-file` keys strictly against its
/// `defineProjectProperties()` schema; a companion project's undeclared keys
/// are ignored so projects written before the schema keep exporting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertySource {
    Project,
    PropsFile,
    Props,
}

/// One source's values and the directory their relative `path` values
/// resolve from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyLayer {
    pub source: PropertySource,
    /// The file the values came from, named in validation messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    pub base_dir: PathBuf,
    pub values: Map<String, Value>,
}

/// Project property values a React entry receives from outside its source,
/// lowest precedence first. The entry validates them against its schema
/// before `prepare()`; see `packages/cli/src/property-inputs.ts`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PropertyInputs {
    layers: Vec<PropertyLayer>,
}

impl PropertyInputs {
    /// Reads `--props-file` and parses `--props`, each a JSON object.
    /// `--props` wins over the file, and its relative paths resolve from the
    /// current directory; the file's resolve from the file's directory.
    pub fn load(
        props_file: Option<&Path>,
        props: Option<&str>,
    ) -> Result<Self, PropertyInputError> {
        let mut layers = Vec::new();
        if let Some(file) = props_file {
            let read_error = |source| PropertyInputError::Read {
                file: file.to_owned(),
                source,
            };
            let absolute = std::path::absolute(file).map_err(read_error)?;
            let text = fs::read_to_string(&absolute).map_err(read_error)?;
            let value =
                serde_json::from_str(&text).map_err(|source| PropertyInputError::Parse {
                    origin: format!("--props-file {}", file.display()),
                    source,
                })?;
            layers.push(PropertyLayer {
                source: PropertySource::PropsFile,
                file: Some(file.to_owned()),
                base_dir: absolute.parent().map(Path::to_owned).unwrap_or_default(),
                values: object(value, || format!("--props-file {}", file.display()))?,
            });
        }
        if let Some(props) = props {
            let value =
                serde_json::from_str(props).map_err(|source| PropertyInputError::Parse {
                    origin: "--props".to_owned(),
                    source,
                })?;
            layers.push(PropertyLayer {
                source: PropertySource::Props,
                file: None,
                base_dir: std::env::current_dir().map_err(PropertyInputError::CurrentDir)?,
                values: object(value, || "--props".to_owned())?,
            });
        }
        Ok(Self { layers })
    }

    /// Adds a companion project's `properties` below every other layer.
    /// `base_dir` is the project's asset root.
    #[must_use]
    pub fn with_project(
        mut self,
        properties: &BTreeMap<String, Value>,
        file: Option<&Path>,
        base_dir: &Path,
    ) -> Self {
        if !properties.is_empty() {
            self.layers.insert(
                0,
                PropertyLayer {
                    source: PropertySource::Project,
                    file: file.map(Path::to_owned),
                    base_dir: std::path::absolute(base_dir).unwrap_or_else(|_| base_dir.to_owned()),
                    values: properties.clone().into_iter().collect(),
                },
            );
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn layers(&self) -> &[PropertyLayer] {
        &self.layers
    }
}

fn object(
    value: Value,
    origin: impl FnOnce() -> String,
) -> Result<Map<String, Value>, PropertyInputError> {
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(PropertyInputError::NotAnObject(origin())),
    }
}

/// `--props` or `--props-file` could not be read as a JSON object.
#[derive(Debug)]
pub enum PropertyInputError {
    Read {
        file: PathBuf,
        source: io::Error,
    },
    Parse {
        origin: String,
        source: serde_json::Error,
    },
    NotAnObject(String),
    CurrentDir(io::Error),
}

impl fmt::Display for PropertyInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { file, source } => {
                write!(
                    formatter,
                    "could not read --props-file {}: {source}",
                    file.display()
                )
            }
            Self::Parse { origin, source } => {
                write!(formatter, "{origin} is not valid JSON: {source}")
            }
            Self::NotAnObject(origin) => {
                write!(
                    formatter,
                    "{origin} must be a JSON object of property values"
                )
            }
            Self::CurrentDir(source) => {
                write!(
                    formatter,
                    "could not resolve --props paths from the current directory: {source}"
                )
            }
        }
    }
}

impl Error for PropertyInputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::CurrentDir(source) => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::NotAnObject(_) => None,
        }
    }
}

/// One value the entry rejected while checking [`PropertyInputs`].
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PropertyIssue {
    /// The property key; empty when the issue concerns a whole source.
    pub key: String,
    /// The option that supplied the value, e.g. `--props-file variant.json`.
    pub source: String,
    pub message: String,
}

impl fmt::Display for PropertyIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.key.is_empty() {
            write!(formatter, "{}: {}", self.source, self.message)
        } else {
            write!(formatter, "{}: {}: {}", self.source, self.key, self.message)
        }
    }
}
