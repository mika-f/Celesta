#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use crate::icons::CelestaAssets;
use celesta_editor_theme as theme;
use celesta_react_bridge::{
    ProjectTsconfig, initialize_project, project_types_template,
    runtime_paths as react_runtime_paths,
};
use clap::Parser;
use gpui_kit::base::GlobalState;
#[cfg(not(target_os = "macos"))]
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::resizable::ResizableState;
use gpui_kit::component::{Root, TitleBar};
use gpui_kit::prelude::*;
use gpui_kit::{App, Bounds, KeyBinding, Menu, MenuItem, WindowBounds, WindowOptions, px, size};
use std::error::Error;

mod actions;
mod audio;
mod audio_control;
mod background;
mod cli;
mod component_schema;
mod export_control;
mod export_worker;
mod helpers;
mod lifecycle;
mod media_probe;
mod panels;
mod playback;
mod preview;
mod react_audio;
mod refresh;
mod render;
mod source;
#[cfg(test)]
mod tests;
mod view;
mod waveform;
mod widgets;

use actions::{
    ClearExportRange, CloseWindow, CreateNewProject, ExportProject, GoToEnd, GoToIn, GoToOut,
    GoToStart, JumpBackward, JumpForward, NextEditPoint, NextFrame, OpenProject, PausePlayback,
    PlayForward, PreviousEditPoint, PreviousFrame, Quit, ReloadProject, SetExportIn, SetExportOut,
    SetUpTypeScript, SetUpTypeScriptInFolder, ToggleLoop, TogglePlayback, ToggleSafeAreas,
    ZoomTimelineIn, ZoomTimelineOut, ZoomTimelineToFit,
};
use view::EditorView;

mod audio_cache;
mod icons;
mod meter;
mod timecode;
mod timeline;
mod viewer;

const EDITOR_DEMO_PROJECT: &str = include_str!("../../../examples/editor-demo.celesta.json");

