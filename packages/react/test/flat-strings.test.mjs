// Every string in a rendered scene is flat. V8's JSON.stringify fast path
// gives up on a whole frame at the first ConsString or SlicedString, which
// made one `FRAME ${n}` text send NEBULA frames 2.8x slower. Run after
// `pnpm run build`.

import assert from 'node:assert/strict';
import v8 from 'node:v8';
import { test } from 'vitest';
import * as React from 'react';

import { Composition, Group, Image, Rect, Text } from '../dist/index.js';
import { mount } from '../dist/render.js';

v8.setFlagsFromString('--allow-natives-syntax');
const sameMap = new Function('a', 'b', 'return %HaveSameMap(a, b)');
const internalized = new Function('value', 'return %IsInternalizedString(value)');
const sequential = [JSON.parse('"a string long enough to be cons"'), JSON.parse('"a string long enough to be cons あ"')];
const flat = (value) => internalized(value) || sequential.some((reference) => sameMap(value, reference));

function nonFlatStrings(value, found = []) {
  if (typeof value === 'string') {
    if (!flat(value)) found.push(value);
  } else if (Array.isArray(value)) {
    for (const item of value) nonFlatStrings(item, found);
  } else if (value && typeof value === 'object') {
    for (const item of Object.values(value)) nonFlatStrings(item, found);
  }
  return found;
}

const h = React.createElement;

test('the check catches a string built by a template literal', () => {
  const frame = String(Math.floor(Math.random() * 1000)).padStart(4, '0');
  assert.deepEqual(nonFlatStrings({ text: `FRAME ${frame} / 600` }).length, 1);
});

test('a frame has no cons or sliced strings in ids, text, colors, or asset paths', () => {
  const nest = (depth, child) => (depth === 0 ? child : h(Group, null, nest(depth - 1, child)));
  const Scene = () => {
    const frame = String(Math.floor(Math.random() * 1000)).padStart(4, '0');
    return h(
      Composition,
      { width: 10, height: 10, fps: 30, durationInFrames: 30 },
      nest(
        6,
        h(
          React.Fragment,
          null,
          h(Text, null, `FRAME ${frame} / 600`),
          h(Rect, { id: `authored-${frame}-rect`, width: 1, height: 1, fill: `rgba(${frame}, 0, 0, 0.5)` }),
          h(Image, { src: `images/frame-${frame}.png`.slice(0, 20) }),
        ),
      ),
    );
  };
  const { scene } = mount(Scene).renderAt({ value: 0, timescale: 30 }, null);
  assert.deepEqual(nonFlatStrings(scene), []);
});
