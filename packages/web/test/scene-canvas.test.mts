import assert from 'node:assert/strict';
import { onTestFinished, test, vi } from 'vitest';
import { SceneCanvas } from '../src/scene-canvas';
import { homography, planeBands, planeRaster, stretchAt } from '../src/projection';
import type { Layer, LayerContent, PathCommand, Stroke, TextStyle, Transform } from '../src/types';
import { still } from './scenes.mjs';
import { fetchStub, standIn } from './stand-in.mjs';

/** The operations of `kind` in a log of calls, each recorded as `[kind, ...details]`. */
function ofKind<O extends readonly [string, ...unknown[]], K extends O[0]>(operations: readonly O[], kind: K) {
  return operations.filter((operation): operation is Extract<O, readonly [K, ...unknown[]]> => operation[0] === kind);
}

test('file font URLs are reused across frames and isolated opacity is applied once', async () => {
  // `[kind, canvas, alpha, ...]`: a fill, or a composite of `[source, filter, mode, x, y]`.
  const faces = new Set<object>(), operations: [kind: string, id: number, alpha: number, ...details: unknown[]][] = [];
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix, FontFace: globalThis.FontFace, fetch: globalThis.fetch };
  onTestFinished(() => { Object.assign(globalThis, originals); });
  const createURL = vi.spyOn(URL, 'createObjectURL').mockImplementation(() => 'blob:font');
  const revokeURL = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  globalThis.FontFace = standIn(class { async load() { return this; } });
  const fetchFont = vi.spyOn(globalThis, 'fetch').mockImplementation(fetchStub(async () => new Response('@font-face {font-family: Demo; src: url(https://fonts.test/font.woff)}', { headers: { 'content-type': 'text/css' } })));
  globalThis.DOMMatrix = standIn(class { translate() { return this; } rotate() { return this; } scale() { return this; } });
  let nextCanvas = 0;
  const canvas = () => {
    const element = { id: nextCanvas++, width: 10, height: 10, getContext() { return context; } };
    const stack: { globalAlpha: number; filter: string; globalCompositeOperation: string }[] = [];
    const context = {
      canvas: element, globalAlpha: 1, filter: 'none', globalCompositeOperation: 'source-over',
      save() { stack.push({ globalAlpha: this.globalAlpha, filter: this.filter, globalCompositeOperation: this.globalCompositeOperation }); },
      restore() { Object.assign(this, stack.pop()); }, setTransform() {}, fillRect() {}, beginPath() {}, roundRect() {},
      fill() { operations.push(['fill', element.id, this.globalAlpha]); },
      drawImage(source: { id: number }, x: number, y: number) { operations.push(['composite', element.id, this.globalAlpha, source.id, this.filter, this.globalCompositeOperation, x, y]); },
    };
    return element;
  };
  globalThis.document = standIn({ fonts: faces, createElement: canvas });
  const renderer = new SceneCanvas(new Map([['font.css', new File(['font'], 'font.css')]]));
  const transform: Transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const rect: Layer = { id: 'rect', opacity: 1, transform, content: { type: 'rect', width: 10, height: 10, cornerRadius: 0, fill: { type: 'solid', color: '#fff' } } };
  const group: Layer = { id: 'group', opacity: 0.5, transform, effects: { glow: { blur: 1, color: '#fff' } }, content: { type: 'group', layers: [rect] } };
  const scene = still({ width: 10, height: 10, fonts: [{ id: 'font', location: { type: 'file', path: 'font.css' } }], layers: [group] });
  const output = canvas();
  renderer['styledText'].set('fallback', standIn({}));
  renderer['textLayouts'].set('fallback', standIn({}));
  await renderer.draw(standIn(output), scene);
  assert.equal(renderer['styledText'].size, 0, 'font changes invalidate painted text');
  assert.equal(renderer['textLayouts'].size, 0, 'font changes invalidate shaped text');
  renderer['textLayouts'].set('loaded-font', standIn({}));
  await renderer.draw(standIn(output), scene);
  assert.equal(renderer['textLayouts'].size, 1, 'unchanged fonts retain the layout cache');
  assert.equal(createURL.mock.calls.length, 1);
  // The stylesheet and the font file it points to, each once.
  assert.equal(fetchFont.mock.calls.length, 2);
  assert.equal(faces.size, 1);
  assert.deepEqual(operations.filter(([kind, id]) => kind === 'fill' || id === output.id).map(([kind, , alpha]) => [kind, alpha]), [['fill', 1], ['composite', 0.5], ['fill', 1], ['composite', 0.5]]);

  operations.length = 0;
  const effects = { blur: 2, shadow: { color: '#0008', blur: 4, offsetX: 3, offsetY: 5 }, glow: { color: '#f008', blur: 6 } };
  await renderer.draw(standIn(output), { ...scene, layers: [{ ...rect, opacity: 0.5, effects }] });
  const source = operations.find(([kind]) => kind === 'fill')![1];
  const composites = operations.filter(([kind]) => kind === 'composite');
  const masks = composites.filter(([, , , from, filter]) => from === source && filter === 'none');
  assert.equal(masks.length, 2, 'shadow and glow independently read the original source');
  const base = composites.find(([, , , from, filter]) => from === source && filter === 'blur(2px)')!;
  const result = base[1];
  assert.deepEqual(composites.filter(([, id]) => id === result).map(([, , alpha, , filter, mode, x, y]) => ({ alpha, filter, mode, x, y })), [
    { alpha: 1, filter: 'blur(4px)', mode: 'source-over', x: 3, y: 5 },
    { alpha: 1, filter: 'blur(6px)', mode: 'source-over', x: 0, y: 0 },
    { alpha: 1, filter: 'blur(2px)', mode: 'source-over', x: 0, y: 0 },
  ]);
  assert.deepEqual(composites.filter(([, id]) => id === output.id).map(([, , alpha, from, filter]) => [alpha, from, filter]), [[0.5, result, 'none']]);
  await assert.rejects(
    renderer.draw(standIn(output), { ...scene, layers: [{ ...rect, effects: { filters: [{ type: 'invert', amount: 1 }, { type: 'shader', id: 'ripple' }] } }] }),
    /custom shaders are not supported in the browser renderer/,
  );
  renderer.dispose();
  assert.equal(revokeURL.mock.calls.length, 1);
  assert.equal(faces.size, 0);

});

