# Tuning the GPU export with SPECTRA

[SPECTRA](../../examples/spectra/README.md) is a React reel made for this
work: five chapters that between them use blur, shadows, glows, every blend
mode, clips, paths, text with strokes and phrase wrapping, highlighted code,
images and a camera, plus crossfades where two chapters draw at once. It is
the `spectra` workload of `celesta-bench`, which measures runs of frames in
every chapter (see [benchmarking.md](benchmarking.md)).

Three changes came out of profiling it. The GPU renderer's output stays
within the existing parity tolerance of the CPU renderer, which is
unchanged except for two bit-identical speedups in the text rasterizer the
two renderers share.

## Where the time went

Apple M4, Metal, release build, 1920x1080. The bench was instrumented
temporarily (not committed) to split each frame into preparing the draws on
the CPU and waiting for the GPU, per chapter:

| Chapter (bench run) | Total | Prepare (CPU) | GPU wait |
| --- | ---: | ---: | ---: |
| IGNITION | 17.7 | 4.1 | 12.9 |
| crossfade into GEOMETRY | 2.2 | 1.1 | 0.7 |
| TYPOGRAPHY | 5.2 | 3.7 | 1.2 |
| crossfade into SOURCE | 16.7 | 1.0 | 15.4 |
| SIGNAL | 5.1 | 0.7 | 4.2 |

ms/frame. The GPU waits were the Gaussian blurs of large radii: IGNITION's
six σ 36 lights and its title's σ 28 glow, SOURCE's editor card with a σ 40
shadow and a σ 20 glow. NEBULA (σ 48 blobs) and the `blur` workload were
GPU-bound the same way.

## 1. Large blurs at reduced resolution

`EffectProcessor::apply` (`crates/gpu-renderer/src/effect.rs`) now blurs a
σ of 8 or more on a copy shrunk by a power of two `f` (up to 8), chosen so
the blur there keeps σ' ≥ 4 texels:

1. `downsample`: each texel is the mean of its `f`x`f` block. Texels past
   the canvas count as transparent, as they do for the blur.
2. The existing paired-tap Gaussian, both directions, at σ'.
3. `upsample`: bilinear enlargement back onto a full-size canvas, where the
   shadow offset and the tint are applied.

The box adds a variance of `(f² − 1) / 12` and the bilinear tent `f² / 6`
(in output pixels), so `σ'² = (σ² − f²/4 + 1/12) / f²` keeps the overall
spread at σ. Each blur pass then shades `f²` times fewer pixels with `f`
times fewer taps. Scissor regions shrink with the copy.

Two edge details matter for parity:

- Within the canvas, the blurred field continues past its outermost texel
  centers, so the upsample extrapolates the last half texel linearly from
  the two texels at the edge. Treating them as transparent darkened the
  edge (17 code values off the CPU in `large_blurs_match_cpu`); clamping
  still left 4.
- A shadow that reads past the canvas reads transparency there, weighted as
  the full-resolution pass's bilinear read is.

Parity with the CPU renderer's exact Gaussian (maximum channel difference,
tolerance 5):

| Test | Full resolution | Reduced |
| --- | ---: | ---: |
| `large_blurs_match_cpu` (σ 24; σ 13.5 + shadow σ 17 + glow σ 64 at two scene edges) | 2 | 2 |
| `reduced_resolution_blurs_match_cpu` (new: factors 2, 4, 8, kernels off the top edge, a shadow reading past the left edge) | — | ≤ 2 |

On real frames (PNG stills from the exporter, old and new binaries),
SPECTRA's frames 30–680 and NEBULA's frames 0–550 differ by at most 4, in
at most 0.001% of channels by more than 2, at 59–88 dB PSNR.

## 2. Text strokes and glyph compositing

Text is rasterized on the CPU (shared with the CPU renderer) and changes
every frame for counters, timecodes, typed text and scaling words.

- `dilate_mask`, which grows the glyph mask for a stroke, spread every pixel
  over a `(2r + 1)²` square. A square's maximum is the maximum of its rows'
  maxima, so it now takes a row pass and then a pass across rows:
  `2(2r + 1)` per pixel instead of `(2r + 1)²`. Same output; a test checks
  it against the old method.
- `blend` returns early when the source is transparent, opaque, or over a
  transparent pixel. In each case the general formula divides by the alpha
  it multiplied by, so it rounds back exactly to the copy; a test checks the
  shortcuts against the formula over a grid of inputs. Most glyph pixels
  take one.

TYPOGRAPHY's preparation went from 3.7 to 1.6 ms/frame.

## 3. Sorting path tiles by row first

`bin_tiles` sorted every edge piece of a path by tile with a comparison
sort. It now counts pieces into tile rows, then sorts each row by column.
For `rings` (24 stroked ellipses, about 35,000 pieces and 235,000 tiles per
frame) the sort fell from 0.52 to 0.33 ms/frame. A counting sort by tile was
slower: the regions are mostly empty tiles. A stable sort that merges the
pieces' runs was no faster either.

## Results

`python3 scripts/bench.py compare --base cbc79eb --rounds 7`, where
`cbc79eb` adds SPECTRA and the `spectra` workload without these changes.
Raw observations are in [spectra-tuning.csv](spectra-tuning.csv). The M4's
GPU speed drifts with temperature over a long run, so absolute numbers vary
between runs; the base's NEBULA ranged from 42 to 73 ms here.

| Workload | Base (ms/frame) | Head (ms/frame) | Change |
| --- | ---: | ---: | ---: |
| nebula | 49.42 | 6.83 (5.36–7.28) | −86% |
| spectra | 16.90 | 4.23 (3.20–8.51) | −75% |
| blur | 41.07 | 6.40 (5.24–10.31) | −84% |

The other workloads (`rings`, `ribbons`, `text`, `shapes`, `images`) were
compared again with 11 rounds and none moved beyond the noise: medians
within −5% to +9%, with overlapping ranges, and `ribbons` and `images` do
not reach the changed code.

### The whole export

`celesta-exporter --react examples/spectra/film.tsx` (690 frames, libx264
`medium`, GPU yuv420p conversion) took 9.7 s before and 6.7–7.9 s after.
The export is no longer limited by rendering:

| Export | `medium` | `ultrafast` |
| --- | ---: | ---: |
| SPECTRA | 106 fps | 206 fps |
| NEBULA | 59 fps | 107 fps |

Rendering now takes about 3 ms/frame for SPECTRA and 5 ms for NEBULA, and
the React evaluation runs on its own thread one frame ahead (about 5–7 ms
for SPECTRA). With the default preset, libx264 sets the pace.

## Not done

- Hardware encoding (VideoToolbox, NVENC) would lift the export's current
  limit, but changes its output and options.
- Text that changes every frame is still rasterized on the CPU and uploaded
  as a texture per layer. A glyph atlas drawn on the GPU would remove most
  of that, at the cost of reworking strokes, gradients and their parity
  with the CPU renderer.
- Path outlines are stroked, flattened and binned on one thread; they could
  be prepared in parallel like text.
- Every effect is now up to four cheap passes per filter, and render pass
  creation shows in the CPU profile (about 12% of SPECTRA's main thread).
