//! The content plugins an open project draws with, and the questions that
//! guard what code runs. Opening a project runs its code, so a project folder
//! is opened only once the user trusts it; a trusted project's own plugin
//! folders (`plugins.paths` in `celesta.config.json`) are searched too.
//! Plugins from the user's folder and from a project run only once the user
//! allowed them as they are now. What may run is found when the project
//! opens and again on File > Reload Plugins, in every location Celesta
//! searches (`celesta_plugin_host::search_paths`), and the one set serves the
//! preview's renderer, every React runtime the editor starts, and exports.
//! What waits for approval and what was skipped show in Problems.

use crate::problems::{Problem, Source, error_chain};
use crate::source::PropertyArgs;
use crate::view::EditorView;
use celesta_plugin_host::{
    InstalledPlugins, PendingPlugin, UserSettings, from_the_internet, project_plugin_paths,
    project_root,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogDescription, DialogFooter};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{Context, Pixels, Window, div, px};
use std::path::{Path, PathBuf};

/// Wide enough for a plugin's folder on one or two lines.
const PLUGIN_DIALOG_WIDTH: Pixels = px(560.);

/// What one discovery found.
#[derive(Clone, Debug, Default)]
pub(crate) struct FoundPlugins {
    /// What may run.
    pub(crate) installed: InstalledPlugins,
    /// What waits for the user to allow it, in search order.
    pub(crate) pending: Vec<PendingPlugin>,
    /// Why each skipped plugin or folder was skipped.
    pub(crate) skipped: Vec<String>,
}

impl FoundPlugins {
    /// Reads every plugin location, with the trusted project at `project`'s
    /// own folders; blocking, so off the UI thread.
    pub(crate) fn find(project: Option<&Path>) -> Self {
        let (settings, settings_error) = UserSettings::current();
        let mut skipped: Vec<String> = settings_error
            .iter()
            .map(|error| error_chain(error))
            .collect();
        let project_paths = match project.map(project_plugin_paths) {
            Some(Ok(paths)) => paths,
            Some(Err(error)) => {
                skipped.push(error_chain(&error));
                Vec::new()
            }
            None => Vec::new(),
        };
        let (found, errors) = InstalledPlugins::find_with_project(&[], &project_paths);
        skipped.extend(errors.iter().map(ToString::to_string));
        let allowed = found.allowed_by(&settings);
        skipped.extend(allowed.unreadable);
        Self {
            installed: allowed.plugins,
            pending: allowed.pending,
            skipped,
        }
    }

    /// The Problems entry for plugins that wait or were skipped, if any.
    pub(crate) fn problem(&self) -> Option<Problem> {
        let waiting = self.pending.iter().map(|pending| {
            format!(
                "{} waits for your approval ({})",
                pending.entry.manifest.id,
                pending.entry.directory.display()
            )
        });
        let detail: Vec<String> = waiting.chain(self.skipped.iter().cloned()).collect();
        let title = match (self.pending.len(), self.skipped.len()) {
            (0, 0) => return None,
            (0, 1) => "A plugin was skipped".to_owned(),
            (0, skipped) => format!("{skipped} plugins were skipped"),
            (1, _) => "A plugin waits for your approval".to_owned(),
            (pending, _) => format!("{pending} plugins wait for your approval"),
        };
        Some(Problem::new(title, detail.join("\n")))
    }
}

/// A project the user has not trusted yet, as opening asks about it.
pub(crate) struct TrustRequest {
    pub(crate) path: PathBuf,
    pub(crate) property_args: PropertyArgs,
    /// The folder trusting covers.
    pub(crate) root: PathBuf,
    /// The entry came from the internet (its Mark of the Web).
    pub(crate) from_the_internet: bool,
}

/// The trust question for opening `path`, or `None` when the user trusts
/// its project folder already. Blocking (it reads the settings).
pub(crate) fn trust_request(path: &Path, property_args: &PropertyArgs) -> Option<TrustRequest> {
    let root = project_root(path);
    let (settings, _) = UserSettings::current();
    (!settings.is_trusted(&root)).then(|| TrustRequest {
        path: path.to_path_buf(),
        property_args: property_args.clone(),
        root,
        from_the_internet: from_the_internet(path),
    })
}

/// The trusted project folder `path` opens in, whose own plugins are used.
pub(crate) fn trusted_root(path: Option<&Path>) -> Option<PathBuf> {
    let root = project_root(path?);
    UserSettings::current().0.is_trusted(&root).then_some(root)
}

impl EditorView {
    /// Takes `found` as this view's plugins and reports what waits and what
    /// was skipped.
    pub(crate) fn set_plugins(&mut self, found: FoundPlugins) {
        self.problems.set(Source::Plugins, found.problem());
        self.plugins = found.installed;
        self.pending_plugins = found.pending;
    }

