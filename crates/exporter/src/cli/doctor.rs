use std::borrow::Cow;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use celesta_composition::Time;
use celesta_gpu_renderer::{GpuRenderOptions, GpuRenderer};
use celesta_media::ComponentRef;
use celesta_plugin_host::without_console_window;
use celesta_react_bridge::runtime_paths;
use clap::error::ErrorKind;
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser};
use serde::Serialize;

use super::{
    AudioEncoderArg, SourceArgs, options, parse_audio_encoder, parse_component, parse_option,
    validate_audio_options, validated_export_range,
};
use crate::project::folder_of;
use crate::{
    AudioChoice, EncoderChoice, ExportCancellation, ExportError, ExportProgress, ExportRange,
    ExportedFile, FrameSelection, GpuDriver, PresetLookup, StillExport, parse_timecode, report,
};

const SCHEMA_VERSION: u32 = 1;

/// Check this machine or test a render without publishing its output.
#[derive(Parser)]
#[command(name = "doctor")]
struct DoctorCli {
    /// The JSON project or, with --react, the React entry to test.
    #[arg(requires = "output")]
    source: Option<PathBuf>,
    /// The intended output path; only temporary files are written.
    #[arg(requires = "source")]
    output: Option<PathBuf>,
    #[command(flatten)]
    input: SourceArgs,
    /// Graphics API to check.
    #[arg(long, default_value_t = GpuDriver::default(), value_enum)]
    driver: GpuDriver,
    /// Test a single still instead of a video.
    #[arg(long, requires = "output", conflicts_with_all = ["from", "to", "preset", "preset_file", "audio_encoder", "audio_option"])]
    frame: Option<u64>,
    /// Start timecode of the video range to test.
    #[arg(long, requires = "output", value_parser = parse_timecode)]
    from: Option<Time>,
    /// End timecode of the video range to test.
    #[arg(long, requires = "output", value_parser = parse_timecode)]
    to: Option<Time>,
    /// Use a named export preset.
    #[arg(long, requires = "output")]
    preset: Option<String>,
    /// An export-presets.json to search first.
    #[arg(long, requires = "output", value_name = "PATH")]
    preset_file: Option<PathBuf>,
    /// Override the default muxer.
    #[arg(long, requires = "output", value_name = "PLUGIN/ID", value_parser = parse_component)]
    muxer: Option<ComponentRef>,
    /// Override the default video or image encoder.
    #[arg(long, requires = "output", value_name = "PLUGIN/ID", value_parser = parse_component)]
    video_encoder: Option<ComponentRef>,
    /// A video or image encoder setting; repeatable.
    #[arg(long, requires = "output", value_name = "KEY=VALUE", value_parser = parse_option)]
    video_option: Vec<(String, String)>,
    /// Override the default audio encoder or disable audio.
    #[arg(long, requires = "output", value_name = "PLUGIN/ID|none", value_parser = parse_audio_encoder)]
    audio_encoder: Option<AudioEncoderArg>,
    /// An audio encoder setting; repeatable.
    #[arg(long, requires = "output", value_name = "KEY=VALUE", value_parser = parse_option)]
    audio_option: Vec<(String, String)>,
    /// Allow an existing output when testing; never actually replace it.
    #[arg(long, requires = "output")]
    overwrite: bool,
    /// Print a single versioned JSON report.
    #[arg(long)]
    json: bool,
}

/// Arguments for `celesta doctor`.
pub fn doctor_command() -> clap::Command {
    DoctorCli::command()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema_version: u32,
    status: &'static str,
    checks: Checks,
}

#[derive(Serialize)]
struct Checks {
    node: Check,
    graphics: Check,
    output: Check,
}

#[derive(Serialize)]
struct Check {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<Cow<'static, str>>,
}

impl Check {
    fn new(status: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code: None,
            message: message.into(),
            hint: None,
        }
    }

    fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code: Some(code),
            ..Self::new("error", message)
        }
    }
}

