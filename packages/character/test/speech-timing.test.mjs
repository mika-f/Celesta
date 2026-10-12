// Unit tests for the engine-neutral speech timing. Run after
// `pnpm run build`. The adapters that produce timings (@celesta/voicevox,
// @celesta/aivis-speech) test their own times.

import assert from 'node:assert/strict';
import { test } from 'vitest';

import { lipSyncFromSpeechTiming, speechVowelShape } from '../dist/index.js';

// sil | コ (k o) | ン | 、 | モ (m o) | sil, built by hand.
function timingOf() {
  const span = (phoneme, start, end) => ({ phoneme, start, end });
  const ko = { text: 'コ', start: 0.1, end: 0.3, consonant: span('k', 0.1, 0.15), vowel: span('o', 0.15, 0.3), accentPhrase: 0 };
  const n = { text: 'ン', start: 0.3, end: 0.4, consonant: null, vowel: span('N', 0.3, 0.4), accentPhrase: 0 };
  const pause = { text: '、', start: 0.4, end: 0.6, accentPhrase: 0 };
  const mo = { text: 'モ', start: 0.6, end: 0.8, consonant: span('m', 0.6, 0.65), vowel: span('o', 0.65, 0.8), accentPhrase: 1 };
  return {
    exact: true,
    durationInSeconds: 0.9,
    speech: { start: 0.1, end: 0.8 },
    accentPhrases: [
      { text: 'コン', start: 0.1, end: 0.4, moras: [ko, n], pause },
      { text: 'モ', start: 0.6, end: 0.8, moras: [mo], pause: null },
    ],
    moras: [ko, n, mo],
    pauses: [pause],
    phonemes: [span('sil', 0, 0.1), ko.consonant, ko.vowel, n.vowel, span('pau', 0.4, 0.6), mo.consonant, mo.vowel, span('sil', 0.8, 0.9)],
  };
}

test('lipSyncFromSpeechTiming: vowels show their shape, consonants anticipate them, bilabials close', () => {
  const track = lipSyncFromSpeechTiming(timingOf());
  assert.equal(track.durationInSeconds, 0.9);
  const at = (seconds) => track.mouthAtSeconds(seconds);
  assert.deepEqual(
    [at(0.05), at(0.12), at(0.2), at(0.35), at(0.5), at(0.62), at(0.7), at(0.85), at(0.9)],
    ['closed', 'o', 'o', 'closed', 'closed', 'closed', 'o', 'closed', 'closed'],
  );
});

test('lipSyncFromSpeechTiming: a copied timing gives the same track', () => {
  const timing = timingOf();
  const copy = JSON.parse(JSON.stringify(timing));
  for (let seconds = 0.005; seconds < 1; seconds += 0.01) {
    assert.equal(lipSyncFromSpeechTiming(copy).mouthAtSeconds(seconds), lipSyncFromSpeechTiming(timing).mouthAtSeconds(seconds));
  }
});

test('lipSyncFromSpeechTiming: empty spans and an empty timing are closed', () => {
  const empty = { exact: false, durationInSeconds: 0.5, speech: { start: 0, end: 0 }, accentPhrases: [], moras: [], pauses: [], phonemes: [] };
  assert.equal(lipSyncFromSpeechTiming(empty).mouthAtSeconds(0.25), 'closed');
});

test('speechVowelShape maps phonemes to mouth shapes', () => {
  assert.deepEqual(
    ['a', 'i', 'u', 'e', 'o', 'A', 'I', 'U', 'E', 'O', 'N', 'cl', 'pau', 'sil', 'x'].map(speechVowelShape),
    ['a', 'i', 'u', 'e', 'o', 'a', 'i', 'u', 'e', 'o', 'closed', 'closed', 'closed', 'closed', 'closed'],
  );
});
