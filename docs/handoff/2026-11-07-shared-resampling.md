# Shared CPU resampling (2026-11-07)

Addresses mika-f/Celesta#576, following the investigation on the issue.
The five resize paths listed there are not one filter. They have different
outputs on purpose. For opaque black beside white shrunk to one pixel, the
contact sheet gives 188 (linear light) and the others give 128. Only kernels
that give the same bytes are shared. The others stay as they are, with a
comment that says why.

## Specification

- **`celesta_renderer::resample`** is the shared module. It holds:
  - `resize_rgba` (moved unchanged from `image_source`): a triangle filter
    with alpha-weighted color in `f32`, for any size change. It is used for
    image and PSD fitting and for GPU texture limits.
  - `box_span`: the even integer partition of `0..from` into `to` blocks.
    The last pixel of a halved odd size folds into the block before it.
  - `average_block`: the rounded integer average of a rectangle of
    straight RGBA. `Coverage::Alpha` weights color by alpha, and a fully
    transparent block becomes transparent black. `Coverage::Straight`
    averages each channel on its own. Alpha is the plain average in both
    modes. Both modes give the same bytes for opaque blocks.
  - `box_downscale`: `average_block` over `box_span` on both axes. It only
    shrinks.
- **GPU mip levels** (`gpu-renderer` `texture::downsample`) are
  `box_downscale(.., Coverage::Alpha)` to `max(1, side / 2)` on each axis:
  half size, rounded down, and a side of 1 stays 1.
- **PSD layer reduction** (`psd_source::reduce_layer`) keeps its own
  canvas-aligned power-of-two blocks. At an odd canvas edge the blocks are
  smaller instead of folded into their neighbours, and they are cropped to
  the layer bounds. Each block is `average_block(.., Coverage::Alpha)`.
- **MCP inline previews** (`mcp` `frames::inline_image`) are
  `box_downscale(.., Coverage::Straight)`. This keeps the old straight
  average. The investigation left the MCP transparency handling as a
  separate image-quality question. The PNG is now encoded with
  `image::codecs::png::PngEncoder`, and `celesta-mcp` no longer depends on
  `png` directly.
- **The contact sheet** (`exporter` `contact_sheet::area_taps`) is not
  shared. It averages premultiplied color in linear light and composites
  the result over the checkerboard in the same pass. Its comment says this.
- **`image::imageops::resize` replaces neither filter.** Its `Triangle`
  filter does not weight color by alpha. Opaque white beside transparent
  black shrinks to `[128, 128, 128, 128]` (grey) there, against
  `[255, 255, 255, 128]` from `resize_rgba`.

## Checks

- A temporary harness compared copies of the old GPU mip, PSD block, and
  MCP loops with the new code, byte for byte, on random pixels. The inputs
  were every size from 1×1 to 23×17. The alpha was opaque, binary, or
  random. The PSD factors were 2, 4, and 8. Every MCP target width was
  tried at three heights, 42,228 MCP cases in all. All outputs were equal.
  The harness was then removed. It is not part of the commit.
- Release timings on a 4096×4096 source, old against new: GPU mip
  45 → 27 ms, PSD factor 4 20 → 18 ms, MCP to 1568×1568 42 → 20 ms.
- New unit tests in `resample`: spans partition every source pixel once,
  alpha against straight coverage next to transparency, the two coverages
  agree on opaque blocks, and the integer box average on odd blocks. The
  existing `resize_rgba` tests moved with it.
- The existing tests pass: `celesta-renderer` lib (260 tests, including the
  PSD reduction and fitting tests), `celesta-gpu-renderer` lib
  (`mipmaps_average_color_by_coverage`), `celesta-exporter` lib (contact
  sheet), and the `celesta-mcp` `frames` tests.
- `cargo clippy -p celesta-renderer -p celesta-gpu-renderer
  -p celesta-exporter -p celesta-mcp --all-targets -- -D warnings` and
  `cargo fmt --all --check` pass.
- Before and after renders of the same scene, a PSD portrait at scales
  0.18 and 0.07 (PSD reduction) and a 360×400 PNG shown at 4x and 8x
  smaller (mip levels) on light and dark backgrounds, exported with
  `celesta-exporter --react .. --frame 0` from the merge base and from this
  branch, are the same PNG file byte for byte (`cmp`). The screenshots are
  on the pull request. The scene was a temporary file and is not committed.
