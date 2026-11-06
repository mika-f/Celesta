# Motion blur and trails: what they cost an export

`<MotionBlur>` and `<Trail>` draw their children again at other moments of
the same frame (see
[the handoff note](../handoff/2026-10-14-motion-blur.md)). Nothing outside
them costs more, and `samples={1}` or `shutterAngle={0}` renders a plain
group at no extra cost.

## Where the time goes

For a `<MotionBlur samples={N}>`, every frame:

- **React (Node):** the children render, and are walked into layers, `N`
  times instead of once. Components outside the blur render once.
- **Bridge:** the frame's JSON carries `N` copies of the children's layers,
  which Rust parses.
- **GPU renderer:** every sample draws onto a canvas the size of the region
  the samples cover, and each is added into the group's canvas by one pass
  over that region: `N` draws of the children plus `N` region-sized passes
  and one extra canvas. A large blurred region costs fill rate even when its
  children are simple.
- **Media:** a `<Video>` inside decodes once (every sample asks for the same
  frame under the same layer id) but uploads its texture `N` times.
  Animated images reuse their decoded frames.
- **Encoding** is unchanged.

The export time therefore grows by about `N − 1` times the share of a frame
spent on the blurred children, plus the averaging passes, which scale with
`N` and the blurred region's area. Wrap only what moves fast.

`<Trail copies={K}>` draws the children `K + 1` times, each copy as an
ordinary group: no extra canvases or averaging passes.

On an adapter that cannot blend `Rgba16Float`, canvases are 8-bit and each
weighted sample is rounded before it is added, so the average bands more;
the cost is the same.

## Measuring it

`packages/cli/examples/with-motion-blur.tsx` blurs a moving card and a
spinning wheel. Its `samples`, `copies`, and `frames` project properties
set the sample count, the trail's copies (0 leaves the trail out), and the
video's length; the motion repeats every 60 frames. Export it at several
sample counts, interleaved so drifting machine speed affects every count
alike, and compare each count with `samples=1` from the same run. The
`--json` report's `stats.elapsedSeconds` covers the whole export, start-up
and encoding included, so the ratio understates the cost of rendering
alone. Leave out the first round, which warms caches.

From the repository root:

```sh
pnpm install --frozen-lockfile
pnpm --dir packages/react run codegen
pnpm run build:runtime
node scripts/plugins/build.mts plugins/ffmpeg
export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"
cargo build --release -p celesta-exporter
for round in 0 1 2 3 4 5; do
  for samples in 1 4 8 16; do
    printf '%s,%s,' "$round" "$samples"
    target/release/celesta-exporter --react packages/cli/examples/with-motion-blur.tsx \
      "out-$samples.mp4" --overwrite --json \
      --props "{\"samples\":$samples,\"copies\":0,\"frames\":300}" \
      | node -p 'JSON.parse(require("fs").readFileSync(0, "utf8")).stats.elapsedSeconds'
  done
done
```

Each line is `round,samples,seconds`.
