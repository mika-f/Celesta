---
name: celesta
description: Write, edit, check, and export Celesta videos. Covers React compositions (.tsx/.jsx files that import @celesta/react), .celesta.json project timelines, animation, text and fonts, images/video/audio, character dialogue with subtitles and lip sync (image or PSD portraits), project properties, registered components, and MP4 export with the Celesta command-line exporter. Use whenever a file imports @celesta/react, a *.celesta.json file is involved, or the user asks to make, change, preview, debug, or render a video with Celesta, even if they only say "the video" or "the scene".
---

# Celesta

Celesta is a code-first video tool. A video is source code: either a React
composition (`.tsx`) built from `@celesta/react` components, or a
`.celesta.json` project timeline, or both combined. The Celesta app only
**previews** and exports; it never edits files. You edit the source; the user
watches the result in the app.

## Pick the format

| The user wants… | Write |
| --- | --- |
| Motion graphics, generated or data-driven scenes, reusable components, anything computed | React composition (`scene.tsx`) |
| An explicit, hand-placed timeline of clips (gameplay footage, voice lines, simple titles) | JSON project (`project.celesta.json`) |
| A JSON timeline with React-made overlays or components | Both: a `.tsx` that renders `<ProjectTimeline />`, exported with `--project` |

When the user already has a file, keep working in that format. Default to
React for new work: it is more expressive and easier to verify.

## Minimal React composition

```tsx
import { Composition, Rect, Text, interpolate, Easings, useCurrentFrame } from '@celesta/react';

function Title() {
  const frame = useCurrentFrame();
  const opacity = interpolate(frame, [0, 30], [0, 1], {
    easing: Easings.easeOutCubic,
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });
  return (
    <Text x={960} y={540} anchorX={0.5} anchorY={0.5} opacity={opacity}
      style={{ fontFamily: 'sans-serif', fontSize: 96, align: 'center',
        fill: { type: 'solid', color: '#ffffff' } }}>
      Hello, Celesta.
    </Text>
  );
}

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={150}>
      <Rect width={1920} height={1080} fill="#20243a" />
      <Title />
    </Composition>
  );
}
```

## Rules that are easy to get wrong

These come from the runtime and renderer. Breaking one either throws or
produces a video that differs between preview and export.

1. **One root.** The default export is a function component that returns
   exactly one `<Composition>`. `width`, `height`, `fps`, and
   `durationInFrames` must be positive integers. For MP4 export, `width` and
   `height` must also be **even**.
2. **Only Celesta elements.** There is no DOM, CSS, or HTML. `<div>`,
   `<span>`, `className`, and `style` on anything but `Text` do not work
   (unknown elements throw `unsupported element <div>`). Use `Rect`, `Text`,
   `Group`, `Image`, `Video`, `Audio`, and the layout helpers.
3. **`Text` children are strings or numbers only.** Build strings with
   template literals: `` {`${name} · Lv.${level}`} `` rather than mixing
   elements inside `Text`.
4. **Colors are hex strings**, `#RRGGBB` or `#RRGGBBAA`. `Rect` takes
   `fill="#ff0000"`; text styles take paint objects:
   `fill: { type: 'solid', color: '#ff0000' }`.
5. **Every frame is a pure function of the frame number.** Derive motion from
   `useCurrentFrame()` / `useCurrentTime()`. Never use `Math.random()`,
   `Date.now()`, timers, `useEffect`, or state that accumulates across
   renders: frames are rendered out of order when scrubbing and exporting. For
   "random" values, use a deterministic hash of an index or seed.
6. **Async work goes in `prepare()`.** Rendering is synchronous. Fetching,
   file reads, `preloadMedia()`, `loadLipSync()`, and `loadPsdPreset()` belong
   in `export async function prepare()`, which runs once before the first
   frame. Store results in module-level `let` variables and keep offline
   fallbacks.
7. **Relative paths resolve from the entry file's folder**, for `src`,
   `loadLipSync`, `loadPsdPreset`, and `preloadMedia`. JSON project asset
   paths resolve from the project file's folder. `http(s)://` URLs are
   downloaded once and cached forever; change the URL to refresh.
8. **Imports.** `react` and `@celesta/react` always resolve to the runtime
   bundled with Celesta; never `npm install` them. Other relative imports
   (including `import data from './data.json'`) are bundled normally.
9. **Draw order is source order.** Later siblings draw on top. In JSON, later
   tracks draw on top of earlier ones.
10. **Anchors differ between formats.** In React, `x`/`y` place the
    **top-left** corner unless you set `anchorX`/`anchorY` (use `0.5` to
    center). In JSON, `transform.position` places the item's **center**
    (anchor defaults to 0.5) and defaults to (0, 0), so an item without a
    position sits centered on the top-left corner.
11. **Time units differ.** React uses frames (`from`, `durationInFrames`) and
    seconds (`startFrom`). JSON and keyframes use `{ "value": n, "timescale": d }`,
    meaning `n / d` seconds.

## Workflow

1. **Read what exists.** Check the entry file, any `.celesta.json`, and the
   media folder. Note the fps and dimensions before computing frame numbers.
2. **Write or edit the source.** Keep media next to the entry and reference
   it with `./relative/paths`. Prefer small named components over one big
   `Root`.
3. **Check it without the GUI.** You cannot see the preview window, so
   verify with the tools in [references/verify-and-export.md](references/verify-and-export.md):
   - `node scripts/inspect.mjs scene.tsx` (in this skill's folder) loads a
     React entry exactly like Celesta does, runs `prepare()`, evaluates
     frames, and reports every layer, audio clip, and missing media file.
     Use `--frames` to check the moments you changed.
   - If the project has a `tsconfig.json` extending `./.celesta/tsconfig.json`,
     type-check with `npx tsc --noEmit -p .`.
   - For JSON projects, or to check actual pixels, run a short export with
     `--from`/`--to` and `--preset ultrafast`, then extract a still with
     `ffmpeg` if it is installed, and look at the image.
4. **Report back.** Tell the user which file to open (File → Open…) or reload
   (JSON projects need File → Reload; React entries reload on save), what you
   verified, and what you could not verify (for example, how a font looks).

Do not claim the video "looks right" unless you actually rendered and viewed a
frame. A clean `inspect.mjs` run proves the scene evaluates and where layers
sit; it does not prove the rendered pixels.

## References

Load the one you need; each is self-contained.

- [references/react-api.md](references/react-api.md): every `@celesta/react`
  component, prop, hook, and helper (layers, text and fonts, animation,
  `Sequence`/`Transition`, layout helpers, media, `prepare()`, project
  properties, `registerComponent`, debug guides).
- [references/project-json.md](references/project-json.md): the complete
  `.celesta.json` schema, validation rules, keyframes and easing names, and
  a full example.
- [references/dialogue.md](references/dialogue.md): characters, portraits,
  subtitles, voice lines, automatic lip sync, PSD portraits and PSDTool
  presets, in both React and JSON.
- [references/verify-and-export.md](references/verify-and-export.md):
  finding the Celesta executables, `inspect.mjs`, export flags, frame
  extraction, and a table of error messages with fixes.
