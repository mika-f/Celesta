use crate::presets::PresetLookup;
use crate::range::ExportRange;
use crate::{GpuDriver, RenderQuality};
use celesta_media::{ComponentRef, LoudnessTarget, SettingValue};
use celesta_plugin_host::InstalledPlugins;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportOptions {
    pub overwrite: bool,
    /// When set, only this composition-time span is rendered; the encoded
    /// output starts at its own `t = 0` (both video and the mixed audio are
    /// shifted so the span's start becomes the file's start). `None` exports
    /// the whole composition, byte-for-byte as before.
    pub range: Option<ExportRange>,
    /// The muxer and encoders a video is written with, and their options:
    /// by default the muxer the output's extension names, with its default
    /// encoders.
    pub output: OutputChoice,
    /// How carefully scaled and rotated layers are drawn. Exports default to
    /// [`RenderQuality::Final`]; this is independent of the encoders'
    /// settings.
    pub render_quality: RenderQuality,
    /// The graphics API frames are rendered through.
    pub driver: GpuDriver,
    /// A video export also writes its subtitles to each of these, as SRT or
    /// WebVTT by the extension, timed like the video: a React entry's
    /// `<Subtitles>` cues and `<Dialogue>` lines, or a project's dialogue
    /// items. A `{lang}` in a path writes one file per language, with the
    /// language in its place (`und` for subtitles without one); a path
    /// without it takes subtitles of one language only.
    pub subtitles: Vec<PathBuf>,
    /// The video is played in a loop, so its sound runs from its end into
    /// its start. For a muxer that trims leading audio (MP4 and MOV), the
    /// audio encoder then starts from the end of the mix instead of silence,
    /// which keeps the first hit from smearing on every pass; the decoded
    /// sound is still exactly the mix. Other muxers (WebM) are written as
    /// without it.
    pub looping: bool,
    /// Brings the whole mix to this integrated loudness before it is
    /// encoded, keeping its true peaks under the target's ceiling. A video
    /// export measures its mix either way and reports the result in
    /// [`ExportedFile::Video`](crate::ExportedFile::Video).
    pub loudness: Option<LoudnessTarget>,
    /// The content plugins the renderer and the React runtime use. `None`
    /// discovers them when the [`Exporter`](crate::Exporter) is made, in
    /// every location Celesta searches (see
    /// [`InstalledPlugins::find`]); a caller that already discovered them,
    /// as the editor and `--plugin-dir` do, passes its set.
    pub plugins: Option<InstalledPlugins>,
    /// The files [`OutputChoice::preset`] and a project's `default` preset
    /// are looked up in. The default reads none, so a library caller is not
    /// changed by a file in the user's home; the CLI, MCP, and the editor
    /// set it from the export's source ([`PresetLookup::for_source`]).
    pub presets: PresetLookup,
}

/// What an export writes its video with, before
/// [`resolve_output`](crate::resolve_output) turns it into components: a
/// preset, a muxer, and the video and audio encoders, each with options.
/// The default takes the muxer the output's extension names and that
/// muxer's default encoders, with their default settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutputChoice {
    /// A preset's name, looked up in [`ExportOptions::presets`]: its
    /// components and options are the base the rest of the choice goes
    /// over. A name found in no file is
    /// [`ExportError::UnknownPreset`](crate::ExportError::UnknownPreset).
    pub preset: Option<String>,
    pub muxer: Option<ComponentRef>,
    pub video: EncoderChoice,
    pub audio: AudioChoice,
}

/// An encoder, or the muxer's default when `component` is `None`, and the
/// options given for it by key.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EncoderChoice {
    pub component: Option<ComponentRef>,
    pub options: BTreeMap<String, SettingInput>,
}

/// The audio of an export: encoded (by the muxer's default audio encoder
/// when the choice names none), or left out.
#[derive(Clone, Debug, PartialEq)]
pub enum AudioChoice {
    Encode(EncoderChoice),
    None,
}

impl Default for AudioChoice {
    fn default() -> Self {
        Self::Encode(EncoderChoice::default())
    }
}

/// One option as given: text, read as the field's type once the
/// component's schema is known (the command line, and
/// `@celesta.studio/exporter`, which passes its options to the command line
/// as `--video-option KEY=VALUE`), or a typed value (MCP, presets).
#[derive(Clone, Debug, PartialEq)]
pub enum SettingInput {
    Text(String),
    Value(SettingValue),
}
