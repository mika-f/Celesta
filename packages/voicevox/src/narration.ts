// Synthesizes a script's voices with a local VOICEVOX Engine during an
// entry's `prepare()`, caching every line on disk.
//
// A line's cache key covers only what changes the sound: the spoken text,
// the style, the query settings, the synthesis flags, and the revisions the
// author sets. Subtitle text, expression, order, gaps and scenes are not in
// it, so editing them reuses the voice. A cache hit reads files only, so the
// engine does not have to be running to reopen or export an unchanged
// script; the engine is asked for nothing until a line misses.
//
// Each entry is `<key>.wav` and `<key>.json`. The WAV is written first and
// the JSON, which records the WAV's size and hash, last; both go through a
// temporary file and a rename, so a crash or a failed request never leaves
// an entry that reads as valid. An entry whose files are missing, do not
// match, or do not decode is synthesized again.
//
// Nothing here writes to stdout, which carries the bridge protocol.

import { createHash } from 'node:crypto';
import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import * as path from 'node:path';

import { readWavHeader } from '@celesta/character';
import type { DialogueLine } from '@celesta/character';

import { lipSyncFromVoicevox } from './lipsync.js';
import type { VoicevoxAudioQuery } from './query.js';

/** Bumped when the key or the entry layout changes; old entries then miss. */
const CACHE_FORMAT = 1;
const DEFAULT_CACHE_DIR = './.celesta/voices';
const DEFAULT_CONCURRENCY = 2;
const DEFAULT_TIMEOUT_MS = 60_000;

/** The top-level AudioQuery fields a voice may set over the engine's values. */
export interface VoicevoxQuerySettings {
  speedScale?: number;
  pitchScale?: number;
  intonationScale?: number;
  volumeScale?: number;
  prePhonemeLength?: number;
  postPhonemeLength?: number;
  pauseLength?: number | null;
  pauseLengthScale?: number;
  outputSamplingRate?: number;
  outputStereo?: boolean;
}

/** What a line is synthesized with. Every field is part of the cache key. */
export interface VoicevoxVoiceSettings {
  /** The engine's style id (`/speakers` → `styles[].id`), e.g. 3 for Zundamon's normal style. */
  styleId: number;
  /** Set over the query `audio_query` returns before `/synthesis`. */
  query?: VoicevoxQuerySettings;
  /**
   * `/synthesis`'s `enable_interrogative_upspeak`; the lip sync uses the same
   * value. Defaults to true, like the engine.
   */
  interrogativeUpspeak?: boolean;
  /** `/audio_query`'s `enable_katakana_english`. Left to the engine when unset. */
  katakanaEnglish?: boolean;
  /**
   * Any string; changing it synthesizes this voice's lines again. Bump it
   * after editing the engine's user dictionary or updating the engine or
   * its voice models, which the cache cannot see.
   */
  revision?: string;
}

/** A speaker of `prepareNarration()`: where its engine is and how it sounds. */
export interface VoicevoxVoice extends VoicevoxVoiceSettings {
  /** The engine to synthesize with, e.g. `http://127.0.0.1:50021`. Not part of the cache key. */
  baseUrl: string;
}

/** One script line for `prepareNarration()`; every other field is passed through. */
export interface NarrationLine extends DialogueLine {
  /** Names the line in errors and in the plan. Must be unique. */
  id: string;
  /** Key into `options.speakers`. Optional when the line has its own `audio`. */
  speaker?: string;
  // `spokenText` (from `DialogueLine`) is what the engine reads; it defaults to `text`.
  /** Overrides the speaker's settings for this line; `query` is merged field by field. */
  voice?: Partial<VoicevoxVoiceSettings>;
}

