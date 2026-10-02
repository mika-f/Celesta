// Unit tests for `lipSyncFromVoicevox` / `lipSyncFromKeyframes`. Run after
// `pnpm run build`:  node --test test/
//
// `voicevox-zundamon-konnichiwa.json` is a real VOICEVOX Engine `audio_query`
// response (Zundamon, normal style) for 「こんにちは、ずんだもんなのだ。」.
// `voicevox-engine-0.26-synthesis.json` holds queries (some edited) together
// with the sample count of the WAV that VOICEVOX Engine 0.26.0-dev
// (docker `voicevox/voicevox_engine:cpu-ubuntu24.04-0.26.0-dev`) synthesized
// from each one.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { lipSyncFromKeyframes, lipSyncFromVoicevox, voicevoxVowelShape } from '../dist/index.js';

const fixture = JSON.parse(
  readFileSync(new URL('./fixtures/voicevox-zundamon-konnichiwa.json', import.meta.url), 'utf8'),
);
const clone = () => structuredClone(fixture);
const synthesized = JSON.parse(
  readFileSync(new URL('./fixtures/voicevox-engine-0.26-synthesis.json', import.meta.url), 'utf8'),
);
// Unrounded lengths, so phoneme midpoints can be computed by hand.
const exact = { frameRate: null };

const moras = fixture.accent_phrases.flatMap((phrase) => phrase.moras);
const moraSum = moras.reduce((sum, m) => sum + (m.consonant_length ?? 0) + m.vowel_length, 0);
const pause = fixture.accent_phrases[0].pause_mora.vowel_length;

// Phoneme spans in order: [length, expected mouth]. The midpoint of each is
// sampled, so the assertion does not depend on boundary rounding.
function expectedSpans(query) {
  const spans = [[query.prePhonemeLength, 'closed']];
  for (const phrase of query.accent_phrases) {
    for (const m of phrase.moras) {
      const vowel = { a: 'a', i: 'i', u: 'u', e: 'e', o: 'o' }[m.vowel] ?? 'closed';
      if (m.consonant) spans.push([m.consonant_length, m.consonant === 'm' ? 'closed' : vowel]);
      spans.push([m.vowel_length, vowel]);
    }
    if (phrase.pause_mora) spans.push([phrase.pause_mora.vowel_length, 'closed']);
  }
  spans.push([query.postPhonemeLength, 'closed']);
  return spans;
}

function assertSpans(track, spans, speedScale) {
  let start = 0;
  for (const [length, mouth] of spans) {
    const mid = (start + length / 2) / speedScale;
    assert.equal(track.mouthAtSeconds(mid), mouth, `at ${mid.toFixed(4)}s`);
    start += length;
  }
}

test('lipSyncFromVoicevox: unrounded duration is pre + moras + pause + post', () => {
  const track = lipSyncFromVoicevox(fixture, exact);
  assert.ok(Math.abs(track.durationInSeconds - (0.1 + moraSum + pause + 0.1)) < 1e-9);
  assert.ok(Math.abs(track.durationInSeconds - 2.5674) < 1e-4);
});

test('lipSyncFromVoicevox: duration matches the WAV VOICEVOX Engine synthesized', () => {
  // 240 frames of 256 samples at 24 kHz.
  assert.ok(Math.abs(lipSyncFromVoicevox(fixture).durationInSeconds - 2.56) < 1e-9);
  for (const { name, query, wavSamples, sampleRate } of synthesized) {
    const track = lipSyncFromVoicevox(query);
    const wavSeconds = wavSamples / sampleRate;
    assert.ok(
      Math.abs(track.durationInSeconds - wavSeconds) < 1e-9,
      `${name}: track ${track.durationInSeconds}s, WAV ${wavSeconds}s`,
    );
  }
});

test('lipSyncFromVoicevox: mouth shape at each phoneme', () => {
  const track = lipSyncFromVoicevox(fixture, exact);
  assertSpans(track, expectedSpans(fixture), 1);

  // Spot checks against hand-computed times.
  assert.equal(track.mouthAtSeconds(0), 'closed'); // pre silence
  assert.equal(track.mouthAtSeconds(0.1), 'o'); // コ's k anticipates o
  assert.equal(track.mouthAtSeconds(0.2), 'o'); // コ's o
  assert.equal(track.mouthAtSeconds(0.36), 'closed'); // ン
  assert.equal(track.mouthAtSeconds(0.45), 'i'); // ニ
  assert.equal(track.mouthAtSeconds(0.7), 'a'); // ワ
  assert.equal(track.mouthAtSeconds(1.0), 'closed'); // 、 pause
  assert.equal(track.mouthAtSeconds(1.3), 'u'); // ズ's z anticipates u
  assert.equal(track.mouthAtSeconds(track.durationInSeconds - 0.05), 'closed'); // post silence
  assert.equal(track.mouthAtSeconds(-1), 'closed');
  assert.equal(track.mouthAtSeconds(track.durationInSeconds), 'closed');
  assert.equal(track.mouthAtSeconds(100), 'closed');

  // モ: the bilabial m closes the lips, then the vowel opens to o.
  const moStart = 0.1 + pause + moras.slice(0, 8).reduce((s, m) => s + (m.consonant_length ?? 0) + m.vowel_length, 0);
  assert.equal(moras[8].text, 'モ');
  assert.equal(track.mouthAtSeconds(moStart + 0.01), 'closed');
  assert.equal(track.mouthAtSeconds(moStart + moras[8].consonant_length + 0.01), 'o');
});

