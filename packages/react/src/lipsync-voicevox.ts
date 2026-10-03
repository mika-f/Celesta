// Builds a lip-sync track from a VOICEVOX Engine `audio_query` result.
//
// Unlike `loadLipSync`, which guesses where the vowels fall inside a WAV,
// an AudioQuery states the length of every consonant and vowel the engine
// will synthesize, so the mouth lines up with the voice exactly. Engines
// that share VOICEVOX's API (AivisSpeech, COEIROINK v2, ...) return the same
// shape; if one does not quantize to VOICEVOX's frame grid, pass its own
// `frameRate` (or `null`).
//
// The timeline mirrors `_query_to_decoder_feature` in voicevox_engine's
// `tts_pipeline/tts_engine.py`: optional interrogative upspeak, then
// `prePhonemeLength` silence, every accent phrase's moras followed by its
// `pause_mora`, `postPhonemeLength` silence; `pauseLength` /
// `pauseLengthScale` adjust only the pause moras, and `speedScale` divides
// everything. Each phoneme is then rounded (half to even, like numpy) to the
// engine's 93.75 Hz frame grid, which is what makes the result match the
// synthesized WAV to the sample; without it the error accumulates (about
// 50 ms over a 1.7 s line at `speedScale: 1.5`).

import { lipSyncFromKeyframes, type LipSyncTrack, type MouthKeyframe } from './lipsync';
import type { MouthShape } from './generated/MouthShape';

/** One mora of a VOICEVOX AudioQuery. Only the fields lip sync reads. */
export interface VoicevoxMora {
  text?: string;
  consonant?: string | null;
  consonant_length?: number | null;
  vowel: string;
  vowel_length: number;
  pitch?: number;
}

export interface VoicevoxAccentPhrase {
  moras: readonly VoicevoxMora[];
  pause_mora?: VoicevoxMora | null;
  is_interrogative?: boolean;
}

/**
 * The JSON returned by VOICEVOX Engine's `POST /audio_query` (and passed to
 * `POST /synthesis`). Fields lip sync does not need are allowed but ignored.
 */
export interface VoicevoxAudioQuery {
  accent_phrases: readonly VoicevoxAccentPhrase[];
  speedScale?: number;
  prePhonemeLength?: number;
  postPhonemeLength?: number;
  /** Newer engines: when not null, replaces the length of every pause mora. */
  pauseLength?: number | null;
  /** Newer engines: multiplies the length of every pause mora. */
  pauseLengthScale?: number;
}

export interface VoicevoxLipSyncOptions {
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

// numpy's `np.round`: halves go to the even neighbour.
function roundHalfToEven(value: number): number {
  const floor = Math.floor(value);
  const fraction = value - floor;
  if (fraction > 0.5) return floor + 1;
  if (fraction < 0.5) return floor;
  return floor % 2 === 0 ? floor : floor + 1;
}

// Consonants made by closing both lips. Showing the next vowel's open mouth
// during them looks like the lips never meet, so they stay `closed`; every
// other consonant anticipates its vowel, which is how the mouth actually
// moves into a mora.
const BILABIAL_CONSONANTS = new Set(['m', 'my', 'b', 'by', 'p', 'py']);

const UPSPEAK_LENGTH = 0.15;

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

function lengthOf(value: number | null | undefined, what: string): number {
  const length = value ?? 0;
  if (!Number.isFinite(length) || length < 0) {
    throw new Error(`lipSyncFromVoicevox: ${what} must be a non-negative number, got ${value}`);
  }
  return length;
}

/**
 * Converts a VOICEVOX AudioQuery into a `LipSyncTrack` for `<Dialogue
 * lipSync>`, `<CharacterView lipSync>` or `useLipSync`. Pass the same query
 * that synthesized the voice, so speed and pause edits are reflected. Time 0
 * is the start of the WAV, including `prePhonemeLength`.
 */
export function lipSyncFromVoicevox(
  query: VoicevoxAudioQuery,
  options: VoicevoxLipSyncOptions = {},
): LipSyncTrack {
  const speedScale = query.speedScale ?? 1;
  if (!Number.isFinite(speedScale) || speedScale <= 0) {
    throw new Error(`lipSyncFromVoicevox: speedScale must be a positive number, got ${query.speedScale}`);
  }
  const pauseLengthScale = query.pauseLengthScale ?? 1;
  const upspeak = options.interrogativeUpspeak ?? true;
  const frameRate = options.frameRate === undefined ? VOICEVOX_FRAME_RATE : options.frameRate;
  if (frameRate !== null && (!Number.isFinite(frameRate) || frameRate <= 0)) {
    throw new Error(`lipSyncFromVoicevox: frameRate must be a positive number or null, got ${frameRate}`);
  }
  // Lengths at 1x speed -> seconds in the WAV, quantized like the engine.
  const scaled = (length: number): number => {
    const seconds = length / speedScale;
    return frameRate === null ? seconds : roundHalfToEven(seconds * frameRate) / frameRate;
  };

  const keyframes: MouthKeyframe[] = [];
  let cursor = 0;
  const push = (length: number, mouth: MouthShape): void => {
    const seconds = scaled(length);
    if (seconds <= 0) return;
    keyframes.push({ seconds: cursor, mouth });
    cursor += seconds;
  };

  push(lengthOf(query.prePhonemeLength, 'prePhonemeLength'), 'closed');
  for (const phrase of query.accent_phrases) {
    const moras = [...phrase.moras];
    const last = moras[moras.length - 1];
    if (upspeak && phrase.is_interrogative && last !== undefined && (last.pitch ?? 0) > 0) {
      moras.push({ vowel: last.vowel, vowel_length: UPSPEAK_LENGTH });
    }
    for (const mora of moras) {
      const vowel = voicevoxVowelShape(mora.vowel);
      if (mora.consonant) {
        const consonant = BILABIAL_CONSONANTS.has(mora.consonant) ? 'closed' : vowel;
        push(lengthOf(mora.consonant_length, 'consonant_length'), consonant);
      }
      push(lengthOf(mora.vowel_length, 'vowel_length'), vowel);
    }
    if (phrase.pause_mora) {
      const base = query.pauseLength ?? lengthOf(phrase.pause_mora.vowel_length, 'pause_mora.vowel_length');
      push(lengthOf(base * pauseLengthScale, 'pause length'), 'closed');
    }
  }
  push(lengthOf(query.postPhonemeLength, 'postPhonemeLength'), 'closed');

  return lipSyncFromKeyframes(keyframes, cursor);
}
