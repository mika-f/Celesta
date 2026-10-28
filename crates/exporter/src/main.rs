use std::num::{NonZeroU32, NonZeroU64};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser, ValueEnum};

mod inspect;
mod json;
mod progress;

use celesta_composition::Time;
use celesta_exporter::{
    AudioLoudness, BitDepth, ColorConversion, ContactSheet, EncoderPreset, ExportCancellation,
    ExportJob, ExportOptions, ExportProgress, ExportRange, ExportedFile, FrameSelection, GpuDriver,
    LoudnessTarget, PeakHandling, ReactRuntimeOptions, RenderQuality, StillExport, StillFormat,
    VideoEncoding, VideoFormat, parse_timecode,
};
use celesta_react_bridge::{PropsFileChoice, resolve_props_file};

const AFTER_HELP: &str = "\
Timecodes are HH:MM:SS(.mmm), MM:SS(.mmm) or SS(.mmm); --from/--to select a
span of the composition to export (the output starts at its own 00:00).

--subtitles writes a video's subtitles beside it as SRT (.srt) or WebVTT
(.vtt): a React entry's <Subtitles> cues and <Dialogue> lines, or a
project's dialogue items, each for as long as it is shown. With --from/--to
the times start at the span's start, and cues are cut to the span. Repeat
it to write several files. A {lang} in the path writes one file per
language (<Subtitles lang>, else the enclosing lang; und without one), as
in --subtitles 'film.{lang}.vtt'; a path without it takes one language.
Without <OUTPUT>, only the subtitle files are written: the frames are
evaluated but not rendered or encoded.

Still images are selected by --frame/--frames or --every and written as PNG
(.png), JPEG (.jpg, .jpeg) or lossy WebP (.webp) by the output's extension;
--quality sets JPEG and WebP quality, 1-100 (default 90). Stills are drawn
over the export's opaque background, as an MP4 is, so JPEG's missing alpha
channel loses nothing. Multiple frames use output-000090.png names with the
output's extension; frame numbers are zero-based. --every <n> picks frames
0, n, 2n, ... of the --from/--to span (or the whole composition) plus its
last frame. --contact-sheet writes the selection as one labelled grid image
instead; a frame that fails to evaluate or render becomes a FAILED tile, the
sheet is still written, and the export then fails naming each such frame.
--debug-guides draws a React entry's <DebugOverlay> and
<DebugBounds> into the stills, as the editor preview does.

--props and --props-file set the values an entry declares with
defineProjectProperties(). Highest precedence first: --props, --props-file,
the --project file's properties, the declared defaults. Every value is
checked against the declaration before anything renders.

A .mov output is ProRes 4444 with 24-bit PCM audio and a .webm output VP9
with Opus; both keep transparency where nothing is drawn (straight alpha,
BT.709). --preset, --crf, --bit-depth and --color-conversion apply to MP4 only.

--loop is for a video played in a loop, as Shorts and TikTok play one: the
MP4's AAC encoder starts from the end of the soundtrack instead of silence,
so the first hit is not smeared on every pass. The decoded sound is still
exactly as long as the video. It applies to MP4 only; a .mov's PCM already
runs from its end into its start unchanged.

