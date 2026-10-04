use crate::actions::{ClearExportRange, ExportProject, SetExportIn, SetExportOut};
use crate::export_worker::{ExportRequest, ExportSource};
use crate::helpers::{export_range_for, export_suggested_name};
use crate::view::EditorView;
use celesta_exporter::{ExportCancellation, ExportProgress};
use celesta_react_bridge::runtime_paths as react_runtime_paths;
use gpui_kit::{ClickEvent, Context, Window};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

impl EditorView {
    pub(crate) fn export_project_action(
        &mut self,
        _: &ExportProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(crate) fn request_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.choosing_export_path || self.export_cancellation.is_some() {
            return;
        }
        self.pause();
        self.choosing_export_path = true;
        self.export_error = None;
        self.export_message = None;
        let directory = self.document.path().and_then(Path::parent).map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            Path::to_path_buf,
        );
        let suggested_name = export_suggested_name(self.document.path(), &self.project_name);
        let selection = cx.prompt_for_new_path(&directory, Some(&suggested_name));
        cx.spawn_in(window, async move |view, cx| {
            let selected_path = match selection.await {
                Ok(Ok(path)) => path,
                Ok(Err(error)) => {
                    view.update_in(cx, |this, _, cx| {
                        this.choosing_export_path = false;
                        this.export_error =
                            Some(format!("could not open Export dialog: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    view.update_in(cx, |this, _, cx| {
                        this.choosing_export_path = false;
                        this.export_error =
                            Some(format!("Export dialog was interrupted: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            view.update_in(cx, |this, _, cx| {
                this.choosing_export_path = false;
                let Some(mut output) = selected_path else {
                    cx.notify();
                    return;
                };
                if output.extension() != Some(OsStr::new("mp4")) {
                    output.set_extension("mp4");
                }
                this.start_export(output);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn start_export(&mut self, output: PathBuf) {
        let cancellation = ExportCancellation::default();
        let (source, range) = match &self.react_preview {
            Some(react) => {
                let (node, cli_script) = react_runtime_paths();
                (
                    ExportSource::ReactEntry {
                        entry: react.entry.clone(),
                        node,
                        cli_script,
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
            cancellation: cancellation.clone(),
        };
        self.export_path = Some(output);
        self.export_progress = Some(ExportProgress::Rendering { frame: 0, total: 0 });
        self.export_cancellation = Some(cancellation);
        self.export_cancelling = false;
        self.export_error = None;
        self.export_message = None;
        if let Err(error) = self.export_worker.request(request) {
            self.export_path = None;
            self.export_progress = None;
            self.export_cancellation = None;
            self.export_error = Some(error.into());
        }
    }

    /// Marks the current playhead frame as the export in-point, dropping a
    /// stale out-point that would now sit at or before it.
    pub(crate) fn set_export_in(&mut self, cx: &mut Context<Self>) {
        let frame = self.clock.frame();
        self.export_in_frame = Some(frame);
        if self.export_out_frame.is_some_and(|out| out <= frame) {
            self.export_out_frame = None;
        }
        self.export_error = None;
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
        self.export_error = None;
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
