import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';
import type { ReactElement } from 'react';

import { Composition, Span, useCurrentFrame } from '@celesta.studio/react';
import type { Layer, LayerContent, Scene, TextMetrics } from '@celesta.studio/react';
import { TextReveal } from '../src/index.js';
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
