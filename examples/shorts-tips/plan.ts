import { getProjectProperty } from '@celesta/react';
import { tokenizeCode, type CodeTheme } from '@celesta/code';
import {
  BEAT, CODE, CODE_MAX_LINES, CODE_SIZE, MONO_ADVANCE, REVEAL_AT, REVEAL_BEATS, SAFE, TYPE_FRAMES,
} from './constants';
import { DEMOS, type TipId } from './demos';

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

/** `wide` runs are East Asian text, drawn in the Japanese face (FONT.ja). */
export type CodeRun = { text: string; color: string; wide: boolean };
export type CodeLine = {
  text: string;
  runs: CodeRun[];
  /** Code points, for `visibleCharacters`. */
  length: number;
  /** Advance of each code point in em, for the caret. */
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

// JetBrains Mono has no East Asian glyphs. Those characters are drawn in
// the loaded Japanese face rather than whichever system font the fallback
// would pick, so they look the same everywhere and advance exactly 1 em.
const WIDE = /[ᄀ-ᅟ⺀-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦]/u;

// Splits the tokenized snippet into lines of coloured runs.
function codeLines(source: string): CodeLine[] {
  const lines: CodeLine[] = [{ text: '', runs: [], length: 0, advances: [] }];
  for (const token of tokenizeCode(source, 'tsx')) {
    const color = THEME.tokens[token.type] ?? THEME.foreground;
    token.text.split('\n').forEach((part, i) => {
      if (i > 0) lines.push({ text: '', runs: [], length: 0, advances: [] });
      const line = lines[lines.length - 1];
      for (const ch of part) {
        const wide = WIDE.test(ch);
        const last = line.runs[line.runs.length - 1];
        if (last?.color === color && last.wide === wide) last.text += ch;
        else line.runs.push({ text: ch, color, wide });
        line.text += ch;
        line.length += 1;
        line.advances.push(wide ? 1 : MONO_ADVANCE);
      }
    });
  }
  return lines;
}

export function makePlan(code: string, stages: string, tip: TipId): Plan {
  const source = code.replace(/\t/g, '  ').replace(/\s+$/, '');
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
}

export function getPlan(): Plan {
  if (!plan) throw new Error('prepare() has not run');
  return plan;
}
