import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';
import type { ReactElement } from 'react';

import { Composition, Span, useCurrentFrame } from '@celesta.studio/react';
import type { Layer, LayerContent, Scene, TextMetrics } from '@celesta.studio/react';
import { TextReveal, phrases } from '../src/index.js';
import { mount, setTextMeasurer } from '@celesta.studio/react/internal';
import type { MeasureTextRequest } from '@celesta.studio/react/internal';

type GroupContent = Extract<LayerContent, { type: 'group' }>;
type TextContent = Extract<LayerContent, { type: 'text' }>;
type Measure = (request: MeasureTextRequest) => TextMetrics;

// Monospaced glyphs `fontSize` wide that wrap after a space, keeping the
// space at the end of the line, as the renderer does. Reports `lineStarts`.
function measureWords({ text, style, maxWidth }: MeasureTextRequest): TextMetrics {
  const size = style.fontSize!;
  const chars = Array.from(text);
  const lineStarts = [0];
  let lineStart = 0;
  let lastBreak = -1;
  chars.forEach((char, i) => {
    if (char === '\n') {
      lineStarts.push(i + 1);
      lineStart = i + 1;
      lastBreak = -1;
      return;
    }
    // Without a maxWidth nothing wraps.
    if (char !== ' ' && (i - lineStart + 1) * size > (maxWidth ?? Infinity) && lastBreak > lineStart) {
      lineStarts.push(lastBreak);
      lineStart = lastBreak;
    }
    if (char === ' ') lastBreak = i + 1;
  });
  const lineHeight = size * 1.25;
  return {
    width: maxWidth ?? 0, height: lineStarts.length * lineHeight, ascent: lineHeight * 0.8, descent: lineHeight * 0.2,
    lineHeight, capHeight: 0, xHeight: 0, lines: lineStarts.length, glyphs: [], lineStarts,
  };
}

function install(measure: Measure = measureWords): MeasureTextRequest[] {
  const requests: MeasureTextRequest[] = [];
  const sync = (request: MeasureTextRequest): TextMetrics => {
    requests.push(request);
    return measure(request);
  };
  setTextMeasurer(async (request) => sync(request), sync);
  return requests;
}

function render(element: ReactElement, lang?: string): Scene {
  const Root = () => React.createElement(
    Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 1, lang }, element,
  );
  return mount(Root).renderAt({ value: 0, timescale: 30 }, null).scene;
}

function groupOf(layer: Layer | undefined): GroupContent {
  assert.equal(layer?.content.type, 'group');
  return layer!.content as GroupContent;
}

function textOf(layer: Layer | undefined): TextContent {
  assert.equal(layer?.content.type, 'text');
  return layer!.content as TextContent;
}

const lineTexts = (scene: Scene): TextContent[] =>
  groupOf(scene.layers[0]).layers.map((line) => textOf(groupOf(line).layers[0]));

test('maxWidth wraps where the measurer breaks, one revealed line each', () => {
  const requests = install();
  const scene = render(React.createElement(TextReveal, {
    maxWidth: 100, lineHeight: 30, style: { fontSize: 10, lineHeight: 30, lineBreak: 'phrase' },
    children: 'aaa bbb ccc ddd\nee',
  }), 'ja-JP');
  assert.deepEqual(lineTexts(scene).map((text) => text.text), ['aaa bbb', 'ccc ddd', 'ee']);
  const [line] = groupOf(scene.layers[0]).layers.slice(1);
  assert.equal(line.transform.position.y, 30);
  assert.equal(requests.length, 1);
  assert.equal(requests[0].text, 'aaa bbb ccc ddd\nee');
  assert.equal(requests[0].maxWidth, 100);
  assert.equal(requests[0].style.lineBreak, 'phrase');
  assert.equal(requests[0].style.lang, 'ja-JP');
  // Each line is drawn alone, so it must not wrap again.
  assert.equal(lineTexts(scene)[0].maxWidth, undefined);
});

