import assert from 'node:assert/strict';
import { test } from 'node:test';
import { loadFonts, requireLoadedFonts, resetFonts } from '../src/fonts.ts';

// A minimal OpenType name table with only a Unicode-platform family record.
function unicodeFont() {
  const bytes = new ArrayBuffer(72), view = new DataView(bytes);
  view.setUint16(4, 1); view.setUint32(12, 0x6e616d65); view.setUint32(20, 28);
  view.setUint16(30, 1); view.setUint16(32, 18);
  view.setUint16(34, 0); view.setUint16(40, 16); view.setUint16(42, 26);
  [...'UnicodeFamily'].forEach((char, i) => view.setUint16(46 + i * 2, char.charCodeAt(0)));
  return bytes;
}

test('Unicode font names, preload requirements, cached failures and partial CSS fallback', async t => {
  const faces = new Set(), fetched = [], warnings = [];
  t.mock.method(console, 'warn', (...args) => warnings.push(args));
  const originals = { fetch: globalThis.fetch, FontFace: globalThis.FontFace, fonts: globalThis.fonts };
  t.after(() => Object.assign(globalThis, originals));
  globalThis.fonts = faces;
  globalThis.FontFace = class {
    constructor(family) { this.family = family; }
    async load() { if (this.family === 'Broken') throw new Error('bad font'); return this; }
  };
  globalThis.fetch = async url => {
    fetched.push(url);
    if (url.endsWith('missing.ttf')) return new Response('', { status: 404 });
    if (url.endsWith('invalid.ttf')) return new Response(new Uint8Array(2));
    if (url.endsWith('.css')) return new Response('@font-face {font-family: Broken; src: url(bad.woff)} @font-face {font-family: Good; src: url(good.woff)}', { headers: { 'content-type': 'text/css' } });
    return new Response(unicodeFont());
  };
  const asset = path => ({ id: path, location: { type: 'url', url: `https://fonts.test/${path}` } });
  const resolve = asset => asset.location.url;
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
  assert.equal(fetched.length, 4);
  assert.deepEqual([...faces].map(face => face.family), ['UnicodeFamily', 'Good']);
  assert.equal(warnings.length, 3);
  const unresolved = { id: 'local', location: { type: 'file', path: 'local.ttf' } };
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
  assert.equal(fetched.length, 5);
  assert.equal(warnings.length, 6);
  resetFonts();
});
