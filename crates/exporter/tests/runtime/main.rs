//! The runtime test group: exports and inspections of React entries, which
//! need Node.js and the built React runtime (`pnpm run build:runtime`). The
//! target builds only with the `runtime-tests` feature, and a runtime that is
//! not built fails its tests. See `docs/handoff/2026-11-11-test-groups.md`.

#[path = "../common/mod.rs"]
mod common;
mod export;
mod inspect;
mod stills;

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, OnceLock};

use celesta_exporter::{ExportOptions, Exporter};
use celesta_media::MediaRegistry;

/// An exporter that reads and writes media with the lossless test codec in
/// this process, so the tests need no codec plugin installed.
pub fn exporter(options: ExportOptions) -> Exporter {
    static MEDIA: OnceLock<Arc<MediaRegistry>> = OnceLock::new();
    let media =
        MEDIA.get_or_init(|| Arc::new(MediaRegistry::new(vec![celesta_codec_test::registered()])));
    celesta_media::set_registry(Some(media.clone()));
    Exporter::new(options)
}

/// The built React runtime, `packages/cli/dist/cli.js`. Every test here
/// runs it with `node` from `PATH`, so a missing `node` or runtime fails the
/// test, naming what is missing.
pub fn runtime() -> PathBuf {
    let works = Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    assert!(
        works,
        "the runtime tests need Node.js: node was not found on PATH"
    );
    let package = common::root().join("packages/cli");
    for required in [package.join("node_modules"), package.join("dist/cli.js")] {
        assert!(
            required.exists(),
            "the runtime tests need the built React runtime: run \
             `pnpm install && pnpm run build:runtime` first ({} is missing)",
            required.display()
        );
    }
    package.join("dist/cli.js")
}
