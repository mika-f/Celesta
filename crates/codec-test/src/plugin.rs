//! A temporary codec plugin folder holding the `celesta-codec-test`
//! program, for tests that start Celesta's own programs: given to the child
//! (`--plugin-dir`, or [`PLUGIN_PATH_ENV`](crate::PLUGIN_PATH_ENV) on the
//! child's own environment), it makes the child read and write media with
//! the test codec.

use crate::{PLUGIN_ID, PROGRAM, components};
use celesta_plugin_host::MANIFEST;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The Cargo profile folder (`target/debug`, say) a test executable was
/// built into: its own folder, or that folder's parent when it is `deps`,
/// where Cargo puts test binaries.
pub fn profile_dir(test_executable: &Path) -> PathBuf {
    let folder = test_executable.parent().unwrap_or(Path::new(""));
    match folder.parent() {
        Some(parent) if folder.file_name().is_some_and(|name| name == "deps") => {
            parent.to_path_buf()
        }
        _ => folder.to_path_buf(),
    }
}

/// A codec plugin folder, removed when dropped: keep it until the child
/// that uses it exits.
#[derive(Debug)]
pub struct TestCodecPlugin {
    folder: TempDir,
}

#[expect(
    clippy::new_without_default,
    reason = "new() panics when the program is not built"
)]
impl TestCodecPlugin {
    /// The folder for the program built beside the running test.
    ///
    /// # Panics
    ///
    /// When the program is not built, with [`TestCodecPlugin::in_profile`]'s
    /// message, which says how to build it.
    pub fn new() -> Self {
        let executable = std::env::current_exe().expect("the running test's executable");
        Self::in_profile(&profile_dir(&executable)).unwrap_or_else(|error| panic!("{error}"))
    }

    /// The folder for the program in `profile` (`target/debug`, say): the
    /// program, hard-linked or else copied, and its `celesta-plugin.json`.
    pub fn in_profile(profile: &Path) -> Result<Self, String> {
        let name = format!("{PROGRAM}{}", std::env::consts::EXE_SUFFIX);
        let program = profile.join(&name);
        if !program.is_file() {
            return Err(format!(
                "{PROGRAM} is not built in {}: build it first with cargo build -p celesta-codec-test \
                 (with --release for release tests)",
                profile.display()
            ));
        }
        let folder = tempfile::Builder::new()
            .prefix("celesta-test-codec-")
            .tempdir()
            .map_err(|error| format!("could not make a plugin folder: {error}"))?;
        // A link, not a symlink: the host refuses a program that leads
        // outside its folder. A copy when the folder is on another volume.
        let linked = folder.path().join(&name);
        std::fs::hard_link(&program, &linked)
            .or_else(|_| std::fs::copy(&program, &linked).map(|_| ()))
            .map_err(|error| {
                format!(
                    "could not put {} in the plugin folder {}: {error}",
                    program.display(),
                    folder.path().display()
                )
            })?;
        let manifest = serde_json::json!({
            "id": PLUGIN_ID,
            "version": env!("CARGO_PKG_VERSION"),
            "protocol": 2,
            "celesta": format!("={}", celesta_plugin_host::CELESTA_VERSION),
            "roles": ["media"],
            "media": components::manifest_json(),
            "executable": { std::env::consts::OS: name },
        });
        std::fs::write(folder.path().join(MANIFEST), manifest.to_string())
            .map_err(|error| format!("could not write the plugin's manifest: {error}"))?;
        Ok(Self { folder })
    }

    /// The plugin folder.
    pub fn folder(&self) -> &Path {
        self.folder.path()
    }
}
