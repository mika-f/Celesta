// `fontFeatures` in the browser renderer: Canvas text has no
// font-feature-settings, so each loaded family and feature set is drawn
// with copies of its faces that carry the `featureSettings` descriptor.

import assert from 'node:assert/strict';
import { afterEach, beforeEach, test } from 'vitest';

import { featureFacesLoaded, loadFonts, releaseFonts, resetFonts } from '../src/fonts.js';
import { checkFontFeatures, cssFont, loadFeatureFonts, textMeasurer } from '../src/text-layout.js';
import type { Asset, FontFeatures, TextStyle } from '../src/types.js';

/** A stand-in for the browser's FontFace: loaded at once unless `pending` holds it. */
class StubFontFace {
  static pending = false;
  status: FontFaceLoadStatus;
  loaded: Promise<StubFontFace>;
  private resolve!: (face: StubFontFace) => void;
  constructor(readonly family: string, readonly source: string | ArrayBuffer, readonly descriptors: FontFaceDescriptors = {}) {
    this.status = StubFontFace.pending ? 'loading' : 'loaded';
    this.loaded = new Promise((resolve) => { this.resolve = resolve; });
    if (this.status === 'loaded') this.resolve(this);
  }
  async load() { return this; }
  finish() { this.status = 'loaded'; this.resolve(this); }
}

// A document's font set, so canvas font strings stay as written (see `fontSetFamily`).
const globals = globalThis as unknown as { document?: { fonts: Set<StubFontFace> }; FontFace: unknown; fetch: typeof fetch; OffscreenCanvas: unknown };
const originals = { FontFace: globals.FontFace, fetch: globals.fetch, OffscreenCanvas: globals.OffscreenCanvas };
const css = `
  @font-face {font-family: 'JetBrains Mono'; font-weight: 400; src: url(https://fonts.test/mono-latin.woff2) format('woff2'); unicode-range: U+0000-00FF}
  @font-face {font-family: 'JetBrains Mono'; font-weight: 700; src: url(https://fonts.test/mono-bold.woff2) format('woff2'); unicode-range: U+0000-00FF}
`;
const mono: Asset = { id: 'mono', location: { type: 'url', url: 'https://fonts.test/mono.css' } };
const resolve = (asset: Asset) => asset.location.type === 'url' ? asset.location.url : asset.location.path;
const ligaturesOff: TextStyle = { fontFamily: 'JetBrains Mono', fontSize: 48, fontFeatures: { liga: false, calt: false } };

const faces = () => [...globals.document!.fonts];
const copies = () => faces().filter((face) => face.descriptors.featureSettings !== undefined);

beforeEach(() => {
  StubFontFace.pending = false;
  globals.document = { fonts: new Set() };
  globals.FontFace = StubFontFace;
  globals.fetch = (async (url: string) => url.endsWith('.css')
    ? new Response(css, { headers: { 'content-type': 'text/css' } })
    : new Response(new TextEncoder().encode(url))) as typeof fetch;
});

afterEach(() => {
  resetFonts();
  delete globals.document;
  Object.assign(globals, originals);
});

test('features draw with copies of the family\'s loaded faces', async () => {
  await loadFonts([mono], resolve);
  const font = cssFont(ligaturesOff);
  assert.equal(font, '400 48px "JetBrains Mono [celesta-features calt=0,liga=0]", system-ui, sans-serif');
  // One copy of each face, from the same file, with the feature settings.
  assert.deepEqual(copies().map((face) => [face.family, face.descriptors.weight, face.descriptors.unicodeRange, face.descriptors.featureSettings]), [
    ['JetBrains Mono [celesta-features calt=0,liga=0]', '400', 'U+0000-00FF', '"calt" 0, "liga" 0'],
    ['JetBrains Mono [celesta-features calt=0,liga=0]', '700', 'U+0000-00FF', '"calt" 0, "liga" 0'],
  ]);
  const [regular] = faces();
  assert.equal(copies()[0]!.source, regular!.source);
  // The same features reuse the copies, in any tag order.
  assert.equal(cssFont({ ...ligaturesOff, fontFeatures: { calt: 0, liga: 0 } }), font);
  assert.equal(copies().length, 2);
  // Text without features draws with the family itself.
  assert.equal(cssFont({ fontFamily: 'JetBrains Mono', fontSize: 48 }), '400 48px "JetBrains Mono", system-ui, sans-serif');
  assert.equal(cssFont({ fontFamily: 'JetBrains Mono', fontSize: 48, fontFeatures: {} }), '400 48px "JetBrains Mono", system-ui, sans-serif');
});

test('a font run\'s features merge over the text\'s, and its family takes them', async () => {
  await loadFonts([mono], resolve);
  const run = { start: 0, end: 1, fontWeight: 700, fontFeatures: { liga: true, zero: 1 } };
  assert.equal(cssFont(ligaturesOff, run), '700 48px "JetBrains Mono [celesta-features calt=0,liga=1,zero=1]", system-ui, sans-serif');
  assert.equal(cssFont({ fontFamily: 'JetBrains Mono', fontSize: 48 }, { start: 0, end: 1, fontFeatures: { ss01: true } }),
    '400 48px "JetBrains Mono [celesta-features ss01=1]", system-ui, sans-serif');
});

