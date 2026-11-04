import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';
import type { ReactElement } from 'react';

import { Audio, Composition, Group, Rect, Text, useCurrentFrame } from '@celesta.studio/react';
import type { Layer, LayerFilter, Paint, PathCommand } from '@celesta.studio/react';
import { Fit, SafeArea } from '@celesta.studio/layout';
import { defineShader } from '@celesta.studio/shader';
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
} from '../src/index.js';
import type {
  IrisOptions,
  TransitionCenter,
  TransitionDefinition,
  TransitionPresentInput,
  TransitionSeriesProps,
  TransitionSeriesScene,
  TransitionSeriesTransitionProps,
  TransitionSeriesType,
} from '../src/index.js';
import { mount } from '@celesta.studio/react/internal';
import type { MountedComposition } from '@celesta.studio/react/internal';
import { childrenOf, contentOf } from './helpers.mjs';

const h = React.createElement;
const FPS = 10;
const at = (composition: MountedComposition, frame: number) => composition.renderAt({ value: frame, timescale: FPS }, null);
const near = (actual: number | undefined, expected: number, label: string) =>
  assert.ok(actual !== undefined && Math.abs(actual - expected) < 1e-9, `${label}: ${actual} is not ${expected}`);

/** The end point of a path command: any but `close`. */
function endOf(command: PathCommand | undefined): { x: number; y: number } {
  assert.ok(command && command.type !== 'close', `${command?.type} has an end point`);
  return command;
}

/** The last of `layers`. */
const lastOf = (layers: Layer[]): Layer | undefined => layers[layers.length - 1];

function Scene({ name }: { name: string }) {
  const frame = useCurrentFrame();
  return h(Text, null, `${name}:${frame}`);
}

/**
 * Two 10-frame scenes, a and b, in a 100×50 composition, joined by
 * `transition` over 4 frames (6–9), and wrapped by `wrap`.
 */
interface SeriesOptions {
  wrap?: (child: ReactElement) => ReactElement;
  props?: TransitionSeriesProps | null;
}

function series(transition: TransitionSeriesTransitionProps, { wrap = (child) => child, props = null }: SeriesOptions = {}) {
  return mount(() => h(Composition, { width: 100, height: 50, fps: FPS, durationInFrames: 16 },
    wrap(h(TransitionSeries, props,
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Scene, { name: 'a' })),
      h(TransitionSeries.Transition, { durationInFrames: 4, ...transition }),
      h(TransitionSeries.Sequence, { durationInFrames: 10 }, h(Scene, { name: 'b' })),
    )),
  ));
}

/** The scene groups of a frame: [a, b] while they overlap. */
function groups(frame: number, transition: TransitionSeriesTransitionProps, options?: SeriesOptions): Layer[] {
  let layers = at(series(transition, options), frame).scene.layers;
  // Past the group of a <SafeArea> or <Fit> to the series' sequences.
  if (options?.wrap) layers = childrenOf(layers[0]);
  return layers.map((sequence) => childrenOf(sequence)[0]);
}

/** Frame 7 is the second of the four overlapped frames: progress 2/5. */
const MIDDLE = 7;

