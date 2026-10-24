// Prop overrides: the editor's drag preview hands the dragged values to the
// runtime for one frame, and every element written at an overridden position
// is created with the dragged value instead of the written one.

import assert from 'node:assert/strict';
import { memo, useMemo } from 'react';
import type { ReactNode } from 'react';
import { test } from 'vitest';

import { Composition, Rect, Static, Text } from '../src/index.js';
import type { Layer, LayerContent, Time } from '../src/index.js';
import type { PropOverride } from '../src/internal.js';
import { jsxDEV } from '../src/jsx-dev-runtime.js';
import type { SourcePosition } from '../src/jsx-dev-runtime.js';
import { mount } from '../src/render.js';
import type { MountedComposition } from '../src/render.js';

const at = (fileName: string, lineNumber: number, columnNumber: number): SourcePosition => ({ fileName, lineNumber, columnNumber });
const resolveFile = (file: string) => `/abs/${file}`;
const FRAME = 30;
const time = (frame: number): Time => ({ value: frame, timescale: FRAME });
const comp = (...children: ReactNode[]) =>
  jsxDEV(Composition, { width: 100, height: 100, fps: FRAME, durationInFrames: 100, children }, undefined, true, at('entry.tsx', 1, 1));
const rect = (id: string, line: number, props: Record<string, unknown> = {}) =>
  jsxDEV(Rect, { id, width: 1, height: 1, ...props }, undefined, false, at('entry.tsx', line, 7));
const override = (line: number, prop: string, value: number, file = 'entry.tsx'): PropOverride =>
  ({ file: resolveFile(file), line, column: 7, prop, value });

function find(layers: Layer[], id: string): Layer | undefined {
  for (const layer of layers) {
    if (layer.id === id) return layer;
    const inner = layer.content.type === 'group' ? find(layer.content.layers, id) : undefined;
    if (inner) return inner;
  }
  return undefined;
}

/** The layers' positions in the frame drawn with `overrides`. */
function positions(composition: MountedComposition, overrides: PropOverride[], ...ids: string[]) {
  const { scene } = composition.renderAt(time(0), null, { propOverrides: { overrides, resolveFile } });
  return ids.map((id) => {
    const layer = find(scene.layers, id);
    assert.ok(layer, `a layer ${id}`);
    return [layer.transform.position.x, layer.transform.position.y];
  });
}

test('an override replaces a written prop on the element written there only', () => {
  const composition = mount(() => comp(rect('a', 3, { x: 10 }), rect('b', 4, { x: 10 })));
  assert.deepEqual(positions(composition, [override(3, 'x', 40)], 'a', 'b'), [[40, 0], [10, 0]]);
});

test('an override adds a prop the element does not write', () => {
  const composition = mount(() => comp(rect('a', 3, { x: 10 })));
  assert.deepEqual(positions(composition, [override(3, 'y', 25)], 'a'), [[10, 25]]);
});

test('an override matches the file as well as the position', () => {
  const composition = mount(() => comp(rect('a', 3, { x: 10 })));
  assert.deepEqual(positions(composition, [override(3, 'x', 40, 'other.tsx')], 'a'), [[10, 0]]);
});

test('an override at a call site reaches every layer reading the parameter', () => {
  // function Label({ x }) { return <><Rect x={x} /><Rect x={x} /></>; }
  function Label({ x }: { x: number }) {
    return [
      jsxDEV(Rect, { id: 'box', x, width: 1, height: 1 }, 'box', false, at('label.tsx', 2, 7)),
      jsxDEV(Rect, { id: 'shadow', x, width: 1, height: 1 }, 'shadow', false, at('label.tsx', 3, 7)),
    ];
  }
  const composition = mount(() => comp(jsxDEV(Label, { x: 10 }, undefined, false, at('entry.tsx', 5, 7))));
  assert.deepEqual(positions(composition, [override(5, 'x', 40)], 'box', 'shadow'), [[40, 0], [40, 0]]);
});

test('an override reaches an element in a memo component that reads no frame', () => {
  const Still = memo(function Still() {
    return rect('a', 3, { x: 10 });
  });
  const composition = mount(() => comp(jsxDEV(Still, {}, undefined, false, at('entry.tsx', 2, 7))));
  assert.deepEqual(positions(composition, [], 'a'), [[10, 0]]);
  assert.deepEqual(positions(composition, [override(3, 'x', 50)], 'a'), [[50, 0]]);
  // Nothing the memo component reads changed since the last frame.
  assert.deepEqual(positions(composition, [override(3, 'x', 60)], 'a'), [[60, 0]]);
  assert.deepEqual(positions(composition, [override(3, 'x', 60)], 'a'), [[60, 0]]);
});