test('color filters change the layer\'s pixels before its other effects', async () => {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix };
  onTestFinished(() => { Object.assign(globalThis, originals); });
  globalThis.DOMMatrix = standIn(class { translate() { return this; } rotate() { return this; } scale() { return this; } });
  // `[kind, canvas, ...]`: a read of `[x, y, width, height]`, a write of
  // `[pixels, ...dirty]`, or a composite of `[source, filter]`.
  const operations: [kind: string, id: number, ...details: unknown[]][] = [];
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
      getImageData(x: number, y: number, width: number, height: number) {
        operations.push(['read', element.id, x, y, width, height]);
        return { data: Uint8ClampedArray.from(pixels) };
      },
      putImageData(image: ImageData, x: number, y: number, dirtyX?: number, dirtyY?: number, dirtyWidth?: number, dirtyHeight?: number) {
        operations.push(['write', element.id, Array.from(image.data.slice(0, 8)), dirtyX, dirtyY, dirtyWidth, dirtyHeight]);
      },
      drawImage(source: { id: number }) { operations.push(['composite', element.id, source.id, this.filter]); },
    };
    return element;
  };
  globalThis.document = standIn({ fonts: new Set(), createElement: canvas });
  const renderer = new SceneCanvas(new Map());
  const transform: Transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const rect: Layer = {
    id: 'rect', opacity: 1, transform,
    effects: { blur: 1, filters: [{ type: 'invert', amount: 1 }, { type: 'brightness', amount: 0.5 }] },
    content: { type: 'rect', width: 2, height: 1, cornerRadius: 0, fill: { type: 'solid', color: '#f00' } },
  };
  const output = canvas();
  await renderer.draw(standIn(output), still({ width: 4, height: 1, layers: [rect] }));
  const [read] = operations.filter(([kind]) => kind === 'read');
  const source = read[1];
  assert.deepEqual(read.slice(2), [0, 0, 4, 1]);
  // Inverted, then halved; only what the layer covers is written back,
  // 2 pixels wider as the GPU reckons it.
  assert.deepEqual(operations.find(([kind]) => kind === 'write'), ['write', source, [0, 128, 128, 255, 0, 128, 128, 255], 0, 0, 4, 1]);
  // The blur reads the filtered source.
  assert.ok(operations.some(([kind, , from, filter]) => kind === 'composite' && from === source && filter === 'blur(1px)'));
  renderer.dispose();
});

test('a blended layer takes the blended colors, then composites source-over inside its clip', async () => {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix };
  onTestFinished(() => { Object.assign(globalThis, originals); });
  globalThis.DOMMatrix = standIn(class { translate() { return this; } rotate() { return this; } scale() { return this; } });
  // `[kind, canvas, ...]`: a read of `[x, y, width, height]`, a write of
  // `[pixels, ...dirty]`, or a composite of `[source, alpha, mode, clipped]`.
  const operations: [kind: string, id: number, ...details: unknown[]][] = [];
  let nextCanvas = 0;
  const rgba = (hex: string) => [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16)).concat(255);
  const canvas = () => {
    const element = { id: nextCanvas++, width: 2, height: 1, getContext() { return context; } };
    // `fillRect` (the background) paints every pixel, `fill` (the rect) the first.
    const pixels = new Uint8ClampedArray(8);
    const stack: { globalAlpha: number; globalCompositeOperation: string; clipped: boolean }[] = [];
    const context = {
      canvas: element, globalAlpha: 1, filter: 'none', globalCompositeOperation: 'source-over', clipped: false, fillStyle: '',
      save() { stack.push({ globalAlpha: this.globalAlpha, globalCompositeOperation: this.globalCompositeOperation, clipped: this.clipped }); },
      restore() { Object.assign(this, stack.pop()); }, setTransform() {}, beginPath() {}, roundRect() {},
      clip() { this.clipped = true; },
      fillRect() { pixels.set(rgba(this.fillStyle), 0); pixels.set(rgba(this.fillStyle), 4); },
      fill() { pixels.set(rgba(this.fillStyle), 0); },
      getImageData(x: number, y: number, width: number, height: number) {
        operations.push(['read', element.id, x, y, width, height]);
        return { data: pixels.slice((y * element.width + x) * 4, (y * element.width + x + width) * 4) };
      },
      putImageData(image: ImageData, x: number, y: number, dirtyX?: number, dirtyY?: number, dirtyWidth?: number, dirtyHeight?: number) {
        operations.push(['write', element.id, Array.from(image.data), dirtyX, dirtyY, dirtyWidth, dirtyHeight]);
      },
      drawImage(source: { id: number }) {
        operations.push(['composite', element.id, source.id, this.globalAlpha, this.globalCompositeOperation, this.clipped]);
      },
    };
    return element;
  };
  globalThis.document = standIn({ fonts: new Set(), createElement: canvas });
  const renderer = new SceneCanvas(new Map());
  const transform: Transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const rect: Layer = {
    id: 'rect', opacity: 0.5, transform, blendMode: 'screen',
    content: { type: 'rect', width: 1, height: 1, cornerRadius: 0, fill: { type: 'solid', color: '#808040' } },
  };
  const clip = { x: 0, y: 0, width: 1, height: 1, cornerRadius: 0 };
  const output = canvas();
  await renderer.draw(standIn(output), still({ width: 2, height: 1, layers: [{ id: 'group', opacity: 1, transform, content: { type: 'group', clip, layers: [rect] } }] }));

  const [write] = operations.filter(([kind]) => kind === 'write');
  const source = write[1];
  // The backdrop is read under the rect alone.
  assert.deepEqual(operations.find(([kind, id]) => kind === 'read' && id === output.id), ['read', output.id, 0, 0, 1, 1]);
  // Screen of the rect over the #14161c background, at the backdrop's full alpha.
  const screened = [0x80, 0x80, 0x40].map((s, i) => Math.round(255 * (1 - (1 - s / 255) * (1 - [0x14, 0x16, 0x1c][i] / 255))));
  assert.deepEqual(write.slice(2), [[...screened, 255, 0, 0, 0, 0], 0, 0, 1, 1]);
  // Its opacity applies once, as it is drawn over the backdrop, and within the group's clip.
  assert.deepEqual(operations.filter(([kind, id]) => kind === 'composite' && id === output.id), [
    ['composite', output.id, source, 0.5, 'source-over', true],
  ]);
  // Not a mode: kebab-case, as CSS spells it.
  await assert.rejects(renderer.draw(standIn(output), still({ width: 2, height: 1, layers: [{ ...rect, blendMode: 'color-dodge' as Layer['blendMode'] }] })), /Unsupported blend mode: color-dodge/);
  renderer.dispose();
});

