# React scene serialization: PR #188

Decision: keep every string in the per-frame scene flat, so V8's
`JSON.stringify` fast path serializes the frame instead of bailing out to
the slow path. Layer ids the walk builds use `[a, b].join('.')`;
author-supplied strings that reach a layer go through `flatString()` in
`packages/react/src/render.ts`. The wire format does not change.

## Cause

The React bridge (`packages/react/src/cli.ts`) sends each frame's
`{ scene, audio }` to Rust as one line of `JSON.stringify` output. In
Node 26, V8's JSON.stringify has a fast path that handles only flat
strings. At the first ConsString or
SlicedString it meets, it bails out and serializes the whole value again on
the slow path. A frame with one such string costs the fast-path work up to
that point plus a full slow-path pass.

V8 makes a ConsString when a concatenation (`+` or a template literal)
produces 13 or more characters, and a SlicedString when slicing does; a
shorter result is copied flat. `charCodeAt` and similar operations flatten
a ConsString in place but leave its ConsString map, so the fast path still
bails out. Where the strings came from:

- **NEBULA** (`examples/versus/bench/celesta/nebula.tsx`, ~1,660 layers):
  only the HUD text `` `FRAME ${n} / 600` ``. Every id (`root.3.17`) is
  shorter than 13 characters and every other string is an internalized
  literal.
- **SPECTRA** (`examples/spectra/film.tsx`): the nested ids built by
  `` `${parentPath}.${index}` `` (`root.2.0.0.0.0`; 735 in one frame) and
  a few texts.

## Measurements

Each frame's stringify (live) is compared with stringifying a `JSON.parse`'d
copy of the same frame (copy), measured in the same process right after.
The copy has identical data and only flat strings. Times are medians after
10 warmup frames. Absolute times varied a lot between runs on the M5 (likely
thermal), so the within-run ratio is the number to compare.

Apple M5, Node v26.10.0:

| Run | live ÷ copy before | live ÷ copy after |
| --- | ---: | ---: |
| NEBULA, first 2 s | 2.77–2.82 (1.04–1.09 ms) | 1.01–1.08 (0.39–0.42 ms) |
| NEBULA, first 10 s | 3.02–3.09 | 1.07–1.19 |
| SPECTRA, whole reel | 2.30 (0.348 ms) | 1.15–1.20 (0.165–0.184 ms) |
| SPECTRA, first 10 s | 1.94–2.22 | 1.11–1.15 |

Intel i7-12700K, NVIDIA RTX A4000 (Vulkan), Windows 11, Node v26.10.0. One
sitting, three runs per workload and revision:

| Run | live ÷ copy before | live ÷ copy after |
| --- | ---: | ---: |
| NEBULA, first 2 s | 3.04–3.10 (2.18–2.30 ms) | 1.04–1.16 (0.80–0.83 ms) |
| SPECTRA, whole reel | 2.65–2.77 (0.45–0.47 ms) | 1.15–1.17 (0.18–0.19 ms) |

Checks that pin the cause:

- On the same live NEBULA frame, replacing its one cons `text` with a flat
  copy took stringify from 1.03–1.10 ms to 0.41–0.52 ms, the copy's speed.
- In a 1,600-layer parsed object, setting one field to a fresh
  `` `FRAME ${i} / 600` `` on each call took stringify from 0.234 ms to
  0.669 ms. A SlicedString has the same effect.
- With `--no-json-stringify-fast-path` on the base revision, NEBULA's live
  and copy took the same time: 0.69/0.67 ms and 0.76/0.75 ms on the M5,
  1.331/1.272 ms on Windows. On SPECTRA (Windows) the copy stayed faster,
  0.442/0.335 ms. A likely but unverified reason is that the slow path
  flattens each ConsString before writing it out, which costs SPECTRA's
  hundreds of cons ids and NEBULA's one almost nothing.

