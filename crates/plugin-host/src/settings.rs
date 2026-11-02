//! The user's Celesta settings that decide what code Celesta may run: the
//! projects the user trusts and the plugins they allowed. They live in
//! `settings.json` in the user's Celesta settings folder
//! ([`user_settings_path`]), written whole and replaced atomically, and keep
//! keys this build does not know so that another build's settings survive.

use crate::locations::unsafe_reason;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

/// The file in the user's Celesta settings folder.
pub const SETTINGS_FILE: &str = "settings.json";

/// The user's Celesta settings folder: `%APPDATA%\Celesta` on Windows,
/// `~/Library/Application Support/Celesta` on macOS, and
/// `$XDG_CONFIG_HOME/celesta` (by default `~/.config/celesta`) elsewhere.
/// `None` when the platform's variables are missing or not absolute.
pub fn user_settings_dir() -> Option<PathBuf> {
    user_settings_dir_from(std::env::consts::OS, |name| std::env::var_os(name))
}

fn user_settings_dir_from(os: &str, var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let absolute = |name: &str| {
        var(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    match os {
        "windows" => absolute("APPDATA").map(|dir| dir.join("Celesta")),
        "macos" => absolute("HOME").map(|home| {
            home.join("Library")
                .join("Application Support")
                .join("Celesta")
        }),
        _ => absolute("XDG_CONFIG_HOME")
            .or_else(|| absolute("HOME").map(|home| home.join(".config")))
            .map(|config| config.join("celesta")),
    }
}

/// Where the user's settings are read from and written to.
pub fn user_settings_path() -> Option<PathBuf> {
    user_settings_dir().map(|dir| dir.join(SETTINGS_FILE))
}

/// A plugin the user allowed to run, as it was then.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowedPlugin {
    pub id: String,
    /// Its directory, links resolved.
    pub directory: PathBuf,
    /// Its files' fingerprint (see [`crate::fingerprint`]); another means
    /// the plugin changed and is asked about again.
    pub fingerprint: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSettings {
    /// Project roots, links resolved, whose code Celesta may run: their
    /// entries are evaluated and their own plugins used. Each covers its own
    /// project, not the projects in folders below it.
    #[serde(default)]
    pub trusted_projects: Vec<PathBuf>,
    /// User and project plugins the user allowed to run.
    #[serde(default)]
    pub allowed_plugins: Vec<AllowedPlugin>,
    /// Keys a newer build wrote, kept as they were.
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path} is not a Celesta settings file: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{0}; Celesta will not trust what it says")]
    Unsafe(String),
    #[error("could not write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("there is no settings folder (the platform's home or settings variable is not set)")]
    NoFolder,
}

/// Creates `folder` and its missing parents, on Unix readable and writable
/// by the user alone (0700) whatever the umask, as Celesta later refuses
/// folders other users can change. Folders that exist are left as they are.
pub fn create_private_dir_all(folder: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(folder)
}

/// Writes `contents` to a new or truncated `file`, on Unix readable and
/// writable by the user alone (0600) whatever the umask.
fn write_private(file: &Path, contents: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(file)?.write_all(contents)
}

/// `path` with links resolved, or as given (absolute) when it does not exist.
fn resolved(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()))
}

