import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild-wasm';
import { createRequire } from 'node:module';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const packages = fileURLToPath(new URL('../../', import.meta.url));
const result = await build({
  entryPoints: [fileURLToPath(new URL('../src/engine.worker.ts', import.meta.url))],
  bundle: true, write: false, format: 'cjs', platform: 'node',
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [{ name: 'worker-in-node', setup(build) {
    // The same source aliases as vite.config.ts.
    build.onResolve({ filter: /^\.\/entry-dir$/ }, () => ({ path: join(packages, 'react/src/entry-dir.browser.ts') }));
    build.onResolve({ filter: /^@celesta\/react$/ }, () => ({ path: join(packages, 'react/src/browser.ts') }));
    build.onResolve({ filter: /^@celesta\/react\/internal$/ }, () => ({ path: join(packages, 'react/src/internal.ts') }));
    build.onResolve({ filter: /^@celesta\/[a-z-]+$/ }, args => ({ path: join(packages, args.path.slice('@celesta/'.length), 'src/index.ts') }));
    build.onResolve({ filter: /^esbuild-wasm$/ }, () => ({ path: 'esbuild', namespace: 'fixture' }));
    build.onLoad({ filter: /.*/, namespace: 'fixture' }, () => ({ contents: `export async function initialize() {} export {build, transform} from ${JSON.stringify(require.resolve('esbuild-wasm'))};` }));
    build.onResolve({ filter: /esbuild-wasm\/lib\/main\.js$/ }, args => ({ path: args.path, external: true }));
    build.onResolve({ filter: /\.wasm\?url$/ }, () => ({ path: 'wasm', namespace: 'wasm' }));
    build.onLoad({ filter: /.*/, namespace: 'wasm' }, () => ({ contents: 'export default "wasm"' }));
  } }],
});
const directory = mkdtempSync(fileURLToPath(new URL('../node_modules/.worker-test-', import.meta.url)));
const modulePath = join(directory, 'worker.cjs');
writeFileSync(modulePath, result.outputFiles[0].text);