/** What `prepareNarration()` made for a line, beside its `audio` and `lipSync`. */
export interface NarrationAsset {
  /** Absolute path of the WAV; the same as the line's `audio`. */
  audio: string;
  /** Absolute path of the entry's JSON, which holds `query`. */
  metadata: string;
  /** Seconds, measured from the WAV's samples. */
  duration: number;
  /**
   * The full query the WAV was synthesized from, including fields Celesta
   * does not read. `timingFromVoicevox(query, { interrogativeUpspeak })`
   * gives the mora times.
   */
  query: VoicevoxAudioQuery & Record<string, unknown>;
  /** The text the engine read. */
  spokenText: string;
  /** The synthesis flag the lip sync was built with. */
  interrogativeUpspeak: boolean;
  /** The cache key (the entry's file name). */
  key: string;
  /** True when the entry was already on disk. */
  cached: boolean;
}

/** A line as `prepareNarration()` returns it: ready for `planDialogue()`. */
export type NarratedLine<L extends NarrationLine = NarrationLine> = L & {
  audio: string;
  /** Absent on lines that brought their own `audio`, which keep their own `lipSync`. */
  narration?: NarrationAsset;
};

export interface PrepareNarrationOptions {
  /** Voices by the lines' `speaker`. */
  speakers: Readonly<Record<string, VoicevoxVoice>>;
  /**
   * Where entries are kept, resolved from the entry file. Defaults to
   * `./.celesta/voices`. Keep it between runs: it is what lets a reload or
   * an export skip synthesis.
   */
  cacheDir?: string;
  /** Lines synthesized at once. Defaults to 2. */
  concurrency?: number;
  /** Milliseconds each engine request may take. Defaults to 60000. */
  timeout?: number;
  /** Cancels synthesis; the call then rejects and keeps no partial entry. */
  signal?: AbortSignal;
  /** Part of every line's key, like a voice's `revision`. */
  revision?: string;
  /**
   * Synthesizes again even when an entry exists: `true` for every line, or
   * the ids of the lines to redo.
   */
  refresh?: boolean | readonly string[];
  /**
   * Phoneme grid for the lip sync, as in `lipSyncFromVoicevox`. Defaults to
   * VOICEVOX's; it does not change the WAV, so it is not in the key.
   */
  frameRate?: number | null;
}

/** Where a line failed. */
export type NarrationStage = 'config' | 'audio_query' | 'synthesis' | 'write';

/** Thrown by `prepareNarration()`; names the line, its speaker, and the stage. */
export class NarrationError extends Error {
  readonly lineId: string;
  readonly speaker: string | undefined;
  readonly stage: NarrationStage;

  constructor(lineId: string, speaker: string | undefined, stage: NarrationStage, message: string, cause?: unknown) {
    const who = speaker === undefined ? `line "${lineId}"` : `line "${lineId}" (speaker "${speaker}")`;
    super(`prepareNarration(): ${who} failed at ${stage}: ${message}`, cause === undefined ? undefined : { cause });
    this.name = 'NarrationError';
    this.lineId = lineId;
    this.speaker = speaker;
    this.stage = stage;
  }
}

interface Job {
  key: string;
  text: string;
  baseUrl: string;
  settings: ResolvedSettings;
  refresh: boolean;
  /** The first line that asked for it; errors name it. */
  line: NarrationLine;
}

interface ResolvedSettings {
  styleId: number;
  query: VoicevoxQuerySettings;
  interrogativeUpspeak: boolean;
  katakanaEnglish: boolean | undefined;
  revision: string | undefined;
}

interface EntryFile {
  format: number;
  key: string;
  input: Record<string, unknown>;
  engine: { baseUrl: string; version: string | null };
  wav: { bytes: number; sha256: string };
  duration: number;
  query: VoicevoxAudioQuery & Record<string, unknown>;
}