test('a masked group draws its children and its mask apart, then keeps the children where the mask is', async () => {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix };
  onTestFinished(() => { Object.assign(globalThis, originals); });
  globalThis.DOMMatrix = standIn(class { translate() { return this; } rotate() { return this; } scale() { return this; } });
  // `[kind, canvas, ...]`: a clip, a fill of `[alpha, color]`, a write of
  // `[pixels]`, or a composite of `[alpha, source, mode]`.
  const operations: [kind: string, id: number, ...details: unknown[]][] = [];
  let nextCanvas = 0;
  // The mask canvas reads back one opaque white pixel and one half-covered red one.
  const read = [255, 255, 255, 255, 255, 0, 0, 128];
  const canvas = () => {
    const element = { id: nextCanvas++, width: 2, height: 1, getContext() { return context; } };
    const context = {
      canvas: element, globalAlpha: 1, filter: 'none', globalCompositeOperation: 'source-over', fillStyle: '',
      save() {}, restore() {}, setTransform() {}, fillRect() {}, beginPath() {}, roundRect() {},
      clip() { operations.push(['clip', element.id]); },
      fill() { operations.push(['fill', element.id, this.globalAlpha, this.fillStyle]); },
      getImageData() { return { data: Uint8ClampedArray.from(read) }; },
      putImageData(image: ImageData) { operations.push(['write', element.id, Array.from(image.data)]); },
      drawImage(source: { id: number }) { operations.push(['composite', element.id, this.globalAlpha, source.id, this.globalCompositeOperation]); },
    };
    return element;
  };
  globalThis.document = standIn({ fonts: new Set(), createElement: canvas });
  const renderer = new SceneCanvas(new Map());
  const transform: Transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const rect = (color: string): Layer => ({ id: color, opacity: 1, transform, content: { type: 'rect', width: 1, height: 1, cornerRadius: 0, fill: { type: 'solid', color } } });
  type Group = Extract<LayerContent, { type: 'group' }>;
  const group = (mask: Group['mask']): Layer => ({
    id: 'group', opacity: 0.5, transform,
    content: { type: 'group', layers: [rect('#f00')], clip: { x: 0, y: 0, width: 1, height: 1, cornerRadius: 0 }, mask },
  });
  const output = canvas();
  const draw = async (mask: Group['mask']) => {
    operations.length = 0;
    await renderer.draw(standIn(output), still({ width: 2, height: 1, layers: [group(mask)] }));
    const child = operations.find(([kind, , , color]) => kind === 'fill' && color === '#f00')!;
    const matte = operations.find(([kind, , , color]) => kind === 'fill' && color === '#fff')!;
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

test('a translucent group draws its children onto a canvas of its own and fades that once', async () => {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix };
  onTestFinished(() => { Object.assign(globalThis, originals); });
  globalThis.DOMMatrix = standIn(class { translate() { return this; } rotate() { return this; } scale() { return this; } });
  // `[kind, canvas, alpha, ...]`: a fill of `[color]`, or a composite of `[source, mode]`.
  const operations: (['fill', number, number, string] | ['composite', number, number, number, string])[] = [];
  let nextCanvas = 0;
  const canvas = () => {
    const element = { id: nextCanvas++, width: 2, height: 1, getContext() { return context; } };
    const context = {
      canvas: element, globalAlpha: 1, filter: 'none', globalCompositeOperation: 'source-over', fillStyle: '',
      save() {}, restore() {}, setTransform() {}, fillRect() {}, beginPath() {}, roundRect() {},
      fill() { operations.push(['fill', element.id, this.globalAlpha, this.fillStyle]); },
      // Blending reads pixels back; every pixel reads as opaque.
      getImageData(x: number, y: number, width: number, height: number) { return { data: new Uint8ClampedArray(width * height * 4).fill(255) }; },
      putImageData() {},
      drawImage(source: { id: number }) { operations.push(['composite', element.id, this.globalAlpha, source.id, this.globalCompositeOperation]); },
    };
    return element;
  };
  globalThis.document = standIn({ fonts: new Set(), createElement: canvas });
  const renderer = new SceneCanvas(new Map());
  const transform: Transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const rect = (color: string, blendMode?: Layer['blendMode']): Layer => ({ id: color, opacity: 1, transform, blendMode, content: { type: 'rect', width: 1, height: 1, cornerRadius: 0, fill: { type: 'solid', color } } });
  const output = canvas();
  const draw = async (layers: Layer[]) => {
    operations.length = 0;
    await renderer.draw(standIn(output), still({ width: 2, height: 1, layers: [{ id: 'group', opacity: 0.5, transform, content: { type: 'group', layers } }] }));
  };

  // Overlapping children draw opaque onto the group's canvas, which reaches
  // the output once, at the group's opacity.
  await draw([rect('#fff'), rect('#f00')]);
  const fills = ofKind(operations, 'fill');
  assert.deepEqual(fills.map(([, id, alpha, color]) => [alpha, color]), [[1, '#fff'], [1, '#f00']]);
  assert.notEqual(fills[0][1], output.id);
  assert.deepEqual(operations.filter(([kind, id]) => kind === 'composite' && id === output.id), [['composite', output.id, 0.5, fills[0][1], 'source-over']]);

  // A lone shape just takes the group's opacity.
  await draw([rect('#f00')]);
  assert.deepEqual(operations, [['fill', output.id, 0.5, '#f00']]);

  // A lone blended child blends with the group's own canvas.
  await draw([rect('#f00', 'multiply')]);
  const [fill] = ofKind(operations, 'fill');
  const [group] = ofKind(operations, 'composite').filter(([, id]) => id === output.id);
  assert.deepEqual(group.slice(2), [0.5, group[3], 'source-over']);
  assert.ok(![output.id, fill[1]].includes(group[3]));
  assert.deepEqual(operations.filter(([kind, id]) => kind === 'composite' && id === group[3]), [['composite', group[3], 1, fill[1], 'source-over']]);
  renderer.dispose();
});

test('a vignette spans the layer\'s shapes as the GPU renderer places them', async () => {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix, DOMPoint: globalThis.DOMPoint };
  onTestFinished(() => { Object.assign(globalThis, originals); });
  // A 2D affine matrix, as DOMMatrix behaves for these calls.
  globalThis.DOMMatrix = standIn(class Matrix {
    constructor(readonly a = 1, readonly b = 0, readonly c = 0, readonly d = 1, readonly e = 0, readonly f = 0) {}
    multiply(m: Matrix): Matrix {
      return new Matrix(this.a * m.a + this.c * m.b, this.b * m.a + this.d * m.b, this.a * m.c + this.c * m.d,
        this.b * m.c + this.d * m.d, this.a * m.e + this.c * m.f + this.e, this.b * m.e + this.d * m.f + this.f);
    }
    translate(x: number, y: number) { return this.multiply(new Matrix(1, 0, 0, 1, x, y)); }
    scale(x: number, y: number) { return this.multiply(new Matrix(x, 0, 0, y, 0, 0)); }
    rotate(degrees: number) {
      const r = degrees * Math.PI / 180;
      return this.multiply(new Matrix(Math.cos(r), Math.sin(r), -Math.sin(r), Math.cos(r), 0, 0));
    }
    transformPoint(p: DOMPointInit) { return { x: this.a * p.x! + this.c * p.y! + this.e, y: this.b * p.x! + this.d * p.y! + this.f }; }
  });
  globalThis.DOMPoint = standIn(class { constructor(readonly x: number, readonly y: number) {} });
  // Every character advances 10px; ink reaches 8 above the baseline and 2
  // below, the font box 9 and 3.
  const ctx = standIn<CanvasRenderingContext2D>({
    canvas: { width: 200, height: 200 }, font: '', letterSpacing: '', save() {}, restore() {},
    measureText: (text: string) => ({
      width: 10 * Array.from(text).length, actualBoundingBoxLeft: 0, actualBoundingBoxRight: 10 * Array.from(text).length,
      actualBoundingBoxAscent: 8, actualBoundingBoxDescent: 2, fontBoundingBoxAscent: 9, fontBoundingBoxDescent: 3,
    }),
  });
  const renderer = new SceneCanvas(new Map());
  const at = (x: number, y: number, extra: Partial<Transform> = {}): Transform => ({ position: { x, y }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0, ...extra });
  const text = (value: string, transform: Transform, style: TextStyle = {}): Layer => ({ id: value, opacity: 1, transform, content: { type: 'text', text: value, style: { fontSize: 10, ...style } } });
  const box = (layer: Layer) => renderer['shapeBox'](ctx, layer, new DOMMatrix(), 1);

  // One line: its visible rows (1 to 11 below the line's top), grown by the
  // stroke, moved onto whole pixels.
  assert.deepEqual(await box(text('ABC', at(10.4, 20.3), { stroke: { width: 2, paint: { type: 'solid', color: '#000' } } })), [8, 18, 42, 32]);
  // A list of strokes, by its widest, in any order.
  const strokes: Stroke[] = [1, 2].map(width => ({ width, paint: { type: 'solid', color: '#000' } }));
  assert.deepEqual(await box(text('ABC', at(10.4, 20.3), { stroke: strokes })), [8, 18, 42, 32]);
  assert.deepEqual(await box(text('ABC', at(10.4, 20.3), { stroke: strokes.toReversed() })), [8, 18, 42, 32]);
  // Several lines: their line boxes, anchored at the centre.
  assert.deepEqual(await box(text('A\nBC', at(50, 50, { anchor: { x: 0.5, y: 0.5 } }))), [40, 38, 60, 62]);
  // A group: the union of its children's shapes; scaled text is not moved
  // onto whole pixels, and a child drawn at opacity 0 has no shape.
  const rect: Layer = { id: 'rect', opacity: 1, transform: at(0, 0), content: { type: 'rect', width: 10, height: 10, cornerRadius: 0 } };
  const hidden: Layer = { ...rect, opacity: 0, transform: at(-50, -50) };
  const scaled = text('A', at(100.25, 0, { scale: { x: 2, y: 2 } }));
  assert.deepEqual(await box({ id: 'group', opacity: 1, transform: at(0, 0), content: { type: 'group', layers: [rect, hidden, scaled] } }), [0, 0, 120.25, 20]);
  // A path: its points, control points included, grown by half the stroke;
  // a path that does not start with moveTo draws from 0,0.
  const stroke: Stroke = { width: 4, paint: { type: 'solid', color: '#fff' } };
  const path = (commands: PathCommand[], transform = at(0.5, 0.5)): Layer => ({ id: 'path', opacity: 1, transform, content: { type: 'path', commands, stroke } });
  assert.deepEqual(await box(path([{ type: 'moveTo', x: 10, y: 10 }, { type: 'lineTo', x: 50, y: 10 }, { type: 'lineTo', x: 50, y: 40 }], at(0, 0))), [8, 8, 52, 42]);
  assert.deepEqual(await box(path([{ type: 'quadTo', x1: 20, y1: -10, x: 30, y: 5 }])), [-1.5, -11.5, 32.5, 7.5]);
  assert.equal(await box(path([{ type: 'moveTo', x: 10, y: 10 }])), null);
  // A rotated rect: the box around its corners.
  const turned = (await box({ ...rect, transform: at(0, 0, { rotation: 90 }) }))!;
  assert.deepEqual(turned.map(value => Math.round(value * 1e9) / 1e9), [-10, 0, 0, 10]);
});

