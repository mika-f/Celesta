import assert from 'node:assert/strict';
import { test } from 'vitest';

import { Easings, interpolateColor } from '../dist/index.js';

test('interpolateColor blends opaque colors channel by channel', () => {
  assert.equal(interpolateColor(15, [0, 30], ['#FF0000', '#0000FF']), '#800080FF');
  assert.equal(interpolateColor(10, [0, 40], ['#000000', '#FFFFFF']), '#404040FF');
});

test('interpolateColor returns each stop color exactly, normalized to uppercase #RRGGBBAA', () => {
  const colors = ['#ff000000', '#00ff00', '#0000FF80'];
  assert.equal(interpolateColor(0, [0, 10, 30], colors), '#FF000000');
  assert.equal(interpolateColor(10, [0, 10, 30], colors), '#00FF00FF');
  assert.equal(interpolateColor(30, [0, 10, 30], colors), '#0000FF80');
  // Exact at the ends of each segment even with an easing and 'extend'.
  const options = { easing: Easings.easeInOutCubic, extrapolateLeft: 'extend', extrapolateRight: 'extend' };
  assert.equal(interpolateColor(0, [0, 10, 30], colors, options), '#FF000000');
  assert.equal(interpolateColor(10, [0, 10, 30], colors, options), '#00FF00FF');
  assert.equal(interpolateColor(30, [0, 10, 30], colors, options), '#0000FF80');
});

test('interpolateColor picks the segment the input falls in', () => {
  // Segment 1 at a quarter: premultiplied (0, 255, 0, 255) → (0, 0, 128, 128).
  assert.equal(interpolateColor(15, [0, 10, 30], ['#FF0000', '#00FF00', '#0000FF80']), '#00DA25DF');
});

test('interpolateColor interpolates premultiplied alpha', () => {
  // A fade to transparent black keeps the color instead of darkening it.
  assert.equal(interpolateColor(15, [0, 30], ['#FF0000FF', '#00000000']), '#FF000080');
  // A fully transparent end's RGB has no effect on the blend.
  assert.equal(interpolateColor(15, [0, 30], ['#FF000000', '#0000FFFF']), '#0000FF80');
  assert.equal(interpolateColor(15, [0, 30], ['#00FF0000', '#0000FFFF']), '#0000FF80');
  // Both ends transparent stays transparent.
  assert.equal(interpolateColor(15, [0, 30], ['#FF000000', '#00FF0000']), '#00000000');
  // Partial alpha on both ends weights each color by its alpha.
  assert.equal(interpolateColor(5, [0, 10], ['#FF000040', '#0000FFC0']), '#4000BF80');
});

test('interpolateColor applies the easing inside each segment', () => {
  // easeInQuad at the midpoint is 0.25.
  assert.equal(interpolateColor(5, [0, 10], ['#000000', '#FFFFFF'], { easing: Easings.easeInQuad }), '#404040FF');
});

test('interpolateColor clamps outside the range by default', () => {
  assert.equal(interpolateColor(-10, [0, 30], ['#FF0000', '#0000FF']), '#FF0000FF');
  assert.equal(interpolateColor(100, [0, 30], ['#FF0000', '#0000FF']), '#0000FFFF');
  assert.equal(interpolateColor(-10, [0, 30], ['#FF0000', '#0000FF'], { extrapolateLeft: 'clamp' }), '#FF0000FF');
});

test("interpolateColor 'extend' continues the boundary segment and clamps channels to 0-255", () => {
  // 0x40 → 0x80 over 0..10 continues to 0xC0 at 20 and saturates at 0xFF.
  const range = [0, 10];
  const colors = ['#404040', '#808080'];
  assert.equal(interpolateColor(20, range, colors, { extrapolateRight: 'extend' }), '#C0C0C0FF');
  assert.equal(interpolateColor(100, range, colors, { extrapolateRight: 'extend' }), '#FFFFFFFF');
  assert.equal(interpolateColor(-10, range, colors, { extrapolateLeft: 'extend' }), '#000000FF');
  // Extending alpha below 0 is fully transparent; above 255 is opaque.
  assert.equal(interpolateColor(-10, [0, 10], ['#FF000080', '#FF0000FF'], { extrapolateLeft: 'extend' }), '#FF000001');
  assert.equal(interpolateColor(-20, [0, 10], ['#FF000080', '#FF0000FF'], { extrapolateLeft: 'extend' }), '#00000000');
  assert.equal(interpolateColor(30, [0, 10], ['#FF000080', '#FF0000FF'], { extrapolateRight: 'extend' }), '#FF0000FF');
  // Each side is independent.
  assert.equal(
    interpolateColor(-10, range, colors, { extrapolateLeft: 'clamp', extrapolateRight: 'extend' }),
    '#404040FF',
  );
});

test('interpolateColor clamps channels an overshooting easing pushes past 0-255', () => {
  const color = interpolateColor(9, [0, 10], ['#000000', '#FFFFFF'], { easing: Easings.easeOutBack });
  assert.equal(color, '#FFFFFFFF');
  const undershoot = interpolateColor(1, [0, 10], ['#000000', '#FFFFFF'], { easing: Easings.easeInBack });
  assert.equal(undershoot, '#000000FF');
});

test('interpolateColor validates its arguments', () => {
  assert.throws(() => interpolateColor(0, [0], ['#000000']), /same length, at least 2/);
  assert.throws(() => interpolateColor(0, [0, 10], ['#000000']), /same length, at least 2/);
  assert.throws(() => interpolateColor(0, [0, 10, 20], ['#000000', '#FFFFFF']), /same length, at least 2/);
  assert.throws(() => interpolateColor(0, [0, 0], ['#000000', '#FFFFFF']), /strictly increasing/);
  assert.throws(() => interpolateColor(0, [10, 0], ['#000000', '#FFFFFF']), /strictly increasing/);
  assert.throws(() => interpolateColor(0, [0, Number.NaN], ['#000000', '#FFFFFF']), /finite inputRange/);
  assert.throws(() => interpolateColor(0, [0, Infinity], ['#000000', '#FFFFFF']), /finite inputRange/);
  // Sparse-array holes are missing values, not skipped entries.
  // eslint-disable-next-line no-sparse-arrays
  assert.throws(() => interpolateColor(5, [0, , 10], ['#000000', '#808080', '#FFFFFF']), /finite inputRange/);
  // eslint-disable-next-line no-sparse-arrays
  assert.throws(() => interpolateColor(5, [0, 5, 10], ['#000000', , '#FFFFFF']), /got undefined at index 1/);
  assert.throws(() => interpolateColor(Number.NaN, [0, 10], ['#000000', '#FFFFFF']), /finite input/);
  assert.throws(() => interpolateColor(Infinity, [0, 10], ['#000000', '#FFFFFF']), /finite input/);
  for (const color of ['red', '#FFF', '#FFFFF', '#FFFFFFF', '#GGGGGG', 'FFFFFF', '#FFFFFFFFFF', 'rgb(0,0,0)', 0]) {
    assert.throws(
      () => interpolateColor(0, [0, 10], ['#000000', color]),
      /#RRGGBB or #RRGGBBAA colors, got .* at index 1/,
      String(color),
    );
  }
  assert.throws(
    () => interpolateColor(0, [0, 10], ['#000000', '#FFFFFF'], { extrapolateLeft: 'identity' }),
    /extrapolateLeft must be 'extend' or 'clamp'/,
  );
  assert.throws(
    () => interpolateColor(0, [0, 10], ['#000000', '#FFFFFF'], { extrapolateRight: 'hold' }),
    /extrapolateRight must be 'extend' or 'clamp'/,
  );
});
