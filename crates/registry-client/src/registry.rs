use std::io::Read;
use std::path::PathBuf;

use celesta_plugin_host::{ADVISORIES_CACHE, load_cached_advisories};
use celesta_registry_format::{
    AdvisoryList, FormatError, Index, Keyring, RootKey, SearchList, check_fresh, is_registry_id,
};
use time::OffsetDateTime;

use crate::cache;
use crate::error::ClientError;
use crate::transport::{MAX_JSON_BYTES, Transport, TransportError};

/// The base URL of the official registry.
pub const OFFICIAL_REGISTRY: &str = "https://registry.celesta.studio";

/// The official registry's root public key (Ed25519), in standard base64.
/// It certifies the online signing keys `config.json` lists; replacing it
/// takes a Celesta release.
const OFFICIAL_ROOT_KEY: &str = "eqOBn7d6SvlrSXx+f3oWpJPhxxAn209/9F8QqhIBSs0=";

/// The official registry's pinned root public key.
pub fn official_root_key() -> RootKey {
    RootKey::from_base64(OFFICIAL_ROOT_KEY).expect("the official root key is 32 bytes of base64")
}

/// A registry's signed files, fetched and verified through a transport and
/// cached in a folder.
pub struct Registry {
    base: String,
    root: RootKey,
    transport: Box<dyn Transport>,
    cache: PathBuf,
}

impl Registry {
    pub fn new(base: &str, root: RootKey, transport: Box<dyn Transport>, cache: PathBuf) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
            root,
            transport,
            cache,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// The transport the registry's files, and the archives they list, are
    /// fetched through.
    pub(crate) fn transport(&self) -> &dyn Transport {
        self.transport.as_ref()
    }

    /// Fetches `config.json` and builds the keyring from it. The keyring is
    /// valid at `now` only, so each fetch builds a fresh one.
    pub fn keyring(&self, now: OffsetDateTime) -> Result<Keyring, ClientError> {
        const PATH: &str = "/config.json";
        let (bytes, sig) = self.read_pair(PATH)?;
        let format = |source| format_error(PATH, source);
        let (config, keyring) =
            Keyring::from_signed_config(&bytes, &sig, &self.root, now).map_err(format)?;
        self.accept(PATH, &bytes, config.generated, now)?;
        Ok(keyring)
    }

    /// The verified `index/<id>.json`.
    pub fn index(&self, id: &str, now: OffsetDateTime) -> Result<Index, ClientError> {
        if !is_registry_id(id) {
            return Err(ClientError::NoSuchPlugin(id.to_owned()));
        }
        let path = format!("/index/{id}.json");
        let keyring = self.keyring(now)?;
        let (bytes, sig) = match self.read_pair(&path) {
            Err(ClientError::Transport(TransportError::NotFound(_))) => {
                return Err(ClientError::NoSuchPlugin(id.to_owned()));
            }
            other => other?,
        };
        self.verify(&keyring, &path, &bytes, &sig)?;
        let index = Index::parse(&bytes, id).map_err(|source| format_error(&path, source))?;
        self.accept(&path, &bytes, index.generated, now)?;
        Ok(index)
    }

    /// The index cached by an earlier `index`, if any.
    pub fn cached_index(&self, id: &str) -> Option<Index> {
        if !is_registry_id(id) {
            return None;
        }
        let bytes =
            cache::read_trusted(&cache::file_path(&self.cache, &format!("/index/{id}.json")))?;
        Index::parse(&bytes, id).ok()
    }

    /// The verified `search.json`.
    pub fn search(&self, now: OffsetDateTime) -> Result<SearchList, ClientError> {
        const PATH: &str = "/search.json";
        let keyring = self.keyring(now)?;
        let (bytes, sig) = self.read_pair(PATH)?;
        self.verify(&keyring, PATH, &bytes, &sig)?;
        let list = SearchList::parse(&bytes).map_err(|source| format_error(PATH, source))?;
        self.accept(PATH, &bytes, list.generated, now)?;
        Ok(list)
    }

