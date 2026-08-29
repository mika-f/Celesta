// Generates a lip-sync mouth track from a spoken WAV file plus its
// transcript, so a `<Character>` PSD/image portrait animates its mouth
// automatically instead of the composition hand-authoring a cue table.
//
// This is the TypeScript port of the editor's waveform-authored lip sync
// (`crates/editor/src/lib.rs`'s `lip_sync_cues_from_waveform` /
// `vowel_shapes`): an adaptive relative noise gate with hysteresis decides
// which moments are voiced, and the transcript's vowel sequence is spread
// across the voiced span. It runs at a fixed internal hop rate, so the
// result is independent of the composition's fps.
//
// Only uncompressed PCM/float WAV is supported (see the plan). Call
// `loadLipSync` from an entry's `prepare()` export and read the result
// through `useLipSync` or `<CharacterView lipSync={...}>`.

import * as fs from 'node:fs';

import { entryRelativePath } from './entry-dir';
import { useCurrentTime } from './hooks';
import type { MouthShape } from './generated/MouthShape';

const A_KANA = 'aAあぁゃかがさざただなはばぱまやらわアァャカガサザタダナハバパマヤラワ';
const I_KANA = 'iIいぃきぎしじちぢにひびぴみりイィキギシジチヂニヒビピミリ';
const U_KANA = 'uUうぅゅくぐすずつづぬふぶぷむゆるゔウゥュクグスズツヅヌフブプムユルヴ';
const E_KANA = 'eEえぇけげせぜてでねへべぺめれゑエェケゲセゼテデネヘベペメレヱ';
const O_KANA = 'oOおぉょこごそぞとどのほぼぽもよろをオォョコゴソゾトドノホボポモヨロヲ';
const SMALL_KANA = 'ぁぃぅぇぉゃゅょァィゥェォャュョ';

/**
 * Maps a transcript to its vowel-shape sequence. Small kana replace the
 * preceding vowel (`きゃ` → `a`) and `ー` repeats it. Verbatim port of
 * `vowel_shapes` in `crates/editor/src/lib.rs`.
 */
export function vowelShapes(text: string): Exclude<MouthShape, 'closed'>[] {
  const shapes: Exclude<MouthShape, 'closed'>[] = [];
  for (const character of text) {
    let shape: Exclude<MouthShape, 'closed'> | undefined;
    if (A_KANA.includes(character)) shape = 'a';
    else if (I_KANA.includes(character)) shape = 'i';
    else if (U_KANA.includes(character)) shape = 'u';
    else if (E_KANA.includes(character)) shape = 'e';
    else if (O_KANA.includes(character)) shape = 'o';

    if (shape === undefined) {
      if (character === 'ー' && shapes.length > 0) {
        shapes.push(shapes[shapes.length - 1]);
      }
      continue;
    }
    if (SMALL_KANA.includes(character) && shapes.length > 0) {
      shapes[shapes.length - 1] = shape;
    } else {
      shapes.push(shape);
    }
  }
  return shapes;
}

export interface WavAudio {
  sampleRate: number;
  /** Down-mixed to mono, normalized to roughly [-1, 1]. */
  samples: Float32Array;
}

function readChunks(view: DataView): Map<string, { offset: number; size: number }> {
  const chunks = new Map<string, { offset: number; size: number }>();
  let offset = 12; // past "RIFF" <size> "WAVE"
  while (offset + 8 <= view.byteLength) {
    const id = String.fromCharCode(
      view.getUint8(offset),
      view.getUint8(offset + 1),
      view.getUint8(offset + 2),
      view.getUint8(offset + 3),
    );
    const size = view.getUint32(offset + 4, true);
    chunks.set(id, { offset: offset + 8, size });
    offset += 8 + size + (size & 1); // chunks are word-aligned
  }
  return chunks;
}

