use crate::actions::{ClearExportRange, ExportProject, SetExportIn, SetExportOut};
use crate::export_dialog::view::{ExportDialog, ExportDialogEvent, open_export_dialog};
use crate::export_worker::{ExportRequest, ExportSource};
use crate::helpers::{ExportLoudness, export_output, export_range_for, export_suggested_name};
use crate::inline_text_view::EditEvent;
use crate::problems::{Source, error_chain};
use crate::timecode::format_timecode;
use crate::view::EditorView;
use celesta_exporter::{
    ExportCancellation, ExportProgress, OutputChoice, PRESETS_FILE, PresetOutput, update_presets,
    user_presets_path,
};
use celesta_media::{ComponentEntry, ComponentKind, MediaRegistry};
use celesta_plugin_host::project_root;
use celesta_react_bridge::runtime_paths as react_runtime_paths;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, ClickEvent, Context, Entity, SharedString, Window};
use std::path::{Path, PathBuf};
use std::sync::Arc;

impl EditorView {
    pub(crate) fn export_project_action(
        &mut self,
        _: &ExportProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.end_inline_text(EditEvent::ProjectCommand, cx);
        self.request_export(window, cx);
    }

    pub(crate) fn export_project_click(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_export(window, cx);
    }

    /// Export…: opens the export dialog (as it was left, within a
    /// session); its `Export…` asks where to write the file.
    pub(crate) fn request_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.choosing_export_path
            || self.export_cancellation.is_some()
            || window.has_active_dialog(cx)
        {
            return;
        }
        self.pause();
        self.problems.clear(Source::Export);
        let project_file = self
            .export_source_path()
            .map(|source| project_root(source).join(PRESETS_FILE));
        // A dialog is kept for the project file it read; an untitled
        // document saved since, or another file, reads its own.
        let kept = self
            .export_dialog
            .as_ref()
            .filter(|(dialog, _)| dialog.read(cx).project_file() == project_file.as_deref());
        let dialog = match kept {
            Some((dialog, _)) => dialog.clone(),
            None => {
                // Without a registry yet no media plugin can export: the
                // dialog lists none and says why Export… cannot run.
                let registry = self
                    .found_plugins
                    .registry
                    .clone()
                    .unwrap_or_else(|| Arc::new(MediaRegistry::new(Vec::new())));
                let loudness = self.export_loudness;
                let dialog = cx.new(|cx| {
                    ExportDialog::new(
                        registry,
                        project_file,
                        user_presets_path(),
                        loudness,
                        window,
                        cx,
                    )
                });
                let events = cx.subscribe_in(&dialog, window, Self::export_dialog_event);
                self.export_dialog = Some((dialog.clone(), events));
                dialog
            }
        };
        let range = self.export_range_label();
        dialog.update(cx, |dialog, cx| dialog.set_range(range, cx));
        open_export_dialog(&dialog, window, cx);
        cx.notify();
    }

    /// The document or React entry an export renders; `None` for an
    /// untitled document, which has no project presets file.
    fn export_source_path(&self) -> Option<&Path> {
        match &self.react_preview {
            Some(react) => Some(react.entry.as_path()),
            None => self.document.path(),
        }
    }

    /// The in- and out-points as the dialog shows them, when they mark
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

    fn export_dialog_event(
        &mut self,
        dialog: &Entity<ExportDialog>,
        event: &ExportDialogEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ExportDialogEvent::Export {
                choice,
                extension,
                last,
            } => {
                self.remember_last(last.clone());
                self.choose_export_path(choice.clone(), extension, window, cx);
            }
            ExportDialogEvent::Loudness(loudness) => self.set_export_loudness(*loudness, cx),
            ExportDialogEvent::ClearRange => {
                self.clear_export_range(cx);
                dialog.update(cx, |dialog, cx| dialog.set_range(None, cx));
            }
        }
    }

    /// Saves the dialog's choice as the user file's `last`; a failure is
    /// reported, and the export goes on.
    fn remember_last(&mut self, last: PresetOutput) {
        let Some(path) = user_presets_path() else {
            return;
        };
        let saved = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .map_err(anyhow::Error::from)
            .and_then(|()| {
                update_presets(&path, |file| file.last = Some(last))
                    .map(drop)
                    .map_err(anyhow::Error::from)
            });
        if let Err(error) = saved {
            self.problems.report(
                Source::Export,
                "Couldn’t remember the export settings",
                error_chain(error.as_ref()),
            );
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
        self.choosing_export_path = true;
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
                        this.choosing_export_path = false;
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
                        this.choosing_export_path = false;
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
            view.update_in(cx, |this, _, cx| {
                this.choosing_export_path = false;
                let Some(selected_path) = selected_path else {
                    cx.notify();
                    return;
                };
                this.start_export(export_output(selected_path, &extensions), choice);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
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
        cx.notify();
    }

    pub(crate) fn clear_export_range(&mut self, cx: &mut Context<Self>) {
        self.export_in_frame = None;
        self.export_out_frame = None;
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
