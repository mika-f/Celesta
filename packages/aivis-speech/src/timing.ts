// Where every mora of an AivisSpeech AudioQuery falls in the WAV synthesized
// from it.
//
// AivisSpeech does not report phoneme lengths, so they are estimated. Two
// things are exact: the engine trims the model's own leading and trailing
// silence and then adds `prePhonemeLength / speedScale` and
// `postPhonemeLength / speedScale` seconds of silence (`synthesize_wave` in
// AivisSpeech-Engine's `style_bert_vits2_tts_engine.py`), so the voice spans
// exactly the rest of the WAV; and the query lists every mora the model
// speaks, in order. Inside the voice, the moras are aligned to the WAV's
// loudness by dynamic programming: punctuation may take any length and
// costs nothing while the WAV is silent, so it lands on the pauses the model
// made; spoken moras cost for the silence they cover (little at their start,
// where a stop's closure is silent) and for straying from their typical
// length, scaled to the line's speaking rate.

import {
  decodeWav,
  type SpeechAccentPhrase,
  type SpeechMora,
  type SpeechPause,
  type SpeechPhoneme,
  type SpeechTiming,
  type WavAudio,
} from '@celesta/character';

import type { AivisSpeechAudioQuery, AivisSpeechMora } from './query.js';

/**
 * A `SpeechTiming` estimated from the WAV (`exact: false`). Punctuation
 * moras stay in `moras` (vowel 'pau', `text` the mark) and are listed in
 * `pauses` too, together with any `pause_mora` (`text` ','), sentence-final
 * marks included; a mark the model did not pause at is short or empty.
 * `speech` is exact; the times inside it are estimated.
 */
export interface AivisSpeechTiming extends SpeechTiming {
  readonly exact: false;
}

// Alignment resolution: 10 ms, about VOICEVOX's phoneme frame.
const FRAMES_PER_SECOND = 100;

// Typical lengths in seconds at speedScale 1, averaged over VOICEVOX Engine
// queries for six voices. Only their proportions matter: the line's rate is
// fitted to the WAV.
const CONSONANT_SECONDS: ReadonlyMap<string, number> = new Map(Object.entries({
  b: 0.045, by: 0.05, d: 0.047, dy: 0.05, r: 0.036, ry: 0.06, w: 0.051, g: 0.053, gy: 0.06, n: 0.055,
  t: 0.053, ty: 0.06, m: 0.061, my: 0.065, p: 0.062, py: 0.065, k: 0.065, ky: 0.08, y: 0.07, v: 0.06,
  f: 0.08, h: 0.08, hy: 0.08, j: 0.071, z: 0.071, ch: 0.072, ny: 0.09, ts: 0.085, sh: 0.089, s: 0.092,
}));
const VOWEL_SECONDS: ReadonlyMap<string, number> = new Map(Object.entries({
  a: 0.084, i: 0.072, u: 0.06, e: 0.08, o: 0.079, N: 0.085, cl: 0.076, A: 0.05, I: 0.044, U: 0.057, E: 0.05, O: 0.05,
}));
const DEFAULT_CONSONANT_SECONDS = 0.06;
const DEFAULT_VOWEL_SECONDS = 0.075;
// っ and devoiced vowels are often silent.
const QUIET_VOWELS = new Set(['cl', 'A', 'I', 'U', 'E', 'O']);

// A frame is fully voiced VOICED_RANGE dB above the floor, which sits
// FLOOR_BELOW_PEAK dB under the line's loud frames, and silent below it.
const FLOOR_BELOW_PEAK = 30;
const VOICED_RANGE = 20;
// Cost of a silent frame inside a mora: cheap during its consonant (a stop's
// closure is silent) or a quiet vowel, full otherwise.
const CHEAP_SILENCE = 0.2;
const DURATION_WEIGHT = 1;
const RATE_PASSES = 2;

interface Unit {
  /** Punctuation or a pause mora: any length, free while silent. */
  readonly pause: boolean;
  readonly consonantSeconds: number;
  readonly vowelSeconds: number;
  readonly vowelSilence: number;
}

function unitOf(mora: AivisSpeechMora): Unit {
  if (mora.vowel === 'pau') {
    return { pause: true, consonantSeconds: 0, vowelSeconds: 0, vowelSilence: 0 };
  }
  const vowel = mora.vowel;
  return {
    pause: false,
    consonantSeconds: mora.consonant ? CONSONANT_SECONDS.get(mora.consonant) ?? DEFAULT_CONSONANT_SECONDS : 0,
    vowelSeconds: VOWEL_SECONDS.get(vowel) ?? DEFAULT_VOWEL_SECONDS,
    vowelSilence: QUIET_VOWELS.has(vowel) ? CHEAP_SILENCE : 1,
  };
}

function lengthOf(value: number | undefined, what: string): number {
  const length = value ?? 0;
  if (!Number.isFinite(length) || length < 0) {
    throw new Error(`timingFromAivisSpeech: ${what} must be a non-negative number, got ${value}`);
  }
  return length;
}

