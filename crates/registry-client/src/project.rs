//! The project and global operations the `celesta` command and the desktop
//! app call: resolving a project's dependencies into `celesta.lock` and
//! installing them, installing what a lock names without changing it, and
//! installing a plugin for the user alone.

use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::path::Path;

use celesta_plugin_host::{
    LockError, LockedArtifact, Lockfile, PluginEntry, PluginSource, ProjectConfigError,
    RegistryPlugin, SearchPath, SettingsError, UserSettings, discover_in, project_dependencies,
};
use celesta_registry_format::{AdvisoryList, Artifact, Index, Target, tree_hash};
use semver::{Version, VersionReq};
use time::OffsetDateTime;

use crate::error::ClientError;
use crate::registry::Registry;
use crate::resolve::{ResolveError, ResolveInput, ResolveWarning, Update, resolve};
use crate::store::{InstallError, InstallRequest, Store};

/// What the operations work with: the registry, the store, the running
/// Celesta and its target, and the folder of bundled plugins.
pub struct Environment<'a> {
    pub registry: &'a Registry,
    pub store: &'a Store,
    pub celesta: &'a Version,
    pub host: &'a Target,
    /// The bundled plugins' folder ([`celesta_plugin_host::bundled_plugin_dir`]
    /// in production): a copy there with a locked version's tree hash
    /// satisfies the lock without a download.
    pub bundled: Option<&'a Path>,
    pub now: OffsetDateTime,
}

/// What [`lock_project`] wrote and installed.
#[derive(Debug)]
pub struct LockOutcome {
    pub lock: Lockfile,
    pub warnings: Vec<ResolveWarning>,
    /// The versions downloaded into the store, in id order.
    pub installed: Vec<(String, Version)>,
}

/// What [`install_locked`] installed and passed over.
#[derive(Debug)]
pub struct InstallOutcome {
    /// The versions downloaded into the store, in id order.
    pub installed: Vec<(String, Version)>,
    /// The locked versions with no build for this host, with why.
    pub skipped: Vec<(String, Version, String)>,
}

/// Why a project's plugins, or a global one, could not be locked or
/// installed.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("{0}")]
    Config(#[from] ProjectConfigError),
    #[error("{0}")]
    Lock(#[from] LockError),
    #[error("{0}")]
    Client(#[from] ClientError),
    #[error("{0}")]
    Resolve(#[from] ResolveError),
    #[error("could not install {id} {version}: {source}")]
    Install {
        id: String,
        version: Version,
        /// Boxed, as it would make every `ProjectError` large.
        source: Box<InstallError>,
    },
    #[error("the project has no celesta.lock; run `celesta plugin update` to write one")]
    NoLock,
    #[error(
        "celesta.lock does not match celesta.config.json for {}; run `celesta plugin update`",
        ids.join(", ")
    )]
    OutOfDate { ids: Vec<String> },
    #[error(
        "celesta.lock was resolved with Celesta {locked}, and this is Celesta {running}; {} cannot load with it. Run `celesta plugin update` to resolve them again",
        ids.join(", ")
    )]
    OtherCelesta {
        locked: Version,
        running: Version,
        ids: Vec<String>,
    },
    #[error("{id} {version} is not installed, and this run is offline")]
    Missing { id: String, version: Version },
    #[error("{id} {version} is no longer distributed")]
    Withdrawn { id: String, version: Version },
    #[error("{id} {version} is blocked by advisory {advisory}")]
    Blocked {
        id: String,
        version: Version,
        advisory: String,
    },
    #[error("celesta.lock's hashes for {id} {version} are not the ones the registry lists")]
    LockMismatch { id: String, version: Version },
    #[error("{0}")]
    Settings(#[from] SettingsError),
}

