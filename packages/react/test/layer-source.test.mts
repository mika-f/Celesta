// layerSource: what the editor needs to write a transform back into source.

import assert from 'node:assert/strict';
import { createContext, createElement as h, useContext } from 'react';
import type { ReactElement, ReactNode } from 'react';
import { test } from 'vitest';

import { Composition, FreezeFrame, Group, Loop, MotionBlur, Rect, Scenes, Sequence, Series, defineScenes } from '../src/index.js';
import type { Layer, Time } from '../src/index.js';
import { jsxDEV } from '../src/jsx-dev-runtime.js';
import type { SourcePosition } from '../src/jsx-dev-runtime.js';
import { mount } from '../src/render.js';
import type { MountedComposition } from '../src/render.js';

import { childrenOf } from './helpers.mjs';

const at = (fileName: string, lineNumber: number, columnNumber: number): SourcePosition => ({ fileName, lineNumber, columnNumber });
const resolve = (file: string) => `/abs/${file}`;
const FRAME = 30;
const time = (frame: number): Time => ({ value: frame, timescale: FRAME });
const comp = (...children: ReactNode[]) =>
  jsxDEV(Composition, { width: 100, height: 100, fps: FRAME, durationInFrames: 100, children }, undefined, true, at('entry.tsx', 1, 1));
const layerIds = (layers: Layer[]): string[] =>
  layers.flatMap((layer) => [layer.id, ...(layer.content.type === 'group' ? layerIds(layer.content.layers) : [])]);

/** The report for the layer at `index` of the frame's layers, depth first; the last (innermost) by default. */
function reportFor(composition: MountedComposition, frame: number, index = -1) {
  const { scene } = composition.renderAt(time(frame), null);
  const id = layerIds(scene.layers).at(index);
  assert.ok(id, `a layer at ${index}`);
  const report = composition.layerSource(time(frame), id, resolve);
  assert.ok(report, `a report for ${id}`);
  return report;
}

test('localFrame is the frame where the element was written, not inside the Sequence', () => {
  // The entry wrote the Rect, so an `interpolate(frame, ...)` there reads the
  // entry's frame.
  const composition = mount(() =>
    comp(jsxDEV(Sequence, { from: 20, children: jsxDEV(Rect, { width: 1, height: 1 }, undefined, false, at('entry.tsx', 5, 7)) }, undefined, false, at('entry.tsx', 4, 5))),
  );
  const report = reportFor(composition, 30);
  assert.equal(report.chain[0].name, 'Rect');
  assert.equal(report.localFrame, 30);
});

// `frame` in `interpolate(frame, ...)` is the value `useCurrentFrame()` had in
// the component that wrote the JSX, which may sit above a `<Sequence>`.
const rectAt = (line: number, props = {}) => jsxDEV(Rect, { width: 1, height: 1, ...props }, undefined, false, at('scene.tsx', line, 7));
const sequence = (from: number, children: ReactNode, line = 4) => jsxDEV(Sequence, { from, children }, undefined, false, at('scene.tsx', line, 5));

test('localFrame is the frame of the component that wrote the element, above a Sequence', () => {
  // const frame = useCurrentFrame(); <Sequence from={30}><Rect x={interpolate(frame, ...)} /></Sequence>
  function Scene() {
    return sequence(30, rectAt(5, { x: 100 }));
  }
  const composition = mount(() => comp(jsxDEV(Scene, {}, undefined, false, at('entry.tsx', 3, 5))));
  const report = reportFor(composition, 60);
  assert.equal(report.localFrame, 60);
  assert.deepEqual(report.candidateFrames, [30, 60]);
});

test('localFrame follows a Sequence above the writing component', () => {
  function Scene() {
    return rectAt(5);
  }
  const composition = mount(() => comp(sequence(20, jsxDEV(Scene, {}, undefined, false, at('entry.tsx', 3, 5)))));
  assert.equal(reportFor(composition, 60).localFrame, 40);
});

test('a component passing children through does not own them', () => {
  function Later({ children }: { children: ReactNode }) {
    return sequence(30, children, 9);
  }
  function Scene() {
    return jsxDEV(Later, { children: rectAt(5) }, undefined, false, at('scene.tsx', 4, 5));
  }
  const composition = mount(() => comp(jsxDEV(Scene, {}, undefined, false, at('entry.tsx', 3, 5))));
  const report = reportFor(composition, 60);
  assert.equal(report.localFrame, 60);
  assert.deepEqual(report.candidateFrames, [30, 60]);
});

