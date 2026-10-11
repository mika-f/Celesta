use crate::actions::{
    CloseWindow, CreateNewProject, OpenProject, ReloadProject, SetUpTypeScript,
    SetUpTypeScriptInFolder,
};
use crate::source::load_source;
use crate::view::EditorView;
use celesta_react_bridge::{
    DEFAULT_TEMPLATE, ProjectTsconfig, ProjectTypesSetup, TemplateInfo, initialize_project,
    list_templates, project_templates_dir, project_types_template,
    runtime_paths as react_runtime_paths, set_up_project_types, set_up_project_types_in,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogDescription, DialogFooter};
use gpui_kit::component::radio::Radio;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{ClickEvent, Context, PathPromptOptions, Pixels, Window, div, px};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Wide enough for a two-line Japanese description per template.
const TEMPLATE_DIALOG_WIDTH: Pixels = px(520.);
/// About four templates; more scroll inside the dialog.
const TEMPLATE_LIST_MAX_HEIGHT: Pixels = px(360.);

impl EditorView {
    pub(crate) fn create_new_project_action(
        &mut self,
        _: &CreateNewProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_create_project(window, cx);
    }

    pub(crate) fn create_new_project_click(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_create_project(window, cx);
    }

    /// File > Create New Project…: asks for a template, then for a folder.
    fn request_create_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.opening || window.has_active_dialog(cx) || !self.can_replace_contents(window, cx) {
            return;
        }
        let (_, cli_script) = react_runtime_paths();
        let dir = project_templates_dir(&cli_script);
        let templates: Rc<[TemplateInfo]> = match list_templates(&dir) {
            Ok(templates) if !templates.is_empty() => templates.into(),
            Ok(_) => {
                self.open_error = Some(
                    format!(
                        "Couldn’t create project: no templates found in {}",
                        dir.display()
                    )
                    .into(),
                );
                cx.notify();
                return;
            }
            Err(error) => {
                self.open_error = Some(format!("Couldn’t create project: {error}").into());
                cx.notify();
                return;
            }
        };
        // `list_templates` puts `blank` first, so it starts selected. The
        // dialog is rebuilt every frame, so the choice lives outside it.
        let selected = Rc::new(Cell::new(0_usize));
        let view = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let muted = cx.theme().muted_foreground;
            // `Radio` draws only the indicator: its own label column is
            // `w_full()` beside the indicator, so long descriptions would run
            // past the dialog's right padding. The text column here can shrink.
            let rows = templates.iter().enumerate().map(|(index, template)| {
                let select = {
                    let selected = selected.clone();
                    move |window: &mut Window| {
                        selected.set(index);
                        window.refresh();
                    }
                };
                let select_from_radio = select.clone();
                h_flex()
                    .id(("project-template", index))
                    .w_full()
                    .items_start()
                    .gap_2()
                    .cursor_pointer()
                    .on_click(move |_, window, _| select(window))
                    .child(
                        Radio::new(("project-template-radio", index))
                            .checked(selected.get() == index)
                            .accessibility_label(template.title.clone())
                            .on_click(move |_, window, _| select_from_radio(window)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().child(template.title.clone()))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(template.description.clone()),
                            ),
                    )
            });
            let templates_on_ok = templates.clone();
            let selected_on_ok = selected.clone();
            let view = view.clone();
            dialog
                .title("Create New Project")
                .w(TEMPLATE_DIALOG_WIDTH)
                .child(
                    v_flex()
                        .gap_3()
                        .child(
                            DialogDescription::new()
                                .child("Choose a template. You’ll pick the project folder next."),
                        )
                        // Only the list scrolls once there are more templates
                        // than fit, so the description and buttons stay put.
                        .child(
                            v_flex()
                                .id("project-templates")
                                .max_h(TEMPLATE_LIST_MAX_HEIGHT)
                                .gap_3()
                                .children(rows)
                                .overflow_y_scrollbar(),
                        ),
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            DialogClose::new().child(
                                Button::new("create-project-cancel")
                                    .label("Cancel")
                                    .outline(),
                            ),
                        )
                        .child(
                            DialogAction::new().child(
                                Button::new("create-project-create")
                                    .label("Create")
                                    .primary(),
                            ),
                        ),
                )
                .on_ok(move |_, window, cx| {
                    let template = templates_on_ok
                        .get(selected_on_ok.get())
                        .and_then(|template| template_argument(&template.name));
                    view.update(cx, |this, cx| {
                        this.create_project_in_folder(template, window, cx)
                    })
                    .ok();
                    true
                })
        });
    }

    /// Asks for a folder and initializes it from `template` (`None` keeps
    /// the `blank` template's keep-existing behaviour).
    fn create_project_in_folder(
        &mut self,
        template: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.opening || !self.can_replace_contents(window, cx) {
            return;
        }
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Create New Project".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let folder = match selection.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    view.update_in(cx, |this, _, cx| {
                        this.open_error =
                            Some(format!("Couldn’t show the folder dialog: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    None
                }
            };
            let Some(folder) = folder else { return };
            let started = view
                .update_in(cx, |this, window, cx| {
                    if this.opening || !this.can_replace_contents(window, cx) {
                        return false;
                    }
                    this.opening = true;
                    this.open_error = None;
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if !started {
                return;
            }
            let result = cx
                .background_executor()
                .spawn(async move {
                    let (_, cli_script) = react_runtime_paths();
                    initialize_project(&cli_script, &folder, template.as_deref())
                        .map(|setup| setup.types)
                })
                .await;
            view.update_in(cx, |this, window, cx| {
                this.opening = false;
                match result {
                    Ok(setup) => {
                        if setup.tsconfig == ProjectTsconfig::MissingExtends {
                            window.push_notification(
                                "Add \"extends\": \"./.celesta/tsconfig.json\" to your tsconfig.json to enable Celesta types.",
                                cx,
                            );
                        }
                        this.load_path(Some(setup.root.join("film.tsx")), window, cx);
                    }
                    Err(error) => {
                        this.open_error =
                            Some(format!("Couldn’t create project: {error}").into());
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// File > Open…: pick a project or React entry and show it in this window.
    pub(crate) fn open_project_action(
        &mut self,
        _: &OpenProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_open(window, cx);
    }

    pub(crate) fn open_project_click(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_open(window, cx);
    }

    pub(crate) fn request_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.opening || !self.can_replace_contents(window, cx) {
            return;
        }
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let path = match selection.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    view.update_in(cx, |this, _, cx| {
                        this.open_error =
                            Some(format!("Couldn’t show the Open dialog: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    None
                }
                Err(_) => None,
            };
            if let Some(path) = path {
                view.update_in(cx, |this, window, cx| {
                    this.load_path(Some(path), window, cx)
                })
                .ok();
            }
        })
        .detach();
    }

    /// File > Reload: read the current file again from disk. A standalone
    /// React entry re-bundles in place; anything else is reopened.
    pub(crate) fn reload_project_action(
        &mut self,
        _: &ReloadProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_react_preview() {
            self.request_react_reload(cx);
            cx.notify();
            return;
        }
        if self.opening || !self.can_replace_contents(window, cx) {
            return;
        }
        self.load_path(self.source_path.clone(), window, cx);
    }

    /// File > Set Up TypeScript: copies this build's `@celesta/*`, React,
    /// and Node declarations into the React entry's project as `.celesta/`,
    /// so editors type-check against the runtime that actually runs the entry.
    pub(crate) fn set_up_typescript_action(
        &mut self,
        _: &SetUpTypeScript,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.typescript_error = None;
        self.typescript_message = None;
        let Some(entry) = self.document.react_entry_absolute_path() else {
            self.typescript_error = Some(
                "Open a React entry, or use Set Up TypeScript in Folder… for a new project".into(),
            );
            cx.notify();
            return;
        };
        self.run_typescript_setup(window, cx, move |template| {
            set_up_project_types(template, &entry)
        });
    }

    /// File > Set Up TypeScript in Folder…: installs `.celesta/` into a folder
    /// picked by the user, so a new project has types before its first entry.
    pub(crate) fn set_up_typescript_in_folder_action(
        &mut self,
        _: &SetUpTypeScriptInFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.typescript_error = None;
        self.typescript_message = None;
        cx.notify();
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Set Up TypeScript".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let folder = match selection.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    view.update_in(cx, |this, _, cx| {
                        this.typescript_error =
                            Some(format!("Couldn’t show the folder dialog: {error}").into());
                        cx.notify();
                    })
                    .ok();
                    None
                }
            };
            if let Some(folder) = folder {
                view.update_in(cx, |this, window, cx| {
                    this.run_typescript_setup(window, cx, move |template| {
                        set_up_project_types_in(template, &folder)
                    });
                })
                .ok();
            }
        })
        .detach();
    }

    /// Runs `set_up` against this build's support template on a background
    /// thread and reports the outcome in the title bar.
    pub(crate) fn run_typescript_setup(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        set_up: impl FnOnce(&Path) -> std::io::Result<ProjectTypesSetup> + Send + 'static,
    ) {
        cx.spawn_in(window, async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let (_, cli_script) = react_runtime_paths();
                    set_up(&project_types_template(&cli_script))
                })
                .await;
            view.update_in(cx, |this, _, cx| {
                match result {
                    Ok(setup) if setup.tsconfig == ProjectTsconfig::MissingExtends => {
                        this.typescript_error = Some(
                            "Types installed — add \"extends\": \"./.celesta/tsconfig.json\" to tsconfig.json"
                                .into(),
                        );
                    }
                    Ok(setup) => {
                        let name = setup.root.file_name().unwrap_or(setup.root.as_os_str());
                        this.typescript_message =
                            Some(format!("TypeScript set up in {}", name.to_string_lossy()).into());
                    }
                    Err(error) => {
                        this.typescript_error =
                            Some(format!("TypeScript setup failed: {error}").into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn close_window_action(
        &mut self,
        _: &CloseWindow,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.remove_window();
    }

    /// Replacing the contents drops the export worker, so a running export
    /// must be cancelled first.
    pub(crate) fn can_replace_contents(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.export_cancellation.is_none() {
            return true;
        }
        window.push_notification("Cancel the export before opening another file.", cx);
        false
    }

    /// Loads `path` on a background thread (a React entry blocks on a Node
    /// handshake), then swaps it into this view. On failure the current
    /// contents stay and the error shows in the title bar.
    pub(crate) fn load_path(
        &mut self,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Reopening the same entry keeps its --props/--props-file; another
        // file may declare different properties, so it starts without them.
        let property_args = self
            .react_preview
            .as_ref()
            .filter(|react| {
                path.as_deref()
                    .and_then(|path| std::path::absolute(path).ok())
                    .is_some_and(|path| path == react.entry)
            })
            .map(|react| react.property_args.clone())
            .unwrap_or_default();
        self.opening = true;
        self.open_error = None;
        self.pause();
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let load_path = path.clone();
            let loaded = cx
                .background_executor()
                .spawn(async move { load_source(load_path.as_deref(), property_args) })
                .await;
            view.update_in(cx, |this, _, cx| {
                this.opening = false;
                let driver = this.driver;
                let result = loaded.and_then(|(document, react_preview)| {
                    EditorView::from_document(path, document, react_preview, driver)
                        .map_err(|error| error.to_string())
                });
                match result {
                    Ok(next) => this.replace_contents(next, cx),
                    Err(error) => {
                        this.open_error = Some(format!("Couldn’t open: {error}").into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Swaps in a freshly opened view while keeping the window-owned UI state
    /// (focus, pane sizes, menu bar, and the preview volume).
    pub(crate) fn replace_contents(&mut self, mut next: EditorView, cx: &mut Context<Self>) {
        next.session = self.session.wrapping_add(1);
        next.focus_handle = self.focus_handle.take();
        next.master_volume_focus = self.master_volume_focus.take();
        next.dock_split = self.dock_split.take();
        next.body_split = self.body_split.take();
        #[cfg(not(target_os = "macos"))]
        {
            next.app_menu_bar = self.app_menu_bar.take();
        }
        next.monitor_volume = self.monitor_volume;
        next.loop_playback = self.loop_playback;
        next.show_safe_areas = self.show_safe_areas;
        *self = next;
        if self.is_react_preview() {
            self.watch_react_entry(cx);
        }
    }
}

/// The `initialize_project` argument for a chosen template: `blank` is the
/// default, which keeps any files already in the folder.
fn template_argument(name: &str) -> Option<String> {
    (name != DEFAULT_TEMPLATE).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::template_argument;

    #[test]
    fn blank_template_uses_the_default_initialization() {
        assert_eq!(template_argument("blank"), None);
        assert_eq!(template_argument("slideshow"), Some("slideshow".to_owned()));
    }
}
