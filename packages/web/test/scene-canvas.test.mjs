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
    const stack = [];
    const context = {
      canvas: element, globalAlpha: 1, filter: 'none', globalCompositeOperation: 'source-over',
      save() { stack.push({ globalAlpha: this.globalAlpha, filter: this.filter, globalCompositeOperation: this.globalCompositeOperation }); },
      restore() { Object.assign(this, stack.pop()); }, setTransform() {}, fillRect() {}, beginPath() {}, roundRect() {},
      fill() { operations.push(['fill', element.id, this.globalAlpha]); },
      drawImage(source, x, y) { operations.push(['composite', element.id, this.globalAlpha, source.id, this.filter, this.globalCompositeOperation, x, y]); },
    };
    return element;
  };
  globalThis.document = { fonts: faces, createElement: canvas };
  const renderer = new SceneCanvas(new Map([['font.css', new File(['font'], 'font.css')]]));
  const transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const rect = { opacity: 1, transform, content: { type: 'rect', width: 10, height: 10, cornerRadius: 0, fill: { type: 'solid', color: '#fff' } } };
  const group = { opacity: 0.5, transform, effects: { glow: { blur: 1, color: '#fff' } }, content: { type: 'group', layers: [rect] } };
  const scene = { width: 10, height: 10, fonts: [{ id: 'font', location: { type: 'file', path: 'font.css' } }], layers: [group] };
  const output = canvas();
  renderer.styledText.set('fallback', {});
  renderer.textLayouts.set('fallback', {});
  await renderer.draw(output, scene);
  assert.equal(renderer.styledText.size, 0, 'font changes invalidate painted text');
  assert.equal(renderer.textLayouts.size, 0, 'font changes invalidate shaped text');
  renderer.textLayouts.set('loaded-font', {});
  await renderer.draw(output, scene);
  assert.equal(renderer.textLayouts.size, 1, 'unchanged fonts retain the layout cache');
  assert.equal(createURL.mock.callCount(), 1);
  assert.equal(fetchFont.mock.callCount(), 1);
  assert.equal(faces.size, 1);
  assert.deepEqual(operations.filter(([kind, id]) => kind === 'fill' || id === output.id).map(([kind, , alpha]) => [kind, alpha]), [['fill', 1], ['composite', 0.5], ['fill', 1], ['composite', 0.5]]);

  operations.length = 0;
  const effects = { blur: 2, shadow: { color: '#0008', blur: 4, offsetX: 3, offsetY: 5 }, glow: { color: '#f008', blur: 6 } };
  await renderer.draw(output, { ...scene, layers: [{ ...rect, opacity: 0.5, effects }] });
  const source = operations.find(([kind]) => kind === 'fill')[1];
  const composites = operations.filter(([kind]) => kind === 'composite');
  const masks = composites.filter(([, , , from, filter]) => from === source && filter === 'none');
  assert.equal(masks.length, 2, 'shadow and glow independently read the original source');
  const base = composites.find(([, , , from, filter]) => from === source && filter === 'blur(2px)');
  const result = base[1];
  assert.deepEqual(composites.filter(([, id]) => id === result).map(([, , alpha, , filter, mode, x, y]) => ({ alpha, filter, mode, x, y })), [
    { alpha: 1, filter: 'blur(4px)', mode: 'source-over', x: 3, y: 5 },
    { alpha: 1, filter: 'blur(6px)', mode: 'source-over', x: 0, y: 0 },
    { alpha: 1, filter: 'blur(2px)', mode: 'source-over', x: 0, y: 0 },
  ]);
  assert.deepEqual(composites.filter(([, id]) => id === output.id).map(([, , alpha, from, filter]) => [alpha, from, filter]), [[0.5, result, 'none']]);
  await assert.rejects(
    renderer.draw(output, { ...scene, layers: [{ ...rect, effects: { filters: [{ type: 'invert', amount: 1 }, { type: 'shader', id: 'ripple' }] } }] }),
    /custom shaders are not supported in the browser renderer/,
  );
  renderer.dispose();
  assert.equal(revokeURL.mock.callCount(), 1);
  assert.equal(faces.size, 0);

});

