// Property evaluation, transforms, shape geometry, trim ranges, and timing,
// tested on the source modules directly.

import assert from 'node:assert/strict';
import { test } from 'vitest';

import type { PathCommand } from '@celesta.studio/react';
import { inspectLottie, lottieFrameAt } from '../src/index.js';
import type { LottieBezier, LottieKeyframe } from '../src/index.js';
import {
  bezierCommands,
  ellipseCommands,
  groupTransform,
  multiply,
  rectCommands,
  starCommands,
  transformMatrix,
} from '../src/geometry.js';
import type { Matrix } from '../src/geometry.js';
import { bezierAtFrame, cubicEase, positionAt, scalarAt, vectorAt } from '../src/property.js';
import { hexColor, trimRanges } from '../src/shapes.js';

const close = (actual: number, expected: number, epsilon = 1e-6) =>
  assert.ok(Math.abs(actual - expected) < epsilon, `${actual} is not ${expected}`);

/** `command`, checked to end at a point. */
function endOf(command: PathCommand | undefined): Exclude<PathCommand, { type: 'close' }> {
  assert.ok(command && command.type !== 'close', 'a command with an end point');
  return command;
}

const ease: Pick<LottieKeyframe<number[]>, 'i' | 'o'> = { o: { x: [0.33], y: [0] }, i: { x: [0.67], y: [1] } };

test('a static value comes back as an array, scalars unwrapped', () => {
  assert.deepEqual(vectorAt({ a: 0, k: [1, 2] }, 5), [1, 2]);
  assert.equal(scalarAt({ a: 0, k: 7 }, 5, 0), 7);
  assert.equal(scalarAt(undefined, 5, 100), 100);
});

test('keyframes hold before the first and after the last, and ease between', () => {
  const prop = { a: 1, k: [{ t: 10, s: [0], ...ease }, { t: 20, s: [100] }] };
  assert.equal(scalarAt(prop, 0, -1), 0);
  assert.equal(scalarAt(prop, 30, -1), 100);
  close(scalarAt(prop, 15, -1), 50, 1e-6);
  assert.ok(scalarAt(prop, 12, -1) < 20, 'eased in slowly');
});

test('linear keyframes without handles interpolate linearly, hold keyframes jump', () => {
  assert.equal(scalarAt({ a: 1, k: [{ t: 0, s: [0] }, { t: 10, s: [10] }] }, 3, 0), 3);
  assert.equal(scalarAt({ a: 1, k: [{ t: 0, s: [0], h: 1 }, { t: 10, s: [10] }] }, 9.9, 0), 0);
  assert.equal(scalarAt({ a: 1, k: [{ t: 0, s: [0], h: 1 }, { t: 10, s: [10] }] }, 10, 0), 10);
});

test('old-style keyframes take their end value from `e`', () => {
  const prop = { a: 1, k: [{ t: 0, s: [0], e: [20] }, { t: 10 }] };
  assert.equal(scalarAt(prop, 5, 0), 10);
  assert.equal(scalarAt(prop, 10, 0), 20);
});

test('easing is per dimension when the handles are arrays', () => {
  const prop = {
    a: 1,
    k: [{ t: 0, s: [0, 0], o: { x: [0, 1], y: [0, 0] }, i: { x: [1, 0], y: [1, 1] } }, { t: 10, s: [10, 10] }],
  };
  const [x, y] = vectorAt(prop, 2, [0, 0]);
  assert.notEqual(x.toFixed(4), y.toFixed(4));
});

test('cubicEase matches CSS ease-in-out at its fixed points', () => {
  close(cubicEase(0.42, 0, 0.58, 1, 0.5), 0.5, 1e-6);
  close(cubicEase(0, 0, 1, 1, 0.3), 0.3, 1e-9);
  assert.equal(cubicEase(0.42, 0, 0.58, 1, 0), 0);
  assert.equal(cubicEase(0.42, 0, 0.58, 1, 1), 1);
});