test('a face is made from the first file the browser can read, which its copies share', async () => {
  const fetched: string[] = [];
  globals.fetch = (async (url: string) => {
    fetched.push(url);
    return url.endsWith('.css')
      ? new Response(`@font-face {font-family: Legacy; src: url(legacy.eot?#iefix) format('embedded-opentype'), url("legacy.svg#font") format("svg"), url(legacy.woff) format("woff"), url(legacy.ttf) format('truetype')}
        @font-face {font-family: Plain; src: local(Plain), url(plain.otf)}
        @font-face {font-family: Installed; src: local(Installed)}`, { headers: { 'content-type': 'text/css' } })
      : new Response(new TextEncoder().encode(url));
  }) as typeof fetch;
  const legacy: Asset = { id: 'legacy', location: { type: 'url', url: 'https://fonts.test/legacy.css' } };
  await loadFonts([legacy], resolve);
  assert.deepEqual(fetched, ['https://fonts.test/legacy.css', 'https://fonts.test/legacy.woff', 'https://fonts.test/plain.otf']);
  const installed = faces().find((face) => face.family === 'Installed');
  assert.equal(installed?.source, 'local("Installed")');
  cssFont({ fontFamily: 'Legacy', fontFeatures: { liga: false } });
  assert.equal(new TextDecoder().decode(copies()[0]!.source as ArrayBuffer), 'https://fonts.test/legacy.woff');
  assert.throws(() => cssFont({ fontFamily: 'Installed', fontFeatures: { liga: false } }),
    /fontFeatures cannot use "Installed": its font CSS gives no url\(\) to load it from/);
});

test('features need every family they apply to loaded with <Font>', async () => {
  await loadFonts([mono], resolve);
  assert.throws(() => cssFont({ fontSize: 48, fontFeatures: { liga: false } }),
    /fontFeatures need a fontFamily loaded with <Font>: the browser renderer cannot apply OpenType features to installed fonts/);
  assert.throws(() => cssFont({ ...ligaturesOff, fontFamily: ['JetBrains Mono', 'Menlo'] }),
    /fontFeatures need "Menlo" loaded with <Font>/);
  assert.throws(() => cssFont({ ...ligaturesOff, fontFamily: 'JetBrains Mono' }, { start: 0, end: 1, fontFamily: 'Menlo' }),
    /fontFeatures need "Menlo" loaded with <Font>/);
});

test('loading or releasing a font drops the copies, so the next ones match the font set', async () => {
  await loadFonts([mono], resolve);
  cssFont(ligaturesOff);
  assert.equal(copies().length, 2);
  releaseFonts([resolve(mono)]);
  assert.equal(faces().length, 0);
  assert.throws(() => cssFont(ligaturesOff), /fontFeatures need "JetBrains Mono" loaded with <Font>/);
  await loadFonts([mono], resolve);
  cssFont(ligaturesOff);
  assert.equal(copies().length, 2);
});

test('features are checked as the native renderer reads them', () => {
  checkFontFeatures({ liga: false, calt: 0, salt: 2, ss01: true });
  const malformed: FontFeatures[] = [{ lig: false }, { ligatures: false }, { 'lïga': false }];
  for (const features of malformed) {
    assert.throws(() => checkFontFeatures(features), /must be four ASCII characters/, JSON.stringify(features));
  }
  for (const value of [-1, 1.5, 2 ** 32, 'off', null]) {
    assert.throws(() => checkFontFeatures({ liga: value } as unknown as TextStyle['fontFeatures']),
      /must be true, false, or a whole number from 0 up/, String(value));
  }
});

test('measuring waits for copies that do not load at once, and a synchronous measurement asks for prepare()', async () => {
  await loadFonts([mono], resolve);
  globals.OffscreenCanvas = class {
    getContext() { return { font: '', measureText: (text: string) => ({ width: text.length * 10, fontBoundingBoxAscent: 8, fontBoundingBoxDescent: 2, actualBoundingBoxAscent: 7 }) }; }
  };
  StubFontFace.pending = true;
  const measure = textMeasurer();
  assert.throws(() => measure({ text: 'a===b', style: ligaturesOff, fonts: [mono] }),
    /Browser text measurements with fontFeatures need their fonts ready\. Call await measureText\(\.\.\., \{ fonts \}\) with the same style in prepare\(\) first/);
  let ready = false;
  const loading = loadFeatureFonts(ligaturesOff).then(() => { ready = true; });
  await Promise.resolve();
  assert.equal(ready, false);
  for (const face of copies()) (face as StubFontFace).finish();
  await loading;
  await featureFacesLoaded();
  assert.equal(measure({ text: 'a===b', style: ligaturesOff, fonts: [mono] }).width, 50);
});
