//! The runtime test group: the server inspecting, rendering, and exporting
//! React entries, which needs Node.js and the built React runtime
//! (`pnpm run build:runtime`). The target builds only with the
//! `runtime-tests` feature, and a runtime that is not built fails its tests.
//! See `docs/handoff/2026-11-11-test-groups.md`.

#[path = "../common/mod.rs"]
mod common;
mod export;
mod inspect;
mod render;
mod server;

use std::path::{Path, PathBuf};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// An entry in a directory where `@celesta.studio/react` resolves.
pub fn entry(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = dir.path().join("scene.tsx");
    std::fs::write(&entry, source).unwrap();
    (dir, entry)
}

/// Fails the test, naming what is missing, unless the server can start the
/// React runtime: `node` on `PATH` and the built `packages/cli/dist/cli.js`.
pub fn require_runtime() {
    let works = std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    assert!(
        works,
        "the runtime tests need Node.js: node was not found on PATH"
    );
    let package = root().join("packages/cli");
    for required in [package.join("node_modules"), package.join("dist/cli.js")] {
        assert!(
            required.exists(),
            "the runtime tests need the built React runtime: run \
             `pnpm install && pnpm run build:runtime` first ({} is missing)",
            required.display()
        );
    }
}