    /// Fetches, verifies, checks against the cached list, and caches
    /// `advisories.json`. When it fails, the cached list stays as it was.
    pub fn refresh_advisories(&self, now: OffsetDateTime) -> Result<AdvisoryList, ClientError> {
        const PATH: &str = "/advisories.json";
        let keyring = self.keyring(now)?;
        let (bytes, sig) = self.read_pair(PATH)?;
        self.verify(&keyring, PATH, &bytes, &sig)?;
        let list = AdvisoryList::parse(&bytes).map_err(|source| format_error(PATH, source))?;
        self.accept(PATH, &bytes, list.generated, now)?;
        Ok(list)
    }

    /// The advisory list cached by `refresh_advisories`; `None` when there
    /// is none. An unreadable or unsafe cache is an error, so nothing is
    /// installed without knowing what the list blocks.
    pub fn cached_advisories(&self) -> Result<Option<AdvisoryList>, ClientError> {
        let path = self.cache.join(ADVISORIES_CACHE);
        load_cached_advisories(&path).map_err(|message| ClientError::Cache { path, message })
    }

    fn verify(
        &self,
        keyring: &Keyring,
        path: &str,
        bytes: &[u8],
        sig: &[u8],
    ) -> Result<(), ClientError> {
        keyring
            .verify(path, bytes, sig)
            .map_err(|source| format_error(path, source))
    }

    /// Checks `generated` against the newest stamp at `path`, then caches the
    /// verified `bytes` and the stamp, in that order.
    fn accept(
        &self,
        path: &str,
        bytes: &[u8],
        generated: OffsetDateTime,
        now: OffsetDateTime,
    ) -> Result<(), ClientError> {
        // Hold the lock over the whole check-then-write, so a stalled thread
        // cannot overwrite what a later one accepted.
        let held = cache::lock();
        let newest = cache::stamp(&self.cache, path)?;
        check_fresh(generated, newest, now).map_err(|source| format_error(path, source))?;
        cache::write_private(&cache::file_path(&self.cache, path), bytes)?;
        cache::record_stamp(&held, &self.cache, path, generated)
    }

    /// The file at `path` and its `.sig`.
    fn read_pair(&self, path: &str) -> Result<(Vec<u8>, Vec<u8>), ClientError> {
        let file = self.read(path)?;
        let sig = self.read(&format!("{path}.sig"))?;
        Ok((file, sig))
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, ClientError> {
        let url = format!("{}{path}", self.base);
        let mut body = Vec::new();
        self.transport
            .open(&url)?
            .take(MAX_JSON_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|e| TransportError::Failed {
                url: url.clone(),
                message: e.to_string(),
            })?;
        if body.len() as u64 > MAX_JSON_BYTES {
            return Err(ClientError::TooLarge { url });
        }
        Ok(body)
    }
}

fn format_error(path: &str, source: FormatError) -> ClientError {
    ClientError::Format {
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use celesta_registry_format::FormatError;

    use super::*;
    use crate::testing::{TestRegistry, at};

    #[test]
    fn the_official_root_key_is_pinned() {
        let parsed = RootKey::from_base64(OFFICIAL_ROOT_KEY).unwrap();
        assert_eq!(official_root_key(), parsed);
        // The bytes round-trip to the same canonical base64.
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(official_root_key().0);
        assert_eq!(encoded, "eqOBn7d6SvlrSXx+f3oWpJPhxxAn209/9F8QqhIBSs0=");
        // An Ed25519 public key, not just 32 bytes.
        assert!(ed25519_dalek::VerifyingKey::from_bytes(&official_root_key().0).is_ok());
    }

    #[test]
    fn a_late_older_file_cannot_replace_a_newer_one() {
        let cache = tempfile::tempdir().unwrap();
        let registry = TestRegistry::new().client(cache.path());
        let now = at("2026-11-21T00:00:00Z");
        let newer = at("2026-11-20T00:00:10Z");
        let older = at("2026-11-20T00:00:00Z");
        registry.accept("/x.json", b"newer", newer, now).unwrap();
        let error = registry
            .accept("/x.json", b"older", older, now)
            .unwrap_err();
        assert!(
            matches!(
                error,
                ClientError::Format {
                    source: FormatError::Rollback,
                    ..
                }
            ),
            "{error:?}"
        );
        let file = cache::file_path(cache.path(), "/x.json");
        assert_eq!(std::fs::read(file).unwrap(), b"newer");
        assert_eq!(cache::stamp(cache.path(), "/x.json").unwrap(), Some(newer));
    }
}