impl UserSettings {
    /// Reads `path`; a missing file is empty settings. On Unix a file other
    /// users can change is refused, since it decides what code runs.
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(SettingsError::Read {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        if let Some(reason) = unsafe_reason(path) {
            return Err(SettingsError::Unsafe(reason));
        }
        let mut settings: Self =
            serde_json::from_str(&text).map_err(|source| SettingsError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        settings.normalize();
        Ok(settings)
    }

    /// Puts what was read in the form this build writes, so comparisons
    /// mean the same folder: folders that are not absolute (an empty one
    /// would cover every path) are dropped, and the rest have their links
    /// resolved.
    fn normalize(&mut self) {
        self.trusted_projects.retain(|folder| folder.is_absolute());
        for folder in &mut self.trusted_projects {
            *folder = resolved(folder);
        }
        self.allowed_plugins
            .retain(|allowed| allowed.directory.is_absolute());
        for allowed in &mut self.allowed_plugins {
            allowed.directory = resolved(&allowed.directory);
        }
    }

    /// Writes the settings to `path` whole, through a temporary file
    /// renamed over it, so a reader never sees half of them. On Unix the
    /// file and any folder made for it are the user's alone, whatever the
    /// umask, since `load` refuses settings others can change.
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let write = |source| SettingsError::Write {
            path: path.to_path_buf(),
            source,
        };
        let folder = path.parent().ok_or_else(|| {
            write(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the settings file has no folder",
            ))
        })?;
        create_private_dir_all(folder).map_err(write)?;
        let text = serde_json::to_string_pretty(self).map_err(|error| write(error.into()))?;
        let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
        write_private(&temporary, (text + "\n").as_bytes()).map_err(write)?;
        std::fs::rename(&temporary, path).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            write(error)
        })
    }

    /// Reads the user's settings, applies `change`, and writes them back.
    pub fn update(change: impl FnOnce(&mut Self)) -> Result<Self, SettingsError> {
        let path = user_settings_path().ok_or(SettingsError::NoFolder)?;
        let mut settings = Self::load(&path)?;
        change(&mut settings);
        settings.save(&path)?;
        Ok(settings)
    }

    /// The user's settings, or empty ones with why they could not be read.
    pub fn current() -> (Self, Option<SettingsError>) {
        match user_settings_path().map(|path| Self::load(&path)) {
            Some(Ok(settings)) => (settings, None),
            Some(Err(error)) => (Self::default(), Some(error)),
            None => (Self::default(), None),
        }
    }

    /// Whether `folder` is a trusted project folder. Only the folder itself
    /// is: a project found inside a trusted one (its own
    /// `celesta.config.json`) is another project, asked about on its own.
    pub fn is_trusted(&self, folder: &Path) -> bool {
        let folder = resolved(folder);
        self.trusted_projects.contains(&folder)
    }

    /// Trusts `folder` from now on.
    pub fn trust(&mut self, folder: &Path) {
        if !self.is_trusted(folder) {
            self.trusted_projects.push(resolved(folder));
        }
    }

    /// Whether the plugin with `id` in `directory` was allowed as it is now
    /// (`fingerprint`), allowed before it changed, or never allowed.
    pub fn allowance(&self, id: &str, directory: &Path, fingerprint: &str) -> Allowance {
        let directory = resolved(directory);
        match self
            .allowed_plugins
            .iter()
            .find(|allowed| allowed.id == id && allowed.directory == directory)
        {
            Some(allowed) if allowed.fingerprint == fingerprint => Allowance::Allowed,
            Some(_) => Allowance::Changed,
            None => Allowance::New,
        }
    }

    /// Allows the plugin as it is now, replacing an earlier allowance.
    pub fn allow(&mut self, id: &str, directory: &Path, fingerprint: &str) {
        let directory = resolved(directory);
        self.allowed_plugins
            .retain(|allowed| !(allowed.id == id && allowed.directory == directory));
        self.allowed_plugins.push(AllowedPlugin {
            id: id.to_owned(),
            directory,
            fingerprint: fingerprint.to_owned(),
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Allowance {
    Allowed,
    /// Allowed once, but its files changed since.
    Changed,
    New,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_folder_follows_each_platform() {
        let vars = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| OsString::from(value))
            }
        };
        if cfg!(windows) {
            assert_eq!(
                user_settings_dir_from(
                    "windows",
                    vars(&[("APPDATA", r"C:\Users\u\AppData\Roaming")])
                ),
                Some(PathBuf::from(r"C:\Users\u\AppData\Roaming\Celesta"))
            );
        } else {
            assert_eq!(
                user_settings_dir_from("linux", vars(&[("HOME", "/home/u")])),
                Some(PathBuf::from("/home/u/.config/celesta"))
            );
            assert_eq!(
                user_settings_dir_from(
                    "linux",
                    vars(&[("HOME", "/home/u"), ("XDG_CONFIG_HOME", "/c")])
                ),
                Some(PathBuf::from("/c/celesta"))
            );
            assert_eq!(
                user_settings_dir_from("macos", vars(&[("HOME", "/Users/u")])),
                Some(PathBuf::from(
                    "/Users/u/Library/Application Support/Celesta"
                ))
            );
        }
        for os in ["windows", "macos", "linux"] {
            assert_eq!(
                user_settings_dir_from(os, vars(&[("APPDATA", "a"), ("HOME", "h")])),
                None
            );
        }
    }

    #[test]
    fn trust_covers_the_project_folder_alone() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir_all(project.join("nested")).unwrap();
        let mut settings = UserSettings::default();
        assert!(!settings.is_trusted(&project));
        settings.trust(&project);
        settings.trust(&project);
        assert_eq!(settings.trusted_projects.len(), 1);
        assert!(settings.is_trusted(&project));
        // A project inside it is another project.
        assert!(!settings.is_trusted(&project.join("nested")));
        assert!(!settings.is_trusted(root.path()));
        // A sibling whose name starts the same is another folder.
        let sibling = root.path().join("project-other");
        std::fs::create_dir_all(&sibling).unwrap();
        assert!(!settings.is_trusted(&sibling));
    }

    #[test]
    fn allowances_follow_the_fingerprint() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = UserSettings::default();
        let dir = root.path();
        assert_eq!(settings.allowance("p", dir, "sha256:a"), Allowance::New);
        settings.allow("p", dir, "sha256:a");
        assert_eq!(settings.allowance("p", dir, "sha256:a"), Allowance::Allowed);
        assert_eq!(settings.allowance("p", dir, "sha256:b"), Allowance::Changed);
        // The same id elsewhere is another plugin.
        assert_eq!(
            settings.allowance("p", &dir.join("other"), "sha256:a"),
            Allowance::New
        );
        settings.allow("p", dir, "sha256:b");
        assert_eq!(settings.allowed_plugins.len(), 1);
    }

    #[test]
    fn settings_round_trip_and_keep_unknown_keys() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("nested").join(SETTINGS_FILE);
        assert_eq!(UserSettings::load(&path).unwrap(), UserSettings::default());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{ "trustedProjects": [], "fromANewerBuild": { "x": 1 } }"#,
        )
        .unwrap();
        let mut settings = UserSettings::load(&path).unwrap();
        settings.trust(root.path());
        settings.save(&path).unwrap();
        let again = UserSettings::load(&path).unwrap();
        assert_eq!(again, settings);
        assert_eq!(again.other["fromANewerBuild"]["x"], 1);
        // No temporary file is left behind.
        let files: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(files, [SETTINGS_FILE]);
    }

    #[test]
    fn unreadable_settings_say_why() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(SETTINGS_FILE);
        std::fs::write(&path, "{").unwrap();
        assert!(matches!(
            UserSettings::load(&path),
            Err(SettingsError::Parse { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn settings_others_can_change_are_refused() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(SETTINGS_FILE);
        std::fs::write(&path, "{}").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        assert!(matches!(
            UserSettings::load(&path),
            Err(SettingsError::Unsafe(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn saved_settings_are_the_users_alone() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("celesta");
        let path = folder.join(SETTINGS_FILE);
        UserSettings::default().save(&path).unwrap();
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        // Without explicit modes a umask such as 002 leaves them group
        // writable, and the next load refuses them.
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&folder), 0o700);
        UserSettings::load(&path).unwrap();
    }

    #[test]
    fn folders_read_are_absolute_and_resolved() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(SETTINGS_FILE);
        let project = root.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            &path,
            serde_json::json!({
                // An empty folder would cover every path; a relative one
                // would depend on the current directory.
                "trustedProjects": ["", "relative", project],
                "allowedPlugins": [
                    { "id": "p", "directory": "relative", "fingerprint": "sha256:a" },
                    { "id": "q", "directory": project, "fingerprint": "sha256:b" },
                ],
            })
            .to_string(),
        )
        .unwrap();
        let settings = UserSettings::load(&path).unwrap();
        assert_eq!(settings.trusted_projects, [resolved(&project)]);
        assert!(!settings.is_trusted(&root.path().join("elsewhere")));
        assert!(settings.is_trusted(&project));
        assert_eq!(settings.allowed_plugins.len(), 1);
        assert_eq!(
            settings.allowance("q", &project, "sha256:b"),
            Allowance::Allowed
        );
    }
}
