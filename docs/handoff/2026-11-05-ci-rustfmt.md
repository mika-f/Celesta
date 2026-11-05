# CI checks rustfmt on the root workspace (2026-11-05)

`cargo fmt --all -- --check` is part of the Rust baseline in `HANDOFF.md`,
but CI ran rustfmt only on the VRM plugin's workspace (`plugins/vrm`). The
rename to the `@celesta.studio` scope lengthened string literals and
identifiers past rustfmt's line width in seven files, and nothing caught
it.

## Specification

- The root workspace is formatted with `cargo fmt --all`, as the
  rustfmt of the toolchain CI pins (Rust 1.99.1) formats it. The change is formatting
  only.
- CI's `Format` job runs `cargo fmt --all --check` on every push to
  `main` and every pull request, and fails on any difference.

## Implementation

- `.github/workflows/ci.yml` gains a `format` job: checkout, the pinned
  toolchain with only `rustfmt`, then the check. It is a job of its own
  rather than a step of `test`:
  - rustfmt needs no build, so it reports in under a minute rather than
    after the test job's build.
  - The `test` job keeps installing `clippy` only, so the toolchain that
    rust-cache keys on does not change.
- The VRM plugin keeps its own `Format` step in the `vrm-plugin` job;
  `--all` does not reach it, as the root workspace excludes it.

## Checks

- Before: `cargo fmt --all --check` reported 10 hunks in
  `crates/editor/src/element_edit/resolve.rs`, `crates/editor/src/preview.rs`,
  `crates/mcp/src/docs.rs`, `crates/react-bridge/src/protocol.rs`, and
  `crates/source-edit/tests/{add,imports,scenes}.rs`.
- After: `cargo fmt --all --check` and `git diff --check` pass.