test('a position with tangents moves along its curve, at an even speed', () => {
  const prop = {
    a: 1,
    k: [{ t: 0, s: [0, 0], to: [0, -40], ti: [0, -40] }, { t: 10, s: [100, 0] }],
  };
  const [x, y] = positionAt(prop, 5);
  close(x, 50, 1e-3);
  close(y, -30, 1e-3);
  assert.deepEqual(positionAt({ s: true, x: { a: 0, k: 3 }, y: { a: 0, k: 4 } }, 0), [3, 4]);
});

test('paths morph vertex by vertex', () => {
  const square = (size: number): LottieBezier => ({ c: true, v: [[0, 0], [size, 0], [size, size]], i: [[0, 0], [0, 0], [0, 0]], o: [[0, 0], [0, 0], [0, 0]] });
  const prop = { a: 1, k: [{ t: 0, s: [square(10)] }, { t: 10, s: [square(30)] }] };
  assert.deepEqual(bezierAtFrame(prop, 5).v, [[0, 0], [20, 0], [20, 20]]);
});

test('a transform is position · rotation · scale · −anchor, and decomposes back', () => {
  const m = transformMatrix({
    a: { a: 0, k: [10, 20] },
    p: { a: 0, k: [100, 50] },
    s: { a: 0, k: [200, 50] },
    r: { a: 0, k: 90 },
  }, 0);
  // The anchor lands on the position.
  close(m[0] * 10 + m[2] * 20 + m[4], 100);
  close(m[1] * 10 + m[3] * 20 + m[5], 50);
  // +x turns clockwise to +y (y points down).
  close(m[1], 2);
  const t = groupTransform(m);
  assert.ok(t);
  close(t.rotation, 90);
  close(t.scaleX, 2);
  close(t.scaleY, 0.5);
  close(t.skewX, 0);
});

test('a skewed and mirrored transform decomposes into props that rebuild it', () => {
  const m = transformMatrix({
    p: { a: 0, k: [5, 6] },
    s: { a: 0, k: [-80, 120] },
    r: { a: 0, k: 30 },
    sk: { a: 0, k: 20 },
    sa: { a: 0, k: 45 },
  }, 0);
  const t = groupTransform(m);
  assert.ok(t);
  const rad = Math.PI / 180;
  const rotation: Matrix = [Math.cos(t.rotation * rad), Math.sin(t.rotation * rad), -Math.sin(t.rotation * rad), Math.cos(t.rotation * rad), 0, 0];
  const skew: Matrix = [1, 0, Math.tan(t.skewX * rad), 1, 0, 0];
  const scale = [t.scaleX, 0, 0, t.scaleY, t.x, t.y];
  const rebuilt = multiply(multiply(rotation, skew), [t.scaleX, 0, 0, t.scaleY, 0, 0]);
  for (let n = 0; n < 4; n += 1) {
    close(rebuilt[n], m[n], 1e-9);
  }
  assert.deepEqual([scale[4], scale[5]], [m[4], m[5]]);
  assert.equal(groupTransform([0, 0, 0, 1, 0, 0]), null);
});

test('a rect starts at its top-right corner and runs clockwise, or back with d: 3', () => {
  const rect = { ty: 'rc', p: { a: 0, k: [0, 0] }, s: { a: 0, k: [20, 10] }, r: { a: 0, k: 0 } };
  assert.deepEqual(rectCommands(rect, 0), [
    { type: 'moveTo', x: 10, y: -5 },
    { type: 'lineTo', x: 10, y: 5 },
    { type: 'lineTo', x: -10, y: 5 },
    { type: 'lineTo', x: -10, y: -5 },
    { type: 'close' },
  ]);
  assert.deepEqual(rectCommands({ ...rect, d: 3 }, 0)[1], { type: 'lineTo', x: -10, y: -5 });
  const rounded = rectCommands({ ...rect, r: { a: 0, k: 100 } }, 0);
  assert.deepEqual(rounded[0], { type: 'moveTo', x: 10, y: 0 });
});

test('an ellipse starts at its top and turns clockwise', () => {
  const commands = ellipseCommands({ ty: 'el', p: { a: 0, k: [0, 0] }, s: { a: 0, k: [20, 20] } }, 0);
  assert.deepEqual(commands[0], { type: 'moveTo', x: 0, y: -10 });
  assert.equal(endOf(commands[1]).x, 10);
  assert.equal(endOf(commands[1]).y, 0);
  assert.equal(commands.at(-1)?.type, 'close');
});

