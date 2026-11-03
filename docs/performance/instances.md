# Instances: 100,000 particles per frame

Decision: draw large numbers of particles of one shape as one
`LayerContent::Instances` layer whose columns travel as base64, instead of
one `Rect` layer per particle. See
[the design](../superpowers/specs/2026-10-12-instances-design.md).

## Where 100,000 rects spent their time

Apple M4, macOS 26.6, Node v26.10.0, release build of `71b9511`, every rect
moving every frame:

| Stage | 3,000 rects | 100,000 rects |
| --- | ---: | ---: |
| React frame round trip (`protocol-bench`: render, walk, stringify, pipe, serde) | 3.9 ms | 158 ms |
| of which `JSON.stringify` | — | 24 ms (26 MB per frame, ~260 B per rect) |
| GPU renderer (`dense-geometry-bench`, scene building excluded) | ~1.2 ms | ~30 ms |

Every stage paid per element. The GPU renderer's time was almost all
CPU-side `prepare_draws` (a `sample` profile of the main thread), not GPU
work.

## Results

Same machine, with the `Instances` layer:

| Stage | 100,000 `Rect` layers | 100,000 particles in one `Instances` | Target |
| --- | ---: | ---: | ---: |
| React frame round trip, median | 157–163 ms | **3.9–4.2 ms** | ≤ 20 ms |
| GPU renderer, scene building excluded | 22.8 ms | **5.7 ms** | ≤ 10 ms |

- The React entry is
  [`instances-100k.tsx`](../../crates/react-bridge/examples/instances-100k.tsx):
  100,000 particles with `x`, `y`, `width`, `height`, `rotation`, and `color`
  columns, every value changing every frame. Its frame is 4.8 MB of base64
  instead of 26 MB of JSON. The 4 ms include the entry's own loop that
  fills the columns.
- The GPU figure is `dense-geometry-bench`'s `instances` geometry: the same
  100,133 ribbon segments as its `rects` geometry, as one `Instances` layer
  per ribbon, minus the time spent building scenes (14.2 ms total, 8.6 ms
  building).
- The `Rect` layers also got faster, from ~30 ms to 22.8 ms: writing each
  `LayerInstance` with one `bytemuck` cast, which `Instances` needed,
  replaced a per-float `to_ne_bytes` iterator for every layer.
- Exporting two seconds (120 frames) of `instances-100k.tsx` to MP4 took
  4.5 s including start-up, about 27 frames per second; encoding a frame of
  100,000 moving dots (43 MB for two seconds) is now the slowest step.

## Flat base64 strings

`JSON.stringify` serializes a frame on V8's fast path only while every
string in it is flat (see
[react-scene-serialization.md](react-scene-serialization.md)). Node's
`Buffer#toString('base64')` returns a flat string up to about 1 MiB of
output and an external string beyond that; one external string sends the
whole frame down the slow path. A 300,000-particle column is 1.6 MB of
base64.

`@celesta.studio/react` therefore encodes a column in pieces of 384 KiB of data
(512 KiB of text, whole 3-byte groups so the pieces concatenate into the
base64 of the whole) and joins them, which copies them into one flat
string. `packages/react/test/flat-strings.test.mts` renders a
300,000-particle `Instances` and finds no non-flat string.

Measured in isolation, stringifying an object holding one external
1.6 MB base64 string took 0.19 ms against 0.13 ms for a flat copy.

## Reproduce

```sh
pnpm install --frozen-lockfile
pnpm --dir packages/react run codegen
pnpm run build:runtime
cargo run --release -p celesta-react-bridge --example protocol-bench -- \
  crates/react-bridge/examples/instances-100k.tsx 120
cargo run --release -p celesta-gpu-renderer --example dense-geometry-bench -- 120 3 298 3
```

On macOS, build against FFmpeg 8 with
`PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`. For the
100,000-`Rect` baseline, write an entry that renders 100,000 `<Rect>`s with
the same positions and pass it to `protocol-bench`.
