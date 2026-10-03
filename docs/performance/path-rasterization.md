# Path rasterization: NEBULA / PR #113

The shared CPU path rasterizer now reuses Rayon's worker pool for coverage
and row compositing. Each row band converts its own premultiplied float
scratch buffer directly into the final straight-alpha RGBA image. This removes
per-frame thread creation and moves scratch allocation, zeroing, and the
previously serial RGBA conversion onto workers. Small images composite and
convert serially.
The existing coverage rasterizer, painter's order, gradients, fill/stroke
overlap, opacity, and output-resolution antialiasing are preserved.

## Measurements

Measured on October 3, 2026: Core i7-13700F (24 logical processors), RTX 4070
(Vulkan), Windows 11, Rust 1.97.1, release builds. Before/after order alternates,
with three observations per version. Raw observations are in
[path-rasterization.csv](path-rasterization.csv). The following are medians:

| Workload | Before | After | Time reduction |
| --- | ---: | ---: | ---: |
| 24 rings: CPU rasterization | 12.89 ms/frame | 6.73 ms/frame | 47.8% |
| 24 rings: GPU submit/drain, including readback | 17.88 ms/frame | 11.55 ms/frame | 35.4% |
| Full NEBULA: export 600 frames | 25.85 s | 19.10 s | 26.1% |

The isolated benchmark uses the ring geometry, transforms, stroke, and opacity
from [PR #113](https://github.com/mika-f/Celesta/pull/113), at 1920x1080 and
60 fps. It prebuilds scenes and warms each stage for 10 frames, then measures
120 frames. Raster hashing happens outside its timer. GPU timing includes
submission and draining the export readback pipeline, without encoding.

Full-scene measurements use the PR's original release exporter as the baseline
and this worktree's release exporter afterward, both with the same PR entry
and fonts. The PR's renderer sources and Cargo lockfile match the base commit
`4519e52`. Timing is CLI launch to exit, including React evaluation, all effects,
text, GPU transfer, and libx264 `medium` / CRF 18 encoding. No render-quality
setting changes. These numbers apply to this workload and machine; full-export
observations ranged from 25.01–26.54 s before and 18.55–22.30 s afterward.

All isolated raster checksums match. All six full exports have the same MP4
SHA-256, `FE712E1C3313EA97B394586677C7E477BE25A55A769ADBBF4DF28D48D60E3EC9`.
FFprobe confirms H.264, 1920x1080, and 600 decoded frames.

After synchronizing the worktree with main at `86aa526`, the official
`examples/versus/bench/run.mjs --only celesta --runs 3` measured 18.377,
19.041, and 21.306 s (median 19.041 s). The edit-loop script measured 1.518,
1.399, and 1.420 s (median 1.420 s), restoring the source byte-for-byte.
The new NEBULA export has the same SHA-256 as above. The comparison film and
its README now use these latest values, with the earlier Remotion/fframes
measurements retained in their respective records.

## Reproduce

From the repository root, using Celesta's normal FFmpeg development setup:

```sh
cargo run --release --locked -p celesta-gpu-renderer --example path-bench -- 120
```

Copy this example to the base revision and run it there for comparison. It prints the raster
checksum and separate CPU and GPU timings. To repeat the full-scene export,
use `examples/versus/bench/celesta/nebula.tsx` and its assets (now included on
main through PR #113), build the React package, then:

```sh
cargo build --release --locked -p celesta-exporter --bin celesta-exporter
target/release/celesta-exporter --react <path-to-nebula.tsx> target/nebula.mp4 --preset medium --crf 18 --overwrite --no-ui
```

## Validation and remaining cost

The renderer and GPU renderer pass 112 tests, including a new comparison of
one-worker and four-worker output for a single large path and overlapping
gradient-filled translucent paths, plus an empty-batch regression check.
Tests use `--test-threads=1` to avoid concurrent GPU devices on this machine.
Changed Rust files pass rustfmt; `git diff --check` passes. The CPU renderer
passes strict Clippy on all targets/features. Strict GPU Clippy encounters
two pre-existing warnings in untouched code (`while_immutable_condition` and
`clone_on_copy`); with only those two lints allowed, all targets/features pass.

Path coverage still runs on the CPU and each batch still uploads an RGBA
texture. Moving coverage and painting to the GPU is the next larger change;
it would need to preserve translucent stroke intersections, fill/stroke
composition, gradients, clipping, and antialiasing. This change improves the
existing CPU/GPU path pipeline without changing those semantics.
