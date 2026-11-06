// Imports a folder of recordings that an external speech tool (VOICEROID,
// A.I.VOICE, AquesTalk front ends, ...) saved as paired `name.wav` +
// `name.txt` files, so a script does not list every line's audio, subtitle,
// and lip sync by hand. Call `importVoiceFolder()` from an entry's
// `prepare()` and pass the result to `planDialogue()`.
//
// Every file read here is recorded as a preparation dependency, so the
// editor reloads the entry when a recording is resaved, added, or removed,
// even in a folder outside the entry's directory. Each WAV is copied into a
// content-addressed snapshot before anything else reads it: the duration
// probe, the lip sync, preview, and export all use the bytes this call read,
// never a newer file that appeared halfway through an export.

import { createHash } from 'node:crypto';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { TextDecoder } from 'node:util';

import { entryRelativePath, trackPrepareDependency } from '@celesta.studio/react/internal';

import { writeFileAtomicSync } from './atomic-file';
import type { DialogueExpression } from './components';
import type { DialogueLine } from './dialogue-series';
import { decodeWav, lipSyncFromAudio } from './lipsync';
import type { LipSyncTrack, WavAudio } from './lipsync';
import { waveformVowelText } from './spoken-text';

/** Text encodings `importVoiceFolder()` decodes transcripts with. */
export type VoiceTextEncoding = 'utf-8' | 'utf-16le' | 'shift_jis';

/**
 * Decodes one transcript's bytes. Throw to report a file it cannot decode;
 * the import fails with that message rather than showing garbled text.
 */
export type VoiceTextDecoder = (bytes: Uint8Array, file: string) => string;

export interface ImportVoiceFolderOptions {
  /** Folder of `.wav` + `.txt` pairs, resolved from the entry file. */
  directory: string;
  /** Speaker of every line, unless the manifest names another. */
  speaker?: string;
  /** Also reads subfolders (not ones starting with `.`). Defaults to false. */
  recursive?: boolean;
  /**
   * How transcripts are encoded. Defaults to `'utf-8'`, with or without a
   * byte order mark. A file that does not decode cleanly fails the import.
   */
  encoding?: VoiceTextEncoding | VoiceTextDecoder;
  /**
   * `'waveform'` (default): the mouth opens with the voice's loudness and
   * takes the vowels of the line's kana/romaji `spokenText`, else its
   * transcript, spread over the voiced span. A line whose text has kanji
   * and no `spokenText`, or no vowels at all, falls back to `'amplitude'`
   * with a warning. `'amplitude'`: open (`a`) and closed only. `false`: no track.
   */
  lipSync?: 'waveform' | 'amplitude' | false;
  /** JSON file of per-line overrides (see `VoiceManifest`), resolved from the entry file. */
  manifest?: string;
  /**
   * A WAV without its TXT (or the other way round), an empty transcript, or
   * a WAV cut short — what a folder looks like while a tool is still
   * writing to it. `'error'` (default) fails the import, so the editor keeps
   * the last good preview until the pair is complete; `'skip'` leaves the
   * line out with a warning.
   */
  incomplete?: 'error' | 'skip';
  /** Receives warnings. Defaults to `console.warn`. */
  onWarning?: (message: string) => void;
}

/** One line of a `VoiceManifest`. Only `file` is required. */
export interface VoiceManifestEntry {
  /** The pair's name: its path from the folder without extension, with `/` separators. */
  file: string;
  /** Defaults to the pair's name, spelled as the WAV is. */
  id?: string;
  speaker?: string;
  /** Subtitle text in place of the transcript. */
  text?: string;
  /** The line's `spokenText`: kana or romaji the lip sync takes its vowels from, in place of the transcript. */
  spokenText?: string;
  scene?: string;
  expression?: DialogueExpression;
  /** Seconds of silence after the line. */
  gap?: number;
}

/**
 * Overrides for an imported folder. Lines listed here come first, in the
 * order listed; the folder's other pairs follow in natural order.
 */
export interface VoiceManifest {
  lines: VoiceManifestEntry[];
}

/** A line `importVoiceFolder()` read; pass a list of them to `planDialogue()`. */
export interface VoiceLine extends DialogueLine {
  id: string;
  /** The transcript (or the manifest's `text`), trimmed, with `\n` line breaks. */
  text: string;
  /** The snapshot of the WAV this import read; see `source.wav` for the original. */
  audio: string;
  /** The manifest's `spokenText`, if any. */
  spokenText?: string;
  /** The WAV's length. */
  durationInSeconds: number;
  /** Absolute paths of the pair. */
  source: { wav: string; txt: string };
}