    /// Asks whether to trust the project `request` opens; trusting saves
    /// the choice and opens it.
    pub(crate) fn ask_to_trust(
        &mut self,
        request: TrustRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let request = std::rc::Rc::new(std::cell::RefCell::new(Some(request)));
        window.open_dialog(cx, move |dialog, _, cx| {
            let muted = cx.theme().muted_foreground;
            let Some((root, from_the_internet)) = request.borrow().as_ref().map(|request| {
                (
                    request.root.display().to_string(),
                    request.from_the_internet,
                )
            }) else {
                return dialog;
            };
            let warning = from_the_internet.then(|| {
                div()
                    .text_color(cx.theme().danger)
                    .child("This project was downloaded from the internet.")
            });
            let view = view.clone();
            let request = request.clone();
            dialog
                .title("Trust this project?")
                .w(PLUGIN_DIALOG_WIDTH)
                .child(
                    v_flex()
                        .gap_3()
                        .child(DialogDescription::new().child(
                            "Opening a project runs its code on your computer, with your \
                             permissions, and uses the plugins it brings. Trust only projects \
                             from people you trust.",
                        ))
                        .child(div().text_sm().text_color(muted).child(root))
                        .children(warning),
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            DialogClose::new().child(
                                Button::new("trust-project-cancel")
                                    .label("Don’t Open")
                                    .outline(),
                            ),
                        )
                        .child(
                            DialogAction::new().child(
                                Button::new("trust-project-trust")
                                    .label("Trust and Open")
                                    .primary(),
                            ),
                        ),
                )
                .on_ok(move |_, window, cx| {
                    let Some(request) = request.borrow_mut().take() else {
                        return true;
                    };
                    view.update(cx, |this, cx| {
                        match UserSettings::update(|settings| settings.trust(&request.root)) {
                            Ok(_) => this.load_path_with(
                                Some(request.path),
                                request.property_args,
                                window,
                                cx,
                            ),
                            Err(error) => {
                                this.problems.report(
                                    Source::Project,
                                    "Couldn’t save that you trust this project",
                                    error_chain(&error),
                                );
                                cx.notify();
                            }
                        }
                    })
                    .ok();
                    true
                })
        });
    }

    /// Asks once per change whether to allow the plugins that wait for it;
    /// allowing saves the choice and reloads the plugins.
    pub(crate) fn ask_about_pending_plugins(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let unasked: Vec<PendingPlugin> = self
            .pending_plugins
            .iter()
            .filter(|pending| {
                !self
                    .asked_plugins
                    .contains(&(pending.entry.directory.clone(), pending.fingerprint.clone()))
            })
            .cloned()
            .collect();
        if unasked.is_empty() || window.has_active_dialog(cx) {
            return;
        }
        self.asked_plugins.extend(
            unasked
                .iter()
                .map(|pending| (pending.entry.directory.clone(), pending.fingerprint.clone())),
        );
        let view = cx.entity().downgrade();
        let unasked = std::rc::Rc::new(unasked);
        window.open_dialog(cx, move |dialog, _, cx| {
            let muted = cx.theme().muted_foreground;
            let danger = cx.theme().danger;
            let rows = unasked.iter().map(|pending| {
                let manifest = &pending.entry.manifest;
                let mut notes = Vec::new();
                if pending.changed {
                    notes.push("Changed since you allowed it.");
                }
                if pending.from_the_internet {
                    notes.push("Downloaded from the internet.");
                }
                v_flex()
                    .gap_1()
                    .child(div().child(format!(
                        "{} {} · {}",
                        manifest.id,
                        manifest.version,
                        pending.entry.source.label()
                    )))
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(pending.entry.directory.display().to_string()),
                    )
                    .children(
                        (!notes.is_empty())
                            .then(|| div().text_sm().text_color(danger).child(notes.join(" "))),
                    )
            });
            let view = view.clone();
            let allowed = unasked.clone();
            dialog
                .title(if unasked.len() == 1 {
                    "Allow this plugin to run?"
                } else {
                    "Allow these plugins to run?"
                })
                .w(PLUGIN_DIALOG_WIDTH)
                .child(
                    v_flex()
                        .gap_3()
                        .child(DialogDescription::new().child(
                            "A plugin runs its own code on your computer, with your \
                             permissions. Allow only plugins from people you trust. Until \
                             you do, their layers are not drawn.",
                        ))
                        .children(rows),
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            DialogClose::new().child(
                                Button::new("allow-plugins-later")
                                    .label("Not Now")
                                    .outline(),
                            ),
                        )
                        .child(
                            DialogAction::new().child(
                                Button::new("allow-plugins-allow")
                                    .label("Allow and Reload")
                                    .primary(),
                            ),
                        ),
                )
                .on_ok(move |_, window, cx| {
                    let saved = UserSettings::update(|settings| {
                        for pending in allowed.iter() {
                            settings.allow(
                                &pending.entry.manifest.id,
                                &pending.entry.directory,
                                &pending.fingerprint,
                            );
                        }
                    });
                    view.update(cx, |this, cx| match saved {
                        Ok(_) => this.load_path(this.source_path.clone(), window, cx),
                        Err(error) => {
                            this.problems.report(
                                Source::Plugins,
                                "Couldn’t save the plugins you allowed",
                                error_chain(&error),
                            );
                            cx.notify();
                        }
                    })
                    .ok();
                    true
                })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_and_skipped_plugins_make_one_problem() {
        assert_eq!(FoundPlugins::default().problem(), None);
        let found = |skipped: &[&str]| FoundPlugins {
            skipped: skipped.iter().map(|line| (*line).to_owned()).collect(),
            ..FoundPlugins::default()
        };
        assert_eq!(
            found(&["a was skipped: bad id"]).problem(),
            Some(Problem::new(
                "A plugin was skipped",
                "a was skipped: bad id"
            ))
        );
        assert_eq!(
            found(&["a was skipped", "b was skipped"]).problem(),
            Some(Problem::new(
                "2 plugins were skipped",
                "a was skipped\nb was skipped"
            ))
        );
    }

    #[test]
    fn a_project_is_trusted_once_its_folder_is() {
        let root = std::env::temp_dir().join(format!("celesta-trust-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let entry = root.join("film.tsx");
        std::fs::write(&entry, "").unwrap();
        // Whatever the user's own settings say, a fresh temporary folder is
        // not among their trusted projects.
        let request = trust_request(&entry, &PropertyArgs::default()).unwrap();
        assert_eq!(request.path, entry);
        assert_eq!(request.root, root);
        assert!(!request.from_the_internet);
        assert_eq!(trusted_root(Some(&entry)), None);
        assert_eq!(trusted_root(None), None);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
