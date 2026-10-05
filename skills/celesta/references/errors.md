# Error messages and fixes

Find the message (or its start) and apply the fix. The exporter prefixes
failures with `Celesta export:`; `inspect.mjs` prints `ERROR loading entry:`
or `ERROR:` per frame. Dialogue, portrait, and lip-sync symptoms are in
[dialogue.md](dialogue.md#troubleshooting).

## Entry and components (React)

| Message | Cause and fix |
| --- | --- |
| `the entry module must have a default export that is a React component` | Add `export default function Root() { … }`. |
| `the entry module's default export must render a single root <Composition> element` | Return exactly one `<Composition>` (not a fragment or array). |
| `<Composition> requires a positive integer \`fps\` prop` (or width/height/durationInFrames) | Use integers ≥ 1; round computed durations with `Math.ceil`. |
| `unsupported element <div>; use Celesta's built-in components` | No HTML elements or CSS; use `Rect`, `Text`, `Group`, … |
| `bare text is only supported inside <Text>` | Wrap strings in `<Text>`. |
| `<Text> children must be a string, a number, or an array of those` | Build the string with a template literal; no elements inside `Text`. Same for `<TextBox>`. |
| `components with asset content require a non-empty \`src\` prop` | An `Image`/`Video`/`Audio`/`Font`/portrait `src` is empty or an unassigned ref. |
| `… must be called from within a Celesta <Composition>`, or React's `Invalid hook call` | Hooks such as `useCurrentFrame()` only work inside components that Celesta renders, never in `prepare()`, at module level, or in plain helper functions. |
| `interpolate() requires inputRange to be strictly increasing` | Sort the input range; no duplicates. |
| `Build failed with 1 error: … Could not resolve "pkg"` | The npm package is not installed in the project: run `pnpm add pkg` (or npm) in the project folder ([setup.md](setup.md#npm-dependencies)). Never install `react`, `@celesta/react`, `@celesta/math`, or `@celesta/code`. A relative path that does not exist fails the same way. |
| `Named export 'random' not found` (or `noise`, `randomRange`, …) | Random, noise, and math helpers moved to `@celesta/math`: `import { random } from '@celesta/math'`. |
| `<ProjectTimeline /> requires evaluated project layers` | Export with `--react entry.tsx --project project.celesta.json`. |
| `… missing component` | A JSON `component` item has no matching `registerComponent`, or the project was exported without its React entry. |

## Text

| Message | Cause and fix |
| --- | --- |
| `inspect.mjs cannot shape text, …` | Expected: `inspect.mjs` cannot run `useTextMetrics`/`measureText`/`TextBox`/`useFitText`/`fitText`/`@celesta/code`. Verify with a PNG export ([verify.md](verify.md#look-at-real-frames)). |
| `fitText: … must be a finite number greater than 0` / `maxFontSize … must not be less than minFontSize` / `maxLines must be a whole number of at least 1` | Fix the `TextBox`/`useFitText`/`fitText` options. |
| `<TextBox> text "…" does not fit W×H … even at minFontSize N` | `overflow="error"` and the text is too long: shorten it, enlarge the box, allow more lines, or lower `minFontSize`. |
| `Code only supports left alignment` / `Code requires positive fontSize and lineHeight` / `Code line is out of range` / `Code column is out of range` | Fix `@celesta/code` props; positions are one-based ([code.md](code.md)). |
| `warning: font family "X" (weight N) is not installed or loaded; text layer "…" uses a fallback font` | Load the font with `<Font src>` and use its internal family name ([text.md](text.md#fonts-fallback-and-emoji)). |
| `warning: font family "X" (weight 400) has no glyph for "…"; text layer "y" draws them with a fallback font` | The family lacks those characters. With a Google Fonts `text=` subset, add the characters to `text=`; otherwise pick a family that covers them. Emoji are reported only when no font has them; then load an emoji font with `<Font>`. |

## Projects (.celesta.json)

| Message | Cause and fix |
| --- | --- |
| `invalid project JSON: …` | JSON syntax (no comments, no trailing commas) or a wrong field type/name (field names are camelCase). |
| `invalid project: tracks[0].items[1].content.asset: references missing asset "x"` | Validation errors, each with a path. See [project-json.md](project-json.md#validation-rules). |
| `unsupported project version N` | Use `"version": 0`. |

## Export

| Message | Cause and fix |
| --- | --- |
| `H.264 MP4 export requires non-zero even dimensions, got 1921x1080` | Make width and height even. |
| `output already exists: …` | Choose another path, or add `--overwrite` if replacing it is intended. |
| `PNG output requires --frame, --frames or --every` | Add a frame selection, or drop `--output-format png` / `--contact-sheet`. |
| `unexpected argument '--every'` (or `--contact-sheet`, `--no-ui`), or the exporter prints its usage text | The installed Celesta is older than this skill; ask the user to update, or use only the options its usage lists. |
| Export is very slow | See [performance.md](performance.md). Without a GPU, see [export.md](export.md#without-a-gpu). |

## Media, PSD, lip sync

| Message | Cause and fix |
| --- | --- |
| `favorite "x" not found in y.pfv (have: …)` | Use one of the listed PSDTool favorites. |
| `not a RIFF/WAVE file` / `unsupported WAV sample size` | `loadLipSync` needs uncompressed WAV; convert with `ffmpeg -i in.mp3 -c:a pcm_s16le out.wav`. |
| `inspect.mjs answers preloadMedia() with ffprobe, which is not installed` | Install FFmpeg's `ffprobe`, or verify with a PNG export instead. |
| `Could not find the Celesta React runtime` (inspect.mjs) | Pass `--runtime <cli.js>` ([setup.md](setup.md#find-the-celesta-tools)), or build the runtime in a source checkout. |
