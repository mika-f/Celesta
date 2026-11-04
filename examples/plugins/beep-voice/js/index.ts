// The beep-voice plugin, imported by entries as `@celesta-plugin/beep-voice`:
// a speech channel that voices dialogue lines with game-style beeps, one per
// character, instead of a speech engine. It has no process of its own; its
// module runs in the entry's `prepare()`, as `@celesta.studio/voicevox` does, and
// hands `planDialogue()` lines with `audio` and `lipSync` set, so
// `<DialogueSeries>` and `<CharacterView>` treat them like any other voice.
//
// `synthesize()` is the engine. A plugin for another speech engine replaces
// it with a request to that engine (over HTTP, or to a program it starts
// with `node:child_process`) and keeps the rest: the cache, the WAV file, and
// the lip-sync track built from the engine's timing.

import { createHash } from 'node:crypto';
import { mkdir, rename, stat, writeFile } from 'node:fs/promises';
import * as path from 'node:path';

import { lipSyncFromKeyframes, vowelShapes } from '@celesta.studio/character';
import type { DialogueLine, MouthKeyframe } from '@celesta.studio/character';
import { entryRelativePath } from '@celesta.studio/react/internal';

/** How a speaker beeps. Every field is part of the cache key. */
export interface BeepVoice {
  /** The base pitch in hertz. Default 220. */
  pitch?: number;
  /** Characters spoken each second. Default 12. */
  speed?: number;
  /** Loudness from 0 to 1. Default 0.3. */
  volume?: number;
  /** `square` (buzzy), `triangle` (soft), or `sine` (pure). Default `square`. */
  waveform?: 'square' | 'triangle' | 'sine';
}

/** One script line; every other field is passed through. */
export interface BeepLine extends DialogueLine {
  /** Names the line in errors. Must be unique. */
  id: string;
  /** Key into `options.speakers`. Optional when the line has its own `audio`. */
  speaker?: string;
}

export interface PrepareBeepNarrationOptions {
  /** Voices by the lines' `speaker`. */
  speakers: Readonly<Record<string, BeepVoice>>;
  /** Where the WAVs are kept, resolved from the entry file. Default `./.celesta/beep-voice`. */
  cacheDir?: string;
}

/** A line as `prepareBeepNarration()` returns it: ready for `planDialogue()`. */
export type BeepNarratedLine<L extends BeepLine = BeepLine> = L & { audio: string };

const SAMPLE_RATE = 24_000;
const LEAD_IN = 0.05;
const TAIL = 0.15;
/** Bumped when the sound for the same text and voice changes. */
const FORMAT = 1;
/** Steps of a major pentatonic scale, in semitones: each character takes one. */
const SCALE = [0, 2, 4, 7, 9];
const STOPS = new Set([...'。．.!！?？…']);
const BREAKS = new Set([...'、，,']);

/** What `synthesize()` makes of a line: mono samples and where the mouth opens. */
export interface Beeps {
  samples: Float32Array;
  sampleRate: number;
  /** Mouth shapes timed from the start of the samples. */
  mouth: MouthKeyframe[];
}

function wave(kind: NonNullable<BeepVoice['waveform']>, phase: number): number {
  switch (kind) {
    case 'sine':
      return Math.sin(phase);
    case 'triangle':
      return (2 / Math.PI) * Math.asin(Math.sin(phase));
    case 'square':
      // The first odd harmonics only, so the edges do not alias.
      return (4 / Math.PI) * (Math.sin(phase) + Math.sin(3 * phase) / 3 + Math.sin(5 * phase) / 5 + Math.sin(7 * phase) / 7);
  }
}

/**
 * The engine: `text` as beeps in `voice`. Each character is one beep on a
 * note picked from its code point; `、` and `。` and the like are pauses; a
 * line ending in a question rises. A pure function of its arguments.
 */
