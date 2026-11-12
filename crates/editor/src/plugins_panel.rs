//! The right dock's Plugins tab: every plugin the last discovery found, where
//! it came from, whether it can run on this computer, and whether it may
//! run; and the folders searched besides the standard ones. Allowing a
//! plugin, turning one on or off, and adding or removing a folder are saved
//! in the user's settings, and the project reopens with the plugins as they
//! now are.

use crate::actions::ReloadPlugins;
use crate::helpers::{PathPick, pick_one_path};
use crate::plugins::FoundPlugins;
use crate::problems::{Source, error_chain};
use crate::view::EditorView;
use crate::widgets::CompactLabel as _;
use celesta_media::DecoderUse;
use celesta_plugin_host::{
    PluginEntry, PluginSource, SettingsError, UserSettings, create_private_dir_all, user_plugin_dir,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, SharedString, Window, div};

/// Whether a listed plugin may run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PluginState {
    /// It runs (allowed, or from a place that is not asked about).
    On,
    /// It waits for the user to allow it as it is now.
    Waiting {
        fingerprint: String,
        changed: bool,
        from_the_internet: bool,
    },
    /// The user turned it off.
    Off,
}

/// How a row's switch shows its plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SwitchLook {
    /// The plugin runs.
    pub(crate) on: bool,
    /// The switch can be used: not while the plugin waits for approval,
    /// which the switch cannot give.
    pub(crate) enabled: bool,
    pub(crate) tooltip: &'static str,
}

/// One plugin as the Plugins tab lists it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PluginRow {
    pub(crate) entry: PluginEntry,
    pub(crate) state: PluginState,
}

impl PluginRow {
    /// Every plugin `found` holds, by where it came from (in search order),
    /// then by id.
    pub(crate) fn all(found: &FoundPlugins) -> Vec<Self> {
        let on = found.installed.plugins.iter().map(|entry| Self {
            entry: entry.clone(),
            state: PluginState::On,
        });
        let waiting = found.pending.iter().map(|pending| Self {
            entry: pending.entry.clone(),
            state: PluginState::Waiting {
                fingerprint: pending.fingerprint.clone(),
                changed: pending.changed,
                from_the_internet: pending.from_the_internet,
            },
        });
        let off = found.disabled.iter().map(|entry| Self {
            entry: entry.clone(),
            state: PluginState::Off,
        });
        let mut rows: Vec<Self> = on.chain(waiting).chain(off).collect();
        rows.sort_by(|a, b| {
            (a.entry.source, &a.entry.manifest.id).cmp(&(b.entry.source, &b.entry.manifest.id))
        });
        rows
    }

    /// What the plugin brings and whether this computer can run it.
    pub(crate) fn availability(&self) -> &'static str {
        let manifest = &self.entry.manifest;
        if manifest.executable.is_empty() {
            "Components only"
        } else if self.entry.executable().is_some() {
            "Runs on this computer"
        } else {
            "Not built for this computer"
        }
    }

    /// How many files this project plugin's decoders read, when it is one
    /// and has read any.
    pub(crate) fn decoder_uses(&self, uses: &[DecoderUse]) -> Option<String> {
        if self.entry.source != PluginSource::Project {
            return None;
        }
        let count = uses
            .iter()
            .filter(|used| used.decoder.plugin == self.entry.manifest.id)
            .count();
        match count {
            0 => None,
            1 => Some("Decodes 1 file in this project".to_owned()),
            count => Some(format!("Decodes {count} files in this project")),
        }
    }

    /// A waiting plugin does not run, so its switch is off, and greyed out
    /// until it is allowed: allowing it is **Allow**'s, and turning it off
    /// **Turn Off**'s.
    pub(crate) fn switch(&self) -> SwitchLook {
        match self.state {
            PluginState::On => SwitchLook {
                on: true,
                enabled: true,
                tooltip: "On or off",
            },
            PluginState::Waiting { .. } => SwitchLook {
                on: false,
                enabled: false,
                tooltip: "Runs once allowed",
            },
            PluginState::Off => SwitchLook {
                on: false,
                enabled: true,
                tooltip: "On or off",
            },
        }
    }

    pub(crate) fn state_label(&self) -> &'static str {
        match &self.state {
            PluginState::On => "On",
            PluginState::Waiting { changed: true, .. } => "Changed: waits for approval",
            PluginState::Waiting { .. } => "Waits for approval",
            PluginState::Off => "Off",
        }
    }
}

