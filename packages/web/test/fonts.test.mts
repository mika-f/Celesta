import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { onTestFinished, test, vi } from 'vitest';
import { loadFonts, loadedFamilies, requireLoadedFonts, resetFonts } from '../src/fonts';
import type { Asset } from '../src/types';
import { fetchStub, scope, standIn } from './stand-in.mjs';

// A minimal OpenType name table with only a Unicode-platform family record.
function unicodeFont() {
  const bytes = new ArrayBuffer(72), view = new DataView(bytes);
  view.setUint16(4, 1); view.setUint32(12, 0x6e616d65); view.setUint32(20, 28);
  view.setUint16(30, 1); view.setUint16(32, 18);
  view.setUint16(34, 0); view.setUint16(40, 16); view.setUint16(42, 26);
  [...'UnicodeFamily'].forEach((char, i) => view.setUint16(46 + i * 2, char.charCodeAt(0)));
  return bytes;
}

/** The URL of an asset at one: what every font here is. */
function urlOf(asset: Asset): string {
  if (asset.location.type !== 'url') throw new Error(`${asset.id} is not at a URL`);
  return asset.location.url;
}

test('Unicode font names, preload requirements, cached failures and partial CSS fallback', async () => {
  const faces = new Set<{ family: string }>(), fetched: string[] = [], warnings: unknown[][] = [];
  vi.spyOn(console, 'warn').mockImplementation((...args) => { warnings.push(args); });
  const originals = { fetch: globalThis.fetch, FontFace: globalThis.FontFace, fonts: scope.fonts };
  onTestFinished(() => { Object.assign(globalThis, originals); });
  scope.fonts = faces;
  globalThis.FontFace = standIn<typeof FontFace>(class {
    constructor(readonly family: string) {}
    async load() { if (this.family === 'Broken') throw new Error('bad font'); return this; }
  });
  globalThis.fetch = fetchStub(async url => {
    fetched.push(url);
    if (url.endsWith('missing.ttf')) return new Response('', { status: 404 });
    if (url.endsWith('invalid.ttf')) return new Response(new Uint8Array(2));
    if (url.endsWith('.css')) return new Response('@font-face {font-family: Broken; src: url(bad.woff)} @font-face {font-family: Good; src: url(good.woff)}', { headers: { 'content-type': 'text/css' } });
    return new Response(unicodeFont());
  });
  const asset = (path: string): Asset => ({ id: path, location: { type: 'url', url: `https://fonts.test/${path}` } });
  const resolve = urlOf;
  const unicode = asset('unicode.ttf');
  assert.throws(() => requireLoadedFonts([unicode], resolve), /prepare\(\)/);
  await Promise.all([loadFonts([unicode], resolve), loadFonts([unicode], resolve)]);
  requireLoadedFonts([unicode], resolve);
  assert.equal(fetched.length, 1);
  assert.equal([...faces][0].family, 'UnicodeFamily');

  for (const path of ['missing.ttf', 'invalid.ttf', 'faces.css']) {
    const font = asset(path);
    await loadFonts([font], resolve);
    await loadFonts([font], resolve);
    requireLoadedFonts([font], resolve);
  }
  // The stylesheet's two faces are fetched as files, once each.
  assert.equal(fetched.length, 6);
  assert.deepEqual(fetched.slice(4), ['https://fonts.test/bad.woff', 'https://fonts.test/good.woff']);
  assert.deepEqual([...faces].map(face => face.family), ['UnicodeFamily', 'Good']);
  assert.equal(warnings.length, 3);
  const unresolved: Asset = { id: 'local', location: { type: 'file', path: 'local.ttf' } };
  const missingMedia = () => { throw new Error('No media directory'); };
  await loadFonts([unresolved], missingMedia);
  requireLoadedFonts([unresolved], missingMedia);
  await loadFonts([unresolved], missingMedia);
  assert.equal(warnings.length, 4);
  resetFonts();
  assert.equal(faces.size, 0);
  assert.throws(() => requireLoadedFonts([unicode], resolve), /preloaded fonts/);
  await loadFonts([asset('missing.ttf')], resolve);
  await loadFonts([unresolved], missingMedia);
  assert.equal(fetched.length, 7);
  assert.equal(warnings.length, 6);
  resetFonts();
});

