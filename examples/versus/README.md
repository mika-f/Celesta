# VERSUS: Celesta, Remotion and fframes

One heavy motion graphic, **NEBULA**, built three times with the same math and
fonts, then measured. [`film.tsx`](film.tsx) is an 82-second, 1920×1080,
30 fps video that plays the three exports and shows the results.

| | Export 600 frames to MP4 | Edit → frame 300 as PNG |
| --- | ---: | ---: |
| Remotion 4.0.532 | 46.1 s | 3.4 s |
| fframes 1.1.0 (Skia on Vulkan) | 11.4 s | 11.4 s |
| Celesta (batch `7b4ef96`) | 17.6 s | 1.5 s |

Medians of three runs on a Core i7-13700F, RTX 4070 and 128 GB of RAM, on
Windows 11. All three tools were remeasured in one session after the GPU Path
coverage change. The batch was recorded in `7b4ef96` and predates the final
shader change in `1d62203`; its last shader change was `7441d2b`, based on
the commit history (the binary's exact build revision was not recorded). These historical
numbers do not measure that final shader or the later fallback fixes. The export
records preserve each batch's timestamp, and `loop.json` records measurement
times per tool. Each export is a cold CLI run timed from start to exit, encoded
with libx264 `medium` at CRF 18. Remotion with `--gl=angle --concurrency=100%`
took 40–61 s, which was no faster than its defaults. The edit loop changes one
color in the source, then renders frame 300 from the command line. fframes
spends that time in an incremental `cargo build --release`.

fframes renders fastest. Celesta exports about 2.6× faster than Remotion.
Reusing Path worker threads and parallelizing the final RGBA conversion reduced
Celesta's export from 25.9 s to 19.0 s, about 26%, with identical MP4 output.
See the [Path measurements](../../docs/performance/path-rasterization.md) for
that change's isolated benchmark and pixel-equivalence checks. Since then the
GPU renderer shades Path coverage itself instead of uploading CPU-rasterized
paths ([GPU Path coverage](../../docs/performance/gpu-path-coverage.md), issue
#116); the table above includes that change. The three Celesta exports took
19.5, 17.6 and 15.7 s, so run-to-run spread is wider than the difference from
the earlier 19.0 s.

## Layout

- [`bench/SCENE.md`](bench/SCENE.md): the scene spec that all three
  implement.
- [`bench/celesta/nebula.tsx`](bench/celesta/nebula.tsx),
  [`bench/remotion/`](bench/remotion/src/Nebula.tsx) and
  [`bench/fframes/`](bench/fframes/src/lib.rs): the three implementations.
  Fonts are shared from `bench/assets/fonts` (SIL OFL; see `bench/assets/licenses`).
- [`bench/run.mjs`](bench/run.mjs) times the full exports and appends to
  `bench/results.json`. [`bench/loop.mjs`](bench/loop.mjs) times the edit loop
  and writes `bench/loop.json`. The film reads its numbers from both files.
- `film.tsx`, `timeline.ts`, `scenes/` and `components/`: the comparison
  video.

## Reproduce

Build Celesta's exporter and React runtime as described in the root
[README](../../README.md). Then set up the other two implementations:

```sh
# Remotion
cd examples/versus/bench/remotion && npm install && cd -
# fframes
cd examples/versus/bench/fframes && cargo build --release && cd -
```

On Windows, fframes links a prebuilt FFmpeg 9.0 shared build (for example
`ffmpeg-n9.0-latest-win64-gpl-shared-9.0.zip` from
[BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds/releases/tag/latest))
and needs LLVM for bindgen. Before building and running it, set:

```powershell
$env:FFMPEG_DIR = "C:\path\to\ffmpeg-n9.0-latest-win64-gpl-shared-9.0"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:PATH = "$env:FFMPEG_DIR\bin;$env:PATH"   # the DLLs are loaded at build and run time
```

macOS and Linux need the system libraries listed in the
[fframes README](https://github.com/dmtrKovalenko/fframes#installation).

Run the benchmarks from the repository root. They write the three MP4s to
`bench/out/`, which the film plays:

```sh
node examples/versus/bench/run.mjs
node examples/versus/bench/run.mjs --only remotion --remotion-flags "--gl=angle --concurrency=100%"
node examples/versus/bench/loop.mjs
```

To refresh Celesta after renderer changes while retaining the other tools'
results, rebuild the release exporter, then run:

```sh
node examples/versus/bench/run.mjs --only celesta --runs 3
node examples/versus/bench/loop.mjs --only celesta --runs 3
```

The film's race, charts, speed ratio, edit-loop bars, and closing export time
all read these files, so re-export the film after updating the measurements.

Then open `film.tsx` in Celesta, or export it:

```sh
cargo run -p celesta-exporter --release -- --react examples/versus/film.tsx examples/versus/versus.mp4 --overwrite
```

Fonts (Space Grotesk, JetBrains Mono) load from Google Fonts on the first run
and are cached after that.
