import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';

import {
  Audio,
  Composition,
  Text,
  TransitionSeries,
  computeTransitionSeries,
  useCurrentFrame,
  useTransitionSeriesScene,
  useTransitionVolume,
  useVideoConfig,
} from '../dist/index.js';
import { mount } from '../dist/render.js';

const h = React.createElement;
const FPS = 10;
const at = (composition, frame) => composition.renderAt({ value: frame, timescale: FPS }, null);

// Each scene renders as <Sequence> > transition <Group> > content.
const scenes = (layers) => layers.map((sequence) => {
  const group = sequence.content.layers[0];
  return { group, text: group.content.layers[0].content.text };
});

function Scene({ name }) {
  const frame = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  assert.ok(frame >= 0 && frame < durationInFrames, `${name} rendered at local frame ${frame}`);
  return h(Text, null, `${name}:${frame}`);
}

function series(transition, total) {
  return mount(() => h(Composition, { width: 100, height: 50, fps: FPS, durationInFrames: total },
    h(TransitionSeries, null,
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Scene, { name: 'a' })),
      transition && h(TransitionSeries.Transition, transition),
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Scene, { name: 'b' })),
    ),
  ));
}

test('computeTransitionSeries overlaps scenes by each transition and shortens the total', () => {
  assert.deepEqual(
    computeTransitionSeries([
      { durationInFrames: 30 },
      { type: 'crossfade', durationInFrames: 10 },
      { durationInFrames: 40 },
      { durationInFrames: 20 },
      { type: 'cut' },
      { durationInFrames: 15 },
      { type: 'wipe', durationInFrames: 15 },
      { durationInFrames: 25 },
    ]),
    {
      sequences: [
        { from: 0, durationInFrames: 30 },
        { from: 20, durationInFrames: 40 },
        { from: 60, durationInFrames: 20 },
        { from: 80, durationInFrames: 15 },
        { from: 80, durationInFrames: 25 },
      ],
      transitions: [
        { from: 20, durationInFrames: 10 },
        { from: 80, durationInFrames: 0 },
        { from: 80, durationInFrames: 15 },
      ],
      durationInFrames: 105,
    },
  );
  // A scene may spend all of its frames in transitions, without three scenes meeting.
  assert.equal(
    computeTransitionSeries([
      { durationInFrames: 10 },
      { type: 'slide', durationInFrames: 4 },
      { durationInFrames: 10 },
      { type: 'slide', durationInFrames: 6 },
      { durationInFrames: 10 },
    ]).durationInFrames,
    20,
  );
});

test('computeTransitionSeries rejects layouts that would leave a gap or show three scenes', () => {
  const cases = [
    [[], /at least one scene/],
    [[{ type: 'crossfade', durationInFrames: 5 }, { durationInFrames: 10 }], /before any scene/],
    [[{ durationInFrames: 10 }, { type: 'crossfade', durationInFrames: 5 }], /cannot end with a transition/],
    [[{ durationInFrames: 10 }, { type: 'cut' }, { type: 'cut' }, { durationInFrames: 10 }], /follows another transition/],
    [[{ durationInFrames: 10 }, { type: 'crossfade', durationInFrames: 11 }, { durationInFrames: 20 }], /scene 0 lasts 10 frames/],
    [[{ durationInFrames: 20 }, { type: 'crossfade', durationInFrames: 11 }, { durationInFrames: 10 }], /scene 1 lasts 10 frames, shorter than its transition in \(11\)/],
    [[
      { durationInFrames: 20 }, { type: 'wipe', durationInFrames: 6 }, { durationInFrames: 10 },
      { type: 'wipe', durationInFrames: 5 }, { durationInFrames: 20 },
    ], /scene 1 lasts 10 frames, too short for its transitions in \(6\) and out \(5\)/],
    [[{ durationInFrames: 10 }, { type: 'dissolve', durationInFrames: 5 }, { durationInFrames: 10 }], /unknown type "dissolve"/],
    [[{ durationInFrames: 10 }, { type: 'cut', durationInFrames: 3 }, { durationInFrames: 10 }], /cut, which takes no frames/],
    [[{ durationInFrames: 10 }, { type: 'slide' }, { durationInFrames: 10 }], /item 1 \(slide\) requires a positive integer/],
    [[{ durationInFrames: 10 }, { type: 'slide', durationInFrames: 2.5 }, { durationInFrames: 10 }], /positive integer/],
    [[{ durationInFrames: 0 }], /item 0 requires a positive integer durationInFrames/],
  ];
  for (const [items, message] of cases) {
    assert.throws(() => computeTransitionSeries(items), message);
  }
});

test('a crossfade shows both scenes only while they overlap, the next one above', () => {
  const composition = series({ type: 'crossfade', durationInFrames: 4 }, 16);
  for (let frame = 0; frame < 16; frame += 1) {
    const shown = scenes(at(composition, frame).scene.layers);
    if (frame < 6) {
      assert.deepEqual(shown.map((s) => s.text), [`a:${frame}`]);
      assert.equal(shown[0].group.opacity, 1);
    } else if (frame < 10) {
      // b is drawn over a, which stays opaque, so the mix never dims.
      assert.deepEqual(shown.map((s) => s.text), [`a:${frame}`, `b:${frame - 6}`]);
      assert.equal(shown[0].group.opacity, 1);
      assert.equal(shown[1].group.opacity, (frame - 6 + 1) / 5);
    } else {
      assert.deepEqual(shown.map((s) => s.text), [`b:${frame - 6}`]);
      assert.equal(shown[0].group.opacity, 1);
    }
  }
});