test('a dashed path is stroked with the dashes the native renderers draw', async () => {
  const originals = { DOMMatrix: globalThis.DOMMatrix };
  onTestFinished(() => { Object.assign(globalThis, originals); });
  globalThis.DOMMatrix = standIn(class { translate() { return this; } rotate() { return this; } scale() { return this; } });
  const strokes: { pattern: number[]; offset: number; cap: string }[] = [];
  const ctx = {
    save() {}, restore() {}, setTransform() {}, beginPath() {}, moveTo() {}, lineTo() {},
    lineDash: [] as number[], lineDashOffset: 0, lineCap: '',
    setLineDash(pattern: number[]) { this.lineDash = pattern; },
    stroke() { strokes.push({ pattern: this.lineDash, offset: this.lineDashOffset, cap: this.lineCap }); },
  };
  const renderer = new SceneCanvas(new Map());
  const transform: Transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const line = (extra: Partial<Extract<LayerContent, { type: 'path' }>>): Layer => ({
    id: 'line', opacity: 1, transform,
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
    await renderer['layer'](standIn(ctx), line({ lineDash, lineCap: 'round' }), new DOMMatrix(), 1);
  }
  // A path without dashes is solid, whatever the last one drew.
  await renderer['layer'](standIn(ctx), line({}), new DOMMatrix(), 1);
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

test('text wrapped at maxWidth is anchored by its line boxes, as text broken with newlines', async () => {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix };
  onTestFinished(() => { Object.assign(globalThis, originals); });
  globalThis.DOMMatrix = standIn(class { translate() { return this; } rotate() { return this; } scale() { return this; } });
  const drawn: [string, number][] = [];
  const canvas = () => {
    const element = { width: 400, height: 400, getContext() { return context; } };
    const context = {
      canvas: element, globalAlpha: 1, filter: 'none', globalCompositeOperation: 'source-over',
      save() {}, restore() {}, setTransform() {}, fillRect() {}, drawImage() {},
      // Each character is 10 px wide; the font's line box is 30 px above
      // the baseline and 10 below, the ink 20 above and none below.
      measureText: (text: string) => ({
        width: text.length * 10, actualBoundingBoxLeft: 0, actualBoundingBoxRight: text.length * 10,
        actualBoundingBoxAscent: 20, actualBoundingBoxDescent: 0, fontBoundingBoxAscent: 30, fontBoundingBoxDescent: 10,
      }),
      fillText(text: string, x: number, y: number) { drawn.push([text, y]); },
    };
    return element;
  };
  globalThis.document = standIn({ fonts: new Set(), createElement: canvas });
  const renderer = new SceneCanvas(new Map());
  const transform: Transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const layer = (text: string): Layer => ({ id: 'text', opacity: 1, transform, content: { type: 'text', text, style: { lineHeight: 40 }, maxWidth: 50 } });
  const baselines = async (text: string) => {
    drawn.length = 0;
    await renderer.draw(standIn(canvas()), still({ width: 400, height: 400, fonts: [], layers: [layer(text)] }));
    return drawn.slice();
  };
  const broken = await baselines('abc\ndef');
  assert.deepEqual(broken, [['abc', 30], ['def', 70]]);
  assert.deepEqual(await baselines('abc def'), broken);
  renderer.dispose();
});

// The same sizes as the native renderers' `sizes_a_video_frame_like_an_image`.
test('sizes a video like an image', async () => {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix };
  onTestFinished(() => { Object.assign(globalThis, originals); });
  globalThis.DOMMatrix = standIn(class { translate() { return this; } rotate() { return this; } scale() { return this; } });
  const drawn: [number, number][] = [];
  const canvas = () => {
    const element = { width: 320, height: 180, getContext() { return context; } };
    const context = {
      canvas: element, globalAlpha: 1, filter: 'none', globalCompositeOperation: 'source-over',
      save() {}, restore() {}, setTransform() {}, fillRect() {}, beginPath() {}, rect() {}, clip() {},
      drawImage(_source: unknown, _x: number, _y: number, width: number, height: number) { drawn.push([width, height]); },
    };
    return element;
  };
  // A 320 x 180 video, loaded once its source is set, at the time asked for.
  const video = () => ({
    videoWidth: 320, videoHeight: 180, currentTime: 0, duration: 1, onloadeddata: () => {},
    set src(_url: string) { queueMicrotask(() => this.onloadeddata()); },
    pause() {}, removeAttribute() {}, load() {},
  });
  globalThis.document = standIn({ fonts: new Set(), createElement: (tag: string) => tag === 'video' ? video() : canvas() });
  const renderer = new SceneCanvas(new Map(), 'https://media.test/');
  const transform: Transform = { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, anchor: { x: 0, y: 0 }, rotation: 0 };
  const size = async (width?: number, height?: number) => {
    drawn.length = 0;
    const content: LayerContent = {
      type: 'video', asset: { id: 'clip.mp4', location: { type: 'file', path: 'clip.mp4' } },
      timing: { sourceTimeSeconds: 0 }, ...(width === undefined ? {} : { width }), ...(height === undefined ? {} : { height }),
    };
    await renderer.draw(standIn(canvas()), still({ width: 320, height: 180, fonts: [], layers: [{ id: 'video', opacity: 1, transform, content }] }));
    return drawn[0];
  };
  assert.deepEqual(await size(160), [160, 90]);
  assert.deepEqual(await size(160, 160), [160, 160]);
  assert.deepEqual(await size(undefined, 90), [160, 90]);
  assert.deepEqual(await size(), [320, 180]);
  renderer.dispose();
});

// A 2D affine matrix, as DOMMatrix behaves for the calls the renderer makes.
class AffineMatrix {
  a: number; b: number; c: number; d: number; e: number; f: number;
  constructor(init = [1, 0, 0, 1, 0, 0]) { [this.a, this.b, this.c, this.d, this.e, this.f] = init; }
  multiply(m: AffineMatrix): AffineMatrix {
    // `constructor` is typed `Function`; it is this class or one extending it.
    return new (this.constructor as typeof AffineMatrix)([this.a * m.a + this.c * m.b, this.b * m.a + this.d * m.b, this.a * m.c + this.c * m.d,
      this.b * m.c + this.d * m.d, this.a * m.e + this.c * m.f + this.e, this.b * m.e + this.d * m.f + this.f]);
  }
  translate(x: number, y: number) { return this.multiply(new AffineMatrix([1, 0, 0, 1, x, y])); }
  scale(x: number, y: number) { return this.multiply(new AffineMatrix([x, 0, 0, y, 0, 0])); }
  rotate(degrees: number) {
    const r = degrees * Math.PI / 180;
    return this.multiply(new AffineMatrix([Math.cos(r), Math.sin(r), -Math.sin(r), Math.cos(r), 0, 0]));
  }
  transformPoint(p: { x: number; y: number }) { return { x: this.a * p.x + this.c * p.y + this.e, y: this.b * p.x + this.d * p.y + this.f }; }
}

test('a hidden backface draws nothing when a parent group turns it away', async () => {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix };
  onTestFinished(() => { Object.assign(globalThis, originals); });
  globalThis.DOMMatrix = standIn(AffineMatrix);
  const fills: string[] = [];
  const canvas = () => {
    const element = { width: 10, height: 10, getContext() { return context; } };
    const context = {
      canvas: element, globalAlpha: 1, filter: 'none', globalCompositeOperation: 'source-over', fillStyle: '',
      save() {}, restore() {}, setTransform() {}, fillRect() {}, beginPath() {}, roundRect() {}, drawImage() {},
      fill() { fills.push(this.fillStyle); },
    };
    return element;
  };
  globalThis.document = standIn({ fonts: new Set(), createElement: canvas });
  const renderer = new SceneCanvas(new Map());
  const at = (extra: Partial<Transform> = {}): Transform => ({ position: { x: 5, y: 5 }, scale: { x: 1, y: 1 }, anchor: { x: 0.5, y: 0.5 }, rotation: 0, ...extra });
  const rect = (color: string, extra?: Partial<Transform>): Layer => ({ id: color, opacity: 1, transform: at(extra), content: { type: 'rect', width: 4, height: 4, cornerRadius: 0, fill: { type: 'solid', color } } });
  const group = (rotationY: number): Layer => ({
    id: 'group', opacity: 1, transform: at({ rotationY }),
    content: { type: 'group', layers: [rect('#f00', { backfaceVisibility: 'hidden' }), rect('#0f0')] },
  });
  await renderer.draw(standIn(canvas()), still({ width: 10, height: 10, layers: [group(180)] }));
  assert.deepEqual(fills, ['#0f0'], 'only the face that may show its back draws');
  fills.length = 0;
  await renderer.draw(standIn(canvas()), still({ width: 10, height: 10, layers: [group(30)] }));
  assert.deepEqual(fills, ['#f00', '#0f0']);
  renderer.dispose();
});

