import assert from 'node:assert/strict';
import { beforeEach, test } from 'vitest';
import * as React from 'react';
import type { ReactElement } from 'react';

import { Composition, Sequence } from '@celesta.studio/react';
import type { Layer, LayerContent, Paint } from '@celesta.studio/react';
import { mount } from '@celesta.studio/react/internal';
import { StatCard } from '../src/index.js';
import type { StatCardProps } from '../src/index.js';
import { installStubMeasurer } from './text-metrics-stub.mjs';

const h = React.createElement;

beforeEach(() => {
  installStubMeasurer();
});

function render(element: ReactElement) {
  const composition = mount(() => h(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 300 }, element));
  return (frame = 0): Layer[] => composition.renderAt({ value: frame, timescale: 30 }, null).scene.layers;
}

/** A layer whose content is of type `T`. */
type LayerOf<T extends LayerContent['type']> = Layer & { content: Extract<LayerContent, { type: T }> };

/** `layer`, checked to have content of `type`. */
function layerOf<T extends LayerContent['type']>(layer: Layer | undefined, type: T): LayerOf<T> {
  assert.ok(layer, `a ${type} layer`);
  assert.equal(layer.content.type, type);
  return layer as LayerOf<T>;
}

/** The color of a solid paint. */
function colorOf(paint: Paint | null | undefined): string {
  assert.ok(paint && paint.type === 'solid', 'a solid paint');
  return paint.color;
}

function parts(card: Layer | undefined) {
  const layers = layerOf(card, 'group').content.layers;
  const background = layers.find((layer) => layer.content.type === 'rect' && layer.content.fill);
  const content = layers.find((layer) => layer.content.type === 'group');
  return { background: layerOf(background, 'rect'), texts: layerOf(content, 'group').content.layers.map((text) => layerOf(text, 'text')) };
}

// Text sizes that make the stub's widths easy: 10 px per character for the
// value, 10 px for the label (20 px for Japanese), 8 px for the note.
const styles: Partial<StatCardProps> = { fill: '#000000', valueStyle: { fontSize: 20 }, labelStyle: { fontSize: 20 }, noteStyle: { fontSize: 16 } };

test('a card stacks the label, the number, and the note inside its padding', () => {
  const [card] = render(h(StatCard, { ...styles, x: 100, y: 200, fill: '#182030', label: '総数', value: 1280, suffix: ' 件', note: 'per week', padding: 20, gap: 10 }))(60);
  const { background, texts } = parts(card);
  // "1,280 件": 6 × 10 + 20; the label is 40, the note 64. Lines 25 + 25 + 20 and two gaps.
  assert.deepEqual([background.content.width, background.content.height], [120, 130]);
  assert.equal(background.content.cornerRadius, 16);
  assert.deepEqual(texts.map((text) => text.content.text), ['総数', '1,280 件', 'per week']);
  assert.deepEqual(texts.map((text) => text.transform.position), [{ x: 0, y: 20 }, { x: 0, y: 55 }, { x: 0, y: 86 }]);
  assert.ok(texts.every((text) => text.content.baselineAnchor));
});

test('the card is sized for the wider end of the count, so it does not grow while counting', () => {
  const card = h(StatCard, { ...styles, value: 100000, padding: 0, durationInFrames: 30 });
  const widths = [0, 5, 30].map((frame) => parts(render(card)(frame)[0]).background.content.width);
  assert.deepEqual(widths, [70, 70, 70]);
  const down = h(StatCard, { ...styles, from: -1000, value: 0, padding: 0 });
  assert.equal(parts(render(down)(30)[0]).background.content.width, 60);
});

test('align right keeps the number\'s last digit at the right edge', () => {
  const [card] = render(h(StatCard, { ...styles, value: 1000, align: 'right', padding: 10, label: 'abc' }))(3);
  const { texts } = parts(card);
  assert.ok(texts.every((text) => text.transform.position.x === 50 && text.transform.anchor.x === 1));
  const [centered] = render(h(StatCard, { ...styles, value: 1000, align: 'center', padding: 10, label: 'abc' }))(3);
  assert.ok(parts(centered).texts.every((text) => text.transform.position.x === 25 && text.transform.anchor.x === 0.5));
});

test('a card without a label or a note holds only the number', () => {
  const [card] = render(h(StatCard, { ...styles, value: 42, padding: 8 }))(60);
  const { background, texts } = parts(card);
  assert.equal(texts.length, 1);
  assert.deepEqual([background.content.width, background.content.height], [36, 41]);
});

test('a given width and height center the lines in the card', () => {
  const [card] = render(h(StatCard, { ...styles, value: 42, padding: 10, width: 300, height: 145, align: 'center' }))(60);
  const { background, texts } = parts(card);
  assert.deepEqual([background.content.width, background.content.height], [300, 145]);
  // 125 px inside, a 25 px line in the middle: its top at 50, its baseline 20 below.
  assert.deepEqual(texts[0].transform.position, { x: 140, y: 70 });
});

test('the formatter, decimals, locale, and color reach the number', () => {
  const [card] = render(h(StatCard, { ...styles, value: 1234.5, decimals: 1, locale: 'de-DE', color: '#ffffff', durationInFrames: 0 }))(0);
  const [text] = parts(card).texts;
  assert.equal(text.content.text, '1.234,5');
  assert.equal(colorOf(text.content.style.fill), '#ffffff');
  const [formatted] = render(h(StatCard, { ...styles, value: 3, formatter: (value) => `${value}/5`, durationInFrames: 0 }))(0);
  assert.equal(parts(formatted).texts[0].content.text, '3/5');
});

test('a card rejects values that are not finite and negative sizes', () => {
  assert.throws(() => render(h(StatCard, { value: Infinity }))(), /<StatCard> value must be a finite number/);
  assert.throws(() => render(h(StatCard, { value: 1, gap: -1 }))(), /<StatCard> gap must be/);
  assert.throws(() => render(h(StatCard, { value: 1, padding: -1 }))(), /<StatCard> padding top must be/);
});

test('a card seeks to the same frame it plays to, inside a Sequence', () => {
  const scene = h(Sequence, { from: 10 }, h(StatCard, { ...styles, value: 500, label: 'x' }));
  const played = render(scene);
  for (let frame = 0; frame <= 30; frame += 1) played(frame);
  assert.deepEqual(played(30), render(scene)(30));
});