/// Resolves the project's dependencies (fetching their indexes), writes
/// `celesta.lock`, and installs each locked version's build for the host
/// (else its `any` build) that the store or a bundled copy does not
/// already hold. A locked version with no such build, such as a pinned
/// media plugin for another OS, is left out quietly.
pub fn lock_project(
    env: &Environment,
    root: &Path,
    update: &Update,
) -> Result<LockOutcome, ProjectError> {
    let project = project_dependencies(root)?;
    let previous = Lockfile::load(root)?;
    let indexes = project
        .dependencies
        .keys()
        .map(|id| Ok((id.clone(), env.registry.index(id, env.now)?)))
        .collect::<Result<BTreeMap<_, _>, ClientError>>()?;
    let advisories = env.registry.cached_advisories();
    let resolution = resolve(&ResolveInput {
        dependencies: &project.dependencies,
        indexes: &indexes,
        advisories: advisories.as_ref(),
        celesta: env.celesta,
        targets: &project.project_targets(),
        host: env.host,
        registry: env.registry.base(),
        previous: previous.as_ref(),
        update,
    })?;
    let lock = resolution.lock;
    lock.save(root)?;

    let bundled = Bundled::new(env.bundled);
    let mut installed = Vec::new();
    for (id, locked) in &lock.plugins {
        let Some(artifact) = lock.artifact(id, env.host) else {
            continue;
        };
        if satisfied(env, &bundled, id, &locked.version, artifact) {
            continue;
        }
        install_as_locked(
            env,
            &indexes[id],
            advisories.as_ref(),
            id,
            &locked.version,
            artifact,
        )?;
        installed.push((id.clone(), locked.version.clone()));
    }
    Ok(LockOutcome {
        lock,
        warnings: resolution.warnings,
        installed,
    })
}

/// Installs what `celesta.lock` names, never changing it.
///
/// Fails when there is no lock, when it names versions the running
/// Celesta cannot load, and when it does not cover `celesta.config.json`.
/// Then each locked version's build for the host (else its `any` build) is
/// installed unless the store or a bundled copy already holds it; one with
/// no such build is skipped. A version the registry no longer lists,
/// withdrew, or blocks fails, as does one whose hashes the registry
/// contradicts. `offline` fails on the first version not already held
/// rather than fetch anything.
pub fn install_locked(
    env: &Environment,
    root: &Path,
    offline: bool,
) -> Result<InstallOutcome, ProjectError> {
    let lock = Lockfile::load(root)?.ok_or(ProjectError::NoLock)?;
    let unloadable = lock.unloadable(env.celesta);
    if !unloadable.is_empty() {
        return Err(ProjectError::OtherCelesta {
            locked: lock.celesta.clone(),
            running: env.celesta.clone(),
            ids: unloadable,
        });
    }
    let project = project_dependencies(root)?;
    let stale: Vec<String> = project
        .dependencies
        .iter()
        .filter(|(id, requirement)| {
            lock.plugins
                .get(*id)
                .is_none_or(|locked| !requirement.matches(&locked.version))
        })
        .map(|(id, _)| id.clone())
        .collect();
    if !stale.is_empty() {
        return Err(ProjectError::OutOfDate { ids: stale });
    }

    let advisories = env.registry.cached_advisories();
    let bundled = Bundled::new(env.bundled);
    let mut outcome = InstallOutcome {
        installed: Vec::new(),
        skipped: Vec::new(),
    };
    for (id, locked) in &lock.plugins {
        let version = &locked.version;
        let Some(artifact) = lock.artifact(id, env.host) else {
            outcome.skipped.push((
                id.clone(),
                version.clone(),
                format!("no build for {}", env.host.as_str()),
            ));
            continue;
        };
        if satisfied(env, &bundled, id, version, artifact) {
            continue;
        }
        if offline {
            return Err(ProjectError::Missing {
                id: id.clone(),
                version: version.clone(),
            });
        }
        let index = match env.registry.index(id, env.now) {
            Err(ClientError::NoSuchPlugin(_)) => {
                return Err(ProjectError::Withdrawn {
                    id: id.clone(),
                    version: version.clone(),
                });
            }
            other => other?,
        };
        install_as_locked(env, &index, advisories.as_ref(), id, version, artifact)?;
        outcome.installed.push((id.clone(), version.clone()));
    }
    Ok(outcome)
}