/**
 * Synthesizes every line that has no `audio` with its speaker's VOICEVOX
 * voice and returns the lines with `audio`, `lipSync`, and `narration` set,
 * in the same order, for `planDialogue()`. Call it from `prepare()`; frame
 * rendering never touches the engine.
 *
 * ```ts
 * const lines = await prepareNarration([
 *   { id: 'intro', speaker: 'zunda', text: 'HPが1です。', spokenText: 'エイチピーがいちです。' },
 *   { id: 'reply', speaker: 'metan', text: '危ないわね。', expression: 'surprised' },
 * ], {
 *   speakers: {
 *     zunda: { baseUrl: 'http://127.0.0.1:50021', styleId: 3 },
 *     metan: { baseUrl: 'http://127.0.0.1:50021', styleId: 2 },
 *   },
 * });
 * plan = await planDialogue(lines, { fps: 30 });
 * ```
 *
 * A line with `audio` is passed through as it is, without synthesis. Lines
 * with the same key share one synthesis. When a line fails the call rejects
 * with a `NarrationError` once the lines in flight settle; lines that
 * succeeded stay cached, so running again redoes only the failed ones.
 */
export async function prepareNarration<L extends NarrationLine>(
  lines: readonly L[],
  options: PrepareNarrationOptions,
): Promise<NarratedLine<L>[]> {
  const concurrency = options.concurrency ?? DEFAULT_CONCURRENCY;
  if (!Number.isInteger(concurrency) || concurrency < 1) {
    throw new Error('prepareNarration(): `concurrency` must be a positive integer');
  }
  const timeout = options.timeout ?? DEFAULT_TIMEOUT_MS;
  if (!(timeout > 0)) {
    throw new Error('prepareNarration(): `timeout` must be a positive number of milliseconds');
  }
  const cacheDir = entryRelativePath(options.cacheDir ?? DEFAULT_CACHE_DIR);
  const refresh = options.refresh ?? false;

  // Validate the whole script before any request, so a typo fails at once.
  const ids = new Set<string>();
  const jobs = new Map<string, Job>();
  const keys = lines.map((line): string | undefined => {
    if (typeof line.id !== 'string' || line.id === '') {
      throw new Error('prepareNarration(): every line needs a non-empty string `id`');
    }
    if (ids.has(line.id)) {
      throw new NarrationError(line.id, line.speaker, 'config', 'the id is used more than once');
    }
    ids.add(line.id);
    if (line.audio !== undefined) {
      return undefined;
    }
    const fail = (message: string) => new NarrationError(line.id, line.speaker, 'config', message);
    if (line.speaker === undefined) {
      throw fail('a line without `audio` needs a `speaker`');
    }
    const voice = Object.prototype.hasOwnProperty.call(options.speakers, line.speaker)
      ? options.speakers[line.speaker]
      : undefined;
    if (!voice) {
      const known = Object.keys(options.speakers).map((name) => `"${name}"`).join(', ') || 'none';
      throw fail(`the speaker is not in \`speakers\` (known: ${known})`);
    }
    if (typeof voice.baseUrl !== 'string' || !/^https?:\/\/./i.test(voice.baseUrl)) {
      throw fail('the speaker needs a `baseUrl` such as http://127.0.0.1:50021');
    }
    const text = line.spokenText ?? line.text;
    if (typeof text !== 'string') {
      throw fail('`text` is not a string; give the line a `spokenText`');
    }
    if (text.trim() === '') {
      throw fail('the spoken text is empty');
    }
    const settings = resolveSettings(voice, line.voice, fail);
    const key = cacheKey(text, settings, options.revision);
    const redo = refresh === true || (Array.isArray(refresh) && refresh.includes(line.id));
    const existing = jobs.get(key);
    if (existing) {
      existing.refresh ||= redo;
    } else {
      jobs.set(key, { key, text, baseUrl: voice.baseUrl.replace(/\/+$/, ''), settings, refresh: redo, line });
    }
    return key;
  });

  const limit = semaphore(concurrency);
  const versions = new Map<string, Promise<string | null>>();
  const results = new Map<string, Promise<NarrationAsset>>();
  for (const job of jobs.values()) {
    results.set(
      job.key,
      runJob(job, { cacheDir, timeout, signal: options.signal, limit, versions }),
    );
  }
  const settled = await Promise.allSettled(results.values());
  const failed = settled.find((result): result is PromiseRejectedResult => result.status === 'rejected');
  if (failed) {
    throw failed.reason;
  }

  const assets = new Map<string, NarrationAsset>();
  [...results.keys()].forEach((key, index) => {
    assets.set(key, (settled[index] as PromiseFulfilledResult<NarrationAsset>).value);
  });
  return lines.map((line, index): NarratedLine<L> => {
    const key = keys[index];
    if (key === undefined) {
      return line as NarratedLine<L>;
    }
    const asset = assets.get(key)!;
    return {
      ...line,
      audio: asset.audio,
      lipSync:
        line.lipSync ??
        lipSyncFromVoicevox(asset.query, {
          interrogativeUpspeak: asset.interrogativeUpspeak,
          ...(options.frameRate === undefined ? {} : { frameRate: options.frameRate }),
        }),
      narration: asset,
    };
  });
}