impl Report {
    fn print(&self, json: bool) {
        if json {
            println!(
                "{}",
                serde_json::to_string(self).expect("a doctor report serializes")
            );
        } else {
            for (name, check) in [
                ("Node.js", &self.checks.node),
                ("Graphics", &self.checks.graphics),
                ("Output", &self.checks.output),
            ] {
                println!("{name}: {} — {}", check.status, check.message);
                if let Some(hint) = &check.hint {
                    println!("  hint: {hint}");
                }
            }
        }
    }
}

/// Report invalid invocations in the doctor schema when requested.
pub fn doctor_parse_failed(error: clap::Error) -> ExitCode {
    if error.use_stderr() && std::env::args_os().any(|arg| arg == "--json") {
        Report {
            schema_version: SCHEMA_VERSION,
            status: "error",
            checks: Checks {
                node: Check::new("skipped", "invalid arguments"),
                graphics: Check::new("skipped", "invalid arguments"),
                output: Check::error("usage", error.to_string()),
            },
        }
        .print(true);
    } else {
        let _ = error.print();
    }
    if error.use_stderr() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

/// Run the host checks and any requested end-to-end render test.
pub fn run_doctor(matches: &ArgMatches) -> ExitCode {
    let cli = match DoctorCli::from_arg_matches(matches) {
        Ok(cli) => cli,
        Err(error) => return doctor_parse_failed(error),
    };
    if let Err(error) = validate_audio_options(cli.audio_encoder.as_ref(), &cli.audio_option) {
        return doctor_usage(ErrorKind::ArgumentConflict, error);
    }
    let range = match validated_export_range(cli.preset_file.as_deref(), cli.from, cli.to) {
        Ok(range) => range,
        Err(error) => return doctor_usage(ErrorKind::ValueValidation, error),
    };
    let node = if cli.source.is_none() || cli.input.react {
        check_node()
    } else {
        Check::new("skipped", "JSON projects do not require Node.js")
    };
    let graphics = check_graphics(cli.driver);
    let output = match (&cli.source, &cli.output) {
        (Some(source), Some(output)) if node.status != "error" && graphics.status != "error" => {
            check_output(&cli, source, output, range)
        }
        (Some(_), _) => Check::new("skipped", "a prerequisite failed"),
        _ => Check::new("skipped", "no render requested"),
    };
    let failed = [&node, &graphics, &output]
        .iter()
        .any(|check| check.status == "error");
    Report {
        schema_version: SCHEMA_VERSION,
        status: if failed { "error" } else { "ok" },
        checks: Checks {
            node,
            graphics,
            output,
        },
    }
    .print(cli.json);
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn doctor_usage(kind: ErrorKind, message: String) -> ExitCode {
    doctor_parse_failed(
        doctor_command()
            .bin_name("celesta doctor")
            .error(kind, message),
    )
}

fn check_node() -> Check {
    let (node, runtime) = runtime_paths();
    let version = match without_console_window(Command::new(&node).arg("--version")).output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        Ok(output) => {
            return Check::error(
                "node_unavailable",
                format!("{} exited with {}", node.display(), output.status),
            );
        }
        Err(error) => {
            return Check::error("node_unavailable", format!("{}: {error}", node.display()));
        }
    };
    let major = version
        .strip_prefix('v')
        .and_then(|version| version.split('.').next())
        .and_then(|major| major.parse::<u32>().ok());
    if major.is_none_or(|major| major < 18) {
        return Check::error(
            "node_version",
            format!("expected Node.js 18 or newer, got {version:?}"),
        );
    }
    match fs::metadata(&runtime) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => {
            return Check::error(
                "runtime_unavailable",
                format!("{} is not a regular file", runtime.display()),
            );
        }
        Err(error) => {
            return Check::error(
                "runtime_unavailable",
                format!("{}: {error}", runtime.display()),
            );
        }
    };
    let mut command = Command::new(&node);
    match without_console_window(command.arg(&runtime).arg("--check-runtime")).output() {
        Ok(output) if output.status.success() => {
            Check::new("ok", format!("Node.js {version}; {}", runtime.display()))
        }
        Ok(output) => Check::error(
            "runtime_unavailable",
            format!(
                "{} exited with {}: {}",
                runtime.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ),
        Err(error) => Check::error(
            "runtime_unavailable",
            format!("{}: {error}", runtime.display()),
        ),
    }
}

fn check_graphics(driver: GpuDriver) -> Check {
    match GpuRenderer::new(GpuRenderOptions {
        driver,
        ..GpuRenderOptions::default()
    }) {
        Ok(renderer) => {
            let adapter = renderer.adapter_info();
            Check::new("ok", format!("{} ({:?})", adapter.name, adapter.backend))
        }
        Err(error) => Check::error("graphics_unavailable", error.to_string()),
    }
}

fn check_output(
    cli: &DoctorCli,
    source: &Path,
    output: &Path,
    range: Option<ExportRange>,
) -> Check {
    match test_render(cli, source, output, range) {
        Ok(()) => Check::new(
            "ok",
            "rendered through the selected media path; output discarded",
        ),
        Err(error) => {
            let (code, hint) = report::classify_source(&error, source, cli.input.react);
            Check {
                hint,
                ..Check::error(code, error.to_string())
            }
        }
    }
}

fn test_render(
    cli: &DoctorCli,
    source: &Path,
    output: &Path,
    range: Option<ExportRange>,
) -> Result<(), ExportError> {
    check_destination(output, cli.overwrite)?;
    let name = output.file_name().ok_or_else(|| {
        ExportError::InvalidSelection("an output file name is required".to_owned())
    })?;
    let parent = folder_of(output);
    let directory = parent
        .ancestors()
        .map(|path| if path.as_os_str().is_empty() { Path::new(".") } else { path })
        .find(|path| !matches!(fs::symlink_metadata(path), Err(error) if error.kind() == io::ErrorKind::NotFound))
        .unwrap_or(parent);
    let scratch = tempfile::Builder::new()
        .prefix(".celesta-doctor-")
        .tempdir_in(directory)
        .map_err(|source| ExportError::Io {
            operation: "create temporary doctor output",
            source,
        })?;
    let mut job = cli.input.job(source);
    job.output = Some(scratch.path().join(name));
    job.stills = cli.frame.map(|frame| StillExport {
        frames: FrameSelection::Frames(vec![frame]),
        contact_sheet: None,
        debug_guides: false,
        quality: None,
    });
    job.options.driver = cli.driver;
    job.options.range = range;
    job.options.output.preset = cli.preset.clone();
    job.options.output.muxer = cli.muxer.clone();
    job.options.output.video = EncoderChoice {
        component: cli.video_encoder.clone(),
        options: options(&cli.video_option),
    };
    job.options.output.audio = match &cli.audio_encoder {
        Some(AudioEncoderArg::None) => AudioChoice::None,
        encoder => AudioChoice::Encode(EncoderChoice {
            component: match encoder {
                Some(AudioEncoderArg::Component(component)) => Some(component.clone()),
                _ => None,
            },
            options: options(&cli.audio_option),
        }),
    };
    if job.stills.is_none() {
        job.options.presets = PresetLookup::for_source(source, cli.preset_file.clone());
    }
    job.options.plugins = Some(cli.input.plugins(source)?);
    let mut produced = Vec::new();
    job.run(&ExportCancellation::default(), &mut |progress| {
        if let ExportProgress::Wrote(file) = progress {
            match file {
                ExportedFile::Video { files, .. } => produced.extend(files),
                ExportedFile::Frame { path, .. }
                | ExportedFile::Subtitles { path, .. }
                | ExportedFile::ContactSheet { path, .. } => produced.push(path),
            }
        }
    })?;
    for file in produced {
        if let Ok(relative) = file.strip_prefix(scratch.path()) {
            check_destination(&parent.join(relative), cli.overwrite)?;
        }
    }
    Ok(())
}

fn check_destination(path: &Path, overwrite: bool) -> Result<(), ExportError> {
    let operation = "check intended output";
    #[cfg(unix)]
    if path.as_os_str().as_encoded_bytes().ends_with(b"/") {
        return Err(ExportError::Io {
            operation,
            source: io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("{} ends with a directory separator", path.display()),
            ),
        });
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Err(ExportError::Io {
            operation,
            source: io::Error::new(
                io::ErrorKind::IsADirectory,
                format!("{} is a directory", path.display()),
            ),
        }),
        Ok(_) if !overwrite => Err(ExportError::OutputExists(path.to_owned())),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ExportError::Io { operation, source }),
    }
}
