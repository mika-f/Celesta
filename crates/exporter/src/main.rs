use std::num::{NonZeroU32, NonZeroU64};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser};

mod inspect;
mod json;
mod progress;

use celesta_composition::Time;
use celesta_exporter::components::{
    ListedComponent, describe_component, description_json, kind_label, list_components,
    listing_json, origin_label,
};
use celesta_exporter::report::{self, HintStyle};
use celesta_exporter::{
    AudioChoice, AudioLoudness, ContactSheet, EncoderChoice, ExportCancellation, ExportError,
    ExportJob, ExportOptions, ExportProgress, ExportRange, ExportedFile, FrameSelection, GpuDriver,
    LoudnessTarget, OutputChoice, PeakHandling, PresetLookup, PresetsFile, ReactRuntimeOptions,
    RenderQuality, SettingInput, StillExport, StillFormat, parse_timecode,
};
use celesta_media::{ComponentRef, SettingKind, SettingValue};
use celesta_plugin_host::{InstalledPlugins, PluginKillSwitch};
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
(.png), JPEG (.jpg, .jpeg) or WebP (.webp) by the output's extension;
--quality sets JPEG and WebP quality, 1-100 (default 90). --video-option
sets the image encoder's settings: compression=none|fastest|fast|balanced|high,
depth=16 and transparent=true for PNG; lossless=true and transparent=true for
WebP (--describe builtin/png lists them). --muxer and --video-encoder name a
plugin's image components. WebP is lossy unless
lossless=true. By default, stills are drawn over the export's opaque
background, as an MP4 is, so JPEG's missing alpha channel loses nothing;
transparent=true keeps PNG and WebP transparent. Multiple frames use output-000090.png names with the
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

The output's extension picks the muxer that writes it, with that muxer's
default encoders: .mp4 is H.264 with AAC, using the bundled system encoder
on macOS/Windows when available, or the separate ffmpeg-x264 download.
The bundled ffmpeg plugin writes .mov as ProRes 4444 with 24-bit PCM audio
and .webm as VP9 with Opus. MOV and WebM keep transparency where nothing is drawn (straight alpha,
BT.709). --muxer, --video-encoder and --audio-encoder (none for no audio)
choose components by <plugin>/<component>, and --video-option and
--audio-option set an encoder's settings as KEY=VALUE, repeatable.
--list-components shows what is installed (with --check, whether each
component is available) and --describe <plugin>/<component> shows one
component's settings; both take --json.