// How voiced each frame of the voice is, 0 (silent) to 1, from its RMS level
// relative to the line's loud frames.
function voicing(audio: WavAudio, start: number, end: number, frames: number): Float64Array {
  const levels = new Float64Array(frames);
  const first = start * audio.sampleRate;
  const step = ((end - start) * audio.sampleRate) / Math.max(1, frames);
  for (let f = 0; f < frames; f += 1) {
    const from = Math.max(0, Math.floor(first + f * step));
    const to = Math.min(audio.samples.length, Math.max(from + 1, Math.floor(first + (f + 1) * step)));
    let sum = 0;
    for (let i = from; i < to; i += 1) sum += audio.samples[i] * audio.samples[i];
    levels[f] = 10 * Math.log10(sum / Math.max(1, to - from) + 1e-12);
  }
  const sorted = Array.from(levels).sort((a, b) => a - b);
  const peak = sorted[Math.floor((sorted.length - 1) * 0.9)] ?? 0;
  const floor = Math.max(peak - FLOOR_BELOW_PEAK, -100);
  return levels.map((level) => Math.min(1, Math.max(0, (level - floor) / VOICED_RANGE)));
}

// The frame boundaries of each unit: `bounds[u]` to `bounds[u + 1]`.
function align(units: readonly Unit[], voiced: Float64Array): number[] {
  const frames = voiced.length;
  const silence = new Float64Array(frames + 1);
  const sound = new Float64Array(frames + 1);
  for (let f = 0; f < frames; f += 1) {
    silence[f + 1] = silence[f] + 1 - voiced[f];
    sound[f + 1] = sound[f] + voiced[f];
  }
  const typical = units.reduce((sum, unit) => sum + unit.consonantSeconds + unit.vowelSeconds, 0);
  let bounds = proportional(units, frames);
  if (typical === 0 || sound[frames] < 1) return bounds;
  // First guess: the voiced frames are the spoken moras.
  let rate = sound[frames] / typical;
  for (let pass = 0; pass < RATE_PASSES; pass += 1) {
    bounds = alignAtRate(units, silence, sound, rate);
    let spoken = 0;
    units.forEach((unit, u) => {
      if (!unit.pause) spoken += bounds[u + 1] - bounds[u];
    });
    rate = Math.max(1, spoken) / typical;
  }
  return bounds;
}

function alignAtRate(units: readonly Unit[], silence: Float64Array, sound: Float64Array, rate: number): number[] {
  const frames = silence.length - 1;
  const stride = frames + 1;
  const from = new Int32Array(units.length * stride);
  let previous = new Float64Array(stride).fill(Infinity);
  previous[0] = 0;
  units.forEach((unit, u) => {
    const current = new Float64Array(stride).fill(Infinity);
    const expected = (unit.consonantSeconds + unit.vowelSeconds) * rate;
    const consonant = Math.round(unit.consonantSeconds * rate);
    if (unit.pause) {
      // Any length, costing the sound it covers: keep the best start so far,
      // the latest one on a tie, so a pause takes no sound it can avoid.
      let bestStart = 0;
      for (let end = 0; end <= frames; end += 1) {
        if (previous[end] - sound[end] <= previous[bestStart] - sound[bestStart]) bestStart = end;
        current[end] = previous[bestStart] + sound[end] - sound[bestStart];
        from[u * stride + end] = bestStart;
      }
      previous = current;
      return;
    }
    // At least an even share of the voice, so the moras can always fill it.
    const longest = Math.max(Math.ceil(expected * 4) + 10, Math.ceil(frames / units.length));
    const stretched = new Float64Array(longest + 1);
    for (let length = 0; length <= longest; length += 1) {
      const stretch = Math.log((length + 0.5) / (expected + 0.5));
      stretched[length] = DURATION_WEIGHT * expected * stretch * stretch;
    }
    for (let end = 0; end <= frames; end += 1) {
      let best = Infinity;
      let bestStart = end;
      for (let length = 0; length <= Math.min(longest, end); length += 1) {
        const start = end - length;
        if (previous[start] === Infinity) continue;
        const split = Math.min(end, start + consonant);
        const cost = previous[start]
          + CHEAP_SILENCE * (silence[split] - silence[start])
          + unit.vowelSilence * (silence[end] - silence[split])
          + stretched[length];
        if (cost < best) {
          best = cost;
          bestStart = start;
        }
      }
      current[end] = best;
      from[u * stride + end] = bestStart;
    }
    previous = current;
  });
  const bounds = new Array<number>(units.length + 1);
  bounds[units.length] = frames;
  for (let u = units.length - 1; u >= 0; u -= 1) {
    bounds[u] = from[u * stride + bounds[u + 1]];
  }
  return bounds;
}

// Without sound to align to, spread the voice by typical lengths.
function proportional(units: readonly Unit[], frames: number): number[] {
  const typical = units.reduce((sum, unit) => sum + unit.consonantSeconds + unit.vowelSeconds, 0);
  const bounds = [0];
  let elapsed = 0;
  for (const unit of units) {
    elapsed += unit.consonantSeconds + unit.vowelSeconds;
    bounds.push(typical > 0 ? Math.round((elapsed / typical) * frames) : 0);
  }
  bounds[units.length] = frames;
  return bounds;
}

