//! Where Celesta looks for plugins, in order: the folders it is told about
//! (`--plugin-dir`, then `CELESTA_PLUGIN_PATH`), the versions a trusted
//! project locks, the open project's own folders (in the editor, once
//! trusted), the user's plugin folder, the versions the user installed from
//! the registry, the folders the user added, and the folder that comes with
//! Celesta. Every
//! editor, exporter, and MCP server finds plugins through [`search_paths`],
//! so they agree on what is installed.
//!
//! No location depends on the current directory: the user's folder comes
//! from the platform's absolute per-user locations, and the bundled one from
//! the running executable (or a path fixed at build time).

use crate::manifest::plugin_paths;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Where a plugin folder came from, in the order Celesta searches them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PluginSource {
    /// `--plugin-dir` or `CELESTA_PLUGIN_PATH`.
    Explicit,
    /// The version a trusted project's `celesta.lock` names, from the
    /// registry client's store ([`registry_store_dir`]), or a bundled copy
    /// that matches the lock (see [`crate::RegistryContext`]).
    Locked,
    /// A folder the open project's `celesta.config.json` lists
    /// ([`crate::project_plugin_paths`]).
    Project,
    /// The user's plugin folder ([`user_plugin_dir`]).
    User,
    /// A version the user installed from the registry without a project
    /// ([`crate::UserSettings::registry_plugins`]), from the store.
    Registry,
    /// A folder the user added in the app's settings
    /// ([`crate::UserSettings::plugin_folders`]).
    Folder,
    /// The folder that comes with Celesta ([`bundled_plugin_dir`]).
    Bundled,
}

impl PluginSource {
    /// Whether Celesta checks who can change these folders before using them
    /// (on Unix; see [`crate::discover_in`]). Folders named explicitly are
    /// the caller's choice.
    pub fn is_checked(self) -> bool {
        !matches!(self, Self::Explicit)
    }

    /// Whether a folder from here that does not exist is reported: one the
    /// user added in the app's settings is meant to be there (a project's
    /// own folders are checked when its `celesta.config.json` is read). The
    /// standard locations need not exist, and `CELESTA_PLUGIN_PATH` often
    /// names folders that do not.
    pub fn reports_missing(self) -> bool {
        match self {
            Self::Folder => true,
            Self::Explicit
            | Self::Locked
            | Self::Project
            | Self::User
            | Self::Registry
            | Self::Bundled => false,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Explicit => "plugin path",
            Self::Locked => "locked by project",
            Self::Project => "project plugins",
            Self::User => "user plugins",
            Self::Registry => "installed from the registry",
            Self::Folder => "added folder",
            Self::Bundled => "bundled plugins",
        }
    }
}

/// A folder to look for plugins in: a plugin folder itself or a folder of
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchPath {
    pub path: PathBuf,
    pub source: PluginSource,
}

/// Every folder to look for plugins in, in search order: `explicit` (made
/// absolute against the current directory, as given on a command line),
/// `CELESTA_PLUGIN_PATH`, `project` (the open project's own folders, once
/// the user trusts it), the user's plugin folder, `folders` the user added,
/// and the bundled one. The registry client's store folders take their
/// places among these in discovery ([`crate::RegistryContext`]): a locked
/// version after `explicit`, a global one after the user's plugin folder.
pub fn search_paths(
    explicit: &[PathBuf],
    project: &[PathBuf],
    folders: &[PathBuf],
) -> Vec<SearchPath> {
    let explicit = plugin_paths(explicit).into_iter().map(|path| SearchPath {
        path,
        source: PluginSource::Explicit,
    });
    let project = project.iter().map(|path| SearchPath {
        path: path.clone(),
        source: PluginSource::Project,
    });
    let user = user_plugin_dir().map(|path| SearchPath {
        path,
        source: PluginSource::User,
    });
    let folders = folders.iter().map(|path| SearchPath {
        path: path.clone(),
        source: PluginSource::Folder,
    });
    let bundled = bundled_plugin_dir().map(|path| SearchPath {
        path,
        source: PluginSource::Bundled,
    });
    explicit
        .chain(project)
        .chain(user)
        .chain(folders)
        .chain(bundled)
        .collect()
}

