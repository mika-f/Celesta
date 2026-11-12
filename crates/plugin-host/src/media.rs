//! Media plugins: the decoders a `media` plugin's manifest declares, and
//! the recognized FFmpeg build programs beside an installed Celesta.

use crate::console::without_console_window;
use crate::locations::PluginSource;
use crate::manifest::{InstalledPlugins, PluginEntry, PluginRole, is_valid_id};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, mpsc};
use std::time::{Duration, Instant, SystemTime};

/// The ID of the FFmpeg media plugin that comes with Celesta, which ranks
/// first among the bundled plugins.
pub const FFMPEG_PLUGIN_ID: &str = "ffmpeg";

/// The plugin ID of the components Celesta has built in (`builtin/png`),
/// which no plugin may take.
pub const RESERVED_PLUGIN_ID: &str = "builtin";

/// The standard FFmpeg program name. [`InstalledPlugins::media`] recognizes
/// this and [`X264_CODEC_PROGRAM`] beside an installed Celesta, as
/// `cargo install` leaves them, never beside a source build in a Cargo folder.
pub const CODEC_PROGRAM: &str = "celesta-codec-ffmpeg";

/// The optional x264 FFmpeg program name, recognized like [`CODEC_PROGRAM`].
pub const X264_CODEC_PROGRAM: &str = "celesta-codec-ffmpeg-x264";

/// The flag that has each recognized sibling media program print its manifest
/// (`id`, `version`, `roles`, and `media`) as JSON and exit without the protocol:
/// `cargo install` cannot put a manifest file beside it.
pub const MANIFEST_FLAG: &str = "--manifest";

/// How long [`MANIFEST_FLAG`] may take before the program is killed and
/// left out.
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a program that failed [`MANIFEST_FLAG`] is given to have its
/// first stderr line read, once it has exited: what it wrote before exiting
/// is already in the pipe, and a process it left behind holding stderr must
/// not keep discovery waiting.
const STDERR_GRACE: Duration = Duration::from_millis(250);

/// How much of a failed program's stderr is kept for its diagnostic.
const STDERR_LIMIT: usize = 4096;

/// The most of the program's output [`MANIFEST_FLAG`] reads.
const MANIFEST_OUTPUT_LIMIT: u64 = 1 << 20;

/// A manifest's `media`: the components a media plugin offers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaManifest {
    pub decoders: Vec<MediaDecoderEntry>,
    pub video_encoders: Vec<MediaVideoEncoderEntry>,
    pub audio_encoders: Vec<MediaAudioEncoderEntry>,
    pub muxers: Vec<MediaMuxerEntry>,
}

/// A decoder, as a manifest's `media.decoders` declares it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaDecoderEntry {
    /// Unique among the plugin's decoders: the decoder's reference is
    /// `<plugin id>/<id>`.
    pub id: String,
    /// English display text.
    pub name: String,
    /// The file types it reads: lowercase, without the dot (written either
    /// way in the manifest).
    #[serde(default)]
    pub extensions: Vec<String>,
}

/// A video encoder, as a manifest's `media.videoEncoders` declares it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MediaVideoEncoderEntry {
    /// Unique among the plugin's video encoders: the encoder's reference is
    /// `<plugin id>/<id>`.
    pub id: String,
    /// English display text.
    pub name: String,
    /// The codec families it writes ([`codec_family`]).
    #[serde(default)]
    pub codecs: Vec<String>,
    /// The upper bound of the pixel formats it takes, as the manifest names
    /// them. None when any format Celesta produces is possible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_formats: Option<Vec<String>>,
}

/// An audio encoder, as a manifest's `media.audioEncoders` declares it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MediaAudioEncoderEntry {
    /// Unique among the plugin's audio encoders.
    pub id: String,
    /// English display text.
    pub name: String,
    /// The codec families it writes ([`codec_family`]).
    #[serde(default)]
    pub codecs: Vec<String>,
}

/// A muxer, as a manifest's `media.muxers` declares it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MediaMuxerEntry {
    /// Unique among the plugin's muxers.
    pub id: String,
    /// English display text.
    pub name: String,
    /// The file types it writes: lowercase, without the dot.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// The video codec families it takes.
    #[serde(default)]
    pub video: Vec<String>,
    /// The audio codec families it takes; None when it takes no audio track.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<Vec<String>>,
    /// One of `video`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_video: Option<String>,
    /// One of `audio`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_audio: Option<String>,
    /// Whether it wants the leading audio (`leadingAudio`) to trim.
    #[serde(default)]
    pub trims_leading_audio: bool,
    /// Whether it writes several files from one pattern.
    #[serde(default)]
    pub sequence: bool,
}

/// The codec family of a codec string: the part before its first `.`, with
/// `avc3` read as `avc1` and `hev1` as `hvc1`.
pub fn codec_family(codec: &str) -> String {
    let family = codec.split('.').next().unwrap_or(codec);
    match family {
        "avc3" => "avc1",
        "hev1" => "hvc1",
        other => other,
    }
    .to_owned()
}

/// Where a media plugin comes from: the user's own plugins, those that come
/// with Celesta, a project's, or Celesta's built-in components. Ranking
/// does not follow this type's derived `Ord`: defaults rank by
/// `celesta_media::default_rank`, and decoders are tried in the registry's
/// decoder order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MediaOrigin {
    /// `--plugin-dir`, `CELESTA_PLUGIN_PATH`, the user's plugin folder, or a
    /// folder the user added.
    User,
    /// The folder that comes with Celesta, or a recognized FFmpeg plugin
    /// program beside an installed Celesta program.
    Bundled,
    /// A folder the open project lists.
    Project,
    /// Celesta's own, in its process (`builtin/png`). Appended: the derived
    /// order is not used for ranking any more.
    BuiltIn,
}

impl From<PluginSource> for MediaOrigin {
    fn from(source: PluginSource) -> Self {
        match source {
            PluginSource::Explicit | PluginSource::User | PluginSource::Folder => Self::User,
            PluginSource::Bundled => Self::Bundled,
            PluginSource::Project => Self::Project,
        }
    }
}

/// A media plugin's process to start, and the components its manifest
/// declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaPlugin {
    pub id: String,
    /// The manifest's display name ([`Manifest::display_name`]).
    pub name: String,
    pub version: String,
    pub executable: PathBuf,
    /// Where it runs.
    pub directory: PathBuf,
    pub origin: MediaOrigin,
    /// What its manifest declares, each list in manifest order.
    pub media: MediaManifest,
}