/** A canvas of `pixelCanvases`. */
type PixelCanvas = { id: number; width: number; height: number; painted?: boolean; getContext(): unknown };

/** What `pixelCanvases` records: each operation's kind, then the canvas it is on. */
type Operation =
  | ['fill', id: number, width: number, height: number, transform: AffineMatrix]
  | ['read', id: number, x: number, y: number, width: number, height: number]
  | ['write', id: number, image: ImageData, x: number, y: number, ...dirty: number[]]
  | ['composite', id: number, source: number, alpha: number, filter: string, x: number, y: number, clipped: boolean];

/**
 * Canvases whose pixels follow what is drawn, coarsely: a fill, or drawing
 * a canvas that holds anything, covers the whole canvas in opaque red.
 * `operations` records fills, reads, writes, clips, and composites.
 */
function pixelCanvases() {
  const originals = { document: globalThis.document, DOMMatrix: globalThis.DOMMatrix, DOMPoint: globalThis.DOMPoint, Path2D: globalThis.Path2D };
  onTestFinished(() => { Object.assign(globalThis, originals); });
  globalThis.DOMMatrix = standIn(AffineMatrix);
  globalThis.DOMPoint = standIn(class { constructor(readonly x: number, readonly y: number) {} });
  globalThis.Path2D = standIn(class { moveTo() {} lineTo() {} closePath() {} addPath() {} });
  const operations: Operation[] = [];
  let nextCanvas = 0;
  const canvas = () => {
    const element: PixelCanvas = { id: nextCanvas++, width: 100, height: 100, getContext() { return context; } };
    const paint = () => {
      element.painted = true;
      context.pixels = new Uint8ClampedArray(element.width * element.height * 4);
      for (let i = 0; i < context.pixels.length; i += 4) context.pixels.set([255, 0, 0, 255], i);
    };
    const stack: { globalAlpha: number; filter: string; globalCompositeOperation: string; clipped: boolean }[] = [];
    const context = {
      canvas: element, globalAlpha: 1, filter: 'none', globalCompositeOperation: 'source-over', clipped: false,
      // What `setTransform` set last, and what has been drawn: nothing yet.
      transform: undefined as AffineMatrix | undefined, pixels: undefined as Uint8ClampedArray | undefined,
      save() { stack.push({ globalAlpha: this.globalAlpha, filter: this.filter, globalCompositeOperation: this.globalCompositeOperation, clipped: this.clipped }); },
      restore() { Object.assign(this, stack.pop()); },
      setTransform(matrix: AffineMatrix) { this.transform = matrix; }, fillRect() {}, beginPath() {}, roundRect() {},
      clip() { this.clipped = true; },
      fill() {
        // Each layer sets its matrix before it fills.
        operations.push(['fill', element.id, element.width, element.height, this.transform!]);
        paint();
      },
      getImageData(x: number, y: number, width: number, height: number) {
        operations.push(['read', element.id, x, y, width, height]);
        return { data: this.pixels ?? new Uint8ClampedArray(width * height * 4), width, height };
      },
      createImageData(width: number, height: number) { return { data: new Uint8ClampedArray(width * height * 4), width, height }; },
      putImageData(image: ImageData, x: number, y: number, ...dirty: number[]) {
        operations.push(['write', element.id, image, x, y, ...dirty]);
        element.painted = true;
      },
      drawImage(source: PixelCanvas, x: number, y: number) {
        operations.push(['composite', element.id, source.id, this.globalAlpha, this.filter, x, y, this.clipped]);
        if (source.painted) paint();
      },
    };
    return element;
  };
  globalThis.document = standIn({ fonts: new Set(), createElement: canvas });
  return { canvas, operations };
}

