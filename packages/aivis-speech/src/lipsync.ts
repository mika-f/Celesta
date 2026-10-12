// Builds a lip-sync track from an AivisSpeech Engine `audio_query` result and
// the WAV synthesized from it.
//
// Unlike `loadLipSync`, which spreads the transcript's vowels evenly over
// the loud parts of a WAV, this reads the query's moras (consonants, ん, っ
// and punctuation included) and places them with `timingFromAivisSpeech`,
// so the mouth and any other use of the timing agree.

import { lipSyncFromKeyframes, type LipSyncTrack, type MouthKeyframe, type MouthShape, type WavAudio } from '@celesta/character';

import type { AivisSpeechAudioQuery } from './query.js';
import { timingFromAivisSpeech } from './timing.js';
import type { AivisSpeechSpan, AivisSpeechTiming } from './timing.js';

// Consonants made by closing both lips. Showing the next vowel's open mouth
// during them looks like the lips never meet, so they stay `closed`; every
// other consonant anticipates its vowel.
const BILABIAL_CONSONANTS = new Set(['m', 'my', 'b', 'by', 'p', 'py']);

/**
 * Mouth shape for an AivisSpeech vowel phoneme. Devoiced vowels (`A I U E O`)
 * keep their vowel's shape; `N` (ん), `cl` (っ), `pau`, `sil` and anything
 * unknown are `closed`.
 */
export function aivisSpeechVowelShape(vowel: string): MouthShape {
  switch (vowel) {
    case 'a':
    case 'A':
      return 'a';
    case 'i':
    case 'I':
      return 'i';
    case 'u':
    case 'U':
      return 'u';
    case 'e':
    case 'E':
      return 'e';
    case 'o':
    case 'O':
      return 'o';
    default:
      return 'closed';
  }
}

/**
 * Converts an AivisSpeech AudioQuery and the WAV synthesized from it, or the
 * `timingFromAivisSpeech` result for them, into a `LipSyncTrack` for
 * `<Dialogue lipSync>`, `<CharacterView lipSync>` or `useLipSync`. Time 0 is
 * the start of the WAV, including `prePhonemeLength`.
 */
export function lipSyncFromAivisSpeech(timing: AivisSpeechTiming): LipSyncTrack;
export function lipSyncFromAivisSpeech(query: AivisSpeechAudioQuery, wav: Uint8Array | WavAudio): LipSyncTrack;
export function lipSyncFromAivisSpeech(
  input: AivisSpeechAudioQuery | AivisSpeechTiming,
  wav?: Uint8Array | WavAudio,
): LipSyncTrack;
export function lipSyncFromAivisSpeech(
  input: AivisSpeechAudioQuery | AivisSpeechTiming,
  wav?: Uint8Array | WavAudio,
): LipSyncTrack {
  if ('accent_phrases' in input) {
    if (wav === undefined) {
      throw new Error('lipSyncFromAivisSpeech: pass the WAV synthesized from the query; AivisSpeech queries hold no phoneme lengths');
    }
    return trackOf(timingFromAivisSpeech(input, wav));
  }
  if (wav !== undefined) {
    throw new Error('lipSyncFromAivisSpeech: the timing already holds the times; pass the WAV to timingFromAivisSpeech');
  }
  return trackOf(input);
}

// Walks the phonemes in order through the timing's structure rather than by
// object identity, so a copied timing (structuredClone, JSON) works too.
function trackOf(timing: AivisSpeechTiming): LipSyncTrack {
  const keyframes: MouthKeyframe[] = [];
  const push = (span: AivisSpeechSpan, mouth: MouthShape): void => {
    if (span.end > span.start) keyframes.push({ seconds: span.start, mouth });
  };
  const { phonemes } = timing;
  push(phonemes[0], 'closed');
  for (const phrase of timing.accentPhrases) {
    for (const mora of phrase.moras) {
      const vowel = aivisSpeechVowelShape(mora.vowel.phoneme);
      if (mora.consonant) {
        push(mora.consonant, BILABIAL_CONSONANTS.has(mora.consonant.phoneme) ? 'closed' : vowel);
      }
      push(mora.vowel, vowel);
    }
    if (phrase.pause) push(phrase.pause, 'closed');
  }
  push(phonemes[phonemes.length - 1], 'closed');
  return lipSyncFromKeyframes(keyframes, timing.durationInSeconds);
}
