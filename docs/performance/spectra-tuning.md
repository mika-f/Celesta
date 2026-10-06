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
σ of 8 or more on a copy shrunk by a power of two `f` (up to 8), the largest
that keeps σ / f ≥ 4 texels (the compensated σ' below then comes out
slightly smaller: 3.97 for σ 8):

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

In this 7-round run, the other workloads' medians moved as much as the
M4's drift does: `ribbons` from 1.08 to 2.10 ms and `text` from 1.08 to
1.93 ms, with ranges overlapping the base's (`ribbons`' head ran from 1.08
to 10.04 ms), and `ribbons` and `images` do not reach the changed code. Run
again with 11 rounds (those observations are not in the CSV), `rings`,
`ribbons`, `text`, `shapes` and `images` stayed within the noise: medians
within −5% to +9%, with overlapping ranges. On the RTX 4070 below, which
does not drift, `text` got faster.

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

## On a GeForce RTX 4070

The same workloads on Windows 11 with an i7-13700F and an RTX 4070
(Vulkan, driver 610.88) told a different story: after the changes above,
the GPU finished every frame almost at once (under 0.2 ms of waiting per
frame), and the CPU set the pace. Split the same way, per frame:

| Workload | Prepare (CPU) | Copy out of the readback buffer | Submit |
| --- | ---: | ---: | ---: |
| nebula | 4.3–4.7 | 1.3–1.5 | 0.5–0.6 |
| spectra | 2.3–3.3 | 1.5–2.0 | 0.4–0.6 |
| rings | 1.5–2.3 | 1.3–2.1 | 0.1–0.2 |
| images | 4.3 | 1.5 | 0.1 |

ms/frame, two runs. Four changes came out of it. Rendered frames stay
bit-identical: every workload's frames 13, 57 and 120 match the previous
binary exactly.

### 4. Copying frames out on a thread of their own

`submit` reclaimed the oldest frame on the caller's thread, copying it out
of its mapped buffer into a new vector. On Windows, that vector's 8 MB are
faulted in page by page: copying into fresh memory took 1.4–1.8 ms, into
reused memory 0.5 ms. A `ReadbackWorker` thread
(`crates/gpu-renderer/src/readback.rs`) now maps, copies and unmaps each
submitted frame in order, and `submit` and `drain` only receive them. The
copy overlaps with preparing the next frames; for workloads that prepare
little, the thread's copy (about 2 ms of RGBA at 1080p, under 1 ms for
the exporter's yuv420p) is now the limit. Two threads taking turns did not
help: the page faults serialize.

### 5. Outlining paths in parallel, alongside the text

`prepare_layer` stroked, flattened and binned each path into tiles as it
walked the scene. Paths are now left for after the walk like text, and
`outline_paths` outlines them on rayon's threads while `rasterize_texts`
rasterizes the frame's new text. Each path's entries are built with indices
counted from its own start; `place_paths` chooses where each goes in the
frame's buffer, in order, and copies them there in parallel, so the buffer
holds exactly the entries it did. `rings` went from 1.4 to about 0.8
ms/frame for its paths, NEBULA from 1.4 to 0.5.

### 6. Unstroked glyphs are the text frame

Text without a stroke was composited onto an empty frame of its own size,
which copies each pixel (0.65 ms for NEBULA's 760x170 title, rasterized
every frame as its letter spacing animates). The glyph pixels are now the
frame as they are; a gradient fill that fades to transparent zeroes those
pixels, as compositing did. Glyph pixels without coverage are skipped.

### 7. One image render per layout

`ImageSources` kept one render per image. The `images` workload draws one
image as both `cover` and `contain`, so every layer rendered it again,
every frame: 4.3 ms. Each source now keeps the latest render of up to four
sizes and fits.

### Results

`python scripts/bench.py compare --base 4847b18 --rounds 5` and
`--base 23bc933 --rounds 7` (the head of the changes above), on the
machine above. Raw observations are in
[spectra-tuning-rtx4070.csv](spectra-tuning-rtx4070.csv): `base` is
`4847b18`, `pr` is `23bc933`, and `head` holds both comparisons' runs.

| Workload | `4847b18` | `23bc933` | Head | vs `4847b18` | vs `23bc933` |
| --- | ---: | ---: | ---: | ---: | ---: |
| nebula | 7.72 | 6.83 | 3.86–3.87 | −50% | −43% |
| spectra | 5.58 | 4.44 | 2.97–3.25 | −42% | −33% |
| rings | 3.36 | 3.36 | 2.39–2.45 | −29% | −27% |
| blur | 4.41 | 2.97 | 2.18–2.26 | −49% | −27% |
| ribbons | 2.74 | 2.70 | 1.96–2.02 | −26% | −28% |
| text | 2.75 | 3.01 | 1.98–2.17 | −21% | −34% |
| shapes | 2.15 | 2.23 | 1.91–1.97 | −11% | −12% |
| images | 5.56 | 5.57 | 1.83–1.88 | −67% | −66% |

Medians in ms/frame; the head's are from each comparison. `shapes` is
within the noise (its ranges overlap), as are `rings` and `blur` against
`23bc933`.

Exporting SPECTRA did not get faster on this machine: 9.2 s with libx264
`medium` and 7.0 s with `ultrafast`, before and after. About 1.4 s of that
is starting up, and the remaining 8 ms per frame is more than rendering's
3 ms: the export is paced by the rest of its pipeline (the React
evaluation and the encoder), not measured separately here.

Also tried, and left out: handing out text to the rasterizer forks one at a
time instead of in fixed shares (no difference beyond the noise, and a
text no longer stays with the fork that has its glyphs cached).

## Not done

- Hardware encoding (VideoToolbox, NVENC) would lift the export's current
  limit, but changes its output and options.
- Text that changes every frame is still rasterized on the CPU and uploaded
  as a texture per layer. A glyph atlas drawn on the GPU would remove most
  of that, at the cost of reworking strokes, gradients and their parity
  with the CPU renderer.
- Every effect is now up to four cheap passes per filter, and render pass
  creation shows in the CPU profile (about 12% of SPECTRA's main thread).
  On the RTX 4070, `queue.submit` takes 0.5–1.3 ms of the `blur` and
  NEBULA frames.
- The readback copy faults in a new frame's memory on Windows. Reusing
  frame buffers would avoid it, but the exporter hands each frame's vector
  to the encoder, which frees it on its own thread.
- On the RTX 4070 the SPECTRA export is paced by the React evaluation and
  the encoder, not by rendering.