test('spans are cut at wrapped lines, and color does not measure again', () => {
  const requests = install();
  // The span's color animates; its weight and the breaks do not.
  const Reveal = () => {
    const fill = useCurrentFrame() === 0 ? '#ff0000' : '#00ff00';
    return React.createElement(TextReveal, {
      maxWidth: 80, style: { fontSize: 10 },
      children: ['aaa ', React.createElement(Span, { style: { fill, fontWeight: 700 } }, 'bbb ccc'), ' ddd'],
    });
  };
  const Root = () => React.createElement(
    Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 2 }, React.createElement(Reveal),
  );
  const mounted = mount(Root);
  const [first, second] = lineTexts(mounted.renderAt({ value: 0, timescale: 30 }, null).scene);
  assert.equal(first.text, 'aaa bbb');
  assert.deepEqual(first.style.colorRuns, [{ start: 4, end: 7, color: '#ff0000' }]);
  assert.deepEqual(first.style.fontRuns, [{ start: 4, end: 7, fontWeight: 700 }]);
  assert.equal(second.text, 'ccc ddd');
  assert.deepEqual(second.style.colorRuns, [{ start: 0, end: 3, color: '#ff0000' }]);
  assert.equal(requests[0].style.colorRuns, undefined);
  assert.deepEqual(requests[0].style.fontRuns, [{ start: 4, end: 11, fontWeight: 700 }]);
  const [, next] = lineTexts(mounted.renderAt({ value: 1, timescale: 30 }, null).scene);
  assert.deepEqual(next.style.colorRuns, [{ start: 0, end: 3, color: '#00ff00' }]);
  assert.equal(requests.length, 1);
});

test('without maxWidth only \\n breaks lines, and nothing is measured', () => {
  const requests = install();
  const scene = render(React.createElement(TextReveal, { style: { fontSize: 10 }, children: 'aaa bbb ccc ddd\n\nee ' }));
  assert.deepEqual(lineTexts(scene).map((text) => text.text), ['aaa bbb ccc ddd', '', 'ee ']);
  assert.equal(requests.length, 0);
});

test('maxWidth must be positive, and needs a measurer that reports line starts', () => {
  install();
  for (const maxWidth of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.throws(() => render(React.createElement(TextReveal, { maxWidth, children: 'a' })), /positive `maxWidth`/);
  }
  install((request) => ({ ...measureWords(request), lineStarts: undefined }));
  assert.throws(() => render(React.createElement(TextReveal, { maxWidth: 10, children: 'a b' })), /reports line starts/);
});

// `measureWords`, with a glyph `fontSize` wide for every code point of a
// single line, as the desktop measurer reports them.
function measureGlyphs(request: MeasureTextRequest): TextMetrics {
  const size = request.style.fontSize!;
  const chars = Array.from(request.text);
  return {
    ...measureWords(request),
    width: chars.length * size,
    glyphs: chars.map((text, i) => ({ start: i, end: i + 1, rtl: false, text, x: i * size, width: size, line: 0, inkAscent: 0, inkDescent: 0 })),
  };
}

// The browser measurer: widths, but no glyphs.
function measureWidths(request: MeasureTextRequest): TextMetrics {
  return {
    ...measureGlyphs(request),
    get glyphs(): TextMetrics['glyphs'] { throw new Error('Shaped glyph metrics are unavailable in the browser runtime.'); },
  };
}

interface Unit { text: string; x: number; y: number; textY: number; style: TextContent['style'] }

/** Every revealed unit: its x in its line's mask, the mask's row, and how far its text has risen. */
const unitsOf = (scene: Scene): Unit[] => groupOf(scene.layers[0]).layers.flatMap((mask) =>
  groupOf(mask).layers.map((inner) => {
    const text = textOf(inner);
    return {
      text: text.text, x: inner.transform.position.x, y: mask.transform.position.y,
      textY: inner.transform.position.y, style: text.style,
    };
  }));

const linear = (t: number) => t;
// 10 px glyphs in 30 px lines: at rest a text's baseline is at 24, and hidden one line lower, at 54.
const unitProps = { lineHeight: 30, style: { fontSize: 10 }, easing: linear };

function renderFrames(element: ReactElement) {
  const Root = () => React.createElement(
    Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 300 }, element,
  );
  const mounted = mount(Root);
  return (frame: number): Scene => mounted.renderAt({ value: frame, timescale: 30 }, null).scene;
}

const place = ({ text, x, y }: Unit) => ({ text, x, y });

