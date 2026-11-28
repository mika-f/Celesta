# Variable font weights in the native renderer (#776)

With cosmic-text 0.19 (#520), the native renderer can shape and rasterize a
named variable font at the requested `fontWeight`. `matched_weight` still
selects a face within the family first. For a face with a `wght` axis, it
passes the requested weight clamped to that axis's range. Static faces still
use CSS nearest-weight matching; stylesheet families still match their
`@font-face` descriptors. No other variable axes are supported.

Weights are not quantized: e.g. 437 remains 437 where the axis supports it.
Animations using many distinct values can still incur cosmic-text font
matching and cache costs.

## Fixture

`crates/renderer/tests/fixtures/CelestaVariableTest.ttf` is a renamed subset of
[Montserrat from Google Fonts](https://github.com/google/fonts/tree/main/ofl/montserrat)
under the SIL Open Font License 1.1 (see the adjacent license). The source
`Montserrat[wght].ttf` has SHA-256
`0f7b311b2f3279e4eef9b2f968bcdbab6e28f4daeb1f049f4f278a902bcd82f7`.
FontTools restricts its only axis, `wght`, to 200–800 and keeps the characters
in `CELESTA 0123456789`.

## Validation

- Regression coverage: glyph advances and ink coverage change between 400
  and 700; out-of-range requests draw at the axis endpoints; static-family
  CSS matching and stylesheet descriptors retain their behavior.
- Run `cargo fmt --all -- --check`,
  `cargo clippy --release -p celesta-renderer -p celesta-gpu-renderer --all-targets -- -D warnings`
  and Vulkan software renderer tests with
  `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json XDG_RUNTIME_DIR=/tmp cargo test --release -p celesta-renderer -p celesta-gpu-renderer`.
- The #520 branch predates the media-plugin split and requires FFmpeg
  development libraries. Tests, Clippy, and the scene were run on current
  `main` with #520 and this change replayed; fmt was run on the feature branch.
- The validation scene renders one named font at 100, 200, 400, 700, 800,
  and 900. Before the change the text uses the same instance on every row;
  after it, 100 matches 200 and 900 matches 800, with 400 and 700 visibly
  distinct between them.
