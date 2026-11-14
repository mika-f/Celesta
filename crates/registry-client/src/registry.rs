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

/// The official registry's pinned root public key; `None` until the
/// maintainer supplies it.
pub fn official_root_key() -> Option<RootKey> {
    None
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

    /// The advisory list cached by `refresh_advisories`; an unreadable
    /// cache reads as no list.
    pub fn cached_advisories(&self) -> Option<AdvisoryList> {
        load_cached_advisories(&self.cache.join(ADVISORIES_CACHE))
            .ok()
            .flatten()
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
        let newest = cache::stamp(&self.cache, path)?;
        check_fresh(generated, newest, now).map_err(|source| format_error(path, source))?;
        cache::write_private(&cache::file_path(&self.cache, path), bytes)?;
        // An older stamp would let a rollback through, so never lower one.
        cache::record_stamp(
            &self.cache,
            path,
            generated.max(newest.unwrap_or(generated)),
        )
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
