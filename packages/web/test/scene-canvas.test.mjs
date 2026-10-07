import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild-wasm';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const directory = mkdtempSync(fileURLToPath(new URL('../node_modules/.canvas-test-', import.meta.url)));
const result = await build({ entryPoints: [fileURLToPath(new URL('../src/scene-canvas.ts', import.meta.url))], bundle: true, write: false, format: 'esm' });
const modulePath = join(directory, 'canvas.mjs');
writeFileSync(modulePath, result.outputFiles[0].text);
const { SceneCanvas } = await import(pathToFileURL(modulePath));
rmSync(directory, { recursive: true });

test('file font URLs are reused across frames and isolated opacity is applied once', async t => {
  const faces = new Set(), operations = [];
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix, FontFace: globalThis.FontFace, fetch: globalThis.fetch };
  t.after(() => Object.assign(globalThis, originals));
  const createURL = t.mock.method(URL, 'createObjectURL', () => 'blob:font');
  const revokeURL = t.mock.method(URL, 'revokeObjectURL', () => {});
  globalThis.FontFace = class { async load() { return this; } };
  const fetchFont = t.mock.method(globalThis, 'fetch', async () => new Response('@font-face {font-family: Demo; src: url(https://fonts.test/font.woff)}', { headers: { 'content-type': 'text/css' } }));
  globalThis.DOMMatrix = class { translate() { return this; } rotate() { return this; } scale() { return this; } };
  let nextCanvas = 0;
  const canvas = () => {
    const element = { id: nextCanvas++, width: 10, height: 10, getContext() { return context; } };
    const context = { canvas: element, save() {}, restore() {}, setTransform() {}, fillRect() {}, beginPath() {}, roundRect() {}, fill() { operations.push(['fill', element.id, this.globalAlpha]); }, drawImage() { operations.push(['composite', element.id, this.globalAlpha]); } };
    return element;
  };
  globalThis.document = { fonts: faces, createElement: canvas };
  const renderer = new SceneCanvas(new Map([['font.css', new File(['font'], 'font.css')]]));
  const transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const rect = { opacity: 1, transform, content: { type: 'rect', width: 10, height: 10, cornerRadius: 0, fill: { type: 'solid', color: '#fff' } } };
  const group = { opacity: 0.5, transform, effects: { glow: { blur: 1, color: '#fff' } }, content: { type: 'group', layers: [rect] } };
  const scene = { width: 10, height: 10, fonts: [{ id: 'font', location: { type: 'file', path: 'font.css' } }], layers: [group] };
  const output = canvas();
  await renderer.draw(output, scene);
  await renderer.draw(output, scene);
  assert.equal(createURL.mock.callCount(), 1);
  assert.equal(fetchFont.mock.callCount(), 1);
  assert.equal(faces.size, 1);
  assert.deepEqual(operations.map(([kind, , alpha]) => [kind, alpha]), [['fill', 1], ['composite', 0.5], ['fill', 1], ['composite', 0.5]]);
  renderer.dispose();
  assert.equal(revokeURL.mock.callCount(), 1);
  assert.equal(faces.size, 0);
});
