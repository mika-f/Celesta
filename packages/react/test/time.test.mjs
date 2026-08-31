import assert from 'node:assert/strict';
import test from 'node:test';

import { timecodeToFrame } from '../dist/index.js';

test('timecodeToFrame converts timecodes to the nearest frame', () => {
  assert.equal(timecodeToFrame('00:00:30.123', 30), 904);
  assert.equal(timecodeToFrame('01:30.5', 24), 2172);
  assert.equal(timecodeToFrame('5', 60), 300);
});

test('timecodeToFrame rejects invalid input', () => {
  assert.throws(() => timecodeToFrame('00:00:30.1234', 30), /Invalid timecode/);
  assert.throws(() => timecodeToFrame('00:00:30', 0), /positive fps/);
});
