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

/// Fails the test, naming what to build, unless the React runtime the
/// server spawns, `packages/cli/dist/cli.js`, is built.
pub fn require_runtime() {
    let cli = root().join("packages/cli/dist/cli.js");
    assert!(
        cli.is_file(),
        "the runtime tests need the built React runtime: run \
         `pnpm install && pnpm run build:runtime` first ({} is missing)",
        cli.display()
    );
}
