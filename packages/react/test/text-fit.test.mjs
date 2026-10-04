import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';

import { Composition, TextBox, fitText, useFitText } from '../dist/index.js';
import { mount } from '../dist/render.js';
import { setTextMeasurer } from '../dist/text-measure.js';

// Monospaced square glyphs: each character is `fontSize` wide, and lines
// wrap by character count at `maxWidth`, but never inside a character.
function measureMonospace({ text, style, maxWidth }) {
  const size = style.fontSize;
  const lineHeight = style.lineHeight ?? size * 1.25;
  const perLine = Math.max(1, Math.floor(maxWidth / size));
  const paragraphs = text.split('\n');
  const rows = paragraphs.flatMap((line) => {
    const chars = Array.from(line);
    if (chars.length === 0) return [0];
    const out = [];
    for (let i = 0; i < chars.length; i += perLine) out.push(Math.min(perLine, chars.length - i));
    return out;
  });
  return {
    width: Math.max(...rows) * size,
    height: rows.length * lineHeight,
    ascent: lineHeight * 0.8,
    descent: lineHeight * 0.2,
    lineHeight,
    lines: rows.length,
    glyphs: [],
  };
}

function install() {
  const requests = [];
  const sync = (request) => {
    requests.push(request);
    return measureMonospace(request);
  };
  setTextMeasurer(async (request) => sync(request), sync);
  return requests;
}

function render(element, frame = 0) {
  const Root = () => React.createElement(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 1 }, element);
  return mount(Root).renderAt({ value: frame, timescale: 30 }, null).scene;
}

test('short text keeps the largest size in one measurement', async () => {
  const requests = install();
  const fit = await fitText('Hi', { width: 400, height: 100, minFontSize: 10, maxFontSize: 64 });
  assert.equal(fit.fontSize, 64);
  assert.equal(fit.fits, true);
  assert.equal(requests.length, 1);
  assert.equal(requests[0].maxWidth, 400);
  assert.deepEqual(fit.style, { fontSize: 64 });
});

test('long text shrinks to the largest size that fits the box and line limit', async () => {
  install();
  const text = '字幕の文言を差し替えても枠に収まる';  // 17 characters
  const fit = await fitText(text, { width: 400, height: 200, minFontSize: 10, maxFontSize: 96, maxLines: 2 });
  // 2 lines of 9 characters: 400 / 9 = 44.4 → 44px; 2 × 55 = 110 ≤ 200.
  assert.equal(fit.fontSize, 44);
  assert.equal(fit.fits, true);
  assert.equal(fit.metrics.lines, 2);
  const larger = measureMonospace({ text, style: { fontSize: 45 }, maxWidth: 400 });
  assert.ok(larger.lines > 2 || larger.height > 200);
});

test('height and line height limit multi-line text', async () => {
  install();
  const text = 'Line one\nLine two\nLine three';
  const fit = await fitText(text, { width: 1000, height: 90, minFontSize: 8, maxFontSize: 80, lineHeight: 1.5 });
  // Three lines of 1.5 × size must fit 90px: 20px.
  assert.equal(fit.fontSize, 20);
  assert.equal(fit.style.lineHeight, 30);
  assert.equal(fit.metrics.height, 90);
});

test('text that does not fit at the minimum reports it', async () => {
  install();
  const fit = await fitText('x'.repeat(10000), { width: 200, height: 50, minFontSize: 12, maxFontSize: 48, maxLines: 2 });
  assert.equal(fit.fontSize, 12);
  assert.equal(fit.fits, false);
});

test('empty text fits at the largest size', async () => {
  install();
  const fit = await fitText('', { width: 200, height: 100, minFontSize: 12, maxFontSize: 48 });
  assert.equal(fit.fontSize, 48);
  assert.equal(fit.fits, true);
});

