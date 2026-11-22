//! Which registry a run uses: the official one, or the one
//! `CELESTA_REGISTRY` names, and how its files are fetched and cached.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use celesta_plugin_host::create_private_dir_all;
use celesta_registry_format::{FormatError, RootKey};
use sha2::{Digest, Sha256};

use crate::{
    DirTransport, HttpTransport, OFFICIAL_REGISTRY, Registry, Transport, official_root_key,
};

/// The registry's base URL, overriding the official registry.
pub const REGISTRY_VAR: &str = "CELESTA_REGISTRY";
/// The root public key of the registry [`REGISTRY_VAR`] names.
pub const ROOT_KEY_VAR: &str = "CELESTA_REGISTRY_ROOT_KEY";

#[derive(Debug, thiserror::Error)]
pub enum SelectionError {
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
}

/// Whether `path` starts with a drive letter, as `C:/x` does.
fn is_drive(path: &str) -> bool {
    matches!(path.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic())
}

/// The registry a run uses: its base URL, its key, and how it is fetched.
#[derive(Debug, Clone)]
pub struct RegistryAccess {
    pub base: String,
    pub root: RootKey,
    pub remote: Remote,
    /// Whether the environment named it instead of the official registry.
    pub overridden: bool,
}

impl RegistryAccess {
    /// The registry the environment names, else the official one.
    pub fn from_env() -> Result<Self, SelectionError> {
        let var = |name| std::env::var_os(name).filter(|value| !value.is_empty());
        select_registry(var(REGISTRY_VAR), var(ROOT_KEY_VAR))
    }

    /// A transport for the registry; an HTTP one limits each phase to
    /// `timeout` when it is given.
    pub fn transport(&self, timeout: Option<Duration>) -> Box<dyn Transport> {
        match &self.remote {
            Remote::Http => {
                Box::new(timeout.map_or_else(HttpTransport::new, HttpTransport::with_timeout))
            }
            Remote::Dir(root) => Box::new(DirTransport {
                base: self.base.trim_end_matches('/').to_owned(),
                root: root.clone(),
            }),
        }
    }

    /// The folder this registry's verified files are cached in: `cache_root`
    /// for the official registry, and `override-<16 hex digits>` in it for
    /// an overriding one, the digits the start of the SHA-256 of the base
    /// URL without a trailing `/`. Rollback stamps and the advisory list
    /// are kept per file path, so registries sharing a folder would be
    /// checked against each other's files, and the desktop app and the
    /// exporter would read another registry's advisories. No served path
    /// is named `override-…`.
    pub fn cache_dir(&self, cache_root: &Path) -> io::Result<PathBuf> {
        let cache = self.cache_path(cache_root);
        if self.overridden {
            create_private_dir_all(&cache)?;
        }
        Ok(cache)
    }

