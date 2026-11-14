//! The project and global operations the `celesta` command and the desktop
//! app call: resolving a project's dependencies into `celesta.lock` and
//! installing them, installing what a lock names without changing it, and
//! installing a plugin for the user alone.

use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::path::Path;

use celesta_plugin_host::{
    LockError, LockProblem, LockedArtifact, Lockfile, PluginEntry, PluginSource,
    ProjectConfigError, RegistryPlugin, SearchPath, SettingsError, UserSettings, check_lock,
    discover_in, is_build, project_dependencies,
};
use celesta_registry_format::{AdvisoryList, Artifact, Hash, Index, Target};
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
    /// Why the project could not be recorded as a user of the store, so
    /// that `celesta plugin gc` may not know of it; the lock and the
    /// installs are done.
    pub record_error: Option<String>,
}

/// What [`install_locked`] installed and passed over.
#[derive(Debug)]
pub struct InstallOutcome {
    /// The versions downloaded into the store, in id order.
    pub installed: Vec<(String, Version)>,
    /// The locked versions with no build for this host, with why.
    pub skipped: Vec<(String, Version, String)>,
    /// Why the project could not be recorded as a user of the store; see
    /// [`LockOutcome::record_error`].
    pub record_error: Option<String>,
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
    #[error("{0}")]
    LockCheck(#[from] LockProblem),
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
    let advisories = env.registry.cached_advisories()?;
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
    // Before the installs, so a failed one still leaves the project known.
    let record_error = record_error(env, root);

    let bundled = BundledPlugins::new(env.bundled);
    let mut installed = Vec::new();
    for (id, locked) in &lock.plugins {
        let Some((target, artifact)) = locked_artifact(&lock, id, env.host) else {
            continue;
        };
        if holds_locked_build(
            env.store,
            &bundled,
            id,
            &locked.version,
            &artifact.tree_hash,
        ) {
            continue;
        }
        install_as_locked(
            env,
            &indexes[id],
            advisories.as_ref(),
            id,
            &locked.version,
            (target, artifact),
        )?;
        installed.push((id.clone(), locked.version.clone()));
    }
    Ok(LockOutcome {
        lock,
        warnings: resolution.warnings,
        installed,
        record_error,
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
    let lock = Lockfile::load(root)?.ok_or(LockProblem::NoLock)?;
    let project = project_dependencies(root)?;
    check_lock(Some(&lock), &project, env.celesta)?;

    let advisories = env.registry.cached_advisories()?;
    let bundled = BundledPlugins::new(env.bundled);
    let mut outcome = InstallOutcome {
        installed: Vec::new(),
        skipped: Vec::new(),
        record_error: None,
    };
    for (id, locked) in &lock.plugins {
        let version = &locked.version;
        let Some((target, artifact)) = locked_artifact(&lock, id, env.host) else {
            outcome.skipped.push((
                id.clone(),
                version.clone(),
                format!("no build for {}", env.host.as_str()),
            ));
            continue;
        };
        if holds_locked_build(env.store, &bundled, id, version, &artifact.tree_hash) {
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
        install_as_locked(
            env,
            &index,
            advisories.as_ref(),
            id,
            version,
            (target, artifact),
        )?;
        outcome.installed.push((id.clone(), version.clone()));
    }
    outcome.record_error = record_error(env, root);
    Ok(outcome)
}

/// Records `root` in the store; a failure is returned as text, as it does
/// not undo the operation it follows.
fn record_error(env: &Environment, root: &Path) -> Option<String> {
    env.store
        .record_project(root)
        .err()
        .map(|error| format!("could not record {} as a project: {error}", root.display()))
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
    let advisories = env.registry.cached_advisories()?;
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
    let (target, artifact) = locked_artifact(&resolution.lock, id, env.host)
        .map(|(target, artifact)| (target.clone(), *artifact))
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
        (&target, &artifact),
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

/// Whether `store` or a bundled copy already holds the build of `version`
/// of `id` with `tree_hash`.
pub fn holds_locked_build(
    store: &Store,
    bundled: &BundledPlugins,
    id: &str,
    version: &Version,
    tree_hash: &Hash,
) -> bool {
    store.contains(id, version, tree_hash) || bundled.holds(id, version, tree_hash)
}

/// The locked plugin's artifact for `host`, else the one for `any`, with
/// the target it is locked under.
fn locked_artifact<'a>(
    lock: &'a Lockfile,
    id: &str,
    host: &'a Target,
) -> Option<(&'a Target, &'a LockedArtifact)> {
    let targets = &lock.plugins.get(id)?.targets;
    targets
        .get_key_value(host)
        .or_else(|| targets.get_key_value(&Target::ANY))
}

/// Installs `version` of `id` from `index`, which must list it as
/// distributed, unblocked, and with the locked hashes for the host's build.
fn install_as_locked(
    env: &Environment,
    index: &Index,
    advisories: Option<&AdvisoryList>,
    id: &str,
    version: &Version,
    (target, locked): (&Target, &LockedArtifact),
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
    // The build the lock chose, under the same target key: a version that
    // gained a host build since is not a mismatch.
    let artifact: &Artifact = entry
        .targets
        .get(target)
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
pub struct BundledPlugins<'a> {
    folder: Option<&'a Path>,
    entries: OnceCell<Vec<PluginEntry>>,
}

impl<'a> BundledPlugins<'a> {
    /// The plugins in `folder`, the bundled plugin folder; none without one.
    pub fn new(folder: Option<&'a Path>) -> Self {
        Self {
            folder,
            entries: OnceCell::new(),
        }
    }

    /// Whether a bundled plugin has this id, version, and tree hash.
    pub fn holds(&self, id: &str, version: &Version, tree_hash: &Hash) -> bool {
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
        entries
            .iter()
            .any(|entry| is_build(&entry.directory, id, version, tree_hash))
    }
}