/** Parses an uncompressed PCM or IEEE-float WAV into mono float samples. */
export function decodeWav(bytes: Uint8Array): WavAudio {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const riff = String.fromCharCode(view.getUint8(0), view.getUint8(1), view.getUint8(2), view.getUint8(3));
  const wave = String.fromCharCode(view.getUint8(8), view.getUint8(9), view.getUint8(10), view.getUint8(11));
  if (riff !== 'RIFF' || wave !== 'WAVE') {
    throw new Error('not a RIFF/WAVE file');
  }
  const chunks = readChunks(view);
  const fmt = chunks.get('fmt ');
  const data = chunks.get('data');
  if (!fmt || !data) {
    throw new Error('WAV file is missing its fmt or data chunk');
  }
  let audioFormat = view.getUint16(fmt.offset, true);
  const channels = view.getUint16(fmt.offset + 2, true);
  const sampleRate = view.getUint32(fmt.offset + 4, true);
  const bits = view.getUint16(fmt.offset + 14, true);
  if (audioFormat === 0xfffe && fmt.size >= 26) {
    // WAVE_FORMAT_EXTENSIBLE: the real format is the first 2 bytes of the
    // sub-format GUID at byte 24 of the fmt chunk.
    audioFormat = view.getUint16(fmt.offset + 24, true);
  }

  const bytesPerSample = bits >> 3;
  const frameCount = Math.floor(data.size / (bytesPerSample * channels));
  const samples = new Float32Array(frameCount);
  let cursor = data.offset;
  const readOne = (): number => {
    let value: number;
    if (audioFormat === 3) {
      value = bits === 64 ? view.getFloat64(cursor, true) : view.getFloat32(cursor, true);
    } else if (bits === 8) {
      value = (view.getUint8(cursor) - 128) / 128;
    } else if (bits === 16) {
      value = view.getInt16(cursor, true) / 0x8000;
    } else if (bits === 24) {
      const b0 = view.getUint8(cursor);
      const b1 = view.getUint8(cursor + 1);
      const b2 = view.getUint8(cursor + 2);
      let int24 = b0 | (b1 << 8) | (b2 << 16);
      if (int24 & 0x800000) int24 -= 0x1000000;
      value = int24 / 0x800000;
    } else if (bits === 32) {
      value = view.getInt32(cursor, true) / 0x80000000;
    } else {
      throw new Error(`unsupported WAV sample size: ${bits}-bit (format ${audioFormat})`);
    }
    cursor += bytesPerSample;
    return value;
  };
  for (let frame = 0; frame < frameCount; frame += 1) {
    let sum = 0;
    for (let channel = 0; channel < channels; channel += 1) {
      sum += readOne();
    }
    samples[frame] = sum / channels;
  }
  return { sampleRate, samples };
}

const DEFAULT_HOP_HZ = 100;

/** Per-hop peak amplitude — the "clip-local peak envelope" the editor gate consumes. */
export function buildEnvelope(audio: WavAudio, hopHz = DEFAULT_HOP_HZ): Float32Array {
  const hop = Math.max(1, Math.round(audio.sampleRate / hopHz));
  const hopCount = Math.max(1, Math.ceil(audio.samples.length / hop));
  const envelope = new Float32Array(hopCount);
  for (let i = 0; i < hopCount; i += 1) {
    let peak = 0;
    const end = Math.min(audio.samples.length, (i + 1) * hop);
    for (let j = i * hop; j < end; j += 1) {
      const amplitude = Math.abs(audio.samples[j]);
      if (amplitude > peak) peak = amplitude;
    }
    envelope[i] = peak;
  }
  return envelope;
}

export interface LipSyncOptions {
  /** WAV path, resolved relative to the entry file's directory. */
  src: string;
  /** Transcript. Its vowel sequence is spread across the voiced span. */
  text: string;
  /** Internal analysis rate. Higher = finer mouth timing. Defaults to 100 Hz. */
  hopHz?: number;
}

