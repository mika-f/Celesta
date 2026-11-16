//! Registry plugins in the app: the window's [`RegistryState`] fetches the
//! registry's files and installs, updates, and removes plugins in the
//! background, with the same library calls as the `celesta` command, and
//! the editor rediscovers its plugins when the work changed them.

mod client;
mod state;

pub(crate) use client::EditorRegistry;
pub(crate) use state::{RegistryEvent, RegistryState};

use crate::actions::ReloadPlugins;
use crate::problems::Source;
use crate::view::EditorView;
use gpui_kit::prelude::*;
use gpui_kit::{Context, Window};
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
            this.registry_event(event, window, cx);
        })
        .detach();
        let project = self.found_plugins.project.clone();
        registry.update(cx, |state, cx| {
            state.project_changed(project);
            state.start_timers(cx);
        });
        self.registry = Some(registry);
    }

    /// Rediscovers (and reopens) when the work changed what may run here;
    /// a failure goes to Problems.
    fn registry_event(
        &mut self,
        event: &RegistryEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            RegistryEvent::PluginsChanged { project } => {
                if project.is_none() || *project == self.found_plugins.project {
                    self.reload_plugins_action(&ReloadPlugins, window, cx);
                }
            }
            RegistryEvent::Failed { title, detail, .. } => {
                self.problems
                    .report(Source::Registry, title.clone(), detail.clone());
                cx.notify();
            }
        }
    }
}
