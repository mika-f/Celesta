# Error messages and fixes

Find the message (or its start) and apply the fix. The exporter prefixes
failures with `Celesta export:`, or with `--json` reports them as
`error.message` next to an `error.code`
([export.md](export.md#json-results)); `inspect.mjs` prints `ERROR loading entry:`
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
| `interpolateColor() requires #RRGGBB or #RRGGBBAA colors, got … at index N` | Write colors as `#RRGGBB` or `#RRGGBBAA`; CSS names, `rgb()`, and `#RGB` are not accepted. |
| `interpolateColor() extrapolateLeft must be 'extend' or 'clamp'` (or `extrapolateRight`) | `'identity'` has no meaning for colors; use `'clamp'` (the default) or `'extend'`. |
| `Build failed with 1 error: … Could not resolve "pkg"` | The npm package is not installed in the project: run `pnpm add pkg` (or npm) in the project folder ([setup.md](setup.md#npm-dependencies)). Never install `react` or a `@celesta/*` package. A relative path that does not exist fails the same way. |
| `@celesta/react does not export Circle; import it from @celesta/shapes` (any name and packages) | Celesta is split into packages; import the name from the package the message names ([SKILL.md](../SKILL.md#packages)). Random and noise helpers are in `@celesta/math`, shapes in `@celesta/shapes`, `Center`/`Stack`/`Camera` in `@celesta/layout`, `TextBox`/`useTypewriter` in `@celesta/text`, characters and dialogue in `@celesta/character`. |
| `… is not a Celesta package` | The `@celesta/…` name is misspelled or does not exist; see [SKILL.md](../SKILL.md#packages). |
| `invalid project properties:` followed by `--props: key: …`, `--props-file <file>: key: …`, or `--project <file>: key: …` lines (`--json` code `invalid_properties`) | A value does not match `defineProjectProperties()`: the wrong type, a color that is not `#RRGGBB`/`#RRGGBBAA`, a `select` value outside `options`, or a `path` that is not an existing file. From `--props`/`--props-file`, an undeclared key is also an error (a `--project` file's undeclared keys are ignored). Fix the value or declare the key ([project-data.md](project-data.md#pass-values-from-the-command-line)). |
| `project property "key" was read at module scope` | Call `getProjectProperty()` in `prepare()` or while rendering, not at module level. |
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
| `unexpected argument '--every'` (or `--contact-sheet`, `--no-ui`, `--json`), or the exporter prints its usage text | The installed Celesta is older than this skill; ask the user to update, or use only the options its usage lists (without `--json`, use `--no-ui`). |
| Export is very slow | See [performance.md](performance.md). Without a GPU, see [export.md](export.md#without-a-gpu). |

## Media, PSD, lip sync

| Message | Cause and fix |
| --- | --- |
| `favorite "x" not found in y.pfv (have: …)` | Use one of the listed PSDTool favorites. |
| `not a RIFF/WAVE file` / `unsupported WAV sample size` | `loadLipSync` needs uncompressed WAV; convert with `ffmpeg -i in.mp3 -c:a pcm_s16le out.wav`. |
| `inspect.mjs answers preloadMedia() with ffprobe, which is not installed` | Install FFmpeg's `ffprobe`, or verify with a PNG export instead. |
| `Could not find the Celesta React runtime` (inspect.mjs) | Pass `--runtime <cli.js>` ([setup.md](setup.md#find-the-celesta-tools)), or build the runtime in a source checkout. |
