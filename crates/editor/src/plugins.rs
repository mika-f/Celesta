//! The content plugins an open project draws with, and the questions that
//! guard what code runs. Opening a project runs its code, so a project folder
//! is opened only once the user trusts it; a trusted project's own plugin
//! folders (`plugins.paths` in `celesta.config.json`) are searched too.
//! Plugins from the user's folder, a folder they added, or a project run
//! only once the user allowed them as they are now, and a plugin the user
//! turned off does not run. What may run is found when the project
//! opens and again on File > Reload Plugins, in every location Celesta
//! searches (`celesta_plugin_host::search_paths`), and the one set serves the
//! preview's renderer, every React runtime the editor starts, and exports.
//! What waits for approval and what was skipped show in Problems.

use crate::problems::{Problem, Source, error_chain};
use crate::source::PropertyArgs;
use crate::view::EditorView;
use celesta_plugin_host::{
    InstalledPlugins, PendingPlugin, PluginEntry, SettingsError, UserSettings, from_the_internet,
    has_project_config, project_plugin_paths, project_root,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogDescription, DialogFooter};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{Context, Pixels, Window, div, px};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

/// Wide enough for a plugin's folder on one or two lines.
const PLUGIN_DIALOG_WIDTH: Pixels = px(560.);

/// What one discovery found.
#[derive(Clone, Debug, Default)]
pub(crate) struct FoundPlugins {
    /// What may run.
    pub(crate) installed: InstalledPlugins,
    /// What waits for the user to allow it, in search order.
    pub(crate) pending: Vec<PendingPlugin>,
    /// What the user turned off, in search order.
    pub(crate) disabled: Vec<PluginEntry>,
    /// The folders of plugins the user added, as the settings said.
    pub(crate) folders: Vec<PathBuf>,
    /// Why each skipped plugin or folder was skipped.
    pub(crate) skipped: Vec<String>,
}

impl FoundPlugins {
    /// Reads every plugin location, with the trusted project at `project`'s
    /// own folders; blocking, so off the UI thread.
    pub(crate) fn find(project: Option<&Path>) -> Self {
        let (settings, settings_error) = UserSettings::current();
        Self::find_with(project, &settings, settings_error)
    }