test('color filters change the layer\'s pixels before its other effects', async t => {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix };
  t.after(() => Object.assign(globalThis, originals));
  globalThis.DOMMatrix = class { translate() { return this; } rotate() { return this; } scale() { return this; } };
  const operations = [];
  let nextCanvas = 0;
  const canvas = () => {
    const element = { id: nextCanvas++, width: 4, height: 1, getContext() { return context; } };
    // Every pixel of a canvas a fill touches is red, opaque.
    let pixels = new Uint8ClampedArray(16);
    const context = {
      canvas: element, globalAlpha: 1, filter: 'none', globalCompositeOperation: 'source-over',
      save() {}, restore() {}, setTransform() {}, beginPath() {}, roundRect() {},
      fillRect() {},
      fill() { pixels = Uint8ClampedArray.from([255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0]); },
      getImageData(x, y, width, height) {
        operations.push(['read', element.id, x, y, width, height]);
        return { data: Uint8ClampedArray.from(pixels) };
      },
      putImageData(image, x, y, dirtyX, dirtyY, dirtyWidth, dirtyHeight) {
        operations.push(['write', element.id, Array.from(image.data.slice(0, 8)), dirtyX, dirtyY, dirtyWidth, dirtyHeight]);
      },
      drawImage(source) { operations.push(['composite', element.id, source.id, this.filter]); },
    };
    return element;
  };
  globalThis.document = { fonts: new Set(), createElement: canvas };
  const renderer = new SceneCanvas(new Map());
  const transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const rect = {
    opacity: 1, transform,
    effects: { blur: 1, filters: [{ type: 'invert', amount: 1 }, { type: 'brightness', amount: 0.5 }] },
    content: { type: 'rect', width: 2, height: 1, cornerRadius: 0, fill: { type: 'solid', color: '#f00' } },
  };
  const output = canvas();
  await renderer.draw(output, { width: 4, height: 1, layers: [rect] });
  const [read] = operations.filter(([kind]) => kind === 'read');
  const source = read[1];
  assert.deepEqual(read.slice(2), [0, 0, 4, 1]);
  // Inverted, then halved; only the layer's box is written back.
  assert.deepEqual(operations.find(([kind]) => kind === 'write'), ['write', source, [0, 128, 128, 255, 0, 128, 128, 255], 0, 0, 2, 1]);
  // The blur reads the filtered source.
  assert.ok(operations.some(([kind, , from, filter]) => kind === 'composite' && from === source && filter === 'blur(1px)'));
  renderer.dispose();
});

