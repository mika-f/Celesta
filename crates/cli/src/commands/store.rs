//! The commands about what is installed: `list`, `install --global`, and
//! `gc`.

use std::path::Path;
use std::process::ExitCode;
use std::time::SystemTime;

use celesta_plugin_host::{LOCK_FILE, Lockfile, UserSettings, has_project_config};
use celesta_registry_client::{
    BundledPlugins, GcInput, GcPlan, PROJECTS_FILE, holds_locked_build, install_global, plan_gc,
    run_gc,
};
use celesta_registry_format::{Advisory, AdvisoryList, Hash};
use semver::{Version, VersionReq};
use serde::Serialize;
use serde_json::json;

use super::{Failure, finish};
use crate::args::parse_spec;
use crate::json::{Error, Output, Scope};
use crate::session::Session;
use crate::{advisories, report};

/// Lists the plugins the project in `root` locks and those installed for
/// every project, with their states. Works offline and without a registry
/// key: it refreshes the advisory list within the run's budget, then uses
/// the cached list and indexes.
pub(crate) fn list(session: &Session, root: &Path, output: Output) -> ExitCode {
    let advisories = advisories::refresh(session);
    match list_plugins(session, root, advisories.as_ref(), output) {
        Ok(exit) => exit,
        Err(error) => output.fail(error),
    }
}

fn list_plugins(
    session: &Session,
    root: &Path,
    advisories: Option<&AdvisoryList>,
    output: Output,
) -> Result<ExitCode, Error> {
    let lock = Lockfile::load(root).map_err(|error| Error::new("lockfile", error))?;
    let settings = UserSettings::load(&session.settings_path)
        .map_err(|error| Error::new("settings", error))?;
    let lines = Lines::new(session, advisories);
    let project: Vec<_> = lock
        .iter()
        .flat_map(|lock| {
            lock.plugins.iter().map(|(id, locked)| {
                let tree_hash = lock
                    .artifact(id, &session.host)
                    .map(|artifact| &artifact.tree_hash);
                lines.entry(Scope::Project, id, &locked.version, tree_hash)
            })
        })
        .collect();
    let global: Vec<_> = settings
        .registry_plugins
        .iter()
        .map(|plugin| {
            lines.entry(
                Scope::Global,
                &plugin.id,
                &plugin.version,
                Some(&plugin.tree_hash),
            )
        })
        .collect();

    if output.json {
        return Ok(output.write(
            project.into_iter().chain(global).collect::<Vec<_>>(),
            json!({ "target": session.host.as_str(), "advisoriesAvailable": advisories.is_some() }),
            None,
        ));
    }
    if project.is_empty() && global.is_empty() {
        println!("no registry plugins");
        return Ok(ExitCode::SUCCESS);
    }
    for (title, lines) in [("This project", project), ("Global", global)] {
        println!("{title}");
        if lines.is_empty() {
            println!("  none");
        }
        for entry in lines {
            println!("  {}", entry.line(session.host.as_str()));
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum ListStatus {
    Installed,
    Missing,
    NoBuild,
    Blocked,
}

#[derive(Serialize)]
struct ListEntry<'a> {
    scope: Scope,
    id: &'a str,
    version: &'a Version,
    status: ListStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    advisory: Option<&'a Advisory>,
    yanked: bool,
    deprecated: bool,
}

impl ListEntry<'_> {
    fn line(&self, host: &str) -> String {
        let state = match (&self.status, self.advisory) {
            (_, Some(advisory)) => format!("blocked ({})", advisory.id),
            (ListStatus::NoBuild, _) => format!("no build for {host}"),
            (ListStatus::Installed, _) => "installed".to_owned(),
            (ListStatus::Missing, _) => "not installed".to_owned(),
            (ListStatus::Blocked, _) => unreachable!(),
        };
        let mut line = format!("{} {}  {state}", self.id, self.version);
        if self.yanked {
            line.push_str(" yanked");
        }
        if self.deprecated {
            line.push_str(" deprecated");
        }
        line
    }
}

struct Lines<'a> {
    session: &'a Session,
    advisories: Option<&'a AdvisoryList>,
    bundled: BundledPlugins<'a>,
}

impl<'a> Lines<'a> {
    fn new(session: &'a Session, advisories: Option<&'a AdvisoryList>) -> Self {
        Self {
            session,
            advisories,
            bundled: BundledPlugins::new(session.bundled.as_deref()),
        }
    }

