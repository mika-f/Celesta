//! Where Celesta looks for plugins, in order: the folders it is told about
//! (`--plugin-dir`, then `CELESTA_PLUGIN_PATH`), the user's plugin folder,
//! and the folder that comes with Celesta. Every editor, exporter, and MCP
//! server finds plugins through [`search_paths`], so they agree on what is
//! installed.
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
    /// The user's plugin folder ([`user_plugin_dir`]).
    User,
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

    pub fn label(self) -> &'static str {
        match self {
            Self::Explicit => "plugin path",
            Self::User => "user plugins",
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
/// `CELESTA_PLUGIN_PATH`, the user's plugin folder, and the bundled one.
pub fn search_paths(explicit: &[PathBuf]) -> Vec<SearchPath> {
    let explicit = plugin_paths(explicit).into_iter().map(|path| SearchPath {
        path,
        source: PluginSource::Explicit,
    });
    let user = user_plugin_dir().map(|path| SearchPath {
        path,
        source: PluginSource::User,
    });
    let bundled = bundled_plugin_dir().map(|path| SearchPath {
        path,
        source: PluginSource::Bundled,
    });
    explicit.chain(user).chain(bundled).collect()
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

/// The plugin folder that comes with Celesta. A package build may fix it
/// with `CELESTA_SYSTEM_PLUGIN_DIR` at build time (such as
/// `/usr/lib/celesta/plugins`); otherwise it is found from the running
/// executable: `plugins` beside it on Windows, `Contents/PlugIns` in a macOS
/// app bundle, and `../lib/celesta/plugins` from its folder elsewhere (so
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
        return Some(contents.join("PlugIns"));
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
                Some(PathBuf::from("/Applications/Celesta.app/Contents/PlugIns"))
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
        let paths = search_paths(std::slice::from_ref(&dir));
        assert_eq!(
            paths[0],
            SearchPath {
                path: dir,
                source: PluginSource::Explicit
            }
        );
        let sources: Vec<_> = paths.iter().map(|path| path.source).collect();
        assert!(sources.is_sorted(), "{sources:?}");
        assert!(
            paths.iter().all(|path| path.path.is_absolute()),
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