interface FoundFile {
  /** Path from the folder without extension, `/`-separated, NFC. */
  stem: string;
  absolute: string;
}

interface Pair {
  stem: string;
  wav?: FoundFile;
  txt?: FoundFile;
}

/**
 * Reads a folder of paired recordings into dialogue lines, in natural order
 * of their names (`voice-2` before `voice-10`). Each line's id is its name
 * (`voice-2`, or `sub/voice-2` when recursive), so ids stay put when lines
 * are added around them.
 *
 * ```tsx
 * export async function prepare() {
 *   const lines = await importVoiceFolder({ directory: './voices/akane', speaker: 'akane' });
 *   plan = await planDialogue(lines, { fps: 30 });
 * }
 * ```
 */
export async function importVoiceFolder(options: ImportVoiceFolderOptions): Promise<VoiceLine[]> {
  const warn = options.onWarning ?? ((message: string) => console.warn(message));
  const encoding = options.encoding ?? 'utf-8';
  const lipSync = options.lipSync ?? 'waveform';
  const incomplete = options.incomplete ?? 'error';
  if (typeof options.directory !== 'string' || options.directory.length === 0) {
    throw new Error('importVoiceFolder() requires a `directory`');
  }
  if (lipSync !== false && lipSync !== 'waveform' && lipSync !== 'amplitude') {
    throw new Error(`importVoiceFolder(): \`lipSync\` must be 'waveform', 'amplitude', or false, got ${JSON.stringify(lipSync)}`);
  }
  if (incomplete !== 'error' && incomplete !== 'skip') {
    throw new Error(`importVoiceFolder(): \`incomplete\` must be 'error' or 'skip', got ${JSON.stringify(incomplete)}`);
  }
  if (typeof encoding !== 'function') {
    textDecoder(encoding);
  }

  const directory = entryRelativePath(options.directory);
  const files = listFiles(directory, options.directory, options.recursive === true);

  const problems: string[] = [];
  const unfinished: string[] = [];
  const pairs = pairFiles(files, problems);

  const manifest = options.manifest === undefined ? undefined : readManifest(options.manifest);
  const ordered = orderPairs(pairs, manifest, problems);

  const lines: VoiceLine[] = [];
  for (const { pair, entry } of ordered) {
    if (!pair.wav || !pair.txt) {
      const [have, missing] = pair.wav ? [pair.wav, 'txt'] : [pair.txt!, 'wav'];
      unfinished.push(`${have.absolute} has no matching .${missing} file`);
      continue;
    }
    trackPrepareDependency(pair.wav.absolute);
    const wavBytes = new Uint8Array(fs.readFileSync(pair.wav.absolute));
    trackPrepareDependency(pair.txt.absolute);
    const txtBytes = new Uint8Array(fs.readFileSync(pair.txt.absolute));

    let audio: WavAudio;
    try {
      audio = decodeWav(wavBytes);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (wavBytes.length < 12 || message.includes('truncated')) {
        unfinished.push(`${pair.wav.absolute} is incomplete (${wavBytes.length === 0 ? 'empty' : message})`);
      } else {
        problems.push(`${pair.wav.absolute}: ${message} (only uncompressed PCM or float WAV is supported)`);
      }
      continue;
    }
    if (audio.samples.length === 0 || audio.sampleRate <= 0) {
      unfinished.push(`${pair.wav.absolute} is incomplete (it has no samples)`);
      continue;
    }

    let transcript: string;
    try {
      transcript = normalizeText(decodeText(txtBytes, encoding, pair.txt.absolute));
    } catch (error) {
      problems.push(error instanceof Error ? error.message : String(error));
      continue;
    }
    if (transcript.length === 0) {
      unfinished.push(`${pair.txt.absolute} is incomplete (the transcript is empty)`);
      continue;
    }

    const id = entry?.id ?? pair.stem;
    const spokenText = entry?.spokenText;
    let track: LipSyncTrack | undefined;
    if (lipSync !== false) {
      const vowels = lipSync === 'waveform'
        ? waveformVowelText(spokenText ?? transcript, spokenText !== undefined, id, 'importVoiceFolder()', warn)
        : '';
      track = lipSyncFromAudio(audio, vowels);
    }

    const line: VoiceLine = {
      id,
      text: entry?.text === undefined ? transcript : normalizeText(entry.text),
      audio: snapshot(wavBytes, path.basename(pair.wav.absolute)),
      durationInSeconds: audio.samples.length / audio.sampleRate,
      source: { wav: pair.wav.absolute, txt: pair.txt.absolute },
    };
    const speaker = entry?.speaker ?? options.speaker;
    if (speaker !== undefined) line.speaker = speaker;
    if (spokenText !== undefined) line.spokenText = spokenText;
    if (entry?.scene !== undefined) line.scene = entry.scene;
    if (entry?.expression !== undefined) line.expression = entry.expression;
    if (entry?.gap !== undefined) line.gap = entry.gap;
    if (track) line.lipSync = track;
    lines.push(line);
  }

  const seen = new Map<string, string>();
  for (const line of lines) {
    const other = seen.get(line.id);
    if (other !== undefined) {
      problems.push(`line id "${line.id}" is used by both ${other} and ${line.source.wav}`);
    }
    seen.set(line.id, line.source.wav);
  }

  if (unfinished.length > 0) {
    if (incomplete === 'skip') {
      for (const message of unfinished) {
        warn(`importVoiceFolder(): skipped ${message}`);
      }
    } else {
      problems.push(...unfinished);
      problems.push(
        "if a voice tool is still writing, the pair loads once it is complete; pass `incomplete: 'skip'` to leave such lines out",
      );
    }
  }
  if (problems.length > 0) {
    throw new Error(`importVoiceFolder(): ${options.directory}:\n${problems.map((problem) => `  - ${problem}`).join('\n')}`);
  }
  if (lines.length === 0) {
    throw new Error(`importVoiceFolder(): no .wav + .txt pairs in ${options.directory} (looked in ${directory})`);
  }
  return lines;
}

