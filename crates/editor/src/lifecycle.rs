use crate::actions::{
    CloseWindow, CreateNewProject, OpenProject, ReloadProject, SetUpTypeScript,
    SetUpTypeScriptInFolder,
};
use crate::source::{PropertyArgs, load_source};
use crate::view::EditorView;
use celesta_react_bridge::{
    ProjectTsconfig, ProjectTypesSetup, initialize_project, project_types_template,
    runtime_paths as react_runtime_paths, set_up_project_types, set_up_project_types_in,
};
use gpui_kit::component::WindowExt as _;
use gpui_kit::{ClickEvent, Context, PathPromptOptions, Window};
use std::path::{Path, PathBuf};

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

    fn request_create_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                    initialize_project(&project_types_template(&cli_script), &folder)
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

    /// File > Set Up TypeScript: copies this build's `@celesta/react`, React,
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
        self.opening = true;
        self.open_error = None;
        self.pause();
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let load_path = path.clone();
            let loaded = cx
                .background_executor()
                .spawn(async move { load_source(load_path.as_deref(), PropertyArgs::default()) })
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
