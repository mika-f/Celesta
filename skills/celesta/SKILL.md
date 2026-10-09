---
name: celesta
description: Write, edit, check, and export Celesta videos. Covers React compositions (.tsx/.jsx files that import @celesta/react or another @celesta/* package), .celesta.json project timelines, starting a project (--init), animation, shapes and diagrams, text and fonts (measuring, fitting, language, Japanese line breaking), syntax-highlighted code, images/video/audio, character dialogue with subtitles, lip sync, and blinking (image or PSD portraits, custom subtitle bands), project properties, registered components, PNG frame and contact-sheet checks, and MP4 export with the Celesta command-line exporter. Use whenever a file imports @celesta/*, a *.celesta.json file is involved, or the user asks to make, change, preview, debug, or render a video with Celesta, even if they only say "the video" or "the scene".
---

# Celesta

Celesta is a code-first video tool. A video is source code: a React
composition (`.tsx`) built from Celesta's React packages, a
`.celesta.json` project timeline, or both combined. The Celesta app only
**previews** and exports; it never edits files. You edit the source; the user
watches the result in the app.

## Pick the format

| The user wants… | Write |
| --- | --- |
| Motion graphics, generated or data-driven scenes, reusable components, anything computed | React composition (`film.tsx`) |
| An explicit, hand-placed timeline of clips (gameplay footage, voice lines, simple titles) | JSON project (`project.celesta.json`) |
| A JSON timeline with React-made overlays or components | Both: a `.tsx` that renders `<ProjectTimeline />`, exported with `--project` |
| One template rendered with different titles, colors, or data files | React with `defineProjectProperties`, exported with `--props-file` per variant ([project-data.md](references/project-data.md#pass-values-from-the-command-line)) |

When the user already has a file, keep working in that format. Default to
React for new work: it is more expressive and easier to verify. For a new
React project, run `--init` first ([setup.md](references/setup.md#create-a-react-project)).

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
   `durationInFrames` must be positive integers, fixed when the entry loads
   (never computed from the frame). For MP4 export, `width` and `height`
   must also be **even**.
2. **Only Celesta elements.** There is no DOM, CSS, or HTML. `<div>`,
   `<span>`, `className`, and `style` on anything but text components do not
   work (unknown elements throw `unsupported element <div>`). Use `Rect`,
   `Text`, `Group`, `Path`, `Circle`/`Ellipse`/`Arrow`, `Image`, `Video`,
   `Audio`, and the layout helpers.
3. **`Text` children are strings or numbers only.** Build strings with
   template literals: `` {`${name} · Lv.${level}`} `` rather than mixing
   elements inside `Text`.
4. **Colors are hex strings**, `#RRGGBB` or `#RRGGBBAA`. `Rect`/`Path` take
   `fill="#ff0000"`; text styles take paint objects:
   `fill: { type: 'solid', color: '#ff0000' }`.
5. **Every frame is a pure function of the frame number.** Derive motion from
   `useCurrentFrame()` / `useCurrentTime()`. Never use `Math.random()`,
   `Date.now()`, timers, `useEffect`, or state that accumulates across
   renders: frames are rendered out of order when scrubbing and exporting. For
   "random" values, use `random(seed)` and `noise(seed, t)` from
   `@celesta/math` (not `@celesta/react`).
6. **Async work goes in `prepare()`.** Rendering is synchronous. Fetching,
   file reads, `preloadMedia()`, `measureText()`, `fitText()`,
   `planDialogue()`, `loadLipSync()`, and `loadPsdPreset()` belong in
   `export async function prepare()`, which runs once before the first
   frame. Store results in module-level `let` variables and keep offline
   fallbacks. To measure text during render, use `useTextMetrics()`.
7. **Relative paths resolve from the entry file's folder**, for `src`,
   `<Font>`, voices, PSDs, and `preloadMedia`. JSON project asset paths
   resolve from the project file's folder. `http(s)://` URLs are downloaded
   once and cached forever; change the URL to refresh.
8. **Imports.** Celesta is split into packages, like Remotion; import each
   API from the package that provides it (see [Packages](#packages)).
   `react` and every `@celesta/*` package always come from the runtime
   bundled with Celesta; never `npm install` them. Relative imports
   (including `import data from './data.json'`) are bundled. Any other npm
   package must be installed in the project folder (`pnpm add <pkg>`).
9. **Draw order is source order.** Later siblings draw on top. In JSON, later
   tracks draw on top of earlier ones.
10. **Anchors differ between formats.** In React, `x`/`y` place the
    **top-left** corner unless you set `anchorX`/`anchorY` (use `0.5` to
    center); `Path` has no anchor. In JSON, `transform.position` places the
    item's **center** (anchor defaults to 0.5) and defaults to (0, 0), so an
    item without a position sits centered on the top-left corner.
11. **Time units differ.** React uses frames (`from`, `durationInFrames`) and
    seconds (`startFrom`). JSON and keyframes use `{ "value": n, "timescale": d }`,
    meaning `n / d` seconds. In React, build `<Audio>` volume keyframes from
    frames with `frameKeyframes()` instead of converting by hand.
12. **A React `<Video>` is silent.** Add an `<Audio>` with the same `src`,
    `startFrom`, and `playbackRate` to hear it. (JSON `video` items play
    their sound.)

## Workflow

1. **Find the tools and read what exists.** Locate the Celesta executables
   ([setup.md](references/setup.md#find-the-celesta-tools)). Check the entry
   file, any `.celesta.json`, and the media folder. Note the fps and
   dimensions before computing frame numbers. For a new React project,
   `Celesta --init <folder>` creates `film.tsx`, `package.json`, and the
   TypeScript setup.
2. **Write or edit the source.** Keep media next to the entry and reference
   it with `./relative/paths`. Prefer small named components and data arrays
   over one big `Root`. Before hand-rolling timing math or layout, check the
   helpers: [animation.md](references/animation.md#pick-a-helper) (scenes in a
   row, staggered entrances, beats, cues, camera), and
   [text.md](references/text.md) (measuring, fitting text into a box,
   Japanese `lineBreak: 'phrase'`, `<Composition lang="ja-JP">` so CJK text
   uses the right fallback fonts). Draw diagrams with `Path`, `Circle`,
   `Ellipse`, and `Arrow`, not with many thin `Rect`s. Effects and blend modes cost GPU passes
   per layer: put one effect on a `Group` rather than on each of dozens of
   layers ([performance.md](references/performance.md)).
3. **Check it without the GUI** ([verify.md](references/verify.md)). You
   cannot see the preview window.
   - `node <this skill>/scripts/inspect.mjs film.tsx --frames 0,45,-1` loads
     a React entry exactly like Celesta does, runs `prepare()`, evaluates
     frames, and reports every layer, audio clip, and missing media file.
     It cannot shape text: entries using `useTextMetrics`, `TextBox`,
     `fitText`, or `@celesta/code` fail there, so go straight to PNG frames.
   - If the project has a `tsconfig.json` extending `./.celesta/tsconfig.json`,
     type-check with `npx tsc --noEmit -p .` (or `pnpm typecheck`).
   - Pass `--json` to every `Celesta-export` call: it prints one line of
     JSON with the frame count, the exact files written, `warnings`, and an
     error `code` ([export.md](references/export.md#json-results)).
   - Look at real pixels: `Celesta-export --json --react film.tsx --frames 0,90 /tmp/celesta-check.png`,
     then open the PNGs listed in `outputs`. Read `warnings` (fonts, missing glyphs).
     JSON projects: `Celesta-export --json project.celesta.json --frame 0 /tmp/celesta-check.png`
     also validates the whole file.
   - See the whole video at once with a contact sheet:
     `--every 150 --contact-sheet /tmp/celesta-sheet.png` (one tile every
     5 s at 30 fps).
   - Export MP4 only for audio or the final file
     ([export.md](references/export.md)).
4. **Report back.** Tell the user which file to open (File → Open…) or
   reload (JSON projects need File → Reload; React entries reload on save),
   what you verified, and what you could not verify (for example, audio you
   did not listen to).

Do not claim the video "looks right" unless you rendered a frame and looked
at the image. A clean `inspect.mjs` run proves the scene evaluates and where
layers sit; it does not prove the rendered pixels.

## Packages

| Package | Provides |
| --- | --- |
| `@celesta/react` | `Composition`, `Sequence`, `Series`, `Stagger`, `FreezeFrame`, `Group`, `Rect`, `Text`, `Image`, `Video`, `Audio`, `Font`, `Assets`, `useCurrentFrame`/`useCurrentTime`/`useVideoConfig`/`useIsPreview`, `interpolate`, `interpolateColor`, `Easings`, `spring`, `progress`, `frameKeyframes`, `useBeat`/`useCue`, timecodes, `measureText`/`useTextMetrics`/`textCaret`, `registerComponent`, `defineProjectProperties`/`getProjectProperty`, and the scene types |
| `@celesta/shapes` | `Line`, `Polyline`, `Path`, `Circle`, `Ellipse`, `Arrow`, `pointOnPolyline` |
| `@celesta/layout` | `Center`, `Stack`, `Grid`, `Fit`, `SafeArea`, `useLayoutBounds`, `Camera` |
| `@celesta/transitions` | `Transition`, `TransitionSeries`, `useTransitionSeriesScene`, `useTransitionVolume` |
| `@celesta/text` | `TextReveal`, `useTypewriter`, `useCountUp`, `TextBox`, `fitText`, `useFitText` |
| `@celesta/character` | `Character`, `CharacterView`, `Dialogue`, `DialogueSeries`, `planDialogue`, `loadLipSync`, `useLipSync`, `blinkPhase`, `loadPsdPreset`, and their types |
| `@celesta/media-utils` | `preloadMedia`, `mediaDurationInFrames` |
| `@celesta/project` | `ProjectProvider`, `ProjectTimeline`, `ProjectTrack`, `useProject`, `useProjectProperty`, `useProjectTrack`, `loadProject`, and the project file types |
| `@celesta/debug` | `DebugOverlay`, `DebugBounds` |
| `@celesta/math` | `random`, `noise`, and math helpers |
| `@celesta/code` | `Code` and syntax highlighting |
| `@celesta/voicevox` | `lipSyncFromVoicevox` |

An import from the wrong package fails with `@celesta/react does not export
Circle; import it from @celesta/shapes`; move it to the named package.

## References

Load only the one you need; each is self-contained.

| Task | Read |
| --- | --- |
| Find `Celesta`/`Celesta-export`, start a project (`--init`), add npm packages, type-check, how the user previews | [setup.md](references/setup.md) |
| Entry file and `prepare()`, layer props, `Rect`/gradients, `Path`/`Line`/`Polyline`, `Circle`/`Ellipse`/`Arrow`, `Group` clip, `Image`, `Video`, `Audio`, `Font`/`Assets`, layout helpers (`Center`, `SafeArea`, `Stack`, `Grid`, `Fit`), `preloadMedia`, audio keyframes and `frameKeyframes`, debug guides | [react-core.md](references/react-core.md) |
| Frame hooks, `interpolate`, `interpolateColor`, `Easings`, `spring`, `progress`, `Sequence`, `Series`, `TransitionSeries`, `Stagger`, `Transition`, `useBeat`, `useCue`, `Camera`, timecodes | [animation.md](references/animation.md) |
| `Text`, `TextStyle`, fonts and fallback, text language (`lang`), emoji, line breaking, `TextReveal`, `useTypewriter`, `useCountUp`, `useTextMetrics`/`measureText`, `TextBox`/`useFitText`/`fitText` | [text.md](references/text.md) |
| Syntax-highlighted code, typing code, highlighted lines, carets (`@celesta/code`) | [code.md](references/code.md) |
| Seeded random, noise, clamp/lerp/remap, waves, angles, points (`@celesta/math`) | [math.md](references/math.md) |
| Characters, portraits, subtitles (incl. custom `subtitle.render` bands and name plates), voices, `planDialogue`/`DialogueSeries`, lip sync (WAV or VOICEVOX), PSD portraits, blinking, in React and JSON | [dialogue.md](references/dialogue.md) |
| `.celesta.json` schema, time values, tracks, item types, transforms, keyframes, easing names, validation rules, full example | [project-json.md](references/project-json.md) |
| Template inputs (`defineProjectProperties`, `useProjectProperty`, `getProjectProperty`, `--props`/`--props-file`), React reading a project, `<ProjectTimeline />`, `registerComponent` | [project-data.md](references/project-data.md) |
| `inspect.mjs`, PNG frames, contact sheets, checking JSON and audio | [verify.md](references/verify.md) |
| MP4 export options, `--json` results, progress output, exporting without a GPU | [export.md](references/export.md) |
| What is slow to render and how to find it | [performance.md](references/performance.md) |
| An error message or warning | [errors.md](references/errors.md) |
