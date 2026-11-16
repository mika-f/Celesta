//! The plugin page (Find plugins): what its list, its details, and its
//! footer say, from the registry's files and what this computer holds.

mod model;
pub(crate) mod view;

use crate::actions::FindPlugins;
use crate::export_control::show_plugins;
use crate::inline_text_view::EditEvent;
use crate::view::EditorView;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{App, AppContext as _, Context, Entity, Subscription, Window};
pub(crate) use model::format_size;
pub(crate) use view::registry_host;
use view::{PluginPage, PluginPageEvent};

/// The plugin page, kept with its search and selection for the window's
/// life, the subscription to what it asks for, and whether it shows.
#[derive(Default)]
pub(crate) struct PluginPageSlot {
    page: Option<(Entity<PluginPage>, Subscription)>,
    open: bool,
}

impl PluginPageSlot {
    /// Whether the page shows in place of Assets.
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// The kept page, shown or not.
    pub(crate) fn page(&self) -> Option<&Entity<PluginPage>> {
        self.page.as_ref().map(|(page, _)| page)
    }

    /// Keeps `page` and the subscription to its `events`; whether it shows
    /// does not change.
    pub(crate) fn set(&mut self, page: Entity<PluginPage>, events: Subscription) {
        self.page = Some((page, events));
    }

    /// Shows the kept page; without one nothing shows.
    pub(crate) fn show(&mut self) {
        self.open = self.page.is_some();
    }

    /// Back to editing, keeping the page's state.
    pub(crate) fn hide(&mut self) {
        self.open = false;
    }

    /// Runs the kept page's clock while it shows, and stops it when not;
    /// after every [`Self::show`] and [`Self::hide`].
    pub(crate) fn sync_clock(&self, cx: &mut App) {
        if let Some(page) = self.page() {
            let shown = self.open;
            page.update(cx, |page, cx| page.set_clock(shown, cx));
        }
    }
}

impl EditorView {
    /// File → Find Plugins…: opens the page, leaving it as it is when it
    /// is open.
    pub(crate) fn find_plugins_action(
        &mut self,
        _: &FindPlugins,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.end_inline_text(EditEvent::ProjectCommand, cx);
        self.open_plugin_page(None, window, cx);
    }

    /// The Plugins tab's Find plugins…: the page's toggle.
    pub(crate) fn toggle_plugin_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.plugin_page.is_open() {
            self.close_plugin_page(window, cx);
        } else {
            self.open_plugin_page(None, window, cx);
        }
    }

    /// Shows the plugin page (as it was left) in place of Assets, closing
    /// the export page, with `select` selected (Show in Find plugins); the
    /// search field takes the focus. Without the window's registry (no
    /// user folder to install plugins in) there is no page.
    pub(crate) fn open_plugin_page(
        &mut self,
        select: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(registry) = self.registry.clone() else {
            return;
        };
        if window.has_active_dialog(cx) {
            return;
        }
        let page = match self.plugin_page.page() {
            Some(page) => page.clone(),
            None => {
                let found = self.found_plugins.clone();
                let page = cx.new(|cx| PluginPage::new(registry, found, window, cx));
                let events = cx.subscribe_in(&page, window, Self::plugin_page_event);
                self.plugin_page.set(page.clone(), events);
                page
            }
        };
        page.update(cx, |page, cx| page.open(select, window, cx));
        show_plugins(&mut self.plugin_page, &mut self.export_page);
        self.plugin_page.sync_clock(cx);
        cx.notify();
    }

    /// Back to editing: Assets shows again and the editor takes the
    /// focus; the page keeps its search and selection, and its work goes
    /// on.
    pub(crate) fn close_plugin_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.plugin_page.hide();
        self.plugin_page.sync_clock(cx);
        self.focus_editor(window, cx);
        cx.notify();
    }

    /// Tells the kept page what discovery found, after an open or a
    /// reload replaced this view's plugins.
    pub(crate) fn sync_plugin_page(&self, cx: &mut Context<Self>) {
        if let Some(page) = self.plugin_page.page() {
            let found = self.found_plugins.clone();
            page.update(cx, |page, cx| page.set_found(found, cx));
        }
    }

    fn plugin_page_event(
        &mut self,
        _: &Entity<PluginPage>,
        event: &PluginPageEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PluginPageEvent::Close => self.close_plugin_page(window, cx),
        }
    }
}
