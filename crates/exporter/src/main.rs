use std::num::{NonZeroU32, NonZeroU64};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, ValueEnum};

mod inspect;
mod progress;

use celesta_composition::Time;
use celesta_exporter::{
    ColorConversion, CompanionProject, ContactSheet, EncoderPreset, ExportOptions, ExportProgress,
    ExportRange, Exporter, FrameSelection, PngExport, ReactRuntimeOptions, RenderQuality,
    VideoEncoding, parse_timecode,
};
use celesta_project::Project;

const AFTER_HELP: &str = "\
Timecodes are HH:MM:SS(.mmm), MM:SS(.mmm) or SS(.mmm); --from/--to select a
span of the composition to export (the output starts at its own 00:00).

PNG output is selected by --frame/--frames or --every. Multiple frames use
output-000090.png names; frame numbers are zero-based. --every <n> picks
frames 0, n, 2n, ... of the --from/--to span (or the whole composition) plus
its last frame. --contact-sheet writes the selection as one labelled grid
image instead.";

/// Exports a Celesta project or React composition to MP4 video or PNG stills.
#[derive(Parser)]
#[command(
    name = "celesta-exporter",
    version,
    after_help = AFTER_HELP,
    args_override_self = true
)]
struct Cli {
    /// The project (.celesta.json), or with --react the entry (.tsx), to export.
    source: PathBuf,
    /// The .mp4 file to write, or the .png file for frame exports.
    #[arg(required_unless_present = "inspect", conflicts_with = "inspect")]
    output: Option<PathBuf>,
    /// Inspect a React entry over stdin/stdout JSON lines without rendering pixels.
    #[arg(
        long,
        requires = "react",
        conflicts_with_all = [
            "project", "overwrite", "from", "to", "frames", "every",
            "contact_sheet", "columns", "tile_width", "output_format",
            "preset", "crf", "color_conversion", "render_quality", "no_ui"
        ]
    )]
    inspect: bool,
    /// React CLI script for inspection (defaults to the bundled runtime).
    #[arg(long)]
    runtime: Option<PathBuf>,
    /// Node.js executable for inspection (defaults to the bundled runtime).
    #[arg(long)]
    node: Option<PathBuf>,
    /// Export a React composition instead of a JSON project.
    #[arg(long)]
    react: bool,
    /// Companion JSON project for <ProjectTimeline />/<ProjectTrack />.
    #[arg(long, value_name = "PROJECT", requires = "react")]
    project: Option<PathBuf>,
    /// Replace an existing output file.
    #[arg(long)]
    overwrite: bool,
    /// Export from this timecode.
    #[arg(long, value_name = "TIMECODE", value_parser = parse_timecode)]
    from: Option<Time>,
    /// Export up to (excluding) this timecode.
    #[arg(long, value_name = "TIMECODE", value_parser = parse_timecode)]
    to: Option<Time>,
    /// Export these zero-based frames as PNG; repeatable and comma-separated.
    #[arg(
        long = "frame",
        visible_alias = "frames",
        value_name = "N,...",
        value_delimiter = ',',
        conflicts_with_all = ["every", "from", "to"]
    )]
    frames: Vec<u64>,
    /// Export every n-th frame (and the last one) as PNG.
    #[arg(long, value_name = "N")]
    every: Option<NonZeroU64>,
    /// Write the selected frames as one labelled grid image.
    #[arg(long)]
    contact_sheet: bool,
    /// Contact sheet columns.
    #[arg(
        long,
        value_name = "N",
        requires = "contact_sheet",
        default_value_t = non_zero(ContactSheet::DEFAULT_COLUMNS)
    )]
    columns: NonZeroU32,
    /// Contact sheet tile width in pixels.
    #[arg(
        long,
        value_name = "PX",
        requires = "contact_sheet",
        default_value_t = non_zero(ContactSheet::DEFAULT_TILE_WIDTH)
    )]
    tile_width: NonZeroU32,
    /// Output format; PNG is implied by a frame selection.
    #[arg(long, value_enum)]
    output_format: Option<OutputFormat>,
    /// libx264 preset: faster presets encode faster but produce larger files.
    #[arg(
        long,
        default_value_t = EncoderPreset::default(),
        value_parser = names(&EncoderPreset::ALL, EncoderPreset::as_str)
    )]
    preset: EncoderPreset,
    /// Constant rate factor, 0-51; lower is higher quality.
    #[arg(
        long,
        default_value_t = VideoEncoding::default().crf,
        value_parser = clap::value_parser!(u8).range(..=i64::from(VideoEncoding::MAX_CRF))
    )]
    crf: u8,
    /// Where RGB frames become YUV: auto uses the GPU unless it is a software renderer.
    #[arg(
        long,
        value_name = "WHERE",
        default_value_t = ColorConversion::default(),
        value_parser = names(&ColorConversion::ALL, ColorConversion::as_str)
    )]
    color_conversion: ColorConversion,
    /// How carefully scaled and rotated layers are drawn; draft skips
    /// re-rasterizing scaled text and is meant for quick timing checks.
    #[arg(
        long,
        value_name = "QUALITY",
        default_value_t = RenderQuality::default(),
        value_parser = names(&RenderQuality::ALL, RenderQuality::as_str)
    )]
    render_quality: RenderQuality,
    /// Disable the terminal progress dashboard (automatic outside a terminal).
    #[arg(long)]
    no_ui: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Png,
    Mp4,
}

