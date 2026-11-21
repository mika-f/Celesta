# SUPERNOVA — an expedition into light

A silent, 36-second, 1920×1080 / 30 fps GPU performance film. A departure
through a star tunnel becomes an orbital sculpture, liquid glass, a folding
event horizon, and a typographic afterimage. Cyan, violet and warm pink tie
the chapters together. All motion is frame-pure, including the star positions;
frames can be requested in any order.

No downloads, remote fonts, media decoders or content plugins are needed.
The only asset is the repository's bundled Bebas Neue font, shared with
NEBULA. Keep `examples/versus/bench/assets/fonts/BebasNeue-Regular.ttf` beside
the example when moving it outside this repository. There is no audio track.

| Frames | Chapter | Rendering work |
| --- | --- | --- |
| 0–215 | Departure | 12,000 instanced stars, additive composition, radial streaks and zoom blur; opening title |
| 216–431 | Orbital mechanics | 240 large thin gradient-stroked ellipses plus 48 cubic Bézier ribbons, continuously transformed; #869 tile coverage |
| 432–647 | Liquid light | 32 large blur-only bokeh layers and 18 color-filter-only glass groups with screen/add blends; #870 lone-effect results |
| 648–863 | Event horizon | Twelve overlapping sectors, nested luminance/alpha masks, perspective X/Y rotations and a custom WGSL gravitational lens |
| 864–1079 | After the light | Nine gradient/stroked text layers behind a rich-text title with chromatic aberration, separate glow/shadow |

The 12,000 stars, all 288 paths, all bokeh and glass remain moving and
rendered throughout; chapters ease their prominence rather than stop the
underlying workload. Heavy rendering includes the title and outro. Effects
intended to exercise #870 do not have shadows or glows. This is a GPU-path
workload, not a CPU-export benchmark or a video-decoding benchmark.

## Run from the repository root

After `pnpm install --frozen-lockfile` and `pnpm run build:runtime`:

```sh
cargo run --release -p celesta-editor -- examples/supernova/film.tsx
node scripts/typecheck-examples.mjs supernova

# Renderer only: pre-evaluated frames, Final quality, pipelined readback.
cargo run --release -p celesta-bench -- run --frames 30 --size 1920x1080 supernova

# Built-in PNG writer: no media plugin needed.
cargo run --release -p celesta-exporter -- --no-ui --react --overwrite \
  examples/supernova/film.tsx --frames 90,300,510,730,960,1070 supernova.png

# Video export needs an installed H.264 encoder/muxer plugin.
cargo run --release -p celesta-exporter -- --no-ui --react --overwrite \
  examples/supernova/film.tsx supernova.mp4
```

On a headless Linux machine prefix rendering commands with
`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json XDG_RUNTIME_DIR=/tmp`.
This selects software Vulkan: its timings are not hardware GPU FPS.

The named workload samples five consecutive runs starting at frames 90,
300, 510, 730 and 960. Warmup precedes the first run; use at least five
measured frames. Compare at the same resolution, warmup and frame count.
`celesta-bench` prints JSON `ms_per_frame`; divide 1000 by it for FPS.
See [benchmark methodology](../../docs/performance/benchmarking.md).
