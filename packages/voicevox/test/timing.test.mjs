// Unit tests for timingFromVoicevox. Run after building @celesta/character
// and @celesta/voicevox: pnpm --dir packages/voicevox test
//
// Fixtures are described in lipsync.test.mjs.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { lipSyncFromVoicevox, timingFromVoicevox } from '../dist/index.js';

const read = (name) => JSON.parse(readFileSync(new URL(`./fixtures/${name}`, import.meta.url), 'utf8'));
const fixture = read('voicevox-zundamon-konnichiwa.json');
const synthesized = read('voicevox-engine-0.26-synthesis.json');
const clone = () => structuredClone(fixture);
const exact = { frameRate: null };
const FRAME = 1 / 93.75;
const close = (actual, expected, what) =>
  assert.ok(Math.abs(actual - expected) < 1e-9, `${what}: ${actual} vs ${expected}`);
const allQueries = [['zundamon', fixture], ...synthesized.map(({ name, query }) => [name, query])];

function assertContiguous(timing, what) {
  const { phonemes } = timing;
  assert.equal(phonemes[0].start, 0, `${what}: starts at 0`);
  for (let i = 1; i < phonemes.length; i += 1) {
    assert.equal(phonemes[i].start, phonemes[i - 1].end, `${what}: phoneme ${i}`);
  }
  assert.equal(phonemes[phonemes.length - 1].end, timing.durationInSeconds, `${what}: ends at duration`);
}

test('timingFromVoicevox: duration matches the WAV and the lip-sync track', () => {
  // Whole frames over 93.75: exact, not a sum of rounded seconds.
  assert.equal(timingFromVoicevox(fixture).durationInSeconds, 240 / 93.75);
  for (const { name, query, wavSamples, sampleRate } of synthesized) {
    const timing = timingFromVoicevox(query);
    close(timing.durationInSeconds, wavSamples / sampleRate, name);
    close(timing.durationInSeconds, lipSyncFromVoicevox(query).durationInSeconds, `${name} lip sync`);
  }
});

test('timingFromVoicevox: phonemes cover the WAV without gaps', () => {
  for (const [name, query] of allQueries) {
    assertContiguous(timingFromVoicevox(query), name);
    assertContiguous(timingFromVoicevox(query, exact), `${name} unrounded`);
  }
});

test('timingFromVoicevox: mirrors the query', () => {
  for (const [name, query] of allQueries) {
    const timing = timingFromVoicevox(query, { interrogativeUpspeak: false });
    assert.equal(timing.accentPhrases.length, query.accent_phrases.length, name);
    query.accent_phrases.forEach((phrase, i) => {
      const timed = timing.accentPhrases[i];
      assert.deepEqual(timed.moras.map((m) => m.text), phrase.moras.map((m) => m.text), `${name} phrase ${i}`);
      assert.equal(timed.text, phrase.moras.map((m) => m.text).join(''));
      assert.ok(timed.moras.every((m) => m.accentPhrase === i && !m.upspeak));
      assert.equal(timed.pause === null, !phrase.pause_mora);
      if (phrase.pause_mora) assert.equal(timed.pause.text, phrase.pause_mora.text);
    });
    assert.deepEqual(timing.moras, timing.accentPhrases.flatMap((p) => p.moras));
    timing.moras.forEach((m, i) => assert.equal(m, timing.accentPhrases.flatMap((p) => p.moras)[i]));
    timing.pauses.forEach((p) => assert.equal(p, timing.accentPhrases[p.accentPhrase].pause));
    assert.equal(timing.pauses.length, query.accent_phrases.filter((p) => p.pause_mora).length);
  }
});

