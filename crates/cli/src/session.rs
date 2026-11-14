//! What one `celesta` run works with: the registry it reaches, the store and
//! caches under the user's folder, and the running Celesta.

use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use celesta_plugin_host::{
    CELESTA_VERSION, bundled_plugin_dir, create_private_dir_all, registry_cache_dir,
    registry_store_dir, user_settings_path,
};
use celesta_registry_client::{
    DirTransport, Environment, HttpTransport, OFFICIAL_REGISTRY, Registry, Store, Transport,
    TransportError, official_root_key,
};
use celesta_registry_format::{FormatError, RootKey, Target};
use semver::Version;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

/// The registry's base URL, overriding the official registry.
pub const REGISTRY_VAR: &str = "CELESTA_REGISTRY";
/// The root public key of the registry [`REGISTRY_VAR`] names.
pub const ROOT_KEY_VAR: &str = "CELESTA_REGISTRY_ROOT_KEY";

/// Why a registry command cannot run in a build without the official key.
pub const NO_KEY_MESSAGE: &str = "this build of Celesta has no registry key yet, so it cannot reach the plugin registry; set CELESTA_REGISTRY and CELESTA_REGISTRY_ROOT_KEY to use another registry";

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("{set} is set but {missing} is not; set both to use another registry")]
    HalfRegistry {
        set: &'static str,
        missing: &'static str,
    },
    #[error(
        "{REGISTRY_VAR} must be an https://, http://, or file://<absolute folder> URL, not {0:?}"
    )]
    BadRegistry(String),
    #[error("{ROOT_KEY_VAR} is not a root key: {0}")]
    BadRootKey(FormatError),
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
        !matches!(self, Self::NoUserFolder | Self::CacheFolder { .. })
    }
}

/// How a registry's files are fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remote {
    Http,
    /// Read from this folder, for a `file://` registry.
    Dir(PathBuf),
}

impl Remote {
    /// How the registry at `base` is fetched; `None` for a URL that is not
    /// `https://`, `http://`, or `file://` with an absolute folder.
    pub fn of(base: &str) -> Option<Self> {
        if base.starts_with("https://") || base.starts_with("http://") {
            return Some(Self::Http);
        }
        let path = base.strip_prefix("file://")?;
        // `file:///C:/x` names `C:/x` on Windows.
        let path = match path.strip_prefix('/') {
            Some(rest) if cfg!(windows) && is_drive(rest) => rest,
            _ => path,
        };
        let path = PathBuf::from(path);
        path.is_absolute().then_some(Self::Dir(path))
    }

    /// A transport for the registry at `base`; an HTTP one limits each
    /// phase to `timeout` when it is given.
    fn transport(&self, base: &str, timeout: Option<Duration>) -> Box<dyn Transport> {
        match self {
            Self::Http => {
                Box::new(timeout.map_or_else(HttpTransport::new, HttpTransport::with_timeout))
            }
            Self::Dir(root) => Box::new(DirTransport {
                base: base.trim_end_matches('/').to_owned(),
                root: root.clone(),
            }),
        }
    }
}

/// Whether `path` starts with a drive letter, as `C:/x` does.
fn is_drive(path: &str) -> bool {
    matches!(path.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic())
}

/// The registry a run uses.
struct Selection {
    base: String,
    /// `None` when there is no key for it.
    access: Option<Access>,
    /// Whether the environment named it instead of the official registry.
    overridden: bool,
}

/// A registry the session can reach: its key and how it is fetched.
struct Access {
    root: RootKey,
    remote: Remote,
}

/// Fails every fetch: the registry of a build without a key.
struct NoKey;

impl Transport for NoKey {
    fn open(&self, url: &str) -> Result<Box<dyn Read + Send + '_>, TransportError> {
        Err(TransportError::Failed {
            url: url.to_owned(),
            message: NO_KEY_MESSAGE.to_owned(),
        })
    }
}

pub struct Session {
    pub registry: Registry,
    /// Whether the registry can be reached: there is a key for it.
    pub has_key: bool,
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
    access: Option<Access>,
}

impl Session {
    /// The session of this run: the registry the environment names, else
    /// the official one, and the user's folders.
    pub fn from_env() -> Result<Self, SessionError> {
        let var = |name| std::env::var_os(name).filter(|value| !value.is_empty());
        let selection = select_registry(var(REGISTRY_VAR), var(ROOT_KEY_VAR))?;
        let store = registry_store_dir().ok_or(SessionError::NoUserFolder)?;
        let cache = registry_cache_dir().ok_or(SessionError::NoUserFolder)?;
        let settings_path = user_settings_path().ok_or(SessionError::NoUserFolder)?;
        Self::new(selection, store, cache, settings_path)
    }