test('a MotionBlur moment function is written by the component around it', () => {
  function Scene() {
    return sequence(10, jsxDEV(MotionBlur, { samples: 1, shutterAngle: 0, children: () => rectAt(5) }, undefined, false, at('scene.tsx', 6, 5)));
  }
  const composition = mount(() => comp(jsxDEV(Scene, {}, undefined, false, at('entry.tsx', 3, 5))));
  assert.equal(reportFor(composition, 60).localFrame, 60);
});

// `frame` may be read by any component above the element: the candidates are
// the frames handed down by every frame provider on the way to the root.

test('candidateFrames lists each frame above the element once, nearest first', () => {
  const composition = mount(() =>
    comp(sequence(10, sequence(0, sequence(20, rectAt(5), 6), 5), 4)),
  );
  // 60 at the root, 50 under `from={10}` (and `from={0}` inside it), 30 under `from={20}`.
  assert.deepEqual(reportFor(composition, 60).candidateFrames, [30, 50, 60]);
});

test('candidateFrames covers a render function nested in an object prop', () => {
  function Later({ config }: { config: { render: () => ReactElement } }) {
    return config.render();
  }
  function Scene() {
    return sequence(30, jsxDEV(Later, { config: { render: () => rectAt(5) } }, undefined, false, at('scene.tsx', 3, 5)));
  }
  const composition = mount(() => comp(jsxDEV(Scene, {}, undefined, false, at('entry.tsx', 3, 5))));
  assert.deepEqual(reportFor(composition, 60).candidateFrames, [30, 60]);
});

test('candidateFrames covers a render function passed through context', () => {
  const Render = createContext<(() => ReactElement) | null>(null);
  function Consumer() {
    return useContext(Render)!();
  }
  function Scene() {
    return h(Render.Provider, { value: () => rectAt(5) }, sequence(30, jsxDEV(Consumer, {}, undefined, false, at('scene.tsx', 3, 5))));
  }
  const composition = mount(() => comp(jsxDEV(Scene, {}, undefined, false, at('entry.tsx', 3, 5))));
  assert.deepEqual(reportFor(composition, 60).candidateFrames, [30, 60]);
});

test('candidateFrames covers a render prop called below a frame change', () => {
  function Later({ render }: { render: () => ReactElement }) {
    return sequence(30, render(), 9);
  }
  function Scene() {
    return sequence(10, jsxDEV(Later, { render: () => rectAt(5) }, undefined, false, at('scene.tsx', 4, 5)));
  }
  const composition = mount(() => comp(jsxDEV(Scene, {}, undefined, false, at('entry.tsx', 3, 5))));
  const report = reportFor(composition, 60);
  assert.deepEqual(report.candidateFrames, [20, 50, 60]);
  assert.equal('frameUncertain' in report, false);
});

test('the report reads the frame just rendered when the element props did not change', () => {
  // The Rect's props are the same every frame, so its host node keeps the
  // fiber of an earlier render; its parents still hold that render's frames.
  function Scene() {
    return sequence(30, rectAt(5, { x: 100 }));
  }
  for (const renders of [1, 2, 3]) {
    const composition = mount(() => comp(jsxDEV(Scene, {}, undefined, false, at('entry.tsx', 3, 5))));
    let id: string | undefined;
    for (let i = 0; i < renders; i++) id = layerIds(composition.renderAt(time(30), null).scene.layers).at(-1);
    assert.ok(id);
    const report = composition.layerSource(time(60), id, resolve);
    assert.ok(report);
    assert.deepEqual(report.candidateFrames, [30, 60], `after ${renders} renders`);
    assert.equal(report.localFrame, 60, `after ${renders} renders`);
  }
});

test('props are the transform props the primitive received', () => {
  const composition = mount(() =>
    comp(jsxDEV(Rect, { x: 12, rotation: 5, width: 1, height: 1, fill: '#ffffff' }, undefined, false, at('entry.tsx', 5, 7))),
  );
  assert.deepEqual(reportFor(composition, 0).props, { x: 12, rotation: 5 });
});

test('renderCount counts the layers one element produced', () => {
  function Many() {
    return jsxDEV('group', {
      children: [0, 1, 2].map((i) => jsxDEV(Rect, { x: i, width: 1, height: 1 }, i, false, at('entry.tsx', 8, 9))),
    }, undefined, true, at('entry.tsx', 7, 3));
  }
  const composition = mount(() => comp(h(Many)));
  const { scene } = composition.renderAt(time(0), null);
  const inner = childrenOf(scene.layers[0])[0].id;
  const report = composition.layerSource(time(0), inner, resolve);
  assert.ok(report);
  assert.equal(report.chain[0].name, 'Rect');
  assert.equal(report.renderCount, 3);
});

