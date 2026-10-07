# Full-text color runs and Code evaluation

Issue: [#99](https://github.com/mika-f/Celesta/issues/99).

## API

`TextStyle.colorRuns` is an optional array of `{ start, end, color }` overrides.
Offsets are half-open **Unicode code-point** ranges in the complete source text,
including line separators; they are not UTF-16 or UTF-8 offsets. Ranges must be
ordered, non-overlapping and within the source. Gaps use `style.fill`, or white
when no fill is supplied. Colors use the same hexadecimal notation as solid
paints. Omitting the fields preserves ordinary single-color Text.

```tsx
<Text style={{
  fontSize: 40,
  fill: { type: 'solid', color: '#ffffff' },
  colorRuns: [{ start: 0, end: 5, color: '#a68bbf' }],
  visibleCharacters: 7,
}}>Hello world</Text>
```

`visibleCharacters` reveals clusters beginning before that code-point offset;
omitting it shows all text. The full text still determines width, alignment,
wrapping and anchors. A ligature or combining cluster uses its first code point's
color and is revealed as a whole. Color emoji retain their original colors.
Paint and visibility do not divide shaping runs. These fields also work on
subtitle styles; language parsing and code themes remain in `@celesta/code`.

`GlyphMetrics` now includes `start`, `end` and `rtl`. `textCaret(metrics, offset)`
returns `{ x, y, line }` from full-source shaping. Native metrics also include
`lineStarts`, so carets on empty and trailing lines retain their line position.
Positions inside a cluster interpolate its advance rather than using font-specific ligature caret stops.
`useCodePoint()` maps original tabs and CRLF positions to this layout; moving a
caret no longer makes prefix measurement RPCs.

## Rendering

Code emits one Text per non-empty source line, with full expanded text and color
ranges. It measures the complete source and a baseline once each. Tokenization,
line preparation and caret tab maps are cached for unchanged inputs. Reveal,
seek, themes and highlight bands do not trigger additional measurements.

CPU and GPU use the same native glyph rasterizer. It caches complete shaped
buffers by text, font/layout settings, width and scale, excluding paint and
visibility. The cache retains at most 32 layouts, skips sources larger than
100 KB and is cleared when fonts load. GPU workers retain their own caches.
Single-line anchors use full glyph coverage even during a partial reveal.
Unwrapped lines wider than the GPU texture limit rasterize below 1x when
necessary; this preserves placement but can reduce sharpness.

The browser rasterizes each full line once, uses full-line SVG character extents
for cluster positions and applies paint/reveal masks to the cached Canvas image.
Its cache retains the active scene, including scenes exceeding 32 lines.
After each frame it prunes unused entries to the larger of the active entry
count and the idle limits (32 layouts / 64 painted variants).
Emoji presentation text uses an additional black/white raster probe to keep
actual color glyphs while tinting monochrome fallback glyphs. It does not shape
individual color fragments or measure prefixes. Browser color masks follow cluster advance
regions; glyph overhangs and overlapping glyphs can differ at color boundaries
from native per-glyph painting. Browser and native fonts/rasterization and the
browser's existing word wrapping are platform-dependent, so identical pixel
output across browsers and desktop is not promised. Custom Font assets and Code
rendering are supported by the browser runtime.
`useCodePoint()` still requires native shaped glyph metrics.

## Release comparison

Measured on 2026-10-07, Apple M4 / Metal, 1920×1080, Menlo 16 px / 24 px line
height. Values are medians of three independent samples, 30 frames per phase.
Before uses the Code sources at `f6ace76`; after uses the implementation at
`fcdb9ed` (before review fixes). Both use the same release-built bridge and
renderers at `fcdb9ed`. This isolates the
Code representation change; it is not a benchmark of two entire repository
revisions. In particular, both sides use the extended glyph measurement payload.

The long line is `'const n = 1; '.repeat(100)` (1,300 code points). Multiline is
10 lines of 10 repeats (1,309 code points including separators). Initial
evaluation includes Node startup, bundling, font discovery, measurement RPCs and
the first Scene. Renderer/device construction is excluded from render timings.
Continued frames use complete text and alternate highlight bands. Typing frames
sample increasing source counts; their scenes are evaluated before rendering.
GPU timings use pipelined submit/drain with readback. The first actual compact
Scene response is observed to count its UTF-8 bytes and initial measurement RPCs;
steady-state responses are not recorded or parsed by the probe.

| Metric | Long line before | Long line after | Multiline before | Multiline after |
| --- | ---: | ---: | ---: | ---: |
| Initial measurement RPCs | 502 | 2 | 502 | 2 |
| Text layers | 500 | 1 | 500 | 10 |
| Compact Scene bytes | 143,494 | 40,205 | 144,645 | 42,295 |
| Initial evaluation, ms | 338.461 | 87.949 | 129.546 | 90.005 |
| Continued evaluation, ms/frame | 2.031 | 0.290 | 1.822 | 0.349 |
| Typing evaluation, ms/frame | 1.219 | 0.230 | 1.013 | 0.187 |
| First CPU render, ms | 2.027 | 2.208 | 2.183 | 2.298 |
| Continued CPU render, ms/frame | 1.015 | 0.774 | 1.386 | 1.405 |
| Typing CPU render, ms/frame | 0.531 | 0.565 | 0.904 | 0.947 |
| First GPU render, ms | 9.539 | 7.911 | 9.390 | 6.858 |
| Continued GPU render, ms/frame | 0.760 | 0.601 | 0.761 | 0.642 |
| Typing GPU render, ms/frame | 0.583 | 0.642 | 0.574 | 0.569 |

The deterministic reduction is in measurement count, layer count and transfer
size. Evaluation and settled GPU rendering improve in this sample. CPU and GPU
typing do not uniformly improve: each reveal changes a whole line's raster,
whereas the previous implementation could reuse textures for completed token
fragments. Larger or animated scenes need their own measurements; these wall
clock values are local observations, not regression thresholds.

## Reproduce and validate

```sh
pnpm install --frozen-lockfile
pnpm --dir packages/react run codegen
pnpm --dir packages/react run build
# macOS with ffmpeg@8 installed:
export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"
node scripts/bench-code-runs.mjs f6ace76 3 > code-runs.jsonl
```

The benchmark writes legacy sources and a small wire probe under ignored
`target/code-runs-baseline`, then runs the release `celesta-bench` example. The
optional sample-count argument selects the number of repetitions. To benchmark a different
Code fixture directly, run `cargo run --release -p celesta-bench --example
code-runs -- path/to/entry.tsx 30`.

Checks exercised for this change:

- Code: 12 tests, including constant RPC counts on long/multiple lines, source
  tabs/CRLF, moving carets, highlight changes, typing/seeking and packaged CLI.
- React: 141 tests, including cluster/BiDi carets; Code example type checking.
- Composition code generation: 49 tests; project code generation: 29 tests.
- Native rasterizer: 104 tests, including identical coverage/layout under color
  changes, fixed reveal geometry, gradients, range errors and source offsets.
- GPU: 76 tests, including CPU comparison after premultiplying the reference,
  texture reuse/reveals and an unwrapped line exceeding the texture limit.
- Bridge: 24 unit tests and 26 Node integration tests.
- Web build and `node packages/web/test/text-runs.mjs [path-to-chromium]`:
  real browser shaping/coverage, bidi, emoji, multiline/wrapping, combining
  reveals, stroke and gradient coverage, gradient origins, compact ink bounds,
  resolved locales, monochrome emoji fallback, and 80-line cache reuse. The default Chromium path is macOS
  Google Chrome; pass its executable explicitly on other platforms.
- Web unit tests: font cache invalidation, source offsets after wrapping and
  existing browser worker/rendering behavior.

The performance Clippy check passes with the pre-existing
`clippy::while_immutable_condition` diagnostic in GPU texture mipmap generation
excluded. The check is `cargo clippy -p celesta-renderer -p celesta-gpu-renderer
-p celesta-react-bridge -p celesta-bench --all-targets --locked -- -D clippy::perf
-A clippy::while_immutable_condition`. Other existing warnings were not changed.
