use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use semver::{Op, Version, VersionReq};
use serde::de::{self, DeserializeOwned};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;

use crate::error::FormatError;
use crate::ids::{is_plugin_id, is_registry_id};

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

/// A build target: `<os>-<arch>` or `any`. `os` is lowercase ASCII letters and
/// digits, `arch` the same plus `_` (Rust's `std::env::consts` names, such as
/// `macos-aarch64`). Any such OS is accepted, so a target for an OS this client
/// does not know does not make it refuse the index; it just never matches.
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
            !os.is_empty()
                && os
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
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

/// Decodes standard base64 with padding into exactly `N` bytes.
fn decode_b64<const N: usize>(s: &str) -> Result<[u8; N], String> {
    let bytes = STANDARD
        .decode(s)
        .map_err(|e| format!("invalid base64 {s:?}: {e}"))?;
    bytes
        .try_into()
        .map_err(|_| format!("invalid base64 {s:?}: expected {N} bytes"))
}

/// Timestamps: RFC 3339 in UTC with whole seconds, spelled exactly
/// `YYYY-MM-DDTHH:MM:SSZ`. No offset other than `Z`, and no fraction.
pub(crate) mod timestamp {
    use time::format_description::well_known::Rfc3339;
    use time::{OffsetDateTime, UtcOffset};

    use super::*;

    fn shape_ok(s: &str) -> bool {
        let b = s.as_bytes();
        b.len() == 20
            && b.iter().enumerate().all(|(i, &c)| match i {
                4 | 7 => c == b'-',
                10 => c == b'T',
                13 | 16 => c == b':',
                19 => c == b'Z',
                _ => c.is_ascii_digit(),
            })
    }

    pub fn parse(s: &str) -> Result<OffsetDateTime, FormatError> {
        let bad = || FormatError::InvalidTimestamp(s.to_owned());
        if !shape_ok(s) {
            return Err(bad());
        }
        // `time` reads a leap second (`:60`) as `:59.999999999`; refuse it.
        let t = OffsetDateTime::parse(s, &Rfc3339).map_err(|_| bad())?;
        if t.nanosecond() != 0 {
            return Err(bad());
        }
        Ok(t)
    }

    pub fn format(t: OffsetDateTime) -> Result<String, String> {
        let t = t.to_offset(UtcOffset::UTC);
        if t.nanosecond() != 0 {
            return Err(format!("timestamp {t} has fractional seconds"));
        }
        t.format(&Rfc3339).map_err(|e| e.to_string())
    }

    pub fn serialize<S: Serializer>(t: &OffsetDateTime, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format(*t).map_err(serde::ser::Error::custom)?)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<OffsetDateTime, D::Error> {
        let s = String::deserialize(d)?;
        parse(&s).map_err(de::Error::custom)
    }
}

/// An online signing key from `config.json`.
///
/// `public_b64` and `expires_text` keep the exact strings the file holds: the
/// root certifies that text, so the certified message is built from it, not
/// from the decoded values. `public` and `expires` are derived conveniences:
/// trust decisions use the raw strings, and the keyring ignores the derived
/// values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawOnlineKey", into = "RawOnlineKey")]
pub struct OnlineKey {
    pub keyid: String,
    /// The decoded `public`.
    pub public: [u8; 32],
    /// `public` exactly as written in `config.json`.
    pub public_b64: String,
    /// The parsed `expires`.
    pub expires: OffsetDateTime,
    /// `expires` exactly as written in `config.json`.
    pub expires_text: String,
    pub signature: [u8; 64],
}

#[derive(Serialize, Deserialize)]
struct RawOnlineKey {
    keyid: String,
    public: String,
    expires: String,
    signature: String,
}

impl TryFrom<RawOnlineKey> for OnlineKey {
    type Error = String;

    fn try_from(raw: RawOnlineKey) -> Result<Self, String> {
        let expires = timestamp::parse(&raw.expires).map_err(|e| e.to_string())?;
        Ok(Self {
            keyid: raw.keyid,
            public: decode_b64(&raw.public)?,
            public_b64: raw.public,
            expires,
            expires_text: raw.expires,
            signature: decode_b64(&raw.signature)?,
        })
    }
}

impl From<OnlineKey> for RawOnlineKey {
    fn from(key: OnlineKey) -> Self {
        Self {
            keyid: key.keyid,
            public: key.public_b64,
            expires: key.expires_text,
            signature: STANDARD.encode(key.signature),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub api: String,
    /// When the registry generated this file. A client refuses a served copy
    /// older than the newest one it verified at the same path (see
    /// [`check_fresh`](crate::check_fresh)).
    #[serde(with = "timestamp")]
    pub generated: OffsetDateTime,
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
    #[serde(with = "timestamp")]
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
    /// When the registry generated this file; see [`Config::generated`].
    #[serde(with = "timestamp")]
    pub generated: OffsetDateTime,
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
    #[serde(with = "timestamp")]
    pub generated: OffsetDateTime,
    pub plugins: Vec<SearchEntry>,
}

/// One advisory. Deserializing it checks the range rule and the plugin id, so
/// every serde path into an `Advisory` (or an `AdvisoryList`) is validated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "RawAdvisory")]
pub struct Advisory {
    pub id: String,
    pub plugin: String,
    pub versions: VersionReq,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub fixed: Option<Version>,
    #[serde(with = "timestamp")]
    pub published: OffsetDateTime,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAdvisory {
    id: String,
    plugin: String,
    versions: String,
    title: String,
    description: String,
    #[serde(default)]
    fixed: Option<Version>,
    #[serde(with = "timestamp")]
    published: OffsetDateTime,
}

impl TryFrom<RawAdvisory> for Advisory {
    type Error = FormatError;

    fn try_from(raw: RawAdvisory) -> Result<Self, FormatError> {
        let bad_range = || FormatError::InvalidRange(raw.versions.clone());
        // `VersionReq` parses build metadata in a comparator and drops it.
        if raw.versions.contains('+') {
            return Err(bad_range());
        }
        let versions = VersionReq::parse(&raw.versions).map_err(|_| bad_range())?;
        if !is_plain_range(&versions) {
            return Err(bad_range());
        }
        if !is_plugin_id(&raw.plugin) {
            return Err(FormatError::InvalidId(raw.plugin));
        }
        Ok(Self {
            id: raw.id,
            plugin: raw.plugin,
            versions,
            title: raw.title,
            description: raw.description,
            fixed: raw.fixed,
            published: raw.published,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisoryList {
    #[serde(with = "timestamp")]
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
        ("invalid timestamp: ", FormatError::InvalidTimestamp),
        ("invalid advisory range: ", FormatError::InvalidRange),
        ("invalid plugin id: ", FormatError::InvalidId),
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

/// Advisory ranges allow only `=`, `<`, `<=`, `>`, and `>=`, each with a full
/// `major.minor.patch`, so that precedence ordering decides every match.
fn is_plain_range(req: &VersionReq) -> bool {
    !req.comparators.is_empty()
        && req.comparators.iter().all(|c| {
            matches!(
                c.op,
                Op::Exact | Op::Less | Op::LessEq | Op::Greater | Op::GreaterEq
            ) && c.minor.is_some()
                && c.patch.is_some()
        })
}

impl AdvisoryList {
    /// Parses `advisories.json`. Each advisory's range and plugin id are
    /// checked while deserializing (see [`Advisory`]).
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        decode(bytes)
    }
}
