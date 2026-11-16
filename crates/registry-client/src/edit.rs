//! Changing a project's plugins: adding, removing, and updating
//! dependencies in `celesta.config.json` and `celesta.lock` together, so a
//! failure leaves both files as they were.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use celesta_plugin_host::{LOCK_FILE, Lockfile, PROJECT_CONFIG, project_dependencies};
use celesta_registry_format::Index;

use crate::config::{ConfigError, ProjectConfig, restore_config, restore_file};
use crate::error::ClientError;
use crate::offer::{NoOffer, OfferInput, OfferScope, offered_requirement, offered_version};
use crate::project::{Environment, LockOutcome, ProjectError, lock_project};
use crate::resolve::{ResolveError, ResolveInput, Update, resolve};

/// Why a project could not be edited.
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error(
        "{id} is a media plugin; install it for every project with `celesta plugin install --global {id}`, or pin it to this project with --pin"
    )]
    MediaNeedsPin { id: String },
    #[error("--pin is for media plugins")]
    PinNeedsMedia { id: String },
    #[error("{id} is not a dependency of this project")]
    NotADependency { id: String },
    #[error("{id}: {reason}")]
    NoOffer { id: String, reason: NoOffer },
    #[error("{0}")]
    Config(#[from] ConfigError),
    #[error("{0}")]
    Project(#[from] ProjectError),
    #[error("{0}")]
    Client(#[from] ClientError),
    #[error("could not read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// `cause` stopped the edit, and some file could not be put back as it
    /// was.
    #[error("{cause}")]
    NotPutBack {
        cause: Box<EditError>,
        /// The file names and why each could not be restored.
        failed: Vec<(&'static str, io::Error)>,
    },
}

/// A requirement that moves to the version the app offers, as its plugin
/// can no longer be resolved within the old one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    pub id: String,
    pub from: String,
    pub to: String,
}

/// Refuses a media plugin without `pin`, and `pin` without a media plugin.
pub fn check_pin(index: &Index, id: &str, pin: bool) -> Result<(), EditError> {
    let media = index.kind.is_media();
    if media && !pin {
        return Err(EditError::MediaNeedsPin { id: id.to_owned() });
    }
    if pin && !media {
        return Err(EditError::PinNeedsMedia { id: id.to_owned() });
    }
    Ok(())
}

/// The bytes of `path`, or `None` when there is no such file.
fn read_if_present(path: &Path) -> Result<Option<Vec<u8>>, EditError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(EditError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Puts `celesta.config.json` and `celesta.lock` back as `config` and
/// `lock` (`None`: there was no such file), and returns `cause`, wrapped
/// when a file could not be put back.
fn put_back(
    root: &Path,
    config: Option<&[u8]>,
    lock: Option<&[u8]>,
    cause: EditError,
) -> EditError {
    let results = [
        (PROJECT_CONFIG, restore_config(root, config)),
        (LOCK_FILE, restore_file(&root.join(LOCK_FILE), lock)),
    ];
    let failed: Vec<_> = results
        .into_iter()
        .filter_map(|(name, result)| result.err().map(|error| (name, error)))
        .collect();
    if failed.is_empty() {
        cause
    } else {
        EditError::NotPutBack {
            cause: Box::new(cause),
            failed,
        }
    }
}

/// Writes `id`'s `requirement` to the project's `celesta.config.json`
/// (creating the file when there is none), locks `id`, and installs it. A
/// media plugin needs `pin`, and `pin` needs a media plugin. On any failure
/// both files are put back as they were.
pub fn add_dependency(
    env: &Environment,
    root: &Path,
    id: &str,
    requirement: &str,
    pin: bool,
) -> Result<LockOutcome, EditError> {
    let index = env.registry.index(id, env.now)?;
    check_pin(&index, id, pin)?;

    let (mut project, config_bytes) = match ProjectConfig::load(root)? {
        Some((project, bytes)) => (project, Some(bytes)),
        None => (ProjectConfig::new(), None),
    };
    let lock_bytes = read_if_present(&root.join(LOCK_FILE))?;
    project.set_dependency(id, requirement);
    let only = Update::Only(BTreeSet::from([id.to_owned()]));
    project
        .save(root)
        .map_err(EditError::from)
        .and_then(|()| lock_project(env, root, &only).map_err(EditError::from))
        .map_err(|error| put_back(root, config_bytes.as_deref(), lock_bytes.as_deref(), error))
}

/// Removes `id` from the project's dependencies and from `celesta.lock`,
/// without the registry.
pub fn remove_dependency(root: &Path, id: &str) -> Result<(), EditError> {
    let not_a_dependency = || EditError::NotADependency { id: id.to_owned() };
    let Some((mut project, config_bytes)) = ProjectConfig::load(root)? else {
        return Err(not_a_dependency());
    };
    if !project.remove_dependency(id) {
        return Err(not_a_dependency());
    }
    let mut lock = Lockfile::load(root).map_err(ProjectError::from)?;
    let lock_changed = lock
        .as_mut()
        .is_some_and(|lock| lock.plugins.remove(id).is_some());

    project.save(root)?;
    if let Some(lock) = lock.filter(|_| lock_changed)
        && let Err(error) = lock.save(root)
    {
        // The lock was replaced whole or not at all, so only the settings
        // changed.
        let cause = EditError::from(ProjectError::from(error));
        return Err(match restore_config(root, Some(&config_bytes)) {
            Ok(()) => cause,
            Err(error) => EditError::NotPutBack {
                cause: Box::new(cause),
                failed: vec![(PROJECT_CONFIG, error)],
            },
        });
    }
    Ok(())
}

/// For each of `ids`, whether it resolves within its requirement; for each
/// that does not, the rewrite to `^<offered>` (the offer for the project,
/// or a pin for a media plugin). Fetches the indexes; changes nothing.
pub fn plan_update(
    env: &Environment,
    root: &Path,
    ids: &BTreeSet<String>,
    include_prereleases: bool,
) -> Result<Vec<Rewrite>, EditError> {
    let project = project_dependencies(root).map_err(ProjectError::from)?;
    let targets = project.project_targets();
    let advisories = env.registry.cached_advisories()?;
    let written = ProjectConfig::load(root)?.map(|(config, _)| config);
    let mut rewrites = Vec::new();
    for id in ids {
        let requirement = project
            .dependencies
            .get(id)
            .ok_or_else(|| EditError::NotADependency { id: id.clone() })?;
        let index = env.registry.index(id, env.now)?;
        let dependencies = BTreeMap::from([(id.clone(), requirement.clone())]);
        let indexes = BTreeMap::from([(id.clone(), index)]);
        let resolved = resolve(&ResolveInput {
            dependencies: &dependencies,
            indexes: &indexes,
            advisories: advisories.as_ref(),
            celesta: env.celesta,
            targets: &targets,
            host: env.host,
            registry: env.registry.base(),
            previous: None,
            update: &Update::All,
        });
        match resolved {
            Ok(_) => continue,
            Err(ResolveError::NoVersion { .. }) => {}
            Err(error) => return Err(ProjectError::from(error).into()),
        }
        let index = &indexes[id];
        let scope = if index.kind.is_media() {
            OfferScope::Pin
        } else {
            OfferScope::Project
        };
        let offered = offered_version(&OfferInput {
            index,
            advisories: advisories.as_ref(),
            celesta: env.celesta,
            host: env.host,
            targets: &targets,
            include_prereleases,
            scope,
        })
        .map_err(|reason| EditError::NoOffer {
            id: id.clone(),
            reason,
        })?;
        rewrites.push(Rewrite {
            id: id.clone(),
            from: written
                .as_ref()
                .and_then(|config| config.dependency(id))
                .map_or_else(|| requirement.to_string(), str::to_owned),
            to: offered_requirement(&offered).to_string(),
        });
    }
    Ok(rewrites)
}

/// Applies `rewrites` to `celesta.config.json`, then locks and installs
/// `ids` again. On any failure both files are put back as they were.
pub fn update_dependencies(
    env: &Environment,
    root: &Path,
    ids: &BTreeSet<String>,
    rewrites: &[Rewrite],
) -> Result<LockOutcome, EditError> {
    let config = ProjectConfig::load(root)?;
    let lock_bytes = read_if_present(&root.join(LOCK_FILE))?;
    let config_bytes = config.as_ref().map(|(_, bytes)| bytes.clone());
    let update = Update::Only(ids.clone());
    let apply = || -> Result<LockOutcome, EditError> {
        if !rewrites.is_empty() {
            let Some((mut project, _)) = config else {
                return Err(EditError::NotADependency {
                    id: rewrites[0].id.clone(),
                });
            };
            for rewrite in rewrites {
                project.set_dependency(&rewrite.id, &rewrite.to);
            }
            project.save(root)?;
        }
        Ok(lock_project(env, root, &update)?)
    };
    apply().map_err(|error| put_back(root, config_bytes.as_deref(), lock_bytes.as_deref(), error))
}

#[cfg(test)]
mod tests {
    use celesta_plugin_host::CELESTA_VERSION;
    use celesta_registry_format::Target;
    use semver::Version;
    use serde_json::json;
    use tempfile::TempDir;
    use time::OffsetDateTime;

    use super::*;
    use crate::Store;
    use crate::testing::{TestRegistry, at};

    const BAD_HASH: &str =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    struct Machine {
        _home: TempDir,
        registry: crate::Registry,
        store: Store,
        celesta: Version,
        host: Target,
    }

    impl Machine {
        fn new(registry: &TestRegistry, celesta: &str) -> Self {
            let home = TempDir::new().unwrap();
            Self {
                registry: registry.client(&home.path().join("cache")),
                store: Store::new(home.path().join("registry")),
                celesta: Version::parse(celesta).unwrap(),
                host: Target::parse("macos-aarch64").unwrap(),
                _home: home,
            }
        }

        fn env(&self) -> Environment<'_> {
            Environment {
                registry: &self.registry,
                store: &self.store,
                celesta: &self.celesta,
                host: &self.host,
                bundled: None,
                now: now(),
                progress: None,
            }
        }
    }

    fn now() -> OffsetDateTime {
        at("2026-11-20T01:00:00Z")
    }

    fn write_config(root: &Path, dependencies: &[(&str, &str)]) {
        let dependencies: serde_json::Map<_, _> = dependencies
            .iter()
            .map(|(id, req)| ((*id).to_owned(), json!(req)))
            .collect();
        std::fs::write(
            root.join(PROJECT_CONFIG),
            serde_json::to_vec(&json!({ "plugins": { "dependencies": dependencies } })).unwrap(),
        )
        .unwrap();
    }

    fn config_of(root: &Path) -> serde_json::Value {
        let bytes = std::fs::read(root.join(PROJECT_CONFIG)).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn lock_of(root: &Path) -> Lockfile {
        Lockfile::load(root).unwrap().unwrap()
    }

    fn ids(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    fn corrupt(registry: &TestRegistry, id: &str, version: &str) {
        registry.edit(id, version, |entry| {
            entry["targets"]["any"]["sha256"] = json!(BAD_HASH);
        });
    }

    #[test]
    fn add_writes_the_requirement_and_locks() {
        let registry = TestRegistry::new();
        registry.publish("acme.charts", "1.2.3", &["any"]);
        let machine = Machine::new(&registry, CELESTA_VERSION);
        let root = TempDir::new().unwrap();

        let outcome =
            add_dependency(&machine.env(), root.path(), "acme.charts", "^1.2.3", false).unwrap();

        let version = Version::new(1, 2, 3);
        assert_eq!(
            config_of(root.path())["plugins"]["dependencies"]["acme.charts"],
            "^1.2.3"
        );
        assert_eq!(lock_of(root.path()).plugins["acme.charts"].version, version);
        assert_eq!(outcome.lock.plugins["acme.charts"].version, version);
        assert!(machine.store.path("acme.charts", &version).is_dir());
    }

    #[test]
    fn a_failed_add_puts_both_files_back() {
        let registry = TestRegistry::new();
        registry.publish("acme.good", "1.0.0", &["any"]);
        registry.publish("acme.bad", "1.0.0", &["any"]);
        corrupt(&registry, "acme.bad", "1.0.0");
        let machine = Machine::new(&registry, CELESTA_VERSION);
        let root = TempDir::new().unwrap();
        write_config(root.path(), &[("acme.good", "1")]);
        lock_project(&machine.env(), root.path(), &Update::Keep).unwrap();
        let config = std::fs::read(root.path().join(PROJECT_CONFIG)).unwrap();
        let lock = std::fs::read(root.path().join(LOCK_FILE)).unwrap();

        let error = add_dependency(&machine.env(), root.path(), "acme.bad", "1", false);

        assert!(matches!(
            error,
            Err(EditError::Project(ProjectError::Install { .. }))
        ));
        assert_eq!(
            std::fs::read(root.path().join(PROJECT_CONFIG)).unwrap(),
            config
        );
        assert_eq!(std::fs::read(root.path().join(LOCK_FILE)).unwrap(), lock);
    }

    #[test]
    fn a_failed_add_without_files_leaves_none() {
        let registry = TestRegistry::new();
        registry.publish("acme.bad", "1.0.0", &["any"]);
        corrupt(&registry, "acme.bad", "1.0.0");
        let machine = Machine::new(&registry, CELESTA_VERSION);
        let root = TempDir::new().unwrap();

        let error = add_dependency(&machine.env(), root.path(), "acme.bad", "1", false);

        assert!(matches!(
            error,
            Err(EditError::Project(ProjectError::Install { .. }))
        ));

        assert!(!root.path().join(PROJECT_CONFIG).exists());
        assert!(!root.path().join(LOCK_FILE).exists());
    }

    #[test]
    fn a_media_plugin_needs_a_pin() {
        let registry = TestRegistry::new();
        registry.publish_with_roles("acme.vrm", "1.0.0", &["any"], &["media"]);
        let machine = Machine::new(&registry, CELESTA_VERSION);
        let root = TempDir::new().unwrap();

        let error = add_dependency(&machine.env(), root.path(), "acme.vrm", "1", false);

        assert!(matches!(error, Err(EditError::MediaNeedsPin { id }) if id == "acme.vrm"));
        assert!(!root.path().join(PROJECT_CONFIG).exists());
    }

    #[test]
    fn a_pin_needs_a_media_plugin() {
        let registry = TestRegistry::new();
        registry.publish("acme.charts", "1.0.0", &["any"]);
        let machine = Machine::new(&registry, CELESTA_VERSION);
        let root = TempDir::new().unwrap();

        let error = add_dependency(&machine.env(), root.path(), "acme.charts", "1", true);

        assert!(matches!(error, Err(EditError::PinNeedsMedia { id }) if id == "acme.charts"));
        assert!(!root.path().join(PROJECT_CONFIG).exists());
    }

    #[test]
    fn remove_drops_the_dependency_and_the_lock_entry() {
        let registry = TestRegistry::new();
        registry.publish("acme.a", "1.0.0", &["any"]);
        registry.publish("acme.b", "1.0.0", &["any"]);
        let machine = Machine::new(&registry, CELESTA_VERSION);
        let root = TempDir::new().unwrap();
        write_config(root.path(), &[("acme.a", "1"), ("acme.b", "1")]);
        lock_project(&machine.env(), root.path(), &Update::Keep).unwrap();

        remove_dependency(root.path(), "acme.a").unwrap();

        let config = config_of(root.path());
        assert!(config["plugins"]["dependencies"].get("acme.a").is_none());
        assert_eq!(config["plugins"]["dependencies"]["acme.b"], "1");
        let lock = lock_of(root.path());
        assert!(!lock.plugins.contains_key("acme.a"));
        assert!(lock.plugins.contains_key("acme.b"));
    }

    #[test]
    fn removing_an_unknown_id_is_an_error() {
        let root = TempDir::new().unwrap();
        write_config(root.path(), &[("acme.a", "1")]);
        let error = remove_dependency(root.path(), "acme.b");
        assert!(matches!(error, Err(EditError::NotADependency { id }) if id == "acme.b"));

        let empty = TempDir::new().unwrap();
        let error = remove_dependency(empty.path(), "acme.b");
        assert!(matches!(error, Err(EditError::NotADependency { .. })));
    }

    /// A project locked at `vrm 0.10.0-alpha.3` and `code 1.0.0`, whose
    /// registry then gained `vrm 0.11.0` and `code 1.1.0`, for Celesta
    /// 0.11.0.
    fn moved_on() -> (TestRegistry, TempDir, Machine) {
        let registry = TestRegistry::new();
        registry.publish("acme.vrm", "0.10.0-alpha.3", &["any"]);
        registry.publish("acme.code", "1.0.0", &["any"]);
        let root = TempDir::new().unwrap();
        write_config(
            root.path(),
            &[("acme.vrm", "^0.10.0-alpha.3"), ("acme.code", "^1.0")],
        );
        let first = Machine::new(&registry, CELESTA_VERSION);
        lock_project(&first.env(), root.path(), &Update::Keep).unwrap();

        registry.publish("acme.vrm", "0.11.0", &["any"]);
        registry.edit("acme.vrm", "0.11.0", |entry| {
            entry["celesta"] = json!(">=0.11.0, <0.12.0");
        });
        registry.publish("acme.code", "1.1.0", &["any"]);
        for version in ["1.0.0", "1.1.0"] {
            registry.edit("acme.code", version, |entry| {
                entry["celesta"] = json!(">=0.10.0-alpha.3, <0.12.0");
            });
        }
        let machine = Machine::new(&registry, "0.11.0");
        (registry, root, machine)
    }

    #[test]
    fn plan_update_rewrites_only_what_cannot_be_met() {
        let (_registry, root, machine) = moved_on();

        let plan = plan_update(
            &machine.env(),
            root.path(),
            &ids(&["acme.vrm", "acme.code"]),
            false,
        )
        .unwrap();

        assert_eq!(
            plan,
            [Rewrite {
                id: "acme.vrm".to_owned(),
                from: "^0.10.0-alpha.3".to_owned(),
                to: "^0.11.0".to_owned(),
            }]
        );
    }

    #[test]
    fn plan_update_names_an_id_that_is_not_a_dependency() {
        let (_registry, root, machine) = moved_on();
        let error = plan_update(&machine.env(), root.path(), &ids(&["acme.none"]), false);
        assert!(matches!(error, Err(EditError::NotADependency { .. })));
    }

    #[test]
    fn update_with_a_rewrite_changes_the_config_and_lock() {
        let (_registry, root, machine) = moved_on();
        let all = ids(&["acme.vrm", "acme.code"]);
        let plan = plan_update(&machine.env(), root.path(), &all, false).unwrap();

        update_dependencies(&machine.env(), root.path(), &all, &plan).unwrap();

        let config = config_of(root.path());
        assert_eq!(config["plugins"]["dependencies"]["acme.vrm"], "^0.11.0");
        assert_eq!(config["plugins"]["dependencies"]["acme.code"], "^1.0");
        let lock = lock_of(root.path());
        assert_eq!(lock.plugins["acme.vrm"].version, Version::new(0, 11, 0));
        assert_eq!(lock.celesta, Version::new(0, 11, 0));
    }

    #[test]
    fn a_failed_update_puts_both_files_back() {
        let (registry, root, machine) = moved_on();
        corrupt(&registry, "acme.vrm", "0.11.0");
        let all = ids(&["acme.vrm", "acme.code"]);
        let plan = plan_update(&machine.env(), root.path(), &all, false).unwrap();
        let config = std::fs::read(root.path().join(PROJECT_CONFIG)).unwrap();
        let lock = std::fs::read(root.path().join(LOCK_FILE)).unwrap();

        let result = update_dependencies(&machine.env(), root.path(), &all, &plan);

        assert!(matches!(
            result,
            Err(EditError::Project(ProjectError::Install { .. }))
        ));
        assert_eq!(
            std::fs::read(root.path().join(PROJECT_CONFIG)).unwrap(),
            config
        );
        assert_eq!(std::fs::read(root.path().join(LOCK_FILE)).unwrap(), lock);
    }

    #[test]
    fn the_rewrite_names_the_requirement_as_written() {
        let (_registry, root, machine) = moved_on();
        write_config(root.path(), &[("acme.vrm", "0.10.0-alpha.3")]);

        let plan = plan_update(&machine.env(), root.path(), &ids(&["acme.vrm"]), false).unwrap();

        assert_eq!(plan[0].from, "0.10.0-alpha.3");
        assert_eq!(plan[0].to, "^0.11.0");
    }

    #[test]
    fn plan_update_pins_a_media_plugin() {
        let registry = TestRegistry::new();
        registry.publish_with_roles("acme.vrm", "0.10.0-alpha.3", &["any"], &["media"]);
        let root = TempDir::new().unwrap();
        write_config(root.path(), &[("acme.vrm", "^0.10.0-alpha.3")]);
        registry.publish_with_roles("acme.vrm", "0.11.0", &["macos-aarch64"], &["media"]);
        registry.edit("acme.vrm", "0.11.0", |entry| {
            entry["celesta"] = json!(">=0.11.0, <0.12.0");
        });
        let machine = Machine::new(&registry, "0.11.0");

        let plan = plan_update(&machine.env(), root.path(), &ids(&["acme.vrm"]), false).unwrap();

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].to, "^0.11.0");
    }

    #[test]
    fn update_without_rewrites_leaves_the_config_alone() {
        let (_registry, root, _) = moved_on();
        let registry = TestRegistry::new();
        registry.publish("acme.code", "1.0.0", &["any"]);
        write_config(root.path(), &[("acme.code", "^1.0")]);
        let machine = Machine::new(&registry, CELESTA_VERSION);
        std::fs::remove_file(root.path().join(LOCK_FILE)).unwrap();
        let config = std::fs::read(root.path().join(PROJECT_CONFIG)).unwrap();

        update_dependencies(&machine.env(), root.path(), &ids(&["acme.code"]), &[]).unwrap();

        assert_eq!(
            std::fs::read(root.path().join(PROJECT_CONFIG)).unwrap(),
            config
        );
        assert_eq!(
            lock_of(root.path()).plugins["acme.code"].version,
            Version::new(1, 0, 0)
        );
    }
}
