import type { TextMetrics, TextStyle } from '@celesta/react';
import { measureText } from '@celesta/react';
import type { FitTextStyle } from '@celesta/text';
import { fitText } from '@celesta/text';
import { C, COL, FONT, FONT_SRC } from './constants';
import { textStyle } from './components/Copy';

// Everything here is measured once, in prepare(), with the renderer's own
// shaping and the same fonts the frames draw with. inspect.mjs cannot shape
// text, so each value starts as a rough estimate (every Japanese glyph one em
// wide) that the measurement replaces.

const LANG = 'ja-JP';
const fonts = [FONT_SRC];
const ja = (style: TextStyle): TextStyle => ({ ...style, lang: LANG, lineBreak: 'phrase' });

// HOOK: the caret sits after the last line of the headline.
export const HOOK_SIZE = 168;
export const HOOK_LAST_LINE = '書く。';
export let hookCaretX = HOOK_LAST_LINE.length * HOOK_SIZE;

// FIT: one copy, four boxes; each gets the largest size that fits it.
export const FIT_COPY = 'どんな長さのコピーも、箱に合わせて、いちばん大きく。';
export const FIT_LINE_HEIGHT = 1.2;
const FIT_STYLE: FitTextStyle = { fontFamily: FONT.ja, fontWeight: 900, fill: { type: 'solid', color: C.ink } };
export const FIT_BOXES = [
  { width: 760, height: 560 },
  { width: 460, height: 640 },
  { width: 760, height: 300 },
  { width: 560, height: 460 },
];
// The copy fits the box's inside, this far from its border.
export const FIT_PAD = 24;
const inside = (box: { width: number; height: number }) =>
  ({ width: box.width - FIT_PAD * 2, height: box.height - FIT_PAD * 2 });
export type Fit = { fontSize: number; style: TextStyle; height: number };
export let fits: Fit[] = FIT_BOXES.map((box) => {
  const { width, height } = inside(box);
  // Biggest em square that tiles the box with every character.
  const fontSize = Math.floor(Math.sqrt((width * height) / (FIT_COPY.length * FIT_LINE_HEIGHT)));
  const style = ja({ ...FIT_STYLE, fontSize, lineHeight: fontSize * FIT_LINE_HEIGHT });
  return { fontSize, style, height: Math.ceil((FIT_COPY.length * fontSize) / width) * fontSize * FIT_LINE_HEIGHT };
});

// SPAN: one paragraph whose lit phrase moves; each phrase's highlight is
// placed from the measured glyphs, one bar per line it covers.
export const SPAN_COPY = '同じ一文でも、光らせる言葉で、意味が変わる。';
export const SPAN_STYLE: TextStyle = { ...textStyle(100, 400), lineHeight: 136 };
export type Segment = { x: number; y: number; width: number };
export let spanLayout = estimateLayout(SPAN_COPY, 100, 136, COL.width);

export function segmentsOf(phrase: string): Segment[] {
  const start = SPAN_COPY.indexOf(phrase);
  const glyphs = spanLayout.glyphs.filter((g) => g.start >= start && g.end <= start + phrase.length);
  const lines = [...new Set(glyphs.map((g) => g.line))];
  return lines.map((line) => {
    const inLine = glyphs.filter((g) => g.line === line);
    const x = Math.min(...inLine.map((g) => g.x));
    return { x, y: line * spanLayout.lineHeight, width: Math.max(...inLine.map((g) => g.x + g.width)) - x };
  });
}

export async function measureAll() {
  try {
    const caret = await measureText(HOOK_LAST_LINE, ja(textStyle(HOOK_SIZE, 900)), { fonts });
    hookCaretX = caret.width;
    spanLayout = await measureText(SPAN_COPY, ja(SPAN_STYLE), { maxWidth: COL.width, fonts });
    fits = await Promise.all(FIT_BOXES.map(async (box) => {
      const fit = await fitText(FIT_COPY, {
        ...inside(box), minFontSize: 28, maxFontSize: 220, lineHeight: FIT_LINE_HEIGHT, step: 2,
        style: ja(FIT_STYLE), fonts,
      });
      return { fontSize: fit.fontSize, style: fit.style, height: fit.metrics.height };
    }));
  } catch (error) {
    // Only inspect.mjs lands here; the estimates above keep every frame evaluable.
    if (!String(error).includes('inspect.mjs')) throw error;
  }
}

// Greedy wrapping with one-em glyphs: a stand-in for the real layout.
function estimateLayout(text: string, size: number, lineHeight: number, maxWidth: number): TextMetrics {
  const perLine = Math.floor(maxWidth / size);
  const glyphs = Array.from(text, (ch, i) => ({
    text: ch, start: i, end: i + 1, rtl: false, x: (i % perLine) * size, width: size, line: Math.floor(i / perLine),
  }));
  const lines = Math.ceil(glyphs.length / perLine);
  return { width: maxWidth, height: lines * lineHeight, ascent: lineHeight * 0.8, descent: lineHeight * 0.2, lineHeight, lines, glyphs };
}
