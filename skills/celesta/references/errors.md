# Error messages and fixes

Find the message (or its start) and apply the fix. The exporter prefixes
failures with `Celesta export:`, or with `--json` reports them as
`error.message` next to an `error.code`
([export.md](export.md#json-results)); `inspect.mjs` prints `ERROR loading entry:`
or `ERROR:` per frame (`UNSUPPORTED inspection:` for text it cannot measure). Dialogue, portrait, and lip-sync symptoms are in
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
| `interpolate() requires a finite input, not NaN` (or `finite inputRange values` / `finite outputRange values, not … at index N`) | A value is `NaN`, infinite, or missing, usually computed from a misspelled field, an unset prop, or a hole in the array. Fix where the number comes from; `'clamp'` does not cover a `NaN`. |
| `spring() requires a finite frame, not NaN` (or `delay`, `from`, `to`; `durationInFrames to be a number`) or `spring() requires a positive fps` | Pass `frame` from `useCurrentFrame()` and `fps` from `useVideoConfig()`; check any arithmetic on them and on the other options. |
| `` <Sequence> `from` must be a finite number of frames, not … `` | `from` is `NaN`, infinite, or not a number, often `scene.start * fps` with a misspelled field. Omit it for 0. |
| `` <Sequence> `durationInFrames` must be a number of frames of at least 0, or Infinity, not … `` | The length is `NaN`, negative, or not a number. Omit it (or pass `Infinity`) to run to the end of the parent; `0` is an empty window. |
| `<Video> startFrom must be a finite number` (or `playbackRate`; `<Audio> startFrom`) | The prop is `NaN`, infinite, or not a number. Omit it for the default. |
| `<Audio> volume must be a finite number or keyframes with finite times and values` (or `playbackRate`, `pitch`; `<Video> volume`) | A number is `NaN` or infinite, or hand-written keyframes are empty, have a hole, or have a non-finite `value` or a `time` whose `value`/`timescale` are not numbers or do not divide to a finite time (a `timescale` of 0, say). Build keyframes with `frameKeyframes()`, which reports the bad key. |
| `interpolateColor() requires #RRGGBB or #RRGGBBAA colors, got … at index N` | Write colors as `#RRGGBB` or `#RRGGBBAA`; CSS names, `rgb()`, and `#RGB` are not accepted. |
| `interpolateColor() extrapolateLeft must be 'extend' or 'clamp'` (or `extrapolateRight`) | `'identity'` has no meaning for colors; use `'clamp'` (the default) or `'extend'`. |
| `Build failed with 1 error: … Could not resolve "pkg"` | The npm package is not installed in the project: run `pnpm add pkg` (or npm) in the project folder ([setup.md](setup.md#npm-dependencies)). Never install `react` or a `@celesta.studio/*` package. A relative path that does not exist fails the same way. |
| `@celesta.studio/react does not export Circle; import it from @celesta.studio/shapes` (any name and packages) | Celesta is split into packages; import the name from the package the message names ([SKILL.md](../SKILL.md#packages)), or run `node <this skill>/scripts/migrate-packages.mjs <project folder>` to move named imports and re-exports at once; it reports `import * as` namespaces that use moved names for you to fix by hand. Random and noise helpers are in `@celesta.studio/math`, shapes in `@celesta.studio/shapes`, `Center`/`Stack`/`Camera` in `@celesta.studio/layout`, `TextBox`/`useTypewriter` in `@celesta.studio/text`, characters and dialogue in `@celesta.studio/character`. |
| `… is not a Celesta package` | The `@celesta.studio/…` name is misspelled or does not exist; see [SKILL.md](../SKILL.md#packages). |
| `invalid project properties:` followed by `--props: key: …`, `--props-file <file>: key: …`, `props.json <file>: key: …` (the entry's automatic `props.json`), or `--project <file>: key: …` lines (`--json` code `invalid_properties`) | A value does not match `defineProjectProperties()`: the wrong type, a color that is not `#RRGGBB`/`#RRGGBBAA`, a `select` value outside `options`, or a `path` that is not an existing file. From `--props`/`--props-file`/`props.json`, an undeclared key is also an error (a `--project` file's undeclared keys are ignored). Fix the value or declare the key ([project-data.md](project-data.md#pass-values-from-the-command-line)). |
| `project property "key" was read at module scope` | Call `getProjectProperty()` in `prepare()` or while rendering, not at module level. |
| `<ProjectTimeline /> requires evaluated project layers` | Export with `--react entry.tsx --project project.celesta.json`. |
| `<CaptionTrack> padding top must be a finite number of 0 or more, not -20` (or `highlight.padding top`, another edge, or `<Panel>`, `<Badge>`, `<StatCard>`, `<Callout>`, `<MediaFrame>` `padding`); `… padding must be a number or { top, right, bottom, left }, not "8"` | Each edge you give must be a finite number of 0 or more: not negative, `NaN`, `Infinity` (often from a calculation), or a string. The padding itself must be one such number for every edge, or an object of `top`/`right`/`bottom`/`left`; a missing edge is 0. |
| `… missing component` | A JSON `component` item has no matching `registerComponent`, or the project was exported without its React entry. |

## Text

| Message | Cause and fix |
| --- | --- |
| `UNSUPPORTED inspection: inspect.mjs cannot shape text, …` | Not a scene error: without an exporter, `inspect.mjs` cannot run `useTextMetrics`/`measureText`/`TextBox`/`useFitText`/`fitText`/`@celesta.studio/code`/`subtitle.render` `metrics`. No exporter supporting `--inspect` was found (or `--no-native` was given): rerun with `--native <Celesta-export>` ([verify.md](verify.md#measured-text---native)), or verify with a PNG export. |
| `… exited before the inspection handshake` (inspect.mjs `--native`) | The exporter is older than `--inspect`: update Celesta, or verify with PNG frames / a contact sheet ([verify.md](verify.md#look-at-real-frames)). |
| `fitText: … must be a finite number greater than 0` / `maxFontSize … must not be less than minFontSize` / `maxLines must be a whole number of at least 1` | Fix the `TextBox`/`useFitText`/`fitText` options. |
| `<TextBox> text "…" does not fit W×H … even at minFontSize N` | `overflow="error"` and the text is too long: shorten it, enlarge the box, allow more lines, or lower `minFontSize`. |
| `Code only supports left alignment` / `Code requires positive fontSize and lineHeight` / `Code line is out of range` / `Code column is out of range` | Fix `@celesta.studio/code` props; positions are one-based ([code.md](code.md)). |
| `warning: font family "X" (weight N) is not installed or loaded; text layer "…" uses a fallback font` | Load the font with `<Font src>` and use its internal family name ([text.md](text.md#fonts-fallback-and-emoji)). |
| `warning: font family "X" (weight 400) has no glyph for "…"; text layer "y" draws them with a fallback font` | The family lacks those characters. With `<Font google text>` or a Google Fonts `text=` URL, add the characters to `text`; with `<Font google subsets>`, add their script or omit `subsets`; otherwise pick a family that covers them. Emoji are reported only when no font has them; then load an emoji font with `<Font>`. |

## Projects (.celesta.json)

| Message | Cause and fix |
| --- | --- |
| `invalid project JSON: …` | JSON syntax (no comments, no trailing commas) or a wrong field type/name (field names are camelCase). |
| `invalid project: tracks[0].items[1].content.asset: references missing asset "x"` | Validation errors, each with a path. See [project-json.md](project-json.md#validation-rules). |
| `unsupported project version N` | Use `"version": 0`. |

## Export

| Message | Cause and fix |
| --- | --- |
| `video export requires non-zero even dimensions, got 1921x1080` | Make width and height even. |
| `output already exists: …` | Choose another path, or add `--overwrite` if replacing it is intended. |
| `still output requires --frame, --frames or --every` | `--contact-sheet` needs a frame selection: add `--every`, `--frame`, or `--frames`, or drop `--contact-sheet`. |
| `no muxer writes .<ext> files` (`--json` code `unsupported_output`) | Name a video output `.mp4`, `.mov`, or `.webm` (the hint lists the extensions installed muxers write), or add a frame selection for a still. See [export.md](export.md#encoders-and-settings). |
| `unknown preset \`<name>\`` (`unknown_preset`) | No presets file (`--preset-file`, the project's `export-presets.json`, the user's) has that name; names are case-sensitive. For x264's preset, use `--video-option preset=<name>`. See [export.md](export.md#export-presets). |
| `the default preset \`<name>\` of <file> is in no presets file` (`unknown_preset`) | The project file's `default` names a missing preset: fix the name or remove `default`, or name the components yourself. |
| `<file>: cannot be read: …`, `is not valid JSON: …`, `unsupported version N; …`, or `is not an export presets file: …` (`invalid_presets`) | Fix that `export-presets.json` (`"version": 1`, a `presets` list). It fails only an export that has to look for a name in that file; a broken user file under a project preset is a warning (below). With `--preset-file`, the same message is a `usage` error. |
| `warning: … does not write .<ext> files; the file is written as … anyway` | A `--muxer` or a preset's muxer does not list the output's extension. Name the output with that muxer's extension, or drop `--muxer`. |
| `warning: preset \`<name>\`: …` | `… is unavailable (… takes no audio); the audio is left out`: the preset names an audio encoder for a muxer without audio, so the file has none (expected for such a muxer; drop the preset's `audio` or use a muxer with audio). Anything else (`has no setting`, `…; using its default`, `is unavailable (…); using the default instead`): the preset is stale (a plugin update), and a key was left out, a value reset, or a component replaced by the default. The export ran; update the preset. |
| `warning: audio options are ignored: preset \`<name>\` leaves the audio out` | `--audio-option` (MCP/TS audio `options`) with a preset whose `"audio"` is `"none"` and no audio encoder named. The export ran without audio; drop the options, or name an `--audio-encoder` to add audio. |
| `warning: your presets file <file> could not be read, so it was not checked for a preset of the same name: …` | The preset (or project `default`) came from the project's file, and the user's `export-presets.json` is broken. The export ran with the project's preset; fix the user file (the same message as `invalid_presets` follows the colon). |
| `<plugin/id> is unavailable: <reason>` (`component_unavailable`) | `not installed`: check the name with `--list-components`. Another reason (`this FFmpeg build has no libx264`): that build lacks the component; use another encoder or the bundled plugin. |
| `<plugin/id> cannot be used with <muxer>: …` (`incompatible_components`) | The encoder writes no codec the muxer takes; choose an encoder for that container, or another extension. |
| `<plugin/id>: <key> …` (`invalid_option`), such as `crf must be at most 51` or `crf applies only when rateControl is crf` | Fix the `--video-option`/`--audio-option` value; `--describe <plugin/id>` lists each key's type, range, and when it applies. |
| `--quality applies to JPEG and WebP …` | `--quality` sets JPEG and WebP still quality; drop it for PNG and video. See [export.md](export.md#still-images). |
| `still output must use the .png, .jpg, .jpeg or .webp extension` | Name the still output `.png`, `.jpg`, `.jpeg`, or `.webp`. |
| `… too large for WebP …` | WebP holds at most 16383 pixels on each side; write the contact sheet as PNG or JPEG, or use fewer columns or a smaller `--tile-width`. |
| `unexpected argument '--every'` (or `--contact-sheet`, `--no-ui`, `--json`), or the exporter prints its usage text | The installed Celesta is older than this skill; ask the user to update, or use only the options its usage lists (without `--json`, use `--no-ui`). |
| Export is very slow | See [performance.md](performance.md). Without a GPU, see [export.md](export.md#without-a-gpu). |

## Media, PSD, lip sync

| Message | Cause and fix |
| --- | --- |
| `favorite "x" not found in y.pfv (have: …)` | Use one of the listed PSDTool favorites. |
| `not a RIFF/WAVE file` / `unsupported WAV sample size` | `loadLipSync` needs uncompressed WAV; convert with `ffmpeg -i in.mp3 -c:a pcm_s16le out.wav`. |
| `inspect.mjs answers preloadMedia() with ffprobe, which is not installed` | Install FFmpeg's `ffprobe`, or verify with a PNG export instead. |
| `inspect.mjs answers loadAudioSpectrum() with ffmpeg, which is not installed` | Install FFmpeg, or let `inspect.mjs` use the exporter (`--native`). |
| `no media decoder is installed, so video and audio files cannot be read` (`--json` code `no_decoder`; the app's Problems says `No media decoder can run`) | No media decoder can run: no media plugin was found, or those found declare no decoder, have no program for this computer, or (in the app) wait for approval or are turned off. Reinstall the app, or in a source checkout build `node scripts/plugins/build.mts plugins/ffmpeg` and set `CELESTA_PLUGIN_PATH` to the absolute path of `plugins/dist` ([plugins.md](plugins.md#media-plugins)). A plugin with `"roles": ["codec"]` is ignored; rebuild it. |
| `no decoder could read <path> (tried <plugin>/<decoder>, …)` (`--json` code `unsupported_media`) | Every decoder said the file is not one it reads. Convert it to a format FFmpeg reads, such as MP4 (H.264/AAC) or WAV, or check that the file really is media. |
| `no media plugin offers a muxer` (`--json` code `unsupported_output`, with the install hint) | No media plugin can write the export; fix it as for `no_decoder`. |
| `could not decode <path>: …` (`loadAudioSpectrum`) | The file is missing, has no audio stream, or FFmpeg cannot read it. In `@celesta.studio/web`, `this browser cannot decode … audio` means WebCodecs lacks that codec there; use WAV or another codec. |
| `<Audio> duck.to must be a finite multiplier or { db }` (or `duck.attack`/`duck.release`/`duck.holdGap` `must be a finite number of seconds`, `duck.under must be a group name or a list of them`, `duck must be an object`, `duckGroup must be a string`) | Thrown in React for `<Audio>` and `<Video>` alike: a `duck` value is not a number (often `NaN` from a calculation) or has the wrong shape. Give `under` a group name or an array of them, `to` a number or `{ db: number }`, and the times in seconds ([react-core.md](react-core.md#ducking)). |
| ``audio `src`: duck.to must be a multiplier from 0 to 1, or { db } at or below 0`` (or `duck.attack`/`duck.release`/`duck.holdGap` `must be finite and not negative`, `duck.under`/`duckGroup` `must be a non-empty name`) | Exporting or previewing a React entry, or `@celesta.studio/web`'s `exportMp4`, found an out-of-range value on the clip playing `src`: keep `to` from 0 to 1 (or `db` at or below 0), the times at 0 or above, and every group name non-empty, with at least one in `under` ([react-core.md](react-core.md#ducking)). |
| `speechIntervals(): the level report has N lines, the plan M; …` (or `line "x" of the plan is "y" in the report`), also from `duckingVolume()` | The `levels` report was made from other lines than the plan. Pass the `report` of the same `matchVoiceLevels()` call whose `lines` went into `planDialogue()` ([dialogue.md](dialogue.md#ducking-bgm-and-game-audio-under-speech)). |
| `Could not find the Celesta React runtime` (inspect.mjs) | Pass `--runtime <cli.js>` ([setup.md](setup.md#find-the-celesta-tools)), or build the runtime in a source checkout. |
