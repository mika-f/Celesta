import type { TextMetrics, TextStyle } from '@celesta/react';
import { measureText } from '@celesta/react';
import type { FitTextStyle } from '@celesta/text';
import { fitText } from '@celesta/text';
import { C, COL, FONT, FONT_SRC } from './constants';
import { textStyle } from './components/Copy';

// Everything here is measured once, in prepare(), with the renderer's own
// shaping and the same fonts the frames draw with. inspect.mjs cannot shape
// text, so each value starts as a rough estimate (Archivo Black capitals are
// about 0.86 em wide, Inter about 0.55 em) that the measurement replaces.

const fonts = [FONT_SRC];
const DISPLAY: FitTextStyle = { fontFamily: FONT.display, fontWeight: 400 };

// A headline line set as large as the column allows, up to `max`.
export type Line = { text: string; accent?: string; size: number; width: number };

function estimateLine(text: string, max: number, width: number = COL.width): Line {
  const size = Math.min(max, Math.floor(width / (text.length * 0.86)));
  return { text, size, width: text.length * size * 0.86 };
}

function lines(spec: [text: string, max: number, accent?: string][], width: number = COL.width): Line[] {
  return spec.map(([text, max, accent]) => ({ ...estimateLine(text, max, width), accent }));
}

async function fitLine(line: Line, max: number, width: number = COL.width): Promise<Line> {
  const fit = await fitText(line.text, {
    width, height: 4000, minFontSize: 40, maxFontSize: max, maxLines: 1, step: 2, style: DISPLAY, fonts,
  });
  return { ...line, size: fit.fontSize, width: fit.metrics.width };
}

// Stacked headlines: each line fitted to the column on its own.
const HOOK_SPEC: [string, number, string?][] = [['WRITE', 240], ['VIDEO', 240], ['IN CODE.', 240, 'CODE']];
const FRAME_SPEC: [string, number, string?][] = [['A FUNCTION', 200], ['OF ITS', 200], ['NUMBER.', 200, 'NUMBER']];
const OUTRO_SPEC: [string, number, string?][] = [['WRITE VIDEO', 200], ['IN CODE.', 200, 'CODE']];
export let hookLines = lines(HOOK_SPEC);
export let frameLines = lines(FRAME_SPEC);
export let outroLines = lines(OUTRO_SPEC);

// BEAT: one word per beat, capped so short words do not fill the screen, and
// fitted a little narrower than the column so the beat's punch still fits.
const BEAT_WIDTH = 700;
const BEAT_SPEC: [string, number][] = [
  ['CUT', 340], ['ON', 300], ['THE', 280], ['BEAT.', 300], ['WRITE.', 260], ['SAVE.', 300], ['WATCH.', 240], ['SHIP.', 340],
];
export let beatWords = lines(BEAT_SPEC, BEAT_WIDTH);

// MEASURE: a word every other beat, fitted, then measured glyph by glyph.
export const MEASURE_WORDS = ['GLYPHS', 'WIDTH', 'KERN', 'MEASURED.'];
export type Measured = { text: string; size: number; metrics: TextMetrics };
export let measured: Measured[] = MEASURE_WORDS.map((text) => {
  const { size } = estimateLine(text, 260);
  return { text, size, metrics: estimateLayout(text, size, size * 1.2, Infinity, 0.86) };
});

// FIT: one copy, four boxes; each gets the largest size that fits it.
export const FIT_COPY = 'Any copy, any box, as large as it fits.';
export const FIT_LINE_HEIGHT = 1.1;
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
const FIT_STYLE: FitTextStyle = { ...DISPLAY, fill: { type: 'solid', color: C.ink } };
export type Fit = { fontSize: number; style: TextStyle; height: number };
export let fits: Fit[] = FIT_BOXES.map((box) => {
  const { width, height } = inside(box);
  const fontSize = Math.floor(Math.sqrt((width * height) / (FIT_COPY.length * 0.86 * FIT_LINE_HEIGHT)));
  const style = { ...FIT_STYLE, fontSize, lineHeight: fontSize * FIT_LINE_HEIGHT };
  return { fontSize, style, height: Math.ceil((FIT_COPY.length * fontSize * 0.86) / width) * fontSize * FIT_LINE_HEIGHT };
});

// SPAN: one paragraph whose lit word moves; each highlight is placed from the
// measured glyphs, one bar per line it covers.
export const SPAN_COPY = 'Same sentence. Light another word, and the meaning moves.';
export const SPAN_STYLE: TextStyle = { ...textStyle(92, 'body', C.paper, 700), lineHeight: 118 };
export type Segment = { x: number; y: number; width: number };
export let spanLayout = estimateLayout(SPAN_COPY, 92, 118, COL.width, 0.55);

export function segmentsOf(word: string): Segment[] {
  const start = SPAN_COPY.indexOf(word);
  const glyphs = spanLayout.glyphs.filter((g) => g.start >= start && g.end <= start + word.length);
  const rows = [...new Set(glyphs.map((g) => g.line))];
  return rows.map((line) => {
    const inLine = glyphs.filter((g) => g.line === line);
    const x = Math.min(...inLine.map((g) => g.x));
    return { x, y: line * spanLayout.lineHeight, width: Math.max(...inLine.map((g) => g.x + g.width)) - x };
  });
}

export async function measureAll() {
  try {
    hookLines = await Promise.all(hookLines.map((l, i) => fitLine(l, HOOK_SPEC[i][1])));
    frameLines = await Promise.all(frameLines.map((l, i) => fitLine(l, FRAME_SPEC[i][1])));
    outroLines = await Promise.all(outroLines.map((l, i) => fitLine(l, OUTRO_SPEC[i][1])));
    beatWords = await Promise.all(beatWords.map((l, i) => fitLine(l, BEAT_SPEC[i][1], BEAT_WIDTH)));
    measured = await Promise.all(MEASURE_WORDS.map(async (text) => {
      const { size } = await fitLine({ text, size: 0, width: 0 }, 260);
      return { text, size, metrics: await measureText(text, { ...DISPLAY, fontSize: size }, { fonts }) };
    }));
    spanLayout = await measureText(SPAN_COPY, SPAN_STYLE, { maxWidth: COL.width, fonts });
    fits = await Promise.all(FIT_BOXES.map(async (box) => {
      const fit = await fitText(FIT_COPY, {
        ...inside(box), minFontSize: 28, maxFontSize: 220, lineHeight: FIT_LINE_HEIGHT, step: 2, style: FIT_STYLE, fonts,
      });
      return { fontSize: fit.fontSize, style: fit.style, height: fit.metrics.height };
    }));
  } catch (error) {
    // Only inspect.mjs lands here; the estimates above keep every frame evaluable.
    if (!String(error).includes('inspect.mjs')) throw error;
  }
}

// Greedy wrapping with fixed-width glyphs: a stand-in for the real layout.
function estimateLayout(text: string, size: number, lineHeight: number, maxWidth: number, em: number): TextMetrics {
  const advance = size * em;
  const perLine = Number.isFinite(maxWidth) ? Math.floor(maxWidth / advance) : text.length;
  const glyphs = Array.from(text, (ch, i) => ({
    text: ch, start: i, end: i + 1, rtl: false, x: (i % perLine) * advance, width: advance, line: Math.floor(i / perLine),
  }));
  const count = Math.ceil(glyphs.length / perLine);
  return {
    width: Math.min(text.length, perLine) * advance, height: count * lineHeight,
    ascent: lineHeight * 0.8, descent: lineHeight * 0.2, lineHeight, lines: count, glyphs,
  };
}
