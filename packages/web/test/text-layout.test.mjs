import assert from 'node:assert/strict';
import { test } from 'node:test';
import { cssFont, measureLine, runPieces, textLines, textLineRanges, textMeasurer } from '../src/text-layout.ts';

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


test('runPieces keeps clusters whole and uses each cluster first code point', () => {
  const runs = [{ start: 1, end: 3, fontWeight: 700 }];
  // e + U+0301 is one cluster at 0..2; the run starts inside it.
  assert.deepEqual(runPieces('éab', 0, runs).map(p => [p.text, p.start, p.utf16, p.run?.fontWeight]), [
    ['é', 0, 0, undefined], ['a', 2, 2, 700], ['b', 3, 3, undefined],
  ]);
  // Offsets are relative to the full text: this line starts at 2.
  assert.deepEqual(runPieces('ab', 2, runs).map(p => [p.text, p.run?.fontWeight]), [['a', 700], ['b', undefined]]);
  // UTF-16 indices step over surrogate pairs.
  assert.deepEqual(runPieces('😀a', 0, [{ start: 1, end: 2, fontWeight: 700 }]).map(p => [p.text, p.start, p.utf16]), [
    ['😀', 0, 0], ['a', 1, 2],
  ]);
});

test('measuring and wrapping use each run font', () => {
  const fonts = [];
  const ctx = {
    font: '',
    measureText(text) {
      fonts.push(this.font);
      const bold = this.font.startsWith('700');
      const width = Array.from(text).length * (bold ? 2 : 1);
      return { width, actualBoundingBoxLeft: 0, actualBoundingBoxRight: width, actualBoundingBoxAscent: bold ? 9 : 8, actualBoundingBoxDescent: 2, fontBoundingBoxAscent: 8, fontBoundingBoxDescent: 2 };
    },
  };
  const style = { fontSize: 10 };
  const runs = [{ start: 2, end: 4, fontWeight: 700 }];
  ctx.font = cssFont(style);
  const line = measureLine(ctx, style, 'abcd', 0, runs);
  assert.equal(line.width, 2 + 4);
  assert.equal(line.actualBoundingBoxAscent, 9);
  assert.equal(line.fontBoundingBoxAscent, 8);
  assert.equal(ctx.font, cssFont(style));
  assert.ok(fonts.includes(cssFont(style, runs[0])));
  const width = (text, start) => measureLine(ctx, style, text, start, runs).width;
  assert.deepEqual(textLineRanges(ctx, 'ab cd ef', 5, 'en', width), [
    { text: 'ab', start: 0 }, { text: 'cd', start: 3 }, { text: 'ef', start: 6 },
  ]);
  assert.deepEqual(textLineRanges(ctx, 'ab cd ef', 5, 'en'), [
    { text: 'ab cd', start: 0 }, { text: 'ef', start: 6 },
  ]);
});

test('cssFont reads a run weight and family over the style', () => {
  assert.equal(cssFont({ fontSize: 20, fontFamily: 'Serif A' }), '400 20px "Serif A", system-ui, sans-serif');
  assert.equal(cssFont({ fontSize: 20, fontFamily: 'Serif A' }, { start: 0, end: 1, fontWeight: 700, fontFamily: 'Mono B' }), '700 20px "Mono B", system-ui, sans-serif');
});

test('the browser measurer wraps and measures with font runs', t => {
  const ctx = {
    font: '',
    measureText(text) {
      const width = Array.from(text).length * (this.font.startsWith('700') ? 2 : 1);
      return { width, fontBoundingBoxAscent: 8, fontBoundingBoxDescent: 2 };
    },
  };
  const original = globalThis.OffscreenCanvas;
  t.after(() => { globalThis.OffscreenCanvas = original; });
  globalThis.OffscreenCanvas = class { getContext() { return ctx; } };
  const metrics = textMeasurer()({ text: 'ab cd', style: { fontSize: 10, fontRuns: [{ start: 0, end: 5, fontWeight: 700 }] }, maxWidth: 6, fonts: [] });
  assert.equal(metrics.lines, 2);
  assert.equal(metrics.width, 4);
});

test('wrapped ranges retain source code-point offsets across trimmed spaces and CRLF', () => {
  const ctx = { measureText: text => ({ width: Array.from(text).length }) };
  assert.deepEqual(textLineRanges(ctx, '😀 AV   AV\r\né', 5, 'en'), [
    { text: '😀 AV', start: 0 }, { text: 'AV', start: 7 }, { text: 'é', start: 11 },
  ]);
});
