// <Theme> font and color names in fitted and animated text.

import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';
import type { ReactElement } from 'react';

import { Composition, Span, Text, Theme } from '@celesta.studio/react';
import type { Layer, LayerContent, Scene, TextMetrics } from '@celesta.studio/react';
import { TextBox, TextReveal, fitText, useFitText, useTypewriter } from '../src/index.js';
import type { FitTextResult, Typewriter } from '../src/index.js';
import { mount, setTextMeasurer } from '@celesta.studio/react/internal';
import type { MeasureTextRequest } from '@celesta.studio/react/internal';

type GroupContent = Extract<LayerContent, { type: 'group' }>;
type TextContent = Extract<LayerContent, { type: 'text' }>;

const h = React.createElement;
const fonts = { mono: ['JetBrains Mono', 'Noto Sans JP'], display: 'Inter' };
const colors = { paper: '#f4f1ea', accent: '#ff3300' };
const MONO = ['JetBrains Mono', 'Noto Sans JP'];

// Every character is `fontSize` wide; lines start at each `\n`.
function measure({ text, style }: MeasureTextRequest): TextMetrics {
  const size = style.fontSize ?? 10;
  const lines = text.split('\n');
  const lineHeight = size * 1.25;
  return {
    width: Math.max(...lines.map((line) => Array.from(line).length)) * size,
    height: lines.length * lineHeight,
    ascent: size,
    descent: size * 0.25,
    lineHeight,
    capHeight: 0,
    xHeight: 0,
    lines: lines.length,
    glyphs: [],
    lineStarts: [0],
  };
}

function install(): MeasureTextRequest[] {
  const requests: MeasureTextRequest[] = [];
  const sync = (request: MeasureTextRequest): TextMetrics => {
    requests.push(request);
    return measure(request);
  };
  setTextMeasurer(async (request) => sync(request), sync);
  return requests;
}

function render(element: ReactElement): Scene {
  const Root = () => h(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 1 }, element);
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

const box = { width: 400, height: 100, minFontSize: 10, maxFontSize: 30 };

test('TextBox measures and draws with theme names looked up', () => {
  const requests = install();
  const scene = render(h(Theme, { fonts, colors },
    h(TextBox, {
      ...box, style: { fontFamily: 'mono', fill: { type: 'solid', color: 'paper' } },
      children: ['Hi ', h(Span, { fontFamily: 'display', color: 'accent' }, 'there')],
    })));
  for (const request of requests) {
    assert.deepEqual(request.style.fontFamily, MONO);
    assert.deepEqual(request.style.fontRuns, [{ start: 3, end: 8, fontFamily: 'Inter' }]);
  }
  const label = textOf(groupOf(scene.layers[0]).layers[0]);
  assert.deepEqual(label.style.fontFamily, MONO);
  const fill = label.style.fill;
  assert.ok(fill?.type === 'solid');
  assert.equal(fill.color, '#f4f1ea');
  assert.deepEqual(label.style.colorRuns, [{ start: 3, end: 8, color: '#ff3300' }]);
});

test('useFitText adds its theme option to the <Theme>s around it', () => {
  const requests = install();
  let fit: FitTextResult | undefined;
  function Probe() {
    fit = useFitText(h(Span, { color: 'accent' }, 'Hi'), {
      ...box, style: { fontFamily: 'mono' }, theme: { fonts: { mono: 'Fira Code' } },
    });
    return null;
  }
  render(h(Theme, { fonts, colors }, h(Probe)));
  assert.equal(requests[0].style.fontFamily, 'Fira Code');
  assert.equal(fit?.style.fontFamily, 'Fira Code');
});

test('fitText looks names up in its theme option, and only there', async () => {
  const requests = install();
  const fit = await fitText(h(Span, { color: 'accent' }, 'Hi'), { ...box, style: { fontFamily: 'mono' }, theme: { fonts, colors } });
  assert.deepEqual(requests[0].style.fontFamily, MONO);
  assert.deepEqual(fit.style.fontFamily, MONO);
  // Without one, a name is used as a family name.
  await fitText('Hi', { ...box, style: { fontFamily: 'mono' } });
  assert.equal(requests[requests.length - 1].style.fontFamily, 'mono');
});

test('TextReveal looks names up in its style and spans', () => {
  const requests = install();
  const scene = render(h(Theme, { fonts, colors },
    h(TextReveal, {
      maxWidth: 1000, style: { fontSize: 10, fontFamily: 'mono' },
      children: ['aa ', h(Span, { color: 'accent', fontFamily: 'display' }, 'bb')],
    })));
  assert.deepEqual(requests[0].style.fontFamily, MONO);
  assert.deepEqual(requests[0].style.fontRuns, [{ start: 3, end: 5, fontFamily: 'Inter' }]);
  const line = textOf(groupOf(groupOf(scene.layers[0]).layers[0]).layers[0]);
  assert.deepEqual(line.style.fontFamily, MONO);
  assert.deepEqual(line.style.colorRuns, [{ start: 3, end: 5, color: '#ff3300' }]);
});

test('useTypewriter accepts spans colored by theme name', () => {
  install();
  let typed: Typewriter | undefined;
  function Typing() {
    typed = useTypewriter(['ab', h(Span, { key: 's', color: 'accent' }, 'cd')], { from: -10 });
    return h(Text, { children: typed.text });
  }
  render(h(Theme, { colors }, h(Typing)));
  assert.equal(typed?.text, 'abcd');
});
