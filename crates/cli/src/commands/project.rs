//! The commands that change one project's plugins: `add`, `remove`,
//! `install` (without `--global`), and `update`. Each takes the project
//! root, the nearest folder holding `celesta.config.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;
use std::process::ExitCode;

use celesta_plugin_host::{LOCK_FILE, Lockfile, check_lock, project_dependencies};
use celesta_registry_client::{
    ProjectError, ResolveInput, ResolveWarning, Update, install_locked, lock_project, resolve,
};
use celesta_registry_format::{AdvisoryList, Index};
use semver::{Version, VersionReq};

use super::{Failure, finish};
use crate::args::parse_spec;
use crate::config::{self, ProjectConfig};
use crate::session::{NO_KEY_MESSAGE, Session};
use crate::{USAGE, advisories, report};

fn not_a_dependency(id: &str) -> Failure {
    Failure(format!("{id} is not a dependency of this project"))
}

/// Adds `spec` (`id[@req]`) to the project's dependencies, locks it, and
/// installs it. Without a requirement, the version resolution chooses is
/// written, so `acme.charts` 1.2.3 is written as `"1.2.3"` (`^1.2.3`). A
/// media plugin needs `pin`, and `pin` needs a media plugin. On any failure
/// `celesta.config.json` and `celesta.lock` are put back as they were.
pub fn add(session: &Session, root: &Path, spec: &str, pin: bool) -> ExitCode {
    let (id, requirement) = match parse_spec(spec) {
        Ok(parsed) => parsed,
        Err(message) => {
            report::error(message);
            return ExitCode::from(USAGE);
        }
    };
    // As the user wrote it, which `parse_spec` accepted.
    let written = requirement.map(|_| &spec[id.len() + 1..]);
    let advisories = advisories::refresh(session);
    finish(add_dependency(
        session,
        root,
        &id,
        written,
        pin,
        advisories.as_ref(),
    ))
}

fn add_dependency(
    session: &Session,
    root: &Path,
    id: &str,
    written: Option<&str>,
    pin: bool,
    advisories: Option<&AdvisoryList>,
) -> Result<(), Failure> {
    let index = session.registry.index(id, session.now)?;
    let media = index.kind.roles.iter().any(|role| role == "media");
    if media && !pin {
        return Err(Failure(format!(
            "{id} is a media plugin; install it for every project with `celesta plugin install --global {id}`, or pin it to this project with --pin"
        )));
    }
    if pin && !media {
        return Err(Failure("--pin is for media plugins".to_owned()));
    }
    let requirement = match written {
        Some(text) => text.to_owned(),
        None => newest(session, root, id, index, advisories)?.to_string(),
    };

    let (mut project, config_bytes) = match ProjectConfig::load(root)? {
        Some((project, bytes)) => (project, Some(bytes)),
        None => (ProjectConfig::new(), None),
    };
    let lock_bytes = read_if_present(&root.join(LOCK_FILE))?;
    project.set_dependency(id, &requirement);
    let outcome = project.save(root).map_err(Failure::from).and_then(|()| {
        let only = Update::Only(BTreeSet::from([id.to_owned()]));
        lock_project(&session.environment(), root, &only).map_err(Failure::from)
    });
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(failure) => {
            put_back(root, config_bytes.as_deref(), lock_bytes.as_deref());
            return Err(failure);
        }
    };

    report::resolve_warnings(&outcome.warnings);
    println!(
        "added {id} {requirement}, locked {}",
        outcome.lock.plugins[id].version
    );
    print_installed(&outcome.installed);
    warn_record_error(outcome.record_error.as_deref());
    Ok(())
}

/// The version of `id` that resolution chooses for the project with any
/// version allowed.
fn newest(
    session: &Session,
    root: &Path,
    id: &str,
    index: Index,
    advisories: Option<&AdvisoryList>,
) -> Result<Version, Failure> {
    let targets = project_dependencies(root)?.project_targets();
    let dependencies = BTreeMap::from([(id.to_owned(), VersionReq::STAR)]);
    let indexes = BTreeMap::from([(id.to_owned(), index)]);
    // Its warnings come again from locking the project.
    let resolution = resolve(&ResolveInput {
        dependencies: &dependencies,
        indexes: &indexes,
        advisories,
        celesta: &session.celesta,
        targets: &targets,
        host: &session.host,
        registry: session.registry.base(),
        previous: None,
        update: &Update::All,
    })?;
    Ok(resolution.lock.plugins[id].version.clone())
}