/** The canvases that what is on canvas `id` is drawn onto, directly or through others. */
function reaches(operations: Operation[], id: number) {
  const found = new Set<number>();
  const visit = (from: number) => {
    for (const [kind, to, source] of operations) {
      if (kind === 'composite' && source === from && !found.has(to)) {
        found.add(to);
        visit(to);
      }
    }
  };
  visit(id);
  return found;
}

const at = (x: number, y: number, extra: Partial<Transform> = {}): Transform => ({ position: { x, y }, scale: { x: 1, y: 1 }, anchor: { x: 0.5, y: 0.5 }, rotation: 0, ...extra });
const square = (transform: Transform, extra: Partial<Layer> = {}): Layer => ({ id: 'square', opacity: 1, transform, content: { type: 'rect', width: 20, height: 20, cornerRadius: 0, fill: { type: 'solid', color: '#f00' } }, ...extra });

test('a projective rect is drawn onto a canvas in its own plane, then onto its parent once', async () => {
  const { canvas, operations } = pixelCanvases();
  const renderer = new SceneCanvas(new Map());
  const transform = at(50, 50, { rotationY: 30, perspective: 200 });
  const output = canvas();
  await renderer.draw(standIn(output), still({ width: 100, height: 100, layers: [square(transform, { opacity: 0.5 })] }));

  const forward = homography(transform);
  const raster = planeRaster(forward, [-10, -10, 10, 10], [0, 0, 100, 100], 8192, 4 * 100 * 100)!;
  const [fill, ...others] = ofKind(operations, 'fill');
  assert.equal(others.length, 0);
  // The plane's canvas is sized by `planeRaster`, its content drawn at the raster's scale.
  assert.deepEqual(fill.slice(2, 4), [raster.width, raster.height]);
  const { a, b, c, d, e, f } = fill[4];
  assert.deepEqual([a, b, c, d, e, f], [raster.scale, 0, 0, raster.scale, 1 - raster.left * raster.scale, 1 - raster.top * raster.scale]);
  // It is drawn piece by piece, each through a clip, onto a surface (or
  // layers that add onto it), which reaches the output once, at the rect's
  // opacity; nothing is read back where no piece is warped (see
  // plane-pieces.test.ts).
  const composites = ofKind(operations, 'composite').filter(([, id]) => id === output.id);
  assert.deepEqual(composites.map(([, , , alpha]) => alpha), [0.5]);
  const result = composites[0][2];
  const pieces = ofKind(operations, 'composite').filter(([, , from, , , , , clipped]) => from === fill[1] && clipped);
  assert.ok(pieces.length > 1 && pieces.every(([, id]) => id === result || reaches(operations, id).has(result)), JSON.stringify(pieces));
  assert.deepEqual(operations.filter(([kind]) => kind === 'read'), []);
  renderer.dispose();
});

