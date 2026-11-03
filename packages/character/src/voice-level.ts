// Preparation-time voice level matching: measures each line's local WAV once
// in `prepare()` and multiplies the line's own `volume` by a constant gain
// that brings its speech toward a target level, so recordings from different
// speakers or engines sit at a similar level without hand-tuning each line.
//
// The metric is deliberately simple and documented, not a loudness standard:
//
// - Blocks of BLOCK_SECONDS are measured on every original channel; a
//   block's power is the mean of its channels' mean squares, so opposite
//   polarity channels never cancel the way a mono down-mix would.
// - Blocks under ABSOLUTE_GATE_DBFS are silence. Of the rest, blocks more
//   than RELATIVE_GATE_DB below their average power (breaths, room tone,
//   tails) are dropped too.
// - "Speech RMS" is the remaining blocks' average power in dBFS (0 dBFS is a
//   full-scale square wave). It is not integrated LUFS: there is no
//   K-weighting and no perceptual model.
// - The peak is the largest absolute sample over every channel (sample peak,
//   not true peak).
//
// Matching only changes gain metadata. The source file, its duration, lip
// sync, and the plan's timing are untouched, and nothing is decoded per
// frame.

import { createHash } from 'node:crypto';
import * as fs from 'node:fs';

import type { AnimatedNumber } from '@celesta.studio/react';
import { entryRelativePath, isRemoteUrl } from '@celesta.studio/react/internal';

import type { DialogueLine } from './dialogue-series';
import { decodeWavChannels } from './lipsync';
import type { WavChannels } from './lipsync';

/** Bumped whenever the analysis below changes, so cached results from older code are not reused. */
export const VOICE_LEVEL_ANALYSIS_VERSION = 1;

const BLOCK_SECONDS = 0.02;
const ABSOLUTE_GATE_DBFS = -60;
const RELATIVE_GATE_DB = 20;
const MIN_SPEECH_SECONDS = 0.1;

const DEFAULT_TARGET_RMS_DBFS = -20;
const DEFAULT_PEAK_CEILING_DBFS = -1;
const DEFAULT_MAX_BOOST_DB = 12;
const DEFAULT_MAX_CUT_DB = 12;

/** What `analyzeVoiceLevel()` measured in one recording. */
export interface VoiceLevelAnalysis {
  /** `VOICE_LEVEL_ANALYSIS_VERSION` of the code that measured it. */
  version: number;
  sampleRate: number;
  channelCount: number;
  durationSeconds: number;
  /**
   * `'ok'` when at least 0.1 s of blocks pass the gates; `'silent'` when no
   * block reaches -60 dBFS; `'insufficient-speech'` when some do but less
   * than 0.1 s in total (too short to trust).
   */
  status: 'ok' | 'silent' | 'insufficient-speech';
  /** Seconds of audio that passed both gates. */
  speechSeconds: number;
  /** Gated speech RMS in dBFS; `null` unless `status` is `'ok'`. */
  rmsDbfs: number | null;
  /** Largest absolute sample over every channel, in dBFS; `null` for an all-zero file. */
  peakDbfs: number | null;
  /** Start of the first block that passed the gates, in seconds; `null` when none did. */
  speechStartSeconds: number | null;
  /** End of the last block that passed the gates, in seconds; `null` when none did. */
  speechEndSeconds: number | null;
}

/** Per-speaker settings for `matchVoiceLevels()`, keyed by `DialogueLine.speaker`. */
export interface VoiceLevelSpeaker {
  /** Replaces the global `targetRmsDbfs` for this speaker. */
  targetRmsDbfs?: number;
  /** Added to the speaker's target, e.g. -1 to sit a little behind the others. */
  offsetDb?: number;
}