    fn entry<'b>(
        &'b self,
        scope: Scope,
        id: &'b str,
        version: &'b Version,
        tree_hash: Option<&Hash>,
    ) -> ListEntry<'b> {
        let advisory = self.advisories.and_then(|list| list.blocking(id, version));
        let session = self.session;
        let status = match (advisory, tree_hash) {
            (Some(_), _) => ListStatus::Blocked,
            (None, None) => ListStatus::NoBuild,
            (None, Some(tree_hash)) => {
                let held =
                    holds_locked_build(&session.store, &self.bundled, id, version, tree_hash);
                if held {
                    ListStatus::Installed
                } else {
                    ListStatus::Missing
                }
            }
        };
        let mut result = ListEntry {
            scope,
            id,
            version,
            status,
            advisory,
            yanked: false,
            deprecated: false,
        };
        if let Some(index) = self.session.registry.cached_index(id)
            && let Some(entry) = index
                .versions
                .iter()
                .find(|entry| entry.version == *version)
        {
            result.yanked = entry.yanked;
            result.deprecated = index.deprecation(entry).is_some();
        }
        result
    }
}

/// Installs `spec` (`id[@req]`, any version by default) for every project,
/// recording it in the user settings, and warns about what resolution
/// found, such as a deprecated version.
pub(crate) fn install_global_plugin(session: &Session, spec: &str, output: Output) -> ExitCode {
    let (id, requirement) = match parse_spec(spec) {
        Ok(parsed) => parsed,
        Err(message) => {
            return output.fail(Error::usage("invalid_plugin_spec", message));
        }
    };
    let requirement = requirement.unwrap_or(VersionReq::STAR);
    advisories::refresh(session);
    match install_global(
        &session.environment(),
        &session.settings_path,
        &id,
        &requirement,
    ) {
        Ok(outcome) => {
            report::resolve_warnings(&outcome.warnings);
            let installed = outcome.plugin;
            if output.json {
                return output.write(
                    [json!({ "scope": Scope::Global, "id": installed.id, "version": installed.version, "status": "installed" })],
                    serde_json::Value::Null,
                    None,
                );
            }
            println!(
                "installed {} {} for every project",
                installed.id, installed.version
            );
            ExitCode::SUCCESS
        }
        Err(error) => output.fail(Error::project(error, Scope::Global)),
    }
}

/// Removes the store versions that no recorded project, the project in
/// `root`, or the user settings names, and unfinished downloads; with
/// `dry_run`, only says what would be removed.
pub fn gc(session: &Session, root: &Path, dry_run: bool) -> ExitCode {
    finish(collect(session, root, dry_run))
}

fn collect(session: &Session, root: &Path, dry_run: bool) -> Result<(), Failure> {
    // Default settings would keep no global plugin, so they are never
    // assumed.
    let settings = UserSettings::load(&session.settings_path)?;
    let current = (has_project_config(root) || root.join(LOCK_FILE).exists()).then_some(root);
    let store = &session.store;
    let plan = plan_gc(&GcInput {
        store,
        settings: &settings,
        current,
        now: SystemTime::now(),
    })
    .map_err(|error| {
        Failure(format!(
            "{error}; the store's project record is {}",
            store.root().join(PROJECTS_FILE).display()
        ))
    })?;
    if !dry_run {
        run_gc(store, &plan)?;
    }
    for line in gc_lines(&plan, dry_run) {
        println!("{line}");
    }
    Ok(())
}

/// What `gc` says it removed, or would remove with `dry_run`.
fn gc_lines(plan: &GcPlan, dry_run: bool) -> Vec<String> {
    let verb = if dry_run { "would remove" } else { "removed" };
    let mut lines: Vec<String> = plan
        .versions
        .iter()
        .map(|(id, version)| format!("{verb} {id} {version}"))
        .collect();
    if !plan.temporary.is_empty() {
        lines.push(format!(
            "{verb} {} unfinished downloads",
            plan.temporary.len()
        ));
    }
    if lines.is_empty() {
        lines.push("nothing to remove".to_owned());
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn gc_lines_name_versions_and_unfinished_downloads() {
        let plan = GcPlan {
            versions: vec![("acme.charts".to_owned(), Version::new(1, 0, 0))],
            temporary: vec![PathBuf::from(".tmp-a"), PathBuf::from(".tmp-b")],
            forgotten: Vec::new(),
        };
        assert_eq!(
            gc_lines(&plan, false),
            [
                "removed acme.charts 1.0.0",
                "removed 2 unfinished downloads"
            ]
        );
        assert_eq!(
            gc_lines(&plan, true),
            [
                "would remove acme.charts 1.0.0",
                "would remove 2 unfinished downloads"
            ]
        );
        assert_eq!(gc_lines(&GcPlan::default(), true), ["nothing to remove"]);
    }
}
