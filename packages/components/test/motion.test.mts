import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';
import type { ReactNode } from 'react';

import { Audio, Composition, Easings, Sequence, Text, Video, useCurrentFrame } from '@celesta.studio/react';
import type { Layer } from '@celesta.studio/react';
import { mount } from '@celesta.studio/react/internal';
import type { MountedComposition } from '@celesta.studio/react/internal';
import { Transition } from '@celesta.studio/transitions';
import type { TransitionEffect } from '@celesta.studio/transitions';
import { Motion } from '../src/index.js';
import type { MotionEffect, MotionPreset, MotionProps } from '../src/index.js';

const h = React.createElement;

const at = (composition: MountedComposition, frame: number) => composition.renderAt({ value: frame, timescale: 30 }, null);
const root = (durationInFrames: number, ...children: ReactNode[]) => () =>
  h(Composition, { width: 320, height: 240, fps: 30, durationInFrames }, ...children);

// The first layer whose id ends with `id`, searched depth first.
function find(layers: Layer[], id: string): Layer | undefined {
  for (const layer of layers) {
    if (layer.id.endsWith(id)) return layer;
    if (layer.content.type === 'group') {
      const found = find(layer.content.layers, id);
      if (found) return found;
    }
  }
  return undefined;
}

// What the effects decide: the group's opacity, position, and scale.
interface Pose {
  opacity: number;
  x: number;
  y: number;
  scaleX: number;
  scaleY: number;
}

function pose(composition: MountedComposition, frame: number, id = 'motion'): Pose | null {
  const layer = find(at(composition, frame).scene.layers, id);
  if (!layer) return null;
  const { position, scale } = layer.transform;
  return { opacity: layer.opacity, x: position.x, y: position.y, scaleX: scale.x, scaleY: scale.y };
}

// A child that shows the clock it sees.
function Clock() {
  return h(Text, { id: 'clock', x: 12, y: 34, opacity: 0.5, scale: 1.5, children: String(useCurrentFrame()) });
}

function clockText(composition: MountedComposition, frame: number): string | null {
  const clock = find(at(composition, frame).scene.layers, 'clock');
  return clock?.content.type === 'text' ? clock.content.text : null;
}

const REST: Pose = { opacity: 0.8, x: 100, y: 50, scaleX: 2, scaleY: 2 };
const own = { id: 'motion', opacity: 0.8, x: 100, y: 50, scale: 2 };

test('Motion holds its own opacity, position, and scale exactly between the entrance and the exit', () => {
  const composition = mount(root(60, h(Motion, {
    ...own,
    enter: { type: 'fade-up', durationInFrames: 10 },
    exit: { type: 'scale', durationInFrames: 8 },
  }, h(Clock))));
  for (let frame = 9; frame <= 52; frame += 1) {
    assert.deepEqual(pose(composition, frame), REST, `frame ${frame}`);
  }
  assert.notDeepEqual(pose(composition, 8), REST);
  assert.notDeepEqual(pose(composition, 53), REST);
  // The children keep their own props.
  const clock = find(at(composition, 30).scene.layers, 'clock');
  assert.ok(clock);
  assert.deepEqual([clock.opacity, clock.transform.position.x, clock.transform.position.y, clock.transform.scale.x],
    [0.5, 12, 34, 1.5]);
});

test('each preset draws what the same <Transition> draws, entrance and exit alike', () => {
  const cases: [MotionPreset, TransitionEffect, TransitionEffect][] = [
    ['fade', { type: 'fade' }, { type: 'fade' }],
    ['fade-up', { type: ['fade', 'slide'], slideFrom: 'bottom' }, { type: ['fade', 'slide'], slideFrom: 'top' }],
    ['fade-down', { type: ['fade', 'slide'], slideFrom: 'top' }, { type: ['fade', 'slide'], slideFrom: 'bottom' }],
    ['fade-left', { type: ['fade', 'slide'], slideFrom: 'right' }, { type: ['fade', 'slide'], slideFrom: 'left' }],
    ['fade-right', { type: ['fade', 'slide'], slideFrom: 'left' }, { type: ['fade', 'slide'], slideFrom: 'right' }],
    ['scale', { type: ['fade', 'scale'] }, { type: ['fade', 'scale'] }],
  ];
  const easing = Easings.easeInOutQuad;
  for (const [preset, enter, exit] of cases) {
    const motion = mount(root(40, h(Motion, {
      ...own,
      enter: { type: preset, durationInFrames: 12, distance: 30, scaleFrom: 0.5, easing },
      exit: { type: preset, durationInFrames: 9, distance: 30, scaleFrom: 0.5, easing },
    }, h(Clock))));
    // The nested pair Motion replaces, one group inside the other.
    const nested = mount(root(40,
      h(Transition, { ...own, ...exit, id: 'outer', direction: 'out', durationInFrames: 9, distance: 30, scaleFrom: 0.5, easing },
        h(Transition, { ...enter, id: 'inner', durationInFrames: 12, distance: 30, scaleFrom: 0.5, easing }, h(Clock)))));
    for (let frame = 0; frame < 40; frame += 1) {
      const outer = pose(nested, frame, 'outer');
      const inner = pose(nested, frame, 'inner');
      assert.ok(outer && inner);
      assert.deepEqual(pose(motion, frame), {
        opacity: outer.opacity * inner.opacity,
        x: outer.x + inner.x,
        y: outer.y + inner.y,
        scaleX: outer.scaleX * inner.scaleX,
        scaleY: outer.scaleY * inner.scaleY,
      }, `${preset} frame ${frame}`);
    }
  }
});

