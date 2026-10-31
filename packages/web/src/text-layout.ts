import { featureFacesLoaded, featureFacesPending, featureFamily, fontSetFamily } from './fonts';
import type { Asset, FontFamily, FontFeatures, FontRun, TextStyle } from './types';
export type { FontRun };

type MeasureTextRequest = { text: string; style: TextStyle; maxWidth?: number; fonts: Asset[] };
type TextMetrics = {
  width: number; height: number; ascent: number; descent: number; lineHeight: number; capHeight: number; xHeight: number; lines: number; lineStarts: number[];
  glyphs: { text: string; x: number; width: number; line: number; inkAscent: number; inkDescent: number; start: number; end: number; rtl: boolean }[];
};

type Context = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

/**
 * The canvas font for `style`, with `run`'s weight and family over its own.
 * A font stack becomes the CSS family list, so the browser picks each
 * character's family from it. With features (the text's, and `run`'s
 * merged over them), each family is its `featureFamily` copy drawing them.
 * In a worker, the font set's family follows `sans-serif` (`fontSetFamily`).
 */
export function cssFont(style: TextStyle, run?: FontRun): string {
  const family = run?.fontFamily ?? style.fontFamily;
  let names = Array.isArray(family) ? [...family] : family ? [family] : [];
  const features = Object.entries({ ...style.fontFeatures, ...run?.fontFeatures }).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0);
  if (features.length > 0) {
    if (names.length === 0) {
      throw new Error('fontFeatures need a fontFamily loaded with <Font>: the browser renderer cannot apply OpenType features to installed fonts by their family name.');
    }
    const settings = features.map(([tag, value]) => `${JSON.stringify(tag)} ${Number(value)}`).join(', ');
    const label = features.map(([tag, value]) => `${tag}=${Number(value)}`).join(',');
    names = names.map(name => featureFamily(name, settings, label));
  }
  const families = names.map(name => JSON.stringify(name) + ', ');
  const fontSet = fontSetFamily();
  return `${run?.fontWeight ?? style.fontWeight ?? 400} ${style.fontSize ?? 32}px ${families.join('')}system-ui, sans-serif${fontSet ? `, ${JSON.stringify(fontSet)}` : ''}`;
}

/** A CSS `font-feature-settings` tag: four characters from U+20 to U+7E. */
const FEATURE_TAG = /^[\x20-\x7e]{4}$/;

/** Throws unless `features` maps feature tags to `true`, `false`, or whole numbers from 0 to 2³² − 1. */
export function checkFontFeatures(features: FontFeatures | null | undefined): void {
  if (features == null) return;
  if (typeof features !== 'object' || Array.isArray(features)) throw new Error('Text fontFeatures must be an object of feature tags');
  for (const [tag, value] of Object.entries(features)) {
    if (!FEATURE_TAG.test(tag)) throw new Error(`Text font feature tag ${JSON.stringify(tag)} must be four ASCII characters, such as "liga"`);
    if (typeof value !== 'boolean' && !(Number.isInteger(value) && value >= 0 && value <= 0xffffffff)) {
      throw new Error(`Text font feature ${tag} must be true, false, or a whole number from 0 up`);
    }
  }
}

/**
 * Checks `style`'s features and its font runs', and makes the families
 * `cssFont` draws them with, waiting for them where they do not load at once.
 */
export async function loadFeatureFonts(style: TextStyle): Promise<void> {
  checkFontFeatures(style.fontFeatures);
  for (const run of style.fontRuns ?? []) checkFontFeatures(run.fontFeatures);
  cssFont(style);
  for (const run of style.fontRuns ?? []) cssFont(style, run);
  await featureFacesLoaded();
}

/**
 * `style` with `families`, the scene's loaded font families, after its own
 * stack and each font run's when it sets `fontFallback: 'loaded'`, so the
 * browser tries them before the system's fonts. Native rendering does the
 * same per grapheme cluster; the browser chooses per character.
 */
export function withLoadedFallback(style: TextStyle, families: readonly string[]): TextStyle {
  if (style.fontFallback !== 'loaded' || families.length === 0) return style;
  if (style.fontFamily == null && !style.fontRuns?.some(run => run.fontFamily != null)) return style;
  const extend = (family: FontFamily): string[] => {
    const names = typeof family === 'string' ? [family] : [...family];
    return [...names, ...families.filter(name => !names.includes(name))];
  };
  return {
    ...style,
    ...(style.fontFamily != null ? { fontFamily: extend(style.fontFamily) } : {}),
    ...(style.fontRuns ? { fontRuns: style.fontRuns.map(run => run.fontFamily == null ? run : { ...run, fontFamily: extend(run.fontFamily) }) } : {}),
  };
}

/** `style.letterSpacing` in px, resolving `{ em }` against the font size. */
export function letterSpacingPx(style: TextStyle): number {
  const spacing = style.letterSpacing ?? 0;
  return typeof spacing === 'number' ? spacing : spacing.em * (style.fontSize ?? 32);
}

export function textStyle(ctx: Context, style: TextStyle) {
  ctx.font = cssFont(style);
  ctx.letterSpacing = `${letterSpacingPx(style)}px`;
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
    checkFontFeatures(style.fontFeatures);
    for (const run of style.fontRuns ?? []) checkFontFeatures(run.fontFeatures);
    textStyle(ctx, style);
    const runs = style.fontRuns ?? [];
    checkFontRuns(runs, Array.from(text).length);
    for (const run of runs) cssFont(style, run);
    if (featureFacesPending()) {
      throw new Error('Browser text measurements with fontFeatures need their fonts ready. Call await measureText(..., { fonts }) with the same style in prepare() first.');
    }
    const measure = (text: string, start: number) => measureLine(ctx, style, text, start, runs).width;
    const ranges = textLineRanges(ctx, text, maxWidth, style.lang, runs.length ? measure : undefined);
    const lines = ranges.map(range => range.text);
    const lineHeight = style.lineHeight ?? (style.fontSize ?? 32) * 1.2;
    const baseline = ctx.measureText('M');
    const ascent = (lineHeight - baseline.fontBoundingBoxAscent - baseline.fontBoundingBoxDescent) / 2 + baseline.fontBoundingBoxAscent;
    const widths = ranges.map(range => measure(range.text, range.start));
    return {
      width: Math.max(0, ...widths), height: lines.length * lineHeight, ascent, descent: lineHeight - ascent, lineHeight,
      capHeight: ctx.measureText('H').actualBoundingBoxAscent, xHeight: ctx.measureText('x').actualBoundingBoxAscent, lines: lines.length,
      lineStarts: ranges.map(range => range.start),
      // Canvas exposes whole-run measurements, but no portable shaped-cluster API.
      // Access fails explicitly so prepare() can select a fallback without using
      // grapheme/prefix estimates as native GlyphMetrics.
      get glyphs(): TextMetrics['glyphs'] {
        throw new Error('Shaped glyph metrics are unavailable in the browser runtime. Use whole-text metrics or the native renderer.');
      },
    };
  };
}
