import assert from 'node:assert/strict';
import { test } from 'vitest';

import { decodeAudio, probeMedia, setAudioDecoder, setMediaProbe } from '../src/media.js';

test('probeMedia probes a path once and shares the result', async () => {
  let calls = 0;
  setMediaProbe(async () => {
    calls += 1;
    return { durationSeconds: 2, audio: [] };
  });
  assert.deepEqual(await probeMedia('/media/a.mp4'), { src: '/media/a.mp4', durationSeconds: 2, audio: [] });
  await probeMedia('/media/a.mp4');
  assert.equal(calls, 1);
});

test('probeMedia does not cache a failure, so a later call probes again', async () => {
  let exists = false;
  let calls = 0;
  setMediaProbe(async (path) => {
    calls += 1;
    if (!exists) throw new Error(`no such file: ${path}`);
    return { durationSeconds: 1, audio: [] };
  });
  await assert.rejects(probeMedia('/media/later.wav'), /no such file: \/media\/later\.wav/);
  exists = true;
  assert.equal((await probeMedia('/media/later.wav')).durationSeconds, 1);
  assert.equal(calls, 2);
});

test('decodeAudio does not cache a failure either', async () => {
  let calls = 0;
  setAudioDecoder(async () => {
    calls += 1;
    if (calls === 1) throw new Error('not yet');
    return { sampleRate: 8000, samples: new Float32Array(1) };
  });
  await assert.rejects(decodeAudio('/media/voice.wav'), /not yet/);
  assert.equal((await decodeAudio('/media/voice.wav')).sampleRate, 8000);
  assert.equal(calls, 2);
});