test('a drag frame renders only the components that create an overridden element', () => {
  const renders = { still: 0, other: 0, kept: 0 };
  const Still = memo(function Still() {
    renders.still += 1;
    return rect('a', 3, { x: 10 });
  });
  const Other = memo(function Other() {
    renders.other += 1;
    return rect('b', 4, { x: 10 });
  });
  function Kept() {
    renders.kept += 1;
    return rect('c', 7, { x: 10 });
  }
  const composition = mount(() =>
    comp(
      jsxDEV(Still, {}, undefined, false, at('entry.tsx', 2, 7)),
      jsxDEV(Other, {}, undefined, false, at('entry.tsx', 5, 7)),
      jsxDEV(Static, { children: jsxDEV(Kept, {}, undefined, false, at('entry.tsx', 6, 9)) }, undefined, false, at('entry.tsx', 6, 7)),
    ));
  assert.deepEqual(positions(composition, [override(3, 'x', 50)], 'a', 'b', 'c'), [[50, 0], [10, 0], [10, 0]]);
  const before = { ...renders };
  assert.deepEqual(positions(composition, [override(3, 'x', 60)], 'a', 'b', 'c'), [[60, 0], [10, 0], [10, 0]]);
  assert.deepEqual(renders, { ...before, still: before.still + 1 });
});

test('an override at a new position mid-drag reaches an element in a memo component', () => {
  const Still = memo(function Still() {
    return rect('b', 4, { x: 10 });
  });
  const composition = mount(() => comp(rect('a', 3, { x: 10 }), jsxDEV(Still, {}, undefined, false, at('entry.tsx', 2, 7))));
  assert.deepEqual(positions(composition, [override(3, 'x', 50)], 'a', 'b'), [[50, 0], [10, 0]]);
  assert.deepEqual(positions(composition, [override(3, 'x', 60), override(4, 'y', 5)], 'a', 'b'), [[60, 0], [10, 5]]);
  assert.deepEqual(positions(composition, [override(3, 'x', 70), override(4, 'y', 6)], 'a', 'b'), [[70, 0], [10, 6]]);
});

test('a frame without overrides draws the written values again', () => {
  function Cached() {
    return useMemo(() => rect('a', 3, { x: 10 }), []);
  }
  const composition = mount(() => comp(jsxDEV(Cached, {}, undefined, false, at('entry.tsx', 2, 7)), rect('b', 4, { x: 10 })));
  assert.deepEqual(positions(composition, [override(3, 'x', 50), override(4, 'x', 50)], 'a', 'b'), [[50, 0], [50, 0]]);
  assert.deepEqual(positions(composition, [], 'a', 'b'), [[10, 0], [10, 0]]);
  // `layerSource` evaluates without overrides too.
  assert.deepEqual(positions(composition, [override(3, 'x', 50)], 'a'), [[50, 0]]);
  const report = composition.layerSource(time(0), 'a', resolveFile);
  assert.equal(report?.props?.x, 10);
});

test('an element cached with useMemo at an override position follows only the first drag frame', () => {
  // Known limitation: later frames reuse the cached element, so `jsxDEV`
  // never sees their values. A fix would flip the middle assertions.
  function Cached() {
    return useMemo(() => rect('a', 3, { x: 10 }), []);
  }
  const composition = mount(() => comp(jsxDEV(Cached, {}, undefined, false, at('entry.tsx', 2, 7))));
  assert.deepEqual(positions(composition, [], 'a'), [[10, 0]]);
  assert.deepEqual(positions(composition, [override(3, 'x', 50)], 'a'), [[50, 0]]);
  assert.deepEqual(positions(composition, [override(3, 'x', 60)], 'a'), [[50, 0]]);
  assert.deepEqual(positions(composition, [override(3, 'x', 70)], 'a'), [[50, 0]]);
  assert.deepEqual(positions(composition, [], 'a'), [[10, 0]]);
});

test('a style key override replaces that key only', () => {
  const style = { fontSize: 40, fontWeight: 700 };
  const text = (props: Record<string, unknown>) =>
    jsxDEV(Text, { id: 't', children: 'a', ...props }, undefined, false, at('entry.tsx', 3, 7));
  const textStyle = (composition: MountedComposition) => {
    const { scene } = composition.renderAt(time(0), null, { propOverrides: { overrides: [override(3, 'style.fontSize', 48)], resolveFile } });
    const layer = find(scene.layers, 't');
    assert.equal(layer?.content.type, 'text');
    return (layer.content as Extract<LayerContent, { type: 'text' }>).style;
  };

  const written = textStyle(mount(() => comp(text({ style }))));
  assert.equal(written.fontSize, 48);
  assert.equal(written.fontWeight, 700);
  assert.deepEqual(style, { fontSize: 40, fontWeight: 700 });

  assert.equal(textStyle(mount(() => comp(text({})))).fontSize, 48);
});

test('an override inside <Static> follows the drag and leaves the cache valid', () => {
  // The cached layers are reused for as long as nothing below changes.
  const composition = mount(() => comp(jsxDEV(Static, { children: rect('a', 3, { x: 10 }) }, undefined, false, at('entry.tsx', 2, 7))));
  assert.deepEqual(positions(composition, [], 'a'), [[10, 0]]);
  for (const value of [50, 60, 60]) {
    assert.deepEqual(positions(composition, [override(3, 'x', value)], 'a'), [[value, 0]]);
  }
  assert.deepEqual(positions(composition, [], 'a'), [[10, 0]]);
  assert.deepEqual(positions(composition, [], 'a'), [[10, 0]]);
});