test('a masked group draws its children and its mask apart, then keeps the children where the mask is', async t => {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix };
  t.after(() => Object.assign(globalThis, originals));
  globalThis.DOMMatrix = class { translate() { return this; } rotate() { return this; } scale() { return this; } };
  const operations = [];
  let nextCanvas = 0;
  // The mask canvas reads back one opaque white pixel and one half-covered red one.
  const read = [255, 255, 255, 255, 255, 0, 0, 128];
  const canvas = () => {
    const element = { id: nextCanvas++, width: 2, height: 1, getContext() { return context; } };
    const context = {
      canvas: element, globalAlpha: 1, filter: 'none', globalCompositeOperation: 'source-over',
      save() {}, restore() {}, setTransform() {}, fillRect() {}, beginPath() {}, roundRect() {},
      clip() { operations.push(['clip', element.id]); },
      fill() { operations.push(['fill', element.id, this.globalAlpha, this.fillStyle]); },
      getImageData() { return { data: Uint8ClampedArray.from(read) }; },
      putImageData(image) { operations.push(['write', element.id, Array.from(image.data)]); },
      drawImage(source) { operations.push(['composite', element.id, this.globalAlpha, source.id, this.globalCompositeOperation]); },
    };
    return element;
  };
  globalThis.document = { fonts: new Set(), createElement: canvas };
  const renderer = new SceneCanvas(new Map());
  const transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const rect = color => ({ opacity: 1, transform, content: { type: 'rect', width: 1, height: 1, cornerRadius: 0, fill: { type: 'solid', color } } });
  const group = mask => ({
    opacity: 0.5, transform,
    content: { type: 'group', layers: [rect('#f00')], clip: { x: 0, y: 0, width: 1, height: 1, cornerRadius: 0 }, mask },
  });
  const output = canvas();
  const draw = async mask => {
    operations.length = 0;
    await renderer.draw(output, { width: 2, height: 1, layers: [group(mask)] });
    const child = operations.find(([kind, , , color]) => kind === 'fill' && color === '#f00');
    const matte = operations.find(([kind, , , color]) => kind === 'fill' && color === '#fff');
    return { child, matte };
  };

  const { child, matte } = await draw({ layers: [rect('#fff')] });
  // Both draw at full opacity; only the children are clipped.
  assert.equal(child[2], 1);
  assert.equal(matte[2], 1);
  assert.deepEqual(operations.filter(([kind]) => kind === 'clip').map(([, id]) => id), [child[1]]);
  assert.deepEqual(operations.filter(([kind, id]) => kind === 'composite' && id === child[1]), [['composite', child[1], 1, matte[1], 'destination-in']]);
  // The masked result reaches the output once, at the group's opacity.
  assert.deepEqual(operations.filter(([kind, id]) => kind === 'composite' && id === output.id), [['composite', output.id, 0.5, child[1], 'source-over']]);
  assert.ok(!operations.some(([kind]) => kind === 'write'));

  const inverted = await draw({ layers: [rect('#fff')], invert: true });
  assert.deepEqual(operations.filter(([kind, id]) => kind === 'composite' && id === inverted.child[1]).map(([, , , , mode]) => mode), ['destination-out']);

  // Luminance: Rec. 709 luma times alpha, written as the mask's alpha.
  const luminance = await draw({ layers: [rect('#fff')], mode: 'luminance' });
  assert.deepEqual(operations.filter(([kind]) => kind === 'write'), [['write', luminance.matte[1], [0, 0, 0, 255, 0, 0, 0, 27]]]);
  renderer.dispose();
});