--loop is for a video played in a loop, as Shorts and TikTok play one: the
audio encoder starts from the end of the soundtrack instead of silence, so
the first hit is not smeared on every pass. The decoded sound is still
exactly as long as the video. Muxers that hide leading audio (MP4 and MOV)
use it; others (WebM) are written as without it.

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
export when stdin reads a `cancel` line or closes; @celesta.studio/exporter uses
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
    #[arg(required_unless_present_any = ["list_components", "describe"])]
    source: Option<PathBuf>,
    /// The .mp4, .mov (ProRes 4444 with alpha) or .webm (VP9 with alpha) file to
    /// write, or the .png, .jpg/.jpeg or .webp file for frame exports. Leave
    /// it out to write only the --subtitles files.
    #[arg(required_unless_present_any = ["inspect", "info", "subtitles", "list_components", "describe"])]
    output: Option<PathBuf>,
    /// Evaluate frames requested on stdin as JSON lines instead of exporting.
    #[arg(
        long,
        requires = "react",
        conflicts_with_all = [
            "output", "project", "overwrite", "from", "to", "frames", "every",
            "contact_sheet", "columns", "tile_width", "quality", "render_quality", "driver",
            "no_ui", "json", "debug_guides", "subtitles", "looping", "loudness", "true_peak",
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
            "columns", "tile_width", "quality", "render_quality", "driver", "debug_guides",
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
    /// A content plugin's directory, or a directory of them, searched before
    /// every other plugin location (CELESTA_PLUGIN_PATH, the user's plugin
    /// folder and the folders they added, the bundled one); repeatable.
    #[arg(long, value_name = "DIR")]
    plugin_dir: Vec<PathBuf>,
    /// Run the plugin with this id for this export without the desktop
    /// app's approval, as it is now; repeatable. Plugins in the user's
    /// plugin folder or a folder they added run only once allowed, in the
    /// app or here. A plugin turned off in the app stays off.
    #[arg(long, value_name = "ID", value_parser = parse_plugin_id)]
    allow_plugin: Vec<String>,
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
    /// An export preset: looked up in --preset-file, then the project's export-presets.json, then yours.
    #[arg(long, value_name = "NAME")]
    preset: Option<String>,
    /// An export-presets.json to look --preset up in first.
    #[arg(long, value_name = "PATH")]
    preset_file: Option<PathBuf>,
    /// The muxer that writes the output (a video, or a still's file or
    /// sequence), instead of the output's extension's.
    #[arg(long, value_name = "PLUGIN/ID", value_parser = parse_component)]
    muxer: Option<ComponentRef>,
    /// The video encoder, or a still's image encoder, instead of the
    /// muxer's default.
    #[arg(long, value_name = "PLUGIN/ID", value_parser = parse_component)]
    video_encoder: Option<ComponentRef>,
    /// A video or image encoder setting; repeatable.
    #[arg(long, value_name = "KEY=VALUE", value_parser = parse_option)]
    video_option: Vec<(String, String)>,
    /// The audio encoder, instead of the muxer's default, or none for a
    /// video without audio.
    #[arg(long, value_name = "PLUGIN/ID|none", value_parser = parse_audio_encoder)]
    audio_encoder: Option<AudioEncoderArg>,
    /// An audio encoder setting; repeatable.
    #[arg(long, value_name = "KEY=VALUE", value_parser = parse_option)]
    audio_option: Vec<(String, String)>,
    /// List the installed media components and exit.
    #[arg(long, conflicts_with = "describe")]
    list_components: bool,
    /// With --list-components, ask each plugin whether its components are
    /// available.
    #[arg(long, requires = "list_components")]
    check: bool,
    /// Show one component's settings and exit.
    #[arg(long, value_name = "PLUGIN/ID", value_parser = parse_component)]
    describe: Option<ComponentRef>,
    /// The video plays in a loop: encode its sound to run from its end into
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

/// `--audio-encoder`: a component, or `none`.
#[derive(Clone, Debug, PartialEq)]
enum AudioEncoderArg {
    None,
    Component(ComponentRef),
}

fn parse_component(value: &str) -> Result<ComponentRef, String> {
    value
        .parse()
        .map_err(|error: celesta_media::InvalidComponentRef| error.to_string())
}

fn parse_audio_encoder(value: &str) -> Result<AudioEncoderArg, String> {
    if value == "none" {
        Ok(AudioEncoderArg::None)
    } else {
        parse_component(value).map(AudioEncoderArg::Component)
    }
}

/// `--video-option` and `--audio-option`: `KEY=VALUE`, split at the first `=`.
fn parse_option(value: &str) -> Result<(String, String), String> {
    match value.split_once('=') {
        Some((key, text)) if !key.is_empty() => Ok((key.to_owned(), text.to_owned())),
        _ => Err(format!("`{value}` is not KEY=VALUE")),
    }
}

fn options(given: &[(String, String)]) -> std::collections::BTreeMap<String, SettingInput> {
    given
        .iter()
        .map(|(key, text)| (key.clone(), SettingInput::Text(text.clone())))
        .collect()
}

impl Cli {
    /// The project or entry to export; clap requires it unless a listing
    /// was asked for.
    fn source(&self) -> &Path {
        self.source
            .as_deref()
            .expect("clap requires a source unless a component listing was asked for")
    }

    /// The muxer and encoders the component flags name.
    fn output_choice(&self) -> OutputChoice {
        let encoder = |component: Option<ComponentRef>, given: &[(String, String)]| EncoderChoice {
            component,
            options: options(given),
        };
        OutputChoice {
            preset: self.preset.clone(),
            muxer: self.muxer.clone(),
            video: encoder(self.video_encoder.clone(), &self.video_option),
            audio: match &self.audio_encoder {
                Some(AudioEncoderArg::None) => AudioChoice::None,
                Some(AudioEncoderArg::Component(component)) => {
                    AudioChoice::Encode(encoder(Some(component.clone()), &self.audio_option))
                }
                None => AudioChoice::Encode(encoder(None, &self.audio_option)),
            },
        }
    }

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
    // A listing wins over an export the line also describes.
    if cli.list_components || cli.describe.is_some() {
        return components(&cli);
    }
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
        .then(|| resolve_props_file(cli.source(), cli.props_file.as_deref()))
        .flatten();
    if cli.info {
        return info(&cli, props_file);
    }
    let explicit = |id: &str| matches.value_source(id) == Some(ValueSource::CommandLine);
    let kind = output_kind(&cli, &explicit)
        .unwrap_or_else(|error| usage_error(cli.json, ErrorKind::ArgumentConflict, &error));
    if let Some(path) = &cli.preset_file
        && let Err(error) = read_preset_file(path)
    {
        usage_error(cli.json, ErrorKind::ValueValidation, &error);
    }
    let range = export_range(cli.from, cli.to)
        .unwrap_or_else(|error| usage_error(cli.json, ErrorKind::ValueValidation, &error));
    let export = |on_progress: &mut dyn FnMut(ExportProgress)| {
        job(&cli, &kind, range).run(&cancellation, on_progress)
    };
    if cli.json {
        return if json::run(cli.source(), props_file, cli.json_progress, export) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let result = match (cli.output(), &kind) {
        (Some(output), OutputKind::Video(_) | OutputKind::Stills) => {
            let settings = progress::Settings {
                source: cli.source(),
                output,
                react: cli.react,
                still: kind == OutputKind::Stills,
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
            print_error("Celesta export", &error);
            ExitCode::FAILURE
        }
    }
}

/// `--preset-file`: an export-presets.json that exists and parses.
fn read_preset_file(path: &Path) -> Result<(), String> {
    PresetsFile::load(path)
        .map(drop)
        .map_err(|error| format!("--preset-file {error}"))
}

/// `--list-components` and `--describe`: what is installed, from the
/// manifests (and with `--check` from each plugin), as JSON with `--json`.
fn components(cli: &Cli) -> ExitCode {
    let plugins = InstalledPlugins::find_reporting(&cli.plugin_dir, &cli.allow_plugin);
    let registry = celesta_exporter::media_registry(&plugins);
    let kill = PluginKillSwitch::new();
    if let Some(reference) = &cli.describe {
        return match describe_component(&registry, reference, &kill) {
            Ok((info, description)) if cli.json => {
                print_json(&description_json(&info, &description));
                ExitCode::SUCCESS
            }
            Ok((info, description)) => {
                print!("{}", describe_text(&info, &description));
                ExitCode::SUCCESS
            }
            Err(error) if cli.json => {
                json::failure(&error);
                ExitCode::FAILURE
            }
            Err(error) => {
                print_error("Celesta export", &error);
                ExitCode::FAILURE
            }
        };
    }
    let listed = list_components(&registry, cli.check, &kill);
    if cli.json {
        print_json(&listing_json(&listed));
    } else {
        print!("{}", listing_text(&listed));
    }
    ExitCode::SUCCESS
}

/// `<context>: <error>` on stderr, then what to change, as `--json`'s
/// report would hint it.
fn print_error(context: &str, error: &ExportError) {
    eprintln!("{context}: {error}");
    if let Some(hint) = report::hint(error, HintStyle::Cli) {
        eprintln!("hint: {hint}");
    }
}

fn print_json(value: &serde_json::Value) {
    println!("{value}");
}

/// One line per component: `<id>  <kind>  <name>  (<origin>)`, then with a
/// check `available` or `unavailable: <reason>`.
fn listing_text(listed: &[ListedComponent]) -> String {
    let mut text = String::new();
    for component in listed {
        let info = &component.info;
        text.push_str(&format!(
            "{}  {}  {}  ({})",
            info.reference,
            kind_label(info.kind),
            info.name,
            origin_label(info.origin)
        ));
        match (component.available, &component.reason) {
            (Some(true), _) => text.push_str("  available"),
            (Some(false), Some(reason)) => text.push_str(&format!("  unavailable: {reason}")),
            (Some(false), None) => text.push_str("  unavailable"),
            (None, _) => {}
        }
        text.push('\n');
    }
    text
}

/// The header line, then each setting as `<key>  <type>  default <value>
/// <label>` and its range or options.
fn describe_text(
    info: &celesta_media::MediaComponentInfo,
    description: &celesta_media::ComponentDescription,
) -> String {
    let mut text = format!(
        "{}  {}  {}  ({})",
        info.reference,
        kind_label(info.kind),
        info.name,
        origin_label(info.origin)
    );
    if description.available {
        text.push_str("  available\n");
    } else {
        let reason = description.reason.as_deref().unwrap_or("no reason given");
        text.push_str(&format!("  unavailable: {reason}\n"));
    }
    for field in &description.settings {
        let (kind, detail) = match &field.kind {
            SettingKind::Select { options } => (
                "select",
                Some(format!(
                    "options: {}",
                    options
                        .iter()
                        .map(|option| format!("{} ({})", option.value, option.label))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            ),
            SettingKind::Number {
                min,
                max,
                integer,
                unit,
                ..
            } => {
                let mut range = Vec::new();
                if let Some(min) = min {
                    range.push(format!("min {min}"));
                }
                if let Some(max) = max {
                    range.push(format!("max {max}"));
                }
                if let Some(unit) = unit {
                    range.push(format!("unit {unit}"));
                }
                (
                    if *integer { "integer" } else { "number" },
                    (!range.is_empty()).then(|| format!("range: {}", range.join(", "))),
                )
            }
            SettingKind::Boolean => ("boolean", None),
            SettingKind::String => ("string", None),
        };
        let default = match &field.default {
            SettingValue::String(value) => value.clone(),
            other => other.to_string(),
        };
        text.push_str(&format!(
            "{}  {kind}  default {default}  {}\n",
            field.key, field.label
        ));
        if let Some(detail) = detail {
            text.push_str(&format!("    {detail}\n"));
        }
    }
    text
}

/// `--info`: reports the composition, as JSON with `--json`.
fn info(cli: &Cli, props_file: Option<PropsFileChoice>) -> ExitCode {
    let job = job(cli, &OutputKind::SubtitlesOnly, None);
    if cli.json {
        let ok = json::run(cli.source(), props_file, cli.json_progress, |on_progress| {
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
            print_error("Celesta export", &error);
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
    let plugins = InstalledPlugins::find_reporting(&cli.plugin_dir, &cli.allow_plugin);
    // The React runtime's media probes go through the media plugins.
    celesta_exporter::use_media(&plugins);
    let result = match ReactRuntimeOptions::for_entry(
        cli.source(),
        cli.props_file.as_deref(),
        cli.props.as_deref(),
    ) {
        Ok(runtime) => inspect::run(
            cli.source(),
            &ReactRuntimeOptions {
                node: cli.node.clone().unwrap_or(runtime.node),
                cli_script: cli.runtime.clone().unwrap_or(runtime.cli_script),
                properties: runtime.properties,
                composition_id: cli.composition.clone(),
            },
            &plugins,
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
#[derive(Clone, Debug, PartialEq)]
#[expect(
    clippy::large_enum_variant,
    reason = "there is one, made once from the command line"
)]
enum OutputKind {
    /// A video, with these components.
    Video(OutputChoice),
    Stills,
    /// Nothing: only the subtitle files.
    SubtitlesOnly,
}

/// The component flags a still takes, by argument id: a muxer, and an image
/// encoder and its settings.
const STILL_COMPONENT_FLAGS: [&str; 3] = ["muxer", "video_encoder", "video_option"];

/// The flags that choose a video's components, by argument id.
const COMPONENT_FLAGS: [(&str, &str); 7] = [
    ("preset", "--preset"),
    ("preset_file", "--preset-file"),
    ("muxer", "--muxer"),
    ("video_encoder", "--video-encoder"),
    ("video_option", "--video-option"),
    ("audio_encoder", "--audio-encoder"),
    ("audio_option", "--audio-option"),
];

/// What `cli` exports, checking the combinations of selection and output
/// flags clap cannot express. `explicit` tells whether the argument with
/// that id was given on the command line.
fn output_kind(cli: &Cli, explicit: &dyn Fn(&str) -> bool) -> Result<OutputKind, String> {
    let Some(output) = cli.output() else {
        return subtitles_only_choice(explicit);
    };
    let selected = !cli.frames.is_empty() || cli.every.is_some();
    let stills = selected || cli.contact_sheet;
    if stills && !selected {
        return Err("still output requires --frame, --frames or --every".into());
    }
    if stills {
        if !cli.subtitles.is_empty() {
            return Err("--subtitles applies to video output, not still frames".into());
        }
        if cli.looping {
            return Err("--loop applies to video output, not still frames".into());
        }
        if let Some(flag) = COMPONENT_FLAGS
            .iter()
            .filter(|(id, _)| !STILL_COMPONENT_FLAGS.contains(id))
            .find(|(id, _)| explicit(id))
        {
            return Err(format!(
                "{} applies to video output, not still frames",
                flag.1
            ));
        }
        if cli.loudness.is_some() {
            return Err("--loudness applies to video output, not still frames".into());
        }
        // The extension is the exporter's to check once the components are
        // named: one the built-in muxers do not write is
        // `invalid_selection`, and a plugin's muxer may write others. A
        // quality for `.png` is refused here only when no encoder is named;
        // a named one says what it takes.
        let format = StillFormat::from_extension(output);
        if cli.quality.is_some() && cli.video_encoder.is_none() && format == Some(StillFormat::Png)
        {
            return Err("--quality applies to JPEG and WebP output; PNG is lossless".into());
        }
        if cli.quality.is_some() && cli.video_option.iter().any(|(key, _)| key == "quality") {
            return Err(
                "--quality and --video-option quality= both set the quality; give one".into(),
            );
        }
        return Ok(OutputKind::Stills);
    }
    if cli.quality.is_some() {
        return Err("--quality applies to JPEG and WebP stills, not video".into());
    }
    if cli.debug_guides {
        return Err(
            "--debug-guides applies to still output; video exports never draw debug guides".into(),
        );
    }
    // Options for no encoder would be dropped unchecked, typos and all.
    if matches!(cli.audio_encoder, Some(AudioEncoderArg::None)) && !cli.audio_option.is_empty() {
        return Err(
            "--audio-option needs an audio encoder; --audio-encoder none writes no audio".into(),
        );
    }
    // The exporter picks the muxer by the output's extension, and reports
    // one no muxer writes as `unsupported_output`.
    Ok(OutputKind::Video(cli.output_choice()))
}

/// Checks a command line with no <OUTPUT>, which clap allows only with
/// --subtitles: flags about rendering or encoding a picture have nothing to
/// apply to.
fn subtitles_only_choice(explicit: &dyn Fn(&str) -> bool) -> Result<OutputKind, String> {
    for (id, flag) in COMPONENT_FLAGS.into_iter().chain([
        ("frames", "--frame"),
        ("every", "--every"),
        ("contact_sheet", "--contact-sheet"),
        ("debug_guides", "--debug-guides"),
        ("quality", "--quality"),
        ("looping", "--loop"),
        ("loudness", "--loudness"),
        ("render_quality", "--render-quality"),
        ("driver", "--driver"),
    ]) {
        if explicit(id) {
            return Err(format!(
                "{flag} needs an <OUTPUT>; without one only the --subtitles files are written"
            ));
        }
    }
    Ok(OutputKind::SubtitlesOnly)
}

/// The export `cli` describes.
fn job(cli: &Cli, kind: &OutputKind, range: Option<ExportRange>) -> ExportJob {
    ExportJob {
        source: cli.source().to_owned(),
        react: cli.react,
        composition_id: cli.composition.clone(),
        companion: cli.project.clone(),
        props_file: cli.props_file.clone(),
        props: cli.props.clone(),
        output: cli.output.clone(),
        stills: (*kind == OutputKind::Stills).then(|| StillExport {
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
            output: match kind {
                OutputKind::Video(choice) => choice.clone(),
                // The audio and preset flags are refused for a still, so the
                // choice carries only its image components.
                OutputKind::Stills => cli.output_choice(),
                OutputKind::SubtitlesOnly => OutputChoice::default(),
            },
            render_quality: cli.render_quality,
            driver: cli.driver,
            subtitles: cli.subtitles.clone(),
            looping: cli.looping,
            loudness: cli.loudness(),
            plugins: Some(InstalledPlugins::find_reporting(
                &cli.plugin_dir,
                &cli.allow_plugin,
            )),
            presets: match kind {
                OutputKind::Video(_) => {
                    PresetLookup::for_source(cli.source(), cli.preset_file.clone())
                }
                OutputKind::Stills | OutputKind::SubtitlesOnly => PresetLookup::default(),
            },
        },
    }
}

/// A `--allow-plugin` value: a plugin id
/// ([`celesta_plugin_host::is_valid_id`]).
fn parse_plugin_id(id: &str) -> Result<String, String> {
    if celesta_plugin_host::is_valid_id(id) {
        Ok(id.to_owned())
    } else {
        Err(format!(
            "{id:?} is not a plugin id (lowercase letters, digits, '-', '_', and '.', starting with a letter or digit)"
        ))
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
    use clap::ValueEnum;

    /// `--help` lists, and clap accepts, the names the library parses.
    #[test]
    fn option_values_take_the_library_names() {
        fn names_match<T: ValueEnum + Copy + Into<&'static str>>() {
            for value in T::value_variants() {
                let name = value.to_possible_value().unwrap();
                assert_eq!(name.get_name(), (*value).into());
            }
        }
        names_match::<PeakHandling>();
        names_match::<RenderQuality>();
        names_match::<GpuDriver>();
    }

    fn choice(args: &[&str]) -> Result<OutputKind, String> {
        let matches = Cli::command()
            .try_get_matches_from(
                ["celesta-exporter", "project.celesta.json"]
                    .into_iter()
                    .chain(args.iter().copied()),
            )
            .unwrap();
        let cli = Cli::from_arg_matches(&matches).unwrap();
        output_kind(&cli, &|id| {
            matches.value_source(id) == Some(ValueSource::CommandLine)
        })
    }

    fn error(args: &[&str]) -> String {
        choice(args).expect_err("the arguments should be rejected")
    }

    fn video() -> Result<OutputKind, String> {
        Ok(OutputKind::Video(OutputChoice::default()))
    }

    #[test]
    fn every_video_takes_the_default_components() {
        // The exporter picks the muxer by the extension, and reports one no
        // muxer writes as `unsupported_output`, with its PNG hint.
        for output in ["out.mov", "out.WEBM", "out.mp4", "out.gif", "out.png"] {
            assert_eq!(choice(&[output]), video(), "{output}");
        }
    }

    #[test]
    fn the_removed_encoder_flags_are_unknown() {
        for flags in [
            ["--output-format", "mp4"].as_slice(),
            &["--crf", "20"],
            &["--bit-depth", "10"],
            &["--color-conversion", "gpu"],
        ] {
            let args = [&["project.celesta.json", "out.mp4"][..], flags].concat();
            assert!(!parses(&args), "{flags:?}");
        }
    }

    #[test]
    fn still_selections_keep_their_rules() {
        assert_eq!(choice(&["--frame", "0", "out.png"]), Ok(OutputKind::Stills));
        assert!(error(&["--contact-sheet", "out.png"]).contains("requires --frame"));
    }

    #[test]
    fn the_output_extension_selects_the_still_format() {
        for output in ["out.jpg", "out.JPEG", "out.webp"] {
            assert_eq!(
                choice(&["--frame", "0", output]),
                Ok(OutputKind::Stills),
                "{output}"
            );
        }
        assert_eq!(
            choice(&["--every", "2", "out.WebP"]),
            Ok(OutputKind::Stills)
        );
        // An unknown extension is left to the exporter, as for video.
        assert_eq!(choice(&["--frame", "0", "out.gif"]), Ok(OutputKind::Stills));
    }

    #[test]
    fn stills_take_video_options() {
        let cli = |args: &[&str]| {
            let matches = Cli::command()
                .try_get_matches_from(
                    ["celesta-exporter", "project.celesta.json"]
                        .into_iter()
                        .chain(args.iter().copied()),
                )
                .unwrap();
            Cli::from_arg_matches(&matches).unwrap()
        };
        let args = [
            "--frame",
            "0",
            "--video-option",
            "transparent=true",
            "out.png",
        ];
        assert_eq!(choice(&args), Ok(OutputKind::Stills));
        let job = job(&cli(&args), &OutputKind::Stills, None);
        assert_eq!(
            job.options.output.video.options["transparent"],
            SettingInput::Text("true".into())
        );
        for (flags, flag) in [
            (["--preset", "web"], "--preset"),
            (["--audio-encoder", "none"], "--audio-encoder"),
        ] {
            let message = error(&[&["--frame", "0"][..], &flags, &["out.png"]].concat());
            assert_eq!(
                message,
                format!("{flag} applies to video output, not still frames")
            );
        }
        let message = error(&[
            "--frame",
            "0",
            "--quality",
            "50",
            "--video-option",
            "quality=60",
            "out.jpg",
        ]);
        assert_eq!(
            message,
            "--quality and --video-option quality= both set the quality; give one"
        );
        assert_eq!(
            choice(&["--frame", "0", "--muxer", "foo/avif", "out.avif"]),
            Ok(OutputKind::Stills)
        );
    }

    #[test]
    fn quality_applies_to_jpeg_and_webp_stills_only() {
        assert_eq!(
            choice(&["--frame", "0", "--quality", "70", "out.jpg"]),
            Ok(OutputKind::Stills)
        );
        assert_eq!(
            choice(&["--every", "3", "--quality", "1", "out.webp"]),
            Ok(OutputKind::Stills)
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
    fn loop_applies_to_every_video() {
        // A muxer that does not hide leading audio writes it as without.
        for output in ["out.mp4", "out.mov", "out.webm"] {
            assert_eq!(choice(&[output, "--loop"]), video(), "{output}");
        }
        for args in [
            ["--frame", "0", "--loop", "out.png"].as_slice(),
            &["--subtitles", "a.srt", "--loop"],
        ] {
            let message = error(args);
            assert!(message.contains("--loop"), "{message}");
        }
    }

    fn reference(text: &str) -> ComponentRef {
        text.parse().unwrap()
    }

    #[test]
    fn component_flags_build_the_choice() {
        assert_eq!(
            choice(&[
                "out.mp4",
                "--muxer",
                "ffmpeg/mp4",
                "--video-encoder",
                "ffmpeg/libx264",
                "--video-option",
                "crf=20",
                "--video-option",
                "preset=slow",
                "--audio-encoder",
                "none",
            ]),
            Ok(OutputKind::Video(OutputChoice {
                muxer: Some(reference("ffmpeg/mp4")),
                video: EncoderChoice {
                    component: Some(reference("ffmpeg/libx264")),
                    options: [
                        ("crf".to_owned(), SettingInput::Text("20".into())),
                        ("preset".to_owned(), SettingInput::Text("slow".into())),
                    ]
                    .into(),
                },
                audio: AudioChoice::None,
                preset: None,
            }))
        );
        // An audio encoder takes its options; a preset is carried to resolve.
        assert_eq!(
            choice(&[
                "out.mp4",
                "--audio-encoder",
                "ffmpeg/aac",
                "--audio-option",
                "bitrate=128000",
                "--preset",
                "web",
            ]),
            Ok(OutputKind::Video(OutputChoice {
                preset: Some("web".into()),
                audio: AudioChoice::Encode(EncoderChoice {
                    component: Some(reference("ffmpeg/aac")),
                    options: [("bitrate".to_owned(), SettingInput::Text("128000".into()))].into(),
                }),
                ..OutputChoice::default()
            }))
        );
    }

    #[test]
    fn no_audio_takes_no_audio_options() {
        assert_eq!(
            error(&[
                "out.mp4",
                "--audio-encoder",
                "none",
                "--audio-option",
                "bitrate=96000",
            ]),
            "--audio-option needs an audio encoder; --audio-encoder none writes no audio"
        );
    }

    #[test]
    fn a_video_option_needs_an_equals_sign() {
        let error = Cli::command()
            .try_get_matches_from([
                "celesta-exporter",
                "project.celesta.json",
                "out.mp4",
                "--video-option",
                "crf",
            ])
            .expect_err("a bare key is not an option");
        assert!(error.to_string().contains("KEY=VALUE"), "{error}");
    }

    #[test]
    fn a_bad_component_reference_is_a_usage_error() {
        assert!(!parses(&[
            "film.tsx",
            "out.mp4",
            "--video-encoder",
            "libx264"
        ]));
        assert!(!parses(&["film.tsx", "out.mp4", "--audio-encoder", "aac/"]));
        assert!(parses(&["film.tsx", "out.mp4", "--audio-encoder", "none"]));
    }

    #[test]
    fn component_flags_need_a_video_output() {
        for (flags, flag) in [
            (
                &["--video-encoder", "ffmpeg/libx264"][..],
                "--video-encoder",
            ),
            (&["--muxer", "ffmpeg/mp4"], "--muxer"),
            (&["--video-option", "crf=20"], "--video-option"),
            (&["--audio-encoder", "none"], "--audio-encoder"),
            (&["--audio-option", "bitrate=1"], "--audio-option"),
            (&["--preset", "web"], "--preset"),
            (&["--preset-file", "presets.json"], "--preset-file"),
        ] {
            if !STILL_COMPONENT_FLAGS
                .contains(&flag.trim_start_matches("--").replace('-', "_").as_str())
            {
                let stills = error(&[&["--frame", "0", "out.png"][..], flags].concat());
                assert!(stills.contains(flag), "{stills}");
            }
            let subtitles = error(&[&["--subtitles", "a.srt"][..], flags].concat());
            assert!(subtitles.contains(flag), "{subtitles}");
        }
        assert!(error(&["--frame", "0", "--loop", "out.png"]).contains("--loop"));
        assert_eq!(choice(&["out.webm", "--loop"]), video());
    }

    #[test]
    fn list_components_needs_no_source() {
        for args in [
            ["celesta-exporter", "--list-components"].as_slice(),
            &["celesta-exporter", "--list-components", "--check", "--json"],
            &["celesta-exporter", "--describe", "ffmpeg/libx264"],
        ] {
            let cli = Cli::try_parse_from(args).unwrap();
            assert_eq!(cli.source, None, "{args:?}");
            assert_eq!(cli.output, None, "{args:?}");
        }
        let cli = Cli::try_parse_from([
            "celesta-exporter",
            "--list-components",
            "--check",
            "film.tsx",
            "out.mp4",
        ])
        .unwrap();
        assert!(cli.list_components && cli.check);
        assert!(Cli::try_parse_from(["celesta-exporter", "--check"]).is_err());
        assert!(
            Cli::try_parse_from(["celesta-exporter", "film.tsx", "out.mp4", "--check"]).is_err()
        );
        assert!(
            Cli::try_parse_from([
                "celesta-exporter",
                "--list-components",
                "--describe",
                "ffmpeg/mp4"
            ])
            .is_err()
        );
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
        assert_eq!(choice(&["out.webm", "--loudness", "-14"]), video());
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
        assert_eq!(choice(&["out.mp4", "--subtitles", "out.srt"]), video());
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
            Ok(OutputKind::SubtitlesOnly)
        );
        for (args, flag) in [
            (["--subtitles", "a.srt", "--quality", "70"], "--quality"),
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
            &["film.tsx", "--info", "--quality", "70"],
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
