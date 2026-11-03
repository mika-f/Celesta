import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';
import type { ReactElement } from 'react';

import { Composition, Easings, Sequence } from '@celesta.studio/react';
import type { Layer, LayerContent, Paint } from '@celesta.studio/react';
import { mount } from '@celesta.studio/react/internal';
import { Counter, formatNumber } from '../src/index.js';

const h = React.createElement;

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

const textAt = (element: ReactElement, frame: number) => layerOf(render(element)(frame)[0], 'text').content.text;

test('formatNumber groups digits for en-US unless given another locale', () => {
  assert.equal(formatNumber(1234567), '1,234,567');
  assert.equal(formatNumber(1234.5, { locale: 'de-DE', decimals: 1 }), '1.234,5');
  assert.equal(formatNumber(1234.5, { locale: 'ja-JP', decimals: 1 }), '1,234.5');
});

test('formatNumber always shows the decimal places it rounds to', () => {
  assert.equal(formatNumber(12, { decimals: 1 }), '12.0');
  assert.equal(formatNumber(0.125, { decimals: 2 }), '0.13');
  assert.equal(formatNumber(2.5), '3');
  assert.equal(formatNumber(-2.5), '-2');
});

test('formatNumber prints a value that rounds to 0 without a minus sign', () => {
  assert.equal(formatNumber(-0), '0');
  assert.equal(formatNumber(-0.004, { decimals: 2 }), '0.00');
  assert.equal(formatNumber(-0.4), '0');
});

test('formatNumber puts prefix and suffix around the number, and around a formatter\'s text', () => {
  assert.equal(formatNumber(1280, { suffix: ' 件' }), '1,280 件');
  assert.equal(formatNumber(-42, { prefix: '$' }), '$-42');
  const seen: number[] = [];
  const text = formatNumber(3.14159, { decimals: 2, prefix: '[', suffix: ']', formatter: (value) => { seen.push(value); return `π≈${value}`; } });
  assert.equal(text, '[π≈3.14]');
  assert.deepEqual(seen, [3.14]);
});

test('formatNumber rejects values that are not finite and bad decimals', () => {
  for (const value of [Number.NaN, Infinity, -Infinity]) {
    assert.throws(() => formatNumber(value), /formatNumber\(\) value must be a finite number/);
  }
  for (const decimals of [-1, 1.5, 21]) {
    assert.throws(() => formatNumber(1, { decimals }), /decimals must be an integer from 0 to 20/);
  }
});

test('a Counter counts from `from` to `value` over its duration, starting at its delay', () => {
  const counter = h(Counter, { value: 100, delay: 10, durationInFrames: 10, easing: (t) => t });
  assert.deepEqual([0, 10, 15, 20, 40].map((frame) => textAt(counter, frame)), ['0', '0', '50', '100', '100']);
});

test('a Counter counts down, through negative numbers, and with decimals', () => {
  const linear = (t: number) => t;
  assert.equal(textAt(h(Counter, { from: 10, value: -10, durationInFrames: 20, easing: linear }), 5), '5');
  assert.equal(textAt(h(Counter, { from: 10, value: -10, durationInFrames: 20, easing: linear }), 15), '-5');
  assert.equal(textAt(h(Counter, { value: 1, decimals: 2, durationInFrames: 3, easing: linear }), 1), '0.33');
  assert.equal(textAt(h(Counter, { value: 0, from: 0 }), 10), '0');
});

test('a Counter passing through 0 from below never prints -0', () => {
  const counter = h(Counter, { from: -1, value: 1, durationInFrames: 20, easing: (t) => t });
  assert.deepEqual([9, 10].map((frame) => textAt(counter, frame)), ['0', '0']);
});

test('a Counter defaults to useCountUp\'s timing: 30 frames of easeOutExpo', () => {
  const counter = h(Counter, { value: 1000 });
  assert.equal(textAt(counter, 6), formatNumber(1000 * Easings.easeOutExpo(0.2)));
  assert.equal(textAt(counter, 30), '1,000');
});

test('a Counter of 0 frames jumps to its value at its delay', () => {
  const counter = h(Counter, { value: 5, delay: 3, durationInFrames: 0 });
  assert.deepEqual([2, 3].map((frame) => textAt(counter, frame)), ['0', '5']);
});

test('a Counter passes its other props to its Text, so the anchor keeps the last digit in place', () => {
  const layer = layerOf(render(h(Counter, { x: 1800, y: 100, anchorX: 1, value: 1280, suffix: ' 件', locale: 'en-US', fontSize: 96, color: '#ffffff' }))(60)[0], 'text');
  assert.equal(layer.content.text, '1,280 件');
  assert.deepEqual(layer.transform.position, { x: 1800, y: 100 });
  assert.equal(layer.transform.anchor.x, 1);
  assert.equal(layer.content.style.fontSize, 96);
  assert.equal(colorOf(layer.content.style.fill), '#ffffff');
});

test('a Counter with a formatter prints its text', () => {
  assert.equal(textAt(h(Counter, { value: 90, durationInFrames: 0, formatter: (value) => `${value / 60} min` }), 0), '1.5 min');
});

test('a Counter rejects values that are not finite', () => {
  assert.throws(() => render(h(Counter, { value: Number.NaN }))(), /<Counter> value must be a finite number, not NaN/);
  assert.throws(() => render(h(Counter, { value: 1, from: Infinity }))(), /<Counter> from must be a finite number, not Infinity/);
  assert.throws(() => render(h(Counter, { value: 1, durationInFrames: -1 }))(), /<Counter> durationInFrames must be/);
});

test('a Counter seeks to the same text it plays to, inside a nested Sequence', () => {
  const scene = h(Sequence, { from: 10 }, h(Sequence, { from: 5 }, h(Counter, { value: 999, durationInFrames: 20 })));
  const played = render(scene);
  for (let frame = 0; frame <= 25; frame += 1) played(frame);
  assert.deepEqual(played(25), render(scene)(25));
  const outer = layerOf(render(scene)(15)[0], 'group');
  const inner = layerOf(outer.content.layers[0], 'group');
  assert.equal(layerOf(inner.content.layers[0], 'text').content.text, '0');
});
