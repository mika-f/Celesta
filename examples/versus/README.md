# VERSUS: Celesta, Remotion and fframes

One heavy motion graphic, **NEBULA**, built three times with the same math and
fonts, then measured. [`film.tsx`](film.tsx) is an 82-second, 1920×1080,
30 fps video that plays the three exports and shows the results.

| | Export 600 frames to MP4 | Edit → frame 300 as PNG |
| --- | ---: | ---: |
| Remotion 4.0.532 | 43.1 s | 3.1 s |
| fframes 1.1.0 (Skia on Vulkan) | 10.6 s | 11.0 s |
| Celesta 0.3.0 (source, `d7d3c8a`) | 10.6 s | 1.4 s |

Medians of three runs on a Core i7-13700F, RTX 4070 and 128 GB of RAM, on
Windows 11. All three tools were remeasured on October 4, 2026 (JST), in one
session with Celesta built from `d7d3c8a`, including compact scene serialization,
parallel text rasterization, and the one-frame-ahead React evaluation pipeline.
This batch predates the Path coverage fallback fixes merged in `a500a33`.
The export
records preserve each batch's timestamp, and `loop.json` records measurement
times per tool. Each export is a cold CLI run timed from start to exit, encoded
with libx264 `medium` at CRF 18. Remotion with `--gl=angle --concurrency=100%`
took 38.2–39.1 s (median 38.7 s), about 10% less time than its default-settings
median in this session. The edit loop changes one
color in the source, then renders frame 300 from the command line. fframes
spends that time in an incremental `cargo build --release`.

Celesta exports about 4.1× faster than Remotion. Celesta and fframes both round
to 10.6 s: their medians differ by only 0.020 s, much less than the run-to-run
spread. The three Celesta exports took 12.188, 10.565 and 9.496 s; fframes took
10.585, 10.813 and 10.494 s. Celesta's median is about 40% lower than the
previous GPU Path coverage batch (17.607 s), though these are separate sessions
rather than an isolated before/after test.
That previous batch was recorded in `7b4ef96` and predates the final shader
change in `1d62203`; its last shader change was `7441d2b`, based on the commit
history (the binary's exact build revision was not recorded).
Earlier isolated results are documented in the
[Path measurements](../../docs/performance/path-rasterization.md) and
[GPU Path coverage measurements](../../docs/performance/gpu-path-coverage.md).
The film's first-time setup durations remain the original one-off measurements;
its source line counts were checked against the current implementations.

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
node examples/versus/bench/run.mjs --only remotion --remotion-flags "--gl=angle --concurrency=100%" --out examples/versus/bench/out/tuned
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
The speed verdict also derives its fastest tool and tuning comparison from the
latest medians. Tuned Remotion exports use a separate output directory so the
film continues to play the default-settings export measured in the table.

Then open `film.tsx` in Celesta, or export it:

```sh
cargo run -p celesta-exporter --release -- --no-ui --react examples/versus/film.tsx examples/versus/versus.mp4 --overwrite
```

Fonts (Space Grotesk, JetBrains Mono) load from Google Fonts on the first run
and are cached after that.