impl EditorView {
    pub(crate) fn plugins_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let rows = PluginRow::all(&self.found_plugins);
        // As discovery read them, not from disk on every frame.
        let folders = self.found_plugins.folders.clone();
        let skipped = self.found_plugins.skipped.clone();
        let toolbar = h_flex()
            .flex_wrap()
            .gap_1()
            .px_3()
            .py_2()
            .child(
                Button::new("plugins-reload")
                    .xsmall()
                    .outline()
                    .compact_label("Reload Plugins")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.reload_plugins_action(&ReloadPlugins, window, cx);
                    })),
            )
            .child(
                Button::new("plugins-add-folder")
                    .xsmall()
                    .ghost()
                    .compact_label("Add Folder…")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_plugin_folder(window, cx);
                    })),
            )
            .child(
                Button::new("plugins-open-user-folder")
                    .xsmall()
                    .ghost()
                    .compact_label("Open Plugin Folder")
                    .on_click(cx.listener(|this, _, _, cx| this.open_user_plugin_folder(cx))),
            );
        div()
            .id("plugins-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .child(toolbar)
            .child(heading("Installed", cx))
            .when(rows.is_empty(), |panel| {
                panel.child(note(
                    "No plugins found. Put a plugin’s folder in your plugin folder, then reload.",
                    cx,
                ))
            })
            .children(rows.into_iter().enumerate().map(|(index, row)| {
                let decoder_uses = row.decoder_uses(self.decoder_notice.uses());
                self.plugin_row(index, row, decoder_uses, cx)
            }))
            .child(heading("Added folders", cx))
            .when(folders.is_empty(), |panel| {
                panel.child(note(
                    "Folders added here are searched after your plugin folder.",
                    cx,
                ))
            })
            .children(folders.into_iter().enumerate().map(|(index, folder)| {
                let shown: SharedString = folder.display().to_string().into();
                h_flex()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .justify_between()
                    .child(div().min_w_0().text_sm().text_color(muted).child(shown))
                    .child(
                        Button::new(("plugins-remove-folder", index))
                            .xsmall()
                            .ghost()
                            .compact_label("Remove")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let folder = folder.clone();
                                this.change_plugin_settings(
                                    move |settings| {
                                        settings.remove_plugin_folder(&folder);
                                        Ok(())
                                    },
                                    window,
                                    cx,
                                );
                            })),
                    )
            }))
            .when(!skipped.is_empty(), |panel| {
                panel
                    .child(heading("Skipped", cx))
                    .children(skipped.into_iter().map(|reason| {
                        div()
                            .px_3()
                            .py_1()
                            .text_sm()
                            .text_color(muted)
                            .child(reason)
                    }))
            })
    }

    fn plugin_row(
        &self,
        index: usize,
        row: PluginRow,
        decoder_uses: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let warning = cx.theme().warning;
        let manifest = &row.entry.manifest;
        let title = format!("{} {}", manifest.id, manifest.version);
        let detail = format!("{} · {}", row.entry.source.label(), row.availability());
        let directory = row.entry.directory.clone();
        let id = manifest.id.clone();
        let internet = matches!(
            row.state,
            PluginState::Waiting {
                from_the_internet: true,
                ..
            }
        );
        let turn_off = matches!(row.state, PluginState::Waiting { .. }).then(|| {
            let (id, directory) = (id.clone(), directory.clone());
            Button::new(("plugin-turn-off", index))
                .xsmall()
                .ghost()
                .compact_label("Turn Off")
                .on_click(cx.listener(move |this, _, window, cx| {
                    let (id, directory) = (id.clone(), directory.clone());
                    this.change_plugin_settings(
                        move |settings| {
                            settings.set_enabled(&id, &directory, false);
                            Ok(())
                        },
                        window,
                        cx,
                    );
                }))
        });
        let allow = match &row.state {
            PluginState::Waiting { fingerprint, .. } => {
                let (id, directory, fingerprint) =
                    (id.clone(), directory.clone(), fingerprint.clone());
                Some(
                    Button::new(("plugin-allow", index))
                        .xsmall()
                        .primary()
                        .compact_label("Allow")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let (id, directory, fingerprint) =
                                (id.clone(), directory.clone(), fingerprint.clone());
                            this.change_plugin_settings(
                                move |settings| {
                                    settings.allow(&id, &directory, &fingerprint);
                                    Ok(())
                                },
                                window,
                                cx,
                            );
                        })),
                )
            }
            _ => None,
        };
        let switch = {
            let (id, directory) = (id.clone(), directory.clone());
            let look = row.switch();
            Switch::new(("plugin-enabled", index))
                .small()
                .checked(look.on)
                .disabled(!look.enabled)
                .tooltip(look.tooltip)
                .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                    let (id, directory, checked) = (id.clone(), directory.clone(), *checked);
                    this.change_plugin_settings(
                        move |settings| {
                            settings.set_enabled(&id, &directory, checked);
                            Ok(())
                        },
                        window,
                        cx,
                    );
                }))
        };
        let show = {
            let directory = directory.clone();
            Button::new(("plugin-show-folder", index))
                .xsmall()
                .ghost()
                .compact_label("Show Folder")
                .on_click(cx.listener(move |_, _, _, cx| cx.open_with_system(&directory)))
        };
        v_flex()
            .px_3()
            .py_2()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .justify_between()
                    .gap_2()
                    .child(div().text_base().child(title))
                    .child(switch),
            )
            .child(div().text_sm().text_color(muted).child(detail))
            .children(decoder_uses.map(|text| div().text_sm().text_color(muted).child(text)))
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(row.entry.directory.display().to_string()),
            )
            .child(
                div()
                    .text_sm()
                    .when(row.state == PluginState::Off, |label| {
                        label.text_color(muted)
                    })
                    .when(matches!(row.state, PluginState::Waiting { .. }), |label| {
                        label.text_color(warning)
                    })
                    .child(row.state_label()),
            )
            .when(internet, |column| {
                column.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child("Downloaded from the internet."),
                )
            })
            .child(
                h_flex()
                    .gap_1()
                    .children(allow)
                    .children(turn_off)
                    .child(show),
            )
            .into_any_element()
    }

    /// Asks for a folder and adds it to the folders searched for plugins.
    fn add_plugin_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selection = pick_one_path(cx, PathPick::Folder, "Add Plugin Folder");
        cx.spawn_in(window, async move |view, cx| {
            let folder = match selection.await {
                Ok(path) => path,
                Err(error) => {
                    view.update(cx, |this, cx| {
                        this.problems.report(
                            Source::Plugins,
                            "Couldn’t show the folder dialog",
                            error_chain(error.as_ref()),
                        );
                        cx.notify();
                    })
                    .ok();
                    None
                }
            };
            let Some(folder) = folder else {
                return;
            };
            view.update_in(cx, |this, window, cx| {
                this.change_plugin_settings(
                    move |settings| settings.add_plugin_folder(&folder),
                    window,
                    cx,
                );
            })
            .ok();
        })
        .detach();
    }

    /// Opens the user's plugin folder, making it first if it is not there.
    fn open_user_plugin_folder(&mut self, cx: &mut Context<Self>) {
        let Some(folder) = user_plugin_dir() else {
            self.problems.report(
                Source::Plugins,
                "There is no user plugin folder",
                "The platform’s home or app-data variable is not set.",
            );
            cx.notify();
            return;
        };
        // The user's alone (on Unix), or discovery would skip it as a
        // folder other users can change.
        if let Err(error) = create_private_dir_all(&folder) {
            self.problems.report(
                Source::Plugins,
                format!("Couldn’t create {}", folder.display()),
                error_chain(&error),
            );
            cx.notify();
            return;
        }
        cx.open_with_system(&folder);
    }

    /// Saves `change` to the user's settings, then reopens the project so
    /// its plugins are found as the settings now say. Refused, before
    /// anything is saved, while the project cannot be reopened (it is
    /// opening, an export runs, or the save panel is up), so the tab never shows a choice that has
    /// not taken effect.
    fn change_plugin_settings(
        &mut self,
        change: impl FnOnce(&mut UserSettings) -> Result<(), SettingsError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.opening {
            window.push_notification("Wait for the project to open, then try again.", cx);
            return;
        }
        if !self.can_replace_contents(window, cx) {
            return;
        }
        let mut outcome = Ok(());
        let saved = UserSettings::update(|settings| outcome = change(settings));
        match saved
            .map_err(|error| error_chain(&error))
            .and_then(|_| outcome.map_err(|error| error_chain(&error)))
        {
            Ok(()) => self.reload_plugins_action(&ReloadPlugins, window, cx),
            Err(error) => {
                self.problems.report(
                    Source::Plugins,
                    "Couldn’t change the plugin settings",
                    error,
                );
                cx.notify();
            }
        }
    }
}