/**
 * `DialogueLine.voiceLevel`: how `matchVoiceLevels()` treats one line.
 *
 * - `false` leaves the line exactly as authored (no analysis).
 * - `{ targetRmsDbfs }` replaces the speaker's target (and its offset) for this
 *   line; `{ offsetDb }` is added to whichever target applies. Bounds still apply.
 * - `{ gainDb }` uses this gain instead of a measured one (no analysis, no bounds).
 */
export type VoiceLevelLineSetting =
  | false
  | { offsetDb?: number; targetRmsDbfs?: number; gainDb?: undefined }
  | { gainDb: number; offsetDb?: undefined; targetRmsDbfs?: undefined };

export interface VoiceLevelOptions {
  /** Gated speech RMS every line is moved toward, in dBFS. Defaults to -20. */
  targetRmsDbfs?: number;
  /**
   * Highest sample peak the matching gain may produce, in dBFS. Defaults
   * to -1. This is the hardest bound: a recording that already peaks above
   * it is turned down even past `maxCutDb`.
   */
  peakCeilingDbfs?: number;
  /** Largest boost, in dB. Defaults to 12. */
  maxBoostDb?: number;
  /** Largest cut, in dB (a positive number). Defaults to 12. */
  maxCutDb?: number;
  /** Targets and offsets per `DialogueLine.speaker`. */
  speakers?: Record<string, VoiceLevelSpeaker>;
}

/** Which bound kept a line's gain from reaching its target. */
export type VoiceLevelLimit = 'max-boost' | 'max-cut' | 'peak-ceiling';

export interface VoiceLevelLineReport {
  id: string;
  index: number;
  speaker?: string;
  /** The line's `audio`, as written. */
  src?: string;
  /**
   * - `'matched'`: the gain reaches the target.
   * - `'limited'`: a bound in `limitedBy` stopped the gain short of the target.
   * - `'silent'` / `'insufficient-speech'`: nothing reliable to measure; gain is 0 dB.
   * - `'fixed-gain'`: the line's `voiceLevel.gainDb` was used.
   * - `'opted-out'`: `voiceLevel: false`.
   * - `'no-audio'` / `'muted'`: nothing is heard; left unchanged.
   */
  status: 'matched' | 'limited' | 'silent' | 'insufficient-speech' | 'fixed-gain' | 'opted-out' | 'no-audio' | 'muted';
  /** The measurement, when the line was analyzed. */
  analysis?: VoiceLevelAnalysis;
  /** Whether `analysis` came from the cache (an earlier call, or an earlier line using the same file content). */
  cached: boolean;
  /** The line's target after speaker and line offsets, in dBFS. */
  targetRmsDbfs?: number;
  /** `targetRmsDbfs - analysis.rmsDbfs`, before bounds. */
  desiredGainDb?: number;
  /** The gain applied to the line's volume, in dB. 0 for lines left unchanged. */
  gainDb: number;
  /** `gainDb` as the linear factor `volume` was multiplied by. */
  gain: number;
  /** Bounds that changed the gain, in the order they were applied. Empty when none did. */
  limitedBy: VoiceLevelLimit[];
  /** `analysis.rmsDbfs + gainDb`: the source after matching, before the line's own volume and the mix. */
  rmsAfterDbfs?: number | null;
  /** `analysis.peakDbfs + gainDb`, measured the same way. */
  peakAfterDbfs?: number | null;
}

export interface VoiceLevelReport {
  analysisVersion: number;
  /** The options with defaults filled in. */
  options: Required<Omit<VoiceLevelOptions, 'speakers'>> & { speakers: Record<string, VoiceLevelSpeaker> };
  /** One entry per input line, in script order. */
  lines: VoiceLevelLineReport[];
}

export interface MatchedVoiceLevels<L extends DialogueLine> {
  /** The lines with `volume` multiplied by their gain; lines left unchanged are the same objects. */
  lines: L[];
  /**
   * What was measured and applied. Levels here describe the source
   * recordings and the matching gain only: the line's own `volume`, BGM,
   * and any master gain come on top and are not reflected.
   */
  report: VoiceLevelReport;
}