    /// Where [`Self::cache_dir`] keeps the cache, without creating it.
    pub fn cache_path(&self, cache_root: &Path) -> PathBuf {
        if !self.overridden {
            return cache_root.to_path_buf();
        }
        let digest = Sha256::digest(self.base.trim_end_matches('/').as_bytes());
        let digits: String = digest[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        cache_root.join(format!("override-{digits}"))
    }

    /// A client of the registry caching in [`Self::cache_dir`].
    pub fn client(&self, cache_root: &Path, timeout: Option<Duration>) -> io::Result<Registry> {
        Ok(Registry::new(
            &self.base,
            self.root,
            self.transport(timeout),
            self.cache_dir(cache_root)?,
        ))
    }
}

/// The registry base, its key, and how to reach it: both variables, else
/// the official registry with its pinned key.
pub fn select_registry(
    registry: Option<OsString>,
    key: Option<OsString>,
) -> Result<RegistryAccess, SelectionError> {
    let (registry, key) = match (registry, key) {
        (Some(registry), Some(key)) => (registry, key),
        (Some(_), None) => {
            return Err(SelectionError::HalfRegistry {
                set: REGISTRY_VAR,
                missing: ROOT_KEY_VAR,
            });
        }
        (None, Some(_)) => {
            return Err(SelectionError::HalfRegistry {
                set: ROOT_KEY_VAR,
                missing: REGISTRY_VAR,
            });
        }
        (None, None) => {
            return Ok(RegistryAccess {
                base: OFFICIAL_REGISTRY.to_owned(),
                root: official_root_key(),
                remote: Remote::Http,
                overridden: false,
            });
        }
    };
    let base = registry
        .into_string()
        .map_err(|registry| SelectionError::BadRegistry(registry.to_string_lossy().into_owned()))?;
    let remote = Remote::of(&base).ok_or_else(|| SelectionError::BadRegistry(base.clone()))?;
    let root = key
        .to_str()
        .ok_or_else(|| {
            SelectionError::BadRootKey(FormatError::Signature("root key: not UTF-8".to_owned()))
        })
        .and_then(|key| RootKey::from_base64(key).map_err(SelectionError::BadRootKey))?;
    Ok(RegistryAccess {
        base,
        root,
        remote,
        overridden: true,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::testing::{TestRegistry, at};

    #[test]
    fn file_urls_become_dir_transports() {
        let now = at("2026-11-21T00:00:00Z");
        let served = TestRegistry::file_at(now);
        let remote = Remote::of(served.base()).unwrap();
        assert_eq!(remote, Remote::Dir(served.served_dir().to_path_buf()));
        assert_eq!(Remote::of("https://registry.test"), Some(Remote::Http));
        assert_eq!(Remote::of("http://localhost:8080"), Some(Remote::Http));
        assert_eq!(Remote::of("file://relative/dir"), None);
        assert_eq!(Remote::of("ftp://registry.test"), None);

        // A client through that transport reads the served files.
        let access = select_registry(
            Some(served.base().into()),
            Some(served.root_key_base64().into()),
        )
        .unwrap();
        let cache = TempDir::new().unwrap();
        let registry = access.client(cache.path(), None).unwrap();
        registry.keyring(now).unwrap();
    }

    #[test]
    fn an_override_registry_has_its_own_cache() {
        let user = TempDir::new().unwrap();
        let cache_root = user.path().join("registry-cache");
        let key = TestRegistry::new().root_key_base64();
        let overridden = |base: &str| {
            select_registry(Some(base.into()), Some(key.clone().into()))
                .unwrap()
                .cache_dir(&cache_root)
                .unwrap()
        };
        let first = overridden("https://registry.test/");
        let again = overridden("https://registry.test");
        let other = overridden("https://other.test");
        assert_ne!(first, cache_root);
        assert_eq!(first.parent(), Some(cache_root.as_path()));
        assert!(first.is_dir());
        assert_eq!(first, again);
        assert_ne!(first, other);
        let name = first.file_name().unwrap().to_str().unwrap();
        let digits = name.strip_prefix("override-").unwrap();
        assert!(
            digits.len() == 16 && digits.bytes().all(|b| b.is_ascii_hexdigit()),
            "{name}"
        );

        let official = select_registry(None, None).unwrap();
        assert_eq!(official.cache_dir(&cache_root).unwrap(), cache_root);
    }

    #[test]
    fn the_registry_variables_go_together() {
        let half = select_registry(Some("https://registry.test".into()), None);
        assert!(
            matches!(half, Err(SelectionError::HalfRegistry { missing, .. }) if missing == ROOT_KEY_VAR)
        );
        let half = select_registry(None, Some("AAAA".into()));
        assert!(
            matches!(half, Err(SelectionError::HalfRegistry { missing, .. }) if missing == REGISTRY_VAR)
        );
        let official = select_registry(None, None).unwrap();
        assert_eq!(official.base, OFFICIAL_REGISTRY);
        assert!(!official.overridden);
        assert_eq!(official.root, official_root_key());
        assert_eq!(official.remote, Remote::Http);
    }
}
