use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mikan_exporter::{
    CompanionProject, ExportOptions, ExportProgress, Exporter, ReactRuntimeOptions,
};
use mikan_project::Project;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mikan-export: {error}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "usage: mikan-exporter [--overwrite] <project.mikan.json> <output.mp4>\n       mikan-exporter [--overwrite] --react <entry.tsx> [--project <project.mikan.json>] <output.mp4>";

fn run() -> Result<(), String> {
    let mut overwrite = false;
    let mut react = false;
    let mut companion_project_path: Option<OsString> = None;
    let mut paths = Vec::<OsString>::new();
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--overwrite" {
            overwrite = true;
        } else if argument == "--react" {
            react = true;
        } else if argument == "--project" {
            companion_project_path = Some(arguments.next().ok_or(USAGE)?);
        } else {
            paths.push(argument);
        }
    }
    let [source, output] = paths.as_slice() else {
        return Err(USAGE.into());
    };
    if companion_project_path.is_some() && !react {
        return Err("--project requires --react".into());
    }
    let exporter = Exporter::new(ExportOptions { overwrite });
    let on_progress = |progress: ExportProgress| match progress {
        ExportProgress::Rendering { frame, total } => {
            eprint!("\rrendering frame {frame}/{total}");
            if frame == total {
                eprintln!();
            }
        }
        ExportProgress::MixingAudio => eprintln!("mixing audio"),
        ExportProgress::Muxing => eprintln!("muxing MP4"),
    };
    if react {
        let runtime = default_react_runtime();
        match companion_project_path {
            Some(project_path) => {
                let project_path = Path::new(&project_path);
                let project = Project::load(project_path).map_err(|error| error.to_string())?;
                let project_asset_root = project_path.parent().unwrap_or_else(|| Path::new("."));
                exporter
                    .export_react_entry_with_project_and_progress(
                        source,
                        &runtime,
                        CompanionProject {
                            project: &project,
                            project_asset_root,
                        },
                        output,
                        on_progress,
                    )
                    .map_err(|error| error.to_string())?;
            }
            None => {
                exporter
                    .export_react_entry_with_progress(source, &runtime, output, on_progress)
                    .map_err(|error| error.to_string())?;
            }
        }
    } else {
        exporter
            .export_file_with_progress(source, output, on_progress)
            .map_err(|error| error.to_string())?;
    }
    eprintln!("export complete: {}", output.to_string_lossy());
    Ok(())
}

/// Resolves the `@mikan/react` runtime shipped alongside this workspace.
/// This assumes a monorepo checkout; a packaged Mikan distribution will need
/// to locate the runtime differently.
fn default_react_runtime() -> ReactRuntimeOptions {
    let cli_script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/react/dist/cli.js");
    ReactRuntimeOptions::new("node", cli_script)
}
