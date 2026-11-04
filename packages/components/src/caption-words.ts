// Word timing for CaptionTrack: a cue's text split into the words (or
// Japanese phrases) a highlight lights one at a time, each with the frames
// it is spoken on, and those words grouped into pages that fit the band.
// Everything counts grapheme clusters of the cue's text, as emphasis does.

import { graphemes, phraseBoundaries } from '@celesta.studio/text';

/** When one word of a cue is spoken. */
export interface CaptionWord {
  /**
   * The word as it appears in the cue's `text`. Words are found in order;
   * whatever lies between two of them, such as punctuation, goes with the
   * word before. Space around the word is ignored.
   */
  text: string;
  /** First frame the word is spoken on, counted like the cue's `from`. */
  from: number;
  /** Frames it is spoken. Defaults to until the next word starts, or the cue ends. */
  durationInFrames?: number;
}

/** How a cue's text is split into the words the highlight lights. */
export type CaptionWordUnit = 'word' | 'phrase';

/** Where `CaptionTrack` starts a new page of a cue. */
export interface CaptionPaging {
  /** Most words on a page. Defaults to no limit; the track's `maxLines` limits the lines. */
  maxWords?: number;
  /** Starts a new page after a word that ends a sentence (`。！？.!?…`). Defaults to true. */
  sentences?: boolean;
  /** Starts a new page where no word is spoken for at least this many frames. */
  pause?: number;
}

/** A word of a cue, as CaptionTrack lights and pages it. */
export interface TimedWord {
  /** The word's clusters in the cue's text, `[start, end)`, without the space after it. */
  start: number;
  end: number;
  /** The first frame it is lit, in the track's frames. */
  from: number;
  /** The frame it stops being lit. */
  until: number;
}

/** A page: the words `[first, last]`, and the clusters of the cue's text it shows. */
export interface CaptionPage {
  first: number;
  last: number;
  start: number;
  end: number;
  /** The first frame it is shown, in the track's frames. */
  from: number;
}

const SPACE = /^\s+$/u;
const SENTENCE_END = /[。．！？!?.…‼⁇⁈⁉][」』）)"'”’\]］]*$/u;

/**
 * The cue's words: the `words` given, found in `text`, or else `text` split
 * at spaces (and, for `'phrase'`, between Japanese phrases) with the cue's
 * frames shared out by length. With `'phrase'`, given words that do not
 * start a phrase join the word before.
 */
