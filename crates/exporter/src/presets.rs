//! Export presets: the `export-presets.json` files and how a name is found
//! in them.
//!
//! A user file sits beside `settings.json` ([`user_presets_path`]); a
//! project file sits at the project root. Both have the same shape
//! ([`PresetsFile`]); only the user's holds `last`, and only the project's
//! holds `default`. Keys this Celesta does not know are kept when a file is
//! rewritten.

use crate::ExportError;
use celesta_media::Settings;
use celesta_plugin_host::{project_root, user_settings_dir};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The name of a presets file.
pub const PRESETS_FILE: &str = "export-presets.json";
/// The only `version` this Celesta reads and writes.
pub const PRESETS_VERSION: u32 = 1;

type Extra = serde_json::Map<String, serde_json::Value>;

/// The contents of one `export-presets.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetsFile {
    pub version: u32,
    #[serde(default)]
    pub presets: Vec<Preset>,
    /// The export dialog's most recent choice (the user file only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<PresetOutput>,
    /// The preset an export of the project uses when it names no preset and
    /// no components (the project file only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(flatten)]
    pub other: Extra,
}

/// A named choice of muxer and encoders.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    #[serde(flatten)]
    pub output: PresetOutput,
}

/// What a preset (or `last`) chooses. A missing part is left to the
/// defaults. Components are `<plugin>/<component>` strings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PresetOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muxer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<PresetEncoder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<PresetAudio>,
    #[serde(flatten)]
    pub other: Extra,
}

/// An encoder and the options saved for it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PresetEncoder {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(default)]
    pub options: Settings,
    #[serde(flatten)]
    pub other: Extra,
}

/// The audio of a preset: an encoder, or `"none"` for no audio.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PresetAudio {
    None(NoAudio),
    Encode(PresetEncoder),
}

/// The string `"none"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoAudio {
    #[serde(rename = "none")]
    None,
}

fn invalid(path: &Path, message: impl Into<String>) -> ExportError {
    ExportError::InvalidPresets {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

impl Default for PresetsFile {
    fn default() -> Self {
        Self::new()
    }
}

impl PresetsFile {
    /// A file with no presets.
    pub fn new() -> Self {
        Self {
            version: PRESETS_VERSION,
            presets: Vec::new(),
            last: None,
            default: None,
            other: Extra::new(),
        }
    }

    /// Reads the file at `path`.
    pub fn load(path: &Path) -> Result<Self, ExportError> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| invalid(path, format!("cannot be read: {error}")))?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| invalid(path, format!("is not valid JSON: {error}")))?;
        if let Some(version) = value.get("version").and_then(serde_json::Value::as_u64)
            && version != u64::from(PRESETS_VERSION)
        {
            return Err(invalid(
                path,
                format!(
                    "unsupported version {version}; this Celesta reads version {PRESETS_VERSION}"
                ),
            ));
        }
        serde_json::from_value(value)
            .map_err(|error| invalid(path, format!("is not an export presets file: {error}")))
    }

    /// Like [`load`](Self::load), `None` when there is no file.
    pub fn load_if_present(path: &Path) -> Result<Option<Self>, ExportError> {
        match std::fs::metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            _ => Self::load(path).map(Some),
        }
    }

    /// Writes the file whole through a temporary file in the same folder,
    /// renamed over `path`, so a reader never sees half of it and a failure
    /// leaves the old file and nothing else. The folder must exist. A
    /// symbolic link at `path` is written through, to the file it points
    /// at; on Unix the file keeps its mode, and a new one is `0644`.
    pub fn save(&self, path: &Path) -> Result<(), ExportError> {
        let fail =
            |error: &dyn std::fmt::Display| invalid(path, format!("cannot be written: {error}"));
        let target = link_target(path);
        let folder = match target.parent() {
            Some(folder) if !folder.as_os_str().is_empty() => folder,
            _ => Path::new("."),
        };
        let text = serde_json::to_string_pretty(self).map_err(|error| fail(&error))?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(folder).map_err(|error| fail(&error))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target)
                .ok()
                .filter(std::fs::Metadata::is_file)
                .map_or(0o644, |held| held.permissions().mode() & 0o7777);
            temporary
                .as_file()
                .set_permissions(std::fs::Permissions::from_mode(mode))
                .map_err(|error| fail(&error))?;
        }
        temporary
            .write_all(text.as_bytes())
            .and_then(|()| temporary.write_all(b"\n"))
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|error| fail(&error))?;
        temporary
            .persist(&target)
            .map_err(|error| fail(&error.error))?;
        Ok(())
    }

    /// The first preset named `name`.
    pub fn find(&self, name: &str) -> Option<&Preset> {
        self.presets.iter().find(|preset| preset.name == name)
    }

    /// Replaces the first preset of `preset`'s name in place (`true`), or
    /// appends it (`false`).
    pub fn put(&mut self, preset: Preset) -> bool {
        match self
            .presets
            .iter_mut()
            .find(|held| held.name == preset.name)
        {
            Some(held) => {
                *held = preset;
                true
            }
            None => {
                self.presets.push(preset);
                false
            }
        }
    }
}

