# @celesta/web

Browser runtime for [Celesta](https://github.com/mika-f/Celesta) React compositions. It compiles a self-contained TSX source string in a dedicated Web Worker, evaluates frames with Celesta's React renderer, draws `Scene` data on Canvas, and exports H.264/AAC MP4 with WebCodecs.

## Install

```sh
pnpm add @celesta/web
```

Use this command after the package is published. For a local build, run
`pnpm --filter @celesta/web build`, pack from `packages/web`, and install the
resulting `.tgz` in your app. The website uses `workspace:*` directly.

The package is browser-only. Its `dist/` includes the worker and esbuild WASM. Serve the packaged assets alongside the JavaScript bundle; bundlers that support `import.meta.url` asset references, including Vite, copy them automatically. A Content Security Policy must allow workers, WebAssembly compilation, and code evaluation in the worker (`unsafe-eval`).

## Use

```ts
import { Engine, SceneCanvas, exportMp4 } from '@celesta/web';

const source = `
  import { Composition, Rect } from '@celesta/react';
  export default function Film() {
    return <Composition width={320} height={180} fps={30} durationInFrames={60}>
      <Rect width={320} height={180} fill={{ type: 'solid', color: '#4e62bc' }} />
    </Composition>;
  }
`;

const engine = new Engine();
const renderer = new SceneCanvas();
try {
  const config = await engine.compile(source);
  const canvas = document.querySelector('canvas')!;
  await renderer.draw(canvas, (await engine.frame(0)).scene);

  const controller = new AbortController();
  const mp4 = await exportMp4(engine, config, renderer, controller.signal, (done, total) => {
    console.log(`${done}/${total}`);
  });
  // Save the Blob with a download link or the File System Access API.
} finally {
  renderer.dispose();
  engine.dispose();
}
```

Pass local media as `new SceneCanvas(new Map<string, File>([['clip.mp4', file]]))`. Use the same file name in a composition's `src`. Remote media must allow CORS. `exportMp4` returns a `video/mp4` Blob and reports completed frames. Abort its signal to cancel.

The source string accepts runtime imports from `@celesta/react`, `@celesta/math`,
`@celesta/code`, and `react`. It needs a default component returning
`<Composition>`. Paths, motion helpers, Canvas text measurement, and custom
OpenType fonts or font CSS URLs are supported. An async `prepare()` may use
browser APIs and `measureText()`; filesystem helpers, companion JSON projects,
PSD portraits, and arbitrary npm imports still require the native runtime.

For a multi-file project, pass virtual sources keyed by relative path:

```ts
const baseURL = new URL('/media/my-sample/', location.origin).href;
const config = await engine.compile(files['film.tsx'], {
  files, entry: 'film.tsx', baseURL,
});
const renderer = new SceneCanvas(new Map(), baseURL);
```

Relative TSX/TS/JS/JSON imports resolve within `files`, including directory
`index.tsx`/`index.ts` entries. Sources are bundled in the Worker. `baseURL`
resolves relative fonts and media; uploaded files take precedence in the canvas
renderer. Fonts used for measurement must also be reachable by the Worker.
Fonts declared with `<Font>` load before the first requested frame. A
synchronous `useTextMetrics(..., { fonts })` or `useFitText(..., { fonts })`
requires those extra fonts to be preloaded using `await measureText(...,
{ fonts })` in `prepare()`; the browser reports an error instead of caching
fallback measurements for unloaded extra fonts. Failed font loads warn once
and use the default font, so a missing face does not prevent playback.
All remote fonts and media need CORS access. `silent: true` explicitly omits
audio from evaluated frames and exported MP4s, for sample editions without
distributed soundtracks.

Preview has no audio playback. MP4 export mixes constant rate and volume audio;
animated audio values produce an error. H.264 WebCodecs support is required,
and AAC support is required when the composition contains audio. Output is
buffered in memory. Browser Canvas text, effects, and video rendering can
differ from Celesta's native renderer.

The worker evaluates author-provided JavaScript. Run only code you trust; a Worker separates it from the page DOM but is not a security boundary for untrusted code.

## Build from source

```sh
pnpm install
pnpm --dir packages/react run codegen
pnpm --filter @celesta/web build
pnpm --filter @celesta/web test
pnpm --filter @celesta/website build
```