function resolveSettings(
  voice: VoicevoxVoice,
  override: Partial<VoicevoxVoiceSettings> | undefined,
  fail: (message: string) => Error,
): ResolvedSettings {
  const styleId = override?.styleId ?? voice.styleId;
  if (!Number.isInteger(styleId) || styleId < 0) {
    throw fail('`styleId` must be a non-negative integer');
  }
  const query: VoicevoxQuerySettings = { ...voice.query, ...override?.query };
  for (const [name, value] of Object.entries(query)) {
    if (value === undefined) {
      delete query[name as keyof VoicevoxQuerySettings];
    } else if (typeof value === 'number' ? !Number.isFinite(value) : value !== null && typeof value !== 'boolean') {
      throw fail(`query setting \`${name}\` must be a finite number, a boolean, or null`);
    }
  }
  const revisions = [voice.revision, override?.revision].filter((value) => value !== undefined);
  return {
    styleId,
    query,
    interrogativeUpspeak: override?.interrogativeUpspeak ?? voice.interrogativeUpspeak ?? true,
    katakanaEnglish: override?.katakanaEnglish ?? voice.katakanaEnglish,
    revision: revisions.length === 0 ? undefined : revisions.join('\u0000'),
  };
}

function keyInput(text: string, settings: ResolvedSettings, revision: string | undefined): Record<string, unknown> {
  return {
    format: CACHE_FORMAT,
    engine: 'voicevox',
    text,
    styleId: settings.styleId,
    query: Object.fromEntries(Object.entries(settings.query).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))),
    interrogativeUpspeak: settings.interrogativeUpspeak,
    katakanaEnglish: settings.katakanaEnglish ?? null,
    revision: settings.revision ?? null,
    scriptRevision: revision ?? null,
  };
}

function cacheKey(text: string, settings: ResolvedSettings, revision: string | undefined): string {
  return sha256(JSON.stringify(keyInput(text, settings, revision))).slice(0, 32);
}

interface RunContext {
  cacheDir: string;
  timeout: number;
  signal: AbortSignal | undefined;
  limit: <T>(task: () => Promise<T>) => Promise<T>;
  versions: Map<string, Promise<string | null>>;
}

