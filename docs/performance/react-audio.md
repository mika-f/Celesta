# React preview audio: issue #97

Decision: use one Node request to evaluate every frame, collect audio without
building visual layers, and default release bridges to production React.
Keep text measurements, font discovery, React hooks, and every frame evaluation.
Do not add a graph disk cache, static timeline analysis, sampling, or progressive
mixing in this change.

## Measurements

Measured on Apple M4, Node 24.21.0, Rust 1.95.0, with a release Rust executable
and FFmpeg 8. The workload is `examples/reel/film.tsx`: 960 frames at 30 fps,
with its generated `music.wav`. Each observation starts a new Node process;
per-frame and batch order alternates. Both paths must produce exactly equal
`AudioGraph`s on every run. Raw observations are in [react-audio.csv](react-audio.csv).

Times below are medians in milliseconds. Stage medians need not sum to the
median total. The first two experiments have five runs per mode; the final
implementation has seven.

| Method | Node / prepare | Scan + graph | Decode + mix | Total |
| --- | ---: | ---: | ---: | ---: |
| Original per-frame protocol, development React | 78 | 1,248 | 110 | 1,434 |
| One request, still building all layers, development React | 80 | 661 | 114 | 854 |
| One request, omit visual layers, development React | 75 | 574 | 109 | 757 |
| Per-frame protocol, production React | 67 | 783 | 108 | 960 |
| **Adopted: one request, omit visual layers, production React** | **68** | **151** | **108** | **327** |

The adopted total ranges from 323–330 ms. Compared with the original mode,
scan time falls about 88% (8.3× faster), and total time about 77% (4.4× faster).
With production React held constant, batching plus omitting visual layers makes
the scan 5.2× faster. The final audio walker also avoids layer-id strings and
flattening empty layer arrays; that reduced the production scan median from
154 ms to 151 ms in an additional seven-run comparison.

**The 300 ms goal is not strictly met:** this benchmark is 27 ms above it.
It measures Node startup through completed PCM mixing, not GUI `Open…` latency;
file selection, editor scheduling, waveform/meter construction and audio-device
setup are outside the timer. FFmpeg decoding is performed on each run, without
the editor's decoded-audio cache. Remote fonts use the existing asset cache;
the first original observation took 394 ms in startup alone. A first network
download or expensive user `prepare()` can exceed these results substantially.

### Long composition

`crates/react-bridge/examples/audio-long.tsx` has 18,000 frames at 60 fps, 64
moving rectangles, 32 seconds of music followed by silence, and a muted clip
appearing only on the very last frame. Three runs per mode, production React:

| Method | Node / prepare | Scan + graph | Decode + mix | Total |
| --- | ---: | ---: | ---: | ---: |
| Per-frame | 52 | 2,331 | 127 | 2,511 |
| Adopted | 49 | 357 | 116 | 523 |

This tests long frame counts and the final-frame boundary; it is **not** a
five-minute version of the reel's visual workload or five minutes of continuous
audio. React evaluation still scales with frame count and component cost.

## Why these choices

1. **Batch requests: adopt.** Removes per-frame IPC and scene serialization.
   The existing Rust request loop continues answering synchronous `measureText`
   requests while Node scans. Rendering errors still propagate through the same
   protocol.
2. **Audio-only output: adopt partially.** Skip visual-layer construction and
   character-view override collection. Do not skip component evaluation,
   layout hooks or text measurement: an audio conditional can depend on those
   results. Font changes retain the normal second reconciliation pass.
3. **Production React: adopt for release builds.** React's development
   validation dominated much of the remaining cost. An explicit `NODE_ENV`
   is respected; debug Rust builds retain the inherited environment. This
   applies consistently to preview, audio collection, and export bridges.
   User code that branches on `NODE_ENV` consequently sees `production` in a
   release build unless overridden.
4. **Graph disk cache: defer.** Not benchmarked or implemented. It would help
   repeat opens, but not first opens or changed-source reloads. A bundle and
   asset hash alone cannot safely cover arbitrary `prepare()` reads, network
   responses, environment variables, or files read by user code. Add only with
   an explicit dependency/invalidation contract and evidence that repeat-open
   graph collection is still the bottleneck.
5. **Static time-structure analysis: defer.** Arbitrary React conditionals and
   hooks require full evaluation to preserve results. A safe fast path needs
   an author-facing declarative audio contract or a proven fallback mechanism.
6. **Coarse sampling: reject for the default path.** Even a one-frame clip must
   be found. **Progressive mixing: defer.** It adds partial-graph lifecycle and
   mixing complexity; it does not reduce total collection work.

The next optimization, if a strict sub-300 ms cold-path target is required,
should profile the remaining ~108 ms mixer and actual editor scheduling. Do not
trade away audio correctness to save the last few tens of milliseconds.

## Correctness

The Rust integration test compares complete graphs against the original
per-frame loop using independent Node processes, including repeated scans and
normal frame rendering after a scan. Fixtures cover no audio, bare audio,
conditional audio, one-frame audio, duplicates, remote and relative paths,
negative/nested sequences, overlapping Series, Dialogue, keyframed volume,
muting, playback rates, retained hook state, and measurement-dependent audio.

JavaScript tests additionally compare every unmerged report and every text
measurement request, including changing font declarations. Visual conversion
is intentionally skipped, so invalid visual-only properties are still diagnosed
by normal preview/export rendering rather than by audio preparation. Component
evaluation failures continue to fail audio collection.

As before, compositions should render deterministically from frame time;
batching does not promise the same wall-clock timing of arbitrary external
asynchronous side effects between frames.

## Reproduce

From the repository root (on macOS, select an installed compatible FFmpeg):

```sh
pnpm install --frozen-lockfile
cargo test -p celesta-project -p celesta-composition --features codegen
pnpm --filter @celesta/react... build
python3 examples/reel/make-music.py
PKG_CONFIG_PATH=/opt/homebrew/opt/ffmpeg@8/lib/pkgconfig \
  cargo build --release -p celesta-react-bridge --example audio-bench
target/release/examples/audio-bench examples/reel/film.tsx 7
target/release/examples/audio-bench crates/react-bridge/examples/audio-long.tsx 3
NODE_ENV=development target/release/examples/audio-bench examples/reel/film.tsx 5
```

The executable prints CSV and asserts graph equality. The per-frame mode is the
original collection algorithm; both modes use the same selected React runtime.
For the batch-only ablation, replace the `renderFrame(time, null, true)` call in
`collectAudio()` with `renderFrame(time, null)` and rebuild the React package;
restore it afterward. No new dependencies are required.
