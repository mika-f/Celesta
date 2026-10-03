# VERSUS: Celesta, Remotion and fframes

One heavy motion graphic, **NEBULA**, built three times with the same math and
fonts, then measured. [`film.tsx`](film.tsx) is an 82-second, 1920×1080,
30 fps video that plays the three exports and shows the results.

| | Export 600 frames to MP4 | Edit → frame 300 as PNG |
| --- | ---: | ---: |
| Remotion 4.0.532 | 46.0 s | 3.5 s |
| fframes 1.1.0 (Skia on Vulkan) | 13.7 s | 13.9 s |
| Celesta 0.2.0 | 25.9 s | 1.8 s |

Medians of three runs on a Core i7-13700F, RTX 4070 and 128 GB of RAM, on
Windows 11. Each export is a cold CLI run timed from start to exit, encoded
with libx264 `medium` at CRF 18. Remotion with `--gl=angle --concurrency=100%`
took 40–61 s, which was no faster than its defaults. The edit loop changes one
color in the source, then renders frame 300 from the command line. fframes
spends that time in an incremental `cargo build --release`.

fframes renders fastest. Celesta exports about 1.8× faster than Remotion. In
this scene, most of Celesta's frame time goes to the 24 rotating ellipses,
because `Path` layers are rasterized on the CPU every frame. Without them,
Celesta renders at 56 fps instead of 28 fps.

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

Then open `film.tsx` in Celesta, or export it:

```sh
cargo run -p celesta-exporter --release -- --react examples/versus/film.tsx versus.mp4
```

Fonts (Space Grotesk, JetBrains Mono) load from Google Fonts on the first run
and are cached after that.