test("unit='line' draws what no unit draws", () => {
  install(measureGlyphs);
  const props = { ...unitProps, maxWidth: 100, children: 'aaa bbb ccc ddd\nee' };
  const plain = renderFrames(React.createElement(TextReveal, props));
  const line = renderFrames(React.createElement(TextReveal, { ...props, unit: 'line' }));
  for (const frame of [0, 7, 60]) assert.deepEqual(line(frame), plain(frame));
  assert.deepEqual(lineTexts(line(0)).map((text) => text.text), ['aaa bbb', 'ccc ddd', 'ee']);
});

test("unit='word' makes one masked text per word at its measured x", () => {
  const requests = install(measureGlyphs);
  const at = renderFrames(React.createElement(TextReveal, { ...unitProps, unit: 'word', children: 'ab cd\nef' }));
  assert.deepEqual(unitsOf(at(100)).map(place), [
    { text: 'ab', x: 0, y: 0 },
    { text: 'cd', x: 30, y: 0 },
    { text: 'ef', x: 0, y: 30 },
  ]);
  // One mask a line, whatever the number of units.
  assert.equal(groupOf(at(100).layers[0]).layers.length, 2);
  // One measurement a line, none again on another frame.
  assert.deepEqual(requests.map((request) => request.text), ['ab cd', 'ef']);
  at(101);
  assert.equal(requests.length, 2);
  // Each unit is drawn alone, on its baseline.
  assert.equal(unitsOf(at(100))[1].style.lineHeight, undefined);
  assert.deepEqual(unitsOf(at(100)).map((unit) => unit.textY), [24, 24, 24]);
});

test('stagger counts units in reading order across lines', () => {
  install(measureGlyphs);
  const at = renderFrames(React.createElement(TextReveal, {
    ...unitProps, unit: 'word', from: 0, stagger: 4, durationInFrames: 20, children: 'ab cd\nef',
  }));
  assert.deepEqual(unitsOf(at(0)).map((unit) => unit.textY), [54, 54, 54]);
  // Frame 4: the first is a fifth of the way, the second starts, the third (on the next line) waits.
  assert.deepEqual(unitsOf(at(4)).map((unit) => unit.textY), [48, 54, 54]);
  assert.deepEqual(unitsOf(at(14)).map((unit) => unit.textY), [33, 39, 45]);
  assert.deepEqual(unitsOf(at(28)).map((unit) => unit.textY), [24, 24, 24]);
});

test("unit='grapheme' splits by grapheme cluster, not by code point", () => {
  install(measureGlyphs);
  // The family is five code points and one character.
  const at = renderFrames(React.createElement(TextReveal, { ...unitProps, unit: 'grapheme', children: '👨‍👩‍👧aé' }));
  assert.deepEqual(unitsOf(at(100)).map(place), [
    { text: '👨‍👩‍👧', x: 0, y: 0 },
    { text: 'a', x: 50, y: 0 },
    { text: 'é', x: 60, y: 0 },
  ]);
});

test("unit='phrase' reveals the phrases of each line", () => {
  install(measureGlyphs);
  const text = '世界中の情報を整理し、';
  const at = renderFrames(React.createElement(TextReveal, { ...unitProps, unit: 'phrase', children: `${text}\n${text}` }));
  const parts = phrases(text);
  assert.deepEqual(parts, ['世界中の', '情報を', '整理し、']);
  assert.deepEqual(unitsOf(at(100)).map(place), [
    { text: '世界中の', x: 0, y: 0 },
    { text: '情報を', x: 40, y: 0 },
    { text: '整理し、', x: 70, y: 0 },
    { text: '世界中の', x: 0, y: 30 },
    { text: '情報を', x: 40, y: 30 },
    { text: '整理し、', x: 70, y: 30 },
  ]);
});

test('whitespace and empty lines make no unit and use no stagger step', () => {
  install(measureGlyphs);
  for (const unit of ['word', 'grapheme'] as const) {
    const at = renderFrames(React.createElement(TextReveal, {
      ...unitProps, unit, from: 0, stagger: 4, durationInFrames: 20, children: '  a   b  \n\n \nc',
    }));
    // The spaces keep their width, and the empty lines their rows.
    assert.deepEqual(unitsOf(at(100)).map(place), [
      { text: 'a', x: 20, y: 0 },
      { text: 'b', x: 60, y: 0 },
      { text: 'c', x: 0, y: 90 },
    ]);
    // `c` is the third unit: it starts on frame 8.
    assert.deepEqual(unitsOf(at(8)).map((u) => u.textY), [42, 48, 54]);
    assert.deepEqual(unitsOf(at(9)).map((u) => u.textY), [40.5, 46.5, 52.5]);
  }
  assert.deepEqual(unitsOf(renderFrames(React.createElement(TextReveal, { ...unitProps, unit: 'word', children: ' \n' }))(0)), []);
});

