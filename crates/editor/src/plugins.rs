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

use crate::problems::{Problem, Problems, Source, error_chain};
use crate::source::PropertyArgs;
use crate::view::EditorView;
use celesta_editor_theme::metrics;
use celesta_media::{DecoderUse, MediaRegistry};
use celesta_plugin_host::{
    InstalledPlugins, PendingPlugin, PluginEntry, PluginRole, SettingsError, UserSettings,
    from_the_internet, has_project_config, project_plugin_paths, project_root,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogDescription, DialogFooter};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{Context, Window, div};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

/// Wide enough for a plugin's folder on one or two lines.
/// Designed against a 16px `rem`; see [`metrics::scaled`].
const PLUGIN_DIALOG_WIDTH: f32 = 560.;

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
    /// No media decoder can run, so video and audio files cannot be read.
    pub(crate) missing_decoder: bool,
    /// The media plugins that may run, which record the files a project's
    /// decoders read; none until [`Self::find`] makes it.
    pub(crate) registry: Option<Arc<MediaRegistry>>,
}

impl FoundPlugins {
    /// Reads every plugin location, with the trusted project at `project`'s
    /// own folders; blocking, so off the UI thread. Media is read and
    /// written with the media plugins among what may run from now on.
    pub(crate) fn find(project: Option<&Path>) -> Self {
        let (settings, settings_error) = UserSettings::current();
        let mut found = Self::find_with(project, &settings, settings_error);
        let registry = celesta_exporter::use_media(&found.installed);
        found.missing_decoder = !registry.has_decoders();
        found.registry = Some(registry);
        found
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
            missing_decoder: false,
            registry: None,
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
        let decoder = self.missing_decoder.then(|| self.missing_decoder_detail());
        let detail: Vec<String> = decoder
            .into_iter()
            .chain(waiting)
            .chain(self.skipped.iter().cloned())
            .collect();
        let title = match (self.pending.len(), self.skipped.len()) {
            (0, 0) if self.missing_decoder => "No media decoder can run".to_owned(),
            (0, 0) => return None,
            (0, 1) => "A plugin was skipped".to_owned(),
            (0, skipped) => format!("{skipped} plugins were skipped"),
            (1, _) => "A plugin waits for your approval".to_owned(),
            (pending, _) => format!("{pending} plugins wait for your approval"),
        };
        Some(Problem::new(title, detail.join("\n")))
    }

