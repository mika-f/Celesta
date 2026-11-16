//! The plugin page's view: what Find plugins… shows in place of Assets,
//! the Assets area widened to hold two columns. The list column searches
//! the registry's `search.json`; the detail column shows the selected
//! plugin's fields and versions from its index, and under them, pinned,
//! the footer with what can be done with it and how that work is going.
//!
//! The view owns the search field and the selection; what the columns say
//! is [`super::model`]'s, and the work runs in the window's
//! [`RegistryState`], so the Plugins tab shows the same install.

use super::model::{
    Details, Footer, FooterButton, ListRow, PageCommand, details, footer, list_footer, list_rows,
    progress_label,
};
use crate::actions::{RunPluginCommand, SelectNextPlugin, SelectPreviousPlugin};
use crate::panel_header::PanelHeader;
use crate::plugins::FoundPlugins;
use crate::registry::{Fetch, RegistryState};
use crate::widgets::CompactLabel as _;
use celesta_editor_theme::metrics;
use celesta_registry_format::{AdvisoryList, Index};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    Hsla, KeyBinding, KeyDownEvent, SharedString, Subscription, Window, div, rems,
};
use time::OffsetDateTime;

/// The list column's width, in `rem`: wide enough for an id and its kind,
/// latest version, and state; the detail column takes the rest.
const LIST_WIDTH: f32 = 16.;

/// The width of the versions table's version lane, in `rem`.
const VERSION_LANE_WIDTH: f32 = 9.;

/// What the page tells its owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PluginPageEvent {
    /// The header's close button or Escape: back to editing.
    Close,
}