test('align pivots the whole line, so units rest where the line would', () => {
  install(measureGlyphs);
  const xs = (align: number) => unitsOf(renderFrames(React.createElement(TextReveal, {
    ...unitProps, unit: 'word', align, children: 'ab cd\nefg',
  }))(100)).map((unit) => unit.x);
  assert.deepEqual(xs(0), [0, 30, 0]);
  assert.deepEqual(xs(0.5), [-25, 5, -15]);
  assert.deepEqual(xs(1), [-50, -20, -30]);
});

test("direction='out' starts at rest and hides units in order", () => {
  install(measureGlyphs);
  const at = renderFrames(React.createElement(TextReveal, {
    ...unitProps, unit: 'word', direction: 'out', from: 0, stagger: 4, durationInFrames: 20, children: 'ab cd',
  }));
  assert.deepEqual(unitsOf(at(0)).map((unit) => unit.textY), [24, 24]);
  assert.deepEqual(unitsOf(at(4)).map((unit) => unit.textY), [30, 24]);
  assert.deepEqual(unitsOf(at(24)).map((unit) => unit.textY), [54, 54]);
});

test('a span keeps its style in each unit it covers, and is measured with its font', () => {
  const requests = install(measureGlyphs);
  const at = renderFrames(React.createElement(TextReveal, {
    ...unitProps, unit: 'word',
    children: ['ab ', React.createElement(Span, { style: { fill: '#ff0000', fontWeight: 700 } }, 'cd ef'), ' gh'],
  }));
  const [ab, cd, ef, gh] = unitsOf(at(100));
  assert.deepEqual([ab, cd, ef, gh].map((unit) => unit.text), ['ab', 'cd', 'ef', 'gh']);
  assert.deepEqual(ab.style.colorRuns ?? [], []);
  assert.deepEqual(cd.style.colorRuns, [{ start: 0, end: 2, color: '#ff0000' }]);
  assert.deepEqual(cd.style.fontRuns, [{ start: 0, end: 2, fontWeight: 700 }]);
  assert.deepEqual(ef.style.colorRuns, [{ start: 0, end: 2, color: '#ff0000' }]);
  assert.deepEqual(gh.style.fontRuns ?? [], []);
  assert.deepEqual(requests[0].style.fontRuns, [{ start: 3, end: 8, fontWeight: 700 }]);
  assert.equal(requests[0].style.colorRuns, undefined);
});

test('without glyph positions a unit sits after the measured text before it', () => {
  const requests = install(measureWidths);
  const at = renderFrames(React.createElement(TextReveal, {
    ...unitProps, unit: 'word', align: 1, children: 'ab cd  ef',
  }));
  assert.deepEqual(unitsOf(at(100)).map(place), [
    { text: 'ab', x: -90, y: 0 },
    { text: 'cd', x: -60, y: 0 },
    { text: 'ef', x: -20, y: 0 },
  ]);
  assert.deepEqual(requests.map((request) => request.text), ['ab cd  ef', 'ab ', 'ab cd  ']);
});

test('wrapped lines are split into units too', () => {
  install(measureGlyphs);
  const at = renderFrames(React.createElement(TextReveal, {
    ...unitProps, unit: 'word', maxWidth: 100, children: 'aaa bbb ccc ddd',
  }));
  assert.deepEqual(unitsOf(at(100)).map(place), [
    { text: 'aaa', x: 0, y: 0 },
    { text: 'bbb', x: 40, y: 0 },
    { text: 'ccc', x: 0, y: 30 },
    { text: 'ddd', x: 40, y: 30 },
  ]);
});

test('an unknown unit throws', () => {
  install(measureGlyphs);
  assert.throws(
    () => renderFrames(React.createElement(TextReveal, { unit: 'letter' as never, children: 'a' }))(0),
    /unit must be 'line', 'word', 'phrase', or 'grapheme', got "letter"/,
  );
});
