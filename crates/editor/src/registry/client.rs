//! What the editor's registry work reads: the registry the environment
//! names (else the official one), the store, the registry cache and the
//! settings under the user's folder, and the running Celesta and its
//! target. Cheap to clone and shared with background work through an
//! `Arc`; a `Registry` client is made where it is used, off the UI thread.

use std::io;
use std::path::{Path, PathBuf};

use celesta_plugin_host::{
    ADVISORIES_CACHE, CELESTA_VERSION, bundled_plugin_dir, load_cached_advisories,
    registry_cache_dir, registry_store_dir, user_settings_path,
};
use celesta_registry_client::{
    Environment, Progress, Registry, RegistryAccess, Store, select_registry,
};
use celesta_registry_format::{AdvisoryList, Target};
use semver::Version;
use time::OffsetDateTime;

#[derive(Clone, Debug)]
pub(crate) struct EditorRegistry {
    access: RegistryAccess,
    store: Store,
    /// The registry cache root ([`registry_cache_dir`]); an overriding
    /// registry caches in a folder of its own in it
    /// ([`RegistryAccess::cache_dir`]).
    cache: PathBuf,
    settings_path: PathBuf,
    celesta: Version,
    host: Target,
    bundled: Option<PathBuf>,
    /// Why `CELESTA_REGISTRY` / `CELESTA_REGISTRY_ROOT_KEY` were not used,
    /// when they were set wrong and the official registry is used instead.
    selection_error: Option<String>,
}

impl EditorRegistry {
    /// The registry the environment names, with the user's folders. When
    /// the variables are set wrong the official registry is used, and
    /// [`Self::selection_error`] says why. Fails without a user folder.
    pub(crate) fn from_env() -> Result<Self, String> {
        let (access, selection_error) = match RegistryAccess::from_env() {
            Ok(access) => (access, None),
            Err(error) => (
                select_registry(None, None).expect("the official registry needs no variables"),
                Some(error.to_string()),
            ),
        };
        let user_folder = || {
            "There is no user folder to keep plugins in. Set HOME (APPDATA on Windows) to an absolute folder.".to_owned()
        };
        Ok(Self {
            access,
            store: Store::new(registry_store_dir().ok_or_else(user_folder)?),
            cache: registry_cache_dir().ok_or_else(user_folder)?,
            settings_path: user_settings_path().ok_or_else(user_folder)?,
            celesta: Version::parse(CELESTA_VERSION)
                .expect("the crate version is a semantic version"),
            host: Target::host(),
            bundled: bundled_plugin_dir(),
            selection_error,
        })
    }

    /// A client of the `served` test registry, with the store, the cache,
    /// and the settings in `user`.
    #[cfg(test)]
    pub(crate) fn for_test(
        served: &celesta_registry_client::testing::TestRegistry,
        user: &Path,
    ) -> Self {
        let access = select_registry(
            Some(served.base().into()),
            Some(served.root_key_base64().into()),
        )
        .expect("the test registry's variables");
        Self {
            access,
            store: Store::new(user.join("store")),
            cache: user.join("cache"),
            settings_path: user.join("settings.json"),
            celesta: Version::parse(CELESTA_VERSION).unwrap(),
            host: Target::host(),
            bundled: None,
            selection_error: None,
        }
    }

    /// Why the registry variables were not used, if they were set wrong.
    pub(crate) fn selection_error(&self) -> Option<&str> {
        self.selection_error.as_deref()
    }

    /// A client of the registry; blocking (it may create the cache
    /// folder), so off the UI thread.
    pub(crate) fn registry(&self) -> io::Result<Registry> {
        self.access.client(&self.cache, None)
    }

    /// What the registry-client operations work with, now.
    pub(crate) fn environment<'a>(
        &'a self,
        registry: &'a Registry,
        progress: Option<&'a (dyn Fn(Progress) + Sync)>,
    ) -> Environment<'a> {
        Environment {
            registry,
            store: &self.store,
            celesta: &self.celesta,
            host: &self.host,
            bundled: self.bundled.as_deref(),
            now: OffsetDateTime::now_utc(),
            progress,
        }
    }

    /// The overriding registry's verified cached advisory list, which
    /// discovery reads besides the official one; `None` for the official
    /// registry, or when it has none that can be read. Blocking.
    pub(crate) fn override_advisories(&self) -> Option<AdvisoryList> {
        if !self.access.overridden {
            return None;
        }
        let path = self.access.cache_path(&self.cache).join(ADVISORIES_CACHE);
        load_cached_advisories(&path).ok().flatten()
    }

    #[allow(dead_code)] // The Plugins tab of a later change reads it.
    pub(crate) fn access(&self) -> &RegistryAccess {
        &self.access
    }

    #[allow(dead_code)] // The Plugins tab of a later change reads it.
    pub(crate) fn store(&self) -> &Store {
        &self.store
    }

    pub(crate) fn settings_path(&self) -> &Path {
        &self.settings_path
    }

    pub(crate) fn celesta(&self) -> &Version {
        &self.celesta
    }

    pub(crate) fn host(&self) -> &Target {
        &self.host
    }

    #[allow(dead_code)] // The Plugins tab of a later change reads it.
    pub(crate) fn bundled(&self) -> Option<&Path> {
        self.bundled.as_deref()
    }
}
