//! Arguments of the rendering tools, checked and turned into export jobs.

use std::path::{Path, PathBuf};

use celesta_exporter::{
    AudioChoice, ContactSheet, EncoderChoice, ExportJob, ExportOptions, ExportRange,
    FrameSelection, LoudnessTarget, OutputChoice, PeakHandling, PresetLookup, PresetsFile,
    SettingInput, StillExport, StillFormat, parse_timecode,
};
use celesta_media::{ComponentRef, SettingValue};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::frames::OutputDirs;

/// What every rendering tool takes.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceArgs {
    /// A JSON project (`.json`, such as `project.celesta.json`) or a React
    /// entry (any other extension, normally `.tsx`).
    source: PathBuf,
    /// React only: root composition ID; defaults to the first root.
    composition: Option<String>,
    /// React only: a `.celesta.json` exported alongside the entry.
    project: Option<PathBuf>,
    /// React only: project property values, over the props file's.
    props: Option<Map<String, Value>>,
    /// React only: the props file; default: the entry's `props.json`.
    props_file: Option<PathBuf>,
    /// Start of the span to render, as a timecode (`HH:MM:SS.mmm`,
    /// `MM:SS`, or seconds).
    start: Option<String>,
    /// End of the span to render, as a timecode; after `start`.
    end: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenderFramesArgs {
    #[serde(flatten)]
    source: SourceArgs,
    /// Zero-based frames to render; negative numbers count from the end
    /// (`-1` is the last frame). Cannot be combined with start/end.
    frames: Option<Vec<i64>>,
    /// Render every `n`th frame of the span (the whole video without
    /// start/end), plus its last frame, instead of `frames`.
    every: Option<u64>,
    /// React only: draw `<DebugOverlay>`, `<DebugBounds>`, and
    /// `useDebugGuides()` guides, which otherwise only the preview shows.
    debug_guides: Option<bool>,
    /// The image format to write and return: `png` (default, lossless),
    /// `jpeg`, or `webp`.
    format: Option<ImageFormat>,
    /// JPEG or WebP quality, 1–100 (default 90); higher is sharper and
    /// larger. Not accepted for PNG unless `videoEncoder` names an encoder,
    /// which then says whether it takes one.
    quality: Option<u8>,
    /// The muxer that writes the image, as `<plugin>/<component>` (see
    /// `list_components`), instead of the built-in one the format names.
    muxer: Option<String>,
    /// The image encoder's component `id`, instead of the built-in one the
    /// format names, and its `options` by key (see `describe_component`):
    /// `compression`, `depth` and `transparent` for `builtin/png`,
    /// `lossless` and `transparent` for `builtin/webp`.
    video_encoder: Option<EncoderArg>,
    /// The folder to write the images to; default: a temporary folder.
    output_dir: Option<PathBuf>,
    /// Replace images that already exist in `outputDir`.
    overwrite: Option<bool>,
    /// Arguments no field above takes; any are a usage error.
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: Map<String, Value>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContactSheetArgs {
    #[serde(flatten)]
    source: SourceArgs,
    /// Zero-based frames to tile; negative numbers count from the end.
    /// Cannot be combined with start/end.
    frames: Option<Vec<i64>>,
    /// Tile every `n`th frame of the span, plus its last frame.
    every: Option<u64>,
    /// Tiles per row; default 5.
    columns: Option<u32>,
    /// Tile width in pixels; default 320. The height keeps the aspect ratio.
    tile_width: Option<u32>,
    /// React only: draw debug guides into the tiles.
    debug_guides: Option<bool>,
    /// The image format when `output` is left out: `png` (default,
    /// lossless), `jpeg`, or `webp`. With `output`, it must match the
    /// output's extension.
    format: Option<ImageFormat>,
    /// JPEG or WebP quality, 1–100 (default 90); higher is sharper and
    /// larger. Not accepted for PNG unless `videoEncoder` names an encoder,
    /// which then says whether it takes one.
    quality: Option<u8>,
    /// The muxer that writes the image, as `<plugin>/<component>` (see
    /// `list_components`), instead of the built-in one the format names.
    muxer: Option<String>,
    /// The image encoder's component `id`, instead of the built-in one the
    /// format names, and its `options` by key (see `describe_component`):
    /// `compression`, `depth` and `transparent` for `builtin/png`,
    /// `lossless` and `transparent` for `builtin/webp`.
    video_encoder: Option<EncoderArg>,
    /// The `.png`, `.jpg`, `.jpeg`, or `.webp` to write; its extension picks
    /// the format. Default: a temporary file.
    output: Option<PathBuf>,
    /// Replace `output` if it exists.
    overwrite: Option<bool>,
    /// Arguments no field above takes; any are a usage error.
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: Map<String, Value>,
}

/// The image format of rendered stills.
#[derive(Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    #[default]
    Png,
    #[serde(alias = "jpg")]
    Jpeg,
    Webp,
}

