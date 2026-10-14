import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';

import { Audio, Composition, Group, Rect, Text, useCurrentFrame } from '@celesta/react';
import { Fit, SafeArea } from '@celesta/layout';
import { defineShader } from '@celesta/shader';
import {
  TransitionSeries,
  blur,
  clockWipe,
  crossfade,
  cube,
  defineTransition,
  dissolve,
  flip,
  iris,
  slide,
  useTransitionSeriesScene,
  useTransitionVolume,
  wipe,
  zoom,
} from '../dist/index.js';
import { mount } from '@celesta/react/internal';

const h = React.createElement;
const FPS = 10;
const at = (composition, frame) => composition.renderAt({ value: frame, timescale: FPS }, null);
const near = (actual, expected, label) => assert.ok(Math.abs(actual - expected) < 1e-9, `${label}: ${actual} is not ${expected}`);

function Scene({ name }) {
  const frame = useCurrentFrame();
  return h(Text, null, `${name}:${frame}`);
}

/**
 * Two 10-frame scenes, a and b, in a 100×50 composition, joined by
 * `transition` over 4 frames (6–9), and wrapped by `wrap`.
 */
function series(transition, { wrap = (child) => child, props = null } = {}) {
  return mount(() => h(Composition, { width: 100, height: 50, fps: FPS, durationInFrames: 16 },
    wrap(h(TransitionSeries, props,
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Scene, { name: 'a' })),
      h(TransitionSeries.Transition, { durationInFrames: 4, ...transition }),
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Scene, { name: 'b' })),
    )),
  ));
}

/** The scene groups of a frame: [a, b] while they overlap. */
function groups(frame, transition, options) {
  let layers = at(series(transition, options), frame).scene.layers;
  // Past the group of a <SafeArea> or <Fit> to the series' sequences.
  if (options?.wrap) layers = layers[0].content.layers;
  return layers.map((sequence) => sequence.content.layers[0]);
}

/** Frame 7 is the second of the four overlapped frames: progress 2/5. */
const MIDDLE = 7;