test('a long plane draws through one canvas per band and composites once', async () => {
  const { canvas, operations } = pixelCanvases();
  const renderer = new SceneCanvas(new Map());
  // A tenth of the GPU tests' crawl: 120 wide and 2000 deep, its near edge
  // near the bottom of a 192x108 scene.
  const transform = at(96, 90, { rotationX: 70, perspective: 80 });
  const column: Layer = { id: 'column', opacity: 1, transform: at(0, -972), content: { type: 'rect', width: 120, height: 2000, cornerRadius: 0, fill: { type: 'solid', color: '#f00' } } };
  const output = canvas();
  output.width = 192;
  output.height = 108;
  await renderer.draw(standIn(output), still({ width: 192, height: 108, layers: [{ id: 'group', opacity: 0.5, transform, content: { type: 'group', layers: [column] } }] }));

  const bands = planeBands(homography(transform), [-60, -1972, 60, 28], [0, 0, 192, 108], 8192, 4 * 192 * 108, 0, true);
  assert.ok(bands.length > 1, `${bands.length} bands`);
  // Each band's canvas, its content drawn at its scale: the scales the GPU
  // picks, or all lowered alike where the browser's wider seams (see
  // `PLANE_SEAM`) take the bands past the budget.
  const fills = ofKind(operations, 'fill');
  const scales = fills.map(([, , , , { a }]) => a);
  assert.equal(scales.length, bands.length);
  const ratios = scales.map((scale, k) => scale / bands[k].raster.scale);
  assert.ok(ratios.every(ratio => ratio <= 1 && Math.abs(ratio - ratios[0]) < 1e-9), JSON.stringify(ratios));
  // Each drawn piece by piece through clips onto one surface (or layers
  // that add onto it), which reaches the output once, at the group's
  // opacity.
  const composites = ofKind(operations, 'composite').filter(([, id]) => id === output.id);
  assert.deepEqual(composites.map(([, , , alpha]) => alpha), [0.5]);
  const result = composites[0][2];
  for (const [, source] of fills) {
    const pieces = ofKind(operations, 'composite').filter(([, , from, , , , , clipped]) => from === source && clipped);
    assert.ok(pieces.length > 0 && pieces.every(([, id]) => id === result || reaches(operations, id).has(result)), `canvas ${source}`);
  }
  renderer.dispose();
});

test('effects inside a plane are drawn at its scale, on a canvas grown to hold them', async () => {
  const { canvas, operations } = pixelCanvases();
  const renderer = new SceneCanvas(new Map());
  const transform = at(50, 50, { rotationY: 30, perspective: 200 });
  const effects = { blur: 2, shadow: { color: '#000', blur: 3, offsetX: 6, offsetY: 4 } };
  const group: Layer = { id: 'group', opacity: 1, transform, content: { type: 'group', layers: [square(at(0, 0), { effects })] } };
  await renderer.draw(standIn(canvas()), still({ width: 100, height: 100, layers: [group] }));

  const forward = homography(transform);
  const [fill] = ofKind(operations, 'fill');
  const [, source, width, height, { a: scale, e: left, f: top }] = fill;
  const effectScale = scale / stretchAt(forward, 0, 0)!;
  assert.ok(effectScale > 1, String(effectScale));
  // The shadow, then the blurred layer, onto the layer's result, at the plane's scale.
  const blurOf = (filter: string) => Number(/blur\((.*)px\)/.exec(filter)?.[1] ?? 0);
  const result = ofKind(operations, 'composite').find(([, , from, , filter]) => from === source && filter !== 'none')![1];
  const drawn = ofKind(operations, 'composite').filter(([, id]) => id === result).map(([, , , , filter, x, y]) => [blurOf(filter), x, y]);
  const close = (a: number, b: number) => Math.abs(a - b) < 1e-9;
  assert.ok(drawn.some(([blur, x, y]) => close(blur, 3 * effectScale) && close(x, 6 * effectScale) && close(y, 4 * effectScale)), JSON.stringify(drawn));
  assert.ok(drawn.some(([blur, x, y]) => close(blur, 2 * effectScale) && x === 0 && y === 0), JSON.stringify(drawn));
  // Larger than the shapes alone need, so the shifted shadow's blur stays on it.
  const shapes = planeRaster(forward, [-10, -10, 10, 10], [0, 0, 100, 100], 8192, 4 * 100 * 100)!;
  assert.ok(width > shapes.width && height > shapes.height, `${width}x${height} vs ${shapes.width}x${shapes.height}`);
  assert.ok(left + scale * 10 + (6 + 3 * 3) * effectScale <= width, `${left}, ${scale}, ${width}`);
  assert.ok(top + scale * 10 + (4 + 3 * 3) * effectScale <= height, `${top}, ${scale}, ${height}`);
  renderer.dispose();
});