/// The bytes of `path`, or `None` when there is no such file.
fn read_if_present(path: &Path) -> Result<Option<Vec<u8>>, Failure> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Failure(format!(
            "could not read {}: {error}",
            path.display()
        ))),
    }
}

/// Puts `celesta.config.json` and `celesta.lock` back as `config` and
/// `lock` (`None`: there was no such file), warning about one that could
/// not be.
fn put_back(root: &Path, config: Option<&[u8]>, lock: Option<&[u8]>) {
    let results = [
        ("celesta.config.json", config::restore(root, config)),
        (LOCK_FILE, config::restore_file(&root.join(LOCK_FILE), lock)),
    ];
    for (name, result) in results {
        if let Err(error) = result {
            report::warn(format_args!("could not put {name} back as it was: {error}"));
        }
    }
}

/// Removes `id` from the project's dependencies and from `celesta.lock`,
/// without the registry.
pub fn remove(root: &Path, id: &str) -> ExitCode {
    finish(remove_dependency(root, id))
}

fn remove_dependency(root: &Path, id: &str) -> Result<(), Failure> {
    let Some((mut project, config_bytes)) = ProjectConfig::load(root)? else {
        return Err(not_a_dependency(id));
    };
    if !project.remove_dependency(id) {
        return Err(not_a_dependency(id));
    }
    let mut lock = Lockfile::load(root)?;
    let lock_changed = lock
        .as_mut()
        .is_some_and(|lock| lock.plugins.remove(id).is_some());

    project.save(root)?;
    if let Some(lock) = lock.filter(|_| lock_changed)
        && let Err(error) = lock.save(root)
    {
        // The lock was replaced whole or not at all, so only the settings
        // changed.
        if let Err(error) = config::restore(root, Some(&config_bytes)) {
            report::warn(format_args!(
                "could not put celesta.config.json back as it was: {error}"
            ));
        }
        return Err(error.into());
    }
    println!("removed {id}");
    Ok(())
}

/// Installs what `celesta.lock` names when `locked` or `offline` is asked
/// for, or when the lock covers `celesta.config.json`; otherwise locks the
/// project again, keeping every entry that still holds, and installs that.
/// Without a registry key, only what the store already holds can be used.
/// A project with no dependencies and no lock has nothing to install.
pub fn install(session: &Session, root: &Path, locked: bool, offline: bool) -> ExitCode {
    match has_registry_plugins(root) {
        Ok(true) => {}
        Ok(false) => {
            println!("this project has no registry plugins");
            return ExitCode::SUCCESS;
        }
        Err(failure) => return finish(Err(failure)),
    }
    if !offline {
        advisories::refresh(session);
    }
    finish(install_project(session, root, locked, offline))
}

/// Whether the project has dependencies or a `celesta.lock`.
fn has_registry_plugins(root: &Path) -> Result<bool, Failure> {
    Ok(root.join(LOCK_FILE).exists() || !project_dependencies(root)?.dependencies.is_empty())
}

fn install_project(
    session: &Session,
    root: &Path,
    locked: bool,
    offline: bool,
) -> Result<(), Failure> {
    let lock = Lockfile::load(root)?;
    let checks_out = match &lock {
        Some(lock) => {
            let project = project_dependencies(root)?;
            check_lock(Some(lock), &project, &session.celesta).is_ok()
        }
        None => false,
    };
    let env = session.environment();
    if !(locked || offline || checks_out) {
        if !session.has_key {
            return Err(Failure(NO_KEY_MESSAGE.to_owned()));
        }
        let outcome = lock_project(&env, root, &Update::Keep)?;
        report::resolve_warnings(&outcome.warnings);
        print_installed(&outcome.installed);
        println!("wrote {LOCK_FILE}");
        warn_record_error(outcome.record_error.as_deref());
        return Ok(());
    }

    let outcome = install_locked(&env, root, offline || !session.has_key).map_err(|error| {
        match error {
            // Offline only for want of a key, not by choice.
            ProjectError::Missing { .. } if !offline && !session.has_key => {
                Failure(NO_KEY_MESSAGE.to_owned())
            }
            error => error.into(),
        }
    })?;
    print_installed(&outcome.installed);
    for (id, version, reason) in &outcome.skipped {
        println!("skipped {id} {version}: {reason}");
    }
    println!("{LOCK_FILE} is satisfied");
    // After the installs, which fetched the indexes of what they downloaded.
    if let Some(lock) = &lock {
        warn_about_locked_states(session, lock);
    }
    warn_record_error(outcome.record_error.as_deref());
    Ok(())
}

