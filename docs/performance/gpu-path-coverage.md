# GPU Path coverage: issue #116

The GPU renderer used to rasterize every `LayerContent::Path` on the CPU
(the CPU renderer's tiny-skia rasterizer) and upload the RGBA result each
frame. It now computes coverage and paint per pixel in `layer.wgsl`; the CPU
strokes and flattens the outline and sorts its edges into tiles. Paths fall
back to the existing CPU rasterizer and a texture for that layer when any one
tile contains more than 64 edges of an outline, or when the path would exceed
the frame's storage buffer budget. Oversized fallback textures are resized
to the device's texture limit, with their original output bounds preserved.
The CPU renderer is unchanged and
remains the reference the GPU output is compared with.

## How it works

1. `celesta_renderer::flatten_path` builds the same outlines
   `rasterize_path` fills: the stroke is made by the same tiny-skia stroker in
   the layer's own coordinates (so a non-uniform scale stretches it, and
   caps, joins and the miter limit are tiny-skia's), then everything is
   transformed to output pixels and cut to the same region of the frame.
   Curves become straight edges within 0.05 px. What lies left of the region
   moves onto its left side and what lies right is dropped, which keeps the
   winding of every pixel inside; every cut lands exactly on its row or side,
   so the edges meet end to end.
2. `ShadedPath` / `bin_tiles` use Vello's backdrop binning scheme with
   8x8-pixel tiles (Vello's GPU pipeline uses 16x16): each tile keeps the parts of edges at or right of its left side,
   cut to its rows, a vertical edge down its left side wherever an edge
   crosses that side, and a backdrop, the winding just left of its top-left
   corner. Only tiles with edges or a nonzero backdrop are stored and drawn,
   so the empty inside of a thin ring costs nothing.
3. The vertex shader places one quad per stored tile. The fragment shader
   measures the nonzero coverage of the fill and of the stroke on four
   scanlines per pixel row (the scanlines tiny-skia samples), each span
   measured exactly. It paints the stroke over the fill (solid colors or
   gradients, evaluated at the pixel center mapped back to the layer's
   coordinates), then the ordinary layer path applies the layer's opacity,
   clip and blend mode once to the composited result.

So a translucent stroke that crosses itself is painted once, the fill does
not show through the stroke, and each path composites as one layer through
the existing clip, group, blend-mode and effect order. Each frame's tiles and
edges go into one storage buffer that is rebuilt every frame and only grows
to the largest frame (to the next power of two), like the instance buffer;
nothing is cached per shape, so continuously changing shapes cannot
accumulate GPU resources.

The scanline loop rescans a tile's edges for each crossing, so its cost is
quadratic in dense geometry. The 64-edge budget bounds that work before any
GPU entries or paints are appended. The storage budget is also checked per
layer before appending, including tile headers and duplicated edges; later
layers can use CPU textures without failing the whole frame. Indices stored
as f32 are kept exact by capping the buffer at 2^24 entries even if the device
allows a larger binding.

## Pixel comparison with the CPU renderer

tiny-skia rounds where scanlines cross edges to quarter pixels and flattens
curves more coarsely, so edge pixels can differ by a few 1/16 samples. The
GPU tests (`crates/gpu-renderer/src/lib.rs`) compare against the CPU renderer
(or, for rotated layers, which the CPU renderer does not draw, against its
rasterizer composited by hand) and allow at most 64 in any channel, 0.5 on
average, and at most 1% of channels differing by more than 16:

| Case | Max | Mean | Channels > 16 |
| --- | ---: | ---: | ---: |
| Curves, closed path seam, open path | 59 | 0.321 | 0.586% |
| Acute miter and bevel joins, repeated point, zero-length segments (round, square, butt caps) | 21 | 0.110 | 0.008% |
| Thin diagonals (0.5–1 px), 0.4 px curve, non-uniform and mirrored scale | 44 | 0.401 | 0.281% |
| Translucent self-intersecting stroke, translucent fill + stroke, layer opacity | 7 | 0.101 | 0% |
| Linear and radial gradients in scaled groups | 44 | 0.244 | 0.439% |
| Rounded clip, multiply blend, blurred group | 15 | 0.128 | 0% |
| 8 rotated rings overhanging the frame | 49 | 0.429 | 0.323% |

Where they differ, the GPU is closer to exact. Against an 8x supersampled
CPU rendering box-filtered to 1x, the curves case differs by up to 28 (0.175
on average) on the GPU and up to 52 (0.541) on the CPU.

Other tests check that the crossings in a translucent self-intersecting
stroke have the color of a single layer, that `render`, the pipelined export
(`submit`/`drain`) and the BGRA preview target produce identical pixels, that
the path buffer grows beyond its initial allocation and stays bounded through
200 frames of animated shapes (with output checked after bind-group replacement),
and that flattened edges meet end to end at many rotations (`celesta-renderer`).
Deterministic CPU tests compare binned winding with the full outline, and GPU
tests cover top-edge epsilon vertices, an exact integer hole on transparent
backgrounds, dense zigzag fallback, and storage-budget fallback.

## Measurements

### Review follow-up: RTX 4070 batch at b98a642

Measured October 4, 2026 at `b98a64254a2ee4a3ce00b5adcec359f679cca2ee`
(the clipping and density/storage fallback fixes, with the final PR #119 shader).
Core i7-13700F, RTX 4070, NVIDIA driver 610.88, Windows 11 build 26200,
**Vulkan** backend, Rust 1.97.1, release builds. Three sequential runs of
each example, 120 frames each at 1920x1080; medians below. Rings use the
example's 10-frame warmup; dense geometry includes initialization of the first
frame and scene construction. Raw observations, including scene-build times,
are in [gpu-path-coverage-rtx4070-review.csv](gpu-path-coverage-rtx4070-review.csv).

| Stage | Median | Range of three runs |
| --- | ---: | ---: |
| Rings: CPU raster reference | 10.04 ms/frame | 9.80–10.26 |
| Rings: CPU outline (excludes binning) | 0.33 ms/frame | 0.31–0.38 |
| Rings: GPU submit/drain, readback included | 5.45 ms/frame | 4.67–6.23 |
| Empty: GPU submit/drain | 3.34 ms/frame | 3.08–3.58 |
| Rings: one-at-a-time render | 7.03 ms/frame | 6.09–7.85 |
| Empty: one-at-a-time render | 4.39 ms/frame | 4.20–4.39 |
| Dense paths (217 layers): submit/drain | 10.74 ms/frame | 8.82–10.91 |
| Dense rects (3029 layers): submit/drain | 6.65 ms/frame | 6.01–7.48 |

This batch measures the implementation at the named commit, with substantial spread. It
has no paired before build, so it does not establish a speedup and should not
be compared directly with the M4 batch. It does not rerun the full NEBULA
MP4 export or the three-tool comparison film; those keep their historical
provenance below and in the versus README. No macOS/Metal machine was
available for remeasuring the M4 batch in this follow-up.

### Historical M4 batch

Apple M4 (10 cores), macOS 26.6.2, Metal, Rust 1.95.0, Homebrew FFmpeg 8.1.3,
release builds, October 3, 2026. "Before" is `main` at `0c72c5d`; "after" is
`9a91b34`. These are historical measurements, not measurements of the merged
shader at `1d62203` or the later fallback fixes. The shader changed in
`08552c6`, `7441d2b` and `1d62203` after this batch; changing CSV metric labels
in `08552c6` did not rerun the benchmark. Runs
alternate before/after, three each, and the tables give medians. Raw
observations are in [gpu-path-coverage.csv](gpu-path-coverage.csv). These
were not measured on the Core i7-13700F / RTX 4070 machine of
[path-rasterization.md](path-rasterization.md), so the two sets of numbers
should not be compared with each other.

The 24 NEBULA rings alone (`path-bench`, 120 frames at 1920x1080):

| Stage | Before | After |
| --- | ---: | ---: |
| CPU work per frame | 4.55 ms rasterization | 0.27 ms stroking and flattening, plus tile binning (timed with the line below) |
| GPU submit/drain, readback included | 6.46 ms/frame | 1.94 ms/frame |
| Same for empty frames (clear, copy, readback) | — | 0.33 ms/frame |
| One frame at a time (`render`), rings / empty | — | 6.18 / 2.54 ms |

The pipelined frames went from 6.46 to 1.94 ms (3.3x). The CPU no longer
fills coverage masks or converts RGBA, and no texture is uploaded: the frame's
tiles and edges are 0.72 MB (in a buffer that settles at 1 MiB), where the
rings' batch was uploaded as an RGBA texture of up to 8.3 MB, the whole
frame. The time beyond an empty frame's 0.33 ms (its clear, copy and
readback) is the rings' whole end-to-end cost: stroking, flattening and
binning on the CPU, the upload, and the shading.

`path-bench` still times the CPU renderer's rasterizer as well, which this
change does not touch; the CSV labels those runs of the new build
`cpu_raster_reference`.

The dense path ribbons of `dense-geometry-bench` (217 path layers, the
AFTERIMAGE-style geometry) went from 8.11 to 2.84 ms/frame (2.9x); its rect
version, which this change does not touch, measured 1.69 and 1.36 ms.

The whole NEBULA export (600 frames, libx264 `medium` CRF 18, CLI start to
exit) measured a median 47.07 s before and 47.46 s after. All three paired
runs were slower after the change, by 3.06, 1.12 and 0.39 s. The same exports
used a median 139.9 s of user CPU before and 121.0 s after; run 1 instead
increased from 96.96 to 105.17 s. Their median peak resident memory dropped
from 998 MB to 830 MB (peak
footprint 1,482 MB to 1,336 MB). Sampling the main thread during an
export suggests why the wall-time improvement was limited: on the M4 the export is
GPU-bound. About half of the main thread's samples wait in
`reclaim_oldest` for the GPU, the libx264 thread mostly waits for frames,
and path preparation is about 5%.

Run-to-run noise is substantial: even the unchanged `cpu_raster_reference`
code measured about 27% slower than the earlier `cpu_raster` batch. These
observations do not establish unchanged wall time or a CPU improvement in
every run, and must be remeasured for performance claims about the current shader.

Frame 300 as a PNG took 0.39 s both before and after.

Each version's three MP4s are byte-identical (SHA-256 `d0eb8dd8021dcaa4…`
before, `36ddf7872c14ab2f…` after; H.264, 1920x1080, 600 frames). Frame 300
differs between them at 56.4 dB PSNR; the MP4s at 44.0 dB, which also
includes the encoder making different decisions.

`examples/versus/bench/results.json`, `loop.json` and the comparison film
were remeasured on the RTX 4070 reference machine in the batch recorded by
`7b4ef96`. That batch also predates the final `1d62203` shader change; the
README identifies the batch and the available shader provenance. The film compares Celesta
with Remotion and fframes on that machine, so these M4 numbers do not belong there.

### Where NEBULA's GPU time goes

To find out, NEBULA's real scenes were taken from the React bridge (60
frames spread over the 600) and rendered with the export's settings
(pipelined `submit`/`drain`, yuv420p readback, final quality), with one
group of layers removed or changed at a time. Variants alternate within one
process, five rounds each, because the M4's GPU speed drifts with
temperature (the full scene measured 59 ms/frame in one session and
71 ms/frame in a later one). Medians from the later session:

