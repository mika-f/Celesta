//! The right dock's Plugins tab: the plugins the open project locks, the
//! ones installed for every project, the ones placed by hand, and the
//! folders searched besides the standard ones (see
//! [`crate::plugins_tab_model`]). Allowing a plugin, turning one on or off,
//! and adding or removing a folder are saved in the user's settings, and the
//! project reopens with the plugins as they now are; updating, downloading,
//! removing, and uninstalling run as the window's registry work.

use crate::actions::ReloadPlugins;
use crate::helpers::{PathPick, pick_one_path};
use crate::plugins_tab_model::{
    MenuCommand, RowAction, RowState, SectionAction, TabModel, TabRow, tab_model,
};
use crate::problems::{Source, error_chain};
use crate::registry::RegistryState;
use crate::toast::Toast;
use crate::view::EditorView;
use crate::widgets::CompactLabel as _;
use celesta_media::DecoderUse;
use celesta_plugin_host::{SettingsError, UserSettings, create_private_dir_all, user_plugin_dir};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Selectable as _, Sizable as _, WindowExt as _,
    h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{Anchor, AnyElement, App, Context, ElementId, SharedString, Window, div, rems};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The width of a row's action lane, in `rem`: wide enough for
/// `Update to 0.10.0` at the xsmall button size; a longer label widens its
/// own row's lane.
const ACTION_LANE_WIDTH: f32 = 8.;

/// Which section a row is in, which says what its commands work on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Project,
    Global,
    ByHand,
}

impl Section {
    fn key(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Global => "global",
            Self::ByHand => "by-hand",
        }
    }
}

/// The copy a row's button or menu item works on.
#[derive(Clone, Debug)]
struct RowTarget {
    section: Section,
    id: String,
    version: String,
    directory: Option<PathBuf>,
}

/// What a row's button or menu item does.
#[derive(Clone, Debug)]
enum RowCommand {
    Action(RowAction),
    Menu(MenuCommand),
}

/// How many files the project plugin `id`'s decoders read, when it has
/// read any.
fn decoder_uses(id: &str, uses: &[DecoderUse]) -> Option<String> {
    let count = uses.iter().filter(|used| used.decoder.plugin == id).count();
    match count {
        0 => None,
        1 => Some("Decodes 1 file in this project".to_owned()),
        count => Some(format!("Decodes {count} files in this project")),
    }
}

/// A row's key among the tab's element ids: its section, its id, and its
/// folder (or version), so two copies of one id do not share one.
fn row_key(section: Section, row: &TabRow) -> SharedString {
    let copy = row
        .directory
        .as_ref()
        .map_or_else(|| row.version.clone(), |dir| dir.display().to_string());
    format!("{}:{}:{copy}", section.key(), row.id).into()
}