/// The user's plugin folder: `%APPDATA%\Celesta\plugins` on Windows,
/// `~/Library/Application Support/Celesta/plugins` on macOS, and
/// `$XDG_DATA_HOME/celesta/plugins` (by default
/// `~/.local/share/celesta/plugins`) elsewhere. `None` when the platform's
/// variables are missing or not absolute.
pub fn user_plugin_dir() -> Option<PathBuf> {
    user_plugin_dir_from(std::env::consts::OS, |name| std::env::var_os(name))
}

fn user_plugin_dir_from(os: &str, var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    // A relative value would make the folder depend on the current
    // directory; the XDG specification says to ignore one.
    let absolute = |name: &str| {
        var(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    match os {
        "windows" => absolute("APPDATA").map(|dir| dir.join("Celesta").join("plugins")),
        "macos" => absolute("HOME").map(|home| {
            home.join("Library")
                .join("Application Support")
                .join("Celesta")
                .join("plugins")
        }),
        _ => absolute("XDG_DATA_HOME")
            .or_else(|| absolute("HOME").map(|home| home.join(".local").join("share")))
            .map(|data| data.join("celesta").join("plugins")),
    }
}

/// Where the registry client keeps the plugins it installed: `registry`
/// beside [`user_plugin_dir`], so the user's own folder of plugins is never
/// mixed with what the client manages. `None` when there is no user folder.
pub fn registry_store_dir() -> Option<PathBuf> {
    registry_dir_from(
        std::env::consts::OS,
        |name| std::env::var_os(name),
        "registry",
    )
}

/// What the registry client downloaded and verified, beside
/// [`user_plugin_dir`] as `registry-cache`: the advisory list and the index
/// files it uses offline. `None` when there is no user folder.
pub fn registry_cache_dir() -> Option<PathBuf> {
    registry_dir_from(
        std::env::consts::OS,
        |name| std::env::var_os(name),
        "registry-cache",
    )
}

/// The folder `name` beside the user's plugin folder.
fn registry_dir_from(
    os: &str,
    var: impl Fn(&str) -> Option<OsString>,
    name: &str,
) -> Option<PathBuf> {
    Some(user_plugin_dir_from(os, var)?.parent()?.join(name))
}

/// The plugin folder that comes with Celesta. A package build may fix it
/// with `CELESTA_SYSTEM_PLUGIN_DIR` at build time (such as
/// `/usr/lib/celesta/plugins`); otherwise it is found from the running
/// executable: `plugins` beside it on Windows, `Contents/Resources/PlugIns`
/// in a macOS app bundle, and `../lib/celesta/plugins` from its folder elsewhere (so
/// `/opt/celesta/bin/celesta` uses `/opt/celesta/lib/celesta/plugins`).
/// `None` when the executable cannot be found; the current directory is
/// never used instead.
pub fn bundled_plugin_dir() -> Option<PathBuf> {
    if let Some(fixed) = option_env!("CELESTA_SYSTEM_PLUGIN_DIR") {
        return Some(PathBuf::from(fixed));
    }
    // Links resolved, so a `/usr/bin/celesta` that links to
    // `/opt/celesta/bin/celesta` finds the folder under `/opt/celesta`.
    let executable = std::env::current_exe().ok()?;
    let executable = std::fs::canonicalize(&executable).unwrap_or(executable);
    bundled_plugin_dir_for(std::env::consts::OS, &executable)
}

fn bundled_plugin_dir_for(os: &str, executable: &Path) -> Option<PathBuf> {
    let folder = executable.parent()?;
    if os == "windows" {
        return Some(folder.join("plugins"));
    }
    if os == "macos"
        && folder.file_name().is_some_and(|name| name == "MacOS")
        && let Some(contents) = folder.parent()
        && contents.file_name().is_some_and(|name| name == "Contents")
    {
        // Not `Contents/PlugIns`: codesign treats everything there as nested
        // code, and a plugin folder also holds its manifest, licenses, and
        // scripts, so the app could not be signed.
        return Some(contents.join("Resources").join("PlugIns"));
    }
    Some(folder.parent()?.join("lib").join("celesta").join("plugins"))
}

/// Why Celesta will not use `path` from a checked source, if it will not:
/// on Unix, when other users can write to it (group or other write
/// permission) or it belongs to neither root nor the current user, as ssh
/// treats its own files. Anyone who can change a plugin's files can run
/// code as whoever uses it. Windows protects these folders with ACLs, which
/// this does not judge.
pub fn unsafe_reason(path: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(path).ok()?;
        if metadata.mode() & 0o022 != 0 {
            return Some(format!(
                "{} can be changed by other users (remove group and other write permission)",
                path.display()
            ));
        }
        // SAFETY: geteuid has no preconditions and cannot fail.
        let user = unsafe { libc::geteuid() };
        if metadata.uid() != 0 && metadata.uid() != user {
            return Some(format!("{} belongs to another user", path.display()));
        }
        None
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn the_user_folder_follows_each_platform() {
        if cfg!(windows) {
            assert_eq!(
                user_plugin_dir_from(
                    "windows",
                    vars(&[("APPDATA", r"C:\Users\u\AppData\Roaming")])
                ),
                Some(PathBuf::from(r"C:\Users\u\AppData\Roaming\Celesta\plugins"))
            );
        } else {
            assert_eq!(
                user_plugin_dir_from("linux", vars(&[("HOME", "/home/u")])),
                Some(PathBuf::from("/home/u/.local/share/celesta/plugins"))
            );
            assert_eq!(
                user_plugin_dir_from(
                    "linux",
                    vars(&[("HOME", "/home/u"), ("XDG_DATA_HOME", "/data")])
                ),
                Some(PathBuf::from("/data/celesta/plugins"))
            );
            // A relative XDG_DATA_HOME is ignored, as the specification says.
            assert_eq!(
                user_plugin_dir_from(
                    "linux",
                    vars(&[("HOME", "/home/u"), ("XDG_DATA_HOME", "data")])
                ),
                Some(PathBuf::from("/home/u/.local/share/celesta/plugins"))
            );
            assert_eq!(
                user_plugin_dir_from("macos", vars(&[("HOME", "/Users/u")])),
                Some(PathBuf::from(
                    "/Users/u/Library/Application Support/Celesta/plugins"
                ))
            );
        }
        // Never the current directory: without an absolute variable there
        // is no user folder.
        for os in ["windows", "macos", "linux"] {
            assert_eq!(user_plugin_dir_from(os, vars(&[])), None, "{os}");
            assert_eq!(
                user_plugin_dir_from(
                    os,
                    vars(&[
                        ("APPDATA", "rel"),
                        ("HOME", "rel"),
                        ("XDG_DATA_HOME", "rel")
                    ])
                ),
                None,
                "{os}"
            );
        }
    }

    #[test]
    fn the_store_and_cache_sit_beside_the_user_folder() {
        type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a str);
        let cases: Vec<Case> = if cfg!(windows) {
            vec![(
                "windows",
                &[("APPDATA", r"C:\Users\u\AppData\Roaming")],
                r"C:\Users\u\AppData\Roaming\Celesta",
            )]
        } else {
            vec![
                (
                    "linux",
                    &[("HOME", "/home/u")],
                    "/home/u/.local/share/celesta",
                ),
                (
                    "linux",
                    &[("HOME", "/home/u"), ("XDG_DATA_HOME", "/data")],
                    "/data/celesta",
                ),
                (
                    "macos",
                    &[("HOME", "/Users/u")],
                    "/Users/u/Library/Application Support/Celesta",
                ),
            ]
        };
        for (os, pairs, base) in cases {
            for name in ["registry", "registry-cache"] {
                assert_eq!(
                    registry_dir_from(os, vars(pairs), name),
                    Some(PathBuf::from(base).join(name)),
                    "{os} {name}"
                );
            }
        }
        for os in ["windows", "macos", "linux"] {
            assert_eq!(registry_dir_from(os, vars(&[]), "registry"), None, "{os}");
        }
    }

    #[test]
    fn the_bundled_folder_follows_the_executable() {
        if cfg!(windows) {
            assert_eq!(
                bundled_plugin_dir_for(
                    "windows",
                    Path::new(r"C:\Program Files\Celesta\celesta.exe")
                ),
                Some(PathBuf::from(r"C:\Program Files\Celesta\plugins"))
            );
        } else {
            assert_eq!(
                bundled_plugin_dir_for("linux", Path::new("/opt/celesta/bin/celesta")),
                Some(PathBuf::from("/opt/celesta/lib/celesta/plugins"))
            );
            assert_eq!(
                bundled_plugin_dir_for("linux", Path::new("/usr/bin/celesta")),
                Some(PathBuf::from("/usr/lib/celesta/plugins"))
            );
            assert_eq!(
                bundled_plugin_dir_for(
                    "macos",
                    Path::new("/Applications/Celesta.app/Contents/MacOS/celesta")
                ),
                Some(PathBuf::from(
                    "/Applications/Celesta.app/Contents/Resources/PlugIns"
                ))
            );
            // Outside an app bundle, macOS lays out like other Unixes.
            assert_eq!(
                bundled_plugin_dir_for("macos", Path::new("/opt/celesta/bin/celesta")),
                Some(PathBuf::from("/opt/celesta/lib/celesta/plugins"))
            );
        }
    }

    #[test]
    fn search_paths_come_in_order() {
        let dir = std::env::temp_dir().join("celesta-explicit");
        let project = std::env::temp_dir().join("celesta-project").join("plugins");
        let added = std::env::temp_dir().join("celesta-added");
        let paths = search_paths(
            std::slice::from_ref(&dir),
            std::slice::from_ref(&project),
            std::slice::from_ref(&added),
        );
        assert!(paths.contains(&SearchPath {
            path: added,
            source: PluginSource::Folder
        }));
        assert_eq!(
            paths[0],
            SearchPath {
                path: dir,
                source: PluginSource::Explicit
            }
        );
        assert!(paths.contains(&SearchPath {
            path: project,
            source: PluginSource::Project
        }));
        let sources: Vec<_> = paths.iter().map(|path| path.source).collect();
        assert!(sources.is_sorted(), "{sources:?}");
        // The derived order is the search order, store sources included.
        assert!(
            [
                PluginSource::Explicit,
                PluginSource::Locked,
                PluginSource::Project,
                PluginSource::User,
                PluginSource::Registry,
                PluginSource::Folder,
                PluginSource::Bundled,
            ]
            .is_sorted()
        );
        // CELESTA_PLUGIN_PATH is the environment's, and a relative entry in
        // it is reported by discovery; the locations Celesta works out
        // itself are always absolute.
        assert!(
            paths
                .iter()
                .filter(|path| path.source != PluginSource::Explicit)
                .all(|path| path.path.is_absolute()),
            "{paths:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn folders_others_can_change_are_unsafe() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let set = |mode| {
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(mode)).unwrap()
        };
        set(0o755);
        assert_eq!(unsafe_reason(dir.path()), None);
        set(0o775);
        assert!(unsafe_reason(dir.path()).unwrap().contains("other users"));
        set(0o757);
        assert!(unsafe_reason(dir.path()).is_some());
        set(0o700);
    }
}