    /// Why there is no decoder to read media with: a media plugin has no
    /// program for this computer, one waits for approval, one is turned off,
    /// those that run declare no decoder, or none was found.
    fn missing_decoder_detail(&self) -> String {
        let is_media = |entry: &PluginEntry| entry.has_role(PluginRole::Media);
        let consequence = "so video and audio files cannot be shown or exported";
        let found = || {
            self.installed
                .plugins
                .iter()
                .chain(self.pending.iter().map(|pending| &pending.entry))
                .chain(&self.disabled)
                .filter(|entry| is_media(entry))
        };
        if let Some(unbuilt) = found().find(|entry| entry.executable().is_none()) {
            let (os, arch) = celesta_plugin_host::platform();
            format!(
                "The media plugin {} has no program for this computer ({os}-{arch}), \
                 {consequence}. Install a build of it for this computer.",
                unbuilt.manifest.id
            )
        } else if let Some(pending) = self.pending.iter().find(|pending| is_media(&pending.entry)) {
            format!(
                "The media plugin {} waits for your approval, {consequence} until you allow it.",
                pending.entry.manifest.id
            )
        } else if let Some(disabled) = self.disabled.iter().find(|entry| is_media(entry)) {
            format!(
                "The media plugin {} is turned off, {consequence}. Turn it on in the Plugins tab.",
                disabled.manifest.id
            )
        } else if self.installed.plugins.iter().any(is_media) {
            let ids: Vec<&str> = self
                .installed
                .plugins
                .iter()
                .filter(|entry| is_media(entry))
                .map(|entry| entry.manifest.id.as_str())
                .collect();
            format!(
                "Media plugins were found, but none declares a decoder Celesta can use, \
                 {consequence}: {}.",
                ids.join(", ")
            )
        } else {
            format!(
                "No media plugin was found, {consequence}. Install a media plugin, or \
                 reinstall Celesta. In a source checkout, build the media plugins under \
                 plugins/ with node scripts/plugins/build.mts and add the absolute path of \
                 plugins/dist to CELESTA_PLUGIN_PATH before starting Celesta."
            )
        }
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

/// The Problems entry for the files a project's own decoders read, so a
/// project cannot decode files unnoticed: one line per plugin, then its
/// files, shown relative to `root` when inside it. `None` without any.
pub(crate) fn decoder_notice(uses: &[DecoderUse], root: Option<&Path>) -> Option<Problem> {
    let mut plugins: Vec<(&str, Vec<String>)> = Vec::new();
    for used in uses {
        let shown = root
            .and_then(|root| used.path.strip_prefix(root).ok())
            .unwrap_or(&used.path)
            .display()
            .to_string();
        let plugin = used.decoder.plugin.as_str();
        match plugins.iter_mut().find(|(id, _)| *id == plugin) {
            Some((_, files)) => files.push(shown),
            None => plugins.push((plugin, vec![shown])),
        }
    }
    let count = |files: usize| match files {
        1 => "1 file".to_owned(),
        files => format!("{files} files"),
    };
    let title = match plugins.as_slice() {
        [] => return None,
        [(plugin, files)] => format!("{plugin} decodes {} in this project", count(files.len())),
        _ => format!(
            "{} project plugins decode {} in this project",
            plugins.len(),
            count(uses.len())
        ),
    };
    let detail: Vec<String> = plugins
        .iter()
        .flat_map(|(plugin, files)| {
            std::iter::once(format!("{plugin} decodes {}:", count(files.len())))
                .chain(files.iter().map(|file| format!("  {file}")))
        })
        .collect();
    Some(Problem::new(title, detail.join("\n")))
}

/// The project-decoder notice as last shown, and the uses it was made from.
#[derive(Debug, Default)]
pub(crate) struct DecoderNotice {
    /// The registry's list generation last read; `None` until the current
    /// registry's list is read.
    generation: Option<u64>,
    uses: Vec<DecoderUse>,
}

impl DecoderNotice {
    /// The files read, as of the last [`Self::sync`].
    pub(crate) fn uses(&self) -> &[DecoderUse] {
        &self.uses
    }

    /// Forgets what was read, for a new registry.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Sets Problems' decoder notice from `registry`'s list, which is read,
    /// and `root` resolved, only when its generation changed since the last
    /// call.
    pub(crate) fn sync(
        &mut self,
        registry: &MediaRegistry,
        root: impl FnOnce() -> Option<PathBuf>,
        problems: &mut Problems,
    ) {
        let generation = registry.project_decoder_generation();
        if self.generation == Some(generation) {
            return;
        }
        let (generation, uses) = registry.project_decoder_uses();
        self.generation = Some(generation);
        self.uses = uses;
        problems.set(
            Source::MediaDecoders,
            decoder_notice(&self.uses, root().as_deref()),
        );
    }
}

impl EditorView {
    /// Shows the files this project's own decoders read in Problems.
    pub(crate) fn sync_decoder_notice(&mut self) {
        let Some(registry) = self.found_plugins.registry.clone() else {
            return;
        };
        let source_path = self.source_path.as_deref();
        self.decoder_notice.sync(
            &registry,
            || source_path.map(project_root),
            &mut self.problems,
        );
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
        // A new registry starts its own list.
        self.decoder_notice.reset();
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
                .w(metrics::scaled_px(PLUGIN_DIALOG_WIDTH))
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
                .w(metrics::scaled_px(PLUGIN_DIALOG_WIDTH))
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

    #[test]
    fn a_missing_decoder_is_a_problem() {
        let missing = FoundPlugins {
            missing_decoder: true,
            ..FoundPlugins::default()
        };
        let problem = missing.problem().unwrap();
        assert_eq!(problem.title, "No media decoder can run");
        for expected in [
            "Install a media plugin",
            "node scripts/plugins/build.mts",
            "absolute path of plugins/dist",
            "CELESTA_PLUGIN_PATH",
            "reinstall Celesta",
        ] {
            assert!(
                problem.detail.contains(expected),
                "{expected}: {}",
                problem.detail
            );
        }
        let skipped = FoundPlugins {
            skipped: vec!["a was skipped".into()],
            ..missing.clone()
        };
        let problem = skipped.problem().unwrap();
        assert_eq!(problem.title, "A plugin was skipped");
        assert!(
            problem.detail.starts_with("No media plugin"),
            "{}",
            problem.detail
        );

        // A media plugin that was found says why it does not run.
        let (os, arch) = celesta_plugin_host::platform();
        let media = PluginEntry {
            manifest: celesta_plugin_host::Manifest {
                id: "ffmpeg".into(),
                name: None,
                version: "1.0.0".into(),
                protocol: 2,
                celesta: Some("*".into()),
                executable: [(format!("{os}-{arch}"), "bin/p".to_owned())].into(),
                module: None,
                types: None,
                components: Vec::new(),
                roles: vec![PluginRole::Media],
                media: None,
            },
            directory: PathBuf::from("plugins/ffmpeg"),
            source: celesta_plugin_host::PluginSource::User,
        };
        let disabled = FoundPlugins {
            disabled: vec![media.clone()],
            ..missing.clone()
        };
        let problem = disabled.problem().unwrap();
        assert_eq!(problem.title, "No media decoder can run");
        assert!(
            problem
                .detail
                .starts_with("The media plugin ffmpeg is turned off"),
            "{}",
            problem.detail
        );
        // Allowing or turning on a media plugin built for another computer would
        // not help.
        let mut elsewhere = media.clone();
        elsewhere.manifest.executable = [("plan9-mips".to_owned(), "bin/p".to_owned())].into();
        let unbuilt = FoundPlugins {
            disabled: vec![elsewhere],
            ..missing.clone()
        };
        assert!(
            unbuilt
                .problem()
                .unwrap()
                .detail
                .starts_with("The media plugin ffmpeg has no program for this computer"),
        );
        let waiting = FoundPlugins {
            pending: vec![PendingPlugin {
                entry: media,
                fingerprint: String::new(),
                changed: false,
                from_the_internet: false,
            }],
            ..missing
        };
        let problem = waiting.problem().unwrap();
        assert_eq!(problem.title, "A plugin waits for your approval");
        assert!(
            problem
                .detail
                .starts_with("The media plugin ffmpeg waits for your approval"),
            "{}",
            problem.detail
        );
    }

    fn used(plugin: &str, path: &str) -> DecoderUse {
        DecoderUse {
            decoder: format!("{plugin}/dec").parse().unwrap(),
            origin: celesta_plugin_host::MediaOrigin::Project,
            path: PathBuf::from(path),
        }
    }

    #[test]
    fn decoder_notice_counts_files() {
        let uses = [used("proj", "/p/a.mov"), used("proj", "/elsewhere/b.mov")];
        let problem = decoder_notice(&uses, Some(Path::new("/p"))).unwrap();
        assert_eq!(problem.title, "proj decodes 2 files in this project");
        assert_eq!(
            problem.detail,
            "proj decodes 2 files:\n  a.mov\n  /elsewhere/b.mov"
        );
        let one = decoder_notice(&uses[..1], None).unwrap();
        assert_eq!(one.title, "proj decodes 1 file in this project");
        let two = decoder_notice(&[used("a", "/x"), used("b", "/y")], None).unwrap();
        assert_eq!(
            two.title,
            "2 project plugins decode 2 files in this project"
        );
    }

    #[test]
    fn decoder_notice_is_none_without_uses() {
        assert_eq!(decoder_notice(&[], None), None);
    }

    #[test]
    fn an_old_codec_plugin_is_not_a_reason() {
        let (os, arch) = celesta_plugin_host::platform();
        let manifest: celesta_plugin_host::Manifest = serde_json::from_value(serde_json::json!({
            "id": "old", "version": "1.0.0", "protocol": 1, "celesta": "*",
            "roles": ["codec"], "executable": { format!("{os}-{arch}"): "bin/p" },
        }))
        .unwrap();
        assert_eq!(manifest.roles, [PluginRole::Unknown]);
        let old = PluginEntry {
            manifest,
            directory: PathBuf::from("plugins/old"),
            source: celesta_plugin_host::PluginSource::User,
        };
        let found = FoundPlugins {
            missing_decoder: true,
            installed: InstalledPlugins {
                plugins: vec![old],
                ..Default::default()
            },
            ..FoundPlugins::default()
        };
        let problem = found.problem().unwrap();
        assert_eq!(problem.title, "No media decoder can run");
        assert!(
            problem.detail.contains("No media plugin was found"),
            "{}",
            problem.detail
        );
    }

    #[test]
    fn the_decoder_notice_is_read_only_when_its_list_changes() {
        let registry = MediaRegistry::new(Vec::new());
        let decoder: celesta_media::ComponentRef = "proj/dec".parse().unwrap();
        let project = celesta_plugin_host::MediaOrigin::Project;
        let mut problems = Problems::default();
        let mut notice = DecoderNotice::default();
        let title = |problems: &Problems| {
            problems
                .active(Source::MediaDecoders)
                .map(|active| active.problem.title.clone())
        };

        notice.sync(&registry, || None, &mut problems);
        assert_eq!(title(&problems), None);

        registry.record_use(&decoder, project, Path::new("/a"));
        notice.sync(&registry, || None, &mut problems);
        assert_eq!(
            title(&problems).as_deref(),
            Some("proj decodes 1 file in this project")
        );
        assert_eq!(notice.uses().len(), 1);

        // Cleared by hand, an unchanged list is not read again, nor the
        // root resolved.
        problems.clear(Source::MediaDecoders);
        notice.sync(
            &registry,
            || panic!("the root is resolved for an unchanged list"),
            &mut problems,
        );
        assert_eq!(title(&problems), None);

        registry.record_use(&decoder, project, Path::new("/b"));
        notice.sync(&registry, || None, &mut problems);
        assert_eq!(
            title(&problems).as_deref(),
            Some("proj decodes 2 files in this project")
        );

        // A new registry (set_plugins) is read afresh, even at generation 0.
        notice.reset();
        assert!(notice.uses().is_empty());
        notice.sync(&MediaRegistry::new(Vec::new()), || None, &mut problems);
        assert_eq!(title(&problems), None);
    }

    #[test]
    fn a_waiting_media_plugin_is_the_reason() {
        let found = FoundPlugins {
            missing_decoder: true,
            pending: vec![PendingPlugin {
                entry: entry_with_role(PluginRole::Media),
                fingerprint: String::new(),
                changed: false,
                from_the_internet: false,
            }],
            ..FoundPlugins::default()
        };
        let problem = found.problem().unwrap();
        assert!(
            problem
                .detail
                .starts_with("The media plugin ffmpeg waits for your approval"),
            "{}",
            problem.detail
        );
    }

    #[test]
    fn media_plugins_without_decoders_are_named() {
        let found = FoundPlugins {
            missing_decoder: true,
            installed: InstalledPlugins {
                plugins: vec![
                    entry_with_role(PluginRole::Media),
                    entry_with_role(PluginRole::Content),
                ],
                ..Default::default()
            },
            ..FoundPlugins::default()
        };
        let problem = found.problem().unwrap();
        assert_eq!(problem.title, "No media decoder can run");
        assert_eq!(
            problem.detail,
            "Media plugins were found, but none declares a decoder Celesta can use, so video \
             and audio files cannot be shown or exported: ffmpeg."
        );
    }

    fn entry_with_role(role: PluginRole) -> PluginEntry {
        let (os, arch) = celesta_plugin_host::platform();
        PluginEntry {
            manifest: celesta_plugin_host::Manifest {
                id: "ffmpeg".into(),
                name: None,
                version: "1.0.0".into(),
                protocol: 2,
                celesta: Some("*".into()),
                executable: [(format!("{os}-{arch}"), "bin/p".to_owned())].into(),
                module: None,
                types: None,
                components: Vec::new(),
                roles: vec![role],
                media: None,
            },
            directory: PathBuf::from("plugins/ffmpeg"),
            source: celesta_plugin_host::PluginSource::User,
        }
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
                r#"{ "id": "p", "version": "1", "protocol": 1, "celesta": "*", "module": "js/index.js" }"#,
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