impl From<ImageFormat> for StillFormat {
    fn from(format: ImageFormat) -> Self {
        match format {
            ImageFormat::Png => Self::Png,
            ImageFormat::Jpeg => Self::Jpeg,
            ImageFormat::Webp => Self::Webp,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportVideoArgs {
    #[serde(flatten)]
    source: SourceArgs,
    /// The video to write: its extension picks the muxer from the installed
    /// media plugins (with those that come with Celesta, `.mov` ProRes 4444
    /// with alpha, `.webm` VP9 with alpha, and `.mp4` H.264/AAC, which needs
    /// a native H.264 encoder or an optional H.264 plugin) unless `muxer`
    /// names one. Leave it out to write only the `subtitles` files, which
    /// evaluates the frames without rendering or encoding them.
    output: Option<PathBuf>,
    /// Replace `output` if it exists.
    overwrite: Option<bool>,
    /// An export preset's name, looked up in presetFile, then the project's
    /// export-presets.json, then the user's. Without it, the project's
    /// default preset applies when no component is chosen.
    preset: Option<String>,
    /// An export-presets.json to look `preset` up in first.
    preset_file: Option<PathBuf>,
    /// The muxer that writes the file, as `<plugin>/<component>` (see
    /// `list_components`), instead of the one the output's extension names.
    muxer: Option<String>,
    /// The video encoder, instead of the muxer's default: its component
    /// `id` and its `options` by key (see `describe_component`).
    video_encoder: Option<EncoderArg>,
    /// The audio encoder like `videoEncoder`, or `"none"` for a video
    /// without audio.
    audio_encoder: Option<AudioEncoderArg>,
    /// The video plays in a loop (Shorts, TikTok): encode its sound to run
    /// from its end into its start, so the first hit is not smeared on
    /// every pass. It changes nothing for a muxer that has no leading audio
    /// to trim.
    #[serde(rename = "loop")]
    looping: Option<bool>,
    /// Bring the whole mix to this integrated loudness before encoding:
    /// LUFS from -70 to -5, `"streaming"` (-14, as YouTube and Spotify play)
    /// or `"broadcast"` (-23, EBU R128). The result's video output reports
    /// the mix's `loudness` (integrated LUFS, true peak, gain) either way.
    loudness: Option<LoudnessArg>,
    /// The highest true peak `loudness` may leave, dBTP, -9 to 0; default -1.
    true_peak: Option<f64>,
    /// Where `loudness`'s gain would take a true peak over `truePeak`:
    /// `"limit"` (default) holds those peaks there with a limiter, `"lower"`
    /// stops the gain short of the target instead.
    peak_handling: Option<String>,
    /// Also write the subtitles (`<Subtitles>` cues, `<Dialogue>` lines, or
    /// a project's dialogue items) to this .srt or .vtt file, or to each of
    /// a list of them, timed like the video. `{lang}` in a path writes one
    /// file per language (`<Subtitles lang>`, else the enclosing `lang`;
    /// `und` without one), such as `film.{lang}.vtt`.
    subtitles: Option<SubtitlePaths>,
    /// Arguments no field above takes; any are a usage error.
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: Map<String, Value>,
}

/// An encoder: its component, or the muxer's default when `id` is left
/// out, and the options given for it.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EncoderArg {
    /// `<plugin>/<component>`, such as `ffmpeg-x264/libx264`.
    id: Option<String>,
    /// Settings by key, as `describe_component` lists them: strings,
    /// numbers, and booleans.
    options: Option<Map<String, Value>>,
}

/// The word that leaves audio out.
#[derive(Deserialize, JsonSchema)]
pub enum NoneWord {
    #[serde(rename = "none")]
    None,
}

/// `"none"`, or an audio encoder.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AudioEncoderArg {
    None(NoneWord),
    Encode(EncoderArg),
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListComponentsArgs {
    /// Also ask each plugin whether its components are available here.
    pub check: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DescribeComponentArgs {
    /// `<plugin>/<component>`, from `list_components`.
    pub component: String,
}

/// `presetFile`: an export-presets.json that exists and parses, as the
/// CLI's `--preset-file`.
fn check_preset_file(path: &Path) -> Result<(), String> {
    PresetsFile::load(path)
        .map(drop)
        .map_err(|error| format!("presetFile {error}"))
}

fn component_ref(text: &str) -> Result<ComponentRef, String> {
    text.parse()
        .map_err(|error: celesta_media::InvalidComponentRef| error.to_string())
}

impl EncoderArg {
    fn choice(&self, field: &str) -> Result<EncoderChoice, String> {
        let component = self.id.as_deref().map(component_ref).transpose()?;
        let mut options = std::collections::BTreeMap::new();
        for (key, value) in self.options.iter().flatten() {
            let value = match value {
                Value::String(text) => SettingValue::String(text.clone()),
                Value::Number(number) => SettingValue::Number(
                    number
                        .as_f64()
                        .ok_or_else(|| format!("{field}.options.{key} is not a finite number"))?,
                ),
                Value::Bool(flag) => SettingValue::Boolean(*flag),
                _ => {
                    return Err(format!(
                        "{field}.options.{key} must be a string, number, or boolean"
                    ));
                }
            };
            options.insert(key.clone(), SettingInput::Value(value));
        }
        Ok(EncoderChoice { component, options })
    }
}

/// A loudness target: LUFS or a preset's name.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum LoudnessArg {
    Lufs(f64),
    Preset(String),
}

/// One subtitle path or several.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SubtitlePaths {
    One(PathBuf),
    Many(Vec<PathBuf>),
}

impl SubtitlePaths {
    fn absolute(&self) -> Vec<PathBuf> {
        match self {
            Self::One(path) => vec![absolute(path)],
            Self::Many(paths) => paths.iter().map(|path| absolute(path)).collect(),
        }
    }
}

/// The checked parts of [`SourceArgs`].
struct Source {
    path: PathBuf,
    react: bool,
    composition: Option<String>,
    companion: Option<PathBuf>,
    props_file: Option<PathBuf>,
    props: Option<String>,
    range: Option<ExportRange>,
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}

impl SourceArgs {
    fn check(&self, debug_guides: bool) -> Result<Source, String> {
        let react = !self
            .source
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
        if !react
            && (self.project.is_some()
                || self.composition.is_some()
                || self.props.is_some()
                || self.props_file.is_some()
                || debug_guides)
        {
            return Err(format!(
                "project, composition, props, propsFile, and debugGuides apply to React entries; {} is a JSON project",
                self.source.display()
            ));
        }
        let timecode = |name: &str, value: &Option<String>| {
            value
                .as_deref()
                .map(|text| parse_timecode(text).map_err(|error| format!("{name}: {error}")))
                .transpose()
        };
        let start = timecode("start", &self.start)?;
        let end = timecode("end", &self.end)?;
        if let (Some(start), Some(end)) = (start, end)
            && end
                .cmp_exact(start)
                .map_err(|error| error.to_string())?
                .is_le()
        {
            return Err("end must be after start".into());
        }
        let range = (start.is_some() || end.is_some()).then(|| ExportRange {
            start: start.unwrap_or(celesta_composition::Time::ZERO),
            end,
        });
        Ok(Source {
            path: absolute(&self.source),
            composition: self.composition.clone(),
            react,
            companion: self.project.as_deref().map(absolute),
            props_file: self.props_file.as_deref().map(absolute),
            props: self
                .props
                .clone()
                .map(|props| Value::Object(props).to_string()),
            range,
        })
    }
}

/// A usage error naming arguments the tool does not take.
pub fn reject_unknown(unknown: &Map<String, Value>) -> Result<(), String> {
    if unknown.is_empty() {
        return Ok(());
    }
    let names: Vec<String> = unknown.keys().map(|name| format!("`{name}`")).collect();
    Err(format!(
        "unknown argument {}; see the tool's input schema for the arguments it takes",
        names.join(", ")
    ))
}

/// `frames` or `every`, exactly one.
fn selection(
    frames: &Option<Vec<i64>>,
    every: Option<u64>,
    range: Option<&ExportRange>,
) -> Result<FrameSelection, String> {
    match (frames, every) {
        (Some(_), Some(_)) => Err("pass frames or every, not both".into()),
        (None, None) => Err("pass frames or every to choose what to render".into()),
        (Some(frames), None) if frames.is_empty() => {
            Err("frames must list at least one frame".into())
        }
        (Some(_), None) if range.is_some() => {
            Err("frames cannot be combined with start/end; use every for a span".into())
        }
        (Some(frames), None) => Ok(FrameSelection::FromEnd(frames.clone())),
        (None, Some(0)) => Err("every must be at least 1".into()),
        (None, Some(step)) => Ok(FrameSelection::Every(step)),
    }
}

/// `output` made absolute, with the still format its extension picks.
fn still_path(output: &Path) -> Result<(PathBuf, StillFormat), String> {
    let Some(format) = StillFormat::from_extension(output) else {
        return Err(format!(
            "output must be a .png, .jpg, .jpeg or .webp file, not {}",
            output.display()
        ));
    };
    Ok((absolute(output), format))
}

/// The muxer and image encoder a still tool names.
fn still_choice(
    muxer: &Option<String>,
    video_encoder: &Option<EncoderArg>,
    quality: Option<u8>,
) -> Result<OutputChoice, String> {
    let video = video_encoder
        .as_ref()
        .map(|encoder| encoder.choice("videoEncoder"))
        .transpose()?
        .unwrap_or_default();
    if quality.is_some() && video.options.contains_key("quality") {
        return Err(
            "quality and videoEncoder.options.quality both set the quality; give one".into(),
        );
    }
    Ok(OutputChoice {
        muxer: muxer.as_deref().map(component_ref).transpose()?,
        video,
        ..OutputChoice::default()
    })
}

/// `quality`, checked against the still format it would encode; a named
/// encoder is left to say what it takes.
fn still_quality(
    format: StillFormat,
    quality: Option<u8>,
    named_encoder: bool,
) -> Result<Option<u8>, String> {
    match quality {
        Some(_) if !format.is_lossy() && !named_encoder => {
            Err("quality applies to JPEG and WebP output; PNG is lossless".into())
        }
        Some(quality) if !(1..=StillExport::MAX_QUALITY).contains(&quality) => Err(format!(
            "quality must be 1 to {}, not {quality}",
            StillExport::MAX_QUALITY
        )),
        quality => Ok(quality),
    }
}

fn job(
    source: Source,
    output: Option<PathBuf>,
    stills: Option<StillExport>,
    options: ExportOptions,
) -> ExportJob {
    ExportJob {
        source: source.path,
        react: source.react,
        composition_id: source.composition,
        companion: source.companion,
        props_file: source.props_file,
        props: source.props,
        output,
        stills,
        options,
    }
}

impl RenderFramesArgs {
    pub fn job(&self, outputs: &OutputDirs) -> Result<ExportJob, String> {
        reject_unknown(&self.unknown)?;
        let debug_guides = self.debug_guides.unwrap_or(false);
        let source = self.source.check(debug_guides)?;
        let frames = selection(&self.frames, self.every, source.range.as_ref())?;
        let format = StillFormat::from(self.format.unwrap_or_default());
        let quality = still_quality(
            format,
            self.quality,
            self.video_encoder
                .as_ref()
                .is_some_and(|encoder| encoder.id.is_some()),
        )?;
        let dir = match &self.output_dir {
            Some(dir) => {
                let dir = absolute(dir);
                std::fs::create_dir_all(&dir)
                    .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
                dir
            }
            None => outputs.fresh().map_err(|error| error.to_string())?,
        };
        let options = ExportOptions {
            overwrite: self.overwrite.unwrap_or(false),
            range: source.range,
            output: still_choice(&self.muxer, &self.video_encoder, self.quality)?,
            ..ExportOptions::default()
        };
        let stills = StillExport {
            frames,
            contact_sheet: None,
            debug_guides,
            quality,
        };
        let output = dir.join(format!("frame.{format}"));
        Ok(job(source, Some(output), Some(stills), options))
    }
}

impl ContactSheetArgs {
    pub fn job(&self, outputs: &OutputDirs) -> Result<ExportJob, String> {
        reject_unknown(&self.unknown)?;
        let debug_guides = self.debug_guides.unwrap_or(false);
        let source = self.source.check(debug_guides)?;
        let frames = selection(&self.frames, self.every, source.range.as_ref())?;
        let columns = self.columns.unwrap_or(ContactSheet::DEFAULT_COLUMNS);
        let tile_width = self.tile_width.unwrap_or(ContactSheet::DEFAULT_TILE_WIDTH);
        if columns == 0 {
            return Err("columns must be at least 1".into());
        }
        if tile_width == 0 {
            return Err("tileWidth must be at least 1".into());
        }
        let (output, format) = match &self.output {
            Some(output) => {
                let (output, format) = still_path(output)?;
                if let Some(given) = self.format
                    && StillFormat::from(given) != format
                {
                    return Err(format!(
                        "format {} disagrees with the output extension, which picks {format}; leave format out or change the extension",
                        StillFormat::from(given)
                    ));
                }
                (output, format)
            }
            None => {
                let format = StillFormat::from(self.format.unwrap_or_default());
                let dir = outputs.fresh().map_err(|error| error.to_string())?;
                (dir.join(format!("sheet.{format}")), format)
            }
        };
        let quality = still_quality(
            format,
            self.quality,
            self.video_encoder
                .as_ref()
                .is_some_and(|encoder| encoder.id.is_some()),
        )?;
        let options = ExportOptions {
            overwrite: self.overwrite.unwrap_or(false),
            range: source.range,
            output: still_choice(&self.muxer, &self.video_encoder, self.quality)?,
            ..ExportOptions::default()
        };
        let stills = StillExport {
            frames,
            contact_sheet: Some(ContactSheet {
                columns,
                tile_width,
            }),
            debug_guides,
            quality,
        };
        Ok(job(source, Some(output), Some(stills), options))
    }
}

impl ExportVideoArgs {
    pub fn job(&self) -> Result<ExportJob, String> {
        reject_unknown(&self.unknown)?;
        let source = self.source.check(false)?;
        let subtitles = self
            .subtitles
            .as_ref()
            .map(SubtitlePaths::absolute)
            .unwrap_or_default();
        let Some(output) = &self.output else {
            if subtitles.is_empty() {
                return Err("give output, or subtitles to write only subtitle files".into());
            }
            let given = [
                ("preset", self.preset.is_some()),
                ("presetFile", self.preset_file.is_some()),
                ("muxer", self.muxer.is_some()),
                ("videoEncoder", self.video_encoder.is_some()),
                ("audioEncoder", self.audio_encoder.is_some()),
                ("loop", self.looping.is_some()),
                ("loudness", self.loudness.is_some()),
            ];
            if let Some((field, _)) = given.into_iter().find(|(_, given)| *given) {
                return Err(format!(
                    "{field} needs an output video; without one only subtitles are written"
                ));
            }
            let options = ExportOptions {
                overwrite: self.overwrite.unwrap_or(false),
                range: source.range,
                subtitles,
                ..ExportOptions::default()
            };
            return Ok(job(source, None, None, options));
        };
        if StillFormat::from_extension(output).is_some() {
            return Err(format!(
                "output must be a video file, not {}; use render_frames for still images",
                output.display()
            ));
        }
        let preset_file = self.preset_file.as_deref().map(absolute);
        if let Some(file) = &preset_file {
            check_preset_file(file)?;
        }
        let choice = OutputChoice {
            preset: self.preset.clone(),
            muxer: self.muxer.as_deref().map(component_ref).transpose()?,
            video: self
                .video_encoder
                .as_ref()
                .map(|encoder| encoder.choice("videoEncoder"))
                .transpose()?
                .unwrap_or_default(),
            audio: match &self.audio_encoder {
                None => AudioChoice::default(),
                Some(AudioEncoderArg::None(_)) => AudioChoice::None,
                Some(AudioEncoderArg::Encode(encoder)) => {
                    AudioChoice::Encode(encoder.choice("audioEncoder")?)
                }
            },
        };
        let loudness = self.loudness()?;
        let options = ExportOptions {
            overwrite: self.overwrite.unwrap_or(false),
            range: source.range,
            output: choice,
            subtitles,
            looping: self.looping.unwrap_or(false),
            loudness,
            presets: PresetLookup::for_source(&source.path, preset_file),
            ..ExportOptions::default()
        };
        Ok(job(source, Some(absolute(output)), None, options))
    }

