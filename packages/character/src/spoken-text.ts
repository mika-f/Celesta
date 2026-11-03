// What a line says, apart from what its subtitle shows: `spokenText` is what
// a speech engine reads and what waveform lip sync takes its vowels from,
// so "HPが1" can be shown while "エイチピーがいち" is spoken. It never
// reaches the screen or the subtitle file.

import * as fs from 'node:fs';

import { entryRelativePath, isRemoteUrl, trackPrepareDependency } from '@celesta.studio/react/internal';

import type { DialogueLine } from './dialogue-series';
import { decodeWav, lipSyncFromAudio, vowelShapes } from './lipsync';
import type { WavAudio } from './lipsync';

/**
 * What `line` says: its `spokenText`, or its `text` when that is a string.
 * Throws, naming the line, when neither gives a string.
 */
export function spokenTextOf(line: DialogueLine, id = line.id ?? '?'): string {
  if (line.spokenText !== undefined) {
    if (typeof line.spokenText !== 'string') throw new Error(`line "${id}": spokenText must be a string`);
    return line.spokenText;
  }
  if (typeof line.text !== 'string') {
    throw new Error(`line "${id}": the text holds <Span>s, so give the line a spokenText to read`);
  }
  return line.text;
}

/**
 * @internal The text waveform lip sync spreads over a line's voice: the
 * spoken text, or '' (open and close only) with a warning when it has kanji
 * without an explicit `spokenText`, or no kana or romaji vowels.
 */
export function waveformVowelText(
  spoken: string,
  explicit: boolean,
  id: string,
  caller: string,
  warn: (message: string) => void,
): string {
  if (!explicit && /\p{Script=Han}/u.test(spoken)) {
    warn(
      `${caller}: line "${id}" has kanji and no spokenText, so its mouth only opens and closes; `
        + 'add a kana spokenText for vowel shapes',
    );
    return '';
  }
  if (vowelShapes(spoken).length === 0) {
    warn(`${caller}: line "${id}" has no kana or romaji vowels, so its mouth only opens and closes`);
    return '';
  }
  return spoken;
}

export interface LoadDialogueLipSyncOptions {
  /** Internal analysis rate, as for `loadLipSync()`. Defaults to 100 Hz. */
  hopHz?: number;
  /** Receives warnings. Defaults to `console.warn`. */
  onWarning?: (message: string) => void;
}

/**
 * Gives each line with a local WAV `audio` and no `lipSync` of its own a
 * waveform lip sync, as `loadLipSync()` builds it, from the line's
 * `spokenText` (else its `text`): the subtitle can show "HPが1" while the
 * mouth follows "エイチピーがいち". Call it in `prepare()` before
 * `planDialogue()`. A line whose spoken text has kanji (with no
 * `spokenText`) or no vowels only opens and closes its mouth, with a
 * warning. Lines without `audio`, or with a `lipSync`, are returned as
 * they are; a remote or non-WAV `audio` is an error.
 */
export async function loadDialogueLipSync<L extends DialogueLine>(
  lines: readonly L[],
  options: LoadDialogueLipSyncOptions = {},
): Promise<(L & Pick<DialogueLine, 'lipSync'>)[]> {
  const warn = options.onWarning ?? ((message: string) => console.warn(message));
  const caller = 'loadDialogueLipSync()';
  // The files are read side by side; warnings still come in script order.
  const decoded = await Promise.all(lines.map(async (line, index): Promise<WavAudio | null> => {
    if (line.audio === undefined || line.lipSync !== undefined) return null;
    const id = line.id ?? String(index);
    if (isRemoteUrl(line.audio)) {
      throw new Error(`${caller}: line "${id}": ${line.audio} is not a local file; give the line its own lipSync`);
    }
    spokenTextOf(line, id);
    const file = entryRelativePath(line.audio);
    trackPrepareDependency(file);
    try {
      return decodeWav(await fs.promises.readFile(file));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      throw new Error(`${caller}: line "${id}": could not read ${line.audio} as a WAV (${message})`);
    }
  }));
  return lines.map((line, index): L & Pick<DialogueLine, 'lipSync'> => {
    const audio = decoded[index];
    if (!audio) return line;
    const id = line.id ?? String(index);
    const vowels = waveformVowelText(spokenTextOf(line, id), line.spokenText !== undefined, id, caller, warn);
    return { ...line, lipSync: lipSyncFromAudio(audio, vowels, options.hopHz) };
  });
}