/// Resolves `id` against `requirement` for the host alone, installs it,
/// and records it in `settings_path`'s `registryPlugins`, replacing that
/// id's entry. The version chosen must have a build for the host or
/// `any`, a media plugin's too.
pub fn install_global(
    env: &Environment,
    settings_path: &Path,
    id: &str,
    requirement: &VersionReq,
) -> Result<RegistryPlugin, ProjectError> {
    let mut index = env.registry.index(id, env.now)?;
    // Resolution lets a project pin a media plugin without a build for
    // every target; installed for this host alone, it needs one here.
    index.kind.roles.retain(|role| role != "media");
    let advisories = env.registry.cached_advisories();
    let dependencies = BTreeMap::from([(id.to_owned(), requirement.clone())]);
    let indexes = BTreeMap::from([(id.to_owned(), index)]);
    let resolution = resolve(&ResolveInput {
        dependencies: &dependencies,
        indexes: &indexes,
        advisories: advisories.as_ref(),
        celesta: env.celesta,
        targets: std::slice::from_ref(env.host),
        host: env.host,
        registry: env.registry.base(),
        previous: None,
        update: &Update::All,
    })?;
    let version = resolution.lock.plugins[id].version.clone();
    let artifact = resolution
        .lock
        .artifact(id, env.host)
        .copied()
        .ok_or_else(|| ResolveError::NoVersion {
            id: id.to_owned(),
            requirement: requirement.clone(),
            reason: format!("no build for {}", env.host.as_str()),
        })?;
    install_as_locked(
        env,
        &indexes[id],
        advisories.as_ref(),
        id,
        &version,
        &artifact,
    )?;

    let installed = RegistryPlugin {
        id: id.to_owned(),
        version,
        tree_hash: artifact.tree_hash,
    };
    let mut settings = UserSettings::load(settings_path)?;
    settings.registry_plugins.retain(|plugin| plugin.id != id);
    settings.registry_plugins.push(installed.clone());
    settings.save(settings_path)?;
    Ok(installed)
}

/// Whether the store or a bundled copy already holds the locked build.
fn satisfied(
    env: &Environment,
    bundled: &Bundled,
    id: &str,
    version: &Version,
    artifact: &LockedArtifact,
) -> bool {
    env.store.contains(id, version, &artifact.tree_hash) || bundled.holds(id, version, artifact)
}

/// Installs `version` of `id` from `index`, which must list it as
/// distributed, unblocked, and with the locked hashes for the host's build.
fn install_as_locked(
    env: &Environment,
    index: &Index,
    advisories: Option<&AdvisoryList>,
    id: &str,
    version: &Version,
    locked: &LockedArtifact,
) -> Result<(), ProjectError> {
    let entry = index
        .versions
        .iter()
        .find(|entry| entry.version == *version && !entry.withdrawn)
        .ok_or_else(|| ProjectError::Withdrawn {
            id: id.to_owned(),
            version: version.clone(),
        })?;
    if let Some(advisory) = advisories.and_then(|list| list.blocking(id, version)) {
        return Err(ProjectError::Blocked {
            id: id.to_owned(),
            version: version.clone(),
            advisory: advisory.id.clone(),
        });
    }
    let artifact: &Artifact = entry
        .targets
        .get(env.host)
        .or_else(|| entry.targets.get(&Target::ANY))
        .filter(|artifact| {
            artifact.sha256 == locked.sha256 && artifact.tree_hash == locked.tree_hash
        })
        .ok_or_else(|| ProjectError::LockMismatch {
            id: id.to_owned(),
            version: version.clone(),
        })?;
    env.store
        .install(
            env.registry.transport(),
            &InstallRequest {
                id,
                version,
                url: &artifact.url,
                size: artifact.size,
                sha256: &artifact.sha256,
                tree_hash: &artifact.tree_hash,
            },
        )
        .map_err(|source| ProjectError::Install {
            id: id.to_owned(),
            version: version.clone(),
            source: Box::new(source),
        })?;
    Ok(())
}

/// The bundled plugins, discovered the first time one is asked for.
struct Bundled<'a> {
    folder: Option<&'a Path>,
    entries: OnceCell<Vec<PluginEntry>>,
}

impl<'a> Bundled<'a> {
    fn new(folder: Option<&'a Path>) -> Self {
        Self {
            folder,
            entries: OnceCell::new(),
        }
    }

    /// Whether a bundled plugin has this id, version, and the locked tree
    /// hash.
    fn holds(&self, id: &str, version: &Version, artifact: &LockedArtifact) -> bool {
        let Some(folder) = self.folder else {
            return false;
        };
        let entries = self.entries.get_or_init(|| {
            discover_in(&[SearchPath {
                path: folder.to_path_buf(),
                source: PluginSource::Bundled,
            }])
            .0
        });
        entries.iter().any(|entry| {
            entry.manifest.id == id
                && Version::parse(&entry.manifest.version).is_ok_and(|found| found == *version)
                && tree_hash(&entry.directory).is_ok_and(|found| found == artifact.tree_hash)
        })
    }
}
