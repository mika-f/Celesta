# SPECTRA and GPU export tuning (2026-10-06)

`examples/spectra` is a five-chapter React reel that uses most renderer
features, added as the `spectra` workload of `celesta-bench`. Profiling it
led to three changes; measurements and details are in
`docs/performance/spectra-tuning.md`.

- **Workload runs.** `Source::React` takes the frames its runs start at.
  `run_frame` splits the measured frames evenly into runs of consecutive
  frames, one per start, each `warmup` frames past its start, with the
  warmup frames before the first run. `nebula` uses `&[0]`, so it renders
  the same frames as before. `spectra` starts at 60, 140, 300, 400 and 600.
  `perf.yml` also triggers on `examples/spectra/**`, `packages/code/**` and
  `packages/math/**`.
- **Reduced-resolution blur.** `effect::reduced_blur` picks a factor of 2,
  4 or 8 for σ ≥ 8 (σ' ≥ 4 texels after the factor), compensating for the
  box and tent variance. `apply` then runs `downsample`, the two blur passes
  and `upsample` (offset and tint), so each filter takes up to
  `PASSES_PER_FILTER` (4) passes and `begin_frame` reserves accordingly.
  `upsample` extrapolates the outermost half texel inside the canvas and
  reads transparency past it for shadows. Smaller σ keep the full-resolution
  path unchanged. `reduced_resolution_blurs_match_cpu` covers every factor
  at the scene edges; the measured maximum difference is 2.
- **Text rasterizer** (`celesta-renderer`, shared by both renderers):
  `dilate_mask` is a separable max filter and `blend` shortcuts transparent
  and opaque cases. Both are bit-identical and tested against the previous
  formulas.
- **Path tiles** are counted into rows and sorted per row by column.
- Validation: `celesta-gpu-renderer` (70), `celesta-renderer` (94 + 1) and
  `celesta-bench` (2) tests pass on an M4 with Metal; `tsc` passes for
  `examples/spectra` against the staged project types. Rust 1.99's rustfmt
  and clippy report findings in files this change does not touch
  (`composition/src/animation.rs`, `budoux`, `renderer/src/layer.rs`, ...);
  the changed files are clean.
- The export is now limited by libx264 (`medium`), not rendering. See
  "Not done" in the performance note for the next steps.
- **On a GeForce RTX 4070** (Windows, i7-13700F) the CPU set the pace, so
  four more changes followed; all keep the output bit-identical.
  `ReadbackWorker` (`readback.rs`) maps, copies and unmaps each `submit`ted
  frame on its own thread, in order, and `reclaim_oldest` only receives
  it; dropping the renderer joins the thread. Paths become
  `PreparedItem::PendingPath` like text; `prepare_draws` runs
  `outline_paths` (flatten and `PathEntries::build`, relative indices) and
  `rasterize_texts` under `rayon::join`, then `place_texts` and
  `place_paths` (which offsets each path's indices and copies the entries
  in parallel). `pending_paths` is cleared at the start of a frame, so a
  failed frame leaves nothing behind. Unstroked text uses its glyph pixels
  as the frame, and `ImageSources` keeps one render per size and fit (up
  to four per source). `celesta-gpu-renderer` (71), `celesta-renderer`
  (95) and `celesta-exporter` tests pass on Windows with Vulkan.
  `scripts/bench.py` prints its report as UTF-8, which Windows consoles
  needed.