const MAX_CACHE_ENTRIES = 256;
const cache = new Map<string, Promise<VoiceLevelAnalysis>>();

/**
 * Measures one local PCM/float WAV with the gated speech RMS and sample
 * peak described in this module. `src` is resolved from the entry file like
 * `<Audio src>`. Results are cached by file content and analysis version,
 * so re-saving a file at the same path is measured again and an unchanged
 * file is not.
 */
export async function analyzeVoiceLevel(src: string): Promise<VoiceLevelAnalysis> {
  return (await analyzeCached(src)).analysis;
}

async function analyzeCached(src: string): Promise<{ analysis: VoiceLevelAnalysis; cached: boolean }> {
  if (isRemoteUrl(src)) {
    throw new Error(`voice level analysis needs a local WAV file; ${src} is a URL`);
  }
  const path = entryRelativePath(src);
  let bytes: Buffer;
  try {
    bytes = await fs.promises.readFile(path);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`could not read ${src} (looked at ${path}): ${message}`);
  }
  const key = `${VOICE_LEVEL_ANALYSIS_VERSION}:${createHash('sha256').update(bytes).digest('hex')}`;
  const existing = cache.get(key);
  if (existing) {
    // Refresh its place so the most recently used entries survive trimming.
    cache.delete(key);
    cache.set(key, existing);
    return { analysis: await existing, cached: true };
  }
  const pending = Promise.resolve().then(() => {
    try {
      return measureVoiceLevel(decodeWavChannels(bytes));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      throw new Error(`could not analyze ${src}: ${message}`);
    }
  });
  cache.set(key, pending);
  pending.catch(() => cache.delete(key));
  while (cache.size > MAX_CACHE_ENTRIES) {
    cache.delete(cache.keys().next().value!);
  }
  return { analysis: await pending, cached: false };
}

/** The analysis behind `analyzeVoiceLevel()`, on already decoded samples. */
export function measureVoiceLevel(audio: WavChannels): VoiceLevelAnalysis {
  const { sampleRate, channels } = audio;
  const frameCount = channels[0].length;
  const blockFrames = Math.max(1, Math.round(sampleRate * BLOCK_SECONDS));

  let peak = 0;
  const blocks: { start: number; frames: number; power: number }[] = [];
  for (let start = 0; start < frameCount; start += blockFrames) {
    const end = Math.min(frameCount, start + blockFrames);
    let power = 0;
    for (const channel of channels) {
      let sum = 0;
      for (let i = start; i < end; i += 1) {
        const value = channel[i];
        if (!Number.isFinite(value)) continue;
        sum += value * value;
        const magnitude = Math.abs(value);
        if (magnitude > peak) peak = magnitude;
      }
      power += sum / (end - start);
    }
    blocks.push({ start, frames: end - start, power: power / channels.length });
  }

  const absoluteGate = dbToPower(ABSOLUTE_GATE_DBFS);
  const loud = blocks.filter((block) => block.power >= absoluteGate);
  const relativeGate = loud.length === 0 ? Infinity : averagePower(loud) * dbToPower(-RELATIVE_GATE_DB);
  const speech = loud.filter((block) => block.power >= relativeGate);
  const speechFrames = speech.reduce((total, block) => total + block.frames, 0);
  const speechSeconds = speechFrames / sampleRate;
  const status = loud.length === 0 ? 'silent' : speechSeconds + 1e-9 < MIN_SPEECH_SECONDS ? 'insufficient-speech' : 'ok';
  const first = speech[0];
  const last = speech[speech.length - 1];
  return {
    version: VOICE_LEVEL_ANALYSIS_VERSION,
    sampleRate,
    channelCount: channels.length,
    durationSeconds: frameCount / sampleRate,
    status,
    speechSeconds,
    // A full-scale square wave has mean square 1, so its RMS is 0 dBFS.
    rmsDbfs: status === 'ok' ? 10 * Math.log10(averagePower(speech)) : null,
    peakDbfs: peak > 0 ? 20 * Math.log10(peak) : null,
    speechStartSeconds: first ? first.start / sampleRate : null,
    speechEndSeconds: last ? (last.start + last.frames) / sampleRate : null,
  };
}

