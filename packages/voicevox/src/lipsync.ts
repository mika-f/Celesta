// Builds a lip-sync track from a VOICEVOX Engine `audio_query` result.
//
// Unlike `loadLipSync`, which guesses where the vowels fall inside a WAV,
// an AudioQuery states the length of every consonant and vowel the engine
// will synthesize, so the mouth lines up with the voice exactly. The times
// come from `timingFromVoicevox`, so the mouth and any other use of the
// timing (lit moras, a face change at a pause) agree.

import { lipSyncFromSpeechTiming, speechVowelShape, type LipSyncTrack } from '@celesta/character';

import type { VoicevoxAudioQuery } from './query.js';
import { timingFromVoicevox } from './timing.js';
import type { VoicevoxTiming, VoicevoxTimingOptions } from './timing.js';

/** The options of `lipSyncFromVoicevox`; the same as `timingFromVoicevox`'s. */
export type VoicevoxLipSyncOptions = VoicevoxTimingOptions;

/**
 * Mouth shape for a VOICEVOX vowel phoneme: `speechVowelShape` from
 * `@celesta/character`. Devoiced vowels (`A I U E O`) keep their vowel's
 * shape; `N` (ん), `cl` (っ), `pau`, `sil` and anything unknown are `closed`.
 */
export const voicevoxVowelShape = speechVowelShape;

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
    return lipSyncFromSpeechTiming(timingFromVoicevox(input, options));
  }
  if (options !== undefined) {
    throw new Error('lipSyncFromVoicevox: options apply when the timing is computed; pass them to timingFromVoicevox');
  }
  return lipSyncFromSpeechTiming(input);
}
