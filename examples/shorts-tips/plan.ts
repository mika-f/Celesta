import { getProjectProperty, measureText } from '@celesta.studio/react';
import { highlightCode, type CodeTheme, type HighlightedCodeLine } from '@celesta.studio/code';
import {
  BEAT, CODE, CODE_MAX_LINES, CODE_SIZE, FONT_URLS, MONO_ADVANCE, REVEAL_AT, REVEAL_BEATS, SAFE, TYPE_FRAMES,
} from './constants';
import { DEMOS, type TipId } from './demos';
import { SLOTS, type SlotName } from './slots';

// Token colours for the dark code panel. Unlisted token names use foreground.
export const THEME: CodeTheme = {
  foreground: '#E6E9F2',
  highlightLine: '#00000000',
  tokens: {
    keyword: '#FF7AB6', builtin: '#FF7AB6', boolean: '#FFB86C', constant: '#FFB86C', number: '#FFB86C',
    function: '#7FD1FF', tag_name: '#7FD1FF', type: '#7FD1FF', namespace: '#7FD1FF',
    string: '#B5E48C', template: '#B5E48C', regex: '#B5E48C',
    attr_name: '#C9B2FF', property: '#C9B2FF', parameter: '#E6E9F2',
    punctuation: '#8A93AD', operator: '#8A93AD', comment: '#5C6680',
  },
};

// A line as highlightCode() returns it, with what typing and the caret need.
export type CodeLine = HighlightedCodeLine & {
  /** Code points of the original source line: how many there are to type. */
  length: number;
  /** Advance of each code point of `text` (tabs expanded) in em. */
  advances: number[];
};

export type Plan = {
  lines: CodeLine[];
  fontSize: number;
  lineHeight: number;
  /** Absolute frame at which each line starts typing. */
  revealAt: number[];
  /** Absolute frame at which each demo stage starts. */
  stageAt: number[];
};

// JetBrains Mono has no East Asian glyphs; the code's font stack
// (CODE_FONTS) draws those in Noto Sans JP, which advances them by 1 em.
const WIDE = /[ᄀ-ᅟ⺀-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦]/u;

// The snippet's lines and colour runs, split exactly as <Code> splits them
// (tabs expanded, CRLF and lone CR ending a line).
function codeLines(source: string): CodeLine[] {
  return highlightCode(source, { language: 'tsx', theme: THEME }).map((line) => ({
    ...line,
    length: line.columns.length - 1,
    advances: Array.from(line.text, (ch) => (WIDE.test(ch) ? 1 : MONO_ADVANCE)),
  }));
}

export function makePlan(code: string, stages: string, tip: TipId): Plan {
  const source = code.replace(/\s+$/, '');
  const lines = codeLines(source);
  if (!source || lines.length > CODE_MAX_LINES) {
    throw new Error(`code: write 1 to ${CODE_MAX_LINES} lines (got ${source ? lines.length : 0})`);
  }

  // The longest line sets the size; a line too long even at the minimum is an error.
  const widest = Math.max(...lines.map((line) => line.advances.reduce((a, b) => a + b, 0)));
  const room = SAFE.right - CODE.textX;
  const fontSize = Math.min(CODE_SIZE.max, Math.floor(room / widest));
  if (fontSize < CODE_SIZE.min) {
    const columns = Math.floor(room / CODE_SIZE.min / MONO_ADVANCE);
    throw new Error(`code: the longest line is too wide; keep lines within ${columns} columns`);
  }
  const roomY = CODE.height - CODE.bar - CODE.padY * 2;
  const lineHeight = Math.min(Math.round(fontSize * 1.45), Math.floor(roomY / lines.length));

  // One beat slot per non-blank line, evenly spread; a blank line appears
  // with the line after it.
  const filled = lines.filter((line) => line.length > 0).length;
  const step = Math.max(1, Math.min(3, Math.floor(REVEAL_BEATS / filled))) * BEAT;
  const revealAt: number[] = new Array(lines.length);
  let slot = filled;
  for (let i = lines.length - 1; i >= 0; i--) {
    if (lines[i].length > 0) slot -= 1;
    revealAt[i] = REVEAL_AT + slot * step;
  }

  const { stages: count } = DEMOS[tip];
  const at = stages.split(',').map((s) => Number(s.trim()));
  if (at.length !== count || at.some((n, i) => !Number.isInteger(n) || n < 1 || n > lines.length || n < (at[i - 1] ?? 1))) {
    throw new Error(
      `stages: the "${tip}" demo needs ${count} line numbers in order, each 1–${lines.length} (got "${stages}")`,
    );
  }
  const stageAt = at.map((n) => revealAt[n - 1] + TYPE_FRAMES);

  return { lines, fontSize, lineHeight, revealAt, stageAt };
}

let plan: Plan | undefined;

// Reads the variant's code once and checks it before the first frame, so a
// bad variant fails here with a clear message instead of on some frame.
export async function prepare(): Promise<void> {
  plan = makePlan(
    getProjectProperty<string>('code'),
    getProjectProperty<string>('stages'),
    getProjectProperty<TipId>('tip'),
  );
  await checkSlots();
}

// The title, hook and closing line wrap, so whether they fit their slot is
// only known by shaping them. Exports always run this check, and so does
// scripts/inspect.mjs when it evaluates frames through an exporter. Without
// one (or with --no-native) it cannot shape text: then the check is skipped
// with a warning and the rest of the inspection still runs.
async function checkSlots(): Promise<void> {
  for (const [name, slot] of Object.entries(SLOTS) as [SlotName, (typeof SLOTS)[SlotName]][]) {
    const text = getProjectProperty<string>(name);
    let lines: number;
    try {
      ({ lines } = await measureText(text, slot.style, { maxWidth: slot.maxWidth, fonts: [...FONT_URLS] }));
    } catch (error) {
      console.warn(`not checking how many lines the title, hook and closing take: ${error instanceof Error ? error.message : error}`);
      return;
    }
    if (lines > slot.maxLines) {
      throw new Error(`${name}: wraps to ${lines} lines, but ${slot.maxLines} fit; shorten it or break it with \\n`);
    }
  }
}

export function getPlan(): Plan {
  if (!plan) throw new Error('prepare() has not run');
  return plan;
}