test('a Group entry carries its layerId', () => {
  const composition = mount(() =>
    comp(
      jsxDEV(Group, { id: 'outer', x: 3, children: jsxDEV(Rect, { width: 1, height: 1 }, undefined, false, at('entry.tsx', 6, 7)) }, undefined, true, at('entry.tsx', 5, 5)),
    ),
  );
  const { scene } = composition.renderAt(time(0), null);
  const inner = childrenOf(scene.layers[0])[0].id;
  const report = composition.layerSource(time(0), inner, resolve);
  assert.ok(report);
  const group = report.chain.find((entry) => entry.name === 'Group');
  assert.ok(group);
  assert.equal(group.kind, 'primitive');
  assert.equal(group.layerId, scene.layers[0].id);
  assert.equal(report.chain[0].layerId, inner);
});

test('timing wrappers written in the source are wrapper entries', () => {
  // <Sequence><Loop><FreezeFrame><Rect /></FreezeFrame></Loop></Sequence> inside <Scene>
  function Scene() {
    return sequence(
      10,
      jsxDEV(Loop, {
        durationInFrames: 20,
        children: jsxDEV(FreezeFrame, { frame: 0, children: rectAt(7) }, undefined, false, at('scene.tsx', 6, 9)),
      }, undefined, false, at('scene.tsx', 5, 7)),
    );
  }
  const composition = mount(() => comp(jsxDEV(Scene, {}, undefined, false, at('entry.tsx', 3, 5))));
  const { chain } = reportFor(composition, 30);
  assert.deepEqual(
    chain.map(({ name, kind }) => [name, kind]),
    [
      ['Rect', 'primitive'],
      ['FreezeFrame', 'wrapper'],
      ['Loop', 'wrapper'],
      ['Sequence', 'wrapper'],
      ['Scene', 'component'],
      ['Composition', 'wrapper'],
    ],
  );
});

test('a user component named Sequence is a component entry', () => {
  function Sequence() {
    return rectAt(5);
  }
  const composition = mount(() => comp(jsxDEV(Sequence, {}, undefined, false, at('entry.tsx', 3, 5))));
  const entry = reportFor(composition, 0).chain[1];
  assert.equal(entry.name, 'Sequence');
  assert.equal(entry.kind, 'component');
});

// `windows`: the timing wrappers around the element, in composition frames.

const pos = (line: number, column: number) => ({ file: '/abs/scene.tsx', line, column });

test('windows lists nested Sequences in composition frames, nearest first', () => {
  const composition = mount(() =>
    comp(
      jsxDEV(Sequence, {
        from: 10,
        durationInFrames: 50,
        children: jsxDEV(Sequence, { from: 5, durationInFrames: 20, children: rectAt(6) }, undefined, false, at('scene.tsx', 5, 7)),
      }, undefined, false, at('scene.tsx', 4, 5)),
    ),
  );
  const report = reportFor(composition, 20);
  assert.deepEqual(report.windows, [
    { kind: 'Sequence', from: 15, to: 35, source: pos(5, 7) },
    { kind: 'Sequence', from: 10, to: 60, source: pos(4, 5) },
  ]);
  assert.deepEqual(report.span, { from: 15, to: 35 });
});

test('a Series item window points at the Series.Sequence', () => {
  const composition = mount(() =>
    comp(
      jsxDEV(Series, {
        children: [
          jsxDEV(Series.Sequence, { durationInFrames: 30 }, undefined, false, at('scene.tsx', 5, 7)),
          jsxDEV(Series.Sequence, { durationInFrames: 40, children: rectAt(7) }, undefined, false, at('scene.tsx', 6, 7)),
        ],
      }, undefined, true, at('scene.tsx', 4, 5)),
    ),
  );
  const report = reportFor(composition, 40);
  assert.deepEqual(report.windows?.[0], { kind: 'Sequence', from: 30, to: 70, source: pos(6, 7) });
});

