// SRT and WebVTT subtitle files, read into frame-timed cues and written back
// out. The cues have the `from` / `durationInFrames` shape a `<Sequence>`
// takes, so a parsed file drives captions directly; `<Subtitles>` declares
// them for the exporter's `--subtitles` file.

import * as React from 'react';

/** A file format `parseSubtitles()` reads and `formatSubtitles()` writes. */
export type SubtitleFormat = 'srt' | 'vtt';

/** One timed subtitle: shown on frames `[from, from + durationInFrames)`. */
export interface SubtitleCue {
  /** The cue's identifier in the file: an SRT index or a WebVTT cue id. */
  id?: string;
  from: number;
  durationInFrames: number;
  /** Lines are separated by `\n`. */
  text: string;
}

export interface ParseSubtitlesOptions {
  /** The composition's frame rate, which the file's times are rounded to. */
  fps: number;
  /** The file's format; by default a file starting with `WEBVTT` is WebVTT, any other SRT. */
  format?: SubtitleFormat;
}

export interface FormatSubtitlesOptions {
  /** The frame rate the cues' frames count at. */
  fps: number;
  format: SubtitleFormat;
}

// `HH:MM:SS,mmm`, `HH:MM:SS.mmm`, or WebVTT's `MM:SS.mmm`. Hours may have any
// number of digits, and the fraction one to three or more.
const TIME = String.raw`(?:(\d+):)?(\d{1,2}):(\d{1,2})(?:[.,](\d+))?`;
const TIMING = new RegExp(String.raw`^\s*${TIME}\s*-->\s*${TIME}(?:\s.*)?$`);

/**
 * Reads an SRT or WebVTT file into cues, in the file's order. Times are
 * rounded to the nearest frame at `fps`; a cue keeps at least one frame.
 *
 * Accepts a byte order mark, CRLF or CR line ends, cues of several lines,
 * `,` or `.` before the milliseconds, a missing hour, and WebVTT cue
 * settings, `NOTE`, `STYLE`, and `REGION` blocks. Markup is removed: SRT's
 * `<i>`/`<b>`/`<u>`/`<font>` tags and `{\an8}`-style overrides, and every
 * WebVTT tag (ruby text included), with WebVTT's character references
 * decoded. Cues with no text are left out.
 *
 * ```tsx
 * import captions from './captions.srt';
 * const cues = parseSubtitles(captions, { fps: 30 });
 * ```
 */
export function parseSubtitles(source: string, options: ParseSubtitlesOptions): SubtitleCue[] {
  const { fps } = options;
  if (!Number.isFinite(fps) || fps <= 0) {
    throw new Error('parseSubtitles() requires a positive fps');
  }
  if (typeof source !== 'string') {
    throw new Error('parseSubtitles() requires the file\'s text');
  }
  const lines = source.replace(/^﻿/, '').split(/\r\n|\r|\n/);
  const format = options.format ?? (/^WEBVTT(?:[ \t]|$)/.test(lines[0] ?? '') ? 'vtt' : 'srt');
  const cues: SubtitleCue[] = [];
  let index = 0;
  if (format === 'vtt' && /^WEBVTT(?:[ \t]|$)/.test(lines[0] ?? '')) {
    // The header runs to the first blank line.
    while (index < lines.length && lines[index].trim() !== '') index += 1;
  }
  while (index < lines.length) {
    while (index < lines.length && lines[index].trim() === '') index += 1;
    if (index >= lines.length) break;
    const start = index;
    while (index < lines.length && lines[index].trim() !== '') index += 1;
    const block = lines.slice(start, index);
    if (format === 'vtt' && /^(?:NOTE|STYLE|REGION)(?:[ \t]|$)/.test(block[0])) {
      continue;
    }
    const timingIndex = block.findIndex((line) => line.includes('-->'));
    if (timingIndex < 0 || timingIndex > 1) {
      throw new Error(
        `parseSubtitles(): line ${start + 1}: expected a timing line such as "00:00:01,000 --> 00:00:02,500"`,
      );
    }
    const lineNumber = start + timingIndex + 1;
    const timing = TIMING.exec(block[timingIndex]);
    if (!timing) {
      throw new Error(`parseSubtitles(): line ${lineNumber}: cannot read the times in "${block[timingIndex].trim()}"`);
    }
    const startSeconds = seconds(timing.slice(1, 5), lineNumber);
    const endSeconds = seconds(timing.slice(5, 9), lineNumber);
    if (endSeconds <= startSeconds) {
      throw new Error(`parseSubtitles(): line ${lineNumber}: the cue ends before it starts`);
    }
    const raw = block.slice(timingIndex + 1).map((line) => line.trimEnd()).join('\n');
    const text = (format === 'vtt' ? vttText(raw) : srtText(raw)).trim();
    if (text === '') continue;
    const from = Math.round(startSeconds * fps);
    const end = Math.round(endSeconds * fps);
    const id = timingIndex === 1 ? block[0].trim() : '';
    cues.push({
      ...(id !== '' ? { id } : {}),
      from,
      durationInFrames: Math.max(1, end - from),
      text,
    });
  }
  return cues;
}

function seconds(parts: (string | undefined)[], lineNumber: number): number {
  const [hours, minutes, wholeSeconds, fraction] = parts;
  const m = Number(minutes);
  const s = Number(wholeSeconds);
  if (m >= 60 || s >= 60) {
    throw new Error(`parseSubtitles(): line ${lineNumber}: minutes and seconds must be below 60`);
  }
  return Number(hours ?? 0) * 3600 + m * 60 + s + (fraction ? Number(`0.${fraction}`) : 0);
}

