# Lone effect results are not copied

## Specification

GPU rendering performance tuning (CPU export is out of scope). A layer whose
only effects are a blur, or custom shaders and built-in color filters, no
longer copies its result onto one more canvas before it is drawn onto its
parent. Output must not change.

## Implementation

`Compositor::draw_group` in `crates/gpu-renderer/src/compositor.rs` returns
the blurred canvas, or the canvas the filters ended on, when the layer has no
shadow and no glow. It takes the result canvas and runs the compositing pass
only when there is more than one texture to draw. The skipped pass drew one
texture through the group's inner instance (identity, full opacity, `Normal`)
onto a transparent canvas, which is a copy.

## Checks

- `lone_effect_results_are_not_copied` (new, `crates/gpu-renderer/src/tests.rs`):
  a blur-only and a saturation-only layer use one canvas fewer than before
  (3 and 2; on `main` the blur case takes 4), and the blurred one matches the
  CPU renderer within 5. The CPU renderer has no color filters;
  `tests/color_filter.rs` covers their pixels.
- `cargo test --release -p celesta-gpu-renderer` on lavapipe.
- Last-frame dumps of `blur`, `shaders`, `shader-chain`, `shader-nest`,
  `color-filters`, `masks` and `shapes` are pixel-identical before and after.
- Benchmarks: see `docs/performance/effect-result-copy.md`. llvmpipe only; not
  measured on a real GPU.
