//! Media plugins: the decoders a `media` plugin's manifest declares, and
//! the FFmpeg plugin's program beside Celesta's own.

use crate::console::without_console_window;
use crate::locations::PluginSource;
use crate::manifest::{InstalledPlugins, PluginEntry, PluginRole, is_valid_id};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock, PoisonError, mpsc};
use std::time::{Duration, Instant, SystemTime};

/// The plugin ID of the components Celesta has built in (`builtin/png`),
/// which no plugin may take.
pub const RESERVED_PLUGIN_ID: &str = "builtin";

/// The name of the FFmpeg media plugin's program, which
/// [`InstalledPlugins::media`] also looks for beside an installed Celesta
/// program, as `cargo install` leaves it (never beside a source build in a
/// Cargo build folder).
pub const CODEC_PROGRAM: &str = "celesta-codec-ffmpeg";

/// The flag that has [`CODEC_PROGRAM`] print its manifest (`id`, `version`,
/// `roles`, and `media`) as JSON and exit, without starting the protocol:
/// `cargo install` cannot put a manifest file beside it.
pub const MANIFEST_FLAG: &str = "--manifest";

/// How long [`MANIFEST_FLAG`] may take before the program is killed and
/// left out.
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(5);

/// The most of the program's output [`MANIFEST_FLAG`] reads.
const MANIFEST_OUTPUT_LIMIT: u64 = 1 << 20;

/// A manifest's `media`: the components a media plugin offers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaManifest {
    pub decoders: Vec<MediaDecoderEntry>,
}

/// A decoder, as a manifest's `media.decoders` declares it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
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

/// Where a media plugin comes from, in the order its decoders are tried:
/// the user's own plugins first, then those that come with Celesta, then a
/// project's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MediaOrigin {
    /// `--plugin-dir`, `CELESTA_PLUGIN_PATH`, the user's plugin folder, or a
    /// folder the user added.
    User,
    /// The folder that comes with Celesta, or [`CODEC_PROGRAM`] beside
    /// Celesta's program.
    Bundled,
    /// A folder the open project lists.
    Project,
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
    pub version: String,
    pub executable: PathBuf,
    /// Where it runs.
    pub directory: PathBuf,
    pub origin: MediaOrigin,
    /// In manifest order.
    pub decoders: Vec<MediaDecoderEntry>,
}

/// A manifest's `media` as far as it can be read, and why each part that
/// could not was skipped. A malformed decoder is left out without the
/// others.
pub(crate) fn parse_media(value: Value) -> (Option<MediaManifest>, Vec<String>) {
    let mut media = match value {
        Value::Null => return (None, Vec::new()),
        Value::Object(media) => media,
        _ => return (None, vec!["its \"media\" is not an object".into()]),
    };
    let mut problems = Vec::new();
    let entries = match media.remove("decoders") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(entries)) => entries,
        Some(_) => {
            problems.push("its \"media.decoders\" is not a list".into());
            Vec::new()
        }
    };
    let mut decoders: Vec<MediaDecoderEntry> = Vec::new();
    for (index, entry) in entries.into_iter().enumerate() {
        let mut decoder = match serde_json::from_value::<MediaDecoderEntry>(entry) {
            Ok(decoder) => decoder,
            Err(error) => {
                problems.push(format!("media.decoders[{index}] cannot be read: {error}"));
                continue;
            }
        };
        // The decoder's reference is `<plugin id>/<id>`, so its id is one
        // segment, as a plugin's is, and names one decoder.
        if !is_valid_id(&decoder.id) {
            problems.push(format!(
                "media.decoders[{index}]'s id {:?} may use only lowercase letters, digits, \
                 '-', '_', and '.', and must start with a letter or digit",
                decoder.id
            ));
            continue;
        }
        if decoders.iter().any(|earlier| earlier.id == decoder.id) {
            problems.push(format!(
                "media.decoders[{index}]'s id {:?} is taken by an earlier decoder",
                decoder.id
            ));
            continue;
        }
        for extension in &mut decoder.extensions {
            *extension = extension
                .strip_prefix('.')
                .unwrap_or(extension)
                .to_lowercase();
        }
        decoders.push(decoder);
    }
    (Some(MediaManifest { decoders }), problems)
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
    version: String,
    #[serde(default)]
    roles: Vec<PluginRole>,
    #[serde(default)]
    media: Value,
}