/// The file `path` names, following symbolic links (each relative one
/// against its link's folder), even to a file that does not exist yet.
/// `path` itself when it is no link; a loop stops after 40 links.
fn link_target(path: &Path) -> PathBuf {
    let mut target = path.to_path_buf();
    for _ in 0..40 {
        let Ok(link) = std::fs::read_link(&target) else {
            break;
        };
        target = match target.parent() {
            Some(folder) => folder.join(link),
            None => link,
        };
    }
    target
}

/// Loads `path` (or starts a new file when there is none), applies
/// `change`, and saves it.
pub fn update_presets(
    path: &Path,
    change: impl FnOnce(&mut PresetsFile),
) -> Result<PresetsFile, ExportError> {
    let mut file = PresetsFile::load_if_present(path)?.unwrap_or_default();
    change(&mut file);
    file.save(path)?;
    Ok(file)
}

/// `user_settings_dir()/export-presets.json`; `None` where the platform has
/// no settings folder.
pub fn user_presets_path() -> Option<PathBuf> {
    user_settings_dir().map(|dir| dir.join(PRESETS_FILE))
}

/// The files a preset name is looked up in, in that order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PresetLookup {
    /// `--preset-file`.
    pub file: Option<PathBuf>,
    pub project: Option<PathBuf>,
    pub user: Option<PathBuf>,
}

/// Which file a preset came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetSource {
    File,
    Project,
    User,
}

/// A preset found by [`PresetLookup::find`].
#[derive(Clone, Debug, PartialEq)]
pub struct FoundPreset {
    pub preset: Preset,
    pub source: PresetSource,
    pub path: PathBuf,
    pub warnings: Vec<String>,
}

impl PresetLookup {
    /// `file` as given, the project file at `project_root(source)`, and the
    /// user file.
    pub fn for_source(source: &Path, file: Option<PathBuf>) -> Self {
        Self {
            file,
            project: Some(project_root(source).join(PRESETS_FILE)),
            user: user_presets_path(),
        }
    }

    /// The first file (file, project, user) holding `name`. Missing files
    /// are skipped; a file that is reached and cannot be read is
    /// [`ExportError::InvalidPresets`]. A match in the project file also
    /// reads the user file, to warn when it hides a preset there; a user
    /// file that cannot be read is then a warning, so a broken personal
    /// file does not fail an export that the project's file answers.
    pub fn find(&self, name: &str) -> Result<Option<FoundPreset>, ExportError> {
        let candidates = [
            (PresetSource::File, &self.file),
            (PresetSource::Project, &self.project),
            (PresetSource::User, &self.user),
        ];
        for (source, path) in candidates {
            let Some(path) = path else { continue };
            let Some(file) = PresetsFile::load_if_present(path)? else {
                continue;
            };
            let Some(preset) = file.find(name) else {
                continue;
            };
            let mut warnings = Vec::new();
            if source == PresetSource::Project
                && let Some(user) = &self.user
            {
                match PresetsFile::load_if_present(user) {
                    Ok(Some(held)) if held.find(name).is_some() => warnings.push(format!(
                        "the project's preset `{name}` hides the user preset of the same name"
                    )),
                    Ok(_) => {}
                    Err(error) => {
                        let reason = match error {
                            ExportError::InvalidPresets { message, .. } => message,
                            other => other.to_string(),
                        };
                        warnings.push(format!(
                            "your presets file {} could not be read, so it was not checked for a preset of the same name: {reason}",
                            user.display()
                        ));
                    }
                }
            }
            return Ok(Some(FoundPreset {
                preset: preset.clone(),
                source,
                path: path.clone(),
                warnings,
            }));
        }
        Ok(None)
    }