/// A parser accepting exactly `name` of each of `all`, listed in `--help`.
fn names<T: Copy + Send + Sync + 'static>(
    all: &'static [T],
    name: fn(T) -> &'static str,
) -> impl TypedValueParser<Value = T> {
    PossibleValuesParser::new(all.iter().map(|&value| name(value))).map(move |text| {
        all.iter()
            .copied()
            .find(|&value| name(value) == text)
            .expect("PossibleValuesParser only accepts listed names")
    })
}

fn non_zero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("contact sheet defaults are positive")
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // `output` conflicts with --inspect, which makes clap treat `requires`
    // on these flags as satisfied during export, so check it here.
    if !cli.inspect && (cli.runtime.is_some() || cli.node.is_some()) {
        Cli::command()
            .error(
                ErrorKind::MissingRequiredArgument,
                "--runtime and --node require --inspect",
            )
            .exit()
    }
    if cli.inspect {
        let runtime = default_react_runtime();
        return match inspect::run(
            &cli.source,
            cli.node.as_deref().unwrap_or(&runtime.node),
            cli.runtime.as_deref().unwrap_or(&runtime.cli_script),
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Celesta inspection: {error}");
                ExitCode::FAILURE
            }
        };
    }
    let png = png_output(&cli).unwrap_or_else(|error| {
        Cli::command()
            .error(ErrorKind::ArgumentConflict, error)
            .exit()
    });
    let range = export_range(cli.from, cli.to).unwrap_or_else(|error| {
        Cli::command()
            .error(ErrorKind::ValueValidation, error)
            .exit()
    });
    match run(cli, png, range) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Celesta export: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Whether `cli` asks for PNG stills rather than a video, checking the
/// combinations of selection and format flags clap cannot express.
fn png_output(cli: &Cli) -> Result<bool, &'static str> {
    let selected = !cli.frames.is_empty() || cli.every.is_some();
    if (selected || cli.contact_sheet) && cli.output_format == Some(OutputFormat::Mp4) {
        return Err("--frame/--frames/--every/--contact-sheet require PNG output");
    }
    let png = selected || cli.contact_sheet || cli.output_format == Some(OutputFormat::Png);
    if png && !selected {
        return Err("PNG output requires --frame, --frames or --every");
    }
    Ok(png)
}