test('without a transition, or with a cut, scenes meet on one frame with no gap', () => {
  for (const transition of [null, { type: 'cut' }]) {
    const composition = series(transition, 20);
    assert.deepEqual(scenes(at(composition, 9).scene.layers).map((s) => s.text), ['a:9']);
    assert.deepEqual(scenes(at(composition, 10).scene.layers).map((s) => s.text), ['b:0']);
  }
});

test('a slide pushes the previous scene out by the frame size', () => {
  const offsets = (from, frame) => scenes(at(series({ type: 'slide', durationInFrames: 3, from }, 17), frame).scene.layers)
    .map(({ group }) => [group.transform.position.x, group.transform.position.y]);
  // Frame 8 is the second of the three overlapped frames: progress 2/4.
  assert.deepEqual(offsets('left', 8), [[50, 0], [-50, 0]]);
  assert.deepEqual(offsets('right', 8), [[-50, 0], [50, 0]]);
  assert.deepEqual(offsets('top', 8), [[0, 25], [0, -25]]);
  assert.deepEqual(offsets('bottom', 8), [[0, -25], [0, 25]]);
  assert.deepEqual(offsets('left', 7), [[25, 0], [-75, 0]]);
  assert.deepEqual(offsets('left', 10), [[0, 0]]);
});

test('a wipe clips the next scene to a growing strip from its edge', () => {
  const clip = (from, frame, easing) => {
    const shown = scenes(at(series({ type: 'wipe', durationInFrames: 4, from, easing }, 16), frame).scene.layers);
    assert.equal(shown[0].group.content.clip, undefined);
    return shown[1].group.content.clip;
  };
  const rect = (x, y, width, height) => ({ x, y, width, height, cornerRadius: 0 });
  assert.deepEqual(clip('left', 6), rect(0, 0, 20, 50));
  assert.deepEqual(clip('right', 7), rect(60, 0, 40, 50));
  assert.deepEqual(clip('top', 8), rect(0, 0, 100, 30));
  assert.deepEqual(clip('bottom', 9), rect(0, 10, 100, 40));
  assert.deepEqual(clip('left', 6, (p) => p * p), rect(0, 0, 100 * (0.2 * 0.2), 50));
});

test('scenes read their transitions; audio overlaps and useTransitionVolume cross-fades it', () => {
  const seen = [];
  function Voiced({ name }) {
    const scene = useTransitionSeriesScene();
    seen.push(`${name}:${JSON.stringify(scene)}`);
    return h(Audio, { src: `${name}.wav`, volume: useTransitionVolume(0.5) });
  }
  const composition = mount(() => h(Composition, { width: 100, height: 50, fps: FPS, durationInFrames: 26 },
    h(TransitionSeries, null,
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Voiced, { name: 'a' })),
      h(TransitionSeries.Transition, { type: 'crossfade', durationInFrames: 4 }),
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Voiced, { name: 'b' })),
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Voiced, { name: 'c' })),
    ),
  ));
  const us = (seconds) => ({ value: Math.round(seconds * 1e6), timescale: 1e6 });
  const clip = (src, start, volume) => ({ src, sourceStart: 0, playbackRate: 1, volume, muted: false, start, duration: 1 });
  seen.length = 0;
  assert.deepEqual(at(composition, 7).audio, [
    clip('a.wav', 0, { type: 'keyframes', keyframes: [{ time: us(0.6), value: 0.5 }, { time: us(1), value: 0 }] }),
    clip('b.wav', 0.6, { type: 'keyframes', keyframes: [{ time: us(0), value: 0 }, { time: us(0.4), value: 0.5 }] }),
  ]);
  assert.deepEqual(seen, [
    'a:{"index":0,"enter":null,"exit":{"type":"crossfade","durationInFrames":4}}',
    'b:{"index":1,"enter":{"type":"crossfade","durationInFrames":4},"exit":null}',
  ]);
  assert.deepEqual(at(composition, 16).audio, [clip('c.wav', 1.6, 0.5)]);
});

test('TransitionSeries parts and hooks throw outside a series', () => {
  const render = (child) => () => mount(() => h(Composition, { width: 10, height: 10, fps: FPS, durationInFrames: 10 }, child));
  assert.throws(render(h(TransitionSeries.Sequence, { durationInFrames: 5 })), /must be a direct child of <TransitionSeries>/);
  assert.throws(render(h(TransitionSeries.Transition, { type: 'cut' })), /must be a direct child of <TransitionSeries>/);
  assert.throws(render(h(TransitionSeries, null, h(Text, null, 'x'))), /children must be/);
  function Lost() {
    useTransitionSeriesScene();
    return null;
  }
  assert.throws(render(h(Lost)), /inside a <TransitionSeries.Sequence>/);
  assert.throws(
    render(h(TransitionSeries, null,
      h(TransitionSeries.Sequence, { durationInFrames: 5 }),
      h(TransitionSeries.Transition, { type: 'slide', durationInFrames: 2, from: 'center' }),
      h(TransitionSeries.Sequence, { durationInFrames: 5 }),
    )),
    /unknown from "center"/,
  );
});