    /// The project file's `default`, if it has the file and one.
    pub fn project_default(&self) -> Result<Option<String>, ExportError> {
        let Some(path) = &self.project else {
            return Ok(None);
        };
        Ok(PresetsFile::load_if_present(path)?.and_then(|file| file.default))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_media::SettingValue;

    const SPEC: &str = r#"{
  "version": 1,
  "presets": [
    { "name": "YouTube 1080p",
      "muxer": "ffmpeg/mp4",
      "video": { "component": "ffmpeg/libx264", "options": { "crf": 18, "preset": "slow" } },
      "audio": { "component": "ffmpeg/aac", "options": { "bitrate": 192000 } } },
    { "name": "Silent", "muxer": "ffmpeg/mp4", "audio": "none" }
  ],
  "last": { "muxer": "ffmpeg/mp4",
            "video": { "component": "ffmpeg/libx264", "options": {} },
            "audio": { "component": "ffmpeg/aac", "options": {} } }
}"#;

    fn write(path: &Path, text: &str) {
        std::fs::write(path, text).unwrap();
    }

    fn named(name: &str) -> Preset {
        Preset {
            name: name.to_owned(),
            output: PresetOutput {
                muxer: Some(format!("m/{name}")),
                ..PresetOutput::default()
            },
        }
    }

    fn file_with(names: &[&str]) -> String {
        let presets: Vec<_> = names
            .iter()
            .map(|name| serde_json::json!({ "name": name }))
            .collect();
        serde_json::json!({ "version": 1, "presets": presets }).to_string()
    }

