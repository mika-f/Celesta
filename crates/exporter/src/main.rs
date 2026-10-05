use std::num::{NonZeroU32, NonZeroU64};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, ValueEnum};

mod progress;
mod report;

use celesta_composition::Time;
use celesta_exporter::{
    ColorConversion, CompanionProject, ContactSheet, EncoderPreset, ExportError, ExportOptions,
    ExportProgress, ExportRange, Exporter, FrameSelection, PngExport, ReactRuntimeOptions,
    RenderQuality, VideoEncoding, parse_timecode,
};
use celesta_project::Project;

const AFTER_HELP: &str = "\
Timecodes are HH:MM:SS(.mmm), MM:SS(.mmm) or SS(.mmm); --from/--to select a
span of the composition to export (the output starts at its own 00:00).

PNG output is selected by --frame/--frames or --every. Multiple frames use
output-000090.png names; frame numbers are zero-based. --every <n> picks
frames 0, n, 2n, ... of the --from/--to span (or the whole composition) plus
its last frame. --contact-sheet writes the selection as one labelled grid
image instead.

--json prints one line of JSON on stdout when the export ends, and nothing
else: status, the composition's size, fps and frame count, every written
file with its frame numbers, warnings, and on failure an error code and
message. It suits scripts and AI agents.";

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
    output: PathBuf,
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
    /// Print only a JSON result on stdout, with no progress output.
    #[arg(long)]
    json: bool,
}

impl Cli {
    fn video(&self) -> VideoEncoding {
        VideoEncoding {
            preset: self.preset,
            crf: self.crf,
            color_conversion: self.color_conversion,
        }
    }
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
    let cli = Cli::try_parse().unwrap_or_else(|error| {
        // Help and version requests are not failures.
        if error.use_stderr() && std::env::args_os().any(|arg| arg == "--json") {
            report::usage_error(&error.to_string());
            std::process::exit(error.exit_code());
        }
        error.exit()
    });
    let png = png_output(&cli)
        .unwrap_or_else(|error| usage_error(cli.json, ErrorKind::ArgumentConflict, error));
    let range = export_range(cli.from, cli.to)
        .unwrap_or_else(|error| usage_error(cli.json, ErrorKind::ValueValidation, &error));
    let export =
        |on_progress: &mut dyn FnMut(ExportProgress)| export(&cli, png, range, on_progress);
    if cli.json {
        return if report::run(&cli.source, export) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let settings = progress::Settings {
        source: &cli.source,
        output: &cli.output,
        react: cli.react,
        png,
        video: cli.video(),
        render_quality: cli.render_quality,
    };
    match progress::run(settings, cli.no_ui, export) {
        Ok(()) => {
            eprintln!("export complete: {}", cli.output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Celesta export: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Exits with a usage error, as JSON with `--json`.
fn usage_error(json: bool, kind: ErrorKind, message: &str) -> ! {
    let error = Cli::command().error(kind, message);
    if json {
        report::usage_error(message);
        std::process::exit(error.exit_code());
    }
    error.exit()
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

fn export(
    cli: &Cli,
    png: bool,
    range: Option<ExportRange>,
    on_progress: &mut dyn FnMut(ExportProgress),
) -> Result<(), ExportError> {
    let exporter = Exporter::new(ExportOptions {
        overwrite: cli.overwrite,
        range,
        video: cli.video(),
        render_quality: cli.render_quality,
    });
    let load = |path: &Path| Project::load(path).map_err(ExportError::Project);
    let companion = cli
        .project
        .as_deref()
        .map(|path| load(path).map(|project| (project, path)))
        .transpose()?;
    let companion = companion.as_ref().map(|(project, path)| CompanionProject {
        project,
        project_asset_root: asset_root(path),
    });
    if png {
        let png_export = PngExport {
            frames: match cli.every {
                Some(step) => FrameSelection::Every(step.get()),
                None => FrameSelection::Frames(cli.frames.clone()),
            },
            contact_sheet: cli.contact_sheet.then_some(ContactSheet {
                columns: cli.columns.get(),
                tile_width: cli.tile_width.get(),
            }),
        };
        if cli.react {
            exporter.export_react_png_with(
                &cli.source,
                &default_react_runtime(),
                companion,
                &png_export,
                &cli.output,
                on_progress,
            )
        } else {
            exporter.export_project_png_with(
                &load(&cli.source)?,
                asset_root(&cli.source),
                &png_export,
                &cli.output,
                on_progress,
            )
        }
    } else if cli.react {
        let runtime = default_react_runtime();
        match companion {
            Some(companion) => exporter.export_react_entry_with_project_and_progress(
                &cli.source,
                &runtime,
                companion,
                &cli.output,
                on_progress,
            ),
            None => exporter.export_react_entry_with_progress(
                &cli.source,
                &runtime,
                &cli.output,
                on_progress,
            ),
        }
    } else {
        exporter.export_file_with_progress(&cli.source, &cli.output, on_progress)
    }
}

/// Where a project's relative asset paths resolve from.
fn asset_root(project_path: &Path) -> &Path {
    project_path.parent().unwrap_or_else(|| Path::new("."))
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