test('compiles serialize, fonts refresh the same mount, and recompile unmounts the old root', async t => {
  const originals = { self: globalThis.self, fonts: globalThis.fonts, FontFace: globalThis.FontFace, OffscreenCanvas: globalThis.OffscreenCanvas, fetch: globalThis.fetch };
  t.after(() => { Object.assign(globalThis, originals); delete globalThis.mountCount; delete globalThis.cleanupCount; rmSync(directory, { recursive: true }); });
  globalThis.mountCount = 0; globalThis.cleanupCount = 0;
  globalThis.fonts = new Set();
  globalThis.FontFace = class { constructor(family, source) { this.family = family; this.source = source; } async load() { return this; } };
  globalThis.OffscreenCanvas = class {
    getContext() { return { measureText(text) { return { width: text.length * (globalThis.fonts.size ? 20 : 10), fontBoundingBoxAscent: 15, fontBoundingBoxDescent: 5 }; } }; }
  };
  const requested = [];
  const fetched = Promise.withResolvers(), unblock = Promise.withResolvers();
  t.mock.method(globalThis, 'fetch', async url => {
    requested.push(url);
    if (url.startsWith('https://first.test/')) { fetched.resolve(); await unblock.promise; }
    return new Response(`@font-face { font-family: Demo; src: url(${new URL('demo.woff', url).href}); }`, { headers: { 'content-type': 'text/css' } });
  });
  const responses = [], completion = new Map();
  globalThis.self = { postMessage(response) { responses.push(response); completion.get(response.id)?.resolve(response); } };
  require(modulePath);
  const send = data => { const pending = Promise.withResolvers(); completion.set(data.id, pending); self.onmessage({ data }); return pending.promise; };
  const source = `import { React, Composition, Font, Rect, Audio, useTextMetrics } from '@celesta/react';
    export default function Root() {
      React.useState(() => { globalThis.mountCount++; return 0; });
      React.useLayoutEffect(() => () => { globalThis.cleanupCount++; }, []);
      const metrics = useTextMetrics('abc', { fontFamily: 'Demo' });
      return <Composition width={100} height={100} fps={30} durationInFrames={3}><Font src="font.css" /><Rect width={metrics.width} height={10}/><Audio src="https://audio.test/tone.wav" /></Composition>;
    }`;
  const first = send({ id: 1, type: 'compile', source, options: { baseURL: 'https://first.test/', silent: true } });
  await fetched.promise;
  const firstFrame = send({ id: 2, type: 'frame', frame: 0 });
  const second = send({ id: 3, type: 'compile', source, options: { baseURL: 'https://second.test/', silent: false } });
  const secondFrame = send({ id: 4, type: 'frame', frame: 0 });
  assert.equal(mountCount, 1);
  assert.deepEqual(requested, ['https://first.test/font.css']);
  unblock.resolve();
  const values = await Promise.all([first, firstFrame, second, secondFrame]);
  assert.deepEqual(responses.map(response => response.id), [1, 2, 3, 4]);
  for (const response of values) assert.equal(response.error, undefined);
  assert.deepEqual(requested, ['https://first.test/font.css', 'https://second.test/font.css']);
  assert.equal(mountCount, 2);
  assert.equal(cleanupCount, 1);
  assert.equal(values[1].value.scene.layers[0].content.width, 60);
  assert.equal(values[1].value.audio.length, 0);
  assert.equal(values[3].value.audio.length, 1);
  assert.equal(fonts.size, 1);
  assert.match([...fonts][0].source, /second\.test\/demo\.woff/);

  const explicit = `import {Composition, Rect, useTextMetrics, measureText} from '@celesta/react';
    export default function Root() { const m=useTextMetrics('abc', {fontFamily:'Demo'}, {fonts:['https://extra.test/font.css']}); return <Composition width={100} height={100} fps={30} durationInFrames={1}><Rect width={m.width} height={1}/></Composition>; }`;
  const expectedError = t.mock.method(console, 'error', () => {});
  const rejected = await send({ id: 5, type: 'compile', source: explicit });
  expectedError.mock.restore();
  assert.match(rejected.error, /preloaded fonts/);
  assert.equal(fonts.size, 0);
  const prepared = await send({ id: 6, type: 'compile', source: explicit + `export async function prepare() { await measureText('abc', {fontFamily:'Demo'}, {fonts:['https://extra.test/font.css']}); }` });
  assert.equal(prepared.error, undefined);
  const preparedFrame = await send({ id: 7, type: 'frame', frame: 0 });
  assert.equal(preparedFrame.value.scene.layers[0].content.width, 60);

  const fallback = await send({ id: 8, type: 'compile', source: explicit + `export async function prepare() { try { const metrics = await measureText('fi'); metrics.glyphs; } catch (error) { if (!error.message.includes('Shaped glyph metrics')) throw error; } await measureText('abc', {fontFamily:'Demo'}, {fonts:['https://extra.test/font.css']}); }` });
  assert.equal(fallback.error, undefined);
  assert.equal(fonts.size, 1);

  // Split packages share the worker's core runtime: layout bounds, and the
  // character elements registered with its scene walker.
  const split = await send({ id: 9, type: 'compile', source: `import { React, Assets, Composition } from '@celesta/react';
    import { Center } from '@celesta/layout';
    import { Circle } from '@celesta/shapes';
    import { Character, CharacterView } from '@celesta/character';
    export default function Root() {
      const actor = React.useRef(null);
      return <Composition width={100} height={100} fps={30} durationInFrames={1}>
        <Assets><Character ref={actor} name="a" portrait={{ defaultExpression: 'n', expressions: { n: 'n.png' } }} /></Assets>
        <Center><Circle radius={5} fill="#ffffff" /></Center>
        <CharacterView character={actor} />
      </Composition>;
    }` });
  assert.equal(split.error, undefined);
  const splitFrame = await send({ id: 10, type: 'frame', frame: 0 });
  const [center, view] = splitFrame.value.scene.layers;
  assert.deepEqual(center.transform.position, { x: 50, y: 50 });
  assert.deepEqual(view.content.layers[0].content.asset.location, { type: 'file', path: 'n.png' });
  const missing = await send({ id: 11, type: 'compile', source: `import { ProjectTimeline } from '@celesta/project'; export default ProjectTimeline;` });
  assert.match(missing.error, /unavailable in the web editor/);
});
