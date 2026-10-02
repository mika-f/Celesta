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

The source string accepts runtime imports from `@celesta/react`, `@celesta/math`, and `react` only. It needs a default component returning `<Composition>`. `prepare()`, companion JSON projects, PSD portraits, npm imports, and custom fonts are not supported by this browser runtime. Preview has no audio playback. MP4 export mixes constant rate and volume audio; animated audio values produce an error. H.264 WebCodecs support is required, and AAC support is required when the composition contains audio. Output is buffered in memory. Browser Canvas text and video rendering can differ from Celesta's native renderer.

The worker evaluates author-provided JavaScript. Run only code you trust; a Worker separates it from the page DOM but is not a security boundary for untrusted code.

## Build from source

```sh
pnpm install
pnpm --filter @celesta/web build
pnpm --filter @celesta/website build
```