test('timingFromVoicevox: hand-computed times (unrounded)', () => {
  const timing = timingFromVoicevox(fixture, exact);
  const moras = fixture.accent_phrases.flatMap((p) => p.moras);
  const lengthOf = (m) => (m.consonant_length ?? 0) + m.vowel_length;
  const firstPhrase = fixture.accent_phrases[0].moras.reduce((s, m) => s + lengthOf(m), 0);
  const pause = fixture.accent_phrases[0].pause_mora.vowel_length;

  assert.equal(timing.phonemes[0].phoneme, 'sil');
  close(timing.phonemes[0].end, 0.1, 'pre silence');
  const ko = timing.moras[0];
  assert.equal(ko.text, 'コ');
  close(ko.start, 0.1, 'コ start');
  assert.equal(ko.consonant.phoneme, 'k');
  close(ko.consonant.end, 0.1 + moras[0].consonant_length, 'コ consonant end');
  assert.equal(ko.vowel.phoneme, 'o');
  assert.equal(ko.end, ko.vowel.end);
  assert.equal(timing.moras[1].consonant, null); // ン

  assert.equal(timing.pauses.length, 1);
  assert.equal(timing.pauses[0].text, '、');
  close(timing.pauses[0].start, 0.1 + firstPhrase, 'pause start');
  close(timing.pauses[0].end, 0.1 + firstPhrase + pause, 'pause end');
  close(timing.accentPhrases[0].end, timing.pauses[0].start, 'phrase end excludes the pause');

  const mo = timing.moras[8];
  assert.equal(mo.text, 'モ');
  const moStart = 0.1 + pause + moras.slice(0, 8).reduce((s, m) => s + lengthOf(m), 0);
  close(mo.start, moStart, 'モ start');
  close(mo.vowel.start, moStart + moras[8].consonant_length, 'モ vowel start');

  assert.equal(timing.speech.start, timing.moras[0].start);
  assert.equal(timing.speech.end, timing.moras[timing.moras.length - 1].end);
  assert.equal(timing.speech.end, timing.phonemes[timing.phonemes.length - 1].start);
  close(timing.durationInSeconds - timing.speech.end, 0.1, 'post silence');
});

test('timingFromVoicevox: half-to-even per phoneme; a mora is the sum of its rounded phonemes', () => {
  // 0.016 s = 1.5 frames -> 2, so k + a = 4 frames, although 0.032 s rounds to 3.
  const query = {
    accent_phrases: [{ moras: [{ text: 'カ', consonant: 'k', consonant_length: 0.016, vowel: 'a', vowel_length: 0.016, pitch: 5 }] }],
    prePhonemeLength: 0,
    postPhonemeLength: 0,
  };
  const timing = timingFromVoicevox(query);
  assert.equal(timing.moras[0].consonant.end, 2 / 93.75);
  assert.equal(timing.moras[0].end, 4 / 93.75);
  assert.equal(timing.durationInSeconds, 4 / 93.75);

  // 0.048 s = 4.5 frames -> 4.
  query.accent_phrases[0].moras[0].vowel_length = 0.048;
  assert.equal(timingFromVoicevox(query).moras[0].vowel.end, 6 / 93.75);
});

test('timingFromVoicevox: interrogative upspeak', () => {
  const query = {
    accent_phrases: [{
      moras: [
        { text: 'ス', consonant: 's', consonant_length: 0.1, vowel: 'U', vowel_length: 0.1, pitch: 0 },
        { text: 'ッ', consonant: null, consonant_length: null, vowel: 'cl', vowel_length: 0.1, pitch: 0 },
        { text: 'カ', consonant: 'k', consonant_length: 0.1, vowel: 'a', vowel_length: 0.1, pitch: 5.5 },
      ],
      pause_mora: null,
      is_interrogative: true,
    }],
    prePhonemeLength: 0,
    postPhonemeLength: 0,
  };
  const timing = timingFromVoicevox(query, exact);
  assert.equal(timing.moras.length, 4);
  const up = timing.moras[3];
  assert.deepEqual(
    { text: up.text, upspeak: up.upspeak, consonant: up.consonant, vowel: up.vowel.phoneme, accentPhrase: up.accentPhrase },
    { text: 'ア', upspeak: true, consonant: null, vowel: 'a', accentPhrase: 0 },
  );
  close(up.start, 0.5, 'upspeak start');
  close(up.end, 0.65, 'upspeak end');
  assert.equal(timing.accentPhrases[0].text, 'スッカア');
  assert.equal(timing.accentPhrases[0].end, up.end);

  assert.equal(timingFromVoicevox(query, { ...exact, interrogativeUpspeak: false }).moras.length, 3);
  const lowered = structuredClone(query);
  lowered.accent_phrases[0].moras[2].pitch = 0;
  assert.equal(timingFromVoicevox(lowered, exact).moras.length, 3);
  const ending = structuredClone(query);
  ending.accent_phrases[0].moras.push({ text: 'ン', vowel: 'N', vowel_length: 0.1, pitch: 5 });
  const endingUp = timingFromVoicevox(ending, exact).moras.at(-1);
  assert.deepEqual({ text: endingUp.text, upspeak: endingUp.upspeak }, { text: 'ン', upspeak: true });
});