test('directions name the way the element moves', () => {
  const y = (enter: MotionPreset | undefined, exit: MotionPreset | undefined, frame: number) => pose(mount(root(30, h(Motion, {
    id: 'motion', enter: enter && { type: enter, durationInFrames: 5 }, exit: exit && { type: exit, durationInFrames: 5 },
  }))), frame)!.y;
  assert.equal(y('fade-up', undefined, 0), 64);
  assert.equal(y('fade-down', undefined, 0), -64);
  assert.equal(y(undefined, 'fade-up', 29), -64);
  assert.equal(y(undefined, 'fade-down', 29), 64);
});

test('a preset name alone eases over half a second with easeOutCubic', () => {
  const short = mount(root(60, h(Motion, { id: 'motion', enter: 'fade', exit: 'fade' })));
  const long = mount(root(60, h(Motion, {
    id: 'motion',
    enter: { type: 'fade', durationInFrames: 15, easing: Easings.easeOutCubic },
    exit: { type: 'fade', durationInFrames: 15, easing: Easings.easeOutCubic },
  })));
  for (const frame of [0, 5, 14, 30, 45, 52, 59]) {
    assert.deepEqual(pose(short, frame), pose(long, frame), `frame ${frame}`);
  }
  assert.equal(pose(short, 0)!.opacity, 0);
  assert.equal(pose(short, 14)!.opacity, 1);
  assert.equal(pose(short, 45)!.opacity, 1);
  assert.equal(pose(short, 59)!.opacity, 0);
});

test('omitting enter or exit cuts, and a one-frame effect is a cut', () => {
  const plain = mount(root(10, h(Motion, own)));
  const cut = mount(root(10, h(Motion, { ...own, enter: { type: 'fade', durationInFrames: 1 }, exit: { type: 'fade', durationInFrames: 0 } })));
  for (let frame = 0; frame < 10; frame += 1) {
    assert.deepEqual(pose(plain, frame), REST);
    assert.deepEqual(pose(cut, frame), REST);
  }
});

test('a one-frame window shows the element at rest', () => {
  const composition = mount(root(30, h(Motion, {
    ...own, from: 4, durationInFrames: 1, enter: 'fade-up', exit: 'scale',
  }, h(Clock))));
  assert.equal(pose(composition, 3), null);
  assert.deepEqual(pose(composition, 4), REST);
  assert.equal(clockText(composition, 4), '0');
  assert.equal(pose(composition, 5), null);
});

test('an entrance and exit longer than the window shrink in proportion and meet on one frame at rest', () => {
  const composition = mount(root(6, h(Motion, {
    ...own, enter: { type: 'fade', durationInFrames: 6 }, exit: { type: 'fade', durationInFrames: 6, easing: Easings.linear },
  })));
  // Five frames of movement split 3:2, linear on both sides here.
  const opacities = [0, 1, 2, 3, 4, 5].map((frame) => pose(composition, frame)!.opacity / own.opacity);
  const enter = (t: number) => Easings.easeOutCubic(t);
  assert.deepEqual(opacities.map((o) => Number(o.toFixed(6))),
    [enter(0), enter(1 / 3), enter(2 / 3), 1, 0.5, 0].map((o) => Number(o.toFixed(6))));
  assert.deepEqual(pose(composition, 3), REST);

  // Two frames: hidden, then at rest.
  const two = mount(root(2, h(Motion, { ...own, enter: { type: 'fade', durationInFrames: 12 }, exit: { type: 'fade', durationInFrames: 10 } })));
  assert.deepEqual([pose(two, 0)!.opacity, pose(two, 1)], [0, REST]);
});