fn heading(title: &'static str, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .pt_3()
        .pb_1()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(title)
}

fn note(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .pb_2()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_plugin_host::{Manifest, PendingPlugin, PluginSource};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn entry(id: &str, source: PluginSource, executable: &[(&str, &str)]) -> PluginEntry {
        PluginEntry {
            manifest: Manifest {
                id: id.into(),
                name: None,
                version: "1.0.0".into(),
                protocol: 1,
                celesta: Some("*".into()),
                executable: executable
                    .iter()
                    .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                    .collect::<BTreeMap<_, _>>(),
                module: Some("js/index.js".into()),
                types: None,
                components: Vec::new(),
                roles: vec![celesta_plugin_host::PluginRole::Content],
                media: None,
            },
            directory: PathBuf::from("plugins").join(id),
            source,
        }
    }

    #[test]
    fn rows_list_every_plugin_in_search_order() {
        let found = FoundPlugins {
            installed: celesta_plugin_host::InstalledPlugins {
                plugins: vec![
                    entry("zeta", PluginSource::Bundled, &[]),
                    entry("alpha", PluginSource::User, &[]),
                ],
                ..Default::default()
            },
            pending: vec![PendingPlugin {
                entry: entry("beta", PluginSource::User, &[]),
                fingerprint: "sha256:1".into(),
                changed: true,
                from_the_internet: false,
            }],
            // The user's zeta is off, so the bundled one is used: both listed.
            disabled: vec![
                entry("gamma", PluginSource::Project, &[]),
                entry("zeta", PluginSource::User, &[]),
            ],
            ..FoundPlugins::default()
        };
        let rows = PluginRow::all(&found);
        let listed: Vec<(&str, &str)> = rows
            .iter()
            .map(|row| (row.entry.manifest.id.as_str(), row.state_label()))
            .collect();
        assert_eq!(
            listed,
            [
                ("gamma", "Off"),
                ("alpha", "On"),
                ("beta", "Changed: waits for approval"),
                ("zeta", "Off"),
                ("zeta", "On"),
            ]
        );
    }

    #[test]
    fn a_project_plugin_row_shows_its_decoder_uses() {
        let used = |plugin: &str, path: &str| DecoderUse {
            decoder: format!("{plugin}/dec").parse().unwrap(),
            origin: celesta_plugin_host::MediaOrigin::Project,
            path: PathBuf::from(path),
        };
        let row = |source| PluginRow {
            entry: entry("proj", source, &[]),
            state: PluginState::On,
        };
        let uses = [
            used("proj", "/a"),
            used("proj", "/b"),
            used("other", "/c"),
            used("proj", "/d"),
        ];
        assert_eq!(
            row(PluginSource::Project).decoder_uses(&uses).as_deref(),
            Some("Decodes 3 files in this project")
        );
        assert_eq!(
            row(PluginSource::Project)
                .decoder_uses(&uses[..1])
                .as_deref(),
            Some("Decodes 1 file in this project")
        );
        assert_eq!(row(PluginSource::Project).decoder_uses(&[]), None);
        assert_eq!(row(PluginSource::User).decoder_uses(&uses), None);
    }

    #[test]
    fn a_waiting_plugins_switch_is_off_and_greyed_out() {
        let row = |state: PluginState| PluginRow {
            entry: entry("p", PluginSource::User, &[]),
            state,
        };
        let waiting = |changed| PluginState::Waiting {
            fingerprint: "sha256:1".into(),
            changed,
            from_the_internet: false,
        };
        assert_eq!(
            row(PluginState::On).switch(),
            SwitchLook {
                on: true,
                enabled: true,
                tooltip: "On or off"
            }
        );
        assert_eq!(
            row(PluginState::Off).switch(),
            SwitchLook {
                on: false,
                enabled: true,
                tooltip: "On or off"
            }
        );
        for changed in [false, true] {
            assert_eq!(
                row(waiting(changed)).switch(),
                SwitchLook {
                    on: false,
                    enabled: false,
                    tooltip: "Runs once allowed"
                }
            );
        }
    }

    #[test]
    fn availability_says_whether_this_computer_can_run_it() {
        let (os, _) = celesta_plugin_host::platform();
        let row = |executable: &[(&str, &str)]| PluginRow {
            entry: entry("p", PluginSource::User, executable),
            state: PluginState::On,
        };
        assert_eq!(row(&[]).availability(), "Components only");
        assert_eq!(
            row(&[(os, "bin/p")]).availability(),
            "Runs on this computer"
        );
        assert_eq!(
            row(&[("plan9", "bin/p")]).availability(),
            "Not built for this computer"
        );
    }
}
