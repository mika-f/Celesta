# GPU path tiles draw only their covered bounds

## Specification

Tune GPU rendering performance. CPU export is out of scope.

Thin stroked paths (NEBULA's rings, the `rings` benchmark) spend most of
their GPU time in the path fragment shader. Every stored 8x8 tile drew a
quad over the whole tile, so a 1 px stroke crossing a tile shaded all 64
pixels to cover a few of them. Output must stay pixel-identical.

## Implementation

- `crates/gpu-renderer/src/path.rs`: `TileBounds::of` computes, per outline
  and tile, the pixel rectangle that can have nonzero winding: the full tile
  with a nonzero backdrop, nothing without edges, otherwise the rows and
  columns the tile's edges reach. Right of the rightmost edge is included
  only when the edges' windings do not cancel on every scanline (a sweep
  over the edges' y extents). `PathEntries::build` stores the union of the
  fill's and stroke's bounds as one more entry per tile, after the outline
  entries; `PathEntries::place` shifts only the outline entries' edge
  indices.
- `crates/gpu-renderer/src/layer.wgsl`: the path vertex shader reads the
  tile's last entry and places the quad over those bounds, clamped to the
  region. The fragment shader is unchanged.

The draw still has six vertices per tile, so `prepare.rs` and `encode.rs`
are unchanged. Each tile costs one more 16-byte entry in the path buffer.

## Checks

- New CPU tests: `tile_bounds_hold_every_pixel_with_winding` (32 seeded
  self-intersecting outlines, every sample with nonzero winding lies inside
  its tile's bounds, and the bounds leave out part of the tiles) and
  `tile_bounds_fit_a_thin_band`.
- `cargo test --release -p celesta-gpu-renderer` on lavapipe: 435 passed.
  `lists_text_layers_with_characters_their_family_has_no_glyph_for` fails
  on `main` too on this machine (its emoji font has the glyph).
- `cargo clippy --release -p celesta-gpu-renderer --all-targets`: clean.
- `celesta-bench --dump` of `rings`, `nebula`, `masks`, `shapes` and `text`
  is pixel-identical before and after.
- Measurements are in
  [`docs/performance/gpu-path-coverage.md`](../performance/gpu-path-coverage.md)
  (llvmpipe only; `rings` -19.9% instructions, 163–167 to 124–125 ms/frame).
  Not measured on a discrete GPU. SPECTRA was not run (its `code` plugin is
  not installed on the test machine).