test('a star alternates its radii from straight up; a polygon keeps the outer one', () => {
  const star = starCommands({
    ty: 'sr', sy: 1, p: { a: 0, k: [0, 0] }, pt: { a: 0, k: 5 }, r: { a: 0, k: 0 },
    or: { a: 0, k: 10 }, ir: { a: 0, k: 4 }, os: { a: 0, k: 0 }, is: { a: 0, k: 0 },
  }, 0);
  assert.equal(star.length, 11);
  close(endOf(star[0]).x, 0);
  close(endOf(star[0]).y, -10);
  close(Math.hypot(endOf(star[1]).x, endOf(star[1]).y), 4);
  const polygon = starCommands({ ty: 'sr', sy: 2, p: { a: 0, k: [0, 0] }, pt: { a: 0, k: 3 }, or: { a: 0, k: 10 } }, 0);
  assert.equal(polygon.length, 4);
});

test('a Bézier path draws cubics where it has handles and lines where it has none', () => {
  const commands = bezierCommands({
    c: true,
    v: [[0, 0], [10, 0], [10, 10]],
    i: [[0, 0], [0, 0], [0, -5]],
    o: [[0, 0], [0, 5], [0, 0]],
  });
  assert.deepEqual(commands.map(command => command.type), ['moveTo', 'lineTo', 'cubicTo', 'close']);
});

test('trim ranges turn by the offset and wrap past the end', () => {
  assert.deepEqual(trimRanges(0, 100, 0), [[0, 1]]);
  assert.deepEqual(trimRanges(50, 50, 0), []);
  assert.deepEqual(trimRanges(60, 20, 0), [[0.2, 0.6]]);
  const [first, second] = trimRanges(0, 50, 270);
  close(first[0], 0.75);
  close(first[1], 1);
  close(second[0], 0);
  close(second[1], 0.25);
});

test('colors are 0–1, or 0–255 from old exporters', () => {
  assert.equal(hexColor([1, 0.5, 0, 1]), '#FF8000FF');
  assert.equal(hexColor([255, 0, 0]), '#FF0000FF');
});

test('playback loops by default, or holds the last frame', () => {
  const timing = { fps: 30, startFrame: 0, endFrame: 60 };
  assert.equal(lottieFrameAt(0.5, timing), 15);
  assert.equal(lottieFrameAt(2.5, timing), 15);
  assert.equal(lottieFrameAt(2.5, { ...timing, loop: false }), 59);
  assert.equal(lottieFrameAt(1, { ...timing, playbackRate: 2, loop: false }), 59);
  assert.equal(lottieFrameAt(0.25, { ...timing, playbackRate: -1 }), 52.5);
  assert.equal(lottieFrameAt(0.5, { ...timing, startFrame: 30 }), 45);
});

test('inspectLottie names each unsupported feature once, with the first layer using it', () => {
  const warnings = inspectLottie({
    fr: 30, ip: 0, op: 10, w: 10, h: 10,
    layers: [
      { ty: 5, nm: 'Title', ip: 0, op: 10 },
      { ty: 4, nm: 'Shapes', ip: 0, op: 10, ks: { o: { a: 0, k: 100, x: 'value * 2' } },
        shapes: [{ ty: 'rp' }, { ty: 'gr', it: [{ ty: 'rd' }, { ty: 'fl', r: 2 }] }] },
      { ty: 4, nm: 'More', ip: 0, op: 10, shapes: [{ ty: 'rp' }] },
    ],
  });
  assert.deepEqual(warnings, [
    '<Lottie> does not support text layers (layer "Title")',
    '<Lottie> does not support expressions (layer "Shapes")',
    '<Lottie> does not support repeaters (layer "Shapes")',
    '<Lottie> does not support round corners (layer "Shapes")',
    '<Lottie> does not support the even-odd fill rule (layer "Shapes")',
  ]);
  assert.deepEqual(inspectLottie({ fr: 30, ip: 0, op: 10, w: 10, h: 10, layers: [{ ty: 4, ip: 0, op: 10, shapes: [] }] }), []);
});