test('a vignette spans the layer\'s shapes as the GPU renderer places them', async t => {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix, DOMPoint: globalThis.DOMPoint };
  t.after(() => Object.assign(globalThis, originals));
  // A 2D affine matrix, as DOMMatrix behaves for these calls.
  globalThis.DOMMatrix = class Matrix {
    constructor(a = 1, b = 0, c = 0, d = 1, e = 0, f = 0) { Object.assign(this, { a, b, c, d, e, f }); }
    multiply(m) {
      return new Matrix(this.a * m.a + this.c * m.b, this.b * m.a + this.d * m.b, this.a * m.c + this.c * m.d,
        this.b * m.c + this.d * m.d, this.a * m.e + this.c * m.f + this.e, this.b * m.e + this.d * m.f + this.f);
    }
    translate(x, y) { return this.multiply(new Matrix(1, 0, 0, 1, x, y)); }
    scale(x, y) { return this.multiply(new Matrix(x, 0, 0, y, 0, 0)); }
    rotate(degrees) {
      const r = degrees * Math.PI / 180;
      return this.multiply(new Matrix(Math.cos(r), Math.sin(r), -Math.sin(r), Math.cos(r), 0, 0));
    }
    transformPoint(p) { return { x: this.a * p.x + this.c * p.y + this.e, y: this.b * p.x + this.d * p.y + this.f }; }
  };
  globalThis.DOMPoint = class { constructor(x, y) { this.x = x; this.y = y; } };
  // Every character advances 10px; ink reaches 8 above the baseline and 2
  // below, the font box 9 and 3.
  const ctx = {
    canvas: { width: 200, height: 200 }, font: '', letterSpacing: '', save() {}, restore() {},
    measureText: text => ({
      width: 10 * Array.from(text).length, actualBoundingBoxLeft: 0, actualBoundingBoxRight: 10 * Array.from(text).length,
      actualBoundingBoxAscent: 8, actualBoundingBoxDescent: 2, fontBoundingBoxAscent: 9, fontBoundingBoxDescent: 3,
    }),
  };
  const renderer = new SceneCanvas(new Map());
  const at = (x, y, extra = {}) => ({ position: { x, y }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0, ...extra });
  const text = (value, transform, style = {}) => ({ opacity: 1, transform, content: { type: 'text', text: value, style: { fontSize: 10, ...style } } });
  const box = layer => renderer.shapeBox(ctx, layer, new DOMMatrix(), 1);

  // One line: its visible rows (1 to 11 below the line's top), grown by the
  // stroke, moved onto whole pixels.
  assert.deepEqual(await box(text('ABC', at(10.4, 20.3), { stroke: { width: 2, paint: { type: 'solid', color: '#000' } } })), [8, 18, 42, 32]);
  // Several lines: their line boxes, anchored at the centre.
  assert.deepEqual(await box(text('A\nBC', at(50, 50, { anchor: { x: 0.5, y: 0.5 } }))), [40, 38, 60, 62]);
  // A group: the union of its children's shapes; scaled text is not moved
  // onto whole pixels, and a child drawn at opacity 0 has no shape.
  const rect = { opacity: 1, transform: at(0, 0), content: { type: 'rect', width: 10, height: 10, cornerRadius: 0 } };
  const hidden = { ...rect, opacity: 0, transform: at(-50, -50) };
  const scaled = text('A', at(100.25, 0, { scale: { x: 2, y: 2 } }));
  assert.deepEqual(await box({ opacity: 1, transform: at(0, 0), content: { type: 'group', layers: [rect, hidden, scaled] } }), [0, 0, 120.25, 20]);
  // A masked group: where its children and its mask meet, or its children
  // alone when the mask is inverted, as the GPU plans it.
  const matte = { ...rect, transform: at(5, 5) };
  const masked = mask => ({ opacity: 1, transform: at(0, 0), content: { type: 'group', layers: [rect], mask } });
  assert.deepEqual(await box(masked({ layers: [matte] })), [5, 5, 10, 10]);
  assert.deepEqual(await box(masked({ layers: [matte], invert: true })), [0, 0, 10, 10]);
  assert.equal(await box(masked({ layers: [{ ...rect, transform: at(50, 50) }] })), null);
  assert.equal(await box(masked({ layers: [] })), null);
  // A path: its points, control points included, grown by half the stroke;
  // a path that does not start with moveTo draws from 0,0.
  const stroke = { width: 4, paint: { type: 'solid', color: '#fff' } };
  const path = (commands, transform = at(0.5, 0.5)) => ({ opacity: 1, transform, content: { type: 'path', commands, stroke } });
  assert.deepEqual(await box(path([{ type: 'moveTo', x: 10, y: 10 }, { type: 'lineTo', x: 50, y: 10 }, { type: 'lineTo', x: 50, y: 40 }], at(0, 0))), [8, 8, 52, 42]);
  assert.deepEqual(await box(path([{ type: 'quadTo', x1: 20, y1: -10, x: 30, y: 5 }])), [-1.5, -11.5, 32.5, 7.5]);
  assert.equal(await box(path([{ type: 'moveTo', x: 10, y: 10 }])), null);
  // A rotated rect: the box around its corners.
  const turned = await box({ ...rect, transform: at(0, 0, { rotation: 90 }) });
  assert.deepEqual(turned.map(value => Math.round(value * 1e9) / 1e9), [-10, 0, 0, 10]);
});

