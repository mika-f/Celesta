use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use mikan_exporter::{ExportOptions, ExportProgress, Exporter, ReactRuntimeOptions};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mikan-export: {error}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "usage: mikan-exporter [--overwrite] <project.mikan.json> <output.mp4>\n       mikan-exporter [--overwrite] --react <entry.tsx> <output.mp4>";

fn run() -> Result<(), String> {
    let mut overwrite = false;
    let mut react = false;
    let mut paths = Vec::<OsString>::new();
    for argument in std::env::args_os().skip(1) {
        if argument == "--overwrite" {
            overwrite = true;
        } else if argument == "--react" {
            react = true;
        } else {
            paths.push(argument);
        }
    }
    let [source, output] = paths.as_slice() else {
        return Err(USAGE.into());
    };
    let exporter = Exporter::new(ExportOptions {
        overwrite,
        ..ExportOptions::default()
    });
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
        exporter
            .export_react_entry_with_progress(source, &runtime, output, on_progress)
            .map_err(|error| error.to_string())?;
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/react/src/cli.js");
    ReactRuntimeOptions::new("node", cli_script)
}
