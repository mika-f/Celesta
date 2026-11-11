use crate::actions::{ClearExportRange, ExportProject, SetExportIn, SetExportOut};
use crate::export_page::view::{ExportPage, ExportPageEvent};
use crate::export_worker::{ExportRequest, ExportSource};
use crate::helpers::{ExportLoudness, export_output, export_range_for, export_suggested_name};
use crate::inline_text_view::EditEvent;
use crate::problems::{Problems, Source, error_chain};
use crate::timecode::format_timecode;
use crate::view::EditorView;
use celesta_exporter::{
    ExportCancellation, ExportProgress, OutputChoice, PRESETS_FILE, PresetOutput, update_presets,
    user_presets_path,
};
use celesta_media::{ComponentEntry, ComponentKind, MediaRegistry};
use celesta_plugin_host::{create_private_dir_all, project_root};
use celesta_react_bridge::runtime_paths as react_runtime_paths;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{
    AppContext as _, ClickEvent, Context, Entity, Focusable as _, SharedString, Subscription,
    Window,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The export page, kept with its state between openings until the
/// media plugins change (Reload Plugins, another project), the
/// subscription to what it asks for, and whether it shows.
#[derive(Default)]
pub(crate) struct ExportPageSlot {
    page: Option<(Entity<ExportPage>, Subscription)>,
    open: bool,
}

impl ExportPageSlot {
    /// Whether the page shows in place of Assets.
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// The kept page, shown or not.
    pub(crate) fn page(&self) -> Option<&Entity<ExportPage>> {
        self.page.as_ref().map(|(page, _)| page)
    }

    /// Keeps `page` and the subscription to its `events`, replacing the
    /// page kept before; whether it shows does not change.
    pub(crate) fn set(&mut self, page: Entity<ExportPage>, events: Subscription) {
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

    /// An export chosen on the page started: back to editing, keeping the
    /// page for the next one.
    pub(crate) fn export_started(&mut self) {
        self.hide();
    }

    /// Drops the page and its state; an open page closes with it.
    pub(crate) fn clear(&mut self) {
        self.page = None;
        self.open = false;
    }
}

/// What the left panel of the dock row shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeftPanel {
    /// Nothing: a React preview has no assets.
    Hidden,
    Assets,
    /// The export page's panel.
    Export,
}

/// The left panel in a React preview (`react`) or not, with the export
/// page open or not: the page shows wherever it is open.
pub(crate) fn left_panel(react: bool, export_open: bool) -> LeftPanel {
    match (react, export_open) {
        (_, true) => LeftPanel::Export,
        (true, false) => LeftPanel::Hidden,
        (false, false) => LeftPanel::Assets,
    }
}

impl EditorView {
    /// File → Export… and ⌘⇧E: opens the page, leaving it open when it is.
    pub(crate) fn export_project_action(
        &mut self,
        _: &ExportProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.end_inline_text(EditEvent::ProjectCommand, cx);
        self.open_export_page(window, cx);
    }

    /// The title bar's Export: the page's toggle.
    pub(crate) fn export_page_click(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_export_page(window, cx);
    }

    /// Opens the page when it is closed and closes it when it is open.
    pub(crate) fn toggle_export_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_page.is_open() {
            self.close_export_page(window, cx);
        } else {
            self.open_export_page(window, cx);
        }
    }

    /// Shows the export page (as it was left, within a session) in place
    /// of Assets and gives it the focus; its `Export…` asks where to write
    /// the file. Playback goes on: the page is not modal.
    pub(crate) fn open_export_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.choosing_export_path
            || self.export_cancellation.is_some()
            || self.opening
            || window.has_active_dialog(cx)
        {
            return;
        }
        // Already open it stays as it is, the focus where it was.
        if self.export_page.is_open() {
            return;
        }
        self.problems.clear(Source::Export);
        let project_file = self
            .export_source_path()
            .map(|source| project_root(source).join(PRESETS_FILE));
        // A page is kept for the project file it read; another file reads
        // its own. (A document's path never changes in place: opening
        // another project replaces the view, and the page with it.)
        let kept = self
            .export_page
            .page()
            .filter(|page| page.read(cx).project_file() == project_file.as_deref());
        let page = match kept {
            Some(page) => {
                let page = page.clone();
                page.update(cx, |page, cx| page.reopen(window, cx));
                page
            }
            None => {
                // Without a registry yet no media plugin can export: the
                // page lists none and says why Export… cannot run.
                let registry = self
                    .found_plugins
                    .registry
                    .clone()
                    .unwrap_or_else(|| Arc::new(MediaRegistry::new(Vec::new())));
                let loudness = self.export_loudness;
                let page = cx.new(|cx| {
                    ExportPage::new(
                        registry,
                        project_file,
                        user_presets_path(),
                        loudness,
                        window,
                        cx,
                    )
                });
                let events = cx.subscribe_in(&page, window, Self::export_page_event);
                self.export_page.set(page.clone(), events);
                page
            }
        };
        self.sync_export_range(cx);
        self.export_page.show();
        page.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    /// Back to editing: Assets shows again and the editor takes the
    /// focus; the page keeps its state.
    pub(crate) fn close_export_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.export_page.hide();
        self.focus_editor(window, cx);
        cx.notify();
    }

    /// Gives the editor the focus, as when the window opened.
    pub(crate) fn focus_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(focus_handle) = &self.focus_handle {
            focus_handle.focus(window, cx);
        }
    }

    /// Shows the in- and out-points on the kept page.
    fn sync_export_range(&self, cx: &mut Context<Self>) {
        let Some(page) = self.export_page.page() else {
            return;
        };
        let range = self.export_range_label();
        let settable = !self.is_react_preview();
        page.update(cx, |page, cx| page.set_range(range, settable, cx));
    }

    /// The document or React entry an export renders; `None` for an
    /// untitled document, which has no project presets file.
    fn export_source_path(&self) -> Option<&Path> {
        match &self.react_preview {
            Some(react) => Some(react.entry.as_path()),
            None => self.document.path(),
        }
    }

    /// The in- and out-points as the page shows them, when they mark
    /// what is exported; a React entry always exports all of it.
    fn export_range_label(&self) -> Option<SharedString> {
        if self.react_preview.is_some() {
            return None;
        }
        let (start, end) = (self.export_in_frame?, self.export_out_frame?);
        export_range_for(Some(start), Some(end), self.frame_rate_value)?;
        let rate = self.frame_rate_value;
        Some(
            format!(
                "{} – {}",
                format_timecode(start, rate),
                format_timecode(end, rate)
            )
            .into(),
        )
    }

    fn export_page_event(
        &mut self,
        _: &Entity<ExportPage>,
        event: &ExportPageEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ExportPageEvent::Export {
                choice,
                extension,
                last,
            } => {
                // A second Export… (a double click, another Enter) while the
                // save panel is up, or once an export runs, would start a
                // second export that Cancel export could not reach.
                if self.choosing_export_path || self.export_cancellation.is_some() {
                    return;
                }
                remember_last(&mut self.problems, user_presets_path().as_deref(), last);
                self.choose_export_path(choice.clone(), extension, window, cx);
            }
            ExportPageEvent::Loudness(loudness) => self.set_export_loudness(*loudness, cx),
            ExportPageEvent::ClearRange => self.clear_export_range(cx),
            ExportPageEvent::Close => self.close_export_page(window, cx),
        }
    }

    /// Asks where to write the export, in the document's folder, named
    /// after it with `extension`, and starts it there with `choice`.
    fn choose_export_path(
        &mut self,
        choice: OutputChoice,
        extension: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_choosing_export_path(true, cx);
        let directory = self.document.path().and_then(Path::parent).map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            Path::to_path_buf,
        );
        let suggested_name =
            export_suggested_name(self.document.path(), &self.project_name, extension);
        let extensions = self.muxer_extensions(&choice);
        let selection = cx.prompt_for_new_path(&directory, Some(&suggested_name));
        cx.spawn_in(window, async move |view, cx| {
            let selected_path = match selection.await {
                Ok(Ok(path)) => path,
                Ok(Err(error)) => {
                    view.update_in(cx, |this, _, cx| {
                        this.set_choosing_export_path(false, cx);
                        this.problems.report(
                            Source::Export,
                            "Couldn’t show the save panel",
                            error_chain(error.as_ref()),
                        );
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    view.update_in(cx, |this, _, cx| {
                        this.set_choosing_export_path(false, cx);
                        this.problems.report(
                            Source::Export,
                            "The save panel was interrupted",
                            error_chain(&error),
                        );
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            view.update_in(cx, |this, window, cx| {
                this.set_choosing_export_path(false, cx);
                // A cancelled save panel leaves the page as it was.
                let Some(selected_path) = selected_path else {
                    cx.notify();
                    return;
                };
                // The export starts: back to editing, its progress in the
                // status bar.
                this.export_page.export_started();
                this.focus_editor(window, cx);
                this.start_export(export_output(selected_path, &extensions), choice);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Marks the save panel `Export…` opened as up or closed, and tells
    /// the page, whose `Export…` waits for it.
    fn set_choosing_export_path(&mut self, choosing: bool, cx: &mut Context<Self>) {
        self.choosing_export_path = choosing;
        if let Some(page) = self.export_page.page() {
            page.update(cx, |page, cx| page.set_waiting(choosing, cx));
        }
    }

    /// The extensions `choice`'s muxer writes, the first one first.
    fn muxer_extensions(&self, choice: &OutputChoice) -> Vec<String> {
        let entry = choice.muxer.as_ref().and_then(|muxer| {
            self.found_plugins
                .registry
                .as_ref()?
                .component(muxer, ComponentKind::Muxer)
        });
        match entry.map(|info| info.entry) {
            Some(ComponentEntry::Muxer(muxer)) => muxer.extensions,
            _ => Vec::new(),
        }
    }

    pub(crate) fn start_export(&mut self, output: PathBuf, output_choice: OutputChoice) {
        let cancellation = ExportCancellation::default();
        let (source, range) = match &self.react_preview {
            Some(react) => {
                let (node, cli_script) = react_runtime_paths();
                (
                    ExportSource::ReactEntry {
                        entry: react.entry.clone(),
                        node,
                        cli_script,
                        properties: react.properties.clone(),
                        composition_id: react.composition_id.clone(),
                    },
                    None,
                )
            }
            None => (
                ExportSource::Project(Box::new(self.document.project().clone())),
                export_range_for(
                    self.export_in_frame,
                    self.export_out_frame,
                    self.frame_rate_value,
                ),
            ),
        };
        let request = ExportRequest {
            source,
            asset_root: self.document.asset_root().to_owned(),
            output: output.clone(),
            range,
            loudness: self.export_loudness.target(),
            output_choice,
            cancellation: cancellation.clone(),
            driver: self.driver,
            plugins: self.plugins.clone(),
        };
        self.export_path = Some(output);
        self.export_progress = Some(ExportProgress::Rendering { frame: 0, total: 0 });
        self.export_cancellation = Some(cancellation);
        self.export_cancelling = false;
        self.export_notices.clear();
        self.problems.clear(Source::Export);
        if let Err(error) = self.export_worker.request(request) {
            self.export_path = None;
            self.export_progress = None;
            self.export_cancellation = None;
            self.problems.fail(Source::Export, error);
        }
    }

    pub(crate) fn set_export_loudness(&mut self, loudness: ExportLoudness, cx: &mut Context<Self>) {
        self.export_loudness = loudness;
        cx.notify();
    }

    /// Marks the current playhead frame as the export in-point, dropping a
    /// stale out-point that would now sit at or before it.
    pub(crate) fn set_export_in(&mut self, cx: &mut Context<Self>) {
        let frame = self.clock.frame();
        self.export_in_frame = Some(frame);
        if self.export_out_frame.is_some_and(|out| out <= frame) {
            self.export_out_frame = None;
        }
        self.problems.clear(Source::Export);
        self.sync_export_range(cx);
        cx.notify();
    }

    /// Marks the frame just after the playhead as the export out-point (so the
    /// current frame is included), dropping a stale in-point.
    pub(crate) fn set_export_out(&mut self, cx: &mut Context<Self>) {
        let frame = self.clock.frame().saturating_add(1);
        self.export_out_frame = Some(frame);
        if self.export_in_frame.is_some_and(|start| start >= frame) {
            self.export_in_frame = None;
        }
        self.problems.clear(Source::Export);
        self.sync_export_range(cx);
        cx.notify();
    }

    pub(crate) fn clear_export_range(&mut self, cx: &mut Context<Self>) {
        self.export_in_frame = None;
        self.export_out_frame = None;
        self.sync_export_range(cx);
        cx.notify();
    }

    pub(crate) fn set_export_in_action(
        &mut self,
        _: &SetExportIn,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_export_in(cx);
    }

    pub(crate) fn set_export_out_action(
        &mut self,
        _: &SetExportOut,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_export_out(cx);
    }

    pub(crate) fn clear_export_range_action(
        &mut self,
        _: &ClearExportRange,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_export_range(cx);
    }

    pub(crate) fn cancel_export_click(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(cancellation) = &self.export_cancellation {
            cancellation.cancel();
            self.export_cancelling = true;
            cx.notify();
        }
    }
}

/// Saves the page's choice as `last` in the user presets file at `path`
/// (none on a platform without a settings folder), creating its folder
/// readable by the user alone. A failure is reported under
/// [`Source::ExportPresets`], which the export that goes on does not
/// clear; a success clears an earlier one.
pub(crate) fn remember_last(problems: &mut Problems, path: Option<&Path>, last: &PresetOutput) {
    let Some(path) = path else {
        return;
    };
    let saved = path
        .parent()
        .map_or(Ok(()), create_private_dir_all)
        .map_err(anyhow::Error::from)
        .and_then(|()| {
            update_presets(path, |file| file.last = Some(last.clone()))
                .map(drop)
                .map_err(anyhow::Error::from)
        });
    match saved {
        Ok(()) => problems.clear(Source::ExportPresets),
        Err(error) => problems.report(
            Source::ExportPresets,
            "Couldn’t remember the export settings",
            error_chain(error.as_ref()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::component::Root;
    use gpui_kit::{Subscription, TestAppContext, px, size};

    /// physical: a test window's size, not a layout length.
    const WINDOW_WIDTH: f32 = 800.;
    /// physical: a test window's size, not a layout length.
    const WINDOW_HEIGHT: f32 = 1000.;

    /// A page over no plugins and no presets files, in a window of its
    /// own, and a subscription standing for the editor's.
    fn a_page(cx: &mut TestAppContext) -> (Entity<ExportPage>, Subscription) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
        });
        let mut page = None;
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
            page = Some(view.clone());
            Root::new(view, window, cx)
        });
        cx.run_until_parked();
        (page.unwrap(), Subscription::new(|| {}))
    }

    #[test]
    fn left_panel_follows_the_page() {
        assert_eq!(left_panel(false, false), LeftPanel::Assets);
        assert_eq!(left_panel(false, true), LeftPanel::Export);
        assert_eq!(left_panel(true, false), LeftPanel::Hidden);
        // A React preview hides Assets, but not the page.
        assert_eq!(left_panel(true, true), LeftPanel::Export);
    }

    #[gpui_kit::test]
    fn the_slot_opens_only_with_a_page(cx: &mut TestAppContext) {
        let mut slot = ExportPageSlot::default();
        slot.show();
        assert!(!slot.is_open());
        assert!(slot.page().is_none());
        let (page, events) = a_page(cx);
        slot.set(page.clone(), events);
        // Setting a page does not show it.
        assert!(!slot.is_open());
        slot.show();
        assert!(slot.is_open());
        assert_eq!(slot.page(), Some(&page));
    }

    #[gpui_kit::test]
    fn hiding_keeps_the_page(cx: &mut TestAppContext) {
        let mut slot = ExportPageSlot::default();
        let (page, events) = a_page(cx);
        slot.set(page.clone(), events);
        slot.show();
        slot.hide();
        assert!(!slot.is_open());
        assert_eq!(slot.page(), Some(&page));
        slot.show();
        assert!(slot.is_open());
    }

    #[gpui_kit::test]
    fn clearing_the_slot_closes_the_page(cx: &mut TestAppContext) {
        let mut slot = ExportPageSlot::default();
        let (page, events) = a_page(cx);
        slot.set(page, events);
        slot.show();
        slot.clear();
        assert!(!slot.is_open());
        assert!(slot.page().is_none());
        // Nothing to show until a new page is set.
        slot.show();
        assert!(!slot.is_open());
    }

    #[gpui_kit::test]
    fn a_started_export_closes_the_page(cx: &mut TestAppContext) {
        let mut slot = ExportPageSlot::default();
        let (page, events) = a_page(cx);
        slot.set(page.clone(), events);
        slot.show();
        slot.export_started();
        assert!(!slot.is_open());
        // The page keeps its state for the next export.
        assert_eq!(slot.page(), Some(&page));
    }
}