test('steps include both ends and round', async () => {
  install();
  // 10 characters on one line must fit 335px: 33.5px at a 0.1 step.
  const fit = await fitText('0123456789', { width: 335, height: 100, minFontSize: 10, maxFontSize: 40.05, step: 0.1, maxLines: 1 });
  assert.equal(fit.fontSize, 33.5);
  const top = await fitText('0', { width: 333, height: 100, minFontSize: 10, maxFontSize: 40.05, step: 0.1 });
  assert.equal(top.fontSize, 40.05);
  const fixed = await fitText('0123456789', { width: 50, height: 50, minFontSize: 20, maxFontSize: 20 });
  assert.equal(fixed.fontSize, 20);
  assert.equal(fixed.fits, false);
});

test('invalid boxes and ranges are rejected', async () => {
  install();
  const base = { width: 100, height: 100, minFontSize: 10, maxFontSize: 20 };
  for (const bad of [
    { width: 0 }, { height: -1 }, { width: Number.NaN }, { height: Infinity },
    { minFontSize: 0 }, { maxFontSize: 5 }, { maxLines: 0 }, { maxLines: 1.5 },
    { lineHeight: 0 }, { step: 0 },
  ]) {
    await assert.rejects(fitText('a', { ...base, ...bad }), RangeError, JSON.stringify(bad));
  }
});

test('useFitText measures with the composition fonts and caches by value', () => {
  const requests = install();
  let result;
  function Probe({ text }) {
    result = useFitText(text, { width: 300, height: 100, minFontSize: 10, maxFontSize: 30, style: { fontFamily: 'Inter' } });
    return null;
  }
  const mounted = mount(() => React.createElement(Composition, { width: 400, height: 200, fps: 30, durationInFrames: 2 },
    React.createElement(Probe, { text: 'hello' })));
  mounted.renderAt({ value: 0, timescale: 30 }, null);
  const count = requests.length;
  mounted.renderAt({ value: 1, timescale: 30 }, null);
  assert.equal(requests.length, count);
  assert.equal(result.fontSize, 30);
  assert.equal(requests[0].style.fontFamily, 'Inter');
});

test('TextBox draws the fitted text with the measured style and wrapping', () => {
  install();
  const scene = render(React.createElement(TextBox, {
    id: 'caption', x: 100, y: 50, width: 400, height: 200, minFontSize: 10, maxFontSize: 96, maxLines: 2,
    verticalAlign: 'middle', style: { align: 'center' },
  }, '字幕の文言を差し替えても枠に収まる'));
  const group = scene.layers[0];
  assert.equal(group.id, 'caption');
  assert.deepEqual(group.transform.position, { x: 100, y: 50 });
  assert.equal(group.content.clip, undefined);
  const [label] = group.content.layers;
  assert.equal(label.content.type, 'text');
  assert.equal(label.content.maxWidth, 400);
  assert.equal(label.content.style.fontSize, 44);
  assert.equal(label.content.style.align, 'center');
  assert.equal(label.content.baselineAnchor, true);
  // Two 55px lines centred in 200px, placed by the first baseline.
  assert.equal(label.transform.position.y, (200 - 110) / 2 + 44);
});

test('TextBox overflow clips, shows, or fails', () => {
  install();
  const props = { width: 100, height: 100, minFontSize: 20, maxFontSize: 40, maxLines: 2, verticalAlign: 'bottom' };
  const long = 'abcdefghijklmnopqrstuvwxyz';
  const clipped = render(React.createElement(TextBox, props, long)).layers[0];
  // The two 25px lines kept sit at the bottom of the 100px box.
  assert.deepEqual(clipped.content.clip, { x: 0, y: 50, width: 100, height: 50, cornerRadius: 0 });
  assert.equal(clipped.content.layers[0].transform.position.y, 50 + 20);
  const visible = render(React.createElement(TextBox, { ...props, overflow: 'visible' }, long)).layers[0];
  assert.equal(visible.content.clip, undefined);
  assert.equal(visible.content.layers[0].transform.position.y, 20);
  assert.throws(
    () => render(React.createElement(TextBox, { ...props, overflow: 'error' }, long)),
    /does not fit 100×100 in 2 lines even at minFontSize 20/,
  );
});

test('TextBox with empty text draws nothing', () => {
  install();
  const group = render(React.createElement(TextBox, { width: 100, height: 100, minFontSize: 10, maxFontSize: 20 }, '')).layers[0];
  assert.deepEqual(group.content.layers, []);
});
