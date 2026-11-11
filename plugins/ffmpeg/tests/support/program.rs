//! Sibling programs built independently by the per-package test runner.
use std::path::PathBuf;

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
