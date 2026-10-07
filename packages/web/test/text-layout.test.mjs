import assert from 'node:assert/strict';
import { test } from 'node:test';
import { textLines, textLineRanges, textMeasurer } from '../src/text-layout.ts';

test('wrapping uses the requested language, including with older canvas contexts', t => {
  const locales = [], Segmenter = Intl.Segmenter;
  t.mock.method(Intl, 'Segmenter', function(locale, options) {
    locales.push(locale);
    return new Segmenter(locale, options);
  });
  const ctx = { measureText: text => ({ width: text.length, fontBoundingBoxAscent: 8, fontBoundingBoxDescent: 2 }) };
  assert.deepEqual(textLines(ctx, 'hello world', 6, ' ja '), ['hello', 'world']);
  const original = globalThis.OffscreenCanvas;
  t.after(() => { globalThis.OffscreenCanvas = original; });
  globalThis.OffscreenCanvas = class { getContext() { return ctx; } };
  const metrics = textMeasurer()({ text: 'hello world', style: { lang: 'th' }, maxWidth: 6, fonts: [] });
  assert.deepEqual(locales, ['ja', 'th']);
  assert.equal(metrics.lines, 2);
});

test('whole-run kerning and ligature widths are preserved without fabricating glyph positions', t => {
  // A shaping engine measures AV and fi differently from separate characters.
  const ctx = { measureText: text => ({ width: ({ AV: 35, fi: 18 })[text] ?? 20, fontBoundingBoxAscent: 8, fontBoundingBoxDescent: 2 }) };
  const original = globalThis.OffscreenCanvas;
  t.after(() => { globalThis.OffscreenCanvas = original; });
  globalThis.OffscreenCanvas = class { getContext() { return ctx; } };
  for (const [text, width] of [['AV', 35], ['fi', 18]]) {
    const metrics = textMeasurer()({ text, style: {}, fonts: [] });
    assert.equal(metrics.width, width);
    assert.throws(() => metrics.glyphs, /Shaped glyph metrics.*native renderer/);
  }
});


test('wrapped ranges retain source code-point offsets across trimmed spaces and CRLF', () => {
  const ctx = { measureText: text => ({ width: Array.from(text).length }) };
  assert.deepEqual(textLineRanges(ctx, '😀 AV   AV\r\né', 5, 'en'), [
    { text: '😀 AV', start: 0 }, { text: 'AV', start: 7 }, { text: 'é', start: 11 },
  ]);
});
