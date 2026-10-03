# Paired Gaussian blur taps: NEBULA's blobs

NEBULA's six blob layers (`blur: 48`) took about 66 of the 71 ms of GPU time
per frame on an Apple M4 (see "Where NEBULA's GPU time goes" in PR #119's
`gpu-path-coverage.md`). The time followed the number of texel reads. The
GPU blur in `crates/gpu-renderer/src/effect.wgsl` now reads half as many
texels for the same kernel, and its output stays within the effect tests'
tolerance of the CPU renderer's exact Gaussian. The CPU renderer is unchanged.

## How it works

Each pass is still the separable Gaussian with extent `ceil(3σ)` (σ clamped
to 64), normalized by the sum of all its weights, with texels outside the
canvas counting as transparent. Two changes:

1. **Two taps per fetch.** Adjacent texels `j` and `j + 1` with weights `a`
   and `b` contribute `a·T(j) + b·T(j+1)`, which is `(a + b)` times a
   bilinear sample at `j + b / (a + b)`. The canvas texture is now bound as
   filterable (`Rgba8Unorm` always is), and the effect's parameter bind group
   carries a linear, clamp-to-edge sampler. The `2·extent + 1` taps become
   `extent + 1` fetches: 145 instead of 289 per pixel and pass at σ 48.
2. **Weights by recurrence.** `g(k+1) = g(k)·r` and `r` shrinks by
   `exp(-1/σ²)` each step, so the loop has no `exp`.

Correctness details:

- **Edges.** Before pairing, a texel outside the canvas gets weight 0. A pair
  with one texel outside therefore samples exactly at the inside texel's
  center, and the clamp-to-edge sampler never mixes in a duplicated edge
  texel. The normalization still sums all weights, as before and as the CPU
  does.
- **Fractional shadow offsets.** The second pass of a shadow samples at
  `center − offset`, which can be fractional. Along the blur, texel `base + k`
  gets weight `(1 − f)·g(k) + f·g(k − 1)`. That is the old per-tap bilinear
  read folded into the kernel. Over `k ∈ [−extent, extent + 1]` the count is
  even, so the pairs tile it with the same `extent + 1` fetches. Across the
  blur, the column (or row) is the same for every fetch. It is resolved once
  per pixel with the same edge rule, so a fractional offset beside the edge
  scales the inside texel instead of reading a clamped one. `blur_reach` is
  unchanged.
- **Tiny σ.** For σ below about 0.09, `g(−extent)` underflows and the first
  ratio overflows, which would give `0·∞`. They are clamped to 1e-30 and
  1e30, so the center still gets weight 1. The true off-center weights there
  are below 1e-30.
- Hardware bilinear filtering has finite sub-texel precision (8 bits is
  typical). That error goes into the pair's interpolation ratio, not its
  weight, and is far below one 8-bit step after normalization.

Shadows and glows go through the same `pass`, so the tinting `mode` is
unchanged. `layer.wgsl` still reads with `textureLoad`; declaring its texture
filterable does not change that.

## Pixel comparison with the CPU renderer

The effect tests require a maximum channel difference of 5 or less against
the CPU renderer. Measured maxima:

| Test | Before | After |
| --- | ---: | ---: |
| `blurred_group_with_shadow_and_glow_matches_cpu` (σ 1.5–3, fractional shadow offset) | 2 | 2 |
| Same, zero-radius shadow with fractional offset | 0 | 0 |
| `effects_on_small_layers_match_cpu` (edge-crossing, nested, empty) | 1 | 2 |
| `large_blurs_match_cpu` (new): σ 24; σ 13.5 + shadow σ 17 at (−9.5, 6.75) + glow σ 64, on content touching two canvas edges, screen blend at 0.55 | 2 | 2 |
| Same scene at σ 0.05 / 0.3 (new) | 1 / 1 | 1 / 1 |

On NEBULA itself, the export's yuv420p frames from the old and new shaders
(60 frames spread over the 600, in one process) differ by at most 1, in
0.08% of samples (147,341 of 186,624,000).

## Measurements

Apple M4 (10 cores), macOS 26.6.2, Metal, Rust 1.99.0, Homebrew FFmpeg 8.1.3,
release builds, October 3, 2026. "Before" is `main` at `c3834a8`. Raw
observations are in [gaussian-blur-pairing.csv](gaussian-blur-pairing.csv).
These are not the Core i7-13700F / RTX 4070 numbers of
[path-rasterization.md](path-rasterization.md) and should not be compared
with them.