export function synthesize(text: string, voice: BeepVoice = {}): Beeps {
  const pitch = voice.pitch ?? 220;
  const speed = voice.speed ?? 12;
  const volume = Math.min(1, Math.max(0, voice.volume ?? 0.3));
  const kind = voice.waveform ?? 'square';
  if (!(pitch > 0) || !(speed > 0)) throw new Error('beep-voice: `pitch` and `speed` must be positive');

  const characters = [...text.trim()];
  const question = /[?？]$/.test(text.trim());
  const beat = 1 / speed;
  const notes: { start: number; length: number; frequency: number; character: string }[] = [];
  let seconds = LEAD_IN;
  characters.forEach((character, index) => {
    if (STOPS.has(character)) {
      seconds += beat * 2;
    } else if (BREAKS.has(character)) {
      seconds += beat;
    } else if (/\s/.test(character)) {
      seconds += beat / 2;
    } else {
      const step = SCALE[character.codePointAt(0)! % SCALE.length]! + (question && index >= characters.length - 2 ? 5 : 0);
      notes.push({ start: seconds, length: beat * 0.75, frequency: pitch * 2 ** (step / 12), character });
      seconds += beat;
    }
  });
  const duration = seconds + TAIL;

  const samples = new Float32Array(Math.ceil(duration * SAMPLE_RATE));
  const mouth: MouthKeyframe[] = [];
  for (const note of notes) {
    const first = Math.round(note.start * SAMPLE_RATE);
    const count = Math.round(note.length * SAMPLE_RATE);
    for (let i = 0; i < count; i++) {
      const t = i / SAMPLE_RATE;
      // A quick attack and release, so beeps do not click.
      const envelope = Math.min(1, t / 0.005, (note.length - t) / 0.03);
      samples[first + i] = volume * envelope * wave(kind, 2 * Math.PI * note.frequency * t);
    }
    mouth.push({ seconds: note.start, mouth: vowelShapes(note.character)[0] ?? 'a' });
    mouth.push({ seconds: note.start + note.length, mouth: 'closed' });
  }
  return { samples, sampleRate: SAMPLE_RATE, mouth };
}

/** 16-bit PCM WAV bytes of mono `samples`. */
export function encodeWav(samples: Float32Array, sampleRate: number): Uint8Array {
  const bytes = new Uint8Array(44 + samples.length * 2);
  const view = new DataView(bytes.buffer);
  const ascii = (offset: number, text: string) => [...text].forEach((c, i) => view.setUint8(offset + i, c.charCodeAt(0)));
  ascii(0, 'RIFF');
  view.setUint32(4, 36 + samples.length * 2, true);
  ascii(8, 'WAVE');
  ascii(12, 'fmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // mono
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  ascii(36, 'data');
  view.setUint32(40, samples.length * 2, true);
  samples.forEach((sample, i) => view.setInt16(44 + i * 2, Math.round(Math.max(-1, Math.min(1, sample)) * 32767), true));
  return bytes;
}

async function exists(file: string): Promise<boolean> {
  try {
    return (await stat(file)).isFile();
  } catch {
    return false;
  }
}

/**
 * Voices every line that has no `audio` with its speaker's beeps and returns
 * the lines with `audio` and `lipSync` set, in the same order, for
 * `planDialogue()`. Call it from `prepare()`.
 *
 * ```ts
 * const lines = await prepareBeepNarration([
 *   { id: 'hi', speaker: 'shizuku', text: 'こんにちは！' },
 * ], { speakers: { shizuku: { pitch: 330 } } });
 * plan = await planDialogue(lines, { fps: 30 });
 * ```
 *
 * A line with `audio` is passed through as it is. WAVs are kept in
 * `cacheDir` under a hash of what they sound like, so they are written once.
 */
export async function prepareBeepNarration<L extends BeepLine>(
  lines: readonly L[],
  options: PrepareBeepNarrationOptions,
): Promise<BeepNarratedLine<L>[]> {
  const cacheDir = entryRelativePath(options.cacheDir ?? './.celesta/beep-voice');
  // The whole script is checked before anything is voiced, so a typo fails at once.
  const ids = new Set<string>();
  const jobs = lines.map((line) => {
    if (ids.has(line.id)) throw new Error(`prepareBeepNarration(): line "${line.id}" is used more than once`);
    ids.add(line.id);
    if (line.audio !== undefined) return undefined;
    const voice = line.speaker === undefined ? undefined : options.speakers[line.speaker];
    if (!voice) throw new Error(`prepareBeepNarration(): line "${line.id}" has no speaker in \`speakers\``);
    const text = line.spokenText ?? line.text;
    if (typeof text !== 'string') {
      throw new Error(`prepareBeepNarration(): line "${line.id}" needs a \`spokenText\`, as its \`text\` is not a string`);
    }
    return { text, voice };
  });

  const result: BeepNarratedLine<L>[] = [];
  for (const [index, line] of lines.entries()) {
    const job = jobs[index];
    if (!job) {
      result.push(line as BeepNarratedLine<L>);
      continue;
    }
    const { text, voice } = job;
    const beeps = synthesize(text, voice);
    const key = createHash('sha256').update(JSON.stringify({ FORMAT, text, voice })).digest('hex').slice(0, 32);
    const audio = path.join(cacheDir, `${key}.wav`);
    if (!(await exists(audio))) {
      await mkdir(cacheDir, { recursive: true });
      // Through a temporary file, so an interrupted write never looks done.
      const partial = `${audio}.${process.pid}.partial`;
      await writeFile(partial, encodeWav(beeps.samples, beeps.sampleRate));
      await rename(partial, audio);
    }
    result.push({
      ...line,
      audio,
      lipSync: line.lipSync ?? lipSyncFromKeyframes(beeps.mouth, beeps.samples.length / beeps.sampleRate),
    });
  }
  return result;
}