fn run(cli: Cli, png: bool, range: Option<ExportRange>) -> Result<(), String> {
    let Cli {
        source,
        output,
        react,
        project: companion_project_path,
        overwrite,
        frames,
        every,
        contact_sheet,
        columns,
        tile_width,
        preset,
        crf,
        color_conversion,
        render_quality,
        no_ui,
        ..
    } = cli;
    let output = output.ok_or("missing output path")?;
    let video = VideoEncoding {
        preset,
        crf,
        color_conversion,
    };
    let png_export = PngExport {
        frames: match every {
            Some(step) => FrameSelection::Every(step.get()),
            None => FrameSelection::Frames(frames),
        },
        contact_sheet: contact_sheet.then_some(ContactSheet {
            columns: columns.get(),
            tile_width: tile_width.get(),
        }),
    };
    let exporter = Exporter::new(ExportOptions {
        overwrite,
        range,
        video,
        render_quality,
    });
    progress::run(
        progress::Settings {
            source: &source,
            output: &output,
            react,
            png,
            video,
            render_quality,
        },
        no_ui,
        |on_progress: &mut dyn FnMut(ExportProgress)| {
            if png {
                let project_path = if react {
                    companion_project_path.as_deref()
                } else {
                    Some(source.as_path())
                };
                let project = project_path
                    .map(Project::load)
                    .transpose()
                    .map_err(|e| e.to_string())?;
                let root = project_path.map(|path| path.parent().unwrap_or_else(|| Path::new(".")));
                if react {
                    exporter
                        .export_react_png_with(
                            &source,
                            &default_react_runtime(),
                            project
                                .as_ref()
                                .zip(root)
                                .map(|(project, project_asset_root)| CompanionProject {
                                    project,
                                    project_asset_root,
                                }),
                            &png_export,
                            &output,
                            on_progress,
                        )
                        .map_err(|e| e.to_string())?;
                } else {
                    exporter
                        .export_project_png_with(
                            project.as_ref().ok_or("missing project")?,
                            root.ok_or("missing asset root")?,
                            &png_export,
                            &output,
                            on_progress,
                        )
                        .map_err(|e| e.to_string())?;
                }
            } else if react {
                let runtime = default_react_runtime();
                match companion_project_path {
                    Some(project_path) => {
                        let project =
                            Project::load(&project_path).map_err(|error| error.to_string())?;
                        let project_asset_root =
                            project_path.parent().unwrap_or_else(|| Path::new("."));
                        exporter
                            .export_react_entry_with_project_and_progress(
                                &source,
                                &runtime,
                                CompanionProject {
                                    project: &project,
                                    project_asset_root,
                                },
                                &output,
                                on_progress,
                            )
                            .map_err(|error| error.to_string())?;
                    }
                    None => {
                        exporter
                            .export_react_entry_with_progress(
                                &source,
                                &runtime,
                                &output,
                                on_progress,
                            )
                            .map_err(|error| error.to_string())?;
                    }
                }
            } else {
                exporter
                    .export_file_with_progress(&source, &output, on_progress)
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        },
    )?;
    eprintln!("export complete: {}", output.display());
    Ok(())
}

/// Builds an [`ExportRange`] from the `--from` / `--to` timecodes. Returns
/// `None` when neither is given (export the whole composition). `--to` must
/// be strictly after `--from`.
fn export_range(start: Option<Time>, end: Option<Time>) -> Result<Option<ExportRange>, String> {
    if start.is_none() && end.is_none() {
        return Ok(None);
    }

    if let (Some(start), Some(end)) = (start, end)
        && end
            .cmp_exact(start)
            .map_err(|error| error.to_string())?
            .is_le()
    {
        return Err("--to must be after --from".into());
    }

    Ok(Some(ExportRange {
        start: start.unwrap_or(Time::ZERO),
        end,
    }))
}

fn default_react_runtime() -> ReactRuntimeOptions {
    let (node, cli_script) = celesta_react_bridge::runtime_paths();
    ReactRuntimeOptions::new(node, cli_script)
}