function listFiles(directory: string, given: string, recursive: boolean): FoundFile[] {
  const found: FoundFile[] = [];
  const walk = (dir: string, prefix: string) => {
    trackPrepareDependency(dir);
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (prefix === '' && (code === 'ENOENT' || code === 'ENOTDIR')) {
        throw new Error(`importVoiceFolder(): voice folder not found: ${given} (looked at ${directory})`);
      }
      throw error;
    }
    for (const entry of entries) {
      // Dotfiles include macOS `._name.wav` metadata and editors' temporaries.
      if (entry.name.startsWith('.')) continue;
      const absolute = path.join(dir, entry.name);
      const name = entry.name.normalize('NFC');
      if (entry.isDirectory()) {
        if (recursive) walk(absolute, `${prefix}${name}/`);
        continue;
      }
      const extension = path.extname(name).toLowerCase();
      if (extension === '.wav' || extension === '.txt') {
        found.push({ stem: `${prefix}${name.slice(0, -extension.length)}`, absolute });
      }
    }
  };
  walk(directory, '');
  // Directory listings come in no fixed order; diagnostics should not either.
  return found.sort((a, b) => compareUnits(a.absolute, b.absolute));
}

/** Pairing key: names that differ only in case pair, the same on every platform. */
function pairKey(stem: string): string {
  return stem.normalize('NFC').toLowerCase();
}

function pairFiles(files: FoundFile[], problems: string[]): Map<string, Pair> {
  const pairs = new Map<string, Pair>();
  for (const file of files) {
    const key = pairKey(file.stem);
    const kind = path.extname(file.absolute).toLowerCase() === '.wav' ? 'wav' : 'txt';
    let pair = pairs.get(key);
    if (!pair) {
      pair = { stem: file.stem };
      pairs.set(key, pair);
    }
    const existing = pair[kind];
    if (existing) {
      problems.push(`${existing.absolute} and ${file.absolute} differ only in case; rename one so each line has one .${kind}`);
      continue;
    }
    pair[kind] = file;
    // The WAV's spelling names the line.
    if (kind === 'wav') pair.stem = file.stem;
  }
  return pairs;
}

