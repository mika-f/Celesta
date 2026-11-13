# FFmpeg for Windows: check every triplet's features (#818)

Follows up the licence check that
[`2026-11-11-ffmpeg-x264-only-split.md`](2026-11-11-ffmpeg-x264-only-split.md)
added to `.github/workflows/ffmpeg-windows.yml`
([issue #818](https://github.com/mika-f/Celesta/issues/818), from the review
of [PR #810](https://github.com/mika-f/Celesta/pull/810)).

## Specification

- A `Celesta.FFmpeg.Windows.*` package ships vcpkg's whole `installed/` tree,
  both `x64-windows-static-md-release` (what Celesta links) and
  `x64-windows-release` (the DLLs). vcpkg resolves features per triplet, so
  every triplet must have every feature in `FFMPEG_FEATURES` and none of
  `nonfree` and `version3`, nor `gpl` in the LGPL package. The old check read
  the static triplet only.
- The check ran only in `ffmpeg-windows.yml`, which is `workflow_dispatch`
  only and had not run since the check was added. It must run on pull
  requests without publishing anything.

## Implementation

- `scripts/check-ffmpeg-features.ps1` holds the check.
  `-InstalledPath` is vcpkg's `installed/`, `-Features` takes a
  comma-separated list, `-License` the SPDX expression. It reads
  `vcpkg/status`, then `vcpkg/updates/*` in name order (vcpkg appends
  installs there and folds them into `status` on its next database load), and
  a later paragraph for the same package, feature, and triplet replaces an
  earlier one. Paragraphs split on blank lines with LF or CRLF; indented
  continuation lines are skipped. Only `Status: install ok installed` counts.
- It checks both triplets, which must have ffmpeg installed, and any other
  triplet that has ffmpeg in the tree. It prints each triplet's features and
  throws one error listing every problem.
- `ffmpeg-windows.yml`'s Pack step calls the script in place of its inline
  parser.
- `setup-ffmpeg` runs it on each pinned package after extracting it, with the
  same features and licence as the workflow's matrix. Every Windows job that
  uses the action checks the parser against the real `8.1.2-2` status files.
- `scripts/check-ffmpeg-features.Tests.ps1` (Pester 5) covers both triplets,
  CRLF, `gpl` on the dynamic triplet only, a missing feature, a missing
  triplet, no ffmpeg, a missing `status`, an extra triplet, not-installed
  paragraphs, `updates/` adding and replacing paragraphs, and the GPL
  package's rules. CI's Linux test job runs it with the runner image's
  PowerShell and Pester.
- `scripts/check-ffmpeg-features.ps1` is in `SHARED_INPUTS`
  (`scripts/plugins/inputs.mts`), since the plugins job's `setup-ffmpeg`
  runs it.

## Validation

- `Invoke-Pester -Path scripts/check-ffmpeg-features.Tests.ps1 -CI` on macOS
  (PowerShell 7.7.0 from `mise exec powershell@7`, Pester 6.2.0 saved to a
  scratch module path): 13 passed.
- `pnpm --dir scripts/plugins test`: 247 passed.
- `actionlint` 1.7.12 on `ci.yml` and `ffmpeg-windows.yml` reports only the
  existing `background`/`wait` steps it does not know.
- Not run locally: the script on the real pinned packages (their download
  needs GitHub Packages access). The Windows CI jobs that run `setup-ffmpeg`
  check it. `ffmpeg-windows.yml` was not rerun.
