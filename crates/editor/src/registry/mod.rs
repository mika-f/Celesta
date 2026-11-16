//! Registry plugins in the app: the window's [`RegistryState`] fetches the
//! registry's files and installs, updates, and removes plugins in the
//! background, with the same library calls as the `celesta` command, and
//! the editor rediscovers its plugins when the work changed them.

mod client;
mod notices;
mod questions;
mod state;

pub(crate) use client::EditorRegistry;
pub(crate) use notices::blocked_problem;
pub(crate) use state::{Fetch, InFlight, RegistryEvent, RegistryState, Work};

use crate::actions::ReloadPlugins;
use crate::problems::Source;
use crate::view::EditorView;
use gpui_kit::prelude::*;
use gpui_kit::{Context, Window};
use std::path::Path;
use std::sync::Arc;

impl EditorView {
    /// Makes the window's registry entity from `client` (an error when
    /// there is no user folder to install plugins in) and listens to it,
    /// then starts the daily advisory refresh. Once per window: the entity
    /// stays across the projects the window opens.
    pub(crate) fn attach_registry(
        &mut self,
        client: Result<EditorRegistry, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let client = match client {
            Ok(client) => client,
            Err(error) => {
                self.problems.fail(Source::Registry, error);
                return;
            }
        };
        if let Some(error) = client.selection_error() {
            self.problems.report(
                Source::Registry,
                "Using the official registry",
                error.to_owned(),
            );
        }
        let registry = cx.new(|_| RegistryState::new(Arc::new(client)));
        cx.subscribe_in(&registry, window, |this, _, event, window, cx| {
            this.registry_event(event, window, cx)
        })
        .detach();
        // The Plugins tab shows its work in flight and its search list, and
        // Problems what the list and the indexes say of the plugins found.
        cx.observe(&registry, |this, _, cx| {
            this.sync_registry_notices(cx);
            cx.notify();
        })
        .detach();
        let project = self.found_plugins.project.clone();
        registry.update(cx, |state, cx| {
            state.project_changed(project, cx);
            state.start_timers(cx);
        });
        self.registry = Some(registry);
        self.sync_registry_notices(cx);
    }

    /// Rediscovers (and reopens) when the work changed what may run here,
    /// once nothing stops a reopen (see [`Self::settle_changed_plugins`]);
    /// asks the requirements question when an update needs a requirement
    /// rewritten; a failure goes to Problems.
    fn registry_event(
        &mut self,
        event: &RegistryEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            RegistryEvent::PluginsChanged { project } => {
                if project.is_none() || *project == self.found_plugins.project {
                    self.plugins_changed = true;
                    cx.notify();
                }
            }
            RegistryEvent::RequirementsUnmet {
                project,
                ids,
                include_prereleases,
                title,
                detail,
            } => {
                if Some(project) == self.found_plugins.project.as_ref() {
                    self.ask_about_requirements(
                        questions::Unmet {
                            root: project.clone(),
                            ids: ids.clone(),
                            include_prereleases: *include_prereleases,
                            title: title.clone(),
                            detail: detail.clone(),
                        },
                        window,
                        cx,
                    );
                }
            }
            RegistryEvent::Failed { title, detail, .. } => {
                self.problems
                    .report(Source::Registry, title.clone(), detail.clone());
                cx.notify();
            }
            RegistryEvent::FailedElsewhere {
                project,
                title,
                detail,
            } => {
                // Another project is open: nothing reopens for this one.
                let (title, detail) = elsewhere_note(project, title, detail);
                self.problems
                    .info_without_toast(Source::Registry, title, detail);
                cx.notify();
            }
        }
    }

    /// Reopens the project with its plugins found again when registry work
    /// changed them, as soon as no open, save panel, or export is in the
    /// way; until then the change waits, without a notice. Each render
    /// looks, so an open or an export that ends picks it up.
    pub(crate) fn settle_changed_plugins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !plugins_reload_due(
            self.plugins_changed,
            self.opening,
            self.choosing_export_path,
            self.export_cancellation.is_some(),
        ) {
            return;
        }
        self.plugins_changed = false;
        cx.defer_in(window, |this, window, cx| {
            this.reload_plugins_action(&ReloadPlugins, window, cx);
        });
    }
}

/// The Problems entry for a job that failed for `project` once another
/// project was open: its title, and the project's folder before the reason.
fn elsewhere_note(project: &Path, title: &str, detail: &str) -> (String, String) {
    (title.to_owned(), format!("{}\n{detail}", project.display()))
}

/// Whether plugins registry work `changed` are reloaded now: not while a
/// project opens (it finds them itself), nor while the save panel is up or
/// an export runs, which a reopen would cut short.
fn plugins_reload_due(
    changed: bool,
    opening: bool,
    choosing_export_path: bool,
    exporting: bool,
) -> bool {
    changed && !opening && !choosing_export_path && !exporting
}

#[cfg(test)]
mod tests {
    use super::{elsewhere_note, plugins_reload_due};
    use std::path::Path;

    #[test]
    fn a_failure_elsewhere_names_its_project() {
        assert_eq!(
            elsewhere_note(
                Path::new("/projects/old"),
                "Couldn’t update acme.charts",
                "no version of acme.charts satisfies ^0.5.0"
            ),
            (
                "Couldn’t update acme.charts".to_owned(),
                "/projects/old\nno version of acme.charts satisfies ^0.5.0".to_owned()
            )
        );
    }

    #[test]
    fn changed_plugins_wait_for_the_open_and_the_export() {
        assert!(plugins_reload_due(true, false, false, false));
        assert!(
            !plugins_reload_due(false, false, false, false),
            "nothing changed"
        );
        assert!(
            !plugins_reload_due(true, true, false, false),
            "an open finds them"
        );
        assert!(
            !plugins_reload_due(true, false, true, false),
            "the save panel is up"
        );
        assert!(
            !plugins_reload_due(true, false, false, true),
            "an export runs"
        );
    }
}
