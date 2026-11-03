// Splits long subtitles into pages in `prepare()`: each line's text is
// measured with the subtitle's real style and width, with the same shaping
// and phrase-aware wrapping `<Text>` draws it with, and broken every
// `maxLines` measured lines. Page starts come from speech timing through an
// explicit mapping of the text to moras when there is one, and are otherwise
// shared out by text length and marked approximate.

import { measureText } from '@celesta.studio/react';
import type { TextMetrics, TextStyle, ThemeValues } from '@celesta.studio/react';
import { emphasisContent, graphemes, resolveEmphasis } from '@celesta.studio/text';
import type { EmphasisStyle } from '@celesta.studio/text';

import { lineDuration } from './dialogue-series';
import type { DialogueLine } from './dialogue-series';
import type { SpeechTiming } from './speech-timing';
import type { SubtitlePage } from './subtitle-pages';

export interface PaginateSubtitlesOptions {
  /** The composition's frame rate, as for `planDialogue()`. */
  fps: number;
  /** The subtitle's text style: the speaker's `subtitle.style`. */
  style: TextStyle;
  /** Width the subtitle wraps at: the speaker's `subtitle.maxWidth`. */
  maxWidth: number;
  /** Most measured lines on a page, a whole number of at least 1. Defaults to 2. */
  maxLines?: number;
  /** Tallest a page may be, in pixels; limits the lines a page holds as `maxLines` does. */
  maxHeight?: number;
  /** Font files to load before measuring, as for `measureText()`. */
  fonts?: string[];
  /** `<Theme>` font and color names the style and emphasis use. */
  theme?: ThemeValues;
  /** The speaker's `subtitle.emphasisStyles`, so emphasis is measured at its weight. */
  emphasisStyles?: Readonly<Record<string, EmphasisStyle>>;
  /**
   * The speech timing of a line, such as `timingFromVoicevox(query)`. With
   * it and the line's `subtitleSegments`, pages break between segments and
   * start on their first mora.
   */
  speechTiming?: (line: DialogueLine) => SpeechTiming | undefined;
  /**
   * A measured line wider than `maxWidth` (a word that cannot wrap):
   * `'error'` (the default) fails, naming the line; `'keep'` leaves it on
   * its page, overflowing.
   */
  overlong?: 'error' | 'keep';
}

const WHITESPACE = /^\s+$/u;

function fail(id: string, message: string): never {
  throw new Error(`paginateSubtitles(): line "${id}": ${message}`);
}

/** The code-point offset each cluster starts at, and the text's length in code points. */
function clusterStarts(clusters: readonly string[]): number[] {
  const starts = [0];
  for (const cluster of clusters) starts.push(starts[starts.length - 1] + Array.from(cluster).length);
  return starts;
}

/** The measured lines' widths, from the glyphs on each; a space hanging at a line's end does not count. */
function lineWidths(metrics: TextMetrics): number[] {
  const widths = new Array<number>(metrics.lines).fill(0);
  for (const glyph of metrics.glyphs) {
    if (glyph.line >= 0 && glyph.line < widths.length && !WHITESPACE.test(glyph.text)) {
      widths[glyph.line] = Math.max(widths[glyph.line], glyph.x + glyph.width);
    }
  }
  return widths;
}

/** The text of clusters `[start, stop)` without whitespace at either end. */
function pageText(clusters: readonly string[], start: number, stop: number): string {
  let from = start;
  let to = stop;
  while (from < to && WHITESPACE.test(clusters[from])) from += 1;
  while (to > from && WHITESPACE.test(clusters[to - 1])) to -= 1;
  return clusters.slice(from, to).join('');
}

