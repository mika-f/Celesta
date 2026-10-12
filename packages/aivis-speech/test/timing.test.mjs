// Unit tests for timingFromAivisSpeech. Run after building @celesta/character
// and @celesta/aivis-speech: pnpm --dir packages/aivis-speech test
//
// The WAVs are synthesized by fixtures/voice.mjs from known phoneme lengths
// (the zundamon line VOICEVOX Engine reads in @celesta/voicevox's tests), so
// the estimated times can be compared with the truth.

import assert from 'node:assert/strict';
import test from 'node:test';

import { decodeWav } from '@celesta/character';
import { timingFromAivisSpeech } from '../dist/index.js';
import { KONNICHIWA, encodeWav, queryOf, synthesize } from './fixtures/voice.mjs';

const close = (actual, expected, tolerance, what) =>
  assert.ok(Math.abs(actual - expected) <= tolerance, `${what}: ${actual} vs ${expected}`);

function assertContiguous(timing, what) {
  const { phonemes } = timing;
  assert.equal(phonemes[0].start, 0, `${what}: starts at 0`);
  for (let i = 1; i < phonemes.length; i += 1) {
    assert.equal(phonemes[i].start, phonemes[i - 1].end, `${what}: phoneme ${i}`);
    assert.ok(phonemes[i].end >= phonemes[i].start, `${what}: phoneme ${i} is not reversed`);
  }
  assert.equal(phonemes[phonemes.length - 1].end, timing.durationInSeconds, `${what}: ends at duration`);
}

const cases = [
  ['1x', {}],
  ['speedScale 1.5', { speedScale: 1.5 }],
  ['slow, long pre, no post', { speedScale: 0.8, prePhonemeLength: 0.3, postPhonemeLength: 0 }],
];

test('timingFromAivisSpeech: the voice spans the WAV minus pre/post silence, exactly', () => {
  for (const [name, extra] of cases) {
    const query = queryOf(KONNICHIWA, extra);
    const voice = synthesize(KONNICHIWA, query);
    const timing = timingFromAivisSpeech(query, voice.wav);
    const speed = query.speedScale;
    close(timing.durationInSeconds, voice.samples.length / voice.sampleRate, 1e-9, `${name} duration`);
    close(timing.speech.start, query.prePhonemeLength / speed, 1e-9, `${name} speech start`);
    close(timing.speech.end, timing.durationInSeconds - query.postPhonemeLength / speed, 1e-9, `${name} speech end`);
    assert.equal(timing.phonemes[0].phoneme, 'sil');
    assert.equal(timing.phonemes[0].end, timing.speech.start);
    assert.equal(timing.phonemes.at(-1).phoneme, 'sil');
    assert.equal(timing.phonemes.at(-1).start, timing.speech.end);
    assert.equal(timing.moras[0].start, timing.speech.start);
    assert.equal(timing.moras.at(-1).end, timing.speech.end);
  }
});

test('timingFromAivisSpeech: pauses land on the silence the voice made', () => {
  for (const [name, extra] of cases) {
    const query = queryOf(KONNICHIWA, extra);
    const voice = synthesize(KONNICHIWA, query);
    const timing = timingFromAivisSpeech(query, voice.wav);
    const truth = voice.moras.filter((m) => m.text === ',');
    const comma = timing.pauses.find((p) => p.text === ',');
    close(comma.start, truth[0].start, 0.02, `${name} pause start`);
    close(comma.end, truth[0].end, 0.02, `${name} pause end`);
  }
});

test('timingFromAivisSpeech: moras start near their true times', () => {
  for (const [name, extra] of cases) {
    const query = queryOf(KONNICHIWA, extra);
    const voice = synthesize(KONNICHIWA, query);
    const timing = timingFromAivisSpeech(query, voice.wav);
    const errors = timing.moras.map((m, i) => Math.abs(m.start - voice.moras[i].start));
    const mean = errors.reduce((sum, e) => sum + e, 0) / errors.length;
    // Within a phrase only loudness and typical lengths guide the estimate.
    assert.ok(mean < 0.05, `${name}: mean error ${mean}`);
    assert.ok(Math.max(...errors) < 0.15, `${name}: worst error ${Math.max(...errors)}`);
  }
});

test('timingFromAivisSpeech: phonemes cover the WAV without gaps', () => {
  const query = queryOf(KONNICHIWA);
  const voice = synthesize(KONNICHIWA, query);
  assertContiguous(timingFromAivisSpeech(query, voice.wav), 'voice');
  // Silence, a WAV shorter than its pre/post silence, an empty query.
  assertContiguous(timingFromAivisSpeech(query, encodeWav(new Array(8000).fill(0), 16000)), 'silent');
  assertContiguous(timingFromAivisSpeech(query, encodeWav(new Array(1000).fill(0), 16000)), 'short');
  assertContiguous(timingFromAivisSpeech({ accent_phrases: [] }, voice.wav), 'empty');
});

