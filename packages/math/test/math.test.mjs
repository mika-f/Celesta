import assert from 'node:assert/strict';
import test from 'node:test';

import {
  TAU,
  angleBetween,
  angleDifference,
  approxEqual,
  cartesianToPolar,
  clamp,
  clamp01,
  cubicBezierPoint,
  degToRad,
  distance,
  fbm,
  fbm2D,
  fbm3D,
  fract,
  inverseLerp,
  lerp,
  lerpAngle,
  lerpPoint,
  midpoint,
  mod,
  noise,
  noise2D,
  noise3D,
  normalizeAngle,
  pingPong,
  polarToCartesian,
  quadraticBezierPoint,
  radToDeg,
  random,
  randomBool,
  randomGaussian,
  randomInCircle,
  randomInt,
  randomPick,
  randomRange,
  randomSign,
  remap,
  remapClamped,
  rotatePoint,
  roundTo,
  sawtoothWave,
  shuffle,
  sineWave,
  smootherstep,
  smoothstep,
  snap,
  squareWave,
  step,
  triangleWave,
  wrap,
} from '../dist/index.js';

const close = (actual, expected, message) => assert.ok(Math.abs(actual - expected) < 1e-9, message ?? `${actual} ≉ ${expected}`);
const closePoint = (actual, expected) => {
  close(actual.x, expected.x, `x: ${actual.x} ≉ ${expected.x}`);
  close(actual.y, expected.y, `y: ${actual.y} ≉ ${expected.y}`);
};

test('random and noise are deterministic and stay in range', () => {
  assert.equal(random(7), random(7));
  assert.equal(random('star-3-x'), random('star-3-x'));
  assert.notEqual(random(1), random(2));
  assert.notEqual(random(0.5), random(0));
  for (let i = 0; i < 500; i += 1) {
    const r = random(i);
    assert.ok(r >= 0 && r < 1, `random(${i}) = ${r}`);
    const n = noise('seed', i / 7);
    assert.ok(n >= -1 && n <= 1, `noise(${i / 7}) = ${n}`);
  }
  // Smooth: tiny steps in t make tiny steps in the value.
  assert.ok(Math.abs(noise(3, 2.5) - noise(3, 2.501)) < 0.01);
  assert.equal(noise(3, 4.25), noise(3, 4.25));
  assert.throws(() => random(Number.NaN), /random\(\) requires a finite number or a string seed/);
  assert.throws(() => noise(1, Number.POSITIVE_INFINITY), /noise\(\) requires a finite t/);
});

test('random and noise keep the values they had in @celesta/react', () => {
  // Existing compositions must render the same after moving packages.
  assert.equal(random(7), 0.3443175407592207);
  assert.equal(random('star-3-x'), 0.11351002054288983);
  assert.equal(noise(3, 2.5), -0.1304047736339271);
  assert.equal(noise('seed', -1.75), 0.4223650675121462);
});

test('random helpers cover their ranges and stay deterministic', () => {
  const ints = new Set();
  let heads = 0;
  for (let i = 0; i < 2000; i += 1) {
    const r = randomRange(i, -5, 5);
    assert.ok(r >= -5 && r < 5);
    const n = randomInt(i, 1, 6);
    assert.ok(Number.isInteger(n) && n >= 1 && n <= 6);
    ints.add(n);
    if (randomBool(i)) heads += 1;
    assert.ok([-1, 1].includes(randomSign(i)));
    const p = randomInCircle(i, 10, { x: 100, y: 50 });
    assert.ok(distance(p, { x: 100, y: 50 }) <= 10);
  }
  assert.deepEqual([...ints].sort(), [1, 2, 3, 4, 5, 6]);
  assert.ok(heads > 900 && heads < 1100, `${heads} heads`);
  assert.equal(randomBool('x', 0), false);
  assert.equal(randomBool('x', 1), true);
  assert.equal(randomInt('x', 3, 3), 3);
  assert.throws(() => randomInt('x', 1.2, 1.8), /no whole number/);

  assert.equal(randomPick('a', ['only']), 'only');
  assert.equal(randomPick(9, ['a', 'b', 'c']), randomPick(9, ['a', 'b', 'c']));
  assert.throws(() => randomPick(1, []), /at least one item/);
});