    /// [`Self::find`] with these settings, and why the user's could not be
    /// read.
    fn find_with(
        project: Option<&Path>,
        settings: &UserSettings,
        settings_error: Option<SettingsError>,
    ) -> Self {
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
        let (found, errors) = InstalledPlugins::find_with(&[], &project_paths, settings);
        skipped.extend(errors.iter().map(ToString::to_string));
        let allowed = found.allowed_by(settings);
        skipped.extend(allowed.unreadable);
        Self {
            installed: allowed.plugins,
            pending: allowed.pending,
            disabled: allowed.disabled,
            folders: settings.plugin_folders.clone(),
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

    /// Every plugin found whose declarations the project's TypeScript
    /// support gets: waiting ones too, since declarations run nothing and an
    /// entry that imports a plugin is written before it is allowed.
    pub(crate) fn declared(&self) -> Vec<PluginEntry> {
        self.installed
            .plugins
            .iter()
            .chain(self.pending.iter().map(|pending| &pending.entry))
            .cloned()
            .collect()
    }

    /// What to add to an open that failed: the plugins waiting for the
    /// user. An entry importing one opens with stand-ins for it, but one
    /// that calls its functions (in `prepare`, say) fails until it is
    /// allowed.
    pub(crate) fn failure_note(&self) -> Option<String> {
        if self.pending.is_empty() {
            return None;
        }
        let ids: Vec<&str> = self
            .pending
            .iter()
            .map(|pending| pending.entry.manifest.id.as_str())
            .collect();
        Some(format!(
            "These plugins wait for your approval: until you allow them, their components draw \
             nothing and calling their functions fails: {}.",
            ids.join(", ")
        ))
    }
}

/// Project folders without `celesta.config.json` that the user trusted while
/// Celesta runs, links resolved. Nothing marks such a folder as a project,
/// so the choice is not saved and is asked again the next time Celesta
/// starts.
static TRUSTED_FOR_NOW: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn trusted_for_now() -> Vec<PathBuf> {
    TRUSTED_FOR_NOW
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// `folder` with links resolved, as the folders trusted for now are kept.
fn resolved(folder: &Path) -> PathBuf {
    std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf())
}

/// Whether the user trusts the project at `root`: saved in their settings,
/// or trusted for now (`for_now`).
fn is_trusted(settings: &UserSettings, for_now: &[PathBuf], root: &Path) -> bool {
    settings.is_trusted(root) || for_now.contains(&resolved(root))
}

/// A project the user has not trusted yet, as opening asks about it.
pub(crate) struct TrustRequest {
    pub(crate) path: PathBuf,
    pub(crate) property_args: PropertyArgs,
    /// The project folder trusting covers.
    pub(crate) root: PathBuf,
    /// The root holds `celesta.config.json`, so trusting it is saved;
    /// otherwise it lasts until Celesta quits.
    pub(crate) remembered: bool,
    /// The entry came from the internet (its Mark of the Web).
    pub(crate) from_the_internet: bool,
}

/// The trust question for opening `path`, or `None` when the user trusts
/// its project folder already. Blocking (it reads the settings). Settings
/// that cannot be read trust nothing, so the question is asked; saving the
/// answer then reports why they could not be read.
pub(crate) fn trust_request(path: &Path, property_args: &PropertyArgs) -> Option<TrustRequest> {
    trust_request_with(
        &UserSettings::current().0,
        &trusted_for_now(),
        path,
        property_args,
    )
}

fn trust_request_with(
    settings: &UserSettings,
    for_now: &[PathBuf],
    path: &Path,
    property_args: &PropertyArgs,
) -> Option<TrustRequest> {
    let root = project_root(path);
    (!is_trusted(settings, for_now, &root)).then(|| TrustRequest {
        path: path.to_path_buf(),
        property_args: property_args.clone(),
        remembered: has_project_config(&root),
        root,
        from_the_internet: from_the_internet(path),
    })
}

/// The trusted project folder `path` opens in, whose own plugins are used.
pub(crate) fn trusted_root(path: Option<&Path>) -> Option<PathBuf> {
    trusted_root_with(&UserSettings::current().0, &trusted_for_now(), path)
}

fn trusted_root_with(
    settings: &UserSettings,
    for_now: &[PathBuf],
    path: Option<&Path>,
) -> Option<PathBuf> {
    let root = project_root(path?);
    is_trusted(settings, for_now, &root).then_some(root)
}

/// Trusts `request`'s project: saved when it has `celesta.config.json`,
/// else until Celesta quits.
fn trust(request: &TrustRequest) -> Result<(), SettingsError> {
    if request.remembered {
        UserSettings::update(|settings| settings.trust(&request.root)).map(drop)
    } else {
        TRUSTED_FOR_NOW
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(resolved(&request.root));
        Ok(())
    }
}

impl EditorView {
    /// Takes `found` as this view's plugins and reports what waits and what
    /// was skipped.
    pub(crate) fn set_plugins(&mut self, found: FoundPlugins) {
        self.problems.set(Source::Plugins, found.problem());
        self.plugins = found.installed.clone();
        self.found_plugins = found;
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
            let Some((root, remembered, from_the_internet)) =
                request.borrow().as_ref().map(|request| {
                    (
                        request.root.display().to_string(),
                        request.remembered,
                        request.from_the_internet,
                    )
                })
            else {
                return dialog;
            };
            let for_now = (!remembered).then(|| {
                div().text_sm().text_color(muted).child(
                    "This folder has no celesta.config.json, so Celesta asks again the next \
                     time it starts.",
                )
            });
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
                             permissions. Plugins it brings are asked about before they run. \
                             Trust only projects from people you trust.",
                        ))
                        .child(div().text_sm().text_color(muted).child(root))
                        .children(for_now)
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
                    view.update(cx, |this, cx| match trust(&request) {
                        Ok(()) => this.load_path_with(
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
                    })
                    .ok();
                    true
                })
        });
    }

    /// Asks once per change whether to allow the plugins that wait for it;
    /// allowing saves the choice and reloads the open project.
    pub(crate) fn ask_about_pending_plugins(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pending = self.found_plugins.pending.clone();
        self.ask_to_allow(&pending, None, window, cx);
    }

    /// Asks once per change whether to allow `pending`; allowing saves the
    /// choice, then opens `reopen` (a project that could not open while they
    /// waited), else reloads the open project.
    pub(crate) fn ask_to_allow(
        &mut self,
        pending: &[PendingPlugin],
        reopen: Option<(Option<PathBuf>, PropertyArgs)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let unasked: Vec<PendingPlugin> = pending
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
        let reopen = std::rc::Rc::new(std::cell::RefCell::new(reopen));
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
            let reopen = reopen.clone();
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
                             you do, it does not run: what it draws is left out, and \
                             calling its functions fails.",
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
                    let reopen = reopen.borrow_mut().take();
                    view.update(cx, |this, cx| match saved {
                        Ok(_) => match reopen {
                            Some((path, property_args)) => {
                                this.load_path_with(path, property_args, window, cx)
                            }
                            None => this.load_path(this.source_path.clone(), window, cx),
                        },
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

    /// A scratch project with `celesta.config.json` listing `plugins/`, which
    /// holds a plugin `p` with a module; removed when dropped.
    struct Project(PathBuf);

    impl Project {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("celesta-trust-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let plugin = root.join("plugins").join("p");
            std::fs::create_dir_all(plugin.join("js")).unwrap();
            std::fs::write(
                root.join(celesta_plugin_host::PROJECT_CONFIG),
                r#"{ "plugins": { "paths": ["plugins"] } }"#,
            )
            .unwrap();
            std::fs::write(
                plugin.join(celesta_plugin_host::MANIFEST),
                r#"{ "id": "p", "version": "1", "protocol": 1, "module": "js/index.js" }"#,
            )
            .unwrap();
            std::fs::write(plugin.join("js/index.js"), "export {};").unwrap();
            std::fs::write(root.join("film.tsx"), "").unwrap();
            Self(root)
        }
    }

    impl Drop for Project {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn ids(plugins: &InstalledPlugins) -> Vec<&str> {
        plugins
            .plugins
            .iter()
            .map(|entry| entry.manifest.id.as_str())
            .collect()
    }

    #[test]
    fn a_project_is_asked_about_until_its_folder_is_trusted() {
        let project = Project::new("ask");
        let entry = project.0.join("film.tsx");
        let mut settings = UserSettings::default();
        let request = trust_request_with(&settings, &[], &entry, &PropertyArgs::default()).unwrap();
        assert_eq!(request.path, entry);
        assert_eq!(request.root, project.0);
        assert!(request.remembered);
        assert!(!request.from_the_internet);
        assert_eq!(trusted_root_with(&settings, &[], Some(&entry)), None);
        assert_eq!(trusted_root_with(&settings, &[], None), None);

        settings.trust(&project.0);
        assert!(trust_request_with(&settings, &[], &entry, &PropertyArgs::default()).is_none());
        assert_eq!(
            trusted_root_with(&settings, &[], Some(&entry)),
            Some(project.0.clone())
        );
    }

    #[test]
    fn a_project_inside_a_trusted_one_is_asked_about_on_its_own() {
        let project = Project::new("nested");
        let inner = project.0.join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join(celesta_plugin_host::PROJECT_CONFIG), "{}").unwrap();
        let entry = inner.join("film.tsx");
        let mut settings = UserSettings::default();
        settings.trust(&project.0);
        let request = trust_request_with(&settings, &[], &entry, &PropertyArgs::default()).unwrap();
        assert_eq!(request.root, inner);
        assert_eq!(trusted_root_with(&settings, &[], Some(&entry)), None);
    }

    #[test]
    fn a_folder_without_project_settings_is_trusted_for_now() {
        let project = Project::new("for-now");
        let folder = project.0.join("loose");
        std::fs::create_dir_all(&folder).unwrap();
        let entry = folder.join("film.tsx");
        std::fs::write(&entry, "").unwrap();
        // With no project settings above, the entry's own folder is the root.
        std::fs::remove_file(project.0.join(celesta_plugin_host::PROJECT_CONFIG)).unwrap();
        let settings = UserSettings::default();
        let request = trust_request_with(&settings, &[], &entry, &PropertyArgs::default()).unwrap();
        assert_eq!(request.root, folder);
        assert!(!request.remembered);
        let for_now = [resolved(&request.root)];
        assert!(
            trust_request_with(&settings, &for_now, &entry, &PropertyArgs::default()).is_none()
        );
        assert_eq!(
            trusted_root_with(&settings, &for_now, Some(&entry)),
            Some(folder)
        );
    }

    #[test]
    fn a_projects_plugins_are_searched_once_trusted_and_run_once_allowed() {
        let project = Project::new("plugins");
        let entry = project.0.join("film.tsx");
        let mut settings = UserSettings::default();
        let found = |settings: &UserSettings| {
            let root = trusted_root_with(settings, &[], Some(&entry));
            FoundPlugins::find_with(root.as_deref(), settings, None)
        };
        let is_project = |entry: &PluginEntry| entry.manifest.id == "p";

        // Untrusted: the project's folders are not even searched.
        let untrusted = found(&settings);
        assert!(!untrusted.declared().iter().any(is_project));

        // Trusted: found, but waiting, and not on the runtime's path.
        settings.trust(&project.0);
        let waiting = found(&settings);
        assert!(!ids(&waiting.installed).contains(&"p"));
        let pending = waiting
            .pending
            .iter()
            .find(|pending| is_project(&pending.entry))
            .unwrap();
        assert!(waiting.declared().iter().any(is_project));
        assert!(waiting.failure_note().unwrap().contains('p'));
        let runtime: Vec<_> = std::env::split_paths(&waiting.installed.runtime_path()).collect();
        assert!(!runtime.contains(&pending.entry.directory));
        // The runtime stands in for it instead.
        let standing_in: Vec<_> =
            std::env::split_paths(&waiting.installed.runtime().waiting).collect();
        assert_eq!(standing_in, std::slice::from_ref(&pending.entry.directory));

        // Allowed: it runs.
        settings.allow("p", &pending.entry.directory, &pending.fingerprint);
        let allowed = found(&settings);
        assert!(ids(&allowed.installed).contains(&"p"));
        let runtime: Vec<_> = std::env::split_paths(&allowed.installed.runtime_path()).collect();
        assert!(runtime.contains(&project.0.join("plugins").join("p")));
    }
}
