// Builds a lip-sync track from a VOICEVOX Engine `audio_query` result.
//
// Unlike `loadLipSync`, which guesses where the vowels fall inside a WAV,
// an AudioQuery states the length of every consonant and vowel the engine
// will synthesize, so the mouth lines up with the voice exactly. The times
// come from `timingFromVoicevox`, so the mouth and any other use of the
// timing (lit moras, a face change at a pause) agree.

import { lipSyncFromKeyframes, type LipSyncTrack, type MouthKeyframe, type MouthShape } from '@celesta/character';

import type { VoicevoxAudioQuery } from './query.js';
import { timingFromVoicevox } from './timing.js';
import type { VoicevoxSpan, VoicevoxTiming, VoicevoxTimingOptions } from './timing.js';

/** The options of `lipSyncFromVoicevox`; the same as `timingFromVoicevox`'s. */
export type VoicevoxLipSyncOptions = VoicevoxTimingOptions;

// Consonants made by closing both lips. Showing the next vowel's open mouth
// during them looks like the lips never meet, so they stay `closed`; every
// other consonant anticipates its vowel, which is how the mouth actually
// moves into a mora.
const BILABIAL_CONSONANTS = new Set(['m', 'my', 'b', 'by', 'p', 'py']);

/**
 * Mouth shape for a VOICEVOX vowel phoneme. Devoiced vowels (`A I U E O`)
 * keep their vowel's shape; `N` (ん), `cl` (っ), `pau`, `sil` and anything
 * unknown are `closed`.
 */
export function voicevoxVowelShape(vowel: string): MouthShape {
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
 * Converts a VOICEVOX AudioQuery, or the `timingFromVoicevox` result for
 * it, into a `LipSyncTrack` for `<Dialogue lipSync>`, `<CharacterView
 * lipSync>` or `useLipSync`. Pass the same query that synthesized the
 * voice, so speed and pause edits are reflected. Time 0 is the start of the
 * WAV, including `prePhonemeLength`.
 */
export function lipSyncFromVoicevox(timing: VoicevoxTiming): LipSyncTrack;
export function lipSyncFromVoicevox(input: VoicevoxAudioQuery | VoicevoxTiming): LipSyncTrack;
export function lipSyncFromVoicevox(query: VoicevoxAudioQuery, options?: VoicevoxTimingOptions): LipSyncTrack;
export function lipSyncFromVoicevox(
  input: VoicevoxAudioQuery | VoicevoxTiming,
  options?: VoicevoxTimingOptions,
): LipSyncTrack {
  if ('accent_phrases' in input) {
    return trackOf(timingFromVoicevox(input, options));
  }
  if (options !== undefined) {
    throw new Error('lipSyncFromVoicevox: options apply when the timing is computed; pass them to timingFromVoicevox');
  }
  return trackOf(input);
}

// Walks the phonemes in order through the timing's structure rather than by
// object identity, so a copied timing (structuredClone, JSON) works too.
function trackOf(timing: VoicevoxTiming): LipSyncTrack {
  const keyframes: MouthKeyframe[] = [];
  const push = (span: VoicevoxSpan, mouth: MouthShape): void => {
    if (span.end > span.start) keyframes.push({ seconds: span.start, mouth });
  };
  const { phonemes } = timing;
  push(phonemes[0], 'closed');
  for (const phrase of timing.accentPhrases) {
    for (const mora of phrase.moras) {
      const vowel = voicevoxVowelShape(mora.vowel.phoneme);
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