/** Cluster boundaries between segments, and the mora each segment starts on. */
function segmentBoundaries(
  line: DialogueLine,
  id: string,
  clusters: readonly string[],
  timing: SpeechTiming,
): Map<number, number> {
  const segments = line.subtitleSegments!;
  if (!Array.isArray(segments)) fail(id, 'subtitleSegments must be a list of { text, moras }');
  const boundaries = new Map<number, number>();
  let at = 0;
  let mora = 0;
  segments.forEach((segment, index) => {
    if (typeof segment?.text !== 'string') fail(id, `segment ${index} needs a string \`text\``);
    if (!Number.isInteger(segment.moras) || segment.moras < 0) {
      fail(id, `segment ${index} ("${segment.text}") needs a whole number of moras, got ${String(segment.moras)}`);
    }
    boundaries.set(at, mora);
    for (const cluster of graphemes(segment.text)) {
      if (clusters[at] !== cluster) {
        fail(id, `segment ${index} ("${segment.text}") does not continue the line's text at cluster ${at}`);
      }
      at += 1;
    }
    mora += segment.moras;
  });
  if (at !== clusters.length) fail(id, `the segments cover ${at} of the text's ${clusters.length} grapheme clusters`);
  if (mora !== timing.moras.length) {
    fail(id, `the segments give ${mora} moras, but the speech timing has ${timing.moras.length}`);
  }
  boundaries.set(at, mora);
  return boundaries;
}

/**
 * Gives each line whose subtitle measures more than `maxLines` lines at
 * `maxWidth` its `subtitlePages`, a page per `maxLines` lines, and returns
 * the lines in order. Call it in `prepare()` before `planDialogue()`, with
 * the speaker's subtitle `style` and `maxWidth`.
 *
 * - Pages break where the measured lines do (`lineBreak: 'phrase'` keeps
 *   Japanese phrases whole), never inside a grapheme cluster, and drop only
 *   the whitespace at a break. A `\n` in the text stays a line break.
 * - With `speechTiming(line)` and the line's `subtitleSegments`, a break
 *   moves back to the last segment boundary before it, and the page starts
 *   on its first mora's frame: `timing: 'aligned'`. Otherwise the voice's
 *   frames are shared out by the pages' non-space clusters, an estimate,
 *   marked `timing: 'approximate'`.
 * - Every page starts where the one before ends, and the last one runs to
 *   the end of the line, so they follow the voice when its length changes
 *   on the next `prepare()`.
 * - Lines that fit, lines with `subtitlePages` of their own, and lines
 *   whose `text` is not a string are returned as they are.
 */
