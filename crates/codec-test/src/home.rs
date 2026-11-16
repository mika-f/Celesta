//! The environment for a child process that must not see the developer's
//! own settings, presets, or plugins.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The variables the user's settings and plugin folders are found from,
/// all at `home`, and `PATH` with the folder of the developer's real
/// Node.js first.
///
/// Set them on a child such as `celesta-exporter` or `celesta-mcp`: it then
/// reads no settings, presets file, or plugin of the developer's, so a
/// codec they installed globally never becomes the default muxer or encoder
/// in place of the test codec. `node` on `PATH` is often a version manager's
/// shim (mise, asdf, Volta) that reads its own state from the real home and
/// fails under `home`, so `PATH` starts with the folder of the program that
/// shim runs, found once with the developer's own environment. When no
/// `node` runs, `PATH` is left as it is.
pub fn isolated_home_env(home: &Path) -> Vec<(&'static str, OsString)> {
    let mut variables: Vec<(&'static str, OsString)> =
        ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "APPDATA"]
            .into_iter()
            .map(|name| (name, home.as_os_str().to_owned()))
            .collect();
    if let Some(folder) = node_folder() {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let folders = std::iter::once(folder.clone()).chain(std::env::split_paths(&path));
        if let Ok(path) = std::env::join_paths(folders) {
            variables.push(("PATH", path));
        }
    }
    variables
}

/// The folder of the Node.js program `node` on `PATH` runs.
fn node_folder() -> Option<&'static PathBuf> {
    static FOLDER: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOLDER
        .get_or_init(|| {
            let output = Command::new("node")
                .args(["-p", "process.execPath"])
                .output()
                .ok()
                .filter(|output| output.status.success())?;
            let program = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
            program.parent().map(Path::to_path_buf)
        })
        .as_ref()
}
