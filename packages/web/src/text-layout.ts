import type { Asset, TextStyle } from './types';

type MeasureTextRequest = { text: string; style: TextStyle; maxWidth?: number; fonts: Asset[] };
type TextMetrics = {
  width: number; height: number; ascent: number; descent: number; lineHeight: number; lines: number;
  glyphs: { text: string; x: number; width: number; line: number; start: number; end: number; rtl: boolean }[];
};

type Context = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

export function textStyle(ctx: Context, style: TextStyle) {
  const size = style.fontSize ?? 32;
  ctx.font = `${style.fontWeight ?? 400} ${size}px ${style.fontFamily ? JSON.stringify(style.fontFamily) + ', ' : ''}system-ui, sans-serif`;
  ctx.letterSpacing = `${style.letterSpacing ?? 0}px`;
  if ('lang' in ctx) ctx.lang = style.lang?.trim() || navigator.language;
}

export function textLineRanges(ctx: Context, text: string, maxWidth?: number | null, lang?: string | null): { text: string; start: number }[] {
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
        if (line && ctx.measureText((line + segment).trimEnd()).width > maxWidth!) {
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
    const lines = textLines(ctx, text, maxWidth, style.lang);
    const lineHeight = style.lineHeight ?? (style.fontSize ?? 32) * 1.2;
    const baseline = ctx.measureText('M');
    const ascent = (lineHeight - baseline.fontBoundingBoxAscent - baseline.fontBoundingBoxDescent) / 2 + baseline.fontBoundingBoxAscent;
    const widths = lines.map(line => ctx.measureText(line).width);
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