pub(crate) struct PluginPage {
    /// The page's; its key context holds the arrows and Enter.
    focus_handle: FocusHandle,
    /// The search field, which takes the focus when the page opens.
    query: Entity<InputState>,
    /// The plugin the detail column shows.
    selected: Option<String>,
    registry: Entity<RegistryState>,
    /// What the last discovery found, as the editor last told.
    found: FoundPlugins,
    /// The cached advisory list the offered version reads; `None` until
    /// read, and when there is none.
    advisories: Option<AdvisoryList>,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<PluginPageEvent> for PluginPage {}

/// The page's keys: Up and Down move the selection, from the search field
/// too, and Enter runs the primary command.
pub(crate) fn key_bindings() -> [KeyBinding; 5] {
    [
        KeyBinding::new("up", SelectPreviousPlugin, Some("PluginPage")),
        KeyBinding::new("down", SelectNextPlugin, Some("PluginPage")),
        // The one-line field has no use for the arrows.
        KeyBinding::new("up", SelectPreviousPlugin, Some("PluginPage > Input")),
        KeyBinding::new("down", SelectNextPlugin, Some("PluginPage > Input")),
        KeyBinding::new("enter", RunPluginCommand, Some("PluginPage")),
    ]
}

impl PluginPage {
    pub(crate) fn new(
        registry: Entity<RegistryState>,
        found: FoundPlugins,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search plugins"));
        let subscriptions = [
            cx.subscribe(&query, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            // Fetches and work in flight show as they come.
            cx.observe(&registry, |_, _, cx| cx.notify()),
        ];
        let mut page = Self {
            focus_handle: cx.focus_handle(),
            query,
            selected: None,
            registry,
            found,
            advisories: None,
            _subscriptions: subscriptions,
        };
        page.read_advisories(cx);
        page
    }

    /// Shows the page as Find plugins… does, with `select` (Show in Find
    /// plugins) selected: the search field takes the focus, and the list
    /// is fetched unless it is shown already.
    pub(crate) fn open(
        &mut self,
        select: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let search_shown = matches!(self.registry.read(cx).search, Fetch::Loaded { .. });
        if !search_shown {
            self.registry.update(cx, |state, cx| state.fetch_search(cx));
        }
        if let Some(id) = select {
            self.select(id, cx);
        }
        self.query.update(cx, |query, cx| query.focus(window, cx));
        cx.notify();
    }

    /// What discovery found after the work changed it (or another project
    /// opened); the advisories are read again with it.
    pub(crate) fn set_found(&mut self, found: FoundPlugins, cx: &mut Context<Self>) {
        self.found = found;
        self.read_advisories(cx);
        cx.notify();
    }

    /// Reads the cached advisory list, off the UI thread.
    fn read_advisories(&mut self, cx: &mut Context<Self>) {
        let client = self.registry.read(cx).client().clone();
        cx.spawn(async move |this, cx| {
            let advisories = cx
                .background_executor()
                .spawn(async move { client.registry().ok()?.cached_advisories().ok().flatten() })
                .await;
            this.update(cx, |page, cx| {
                page.advisories = advisories;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Shows `id` in the detail column, fetching its index unless it has
    /// been fetched (or failed: Try Again fetches it again).
    pub(crate) fn select(&mut self, id: String, cx: &mut Context<Self>) {
        let fetched = self
            .registry
            .read(cx)
            .indexes
            .get(&id)
            .is_some_and(|fetch| !matches!(fetch, Fetch::Idle));
        if !fetched {
            self.registry
                .update(cx, |state, cx| state.fetch_index(&id, cx));
        }
        self.selected = Some(id);
        cx.notify();
    }

    /// Selects the row `step` rows from the selected one, within the
    /// list; with none selected (or one the search hides), the first row
    /// going down and the last going up.
    fn move_selection(&mut self, step: isize, cx: &mut Context<Self>) {
        let rows = self.rows(cx);
        let at = self
            .selected
            .as_ref()
            .and_then(|id| rows.iter().position(|row| &row.id == id));
        let next = match at {
            Some(at) => at
                .saturating_add_signed(step)
                .min(rows.len().saturating_sub(1)),
            None if step > 0 => 0,
            None => rows.len().saturating_sub(1),
        };
        if let Some(row) = rows.get(next) {
            self.select(row.id.clone(), cx);
        }
    }

    /// Enter: the primary command, when it is enabled.
    fn run_primary(&mut self, cx: &mut Context<Self>) {
        if let Some(Footer { primary, .. }) = self.footer(cx)
            && primary.enabled
        {
            self.run(primary.command, cx);
        }
    }

    /// Starts `command` for the selected plugin in the window's registry
    /// work; the footer shows it in flight, and refuses what another job
    /// holds, so a refusal here has nothing more to say.
    fn run(&mut self, command: PageCommand, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let media = self.index(cx).is_some_and(|index| index.kind.is_media());
        // A media plugin's Update is the store's; another plugin's is the
        // lock's when this project locks it (as the footer decides).
        let locked = !media
            && self
                .found
                .lock
                .as_ref()
                .is_some_and(|lock| lock.plugins.contains_key(&id));
        let root = self.found.project.clone();
        self.registry.update(cx, |state, cx| {
            match (command, root) {
                (PageCommand::Install(version), _) => state.install_global(&id, version, cx),
                (PageCommand::Add(version), Some(root)) => state.add(root, &id, version, false, cx),
                (PageCommand::Pin(version), Some(root)) => state.add(root, &id, version, true, cx),
                (PageCommand::Update, Some(root)) if locked => {
                    state.update(root, [id.clone()].into(), Vec::new(), cx)
                }
                (PageCommand::Update, _) => state.update_global(&id, cx),
                (PageCommand::Add(_) | PageCommand::Pin(_) | PageCommand::None, _) => false,
            };
        });
    }

    /// The list column's rows for the search, once the list is fetched.
    pub(crate) fn rows(&self, cx: &App) -> Vec<ListRow> {
        let registry = self.registry.read(cx);
        match &registry.search {
            Fetch::Loaded { value, .. } => list_rows(
                value,
                &self.query.read(cx).value(),
                &self.found,
                registry.client().host(),
            ),
            _ => Vec::new(),
        }
    }

    /// The selected plugin's index, once fetched.
    fn index<'a>(&self, cx: &'a App) -> Option<&'a Index> {
        let id = self.selected.as_deref()?;
        match self.registry.read(cx).indexes.get(id)? {
            Fetch::Loaded { value, .. } => Some(value),
            _ => None,
        }
    }

    /// The selected plugin's fields and versions, once its index is
    /// fetched.
    pub(crate) fn details(&self, cx: &App) -> Option<Details> {
        let registry = self.registry.read(cx);
        Some(details(
            self.index(cx)?,
            &self.found,
            self.advisories.as_ref(),
            registry.client().host(),
            registry.include_prereleases,
        ))
    }

    /// What the selected plugin's footer offers, once its index is
    /// fetched.
    pub(crate) fn footer(&self, cx: &App) -> Option<Footer> {
        let registry = self.registry.read(cx);
        Some(footer(
            self.index(cx)?,
            &self.found,
            registry,
            self.advisories.as_ref(),
            registry.client().host(),
        ))
    }
}

/// The registry's host as the list's footer names it:
/// `registry.celesta.studio`.
fn registry_host(base: &str) -> &str {
    base.split_once("://")
        .map_or(base, |(_, rest)| rest)
        .trim_end_matches('/')
}

/// A line of text in `color`, at the panel's inset.
fn line(text: impl Into<SharedString>, color: Hsla) -> impl IntoElement {
    div()
        .px(metrics::panel_padding())
        .py_2()
        .text_sm()
        .text_color(color)
        .child(text.into())
}

/// An error under what it is about: the icon and the text in `color`,
/// never the color alone.
fn problem_line(text: impl Into<SharedString>, color: Hsla) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap_1()
        .text_sm()
        .text_color(color)
        .child(
            Icon::new(IconName::TriangleAlert)
                .xsmall()
                .text_color(color)
                .flex_none(),
        )
        .child(div().min_w_0().child(text.into()))
}

impl PluginPage {
    /// The list column: its header, the search field and Include
    /// pre-releases, the rows, and the line under them.
    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (status, shown, failed, include_prereleases) = {
            let registry = self.registry.read(cx);
            let host = registry_host(&registry.client().access().base);
            (
                list_footer(&registry.search, host, OffsetDateTime::now_utc()),
                matches!(registry.search, Fetch::Loaded { .. }),
                matches!(registry.search, Fetch::Failed(_)),
                registry.include_prereleases,
            )
        };
        let theme = cx.theme();
        let (muted, danger, border) = (theme.muted_foreground, theme.danger, theme.border);
        let rows = self.rows(cx);
        // Until the list is shown, the line that says why stands in its
        // place; once it is, the line under it says where it is from.
        let body: Vec<AnyElement> = if failed {
            vec![
                v_flex()
                    .items_start()
                    .gap_2()
                    .px(metrics::panel_padding())
                    .py_2()
                    .child(problem_line(status.clone(), danger))
                    .child(
                        Button::new("plugin-page-search-retry")
                            .xsmall()
                            .outline()
                            .compact_label("Try Again")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.registry.update(cx, |state, cx| state.fetch_search(cx));
                            })),
                    )
                    .into_any_element(),
            ]
        } else if !shown {
            vec![line(status.clone(), muted).into_any_element()]
        } else if rows.is_empty() {
            vec![line("No plugins match the search", muted).into_any_element()]
        } else {
            rows.into_iter()
                .map(|row| self.render_row(row, cx))
                .collect()
        };
        let footer_line = shown.then_some(status);
        v_flex()
            .flex_none()
            .w(rems(LIST_WIDTH))
            .h_full()
            .min_h_0()
            .border_r_1()
            .border_color(border)
            .child(PanelHeader::new("plugin-page-list", "Plugins"))
            .child(
                v_flex()
                    .flex_none()
                    .gap_2()
                    .p(metrics::panel_padding())
                    .border_b_1()
                    .border_color(border)
                    .child(Input::new(&self.query).small())
                    .child(
                        Checkbox::new("plugin-page-prereleases")
                            .small()
                            .label("Include pre-releases")
                            .checked(include_prereleases)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                let checked = *checked;
                                this.registry.update(cx, |state, cx| {
                                    state.include_prereleases = checked;
                                    cx.notify();
                                });
                            })),
                    ),
            )
            .child(
                div()
                    .id("plugin-page-rows")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(body),
            )
            .children(footer_line.map(|text| {
                div()
                    .flex_none()
                    .px(metrics::panel_padding())
                    .py_1()
                    .border_t_1()
                    .border_color(border)
                    .text_sm()
                    .text_color(muted)
                    .child(text)
            }))
    }

    /// One plugin: its id, then its kind, the registry's latest, and a
    /// short state.
    fn render_row(&self, row: ListRow, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (border, active, hover) = (theme.border, theme.list_active, theme.list_hover);
        let selected = self.selected.as_deref() == Some(row.id.as_str());
        let id: SharedString = row.id.clone().into();
        let selector = format!("plugin-page-row-{id}");
        let mut meta = h_flex()
            .gap_1()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(row.kind);
        if let Some(latest) = row.latest {
            meta = meta.child("·").child(
                div()
                    .font_family(theme.mono_font_family.clone())
                    .child(latest),
            );
        }
        if row.prerelease {
            meta = meta.child("·").child("Pre-release");
        }
        if let Some(note) = row.note {
            meta = meta.child("·").child(note);
        }
        v_flex()
            .id((ElementId::from("plugin-page-row"), id.clone()))
            .debug_selector(move || selector.clone())
            .gap_0p5()
            .px(metrics::panel_padding())
            .py_1p5()
            .border_b_1()
            .border_color(border)
            .when(selected, |row| row.bg(active))
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select(id.to_string(), cx);
            }))
            .child(div().text_base().child(row.id))
            .child(meta)
            .into_any_element()
    }

    /// The detail column: the selected plugin's header with Close, its
    /// fields and versions, and the footer pinned under them.
    fn render_detail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let close = Button::new("plugin-page-close")
            .ghost()
            .xsmall()
            .icon(IconName::Close)
            .tooltip("Back to editing")
            .accessibility_label("Back to editing")
            .debug_selector(|| "plugin-page-close".to_owned())
            .on_click(cx.listener(|_, _, _, cx| cx.emit(PluginPageEvent::Close)));
        let title = self.selected.clone().unwrap_or_default();
        let muted = cx.theme().muted_foreground;
        let fetch = self
            .selected
            .as_ref()
            .and_then(|id| self.registry.read(cx).indexes.get(id).cloned());
        let (body, footer): (AnyElement, Option<AnyElement>) = match (&self.selected, fetch) {
            (None, _) => (
                line("Select a plugin to see its versions", muted).into_any_element(),
                None,
            ),
            (Some(_), None | Some(Fetch::Idle | Fetch::Loading)) => {
                (line("Loading versions…", muted).into_any_element(), None)
            }
            (Some(id), Some(Fetch::Failed(error))) => {
                let id = id.clone();
                (
                    v_flex()
                        .items_start()
                        .gap_2()
                        .px(metrics::panel_padding())
                        .py_2()
                        .child(problem_line(error, cx.theme().danger))
                        .child(
                            Button::new((
                                ElementId::from("plugin-page-index-retry"),
                                SharedString::from(id.clone()),
                            ))
                            .xsmall()
                            .outline()
                            .compact_label("Try Again")
                            .debug_selector(|| "plugin-page-index-retry".to_owned())
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.registry
                                        .update(cx, |state, cx| state.fetch_index(&id, cx));
                                },
                            )),
                        )
                        .into_any_element(),
                    None,
                )
            }
            (Some(_), Some(Fetch::Loaded { .. })) => (
                self.details(cx)
                    .map(|details| self.render_details(details, cx).into_any_element())
                    .unwrap_or_else(|| div().into_any_element()),
                self.footer(cx)
                    .map(|footer| self.render_footer(footer, cx).into_any_element()),
            ),
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .min_h_0()
            .child(PanelHeader::new("plugin-page-detail", title).action(close))
            .child(
                div()
                    .id("plugin-page-details")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
            .children(footer)
    }

    /// The fields, then the versions table under its band.
    fn render_details(&self, details: Details, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (muted, mono, border) = (
            theme.muted_foreground,
            theme.mono_font_family.clone(),
            theme.border,
        );
        let text = |lines: Vec<String>, empty: &str, monospace: bool| -> AnyElement {
            let lines = if lines.is_empty() {
                vec![empty.to_owned()]
            } else {
                lines
            };
            v_flex()
                .min_w_0()
                .when(monospace, |column| column.font_family(mono.clone()))
                .children(lines)
                .into_any_element()
        };
        let fields = [
            ("Kind", text(vec![details.kind], "", false)),
            ("Latest", text(vec![details.latest], "", true)),
            ("Builds", text(details.builds, "None", true)),
            (
                "Source",
                text(details.source.into_iter().collect(), "Not recorded", true),
            ),
            ("Here", text(details.here, "Not on this computer", false)),
            ("Advisories", text(details.advisories, "None", false)),
        ];
        let versions: Vec<AnyElement> = if details.versions.is_empty() {
            vec![line("No versions to show", muted).into_any_element()]
        } else {
            details
                .versions
                .into_iter()
                .map(|version| {
                    h_flex()
                        .min_h(metrics::list_row_height())
                        .px(metrics::panel_padding())
                        .gap_2()
                        .border_b_1()
                        .border_color(border)
                        .text_sm()
                        .child(
                            div()
                                .flex_none()
                                .w(rems(VERSION_LANE_WIDTH))
                                .font_family(mono.clone())
                                .child(version.version),
                        )
                        .child(div().flex_1().text_color(muted).child(version.state))
                        .children(version.size.map(|size| {
                            div()
                                .flex_none()
                                .font_family(mono.clone())
                                .text_color(muted)
                                .child(size)
                        }))
                        .into_any_element()
                })
                .collect()
        };
        v_flex()
            .w_full()
            .child(
                v_flex()
                    .px(metrics::panel_padding())
                    .py_2()
                    .children(fields.into_iter().map(|(label, value)| {
                        h_flex()
                            .items_start()
                            .min_h(metrics::list_row_height())
                            .text_sm()
                            .child(
                                div()
                                    .flex_none()
                                    .w(metrics::inspector_label_width())
                                    .pr_2()
                                    .text_color(muted)
                                    .child(label),
                            )
                            .child(div().flex_1().min_w_0().child(value))
                    })),
            )
            .child(
                h_flex()
                    .flex_none()
                    .w_full()
                    .h(metrics::list_row_height())
                    .px(metrics::panel_padding())
                    .bg(theme.title_bar)
                    .border_t_1()
                    .border_b_1()
                    .border_color(border)
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child("Versions"),
            )
            .children(versions)
    }

    /// The footer: the primary and secondary buttons with the download's
    /// progress beside them and its bar under them, then why the primary
    /// one is disabled (muted) or what the last attempt said (danger).
    fn render_footer(&self, footer: Footer, cx: &mut Context<Self>) -> impl IntoElement {
        let id: SharedString = self.selected.clone().unwrap_or_default().into();
        let theme = cx.theme();
        let (muted, danger, mono) = (
            theme.muted_foreground,
            theme.danger,
            theme.mono_font_family.clone(),
        );
        let button =
            |name: &'static str, button: FooterButton, primary: bool, cx: &mut Context<Self>| {
                let FooterButton {
                    label,
                    enabled,
                    command,
                } = button;
                let built = Button::new((ElementId::from(name), id.clone()))
                    .small()
                    .label(label)
                    .disabled(!enabled)
                    .debug_selector(move || name.to_owned())
                    .on_click(cx.listener(move |this, _, _, cx| this.run(command.clone(), cx)));
                if primary {
                    built.primary()
                } else {
                    built.outline()
                }
            };
        let primary = button("plugin-page-primary", footer.primary, true, cx);
        let secondary = footer
            .secondary
            .map(|secondary| button("plugin-page-secondary", secondary, false, cx));
        let progress = footer.progress;
        v_flex()
            .flex_none()
            .w_full()
            .gap_2()
            .p(metrics::panel_padding())
            .bg(cx.theme().title_bar)
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(primary)
                    .children(secondary)
                    .children(
                        progress
                            .filter(|(_, total)| *total > 0)
                            .map(|(done, total)| {
                                div()
                                    .font_family(mono.clone())
                                    .text_sm()
                                    .text_color(muted)
                                    .child(progress_label(done, total))
                            }),
                    ),
            )
            .children(progress.map(|(done, total)| {
                let bar =
                    Progress::new((ElementId::from("plugin-page-progress"), id.clone())).xsmall();
                if total > 0 {
                    bar.value(done as f32 / total as f32 * 100.0)
                } else {
                    bar.loading(true)
                }
            }))
            .children(
                footer
                    .reason
                    .map(|reason| div().text_sm().text_color(muted).child(reason)),
            )
            .children(footer.failure.map(|failure| problem_line(failure, danger)))
    }
}

