# VERSUS: Celesta, Remotion and fframes

One heavy motion graphic, **NEBULA**, built three times with the same math and
fonts, then measured. [`film.tsx`](film.tsx) is an 82-second, 1920×1080,
30 fps video that plays the three exports and shows the results.

| | Export 600 frames to MP4 | Edit → frame 300 as PNG |
| --- | ---: | ---: |
| Remotion 4.0.532 | 35.4 s | 3.0 s |
| fframes 1.1.0 (Skia on Vulkan) | 9.9 s | 9.8 s |
| Celesta 0.3.0 (source, `9daa93b`) | 8.8 s | 1.4 s |

Medians of 15 interleaved rounds (3 runs for the edit loop) on a Core
i7-13700F, RTX 4070 and 128 GB of RAM, on Windows 11. All tools were measured
on October 4, 2026 (JST), in one session with Celesta built from `9daa93b`
plus the uncommitted benchmark-harness changes, so it includes the Path
coverage fallback fixes merged in `a500a33`. Each round ran Celesta, Remotion,
tuned Remotion and fframes once, rotating the order, after a warm-up round
whose times are kept in `firstRuns` and excluded from the medians. The record
in `results.json` stores the revision, a dirty flag and the executables'
hashes. Celesta links FFmpeg 8.1.x with x264 statically through vcpkg; fframes
used the shared BtbN FFmpeg n9.0 build, so part of the export gap may come from
the x264 builds rather than the renderers. Each export is a cold CLI run timed
from start to exit, encoded with libx264 `medium` at CRF 18. The edit loop
changes one color in the source, then renders frame 300 from the command line.
fframes spends that time in an incremental `cargo build --release`; its first
run (39.1 s) also recompiled after the FFmpeg environment changed, and the
median is unaffected.

Celesta exports 4.0× faster than default Remotion and 3.7× faster than Remotion
with `--gl=angle --concurrency=100%` (32.8 s, a 7% gain over its defaults in
this session). Celesta's 15 exports ranged 8.63–9.26 s and fframes' 9.66–10.33 s,
so Celesta's median is about 11% lower, a gap larger than the 5% the film
treats as a tie. An independent interleaved run on another machine (i7-12700K,
RTX A4000) ranked fframes 7.3% ahead and found tuned Remotion about 50% faster
than default, so both the order of the two fastest tools and the size of the
tuning effect depend on the machine. That run also saw the first launch of a
newly built Celesta executable cost about 5–6 s; here the warm-up run was 0.9 s
slower than the median (9.70 s against 8.77 s).
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
latest medians, and names a winner only when the gap exceeds 5%; closer
medians are reported as a tie. The speed ratio is against default Remotion.
`run.mjs` interleaves the configurations (Celesta, Remotion, tuned Remotion and
fframes) over 15 rounds, rotating the order each round. A warm-up round runs
first and is stored as `firstRuns`, outside the medians, because a newly built
executable pays a one-off launch cost. Each record also stores the git
revision, a dirty flag and the executables' hashes; pass `--build-notes` to
note the FFmpeg/x264 builds. Each tool writes `<config>.mp4` in `bench/out`, so
tuned Remotion does not overwrite the default export the film plays.

Then open `film.tsx` in Celesta, or export it:

```sh
cargo run -p celesta-exporter --release -- --no-ui --react examples/versus/film.tsx examples/versus/versus.mp4 --overwrite
```

Fonts (Space Grotesk, JetBrains Mono) load from Google Fonts on the first run
and are cached after that.