test('lipSyncFromVoicevox: speedScale divides every length', () => {
  const query = clone();
  query.speedScale = 1.5;
  const track = lipSyncFromVoicevox(query, exact);
  assert.ok(Math.abs(track.durationInSeconds - (0.1 + moraSum + pause + 0.1) / 1.5) < 1e-9);
  assertSpans(track, expectedSpans(fixture), 1.5);
  // Frame 2 at 30 fps = 0.0667 s = 0.1 s of 1x time: コ's k (anticipating o).
  assert.equal(track.mouthAtFrame(2, 30), 'o');
  // Frame 30 = 1 s = 1.5 s of 1x time: the first ン of ずんだもん.
  assert.equal(track.mouthAtFrame(30, 30), 'closed');
  // Frame 28 = 0.933 s = 1.4 s of 1x time: ズ's u.
  assert.equal(track.mouthAtFrame(28, 30), 'u');
});

test('lipSyncFromVoicevox: pauseLength replaces and pauseLengthScale scales pause moras', () => {
  const base = 0.1 + moraSum + 0.1;

  const replaced = clone();
  replaced.pauseLength = 0.5;
  assert.ok(Math.abs(lipSyncFromVoicevox(replaced, exact).durationInSeconds - (base + 0.5)) < 1e-9);

  const scaled = clone();
  scaled.pauseLengthScale = 2;
  assert.ok(Math.abs(lipSyncFromVoicevox(scaled, exact).durationInSeconds - (base + pause * 2)) < 1e-9);

  const both = clone();
  both.pauseLength = 0.5;
  both.pauseLengthScale = 2;
  both.speedScale = 2;
  // pre/post silence are not pauses: only speedScale touches them.
  assert.ok(Math.abs(lipSyncFromVoicevox(both, exact).durationInSeconds - (base + 1) / 2) < 1e-9);
});

test('lipSyncFromVoicevox: devoiced vowels, cl and interrogative upspeak', () => {
  const query = {
    accent_phrases: [
      {
        moras: [
          { consonant: 's', consonant_length: 0.1, vowel: 'U', vowel_length: 0.1, pitch: 0 },
          { consonant: null, consonant_length: null, vowel: 'cl', vowel_length: 0.1, pitch: 0 },
          { consonant: 'k', consonant_length: 0.1, vowel: 'a', vowel_length: 0.1, pitch: 5.5 },
        ],
        pause_mora: null,
        is_interrogative: true,
      },
    ],
    speedScale: 1,
    prePhonemeLength: 0,
    postPhonemeLength: 0,
  };
  const track = lipSyncFromVoicevox(query, exact);
  assert.ok(Math.abs(track.durationInSeconds - 0.65) < 1e-9); // + 0.15 upspeak
  assert.equal(track.mouthAtSeconds(0.05), 'u');
  assert.equal(track.mouthAtSeconds(0.15), 'u');
  assert.equal(track.mouthAtSeconds(0.25), 'closed');
  assert.equal(track.mouthAtSeconds(0.35), 'a');
  assert.equal(track.mouthAtSeconds(0.55), 'a'); // upspeak repeats the vowel

  const flat = lipSyncFromVoicevox(query, { ...exact, interrogativeUpspeak: false });
  assert.ok(Math.abs(flat.durationInSeconds - 0.5) < 1e-9);
});

test('lipSyncFromVoicevox rejects a non-positive speedScale', () => {
  const query = clone();
  query.speedScale = 0;
  assert.throws(() => lipSyncFromVoicevox(query), /speedScale/);
  assert.throws(() => lipSyncFromVoicevox(fixture, { frameRate: 0 }), /frameRate/);
});

test('lipSyncFromVoicevox: frame rounding is half-to-even per phoneme', () => {
  // 0.016 s * 93.75 = 1.5 frames -> 2; 0.048 s = 4.5 frames -> 4.
  const query = {
    accent_phrases: [
      {
        moras: [
          { consonant: 'k', consonant_length: 0.016, vowel: 'a', vowel_length: 0.048, pitch: 5 },
        ],
      },
    ],
    prePhonemeLength: 0,
    postPhonemeLength: 0,
  };
  const track = lipSyncFromVoicevox(query);
  assert.ok(Math.abs(track.durationInSeconds - 6 / 93.75) < 1e-9);
  assert.equal(track.mouthAtSeconds(1.9 / 93.75), 'a'); // still the k frame
  assert.equal(track.mouthAtSeconds(5.9 / 93.75), 'a');
  assert.equal(track.mouthAtSeconds(6 / 93.75), 'closed');
});

test('voicevoxVowelShape maps phonemes to mouth shapes', () => {
  assert.deepEqual(
    ['a', 'i', 'u', 'e', 'o', 'A', 'I', 'U', 'E', 'O', 'N', 'cl', 'pau', 'sil'].map(voicevoxVowelShape),
    ['a', 'i', 'u', 'e', 'o', 'a', 'i', 'u', 'e', 'o', 'closed', 'closed', 'closed', 'closed'],
  );
});

test('lipSyncFromKeyframes holds each shape until the next keyframe', () => {
  const track = lipSyncFromKeyframes(
    [
      { seconds: 0.5, mouth: 'i' },
      { seconds: 0.2, mouth: 'a' },
      { seconds: 0.8, mouth: 'closed' },
    ],
    1,
  );
  assert.equal(track.durationInSeconds, 1);
  assert.equal(track.mouthAtSeconds(0.1), 'closed');
  assert.equal(track.mouthAtSeconds(0.2), 'a');
  assert.equal(track.mouthAtSeconds(0.49), 'a');
  assert.equal(track.mouthAtSeconds(0.5), 'i');
  assert.equal(track.mouthAtSeconds(0.9), 'closed');
  assert.equal(track.mouthAtFrame(15, 30), 'i');
  assert.equal(lipSyncFromKeyframes([], 1).mouthAtSeconds(0.5), 'closed');
  assert.throws(() => lipSyncFromKeyframes([], -1), /durationInSeconds/);
});