/// A manifest's `media` as far as it can be read, and why each part that
/// could not was skipped. A malformed entry is left out without the others.
pub(crate) fn parse_media(value: Value) -> (Option<MediaManifest>, Vec<String>) {
    match value {
        Value::Null => (None, Vec::new()),
        Value::Object(_) => {
            let (media, problems) = MediaManifest::from_json(value);
            (Some(media), problems)
        }
        _ => (None, vec!["its \"media\" is not an object".into()]),
    }
}

impl MediaManifest {
    /// The `media` object `value` as far as it can be read, and why each
    /// entry that could not was skipped: an id that is not valid or repeats
    /// an earlier one of the same kind, an encoder with no `codecs`, a muxer
    /// with no `extensions` or `video`, or a default that is not among the
    /// codecs it takes. Codecs are kept as families ([`codec_family`]),
    /// extensions lowercase without the dot.
    pub fn from_json(value: Value) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let Value::Object(mut media) = value else {
            problems.push("its \"media\" is not an object".into());
            return (Self::default(), problems);
        };
        let manifest = Self {
            decoders: read_entries(
                &mut media,
                "decoders",
                &mut problems,
                |decoder: &MediaDecoderEntry| &decoder.id,
                |decoder| {
                    normalize_extensions(&mut decoder.extensions);
                    Ok(())
                },
            ),
            video_encoders: read_entries(
                &mut media,
                "videoEncoders",
                &mut problems,
                |encoder: &MediaVideoEncoderEntry| &encoder.id,
                |encoder| normalize_codecs(&mut encoder.codecs),
            ),
            audio_encoders: read_entries(
                &mut media,
                "audioEncoders",
                &mut problems,
                |encoder: &MediaAudioEncoderEntry| &encoder.id,
                |encoder| normalize_codecs(&mut encoder.codecs),
            ),
            muxers: read_entries(
                &mut media,
                "muxers",
                &mut problems,
                |muxer: &MediaMuxerEntry| &muxer.id,
                normalize_muxer,
            ),
        };
        (manifest, problems)
    }
}

/// The families of `codecs`, which must not be empty.
fn normalize_codecs(codecs: &mut [String]) -> Result<(), String> {
    if codecs.is_empty() {
        return Err("it has no \"codecs\"".into());
    }
    for codec in codecs {
        *codec = codec_family(codec);
    }
    Ok(())
}

fn normalize_extensions(extensions: &mut [String]) {
    for extension in extensions {
        *extension = extension
            .strip_prefix('.')
            .unwrap_or(extension)
            .to_lowercase();
    }
}

fn normalize_muxer(muxer: &mut MediaMuxerEntry) -> Result<(), String> {
    if muxer.extensions.is_empty() {
        return Err("it has no \"extensions\"".into());
    }
    if muxer.video.is_empty() {
        return Err("it has no \"video\" codecs".into());
    }
    normalize_extensions(&mut muxer.extensions);
    normalize_codecs(&mut muxer.video)?;
    for codec in muxer.audio.iter_mut().flatten() {
        *codec = codec_family(codec);
    }
    for (key, default, list) in [
        ("defaultVideo", &mut muxer.default_video, Some(&muxer.video)),
        (
            "defaultAudio",
            &mut muxer.default_audio,
            muxer.audio.as_ref(),
        ),
    ] {
        if let Some(default) = default {
            *default = codec_family(default);
            if !list.is_some_and(|list| list.contains(default)) {
                return Err(format!(
                    "its \"{key}\" {default:?} is not among the codecs it takes"
                ));
            }
        }
    }
    Ok(())
}

/// The list `media[key]` read entry by entry: each from its own value, so
/// one that cannot be read, has an id that is not valid or taken by an
/// earlier entry, or that `normalize` refuses is left out with its reason in
/// `problems`.
fn read_entries<T: DeserializeOwned>(
    media: &mut Map<String, Value>,
    key: &str,
    problems: &mut Vec<String>,
    id: fn(&T) -> &str,
    normalize: fn(&mut T) -> Result<(), String>,
) -> Vec<T> {
    let entries = match media.remove(key) {
        None | Some(Value::Null) => return Vec::new(),
        Some(Value::Array(entries)) => entries,
        Some(_) => {
            problems.push(format!("its \"media.{key}\" is not a list"));
            return Vec::new();
        }
    };
    let mut kept: Vec<T> = Vec::new();
    for (index, entry) in entries.into_iter().enumerate() {
        let mut entry = match serde_json::from_value::<T>(entry) {
            Ok(entry) => entry,
            Err(error) => {
                problems.push(format!("media.{key}[{index}] cannot be read: {error}"));
                continue;
            }
        };
        // A component's reference is `<plugin id>/<id>`, so its id is one
        // segment, as a plugin's is, and names one component of its kind.
        if !is_valid_id(id(&entry)) {
            problems.push(format!(
                "media.{key}[{index}]'s id {:?} may use only lowercase letters, digits, \
                 '-', '_', and '.', and must start with a letter or digit",
                id(&entry)
            ));
            continue;
        }
        if kept.iter().any(|earlier| id(earlier) == id(&entry)) {
            problems.push(format!(
                "media.{key}[{index}]'s id {:?} is taken by an earlier entry",
                id(&entry)
            ));
            continue;
        }
        if let Err(reason) = normalize(&mut entry) {
            problems.push(format!("media.{key}[{index}]: {reason}"));
            continue;
        }
        kept.push(entry);
    }
    kept
}

/// [`crate::Manifest::media`] read leniently, leaving out what
/// [`parse_media`] cannot read (discovery reports it).
pub(crate) fn deserialize_media<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<MediaManifest>, D::Error> {
    Ok(parse_media(Value::deserialize(deserializer)?).0)
}

/// Why each part of the manifest `text`'s `media` was skipped.
pub(crate) fn media_problems(text: &str) -> Vec<String> {
    #[derive(Deserialize)]
    struct Media {
        #[serde(default)]
        media: Value,
    }
    serde_json::from_str::<Media>(text)
        .map(|manifest| parse_media(manifest.media).1)
        .unwrap_or_default()
}

/// What [`MANIFEST_FLAG`] prints.
#[derive(Deserialize)]
struct PrintedManifest {
    id: String,
    #[serde(default)]
    name: Option<String>,
    version: String,
    #[serde(default)]
    roles: Vec<PluginRole>,
    #[serde(default)]
    media: Value,
}

