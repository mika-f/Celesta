//! A user home in a temporary folder that the `celesta` binary runs in, so
//! tests never touch the real user folders.

// Each test file uses its own part of this module.
#![allow(dead_code)]

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use celesta_registry_client::testing::TestRegistry;
use tempfile::TempDir;

pub struct Home {
    dir: TempDir,
    projects: Cell<usize>,
}

impl Default for Home {
    fn default() -> Self {
        Self::new()
    }
}

impl Home {
    pub fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
            projects: Cell::new(0),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// `celesta` with every user folder under this home, no registry
    /// variables, and this home as the current folder.
    pub fn command(&self) -> Command {
        let home = self.path();
        let mut command = Command::new(env!("CARGO_BIN_EXE_celesta"));
        command
            .current_dir(home)
            .env("HOME", home)
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("APPDATA", home.join("appdata"))
            .env_remove("CELESTA_REGISTRY")
            .env_remove("CELESTA_REGISTRY_ROOT_KEY");
        command
    }

    /// `command` using `registry`, which must be made with
    /// [`TestRegistry::file`] so its base is the folder it serves.
    pub fn with_registry(&self, registry: &TestRegistry) -> Command {
        let mut command = self.command();
        command
            .env("CELESTA_REGISTRY", registry.base())
            .env("CELESTA_REGISTRY_ROOT_KEY", registry.root_key_base64());
        command
    }

    /// The store `celesta` installs registry plugins into under this home:
    /// `registry` beside the user plugin folder.
    pub fn store(&self) -> PathBuf {
        let home = self.path();
        if cfg!(windows) {
            home.join("appdata").join("Celesta").join("registry")
        } else if cfg!(target_os = "macos") {
            home.join("Library")
                .join("Application Support")
                .join("Celesta")
                .join("registry")
        } else {
            home.join("data").join("celesta").join("registry")
        }
    }

    /// A new project folder under this home whose `celesta.config.json`
    /// holds `config`.
    pub fn project(&self, config: &str) -> PathBuf {
        let number = self.projects.get();
        self.projects.set(number + 1);
        let dir = self.path().join(format!("project-{number}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("celesta.config.json"), config).unwrap();
        dir
    }
}