test('a dashed path is stroked with the dashes the native renderers draw', async t => {
  const originals = { DOMMatrix: globalThis.DOMMatrix };
  t.after(() => Object.assign(globalThis, originals));
  globalThis.DOMMatrix = class { translate() { return this; } rotate() { return this; } scale() { return this; } };
  const strokes = [];
  const ctx = {
    save() {}, restore() {}, setTransform() {}, beginPath() {}, moveTo() {}, lineTo() {},
    lineDash: [], lineDashOffset: 0,
    setLineDash(pattern) { this.lineDash = pattern; },
    stroke() { strokes.push({ pattern: this.lineDash, offset: this.lineDashOffset, cap: this.lineCap }); },
  };
  const renderer = new SceneCanvas(new Map());
  const transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const line = (extra) => ({
    opacity: 1, transform,
    content: { type: 'path', commands: [{ type: 'moveTo', x: 0, y: 0 }, { type: 'lineTo', x: 100, y: 0 }], stroke: { width: 4, paint: { type: 'solid', color: '#fff' } }, ...extra },
  });
  for (const lineDash of [
    { pattern: [12, 6], offset: 3 },
    // An odd count repeats, as SVG and the native renderers do.
    { pattern: [5, 2, 1] },
    // An offset many repeats on, or before the start, within one repeat.
    { pattern: [12, 6], offset: 180_003 },
    { pattern: [12, 6], offset: -15 },
    // All zero is solid; dots are zero-length dashes with round caps.
    { pattern: [0, 0], offset: 4 },
    { pattern: [0, 8] },
  ]) {
    await renderer.layer(ctx, line({ lineDash, lineCap: 'round' }), new DOMMatrix(), 1);
  }
  // A path without dashes is solid, whatever the last one drew.
  await renderer.layer(ctx, line({}), new DOMMatrix(), 1);
  assert.deepEqual(strokes, [
    { pattern: [12, 6], offset: 3, cap: 'round' },
    { pattern: [5, 2, 1, 5, 2, 1], offset: 0, cap: 'round' },
    { pattern: [12, 6], offset: 3, cap: 'round' },
    { pattern: [12, 6], offset: 3, cap: 'round' },
    { pattern: [], offset: 0, cap: 'round' },
    { pattern: [0, 8], offset: 0, cap: 'round' },
    { pattern: [], offset: 0, cap: 'butt' },
  ]);
});

test('text wrapped at maxWidth is anchored by its line boxes, as text broken with newlines', async t => {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix };
  t.after(() => Object.assign(globalThis, originals));
  globalThis.DOMMatrix = class { translate() { return this; } rotate() { return this; } scale() { return this; } };
  const drawn = [];
  const canvas = () => {
    const element = { width: 400, height: 400, getContext() { return context; } };
    const context = {
      canvas: element, globalAlpha: 1, filter: 'none', globalCompositeOperation: 'source-over',
      save() {}, restore() {}, setTransform() {}, fillRect() {}, drawImage() {},
      // Each character is 10 px wide; the font's line box is 30 px above
      // the baseline and 10 below, the ink 20 above and none below.
      measureText: text => ({
        width: text.length * 10, actualBoundingBoxLeft: 0, actualBoundingBoxRight: text.length * 10,
        actualBoundingBoxAscent: 20, actualBoundingBoxDescent: 0, fontBoundingBoxAscent: 30, fontBoundingBoxDescent: 10,
      }),
      fillText(text, x, y) { drawn.push([text, y]); },
    };
    return element;
  };
  globalThis.document = { fonts: new Set(), createElement: canvas };
  const renderer = new SceneCanvas(new Map());
  const transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const layer = text => ({ opacity: 1, transform, content: { type: 'text', text, style: { lineHeight: 40 }, maxWidth: 50 } });
  const baselines = async text => {
    drawn.length = 0;
    await renderer.draw(canvas(), { width: 400, height: 400, fonts: [], layers: [layer(text)] });
    return drawn.slice();
  };
  const broken = await baselines('abc\ndef');
  assert.deepEqual(broken, [['abc', 30], ['def', 70]]);
  assert.deepEqual(await baselines('abc def'), broken);
  renderer.dispose();
});
