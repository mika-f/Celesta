// Builds a lip-sync track from an AivisSpeech Engine `audio_query` result and
// the WAV synthesized from it.
//
// Unlike `loadLipSync`, which spreads the transcript's vowels evenly over
// the loud parts of a WAV, this reads the query's moras (consonants, ん, っ
// and punctuation included) and places them with `timingFromAivisSpeech`,
// so the mouth and any other use of the timing agree. The mouth rules are
// `lipSyncFromSpeechTiming`'s, shared with `@celesta/voicevox`.

import { lipSyncFromSpeechTiming, type LipSyncTrack, type WavAudio } from '@celesta/character';

import type { AivisSpeechAudioQuery } from './query.js';
import { timingFromAivisSpeech } from './timing.js';
import type { AivisSpeechTiming } from './timing.js';

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
    return lipSyncFromSpeechTiming(timingFromAivisSpeech(input, wav));
  }
  if (wav !== undefined) {
    throw new Error('lipSyncFromAivisSpeech: the timing already holds the times; pass the WAV to timingFromAivisSpeech');
  }
  return lipSyncFromSpeechTiming(input);
}