    fn loudness(&self) -> Result<Option<LoudnessTarget>, String> {
        let in_range = |value: f64, (low, high): (f64, f64)| (low..=high).contains(&value);
        let target = match &self.loudness {
            None => {
                let given = [
                    ("truePeak", self.true_peak.is_some()),
                    ("peakHandling", self.peak_handling.is_some()),
                ];
                if let Some((field, _)) = given.into_iter().find(|(_, given)| *given) {
                    return Err(format!("{field} applies with loudness only"));
                }
                return Ok(None);
            }
            Some(LoudnessArg::Lufs(lufs)) if in_range(*lufs, LoudnessTarget::INTEGRATED_RANGE) => {
                LoudnessTarget::new(*lufs)
            }
            Some(LoudnessArg::Preset(name)) if name == "streaming" => LoudnessTarget::STREAMING,
            Some(LoudnessArg::Preset(name)) if name == "broadcast" => LoudnessTarget::BROADCAST,
            Some(_) => {
                return Err(
                    "loudness must be -70 to -5 LUFS, \"streaming\" or \"broadcast\"".into(),
                );
            }
        };
        let true_peak_dbtp = match self.true_peak {
            Some(dbtp) if !in_range(dbtp, LoudnessTarget::TRUE_PEAK_RANGE) => {
                return Err(format!("truePeak must be -9 to 0 dBTP, not {dbtp}"));
            }
            Some(dbtp) => dbtp,
            None => target.true_peak_dbtp,
        };
        let peak_handling = match &self.peak_handling {
            Some(name) => name.parse::<PeakHandling>().map_err(|_| {
                format!("peakHandling must be \"limit\" or \"lower\", not {name:?}")
            })?,
            None => target.peak_handling,
        };
        Ok(Some(LoudnessTarget {
            true_peak_dbtp,
            peak_handling,
            ..target
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn outputs() -> OutputDirs {
        OutputDirs::new().unwrap()
    }

    fn frames_job(args: Value) -> Result<ExportJob, String> {
        serde_json::from_value::<RenderFramesArgs>(args)
            .unwrap()
            .job(&outputs())
    }

    fn sheet_job(args: Value) -> Result<ExportJob, String> {
        serde_json::from_value::<ContactSheetArgs>(args)
            .unwrap()
            .job(&outputs())
    }

    fn video_job(args: Value) -> Result<ExportJob, String> {
        serde_json::from_value::<ExportVideoArgs>(args)
            .unwrap()
            .job()
    }

    fn rejects(result: Result<ExportJob, String>, names: &str) {
        let error = result.expect_err("a usage error");
        assert!(error.contains(names), "{error:?} does not name {names}");
    }

    #[test]
    fn frames_and_every_are_exclusive() {
        rejects(
            frames_job(json!({ "source": "a.tsx", "frames": [0], "every": 2 })),
            "every",
        );
        rejects(frames_job(json!({ "source": "a.tsx" })), "frames");
        rejects(
            frames_job(json!({ "source": "a.tsx", "frames": [] })),
            "frames",
        );
        rejects(sheet_job(json!({ "source": "a.tsx" })), "every");
    }

    #[test]
    fn counts_must_be_positive() {
        rejects(
            frames_job(json!({ "source": "a.tsx", "every": 0 })),
            "every",
        );
        rejects(
            sheet_job(json!({ "source": "a.tsx", "every": 5, "columns": 0 })),
            "columns",
        );
        rejects(
            sheet_job(json!({ "source": "a.tsx", "every": 5, "tileWidth": 0 })),
            "tileWidth",
        );
    }

    #[test]
    fn frames_cannot_take_a_range() {
        rejects(
            frames_job(json!({ "source": "a.tsx", "frames": [0], "start": "1" })),
            "start/end",
        );
    }

    #[test]
    fn timecodes_are_checked() {
        rejects(
            frames_job(json!({ "source": "a.tsx", "every": 5, "start": "soon" })),
            "start",
        );
        rejects(
            frames_job(json!({ "source": "a.tsx", "every": 5, "start": "2", "end": "1" })),
            "end",
        );
        let job =
            frames_job(json!({ "source": "a.tsx", "every": 5, "start": "1", "end": "2" })).unwrap();
        assert!(job.options.range.is_some());
    }

    #[test]
    fn react_arguments_on_json_projects_are_rejected() {
        for extra in [
            json!({ "props": { "a": 1 } }),
            json!({ "propsFile": "p.json" }),
            json!({ "project": "p.celesta.json" }),
            json!({ "debugGuides": true }),
        ] {
            let mut args = json!({ "source": "a.celesta.json", "frames": [0] });
            args.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            rejects(frames_job(args), "React");
        }
    }

    #[test]
    fn outputs_need_the_right_extension() {
        rejects(
            sheet_job(json!({ "source": "a.tsx", "every": 5, "output": "a.gif" })),
            ".png, .jpg, .jpeg or .webp",
        );
        rejects(
            video_job(json!({ "source": "a.tsx", "output": "a.png" })),
            "render_frames",
        );
    }

    #[test]
    fn contact_sheet_output_extension_picks_the_format() {
        for output in ["a.png", "a.jpg", "a.JPEG", "a.webp"] {
            let job =
                sheet_job(json!({ "source": "a.tsx", "every": 5, "output": output })).unwrap();
            let path = job.output.unwrap();
            assert!(path.ends_with(output), "{}", path.display());
        }
        let job = sheet_job(json!({ "source": "a.tsx", "every": 5, "format": "webp" })).unwrap();
        assert_eq!(job.output.unwrap().file_name().unwrap(), "sheet.webp");
        let job = sheet_job(json!({
            "source": "a.tsx", "every": 5, "format": "jpeg", "output": "a.jpg"
        }))
        .unwrap();
        assert!(job.output.unwrap().ends_with("a.jpg"));
        rejects(
            sheet_job(json!({
                "source": "a.tsx", "every": 5, "format": "webp", "output": "a.png"
            })),
            "format webp",
        );
    }

    #[test]
    fn render_frames_format_names_the_files() {
        let output = |format: Value| {
            frames_job(json!({ "source": "a.tsx", "frames": [0], "format": format }))
                .unwrap()
                .output
                .unwrap()
        };
        assert_eq!(output(json!("png")).file_name().unwrap(), "frame.png");
        assert_eq!(output(json!("jpeg")).file_name().unwrap(), "frame.jpeg");
        assert_eq!(output(json!("jpg")).file_name().unwrap(), "frame.jpeg");
        assert_eq!(output(json!("webp")).file_name().unwrap(), "frame.webp");
        let job = frames_job(json!({ "source": "a.tsx", "frames": [0] })).unwrap();
        assert_eq!(job.output.unwrap().file_name().unwrap(), "frame.png");
        assert!(
            serde_json::from_value::<RenderFramesArgs>(
                json!({ "source": "a.tsx", "frames": [0], "format": "gif" })
            )
            .is_err()
        );
    }

    #[test]
    fn still_tools_take_components() {
        let reference = |plugin: &str, component: &str| ComponentRef {
            plugin: plugin.into(),
            component: component.into(),
        };
        let job = frames_job(json!({
            "source": "a.tsx", "frames": [0], "format": "webp",
            "videoEncoder": { "options": { "lossless": true } }
        }))
        .unwrap();
        assert_eq!(
            job.options.output.video.options["lossless"],
            SettingInput::Value(SettingValue::Boolean(true))
        );
        let job = sheet_job(json!({
            "source": "a.tsx", "every": 5, "muxer": "builtin/image",
            "videoEncoder": { "id": "builtin/png", "options": { "depth": "16" } }
        }))
        .unwrap();
        assert_eq!(
            job.options.output.video.options["depth"],
            SettingInput::Value(SettingValue::String("16".into()))
        );
        assert_eq!(
            job.options.output.muxer,
            Some(reference("builtin", "image"))
        );
        assert_eq!(
            job.options.output.video.component,
            Some(reference("builtin", "png"))
        );
        rejects(
            frames_job(json!({ "source": "a.tsx", "frames": [0], "muxer": "avif" })),
            "`avif` is not a component reference",
        );
        rejects(
            frames_job(json!({
                "source": "a.tsx", "frames": [0], "format": "jpeg", "quality": 50,
                "videoEncoder": { "options": { "quality": 60 } }
            })),
            "both set the quality",
        );
    }

    #[test]
    fn quality_applies_to_jpeg_and_webp_only() {
        let quality = |job: ExportJob| job.stills.unwrap().quality;
        assert_eq!(
            quality(
                frames_job(
                    json!({ "source": "a.tsx", "frames": [0], "format": "jpeg", "quality": 75 })
                )
                .unwrap()
            ),
            Some(75)
        );
        assert_eq!(
            quality(
                frames_job(json!({ "source": "a.tsx", "frames": [0], "format": "webp" })).unwrap()
            ),
            None
        );
        assert_eq!(
            quality(
                sheet_job(
                    json!({ "source": "a.tsx", "every": 5, "output": "a.webp", "quality": 1 })
                )
                .unwrap()
            ),
            Some(1)
        );
        rejects(
            frames_job(json!({ "source": "a.tsx", "frames": [0], "quality": 80 })),
            "PNG is lossless",
        );
        rejects(
            sheet_job(json!({ "source": "a.tsx", "every": 5, "quality": 80 })),
            "PNG is lossless",
        );
        rejects(
            sheet_job(json!({ "source": "a.tsx", "every": 5, "output": "a.png", "quality": 80 })),
            "PNG is lossless",
        );
        for quality in [0, 101] {
            rejects(
                frames_job(json!({
                    "source": "a.tsx", "frames": [0], "format": "jpeg", "quality": quality
                })),
                "quality must be 1 to 100",
            );
            rejects(
                sheet_job(json!({
                    "source": "a.tsx", "every": 5, "output": "a.webp", "quality": quality
                })),
                "quality must be 1 to 100",
            );
        }
    }

    #[test]
    fn component_arguments_build_the_choice() {
        let job = video_job(json!({
            "source": "a.tsx", "output": "a.mp4", "muxer": "ffmpeg/mp4",
            "videoEncoder": { "id": "ffmpeg-x264/libx264", "options": { "crf": 20, "preset": "slow" } },
            "audioEncoder": "none",
        }))
        .unwrap();
        let reference = |plugin: &str, component: &str| ComponentRef {
            plugin: plugin.into(),
            component: component.into(),
        };
        assert_eq!(
            job.options.output,
            OutputChoice {
                preset: None,
                muxer: Some(reference("ffmpeg", "mp4")),
                video: EncoderChoice {
                    component: Some(reference("ffmpeg-x264", "libx264")),
                    options: [
                        (
                            "crf".to_owned(),
                            SettingInput::Value(SettingValue::Number(20.0))
                        ),
                        (
                            "preset".to_owned(),
                            SettingInput::Value(SettingValue::String("slow".into()))
                        ),
                    ]
                    .into(),
                },
                audio: AudioChoice::None,
            }
        );
        let plain = video_job(json!({ "source": "a.tsx", "output": "a.mp4" })).unwrap();
        assert_eq!(plain.options.output, OutputChoice::default());
    }

    #[test]
    fn option_values_must_be_scalars() {
        rejects(
            video_job(json!({
                "source": "a.tsx", "output": "a.mp4",
                "videoEncoder": { "options": { "crf": [1] } }
            })),
            "crf",
        );
        rejects(
            video_job(json!({
                "source": "a.tsx", "output": "a.mp4",
                "audioEncoder": { "options": { "bitrate": null } }
            })),
            "bitrate",
        );
    }

    #[test]
    fn a_bad_reference_is_a_usage_error() {
        rejects(
            video_job(json!({ "source": "a.tsx", "output": "a.mp4", "muxer": "mp4" })),
            "`mp4` is not a component reference",
        );
        rejects(
            video_job(json!({
                "source": "a.tsx", "output": "a.mp4", "videoEncoder": { "id": "x264" }
            })),
            "`x264` is not a component reference",
        );
    }

    #[test]
    fn loop_applies_to_every_video() {
        for output in ["a.mp4", "a.mov", "a.webm"] {
            let job =
                video_job(json!({ "source": "a.tsx", "output": output, "loop": true })).unwrap();
            assert!(job.options.looping, "{output}");
        }
    }

    #[test]
    fn a_preset_file_must_be_a_presets_file() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, text: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, text).unwrap();
            path
        };
        let job = |path: &Path| {
            video_job(json!({ "source": "a.tsx", "output": "a.mp4", "presetFile": path }))
        };
        rejects(job(&write("none.json", "{}")), "presetFile");
        rejects(
            job(&write("future.json", r#"{"version": 2}"#)),
            "unsupported version 2",
        );
        rejects(job(&write("bad.json", "{")), "not valid JSON");
        rejects(job(&dir.path().join("missing.json")), "presetFile");

        let ok = write("ok.json", r#"{"version": 1, "presets": []}"#);
        let job = job(&ok).unwrap();
        assert_eq!(job.options.presets.file.as_deref(), Some(ok.as_path()));
        assert_eq!(
            job.options.presets.project,
            Some(celesta_plugin_host::project_root(&job.source).join("export-presets.json"))
        );
    }

    #[test]
    fn old_fields_are_unknown() {
        for field in ["crf", "bitDepth"] {
            rejects(
                video_job(json!({ "source": "a.tsx", "output": "a.mp4", field: 20 })),
                field,
            );
        }
    }

    #[test]
    fn loudness_takes_lufs_or_a_preset() {
        let loudness = |args: Value| video_job(args).map(|job| job.options.loudness);
        assert_eq!(
            loudness(json!({ "source": "a.tsx", "output": "a.mp4" })),
            Ok(None)
        );
        assert_eq!(
            loudness(json!({
                "source": "a.tsx", "output": "a.webm", "loudness": -16, "truePeak": -2
            })),
            Ok(Some(LoudnessTarget {
                true_peak_dbtp: -2.0,
                ..LoudnessTarget::new(-16.0)
            }))
        );
        assert_eq!(
            loudness(json!({
                "source": "a.tsx", "output": "a.mov", "loudness": "streaming",
                "peakHandling": "lower"
            })),
            Ok(Some(LoudnessTarget {
                peak_handling: PeakHandling::Lower,
                ..LoudnessTarget::STREAMING
            }))
        );
        for (args, names) in [
            (
                json!({ "source": "a.tsx", "output": "a.mp4", "loudness": -2 }),
                "loudness",
            ),
            (
                json!({ "source": "a.tsx", "output": "a.mp4", "loudness": "loud" }),
                "loudness",
            ),
            (
                json!({ "source": "a.tsx", "output": "a.mp4", "loudness": -14, "truePeak": 1 }),
                "truePeak",
            ),
            (
                json!({
                    "source": "a.tsx", "output": "a.mp4", "loudness": -14,
                    "peakHandling": "clip"
                }),
                "peakHandling",
            ),
            (
                json!({ "source": "a.tsx", "output": "a.mp4", "truePeak": -1 }),
                "truePeak",
            ),
            (
                json!({ "source": "a.tsx", "subtitles": "a.srt", "loudness": -14 }),
                "loudness needs an output video",
            ),
        ] {
            rejects(video_job(args), names);
        }
    }

    #[test]
    fn a_looping_video_is_asked_for_with_loop() {
        let looping = |args: Value| video_job(args).unwrap().options.looping;
        assert!(looping(
            json!({ "source": "a.tsx", "output": "a.mp4", "loop": true })
        ));
        assert!(!looping(json!({ "source": "a.tsx", "output": "a.mp4" })));
        rejects(
            video_job(json!({ "source": "a.tsx", "subtitles": "a.srt", "loop": true })),
            "loop needs an output video",
        );
    }

    #[test]
    fn subtitles_ride_along_with_a_video() {
        let job = video_job(json!({ "source": "a.tsx", "output": "a.mp4" })).unwrap();
        assert!(job.stills.is_none());
        assert!(job.options.subtitles.is_empty());
        let job = video_job(json!({
            "source": "a.tsx", "output": "a.mp4", "subtitles": "a.srt"
        }))
        .unwrap();
        assert_eq!(job.options.subtitles.len(), 1);
        assert!(job.options.subtitles[0].is_absolute());
    }

    #[test]
    fn subtitles_can_be_several_and_stand_alone() {
        let job = video_job(json!({
            "source": "a.tsx", "subtitles": ["a.{lang}.srt", "a.{lang}.vtt"], "end": "2"
        }))
        .unwrap();
        assert_eq!(job.output, None);
        assert!(job.stills.is_none());
        assert!(job.options.range.is_some());
        assert_eq!(job.options.subtitles.len(), 2);
        assert!(job.options.subtitles[1].ends_with("a.{lang}.vtt"));
        rejects(video_job(json!({ "source": "a.tsx" })), "subtitles");
        rejects(
            video_job(json!({ "source": "a.tsx", "subtitles": "a.srt", "muxer": "a/b" })),
            "muxer",
        );
    }

    #[test]
    fn relative_source_becomes_absolute() {
        let job = frames_job(json!({ "source": "a.tsx", "frames": [0, -1] })).unwrap();
        assert!(job.source.is_absolute());
        assert!(job.react);
        assert!(job.output.unwrap().is_absolute());
        assert_eq!(
            job.stills.unwrap().frames,
            celesta_exporter::FrameSelection::FromEnd(vec![0, -1])
        );
        let job = frames_job(json!({ "source": "a.celesta.json", "every": 5 })).unwrap();
        assert!(!job.react);
    }

    #[test]
    fn each_call_without_an_output_gets_its_own_folder() {
        let outputs = outputs();
        let args = || {
            serde_json::from_value::<RenderFramesArgs>(json!({ "source": "a.tsx", "frames": [0] }))
                .unwrap()
        };
        let first = args().job(&outputs).unwrap().output.unwrap();
        let second = args().job(&outputs).unwrap().output.unwrap();
        assert_ne!(first.parent(), second.parent());
        assert!(first.parent().unwrap().is_dir());
    }
}
