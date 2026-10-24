// collectOccurrences: where each element renders over the timeline.

import assert from 'node:assert/strict';
import { createElement as h } from 'react';
import type { ReactNode } from 'react';
import { test } from 'vitest';

import { Composition, MotionBlur, Rect, Scenes, Sequence, Trail, defineScenes, useCurrentFrame } from '../src/index.js';
import type { Layer } from '../src/index.js';
import { jsxDEV } from '../src/jsx-dev-runtime.js';
import type { SourcePosition } from '../src/jsx-dev-runtime.js';
import { mount } from '../src/render.js';
import type { MountedComposition } from '../src/render.js';

const at = (fileName: string, lineNumber: number, columnNumber: number): SourcePosition => ({ fileName, lineNumber, columnNumber });
const resolve = (file: string) => `/abs/${file}`;
const pos = (file: string, line: number, column: number) => ({ file: `/abs/${file}`, line, column });
const comp = (...children: ReactNode[]) =>
  jsxDEV(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 100, children }, undefined, true, at('entry.tsx', 1, 1));
const rectAt = (line: number, props = {}) => jsxDEV(Rect, { width: 1, height: 1, ...props }, undefined, false, at('scene.tsx', line, 7));

const layerIds = (layers: Layer[]): string[] =>
  layers.flatMap((layer) => [layer.id, ...(layer.content.type === 'group' ? layerIds(layer.content.layers) : [])]);

/** The id of the innermost layer drawn at `frame`, depth first. */
function innermostLayerId(composition: MountedComposition, frame: number): string {
  const id = layerIds(composition.renderAt({ value: frame, timescale: 30 }, null).scene.layers).at(-1);
  assert.ok(id, `a layer at frame ${frame}`);
  return id;
}

function Card() {
  return rectAt(3);
}

test('one component used in two scenes is one element with two instances', () => {
  const card = (line: number) => jsxDEV(Card, {}, undefined, false, at('entry.tsx', line, 9));
  const composition = mount(() =>
    comp(
      jsxDEV(Sequence, { from: 0, durationInFrames: 30, children: card(3) }, undefined, false, at('entry.tsx', 2, 5)),
      jsxDEV(Sequence, { from: 30, durationInFrames: 30, children: card(5) }, undefined, false, at('entry.tsx', 4, 5)),
    ),
  );
  const { elements } = composition.collectOccurrences(0, 100, resolve);
  assert.equal(elements.length, 1);
  assert.deepEqual(elements[0].element, pos('scene.tsx', 3, 7));
  assert.deepEqual(elements[0].instances, [
    { chain: [pos('scene.tsx', 3, 7), pos('entry.tsx', 3, 9)], ranges: [[0, 30]], maxPerFrame: 1 },
    { chain: [pos('scene.tsx', 3, 7), pos('entry.tsx', 5, 9)], ranges: [[30, 60]], maxPerFrame: 1 },
  ]);
});

test('one Scene component in two defineScenes entries is one instance with ranges in both', () => {
  const Scene = () => rectAt(5);
  const scenes = defineScenes([
    { id: 'a', durationInFrames: 30, Scene },
    { id: 'b', durationInFrames: 30, Scene: () => rectAt(9) },
    { id: 'c', durationInFrames: 30, Scene },
  ]);
  const composition = mount(() => comp(jsxDEV(Scenes, { scenes }, undefined, false, at('entry.tsx', 2, 5))));
  const { elements } = composition.collectOccurrences(0, 100, resolve);
  const shared = elements.find(({ element }) => element.line === 5);
  assert.ok(shared, JSON.stringify(elements));
  assert.equal(shared.instances.length, 1);
  // `<Scenes>` is a wrapper, as the layer report says, so its position is
  // not part of the instance key.
  assert.deepEqual(shared.instances[0].chain, [pos('scene.tsx', 5, 7)]);
  assert.deepEqual(shared.instances[0].ranges, [[0, 30], [60, 90]]);
  const report = composition.layerSource({ value: 0, timescale: 30 }, innermostLayerId(composition, 0), resolve);
  assert.deepEqual(
    report?.chain.filter(({ kind }) => kind !== 'wrapper').map(({ file, line, column }) => ({ file, line, column })),
    shared.instances[0].chain,
  );
});

