# Publishing the crates to crates.io

The workspace's crates can be packaged for crates.io. `celesta-bench` and
`celesta-plugin-flag` stay `publish = false`; the other 17 are publishable.

## Specification

- Every crate depends on the workspace's own crates through
  `[workspace.dependencies]` in the root `Cargo.toml`, which gives each a
  `path` and an exact `version` (`=0.9.0`). A published crate names its
  dependencies by version, so a path alone is refused by `cargo publish`.
- The pins match `workspace.package.version`. A release bump changes them
  with it; one left behind makes Cargo fail to resolve the workspace, so it
  cannot slip through.
- Every publishable crate has a `description`.
- `celesta-mcp` embeds the `celesta` skill and `celesta-editor` embeds
  `examples/editor-demo.celesta.json`, both from outside their crate folder.
  Their `build.rs` passes the path to `include_str!` through an environment
  variable (`CELESTA_SKILL_DIR`, `CELESTA_EDITOR_DEMO_PROJECT`): the
  repository's file when it exists, otherwise the copy in the crate's
  `bundled/`. `scripts/bundle-crate-files.mjs` makes those copies; they are
  ignored by git and listed in each crate's `include`, which Cargo packages
  regardless of `.gitignore`. Cargo counts those packaged, uncommitted files
  as dirty, so these two crates are published with `--allow-dirty` (their
  `.cargo_vcs_info.json` records `"dirty": true`), apart from the rest.
- Tests that read repository files (`examples/`, `packages/web/test/`) are
  compiled only by `cargo test`, which `cargo publish` does not run, so they
  stay as they are and only run in the repository.

## Publishing

```bash
export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"
node scripts/bundle-crate-files.mjs
cargo publish --workspace --dry-run --allow-dirty
cargo publish --workspace --exclude celesta-editor --exclude celesta-mcp
cargo publish -p celesta-editor -p celesta-mcp --allow-dirty
```

`cargo publish --workspace` (Cargo 1.90+) publishes in dependency order and
skips `publish = false` crates. The dry run takes `--allow-dirty` for the
`bundled/` copies only. Its verification builds every crate, so
FFmpeg 8 must be on `PKG_CONFIG_PATH`. crates.io rate-limits new crates, so a
first publish of all 17 may stop partway; later, publish the rest with
`--exclude` for each crate already on crates.io (or `-p` for each one left).

Trusted Publishing (GitHub Actions OIDC) can be set up per crate on
crates.io only after the crate's first version exists.

## Checks

- `cargo package -p celesta-mcp --list` and `-p celesta-editor --list` show
  `build.rs` and the `bundled/` copies.
- `cargo check --workspace --all-targets`, `cargo test -p celesta-mcp docs`.
- `cargo publish --workspace --dry-run`.