fn main() {
    if let Err(error) = run() {
        eprintln!("Celesta: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = cli::Args::parse();
    if let Some(directory) = args.init {
        let (_, cli_script) = react_runtime_paths();
        let setup = initialize_project(&project_types_template(&cli_script), &directory)?;
        println!("Initialized Celesta project in {}", setup.root.display());
        if setup.tsconfig == ProjectTsconfig::MissingExtends {
            eprintln!(
                "Add \"extends\": \"./.celesta/tsconfig.json\" to your existing tsconfig.json to enable Celesta types."
            );
        }
        println!(
            "Open film.tsx with Celesta to preview. Run pnpm install for TypeScript or external dependencies."
        );
        return Ok(());
    }
    let path = args.path;
    let mut editor = EditorView::open(path.as_deref(), args.driver)?;

    let app = gpui_kit::application().with_assets(CelestaAssets);
    app.run(move |cx: &mut App| {
        gpui_kit::init(cx);
        theme::init(cx);
        cx.bind_keys([
            KeyBinding::new("secondary-n", CreateNewProject, Some("CelestaEditor")),
            KeyBinding::new("secondary-o", OpenProject, Some("CelestaEditor")),
            KeyBinding::new("secondary-r", ReloadProject, Some("CelestaEditor")),
            KeyBinding::new("secondary-w", CloseWindow, Some("CelestaEditor")),
            KeyBinding::new("secondary-q", Quit, None),
            KeyBinding::new("secondary-shift-e", ExportProject, Some("CelestaEditor")),
            KeyBinding::new("i", SetExportIn, Some("CelestaEditor")),
            KeyBinding::new("o", SetExportOut, Some("CelestaEditor")),
            KeyBinding::new("shift-x", ClearExportRange, Some("CelestaEditor")),
            KeyBinding::new("shift-i", GoToIn, Some("CelestaEditor")),
            KeyBinding::new("shift-o", GoToOut, Some("CelestaEditor")),
            KeyBinding::new("space", TogglePlayback, Some("CelestaEditor")),
            KeyBinding::new("k", PausePlayback, Some("CelestaEditor")),
            KeyBinding::new("l", PlayForward, Some("CelestaEditor")),
            KeyBinding::new("left", PreviousFrame, Some("CelestaEditor")),
            KeyBinding::new("right", NextFrame, Some("CelestaEditor")),
            KeyBinding::new("shift-left", JumpBackward, Some("CelestaEditor")),
            KeyBinding::new("shift-right", JumpForward, Some("CelestaEditor")),
            KeyBinding::new("home", GoToStart, Some("CelestaEditor")),
            KeyBinding::new("end", GoToEnd, Some("CelestaEditor")),
            KeyBinding::new("up", PreviousEditPoint, Some("CelestaEditor")),
            KeyBinding::new("down", NextEditPoint, Some("CelestaEditor")),
            KeyBinding::new("secondary-/", ToggleLoop, Some("CelestaEditor")),
            KeyBinding::new("'", ToggleSafeAreas, Some("CelestaEditor")),
            KeyBinding::new("=", ZoomTimelineIn, Some("CelestaEditor")),
            KeyBinding::new("-", ZoomTimelineOut, Some("CelestaEditor")),
            KeyBinding::new("shift-z", ZoomTimelineToFit, Some("CelestaEditor")),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        // macOS shows these in the system menu bar; elsewhere the title bar's
        // `AppMenuBar` renders the same list.
        cx.set_menus(app_menus());
        GlobalState::global_mut(cx)
            .set_app_menus(app_menus().into_iter().map(Menu::owned).collect());
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(960.0), px(600.0))),
                ..TitleBar::window_options()
            },
            |window, cx| {
                let view = cx.new(|cx| {
                    let focus_handle = cx.focus_handle();
                    let master_volume_focus = cx.focus_handle().tab_stop(true).tab_index(0);
                    focus_handle.focus(window, cx);
                    editor.focus_handle = Some(focus_handle);
                    editor.master_volume_focus = Some(master_volume_focus);
                    editor.dock_split = Some(cx.new(|_| ResizableState::default()));
                    editor.body_split = Some(cx.new(|_| ResizableState::default()));
                    #[cfg(not(target_os = "macos"))]
                    {
                        editor.app_menu_bar = Some(AppMenuBar::new(cx));
                    }
                    if editor.is_react_preview() {
                        editor.watch_react_entry(cx);
                    }
                    editor
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("could not open the Celesta window");
        cx.activate(true);
    });
    Ok(())
}

fn app_menus() -> Vec<Menu> {
    vec![
        Menu::new("Celesta").items([MenuItem::action("Quit Celesta", Quit)]),
        Menu::new("File").items([
            MenuItem::action("Create New Project…", CreateNewProject),
            MenuItem::action("Open…", OpenProject),
            MenuItem::action("Reload", ReloadProject),
            MenuItem::separator(),
            MenuItem::action("Export…", ExportProject),
            MenuItem::separator(),
            MenuItem::action("Set Up TypeScript", SetUpTypeScript),
            MenuItem::action("Set Up TypeScript in Folder…", SetUpTypeScriptInFolder),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        Menu::new("Playback").items([
            MenuItem::action("Play/Pause", TogglePlayback),
            MenuItem::action("Play", PlayForward),
            MenuItem::action("Stop", PausePlayback),
            MenuItem::action("Loop", ToggleLoop),
            MenuItem::separator(),
            MenuItem::action("Previous Frame", PreviousFrame),
            MenuItem::action("Next Frame", NextFrame),
            MenuItem::action("Back One Second", JumpBackward),
            MenuItem::action("Forward One Second", JumpForward),
            MenuItem::separator(),
            MenuItem::action("Previous Edit", PreviousEditPoint),
            MenuItem::action("Next Edit", NextEditPoint),
            MenuItem::action("Go to Start", GoToStart),
            MenuItem::action("Go to End", GoToEnd),
        ]),
        Menu::new("Mark").items([
            MenuItem::action("Mark In", SetExportIn),
            MenuItem::action("Mark Out", SetExportOut),
            MenuItem::action("Clear In and Out", ClearExportRange),
            MenuItem::separator(),
            MenuItem::action("Go to In", GoToIn),
            MenuItem::action("Go to Out", GoToOut),
        ]),
        Menu::new("View").items([
            MenuItem::action("Safe Areas", ToggleSafeAreas),
            MenuItem::separator(),
            MenuItem::action("Zoom In", ZoomTimelineIn),
            MenuItem::action("Zoom Out", ZoomTimelineOut),
            MenuItem::action("Zoom to Fit", ZoomTimelineToFit),
        ]),
    ]
}
