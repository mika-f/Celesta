# Metal backdrop copy workaround (#130)

## Specification

A blended draw (any blend mode other than `normal`) copies the canvas area
under the draw into the backdrop texture, then `fs_blend` reads it. On an
Apple M5 (macOS 26.3, Metal, wgpu 30.0.1), about 0.26% of these copies left
one 128-pixel block (16×8 or 8×16) of unrelated values. The block was always
in the 16-pixel column just left of the column holding the copy's right edge.
NEBULA's export showed it in 46 of 18,000 frames. Windows showed 0 in 10,000.

Workaround C from the issue: widen the copied columns to multiples of 64
pixels. The root cause (wgpu's Metal backend or Apple's driver) is not known,
and the code says so.

## Implementation

Only `crates/gpu-renderer/src/compositor.rs` changes.

- `BACKDROP_COPY_ALIGN = 64`. Its doc comment records the fault, the issue
  number, and that it is a workaround.
- `aligned_copy_columns(x, width, limit)` rounds the start down and the end up
  to 64-pixel columns, cut at `limit`, the narrower of the canvas and backdrop
  texture widths. The end never drops below `x + width`.
- `Compositor::draw_canvas` uses it for the blended branch's
  `copy_texture_to_texture`. Rows are unchanged.
- `fs_blend` reads the backdrop only under the draw, so the extra columns do
  not change the output. Each copy grows by at most 126 pixels per row.

## Checks

Run from the repository root on an Apple M4 with
`PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`:

```sh
cargo test -p celesta-gpu-renderer compositor
cargo clippy -p celesta-gpu-renderer --all-targets -- -D warnings
```

Both passed. The two new unit tests cover the NEBULA case (frames 146 and 303,
right edge at 1265..1271), already-aligned areas, and the cut at the texture
edge.

NEBULA determinism on the M4, release `celesta-exporter` built with and
without the change:

```sh
celesta-exporter --no-ui --overwrite --react \
  examples/versus/bench/celesta/nebula.tsx --every 1 out/f.png
shasum -a 256 out/f-*.png
```

- Without the change: 5 runs × 600 frames. All 5 runs gave identical hashes.
- With the change: 10 runs × 600 frames. All 10 runs gave identical hashes,
  equal to the runs without the change.
- Run time was about 166–170 s per run either way (the first run of each build
  took about 450 s while caches warmed).
- Frame 146 exported alone with `--frame 146` is byte-identical between the
  two builds.

The M4 did not show the fault at all: at 0.26%, 3,000 frames would show about
8 faults, and 0 has a probability of about 0.04%. So these runs show the
change does not alter the output, but they cannot show that it removes the
fault. The issue's own experiment C (2,000 redraws on the M5) is the only
evidence of that.

## Not verified

- The fault's absence with this exact commit on an M5.
- Windows output after this commit (the issue checked the same change there).
- MP4 `framemd5` comparisons: no media plugins were built here, so only PNG
  output was compared.
- Preview and PNG export paths outside NEBULA, which share the compositor.