test('defineTransition checks its name and present function', () => {
  assert.throws(() => defineTransition(null), /expects an object/);
  assert.throws(() => defineTransition({ name: '', present: () => ({}) }), /non-empty string/);
  assert.throws(() => defineTransition({ name: 'cut', present: () => ({}) }), /takes no frames/);
  assert.throws(() => defineTransition({ name: 'spin' }), /spin's present must be a function/);
  const spin = defineTransition({ name: 'spin', present: () => ({}) });
  assert.equal(spin.name, 'spin');
  assert.ok(Object.isFrozen(spin));
});

test('a defined transition gets the progress, frame, direction, and size, and styles both scenes', () => {
  const inputs = [];
  const tilt = defineTransition({
    name: 'tilt',
    present: (input) => {
      inputs.push(input);
      return { enter: { opacity: input.progress, x: 3 }, exit: { rotation: input.progress * 10 } };
    },
  });
  const easing = (p) => p * p;
  const [a, b] = groups(MIDDLE, { type: tilt, from: 'bottom', easing });
  const { progress, ...rest } = inputs.at(-1);
  near(progress, 0.16, 'eased progress');
  assert.deepEqual(rest, { frame: 1, durationInFrames: 4, from: 'bottom', width: 100, height: 50 });
  near(a.transform.rotation, 1.6, 'exit rotation');
  near(b.opacity, 0.16, 'enter opacity');
  assert.equal(b.transform.position.x, 3);
  // Outside the overlap, each scene is drawn as it is.
  const [after] = groups(11, { type: tilt });
  assert.equal(after.opacity, 1);
  assert.equal(after.transform.position.x, 0);
});

test('a defined transition can mask and shade the scenes, and keeps their layer ids', () => {
  const tint = defineShader({ name: 'tint', wgsl: 'fn effect(input: EffectInput) -> vec4f { return source_at(input.position); }', params: { amount: 'f32' } });
  const reveal = defineTransition({
    name: 'reveal',
    present: ({ progress, width, height }) => ({
      enter: { mask: h(Rect, { width: width * progress, height, fill: '#FFFFFF' }) },
      exit: { shader: tint({ amount: progress }) },
    }),
  });
  const composition = series({ type: reveal });
  const ids = (frame) => at(composition, frame).scene.layers.at(-1).content.layers[0].content.layers.map((layer) => layer.id);
  const [a, b] = at(composition, MIDDLE).scene.layers.map((sequence) => sequence.content.layers[0]);
  assert.equal(b.content.mask.layers[0].content.width, 40);
  assert.deepEqual(a.effects.filters, [{ type: 'shader', id: tint.id, params: [0.4] }]);
  // The scene's own layers keep their ids as the mask comes and goes.
  assert.deepEqual(ids(MIDDLE), ids(12));
  assert.equal('mask' in at(composition, 12).scene.layers[0].content.layers[0].content, false);
});

test('a defined transition names itself to the scenes and must return styles', () => {
  const seen = [];
  function Named() {
    seen.push(useTransitionSeriesScene());
    return null;
  }
  const spin = defineTransition({ name: 'spin', present: () => ({}) });
  mount(() => h(Composition, { width: 10, height: 10, fps: FPS, durationInFrames: 16 },
    h(TransitionSeries, null,
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Named)),
      h(TransitionSeries.Transition, { type: spin, durationInFrames: 4 }),
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Named)),
    ),
  ));
  // Frame 0 shows the first scene, leaving through the spin.
  assert.deepEqual(seen.at(-1).exit, { type: 'spin', durationInFrames: 4 });
  for (const [present, message] of [
    [() => null, /broken's present must return an object/],
    [() => ({ enter: 3 }), /broken's enter style must be an object/],
    [() => ({ exit: { children: 'x' } }), /broken's exit style cannot set children/],
  ]) {
    const broken = defineTransition({ name: 'broken', present });
    assert.throws(() => groups(MIDDLE, { type: broken }), message);
  }
});

test('the built-in functions give the same transitions as the names', () => {
  for (const [name, definition] of [['crossfade', crossfade()], ['slide', slide()], ['wipe', wipe()]]) {
    assert.equal(definition.name, name);
    assert.deepEqual(groups(MIDDLE, { type: definition, from: 'right' }), groups(MIDDLE, { type: name, from: 'right' }));
  }
  for (const [name, definition] of [
    ['iris', iris()], ['clock-wipe', clockWipe()], ['zoom', zoom()], ['dissolve', dissolve()],
    ['blur', blur()], ['flip', flip()], ['cube', cube()],
  ]) {
    assert.equal(definition.name, name);
    assert.deepEqual(groups(MIDDLE, { type: definition }), groups(MIDDLE, { type: name }));
  }
});

test('the series size comes from width and height, then the layout bounds, then the composition', () => {
  const offset = (options) => groups(MIDDLE, { type: 'slide' }, options)[1].transform.position.x;
  // Two of five steps in from the left: the entering scene is 3/5 of the width away.
  assert.equal(offset(), -60);
  assert.equal(offset({ props: { width: 20, height: 10 } }), -12);
  assert.equal(offset({ wrap: (child) => h(SafeArea, { padding: { left: 10, right: 40 } }, child) }), -30);
  assert.equal(offset({ wrap: (child) => h(Fit, { sourceWidth: 400, sourceHeight: 200 }, child) }), -240);
  // A wipe in a 40 px frame crosses the frame, not the composition.
  const clip = groups(MIDDLE, { type: 'wipe' }, { props: { width: 40, height: 20 } })[1].content.clip;
  assert.deepEqual(clip, { x: 0, y: 0, width: 16, height: 20, cornerRadius: 0 });
  assert.throws(() => groups(MIDDLE, { type: 'slide' }, { props: { width: 0 } }), /width must be a positive finite number/);
  assert.throws(() => groups(MIDDLE, { type: 'slide' }, { props: { height: Number.NaN } }), /height must be a positive finite number/);
});

test('without width and height, a series keeps drawing as it did', () => {
  const plain = groups(MIDDLE, { type: 'wipe', from: 'right' });
  const sized = groups(MIDDLE, { type: 'wipe', from: 'right' }, { props: { width: 100, height: 50 } });
  assert.deepEqual(plain, sized);
});

test('an iris masks the next scene with a disc that opens to the farthest corner, or closes', () => {
  const disc = (transition) => {
    const [, b] = groups(MIDDLE, { type: transition });
    return { mask: b.content.mask, rect: b.content.mask.layers[0] };
  };
  const reach = Math.hypot(50, 25);
  const open = disc('iris');
  assert.equal('invert' in open.mask, false);
  near(open.rect.content.width, 2 * 0.4 * reach, 'open diameter');
  near(open.rect.content.cornerRadius, 0.4 * reach, 'open radius');
  near(open.rect.transform.position.x, 50 - 0.4 * reach, 'open left');
  const closed = disc(iris({ direction: 'close', center: { x: 0, y: 0 } }));
  assert.equal(closed.mask.invert, true);
  near(closed.rect.content.cornerRadius, 0.6 * Math.hypot(100, 50), 'closing radius');
  near(closed.rect.transform.position.x, -0.6 * Math.hypot(100, 50), 'closing left');
  const soft = disc(iris({ feather: 10 }));
  const radius = 0.4 * (reach + 10);
  assert.equal(soft.rect.content.fill.type, 'radial');
  near(soft.rect.content.fill.stops[1].offset, 1 - 10 / radius, 'feather stop');
  assert.throws(() => iris({ feather: -1 }), /iris feather must be a finite number of at least 0/);
  assert.throws(() => iris({ direction: 'sideways' }), /iris direction must be one of "open", "close"/);
  assert.throws(() => iris({ center: { x: 0.5 } }), /iris center.y must be a finite number/);
});

test('a clock wipe masks the next scene with a sector swept from 12 o\'clock', () => {
  const sector = (transition) => groups(MIDDLE, { type: transition })[1].content.mask.layers[0].content.commands;
  // 2/5 of a turn is 144°: five chords of 28.8°.
  const commands = sector('clock-wipe');
  assert.deepEqual(commands[0], { type: 'moveTo', x: 50, y: 25 });
  assert.equal(commands.length, 1 + 6 + 1);
  const reach = Math.hypot(50, 25) / Math.cos(Math.PI / 12) + 1;
  near(commands[1].x, 50, 'start x');
  near(commands[1].y, 25 - reach, 'start y');
  near(commands[6].x, 50 + reach * Math.sin((144 * Math.PI) / 180), 'end x');
  const back = sector(clockWipe({ direction: 'counterclockwise', startAngle: 90 }));
  near(back[1].x, 50 + reach, 'start at 3 o\'clock');
  near(back[6].x, 50 + reach * Math.sin(((90 - 144) * Math.PI) / 180), 'swept back');
  // The first frame's 72° takes three chords, none over 30°.
  assert.equal(groups(6, { type: 'clock-wipe' })[1].content.mask.layers[0].content.commands.length, 1 + 4 + 1);
});

test('a zoom scales both scenes about the centre while the next one fades in', () => {
  const [a, b] = groups(MIDDLE, { type: 'zoom' });
  const exitScale = 1 + 0.5 * 0.4;
  const enterScale = 1 / 1.5 + (1 - 1 / 1.5) * 0.4;
  near(a.transform.scale.x, exitScale, 'exit scale');
  near(a.transform.position.x, 50 * (1 - exitScale), 'exit x');
  near(a.transform.position.y, 25 * (1 - exitScale), 'exit y');
  near(b.transform.scale.x, enterScale, 'enter scale');
  near(b.opacity, 0.4, 'enter opacity');
  const [outA, outB] = groups(MIDDLE, { type: zoom({ direction: 'out', scale: 2, center: { x: 0, y: 1 } }) });
  near(outA.transform.scale.x, 1 - 0.5 * 0.4, 'out exit scale');
  near(outA.transform.position.y, 50 * 0.5 * 0.4, 'out exit y about the bottom');
  near(outB.transform.scale.x, 2 - 0.4, 'out enter scale');
  assert.throws(() => zoom({ scale: 0 }), /zoom scale must be a finite number/);
});

test('a dissolve shows the next scene square by square, the same way every time', () => {
  const area = (transition, frame = MIDDLE) => {
    const mask = groups(frame, { type: transition })[1].content.mask;
    const commands = mask.layers[0]?.content.commands ?? [];
    let total = 0;
    for (let index = 0; index < commands.length; index += 5) {
      total += (commands[index + 1].x - commands[index].x) * (commands[index + 2].y - commands[index + 1].y);
    }
    return { total, commands };
  };
  // 100×50 at 5 px: 200 squares, about 2/5 of them shown.
  const fine = area(dissolve({ cellSize: 5 }));
  assert.ok(fine.total > 0.25 * 5000 && fine.total < 0.55 * 5000, `shown area ${fine.total}`);
  assert.deepEqual(area(dissolve({ cellSize: 5 })).commands, fine.commands);
  assert.notDeepEqual(area(dissolve({ cellSize: 5, seed: 1 })).commands, fine.commands);
  // More frames show more; a run of squares is one rectangle.
  assert.ok(area(dissolve({ cellSize: 5 }), 9).total > fine.total);
  for (let index = 0; index < fine.commands.length; index += 5) {
    assert.equal(fine.commands[index].type, 'moveTo');
    assert.equal(fine.commands[index + 4].type, 'close');
  }
  assert.throws(() => dissolve({ cellSize: -2 }), /dissolve cellSize must be a finite number/);
});

test('a blur blurs the previous scene away and the next one into focus', () => {
  const [a, b] = groups(MIDDLE, { type: blur({ amount: 20 }) });
  near(a.effects.blur, 8, 'exit blur');
  near(b.effects.blur, 12, 'enter blur');
  near(b.opacity, 0.4, 'enter opacity');
  assert.throws(() => blur({ amount: 65 }), /blur amount must be a finite number between 0 and 64/);
});

test('a flip narrows the previous scene to its edge, then widens the next from it', () => {
  const first = groups(6, { type: 'flip' });
  const angle = Math.PI / 5;
  near(first[0].transform.scale.x, Math.cos(angle), 'narrowing');
  near(first[0].transform.position.x, 50 * (1 - Math.cos(angle)), 'about the centre');
  near(first[0].effects.filters[0].amount, 1 - 0.4 * Math.sin(angle), 'shaded');
  assert.equal(first[1].opacity, 0);
  const last = groups(9, { type: flip({ shade: 0 }), from: 'top' });
  assert.equal(last[0].opacity, 0);
  near(last[1].transform.scale.y, Math.abs(Math.cos(Math.PI * 0.8)), 'widening');
  assert.equal(last[1].transform.scale.x, 1);
  assert.equal(last[1].effects, undefined);
});

test('a cube turns two faces that meet at an edge and together span the series', () => {
  for (const from of ['left', 'right']) {
    const [a, b] = groups(MIDDLE, { type: 'cube', from });
    const angle = (Math.PI / 2) * 0.4;
    const fit = 1 / (Math.cos(angle) + Math.sin(angle));
    const exitWidth = 100 * a.transform.scale.x;
    const enterWidth = 100 * b.transform.scale.x;
    near(exitWidth, 100 * fit * Math.cos(angle), `${from} exit face`);
    near(enterWidth, 100 * fit * Math.sin(angle), `${from} enter face`);
    near(exitWidth + enterWidth, 100, `${from} faces span the width`);
    const [left, right] = from === 'left' ? [b, a] : [a, b];
    near(left.transform.position.x, 0, `${from} starts at the left edge`);
    near(right.transform.position.x, 100 * left.transform.scale.x, `${from} faces meet`);
    near(a.transform.scale.y, fit, `${from} scaled down`);
    near(a.transform.position.y, 25 * (1 - fit), `${from} centred`);
  }
  const [, b] = groups(MIDDLE, { type: 'cube', from: 'top' });
  near(b.transform.position.y, 0, 'from the top');
  near(b.transform.scale.x, 1 / (Math.cos(Math.PI / 5) + Math.sin(Math.PI / 5)), 'scaled across');
});

test('useTransitionVolume cross-fades the sound of every type the same way', () => {
  const types = ['crossfade', 'slide', 'wipe', 'iris', 'clock-wipe', 'zoom', 'dissolve', 'blur', 'flip', 'cube'];
  const tracks = types.map((type) => {
    function Voiced({ name }) {
      return h(Audio, { src: `${name}.wav`, volume: useTransitionVolume(0.5) });
    }
    const composition = mount(() => h(Composition, { width: 100, height: 50, fps: FPS, durationInFrames: 16 },
      h(TransitionSeries, null,
        h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Voiced, { name: 'a' })),
        h(TransitionSeries.Transition, { type, durationInFrames: 4 }),
        h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Voiced, { name: 'b' })),
      ),
    ));
    return at(composition, MIDDLE).audio;
  });
  const us = (seconds) => ({ value: Math.round(seconds * 1e6), timescale: 1e6 });
  assert.deepEqual(tracks[0].map((clip) => clip.volume), [
    { type: 'keyframes', keyframes: [{ time: us(0.6), value: 0.5 }, { time: us(1), value: 0 }] },
    { type: 'keyframes', keyframes: [{ time: us(0), value: 0 }, { time: us(0.4), value: 0.5 }] },
  ]);
  for (const [index, track] of tracks.entries()) {
    assert.deepEqual(track, tracks[0], types[index]);
  }
});

test('the masks of the built-in transitions keep the scene\'s layer ids', () => {
  for (const type of ['iris', 'clock-wipe', 'dissolve']) {
    const composition = series({ type });
    const ids = (frame) => at(composition, frame).scene.layers.at(-1).content.layers[0].content.layers.map((layer) => layer.id);
    assert.deepEqual(ids(MIDDLE), ids(12), type);
  }
  // A group in the scene keeps its id too.
  const composition = mount(() => h(Composition, { width: 100, height: 50, fps: FPS, durationInFrames: 16 },
    h(TransitionSeries, null,
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Group, { id: 'card' })),
      h(TransitionSeries.Transition, { type: 'iris', durationInFrames: 4 }),
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Group, null, h(Rect, { width: 1, height: 1 }))),
    ),
  ));
  const inner = (frame) => at(composition, frame).scene.layers.at(-1).content.layers[0].content.layers[0].id;
  assert.equal(inner(MIDDLE), inner(12));
});
