import assert from 'node:assert/strict';
import { test } from 'vitest';

import { isHexColor, parseHexColor } from '../src/index.js';

test('parseHexColor reads six or eight hex digits in either case', () => {
  assert.deepEqual(parseHexColor('#1a2B3c'), [0x1a, 0x2b, 0x3c, 255]);
  assert.deepEqual(parseHexColor('#1A2B3C80'), [0x1a, 0x2b, 0x3c, 0x80]);
  assert.equal(isHexColor('#1a2B3c'), true);
  assert.equal(isHexColor('#1A2B3C80'), true);
});

test('parseHexColor and isHexColor reject anything else', () => {
  for (const invalid of ['', '#', '1A2B3C', '#fff', '#1A2B3', '#1A2B3C8', '#1A2B3C800', '#xyzxyz', ' #1A2B3C', '#1A2B3C\n', 'red', '#aéaaa']) {
    assert.equal(parseHexColor(invalid), undefined, JSON.stringify(invalid));
    assert.equal(isHexColor(invalid), false, JSON.stringify(invalid));
  }
  for (const value of [undefined, null, 0xffffff, ['#FFFFFF']]) {
    assert.equal(isHexColor(value), false);
  }
});
