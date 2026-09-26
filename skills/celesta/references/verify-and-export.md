# Verify and export

You cannot see the Celesta preview window. Verify your work with the
command-line tools below, in this order of cost: `inspect.mjs` (seconds),
type-check, then a short export and a still frame (real pixels).

## Contents

- [Find the Celesta tools](#find-the-celesta-tools)
- [inspect.mjs](#inspectmjs)
- [Type-check](#type-check)
- [Export](#export)
- [Look at real frames](#look-at-real-frames)
- [Error messages](#error-messages)

## Find the Celesta tools

| Install | Exporter | Bundled Node.js | React runtime (`cli.js`) |
| --- | --- | --- | --- |
| macOS app | `/Applications/Celesta.app/Contents/MacOS/Celesta-export` | `…/Contents/Helpers/node` | `…/Contents/Resources/react/dist/cli.js` |
| Windows installer | `%LOCALAPPDATA%\Programs\Celesta\Celesta-export.exe` | `…\runtime\node.exe` | `…\runtime\react\dist\cli.js` |
| Windows portable ZIP | `<extracted folder>\Celesta-export.exe` | `…\runtime\node.exe` | `…\runtime\react\dist\cli.js` |
| Source checkout | `cargo run -p celesta-exporter --release --` (from the repo root) | `node` on PATH | `packages/react/dist/cli.js` |

- The Windows installer does not add Celesta to PATH; quote the full path.
  In PowerShell, call it as `& "C:\…\Celesta-export.exe" …`.
- A source checkout needs the React runtime built once:
  `cd packages/react && pnpm install && pnpm run codegen && pnpm run build`.
- If nothing is found, ask the user where Celesta is installed.

## inspect.mjs

`scripts/inspect.mjs` in this skill loads a React entry with Celesta's own
runtime (same bundler, same `prepare()` call, same renderer protocol as the
app and exporter), evaluates frames, and prints the resulting layer tree.

```sh
node <skill>/scripts/inspect.mjs scene.tsx                  # frames 0, middle, last
node <skill>/scripts/inspect.mjs scene.tsx --frames 0,45,-1 # -1 is the last frame
node <skill>/scripts/inspect.mjs scene.tsx --every 15       # every 15th frame
node <skill>/scripts/inspect.mjs scene.tsx --json           # raw Scene JSON
node <skill>/scripts/inspect.mjs --psd-layers hana.psd      # PSD layer paths
```

It finds the runtime automatically (a source checkout above the entry or the
current directory, then the default macOS and Windows install locations).
Override with `--runtime <cli.js>` and `--node <node>`. With no Node.js on
PATH, run the script with Celesta's bundled Node:

```sh
"/Applications/Celesta.app/Contents/Helpers/node" <skill>/scripts/inspect.mjs scene.tsx
```

Sample output:

```text
composition: 1920×1080 @ 30 fps, 150 frames (5 s)

── frame 75 (2.5 s) ──
rect 1920×1080 at (0, 0) fill #20243a
text "Hello, Celesta." at (960, 540) anchor (0.5, 0.5) [sans-serif 96px #57456c]
group at (1400, 200)
  image ./mira/calm.png at (0, 0)  ← MISSING FILE
audio ./voices/line-01.wav from 0 s for 3 s

missing: ./mira/calm.png (relative paths resolve from /Users/me/my-video)
1 problem(s) found
```

Read it as: one line per layer in draw order (later lines draw on top),
nested groups indented, positions in canvas pixels. Check that:

- the frames you changed contain the layers you expect, with the text,
  positions, and opacity you intended (an element fully faded out shows
  `opacity 0`; an inactive `Sequence` contributes nothing);
- nothing is `MISSING FILE` and no frame reports `ERROR`;
- audio clips start and last as long as intended.

Limits:

- `preloadMedia()` is answered with `ffprobe` when it is installed;
  otherwise `prepare()` fails with a clear message.
- Entries that render `<ProjectTimeline />`, `<ProjectTrack />`, or
  `useProjectTrack()` need the Rust evaluator and fail here by design;
  verify those with a short export using `--project`.
- It cannot tell whether a font is installed, how wrapped text measures, or
  what an image looks like. Use a still frame for that.

## Type-check

If the project folder has a `tsconfig.json` that extends
`./.celesta/tsconfig.json` (created by **File → Set Up TypeScript** in the
app), run `npx tsc --noEmit -p .` in that folder. The `.celesta/` folder
contains the matching `@celesta/react`, React, and Node.js declarations, so
nothing else needs installing. Without it, rely on `inspect.mjs`.

## Export

```sh
Celesta-export [options] project.celesta.json out.mp4
Celesta-export [options] --react scene.tsx [--project project.celesta.json] out.mp4
```

| Option | Meaning |
| --- | --- |
| `--react <entry>` | Export a React composition instead of a JSON project. |
| `--project <file>` | Companion JSON project for `<ProjectTimeline />`/`<ProjectTrack />` (needs `--react`). |
| `--from <t>`, `--to <t>` | Export only this span; the output starts at 00:00. Times are seconds, `MM:SS.mmm`, or `HH:MM:SS.mmm`. |
| `--overwrite` | Replace an existing output file. Without it, export refuses. |
| `--preset <p>` | libx264 preset, `ultrafast` … `veryslow` (default `medium`). |
| `--crf <n>` | 0–51, lower is better quality (default 18). |
| `--color-conversion <w>` | `auto` (default), `gpu`, or `encoder`. |

Output is H.264 + AAC MP4. Export renders on the GPU when one is available.

**Only overwrite a file the user asked for or one you created.** For checks,
write to a scratch path such as `/tmp/celesta-check.mp4` with `--overwrite`.

A fast check of one second around the part you changed:

```sh
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --overwrite \
  --preset ultrafast --from 2 --to 3 --react scene.tsx /tmp/celesta-check.mp4
```

A JSON project is validated in full when the exporter loads it, so the same
short export is also the way to check a `.celesta.json` for errors.

## Look at real frames

With FFmpeg's command-line tools installed, pull stills from the check
export and look at them (most agents can read PNG images):

```sh
# the frame at 0.5 s into the exported span
ffmpeg -v error -y -ss 0.5 -i /tmp/celesta-check.mp4 -frames:v 1 /tmp/celesta-frame.png
# a contact sheet: one frame every 0.5 s, four across
ffmpeg -v error -y -i /tmp/celesta-check.mp4 -vf "fps=2,scale=480:-1,tile=4x2" -frames:v 1 /tmp/celesta-sheet.png
# confirm there is an audio stream and the duration
ffprobe -v error -show_entries format=duration:stream=codec_type -of compact /tmp/celesta-check.mp4
```

Without FFmpeg, say that you verified structure only and ask the user to
check the preview.

## Error messages

| Message | Cause and fix |
| --- | --- |
| `the entry module must have a default export that is a React component` | Add `export default function Root() { … }`. |
| `the entry module's default export must render a single root <Composition> element` | Return exactly one `<Composition>` (not a fragment or array). |
| `<Composition> requires a positive integer \`fps\` prop` (or width/height/durationInFrames) | Use integers ≥ 1; round computed durations with `Math.ceil`. |
| `unsupported element <div>; use Celesta's built-in components` | No HTML elements; use `Rect`, `Text`, `Group`, … |
| `bare text is only supported inside <Text>` | Wrap strings in `<Text>`. |
| `<Text> children must be a string, a number, or an array of those` | Build the string with a template literal; no elements inside `Text`. |
| `components with asset content require a non-empty \`src\` prop` | An `Image`/`Video`/`Audio`/`Font`/portrait `src` is empty or an unassigned ref. |
| `… must be called from within a Celesta <Composition>`, or React's `Invalid hook call` | Hooks such as `useCurrentFrame()` only work inside components that Celesta renders, never in `prepare()`, at module level, or in plain helper functions. |
| `interpolate() requires inputRange to be strictly increasing` | Sort the input range; no duplicates. |
| `<ProjectTimeline /> requires evaluated project layers` | Export with `--react entry.tsx --project project.celesta.json`. |
| `invalid project JSON: …` | JSON syntax or a wrong field type/name (field names are camelCase). |
| `invalid project: tracks[0].items[1].content.asset: references missing asset "x"` | Validation errors, each with a path. See [project-json.md](project-json.md#validation-rules). |
| `unsupported project version N` | Use `"version": 0`. |
| `H.264 MP4 export requires non-zero even dimensions, got 1921x1080` | Make width and height even. |
| `output already exists: …` | Choose another path, or add `--overwrite` if replacing it is intended. |
| `… missing component` | A JSON `component` item has no matching `registerComponent`, or the project was exported without its React entry. |
| `favorite "x" not found in y.pfv (have: …)` | Use one of the listed favorites. |
| `not a RIFF/WAVE file` / `unsupported WAV sample size` | `loadLipSync` needs uncompressed WAV; convert with `ffmpeg -i in.mp3 -c:a pcm_s16le out.wav`. |
| `Could not find the Celesta React runtime` (inspect.mjs) | Pass `--runtime`, or build the runtime in a source checkout. |
