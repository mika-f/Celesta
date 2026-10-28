use crate::types::ComponentPropertyField;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fmt, fs, io};

/// The file beside an entry that supplies its project property values.
pub const ENTRY_PROPS_FILE: &str = "props.json";

/// Where one layer of [`PropertyInputs`] came from. The entry checks
/// `--props`/`--props-file` keys strictly against its
/// `defineProjectProperties()` schema, and `props.json` strictly when the
/// entry declares properties (ignoring it, with a warning, when it declares
/// none); a companion project's undeclared keys are ignored so projects
/// written before the schema keep exporting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertySource {
    Project,
    PropsFile,
    EntryProps,
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

/// The property file in effect for an entry, see [`resolve_props_file`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropsFileChoice {
    /// Absolute.
    pub path: PathBuf,
    /// True when found beside the entry rather than named with `--props-file`.
    pub auto: bool,
}

/// The one rule for which file supplies an entry's property values: an
/// explicit `--props-file`, else `props.json` in the entry's own folder when
/// it is a file. An explicit file is not checked for existence here; loading
/// it reports that.
pub fn resolve_props_file(entry: &Path, props_file: Option<&Path>) -> Option<PropsFileChoice> {
    let absolute = |path: &Path| std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    if let Some(file) = props_file {
        return Some(PropsFileChoice {
            path: absolute(file),
            auto: false,
        });
    }
    let path = absolute(entry).parent()?.join(ENTRY_PROPS_FILE);
    path.is_file()
        .then_some(PropsFileChoice { path, auto: true })
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
            layers.push(read_layer(
                file,
                PropertySource::PropsFile,
                format!("--props-file {}", file.display()),
            )?);
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

    /// [`load`](Self::load) for a React entry: the file [`resolve_props_file`]
    /// picks, then `--props` on top. A discovered `props.json` becomes an
    /// [`PropertySource::EntryProps`] layer whose relative paths resolve from
    /// the entry's folder.
    pub fn load_for_entry(
        entry: &Path,
        props_file: Option<&Path>,
        props: Option<&str>,
    ) -> Result<Self, PropertyInputError> {
        match resolve_props_file(entry, props_file) {
            Some(PropsFileChoice { path, auto: true }) => {
                let origin = format!("{ENTRY_PROPS_FILE} {}", path.display());
                let mut layers = vec![read_layer(&path, PropertySource::EntryProps, origin)?];
                layers.extend(Self::load(None, props)?.layers);
                Ok(Self { layers })
            }
            _ => Self::load(props_file, props),
        }
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

    #[cfg(test)]
    pub(crate) fn from_layers(layers: Vec<PropertyLayer>) -> Self {
        Self { layers }
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn layers(&self) -> &[PropertyLayer] {
        &self.layers
    }
}

/// Where a property's effective value comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueSource {
    /// The `--props` option.
    Props,
    /// The props file (`--props-file` or the entry's `props.json`).
    File,
    /// The default declared in the schema.
    Default,
}

/// The value the preview uses for one declared property, and where it came
/// from. The value is exactly as written in its layer (paths are not resolved).
#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveValue {
    pub value: Value,
    pub source: ValueSource,
}

/// Per declared key: the `--props` layer, else the props-file layer
/// (`PropsFile` or `EntryProps`), else the declared default. Companion
/// `Project` layers are ignored; standalone entries never have them.
pub fn effective_values(
    schema: &BTreeMap<String, ComponentPropertyField>,
    inputs: &PropertyInputs,
) -> BTreeMap<String, EffectiveValue> {
    // The last layer of a kind that has the key wins.
    let find = |key: &str, wanted: fn(PropertySource) -> bool| {
        inputs
            .layers()
            .iter()
            .rev()
            .filter(|layer| wanted(layer.source))
            .find_map(|layer| layer.values.get(key))
    };

    schema
        .iter()
        .map(|(key, field)| {
            let effective = if let Some(value) = find(key, |source| source == PropertySource::Props)
            {
                EffectiveValue {
                    value: value.clone(),
                    source: ValueSource::Props,
                }
            } else if let Some(value) = find(key, |source| {
                matches!(
                    source,
                    PropertySource::PropsFile | PropertySource::EntryProps
                )
            }) {
                EffectiveValue {
                    value: value.clone(),
                    source: ValueSource::File,
                }
            } else {
                EffectiveValue {
                    value: field.default_json(),
                    source: ValueSource::Default,
                }
            };
            (key.clone(), effective)
        })
        .collect()
}

/// Reads one property file into a layer. `origin` names the file in errors,
/// e.g. `--props-file a.json`; relative paths in the values resolve from the
/// file's directory.
fn read_layer(
    file: &Path,
    source: PropertySource,
    origin: String,
) -> Result<PropertyLayer, PropertyInputError> {
    let read_error = |source| PropertyInputError::Read {
        origin: origin.clone(),
        source,
    };
    let absolute = std::path::absolute(file).map_err(read_error)?;
    let text = fs::read_to_string(&absolute).map_err(read_error)?;
    let value = serde_json::from_str(&text).map_err(|source| PropertyInputError::Parse {
        origin: origin.clone(),
        source,
    })?;
    Ok(PropertyLayer {
        source,
        file: Some(file.to_owned()),
        base_dir: absolute.parent().map(Path::to_owned).unwrap_or_default(),
        values: object(value, || origin.clone())?,
    })
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
#[derive(Debug, thiserror::Error)]
pub enum PropertyInputError {
    #[error("could not read {origin}: {source}")]
    Read {
        /// The file as named in messages, e.g. `--props-file a.json`.
        origin: String,
        source: io::Error,
    },
    #[error("{origin} is not valid JSON: {source}")]
    Parse {
        origin: String,
        source: serde_json::Error,
    },
    #[error("{0} must be a JSON object of property values")]
    NotAnObject(String),
    #[error("could not resolve --props paths from the current directory: {0}")]
    CurrentDir(#[source] io::Error),
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
