import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';
import type { ReactElement, ReactNode } from 'react';

import { Composition, Sequence, Span, Theme } from '@celesta.studio/react';
import type { GlyphMetrics, Layer, LayerContent, Paint, TextMetrics } from '@celesta.studio/react';
import { mount, setTextMeasurer } from '@celesta.studio/react/internal';
import type { MeasureTextRequest } from '@celesta.studio/react/internal';
import { useTypewriter } from '@celesta.studio/text';
import type { Typewriter } from '@celesta.studio/text';
import { TypewriterText } from '../src/index.js';
import type { TypewriterCaretShape, TypewriterTextProps } from '../src/index.js';

const h = React.createElement;

// A proportional font: `i` is narrow, `W` wide, kana and kanji a full em,
// and a combining mark joins the cluster before it.
const ADVANCE: Record<string, number> = { i: 0.25, W: 0.9, ' ': 0.3, 0: 0.55 };
const advance = (c: string, size: number) => size * (/[　-鿿]/u.test(c) ? 1 : ADVANCE[c] ?? 0.5);

// Lays text out as the native measurer reports it: lines at `\n`, wrapped
// before the character that would pass `maxWidth`, every cluster on its line.
function measure({ text, style, maxWidth }: MeasureTextRequest): TextMetrics {
  const size = style.fontSize ?? 32;
  const lineHeight = style.lineHeight ?? size * 1.25;
  const characters = Array.from(text);
  const glyphs: GlyphMetrics[] = [];
  const lineStarts = [0];
  const widths = [0];
  let x = 0;
  characters.forEach((c, i) => {
    if (c === '\n') {
      lineStarts.push(i + 1); widths.push(0); x = 0;
      return;
    }
    if (/\p{M}/u.test(c) && glyphs.length > 0) {
      glyphs.at(-1)!.end = i + 1;
      glyphs.at(-1)!.text += c;
      return;
    }
    const width = advance(c, size);
    if (maxWidth !== undefined && x > 0 && x + width > maxWidth) {
      lineStarts.push(i); widths.push(0); x = 0;
    }
    glyphs.push({ start: i, end: i + 1, rtl: false, text: c, x, width, line: lineStarts.length - 1, inkAscent: 0, inkDescent: 0 });
    x += width;
    widths[widths.length - 1] = x;
  });
  // Alignment within `maxWidth`, else within the widest line, as the browser draws it.
  const width = Math.max(...widths);
  const shift = style.align === 'center' ? 0.5 : style.align === 'right' ? 1 : 0;
  for (const glyph of glyphs) glyph.x += shift * ((maxWidth ?? width) - widths[glyph.line]);
  const ascent = (lineHeight - size * 1.25) / 2 + size;
  return {
    width, height: lineStarts.length * lineHeight, ascent, descent: lineHeight - ascent, lineHeight,
    capHeight: size * 0.7, xHeight: size * 0.5, lines: lineStarts.length, glyphs, lineStarts,
  };
}

// The browser measurer: the same whole-text metrics, but no glyphs.
function browserMeasure(request: MeasureTextRequest): TextMetrics {
  const metrics = measure(request);
  return {
    ...metrics,
    get glyphs(): GlyphMetrics[] { throw new Error('Shaped glyph metrics are unavailable in the browser runtime.'); },
  };
}

function install(measurer: (request: MeasureTextRequest) => TextMetrics = measure) {
  setTextMeasurer(async (request) => measurer(request), measurer);
}

/** A leaf layer's content, with where it ends up (over any `x` of its own). */
type Placed<C> = C extends unknown ? Omit<C, 'x' | 'y'> & { x: number; y: number } : never;
type Leaf = Placed<Exclude<LayerContent, { type: 'group' }>>;
type LeafOf<T extends Leaf['type']> = Extract<Leaf, { type: T }>;

const is = <T extends Leaf['type']>(type: T) => (leaf: Leaf): leaf is LeafOf<T> => leaf.type === type;

/** The color of a solid paint. */
function colorOf(paint: Paint | null | undefined): string {
  assert.ok(paint && paint.type === 'solid', 'a solid paint');
  return paint.color;
}

// Every leaf layer with its position summed through its groups.
function leaves(layers: Layer[], x = 0, y = 0, out: Leaf[] = []): Leaf[] {
  for (const layer of layers) {
    const lx = x + layer.transform.position.x;
    const ly = y + layer.transform.position.y;
    if (layer.content.type === 'group') leaves(layer.content.layers, lx, ly, out);
    else out.push({ ...layer.content, x: lx, y: ly });
  }
  return out;
}

function render(element: ReactElement, { durationInFrames = 120 } = {}) {
  const composition = mount(() => h(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames }, element));
  return (frame: number) => {
    const layers = leaves(composition.renderAt({ value: frame, timescale: 30 }, null).scene.layers);
    return { text: layers.find(is('text')), caret: layers.find(is('rect')) };
  };
}