/// Warns about each locked version that the cached index marks yanked or
/// deprecated.
fn warn_about_locked_states(session: &Session, lock: &Lockfile) {
    for (id, locked) in &lock.plugins {
        let Some(index) = session.registry.cached_index(id) else {
            continue;
        };
        let Some(entry) = index
            .versions
            .iter()
            .find(|entry| entry.version == locked.version)
        else {
            continue;
        };
        if entry.yanked {
            report::warn(ResolveWarning::LockedYanked {
                id: id.clone(),
                version: entry.version.clone(),
            });
        }
        if let Some(deprecation) = index.deprecation(entry) {
            report::warn(ResolveWarning::Deprecated {
                id: id.clone(),
                version: entry.version.clone(),
                message: deprecation.message.clone(),
                replacement: deprecation.replacement.clone(),
            });
        }
    }
}

/// Resolves the project's dependencies again, `id` alone when given, to the
/// newest versions their requirements allow, and installs them.
pub fn update(session: &Session, root: &Path, id: Option<&str>) -> ExitCode {
    let update = match id {
        None => Update::All,
        Some(id) => match project_dependencies(root) {
            Ok(project) if project.dependencies.contains_key(id) => {
                Update::Only(BTreeSet::from([id.to_owned()]))
            }
            Ok(_) => return finish(Err(not_a_dependency(id))),
            Err(error) => return finish(Err(error.into())),
        },
    };
    advisories::refresh(session);
    finish(update_project(session, root, &update))
}

fn update_project(session: &Session, root: &Path, update: &Update) -> Result<(), Failure> {
    let previous = Lockfile::load(root)?;
    let outcome = lock_project(&session.environment(), root, update)?;
    report::resolve_warnings(&outcome.warnings);
    let changes = changes(previous.as_ref(), &outcome.lock);
    if changes.is_empty() {
        println!("{LOCK_FILE} is up to date");
    }
    for change in changes {
        println!("{change}");
    }
    print_installed(&outcome.installed);
    warn_record_error(outcome.record_error.as_deref());
    Ok(())
}

/// What moved from `previous` to `lock`: `{id} {old} -> {new}` for each
/// version that changed, `locked {id} {new}` for each id newly locked, and
/// `unlocked {id} {old}` for each id no longer locked.
fn changes(previous: Option<&Lockfile>, lock: &Lockfile) -> Vec<String> {
    let before = |id: &str| previous.and_then(|previous| previous.plugins.get(id));
    let mut changes = Vec::new();
    for (id, locked) in &lock.plugins {
        let new = &locked.version;
        match before(id) {
            Some(old) if old.version == *new => {}
            Some(old) => changes.push(format!("{id} {} -> {new}", old.version)),
            None => changes.push(format!("locked {id} {new}")),
        }
    }
    for (id, old) in previous.iter().flat_map(|previous| &previous.plugins) {
        if !lock.plugins.contains_key(id) {
            changes.push(format!("unlocked {id} {}", old.version));
        }
    }
    changes
}

fn print_installed(installed: &[(String, Version)]) {
    for (id, version) in installed {
        println!("installed {id} {version}");
    }
}

fn warn_record_error(error: Option<&str>) {
    if let Some(error) = error {
        report::warn(format_args!(
            "could not record this project for celesta plugin gc: {error}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use celesta_plugin_host::LockedPlugin;

    use super::*;

    fn lock(plugins: &[(&str, &str)]) -> Lockfile {
        Lockfile {
            celesta: Version::new(0, 9, 0),
            plugins: plugins
                .iter()
                .map(|(id, version)| {
                    let locked = LockedPlugin {
                        celesta: VersionReq::STAR,
                        registry: "https://registry.test".to_owned(),
                        targets: BTreeMap::new(),
                        version: Version::parse(version).unwrap(),
                        other: Default::default(),
                    };
                    ((*id).to_owned(), locked)
                })
                .collect(),
            targets: Vec::new(),
            other: Default::default(),
        }
    }

    #[test]
    fn changes_name_moved_new_and_dropped_ids() {
        let previous = lock(&[
            ("acme.a", "1.0.0"),
            ("acme.b", "1.0.0"),
            ("acme.c", "2.0.0"),
        ]);
        let next = lock(&[
            ("acme.a", "1.1.0"),
            ("acme.b", "1.0.0"),
            ("acme.d", "0.1.0"),
        ]);
        assert_eq!(
            changes(Some(&previous), &next),
            [
                "acme.a 1.0.0 -> 1.1.0",
                "locked acme.d 0.1.0",
                "unlocked acme.c 2.0.0"
            ]
        );
        assert!(changes(Some(&next), &next).is_empty());
        assert_eq!(changes(None, &next).len(), 3);
    }
}
