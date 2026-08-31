import assert from 'node:assert/strict';
import test from 'node:test';

import { mediaDurationInFrames } from '../dist/index.js';

test('mediaDurationInFrames rounds a probed duration up to a whole frame', () => {
  assert.equal(mediaDurationInFrames({ src: 'clip.mp4', durationSeconds: 1.01, audio: [] }, 30), 31);
});

test('mediaDurationInFrames preserves an unknown duration', () => {
  assert.equal(mediaDurationInFrames({ src: 'stream', audio: [] }, 30), undefined);
});
