//! Arguments of the rendering tools, checked and turned into export jobs.

use std::path::{Path, PathBuf};

use celesta_exporter::{
    BitDepth, ContactSheet, EncoderPreset, ExportJob, ExportOptions, ExportRange, FrameSelection,
    PngExport, VideoEncoding, VideoFormat, parse_timecode,
};
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
    /// The folder to write PNGs to; default: a temporary folder.
    output_dir: Option<PathBuf>,
    /// Replace PNGs that already exist in `outputDir`.
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
    /// The `.png` to write; default: a temporary file.
    output: Option<PathBuf>,
    /// Replace `output` if it exists.
    overwrite: Option<bool>,
    /// Arguments no field above takes; any are a usage error.
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: Map<String, Value>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportVideoArgs {
    #[serde(flatten)]
    source: SourceArgs,
    /// The .mp4, .mov (ProRes 4444 with alpha) or .webm (VP9 with alpha) to
    /// write. Leave it out to write only `subtitles`, rendering nothing.
    output: Option<PathBuf>,
    /// Replace `output` and subtitle files that exist.
    overwrite: Option<bool>,
    /// libx264 preset, `ultrafast` to `veryslow`; default `medium`.
    preset: Option<String>,
    /// Constant rate factor, 0–51; lower is higher quality. Default 18.
    crf: Option<u8>,
    /// 8 (default, plays everywhere) or 10 (smoother gradients, fewer
    /// hardware decoders).
    bit_depth: Option<u8>,
    /// Also write the subtitles (`<Subtitles>` cues, `<Dialogue>` lines, or
    /// a project's dialogue items) to this .srt or .vtt file, or each of
    /// these, timed like the video. A path with `{lang}` (`film.{lang}.vtt`)
    /// is one file per `<Subtitles lang>`, holding that language's cues and
    /// those without a lang.
    subtitles: Option<OneOrMore>,
    /// Arguments no field above takes; any are a usage error.
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: Map<String, Value>,
}

/// A path, or a list of them.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum OneOrMore {
    One(PathBuf),
    More(Vec<PathBuf>),
}

impl OneOrMore {
    fn paths(&self) -> &[PathBuf] {
        match self {
            Self::One(path) => std::slice::from_ref(path),
            Self::More(paths) => paths,
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

fn png_path(output: &Path) -> Result<PathBuf, String> {
    if !output
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        return Err(format!(
            "output must be a .png file, not {}",
            output.display()
        ));
    }
    Ok(absolute(output))
}

fn job(
    source: Source,
    output: Option<PathBuf>,
    png: Option<PngExport>,
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
        png,
        options,
    }
}

impl RenderFramesArgs {
    pub fn job(&self, outputs: &OutputDirs) -> Result<ExportJob, String> {
        reject_unknown(&self.unknown)?;
        let debug_guides = self.debug_guides.unwrap_or(false);
        let source = self.source.check(debug_guides)?;
        let frames = selection(&self.frames, self.every, source.range.as_ref())?;
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
            ..ExportOptions::default()
        };
        let png = PngExport {
            frames,
            contact_sheet: None,
            debug_guides,
        };
        Ok(job(source, Some(dir.join("frame.png")), Some(png), options))
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
        let output = match &self.output {
            Some(output) => png_path(output)?,
            None => outputs
                .fresh()
                .map_err(|error| error.to_string())?
                .join("sheet.png"),
        };
        let options = ExportOptions {
            overwrite: self.overwrite.unwrap_or(false),
            range: source.range,
            ..ExportOptions::default()
        };
        let png = PngExport {
            frames,
            contact_sheet: Some(ContactSheet {
                columns,
                tile_width,
            }),
            debug_guides,
        };
        Ok(job(source, Some(output), Some(png), options))
    }
}

