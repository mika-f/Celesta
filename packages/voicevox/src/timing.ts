// Where every phoneme of a VOICEVOX AudioQuery falls in the synthesized WAV.
//
// The arithmetic mirrors `_query_to_decoder_feature` in voicevox_engine's
// `tts_pipeline/tts_engine.py`: optional interrogative upspeak (a 0.15 s
// mora repeating the last vowel, its `text` the vowel's kana), then a
// `prePhonemeLength` silence, every accent phrase's moras followed by its
// `pause_mora`, a `postPhonemeLength` silence; `pauseLength` /
// `pauseLengthScale` adjust only the pause moras, and `speedScale` divides
// everything. Each phoneme is then rounded (half to even, like numpy) to the
// engine's 93.75 Hz frame grid, so a mora lasts the sum of its rounded
// phonemes. That rounding is what makes the times match the WAV to the
// sample; without it the error accumulates (about 50 ms over a 1.7 s line at
// `speedScale: 1.5`).

import type { SpeechAccentPhrase, SpeechMora, SpeechPause, SpeechPhoneme, SpeechSpan, SpeechTiming } from '@celesta/character';

import type { VoicevoxAudioQuery, VoicevoxMora } from './query.js';

/** A stretch of the WAV, in seconds from its first sample. `end >= start`. */
export type VoicevoxSpan = SpeechSpan;

/**
 * Phonemes as in the query: 'k', 'o', 'N', 'cl', 'pau'. The pre/post
 * silence is 'sil', the engine's mora name for it (its decoder folds it into
 * 'pau'); keeping 'sil' tells the surrounding silence apart from a pause.
 */
export type VoicevoxTimedPhoneme = SpeechPhoneme;

export interface VoicevoxTimedMora extends SpeechMora {
  /** The query's `text` ('コ'); for an upspeak mora, its vowel's kana like the engine ('ア'). */
  readonly text: string;
  /** True for the mora interrogative upspeak appended; it is not in the query. */
  readonly upspeak: boolean;
}

/** A pause mora; its `text` is usually the punctuation ('、'). */
export type VoicevoxTimedPause = SpeechPause;

export interface VoicevoxTimedAccentPhrase extends SpeechAccentPhrase {
  /** `moras[j]` is `query.accent_phrases[i].moras[j]`; an upspeak mora comes last. */
  readonly moras: readonly VoicevoxTimedMora[];
}

/**
 * A `SpeechTiming` with the engine's own times (`exact: true`). Without
 * moras, `speech` starts and ends at the end of the pre silence.
 */
export interface VoicevoxTiming extends SpeechTiming {
  readonly exact: true;
  /** One per `query.accent_phrases`, same order. */
  readonly accentPhrases: readonly VoicevoxTimedAccentPhrase[];
  /** Every mora in order, upspeak moras included. The same objects as in `accentPhrases`. */
  readonly moras: readonly VoicevoxTimedMora[];
}

export interface VoicevoxTimingOptions {
  /**
   * Whether the voice was synthesized with interrogative upspeak, which
   * appends a 0.15 s mora to accent phrases marked `is_interrogative`.
   * Matches `/synthesis`'s `enable_interrogative_upspeak`, default `true`.
   */
  interrogativeUpspeak?: boolean;
  /**
   * Frames per second the engine quantizes each phoneme length to.
   * VOICEVOX synthesizes at 24 kHz with a 256-sample hop: 93.75. Pass `null`
   * to keep the unrounded lengths, e.g. for an engine that does not use
   * this grid.
   */
  frameRate?: number | null;
}

const VOICEVOX_FRAME_RATE = 24000 / 256;
const UPSPEAK_LENGTH = 0.15;

// `mora_phonemes_to_mora_kana` in voicevox_engine's
// `tts_pipeline/mora_mapping.py`, for the vowels an upspeak mora can repeat
// (the others have pitch 0).
const UPSPEAK_KANA: Readonly<Record<string, string>> = { a: 'ア', i: 'イ', u: 'ウ', e: 'エ', o: 'オ', N: 'ン' };

// numpy's `np.round`: halves go to the even neighbour.
function roundHalfToEven(value: number): number {
  const floor = Math.floor(value);
  const fraction = value - floor;
  if (fraction > 0.5) return floor + 1;
  if (fraction < 0.5) return floor;
  return floor % 2 === 0 ? floor : floor + 1;
}

