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
use celesta_registry_format::{AdvisoryList, Hash};
use semver::{Version, VersionReq};

use super::{Failure, finish};
use crate::args::parse_spec;
use crate::session::Session;
use crate::{USAGE, advisories, report};

/// Lists the plugins the project in `root` locks and those installed for
/// every project, with their states. Works offline and without a registry
/// key: it refreshes the advisory list within the run's budget, then uses
/// the cached list and indexes.
pub fn list(session: &Session, root: &Path) -> ExitCode {
    let advisories = advisories::refresh(session);
    finish(list_plugins(session, root, advisories.as_ref()))
}

fn list_plugins(
    session: &Session,
    root: &Path,
    advisories: Option<&AdvisoryList>,
) -> Result<(), Failure> {
    let lock = Lockfile::load(root)?;
    let settings = UserSettings::load(&session.settings_path)?;
    let lines = Lines::new(session, advisories);
    let project: Vec<String> = lock
        .iter()
        .flat_map(|lock| {
            lock.plugins.iter().map(|(id, locked)| {
                let tree_hash = lock
                    .artifact(id, &session.host)
                    .map(|artifact| &artifact.tree_hash);
                lines.line(id, &locked.version, tree_hash)
            })
        })
        .collect();
    let global: Vec<String> = settings
        .registry_plugins
        .iter()
        .map(|plugin| lines.line(&plugin.id, &plugin.version, Some(&plugin.tree_hash)))
        .collect();

    if project.is_empty() && global.is_empty() {
        println!("no registry plugins");
        return Ok(());
    }
    for (title, lines) in [("This project", project), ("Global", global)] {
        println!("{title}");
        if lines.is_empty() {
            println!("  none");
        }
        for line in lines {
            println!("  {line}");
        }
    }
    Ok(())
}

/// The lines `list` prints, one per plugin.
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

    /// `{id} {version}  {state}` for the host's build with `tree_hash`
    /// (`None`: there is none), the state `blocked ({advisory})`,
    /// `no build for {host}`, `installed`, or `not installed`, then
    /// ` yanked` and ` deprecated` when the cached index says so.
    fn line(&self, id: &str, version: &Version, tree_hash: Option<&Hash>) -> String {
        let blocking = self.advisories.and_then(|list| list.blocking(id, version));
        let session = self.session;
        let state = match (blocking, tree_hash) {
            (Some(advisory), _) => format!("blocked ({})", advisory.id),
            (None, None) => format!("no build for {}", session.host.as_str()),
            (None, Some(tree_hash)) => {
                let held =
                    holds_locked_build(&session.store, &self.bundled, id, version, tree_hash);
                if held { "installed" } else { "not installed" }.to_owned()
            }
        };
        let mut line = format!("{id} {version}  {state}");
        if let Some(index) = self.session.registry.cached_index(id)
            && let Some(entry) = index
                .versions
                .iter()
                .find(|entry| entry.version == *version)
        {
            if entry.yanked {
                line.push_str(" yanked");
            }
            if index.deprecation(entry).is_some() {
                line.push_str(" deprecated");
            }
        }
        line
    }
}

/// Installs `spec` (`id[@req]`, any version by default) for every project,
/// recording it in the user settings, and warns about what resolution
/// found, such as a deprecated version.
pub fn install_global_plugin(session: &Session, spec: &str) -> ExitCode {
    let (id, requirement) = match parse_spec(spec) {
        Ok(parsed) => parsed,
        Err(message) => {
            report::error(message);
            return ExitCode::from(USAGE);
        }
    };
    let requirement = requirement.unwrap_or(VersionReq::STAR);
    advisories::refresh(session);
    finish(
        install_global(
            &session.environment(),
            &session.settings_path,
            &id,
            &requirement,
        )
        .map(|outcome| {
            report::resolve_warnings(&outcome.warnings);
            let installed = outcome.plugin;
            println!(
                "installed {} {} for every project",
                installed.id, installed.version
            );
        })
        .map_err(Failure::from),
    )
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