impl ExportVideoArgs {
    pub fn job(&self) -> Result<ExportJob, String> {
        reject_unknown(&self.unknown)?;
        let source = self.source.check(false)?;
        let subtitles: Vec<PathBuf> = self
            .subtitles
            .as_ref()
            .map_or(&[][..], OneOrMore::paths)
            .iter()
            .map(|path| absolute(path))
            .collect();
        let Some(output) = &self.output else {
            if subtitles.is_empty() {
                return Err("give output, subtitles, or both".into());
            }
            let given = [
                ("preset", self.preset.is_some()),
                ("crf", self.crf.is_some()),
                ("bitDepth", self.bit_depth.is_some()),
            ];
            if let Some((field, _)) = given.into_iter().find(|(_, given)| *given) {
                return Err(format!(
                    "{field} applies to a video; without output only the subtitle files are written"
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
        let Some(video_format) = VideoFormat::from_extension(output) else {
            return Err(format!(
                "output must be a .mp4, .mov or .webm file, not {}; use render_frames for PNG",
                output.display()
            ));
        };
        if video_format.has_alpha() {
            let given = [
                ("preset", self.preset.is_some()),
                ("crf", self.crf.is_some()),
                ("bitDepth", self.bit_depth.is_some()),
            ];
            if let Some((field, _)) = given.into_iter().find(|(_, given)| *given) {
                return Err(format!("{field} applies to MP4 output only"));
            }
        }
        let defaults = VideoEncoding::default();
        let preset = match &self.preset {
            Some(name) => name.parse::<EncoderPreset>().map_err(|_| {
                format!(
                    "unknown preset {name:?}; use one of {}",
                    EncoderPreset::ALL.map(EncoderPreset::as_str).join(", ")
                )
            })?,
            None => defaults.preset,
        };
        let bit_depth = match self.bit_depth {
            None | Some(8) => BitDepth::Eight,
            Some(10) => BitDepth::Ten,
            Some(other) => return Err(format!("bitDepth must be 8 or 10, not {other}")),
        };
        let options = ExportOptions {
            overwrite: self.overwrite.unwrap_or(false),
            range: source.range,
            video_format,
            video: VideoEncoding {
                preset,
                crf: self.crf.unwrap_or(defaults.crf),
                bit_depth,
                ..defaults
            },
            subtitles,
            ..ExportOptions::default()
        };
        Ok(job(source, Some(absolute(output)), None, options))
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
            sheet_job(json!({ "source": "a.tsx", "every": 5, "output": "a.jpg" })),
            ".png",
        );
        rejects(
            video_job(json!({ "source": "a.tsx", "output": "a.png" })),
            ".mp4, .mov or .webm",
        );
    }

    #[test]
    fn transparent_formats_follow_the_extension() {
        let format = |output: &str| {
            video_job(json!({ "source": "a.tsx", "output": output }))
                .unwrap()
                .options
                .video_format
        };
        assert_eq!(format("a.mp4"), VideoFormat::Mp4);
        assert_eq!(format("a.mov"), VideoFormat::Mov);
        assert_eq!(format("a.WEBM"), VideoFormat::Webm);
        rejects(
            video_job(json!({ "source": "a.tsx", "output": "a.mov", "crf": 20 })),
            "crf applies to MP4",
        );
        rejects(
            video_job(json!({ "source": "a.tsx", "output": "a.webm", "preset": "fast" })),
            "preset applies to MP4",
        );
        rejects(
            video_job(json!({ "source": "a.tsx", "output": "a.mov", "bitDepth": 10 })),
            "bitDepth applies to MP4",
        );
    }

    #[test]
    fn encoder_settings_are_checked() {
        rejects(
            video_job(json!({ "source": "a.tsx", "output": "a.mp4", "preset": "warp" })),
            "preset",
        );
        rejects(
            video_job(json!({ "source": "a.tsx", "output": "a.mp4", "bitDepth": 12 })),
            "bitDepth",
        );
        let job = video_job(json!({
            "source": "a.tsx", "output": "a.mp4", "preset": "fast", "crf": 20, "bitDepth": 10
        }))
        .unwrap();
        assert_eq!(job.options.video.crf, 20);
        assert_eq!(job.options.video.bit_depth, BitDepth::Ten);
        assert!(job.png.is_none());
        assert!(job.options.subtitles.is_empty());
        let job = video_job(json!({
            "source": "a.tsx", "output": "a.mp4", "subtitles": "a.srt"
        }))
        .unwrap();
        assert_eq!(job.options.subtitles.len(), 1);
        assert!(job.options.subtitles[0].is_absolute());
    }

    #[test]
    fn subtitles_take_a_list_and_go_without_a_video() {
        let job = video_job(json!({
            "source": "a.tsx", "output": "a.mp4",
            "subtitles": ["a.{lang}.srt", "a.{lang}.vtt"]
        }))
        .unwrap();
        let names: Vec<_> = job
            .options
            .subtitles
            .iter()
            .map(|path| path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, ["a.{lang}.srt", "a.{lang}.vtt"]);
        assert!(job.output.is_some());

        let job = video_job(json!({
            "source": "a.tsx", "subtitles": ["a.srt"], "start": "1", "overwrite": true
        }))
        .unwrap();
        assert_eq!(job.output, None);
        assert!(job.png.is_none());
        assert!(job.options.overwrite);
        assert!(job.options.range.is_some());

        rejects(video_job(json!({ "source": "a.tsx" })), "output");
        rejects(
            video_job(json!({ "source": "a.tsx", "subtitles": "a.srt", "crf": 20 })),
            "crf",
        );
    }

    #[test]
    fn relative_source_becomes_absolute() {
        let job = frames_job(json!({ "source": "a.tsx", "frames": [0, -1] })).unwrap();
        assert!(job.source.is_absolute());
        assert!(job.react);
        assert!(job.output.unwrap().is_absolute());
        assert_eq!(
            job.png.unwrap().frames,
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