/**
 * Where each mora, phoneme, and pause of an AivisSpeech AudioQuery falls in
 * the WAV synthesized from it, in seconds from its first sample (including
 * `prePhonemeLength`). Pass the query that synthesized the voice and the
 * WAV's bytes (or `decodeWav`'s result). AivisSpeech does not report
 * phoneme lengths, so the times inside the voice are estimated from the
 * WAV; the voice's start and end are exact. The result is plain data shaped
 * like the query: `accentPhrases[i].moras[j]` is
 * `query.accent_phrases[i].moras[j]`.
 */
export function timingFromAivisSpeech(
  query: AivisSpeechAudioQuery,
  wav: Uint8Array | WavAudio,
): AivisSpeechTiming {
  const speedScale = query.speedScale ?? 1;
  if (!Number.isFinite(speedScale) || speedScale <= 0) {
    throw new Error(`timingFromAivisSpeech: speedScale must be a positive number, got ${query.speedScale}`);
  }
  const audio = wav instanceof Uint8Array ? decodeWav(wav) : wav;
  if (!(audio.sampleRate > 0)) {
    throw new Error(`timingFromAivisSpeech: the WAV's sample rate must be positive, got ${audio.sampleRate}`);
  }
  const durationInSeconds = audio.samples.length / audio.sampleRate;
  const pre = lengthOf(query.prePhonemeLength, 'prePhonemeLength') / speedScale;
  const post = lengthOf(query.postPhonemeLength, 'postPhonemeLength') / speedScale;
  const speechStart = Math.min(pre, durationInSeconds);

  // Every unit the model speaks, in order: each phrase's moras, then its pause mora.
  const sources: { mora: AivisSpeechMora; accentPhrase: number; pauseMora: boolean }[] = [];
  query.accent_phrases.forEach((phrase, index) => {
    for (const mora of phrase.moras) sources.push({ mora, accentPhrase: index, pauseMora: false });
    if (phrase.pause_mora) {
      sources.push({ mora: { ...phrase.pause_mora, text: ',', consonant: null, vowel: 'pau' }, accentPhrase: index, pauseMora: true });
    }
  });
  const units = sources.map(({ mora }) => unitOf(mora));
  // Without moras, the silence after the pre silence is all post silence.
  const speechEnd = units.length > 0 ? Math.max(speechStart, durationInSeconds - post) : speechStart;
  const frames = Math.round((speechEnd - speechStart) * FRAMES_PER_SECOND);
  const bounds = frames > 0 ? align(units, voicing(audio, speechStart, speechEnd, frames)) : units.map(() => 0).concat(0);
  const seconds = (frame: number): number =>
    frame >= frames ? speechEnd : speechStart + ((speechEnd - speechStart) * frame) / Math.max(1, frames);

  const phonemes: SpeechPhoneme[] = [];
  const phoneme = (name: string, start: number, end: number): SpeechPhoneme => {
    const timed = { phoneme: name, start, end };
    phonemes.push(timed);
    return timed;
  };
  phoneme('sil', 0, speechStart);
  const timedMoras: SpeechMora[][] = query.accent_phrases.map(() => []);
  const phrasePauses: (SpeechPause | null)[] = query.accent_phrases.map(() => null);
  const pauses: SpeechPause[] = [];
  sources.forEach(({ mora, accentPhrase, pauseMora }, u) => {
    const start = seconds(bounds[u]);
    const end = seconds(bounds[u + 1]);
    const unit = units[u];
    if (unit.pause) {
      const vowel = phoneme('pau', start, end);
      const pause = { text: pauseMora ? ',' : mora.text, start, end, accentPhrase };
      pauses.push(pause);
      if (pauseMora) {
        phrasePauses[accentPhrase] = pause;
        return;
      }
      timedMoras[accentPhrase].push({ text: mora.text, start, end, consonant: null, vowel, accentPhrase });
      return;
    }
    // The consonant takes its typical share of the mora.
    const split = start + ((end - start) * unit.consonantSeconds) / (unit.consonantSeconds + unit.vowelSeconds);
    const consonant = mora.consonant ? phoneme(mora.consonant, start, split) : null;
    const vowel = phoneme(mora.vowel, consonant ? split : start, end);
    timedMoras[accentPhrase].push({ text: mora.text, start, end, consonant, vowel, accentPhrase });
  });
  phoneme('sil', speechEnd, durationInSeconds);

  let cursor = speechStart;
  const accentPhrases = timedMoras.map((moras, index): SpeechAccentPhrase => {
    const start = moras.length > 0 ? moras[0].start : cursor;
    const end = moras.length > 0 ? moras[moras.length - 1].end : start;
    cursor = phrasePauses[index]?.end ?? end;
    return { text: moras.map((m) => m.text).join(''), start, end, moras, pause: phrasePauses[index] };
  });
  const moras = accentPhrases.flatMap((phrase) => phrase.moras);
  return { exact: false, durationInSeconds, speech: { start: speechStart, end: speechEnd }, accentPhrases, moras, pauses, phonemes };
}
