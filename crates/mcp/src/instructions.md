Celesta is a code-first video tool: a video is source code, either a React composition (`film.tsx`) built from Celesta's `@celesta/*` packages or a `.celesta.json` project timeline. You edit the source files yourself; these tools check and render them.

Rules that are easy to get wrong:
- The default export returns exactly one `<Composition>` with fixed positive integer `width`, `height`, `fps`, `durationInFrames` (even width/height for video export).
- Only Celesta elements (`Rect`, `Text`, `Group`, `Image`, `Video`, `Audio`, shapes, layout helpers); no DOM, HTML, or CSS.
- Every frame is a pure function of the frame number: no `Math.random()`, `Date.now()`, timers, effects, or accumulated state. Use `random`/`noise` from `@celesta/math`.
- Async work (fetching, `measureText`, `preloadMedia`, `loadAudioSpectrum`, …) goes in `export async function prepare()`.
- Colors are hex strings; relative paths resolve from the entry's folder; import each API from the package that provides it.

Workflow:
1. Read the existing source, or create a project with `init_project` (see `list_templates`).
2. Edit the source.
3. `inspect` the React entry: it evaluates frames without rendering and reports layers, audio, and missing files.
4. `render_frames` and look at the images; `contact_sheet` shows the whole video in one image. Do not claim the video looks right without looking at a rendered frame.
5. `export_video` only for audio or the final video (.mp4, .mov or .webm), or without `output` to write only its `subtitles` files.

Call `read_docs` before writing a feature you have not used: `read_docs` with no topic lists the topics. The docs mention shell commands; these tools replace them: `inspect.mjs` → `inspect`, `Celesta-export --frame/--frames/--every` → `render_frames`, `--contact-sheet` → `contact_sheet`, video export → `export_video`, `Celesta --init` → `init_project`, `--list-templates` → `list_templates`. `inspect.mjs --psd-layers` has no tool; run it in a shell.

Failed calls return `isError` with `error.code`, `error.message`, an optional `error.hint`, and `error.issues`; the codes are listed in `read_docs` topic `export` (JSON results).
