//! Sibling programs built independently by the per-package test runner: the
//! one way every plugin's tests find `celesta-codec-<id>` beside their own
//! test binary.
#![allow(dead_code)]

use celesta_media::{MediaProcess, ProviderFactory};
use std::path::PathBuf;

/// The built program `celesta-codec-<id>` in the target directory.
pub fn executable(id: &str) -> PathBuf {
    let directory = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let program = directory.join(format!(
        "celesta-codec-{id}{}",
        std::env::consts::EXE_SUFFIX
    ));
    assert!(
        program.is_file(),
        "build the codec programs first: {}",
        program.display()
    );
    program
}

/// Starts [`executable`]`(id)` in its own folder, as Celesta starts a media
/// plugin.
pub fn factory(id: &str) -> ProviderFactory {
    let executable = executable(id);
    let directory = executable.parent().unwrap().to_path_buf();
    MediaProcess::factory(executable, directory)
}