test('shuffle returns a deterministic permutation without touching its input', () => {
  const items = Object.freeze([1, 2, 3, 4, 5, 6, 7, 8]);
  const shuffled = shuffle('deck', items);
  assert.deepEqual(shuffle('deck', items), shuffled);
  assert.deepEqual([...shuffled].sort((a, b) => a - b), [...items]);
  assert.notDeepEqual(shuffle('other deck', items), shuffled);
  assert.deepEqual(shuffle(1, []), []);
});

test('randomGaussian centres on its mean', () => {
  let sum = 0;
  let squares = 0;
  const count = 4000;
  for (let i = 0; i < count; i += 1) {
    const value = randomGaussian(i, 10, 2);
    assert.ok(Number.isFinite(value));
    sum += value;
    squares += (value - 10) ** 2;
  }
  assert.ok(Math.abs(sum / count - 10) < 0.15, `mean ${sum / count}`);
  assert.ok(Math.abs(Math.sqrt(squares / count) - 2) < 0.15, `stdDev ${Math.sqrt(squares / count)}`);
});

test('2D and 3D noise are smooth, seeded, and stay in range', () => {
  let min = Infinity;
  let max = -Infinity;
  for (let i = 0; i < 4000; i += 1) {
    const x = (i % 63) / 6.1 - 5;
    const y = Math.floor(i / 63) / 5.3 - 6;
    for (const value of [noise2D('n', x, y), noise3D('n', x, y, x * 0.7 - y), fbm2D('n', x, y), fbm3D('n', x, y, 1.5)]) {
      assert.ok(value >= -1 && value <= 1, `${value}`);
      min = Math.min(min, value);
      max = Math.max(max, value);
    }
  }
  // It actually uses most of the range rather than hugging zero.
  assert.ok(min < -0.5 && max > 0.5, `range ${min}..${max}`);
  // Gradient noise is zero on lattice points and continuous between them.
  assert.equal(noise2D(4, 3, -2), 0);
  assert.equal(noise3D(4, 3, -2, 7), 0);
  assert.ok(Math.abs(noise2D(4, 1.3, 2.7) - noise2D(4, 1.3001, 2.7)) < 0.01);
  assert.ok(Math.abs(noise3D(4, 1.3, 2.7, 0.2) - noise3D(4, 1.3, 2.7, 0.2001)) < 0.01);
  assert.notEqual(noise2D('a', 0.5, 0.5), noise2D('b', 0.5, 0.5));
  assert.throws(() => noise2D(1, Number.NaN, 0), /noise2D\(\) requires a finite x/);
});

test('fbm layers octaves and keeps the first octave', () => {
  assert.equal(fbm('s', 1.7, { octaves: 1 }), noise('s', 1.7));
  assert.notEqual(fbm('s', 1.7), noise('s', 1.7));
  for (let i = 0; i < 300; i += 1) {
    const value = fbm(i, i / 9, { octaves: 6 });
    assert.ok(value >= -1 && value <= 1);
  }
  assert.throws(() => fbm('s', 1, { octaves: 0 }), /whole number of octaves/);
});