export async function paginateSubtitles<L extends DialogueLine>(
  lines: readonly L[],
  options: PaginateSubtitlesOptions,
): Promise<(L & Pick<DialogueLine, 'subtitlePages'>)[]> {
  const { fps, style, maxWidth, maxLines = 2, maxHeight, fonts, theme, emphasisStyles, overlong = 'error' } = options;
  if (!Number.isFinite(fps) || fps <= 0) throw new Error('paginateSubtitles() requires a positive finite fps');
  if (!Number.isFinite(maxWidth) || maxWidth <= 0) throw new Error('paginateSubtitles() requires a positive maxWidth');
  if (!Number.isInteger(maxLines) || maxLines < 1) throw new Error('paginateSubtitles(): maxLines must be a whole number of at least 1');
  if (maxHeight !== undefined && (!Number.isFinite(maxHeight) || maxHeight <= 0)) {
    throw new Error('paginateSubtitles(): maxHeight must be a positive number of pixels');
  }
  const resolvedTheme = { fonts: theme?.fonts ?? {}, colors: theme?.colors ?? {} };
  return Promise.all(lines.map(async (line, index): Promise<L & Pick<DialogueLine, 'subtitlePages'>> => {
    const id = line.id ?? String(index);
    const { text } = line;
    if (line.subtitlePages !== undefined || typeof text !== 'string') return line;
    const emphasis = resolveEmphasis(text, line.emphasis, { styles: emphasisStyles, theme: resolvedTheme, what: `paginateSubtitles(): line "${id}"` });
    const metrics = await measureText(emphasisContent(text, emphasis), style, { maxWidth, fonts, theme });
    const perPage = Math.min(maxLines, maxHeight === undefined ? Infinity : Math.floor((maxHeight + 1e-3) / metrics.lineHeight));
    if (perPage < 1) fail(id, `maxHeight ${maxHeight} px is less than one line (${metrics.lineHeight} px)`);
    if (overlong === 'error') {
      lineWidths(metrics).forEach((width, row) => {
        if (width > maxWidth + 1e-3) {
          const start = metrics.lineStarts?.[row] ?? 0;
          const end = metrics.lineStarts?.[row + 1] ?? undefined;
          const shown = Array.from(text).slice(start, end).join('').trim();
          fail(id, `"${shown}" is ${Math.round(width)} px wide, wider than maxWidth ${maxWidth}; shorten it, or pass overlong: 'keep'`);
        }
      });
    }
    if (metrics.lines <= perPage) return line;
    const lineStarts = metrics.lineStarts;
    if (!lineStarts || lineStarts.length !== metrics.lines) {
      fail(id, 'the text measurer gave no line starts, so the text cannot be split by its measured lines');
    }
    const clusters = graphemes(text);
    const starts = clusterStarts(clusters);
    // A measured line start as a cluster boundary, never inside a cluster.
    const toCluster = (codePoint: number) => {
      const found = starts.findIndex((start) => start >= codePoint);
      return found < 0 ? clusters.length : found;
    };
    let breaks: number[] = [];
    for (let row = perPage; row < metrics.lines; row += perPage) breaks.push(toCluster(lineStarts[row]));

    const timing = line.subtitleSegments !== undefined ? options.speechTiming?.(line) : undefined;
    const duration = await lineDuration(line, id, fps);
    let froms: number[];
    let kind: SubtitlePage['timing'];
    if (timing) {
      const boundaries = segmentBoundaries(line, id, clusters, timing);
      const usable = [...boundaries.keys()].sort((a, b) => a - b);
      const snapped: number[] = [];
      let previous = 0;
      for (const wanted of breaks) {
        let choice = [...usable].reverse().find((boundary) => boundary <= wanted && boundary > previous);
        if (choice === undefined) {
          if (overlong === 'error') {
            fail(id, `a segment from cluster ${previous} runs past a page of ${perPage} line${perPage === 1 ? '' : 's'}; split it, or pass overlong: 'keep'`);
          }
          choice = usable.find((boundary) => boundary > previous);
        }
        if (choice === undefined || choice >= clusters.length) break;
        if (snapped[snapped.length - 1] !== choice) snapped.push(choice);
        previous = choice;
      }
      breaks = snapped;
      froms = [0, ...breaks.map((at) => {
        const mora = timing.moras[boundaries.get(at)!];
        return Math.round((mora ? mora.start : timing.durationInSeconds) * fps);
      })];
      kind = 'aligned';
    } else {
      const edges = [0, ...breaks, clusters.length];
      const weights = edges.slice(1).map((end, page) =>
        Math.max(1, clusters.slice(edges[page], end).filter((cluster) => !WHITESPACE.test(cluster)).length));
      const total = weights.reduce((sum, weight) => sum + weight, 0);
      let sum = 0;
      froms = weights.map((weight) => {
        const from = Math.round((sum / total) * duration);
        sum += weight;
        return from;
      });
      kind = 'approximate';
    }
    // Two pages may not start together; a page pushed past the voice's end has no frames.
    for (let page = 1; page < froms.length; page += 1) froms[page] = Math.max(froms[page], froms[page - 1] + 1);
    if (froms[froms.length - 1] >= duration) {
      fail(id, `its ${froms.length} pages do not fit its ${duration} frames`);
    }
    const edges = [0, ...breaks, clusters.length];
    const subtitlePages: SubtitlePage[] = froms.map((from, page) => ({
      from,
      text: pageText(clusters, edges[page], edges[page + 1]),
      timing: kind,
    }));
    return { ...line, subtitlePages };
  }));
}