type Props = Omit<TypewriterTextProps, 'children'>;

const typewriter = (props: Props, children: ReactNode) => h(TypewriterText, { fontSize: 20, ...props, children });

// Typing from frame 30, so that the caret is on before typing (its blink is
// 40 frames) and up to 20 frames after: the frame `length` characters show.
const typed = (props: Props, children: ReactNode) => typewriter({ from: 30, blinkFrames: 40, ...props }, children);
const frameOf = (length: number) => length === 0 ? 0 : 29 + length;

test('the typed text and the caret match useTypewriter on every frame, on a seek and in a nested Sequence', () => {
  install();
  const options = { from: 12, framesPerChar: 2, blinkFrames: 10 };
  const content = 'pnpm run build';
  let hook!: Typewriter;
  function Probe() {
    hook = useTypewriter(content, options);
    return null;
  }
  const probe = mount(() => h(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 120 }, h(Probe)));
  const direct = render(typewriter(options, content));
  const nested = render(h(Sequence, { from: 20 }, h(Sequence, { from: 10 }, typewriter(options, content))),
    { durationInFrames: 150 });
  // Before the start, the first character, mid-way, the last, and long after, out of order.
  for (const frame of [100, 0, 11, 12, 13, 25, 38, 39, 40, 41, 55, 5, 20]) {
    probe.renderAt({ value: frame, timescale: 30 }, null);
    for (const { text, caret } of [direct(frame), nested(frame + 30)]) {
      assert.equal(text?.text, content, `frame ${frame}`);
      assert.equal(text?.style.visibleCharacters, hook.length, `frame ${frame}`);
      assert.equal(caret !== undefined, hook.caretVisible, `frame ${frame}`);
    }
  }
  // Typing starts on `from`, ends on the last character's frame, and then blinks.
  probe.renderAt({ value: 11, timescale: 30 }, null);
  assert.deepEqual([hook.length, hook.caretVisible], [0, false]);
  probe.renderAt({ value: 12, timescale: 30 }, null);
  assert.equal(hook.length, 1);
  probe.renderAt({ value: 12 + 13 * 2, timescale: 30 }, null);
  assert.deepEqual([hook.length, hook.done], [14, true]);
});

test('the caret follows a proportional font character by character', () => {
  install();
  const frame = render(typed({}, 'iW iW'));
  const xs = [0, 1, 2, 3, 4, 5].map((length) => frame(frameOf(length)).caret?.x);
  // After i (5), W (18), space (6), i (5), W (18).
  assert.deepEqual(xs, [0, 5, 23, 29, 34, 52]);
  // The caret's offset is the hook's length, one code point at a time.
  assert.deepEqual([0, 1, 2, 3, 4, 5].map((length) => frame(frameOf(length)).text?.style.visibleCharacters), [0, 1, 2, 3, 4, 5]);
  // e + U+0301 is one cluster: between its code points, the caret is halfway across it.
  const accent = render(typed({}, 'iéi'));
  assert.deepEqual([1, 2, 3, 4].map((length) => accent(frameOf(length)).caret?.x), [5, 10, 15, 20]);
});

test('the caret goes down a line at \\n and where maxWidth wraps, in Japanese too', () => {
  install();
  // 20px per kanji and kana: a 70px line holds three.
  const frame = render(typed({ maxWidth: 70 }, '日本語の文章\nです'));
  const at = (length: number) => {
    const { caret } = frame(frameOf(length));
    return [caret?.x, caret?.y];
  };
  const top = -20; // the font's ascent above the baseline, which is 20 below the line's top
  // The first line, then the wrap before the fourth character.
  assert.deepEqual(at(0), [0, 20 + top]);
  assert.deepEqual(at(2), [40, 20 + top]);
  assert.deepEqual(at(3), [0, 45 + top]);
  assert.deepEqual(at(5), [40, 45 + top]);
  // Before the `\n` is typed, then on the line after it.
  assert.deepEqual(at(6), [60, 45 + top]);
  assert.deepEqual(at(7), [0, 70 + top]);
  assert.deepEqual(at(9), [40, 70 + top]);
  assert.equal(frame(0).text?.maxWidth, 70);
});

test('the box of the finished text anchors it, so nothing moves while it types', () => {
  install();
  const content = 'WWWW\nii';
  const at = (props: Props, f: number) => render(typewriter(props, content))(f);
  // 72 wide (four W), two 25px lines; the baseline is 20 below the top.
  for (const f of [0, 2, 6]) {
    assert.deepEqual([at({}, f).text?.x, at({}, f).text?.y], [0, 20]);
    assert.deepEqual([at({ anchorX: 0.5, anchorY: 0.5 }, f).text?.x, at({ anchorX: 0.5, anchorY: 0.5 }, f).text?.y], [-36, -5]);
    assert.deepEqual([at({ anchorY: 'baseline' }, f).text?.y, at({ anchorX: 1 }, f).text?.x], [0, -72]);
  }
  const centered = at({ anchorX: 0.5, x: 960, y: 540 }, 5);
  assert.deepEqual([centered.text?.x, centered.caret?.x], [960 - 36, 960 - 36 + 5]);
});

