// `defineScore()`: the score a composition plays, rendered to a WAV file in
// the entry's `prepare()` and played by `<ScoreAudio>`.
//
// The file is named by a hash of everything that changes its samples (the
// resolved events, the master, the seed, and SYNTH_VERSION), so an unchanged
// score is never rendered twice and a changed one never reuses a stale file.
// It is written through a temporary file and a rename, so a crash never
// leaves a partial file under the final name. Nothing here writes to stdout,
// which carries the bridge protocol.

import { SAMPLE_RATE } from './dsp.js';
import { encodeWav, master, mix } from './render.js';
import { SYNTH_VERSION, buildScore } from './score.js';
import type { ScoreBuilder, ScoreData, ScoreOptions } from './score.js';

const DEFAULT_CACHE_DIR = './.celesta/score';

export interface PrepareScoreOptions {
  /**
   * Where rendered files are kept, resolved from the entry file. Defaults to
   * `./.celesta/score`.
   */
  cacheDir?: string;
}

/** A score: its resolved events, and once prepared, the file that plays it. */
export interface Score extends ScoreData {
  /**
   * Renders the score's samples: 16-bit stereo PCM WAV at 48 kHz, exactly
   * `durationInFrames / fps` long. The same bytes on every run and system.
   */
  toWav(): Uint8Array;
  /**
   * Writes the WAV into the cache (unless the same score is already there)
   * and resolves to its absolute path. Call it from the entry's `prepare()`.
   */
  prepare(options?: PrepareScoreOptions): Promise<string>;
  /** The WAV's absolute path. Throws until `prepare()` has finished. */
  readonly src: string;
  /** The hex SHA-256 that names the file; set by `prepare()`. */
  readonly key: string | undefined;
}

/**
 * Records a score against the composition's timeline. `build` runs once,
 * now; every name and value is checked as it is recorded, and an event
 * starting at or after the end is dropped with a warning (or thrown, with
 * `strict`).
 *
 * ```tsx
 * export const score = defineScore({ fps: 30, durationInFrames: 300 }, s => {
 *   s.tempo({ bpm: 112 });
 *   s.program('lead', 'marimba');
 *   s.note('lead', 'C5', { beat: 0, beats: 1 });
 *   s.ticks(typewriterFrames('npm run build', { from: 20, framesPerChar: 2 }));
 *   s.fx('whoosh', { frame: 150, reverse: true });
 * });
 *
 * export async function prepare() {
 *   await score.prepare();
 * }
 * // In the composition: <ScoreAudio score={score} />
 * ```
 */
export function defineScore(options: ScoreOptions, build: (s: ScoreBuilder) => void): Score {
  const data = buildScore(options, build);
  for (const warning of data.warnings) console.warn(`celesta score: ${warning}`);

  let wav: Uint8Array | undefined;
  let src: string | undefined;
  let key: string | undefined;
  let pending: Promise<string> | undefined;

  const toWav = (): Uint8Array => {
    if (wav === undefined) {
      const samples = Math.round(data.duration * SAMPLE_RATE);
      wav = encodeWav(master(mix({ events: data.events, samples, master: data.master, seed: data.seed }), data.master));
    }
    return wav;
  };

  const prepare = (prepareOptions: PrepareScoreOptions = {}): Promise<string> => {
    pending ??= (async () => {
      const { writeScoreFile } = await import('./cache.js');
      const size = 44 + Math.round(data.duration * SAMPLE_RATE) * 4;
      const written = await writeScoreFile(cacheInput(data), size, toWav, prepareOptions.cacheDir ?? DEFAULT_CACHE_DIR);
      key = written.key;
      src = written.path;
      return written.path;
    })();
    pending.catch(() => {
      pending = undefined;
    });
    return pending;
  };

  return {
    ...data,
    toWav,
    prepare,
    get src(): string {
      if (src === undefined) {
        throw new Error('score.src: the score is not rendered yet; call `await score.prepare()` from the entry\'s prepare()');
      }
      return src;
    },
    get key(): string | undefined {
      return key;
    },
  };
}

/** Everything the samples depend on, in a fixed order, for the file's hash. */
function cacheInput(data: ScoreData): string {
  return JSON.stringify({
    synth: SYNTH_VERSION,
    sampleRate: data.sampleRate,
    duration: [data.durationInFrames, data.fps],
    seed: data.seed,
    master: [data.master.gain, data.master.fadeIn, data.master.fadeOut, data.master.reverb],
    events: data.events.map(e => [
      e.kind, e.name, e.time, e.length ?? null, e.midi ?? null, e.vel, e.gain, e.pan, e.reverb, e.pitch, e.bright, e.reverse,
    ]),
  });
}