function orderPairs(
  pairs: Map<string, Pair>,
  manifest: VoiceManifest | undefined,
  problems: string[],
): { pair: Pair; entry?: VoiceManifestEntry }[] {
  const ordered: { pair: Pair; entry?: VoiceManifestEntry }[] = [];
  const listed = new Set<string>();
  for (const entry of manifest?.lines ?? []) {
    const key = pairKey(entry.file);
    if (listed.has(key)) {
      problems.push(`the manifest lists "${entry.file}" more than once`);
      continue;
    }
    listed.add(key);
    const pair = pairs.get(key);
    if (!pair) {
      problems.push(`the manifest lists "${entry.file}", but the folder has no ${entry.file}.wav or ${entry.file}.txt`);
      continue;
    }
    ordered.push({ pair, entry });
  }
  const rest = [...pairs.entries()].filter(([key]) => !listed.has(key)).map(([, pair]) => pair);
  rest.sort((a, b) => naturalCompare(a.stem, b.stem));
  for (const pair of rest) ordered.push({ pair });
  return ordered;
}

/**
 * Natural order that does not depend on the locale or platform: digit runs
 * compare by value, other text by its lowercase UTF-16 code units. Names
 * equal by that (`a01` and `a1`, `A` and `a`) fall back to plain code-unit
 * order, so every list has one order.
 */
export function naturalCompare(a: string, b: string): number {
  const left = a.match(/\d+|\D+/g) ?? [];
  const right = b.match(/\d+|\D+/g) ?? [];
  for (let i = 0; i < Math.min(left.length, right.length); i += 1) {
    const x = left[i];
    const y = right[i];
    const digits = /^\d/.test(x) && /^\d/.test(y);
    let order: number;
    if (digits) {
      const xv = x.replace(/^0+(?=\d)/, '');
      const yv = y.replace(/^0+(?=\d)/, '');
      order = xv.length - yv.length || compareUnits(xv, yv);
    } else {
      order = compareUnits(x.toLowerCase(), y.toLowerCase());
    }
    if (order !== 0) return order;
  }
  return left.length - right.length || compareUnits(a, b);
}

