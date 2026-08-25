use std::ffi::OsString;
use std::process::ExitCode;

use mikan_exporter::{ExportOptions, ExportProgress, Exporter};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mikan-export: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut overwrite = false;
    let mut paths = Vec::<OsString>::new();
    for argument in std::env::args_os().skip(1) {
        if argument == "--overwrite" {
            overwrite = true;
        } else {
            paths.push(argument);
        }
    }
    let [project, output] = paths.as_slice() else {
        return Err("usage: mikan-exporter [--overwrite] <project.mikan.json> <output.mp4>".into());
    };
    let exporter = Exporter::new(ExportOptions {
        overwrite,
        ..ExportOptions::default()
    });
    exporter
        .export_file_with_progress(project, output, |progress| match progress {
            ExportProgress::Rendering { frame, total } => {
                eprint!("\rrendering frame {frame}/{total}");
                if frame == total {
                    eprintln!();
                }
            }
            ExportProgress::MixingAudio => eprintln!("mixing audio"),
            ExportProgress::Muxing => eprintln!("muxing MP4"),
        })
        .map_err(|error| error.to_string())?;
    eprintln!("export complete: {}", output.to_string_lossy());
    Ok(())
}