export function timedWords(
  text: string,
  cue: { from: number; end: number; words?: readonly CaptionWord[] },
  unit: CaptionWordUnit,
  what: string,
): TimedWord[] {
  const clusters = graphemes(text);
  // `at[k]` is where cluster k starts in UTF-16 code units; the last entry is the text's length.
  const at = [0];
  for (const cluster of clusters) at.push(at[at.length - 1] + cluster.length);
  const starts = startsOf(clusters, unit);
  const trimmed = (start: number, end: number) => {
    while (end > start && SPACE.test(clusters[end - 1])) end -= 1;
    return end;
  };

  if (cue.words === undefined) {
    const begin = starts.filter((index) => index < clusters.length);
    const spans = begin.map((start, index) => ({ start, end: trimmed(start, begin[index + 1] ?? clusters.length) }));
    const total = spans.reduce((sum, span) => sum + span.end - span.start, 0);
    let before = 0;
    const froms = spans.map((span) => {
      const from = cue.from + Math.round(((cue.end - cue.from) * before) / Math.max(1, total));
      before += span.end - span.start;
      return from;
    });
    return spans.map((span, index) => ({ ...span, from: froms[index], until: froms[index + 1] ?? cue.end }));
  }

  if (!Array.isArray(cue.words)) throw new Error(`${what}: words must be a list of { text, from }`);
  const index = new Map(at.map((offset, cluster) => [offset, cluster]));
  const isStart = new Set(starts);
  const found: { start: number; from: number; until?: number }[] = [];
  let search = 0;
  cue.words.forEach((word, i) => {
    const where = `${what}: word ${i}`;
    if (word === null || typeof word !== 'object' || typeof word.text !== 'string' || word.text.trim() === '') {
      throw new Error(`${where} needs a non-empty \`text\``);
    }
    const { from, durationInFrames } = word;
    if (!Number.isInteger(from) || from < cue.from || from >= cue.end) {
      throw new Error(`${where} ("${word.text}") from must be a whole frame inside the cue (${cue.from}–${cue.end - 1}), not ${from}`);
    }
    if (durationInFrames !== undefined && (!Number.isInteger(durationInFrames) || durationInFrames < 1)) {
      throw new Error(`${where} ("${word.text}") durationInFrames must be a whole number of at least 1, not ${durationInFrames}`);
    }
    const previous = found[found.length - 1];
    if (previous && from < previous.from) {
      throw new Error(`${where} ("${word.text}") starts at ${from}, before word ${i - 1} at ${previous.from}`);
    }
    const offset = text.indexOf(word.text.trim(), search);
    const start = index.get(offset);
    if (offset < 0 || start === undefined || !index.has(offset + word.text.trim().length)) {
      throw new Error(`${where} "${word.text.trim()}" is not in the cue's text "${text}" after the words before it`);
    }
    search = offset + word.text.trim().length;
    const until = durationInFrames === undefined ? undefined : Math.min(cue.end, from + durationInFrames);
    if (previous && unit === 'phrase' && !isStart.has(start)) {
      // Not the start of a phrase: the word joins the one before.
      previous.until = until;
      return;
    }
    found.push({ start, from, ...(until !== undefined ? { until } : {}) });
  });
  return found.map((word, i) => {
    const next = found[i + 1];
    return {
      start: word.start,
      end: trimmed(word.start, next?.start ?? clusters.length),
      from: word.from,
      until: Math.min(word.until ?? next?.from ?? cue.end, next?.from ?? cue.end),
    };
  });
}

/** The clusters a word may start at: after a space, and for `'phrase'` where a Japanese phrase starts. */
function startsOf(clusters: readonly string[], unit: CaptionWordUnit): number[] {
  const starts = new Set<number>();
  clusters.forEach((cluster, index) => {
    if (!SPACE.test(cluster) && (index === 0 || SPACE.test(clusters[index - 1]))) starts.add(index);
  });
  if (unit === 'phrase') {
    // Phrase boundaries count code points; keep those that fall between clusters.
    const byCodePoint = new Map<number, number>();
    let codePoints = 0;
    clusters.forEach((cluster, index) => {
      byCodePoint.set(codePoints, index);
      codePoints += Array.from(cluster).length;
    });
    for (const boundary of phraseBoundaries(clusters.join(''))) {
      const index = byCodePoint.get(boundary);
      if (index !== undefined && !SPACE.test(clusters[index])) starts.add(index);
    }
  }
  return [...starts].sort((a, b) => a - b);
}

/**
 * The words grouped into pages, in order: a page takes words until the next
 * would break one of `rules` or not `fits()`; a word too long for a page of
 * its own gets one anyway. The first page starts at the text's start and is
 * shown from `cueFrom`.
 */
export function paginate(
  clusters: readonly string[],
  words: readonly TimedWord[],
  rules: { maxWords: number; sentences: boolean; pause?: number },
  fits: (start: number, end: number) => boolean,
  cueFrom: number,
): CaptionPage[] {
  const pages: CaptionPage[] = [];
  let first = 0;
  while (first < words.length) {
    const start = pages.length === 0 ? 0 : words[first].start;
    let last = first;
    for (let next = first + 1; next < words.length; next += 1) {
      const before = words[next - 1];
      if (next - first >= rules.maxWords) break;
      if (rules.sentences && SENTENCE_END.test(clusters.slice(before.start, before.end).join(''))) break;
      if (rules.pause !== undefined && words[next].from - before.until >= rules.pause) break;
      if (!fits(start, words[next].end)) break;
      last = next;
    }
    pages.push({ first, last, start, end: words[last].end, from: pages.length === 0 ? cueFrom : words[first].from });
    first = last + 1;
  }
  return pages;
}