impl EditorView {
    pub(crate) fn plugins_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let model = tab_model(
            &self.found_plugins,
            self.registry.as_ref().map(|registry| registry.read(cx)),
        );
        let TabModel {
            project,
            global,
            by_hand,
            folders,
            skipped,
        } = model;
        let muted = cx.theme().muted_foreground;
        let view = cx.entity().downgrade();
        let toolbar = h_flex()
            .gap_1()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                // A toggle: "on" while the plugin page shows.
                Button::new("plugins-find")
                    .xsmall()
                    .outline()
                    .compact_label("Find plugins…")
                    .selected(self.plugin_page.is_open())
                    .toggled(self.plugin_page.is_open())
                    .disabled(self.registry.is_none())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_plugin_page(window, cx);
                    })),
            )
            .child(
                Button::new("plugins-reload")
                    .xsmall()
                    .outline()
                    .compact_label("Reload Plugins")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.reload_plugins_action(&ReloadPlugins, window, cx);
                    })),
            )
            .child(div().flex_1())
            .child(
                Button::new("plugins-options")
                    .xsmall()
                    .ghost()
                    .icon(IconName::Ellipsis)
                    .tooltip("Options")
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                        let (add, open) = (view.clone(), view.clone());
                        menu.item(PopupMenuItem::new("Add Folder…").on_click(
                            move |_, window, cx| {
                                let _ =
                                    add.update(cx, |this, cx| this.add_plugin_folder(window, cx));
                            },
                        ))
                        .item(
                            PopupMenuItem::new("Open Plugin Folder").on_click(move |_, _, cx| {
                                let _ =
                                    open.update(cx, |this, cx| this.open_user_plugin_folder(cx));
                            }),
                        )
                    }),
            );
        let uses = self.decoder_notice.uses();
        let mut panel = div()
            .id("plugins-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .child(toolbar);
        if let Some(section) = project {
            let trailing = h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(section.lock_label),
                )
                .children(section.action.map(|action| self.section_action(action, cx)));
            panel = panel
                .child(heading(
                    "This project",
                    Some(trailing.into_any_element()),
                    cx,
                ))
                .when(section.rows.is_empty(), |panel| {
                    panel.child(note("No plugins locked", cx))
                })
                .children(
                    section
                        .rows
                        .into_iter()
                        .map(|row| self.tab_row(Section::Project, row, None, cx)),
                );
        }
        panel
            .child(heading("Global", None, cx))
            .when(global.is_empty(), |panel| {
                panel.child(note("No global plugins", cx))
            })
            .children(
                global
                    .into_iter()
                    .map(|row| self.tab_row(Section::Global, row, None, cx)),
            )
            .child(heading("Installed by hand", None, cx))
            .when(by_hand.is_empty(), |panel| {
                panel.child(note(
                    "Put a plugin’s folder in your plugin folder, then reload.",
                    cx,
                ))
            })
            .children(by_hand.into_iter().map(|row| {
                let uses = row
                    .project_folder
                    .then(|| decoder_uses(&row.id, uses))
                    .flatten();
                self.tab_row(Section::ByHand, row, uses, cx)
            }))
            .child(heading("Added folders", None, cx))
            .when(folders.is_empty(), |panel| {
                panel.child(note("No folders added", cx))
            })
            .children(folders.into_iter().map(|folder| {
                let shown: SharedString = folder.display().to_string().into();
                h_flex()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .justify_between()
                    .child(
                        div()
                            .min_w_0()
                            .text_sm()
                            .text_color(muted)
                            .child(shown.clone()),
                    )
                    .child(
                        Button::new((ElementId::from("plugins-remove-folder"), shown))
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
                    .child(heading("Skipped", None, cx))
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

    /// The This project heading's action.
    fn section_action(&self, action: SectionAction, cx: &mut Context<Self>) -> AnyElement {
        match action {
            SectionAction::Download { .. } => Button::new("plugins-tab-download")
                .xsmall()
                .outline()
                .compact_label("Download")
                .on_click(cx.listener(|this, _, window, cx| {
                    let Some(root) = this.found_plugins.project.clone() else {
                        return;
                    };
                    this.registry_work(window, cx, |state, cx| state.download_locked(root, cx));
                }))
                .into_any_element(),
            SectionAction::UpdatePlugins => Button::new("plugins-tab-update-plugins")
                .xsmall()
                .outline()
                // It may ask first (requirements to rewrite).
                .compact_label("Update Plugins…")
                .disabled(self.preparing_update)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.update_project_plugins(window, cx);
                }))
                .into_any_element(),
            SectionAction::Working { label, done, total } => h_flex()
                .gap_1()
                .child(label)
                .when(total > 0, |working| {
                    working.child(
                        div()
                            .font_family(cx.theme().mono_font_family.clone())
                            .child(percent(done, total)),
                    )
                })
                .into_any_element(),
        }
    }

    /// One copy: its id and version, then its kind and state, with its one
    /// action and its ⋯ menu trailing.
    fn tab_row(
        &self,
        section: Section,
        row: TabRow,
        decoder_uses: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (muted, mono, border) = (
            theme.muted_foreground,
            theme.mono_font_family.clone(),
            theme.border,
        );
        let state_color = match &row.state {
            RowState::Blocked { .. } => theme.danger,
            RowState::Waiting { .. }
            | RowState::Deprecated { .. }
            | RowState::Yanked
            | RowState::NotInstalled => theme.warning,
            _ => muted,
        };
        let danger = theme.danger;
        let key = row_key(section, &row);
        let detail = row.detail();
        let progress = match row.state {
            RowState::Working { done, total } if total > 0 => Some(percent(done, total)),
            _ => None,
        };
        let internet = matches!(
            row.state,
            RowState::Waiting {
                from_the_internet: true,
                ..
            }
        );
        let target = RowTarget {
            section,
            id: row.id.clone(),
            version: row.version.clone(),
            directory: row.directory.clone(),
        };
        let action = row.action.clone().map(|action| {
            let name = match action {
                RowAction::Update { .. } => "plugins-tab-update",
                RowAction::Download => "plugins-tab-download",
                RowAction::Allow { .. } => "plugins-tab-allow",
            };
            let button = Button::new((ElementId::from(name), key.clone()))
                .xsmall()
                .compact_label(action.title());
            let button = match action {
                RowAction::Allow { .. } => button.primary(),
                _ => button.outline(),
            };
            let target = target.clone();
            button.on_click(cx.listener(move |this, _, window, cx| {
                this.run_row_command(&target, RowCommand::Action(action.clone()), window, cx);
            }))
        });
        let commands = row.menu.clone();
        let view = cx.entity().downgrade();
        let menu = Button::new((ElementId::from("plugins-tab-menu"), key))
            .xsmall()
            .ghost()
            .icon(IconName::Ellipsis)
            .tooltip("More")
            .disabled(commands.is_empty())
            .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                for (index, command) in commands.iter().enumerate() {
                    // The commands that take the plugin away stand apart.
                    let removes = matches!(
                        command,
                        MenuCommand::RemoveFromProject | MenuCommand::Uninstall
                    );
                    if removes && index > 0 {
                        menu = menu.separator();
                    }
                    let (view, target, command) = (view.clone(), target.clone(), *command);
                    menu = menu.item(PopupMenuItem::new(command.title()).on_click(
                        move |_, window, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.run_row_command(
                                    &target,
                                    RowCommand::Menu(command),
                                    window,
                                    cx,
                                );
                            });
                        },
                    ));
                }
                menu
            });
        h_flex()
            .px_3()
            .py_1p5()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(border)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        h_flex()
                            .gap_1p5()
                            .items_baseline()
                            .child(div().text_base().child(row.id))
                            .child(
                                div()
                                    .font_family(mono.clone())
                                    .text_sm()
                                    .text_color(muted)
                                    .child(row.version),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .text_sm()
                            .child(div().text_color(state_color).child(detail))
                            .children(progress.map(|progress| {
                                div().font_family(mono).text_color(muted).child(progress)
                            })),
                    )
                    .children(
                        decoder_uses.map(|text| div().text_sm().text_color(muted).child(text)),
                    )
                    .when(internet, |column| {
                        column.child(
                            div()
                                .text_sm()
                                .text_color(danger)
                                .child("Downloaded from the internet."),
                        )
                    }),
            )
            // A lane of its own, so actions and menus line up down the tab
            // whether or not a row has an action.
            .child(
                h_flex()
                    .flex_none()
                    .min_w(rems(ACTION_LANE_WIDTH))
                    .justify_end()
                    .children(action),
            )
            .child(menu)
            .into_any_element()
    }

    /// Runs a row's action or menu command on the copy `target` names.
    fn run_row_command(
        &mut self,
        target: &RowTarget,
        command: RowCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = target.id.clone();
        let root = self.found_plugins.project.clone();
        match command {
            RowCommand::Action(RowAction::Allow { fingerprint }) => {
                let Some(directory) = target.directory.clone() else {
                    return;
                };
                self.change_plugin_settings(
                    move |settings| {
                        settings.allow(&id, &directory, &fingerprint);
                        Ok(())
                    },
                    window,
                    cx,
                );
            }
            RowCommand::Action(RowAction::Update { .. }) => match (target.section, root) {
                // `search.json`'s latest is within the requirement, so no
                // requirement changes.
                (Section::Project, Some(root)) => {
                    let ids = BTreeSet::from([id]);
                    self.registry_work(window, cx, |state, cx| {
                        state.update(root, ids, Vec::new(), cx)
                    });
                }
                (Section::Global, _) => {
                    self.registry_work(window, cx, |state, cx| state.update_global(&id, cx));
                }
                _ => {}
            },
            RowCommand::Action(RowAction::Download) => match (target.section, root) {
                (Section::Project, Some(root)) => {
                    self.registry_work(window, cx, |state, cx| state.download_locked(root, cx));
                }
                (Section::Global, _) => {
                    let Ok(version) = semver::Version::parse(&target.version) else {
                        return;
                    };
                    self.registry_work(window, cx, |state, cx| {
                        state.install_global(&id, version, cx)
                    });
                }
                _ => {}
            },
            RowCommand::Menu(command @ (MenuCommand::TurnOff | MenuCommand::TurnOn)) => {
                let Some(directory) = target.directory.clone() else {
                    return;
                };
                let on = command == MenuCommand::TurnOn;
                self.change_plugin_settings(
                    move |settings| {
                        settings.set_enabled(&id, &directory, on);
                        Ok(())
                    },
                    window,
                    cx,
                );
            }
            RowCommand::Menu(MenuCommand::ShowFolder) => {
                if let Some(directory) = &target.directory {
                    cx.open_with_system(directory);
                }
            }
            RowCommand::Menu(MenuCommand::RemoveFromProject) => {
                if let Some(root) = root {
                    self.registry_work(window, cx, |state, cx| state.remove(root, &id, cx));
                }
            }
            RowCommand::Menu(MenuCommand::Uninstall) => {
                self.registry_work(window, cx, |state, cx| state.uninstall(&id, cx));
            }
            RowCommand::Menu(MenuCommand::ShowInFindPlugins) => {
                self.open_plugin_page(Some(id), window, cx);
            }
        }
    }

    /// Starts registry work with `start`; work it refuses (more for the same
    /// plugin, project, or settings is running) is said so in a note.
    pub(crate) fn registry_work(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        start: impl FnOnce(&mut RegistryState, &mut Context<RegistryState>) -> bool,
    ) {
        let Some(registry) = self.registry.clone() else {
            return;
        };
        if !registry.update(cx, start) {
            window.push_notification(
                Toast::note(
                    "Another plugin change is running. Try again when it finishes.",
                    cx,
                ),
                cx,
            );
        }
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
            window.push_notification(
                Toast::note("Wait for the project to open, then try again.", cx),
                cx,
            );
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

/// A section's title, with what it shows of the section as a whole
/// trailing.
fn heading(title: &'static str, trailing: Option<AnyElement>, cx: &App) -> impl IntoElement {
    h_flex()
        .px_3()
        .pt_3()
        .pb_1()
        .gap_2()
        .justify_between()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(title)
        .children(trailing)
}

fn note(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .pb_2()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

/// How far a download has come, in whole percent.
fn percent(done: u64, total: u64) -> String {
    format!("{}%", done.saturating_mul(100) / total.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_project_plugin_row_shows_its_decoder_uses() {
        let used = |plugin: &str, path: &str| DecoderUse {
            decoder: format!("{plugin}/dec").parse().unwrap(),
            origin: celesta_plugin_host::MediaOrigin::Project,
            path: PathBuf::from(path),
        };
        let uses = [
            used("proj", "/a"),
            used("proj", "/b"),
            used("other", "/c"),
            used("proj", "/d"),
        ];
        assert_eq!(
            decoder_uses("proj", &uses).as_deref(),
            Some("Decodes 3 files in this project")
        );
        assert_eq!(
            decoder_uses("proj", &uses[..1]).as_deref(),
            Some("Decodes 1 file in this project")
        );
        assert_eq!(decoder_uses("proj", &[]), None);
    }

    #[test]
    fn two_copies_of_one_id_have_their_own_element_ids() {
        let row = |directory: &str| TabRow {
            id: "avfoundation".into(),
            version: "0.10.0-alpha.3".into(),
            kind: "Media",
            state: RowState::On,
            action: None,
            menu: Vec::new(),
            directory: Some(PathBuf::from(directory)),
            project_folder: false,
            bundled: false,
        };
        let (store, bundled) = (row("/store/avfoundation"), row("/bundled/avfoundation"));
        assert_ne!(
            row_key(Section::Global, &store),
            row_key(Section::Global, &bundled)
        );
        assert_ne!(
            row_key(Section::Global, &store),
            row_key(Section::Project, &store)
        );
    }

    #[test]
    fn progress_is_in_whole_percent() {
        assert_eq!(percent(1, 4), "25%");
        assert_eq!(percent(0, 0), "0%");
        assert_eq!(percent(40, 40), "100%");
    }
}