test('the caret takes its size from the font, not from the line height, and its color from the text', () => {
  install();
  const at = (props: Props) => render(h(Theme, { colors: { ink: '#101820' } }, typed(props, 'ab')))(0).caret;
  // The font's own 25px line, centered on the text's taller line box.
  const line = at({ style: { lineHeight: 60, fill: { type: 'solid', color: 'ink' } } });
  assert.deepEqual([line?.x, line?.y, line?.width, line?.height], [0, 20 + 17.5 - 20, 2, 25]);
  assert.deepEqual(line?.fill, { type: 'solid', color: '#101820' });
  // A block is one `0` wide.
  const block = at({ caretShape: 'block', fontSize: 40, color: '#ff0000' });
  assert.deepEqual([block?.width, block?.height, colorOf(block?.fill)], [22, 50, '#ff0000']);
  const custom = at({ caretWidth: 6, caretHeight: 15, caretColor: 'ink' });
  assert.deepEqual([custom?.y, custom?.width, custom?.height, colorOf(custom?.fill)], [5, 6, 15, '#101820']);
  // Without a solid fill, the caret is white, as the text is.
  assert.equal(colorOf(at({})?.fill), '#ffffff');
});

test('the text and caret look up <Theme> names', () => {
  install();
  const frame = render(h(Theme, { fonts: { mono: 'JetBrains Mono' }, colors: { ink: '#101820', accent: '#ff3300' } },
    typewriter({ fontFamily: 'mono', color: 'ink', caretColor: 'accent' }, ['Hi ', h(Span, { color: 'accent' }, 'there')])));
  const { text, caret } = frame(2);
  assert.equal(text?.text, 'Hi there');
  assert.equal(text?.style.fontFamily, 'JetBrains Mono');
  assert.equal(colorOf(text?.style.fill), '#101820');
  assert.deepEqual(text?.style.colorRuns, [{ start: 3, end: 8, color: '#ff3300' }]);
  assert.equal(colorOf(caret?.fill), '#ff3300');
});

test('empty text draws only the blinking caret; caret={false} draws none', () => {
  install();
  const empty = render(typewriter({ blinkFrames: 10 }, ''));
  assert.equal(empty(0).text, undefined);
  assert.deepEqual([empty(0).caret?.x, empty(0).caret?.y], [0, 0]);
  assert.equal(empty(5).caret, undefined);
  assert.equal(empty(10).caret !== undefined, true);
  const hidden = render(typewriter({ caret: false }, 'ab'));
  assert.equal(hidden(0).caret, undefined);
  assert.equal(hidden(0).text?.style.visibleCharacters, 1);
});

test('without glyph positions (the browser), the caret is measured where the native layout puts it', () => {
  const cases: [Props, string][] = [
    [{}, 'iW iW'],
    [{ maxWidth: 70 }, '日本語の文章\nです'],
    [{ style: { align: 'center' } }, 'WWWW\nii\niW'],
    [{ style: { align: 'right' }, anchorX: 0.5 }, 'iii\nWWW'],
    [{ style: { align: 'center' }, maxWidth: 200 }, 'Centered\niW'],
  ];
  for (const [props, content] of cases) {
    install(measure);
    const native = render(typed(props, content));
    const expected = Array.from({ length: Array.from(content).length + 1 }, (_, length) => native(frameOf(length)).caret);
    install(browserMeasure);
    const browser = render(typed(props, content));
    expected.forEach((caret, length) => {
      const actual = browser(frameOf(length)).caret;
      assert.ok(actual && caret, `${JSON.stringify(content)} at ${length}`);
      assert.deepEqual([actual.x, actual.y], [caret.x, caret.y], `${JSON.stringify(content)} at ${length}`);
    });
  }
});

test('TypewriterText rejects invalid sizes and shapes', () => {
  install();
  assert.throws(() => render(typewriter({ maxWidth: 0 }, 'a'))(0), /positive `maxWidth`/);
  assert.throws(() => render(typewriter({ caretWidth: -1 }, 'a'))(0), /positive `caretWidth`/);
  // Not a caret shape, cast to check the error.
  assert.throws(() => render(typewriter({ caretShape: 'underline' as TypewriterCaretShape }, 'a'))(0), /caretShape/);
  assert.throws(() => render(typewriter({ caretColor: 'nope' }, 'a'))(0), /caretColor "nope"/);
  assert.throws(() => render(typewriter({ framesPerChar: 0 }, 'a'))(0), /framesPerChar/);
});
