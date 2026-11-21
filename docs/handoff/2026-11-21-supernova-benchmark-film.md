# SUPERNOVA GPU benchmark film

## Specification

Create a reproducible, visually interesting example substantially heavier than
SPECTRA and the DELTA release film. Focus on GPU rendering, not CPU export or
video decoding. Use generated visuals, bundled fonts, and no network assets.

## Implementation

`examples/supernova/film.tsx`: 1920×1080, 30 fps, 1080 frames (36 seconds),
five 216-frame chapters, with title and outro integrated into the first/last.
12,000 instanced stars, 240 gradient rings, 48 cubic curves, 32 blur-only
bokeh lights and 18 filtered glass groups remain animated throughout. Chapter
prominence eases across boundaries. The fourth chapter adds twelve nested
alpha/luminance masked sectors, perspective and a custom WGSL lens; the last
adds gradient/stroked typography, rich text, chromatic aberration and separate
glow/shadow. The bundled NEBULA Bebas Neue font is the only file asset.

`crates/bench/src/workloads.rs` registers `supernova` using the existing React
source path, sampling runs from frames 90, 300, 510, 730 and 960. No renderer
behavior changed. `scripts/bench.py` builds the runtime when explicitly
selecting this React workload (and the existing SPECTRA workload).

Local README documents the narrative, costs, asset dependency and commands;
the benchmark guide lists the workload. Root README does not list SPECTRA,
so its selected showcase was left unchanged.

## Checks and measurements

- Installed Node 26.13.0 and pnpm 12.17.0 under `/home/ubuntu`, then ran
  `pnpm install --frozen-lockfile` and `pnpm run build:runtime`:
  `Tasks: 18 successful, 18 total`. Codegen emits existing ts-rs serde-attribute
  warnings, but completed successfully; no generated files are committed.
- `node scripts/typecheck-examples.mjs supernova`: `examples/supernova`, exit 0.
- `cargo fmt --all --check`: exit 0.
- `cargo test --release -p celesta-bench`: 2 passed, 0 failed.
- `cargo clippy -p celesta-bench --all-targets --locked -- -D warnings`: exit 0.
- Benchmark rejects four measured frames as expected:
  `celesta-bench: supernova: measures 5 runs of frames, so it needs --frames 5 or more`.
- Final benchmark (30 measured frames, default 10 warmup, 1920×1080,
  llvmpipe LLVM 15.0.7 Vulkan, no concurrent build/render processes):

  | Workload | ms/frame | Approximate FPS |
  | --- | ---: | ---: |
  | supernova | 1015.5002 | 0.98 |
  | spectra | 298.3564 | 3.35 |

  SUPERNOVA is 3.40× the rendering time of SPECTRA on this software Vulkan
  machine; this is not a real-GPU performance claim. Command:
  `CELESTA_PLUGIN_PATH=/home/ubuntu/supernova-plugins VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json XDG_RUNTIME_DIR=/tmp target/release/celesta-bench run --frames 30 --size 1920x1080 supernova spectra`.
  Built SPECTRA's code plugin locally with
  `node scripts/plugins/build.mts plugins/code --out /home/ubuntu/supernova-plugins`.
  Raw results: `/home/ubuntu/supernova-bench.jsonl`. No DELTA-film named
  workload exists, so no renderer-only DELTA timing was taken.
- Rendered and personally inspected six full-resolution PNGs at frames
  90, 300, 510, 730, 960 and 1070. Improved color saturation and centered the
  aperture after the first inspection, then re-rendered and inspected final
  images in `/home/ubuntu/supernova-shots/supernova-<six-digit-frame>.png`.
  Export output: `export complete: /home/ubuntu/supernova-shots/supernova.png`.
- MP4 export attempted but unavailable: `Celesta export: no media plugin offers a muxer`.
  Skipped building FFmpeg 8/plugins solely for a preview; still rendering and
  the benchmark need no media plugin. Continuous playback and hardware GPU
  performance remain unverified.
