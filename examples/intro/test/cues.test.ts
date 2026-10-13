import assert from 'node:assert/strict';
import test from 'node:test';
import { BAR, DURATION } from '../constants.ts';
import { HOLD, S, SCENE_ORDER, sceneAt } from '../shared/cues.ts';

test('scenes start on bar lines in storyboard order', () => {
  assert.deepEqual(SCENE_ORDER.map((k) => S[k]), [0, 180, 300, 540, 780, 1020, 1320, 1560]);
  for (const k of SCENE_ORDER) assert.equal(S[k] % BAR, 0);
  assert.equal(DURATION, 1800);
  assert.equal(HOLD, 1650);
});

test('sceneAt switches exactly on the cut', () => {
  assert.equal(sceneAt(179), 'coldOpen');
  assert.equal(sceneAt(180), 'reveal');
  assert.equal(sceneAt(1559), 'export');
  assert.equal(sceneAt(1799), 'outro');
});
