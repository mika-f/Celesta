// Where the moras and phonemes of a synthesized voice fall in its WAV, shared
// by the speech-engine adapters (`@celesta/voicevox`, `@celesta/aivis-speech`)
// so code that lights moras, changes a face at a pause, or builds the mouth
// accepts any of them. Phonemes use VOICEVOX's names, which engines that
// serve its API share: consonants ('k', 'sh', 'ch'), vowels 'a i u e o'
// (devoiced 'A I U E O'), 'N' (ん), 'cl' (っ), 'pau' for a pause and 'sil'
// for the silence before and after the voice.

import type { MouthShape } from '@celesta/react/internal';

import { lipSyncFromKeyframes } from './lipsync';
import type { LipSyncTrack, MouthKeyframe } from './lipsync';

/** A stretch of the WAV, in seconds from its first sample. `end >= start`. */
export interface SpeechSpan {
  readonly start: number;
  readonly end: number;
}

export interface SpeechPhoneme extends SpeechSpan {
  /** 'k', 'o', 'N', 'cl', 'pau', or 'sil' for the pre/post silence. */
  readonly phoneme: string;
}

export interface SpeechMora extends SpeechSpan {
  /** The mora's kana ('コ'), or the mark for a punctuation mora. */
  readonly text: string;
  readonly consonant: SpeechPhoneme | null;
  readonly vowel: SpeechPhoneme;
  /** Index into the query's `accent_phrases`. */
  readonly accentPhrase: number;
}

export interface SpeechPause extends SpeechSpan {
  /** Usually the punctuation ('、' or ','). Empty when unknown. */
  readonly text: string;
  readonly accentPhrase: number;
}

/**
 * From the first mora's start to the last mora's end; the pause after it is
 * not included. A phrase without moras has `start === end`.
 */
export interface SpeechAccentPhrase extends SpeechSpan {
  /** Its moras' text joined. */
  readonly text: string;
  readonly moras: readonly SpeechMora[];
  readonly pause: SpeechPause | null;
}

export interface SpeechTiming {
  /**
   * True when the times are the engine's own (VOICEVOX); false when they are
   * estimated from the WAV (AivisSpeech), within a few tens of milliseconds.
   */
  readonly exact: boolean;
  /** The WAV's length; equal to the `LipSyncTrack`'s. */
  readonly durationInSeconds: number;
  /** From the first mora's start to the last mora's end: the voice without pre/post silence. */
  readonly speech: SpeechSpan;
  /** One per query accent phrase, same order. */
  readonly accentPhrases: readonly SpeechAccentPhrase[];
  /** Every mora in order. The same objects as in `accentPhrases`. */
  readonly moras: readonly SpeechMora[];
  /** Every pause in order. */
  readonly pauses: readonly SpeechPause[];
  /** Every phoneme in order, `sil` and `pau` included; together they cover [0, durationInSeconds]. */
  readonly phonemes: readonly SpeechPhoneme[];
}

// Consonants made by closing both lips. Showing the next vowel's open mouth
// during them looks like the lips never meet, so they stay `closed`; every
// other consonant anticipates its vowel, which is how the mouth actually
// moves into a mora.
const BILABIAL_CONSONANTS = new Set(['m', 'my', 'b', 'by', 'p', 'py']);

/**
 * Mouth shape for a vowel phoneme. Devoiced vowels (`A I U E O`) keep their
 * vowel's shape; `N` (ん), `cl` (っ), `pau`, `sil` and anything unknown are
 * `closed`.
 */
export function speechVowelShape(vowel: string): MouthShape {
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
 * A `LipSyncTrack` from a speech timing: each vowel shows its shape, and a
 * consonant shows its mora's vowel except `m`/`b`/`p`, which close the lips;
 * pauses and silence are `closed`. Time 0 is the start of the WAV.
 */
export function lipSyncFromSpeechTiming(timing: SpeechTiming): LipSyncTrack {
  // Walks the timing's structure rather than relying on object identity, so
  // a copied timing (structuredClone, JSON) works too.
  const keyframes: MouthKeyframe[] = [];
  const push = (span: SpeechSpan, mouth: MouthShape): void => {
    if (span.end > span.start) keyframes.push({ seconds: span.start, mouth });
  };
  const { phonemes } = timing;
  if (phonemes.length > 0) push(phonemes[0], 'closed');
  for (const phrase of timing.accentPhrases) {
    for (const mora of phrase.moras) {
      const vowel = speechVowelShape(mora.vowel.phoneme);
      if (mora.consonant) {
        push(mora.consonant, BILABIAL_CONSONANTS.has(mora.consonant.phoneme) ? 'closed' : vowel);
      }
      push(mora.vowel, vowel);
    }
    if (phrase.pause) push(phrase.pause, 'closed');
  }
  if (phonemes.length > 0) push(phonemes[phonemes.length - 1], 'closed');
  return lipSyncFromKeyframes(keyframes, timing.durationInSeconds);
}