test('blurs are drawn no larger than the GPU renderer draws them', async () => {
  const { canvas, operations } = pixelCanvases();
  const renderer = new SceneCanvas(new Map());
  const effects = { blur: 100, glow: { color: '#fff', blur: 80 } };
  await renderer.draw(standIn(canvas()), still({ width: 100, height: 100, layers: [square(at(50, 50), { effects })] }));
  const filters = ofKind(operations, 'composite').filter(([, , , , filter]) => filter !== 'none').map(([, , , , filter]) => filter);
  assert.deepEqual(filters, ['blur(64px)', 'blur(64px)']);
  renderer.dispose();
});

test('a plane draws within its parent\'s clip, and through its blend mode', async () => {
  const { canvas, operations } = pixelCanvases();
  const renderer = new SceneCanvas(new Map());
  const transform = at(50, 50, { rotationY: 30, perspective: 200 });
  const output = canvas();
  const clip = { x: 0, y: 0, width: 50, height: 100, cornerRadius: 0 };
  await renderer.draw(standIn(output), still({ width: 100, height: 100, layers: [{ id: 'group', opacity: 1, transform: at(0, 0), content: { type: 'group', clip, layers: [square(transform)] } }] }));
  const clipped = ofKind(operations, 'composite').filter(([, id]) => id === output.id);
  assert.equal(clipped.length, 1);
  assert.equal(clipped[0].at(-1), true, 'drawn inside the group\'s clip');

  operations.length = 0;
  await renderer.draw(standIn(output), still({ width: 100, height: 100, layers: [square(transform, { opacity: 0.5, blendMode: 'multiply' })] }));
  // The plane is drawn onto a surface of its own, which then blends over
  // the backdrop it reads onto another, at the layer's opacity.
  const [plane] = ofKind(operations, 'fill');
  const composites = ofKind(operations, 'composite').filter(([, id]) => id === output.id);
  assert.deepEqual(composites.map(([, , , alpha]) => alpha), [0.5]);
  const blended = composites[0][2];
  const pieces = ofKind(operations, 'composite').filter(([, , from, , , , , clipped]) => from === plane[1] && clipped);
  assert.ok(reaches(operations, plane[1]).has(blended) && pieces.every(([, id]) => id !== blended));
  assert.ok(operations.some(([kind, id]) => kind === 'read' && id === output.id), 'the backdrop is read');
  renderer.dispose();
});

test('a skewed layer draws through its linear part, and an edge-on one not at all', async () => {
  const { canvas, operations } = pixelCanvases();
  let rotations = 0;
  globalThis.DOMMatrix = standIn(class extends AffineMatrix {
    rotate(degrees: number) { rotations++; return super.rotate(degrees); }
  });
  const renderer = new SceneCanvas(new Map());
  const fills = async (transform: Transform) => {
    operations.length = 0;
    rotations = 0;
    await renderer.draw(standIn(canvas()), still({ width: 100, height: 100, layers: [square(transform)] }));
    return ofKind(operations, 'fill');
  };
  const [skewed] = await fills(at(10, 20, { skew: { x: 45, y: 0 } }));
  const { a, b, c, d, e, f } = skewed[4];
  assert.deepEqual([a, b, d, e, f], [1, 0, 1, 10, 20]);
  assert.ok(Math.abs(c - 1) < 1e-12, String(c));
  assert.equal(rotations, 0, 'not through translate · rotate · scale');
  assert.equal((await fills(at(10, 20, { rotation: 30 }))).length, 1);
  assert.equal(rotations, 1, 'a flat layer keeps translate · rotate · scale');
  assert.equal((await fills(at(10, 20, { rotationY: 90 }))).length, 0);
  assert.equal((await fills(at(10, 20, { rotationX: 90 }))).length, 0);
  // Skewed so far the linear part is past 1e7, as the GPU renderer's
  // `Affine::is_degenerate` caps it; a far translation alone is not.
  assert.equal((await fills(at(10, 20, { skew: { x: 90, y: 0 } }))).length, 0);
  assert.equal((await fills(at(2e7, 20, { skew: { x: 45, y: 0 } }))).length, 1);
  assert.equal((await fills(at(10, 20, { rotationY: 60 }))).length, 1);
  renderer.dispose();
});

test('a layer turns and scales about its origin', async () => {
  const { canvas, operations } = pixelCanvases();
  const renderer = new SceneCanvas(new Map());
  const matrixOf = async (transform: Transform) => {
    operations.length = 0;
    await renderer.draw(standIn(canvas()), still({ width: 100, height: 100, layers: [square(transform)] }));
    return ofKind(operations, 'fill')[0][4];
  };
  // Scaled 2x about (5, 5), which stays at position + origin = (15, 25).
  const flat = await matrixOf(at(10, 20, { scale: { x: 2, y: 2 }, origin: { x: 5, y: 5 } }));
  assert.deepEqual([flat.a, flat.d, flat.e, flat.f], [2, 2, 5, 15]);
  // Skewed (through the linear part) about (0, 10): the row y = 10 does not slide.
  const skewed = await matrixOf(at(10, 20, { skew: { x: 45, y: 0 }, origin: { x: 0, y: 10 } }));
  assert.ok(Math.abs(skewed.e - 0) < 1e-9 && Math.abs(skewed.f - 20) < 1e-9, `${skewed.e}, ${skewed.f}`);
  renderer.dispose();
});