function averagePower(blocks: readonly { frames: number; power: number }[]): number {
  let weighted = 0;
  let frames = 0;
  for (const block of blocks) {
    weighted += block.power * block.frames;
    frames += block.frames;
  }
  return weighted / frames;
}

function dbToPower(db: number): number {
  return 10 ** (db / 10);
}

function dbToGain(db: number): number {
  return 10 ** (db / 20);
}

/**
 * Multiplies a volume by `gain`, keeping its shape: a number is scaled, and
 * every keyframe's value is scaled with its time and easing untouched (so
 * two keys on one time stay a cut, and sequence-local times stay local).
 * `undefined` is treated as the default volume of 1.
 */
export function scaleVolume(volume: AnimatedNumber | undefined, gain: number): AnimatedNumber {
  if (!Number.isFinite(gain) || gain < 0) {
    throw new Error(`scaleVolume() requires a finite gain >= 0, got ${gain}`);
  }
  if (volume === undefined) {
    return gain;
  }
  if (typeof volume === 'number') {
    return volume * gain;
  }
  return { ...volume, keyframes: volume.keyframes.map((key) => ({ ...key, value: key.value * gain })) };
}

/**
 * Brings every line's voice toward a common speech level. Opt-in: call it in
 * `prepare()` before `planDialogue()` and pass its `lines` on; nothing
 * changes for scripts that do not call it.
 *
 * Each line with a local WAV `audio` is measured (once per file content) and
 * gets a constant gain of `target - rms`, clamped to `maxBoostDb` /
 * `maxCutDb` and then to whatever keeps the sample peak under
 * `peakCeilingDbfs`. The gain multiplies the line's `volume` — a number or
 * keyframes — so authored fades keep their timing. Volumes above 1 authored
 * on the line still push past the ceiling; the ceiling covers the matching
 * gain only.
 *
 * ```ts
 * const matched = await matchVoiceLevels(script, {
 *   targetRmsDbfs: -20,
 *   speakers: { aoi: { offsetDb: -1 } },
 * });
 * plan = await planDialogue(matched.lines, { fps: 30 });
 * ```
 */
