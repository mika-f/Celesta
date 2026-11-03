// A line's subtitle shown a page at a time while its one voice plays on:
// pages are timed on the line's own clock, each shows its part of the line's
// text, and emphasis on the line is cut to the page it falls on. Pages change
// only what the subtitle shows, never the voice, the lip sync, or the clock
// a `subtitle.render` band runs on.

import { graphemes, resolveEmphasis, resolveTimedRanges, sliceEmphasis, timedRangeAt } from '@celesta.studio/text';
import type { ResolvedEmphasis, TextEmphasis, TimedRange } from '@celesta.studio/text';

/** How a page's start was found. */
export type SubtitlePageTiming = 'manual' | 'aligned' | 'approximate';

/** One page of a line's subtitle. */
export interface SubtitlePage {
  /**
   * First frame the page shows on, a whole number counted from the line's
   * start (the first frame of its voice).
   */
  from: number;
  /**
   * Frames it shows. Omitted, it lasts until the next page, and the last
   * page until the line ends (through the gap under `holdThroughGap`).
   */
  durationInFrames?: number;
  /**
   * The page's part of the line's `text`. The pages, in order, are the
   * text: each continues where the one before ends, and only whitespace
   * between two pages (a space, a `\n`) may be left out.
   */
  text: string;
  /**
   * How `from` was found: `'manual'` (the default) when an author set it,
   * `'aligned'` from speech timing and an explicit segment mapping,
   * `'approximate'` when frames were shared out by text length.
   */
  timing?: SubtitlePageTiming;
}

/** A piece of a line's text and how many moras of its speech timing say it; see `DialogueLine.subtitleSegments`. */
export interface SubtitleSegment {
  text: string;
  /** A whole number of 0 or more. */
  moras: number;
}

/** A checked page: its frames, and where its text sits in the line's. */
export interface ResolvedSubtitlePage {
  index: number;
  /** Frames `[from, end)` on the line's clock. */
  from: number;
  end: number;
  text: string;
  /** Grapheme clusters `[start, stop)` of the line's text the page shows. */
  start: number;
  stop: number;
  timing: SubtitlePageTiming;
}

/** What `CharacterSubtitle.render` gets for the page it shows. */
export interface SubtitlePageInfo {
  /** The page's index in the line's `subtitlePages`. */
  index: number;
  /** How many pages the line has. */
  count: number;
  /** Frames `[from, from + durationInFrames)` on the line's clock. */
  from: number;
  durationInFrames: number;
  /** Frames since the page started; past its end on a frame no page covers. */
  frame: number;
  text: string;
  timing: SubtitlePageTiming;
}

const WHITESPACE = /^\s+$/u;
const TIMINGS = new Set<SubtitlePageTiming>(['manual', 'aligned', 'approximate']);

function preview(clusters: readonly string[], at: number): string {
  const rest = clusters.slice(at, at + 12).join('');
  return rest === '' ? 'its end' : `"${rest}${at + 12 < clusters.length ? '…' : ''}"`;
}

/**
 * Checks `pages` against the line's `text` and its length in frames, and
 * returns them in order. `what` names the line in errors, such as
 * `planDialogue(): line "danger"`.
 */
export function resolveSubtitlePages(
  text: unknown,
  pages: readonly SubtitlePage[],
  end: number,
  what: string,
): ResolvedSubtitlePage[] {
  if (!Array.isArray(pages)) throw new Error(`${what}: subtitlePages must be a list of { from, durationInFrames?, text }`);
  if (typeof text !== 'string') {
    throw new Error(`${what}: subtitlePages need the line's text as a string; style it with \`emphasis\` instead of <Span>`);
  }
  const label = (index: number) => `page ${index}`;
  const ranges = resolveTimedRanges(pages, { what, label, end, ordered: true });
  const clusters = graphemes(text);
  let at = 0;
  const placed = pages.map((page, index) => {
    if (typeof page.text !== 'string') throw new Error(`${what}: ${label(index)} needs a string \`text\``);
    if (page.timing !== undefined && !TIMINGS.has(page.timing)) {
      throw new Error(`${what}: ${label(index)} timing must be 'manual', 'aligned', or 'approximate', not ${JSON.stringify(page.timing)}`);
    }
    const own = graphemes(page.text);
    while (at < clusters.length && WHITESPACE.test(clusters[at]) && own[0] !== clusters[at]) at += 1;
    const start = at;
    for (const cluster of own) {
      if (clusters[at] !== cluster) {
        throw new Error(
          `${what}: ${label(index)} text "${page.text}" does not continue the line's text, which goes on with ${preview(clusters, start)}`,
        );
      }
      at += 1;
    }
    return { start, stop: at };
  });
  const rest = clusters.slice(at);
  if (rest.some((cluster) => !WHITESPACE.test(cluster))) {
    throw new Error(`${what}: the pages leave out the end of the line's text, ${preview(clusters, at)}`);
  }
  return ranges.map((range: TimedRange): ResolvedSubtitlePage => ({
    index: range.index,
    from: range.from,
    end: range.end,
    text: pages[range.index].text,
    start: placed[range.index].start,
    stop: placed[range.index].stop,
    timing: pages[range.index].timing ?? 'manual',
  }));
}

/**
 * Checks a line's emphasis ranges against its text, without looking up
 * styles (those come from the speaker's `subtitle.emphasisStyles` when it
 * is drawn).
 */
export function checkEmphasisRanges(text: unknown, emphasis: readonly TextEmphasis[] | undefined, what: string): void {
  if (emphasis === undefined) return;
  if (typeof text !== 'string') {
    throw new Error(`${what}: emphasis needs the line's text as a string; style it with emphasis instead of <Span>`);
  }
  const unstyled = Array.isArray(emphasis)
    ? emphasis.map((entry) => (entry !== null && typeof entry === 'object' ? { start: entry.start, end: entry.end, style: {} } : entry))
    : emphasis;
  resolveEmphasis(text, unstyled as readonly TextEmphasis[], { what });
}

/** What a subtitle shows on one frame of its line. */
export interface ShownSubtitle {
  /** The page shown, or on a frame no page covers the one shown last (the first before any). */
  page: ResolvedSubtitlePage;
  /** False on a frame no page covers. */
  covered: boolean;
  emphasis: ResolvedEmphasis[];
}

/** The page on `frame` of the line's clock, with the line's emphasis cut to it. */
export function shownSubtitle(
  pages: readonly ResolvedSubtitlePage[],
  frame: number,
  emphasis: readonly ResolvedEmphasis[],
): ShownSubtitle {
  const range = timedRangeAt(pages.map((page) => ({ index: page.index, from: page.from, end: page.end })), frame);
  let page = range ? pages.find((candidate) => candidate.index === range.index)! : undefined;
  if (!page) {
    const before = pages.filter((candidate) => candidate.from <= frame);
    page = before[before.length - 1] ?? pages[0];
  }
  return { page, covered: range !== null, emphasis: sliceEmphasis(emphasis, page.start, page.stop) };
}
