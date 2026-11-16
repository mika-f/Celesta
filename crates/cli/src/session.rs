//! What one `celesta` run works with: the registry it reaches, the store and
//! caches under the user's folder, and the running Celesta.

use std::path::PathBuf;
use std::time::Duration;

use celesta_plugin_host::{
    CELESTA_VERSION, bundled_plugin_dir, registry_cache_dir, registry_store_dir, user_settings_path,
};
use celesta_registry_client::{Environment, Registry, RegistryAccess, SelectionError, Store};
use celesta_registry_format::Target;
use semver::Version;
use time::OffsetDateTime;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Selection(#[from] SelectionError),
    #[error(
        "there is no user folder to keep plugins in; set HOME (APPDATA on Windows) to an absolute folder"
    )]
    NoUserFolder,
    #[error("could not create the registry cache folder {}: {source}", path.display())]
    CacheFolder {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl SessionError {
    /// Whether the error is in how `celesta` was invoked (exit 2).
    pub fn is_usage(&self) -> bool {
        matches!(self, Self::Selection(_))
    }
}

pub struct Session {
    pub registry: Registry,
    pub store: Store,
    /// The folder this registry's verified files are cached in:
    /// [`registry_cache_dir`] for the official registry, and
    /// `override-<16 hex digits>` in it for a registry
    /// `CELESTA_REGISTRY` names, the digits the start of the SHA-256 of
    /// the base URL without a trailing `/`. Rollback stamps and the
    /// advisory list are kept per file path, so registries sharing a folder
    /// would be checked against each other's files, and the desktop app
    /// and the exporter would read another registry's advisories. No
    /// served path is named `override-…`.
    pub cache: PathBuf,
    pub settings_path: PathBuf,
    pub celesta: Version,
    pub host: Target,
    pub bundled: Option<PathBuf>,
    pub now: OffsetDateTime,
    access: RegistryAccess,
}

impl Session {
    /// The session of this run: the registry the environment names, else
    /// the official one, and the user's folders.
    pub fn from_env() -> Result<Self, SessionError> {
        let access = RegistryAccess::from_env()?;
        let store = registry_store_dir().ok_or(SessionError::NoUserFolder)?;
        let cache = registry_cache_dir().ok_or(SessionError::NoUserFolder)?;
        let settings_path = user_settings_path().ok_or(SessionError::NoUserFolder)?;
        Self::new(access, store, cache, settings_path)
    }

    /// The session for `access` with the user's folders: the store, the
    /// registry cache folder ([`registry_cache_dir`]), and the settings.
    fn new(
        access: RegistryAccess,
        store: PathBuf,
        cache_root: PathBuf,
        settings_path: PathBuf,
    ) -> Result<Self, SessionError> {
        let cache = access
            .cache_dir(&cache_root)
            .map_err(|source| SessionError::CacheFolder {
                path: access.cache_path(&cache_root),
                source,
            })?;
        let registry = Registry::new(
            &access.base,
            access.root,
            access.transport(None),
            cache.clone(),
        );
        Ok(Self {
            registry,
            store: Store::new(store),
            cache,
            settings_path,
            celesta: Version::parse(CELESTA_VERSION)
                .expect("the crate version is a semantic version"),
            host: Target::host(),
            bundled: bundled_plugin_dir(),
            now: OffsetDateTime::now_utc(),
            access,
        })
    }

    pub fn environment(&self) -> Environment<'_> {
        Environment {
            registry: &self.registry,
            store: &self.store,
            celesta: &self.celesta,
            host: &self.host,
            bundled: self.bundled.as_deref(),
            now: self.now,
            progress: None,
        }
    }

    /// Another client of the session's registry whose HTTP fetches are each
    /// limited to `timeout`.
    pub fn registry_within(&self, timeout: Duration) -> Registry {
        Registry::new(
            &self.access.base,
            self.access.root,
            self.access.transport(Some(timeout)),
            self.cache.clone(),
        )
    }
}