    /// The session for `selection` with the user's folders: the store, the
    /// registry cache folder ([`registry_cache_dir`]), and the settings.
    fn new(
        selection: Selection,
        store: PathBuf,
        cache_root: PathBuf,
        settings_path: PathBuf,
    ) -> Result<Self, SessionError> {
        let Selection {
            base,
            access,
            overridden,
        } = selection;
        let cache = if overridden {
            let digest = Sha256::digest(base.trim_end_matches('/').as_bytes());
            let digits: String = digest[..8]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let cache = cache_root.join(format!("override-{digits}"));
            create_private_dir_all(&cache).map_err(|source| SessionError::CacheFolder {
                path: cache.clone(),
                source,
            })?;
            cache
        } else {
            cache_root
        };
        let registry = match &access {
            Some(access) => Registry::new(
                &base,
                access.root,
                access.remote.transport(&base, None),
                cache.clone(),
            ),
            // Nothing is fetched, so nothing is verified with this key.
            None => Registry::new(&base, RootKey([0; 32]), Box::new(NoKey), cache.clone()),
        };
        Ok(Self {
            registry,
            has_key: access.is_some(),
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
        }
    }

    /// Another client of the session's registry whose HTTP fetches are each
    /// limited to `timeout`; `None` without a key.
    pub fn registry_within(&self, timeout: Duration) -> Option<Registry> {
        let access = self.access.as_ref()?;
        let base = self.registry.base();
        Some(Registry::new(
            base,
            access.root,
            access.remote.transport(base, Some(timeout)),
            self.cache.clone(),
        ))
    }
}

/// The registry base and, when there is a key, how to reach it: both
/// variables, else the official registry with its key if this build has one.
fn select_registry(
    registry: Option<OsString>,
    key: Option<OsString>,
) -> Result<Selection, SessionError> {
    let (registry, key) = match (registry, key) {
        (Some(registry), Some(key)) => (registry, key),
        (Some(_), None) => {
            return Err(SessionError::HalfRegistry {
                set: REGISTRY_VAR,
                missing: ROOT_KEY_VAR,
            });
        }
        (None, Some(_)) => {
            return Err(SessionError::HalfRegistry {
                set: ROOT_KEY_VAR,
                missing: REGISTRY_VAR,
            });
        }
        (None, None) => {
            let access = official_root_key().map(|root| Access {
                root,
                remote: Remote::Http,
            });
            return Ok(Selection {
                base: OFFICIAL_REGISTRY.to_owned(),
                access,
                overridden: false,
            });
        }
    };
    let base = registry
        .into_string()
        .map_err(|registry| SessionError::BadRegistry(registry.to_string_lossy().into_owned()))?;
    let remote = Remote::of(&base).ok_or_else(|| SessionError::BadRegistry(base.clone()))?;
    let root = key
        .to_str()
        .ok_or_else(|| {
            SessionError::BadRootKey(FormatError::Signature("root key: not UTF-8".to_owned()))
        })
        .and_then(|key| RootKey::from_base64(key).map_err(SessionError::BadRootKey))?;
    Ok(Selection {
        base,
        access: Some(Access { root, remote }),
        overridden: true,
    })
}

#[cfg(test)]
mod tests {
    use celesta_registry_client::testing::{TestRegistry, at};
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn file_urls_become_dir_transports() {
        let served = TestRegistry::file();
        let remote = Remote::of(served.base()).unwrap();
        assert_eq!(remote, Remote::Dir(served.served_dir().to_path_buf()));
        assert_eq!(Remote::of("https://registry.test"), Some(Remote::Http));
        assert_eq!(Remote::of("http://localhost:8080"), Some(Remote::Http));
        assert_eq!(Remote::of("file://relative/dir"), None);
        assert_eq!(Remote::of("ftp://registry.test"), None);

        // A client through that transport reads the served files.
        let Selection { base, access, .. } = select_registry(
            Some(served.base().into()),
            Some(served.root_key_base64().into()),
        )
        .unwrap();
        let access = access.unwrap();
        let cache = TempDir::new().unwrap();
        let registry = Registry::new(
            &base,
            access.root,
            access.remote.transport(&base, None),
            cache.path().to_path_buf(),
        );
        registry.keyring(at("2026-11-21T00:00:00Z")).unwrap();
    }

    #[test]
    fn an_override_registry_has_its_own_cache() {
        let user = TempDir::new().unwrap();
        let cache_root = user.path().join("registry-cache");
        let build = |selection| {
            Session::new(
                selection,
                user.path().join("registry"),
                cache_root.clone(),
                user.path().join("settings.json"),
            )
            .unwrap()
        };
        let key = TestRegistry::new().root_key_base64();
        let overridden = |base: &str| {
            build(select_registry(Some(base.into()), Some(key.clone().into())).unwrap())
        };
        let first = overridden("https://registry.test/");
        let again = overridden("https://registry.test");
        let other = overridden("https://other.test");
        assert_ne!(first.cache, cache_root);
        assert_eq!(first.cache.parent(), Some(cache_root.as_path()));
        assert!(first.cache.is_dir());
        assert_eq!(first.cache, again.cache);
        assert_ne!(first.cache, other.cache);
        let name = first.cache.file_name().unwrap().to_str().unwrap();
        let digits = name.strip_prefix("override-").unwrap();
        assert!(
            digits.len() == 16 && digits.bytes().all(|b| b.is_ascii_hexdigit()),
            "{name}"
        );

        let official = build(select_registry(None, None).unwrap());
        assert_eq!(official.cache, cache_root);
    }

    #[test]
    fn the_registry_variables_go_together() {
        let half = select_registry(Some("https://registry.test".into()), None);
        assert!(
            matches!(half, Err(SessionError::HalfRegistry { missing, .. }) if missing == ROOT_KEY_VAR)
        );
        let half = select_registry(None, Some("AAAA".into()));
        assert!(
            matches!(half, Err(SessionError::HalfRegistry { missing, .. }) if missing == REGISTRY_VAR)
        );
        let official = select_registry(None, None).unwrap();
        assert_eq!(official.base, OFFICIAL_REGISTRY);
        assert!(!official.overridden);
        assert!(
            official.access.is_none(),
            "this build has no official key yet"
        );
    }
}