async function runJob(job: Job, context: RunContext): Promise<NarrationAsset> {
  const { line } = job;
  const wavPath = path.join(context.cacheDir, `${job.key}.wav`);
  const jsonPath = path.join(context.cacheDir, `${job.key}.json`);
  if (!job.refresh) {
    const hit = await readEntry(job.key, wavPath, jsonPath);
    if (hit) {
      return { ...hit, spokenText: job.text, interrogativeUpspeak: job.settings.interrogativeUpspeak, cached: true };
    }
  }
  return context.limit(async () => {
    const fail = (stage: NarrationStage, message: string, cause?: unknown) =>
      new NarrationError(line.id, line.speaker, stage, message, cause);
    if (context.signal?.aborted) {
      throw fail('audio_query', 'cancelled', context.signal.reason);
    }
    const { settings, baseUrl } = job;

    const queryUrl = new URL(`${baseUrl}/audio_query`);
    queryUrl.searchParams.set('text', job.text);
    queryUrl.searchParams.set('speaker', String(settings.styleId));
    if (settings.katakanaEnglish !== undefined) {
      queryUrl.searchParams.set('enable_katakana_english', String(settings.katakanaEnglish));
    }
    const queryBody = await request(queryUrl, undefined, context, (message, cause) => fail('audio_query', message, cause));
    let query: VoicevoxAudioQuery & Record<string, unknown>;
    try {
      query = JSON.parse(new TextDecoder().decode(queryBody));
    } catch (error) {
      throw fail('audio_query', 'the engine returned a query that is not JSON', error);
    }
    if (!query || typeof query !== 'object' || !Array.isArray(query.accent_phrases)) {
      throw fail('audio_query', 'the engine returned a query without `accent_phrases`');
    }
    Object.assign(query, settings.query);

    const version = versionOf(baseUrl, context);
    const synthesisUrl = new URL(`${baseUrl}/synthesis`);
    synthesisUrl.searchParams.set('speaker', String(settings.styleId));
    synthesisUrl.searchParams.set('enable_interrogative_upspeak', String(settings.interrogativeUpspeak));
    const wav = await request(synthesisUrl, JSON.stringify(query), context, (message, cause) =>
      fail('synthesis', message, cause),
    );
    let duration: number;
    try {
      duration = wavDuration(wav);
    } catch (error) {
      throw fail('synthesis', `the engine returned audio that is not a readable WAV (${messageOf(error)})`, error);
    }
    // Do not commit an entry for work the caller gave up on.
    if (context.signal?.aborted) {
      throw fail('synthesis', 'cancelled', context.signal.reason);
    }

    const entry: EntryFile = {
      format: CACHE_FORMAT,
      key: job.key,
      input: keyInput(job.text, settings, undefined),
      engine: { baseUrl, version: await version },
      wav: { bytes: wav.byteLength, sha256: sha256(wav) },
      duration,
      query,
    };
    try {
      await mkdir(context.cacheDir, { recursive: true });
      // The JSON is the commit marker: written last, and only after the WAV it describes.
      await rm(jsonPath, { force: true });
      await writeAtomically(wavPath, wav);
      await writeAtomically(jsonPath, `${JSON.stringify(entry, null, 2)}\n`);
    } catch (error) {
      throw fail('write', `could not write the cache entry in ${context.cacheDir}: ${messageOf(error)}`, error);
    }
    return {
      audio: wavPath,
      metadata: jsonPath,
      duration,
      query,
      spokenText: job.text,
      interrogativeUpspeak: settings.interrogativeUpspeak,
      key: job.key,
      cached: false,
    };
  });
}

/** A valid entry on disk, or undefined when it is missing or does not check out. */
async function readEntry(
  key: string,
  wavPath: string,
  jsonPath: string,
): Promise<Omit<NarrationAsset, 'spokenText' | 'interrogativeUpspeak' | 'cached'> | undefined> {
  let entry: EntryFile;
  let wav: Uint8Array;
  try {
    entry = JSON.parse(await readFile(jsonPath, 'utf8'));
    wav = await readFile(wavPath);
  } catch {
    return undefined;
  }
  if (
    entry?.format !== CACHE_FORMAT ||
    entry.key !== key ||
    !entry.query ||
    !Array.isArray(entry.query.accent_phrases) ||
    entry.wav?.bytes !== wav.byteLength ||
    entry.wav.sha256 !== sha256(wav)
  ) {
    return undefined;
  }
  let duration: number;
  try {
    duration = wavDuration(wav);
  } catch {
    return undefined;
  }
  return { audio: wavPath, metadata: jsonPath, duration, query: entry.query, key };
}

