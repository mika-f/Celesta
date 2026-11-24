//! The commands that change one project's plugins: `add`, `remove`,
//! `install` (without `--global`), and `update`. Each takes the project
//! root, the nearest folder holding `celesta.config.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;

use celesta_plugin_host::{LOCK_FILE, Lockfile, check_lock, project_dependencies};
use celesta_registry_client::{
    EditError, ResolveInput, ResolveWarning, Update, add_dependency as add_to_project, check_pin,
    install_locked, lock_project, remove_dependency as remove_from_project, resolve,
};
use celesta_registry_format::{AdvisoryList, Index};
use semver::{Version, VersionReq};
use serde_json::json;

use super::{Failure, finish};
use crate::args::parse_spec;
use crate::json::{Error, Output, Scope};
use crate::session::Session;
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
    let requirement = match written {
        Some(text) => text.to_owned(),
        None => {
            // Checked before choosing a version, as a media plugin may have
            // none that suits the project.
            let index = session.registry.index(id, session.now)?;
            check_pin(&index, id, pin).map_err(edit_failure)?;
            newest(session, root, id, index, advisories)?.to_string()
        }
    };
    let outcome = add_to_project(&session.environment(), root, id, &requirement, pin)
        .map_err(edit_failure)?;

    report::resolve_warnings(&outcome.warnings);
    println!(
        "added {id} {requirement}, locked {}",
        outcome.lock.plugins[id].version
    );
    print_installed(&outcome.installed);
    warn_record_error(outcome.record_error.as_deref());
    Ok(())
}

/// The failure for `error`, warning first about each file that could not
/// be put back as it was.
fn edit_failure(error: EditError) -> Failure {
    if let EditError::NotPutBack { failed, .. } = &error {
        for (name, error) in failed {
            report::warn(format_args!("could not put {name} back as it was: {error}"));
        }
    }
    Failure::from(error)
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

/// Removes `id` from the project's dependencies and from `celesta.lock`,
/// without the registry.
pub fn remove(root: &Path, id: &str) -> ExitCode {
    finish(remove_dependency(root, id))
}

fn remove_dependency(root: &Path, id: &str) -> Result<(), Failure> {
    remove_from_project(root, id).map_err(edit_failure)?;
    println!("removed {id}");
    Ok(())
}

/// Installs what `celesta.lock` names when `locked` or `offline` is asked
/// for, or when the lock covers `celesta.config.json`; otherwise locks the
/// project again, keeping every entry that still holds, and installs that.
/// Without a registry key, only what the store already holds can be used.
/// A project with no dependencies and no lock has nothing to install.
pub(crate) fn install(
    session: &Session,
    root: &Path,
    locked: bool,
    offline: bool,
    output: Output,
) -> ExitCode {
    match has_registry_plugins(root) {
        Ok(true) => {}
        Ok(false) => {
            if output.json {
                return output.write(
                    [] as [(); 0],
                    json!({ "target": session.host.as_str(), "lockfile": "unchanged" }),
                    None,
                );
            }
            println!("this project has no registry plugins");
            return ExitCode::SUCCESS;
        }
        Err(error) => return output.fail(Error::new("project_config", error)),
    }
    if !offline {
        advisories::refresh(session);
    }
    match install_project(session, root, locked, offline, output) {
        Ok(exit) => exit,
        Err(error) => output.fail(Error::project(error, Scope::Project)),
    }
}

/// Whether the project has dependencies or a `celesta.lock`.
fn has_registry_plugins(root: &Path) -> Result<bool, celesta_plugin_host::ProjectConfigError> {
    Ok(root.join(LOCK_FILE).exists() || !project_dependencies(root)?.dependencies.is_empty())
}

fn install_project(
    session: &Session,
    root: &Path,
    locked: bool,
    offline: bool,
    output: Output,
) -> Result<ExitCode, celesta_registry_client::ProjectError> {
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
        let outcome = lock_project(&env, root, &Update::Keep)?;
        report::resolve_warnings(&outcome.warnings);
        if output.json {
            warn_record_error(outcome.record_error.as_deref());
            return Ok(install_report(
                output,
                session,
                Some(&outcome.lock),
                &outcome.installed,
                "written",
            ));
        }
        print_installed(&outcome.installed);
        println!("wrote {LOCK_FILE}");
        warn_record_error(outcome.record_error.as_deref());
        return Ok(ExitCode::SUCCESS);
    }

    let outcome = install_locked(&env, root, offline)?;
    if !output.json {
        print_installed(&outcome.installed);
        for (id, version, reason) in &outcome.skipped {
            println!("skipped {id} {version}: {reason}");
        }
        println!("{LOCK_FILE} is satisfied");
    }
    // After the installs, which fetched the indexes of what they downloaded.
    if let Some(lock) = &lock {
        warn_about_locked_states(session, lock);
    }
    warn_record_error(outcome.record_error.as_deref());
    Ok(if output.json {
        install_report(
            output,
            session,
            lock.as_ref(),
            &outcome.installed,
            "satisfied",
        )
    } else {
        ExitCode::SUCCESS
    })
}

fn install_report(
    output: Output,
    session: &Session,
    lock: Option<&Lockfile>,
    installed: &[(String, Version)],
    lockfile: &str,
) -> ExitCode {
    let installed: BTreeSet<_> = installed.iter().map(|(id, _)| id.as_str()).collect();
    let plugins: Vec<_> = lock
        .into_iter()
        .flat_map(|lock| {
            lock.plugins.iter().map(|(id, plugin)| {
                let mut entry =
                    json!({ "scope": Scope::Project, "id": id, "version": plugin.version });
                if lock.artifact(id, &session.host).is_none() {
                    entry["status"] = json!("skipped");
                    entry["reason"] = json!("no_build");
                } else if installed.contains(id.as_str()) {
                    entry["status"] = json!("installed");
                } else {
                    entry["status"] = json!("already_installed");
                }
                entry
            })
        })
        .collect();
    output.write(
        plugins,
        json!({ "lockfile": lockfile, "target": session.host.as_str() }),
        None,
    )
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
/// newest versions their requirements allow, and installs them. A project
/// with no dependencies and no lock has nothing to update, and no lock is
/// written for it.
pub fn update(session: &Session, root: &Path, id: Option<&str>) -> ExitCode {
    let update = match id {
        None => match has_registry_plugins(root) {
            Ok(true) => Update::All,
            Ok(false) => {
                println!("this project has no registry plugins");
                return ExitCode::SUCCESS;
            }
            Err(error) => return finish(Err(error.into())),
        },
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