With the fast path off, NEBULA's live stringify was faster than with it on
(1.33 ms vs 2.18–2.30 ms on Windows). That shows what causes the gap, but
turning the fast path off is not a fix. How much a bailout costs depends on
where the first non-flat string is. NEBULA's sits near the end of the layer
list, so almost all of the fast-path work is wasted. A string near the
start would cost about as much as the slow path alone, and author code can
put one anywhere. For a frame of flat strings the fast path is about twice
as fast as the slow path (0.37–0.39 ms vs 0.67–0.75 ms on NEBULA's copy, M5).
Keeping strings flat helps on both paths.

`renderAt` (React reconcile plus the scene walk) did not get slower: NEBULA
took 4.5–5.2 ms after vs 4.6–4.9 ms before, and SPECTRA 2.2–2.4 ms vs
2.8 ms (M5, same runs).

## Why these choices

- **`join` for walk-built ids.** `[a, b].join('.')` always returns a flat
  string and is cheaper than the template literal it replaces (0.05 µs vs
  0.10 µs per id).
- **A JSON round trip for author strings.** JavaScript cannot test whether
  a string is flat, and of the operations tried on an arbitrary string, only
  a JSON round trip and `normalize()` reliably returned a flat one. `normalize()` rewrites
  non-NFC text, so it would change the wire format; the round trip keeps
  every string as it is, lone surrogates included. `[s, ''].join('')`
  returns `s` itself. Using the string as an object key also works, but
  costs 1.6 µs.
- **Author strings are covered, not just the ones the walk builds.**
  NEBULA's only cons string is text the author's component builds. An
  author's literal can't be told apart from a string their component built,
  so author strings can't be skipped.
- **Strings shorter than 13 characters skip the round trip**, since V8 never
  makes those cons or sliced. Longer strings pay for it even when they are
  already flat. Per frame, 1,500 flat strings add about 0.07 ms at 19
  characters (asset paths), 0.19 ms at 60 (subtitles), and 0.48 ms at 400
  (M5). One non-flat string costs a NEBULA or SPECTRA frame 0.25–0.6 ms.

Covered: authored layer ids, text content, solid color strings, and asset
paths and ids. Strings inside author objects passed through as they are
still reach the frame unflattened, and a non-flat one there still sends that
frame down the slow path: `style` (text fill colors, `fontFamily`), gradient
stop colors, effect colors, `<Font name>` (the font id), PSD
`visibleLayers`/`enabledLayers`/`disabledLayers` (including the parts
`resolveVisibleLayers` splits out of a layer-state string), and `rawLayers`.
A scan of every frame of NEBULA and SPECTRA finds no remaining non-flat
strings.

## Correctness

`packages/react/test/flat-strings.test.mjs` renders deep groups, a
template-literal text, an authored id, a template color, and a sliced asset
path, and asserts that the frame has no cons or sliced strings. It checks
with `%HaveSameMap`/`%IsInternalizedString` under `--allow-natives-syntax`,
so it depends on V8 internals. Without the fix, it lists every one of those
strings.

Every frame line the bridge sends hashes the same before and after the
change (SHA-256 of all 600 NEBULA and 690 SPECTRA frames), so the renderer
receives exactly the same scenes.

`scripts/bench.py` and the CI instruction count don't measure this code:
`celesta-bench` builds every scene, React evaluation included, before it
starts timing (see [benchmarking.md](benchmarking.md)). Their results for
PR #188 were within noise: +0.01% (nebula) and +0.11% (spectra) instructions
in CI, and within ±9% in local GPU time with nothing marked.

## Reproduce

Build packages/react and the exporter:

```sh
pnpm --dir packages/react install --frozen-lockfile
pnpm --dir packages/react run codegen --locked
pnpm --dir packages/react run build
cargo build --release -p celesta-exporter
```

Save this preload as `stringify-probe.mjs`:

```js
// Times the bridge's per-frame JSON.stringify against a JSON.parse'd copy of
// the same frame (identical data, all strings flat). The bridge owns stdout,
// so the result goes to STRINGIFY_PROBE_OUT, rewritten every 20 frames.
import { writeFileSync } from 'node:fs';

const stringify = JSON.stringify;
const live = [];
const copy = [];
let frames = 0;
const median = (values) => [...values].sort((a, b) => a - b)[values.length >> 1];

JSON.stringify = function (value, ...rest) {
  if (rest.length > 0 || value === null || typeof value !== 'object' || !('scene' in value)) {
    return stringify.call(JSON, value, ...rest);
  }
  const started = performance.now();
  const json = stringify.call(JSON, value);
  const elapsed = performance.now() - started;
  if (++frames > 10) {
    live.push(elapsed);
    const parsed = JSON.parse(json);
    const copyStarted = performance.now();
    stringify.call(JSON, parsed);
    copy.push(performance.now() - copyStarted);
    if (frames % 20 === 0) {
      const ratio = median(live) / median(copy);
      writeFileSync(process.env.STRINGIFY_PROBE_OUT,
        `frames=${live.length} live=${median(live).toFixed(3)}ms copy=${median(copy).toFixed(3)}ms ratio=${ratio.toFixed(2)}\n`);
    }
  }
  return json;
};
```

Run both workloads, and compare revisions back to back in one sitting so
heat affects them alike (rebuild packages/react after switching):

```sh
STRINGIFY_PROBE_OUT=nebula.txt NODE_OPTIONS=--import=$PWD/stringify-probe.mjs \
  ./target/release/celesta-exporter --react --no-ui --overwrite --to 00:02 \
  examples/versus/bench/celesta/nebula.tsx /tmp/nebula.mp4 && cat nebula.txt
STRINGIFY_PROBE_OUT=spectra.txt NODE_OPTIONS=--import=$PWD/stringify-probe.mjs \
  ./target/release/celesta-exporter --react --no-ui --overwrite \
  examples/spectra/film.tsx /tmp/spectra.mp4 && cat spectra.txt
```

On Windows, `--import` needs a file URL such as
`--import=file:///C:/path/to/stringify-probe.mjs`. With all strings flat,
`ratio` is about 1.0–1.2. To turn the fast path off, load a preload with
`import v8 from 'node:v8'; v8.setFlagsFromString('--no-json-stringify-fast-path');`
before the probe; V8 flags are not accepted in `NODE_OPTIONS`.

The fast path, and with it this effect, depends on the V8 version. Run the
probe on the Node version Celesta ships with.