test('Google Fonts loads every Japanese split face and reports unknown names without caching failures', async () => {
  const faces = new Set<{ family: string; descriptors: FontFaceDescriptors }>();
  const originals = { fetch: globalThis.fetch, FontFace: globalThis.FontFace, fonts: scope.fonts };
  onTestFinished(() => { resetFonts(); Object.assign(globalThis, originals); });
  scope.fonts = faces;
  globalThis.FontFace = standIn<typeof FontFace>(class {
    constructor(readonly family: string, readonly source: string, readonly descriptors: FontFaceDescriptors) {}
    async load() { return this; }
  });
  let hits = 0;
  globalThis.fetch = fetchStub(async url => {
    hits++;
    if (url.includes('Missing')) return new Response('', { status: 400 });
    if (url.endsWith('.woff2')) return new Response(new Uint8Array(4));
    return new Response(`
      @font-face {font-family: 'Noto Sans JP'; font-weight: 700; src: url(https://fonts.gstatic.com/japanese.woff2); unicode-range: U+3040-30FF}
      @font-face {font-family: 'Noto Sans JP'; font-weight: 700; src: url(https://fonts.gstatic.com/latin.woff2); unicode-range: U+0000-00FF}
    `, { headers: { 'content-type': 'text/css' } });
  });
  const url = 'https://fonts.googleapis.com/css?family=Noto+Sans+JP:700&subset=japanese';
  const font: Asset = { id: url, location: { type: 'url', url } };
  const resolve = urlOf;
  await loadFonts([font], resolve);
  await loadFonts([font], resolve);
  // The stylesheet, then each split face's file.
  assert.equal(hits, 3);
  assert.deepEqual([...faces].map(face => [face.family, face.descriptors.weight, face.descriptors.unicodeRange]), [
    ['Noto Sans JP', '700', 'U+3040-30FF'], ['Noto Sans JP', '700', 'U+0000-00FF'],
  ]);
  for (const missingURL of [
    'https://fonts.googleapis.com/css?family=Missing+Family:400',
    'https://fonts.googleapis.com/css2?family=Missing+Family:wght@400',
  ]) {
    const missing: Asset = { id: missingURL, location: { type: 'url', url: missingURL } };
    for (let attempt = 0; attempt < 2; attempt++) {
      await assert.rejects(loadFonts([missing], resolve), /could not find family "Missing Family" or the requested weights/);
      assert.throws(() => requireLoadedFonts([missing], resolve), /preloaded fonts/);
    }
  }
  assert.equal(hits, 7);
});

test('a stylesheet face loads the file the native renderer picks from its last valid src', async () => {
  const fetched: string[] = [];
  const originals = { fetch: globalThis.fetch, FontFace: globalThis.FontFace, fonts: scope.fonts };
  onTestFinished(() => { resetFonts(); Object.assign(globalThis, originals); });
  const faces = new Set<{ family: string; source: string | ArrayBuffer }>(), warnings: unknown[][] = [];
  vi.spyOn(console, 'warn').mockImplementation((...args) => { warnings.push(args); });
  scope.fonts = faces;
  globalThis.FontFace = standIn<typeof FontFace>(class {
    constructor(readonly family: string, readonly source: string | ArrayBuffer) {}
    async load() { return this; }
  });
  // Shared with `picks_the_same_sources_as_the_web_renderer` in
  // crates/remote/src/stylesheet.rs, so both renderers pick the same files
  // under the same families.
  const fixtures = new URL('./fixtures/font-face-src/', import.meta.url);
  const css = readFileSync(new URL('stylesheet.css', fixtures), 'utf8');
  const expected = readFileSync(new URL('expected.txt', fixtures), 'utf8').trimEnd().split('\n');
  // Each file's bytes are its URL, so a face tells which file it came from.
  globalThis.fetch = fetchStub(async url => {
    fetched.push(url);
    return url.endsWith('.css')
      ? new Response(css, { headers: { 'content-type': 'text/css' } })
      : new Response(url);
  });
  const font: Asset = { id: 'faces', location: { type: 'url', url: 'https://fonts.test/css/faces.css' } };
  await loadFonts([font], urlOf);
  assert.deepEqual(fetched.slice(1), expected.map(line => line.split('\t')[1]));
  const loaded = [...faces].map(face => `${face.family}\t${typeof face.source === 'string' ? face.source : new TextDecoder().decode(face.source)}`);
  assert.deepEqual(loaded.sort(), [...expected].sort());
  // The rule whose last `src` names only local() faces loads nothing, as natively.
  assert.equal(warnings.length, 1);
  assert.match(String(warnings[0][1]), /OnlyLocal.*no file Celesta can load/);
});

test('loadedFamilies lists the families of each font in list order', async () => {
  const originals = { fetch: globalThis.fetch, FontFace: globalThis.FontFace, fonts: scope.fonts };
  onTestFinished(() => { resetFonts(); Object.assign(globalThis, originals); });
  scope.fonts = new Set();
  globalThis.FontFace = standIn<typeof FontFace>(class {
    constructor(readonly family: string) {}
    async load() { return this; }
  });
  globalThis.fetch = fetchStub(async url => url.endsWith('a.css')
    ? new Response('@font-face {font-family: Alpha; src: url(a1.woff)} @font-face {font-family: Alpha; src: url(a2.woff)} @font-face {font-family: Beta; src: url(b.woff)}', { headers: { 'content-type': 'text/css' } })
    : new Response(unicodeFont()));
  const asset = (path: string): Asset => ({ id: path, location: { type: 'url', url: `https://fonts.test/${path}` } });
  const resolve = urlOf;
  const unresolved: Asset = { id: 'local', location: { type: 'file', path: 'local.ttf' } };
  const fonts = [asset('a.css'), asset('unicode.ttf')];
  await loadFonts(fonts, resolve);
  assert.deepEqual(loadedFamilies(fonts, resolve), ['Alpha', 'Beta', 'UnicodeFamily']);
  assert.deepEqual(loadedFamilies([...fonts].reverse(), resolve), ['UnicodeFamily', 'Alpha', 'Beta']);
  assert.deepEqual(loadedFamilies([unresolved], () => { throw new Error('No media directory'); }), []);
});