--loudness brings the whole mix to an integrated loudness (ITU-R BS.1770,
as FFmpeg's ebur128 filter measures it): a number of LUFS from -70 to -5,
streaming (-14, as YouTube and Spotify play) or broadcast (-23, EBU R128).
One gain moves the whole mix; where it would take a true peak over
--true-peak (default -1 dBTP), a limiter holds the peak there, or with
--peak-handling lower the gain stops short of the target instead. Every
video export measures its mix, and --json reports the loudness, the true
peak and the gain. AAC and Opus move the peaks by up to about half a dB.

--json prints one line of JSON on stdout when the export ends, and nothing
else: status, the composition's size, fps and frame count, every written
file with its frame numbers, warnings, and on failure an error code and
message. It suits scripts and AI agents. --json-progress adds a JSON line
for each progress event before the result, and --stdin-cancel cancels the
export when stdin reads a `cancel` line or closes; @celesta/exporter uses
both. --info prints the composition's size, frame rate and length (and a
React entry's compositions) without rendering.

--inspect evaluates a React entry's frames without rendering pixels, with
the same text shaping and media probing as an export. It reads one JSON
request per stdin line, each a composition time (with openEnd, a time past
the end, evaluated as if the video went on), and answers each with one JSON
line on stdout; skills/celesta/scripts/inspect.mjs --native drives it.";

/// Exports a Celesta project or React composition to MP4, MOV or WebM video or
/// PNG, JPEG or WebP stills.
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
    /// The .mp4, .mov (ProRes 4444 with alpha) or .webm (VP9 with alpha) file to
    /// write, or the .png, .jpg/.jpeg or .webp file for frame exports. Leave
    /// it out to write only the --subtitles files.
    #[arg(required_unless_present_any = ["inspect", "info", "subtitles"])]
    output: Option<PathBuf>,
    /// Evaluate frames requested on stdin as JSON lines instead of exporting.
    #[arg(
        long,
        requires = "react",
        conflicts_with_all = [
            "output", "project", "overwrite", "from", "to", "frames", "every",
            "contact_sheet", "columns", "tile_width", "output_format", "quality", "preset",
            "crf", "color_conversion", "bit_depth", "render_quality", "driver", "no_ui",
            "json", "debug_guides", "subtitles", "looping", "loudness", "true_peak",
            "peak_handling", "info", "stdin_cancel",
        ]
    )]
    inspect: bool,
    /// Print the composition's size, frame rate and length, and a React
    /// entry's compositions, without rendering anything.
    #[arg(
        long,
        conflicts_with_all = [
            "output", "overwrite", "from", "to", "frames", "every", "contact_sheet",
            "columns", "tile_width", "output_format", "quality", "preset", "crf",
            "color_conversion", "bit_depth", "render_quality", "driver", "debug_guides",
            "subtitles", "looping", "loudness", "true_peak", "peak_handling",
        ]
    )]
    info: bool,
    /// The React runtime's cli.js for --inspect (default: the bundled one).
    #[arg(long, value_name = "CLI_JS")]
    runtime: Option<PathBuf>,
    /// The Node.js executable for --inspect (default: the bundled one).
    #[arg(long, value_name = "NODE")]
    node: Option<PathBuf>,
    /// Export a React composition instead of a JSON project.
    #[arg(long)]
    react: bool,
    /// Root composition ID; defaults to the first declared composition.
    #[arg(long, value_name = "ID", requires = "react")]
    composition: Option<String>,
    /// Companion JSON project for <ProjectTimeline />/<ProjectTrack />.
    #[arg(long, value_name = "PROJECT", requires = "react")]
    project: Option<PathBuf>,
    /// Project property values as a JSON object; wins over --props-file.
    #[arg(long, value_name = "JSON", requires = "react")]
    props: Option<String>,
    /// JSON file of project property values; relative paths in it resolve
    /// from the file's directory.
    #[arg(long, value_name = "FILE", requires = "react")]
    props_file: Option<PathBuf>,
    /// Replace an existing output file.
    #[arg(long)]
    overwrite: bool,
    /// Export from this timecode.
    #[arg(long, value_name = "TIMECODE", value_parser = parse_timecode)]
    from: Option<Time>,
    /// Export up to (excluding) this timecode.
    #[arg(long, value_name = "TIMECODE", value_parser = parse_timecode)]
    to: Option<Time>,
    /// Also write the video's subtitles to this .srt or .vtt file; repeatable.
    /// {lang} in the path writes one file per language.
    #[arg(long, value_name = "FILE")]
    subtitles: Vec<PathBuf>,
    /// Export these zero-based frames as stills; repeatable and comma-separated.
    #[arg(
        long = "frame",
        visible_alias = "frames",
        value_name = "N,...",
        value_delimiter = ',',
        conflicts_with_all = ["every", "from", "to"]
    )]
    frames: Vec<u64>,
    /// Export every n-th frame (and the last one) as stills.
    #[arg(long, value_name = "N")]
    every: Option<NonZeroU64>,
    /// Write the selected frames as one labelled grid image.
    #[arg(long)]
    contact_sheet: bool,
    /// JPEG or WebP quality, 1-100; higher is sharper and larger [default: 90].
    #[arg(
        long,
        value_name = "1-100",
        value_parser = clap::value_parser!(u8).range(1..=i64::from(StillExport::MAX_QUALITY))
    )]
    quality: Option<u8>,
    /// Draw the entry's debug guides (<DebugOverlay>, <DebugBounds>) into
    /// still frames and contact sheets; never into a video.
    #[arg(long, requires = "react")]
    debug_guides: bool,
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
    /// Output format; the output's extension implies it, and must match it when
    /// it is a known one. A still format requires a frame selection.
    #[arg(long, value_enum)]
    output_format: Option<OutputFormat>,
    /// libx264 preset: faster presets encode faster but produce larger files.
    #[arg(
        long,
        default_value_t = EncoderPreset::default(),
        value_enum
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
        value_enum
    )]
    color_conversion: ColorConversion,
    /// Bits per sample of the MP4: 10 encodes yuv420p10le (H.264 High 10),
    /// which keeps dark gradients smooth but plays on fewer devices.
    #[arg(
        long,
        value_name = "BITS",
        default_value_t = BitDepth::default(),
        value_enum
    )]
    bit_depth: BitDepth,
    /// The MP4 plays in a loop: encode its sound to run from its end into
    /// its start.
    #[arg(long = "loop")]
    looping: bool,
    /// Bring the mix to this integrated loudness: LUFS (-70 to -5),
    /// streaming (-14) or broadcast (-23).
    #[arg(
        long,
        value_name = "LUFS",
        allow_negative_numbers = true,
        value_parser = parse_loudness
    )]
    loudness: Option<f64>,
    /// The highest true peak --loudness may leave, -9 to 0 [default: -1].
    #[arg(
        long,
        value_name = "DBTP",
        allow_negative_numbers = true,
        requires = "loudness",
        value_parser = parse_true_peak
    )]
    true_peak: Option<f64>,
    /// What --loudness does where its gain would take a true peak over
    /// --true-peak: limit the peak, or lower the gain [default: limit].
    #[arg(long, value_name = "HOW", requires = "loudness", value_enum)]
    peak_handling: Option<PeakHandling>,
    /// How carefully scaled and rotated layers are drawn; draft skips
    /// re-rasterizing scaled text and is meant for quick timing checks.
    #[arg(
        long,
        value_name = "QUALITY",
        default_value_t = RenderQuality::default(),
        value_enum
    )]
    render_quality: RenderQuality,
    /// Graphics API to render with; auto takes the first one with a GPU.
    #[arg(
        long,
        default_value_t = GpuDriver::default(),
        value_enum
    )]
    driver: GpuDriver,
    /// Disable the terminal progress dashboard (automatic outside a terminal).
    #[arg(long)]
    no_ui: bool,
    /// Print only a JSON result on stdout, with no progress output.
    #[arg(long)]
    json: bool,
    /// With --json, also print each progress event as a JSON line on stdout
    /// before the result.
    #[arg(long, requires = "json")]
    json_progress: bool,
    /// Cancel the export when stdin reads a `cancel` line or closes; what was
    /// being written is removed, as on any failure.
    #[arg(long)]
    stdin_cancel: bool,
}