| Scene | GPU ms/frame |
| --- | ---: |
| Full | 70.7 |
| Blobs without blur | 4.9 |
| Blobs blurred with σ 24 / 12 / 6 instead of 48 | 30.2 / 16.9 / 11.7 |
| Title without glow | 70.3 |
| Without the 24 rings | within the noise (0.9 less in a single earlier run) |
| Background only | 0.5 |

The six blobs' σ48 blurs take about 66 of the 71 ms, over 90% of the
frame. The blobs are not full-frame: they are circles of 405–630 px, but a
σ48 blur reads 3σ = 144 px on each side of every pixel in both passes, so
each blob's two passes cover about 1.1 million pixels at 289 taps each,
about 2 billion texel reads per frame for the six. The time follows that
tap count: halving σ to 24 cuts the predicted taps to 0.37 and the measured
blur time to 0.38 (0.16 and 0.18 at σ 12).

The rest is small. Screen blending costs nothing measurable. The title's
σ24 glow saves 0.4 ms in the table above, and 1.4 and 2.2 ms in two other
sessions, so it costs at most about 2 ms. Removing the particles, the
spectrum or the HUD makes no measurable difference. React evaluation takes
about 5 ms of CPU per frame, which overlaps the GPU.

Micro-optimizing the blur loop does not change this: computing the
Gaussian weights incrementally instead of with `exp` per tap, and loading
whole-pixel taps directly, measured 54–56 ms against the original's 59 ms
in separate runs, within the drift above. Making blurs of this size cheaper takes fewer taps, for example
pairing taps through bilinear filtering (about half the reads, nearly the
same result) or blurring large σ at reduced resolution (far fewer reads, but
the CPU renderer's exact Gaussian would have to change too, or the GPU
would stop matching it within the effect tests' tolerance of 5). That is
beyond this change; PR #120 has since paired the taps, see
[gaussian-blur-pairing.md](gaussian-blur-pairing.md).

## Reproduce

```sh
cargo run --release --locked -p celesta-gpu-renderer --example path-bench -- 120
cargo run --release --locked -p celesta-gpu-renderer --example dense-geometry-bench
cargo build --release --locked -p celesta-exporter --bin celesta-exporter
/usr/bin/time -l target/release/celesta-exporter --no-ui --overwrite --react examples/versus/bench/celesta/nebula.tsx target/nebula.mp4
```

On macOS with Homebrew's FFmpeg 9 installed as the default, point
`PKG_CONFIG_PATH` at `$(brew --prefix ffmpeg@8)/lib/pkgconfig` first.