test('timingFromAivisSpeech: mirrors the query', () => {
  const query = queryOf(KONNICHIWA);
  query.accent_phrases[1].pause_mora = { text: '、', vowel: 'pau', vowel_length: 0 };
  const timing = timingFromAivisSpeech(query, synthesize(KONNICHIWA, query).wav);
  assert.equal(timing.accentPhrases.length, 3);
  query.accent_phrases.forEach((phrase, i) => {
    const timed = timing.accentPhrases[i];
    assert.deepEqual(timed.moras.map((m) => m.text), phrase.moras.map((m) => m.text));
    assert.equal(timed.text, phrase.moras.map((m) => m.text).join(''));
    assert.ok(timed.moras.every((m) => m.accentPhrase === i));
    assert.equal(timed.start, timed.moras[0].start);
    assert.equal(timed.end, timed.moras.at(-1).end);
  });
  assert.deepEqual(timing.moras, timing.accentPhrases.flatMap((p) => p.moras));
  timing.moras.forEach((m, i) => assert.equal(m, timing.accentPhrases.flatMap((p) => p.moras)[i]));

  const [, ko, , , chi] = [null, ...timing.accentPhrases[0].moras];
  assert.equal(ko.consonant.phoneme, 'k');
  assert.equal(ko.vowel.phoneme, 'o');
  assert.equal(ko.consonant.start, ko.start);
  assert.equal(ko.consonant.end, ko.vowel.start);
  assert.equal(ko.vowel.end, ko.end);
  assert.equal(chi.consonant.phoneme, 'ch');
  const n = timing.accentPhrases[0].moras[1];
  assert.equal(n.consonant, null);
  assert.equal(n.vowel.phoneme, 'N');

  // Punctuation stays a mora; it and the pause mora are the pauses, in order.
  const comma = timing.accentPhrases[0].moras[5];
  assert.equal(comma.vowel.phoneme, 'pau');
  assert.equal(comma.consonant, null);
  assert.deepEqual(timing.pauses.map((p) => [p.text, p.accentPhrase]), [[',', 0], [',', 1], ['.', 2]]);
  assert.equal(timing.pauses[0].start, comma.start);
  assert.equal(timing.pauses[0].end, comma.end);
  assert.equal(timing.accentPhrases[0].pause, null);
  assert.equal(timing.accentPhrases[1].pause, timing.pauses[1]);
  assert.equal(timing.accentPhrases[1].pause.start, timing.accentPhrases[1].end);
  assert.equal(timing.accentPhrases[2].start, timing.accentPhrases[1].pause.end);
  assert.deepEqual(
    timing.phonemes.map((p) => p.phoneme).join(' '),
    'sil k o N n i ch i w a pau z u N d a m o N n a pau n o d a pau sil',
  );
});

test('timingFromAivisSpeech: takes the WAV bytes or decodeWav\'s result', () => {
  const query = queryOf(KONNICHIWA);
  const voice = synthesize(KONNICHIWA, query);
  assert.deepEqual(timingFromAivisSpeech(query, decodeWav(voice.wav)), timingFromAivisSpeech(query, voice.wav));
  assert.deepEqual(timingFromAivisSpeech(query, Buffer.from(voice.wav)), timingFromAivisSpeech(query, voice.wav));
});

test('timingFromAivisSpeech: a silent voice spreads moras by typical length', () => {
  const query = queryOf(KONNICHIWA);
  const timing = timingFromAivisSpeech(query, encodeWav(new Array(32000).fill(0), 16000));
  const spoken = timing.moras.filter((m) => m.vowel.phoneme !== 'pau');
  assert.ok(spoken.every((m) => m.end > m.start));
  assertContiguous(timing, 'silent');
});

test('timingFromAivisSpeech: moras fill a voice that is mostly silent', () => {
  // A 0.1 s burst, then 1.9 s of silence: the moras stretch over it.
  const samples = Float32Array.from({ length: 16000 }, (_, i) => (i < 800 ? 0.3 * Math.sin(i / 4) : 0));
  const timing = timingFromAivisSpeech(
    { accent_phrases: [{ moras: [{ text: 'ア', vowel: 'a' }, { text: 'イ', vowel: 'i' }] }] },
    { sampleRate: 8000, samples },
  );
  assert.equal(timing.moras[0].start, 0);
  assert.ok(timing.moras.every((m) => m.end > m.start), JSON.stringify(timing.moras));
  assert.equal(timing.moras[1].end, 2);
  assertContiguous(timing, 'mostly silent');
});

test('timingFromAivisSpeech rejects invalid input', () => {
  const query = queryOf(KONNICHIWA);
  const { wav } = synthesize(KONNICHIWA, query);
  assert.throws(() => timingFromAivisSpeech({ ...query, speedScale: 0 }, wav), /speedScale must be a positive number/);
  assert.throws(() => timingFromAivisSpeech({ ...query, prePhonemeLength: -1 }, wav), /prePhonemeLength must be a non-negative number/);
  assert.throws(() => timingFromAivisSpeech(query, { sampleRate: 0, samples: new Float32Array(10) }), /sample rate must be positive/);
  assert.throws(() => timingFromAivisSpeech(query, new Uint8Array(64)), /RIFF\/WAVE/);
});
