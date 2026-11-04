// <Theme> font and color names in <Dialogue> subtitles.

import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';
import type { ReactNode } from 'react';

import { Assets, Composition, Span, Text, Theme } from '@celesta.studio/react';
import type { AssetReference, Layer, LayerContent, Paint, TextMetrics, TextStyle } from '@celesta.studio/react';
import { Character, CharacterView, Dialogue } from '../src/index.js';
import type { CharacterSubtitle, CharacterViewReference, DialogueProps, SubtitleRenderProps } from '../src/index.js';
import { mount, setTextMeasurer } from '@celesta.studio/react/internal';
import type { MeasureTextRequest } from '@celesta.studio/react/internal';

const h = React.createElement;
const fonts = { serif: 'Noto Serif JP', bold: 'Inter' };
const colors = { paper: '#f4f1ea', accent: '#ff3300' };
const style: TextStyle = { fontSize: 10, fontFamily: 'serif', fill: { type: 'solid', color: 'paper' } };

// The line goes in as createElement's children, as JSX would pass it; they
// replace the `children: undefined` that DialogueProps requires.
const dialogue = (props: Omit<DialogueProps, 'children'>, ...children: ReactNode[]) =>
  h(Dialogue, { ...props, children: undefined }, ...children);

/** The text content of `layer`. */
function textOf(layer: Layer | undefined): Extract<LayerContent, { type: 'text' }> {
  assert.ok(layer && layer.content.type === 'text', 'a text layer');
  return layer.content;
}

/** The color of a solid paint. */
function colorOf(paint: Paint | null | undefined): string {
  assert.ok(paint && paint.type === 'solid', 'a solid paint');
  return paint.color;
}

function installMeasurer() {
  const requests: MeasureTextRequest[] = [];
  const sync = (request: MeasureTextRequest): TextMetrics => {
    requests.push(request);
    const size = request.style.fontSize ?? 10;
    return {
      width: Array.from(request.text).length * size, height: size * 1.25, ascent: size, descent: size * 0.25,
      lineHeight: size * 1.25, capHeight: size * 0.7, xHeight: size * 0.5, lines: 1, glyphs: [],
    };
  };
  setTextMeasurer(async (request) => sync(request), sync);
  return requests;
}

function find(layers: Layer[], test: (layer: Layer) => boolean): Layer | undefined {
  for (const layer of layers) {
    if (test(layer)) return layer;
    if (layer.content.type === 'group') {
      const found = find(layer.content.layers, test);
      if (found) return found;
    }
  }
  return undefined;
}

function scene(subtitle: CharacterSubtitle) {
  const mira = React.createRef<AssetReference>();
  const view = React.createRef<CharacterViewReference>();
  const Root = () => h(Composition, { width: 640, height: 360, fps: 30, durationInFrames: 30 },
    h(Assets, null, h(Character, {
      ref: mira, name: 'mira', portrait: { defaultExpression: 'calm', expressions: { calm: 'calm.png' } }, subtitle,
    })),
    h(Theme, { fonts, colors },
      h(CharacterView, { ref: view, character: mira }),
      dialogue({ character: view, id: 'line' }, 'Hello ', h(Span, { color: 'accent', fontFamily: 'bold' }, 'world'))));
  return mount(Root).renderAt({ value: 0, timescale: 30 }, null).scene;
}

test('a plain subtitle draws with the theme names of its line looked up', () => {
  const layers = scene({ style }).layers;
  const subtitle = textOf(find(layers, (layer) => layer.id === 'line.subtitle'));
  assert.equal(subtitle.text, 'Hello world');
  assert.equal(subtitle.style.fontFamily, 'Noto Serif JP');
  assert.equal(colorOf(subtitle.style.fill), '#f4f1ea');
  assert.deepEqual(subtitle.style.colorRuns, [{ start: 6, end: 11, color: '#ff3300' }]);
  assert.deepEqual(subtitle.style.fontRuns, [{ start: 6, end: 11, fontFamily: 'Inter' }]);
});

test('subtitle.render measures and gets its style with theme names looked up', () => {
  const requests = installMeasurer();
  let given: SubtitleRenderProps | undefined;
  const render = (props: SubtitleRenderProps) => {
    given = props;
    void props.metrics;
    return h(Text, { style: props.style, children: props.content });
  };
  const layers = scene({ style, render }).layers;
  assert.ok(given);
  assert.equal(given.style.fontFamily, 'Noto Serif JP');
  assert.equal(colorOf(given.style.fill), '#f4f1ea');
  const request = requests.at(-1);
  assert.ok(request);
  assert.equal(request.style.fontFamily, 'Noto Serif JP');
  assert.deepEqual(request.style.fontRuns, [{ start: 6, end: 11, fontFamily: 'Inter' }]);
  const drawn = textOf(find(layers, (layer) => layer.content.type === 'text'));
  assert.deepEqual(drawn.style.colorRuns, [{ start: 6, end: 11, color: '#ff3300' }]);
});