async function request(
  url: URL,
  body: string | undefined,
  context: RunContext,
  fail: (message: string, cause?: unknown) => Error,
): Promise<Uint8Array> {
  const controller = new AbortController();
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, context.timeout);
  const cancel = () => controller.abort();
  context.signal?.addEventListener('abort', cancel, { once: true });
  const where = `${url.origin}${url.pathname}`;
  try {
    let response: Response;
    try {
      response = await fetch(url, {
        method: 'POST',
        headers: body === undefined ? undefined : { 'content-type': 'application/json' },
        body,
        signal: controller.signal,
      });
    } catch (error) {
      if (timedOut) throw fail(`${where} did not answer within ${context.timeout} ms`, error);
      if (context.signal?.aborted) throw fail('cancelled', context.signal.reason);
      throw fail(
        `could not reach the VOICEVOX Engine at ${url.origin} (${messageOf(error)}); start the engine or fix the speaker's baseUrl`,
        error,
      );
    }
    let bytes: Uint8Array;
    try {
      bytes = new Uint8Array(await response.arrayBuffer());
    } catch (error) {
      if (timedOut) throw fail(`${where} did not finish within ${context.timeout} ms`, error);
      if (context.signal?.aborted) throw fail('cancelled', context.signal.reason);
      throw fail(`reading the response of ${where} failed (${messageOf(error)})`, error);
    }
    if (!response.ok) {
      const detail = new TextDecoder().decode(bytes.subarray(0, 500)).trim();
      throw fail(`${where} answered ${response.status}${detail ? `: ${detail}` : ''}`);
    }
    return bytes;
  } finally {
    clearTimeout(timer);
    context.signal?.removeEventListener('abort', cancel);
  }
}

/** The engine's `/version`, recorded in new entries; null when it cannot be read. */
function versionOf(baseUrl: string, context: RunContext): Promise<string | null> {
  let pending = context.versions.get(baseUrl);
  if (!pending) {
    pending = (async () => {
      try {
        const response = await fetch(`${baseUrl}/version`, { signal: AbortSignal.timeout(context.timeout) });
        if (!response.ok) return null;
        const version: unknown = await response.json();
        return typeof version === 'string' ? version : null;
      } catch {
        return null;
      }
    })();
    context.versions.set(baseUrl, pending);
  }
  return pending;
}

function wavDuration(bytes: Uint8Array): number {
  const { sampleRate, frameCount } = readWavHeader(bytes);
  if (!(sampleRate > 0) || frameCount === 0) {
    throw new Error('it has no samples');
  }
  return frameCount / sampleRate;
}

async function writeAtomically(target: string, data: Uint8Array | string): Promise<void> {
  const temporary = `${target}.${process.pid}.${Math.random().toString(36).slice(2)}.tmp`;
  try {
    await writeFile(temporary, data);
    await rename(temporary, target);
  } catch (error) {
    await rm(temporary, { force: true });
    throw error;
  }
}

function semaphore(size: number): <T>(task: () => Promise<T>) => Promise<T> {
  let active = 0;
  const waiting: (() => void)[] = [];
  return async (task) => {
    if (active < size) {
      active += 1;
    } else {
      // The finishing task hands its slot over without releasing it.
      await new Promise<void>((resolve) => waiting.push(resolve));
    }
    try {
      return await task();
    } finally {
      const next = waiting.shift();
      if (next) next();
      else active -= 1;
    }
  };
}

// `@celesta/react/internal`'s `entryRelativePath`: the CLI sets
// CELESTA_REACT_ENTRY_DIR before `prepare()`, so a relative `cacheDir` sits
// beside the composition like a relative `<Audio src>`.
function entryRelativePath(src: string): string {
  return path.isAbsolute(src) ? src : path.resolve(process.env.CELESTA_REACT_ENTRY_DIR ?? process.cwd(), src);
}

function sha256(data: Uint8Array | string): string {
  return createHash('sha256').update(data).digest('hex');
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
