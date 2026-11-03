// <Code> draws with the font's own features, ligatures included; a style's
// fontFeatures reach every line and the measurements its layout uses.

import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';

import { Composition } from '@celesta.studio/react';
import type { Layer, LayerContent, TextMetrics, TextStyle } from '@celesta.studio/react';
import { mount, setTextMeasurer } from '@celesta.studio/react/internal';
import type { MeasureTextRequest } from '@celesta.studio/react/internal';
import { Code } from '../src/index.js';

type TextContent = Extract<LayerContent, { type: 'text' }>;

// Every code point is 10 px wide.
function measure({ text, style }: MeasureTextRequest): TextMetrics {
  const lines = text.split('\n');
  const lineHeight = style.lineHeight ?? 36;
  return {
    width: Math.max(...lines.map((line) => Array.from(line).length)) * 10,
    height: lines.length * lineHeight, ascent: lineHeight * 0.75, descent: lineHeight * 0.25,
    lineHeight, capHeight: 0, xHeight: 0, lines: lines.length, lineStarts: [], glyphs: [],
  };
}

function textLayers(layers: readonly Layer[]): TextContent[] {
  return layers.flatMap((layer) => layer.content.type === 'group'
    ? textLayers(layer.content.layers)
    : layer.content.type === 'text' ? [layer.content] : []);
}

/** The text layers and measurement requests of one frame of `<Code style={style}>`. */
function render(style?: TextStyle): { texts: TextContent[]; requests: MeasureTextRequest[] } {
  const requests: MeasureTextRequest[] = [];
  setTextMeasurer(async (request) => measure(request), (request) => {
    requests.push(request);
    return measure(request);
  });
  const Root = () => React.createElement(Composition, { width: 800, height: 300, fps: 30, durationInFrames: 1 },
    React.createElement(Code, { language: 'ts', ...(style ? { style } : {}), children: 'a === b;\nf = () => x != y;\n' }));
  const { scene } = mount(Root).renderAt({ value: 0, timescale: 30 }, null);
  return { texts: textLayers(scene.layers), requests };
}

test('Code sets no features of its own', () => {
  const { texts, requests } = render();
  assert.equal(texts.length, 2);
  assert.ok(requests.length > 0);
  for (const { style } of [...texts, ...requests]) assert.equal(style.fontFeatures, undefined);
});

test('a Code style\'s features reach every line and measurement', () => {
  const { texts, requests } = render({ fontFeatures: { liga: false, calt: false } });
  assert.equal(texts.length, 2);
  for (const { style } of [...texts, ...requests]) {
    assert.deepEqual(style.fontFeatures, { liga: false, calt: false });
  }
});
