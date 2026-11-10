# FFmpeg codec download PR preparation

## Specification

Commit the approved FFmpeg build/plugin split, scoped license placement and
optional codec downloads, then open a pull request against main.

## Implementation

- Branch: `codex/ffmpeg-codec-downloads`.
- Rebased the implementation onto main at `a85ab48a` before pushing. Conflict
  resolution preserves main's macOS `Contents/Resources/PlugIns` layout,
  metadata collection without `--offline`, manual packaging CI support and
  existing MCP component tools in the smoke test. The standard-only bundle
  and separate native ZIP policy remain intact.
- Commit `d88563ef` contains the implementation and rebase resolutions, with
  the required Co-Authored-By trailer. No commits were amended.
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
