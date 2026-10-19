import assert from 'node:assert/strict';
import { beforeEach, test } from 'vitest';
import * as React from 'react';

import { Composition, Rect, Span, Theme } from '@celesta/react';
import { mount, setTextMeasurer } from '@celesta/react/internal';
import { Badge } from '../dist/index.js';

const h = React.createElement;

// Each character is `fontSize` wide, or half that at weight 300, so a font
// change shows in the width. With `maxWidth`, characters fill lines of that
// width; an empty text has no lines.
function measure({ text, style, maxWidth }) {
  const size = style.fontSize ?? 10;
  const advance = style.fontWeight === 300 ? size / 2 : size;
  const count = Array.from(text).length;
  const perLine = maxWidth === undefined ? Infinity : Math.max(1, Math.floor(maxWidth / advance));
  const lines = Math.ceil(count / Math.min(perLine, Math.max(count, 1)));
  const lineHeight = size * 1.5;
  return {
    width: Math.min(count, perLine) * advance,
    height: lines * lineHeight,
    ascent: size * 1.1,
    descent: size * 0.4,
    lineHeight,
    lines,
    glyphs: [],
  };
}

let requests;
beforeEach(() => {
  requests = [];
  const sync = (request) => {
    requests.push(request);
    return measure(request);
  };
  setTextMeasurer(async (request) => sync(request), sync);
});

function render(element) {
  const Root = () => h(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 1 }, element);
  return mount(Root).renderAt({ value: 0, timescale: 30 }, null).scene.layers;
}

// The badge's outer group, its inner group, and the inner group's layers.
function parts(layers) {
  const [outer] = layers;
  const [inner] = outer.content.layers;
  const [box, ...rest] = inner.content.layers;
  return { outer, inner, box, text: rest.at(-1), leading: rest.length > 1 ? rest[0] : undefined };
}

test('the background is the measured text plus the padding', () => {
  const { box, text } = parts(render(h(Badge, { padding: { horizontal: 20, vertical: 10 }, fontSize: 20 }, '第1章')));
  assert.deepEqual([box.content.width, box.content.height], [3 * 20 + 40, 30 + 20]);
  assert.deepEqual([text.transform.position.x, text.transform.position.y], [20, 10 + 22]);
  assert.equal(text.content.text, '第1章');
});

test('padding takes one number, axes, or sides', () => {
  const size = (padding) => {
    const { box } = parts(render(h(Badge, { padding, fontSize: 10 }, 'ab')));
    return [box.content.width, box.content.height];
  };
  assert.deepEqual(size(5), [30, 25]);
  assert.deepEqual(size({ horizontal: 4 }), [28, 15]);
  assert.deepEqual(size({ horizontal: 4, vertical: 2, left: 0, bottom: 6 }), [24, 23]);
});

test('the text style, font changes included, decides the width', () => {
  const width = (props) => parts(render(h(Badge, { padding: 0, fontSize: 20, ...props }, 'abcd'))).box.content.width;
  assert.equal(width({}), 80);
  assert.equal(width({ fontWeight: 300 }), 40);
  assert.equal(width({ fontWeight: 300, style: { fontWeight: 700, fontSize: 10 } }), 40);
  assert.equal(width({ fontSize: 30 }), 120);
});

test('spans are measured and drawn with the rest of the text', () => {
  const { box, text } = parts(render(h(Badge, { padding: 0, fontSize: 10 }, 'LIVE ', h(Span, { fontWeight: 300 }, '生放送'))));
  assert.equal(text.content.text, 'LIVE 生放送');
  const request = requests.find((r) => r.text === 'LIVE 生放送');
  assert.deepEqual(request.style.fontRuns, [{ start: 5, end: 8, fontWeight: 300 }]);
  assert.equal(box.content.width, 80);
});

test('fill and stroke take <Theme> color names, and pill rounds the ends', () => {
  const { box, text } = parts(render(h(Theme, { colors: { navy: '#204060', paper: '#f4f1ea' } },
    h(Badge, { fill: 'navy', stroke: 'paper', strokeWidth: 2, cornerRadius: 'pill', color: 'paper', padding: 10, fontSize: 20 }, 'Hi'))));
  assert.equal(box.content.fill.color, '#204060');
  assert.equal(box.content.stroke.paint.color, '#f4f1ea');
  assert.equal(box.content.cornerRadius, 25);
  assert.equal(text.content.style.fill.color, '#f4f1ea');
});

test('an empty text keeps the height of one line and drops the gap after the leading', () => {
  const dot = h(Rect, { y: -4, width: 8, height: 8, fill: '#ff0000' });
  const full = parts(render(h(Badge, { padding: 10, fontSize: 20, leading: dot, leadingWidth: 8, gap: 6 }, 'ab')));
  assert.equal(full.box.content.width, 10 + 8 + 6 + 40 + 10);
  assert.deepEqual([full.leading.transform.position.x, full.leading.transform.position.y], [10, 10 + 15]);
  assert.equal(full.text.transform.position.x, 24);

  const empty = parts(render(h(Badge, { padding: 10, fontSize: 20, leading: dot, leadingWidth: 8, gap: 6 }, '')));
  assert.deepEqual([empty.box.content.width, empty.box.content.height], [28, 50]);
  const none = parts(render(h(Badge, { padding: 10, fontSize: 20 })));
  assert.deepEqual([none.box.content.width, none.box.content.height], [20, 50]);
});

test('long text wraps inside maxWidth and the badge grows taller', () => {
  const short = parts(render(h(Badge, { padding: 10, fontSize: 10, maxWidth: 100 }, 'abc')));
  assert.equal(short.box.content.width, 50);
  assert.equal(short.text.content.maxWidth, undefined);

  // 80 px of room fits 8 characters a line: 3 lines, hugging the widest.
  const long = parts(render(h(Badge, { padding: 10, fontSize: 10, maxWidth: 100 }, 'abcdefghijklmnopqrst')));
  assert.deepEqual([long.box.content.width, long.box.content.height], [100, 3 * 15 + 20]);
  assert.equal(long.text.content.maxWidth, 80);

  const centered = parts(render(h(Badge, { padding: 10, fontSize: 10, maxWidth: 105, style: { align: 'center' } }, 'abcdefghijklmnopqrst')));
  assert.equal(centered.box.content.width, 105);
});

test('anchor and transforms place the background and the text together', () => {
  const { outer, inner, box } = parts(render(h(Badge, {
    x: 500, y: 300, anchorX: 0.5, anchorY: 1, rotation: 30, scale: 2, opacity: 0.5, padding: 10, fontSize: 20,
  }, 'abcd')));
  assert.deepEqual([outer.transform.position.x, outer.transform.position.y], [500, 300]);
  assert.equal(outer.transform.rotation, 30);
  assert.deepEqual(outer.transform.scale, { x: 2, y: 2 });
  assert.equal(outer.opacity, 0.5);
  assert.deepEqual([inner.transform.position.x, inner.transform.position.y], [-50, -50]);
  assert.deepEqual([box.content.width, box.content.height], [100, 50]);
});