test('.map() copies in one frame raise maxPerFrame', () => {
  function Row() {
    return [1, 2, 3].map((i) => jsxDEV(Rect, { width: 1, height: 1, x: i }, i, false, at('scene.tsx', 4, 9)));
  }
  const composition = mount(() => comp(jsxDEV(Row, {}, undefined, false, at('entry.tsx', 2, 5))));
  const { elements } = composition.collectOccurrences(0, 10, resolve);
  assert.equal(elements.length, 1);
  assert.equal(elements[0].instances.length, 1);
  assert.equal(elements[0].instances[0].maxPerFrame, 3);
  assert.deepEqual(elements[0].instances[0].ranges, [[0, 10]]);
});

test('MotionBlur samples and Trail copies are one place', () => {
  // Each moment is a group layer of its own, whose chain is the component
  // around the blur; the Rect inside has one id in every moment.
  function Blurred() {
    return jsxDEV(MotionBlur, { samples: 4, children: rectAt(4) }, undefined, false, at('scene.tsx', 3, 5));
  }
  function Trailed() {
    return jsxDEV(Trail, { copies: 3, children: rectAt(4) }, undefined, false, at('scene.tsx', 3, 5));
  }
  for (const Scene of [Blurred, Trailed]) {
    const composition = mount(() => comp(jsxDEV(Scene, {}, undefined, false, at('entry.tsx', 2, 5))));
    const { elements } = composition.collectOccurrences(5, 10, resolve);
    assert.deepEqual(
      elements.map(({ element, instances }) => [element, instances.map(({ maxPerFrame, ranges }) => [maxPerFrame, ranges])]),
      [
        // The blur's group and its moments' groups.
        [pos('entry.tsx', 2, 5), [[1, [[5, 15]]]]],
        [pos('scene.tsx', 4, 7), [[1, [[5, 15]]]]],
      ],
      Scene.name,
    );
  }
});

test('.map() copies inside a MotionBlur still count once each', () => {
  function Row() {
    return [1, 2, 3].map((i) => jsxDEV(Rect, { width: 1, height: 1, x: i }, i, false, at('scene.tsx', 4, 9)));
  }
  const composition = mount(() =>
    comp(
      jsxDEV(MotionBlur, { samples: 4, children: jsxDEV(Row, {}, undefined, false, at('entry.tsx', 3, 7)) }, undefined, false, at('entry.tsx', 2, 5)),
    ),
  );
  const { elements } = composition.collectOccurrences(0, 10, resolve);
  assert.equal(elements[0].instances[0].maxPerFrame, 3);
});

function Early() {
  const frame = useCurrentFrame();
  return frame < 40 && rectAt(6);
}

test('a conditionally rendered element stops where the condition does', () => {
  const composition = mount(() => comp(jsxDEV(Early, {}, undefined, false, at('entry.tsx', 2, 5))));
  const { elements } = composition.collectOccurrences(0, 100, resolve);
  assert.equal(elements.length, 1);
  assert.deepEqual(elements[0].instances[0].ranges, [[0, 40]]);
});

test('a chunk reports only its own frames', () => {
  const composition = mount(() => comp(jsxDEV(Early, {}, undefined, false, at('entry.tsx', 2, 5))));
  assert.deepEqual(composition.collectOccurrences(20, 10, resolve).elements[0].instances[0].ranges, [[20, 30]]);
  // Clipped to the composition, too.
  const late = mount(() => comp(rectAt(2)));
  assert.deepEqual(late.collectOccurrences(90, 50, resolve).elements[0].instances[0].ranges, [[90, 100]]);
});

test('layers without a source position are skipped', () => {
  const composition = mount(() => comp(h(Rect, { width: 1, height: 1 })));
  assert.deepEqual(composition.collectOccurrences(0, 10, resolve).elements, []);
});