impl Focusable for PluginPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PluginPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("plugin-page")
            .key_context("PluginPage")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &SelectPreviousPlugin, _, cx| {
                this.move_selection(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectNextPlugin, _, cx| {
                this.move_selection(1, cx);
            }))
            .on_action(cx.listener(|this, _: &RunPluginCommand, _, cx| {
                this.run_primary(cx);
            }))
            // Escape closes the page, from the search field too (whose own
            // Escape lets it through).
            .capture_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.emit(PluginPageEvent::Close);
                    cx.stop_propagation();
                }
            }))
            .size_full()
            .min_h_0()
            .items_start()
            .bg(cx.theme().sidebar)
            .child(self.render_list(cx))
            .child(self.render_detail(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_control::{ExportPageSlot, LeftPanel, left_panel, show_export, show_plugins};
    use crate::export_page::view::ExportPage;
    use crate::helpers::ExportLoudness;
    use crate::plugin_page::PluginPageSlot;
    use crate::registry::{EditorRegistry, RegistryEvent};
    use crate::scratch::scratch_dir;
    use celesta_media::MediaRegistry;
    use celesta_plugin_host::UserSettings;
    use celesta_registry_client::testing::TestRegistry;
    use celesta_registry_format::Target;
    use gpui_kit::component::Root;
    use gpui_kit::{
        Modifiers, Subscription, TestAppContext, VisualTestContext, WindowHandle, px, size,
    };
    use serde_json::json;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::Arc;

    /// physical: a test window's size, not a layout length.
    const WINDOW_WIDTH: f32 = 1000.;
    /// physical: a test window's size, not a layout length.
    const WINDOW_HEIGHT: f32 = 800.;

    /// A test registry serving `acme.codec` (a media plugin with a build
    /// for this computer) and `acme.charts` (one build for any), a user
    /// folder of its own, and a page over them in a window, its keys bound
    /// as the editor binds them.
    struct Fixture {
        served: TestRegistry,
        user: PathBuf,
        handle: WindowHandle<Root>,
        page: Entity<PluginPage>,
        registry: Entity<RegistryState>,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.user);
        }
    }

    fn search_entry(id: &str, role: &str) -> serde_json::Value {
        json!({
            "id": id,
            "publisher": "acme",
            "kind": { "roles": [role], "program": false, "module": true },
            "description": format!("The {role} test plugin."),
            "latest": "1.0.0",
        })
    }

    fn fixture(cx: &mut TestAppContext) -> Fixture {
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
            cx.bind_keys(key_bindings());
        });
        let served = TestRegistry::file();
        served.publish_with_roles(
            "acme.codec",
            "1.0.0",
            &[Target::host().as_str()],
            &["media"],
        );
        served.publish("acme.charts", "1.0.0", &["any"]);
        served.search(json!([
            search_entry("acme.charts", "content"),
            search_entry("acme.codec", "media"),
        ]));
        let user = scratch_dir("plugin-page");
        let client = Arc::new(EditorRegistry::for_test(&served, &user));
        let registry = cx.new(|_| RegistryState::new(client));
        let mut page = None;
        let handle = cx.open_window(size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), |window, cx| {
            let view =
                cx.new(|cx| PluginPage::new(registry.clone(), FoundPlugins::default(), window, cx));
            page = Some(view.clone());
            Root::new(view, window, cx)
        });
        cx.run_until_parked();
        Fixture {
            served,
            user,
            handle,
            page: page.unwrap(),
            registry,
        }
    }

    impl Fixture {
        fn window(&self, cx: &mut TestAppContext) -> VisualTestContext {
            VisualTestContext::from_window(self.handle.into(), cx)
        }

        /// Opens the page as Find plugins… (or Show in Find plugins on
        /// `select`) does.
        fn open(&self, cx: &mut VisualTestContext, select: Option<&str>) {
            let page = self.page.clone();
            cx.update(|window, cx| {
                page.update(cx, |page, cx| {
                    page.open(select.map(str::to_owned), window, cx)
                });
            });
            cx.run_until_parked();
        }

        fn ids(&self, cx: &mut VisualTestContext) -> Vec<String> {
            self.page
                .read_with(cx, |page, cx| page.rows(cx))
                .into_iter()
                .map(|row| row.id)
                .collect()
        }

        fn footer(&self, cx: &mut VisualTestContext) -> Option<Footer> {
            self.page.read_with(cx, |page, cx| page.footer(cx))
        }

        fn click(&self, cx: &mut VisualTestContext, selector: &'static str) {
            let bounds = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} is drawn"));
            cx.simulate_click(bounds.center(), Modifiers::none());
            cx.run_until_parked();
        }

        /// What the page asks of its owner.
        fn events(&self, cx: &mut VisualTestContext) -> Rc<RefCell<Vec<PluginPageEvent>>> {
            let events = Rc::new(RefCell::new(Vec::new()));
            cx.update(|_, cx| {
                let events = events.clone();
                cx.subscribe(&self.page, move |_, event: &PluginPageEvent, _| {
                    events.borrow_mut().push(event.clone());
                })
                .detach();
            });
            events
        }
    }

    #[gpui_kit::test]
    fn opening_fetches_and_lists_the_registry(cx: &mut TestAppContext) {
        let fixture = fixture(cx);
        let cx = &mut fixture.window(cx);
        assert!(fixture.ids(cx).is_empty(), "nothing is fetched before");
        fixture.open(cx, None);
        assert_eq!(fixture.ids(cx), ["acme.charts", "acme.codec"]);
        let rows = fixture.page.read_with(cx, |page, cx| page.rows(cx));
        assert_eq!(rows[1].kind, "Media");
        assert_eq!(rows[1].latest.as_deref(), Some("1.0.0"));
        // The search field has the focus, so typing searches at once.
        let query = fixture.page.read_with(cx, |page, _| page.query.clone());
        cx.update(|window, cx| assert!(query.focus_handle(cx).is_focused(window)));
        assert!(cx.debug_bounds("plugin-page-row-acme.codec").is_some());
    }

    #[gpui_kit::test]
    fn typing_filters_the_list(cx: &mut TestAppContext) {
        let fixture = fixture(cx);
        let cx = &mut fixture.window(cx);
        fixture.open(cx, None);
        cx.simulate_input("MEDIA");
        cx.run_until_parked();
        // By description, ignoring case.
        assert_eq!(fixture.ids(cx), ["acme.codec"]);
        assert!(cx.debug_bounds("plugin-page-row-acme.charts").is_none());
    }

    #[gpui_kit::test]
    fn selecting_shows_details_and_the_footer(cx: &mut TestAppContext) {
        let fixture = fixture(cx);
        let cx = &mut fixture.window(cx);
        fixture.open(cx, None);
        assert!(fixture.footer(cx).is_none(), "nothing is selected");
        fixture.click(cx, "plugin-page-row-acme.codec");
        assert_eq!(
            fixture.page.read_with(cx, |page, _| page.selected.clone()),
            Some("acme.codec".to_owned())
        );
        let details = fixture
            .page
            .read_with(cx, |page, cx| page.details(cx))
            .expect("the index is fetched on selection");
        assert_eq!(details.kind, "Media plugin by acme");
        assert_eq!(details.versions[0].version, "1.0.0");
        let footer = fixture.footer(cx).unwrap();
        assert_eq!(footer.primary.label, "Install");
        assert!(footer.primary.enabled);
        // Outside a project there is nothing to pin to.
        assert_eq!(footer.secondary, None);
        assert!(cx.debug_bounds("plugin-page-primary").is_some());

        // Up from the search field selects the row above.
        cx.simulate_keystrokes("up");
        cx.run_until_parked();
        assert_eq!(
            fixture.page.read_with(cx, |page, _| page.selected.clone()),
            Some("acme.charts".to_owned())
        );
        let footer = fixture.footer(cx).unwrap();
        assert_eq!(footer.primary.label, "Add to this project");
        assert!(!footer.primary.enabled);
        assert_eq!(
            footer.reason.as_deref(),
            Some("Add plugins to a project with celesta.config.json")
        );
    }

    #[gpui_kit::test]
    fn install_runs_and_the_button_becomes_installed(cx: &mut TestAppContext) {
        let fixture = fixture(cx);
        let cx = &mut fixture.window(cx);
        // The editor finds its plugins again when the work changed them; so
        // does this test, from the settings the install wrote.
        let settings = fixture.user.join("settings.json");
        let page = fixture.page.downgrade();
        cx.update(|_, cx| {
            cx.subscribe(&fixture.registry, move |_, event: &RegistryEvent, cx| {
                if let RegistryEvent::PluginsChanged { .. } = event {
                    let found = FoundPlugins {
                        registry_plugins: UserSettings::load(&settings).unwrap().registry_plugins,
                        ..FoundPlugins::default()
                    };
                    page.update(cx, |page, cx| page.set_found(found, cx))
                        .unwrap();
                }
            })
            .detach();
        });
        fixture.open(cx, Some("acme.codec"));
        assert_eq!(fixture.footer(cx).unwrap().primary.label, "Install");
        fixture.click(cx, "plugin-page-primary");
        let footer = fixture.footer(cx).unwrap();
        assert_eq!(footer.primary.label, "Installed");
        assert!(!footer.primary.enabled);
        assert_eq!(footer.failure, None);
    }

    #[gpui_kit::test]
    fn enter_runs_the_primary_command(cx: &mut TestAppContext) {
        let fixture = fixture(cx);
        let cx = &mut fixture.window(cx);
        fixture.open(cx, Some("acme.codec"));
        cx.simulate_keystrokes("enter");
        // The install started (and, with nothing to rediscover, ended).
        let settings = UserSettings::load(&fixture.user.join("settings.json")).unwrap();
        assert_eq!(settings.registry_plugins[0].id, "acme.codec");
    }

    #[gpui_kit::test]
    fn a_plugin_without_an_index_says_why_and_tries_again(cx: &mut TestAppContext) {
        let fixture = fixture(cx);
        fixture
            .served
            .search(json!([search_entry("acme.gone", "content")]));
        let cx = &mut fixture.window(cx);
        fixture.open(cx, Some("acme.gone"));
        assert!(fixture.footer(cx).is_none());
        assert!(cx.debug_bounds("plugin-page-index-retry").is_some());
        fixture.served.publish("acme.gone", "1.0.0", &["any"]);
        fixture.click(cx, "plugin-page-index-retry");
        assert!(
            fixture
                .page
                .read_with(cx, |page, cx| page.details(cx))
                .is_some()
        );
    }

    #[gpui_kit::test]
    fn escape_closes_the_page(cx: &mut TestAppContext) {
        let fixture = fixture(cx);
        let cx = &mut fixture.window(cx);
        let events = fixture.events(cx);
        fixture.open(cx, None);
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(*events.borrow(), [PluginPageEvent::Close]);
    }

    #[gpui_kit::test]
    fn opening_the_export_page_closes_the_plugin_page(cx: &mut TestAppContext) {
        let fixture = fixture(cx);
        let mut export_page = None;
        cx.open_window(size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), |window, cx| {
            let view = cx.new(|cx| {
                ExportPage::new(
                    Arc::new(MediaRegistry::new(Vec::new())),
                    None,
                    None,
                    ExportLoudness::Off,
                    window,
                    cx,
                )
            });
            export_page = Some(view.clone());
            Root::new(view, window, cx)
        });
        cx.run_until_parked();
        let mut export = ExportPageSlot::default();
        export.set(export_page.unwrap(), Subscription::new(|| {}));
        let mut plugins = PluginPageSlot::default();
        plugins.set(fixture.page.clone(), Subscription::new(|| {}));

        show_plugins(&mut plugins, &mut export);
        assert!(plugins.is_open() && !export.is_open());
        assert_eq!(
            left_panel(false, export.is_open(), plugins.is_open()),
            LeftPanel::Plugins
        );
        show_export(&mut export, &mut plugins);
        assert!(export.is_open() && !plugins.is_open());
        // Both pages keep their state.
        assert_eq!(plugins.page(), Some(&fixture.page));
        show_plugins(&mut plugins, &mut export);
        assert!(plugins.is_open() && !export.is_open());
        // A React preview hides Assets, not the page.
        assert_eq!(left_panel(true, false, true), LeftPanel::Plugins);
    }
}