/// A recognized FFmpeg build's `program_name` beside `executable` (Celesta's own,
/// links resolved), as `cargo install` leaves it, made from what it prints
/// for [`MANIFEST_FLAG`]. None when there is none, when it does not print a
/// media plugin's manifest within [`MANIFEST_TIMEOUT`] and exit cleanly (said
/// on stderr), or when `executable` sits in a Cargo build folder (it has
/// `.fingerprint` or `.cargo-lock`; with `build.build-dir` set, only
/// `.cargo-lock` stays beside the programs): a source build takes its media
/// plugins from the plugin folders, so a program an older build left there is
/// never picked up. The answer is kept for as long as the program's file
/// stays the same.
fn sibling_media(executable: &Path, program_name: &str) -> Option<MediaPlugin> {
    type Key = (PathBuf, u64, SystemTime);
    static SIBLINGS: OnceLock<Mutex<HashMap<Key, Option<MediaPlugin>>>> = OnceLock::new();

    let directory = executable.parent()?;
    if directory.join(".fingerprint").is_dir() || directory.join(".cargo-lock").is_file() {
        return None;
    }
    let program = directory.join(format!("{program_name}{}", std::env::consts::EXE_SUFFIX));
    let metadata = std::fs::metadata(&program).ok().filter(|m| m.is_file())?;
    let key = (program.clone(), metadata.len(), metadata.modified().ok()?);
    let siblings = SIBLINGS.get_or_init(Default::default);
    if let Some(sibling) = siblings
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return sibling.clone();
    }
    let sibling = match printed_manifest(&program) {
        Ok(sibling) => Some(sibling),
        Err(reason) => {
            eprintln!(
                "celesta: skipped the media plugin {}: {reason}",
                program.display()
            );
            None
        }
    };
    siblings
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, sibling.clone());
    sibling
}

/// The media plugin `program` says it is when run with [`MANIFEST_FLAG`],
/// or why it gave none.
fn printed_manifest(program: &Path) -> Result<MediaPlugin, String> {
    let mut child = without_console_window(Command::new(program).arg(MANIFEST_FLAG))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("it could not be started: {error}"))?;
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    // Drained on a thread of its own, so a noisy program cannot fill the
    // pipe and block before it prints its manifest. The first STDERR_LIMIT
    // bytes are kept where the failure path can read them as they arrive,
    // and `line_ready` says when a whole line (or the end) has been read,
    // so a process the program left holding stderr open delays nothing.
    let errors = Arc::new(Mutex::new(Vec::new()));
    let (line_ready, first_line) = mpsc::channel();
    std::thread::spawn({
        let errors = errors.clone();
        move || {
            let mut buffer = [0; 4096];
            let mut told = false;
            while let Ok(count) = stderr.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                let mut errors = errors.lock().unwrap_or_else(PoisonError::into_inner);
                let retain = count.min(STDERR_LIMIT - errors.len());
                errors.extend_from_slice(&buffer[..retain]);
                if !told && (errors.contains(&b'\n') || errors.len() == STDERR_LIMIT) {
                    told = true;
                    let _ = line_ready.send(());
                }
            }
            let _ = line_ready.send(());
        }
    });
    // Read on a thread of its own, so a program that never closes its
    // output (or leaves a process that holds it) cannot hold up discovery.
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let read = (&mut stdout)
            .take(MANIFEST_OUTPUT_LIMIT)
            .read_to_end(&mut output);
        let _ = send.send(read.map(|_| output));
    });
    let timed_out = || {
        format!(
            "it did not answer {MANIFEST_FLAG} within {} seconds",
            MANIFEST_TIMEOUT.as_secs()
        )
    };
    let deadline = Instant::now() + MANIFEST_TIMEOUT;
    let output = receive.recv_timeout(MANIFEST_TIMEOUT);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => break None,
        }
    };
    let Some(status) = status else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(timed_out());
    };
    if !status.success() {
        let mut reason = format!("{MANIFEST_FLAG} failed ({status})");
        let _ = first_line
            .recv_timeout(STDERR_GRACE.min(deadline.saturating_duration_since(Instant::now())));
        let stderr = errors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(line) = String::from_utf8_lossy(&stderr)
            .lines()
            .find(|line| !line.trim().is_empty())
        {
            let line: String = line
                .chars()
                .filter(|character| !character.is_control())
                .take(512)
                .collect();
            reason.push_str(": ");
            reason.push_str(&line);
        }
        return Err(reason);
    }
    let output = match output {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(format!(
                "its {MANIFEST_FLAG} output could not be read: {error}"
            ));
        }
        // It exited, but something it started still holds its output.
        Err(_) => return Err(timed_out()),
    };
    let printed: PrintedManifest = serde_json::from_slice(&output)
        .map_err(|error| format!("its {MANIFEST_FLAG} output is not a manifest: {error}"))?;
    if !is_valid_id(&printed.id) || printed.id == RESERVED_PLUGIN_ID {
        return Err(format!("its id {:?} cannot name a plugin", printed.id));
    }
    if !printed.roles.contains(&PluginRole::Media) {
        return Err("it is not a media plugin".into());
    }
    let (media, problems) = parse_media(printed.media);
    for problem in problems {
        eprintln!(
            "celesta: {}: a media component was skipped: {problem}",
            program.display()
        );
    }
    Ok(MediaPlugin {
        name: crate::manifest::display_name(printed.name.as_deref(), &printed.id).to_owned(),
        id: printed.id,
        version: printed.version,
        executable: program.to_path_buf(),
        directory: program
            .parent()
            .expect("the program is in a folder")
            .to_path_buf(),
        origin: MediaOrigin::Bundled,
        media: media.unwrap_or_default(),
    })
}

impl InstalledPlugins {
    /// The media plugins to read media with, in search order: each of these
    /// plugins that is a media plugin with a program for this platform
    /// (discovery leaves out one this Celesta is outside the range of, so the
    /// next one with its id is listed instead), then recognized FFmpeg plugin
    /// programs beside the running one ([`CODEC_PROGRAM`] and
    /// [`X264_CODEC_PROGRAM`]), each when that
    /// is an installed program rather than a source build in a Cargo build
    /// folder and none of these but a project's has the id it prints. A
    /// project's plugin cannot replace it, as discovery keeps one in the
    /// bundled folder: the project's is left out (said on stderr). Ordering
    /// decoders by [`MediaOrigin`] is the caller's.
    pub fn media(&self) -> Vec<MediaPlugin> {
        let executable = std::env::current_exe()
            .ok()
            .map(|executable| std::fs::canonicalize(&executable).unwrap_or(executable));
        self.media_with_sibling(executable.as_deref())
    }