test('timingFromVoicevox: upspeak text falls back to the vowel', () => {
  // An edited query: a devoiced vowel with a positive pitch. The engine's
  // kana lookup would fail; the timing keeps the vowel string.
  const query = {
    accent_phrases: [{ moras: [{ text: 'ス', consonant: 's', consonant_length: 0.1, vowel: 'U', vowel_length: 0.1, pitch: 5 }], is_interrogative: true }],
  };
  const timing = timingFromVoicevox(query, exact);
  assert.equal(timing.moras[1].text, 'U');
  assert.equal(timing.moras[1].upspeak, true);
});

test('timingFromVoicevox: pause settings move pauses only; speedScale divides everything', () => {
  const query = clone();
  query.pauseLength = 0.5;
  query.pauseLengthScale = 2;
  query.speedScale = 2;
  const timing = timingFromVoicevox(query, exact);
  close(timing.phonemes[0].end, 0.05, 'pre silence is not a pause');
  close(timing.pauses[0].end - timing.pauses[0].start, 0.5, 'pause length');
  close(timing.moras[0].vowel.end - timing.moras[0].vowel.start, fixture.accent_phrases[0].moras[0].vowel_length / 2, 'vowel');
});

test('timingFromVoicevox: zero-length phonemes stay in place', () => {
  const query = {
    accent_phrases: [{
      moras: [
        { text: 'カ', consonant: 'k', consonant_length: null, vowel: 'a', vowel_length: 0.1 },
        { text: 'イ', vowel: 'i', vowel_length: 0.001 }, // 0.09 frames -> 0
      ],
    }],
  };
  const timing = timingFromVoicevox(query);
  assert.equal(timing.moras[0].consonant.phoneme, 'k');
  assert.equal(timing.moras[0].consonant.start, timing.moras[0].consonant.end);
  assert.equal(timing.moras[1].start, timing.moras[1].end);
  assert.equal(timing.moras.length, 2);
  assertContiguous(timing, 'zero-length');
});

test('timingFromVoicevox: minimal query', () => {
  const query = { accent_phrases: [{ moras: [{ consonant: '', vowel: 'a', vowel_length: 0.2 }] }] };
  const timing = timingFromVoicevox(query, exact);
  assert.equal(timing.moras[0].text, '');
  assert.equal(timing.moras[0].consonant, null);
  assert.equal(timing.accentPhrases[0].pause, null);
  assert.deepEqual(timing.phonemes.map((p) => p.phoneme), ['sil', 'a', 'sil']);
  assert.equal(timing.phonemes[0].end, 0);
  close(timing.durationInSeconds, 0.2, 'duration');
});

test('timingFromVoicevox: no accent phrases', () => {
  const timing = timingFromVoicevox({ accent_phrases: [], prePhonemeLength: 0.1, postPhonemeLength: 0.1 }, exact);
  assert.deepEqual(timing.phonemes.map((p) => p.phoneme), ['sil', 'sil']);
  assert.deepEqual([timing.moras, timing.pauses, timing.accentPhrases], [[], [], []]);
  assert.deepEqual(timing.speech, { start: timing.phonemes[0].end, end: timing.phonemes[0].end });
  close(timing.durationInSeconds, 0.2, 'duration');
});

test('timingFromVoicevox: custom frameRate', () => {
  const query = {
    accent_phrases: [{ moras: [{ consonant: 'k', consonant_length: 0.034, vowel: 'a', vowel_length: 0.105 }] }],
    prePhonemeLength: 0.1,
    postPhonemeLength: 0.1,
  };
  const timing = timingFromVoicevox(query, { frameRate: 100 });
  assertContiguous(timing, 'frameRate 100');
  // pre 10, k 3.4 -> 3, a exactly 10.5 -> 10 (half to even), post 10.
  assert.equal(timing.durationInSeconds, (10 + 3 + 10 + 10) / 100);
});

test('timingFromVoicevox: rejects invalid input', () => {
  const query = clone();
  query.speedScale = 0;
  assert.throws(() => timingFromVoicevox(query), /^Error: timingFromVoicevox: speedScale must be a positive number/);
  assert.throws(() => timingFromVoicevox(fixture, { frameRate: 0 }), /timingFromVoicevox: frameRate must be a positive number or null/);
  const negative = clone();
  negative.accent_phrases[0].moras[0].vowel_length = -1;
  assert.throws(() => timingFromVoicevox(negative), /timingFromVoicevox: vowel_length must be a non-negative number/);
  const nan = clone();
  nan.prePhonemeLength = Number.NaN;
  assert.throws(() => timingFromVoicevox(nan), /prePhonemeLength/);
  const pause = clone();
  pause.pauseLengthScale = -1;
  assert.throws(() => timingFromVoicevox(pause), /pause length/);
});
