# Quality and safety lint

Agents cannot see the preview, so Celesta now reports the video quality and
safety rules of a motion-film spec as numbers: no flashing over 3 a second,
text contrast of at least 4.5:1, no scene under a second, and a frame 0 that
is never blank. Palette-level contrast (role colors checked against each
other) is another task's; this one covers what the layers and the rendered
pixels show.

## Specification

Every issue is a warning with a stable code (`celesta_exporter::quality::QualityCode`):

| Code | Rule | Measured from |
| --- | --- | --- |
| `flashing` | More than 3 general flashes in any one-second window (WCAG 2.3.1). | pixels |
| `red-flashing` | More than 3 saturated-red flashes in a second (WCAG 2.3.1's red flash). | pixels |
| `low-contrast-text` | Text below 4.5:1 against what is behind it, 3:1 for large text (WCAG 1.4.3). | layers and pixels |
| `short-scene` | An outermost React chapter, or an enabled item on an enabled JSON video track, under 1 s; an item's frames are those whose time falls in its range, as the evaluator shows it. | layers |
| `short-shot` | The picture between two hard cuts lasts under 1 s. | pixels |
| `blank-first-frame` | Frame 0 has nothing but solid rectangles (layers) or is one flat color (pixels). | layers and pixels |

An issue (`QualityIssue`) has `code`, `message`, `measured` (`layers` or
`pixels`), `frames` (`[{from, to, start, end}]`, `to` exclusive, seconds
rounded to ms), and where they apply `layer`, `region` (canvas pixels),
`value`, and `limit`. Printed as a warning it reads `code: message`.

### What runs when

The cost decides it:

- **Layer checks, always.** Every video and still export, `--info`, the
  exporter's `--inspect` handshake (chapters), MCP `inspect`, and
  `inspect.mjs` read the scenes they already evaluate: `short_chapters` /
  `short_items` once, and `SceneLint` per frame (blank frame 0, layer-level
  text contrast). A walk over each scene's layers, no rendering.
- **Pixel checks, on request.** `Celesta-export --check-quality` (MCP
  `check_quality`) renders every frame of the composition (or the
  `--from`/`--to` span) and runs `PixelLint` (flashing, red flashing,
  short shots, flat frame 0) and `TextContrast`, writing no file. In check
  mode the layer contrast is off (pixels replace it) and a layer
  `blank-first-frame` is dropped when frame 0 is rendered.

The timeline checks report only scenes and items that touch the frames an
export or check covers (`--from`/`--to`, or the stills selected, from the
first to the last); `--info` reports them all.

`--check-quality` exits 1 when it reports anything, 0 when not, 2 for usage
errors; its `--json` status stays `ok`. It conflicts with an output, frame
selections, encoder, preset, subtitle, loudness, and loop flags.
(`--check` already belonged to `--list-components`.)

### Layer-level text contrast (`quality/scene.rs`)

Each text layer's anchor point is mapped to the canvas through the same flat
affine the renderer uses (`EvaluatedTransform::linear` and `translation`).
The backdrop there is the export background `#14161C` with every solid-fill
rect drawn before it that covers the point composited over it, in encoded
sRGB. Anything else that may cover the point (an image or video of known
size, a path's bounding box, a gradient or filtered rect, or anything of
unknown extent: instances, plugins, PSDs, sizeless images) makes the
backdrop unknown and the text is skipped. Text under a skew, 3D turn,
perspective, mask, or non-normal blend, gradient text, and text with color
runs are skipped too, and so is text whose anchor point its groups' clips
cut away; a rect counts as behind the point only inside every clip it was
drawn through (corners taken as square). The text color is its fill (white by default)
composited at its effective opacity. A stroke (the innermost), shadow, or
glow of at least 50 % alpha counts as a backdrop: the ratio is the best of
them. Large text is a font size times the groups' scale of at least 24 px
(18.66 px with weight 700 or more) on a 1080-pixel short side.

A frame counts only when the text holds still: its opacity and scale equal
those of the adjacent frames before and after (when those were observed), so
fades and pop-ins do not warn. One warning per layer, with the failing
frames joined into sorted spans (frames may come in any order, as
`--frames 90,0` gives them) and the worst ratio.

### Pixel checks (`quality/pixels.rs`)

Frames are read as opaque RGBA8 (`GpuRenderer::render`) and averaged in
linear light onto a grid at most 160 cells across (every second source
pixel above 960 px wide). Per cell:

- A general transition: relative luminance moved at least 0.1 from the last
  extreme against the previous direction, the darker side below 0.8. A
  move of 0.1 that does not count (both sides at 0.8 or above) still turns
  the path, so a flash that starts after a bright lead-in is measured from
  where it turned.
- A red transition: `(R − G − B) · 320` (linear, negatives 0) moved more
  than 20, one side saturated red (`R / (R + G + B) ≥ 0.8`).

Windows a third of the grid wide and high slide in quarter steps (WCAG's
10° field: 341×256 at 1024×768). A window counts a transition in a frame
when cells that made one cover more than a quarter of it. A flash is a pair
of opposing transitions, so eight transitions (four whole flashes) within
`round(fps)` frames is a violation, and a seventh that stays is not; the
reported `value` is whole flashes;
overlapping violations join, their regions united. A hard cut is a frame
where at least 60 % of the cells moved 0.2 (encoded) in some channel; shots
between cuts shorter than a second warn, runs of them joined, except shots
the range cuts off and shots inside a flashing span. Frame 0 is flat when
fewer than 0.05 % of its pixels differ from its median color by more than 8
codes.

### Rendered text contrast (`quality/contrast.rs`, `check.rs`)

Every half second's worth of frames (and the first), each visible text layer
that holds still from the frame before to the frame after is rendered three
times more: twice as probes (`with_probe_fill`: the whole frame with the
text's fill solid black, then white, with its color runs dropped, or for
an outline-only text (transparent fill, visible stroke) its strokes; its own
color filters dropped, so a tint cannot paint both alike, while filters
that move pixels (wave, pixelate, chromatic aberration, zoom blur,
halftone, custom shaders) stay, and a chroma key, which goes by color, is
dropped too; only its visible strokes are painted, so a transparent inner
stroke keeps its gap; and its own opacity 1) and once without it (`without_layer`). Where the probes differ is
where the glyphs show (a text whose fill, color runs, and strokes are
all fully transparent draws nothing and is not sampled): a layer drawn over the text makes them equal there,
so covered text is not measured, and a faded text is found at its full
extent. Halos (strokes, shadows, glows) are the same in both probes and
drop out. Over the pixels that differ by at least 90 % of the largest
difference (45 % when fewer than 16 do; none below 16 codes), each pixel's
contrast between the frame and the frame without the text is taken,
credited with the halo colors as above; the 10th percentile is the
measure. The check holds one sampled frame's render until the next frame
says whether its texts moved.

## Implementation

- `crates/exporter/src/quality/`: `mod.rs` (codes, `QualityIssue`,
  `FrameSpan`, `Region`, `report`, `sort_issues`), `luminance.rs`,
  `scene.rs`, `pixels.rs`, `contrast.rs`.
- `ExportProgress::Quality(QualityIssue)`. The JSON report lists each among
  `warnings` (`code: message`) and in `quality`; the `--json-progress`
  `warning` event carries `quality` too. The terminal progress, `--info`,
  and the editor's export notices print it as a warning.
- `render_video`, `render_react_video`, and the stills loop feed every scene
  to a `SceneLint`; `report_composition` (now shared with stills) reports
  short chapters; project exports, stills, and `describe_project` report
  short items.
- `check.rs`: `Exporter::check_project`, `Exporter::check_react_entry`,
  and `ReactScenes`, the React-plus-companion scene source the stills now
  use too. `ExportJob::check`.
- MCP: `check_quality` (source, start, end); `inspect` adds `quality` and
  the `warning:` lines.
- `inspect.mjs`: the layer checks in JavaScript with the same rules and
  messages, `quality` in `--json`; the exporter's `--inspect` handshake
  sends `config.chapters` like the Node runtime.
- `@celesta.studio/exporter`: `QualityIssue`, `ExportResult.quality`, and the
  warning event's `quality`.
- Docs: the export guide (en/ja) "Check for flashing, contrast, and
  pacing"; the skill's `SKILL.md` workflow, `verify.md`, `errors.md`,
  `export.md`, `setup.md`; the MCP instructions.

## Limits

- Sequences that are not chapters are not short scenes; `short-shot` covers
  cuts in the pixels instead. JSON overlay tracks are not scenes.
- Layer contrast looks only at the anchor point and at solid rectangles;
  the pixel check covers the rest about twice a second.
- The flash measure follows WCAG's definitions on a reduced grid; it is an
  aid, not a certified photosensitivity analysis (PEAT, Harding), which the
  docs say.
- `inspect` and `inspect.mjs` see only the frames they evaluate; spans and
  steadiness need adjacent frames (`--every 1`).

## Checks

Run on macOS (Apple GPU) with FFmpeg 8 on `PKG_CONFIG_PATH`, after
`pnpm install` and `pnpm run build:runtime`:

- `cargo test -p celesta-exporter --lib quality`: the unit tests of the
  layer, pixel, contrast, and luminance checks (25).
- `cargo test -p celesta-exporter --test integration quality`:
  `--check-quality` on JSON projects (a 5 Hz full-frame strobe, dark text,
  a calm titled video and a `--from` span, a usage error) and `--info`'s
  short scene.
- `cargo test -p celesta-exporter -p celesta-mcp --no-fail-fast`: all pass
  but 16 exporter tests that timed out with eight other sessions building on
  the machine (the test codec's program not answering `hello` within 30 s,
  `inspect.mjs`'s 10 s `--help` probe); rerun alone, every one passed.
  The MCP tests include `check_quality`, `inspect`'s quality warnings, and
  the ten tools.
- `cargo clippy -p celesta-exporter -p celesta-mcp -p celesta-editor
  --all-targets -- -D warnings`, `cargo fmt --all -- --check`,
  `git diff --check`.
- `pnpm --dir packages/cli exec vitest run test/inspect.test.mts` (7, the
  two new ones included) and `pnpm --dir packages/exporter exec vitest run`
  (25; with the real `node` first on `PATH`, since the fake exporter's
  `#!/usr/bin/env node` hung on this machine's mise shim).
- By hand: a React entry with a 5 Hz strobe from 1 s to 2 s, a light-grey
  caption on a lighter panel, white text, and a 0.5 s first chapter.
  `--check-quality` (about 2.5 s for 90 frames at 640×360) reported
  `flashing` (5 flashes, frames 30–57, the whole frame), both texts'
  `low-contrast-text` in pixels, and `short-scene`, and exited 1;
  `inspect.mjs --native --every 1` and a still export reported the layer
  ones. Rendering is unchanged, so there are no before/after screenshots.
