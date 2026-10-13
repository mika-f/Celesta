import assert from 'node:assert/strict';
import test from 'node:test';
import { DURATION } from '../constants.ts';
import { DONE_AT, EXPORT_COMMAND, TYPED_AT, framesDone } from '../shared/export.ts';

test('the export counter fills to the full length and holds', () => {
  assert.equal(framesDone(0), 0);
  assert.equal(framesDone(TYPED_AT), 0);
  assert.equal(framesDone(DONE_AT), DURATION);
  assert.equal(framesDone(239), DURATION);
  for (let l = 1; l < 240; l += 1) assert.ok(framesDone(l) >= framesDone(l - 1));
});

test('the command shown is the one in the README', () => {
  assert.equal(EXPORT_COMMAND, 'celesta-exporter --react film.tsx intro.mp4');
  assert.equal(DONE_AT, 120); // make-score.py rings the bell here
});
