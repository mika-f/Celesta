// Unit tests for lipSyncFromAivisSpeech. Run after building
// @celesta/character and @celesta/aivis-speech:
// pnpm --dir packages/aivis-speech test

import assert from 'node:assert/strict';
import test from 'node:test';

import { lipSyncFromSpeechTiming } from '@celesta/character';
import { lipSyncFromAivisSpeech, timingFromAivisSpeech } from '../dist/index.js';
import { KONNICHIWA, queryOf, synthesize } from './fixtures/voice.mjs';

const query = queryOf(KONNICHIWA);
const { wav } = synthesize(KONNICHIWA, query);
const timing = timingFromAivisSpeech(query, wav);
const mid = (span) => (span.start + span.end) / 2;

// Every 0.5 ms offset, so samples never sit exactly on a boundary.
function shapesOf(track) {
  const shapes = [];
  for (let i = 0; i < Math.ceil(track.durationInSeconds * 1000) + 5; i += 1) {
    shapes.push(track.mouthAtSeconds((i + 0.5) / 1000));
  }
  return shapes;
}

test('lipSyncFromAivisSpeech: the track lasts the WAV and follows the timing', () => {
  const track = lipSyncFromAivisSpeech(query, wav);
  assert.equal(track.durationInSeconds, timing.durationInSeconds);
  assert.equal(track.mouthAtSeconds(mid(timing.phonemes[0])), 'closed'); // pre silence
  assert.equal(track.mouthAtSeconds(mid(timing.phonemes.at(-1))), 'closed'); // post silence
  assert.equal(track.mouthAtSeconds(-1), 'closed');
  assert.equal(track.mouthAtSeconds(track.durationInSeconds), 'closed');

  const [ko, n, ni, , wa, comma] = timing.accentPhrases[0].moras;
  assert.equal(track.mouthAtSeconds(mid(ko.consonant)), 'o'); // k anticipates o
  assert.equal(track.mouthAtSeconds(mid(ko.vowel)), 'o');
  assert.equal(track.mouthAtSeconds(mid(n)), 'closed'); // ン
  assert.equal(track.mouthAtSeconds(mid(ni.vowel)), 'i');
  assert.equal(track.mouthAtSeconds(mid(wa.vowel)), 'a');
  assert.equal(track.mouthAtSeconds(mid(comma)), 'closed'); // , pause
  // モ: the bilabial m closes the lips, then the vowel opens to o.
  const mo = timing.accentPhrases[1].moras[3];
  assert.equal(mo.text, 'モ');
  assert.equal(track.mouthAtSeconds(mid(mo.consonant)), 'closed');
  assert.equal(track.mouthAtSeconds(mid(mo.vowel)), 'o');
});

test('lipSyncFromAivisSpeech: a timing, or a copy of it, gives the same track as its query and WAV', () => {
  const fromQuery = shapesOf(lipSyncFromAivisSpeech(query, wav));
  assert.deepEqual(shapesOf(lipSyncFromAivisSpeech(timing)), fromQuery);
  assert.deepEqual(shapesOf(lipSyncFromAivisSpeech(JSON.parse(JSON.stringify(timing)))), fromQuery);
  assert.deepEqual(shapesOf(lipSyncFromAivisSpeech(structuredClone(timing))), fromQuery);
});

test('lipSyncFromAivisSpeech: a pause mora closes the mouth', () => {
  const withPause = queryOf(KONNICHIWA);
  withPause.accent_phrases[1].pause_mora = { text: '、', vowel: 'pau', vowel_length: 0 };
  const phrases = [KONNICHIWA[0], [...KONNICHIWA[1], [',', null, 0, 'pau', 0.3]], KONNICHIWA[2]];
  const voice = synthesize(phrases, withPause);
  const paused = timingFromAivisSpeech(withPause, voice.wav);
  const { pause } = paused.accentPhrases[1];
  assert.ok(pause.end - pause.start > 0.25, `pause lasts ${pause.end - pause.start}s`);
  assert.equal(lipSyncFromAivisSpeech(paused).mouthAtSeconds(mid(pause)), 'closed');
});

test('lipSyncFromAivisSpeech: a query without its WAV, or a timing with one, throws', () => {
  assert.throws(
    () => lipSyncFromAivisSpeech(query),
    /lipSyncFromAivisSpeech: pass the WAV synthesized from the query; AivisSpeech queries hold no phoneme lengths/,
  );
  assert.throws(
    () => lipSyncFromAivisSpeech(timing, wav),
    /lipSyncFromAivisSpeech: the timing already holds the times; pass the WAV to timingFromAivisSpeech/,
  );
});

test('lipSyncFromAivisSpeech: the timing is a SpeechTiming marked approximate', () => {
  assert.equal(timing.exact, false);
  assert.deepEqual(shapesOf(lipSyncFromSpeechTiming(timing)), shapesOf(lipSyncFromAivisSpeech(timing)));
});
