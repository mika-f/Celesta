# Scope third-party license texts to media plugins

## Specification

Avoid suggesting that all Celesta source is offered under MIT, Apache, GPL
and LGPL merely because four license texts sit at the repository root.
Celesta's own source remains MIT OR Apache-2.0. Third-party FFmpeg texts
belong with the plugin builds to which they apply.

## Implementation

- Root retains only `LICENSE-MIT` and `LICENSE-APACHE`.
- GPL text moves to `plugins/ffmpeg-x264/LICENSE-GPL-2.0`.
- LGPL text moves to `plugins/ffmpeg/LICENSE-LGPL-2.1`; Opus and VPX manifests
  reference that shared file through `../ffmpeg/`.
- All four plugin manifests still copy their declared license texts and
  source/relinking notices into their own distribution `licenses/` folder.
- macOS and Windows packaging explicitly copy only MIT/Apache texts into the
  package-wide license folder. The GPL source offer is copied only by the
  x264 plugin manifest, not duplicated in the package-wide folder.
- README, source notices, packaging guides/Windows startup text, English and
  Japanese plugin guides, and the Celesta Agent Skill clarify the distinction
  between Celesta's source licensing and the linked native plugin builds.
- Dependency-based affected-plugin test expectations follow the moved texts.
  Historical handoff/planning notes are unchanged.

## Validation

- `pnpm --dir scripts/plugins test`: TypeScript checks and 230 Vitest tests
  passed; repository plugin input discovery resolves all license paths.
  Used absolute local `CARGO`/`RUSTC` paths for scratch-tree tests, as in the
  build-variant task.
- `python3 -B -m unittest scripts/test_package_macos.py`: 8 passed.
- `git diff --check`: passed.
- File inventory confirms only MIT/Apache license texts at the root.

No Rust/runtime/rendering code changed in this follow-up. Native codec tests
from the preceding build-variant task were not repeated. Windows and full
release packaging were not executed locally.