    fn message(error: ExportError) -> (PathBuf, String) {
        match error {
            ExportError::InvalidPresets { path, message } => (path, message),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_spec_example_round_trips() {
        let file: PresetsFile = serde_json::from_str(SPEC).unwrap();
        let video = file.presets[0].output.video.as_ref().unwrap();
        assert_eq!(video.component.as_deref(), Some("ffmpeg/libx264"));
        assert_eq!(video.options["crf"], SettingValue::Number(18.0));
        assert_eq!(video.options["preset"], SettingValue::String("slow".into()));
        assert_eq!(
            file.presets[1].output.audio,
            Some(PresetAudio::None(NoAudio::None))
        );
        assert!(file.last.is_some());
        let again: PresetsFile =
            serde_json::from_str(&serde_json::to_string(&file).unwrap()).unwrap();
        assert_eq!(again, file);
    }

    #[test]
    fn unknown_keys_survive_a_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PRESETS_FILE);
        write(
            &path,
            r#"{"version":1,"future":1,"presets":[
                {"name":"a","note":"x",
                 "video":{"component":"b/x264","options":{},"pass":2},
                 "audio":{"component":"b/aac","channels":"stereo"}},
                {"name":"b","audio":"none"}]}"#,
        );
        PresetsFile::load(&path).unwrap().save(&path).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["future"], 1);
        assert_eq!(value["presets"][0]["note"], "x");
        assert_eq!(value["presets"][0]["video"]["pass"], 2);
        assert_eq!(value["presets"][0]["audio"]["channels"], "stereo");
        assert_eq!(value["presets"][1]["audio"], "none");
        let file = PresetsFile::load(&path).unwrap();
        assert_eq!(file.presets[0].name, "a");
        assert_eq!(
            file.presets[1].output.audio,
            Some(PresetAudio::None(NoAudio::None))
        );
    }

    #[test]
    fn the_version_must_be_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PRESETS_FILE);
        write(&path, r#"{"version": 2, "presets": []}"#);
        let (named_path, message) = message(PresetsFile::load(&path).unwrap_err());
        assert_eq!(named_path, path);
        assert!(message.contains("unsupported version 2"), "{message}");
        write(&path, r#"{"presets": []}"#);
        assert!(matches!(
            PresetsFile::load(&path),
            Err(ExportError::InvalidPresets { .. })
        ));
        write(&path, "{");
        assert!(matches!(
            PresetsFile::load(&path),
            Err(ExportError::InvalidPresets { .. })
        ));
    }

    #[test]
    fn put_replaces_in_place_and_keeps_unknown_keys() {
        let mut file = PresetsFile::new();
        file.other.insert("future".into(), 1.into());
        for name in ["a", "b", "c"] {
            assert!(!file.put(named(name)));
        }
        let mut replacement = named("b");
        replacement.output.muxer = Some("m/new".into());
        assert!(file.put(replacement));
        let names: Vec<_> = file.presets.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert_eq!(
            file.find("b").unwrap().output.muxer.as_deref(),
            Some("m/new")
        );
        assert!(!file.put(named("d")));
        assert_eq!(file.presets.len(), 4);
        assert_eq!(file.presets[3].name, "d");
        assert_eq!(file.other["future"], 1);
        assert!(file.find("zzz").is_none());
    }

    #[test]
    fn lookup_goes_file_then_project_then_user() {
        let dir = tempfile::tempdir().unwrap();
        let (file, project, user) = (
            dir.path().join("file.json"),
            dir.path().join("project.json"),
            dir.path().join("user.json"),
        );
        write(&file, &file_with(&["x"]));
        write(&project, &file_with(&["x"]));
        write(&user, &file_with(&["x"]));
        let mut lookup = PresetLookup {
            file: Some(file.clone()),
            project: Some(project.clone()),
            user: Some(user.clone()),
        };
        let found = lookup.find("x").unwrap().unwrap();
        assert_eq!((found.source, &found.path), (PresetSource::File, &file));
        assert!(found.warnings.is_empty());

        lookup.file = None;
        let found = lookup.find("x").unwrap().unwrap();
        assert_eq!(
            (found.source, &found.path),
            (PresetSource::Project, &project)
        );
        assert_eq!(
            found.warnings,
            ["the project's preset `x` hides the user preset of the same name"]
        );

        write(&project, &file_with(&["y"]));
        let found = lookup.find("x").unwrap().unwrap();
        assert_eq!((found.source, &found.path), (PresetSource::User, &user));
        assert!(found.warnings.is_empty());

        assert_eq!(lookup.find("nowhere").unwrap(), None);
    }

    #[test]
    fn missing_files_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let lookup = PresetLookup {
            file: Some(dir.path().join("a.json")),
            project: Some(dir.path().join("b.json")),
            user: Some(dir.path().join("c.json")),
        };
        assert_eq!(lookup.find("x").unwrap(), None);
        assert_eq!(lookup.project_default().unwrap(), None);
        assert_eq!(PresetLookup::default().find("x").unwrap(), None);
    }

    #[test]
    fn a_broken_file_is_read_only_when_the_lookup_reaches_it() {
        let dir = tempfile::tempdir().unwrap();
        let (file, project, user) = (
            dir.path().join("file.json"),
            dir.path().join("project.json"),
            dir.path().join("user.json"),
        );
        write(&user, "{");
        write(&project, &file_with(&["other"]));
        let mut lookup = PresetLookup {
            file: None,
            project: Some(project.clone()),
            user: Some(user.clone()),
        };
        let (path, _) = message(lookup.find("x").unwrap_err());
        assert_eq!(path, user);
        assert_eq!(lookup.project_default().unwrap(), None);

        write(&file, &file_with(&["x"]));
        lookup.file = Some(file);
        assert_eq!(
            lookup.find("x").unwrap().unwrap().source,
            PresetSource::File
        );
        assert_eq!(lookup.project_default().unwrap(), None);

        // A project match reads the user file only for the shadow check, so
        // a broken one is a warning there.
        lookup.file = None;
        write(&project, &file_with(&["x"]));
        let found = lookup.find("x").unwrap().unwrap();
        assert_eq!(found.source, PresetSource::Project);
        let (_, reason) = message(PresetsFile::load(&user).unwrap_err());
        assert_eq!(
            found.warnings,
            [format!(
                "your presets file {} could not be read, so it was not checked for a preset of the same name: {reason}",
                user.display()
            )]
        );
        // A name the lookup has to find in it still fails.
        let (path, _) = message(lookup.find("y").unwrap_err());
        assert_eq!(path, user);
    }

    #[test]
    fn project_default_reads_the_project_file() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project.json");
        write(&project, r#"{"version":1,"presets":[],"default":"web"}"#);
        let lookup = PresetLookup {
            project: Some(project),
            ..PresetLookup::default()
        };
        assert_eq!(lookup.project_default().unwrap().as_deref(), Some("web"));
    }

    #[test]
    fn a_failed_save_leaves_the_file_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        let (path, _) = message(PresetsFile::new().save(&folder).unwrap_err());
        assert_eq!(path, folder);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let locked = dir.path().join("locked");
            std::fs::create_dir(&locked).unwrap();
            let target = locked.join(PRESETS_FILE);
            write(&target, "before");
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
            let result = PresetsFile::new().save(&target);
            let entries = std::fs::read_dir(&locked).unwrap().count();
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            if let Err(error) = result {
                assert!(error.to_string().contains("cannot be written"), "{error}");
                assert_eq!(std::fs::read_to_string(&target).unwrap(), "before");
                assert_eq!(entries, 1);
            } else {
                // Running as a user the mode does not bind (root).
                assert_eq!(entries, 1);
            }
        }
    }

    #[test]
    fn update_presets_starts_a_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PRESETS_FILE);
        let file = update_presets(&path, |file| {
            file.put(named("a"));
        })
        .unwrap();
        assert_eq!(file.presets.len(), 1);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.ends_with("}\n"), "{text:?}");
        assert!(
            text.starts_with("{\n  \"version\": 1,\n  \"presets\": [\n"),
            "{text}"
        );
        update_presets(&path, |file| {
            file.put(named("b"));
        })
        .unwrap();
        assert_eq!(PresetsFile::load(&path).unwrap().presets.len(), 2);
    }

    #[test]
    fn update_presets_never_writes_over_a_file_it_cannot_read() {
        let dir = tempfile::tempdir().unwrap();
        let (broken, newer) = (
            dir.path().join("broken.json"),
            dir.path().join("newer.json"),
        );
        write(&broken, "{ \"version\": 1, ");
        write(&newer, r#"{"version":2,"profiles":{"web":{}}}"#);
        for path in [&broken, &newer] {
            let before = std::fs::read(path).unwrap();
            let (named_path, _) = message(
                update_presets(path, |file| {
                    file.put(named("a"));
                })
                .unwrap_err(),
            );
            assert_eq!(&named_path, path);
            assert_eq!(std::fs::read(path).unwrap(), before, "{}", path.display());
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn a_save_keeps_the_files_mode_and_makes_new_files_readable() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let dir = tempfile::tempdir().unwrap();
        let fresh = dir.path().join("fresh.json");
        PresetsFile::new().save(&fresh).unwrap();
        assert_eq!(mode(&fresh), 0o644);

        let held = dir.path().join("held.json");
        for kept in [0o644, 0o640] {
            write(&held, &file_with(&[]));
            std::fs::set_permissions(&held, std::fs::Permissions::from_mode(kept)).unwrap();
            update_presets(&held, |file| {
                file.put(named("a"));
            })
            .unwrap();
            assert_eq!(mode(&held), kept);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_save_writes_through_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let target_dir = dir.path().join("dotfiles");
        std::fs::create_dir(&target_dir).unwrap();
        let target = target_dir.join(PRESETS_FILE);
        write(&target, &file_with(&["a"]));
        let link = dir.path().join(PRESETS_FILE);
        std::os::unix::fs::symlink("dotfiles/export-presets.json", &link).unwrap();

        update_presets(&link, |file| {
            file.put(named("b"));
        })
        .unwrap();
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(PresetsFile::load(&target).unwrap().presets.len(), 2);
        assert_eq!(std::fs::read_dir(&target_dir).unwrap().count(), 1);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);

        // A link to a file not made yet makes it.
        let dangling = dir.path().join("later.json");
        std::os::unix::fs::symlink("dotfiles/later.json", &dangling).unwrap();
        PresetsFile::new().save(&dangling).unwrap();
        assert!(
            std::fs::symlink_metadata(&dangling)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(PresetsFile::load(&target_dir.join("later.json")).is_ok());
    }

    #[test]
    fn the_user_file_sits_beside_settings_json() {
        assert_eq!(
            user_presets_path(),
            celesta_plugin_host::user_settings_path()
                .map(|settings| settings.parent().unwrap().join("export-presets.json"))
        );
    }
}