export interface LipSyncTrack {
  readonly durationInSeconds: number;
  mouthAtSeconds(seconds: number): MouthShape;
  mouthAtFrame(frame: number, fps: number): MouthShape;
}

/**
 * Builds a per-hop mouth-shape timeline. Port of the gate + vowel-mapping in
 * `lip_sync_cues_from_waveform`: adaptive open threshold
 * `max(peak * 0.18, 0.015)`, close threshold `open * 0.6`, hysteresis so the
 * mouth does not chatter, then the transcript's vowels distributed evenly
 * across the voiced hops.
 */
export function lipSyncTimeline(envelope: Float32Array, text: string): MouthShape[] {
  let peak = 0;
  for (const value of envelope) {
    if (Number.isFinite(value) && value > peak) peak = value;
  }
  const openThreshold = Math.max(peak * 0.18, 0.015);
  const closeThreshold = openThreshold * 0.6;

  const voiced: boolean[] = [];
  let isVoiced = false;
  for (let i = 0; i < envelope.length; i += 1) {
    const amplitude = envelope[i];
    if (!isVoiced && amplitude >= openThreshold) isVoiced = true;
    else if (isVoiced && amplitude <= closeThreshold) isVoiced = false;
    voiced.push(isVoiced);
  }

  const vowels = vowelShapes(text);
  const voicedCount = voiced.reduce((count, v) => count + (v ? 1 : 0), 0);
  const timeline: MouthShape[] = [];
  let voicedIndex = 0;
  for (const v of voiced) {
    if (!v) {
      timeline.push('closed');
      continue;
    }
    if (vowels.length === 0) {
      timeline.push('a');
    } else {
      const index = Math.min(
        Math.floor((voicedIndex * vowels.length) / Math.max(1, voicedCount)),
        vowels.length - 1,
      );
      timeline.push(vowels[index]);
    }
    voicedIndex += 1;
  }
  return timeline;
}

function makeTrack(timeline: MouthShape[], hopHz: number): LipSyncTrack {
  const durationInSeconds = timeline.length / hopHz;
  const sample = (seconds: number): MouthShape => {
    if (timeline.length === 0 || !Number.isFinite(seconds) || seconds < 0) {
      return 'closed';
    }
    const index = Math.min(timeline.length - 1, Math.floor(seconds * hopHz));
    return timeline[index];
  };
  return {
    durationInSeconds,
    mouthAtSeconds: sample,
    mouthAtFrame: (frame, fps) => sample(fps > 0 ? frame / fps : 0),
  };
}

/**
 * Reads a WAV file and builds its lip-sync track. Call from an entry's
 * `prepare()` export and stash the result in module state (like
 * `homepage-demo.tsx` does with fetched data), then read it per frame with
 * `useLipSync` or by passing it to `<CharacterView lipSync={...}>`.
 */
export async function loadLipSync(options: LipSyncOptions): Promise<LipSyncTrack> {
  const hopHz = options.hopHz ?? DEFAULT_HOP_HZ;
  const bytes = await fs.promises.readFile(entryRelativePath(options.src));
  const audio = decodeWav(bytes);
  const envelope = buildEnvelope(audio, hopHz);
  const timeline = lipSyncTimeline(envelope, options.text);
  return makeTrack(timeline, hopHz);
}

/** The mouth shape for the current composition frame from a `LipSyncTrack`. */
export function useLipSync(track: LipSyncTrack): MouthShape {
  const time = useCurrentTime();
  return track.mouthAtSeconds(time.value / time.timescale);
}

/**
 * Like `useLipSync` but tolerates a missing track (returns `undefined`),
 * so `<CharacterView>` can call it unconditionally whether or not a
 * `lipSync` prop was given.
 */
export function useOptionalLipSync(track: LipSyncTrack | undefined): MouthShape | undefined {
  const time = useCurrentTime();
  return track ? track.mouthAtSeconds(time.value / time.timescale) : undefined;
}