    /// [`Self::media`], looking for the sibling beside `executable`, and for
    /// none without one.
    pub(crate) fn media_with_sibling(&self, executable: Option<&Path>) -> Vec<MediaPlugin> {
        let mut media: Vec<MediaPlugin> = self
            .plugins
            .iter()
            .filter(|entry| entry.has_role(PluginRole::Media))
            .filter_map(media_plugin)
            .collect();
        for program in [CODEC_PROGRAM, X264_CODEC_PROGRAM] {
            if let Some(sibling) =
                executable.and_then(|executable| sibling_media(executable, program))
                && !media
                    .iter()
                    .any(|plugin| plugin.id == sibling.id && plugin.origin != MediaOrigin::Project)
            {
                media.retain(|plugin| {
                    let replaces = plugin.id == sibling.id;
                    if replaces {
                        eprintln!(
                            "celesta: skipped the media plugin {}: a project plugin cannot replace \
                         Celesta's bundled media plugin {}",
                            plugin.directory.display(),
                            plugin.id
                        );
                    }
                    !replaces
                });
                media.push(sibling);
            }
        }
        media
    }
}

/// `entry` as a media plugin, when it has a program for this platform.
fn media_plugin(entry: &PluginEntry) -> Option<MediaPlugin> {
    Some(MediaPlugin {
        id: entry.manifest.id.clone(),
        name: entry.manifest.display_name().to_owned(),
        version: entry.manifest.version.clone(),
        executable: entry.executable()?,
        directory: entry.directory.clone(),
        origin: entry.source.into(),
        media: entry.manifest.media.clone().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::{COMPATIBLE, write_manifest};
    use crate::{
        DiscoveryError, MANIFEST, PluginSource, SearchPath, discover, discover_in, platform,
    };

    /// A media plugin's manifest declaring `decoders`.
    fn media_manifest(id: &str, decoders: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "id": id, "version": "1.0.0", "protocol": 2, "celesta": COMPATIBLE,
            "roles": ["media"], "executable": { platform().0: "bin/media" },
            "media": { "decoders": decoders },
        })
    }

    #[test]
    fn entries_serialize_without_their_unset_fields() {
        let video = MediaVideoEncoderEntry {
            id: "video".into(),
            name: "Video".into(),
            codecs: vec!["avc1".into()],
            input_formats: None,
        };
        assert_eq!(
            serde_json::to_value(&video).unwrap(),
            serde_json::json!({ "id": "video", "name": "Video", "codecs": ["avc1"] })
        );
        let muxer = MediaMuxerEntry {
            id: "mux".into(),
            name: "Mux".into(),
            extensions: vec!["mp4".into()],
            video: vec!["avc1".into()],
            audio: None,
            default_video: None,
            default_audio: None,
            trims_leading_audio: true,
            sequence: false,
        };
        assert_eq!(
            serde_json::to_value(&muxer).unwrap(),
            serde_json::json!({
                "id": "mux", "name": "Mux", "extensions": ["mp4"], "video": ["avc1"],
                "trimsLeadingAudio": true, "sequence": false,
            })
        );
    }

    fn installed(plugins: Vec<PluginEntry>) -> InstalledPlugins {
        InstalledPlugins {
            plugins,
            ..InstalledPlugins::default()
        }
    }

    fn decoder(id: &str, name: &str, extensions: &[&str]) -> MediaDecoderEntry {
        MediaDecoderEntry {
            id: id.into(),
            name: name.into(),
            extensions: extensions
                .iter()
                .map(|&extension| extension.into())
                .collect(),
        }
    }

    fn ids(plugins: &[MediaPlugin]) -> Vec<&str> {
        plugins.iter().map(|plugin| plugin.id.as_str()).collect()
    }

    #[test]
    fn reads_media_decoders() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("ffmpeg");
        write_manifest(
            &directory,
            media_manifest(
                "ffmpeg",
                serde_json::json!([{ "id": "ffmpeg", "name": "FFmpeg", "extensions": ["mp4", "MOV", ".m4v"] }]),
            ),
        );
        let (plugins, errors) = discover(std::slice::from_ref(&directory));
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            installed(plugins).media_with_sibling(None),
            [MediaPlugin {
                id: "ffmpeg".into(),
                name: "ffmpeg".into(),
                version: "1.0.0".into(),
                executable: directory.join("bin").join("media"),
                directory: directory.clone(),
                origin: MediaOrigin::User,
                media: MediaManifest {
                    decoders: vec![decoder("ffmpeg", "FFmpeg", &["mp4", "mov", "m4v"])],
                    ..Default::default()
                },
            }]
        );
    }

    #[test]
    fn a_codec_role_is_ignored() {
        let root = tempfile::tempdir().unwrap();
        write_manifest(
            &root.path().join("old"),
            serde_json::json!({
                "id": "old", "version": "1", "protocol": 1, "celesta": COMPATIBLE,
                "roles": ["codec"], "executable": { platform().0: "bin/codec" },
            }),
        );
        let (plugins, errors) = discover(&[root.path().to_path_buf()]);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].manifest.roles, [PluginRole::Unknown]);
        let installed = installed(plugins);
        assert!(installed.media_with_sibling(None).is_empty());
        assert_eq!(installed.host().plugins().count(), 0);
    }

    /// The reason the only plugin under `root` was skipped.
    fn skipped(root: &Path) -> Vec<String> {
        let (plugins, errors) = discover(&[root.to_path_buf()]);
        assert!(plugins.is_empty(), "{plugins:?}");
        errors
            .iter()
            .map(|error| match error {
                DiscoveryError::Invalid { reason, .. } => reason.clone(),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn content_and_media_together_are_invalid() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest = media_manifest("both", serde_json::json!([]));
        manifest["roles"] = serde_json::json!(["content", "media"]);
        write_manifest(&root.path().join("both"), manifest);
        let reasons = skipped(root.path());
        assert!(
            matches!(&reasons[..], [reason] if reason.contains("both content and media")),
            "{reasons:?}"
        );
    }

    #[test]
    fn builtin_is_reserved() {
        let root = tempfile::tempdir().unwrap();
        write_manifest(
            &root.path().join(RESERVED_PLUGIN_ID),
            media_manifest(RESERVED_PLUGIN_ID, serde_json::json!([])),
        );
        let reasons = skipped(root.path());
        assert!(
            matches!(&reasons[..], [reason] if reason.contains("reserved")),
            "{reasons:?}"
        );
    }

    #[test]
    fn media_needs_protocol_2() {
        let root = tempfile::tempdir().unwrap();
        let mut media = media_manifest("a-media", serde_json::json!([]));
        media["protocol"] = 1.into();
        write_manifest(&root.path().join("a-media"), media);
        write_manifest(
            &root.path().join("b-content"),
            serde_json::json!({
                "id": "b-content", "version": "1", "protocol": 2, "celesta": COMPATIBLE,
                "module": "js/index.js",
            }),
        );
        let reasons = skipped(root.path());
        assert_eq!(
            reasons,
            [
                "it speaks plugin protocol 1, and this Celesta speaks protocol 2 with media plugins",
                "it speaks plugin protocol 2, and this Celesta speaks protocol 1 with content plugins",
            ]
        );
    }

    #[test]
    fn a_bad_decoder_is_skipped_alone() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("p");
        write_manifest(
            &directory,
            media_manifest(
                "p",
                serde_json::json!([{ "id": "ok", "name": "OK", "extensions": [] }, { "id": 3 }]),
            ),
        );
        let (plugins, errors) = discover(&[root.path().to_path_buf()]);
        assert!(
            matches!(&errors[..], [DiscoveryError::BadComponent { path, .. }] if *path == directory.join(MANIFEST)),
            "{errors:?}"
        );
        let message = errors[0].to_string();
        assert!(
            message.starts_with(&format!(
                "{}: a media component was skipped: ",
                directory.join(MANIFEST).display()
            )),
            "{message}"
        );
        let media = installed(plugins).media_with_sibling(None);
        assert_eq!(ids(&media), ["p"]);
        assert_eq!(media[0].media.decoders, [decoder("ok", "OK", &[])]);
    }

    #[test]
    fn a_decoder_needs_an_id_of_its_own() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("p");
        write_manifest(
            &directory,
            media_manifest(
                "p",
                serde_json::json!([
                    { "id": "ok", "name": "OK" },
                    { "id": "a/b", "name": "Slashed" },
                    { "id": "", "name": "Empty" },
                    { "id": "ok", "name": "Again" },
                    { "id": "next", "name": "Next" },
                ]),
            ),
        );
        let (plugins, errors) = discover(&[root.path().to_path_buf()]);
        let reasons: Vec<_> = errors
            .iter()
            .map(|error| match error {
                DiscoveryError::BadComponent { reason, .. } => reason.as_str(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(reasons.len(), 3, "{reasons:?}");
        assert!(
            reasons[0].starts_with("media.decoders[1]'s id \"a/b\""),
            "{reasons:?}"
        );
        assert!(
            reasons[1].starts_with("media.decoders[2]'s id \"\""),
            "{reasons:?}"
        );
        assert_eq!(
            reasons[2],
            "media.decoders[3]'s id \"ok\" is taken by an earlier entry"
        );
        let media = installed(plugins).media_with_sibling(None);
        assert_eq!(
            media[0].media.decoders,
            [decoder("ok", "OK", &[]), decoder("next", "Next", &[])]
        );
    }

    #[test]
    fn origins_follow_sources() {
        let root = tempfile::tempdir().unwrap();
        let sources = [
            ("a", PluginSource::Explicit),
            ("b", PluginSource::Project),
            ("c", PluginSource::User),
            ("d", PluginSource::Bundled),
            ("e", PluginSource::Folder),
        ];
        let paths: Vec<_> = sources
            .iter()
            .map(|&(id, source)| {
                let folder = root.path().join(format!("{id}-folder"));
                write_manifest(&folder.join(id), media_manifest(id, serde_json::json!([])));
                SearchPath {
                    path: folder,
                    source,
                }
            })
            .collect();
        let (plugins, errors) = discover_in(&paths);
        assert!(errors.is_empty(), "{errors:?}");
        let origins: Vec<_> = installed(plugins)
            .media_with_sibling(None)
            .iter()
            .map(|plugin| plugin.origin)
            .collect();
        assert_eq!(
            origins,
            [
                MediaOrigin::User,
                MediaOrigin::Project,
                MediaOrigin::User,
                MediaOrigin::Bundled,
                MediaOrigin::User,
            ]
        );
        assert!(MediaOrigin::User < MediaOrigin::Bundled);
        assert!(MediaOrigin::Bundled < MediaOrigin::Project);
    }

    #[test]
    fn media_lists_in_search_order() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("z");
        let second = root.path().join("a");
        write_manifest(
            &first.join("zed"),
            media_manifest("zed", serde_json::json!([])),
        );
        write_manifest(
            &second.join("ann"),
            media_manifest("ann", serde_json::json!([])),
        );
        // Not a media plugin, so not listed.
        write_manifest(
            &second.join("content"),
            serde_json::json!({ "id": "content", "version": "1", "protocol": 1, "celesta": COMPATIBLE, "module": "js/index.js" }),
        );
        let (plugins, errors) = discover(&[first, second]);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            ids(&installed(plugins).media_with_sibling(None)),
            ["zed", "ann"]
        );
    }

    /// A search of `folders`, each a folder of plugins from its source.
    fn search(folders: &[(&Path, PluginSource)]) -> Vec<SearchPath> {
        folders
            .iter()
            .map(|&(path, source)| SearchPath {
                path: path.to_path_buf(),
                source,
            })
            .collect()
    }

    #[test]
    fn a_project_plugin_cannot_replace_a_bundled_media_plugin() {
        let (project, user, bundled) = (
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
        );
        for folder in [&project, &bundled] {
            write_manifest(
                &folder.path().join("ffmpeg"),
                media_manifest("ffmpeg", serde_json::json!([])),
            );
        }
        let rejected = |errors: &[DiscoveryError]| {
            assert!(
                matches!(
                    errors,
                    [DiscoveryError::Invalid { path, reason }]
                        if *path == project.path().join("ffmpeg").join(MANIFEST)
                            && reason == "a project plugin cannot replace Celesta's bundled media plugin ffmpeg"
                ),
                "{errors:?}"
            );
        };
        let (plugins, errors) = discover_in(&search(&[
            (project.path(), PluginSource::Project),
            (bundled.path(), PluginSource::Bundled),
        ]));
        rejected(&errors);
        let media = installed(plugins).media_with_sibling(None);
        assert_eq!(ids(&media), ["ffmpeg"]);
        assert_eq!(media[0].directory, bundled.path().join("ffmpeg"));
        assert_eq!(media[0].origin, MediaOrigin::Bundled);

        // The user's own plugin of that id still comes first, as the
        // project's takes no id.
        write_manifest(
            &user.path().join("ffmpeg"),
            media_manifest("ffmpeg", serde_json::json!([])),
        );
        let (plugins, errors) = discover_in(&search(&[
            (project.path(), PluginSource::Project),
            (user.path(), PluginSource::User),
            (bundled.path(), PluginSource::Bundled),
        ]));
        rejected(&errors);
        let media = installed(plugins).media_with_sibling(None);
        assert_eq!(ids(&media), ["ffmpeg"]);
        assert_eq!(media[0].directory, user.path().join("ffmpeg"));

        // Nor can a project's content plugin take the media plugin's id.
        std::fs::remove_dir_all(project.path().join("ffmpeg")).unwrap();
        write_manifest(
            &project.path().join("ffmpeg"),
            serde_json::json!({ "id": "ffmpeg", "version": "1", "protocol": 1, "celesta": COMPATIBLE, "module": "js/index.js" }),
        );
        let (plugins, errors) = discover_in(&search(&[
            (project.path(), PluginSource::Project),
            (bundled.path(), PluginSource::Bundled),
        ]));
        rejected(&errors);
        assert_eq!(
            ids(&installed(plugins).media_with_sibling(None)),
            ["ffmpeg"]
        );
    }

    /// A plugin with no role this Celesta knows, such as an old build of
    /// the FFmpeg plugin with the `codec` role, runs nowhere, so it takes
    /// no id: the bundled media plugin of that id is still used.
    #[test]
    fn a_plugin_with_no_known_role_takes_no_id() {
        let (user, bundled) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        write_manifest(
            &user.path().join("ffmpeg"),
            serde_json::json!({
                "id": "ffmpeg", "version": "0.9.0", "protocol": 1, "celesta": COMPATIBLE,
                "roles": ["codec"], "executable": { platform().0: "bin/media" },
            }),
        );
        write_manifest(
            &bundled.path().join("ffmpeg"),
            media_manifest("ffmpeg", serde_json::json!([])),
        );
        let (plugins, errors) = discover_in(&search(&[
            (user.path(), PluginSource::User),
            (bundled.path(), PluginSource::Bundled),
        ]));
        assert!(errors.is_empty(), "{errors:?}");
        let media = installed(plugins).media_with_sibling(None);
        assert_eq!(ids(&media), ["ffmpeg"]);
        assert_eq!(media[0].directory, bundled.path().join("ffmpeg"));
    }

    #[test]
    fn a_project_content_plugin_still_comes_first() {
        let (project, bundled) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        for folder in [&project, &bundled] {
            write_manifest(
                &folder.path().join("shapes"),
                serde_json::json!({ "id": "shapes", "version": "1", "protocol": 1, "celesta": COMPATIBLE, "module": "js/index.js" }),
            );
        }
        let (plugins, errors) = discover_in(&search(&[
            (project.path(), PluginSource::Project),
            (bundled.path(), PluginSource::Bundled),
        ]));
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].directory, project.path().join("shapes"));
        assert_eq!(plugins[0].source, PluginSource::Project);
    }

    /// The `media` object of the design spec's manifest example.
    fn spec_media() -> serde_json::Value {
        serde_json::json!({
            "decoders": [{ "id": "ffmpeg", "name": "FFmpeg", "extensions": ["mp4", "m4v", "mov"] }],
            "videoEncoders": [
                { "id": "libx264", "name": "H.264 (x264)", "codecs": ["avc1"],
                  "inputFormats": ["yuv420p", "yuv420p10", "nv12", "p010", "rgba8", "rgba16"] },
                { "id": "prores", "name": "ProRes (prores_ks)", "codecs": ["ap4h", "apch"] },
                { "id": "vp9", "name": "VP9 (libvpx)", "codecs": ["vp09"] }
            ],
            "audioEncoders": [
                { "id": "aac", "name": "AAC", "codecs": ["mp4a"] },
                { "id": "opus", "name": "Opus", "codecs": ["opus"] },
                { "id": "pcm", "name": "PCM", "codecs": ["lpcm"] }
            ],
            "muxers": [
                { "id": "mp4", "name": "MP4", "extensions": ["mp4", "m4v"],
                  "video": ["avc1", "hvc1", "av01"], "audio": ["mp4a", "opus"],
                  "defaultVideo": "avc1", "defaultAudio": "mp4a", "trimsLeadingAudio": true },
                { "id": "mov", "name": "QuickTime", "extensions": ["mov"],
                  "video": ["avc1", "hvc1", "ap4h", "apch"], "audio": ["mp4a", "lpcm"],
                  "defaultVideo": "ap4h", "defaultAudio": "lpcm", "trimsLeadingAudio": true },
                { "id": "webm", "name": "WebM", "extensions": ["webm"],
                  "video": ["vp09", "av01"], "audio": ["opus"],
                  "defaultVideo": "vp09", "defaultAudio": "opus" }
            ]
        })
    }

    /// The media plugin `p` that `media` declares, discovered, with the
    /// reasons its parts were skipped.
    fn discovered(media: serde_json::Value) -> (MediaManifest, Vec<String>) {
        let root = tempfile::tempdir().unwrap();
        let mut manifest = media_manifest("p", serde_json::json!([]));
        manifest["media"] = media;
        write_manifest(&root.path().join("p"), manifest);
        let (plugins, errors) = discover(&[root.path().to_path_buf()]);
        let reasons = errors
            .iter()
            .map(|error| match error {
                DiscoveryError::BadComponent { reason, .. } => reason.clone(),
                other => panic!("{other:?}"),
            })
            .collect();
        let media = installed(plugins).media_with_sibling(None);
        (media.into_iter().next().unwrap().media, reasons)
    }

    #[test]
    fn reads_encoders_and_muxers() {
        let (media, reasons) = discovered(spec_media());
        assert!(reasons.is_empty(), "{reasons:?}");
        assert_eq!(media.decoders.len(), 1);
        assert_eq!(media.video_encoders.len(), 3);
        assert_eq!(media.audio_encoders.len(), 3);
        assert_eq!(media.muxers.len(), 3);
        assert_eq!(
            media.video_encoders[0]
                .input_formats
                .as_deref()
                .map(<[_]>::len),
            Some(6)
        );
        assert_eq!(media.video_encoders[1].input_formats, None);
        assert_eq!(media.video_encoders[1].codecs, ["ap4h", "apch"]);
        let mp4 = &media.muxers[0];
        assert_eq!(mp4.extensions, ["mp4", "m4v"]);
        assert_eq!(mp4.default_video.as_deref(), Some("avc1"));
        assert_eq!(mp4.default_audio.as_deref(), Some("mp4a"));
        assert!(mp4.trims_leading_audio);
        assert!(!media.muxers[2].trims_leading_audio);
        assert!(!media.muxers[2].sequence);
    }

    #[test]
    fn codec_families_are_normalized() {
        let (media, reasons) = discovered(serde_json::json!({
            "videoEncoders": [{ "id": "e", "name": "E", "codecs": ["avc3", "hev1.1.6.L93"] }],
            "muxers": [{ "id": "m", "name": "M", "extensions": [".MP4"], "video": ["avc3"],
                         "audio": ["mp4a.40.2"], "defaultVideo": "avc3", "defaultAudio": "mp4a" }],
        }));
        assert!(reasons.is_empty(), "{reasons:?}");
        assert_eq!(media.video_encoders[0].codecs, ["avc1", "hvc1"]);
        assert_eq!(codec_family("mp4a.40.2"), "mp4a");
        let muxer = &media.muxers[0];
        assert_eq!(muxer.extensions, ["mp4"]);
        assert_eq!(muxer.video, ["avc1"]);
        assert_eq!(muxer.audio.as_deref(), Some(&["mp4a".to_owned()][..]));
        assert_eq!(muxer.default_video.as_deref(), Some("avc1"));
    }

    #[test]
    fn entries_serialize_in_camel_case() {
        let (media, reasons) = discovered(spec_media());
        assert!(reasons.is_empty(), "{reasons:?}");
        let muxer = serde_json::to_value(&media.muxers[0]).unwrap();
        assert_eq!(muxer["defaultVideo"], "avc1");
        assert_eq!(muxer["trimsLeadingAudio"], true);
        let encoder = serde_json::to_value(&media.video_encoders[0]).unwrap();
        assert_eq!(encoder["inputFormats"][0], "yuv420p");

        let (from_json, problems) = MediaManifest::from_json(spec_media());
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(from_json, media);
        let (parsed, problems) = parse_media(spec_media());
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(parsed, Some(media));
    }

    #[test]
    fn a_bad_encoder_is_skipped_alone() {
        let (media, reasons) = discovered(serde_json::json!({
            "videoEncoders": [
                { "id": "ok", "name": "OK", "codecs": ["avc1"] },
                { "id": "none", "name": "No codecs", "codecs": [] },
            ],
        }));
        assert_eq!(media.video_encoders.len(), 1);
        assert_eq!(media.video_encoders[0].id, "ok");
        assert!(
            matches!(&reasons[..], [reason] if reason.contains("codecs")),
            "{reasons:?}"
        );
    }

    #[test]
    fn a_muxer_default_must_be_accepted() {
        let (media, reasons) = discovered(serde_json::json!({
            "muxers": [
                { "id": "m", "name": "M", "extensions": ["x"], "video": ["avc1"], "defaultVideo": "vp09" },
                { "id": "a", "name": "A", "extensions": ["x"], "video": ["avc1"], "defaultAudio": "mp4a" },
                { "id": "no-ext", "name": "N", "extensions": [], "video": ["avc1"] },
                { "id": "no-video", "name": "N", "extensions": ["x"], "video": [] },
            ],
        }));
        assert!(media.muxers.is_empty(), "{media:?}");
        assert_eq!(reasons.len(), 4, "{reasons:?}");
        assert!(reasons[0].contains("defaultVideo"), "{reasons:?}");
        assert!(reasons[1].contains("defaultAudio"), "{reasons:?}");
        assert!(reasons[2].contains("extensions"), "{reasons:?}");
        assert!(reasons[3].contains("video"), "{reasons:?}");
    }

    #[test]
    fn ids_are_unique_per_kind() {
        let (media, reasons) = discovered(serde_json::json!({
            "decoders": [{ "id": "ffmpeg", "name": "D" }],
            "videoEncoders": [{ "id": "ffmpeg", "name": "V", "codecs": ["avc1"] }],
            "muxers": [
                { "id": "mp4", "name": "First", "extensions": ["mp4"], "video": ["avc1"] },
                { "id": "mp4", "name": "Second", "extensions": ["mp4"], "video": ["avc1"] },
                { "id": "Bad Id", "name": "Bad", "extensions": ["mp4"], "video": ["avc1"] },
            ],
        }));
        assert_eq!(media.decoders.len(), 1);
        assert_eq!(media.video_encoders.len(), 1);
        assert_eq!(media.muxers.len(), 1);
        assert_eq!(media.muxers[0].name, "First");
        assert_eq!(reasons.len(), 2, "{reasons:?}");
    }

    #[cfg(unix)]
    mod sibling {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        use std::time::{Duration, Instant};

        /// What the fake sibling prints for `--manifest`.
        const PRINTED: &str = r#"{"id":"ffmpeg","version":"0.10.0","roles":["media"],"media":{"decoders":[{"id":"ffmpeg","name":"FFmpeg","extensions":["mp4","MOV"]}]}}"#;

        /// The FFmpeg plugin's program beside a Celesta program in
        /// `directory`, made of a shell script, and that Celesta program's
        /// path.
        fn fake_sibling(directory: &Path, body: &str) -> (PathBuf, PathBuf) {
            let program = directory.join(CODEC_PROGRAM);
            std::fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            // A sibling test that forked while the file was open for writing
            // here holds it open until its child execs, and exec then fails
            // with ETXTBSY, which the lookup would remember as a failure.
            // Once one start succeeds, no process can hold it open any more.
            for attempt in 0.. {
                match std::process::Command::new(&program)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    Ok(mut started) => {
                        let _ = started.kill();
                        let _ = started.wait();
                        break;
                    }
                    Err(error)
                        if attempt < 40
                            && error.kind() == std::io::ErrorKind::ExecutableFileBusy =>
                    {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(error) => panic!("{}: {error}", program.display()),
                }
            }
            (program, directory.join("celesta-exporter"))
        }

        fn printing_its_manifest(directory: &Path) -> (PathBuf, PathBuf) {
            fake_sibling(
                directory,
                &format!("[ \"$1\" = {MANIFEST_FLAG} ] || exit 2\nprintf '%s\\n' '{PRINTED}'"),
            )
        }

        #[test]
        fn the_x264_build_is_discovered_independently() {
            let root = tempfile::tempdir().unwrap();
            let (_, executable) = printing_its_manifest(root.path());
            let printed = serde_json::json!({
                "id": "ffmpeg-x264", "version": "0.10.0", "roles": ["media"], "media": {},
            });
            let scratch = tempfile::tempdir().unwrap();
            let (program, _) = fake_sibling(scratch.path(), &format!("printf '%s\\n' '{printed}'"));
            std::fs::rename(program, root.path().join(X264_CODEC_PROGRAM)).unwrap();
            let listed = InstalledPlugins::default().media_with_sibling(Some(&executable));
            assert_eq!(ids(&listed), ["ffmpeg", "ffmpeg-x264"]);
        }

        #[test]
        fn the_sibling_prints_every_kind() {
            let root = tempfile::tempdir().unwrap();
            let printed = serde_json::json!({
                "id": "ffmpeg", "version": "0.10.0", "roles": ["media"], "media": spec_media(),
            });
            let (_, executable) = fake_sibling(root.path(), &format!("printf '%s\\n' '{printed}'"));
            let listed = InstalledPlugins::default().media_with_sibling(Some(&executable));
            assert_eq!(ids(&listed), ["ffmpeg"]);
            assert_eq!(listed[0].media, MediaManifest::from_json(spec_media()).0);
            assert_eq!(listed[0].media.video_encoders.len(), 3);
            assert_eq!(listed[0].media.audio_encoders.len(), 3);
            assert_eq!(listed[0].media.muxers.len(), 3);
            // Without a `name` the display name is the id.
            assert_eq!(listed[0].name, "ffmpeg");

            let named = tempfile::tempdir().unwrap();
            let printed = serde_json::json!({
                "id": "ffmpeg", "name": "FFmpeg", "version": "0.10.0", "roles": ["media"],
                "media": spec_media(),
            });
            let (_, executable) =
                fake_sibling(named.path(), &format!("printf '%s\\n' '{printed}'"));
            let listed = InstalledPlugins::default().media_with_sibling(Some(&executable));
            assert_eq!(listed[0].name, "FFmpeg");
        }

        #[test]
        fn the_sibling_is_listed_once() {
            let root = tempfile::tempdir().unwrap();
            let (program, executable) = printing_its_manifest(root.path());
            let sibling = MediaPlugin {
                id: "ffmpeg".into(),
                name: "ffmpeg".into(),
                version: "0.10.0".into(),
                executable: program,
                directory: root.path().to_path_buf(),
                origin: MediaOrigin::Bundled,
                media: MediaManifest {
                    decoders: vec![decoder("ffmpeg", "FFmpeg", &["mp4", "mov"])],
                    ..Default::default()
                },
            };
            assert_eq!(
                InstalledPlugins::default().media_with_sibling(Some(&executable)),
                std::slice::from_ref(&sibling)
            );

            // After the plugins found in search order, unless one of them
            // has its id.
            let plugins = root.path().join("plugins");
            write_manifest(
                &plugins.join("other"),
                media_manifest("other", serde_json::json!([])),
            );
            let (found, errors) = discover(std::slice::from_ref(&plugins));
            assert!(errors.is_empty(), "{errors:?}");
            let listed = installed(found).media_with_sibling(Some(&executable));
            assert_eq!(ids(&listed), ["other", "ffmpeg"]);
            assert_eq!(listed[1], sibling);

            write_manifest(
                &plugins.join("ffmpeg"),
                media_manifest("ffmpeg", serde_json::json!([])),
            );
            let (found, errors) = discover(std::slice::from_ref(&plugins));
            assert!(errors.is_empty(), "{errors:?}");
            let listed = installed(found).media_with_sibling(Some(&executable));
            assert_eq!(ids(&listed), ["ffmpeg", "other"]);
            assert_eq!(listed[0].directory, plugins.join("ffmpeg"));
            assert_eq!(listed[0].origin, MediaOrigin::User);
        }

        #[test]
        fn a_project_plugin_cannot_replace_the_sibling() {
            let root = tempfile::tempdir().unwrap();
            let (program, executable) = printing_its_manifest(root.path());
            let project = root.path().join("project-plugins");
            write_manifest(
                &project.join("ffmpeg"),
                media_manifest("ffmpeg", serde_json::json!([])),
            );
            let (found, errors) = discover_in(&[SearchPath {
                path: project.clone(),
                source: PluginSource::Project,
            }]);
            assert!(errors.is_empty(), "{errors:?}");
            let listed = installed(found).media_with_sibling(Some(&executable));
            assert_eq!(ids(&listed), ["ffmpeg"]);
            assert_eq!(listed[0].executable, program);
            assert_eq!(listed[0].origin, MediaOrigin::Bundled);
        }

        #[test]
        fn a_sibling_that_hangs_is_left_out() {
            let root = tempfile::tempdir().unwrap();
            let (_, executable) = fake_sibling(root.path(), "exec sleep 30");
            let started = Instant::now();
            assert!(
                InstalledPlugins::default()
                    .media_with_sibling(Some(&executable))
                    .is_empty()
            );
            assert!(
                started.elapsed() < Duration::from_secs(6),
                "{:?}",
                started.elapsed()
            );
        }

        #[test]
        fn a_sibling_that_fails_is_left_out() {
            for body in [
                format!("printf '%s\\n' '{PRINTED}'\nexit 1"),
                "echo 'not a manifest'".to_owned(),
                r#"printf '%s\n' '{"id":"ffmpeg","version":"0.10.0","roles":["codec"]}'"#
                    .to_owned(),
            ] {
                let root = tempfile::tempdir().unwrap();
                let (_, executable) = fake_sibling(root.path(), &body);
                assert!(
                    InstalledPlugins::default()
                        .media_with_sibling(Some(&executable))
                        .is_empty(),
                    "{body}"
                );
            }
        }

        #[test]
        fn failed_discovery_keeps_the_first_diagnostic_without_blocking_on_stderr() {
            let root = tempfile::tempdir().unwrap();
            let (program, _) = fake_sibling(
                root.path(),
                "echo 'ffmpeg-x264: this FFmpeg build has no libx264' >&2\nhead -c 100000 /dev/zero >&2\nexit 1",
            );
            let reason = printed_manifest(&program).unwrap_err();
            assert!(reason.contains("--manifest failed"), "{reason}");
            assert!(
                reason.ends_with("ffmpeg-x264: this FFmpeg build has no libx264"),
                "{reason}"
            );
        }

        #[test]
        fn a_process_left_holding_stderr_does_not_delay_the_diagnostic() {
            let root = tempfile::tempdir().unwrap();
            // The program exits at once, but leaves a process that holds its
            // stderr (not its stdout) open for longer than the deadline.
            let (program, _) = fake_sibling(
                root.path(),
                "echo 'ffmpeg-x264: this FFmpeg build has no libx264' >&2\nsleep 30 >/dev/null &\nexit 1",
            );
            let started = Instant::now();
            let reason = printed_manifest(&program).unwrap_err();
            assert!(
                started.elapsed() < MANIFEST_TIMEOUT / 2,
                "took {:?}",
                started.elapsed()
            );
            assert!(
                reason.ends_with("ffmpeg-x264: this FFmpeg build has no libx264"),
                "{reason}"
            );
        }

        #[test]
        fn a_sibling_in_a_build_folder_is_ignored() {
            // A source build's folder has `.fingerprint`, or with
            // `build.build-dir` set, only `.cargo-lock` beside the programs.
            for marker in [".fingerprint", ".cargo-lock"] {
                let root = tempfile::tempdir().unwrap();
                let (_, executable) = printing_its_manifest(root.path());
                if marker == ".fingerprint" {
                    std::fs::create_dir(root.path().join(marker)).unwrap();
                } else {
                    std::fs::write(root.path().join(marker), "").unwrap();
                }
                assert!(
                    InstalledPlugins::default()
                        .media_with_sibling(Some(&executable))
                        .is_empty(),
                    "{marker}"
                );
            }
        }
    }
}
