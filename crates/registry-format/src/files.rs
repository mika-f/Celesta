use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use semver::{Version, VersionReq};
use serde::de::{self, DeserializeOwned};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;

use crate::error::FormatError;
use crate::ids::is_registry_id;

const OSES: [&str; 3] = ["windows", "linux", "macos"];

/// A SHA-256 digest, written `sha256:<64 lowercase hex>`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash(pub [u8; 32]);

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("sha256:")?;
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl Hash {
    pub fn parse(s: &str) -> Result<Self, FormatError> {
        let bad = || FormatError::InvalidHash(s.to_owned());
        let hex = s.strip_prefix("sha256:").ok_or_else(bad)?;
        if hex.len() != 64 || !hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(bad());
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| bad())?;
        }
        Ok(Self(out))
    }
}

impl Serialize for Hash {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(de::Error::custom)
    }
}

/// A build target: `<os>-<arch>` or `any`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Target(Cow<'static, str>);

impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Target({})", self.0)
    }
}

impl Target {
    pub const ANY: Target = Target(Cow::Borrowed("any"));

    pub fn parse(s: &str) -> Result<Self, FormatError> {
        if s == "any" {
            return Ok(Self::ANY);
        }
        let ok = s.split_once('-').is_some_and(|(os, arch)| {
            OSES.contains(&os)
                && !arch.is_empty()
                && arch
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        });
        if ok {
            Ok(Self(Cow::Owned(s.to_owned())))
        } else {
            Err(FormatError::InvalidTarget(s.to_owned()))
        }
    }

    /// The target of the running process.
    pub fn host() -> Self {
        Self(Cow::Owned(format!(
            "{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Target {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Target {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(de::Error::custom)
    }
}

mod b64 {
    use super::*;

    pub fn serialize<const N: usize, S: Serializer>(v: &[u8; N], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(v))
    }

    pub fn deserialize<'de, const N: usize, D: Deserializer<'de>>(
        d: D,
    ) -> Result<[u8; N], D::Error> {
        let s = String::deserialize(d)?;
        let bytes = STANDARD.decode(&s).map_err(de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| de::Error::custom(format!("expected {N} bytes")))
    }
}

mod rfc3339 {
    pub use time::serde::rfc3339::*;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineKey {
    pub keyid: String,
    #[serde(with = "b64")]
    pub public: [u8; 32],
    #[serde(with = "rfc3339")]
    pub expires: OffsetDateTime,
    #[serde(with = "b64")]
    pub signature: [u8; 64],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub api: String,
    pub keys: Vec<OnlineKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kind {
    pub roles: Vec<String>,
    pub program: bool,
    pub module: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deprecation {
    pub message: String,
    #[serde(default)]
    pub replacement: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub repository: String,
    pub commit: String,
    pub workflow: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub url: String,
    pub size: u64,
    pub sha256: Hash,
    pub tree_hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub version: Version,
    pub celesta: VersionReq,
    pub protocol: u32,
    #[serde(with = "rfc3339")]
    pub published: OffsetDateTime,
    pub yanked: bool,
    pub withdrawn: bool,
    #[serde(default)]
    pub deprecated: Option<Deprecation>,
    #[serde(default)]
    pub source: Option<Source>,
    pub targets: BTreeMap<Target, Artifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub id: String,
    pub publisher: String,
    pub kind: Kind,
    pub description: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub deprecated: Option<Deprecation>,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchEntry {
    pub id: String,
    pub publisher: String,
    pub kind: Kind,
    pub description: String,
    #[serde(default)]
    pub latest: Option<Version>,
    #[serde(default)]
    pub deprecated: Option<Deprecation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchList {
    #[serde(with = "rfc3339")]
    pub generated: OffsetDateTime,
    pub plugins: Vec<SearchEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Advisory {
    pub id: String,
    pub plugin: String,
    pub versions: VersionReq,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub fixed: Option<Version>,
    #[serde(with = "rfc3339")]
    pub published: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisoryList {
    #[serde(with = "rfc3339")]
    pub generated: OffsetDateTime,
    pub advisories: Vec<Advisory>,
}

/// Recovers the typed variant of a validation failure raised inside a deserializer.
fn lift(e: serde_json::Error) -> FormatError {
    let msg = e.to_string();
    for (prefix, make) in [
        (
            "invalid hash: ",
            FormatError::InvalidHash as fn(String) -> FormatError,
        ),
        ("invalid target: ", FormatError::InvalidTarget),
    ] {
        if let Some(rest) = msg.strip_prefix(prefix) {
            let value = rest.split(" at line ").next().unwrap_or(rest);
            return make(value.to_owned());
        }
    }
    FormatError::Json(e)
}

/// Checks `format == 1` before decoding into the typed file.
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, FormatError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    match value.get("format").and_then(serde_json::Value::as_u64) {
        Some(1) => serde_json::from_value(value).map_err(lift),
        Some(other) => Err(FormatError::Format(other)),
        None => Err(FormatError::Json(de::Error::missing_field("format"))),
    }
}

impl Config {
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        decode(bytes)
    }
}

impl Index {
    pub fn parse(bytes: &[u8], expected_id: &str) -> Result<Self, FormatError> {
        let index: Self = decode(bytes)?;
        if !is_registry_id(&index.id) {
            return Err(FormatError::InvalidId(index.id));
        }
        if index.id != expected_id {
            return Err(FormatError::IdMismatch {
                expected: expected_id.to_owned(),
                found: index.id,
            });
        }
        Ok(index)
    }
}

impl SearchList {
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let list: Self = decode(bytes)?;
        match list.plugins.iter().find(|p| !is_registry_id(&p.id)) {
            Some(p) => Err(FormatError::InvalidId(p.id.clone())),
            None => Ok(list),
        }
    }
}

impl AdvisoryList {
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let list: Self = decode(bytes)?;
        match list.advisories.iter().find(|a| !is_registry_id(&a.plugin)) {
            Some(a) => Err(FormatError::InvalidId(a.plugin.clone())),
            None => Ok(list),
        }
    }
}