test('a Scenes scene window has no source', () => {
  const scenes = defineScenes([
    { id: 'a', durationInFrames: 30, Scene: () => rectAt(5) },
    { id: 'b', durationInFrames: 40, Scene: () => rectAt(9) },
  ]);
  const composition = mount(() => comp(jsxDEV(Scenes, { scenes }, undefined, false, at('scene.tsx', 12, 5))));
  const report = reportFor(composition, 35);
  assert.equal(report.chain[0].line, 9);
  const nearest = report.windows?.[0];
  assert.deepEqual(nearest, { kind: 'Sequence', from: 30, to: 70 });
  assert.equal('source' in nearest!, false);
});

test('a Loop reports its outer window, remapped, and windows inside it inherit it', () => {
  const composition = mount(() =>
    comp(
      jsxDEV(Loop, {
        from: 10,
        durationInFrames: 20,
        times: 3,
        children: jsxDEV(Sequence, { from: 5, children: rectAt(6) }, undefined, false, at('scene.tsx', 5, 7)),
      }, undefined, false, at('scene.tsx', 4, 5)),
    ),
  );
  const report = reportFor(composition, 37);
  assert.deepEqual(report.windows, [
    { kind: 'Sequence', from: 10, to: 70, remapped: true, source: pos(5, 7) },
    { kind: 'Loop', from: 10, to: 70, remapped: true, source: pos(4, 5) },
  ]);
  assert.deepEqual(report.span, { from: 10, to: 70 });
});

test('a FreezeFrame window and windows inside it take the enclosing window', () => {
  const composition = mount(() =>
    comp(
      jsxDEV(Sequence, {
        from: 10,
        durationInFrames: 50,
        children: jsxDEV(FreezeFrame, {
          frame: 80,
          children: jsxDEV(Sequence, { from: 70, children: rectAt(7) }, undefined, false, at('scene.tsx', 6, 9)),
        }, undefined, false, at('scene.tsx', 5, 7)),
      }, undefined, false, at('scene.tsx', 4, 5)),
    ),
  );
  const report = reportFor(composition, 20);
  assert.deepEqual(report.windows, [
    { kind: 'Sequence', from: 10, to: 60, remapped: true, source: pos(6, 9) },
    { kind: 'FreezeFrame', from: 10, to: 60, remapped: true, source: pos(5, 7) },
    { kind: 'Sequence', from: 10, to: 60, source: pos(4, 5) },
  ]);
  assert.deepEqual(report.span, { from: 10, to: 60 });
});

test('a Sequence inside a MotionBlur is placed at its own from, not at the shifted moment', () => {
  const composition = mount(() =>
    comp(
      jsxDEV(MotionBlur, {
        samples: 4,
        children: jsxDEV(Sequence, { from: 10, durationInFrames: 50, children: rectAt(6) }, undefined, false, at('scene.tsx', 5, 7)),
      }, undefined, false, at('scene.tsx', 4, 5)),
    ),
  );
  const { scene } = composition.renderAt(time(30), null);
  const rects = layerIds(scene.layers).filter((id) => composition.layerSource(time(30), id, resolve)?.chain[0]?.name === 'Rect');
  assert.ok(rects.length > 1, 'one Rect layer per sample');
  for (const id of rects) {
    const report = composition.layerSource(time(30), id, resolve);
    assert.deepEqual(report?.windows, [{ kind: 'Sequence', from: 10, to: 60, source: pos(5, 7) }], id);
    assert.deepEqual(report?.span, { from: 10, to: 60 }, id);
  }
});

test('a fractional Sequence from is kept', () => {
  const composition = mount(() => comp(jsxDEV(Sequence, { from: 10.5, durationInFrames: 20, children: rectAt(5) }, undefined, false, at('scene.tsx', 4, 5))));
  const report = reportFor(composition, 20);
  assert.deepEqual(report.windows, [{ kind: 'Sequence', from: 10.5, to: 30.5, source: pos(4, 5) }]);
  assert.deepEqual(report.span, { from: 10.5, to: 30.5 });
});

test('an element without wrappers spans the composition', () => {
  const composition = mount(() => comp(rectAt(5)));
  const report = reportFor(composition, 0);
  assert.deepEqual(report.windows, []);
  assert.deepEqual(report.span, { from: 0, to: 100 });
});

test('a missing layer reports null', () => {
  const composition = mount(() => comp());
  assert.equal(composition.layerSource(time(0), 'nope', resolve), null);
});

test('an empty chain reports only the chain', () => {
  const composition = mount(() =>
    h(Composition, { width: 100, height: 100, fps: FRAME, durationInFrames: 100 }, h(Rect, { width: 1, height: 1 })),
  );
  assert.deepEqual(reportFor(composition, 0), { chain: [] });
});