### NEBULA's scenes, GPU only

NEBULA's real scenes were taken from the React bridge (60 frames spread over
the 600) and rendered with the export's settings: pipelined `submit`/`drain`,
yuv420p readback, `RenderQuality::Final`, and the entry's directory as the
asset root. Three renderers lived in one process: the old shader, pairing
with `exp` per tap, and the final shader. They ran interleaved, in rotated
order, for nine rounds. The M4's GPU speed drifts with temperature: the old
shader ranged from 71.7 to 91.3 ms within this run.

| Shader | Median GPU ms/frame | Range | Ratio to before, per round |
| --- | ---: | ---: | ---: |
| Before (289 `textureLoad`s, `exp` per tap) | 78.62 | 71.72–91.34 | — |
| Paired taps, `exp` per tap | 44.17 | 37.52–53.13 | 0.51–0.63 |
| Paired taps, recurrence (this change) | 35.99 | 31.54–44.55 | 0.41–0.50 |

The frame time drops to 0.46× of before. Most of it is the blob blurs, so
the blur itself sped up by more than the 2× in reads. Removing `exp` did not
help before (PR #119 measured 54–56 against 59 ms, within the drift), because
the loop was bound by texel reads. With half the reads, the two `exp`s per
fetch became visible. The recurrence was faster than pairing with `exp` in
every round, by 0.02–0.16 of the old shader's time (median 44.17 to
35.99 ms).

An earlier exploratory run (eight rounds, medians only, not in the CSV) also
tried dropping the per-pair edge checks. That variant is wrong at the edges
and measured 38.7 against 40.4 ms, so a separate fast path for interior
pixels is not worth it.

### Full NEBULA export

600 frames, libx264 `medium` CRF 18. Timed with `/usr/bin/time -l` from CLI
start to exit, alternating the before/after binaries, three runs each
(medians):

| | Before | After | Change |
| --- | ---: | ---: | ---: |
| Wall time | 49.39 s | 37.16 s | −24.8% |
| User CPU | 143.5 s | 153.2 s | +6.8% |
| Max resident set | 970 MB | 970 MB | — |
| Peak memory footprint | 1,478 MB | 1,359 MB | −8.0% |

The ranges did not overlap: 48.89–58.79 s before and 34.37–39.31 s after.
The change is GPU-only and adds no CPU work. The user-CPU difference was not
investigated. It may come from the encoder and React evaluation overlapping
more of a shorter export.

Each version's three MP4s are byte-identical: SHA-256 `d0eb8dd8021dcaa4…`
before (the same as PR #119's "before") and `b9c78a8a86eaae4f…` after.
FFprobe confirms H.264, 1920x1080, and 600 frames. The two versions' MP4s
differ at 44.1 dB PSNR (y 43.0). That mostly reflects the encoder making
different decisions: the raw frames differ by at most 1, as above.

`examples/versus/bench/results.json` and the comparison film keep the
RTX 4070 measurements. These M4 numbers do not belong there.

### Not done: reduced-resolution blur

For large σ, blurring a downsampled copy would cut reads by many times more.
But the CPU renderer's exact Gaussian would then have to change the same
way, or the parity tolerance would have to be relaxed explicitly. Pairing
keeps both renderers on the same kernel. With pairing in place, the blobs
still account for most of NEBULA's GPU time, so downsampling is the next
step if more is needed.

## Reproduce

The GPU-only comparison used a temporary example (not committed). It spawned
`celesta_react_bridge::ReactBridge` with `runtime_paths()` on the
canonicalized `nebula.tsx`, collected `evaluate_at(Time::frames(i *
duration / 60, rate), None).scene` for `i` in `0..60`, and timed
`GpuRenderer::submit` over all 60 scenes plus `drain`. The renderers were
built like `export_renderer`. The old shader was selected through a
temporary constructor switch. For the exports:

```sh
cargo build --release --locked -p celesta-exporter --bin celesta-exporter
/usr/bin/time -l target/release/celesta-exporter --no-ui --overwrite --react examples/versus/bench/celesta/nebula.tsx target/nebula.mp4
```

On macOS with Homebrew's FFmpeg 9 installed as the default, point
`PKG_CONFIG_PATH` at `$(brew --prefix ffmpeg@8)/lib/pkgconfig` first.