test('from and durationInFrames open the same window and clock as <Sequence>', () => {
  const window = { from: 10, durationInFrames: 20 };
  const motion = mount(root(40, h(Motion, { ...window, enter: 'fade', exit: 'fade' }, h(Clock))));
  const sequence = mount(root(40, h(Sequence, window, h(Clock))));
  for (let frame = 0; frame < 40; frame += 1) {
    assert.equal(clockText(motion, frame), clockText(sequence, frame), `frame ${frame}`);
  }
  assert.equal(clockText(motion, 9), null);
  assert.equal(clockText(motion, 10), '0');
  assert.equal(clockText(motion, 29), '19');
  assert.equal(clockText(motion, 30), null);

  // The exit ends with the Motion's own window, not the composition.
  const exit = mount(root(40, h(Motion, { ...window, id: 'motion', exit: { type: 'fade', durationInFrames: 5 } })));
  assert.equal(pose(exit, 25)!.opacity, 1);
  assert.equal(pose(exit, 29)!.opacity, 0);
  assert.equal(pose(exit, 30), null);
});

test('Motion leaves media timing and volume as <Sequence> plays them', () => {
  const media = () => [h(Audio, { key: 'a', src: './voice.wav', volume: 0.7 }), h(Video, { key: 'v', src: './clip.mp4', playbackRate: 2 })];
  for (const window of [{}, { from: 6, durationInFrames: 20 }]) {
    const motion = mount(root(40, h(Motion, { ...window, enter: 'fade-up', exit: 'scale' }, ...media())));
    const sequence = mount(root(40, h(Sequence, window, ...media())));
    for (const frame of [0, 6, 7, 15, 25, 26, 39]) {
      assert.deepEqual(at(motion, frame).audio, at(sequence, frame).audio, `frame ${frame}`);
    }
    assert.deepEqual(motion.collectAudio(), sequence.collectAudio());
  }
});

test('without a window, the children keep the enclosing clock and the exit ends with the enclosing sequence', () => {
  const composition = mount(root(60,
    h(Sequence, { from: 5, durationInFrames: 30 },
      h(Sequence, { from: 10 },
        h(Motion, { ...own, enter: { type: 'fade-up', durationInFrames: 6 }, exit: { type: 'fade', durationInFrames: 4 } }, h(Clock))))));
  const flat = mount(root(20, h(Motion, { ...own, enter: { type: 'fade-up', durationInFrames: 6 }, exit: { type: 'fade', durationInFrames: 4 } }, h(Clock))));
  for (let local = 0; local < 20; local += 1) {
    assert.deepEqual(pose(composition, 15 + local), pose(flat, local), `local frame ${local}`);
    assert.equal(clockText(composition, 15 + local), String(local));
  }
  assert.equal(pose(composition, 34)!.opacity, 0);
  assert.equal(pose(composition, 35), null);
});

test('seeking to any frame gives what playing up to it gives', () => {
  const composition = () => mount(root(30, h(Motion, {
    ...own, from: 3, durationInFrames: 24, enter: 'fade-left', exit: { type: 'fade-right', durationInFrames: 7 },
  }, h(Clock))));
  const played = composition();
  const expected = Array.from({ length: 30 }, (_, frame) => pose(played, frame));
  const seeked = composition();
  for (const frame of [17, 3, 29, 4, 26, 0, 12, 25, 12]) {
    assert.deepEqual(pose(seeked, frame), expected[frame], `frame ${frame}`);
  }
});

test('Motion rejects unknown presets and bad numbers', () => {
  const render = (props: MotionProps) => () => at(mount(root(10, h(Motion, props))), 0);
  // Not presets, cast to check the errors.
  assert.throws(render({ enter: 'slide-up' as MotionEffect }), /unknown <Motion> enter "slide-up"/);
  assert.throws(render({ exit: { type: 'zoom' as MotionPreset } }), /unknown <Motion> exit "zoom"/);
  assert.throws(render({ enter: { type: 'fade', durationInFrames: -1 } }), /enter durationInFrames/);
  assert.throws(render({ exit: { type: 'fade', durationInFrames: 1.5 } }), /exit durationInFrames/);
  assert.throws(render({ enter: { type: 'fade-up', distance: Number.NaN } }), /distance/);
  assert.throws(render({ exit: { type: 'scale', scaleFrom: -1 } }), /scaleFrom/);
});