export async function matchVoiceLevels<L extends DialogueLine>(
  lines: readonly L[],
  options: VoiceLevelOptions = {},
): Promise<MatchedVoiceLevels<L>> {
  const resolved = {
    targetRmsDbfs: finite(options.targetRmsDbfs ?? DEFAULT_TARGET_RMS_DBFS, 'targetRmsDbfs'),
    peakCeilingDbfs: finite(options.peakCeilingDbfs ?? DEFAULT_PEAK_CEILING_DBFS, 'peakCeilingDbfs'),
    maxBoostDb: nonNegative(options.maxBoostDb ?? DEFAULT_MAX_BOOST_DB, 'maxBoostDb'),
    maxCutDb: nonNegative(options.maxCutDb ?? DEFAULT_MAX_CUT_DB, 'maxCutDb'),
    speakers: options.speakers ?? {},
  };
  for (const [name, speaker] of Object.entries(resolved.speakers)) {
    if (speaker.targetRmsDbfs !== undefined) finite(speaker.targetRmsDbfs, `speakers.${name}.targetRmsDbfs`);
    if (speaker.offsetDb !== undefined) finite(speaker.offsetDb, `speakers.${name}.offsetDb`);
  }

  const results = await Promise.all(
    lines.map(async (line, index): Promise<{ line: L; report: VoiceLevelLineReport }> => {
      const id = line.id ?? String(index);
      const base = { id, index, speaker: line.speaker, src: line.audio, cached: false, gainDb: 0, gain: 1, limitedBy: [] };
      const setting = line.voiceLevel;
      if (setting === false) return { line, report: { ...base, status: 'opted-out' } };
      if (line.audio === undefined) return { line, report: { ...base, status: 'no-audio' } };
      if (line.muted) return { line, report: { ...base, status: 'muted' } };
      if (setting?.gainDb !== undefined) {
        const gainDb = finite(setting.gainDb, `line "${id}"'s voiceLevel.gainDb`);
        const gain = dbToGain(gainDb);
        return { line: { ...line, volume: scaleVolume(line.volume, gain) }, report: { ...base, status: 'fixed-gain', gainDb, gain } };
      }

      let analyzed: { analysis: VoiceLevelAnalysis; cached: boolean };
      try {
        analyzed = await analyzeCached(line.audio);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`matchVoiceLevels(): line "${id}": ${message}. Set \`voiceLevel: false\` on the line to skip it.`);
      }
      const { analysis, cached } = analyzed;
      const speaker = line.speaker === undefined ? undefined : resolved.speakers[line.speaker];
      // A line's own target replaces the speaker's (offset included); its offset adds to either.
      const targetRmsDbfs =
        (setting?.targetRmsDbfs !== undefined
          ? finite(setting.targetRmsDbfs, `line "${id}"'s voiceLevel.targetRmsDbfs`)
          : (speaker?.targetRmsDbfs ?? resolved.targetRmsDbfs) + (speaker?.offsetDb ?? 0))
        + finite(setting?.offsetDb ?? 0, `line "${id}"'s voiceLevel.offsetDb`);
      if (analysis.rmsDbfs === null) {
        return {
          line,
          report: {
            ...base, status: analysis.status as 'silent' | 'insufficient-speech', analysis, cached, targetRmsDbfs,
            rmsAfterDbfs: null, peakAfterDbfs: analysis.peakDbfs,
          },
        };
      }

      const desiredGainDb = targetRmsDbfs - analysis.rmsDbfs;
      const limitedBy: VoiceLevelLimit[] = [];
      let gainDb = desiredGainDb;
      if (gainDb > resolved.maxBoostDb) {
        gainDb = resolved.maxBoostDb;
        limitedBy.push('max-boost');
      } else if (gainDb < -resolved.maxCutDb) {
        gainDb = -resolved.maxCutDb;
        limitedBy.push('max-cut');
      }
      // rmsDbfs is non-null, so some sample is non-zero and peakDbfs is too.
      const headroom = resolved.peakCeilingDbfs - analysis.peakDbfs!;
      if (gainDb > headroom) {
        gainDb = headroom;
        limitedBy.push('peak-ceiling');
      }
      const gain = dbToGain(gainDb);
      return {
        line: gain === 1 ? line : { ...line, volume: scaleVolume(line.volume, gain) },
        report: {
          ...base,
          status: limitedBy.length === 0 ? 'matched' : 'limited',
          analysis,
          cached,
          targetRmsDbfs,
          desiredGainDb,
          gainDb,
          gain,
          limitedBy,
          rmsAfterDbfs: analysis.rmsDbfs + gainDb,
          peakAfterDbfs: analysis.peakDbfs! + gainDb,
        },
      };
    }),
  );

  return {
    lines: results.map((result) => result.line),
    report: {
      analysisVersion: VOICE_LEVEL_ANALYSIS_VERSION,
      options: resolved,
      lines: results.map((result) => result.report),
    },
  };
}

function finite(value: number, what: string): number {
  if (!Number.isFinite(value)) {
    throw new Error(`matchVoiceLevels(): ${what} must be a finite number of dB, got ${value}`);
  }
  return value;
}

function nonNegative(value: number, what: string): number {
  if (!Number.isFinite(value) || value < 0) {
    throw new Error(`matchVoiceLevels(): ${what} must be a finite number of dB >= 0, got ${value}`);
  }
  return value;
}
