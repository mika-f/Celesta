# FFmpeg codec download PR preparation

> Superseded by [the x264-only split](2026-11-11-ffmpeg-x264-only-split.md): there is one optional download, `ffmpeg-x264`, and one standalone ZIP per system; Opus and VP9 are back in the standard `ffmpeg` plugin.

## Specification

Commit the approved FFmpeg build/plugin split, scoped license placement and
optional codec downloads, then open a pull request against main.

## Implementation

- Branch: `codex/ffmpeg-codec-downloads`.
- Rebased the implementation onto the then-current main before the initial push. Conflict
  resolution preserves main's macOS `Contents/Resources/PlugIns` layout,
  metadata collection without `--offline`, manual packaging CI support and
  existing MCP component tools in the smoke test. The standard-only bundle
  and separate native ZIP policy remain intact.
- The implementation commit contains the rebase resolutions and required
  Co-Authored-By trailer; its revision changes on subsequent rebases. No commits
  were amended.
- Pull request: https://github.com/mika-f/Celesta/pull/810.

## Checks after rebase

- Per-build codec tests: 103 passed, including GPU export and optional codec
  process transport/alpha checks, with separate local native FFmpeg prefixes.
- `cargo test -p celesta-plugin-host --locked`: 111 passed.
- Per-build codec and plugin-host Clippy with `--all-targets --locked -- -D warnings`: passed.
- Plugin-script TypeScript checks and Vitest: 230 passed.
- Python package checks: 11 passed.
- Root and plugins Rust formatting, Node syntax and diff whitespace checks:
  passed.
- Website content check: passed, with 73 nonfatal metadata warnings.
- Release workflow lint: passed when excluding only the installed actionlint
  version's unsupported `$/.github/actions/setup-ffmpeg` notation diagnostic.

The three macOS standalone ZIPs were already built and verified after
extraction during the optional-download task. Windows/Linux native packaging,
Developer ID notarization and complete app installers remain untested locally;
release CI performs those builds. This task publishes a PR, not a release.