/// The FFmpeg media plugin's program beside `executable` (Celesta's own,
/// links resolved), as `cargo install` leaves it, made from what it prints
/// for [`MANIFEST_FLAG`]. None when there is none, when it does not print a
/// media plugin's manifest within [`MANIFEST_TIMEOUT`] and exit cleanly (said
/// on stderr), or when `executable` sits in a Cargo build folder (it has
/// `.fingerprint` or `.cargo-lock`; with `build.build-dir` set, only
/// `.cargo-lock` stays beside the programs): a source build takes its media
/// plugins from the plugin folders, so a program an older build left there is
/// never picked up. The answer is kept for as long as the program's file
/// stays the same.
fn sibling_media(executable: &Path) -> Option<MediaPlugin> {
    type Key = (PathBuf, u64, SystemTime);
    static SIBLINGS: OnceLock<Mutex<HashMap<Key, Option<MediaPlugin>>>> = OnceLock::new();

    let directory = executable.parent()?;
    if directory.join(".fingerprint").is_dir() || directory.join(".cargo-lock").is_file() {
        return None;
    }
    let program = directory.join(format!("{CODEC_PROGRAM}{}", std::env::consts::EXE_SUFFIX));
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
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("it could not be started: {error}"))?;
    let mut stdout = child.stdout.take().expect("stdout is piped");
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
        return Err(format!("{MANIFEST_FLAG} failed ({status})"));
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
        id: printed.id,
        version: printed.version,
        executable: program.to_path_buf(),
        directory: program
            .parent()
            .expect("the program is in a folder")
            .to_path_buf(),
        origin: MediaOrigin::Bundled,
        decoders: media.unwrap_or_default().decoders,
    })
}

impl InstalledPlugins {
    /// The media plugins to read media with, in search order: each of these
    /// plugins that is a media plugin with a program for this platform
    /// (discovery leaves out one this Celesta is outside the range of, so the
    /// next one with its id is listed instead), then the FFmpeg media
    /// plugin's program beside the running one ([`CODEC_PROGRAM`]) when that
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
        if let Some(sibling) = executable.and_then(sibling_media)
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
        media
    }
}

/// `entry` as a media plugin, when it has a program for this platform.
fn media_plugin(entry: &PluginEntry) -> Option<MediaPlugin> {
    Some(MediaPlugin {
        id: entry.manifest.id.clone(),
        version: entry.manifest.version.clone(),
        executable: entry.executable()?,
        directory: entry.directory.clone(),
        origin: entry.source.into(),
        decoders: entry
            .manifest
            .media
            .as_ref()
            .map(|media| media.decoders.clone())
            .unwrap_or_default(),
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
                version: "1.0.0".into(),
                executable: directory.join("bin").join("media"),
                directory: directory.clone(),
                origin: MediaOrigin::User,
                decoders: vec![decoder("ffmpeg", "FFmpeg", &["mp4", "mov", "m4v"])],
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
        assert_eq!(media[0].decoders, [decoder("ok", "OK", &[])]);
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
            "media.decoders[3]'s id \"ok\" is taken by an earlier decoder"
        );
        let media = installed(plugins).media_with_sibling(None);
        assert_eq!(
            media[0].decoders,
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
            (program, directory.join("celesta-exporter"))
        }

        fn printing_its_manifest(directory: &Path) -> (PathBuf, PathBuf) {
            fake_sibling(
                directory,
                &format!("[ \"$1\" = {MANIFEST_FLAG} ] || exit 2\nprintf '%s\\n' '{PRINTED}'"),
            )
        }

        #[test]
        fn the_sibling_is_listed_once() {
            let root = tempfile::tempdir().unwrap();
            let (program, executable) = printing_its_manifest(root.path());
            let sibling = MediaPlugin {
                id: "ffmpeg".into(),
                version: "0.10.0".into(),
                executable: program,
                directory: root.path().to_path_buf(),
                origin: MediaOrigin::Bundled,
                decoders: vec![decoder("ffmpeg", "FFmpeg", &["mp4", "mov"])],
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