test('scalar helpers', () => {
  assert.equal(clamp(5, 0, 3), 3);
  assert.equal(clamp(-1, 0, 3), 0);
  assert.equal(clamp01(0.4), 0.4);
  assert.equal(lerp(10, 20, 0.25), 12.5);
  assert.equal(inverseLerp(10, 20, 12.5), 0.25);
  assert.equal(inverseLerp(5, 5, 5), 0);
  assert.equal(remap(5, 0, 10, 100, 200), 150);
  assert.equal(remap(20, 0, 10, 100, 200), 300);
  assert.equal(remapClamped(20, 0, 10, 100, 200), 200);
  assert.equal(step(0.5, 0.49), 0);
  assert.equal(step(0.5, 0.5), 1);
  assert.equal(smoothstep(0, 10, -1), 0);
  assert.equal(smoothstep(0, 10, 5), 0.5);
  assert.equal(smoothstep(0, 10, 11), 1);
  assert.equal(smootherstep(0, 10, 5), 0.5);
  close(fract(-0.25), 0.75);
  assert.equal(mod(-1, 4), 3);
  assert.equal(wrap(370, 0, 360), 10);
  assert.equal(wrap(-10, 0, 360), 350);
  assert.equal(pingPong(3, 4), 3);
  assert.equal(pingPong(5, 4), 3);
  assert.equal(pingPong(9, 4), 1);
  assert.equal(snap(17, 5), 15);
  assert.equal(snap(17, 0), 17);
  assert.equal(roundTo(3.14159, 2), 3.14);
  assert.ok(approxEqual(0.1 + 0.2, 0.3));
  assert.ok(!approxEqual(1, 1.1));
});

test('angle helpers work in radians and turn the short way', () => {
  assert.equal(TAU, Math.PI * 2);
  close(degToRad(180), Math.PI);
  close(radToDeg(Math.PI / 2), 90);
  close(normalizeAngle(3 * Math.PI), -Math.PI);
  close(normalizeAngle(-Math.PI / 2 - TAU), -Math.PI / 2);
  close(angleDifference(degToRad(350), degToRad(10)), degToRad(20));
  close(lerpAngle(degToRad(350), degToRad(10), 0.5), degToRad(360));
});

test('waves share a period of 1 and start at 0 rising', () => {
  for (const wave of [sineWave, triangleWave, sawtoothWave]) {
    close(wave(0), 0);
    assert.ok(wave(0.1) > 0);
    close(wave(1.37), wave(0.37));
  }
  close(sineWave(0.25), 1);
  close(triangleWave(0.25), 1);
  close(triangleWave(0.75), -1);
  close(triangleWave(0.125), 0.5);
  assert.equal(squareWave(0.1), 1);
  assert.equal(squareWave(0.6), -1);
  close(sawtoothWave(0.25), 0.5);
  close(sawtoothWave(0.5), -1);
});

test('point helpers', () => {
  assert.equal(distance({ x: 0, y: 0 }, { x: 3, y: 4 }), 5);
  close(angleBetween({ x: 0, y: 0 }, { x: 0, y: 5 }), Math.PI / 2);
  closePoint(lerpPoint({ x: 0, y: 0 }, { x: 10, y: 20 }, 0.25), { x: 2.5, y: 5 });
  closePoint(midpoint({ x: 0, y: 0 }, { x: 10, y: 20 }), { x: 5, y: 10 });
  closePoint(rotatePoint({ x: 1, y: 0 }, Math.PI / 2), { x: 0, y: 1 });
  closePoint(rotatePoint({ x: 2, y: 1 }, Math.PI, { x: 1, y: 1 }), { x: 0, y: 1 });
  closePoint(polarToCartesian(Math.PI, 2, { x: 5, y: 5 }), { x: 3, y: 5 });
  const polar = cartesianToPolar({ x: 5, y: 7 }, { x: 5, y: 5 });
  close(polar.angle, Math.PI / 2);
  close(polar.radius, 2);
  const [p0, p1, p2, p3] = [{ x: 0, y: 0 }, { x: 0, y: 10 }, { x: 10, y: 10 }, { x: 10, y: 0 }];
  closePoint(quadraticBezierPoint(p0, p1, p2, 0), p0);
  closePoint(quadraticBezierPoint(p0, p1, p2, 1), p2);
  closePoint(quadraticBezierPoint(p0, p1, p2, 0.5), { x: 2.5, y: 7.5 });
  closePoint(cubicBezierPoint(p0, p1, p2, p3, 0.5), { x: 5, y: 7.5 });
  closePoint(cubicBezierPoint(p0, p1, p2, p3, 1), p3);
});
