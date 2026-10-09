import type { Asset, FontRun, TextStyle } from './types';
export type { FontRun };

type MeasureTextRequest = { text: string; style: TextStyle; maxWidth?: number; fonts: Asset[] };
type TextMetrics = {
  width: number; height: number; ascent: number; descent: number; lineHeight: number; lines: number;
  glyphs: { text: string; x: number; width: number; line: number; start: number; end: number; rtl: boolean }[];
};

type Context = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

/** The canvas font for `style`, with `run`'s weight and family over its own. */
export function cssFont(style: TextStyle, run?: FontRun): string {
  const family = run?.fontFamily ?? style.fontFamily;
  return `${run?.fontWeight ?? style.fontWeight ?? 400} ${style.fontSize ?? 32}px ${family ? JSON.stringify(family) + ', ' : ''}system-ui, sans-serif`;
}

export function textStyle(ctx: Context, style: TextStyle) {
  ctx.font = cssFont(style);
  ctx.letterSpacing = `${style.letterSpacing ?? 0}px`;
  if ('lang' in ctx) ctx.lang = style.lang?.trim() || navigator.language;
}

export type RunPiece = { text: string; start: number; utf16: number; run?: FontRun };

/** Throws unless `runs` are ordered, non-overlapping ranges within `count` code points. */
export function checkFontRuns(runs: FontRun[] | undefined, count: number): void {
  let previousEnd = 0;
  for (const run of runs ?? []) {
    if (!Number.isSafeInteger(run.start) || !Number.isSafeInteger(run.end) || run.start < previousEnd || run.end < run.start || run.end > count) {
      throw new Error('Invalid text font run range');
    }
    previousEnd = run.end;
  }
}

// Reused: `measureLine` cuts pieces for every width probe while wrapping.
const graphemes = new Intl.Segmenter(undefined, { granularity: 'grapheme' });

/**
 * `text` (code point `start` of the full text onward) cut where the font run
 * changes: `start` and `utf16` are each piece's code-point and UTF-16 offset
 * within `text`. A grapheme cluster takes the run at its first code point.
 * `runs` are ordered and non-overlapping (`checkFontRuns`).
 */
export function runPieces(text: string, start: number, runs: FontRun[]): RunPiece[] {
  const pieces: RunPiece[] = [];
  let point = 0;
  let next = 0;
  for (const { segment, index } of graphemes.segment(text)) {
    const at = start + point;
    while (next < runs.length && runs[next].end <= at) next++;
    const run = next < runs.length && runs[next].start <= at ? runs[next] : undefined;
    const last = pieces[pieces.length - 1];
    if (last && last.run === run) last.text += segment;
    else pieces.push({ text: segment, start: point, utf16: index, run });
    point += Array.from(segment).length;
  }
  return pieces;
}

export type LineMetrics = {
  width: number; actualBoundingBoxLeft: number; actualBoundingBoxRight: number;
  actualBoundingBoxAscent: number; actualBoundingBoxDescent: number;
  fontBoundingBoxAscent: number; fontBoundingBoxDescent: number;
};

/**
 * Measures one line (code point `start` of the full text onward) piece by
 * piece in each run's font. The font box comes from the text's own font,
 * which places the baseline, as natively. The ink box is the union of the
 * pieces' boxes, each placed at its advance.
 */
export function measureLine(ctx: Context, style: TextStyle, text: string, start: number, runs: FontRun[]): LineMetrics {
  const own = ctx.measureText(text);
  if (runs.length === 0) return own;
  const base = ctx.font;
  let width = 0, left = -Infinity, right = -Infinity, ascent = 0, descent = 0;
  for (const piece of runPieces(text, start, runs)) {
    ctx.font = piece.run ? cssFont(style, piece.run) : base;
    const m = ctx.measureText(piece.text);
    left = Math.max(left, m.actualBoundingBoxLeft - width);
    right = Math.max(right, width + m.actualBoundingBoxRight);
    ascent = Math.max(ascent, m.actualBoundingBoxAscent);
    descent = Math.max(descent, m.actualBoundingBoxDescent);
    width += m.width;
  }
  ctx.font = base;
  return {
    width, actualBoundingBoxLeft: left, actualBoundingBoxRight: right,
    actualBoundingBoxAscent: ascent, actualBoundingBoxDescent: descent,
    fontBoundingBoxAscent: own.fontBoundingBoxAscent, fontBoundingBoxDescent: own.fontBoundingBoxDescent,
  };
}

/**
 * `text` split into lines at line endings and, with `maxWidth`, wrapped at
 * word boundaries. `width` measures a line starting at code point `start`.
 */
export function textLineRanges(
  ctx: Context, text: string, maxWidth?: number | null, lang?: string | null,
  width: (text: string, start: number) => number = text => ctx.measureText(text).width,
): { text: string; start: number }[] {
  const lines: { text: string; start: number }[] = [];
  const parts = text.split(/(\r\n|\r|\n)/);
  const segmenter = maxWidth ? new Intl.Segmenter(lang?.trim() || navigator.language, { granularity: 'word' }) : undefined;
  let start = 0;
  for (let part = 0; part < parts.length; part += 2) {
    const paragraph = parts[part];
    if (!segmenter) lines.push({ text: paragraph, start });
    else {
      let line = '', used = 0, lineStart = start;
      for (const { segment } of segmenter.segment(paragraph)) {
        if (line && width((line + segment).trimEnd(), lineStart) > maxWidth!) {
          lines.push({ text: line.trimEnd(), start: lineStart });
          line = segment.trimStart();
          lineStart = start + used + Array.from(segment).length - Array.from(line).length;
        } else line += segment;
        used += Array.from(segment).length;
      }
      lines.push({ text: line.trimEnd(), start: lineStart });
    }
    start += Array.from(paragraph).length + (parts[part + 1]?.length ?? 0);
  }
  return lines;
}

export function textLines(ctx: Context, text: string, maxWidth?: number | null, lang?: string | null): string[] {
  return textLineRanges(ctx, text, maxWidth, lang).map(line => line.text);
}

export function textMeasurer() {
  const ctx = new OffscreenCanvas(1, 1).getContext('2d')!;
  return ({ text, style, maxWidth }: MeasureTextRequest): TextMetrics => {
    textStyle(ctx, style);
    const runs = style.fontRuns ?? [];
    checkFontRuns(runs, Array.from(text).length);
    const measure = (text: string, start: number) => measureLine(ctx, style, text, start, runs).width;
    const ranges = textLineRanges(ctx, text, maxWidth, style.lang, runs.length ? measure : undefined);
    const lines = ranges.map(range => range.text);
    const lineHeight = style.lineHeight ?? (style.fontSize ?? 32) * 1.2;
    const baseline = ctx.measureText('M');
    const ascent = (lineHeight - baseline.fontBoundingBoxAscent - baseline.fontBoundingBoxDescent) / 2 + baseline.fontBoundingBoxAscent;
    const widths = ranges.map(range => measure(range.text, range.start));
    return {
      width: Math.max(0, ...widths), height: lines.length * lineHeight, ascent, descent: lineHeight - ascent, lineHeight, lines: lines.length,
      // Canvas exposes whole-run measurements, but no portable shaped-cluster API.
      // Access fails explicitly so prepare() can select a fallback without using
      // grapheme/prefix estimates as native GlyphMetrics.
      get glyphs(): TextMetrics['glyphs'] {
        throw new Error('Shaped glyph metrics are unavailable in the browser runtime. Use whole-text metrics or the native renderer.');
      },
    };
  };
}