function srtText(text: string): string {
  return text.replace(/<\/?(?:b|i|u|s|font)\b[^>]*>/gi, '').replace(/\{\\[^}]*\}/g, '');
}

const ENTITIES: Record<string, string> = {
  amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ', lrm: '‎', rlm: '‏',
};

function vttText(text: string): string {
  return text
    .replace(/<rt\b[^>]*>[\s\S]*?(?:<\/rt>|(?=<\/ruby>)|$)/g, '')
    .replace(/<[^>]*>/g, '')
    .replace(/&(?:#(\d+)|#x([0-9a-fA-F]+)|([a-zA-Z]+));/g, (match, decimal, hex, name) => {
      if (decimal || hex) {
        const code = decimal ? Number(decimal) : parseInt(hex, 16);
        return code <= 0x10ffff ? String.fromCodePoint(code) : match;
      }
      return ENTITIES[name] ?? match;
    });
}

/**
 * Writes cues as an SRT or WebVTT file, ordered by `from`. SRT cues are
 * numbered from 1; WebVTT keeps each cue's `id`. Blank lines inside a cue's
 * text are dropped (they would end the cue), and WebVTT escapes `&`, `<`,
 * and `>`. Cues with no text are left out.
 */
export function formatSubtitles(cues: readonly SubtitleCue[], options: FormatSubtitlesOptions): string {
  const { fps, format } = options;
  if (!Number.isFinite(fps) || fps <= 0) {
    throw new Error('formatSubtitles() requires a positive fps');
  }
  if (format !== 'srt' && format !== 'vtt') {
    throw new Error('formatSubtitles() requires format "srt" or "vtt"');
  }
  const blocks: string[] = [];
  const sorted = cues.map((cue, index) => {
    checkCue(cue, index, 'formatSubtitles()');
    return cue;
  }).sort((a, b) => a.from - b.from);
  for (const cue of sorted) {
    const text = cueText(cue.text, format);
    if (text === '') continue;
    const start = timestamp(cue.from / fps, format);
    const end = timestamp((cue.from + cue.durationInFrames) / fps, format);
    const id = format === 'srt'
      ? String(blocks.length + 1)
      : cue.id !== undefined && cue.id !== '' && !/-->|[\r\n]/.test(cue.id) ? cue.id : undefined;
    blocks.push([...(id !== undefined ? [id] : []), `${start} --> ${end}`, text].join('\n'));
  }
  const body = blocks.map((block) => `${block}\n`).join('\n');
  return format === 'vtt' ? `WEBVTT\n\n${body}` : body;
}

function cueText(text: string, format: SubtitleFormat): string {
  const lines = text.split(/\r\n|\r|\n/).map((line) => line.trimEnd()).filter((line) => line.trim() !== '');
  const joined = lines.join('\n');
  return format === 'vtt' ? joined.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;') : joined;
}

/** `HH:MM:SS,mmm` (SRT) or `HH:MM:SS.mmm` (WebVTT), rounded to the millisecond. */
function timestamp(secondsValue: number, format: SubtitleFormat): string {
  const total = Math.max(0, Math.round(secondsValue * 1000));
  const pad = (value: number, width = 2) => String(value).padStart(width, '0');
  const hours = Math.floor(total / 3_600_000);
  const minutes = Math.floor(total / 60_000) % 60;
  const secondsPart = Math.floor(total / 1000) % 60;
  return `${pad(hours)}:${pad(minutes)}:${pad(secondsPart)}${format === 'srt' ? ',' : '.'}${pad(total % 1000, 3)}`;
}

/** @internal Throws unless `cue` is a well-formed `SubtitleCue`. */
export function checkCue(cue: unknown, index: number, what: string): asserts cue is SubtitleCue {
  const value = cue as Partial<SubtitleCue> | null;
  if (value === null || typeof value !== 'object') {
    throw new Error(`${what}: cue ${index} is not an object`);
  }
  if (typeof value.from !== 'number' || !Number.isFinite(value.from)) {
    throw new Error(`${what}: cue ${index} requires a finite \`from\``);
  }
  if (typeof value.durationInFrames !== 'number' || !(value.durationInFrames > 0) ||
      !Number.isFinite(value.durationInFrames)) {
    throw new Error(`${what}: cue ${index} requires a positive \`durationInFrames\``);
  }
  if (typeof value.text !== 'string') {
    throw new Error(`${what}: cue ${index} requires a string \`text\``);
  }
}

export interface SubtitlesProps {
  /** Frames count from the enclosing `<Sequence>`'s start, as its children's do. */
  cues: readonly SubtitleCue[];
  /**
   * The cues' language, a BCP 47 tag such as `'en'` or `'pt-BR'`. A
   * `--subtitles` path with `{lang}` writes each language to its own file,
   * with the tag in place of `{lang}`. It is not inherited from a
   * `<Composition>`'s or `<Sequence>`'s `lang`.
   */
  lang?: string;
}

/**
 * Declares cues for the subtitle files an export writes with `--subtitles`;
 * it draws nothing. Each cue goes in for the frames it is shown, cut to the
 * enclosing `<Sequence>`, the way `<Audio>` plays only inside it. `<Dialogue>`
 * lines go in the same files on their own, in no language.
 *
 * ```tsx
 * <Subtitles lang="en" cues={english} />
 * <Subtitles lang="ja" cues={japanese} />
 * ```
 */
export function Subtitles(props: SubtitlesProps): ReturnType<typeof React.createElement> {
  return React.createElement('subtitles', props);
}