function compareUnits(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

const decoders = new Map<VoiceTextEncoding, TextDecoder>();

function textDecoder(encoding: VoiceTextEncoding): TextDecoder {
  let decoder = decoders.get(encoding);
  if (decoder) return decoder;
  if (encoding !== 'utf-8' && encoding !== 'utf-16le' && encoding !== 'shift_jis') {
    throw new Error(
      `importVoiceFolder(): unsupported encoding ${JSON.stringify(encoding)}; use 'utf-8', 'utf-16le', 'shift_jis', or a decoding function`,
    );
  }
  try {
    decoder = new TextDecoder(encoding, { fatal: true });
  } catch {
    throw new Error(`importVoiceFolder(): this Node.js runtime cannot decode ${encoding}; pass a decoding function as \`encoding\``);
  }
  decoders.set(encoding, decoder);
  return decoder;
}

function decodeText(bytes: Uint8Array, encoding: VoiceTextEncoding | VoiceTextDecoder, file: string): string {
  const name = typeof encoding === 'function' ? 'the `encoding` function' : encoding;
  let text: string;
  if (typeof encoding === 'function') {
    text = encoding(bytes, file);
    if (typeof text !== 'string') {
      throw new Error(`${file}: the \`encoding\` function returned ${typeof text}, not a string`);
    }
  } else {
    const utf16Bom = bytes.length >= 2 && ((bytes[0] === 0xff && bytes[1] === 0xfe) || (bytes[0] === 0xfe && bytes[1] === 0xff));
    if (utf16Bom && encoding !== 'utf-16le') {
      throw new Error(`${file} starts with a UTF-16 byte order mark; pass encoding: 'utf-16le'`);
    }
    try {
      text = textDecoder(encoding).decode(bytes);
    } catch {
      throw new Error(`${file} is not valid ${encoding}; set \`encoding\` to the one the voice tool saved with`);
    }
  }
  if (text.includes('\u0000')) {
    throw new Error(`${file} decoded with ${name} contains NUL characters; it is probably UTF-16, so try encoding: 'utf-16le'`);
  }
  if (text.includes('�')) {
    throw new Error(`${file} decoded with ${name} contains replacement characters (U+FFFD)`);
  }
  return text;
}

/** `\n` line breaks, without the whitespace (and BOM) around the text. */
function normalizeText(text: string): string {
  return text.replace(/\r\n?/g, '\n').trim();
}

const MANIFEST_KEYS = new Set(['file', 'id', 'speaker', 'text', 'spokenText', 'scene', 'expression', 'gap']);

function readManifest(src: string): VoiceManifest {
  const file = entryRelativePath(src);
  trackPrepareDependency(file);
  let raw: string;
  try {
    raw = fs.readFileSync(file, 'utf8');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      throw new Error(`importVoiceFolder(): manifest not found: ${src} (looked at ${file})`);
    }
    throw error;
  }
  let json: unknown;
  try {
    json = JSON.parse(raw.replace(/^﻿/, ''));
  } catch (error) {
    throw new Error(`importVoiceFolder(): manifest ${file} is not valid JSON: ${error instanceof Error ? error.message : String(error)}`);
  }
  const fail = (message: string): never => {
    throw new Error(`importVoiceFolder(): manifest ${file}: ${message}`);
  };
  if (!isObject(json)) fail('expected an object with a `lines` array');
  const manifest = json as Record<string, unknown>;
  for (const key of Object.keys(manifest)) {
    if (key !== 'lines' && key !== '$schema') fail(`unknown key "${key}"`);
  }
  if (!Array.isArray(manifest.lines)) fail('`lines` must be an array');
  const lines = (manifest.lines as unknown[]).map((value, index): VoiceManifestEntry => {
    const at = `lines[${index}]`;
    if (!isObject(value)) fail(`${at} must be an object`);
    const entry = value as Record<string, unknown>;
    for (const key of Object.keys(entry)) {
      if (key === 'reading') fail(`${at}.reading is now spokenText, the name every dialogue line uses`);
      if (!MANIFEST_KEYS.has(key)) fail(`${at} has unknown key "${key}"`);
    }
    if (typeof entry.file !== 'string' || entry.file.length === 0) fail(`${at}.file must be a non-empty string`);
    const fileName = (entry.file as string).replace(/\\/g, '/');
    if (/\.(wav|txt)$/i.test(fileName)) fail(`${at}.file names the pair without its extension: "${fileName.replace(/\.(wav|txt)$/i, '')}"`);
    for (const key of ['id', 'speaker', 'text', 'spokenText', 'scene'] as const) {
      if (entry[key] !== undefined && typeof entry[key] !== 'string') fail(`${at}.${key} must be a string`);
    }
    if (entry.id === '') fail(`${at}.id must not be empty`);
    if (entry.gap !== undefined && (typeof entry.gap !== 'number' || !Number.isFinite(entry.gap) || entry.gap < 0)) {
      fail(`${at}.gap must be a number of seconds >= 0`);
    }
    const expression = entry.expression;
    if (
      expression !== undefined &&
      typeof expression !== 'string' &&
      !(
        Array.isArray(expression) &&
        expression.every(
          (cue) => isObject(cue) && typeof cue.at === 'number' && Number.isFinite(cue.at) && typeof cue.expression === 'string',
        )
      )
    ) {
      fail(`${at}.expression must be a name or a list of { "at": seconds, "expression": name } cues`);
    }
    return { ...(entry as Omit<VoiceManifestEntry, 'file'>), file: fileName.normalize('NFC') };
  });
  return { lines };
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Celesta's per-user cache folder for voice snapshots, beside the editor's audio cache. */
function snapshotRoot(): string {
  const env = process.env;
  if (process.platform === 'darwin' && env.HOME) {
    return path.join(env.HOME, 'Library/Caches/com.natsuneko.celesta/voice-snapshots-v1');
  }
  if (process.platform === 'win32' && env.LOCALAPPDATA) {
    return path.join(env.LOCALAPPDATA, 'Celesta/voice-snapshots-v1');
  }
  if (env.XDG_CACHE_HOME) return path.join(env.XDG_CACHE_HOME, 'celesta/voice-snapshots-v1');
  if (env.HOME) return path.join(env.HOME, '.cache/celesta/voice-snapshots-v1');
  return path.join(os.tmpdir(), 'celesta-voice-snapshots-v1');
}

/**
 * Copies `bytes` to `<hash>/<name>` in the snapshot folder and returns its
 * path. The same recording always maps to the same file, and a resaved one
 * to a new file, so nothing downstream can read a WAV newer than this
 * import's. Keeping `name` shows the original file name in the timeline.
 */
function snapshot(bytes: Uint8Array, name: string): string {
  const folder = path.join(snapshotRoot(), createHash('sha256').update(bytes).digest('hex').slice(0, 32));
  const target = path.join(folder, name);
  if (fs.statSync(target, { throwIfNoEntry: false })?.size === bytes.length) {
    return target;
  }
  fs.mkdirSync(folder, { recursive: true });
  // Another process may publish the same bytes first.
  writeFileAtomicSync(target, bytes, { ifExists: 'same-size' });
  return target;
}