impl Cli {
    /// The video or PNG output; `None` writes only subtitles.
    fn output(&self) -> Option<&Path> {
        self.output.as_deref()
    }

    fn loudness(&self) -> Option<LoudnessTarget> {
        let target = LoudnessTarget::new(self.loudness?);
        Some(LoudnessTarget {
            true_peak_dbtp: self.true_peak.unwrap_or(target.true_peak_dbtp),
            peak_handling: self.peak_handling.unwrap_or(target.peak_handling),
            ..target
        })
    }

    fn video(&self) -> VideoEncoding {
        VideoEncoding {
            preset: self.preset,
            crf: self.crf,
            color_conversion: self.color_conversion,
            bit_depth: self.bit_depth,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Png,
    Jpeg,
    Webp,
    Mp4,
    Mov,
    Webm,
}

impl OutputFormat {
    /// The video format this names, or `None` for a still format.
    fn video(self) -> Option<VideoFormat> {
        match self {
            Self::Png | Self::Jpeg | Self::Webp => None,
            Self::Mp4 => Some(VideoFormat::Mp4),
            Self::Mov => Some(VideoFormat::Mov),
            Self::Webm => Some(VideoFormat::Webm),
        }
    }

    /// The still format this names, or `None` for a video format.
    fn still(self) -> Option<StillFormat> {
        match self {
            Self::Png => Some(StillFormat::Png),
            Self::Jpeg => Some(StillFormat::Jpeg),
            Self::Webp => Some(StillFormat::Webp),
            Self::Mp4 | Self::Mov | Self::Webm => None,
        }
    }
}

/// `--loudness`: a number of LUFS in [`LoudnessTarget::INTEGRATED_RANGE`],
/// or a preset's name.
fn parse_loudness(value: &str) -> Result<f64, String> {
    match value {
        "streaming" => Ok(LoudnessTarget::STREAMING.integrated_lufs),
        "broadcast" => Ok(LoudnessTarget::BROADCAST.integrated_lufs),
        _ => decibels(value, LoudnessTarget::INTEGRATED_RANGE, "LUFS")
            .map_err(|error| format!("{error}, streaming or broadcast")),
    }
}

/// `--true-peak`: dBTP in [`LoudnessTarget::TRUE_PEAK_RANGE`].
fn parse_true_peak(value: &str) -> Result<f64, String> {
    decibels(value, LoudnessTarget::TRUE_PEAK_RANGE, "dBTP")
}

fn decibels(value: &str, (low, high): (f64, f64), unit: &str) -> Result<f64, String> {
    value
        .parse::<f64>()
        .ok()
        .filter(|value| (low..=high).contains(value))
        .ok_or_else(|| format!("expected {low} to {high} {unit}"))
}

/// How the human-readable output reports an exported mix's loudness.
fn loudness_line(loudness: &AudioLoudness) -> String {
    let mut line = match loudness.integrated_lufs {
        Some(lufs) => format!("loudness: {lufs:.1} LUFS"),
        None => "loudness: none (silent or under 0.4 s)".to_owned(),
    };
    if let Some(peak) = loudness.true_peak_dbtp {
        line.push_str(&format!(", true peak {peak:.1} dBTP"));
    }
    if let Some(gain) = loudness.gain_db {
        line.push_str(&format!(", gain {gain:+.1} dB"));
    }
    line
}

fn non_zero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("contact sheet defaults are positive")
}

fn main() -> ExitCode {
    let matches = Cli::command()
        .try_get_matches()
        .unwrap_or_else(|error| parse_failed(error));
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| parse_failed(error));
    if cli.inspect {
        return inspect(&cli);
    }
    // clap counts `requires = "inspect"` as met once <OUTPUT>, which
    // conflicts with --inspect, is given, so check it here.
    if cli.runtime.is_some() || cli.node.is_some() {
        usage_error(
            cli.json,
            ErrorKind::MissingRequiredArgument,
            "--runtime and --node require --inspect",
        );
    }
    let cancellation = ExportCancellation::default();
    if cli.stdin_cancel {
        cancel_on_stdin(cancellation.clone());
    }
    // JSON projects carry their own properties, so only React exports have a props file.
    let props_file = cli
        .react
        .then(|| resolve_props_file(&cli.source, cli.props_file.as_deref()))
        .flatten();
    if cli.info {
        return info(&cli, props_file);
    }
    let explicit = |id: &str| matches.value_source(id) == Some(ValueSource::CommandLine);
    let format = output_choice(&cli, &explicit)
        .unwrap_or_else(|error| usage_error(cli.json, ErrorKind::ArgumentConflict, &error));
    let range = export_range(cli.from, cli.to)
        .unwrap_or_else(|error| usage_error(cli.json, ErrorKind::ValueValidation, &error));
    let export = |on_progress: &mut dyn FnMut(ExportProgress)| {
        job(&cli, format, range).run(&cancellation, on_progress)
    };
    if cli.json {
        return if json::run(&cli.source, props_file, cli.json_progress, export) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let result = match (cli.output(), format) {
        (Some(output), OutputChoice::Video(_) | OutputChoice::Stills) => {
            let settings = progress::Settings {
                source: &cli.source,
                output,
                react: cli.react,
                still: (format == OutputChoice::Stills)
                    .then(|| StillFormat::from_extension(output).unwrap_or_default()),
                still_quality: cli.quality.unwrap_or(StillExport::DEFAULT_QUALITY),
                format: format.video().unwrap_or_default(),
                video: cli.video(),
                render_quality: cli.render_quality,
                props: props_file.as_ref().map(props_label),
            };
            let mut loudness = None;
            progress::run(settings, cli.no_ui, |on_progress| {
                export(&mut |event| {
                    if let ExportProgress::Wrote(ExportedFile::Video {
                        loudness: Some(measured),
                        ..
                    }) = &event
                    {
                        loudness = Some(*measured);
                    }
                    on_progress(event);
                })
            })
            .map(|()| {
                eprintln!("export complete: {}", output.display());
                if let Some(loudness) = &loudness {
                    eprintln!("{}", loudness_line(loudness));
                }
            })
        }
        // There is no picture to show progress for, so no dashboard either.
        _ => progress::run_plain(|on_progress| {
            export(&mut |event| {
                if let ExportProgress::Wrote(ExportedFile::Subtitles { path, cues, .. }) = &event {
                    eprintln!("wrote {} ({cues} cues)", path.display());
                }
                on_progress(event);
            })
        })
        .map(|()| eprintln!("export complete")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Celesta export: {error}");
            ExitCode::FAILURE
        }
    }
}

/// `--info`: reports the composition, as JSON with `--json`.
fn info(cli: &Cli, props_file: Option<PropsFileChoice>) -> ExitCode {
    let job = job(cli, OutputChoice::SubtitlesOnly, None);
    if cli.json {
        let ok = json::run(&cli.source, props_file, cli.json_progress, |on_progress| {
            job.describe(on_progress)
        });
        return if ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let result = job.describe(&mut |event| match event {
        ExportProgress::Composition(info) => {
            let rate = info.frame_rate;
            let fps = f64::from(rate.numerator) / f64::from(rate.denominator);
            let seconds = info.frames as f64 / fps;
            println!("size: {}x{}", info.width, info.height);
            if rate.denominator == 1 {
                println!("fps: {}", rate.numerator);
            } else {
                println!("fps: {}/{} ({fps:.3})", rate.numerator, rate.denominator);
            }
            println!("frames: {}", info.frames);
            println!("duration: {seconds:.3}s");
        }
        ExportProgress::ReactCompositions {
            selected,
            compositions,
        } => {
            let ids: Vec<_> = compositions
                .iter()
                .map(|root| {
                    if root.id == selected {
                        format!("{} (selected)", root.id)
                    } else {
                        root.id.clone()
                    }
                })
                .collect();
            println!("compositions: {}", ids.join(", "));
        }
        _ => {}
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Celesta export: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Cancels `cancellation` once stdin reads a `cancel` line, closes, or fails.
fn cancel_on_stdin(cancellation: ExportCancellation) {
    std::thread::spawn(move || {
        for line in std::io::stdin().lines() {
            match line {
                Ok(line) if line.trim() == "cancel" => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        cancellation.cancel();
    });
}

fn inspect(cli: &Cli) -> ExitCode {
    let result = match ReactRuntimeOptions::for_entry(
        &cli.source,
        cli.props_file.as_deref(),
        cli.props.as_deref(),
    ) {
        Ok(runtime) => inspect::run(
            &cli.source,
            &ReactRuntimeOptions {
                node: cli.node.clone().unwrap_or(runtime.node),
                cli_script: cli.runtime.clone().unwrap_or(runtime.cli_script),
                properties: runtime.properties,
                composition_id: cli.composition.clone(),
            },
        ),
        Err(error) => inspect::startup_failed(&error.to_string()),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("Celesta inspection: {error}");
            ExitCode::FAILURE
        }
    }
}

/// How the export profile names the property file: `props.json (auto)` for a
/// discovered one, else the `--props-file` name.
fn props_label(choice: &PropsFileChoice) -> String {
    if choice.auto {
        format!("{} (auto)", celesta_react_bridge::ENTRY_PROPS_FILE)
    } else {
        choice
            .path
            .file_name()
            .unwrap_or(choice.path.as_os_str())
            .to_string_lossy()
            .into_owned()
    }
}

/// Exits on a command line clap rejected; help and version requests are not failures.
fn parse_failed(error: clap::Error) -> ! {
    if error.use_stderr() && std::env::args_os().any(|arg| arg == "--json") {
        json::usage_error(&error.to_string());
        std::process::exit(error.exit_code());
    }
    error.exit()
}

/// Exits with a usage error, as JSON with `--json`.
fn usage_error(json: bool, kind: ErrorKind, message: &str) -> ! {
    let error = Cli::command().error(kind, message);
    if json {
        json::usage_error(message);
        std::process::exit(error.exit_code());
    }
    error.exit()
}

/// What an export writes besides any subtitle files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputChoice {
    Video(VideoFormat),
    Stills,
    /// Nothing: only the subtitle files.
    SubtitlesOnly,
}

impl OutputChoice {
    const fn video(self) -> Option<VideoFormat> {
        match self {
            Self::Video(format) => Some(format),
            Self::Stills | Self::SubtitlesOnly => None,
        }
    }
}

/// What `cli` exports, checking the combinations of selection, format and
/// encoder flags clap cannot express. `explicit` tells whether the argument
/// with that id was given on the command line.
fn output_choice(cli: &Cli, explicit: &dyn Fn(&str) -> bool) -> Result<OutputChoice, String> {
    let Some(output) = cli.output() else {
        return subtitles_only_choice(explicit);
    };
    let selected = !cli.frames.is_empty() || cli.every.is_some();
    let named = cli.output_format.and_then(OutputFormat::video);
    let named_still = cli.output_format.and_then(OutputFormat::still);
    if (selected || cli.contact_sheet) && named.is_some() {
        return Err(
            "--frame/--frames/--every/--contact-sheet require still output (PNG, JPEG or WebP)"
                .into(),
        );
    }
    let stills = selected || cli.contact_sheet || named_still.is_some();
    if stills && !selected {
        return Err("still output requires --frame, --frames or --every".into());
    }
    if stills {
        if !cli.subtitles.is_empty() {
            return Err("--subtitles applies to video output, not still frames".into());
        }
        if cli.bit_depth != BitDepth::Eight {
            return Err("--bit-depth applies to MP4 output; stills are 8-bit".into());
        }
        if cli.looping {
            return Err("--loop applies to MP4 output, not still frames".into());
        }
        if cli.loudness.is_some() {
            return Err("--loudness applies to video output, not still frames".into());
        }
        // An output with no still extension is left to the exporter, which
        // reports it as `invalid_selection`.
        let format = StillFormat::from_extension(output);
        if let (Some(named), Some(format)) = (named_still, format)
            && named != format
        {
            let extension = output.extension().unwrap_or_default();
            return Err(format!(
                "--output-format {named} does not match the .{} output",
                extension.to_string_lossy()
            ));
        }
        if cli.quality.is_some() && format == Some(StillFormat::Png) {
            return Err("--quality applies to JPEG and WebP output; PNG is lossless".into());
        }
        return Ok(OutputChoice::Stills);
    }
    if cli.quality.is_some() {
        return Err("--quality applies to JPEG and WebP stills, not video".into());
    }
    if cli.debug_guides {
        return Err(
            "--debug-guides applies to still output; video exports never draw debug guides".into(),
        );
    }
    // An output with no video extension is left to the exporter, which reports
    // it as `unsupported_output`; only a video extension that contradicts
    // `--output-format` is a usage error.
    let format = match VideoFormat::from_extension(output) {
        Some(format) => {
            if let Some(named) = named.filter(|&named| named != format) {
                return Err(format!(
                    "--output-format {named} does not match the .{format} output"
                ));
            }
            format
        }
        None => named.unwrap_or_default(),
    };
    if format.has_alpha() {
        if cli.looping {
            return Err(format!(
                "--loop applies to MP4 (AAC) output; .{format} output does not use it"
            ));
        }
        for (id, flag) in [
            ("preset", "--preset"),
            ("crf", "--crf"),
            ("bit_depth", "--bit-depth"),
            ("color_conversion", "--color-conversion"),
        ] {
            if explicit(id) {
                return Err(format!(
                    "{flag} applies to H.264 (MP4) output; .{format} output does not use it"
                ));
            }
        }
    }
    Ok(OutputChoice::Video(format))
}

/// Checks a command line with no <OUTPUT>, which clap allows only with
/// --subtitles: flags about rendering or encoding a picture have nothing to
/// apply to.
fn subtitles_only_choice(explicit: &dyn Fn(&str) -> bool) -> Result<OutputChoice, String> {
    for (id, flag) in [
        ("frames", "--frame"),
        ("every", "--every"),
        ("contact_sheet", "--contact-sheet"),
        ("debug_guides", "--debug-guides"),
        ("output_format", "--output-format"),
        ("quality", "--quality"),
        ("preset", "--preset"),
        ("crf", "--crf"),
        ("bit_depth", "--bit-depth"),
        ("color_conversion", "--color-conversion"),
        ("looping", "--loop"),
        ("loudness", "--loudness"),
        ("render_quality", "--render-quality"),
        ("driver", "--driver"),
    ] {
        if explicit(id) {
            return Err(format!(
                "{flag} needs an <OUTPUT>; without one only the --subtitles files are written"
            ));
        }
    }
    Ok(OutputChoice::SubtitlesOnly)
}

/// The export `cli` describes.
fn job(cli: &Cli, format: OutputChoice, range: Option<ExportRange>) -> ExportJob {
    ExportJob {
        source: cli.source.clone(),
        react: cli.react,
        composition_id: cli.composition.clone(),
        companion: cli.project.clone(),
        props_file: cli.props_file.clone(),
        props: cli.props.clone(),
        output: cli.output.clone(),
        stills: (format == OutputChoice::Stills).then(|| StillExport {
            frames: match cli.every {
                Some(step) => FrameSelection::Every(step.get()),
                None => FrameSelection::Frames(cli.frames.clone()),
            },
            contact_sheet: cli.contact_sheet.then_some(ContactSheet {
                columns: cli.columns.get(),
                tile_width: cli.tile_width.get(),
            }),
            debug_guides: cli.debug_guides,
            quality: cli.quality,
        }),
        options: ExportOptions {
            overwrite: cli.overwrite,
            range,
            video_format: format.video().unwrap_or_default(),
            video: cli.video(),
            render_quality: cli.render_quality,
            driver: cli.driver,
            subtitles: cli.subtitles.clone(),
            looping: cli.looping,
            loudness: cli.loudness(),
        },
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `--help` lists, and clap accepts, the names the library parses.
    #[test]
    fn option_values_take_the_library_names() {
        fn names_match<T: ValueEnum + Copy + Into<&'static str>>() {
            for value in T::value_variants() {
                let name = value.to_possible_value().unwrap();
                assert_eq!(name.get_name(), (*value).into());
            }
        }
        names_match::<EncoderPreset>();
        names_match::<ColorConversion>();
        names_match::<BitDepth>();
        names_match::<PeakHandling>();
        names_match::<RenderQuality>();
        names_match::<GpuDriver>();
    }

    fn choice(args: &[&str]) -> Result<OutputChoice, String> {
        let matches = Cli::command()
            .try_get_matches_from(
                ["celesta-exporter", "project.celesta.json"]
                    .into_iter()
                    .chain(args.iter().copied()),
            )
            .unwrap();
        let cli = Cli::from_arg_matches(&matches).unwrap();
        output_choice(&cli, &|id| {
            matches.value_source(id) == Some(ValueSource::CommandLine)
        })
    }

    fn error(args: &[&str]) -> String {
        choice(args).expect_err("the arguments should be rejected")
    }

    #[test]
    fn the_output_extension_selects_the_video_format() {
        assert_eq!(
            choice(&["out.mov"]),
            Ok(OutputChoice::Video(VideoFormat::Mov))
        );
        assert_eq!(
            choice(&["out.WEBM"]),
            Ok(OutputChoice::Video(VideoFormat::Webm))
        );
        assert_eq!(
            choice(&["out.mp4"]),
            Ok(OutputChoice::Video(VideoFormat::Mp4))
        );
        assert_eq!(
            choice(&["--output-format", "webm", "out.webm"]),
            Ok(OutputChoice::Video(VideoFormat::Webm))
        );
    }

    #[test]
    fn an_output_format_must_match_the_extension() {
        assert!(error(&["--output-format", "mp4", "out.mov"]).contains("does not match"));
    }

    #[test]
    fn an_unknown_extension_is_left_to_the_exporter() {
        // The library reports it as `unsupported_output`, with its PNG hint.
        assert_eq!(
            choice(&["out.gif"]),
            Ok(OutputChoice::Video(VideoFormat::Mp4))
        );
        assert_eq!(
            choice(&["out.png"]),
            Ok(OutputChoice::Video(VideoFormat::Mp4))
        );
        assert_eq!(
            choice(&["--output-format", "webm", "out.gif"]),
            Ok(OutputChoice::Video(VideoFormat::Webm))
        );
    }

    #[test]
    fn h264_flags_are_rejected_for_alpha_formats() {
        for (args, flag) in [
            (["out.mov", "--crf", "20"], "--crf"),
            (["out.webm", "--preset", "fast"], "--preset"),
            (["out.mov", "--bit-depth", "10"], "--bit-depth"),
            (
                ["out.webm", "--color-conversion", "gpu"],
                "--color-conversion",
            ),
        ] {
            let message = error(&args);
            assert!(message.contains(flag), "{message}");
            assert!(message.contains("H.264"), "{message}");
        }
        assert_eq!(
            choice(&["out.mp4", "--crf", "20"]),
            Ok(OutputChoice::Video(VideoFormat::Mp4))
        );
    }

    #[test]
    fn video_formats_conflict_with_frame_selections() {
        for args in [
            ["--output-format", "webm", "--frame", "0", "out.png"].as_slice(),
            &[
                "--output-format",
                "mov",
                "--contact-sheet",
                "--every",
                "5",
                "out.png",
            ],
        ] {
            assert!(error(args).contains("require still output"));
        }
    }

    #[test]
    fn still_selections_keep_their_rules() {
        assert_eq!(
            choice(&["--frame", "0", "out.png"]),
            Ok(OutputChoice::Stills)
        );
        assert!(error(&["--output-format", "png", "out.png"]).contains("requires --frame"));
        assert!(error(&["--output-format", "jpeg", "out.jpg"]).contains("requires --frame"));
        assert!(error(&["--frame", "0", "--bit-depth", "10", "out.png"]).contains("--bit-depth"));
        assert!(error(&["--frame", "0", "--bit-depth", "10", "out.jpg"]).contains("--bit-depth"));
    }

    #[test]
    fn the_output_extension_selects_the_still_format() {
        for output in ["out.jpg", "out.JPEG", "out.webp"] {
            assert_eq!(
                choice(&["--frame", "0", output]),
                Ok(OutputChoice::Stills),
                "{output}"
            );
        }
        assert_eq!(
            choice(&["--output-format", "jpeg", "--frame", "0", "out.jpg"]),
            Ok(OutputChoice::Stills)
        );
        assert_eq!(
            choice(&["--output-format", "webp", "--every", "2", "out.WebP"]),
            Ok(OutputChoice::Stills)
        );
        let message = error(&["--output-format", "png", "--frame", "0", "out.webp"]);
        assert!(
            message.contains("--output-format png does not match the .webp output"),
            "{message}"
        );
        // An unknown extension is left to the exporter, as for video.
        assert_eq!(
            choice(&["--frame", "0", "out.gif"]),
            Ok(OutputChoice::Stills)
        );
    }

    #[test]
    fn quality_applies_to_jpeg_and_webp_stills_only() {
        assert_eq!(
            choice(&["--frame", "0", "--quality", "70", "out.jpg"]),
            Ok(OutputChoice::Stills)
        );
        assert_eq!(
            choice(&["--every", "3", "--quality", "1", "out.webp"]),
            Ok(OutputChoice::Stills)
        );
        let message = error(&["--frame", "0", "--quality", "70", "out.png"]);
        assert!(message.contains("PNG is lossless"), "{message}");
        for output in ["out.mp4", "out.webm"] {
            let message = error(&["--quality", "70", output]);
            assert!(
                message.contains("--quality applies to JPEG and WebP stills"),
                "{message}"
            );
        }
    }

    #[test]
    fn loop_applies_to_mp4_only() {
        assert_eq!(
            choice(&["out.mp4", "--loop"]),
            Ok(OutputChoice::Video(VideoFormat::Mp4))
        );
        for args in [
            ["out.mov", "--loop"].as_slice(),
            &["out.webm", "--loop"],
            &["--frame", "0", "--loop", "out.png"],
            &["--subtitles", "a.srt", "--loop"],
        ] {
            let message = error(args);
            assert!(message.contains("--loop"), "{message}");
        }
    }

    fn loudness(args: &[&str]) -> Result<Option<LoudnessTarget>, clap::Error> {
        let matches = Cli::command().try_get_matches_from(
            ["celesta-exporter", "project.celesta.json", "out.mp4"]
                .into_iter()
                .chain(args.iter().copied()),
        )?;
        Ok(Cli::from_arg_matches(&matches)?.loudness())
    }

    #[test]
    fn loudness_takes_lufs_or_a_preset() {
        assert_eq!(loudness(&[]).unwrap(), None);
        assert_eq!(
            loudness(&["--loudness", "-14", "--true-peak", "-1.5"]).unwrap(),
            Some(LoudnessTarget {
                true_peak_dbtp: -1.5,
                ..LoudnessTarget::new(-14.0)
            })
        );
        assert_eq!(
            loudness(&["--loudness=broadcast"]).unwrap(),
            Some(LoudnessTarget::BROADCAST)
        );
        assert_eq!(
            loudness(&["--loudness", "streaming", "--peak-handling", "lower"]).unwrap(),
            Some(LoudnessTarget {
                peak_handling: PeakHandling::Lower,
                ..LoudnessTarget::STREAMING
            })
        );
        for args in [
            ["--loudness", "-3"].as_slice(),
            &["--loudness", "loud"],
            &["--loudness", "-14", "--true-peak", "1"],
            &["--true-peak", "-1"],
            &["--peak-handling", "lower"],
        ] {
            assert!(loudness(args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn loudness_applies_to_video_only() {
        assert_eq!(
            choice(&["out.webm", "--loudness", "-14"]),
            Ok(OutputChoice::Video(VideoFormat::Webm))
        );
        for args in [
            ["--frame", "0", "--loudness", "-14", "out.png"].as_slice(),
            &["--subtitles", "a.srt", "--loudness", "-14"],
        ] {
            let message = error(args);
            assert!(message.contains("--loudness"), "{message}");
        }
    }

    #[test]
    fn subtitles_go_with_video_only() {
        assert_eq!(
            choice(&["out.mp4", "--subtitles", "out.srt"]),
            Ok(OutputChoice::Video(VideoFormat::Mp4))
        );
        assert!(
            error(&["--frame", "0", "--subtitles", "out.srt", "out.png"]).contains("--subtitles")
        );
        assert!(
            error(&["--frame", "0", "--subtitles", "out.srt", "out.jpg"]).contains("--subtitles")
        );
    }

    #[test]
    fn subtitles_repeat_and_stand_alone() {
        let matches = Cli::command()
            .try_get_matches_from([
                "celesta-exporter",
                "film.tsx",
                "--subtitles",
                "film.srt",
                "--subtitles",
                "film.{lang}.vtt",
            ])
            .unwrap();
        let cli = Cli::from_arg_matches(&matches).unwrap();
        assert_eq!(
            cli.subtitles,
            [PathBuf::from("film.srt"), PathBuf::from("film.{lang}.vtt")]
        );
        assert_eq!(cli.output, None);
        assert_eq!(
            choice(&["--subtitles", "film.srt", "--from", "1"]),
            Ok(OutputChoice::SubtitlesOnly)
        );
        for (args, flag) in [
            (["--subtitles", "a.srt", "--crf", "20"], "--crf"),
            (["--subtitles", "a.srt", "--every", "5"], "--every"),
            (["--subtitles", "a.srt", "--driver", "metal"], "--driver"),
        ] {
            let message = error(&args);
            assert!(message.contains(flag), "{message}");
            assert!(message.contains("<OUTPUT>"), "{message}");
        }
        assert!(
            Cli::command()
                .try_get_matches_from(["celesta-exporter", "film.tsx"])
                .is_err(),
            "an export with neither <OUTPUT> nor --subtitles is a usage error"
        );
    }

    fn parses(args: &[&str]) -> bool {
        Cli::command()
            .try_get_matches_from(["celesta-exporter"].into_iter().chain(args.iter().copied()))
            .is_ok()
    }

    #[test]
    fn info_needs_no_output_and_takes_no_render_flags() {
        assert!(parses(&["film.tsx", "--react", "--info"]));
        assert!(parses(&[
            "film.tsx",
            "--react",
            "--info",
            "--json",
            "--composition",
            "intro"
        ]));
        assert!(parses(&["project.celesta.json", "--info", "--json"]));
        for args in [
            ["film.tsx", "--info", "out.mp4"].as_slice(),
            &["film.tsx", "--info", "--crf", "20"],
            &["film.tsx", "--info", "--frame", "0"],
            &["film.tsx", "--info", "--subtitles", "film.srt"],
            &["film.tsx", "--info", "--loop"],
        ] {
            assert!(!parses(args), "{args:?}");
        }
    }

    #[test]
    fn json_progress_needs_json_and_stdin_cancel_stays_off_inspect() {
        assert!(parses(&[
            "film.tsx",
            "out.mp4",
            "--json",
            "--json-progress"
        ]));
        assert!(!parses(&["film.tsx", "out.mp4", "--json-progress"]));
        assert!(parses(&["film.tsx", "out.mp4", "--stdin-cancel"]));
        assert!(!parses(&[
            "film.tsx",
            "--react",
            "--inspect",
            "--stdin-cancel"
        ]));
    }
}