function lengthOf(value: number | null | undefined, what: string): number {
  const length = value ?? 0;
  if (!Number.isFinite(length) || length < 0) {
    throw new Error(`timingFromVoicevox: ${what} must be a non-negative number, got ${value}`);
  }
  return length;
}

/**
 * Where each mora, phoneme, and pause of a VOICEVOX AudioQuery falls in the
 * WAV synthesized from it, in seconds from its first sample (including
 * `prePhonemeLength`). Pass the same query and options that synthesized the
 * voice. The result is plain data shaped like the query:
 * `accentPhrases[i].moras[j]` is `query.accent_phrases[i].moras[j]`.
 */
export function timingFromVoicevox(
  query: VoicevoxAudioQuery,
  options: VoicevoxTimingOptions = {},
): VoicevoxTiming {
  const speedScale = query.speedScale ?? 1;
  if (!Number.isFinite(speedScale) || speedScale <= 0) {
    throw new Error(`timingFromVoicevox: speedScale must be a positive number, got ${query.speedScale}`);
  }
  const pauseLengthScale = query.pauseLengthScale ?? 1;
  const upspeak = options.interrogativeUpspeak ?? true;
  const frameRate = options.frameRate === undefined ? VOICEVOX_FRAME_RATE : options.frameRate;
  if (frameRate !== null && (!Number.isFinite(frameRate) || frameRate <= 0)) {
    throw new Error(`timingFromVoicevox: frameRate must be a positive number or null, got ${frameRate}`);
  }
  // The clock counts engine frames when quantizing, so boundaries stay exact
  // integers until they become seconds; otherwise it counts seconds.
  const ticks = frameRate === null
    ? (length: number): number => length / speedScale
    : (length: number): number => roundHalfToEven((length / speedScale) * frameRate);
  const seconds = frameRate === null ? (clock: number): number => clock : (clock: number): number => clock / frameRate;

  let clock = 0;
  const phonemes: VoicevoxTimedPhoneme[] = [];
  const phoneme = (name: string, length: number): VoicevoxTimedPhoneme => {
    const start = seconds(clock);
    clock += ticks(length);
    const timed = { phoneme: name, start, end: seconds(clock) };
    phonemes.push(timed);
    return timed;
  };
  const mora = (source: VoicevoxMora, text: string, accentPhrase: number, isUpspeak: boolean): VoicevoxTimedMora => {
    const start = seconds(clock);
    const consonant = source.consonant
      ? phoneme(source.consonant, lengthOf(source.consonant_length, 'consonant_length'))
      : null;
    const vowel = phoneme(source.vowel, lengthOf(source.vowel_length, 'vowel_length'));
    return { text, start, end: vowel.end, consonant, vowel, accentPhrase, upspeak: isUpspeak };
  };

  const leading = phoneme('sil', lengthOf(query.prePhonemeLength, 'prePhonemeLength'));
  const accentPhrases = query.accent_phrases.map((phrase, index): VoicevoxTimedAccentPhrase => {
    const start = seconds(clock);
    const moras = phrase.moras.map((source) => mora(source, source.text ?? '', index, false));
    const last = phrase.moras[phrase.moras.length - 1];
    if (upspeak && phrase.is_interrogative && last !== undefined && (last.pitch ?? 0) > 0) {
      const text = UPSPEAK_KANA[last.vowel] ?? last.vowel;
      moras.push(mora({ vowel: last.vowel, vowel_length: UPSPEAK_LENGTH }, text, index, true));
    }
    const end = seconds(clock);
    let pause: VoicevoxTimedPause | null = null;
    if (phrase.pause_mora) {
      const base = query.pauseLength ?? lengthOf(phrase.pause_mora.vowel_length, 'pause_mora.vowel_length');
      const timed = phoneme('pau', lengthOf(base * pauseLengthScale, 'pause length'));
      pause = { text: phrase.pause_mora.text ?? '', start: timed.start, end: timed.end, accentPhrase: index };
    }
    return { text: moras.map((m) => m.text).join(''), start, end, moras, pause };
  });
  phoneme('sil', lengthOf(query.postPhonemeLength, 'postPhonemeLength'));

  const moras = accentPhrases.flatMap((phrase) => phrase.moras);
  const pauses = accentPhrases.flatMap((phrase) => (phrase.pause ? [phrase.pause] : []));
  const speech = moras.length > 0
    ? { start: moras[0].start, end: moras[moras.length - 1].end }
    : { start: leading.end, end: leading.end };
  return { exact: true, durationInSeconds: seconds(clock), speech, accentPhrases, moras, pauses, phonemes };
}
