import assert from 'node:assert/strict';
import { onTestFinished, test, vi } from 'vitest';
import { build } from 'esbuild-wasm';
import { createRequire } from 'node:module';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { CompileOptions } from '../src/engine';
import type { Frame, Layer, LayerContent } from '../src/types';
import { fetchStub, scope, standIn } from './stand-in.mjs';

// The worker, bundled from source as Vite bundles it for the browser, and
// loaded through Node's own `require`, as a worker loads its script.

const require = createRequire(import.meta.url);
const packages = fileURLToPath(new URL('../../', import.meta.url));
const result = await build({
  entryPoints: [fileURLToPath(new URL('../src/engine.worker.ts', import.meta.url))],
  bundle: true, write: false, format: 'cjs', platform: 'node',
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [{ name: 'worker-in-node', setup(build) {
    // The same source aliases as vite.config.ts.
    build.onResolve({ filter: /^\.\/entry-dir$/ }, () => ({ path: join(packages, 'react/src/entry-dir.browser.ts') }));
    build.onResolve({ filter: /^@celesta\.studio\/react$/ }, () => ({ path: join(packages, 'react/src/browser.ts') }));
    build.onResolve({ filter: /^@celesta\.studio\/react\/internal$/ }, () => ({ path: join(packages, 'react/src/internal.ts') }));
    build.onResolve({ filter: /^@celesta\.studio\/[a-z-]+$/ }, args => ({ path: join(packages, args.path.slice('@celesta.studio/'.length), 'src/index.ts') }));
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

/** A message to the worker, as `Engine` posts it. */
type Request = { id: number; type: 'compile' | 'frame'; source?: string; frame?: number; options?: CompileOptions };
/** The worker's answer: a frame's `value` (a compile's is its config, which these tests do not read), or the error. */
type Reply = { id: number; value?: Frame; error?: string };
/** A font face of the stand-in `FontFace`. */
// The worker makes each face from the font file's bytes.
type Face = { family: string; source: BufferSource };

/** `layer`'s content, which the test expects to be a `type`. */
function content<T extends LayerContent['type']>(layer: Layer, type: T) {
  assert.equal(layer.content.type, type);
  return layer.content as Extract<LayerContent, { type: T }>;
}

test('compiles serialize, fonts refresh the same mount, and recompile unmounts the old root', async () => {
  // The counts the compiled scenes below keep on the global scope.
  const counted = standIn<{ mountCount?: number; cleanupCount?: number }>(globalThis);
  const originals = { self: globalThis.self, fonts: scope.fonts, FontFace: globalThis.FontFace, OffscreenCanvas: globalThis.OffscreenCanvas, fetch: globalThis.fetch };
  onTestFinished(() => { Object.assign(globalThis, originals); delete counted.mountCount; delete counted.cleanupCount; rmSync(directory, { recursive: true }); });
  counted.mountCount = 0; counted.cleanupCount = 0;
  const fonts = new Set<Face>();
  scope.fonts = fonts;
  globalThis.FontFace = standIn(class { constructor(readonly family: string, readonly source: BufferSource) {} async load() { return this; } });
  // As in Chromium's workers, a font string keeps the font it first
  // resolved to, in every context, whatever the font set does later.
  const resolved = new Map<string, number>();
  globalThis.OffscreenCanvas = standIn(class {
    getContext() {
      return {
        font: '',
        measureText(text: string) {
          if (!resolved.has(this.font)) resolved.set(this.font, fonts.size ? 20 : 10);
          return { width: text.length * resolved.get(this.font)!, fontBoundingBoxAscent: 15, fontBoundingBoxDescent: 5 };
        },
      };
    }
  });
  const requested: string[] = [];
  const fetched = Promise.withResolvers<void>(), unblock = Promise.withResolvers<void>();
  const voice = readFileSync(fileURLToPath(new URL('../../../examples/assets/voices/001.wav', import.meta.url)));
  vi.spyOn(globalThis, 'fetch').mockImplementation(fetchStub(async url => {
    if (url.startsWith('https://audio.test/')) return new Response(voice);
    requested.push(url);
    if (url.startsWith('https://first.test/')) { fetched.resolve(); await unblock.promise; }
    return new Response(`@font-face { font-family: Demo; src: url(${new URL('demo.woff', url).href}); }`, { headers: { 'content-type': 'text/css' } });
  }));
  const responses: Reply[] = [], completion = new Map<number, PromiseWithResolvers<Reply>>();
  // The worker's `self`: it answers here, and sets `onmessage` when it loads.
  const worker: { postMessage(response: Reply): void; onmessage?: (event: { data: Request }) => void } = {
    postMessage(response) { responses.push(response); completion.get(response.id)?.resolve(response); },
  };
  globalThis.self = standIn(worker);
  require(modulePath);
  const send = (data: Request) => { const pending = Promise.withResolvers<Reply>(); completion.set(data.id, pending); worker.onmessage!({ data }); return pending.promise; };
  const source = `import { React, Composition, Font, Rect, Audio, useTextMetrics } from '@celesta.studio/react';
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
  assert.equal(counted.mountCount, 1);
  assert.deepEqual(requested, ['https://first.test/font.css']);
  unblock.resolve();
  const values = await Promise.all([first, firstFrame, second, secondFrame]);
  assert.deepEqual(responses.map(response => response.id), [1, 2, 3, 4]);
  for (const response of values) assert.equal(response.error, undefined);
  // Each stylesheet, then the font file it points to, whose bytes the face is made from.
  assert.deepEqual(requested, ['https://first.test/font.css', 'https://first.test/demo.woff', 'https://second.test/font.css', 'https://second.test/demo.woff']);
  assert.equal(counted.mountCount, 2);
  assert.equal(counted.cleanupCount, 1);
  assert.equal(content(values[1].value!.scene.layers[0], 'rect').width, 60);
  assert.equal(values[1].value!.audio.length, 0);
  assert.equal(values[3].value!.audio.length, 1);
  assert.equal(fonts.size, 1);
  // The stand-in server answers the font file with the stylesheet's text.
  assert.match(new TextDecoder().decode([...fonts][0].source), /second\.test\/demo\.woff/);
  // The mount measured Demo before <Font> loaded it; frames measure with it.
  // A frame-dependent string is not the mount's memoized result.
  const growing = `import { Composition, Font, Rect, useCurrentFrame, useTextMetrics } from '@celesta.studio/react';
    export default function Root() {
      const metrics = useTextMetrics('0'.repeat(useCurrentFrame() + 1), { fontFamily: 'Demo' });
      return <Composition width={100} height={100} fps={30} durationInFrames={3}><Font src="font.css" /><Rect width={metrics.width} height={10}/></Composition>;
    }`;
  assert.equal((await send({ id: 20, type: 'compile', source: growing, options: { baseURL: 'https://third.test/' } })).error, undefined);
  assert.equal(content((await send({ id: 21, type: 'frame', frame: 0 })).value!.scene.layers[0], 'rect').width, 20);
  assert.equal(content((await send({ id: 22, type: 'frame', frame: 2 })).value!.scene.layers[0], 'rect').width, 60);
  // Without the <Font>, the recompile's measurements drop the removed face.
  assert.equal((await send({ id: 23, type: 'compile', source: growing.replace('<Font src="font.css" />', '') })).error, undefined);
  assert.equal(fonts.size, 0);
  assert.equal(content((await send({ id: 24, type: 'frame', frame: 2 })).value!.scene.layers[0], 'rect').width, 30);

  const explicit = `import {Composition, Rect, useTextMetrics, measureText} from '@celesta.studio/react';
    export default function Root() { const m=useTextMetrics('abc', {fontFamily:'Demo'}, {fonts:['https://extra.test/font.css']}); return <Composition width={100} height={100} fps={30} durationInFrames={1}><Rect width={m.width} height={1}/></Composition>; }`;
  const expectedError = vi.spyOn(console, 'error').mockImplementation(() => {});
  const rejected = await send({ id: 5, type: 'compile', source: explicit });
  expectedError.mockRestore();
  assert.match(rejected.error!, /preloaded fonts/);
  assert.equal(fonts.size, 0);
  const prepared = await send({ id: 6, type: 'compile', source: explicit + `export async function prepare() { await measureText('abc', {fontFamily:'Demo'}, {fonts:['https://extra.test/font.css']}); }` });
  assert.equal(prepared.error, undefined);
  const preparedFrame = await send({ id: 7, type: 'frame', frame: 0 });
  assert.equal(content(preparedFrame.value!.scene.layers[0], 'rect').width, 60);

  const fallback = await send({ id: 8, type: 'compile', source: explicit + `export async function prepare() { try { const metrics = await measureText('fi'); metrics.glyphs; } catch (error) { if (!error.message.includes('Shaped glyph metrics')) throw error; } await measureText('abc', {fontFamily:'Demo'}, {fonts:['https://extra.test/font.css']}); }` });
  assert.equal(fallback.error, undefined);
  assert.equal(fonts.size, 1);

  // Split packages share the worker's core runtime: layout bounds (also
  // across packages, as a Panel's for a Center inside it), the text
  // measurer (a Badge sizes itself with it), and the character elements
  // registered with its scene walker.
  const split = await send({ id: 9, type: 'compile', source: `import { React, Assets, Composition } from '@celesta.studio/react';
    import { Center } from '@celesta.studio/layout';
    import { Badge, Callout, CaptionTrack, Counter, MediaFrame, Panel } from '@celesta.studio/components';
    import { Circle } from '@celesta.studio/shapes';
    import { Character, CharacterView } from '@celesta.studio/character';
    export default function Root() {
      const actor = React.useRef(null);
      return <Composition width={100} height={100} fps={30} durationInFrames={1}>
        <Assets><Character ref={actor} name="a" portrait={{ defaultExpression: 'n', expressions: { n: 'n.png' } }} /></Assets>
        <Center><Circle radius={5} fill="#ffffff" /></Center>
        <CharacterView character={actor} />
        <Panel width={40} height={20} padding={5}><Center><Circle radius={2} fill="#ffffff" /></Center></Panel>
        <Badge padding={4} fill="#000000">ab</Badge>
        <Counter value={1234.5} decimals={1} durationInFrames={0} suffix=" 件" />
        <MediaFrame kind="image" src="shot.png" width={40} height={30} padding={2} fit="cover" />
        <CaptionTrack enter={null} exit={null} background={null}
          cues={[{ from: 0, durationInFrames: 1, text: 'a👍b', emphasis: [{ start: 1, end: 3, style: { color: '#ff0000' } }] }]} />
        <Callout target={{ x: 90, y: 70 }} x={0} y={50} width={60} padding={4} fill="#000000" title="ab" />
      </Composition>;
    }` });
  assert.equal(split.error, undefined);
  const splitFrame = await send({ id: 10, type: 'frame', frame: 0 });
  const [center, view, panel, badge, counter, frame, caption, callout] = splitFrame.value!.scene.layers;
  // A caption's emphasis counts grapheme clusters, here too: the emoji and the "b".
  type Text = Extract<LayerContent, { type: 'text' }>;
  const firstText = (layer: Layer): Text | undefined => layer.content.type === 'text'
    ? layer.content
    : (layer.content.type === 'group' ? layer.content.layers : []).map(firstText).find(Boolean);
  const captionText = firstText(caption)!;
  assert.deepEqual([captionText.text, captionText.style.colorRuns], ['a👍b', [{ start: 1, end: 3, color: '#ff0000' }]]);
  // A Callout's arrow, from @celesta.studio/shapes, ends at its target, and its box
  // is the measured title (a 33.6 px line box at 28 px) plus the padding.
  const [arrow, box] = content(callout, 'group').layers;
  assert.equal(arrow.content.type, 'path');
  assert.ok(content(arrow, 'path').commands.some((command) => command.type !== 'close' && command.x === 90 && command.y === 70));
  assert.equal(content(content(box, 'group').layers[0], 'rect').height, 41.6);
  // A MediaFrame's image gets the area inside its padding and the fit.
  const image = content(content(content(content(frame, 'group').layers[0], 'group').layers[0], 'group').layers[0], 'image');
  assert.deepEqual([image.type, image.width, image.height, image.fit, image.asset.location], ['image', 36, 26, 'cover', { type: 'file', path: 'shot.png' }]);
  // The worker formats with the same fixed locale as the native runtime.
  assert.equal(content(counter, 'text').text, '1,234.5 件');
  assert.deepEqual(center.transform.position, { x: 50, y: 50 });
  assert.deepEqual(content(content(panel, 'group').layers[0], 'group').layers[0].transform.position, { x: 15, y: 5 });
  // The stub canvas's two 10 px characters and the default 38.4 px line
  // box (32 px text × 1.2), plus the padding.
  const badgeBox = content(content(badge, 'group').layers[0], 'rect');
  assert.deepEqual([badgeBox.width, badgeBox.height], [28, 46.4]);
  assert.deepEqual(content(content(view, 'group').layers[0], 'image').asset.location, { type: 'file', path: 'n.png' });
  const missing = await send({ id: 11, type: 'compile', source: `import { ProjectTimeline } from '@celesta.studio/project'; export default ProjectTimeline;` });
  assert.match(missing.error!, /unavailable in the web editor/);
  // The color filters run in the browser; custom shaders fail when drawn.
  const shader = await send({ id: 12, type: 'compile', source: `import { Composition, Rect } from '@celesta.studio/react';
    import { grayscale } from '@celesta.studio/shader';
    export default function Scene() {
      return <Composition width={100} height={100} fps={30} durationInFrames={1}>
        <Rect width={10} height={10} fill="#ff0000" shader={grayscale()} brightness={1.5} />
      </Composition>;
    }` });
  assert.equal(shader.error, undefined);
  const shaded = await send({ id: 13, type: 'frame', frame: 0 });
  assert.deepEqual(shaded.value!.scene.layers[0].effects!.filters, [
    { type: 'grayscale', amount: 1 },
    { type: 'brightness', amount: 1.5 },
  ]);

  // Lottie animations draw as paths here too, at the composition's time.
  const demo = readFileSync(fileURLToPath(new URL('../../../examples/assets/lottie/demo.json', import.meta.url)), 'utf8');
  const lottie = await send({ id: 30, type: 'compile', source: `import { Composition } from '@celesta.studio/react';
    import { Lottie } from '@celesta.studio/lottie';
    const demo = ${demo};
    export default function Scene() {
      return <Composition width={480} height={270} fps={30} durationInFrames={90}><Lottie data={demo} /></Composition>;
    }` });
  assert.equal(lottie.error, undefined);
  const lottieFrame = await send({ id: 31, type: 'frame', frame: 45 });
  const paths = JSON.stringify(lottieFrame.value!.scene).match(/"type":"path"/g) ?? [];
  assert.ok(paths.length > 5, `${paths.length} paths`);

  // prepare() decodes an uploaded file with mediabunny, to the same samples
  // as the WAV parser reads from the same bytes.
  const spectrum = await send({ id: 14, type: 'compile', options: { assets: new Map([['voice.wav', new File([voice], 'voice.wav')]]) }, source: `import { Composition, Rect } from '@celesta.studio/react';
    import { analyzeAudioSpectrum, loadAudioSpectrum, useAudioSpectrum } from '@celesta.studio/media-utils';
    import { decodeWav } from '@celesta.studio/character';
    let music, reference;
    export async function prepare() {
      music = await loadAudioSpectrum('./voice.wav', { bands: 4 });
      const bytes = new Uint8Array(await (await fetch('https://audio.test/voice.wav')).arrayBuffer());
      reference = analyzeAudioSpectrum(decodeWav(bytes), { bands: 4 });
    }
    function Meter({ spectrum }) {
      const { bands, rms } = useAudioSpectrum(spectrum);
      return <Rect width={rms} height={bands[2]} />;
    }
    export default function Scene() {
      return <Composition width={100} height={100} fps={30} durationInFrames={60}>
        <Meter spectrum={music} /><Meter spectrum={reference} />
      </Composition>;
    }` });
  assert.equal(spectrum.error, undefined);
  const metered = await send({ id: 15, type: 'frame', frame: 30 });
  const [decoded, parsed] = metered.value!.scene.layers.map(layer => content(layer, 'rect'));
  assert.ok(decoded.width > 0.01 && decoded.height > 0);
  assert.equal(decoded.width, parsed.width);
  assert.equal(decoded.height, parsed.height);
  // A recompile reads the file uploaded under that name now, not the last one.
  const silence = Buffer.alloc(44 + 2 * 44100);
  silence.write('RIFF', 0); silence.writeUInt32LE(silence.length - 8, 4); silence.write('WAVEfmt ', 8);
  silence.writeUInt32LE(16, 16); silence.writeUInt16LE(1, 20); silence.writeUInt16LE(1, 22);
  silence.writeUInt32LE(44100, 24); silence.writeUInt32LE(88200, 28); silence.writeUInt16LE(2, 32); silence.writeUInt16LE(16, 34);
  silence.write('data', 36); silence.writeUInt32LE(2 * 44100, 40);
  const recompiled = await send({ id: 17, type: 'compile', options: { assets: new Map([['voice.wav', new File([silence], 'voice.wav')]]) }, source: `import { Composition, Rect } from '@celesta.studio/react';
    import { loadAudioSpectrum, useAudioSpectrum } from '@celesta.studio/media-utils';
    let music;
    export async function prepare() { music = await loadAudioSpectrum('./voice.wav', { bands: 4 }); }
    function Meter() { const { rms } = useAudioSpectrum(music); return <Rect width={rms} height={1} />; }
    export default function Scene() { return <Composition width={100} height={100} fps={30} durationInFrames={30}><Meter /></Composition>; }` });
  assert.equal(recompiled.error, undefined);
  const quiet = await send({ id: 18, type: 'frame', frame: 15 });
  assert.equal(content(quiet.value!.scene.layers[0], 'rect').width, 0);
  const missingAudio = await send({ id: 19, type: 'compile', source: `import { Composition } from '@celesta.studio/react';
    import { loadAudioSpectrum } from '@celesta.studio/media-utils';
    export async function prepare() { await loadAudioSpectrum('nowhere.wav'); }
    export default function Scene() { return <Composition width={10} height={10} fps={30} durationInFrames={1} />; }` });
  assert.match(missingAudio.error!, /Missing audio file: nowhere\.wav/);
});