test('defineTransition checks its name and present function', () => {
  // Wrong on purpose: the checks are for untyped JavaScript callers.
  assert.throws(() => defineTransition(null as unknown as TransitionDefinition), /expects an object/);
  assert.throws(() => defineTransition({ name: '', present: () => ({}) }), /non-empty string/);
  assert.throws(() => defineTransition({ name: 'cut', present: () => ({}) }), /takes no frames/);
  // Wrong on purpose, as above.
  assert.throws(() => defineTransition({ name: 'spin' } as TransitionDefinition), /spin's present must be a function/);
  const spin = defineTransition({ name: 'spin', present: () => ({}) });
  assert.equal(spin.name, 'spin');
  assert.ok(Object.isFrozen(spin));
});

test('a defined transition gets the progress, frame, direction, and size, and styles both scenes', () => {
  const inputs: TransitionPresentInput[] = [];
  const tilt = defineTransition({
    name: 'tilt',
    present: (input) => {
      inputs.push(input);
      return { enter: { opacity: input.progress, x: 3 }, exit: { rotation: input.progress * 10 } };
    },
  });
  const easing = (p: number) => p * p;
  const [a, b] = groups(MIDDLE, { type: tilt, from: 'bottom', easing });
  const { progress, ...rest } = inputs[inputs.length - 1];
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
  const ids = (frame: number) => childrenOf(childrenOf(lastOf(at(composition, frame).scene.layers))[0]).map((layer) => layer.id);
  const [a, b] = at(composition, MIDDLE).scene.layers.map((sequence) => childrenOf(sequence)[0]);
  assert.equal(contentOf(contentOf(b, 'group').mask?.layers[0], 'rect').width, 40);
  assert.deepEqual(a.effects?.filters, [{ type: 'shader', id: tint.id, params: [0.4] }]);
  // The scene's own layers keep their ids as the mask comes and goes.
  assert.deepEqual(ids(MIDDLE), ids(12));
  assert.equal('mask' in contentOf(childrenOf(at(composition, 12).scene.layers[0])[0], 'group'), false);
});

test('a defined transition names itself to the scenes and must return styles', () => {
  const seen: TransitionSeriesScene[] = [];
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
  assert.deepEqual(seen[seen.length - 1].exit, { type: 'spin', durationInFrames: 4 });
  const cases: [() => unknown, RegExp][] = [
    [() => null, /broken's present must return an object/],
    [() => ({ enter: 3 }), /broken's enter style must be an object/],
    [() => ({ exit: { children: 'x' } }), /broken's exit style cannot set children/],
  ];
  for (const [present, message] of cases) {
    // Wrong on purpose: the checks are for untyped JavaScript transitions.
    const broken = defineTransition({ name: 'broken', present: present as TransitionDefinition['present'] });
    assert.throws(() => groups(MIDDLE, { type: broken }), message);
  }
});

test('the built-in functions give the same transitions as the names', () => {
  const moving: [TransitionSeriesType, TransitionDefinition][] = [['crossfade', crossfade()], ['slide', slide()], ['wipe', wipe()]];
  for (const [name, definition] of moving) {
    assert.equal(definition.name, name);
    assert.deepEqual(groups(MIDDLE, { type: definition, from: 'right' }), groups(MIDDLE, { type: name, from: 'right' }));
  }
  const others: [TransitionSeriesType, TransitionDefinition][] = [
    ['iris', iris()], ['clock-wipe', clockWipe()], ['zoom', zoom()], ['dissolve', dissolve()],
    ['blur', blur()], ['flip', flip()], ['cube', cube()],
  ];
  for (const [name, definition] of others) {
    assert.equal(definition.name, name);
    assert.deepEqual(groups(MIDDLE, { type: definition }), groups(MIDDLE, { type: name }));
  }
});

test('the series size comes from width and height, then the layout bounds, then the composition', () => {
  const offset = (options?: SeriesOptions) => groups(MIDDLE, { type: 'slide' }, options)[1].transform.position.x;
  // Two of five steps in from the left: the entering scene is 3/5 of the width away.
  assert.equal(offset(), -60);
  assert.equal(offset({ props: { width: 20, height: 10 } }), -12);
  assert.equal(offset({ wrap: (child) => h(SafeArea, { padding: { left: 10, right: 40 } }, child) }), -30);
  assert.equal(offset({ wrap: (child) => h(Fit, { sourceWidth: 400, sourceHeight: 200 }, child) }), -240);
  // A wipe in a 40 px frame crosses the frame, not the composition.
  const { clip } = contentOf(groups(MIDDLE, { type: 'wipe' }, { props: { width: 40, height: 20 } })[1], 'group');
  assert.deepEqual(clip, { x: 0, y: 0, width: 16, height: 20, cornerRadius: 0 });
  assert.throws(() => groups(MIDDLE, { type: 'slide' }, { props: { width: 0 } }), /width must be a positive finite number/);
  assert.throws(() => groups(MIDDLE, { type: 'slide' }, { props: { height: Number.NaN } }), /height must be a positive finite number/);
});

test('in a 560 px frame, a wipe crosses the frame over every frame and a slide moves by its width', () => {
  const frames = 20;
  const composition = mount(() => h(Composition, { width: 1080, height: 1920, fps: FPS, durationInFrames: 40 },
    h(Group, { x: 260, y: 400 },
      h(TransitionSeries, { width: 560, height: 315 },
        h(TransitionSeries.Sequence, { durationInFrames: 30 }, h(Scene, { name: 'a' })),
        h(TransitionSeries.Transition, { type: 'wipe', durationInFrames: frames }),
        h(TransitionSeries.Sequence, { durationInFrames: 30 }, h(Scene, { name: 'b' })),
      ),
    ),
  ));
  let previous = 0;
  for (let frame = 0; frame < frames; frame++) {
    const [, b] = childrenOf(at(composition, 10 + frame).scene.layers[0]);
    const { clip } = contentOf(childrenOf(b)[0], 'group');
    assert.ok(clip);
    const edge = clip.width;
    // Equal steps from the frame's left edge to its right one, never past it.
    near(edge, (560 * (frame + 1)) / (frames + 1), `edge on frame ${frame}`);
    assert.ok(edge > previous && edge < 560);
    previous = edge;
  }
  const [, entering] = groups(MIDDLE, { type: 'slide' }, { props: { width: 560, height: 315 } });
  assert.equal(entering.transform.position.x, -560 * 0.6);
});

test('without width and height, a series keeps drawing as it did', () => {
  const plain = groups(MIDDLE, { type: 'wipe', from: 'right' });
  const sized = groups(MIDDLE, { type: 'wipe', from: 'right' }, { props: { width: 100, height: 50 } });
  assert.deepEqual(plain, sized);
});

test('an iris masks the next scene with a disc that opens to the farthest corner, or closes', () => {
  const disc = (transition: TransitionSeriesTransitionProps['type']) => {
    const [, b] = groups(MIDDLE, { type: transition });
    const { mask } = contentOf(b, 'group');
    assert.ok(mask);
    const rect = mask.layers[0];
    return { mask, rect, box: contentOf(rect, 'rect') };
  };
  const reach = Math.hypot(50, 25);
  const open = disc('iris');
  assert.equal('invert' in open.mask, false);
  near(open.box.width, 2 * 0.4 * reach, 'open diameter');
  near(open.box.cornerRadius, 0.4 * reach, 'open radius');
  near(open.rect.transform.position.x, 50 - 0.4 * reach, 'open left');
  const closed = disc(iris({ direction: 'close', center: { x: 0, y: 0 } }));
  assert.equal(closed.mask.invert, true);
  near(closed.box.cornerRadius, 0.6 * Math.hypot(100, 50), 'closing radius');
  near(closed.rect.transform.position.x, -0.6 * Math.hypot(100, 50), 'closing left');
  const soft = disc(iris({ feather: 10 }));
  const radius = 0.4 * (reach + 10);
  assert.equal(soft.box.fill?.type, 'radial');
  near((soft.box.fill as Extract<Paint, { type: 'radial' }>).stops[1].offset, 1 - 10 / radius, 'feather stop');
  assert.throws(() => iris({ feather: -1 }), /iris feather must be a finite number of at least 0/);
  // Wrong on purpose: the checks are for untyped JavaScript callers.
  assert.throws(() => iris({ direction: 'sideways' as IrisOptions['direction'] }), /iris direction must be one of "open", "close"/);
  assert.throws(() => iris({ center: { x: 0.5 } as TransitionCenter }), /iris center.y must be a finite number/);
});

test('a clock wipe masks the next scene with a sector swept from 12 o\'clock', () => {
  const sector = (transition: TransitionSeriesTransitionProps['type']) =>
    contentOf(contentOf(groups(MIDDLE, { type: transition })[1], 'group').mask?.layers[0], 'path').commands;
  // 2/5 of a turn is 144°: five chords of 28.8°.
  const commands = sector('clock-wipe');
  assert.deepEqual(commands[0], { type: 'moveTo', x: 50, y: 25 });
  assert.equal(commands.length, 1 + 6 + 1);
  const reach = Math.hypot(50, 25) / Math.cos(Math.PI / 12) + 1;
  near(endOf(commands[1]).x, 50, 'start x');
  near(endOf(commands[1]).y, 25 - reach, 'start y');
  near(endOf(commands[6]).x, 50 + reach * Math.sin((144 * Math.PI) / 180), 'end x');
  const back = sector(clockWipe({ direction: 'counterclockwise', startAngle: 90 }));
  near(endOf(back[1]).x, 50 + reach, 'start at 3 o\'clock');
  near(endOf(back[6]).x, 50 + reach * Math.sin(((90 - 144) * Math.PI) / 180), 'swept back');
  // The first frame's 72° takes three chords, none over 30°.
  assert.equal(contentOf(contentOf(groups(6, { type: 'clock-wipe' })[1], 'group').mask?.layers[0], 'path').commands.length, 1 + 4 + 1);
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
  const area = (transition: TransitionSeriesTransitionProps['type'], frame = MIDDLE) => {
    const { mask } = contentOf(groups(frame, { type: transition })[1], 'group');
    assert.ok(mask);
    const [squares] = mask.layers;
    const commands = squares ? contentOf(squares, 'path').commands : [];
    let total = 0;
    for (let index = 0; index < commands.length; index += 5) {
      const [start, right, down] = commands.slice(index, index + 3).map(endOf);
      total += (right.x - start.x) * (down.y - right.y);
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
  near(a.effects?.blur, 8, 'exit blur');
  near(b.effects?.blur, 12, 'enter blur');
  near(b.opacity, 0.4, 'enter opacity');
  assert.throws(() => blur({ amount: 65 }), /blur amount must be a finite number between 0 and 64/);
});

test('a flip narrows the previous scene to its edge, then widens the next from it', () => {
  const first = groups(6, { type: 'flip' });
  const angle = Math.PI / 5;
  near(first[0].transform.scale.x, Math.cos(angle), 'narrowing');
  near(first[0].transform.position.x, 50 * (1 - Math.cos(angle)), 'about the centre');
  const [shade] = first[0].effects?.filters ?? [];
  assert.equal(shade?.type, 'brightness');
  near((shade as Extract<LayerFilter, { type: 'brightness' }>).amount, 1 - 0.4 * Math.sin(angle), 'shaded');
  assert.equal(first[1].opacity, 0);
  const last = groups(9, { type: flip({ shade: 0 }), from: 'top' });
  assert.equal(last[0].opacity, 0);
  near(last[1].transform.scale.y, Math.abs(Math.cos(Math.PI * 0.8)), 'widening');
  assert.equal(last[1].transform.scale.x, 1);
  assert.equal(last[1].effects, undefined);
});

test('a cube turns two faces that meet at an edge and together span the series', () => {
  for (const from of ['left', 'right'] as const) {
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

test('a flip with a perspective turns the scenes in 3D about the centre', () => {
  const [a, b] = groups(6, { type: flip({ perspective: 300 }) });
  assert.deepEqual(a.transform.origin, { x: 50, y: 25 });
  near(a.transform.rotationY, 36, 'the left edge comes forward');
  assert.equal(a.transform.perspective, 300);
  assert.deepEqual(a.transform.scale, { x: 1, y: 1 });
  assert.deepEqual(a.transform.position, { x: 0, y: 0 });
  assert.equal(b.opacity, 0);
  const [, last] = groups(9, { type: flip({ perspective: 300 }), from: 'top' });
  // 144° of the half turn: the next scene, 36° short of facing the viewer.
  near(last.transform.rotationX, 36, 'the next scene coming round');
  assert.equal(last.transform.rotationY, 0);
  const [right] = groups(6, { type: flip({ perspective: 300 }), from: 'right' });
  near(right.transform.rotationY, -36, 'the right edge comes forward');
  assert.throws(() => flip({ perspective: 0 }), /flip perspective must be a finite number/);
});

test('a cube with a perspective turns both faces in 3D about the edge they share', () => {
  const perspective = 400;
  const degrees = 36;
  const turn = (degrees * Math.PI) / 180;
  // Drawn widths: a face's far edge recedes by its width times the sine.
  const drawn = (angle: number) => (100 * Math.cos(angle) * perspective) / (perspective + 100 * Math.sin(angle));
  const leaving = drawn(turn);
  const entering = drawn(Math.PI / 2 - turn);
  for (const from of ['left', 'right'] as const) {
    const [a, b] = groups(MIDDLE, { type: cube({ perspective }), from });
    const edge = from === 'left' ? (100 * entering) / (leaving + entering) : 100 - (100 * entering) / (leaving + entering);
    // Each face's own edge on the shared one is its origin, put on `edge`.
    near(a.transform.position.x + a.transform.origin!.x, edge, `${from} exit on the edge`);
    near(b.transform.position.x + b.transform.origin!.x, edge, `${from} enter on the edge`);
    assert.equal(a.transform.origin?.y, 25);
    assert.equal(a.transform.perspective, perspective);
    const sign = from === 'left' ? 1 : -1;
    near(a.transform.rotationY, sign * degrees, `${from} exit turns away`);
    near(b.transform.rotationY, -sign * (90 - degrees), `${from} enter turns away`);
    // The exit face lies past the edge from `from`, the entering one before it.
    assert.equal(a.transform.origin?.x, from === 'left' ? 0 : 100);
    assert.equal(b.transform.origin?.x, from === 'left' ? 100 : 0);
    // What the pair leaves uncovered either side is in proportion to the faces.
    const [before, after] = from === 'left' ? [entering, leaving] : [leaving, entering];
    near((edge - before) * after, (100 - edge - after) * before, `${from} gaps`);
  }
  const [a, b] = groups(MIDDLE, { type: cube({ perspective }), from: 'top' });
  assert.deepEqual(a.transform.origin, { x: 50, y: 0 });
  near(a.transform.rotationX, -degrees, 'the leaving face\'s bottom turns away');
  near(b.transform.rotationX, 90 - degrees, 'the entering face\'s top turns away');
  assert.throws(() => cube({ perspective: -1 }), /cube perspective must be a finite number/);
});

test('useTransitionVolume cross-fades the sound of every type the same way', () => {
  const types: TransitionSeriesType[] = ['crossfade', 'slide', 'wipe', 'iris', 'clock-wipe', 'zoom', 'dissolve', 'blur', 'flip', 'cube'];
  const tracks = types.map((type) => {
    function Voiced({ name }: { name: string }) {
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
  const us = (seconds: number) => ({ value: Math.round(seconds * 1e6), timescale: 1e6 });
  assert.deepEqual(tracks[0].map((clip) => clip.volume), [
    { type: 'keyframes', keyframes: [{ time: us(0.6), value: 0.5 }, { time: us(1), value: 0 }] },
    { type: 'keyframes', keyframes: [{ time: us(0), value: 0 }, { time: us(0.4), value: 0.5 }] },
  ]);
  for (const [index, track] of tracks.entries()) {
    assert.deepEqual(track, tracks[0], types[index]);
  }
});

test('the masks of the built-in transitions keep the scene\'s layer ids', () => {
  for (const type of ['iris', 'clock-wipe', 'dissolve'] as const) {
    const composition = series({ type });
    const ids = (frame: number) => childrenOf(childrenOf(lastOf(at(composition, frame).scene.layers))[0]).map((layer) => layer.id);
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
  const inner = (frame: number) => childrenOf(childrenOf(lastOf(at(composition, frame).scene.layers))[0])[0].id;
  assert.equal(inner(MIDDLE), inner(12));
});
