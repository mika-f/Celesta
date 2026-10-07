import * as React from 'react';

import { entryRelativePath, isRemoteUrl } from './entry-dir';
import { CompositionRuntimeContext } from './hooks';
import type { ResolvedAsset, TextStyle } from './scene';

/** One shaped glyph cluster (a ligature or combining sequence is one entry). */
export interface GlyphMetrics {
  /** Half-open Unicode code-point range in the complete source text. */
  start: number;
  end: number;
  rtl: boolean;
  /** The source text the cluster covers. */
  text: string;
  /** Left edge within its line, in composition pixels, alignment included. */
  x: number;
  /** Advance width. */
  width: number;
  /** Zero-based line the cluster sits on. */
  line: number;
}

export interface TextMetrics {
  /** Advance width of the widest line. */
  width: number;
  /** Height of all lines together. */
  height: number;
  /** First line's top edge to its baseline, half-leading included. */
  ascent: number;
  /** First line's baseline to its bottom edge; `ascent + descent === lineHeight`. */
  descent: number;
  lineHeight: number;
  lines: number;
  glyphs: GlyphMetrics[];
  /** Source code-point offsets for visual line starts, including empty lines. */
  lineStarts?: number[];
}

export interface MeasureTextOptions {
  /** Wrap at this width, like `<Text maxWidth>`. */
  maxWidth?: number;
  /**
   * Font files to load before measuring, like `<Font src>`. `prepare()` runs
   * before the tree renders, so `<Font>` declarations are not loaded yet.
   */
  fonts?: string[];
}

export type MeasureTextRequest = {
  text: string;
  style: TextStyle;
  maxWidth?: number;
  fonts: ResolvedAsset[];
};

let measure: ((request: MeasureTextRequest) => Promise<TextMetrics>) | undefined;
let measureSync: ((request: MeasureTextRequest) => TextMetrics) | undefined;

/** @internal Fonts collected from the composition, shared with component previews. */
export const TextMetricsFontsContext = React.createContext<readonly ResolvedAsset[]>([]);

/** @internal Resolves the same language for text layers and their measurements. */
export function withTextLanguage(style: TextStyle, lang?: string): TextStyle {
  return lang !== undefined && style.lang == null ? { ...style, lang } : style;
}

/** @internal Installed by the Celesta CLI before an entry's `prepare()` runs. */
export function setTextMeasurer(
  next: (request: MeasureTextRequest) => Promise<TextMetrics>,
  synchronous?: (request: MeasureTextRequest) => TextMetrics,
): void {
  measure = next;
  measureSync = synchronous;
}

/**
 * Measures `text` laid out with `style` — the same shaping `<Text>` renders
 * with — during an entry's async `prepare()`. Stash the result in module
 * state for the synchronous render to read.
 */
export async function measureText(
  text: string,
  style: TextStyle = {},
  options: MeasureTextOptions = {},
): Promise<TextMetrics> {
  return asynchronousMeasurer('measureText()')({
    text,
    style,
    ...(options.maxWidth !== undefined ? { maxWidth: options.maxWidth } : {}),
    fonts: prepareFonts(options.fonts),
  });
}

/** @internal Font files for a `prepare()`-time measurement. */
export function prepareFonts(sources: readonly string[] = []): ResolvedAsset[] {
  return sources.map((src): ResolvedAsset => {
    const path = isRemoteUrl(src) ? src : entryRelativePath(src);
    return { id: src, location: isRemoteUrl(src) ? { type: 'url', url: src } : { type: 'file', path } };
  });
}

/** @internal The asynchronous measurer, or an error naming `caller`. */
export function asynchronousMeasurer(caller: string): (request: MeasureTextRequest) => Promise<TextMetrics> {
  if (!measure) {
    throw new Error(`${caller} requires a Celesta editor or exporter runtime`);
  }
  return measure;
}

/** @internal The composition's `<Font>` declarations plus `extra`, for a render-time measurement. */
export function useMeasurementFonts(extra: readonly string[] = []): ResolvedAsset[] {
  const declaredFonts = React.useContext(TextMetricsFontsContext);
  return [
    ...declaredFonts.map((font) => font.location.type === 'file'
      ? { ...font, location: { ...font.location, path: entryRelativePath(font.location.path) } }
      : font),
    ...extra.map((src): ResolvedAsset => ({
      id: src,
      location: isRemoteUrl(src) ? { type: 'url', url: src } : { type: 'file', path: entryRelativePath(src) },
    })),
  ];
}

/** @internal The synchronous measurer, or an error naming `caller`. */
export function synchronousMeasurer(caller: string): (request: MeasureTextRequest) => TextMetrics {
  if (!measureSync) {
    throw new Error(`${caller} requires a Celesta editor or exporter runtime`);
  }
  return measureSync;
}

/**
 * Measures computed text synchronously during render, using the same fonts
 * and shaping as `<Text>`. Unchanged inputs reuse this hook's last result.
 */
export function useTextMetrics(
  text: string,
  style: TextStyle = {},
  options: MeasureTextOptions = {},
): TextMetrics {
  const lang = React.useContext(CompositionRuntimeContext)?.lang;
  const resolvedStyle = withTextLanguage(style, lang);
  const fonts = useMeasurementFonts(options.fonts);
  const { colorRuns, visibleCharacters, fill, stroke, ...layoutStyle } = resolvedStyle;
  const key = JSON.stringify({ text, style: layoutStyle, maxWidth: options.maxWidth, fonts });
  return React.useMemo(
    () => synchronousMeasurer('useTextMetrics()')(JSON.parse(key) as MeasureTextRequest),
    [key],
  );
}

/** A caret from complete-text shaping; positions inside a cluster interpolate its advance. */
export function textCaret(metrics: TextMetrics, offset: number): { x: number; y: number; line: number } {
  if (!Number.isSafeInteger(offset) || offset < 0) throw new Error('Text caret offset must be a non-negative integer');
  const line = metrics.lineStarts?.reduce((line, start, index) => start <= offset ? index : line, 0);
  const glyphs = line !== undefined ? metrics.glyphs.filter(glyph => glyph.line === line) : metrics.glyphs;
  const glyph = glyphs.find(glyph => glyph.start <= offset && offset < glyph.end)
    ?? glyphs.reduce<GlyphMetrics | undefined>((last, glyph) =>
      glyph.end <= offset && (!last || glyph.end >= last.end) ? glyph : last, undefined)
    ?? glyphs.find(glyph => glyph.start >= offset);
  if (!glyph) return { x: 0, y: Math.max(0, line ?? 0) * metrics.lineHeight, line: Math.max(0, line ?? 0) };
  const fraction = Math.max(0, Math.min(1, (offset - glyph.start) / Math.max(1, glyph.end - glyph.start)));
  return { x: glyph.x + glyph.width * (glyph.rtl ? 1 - fraction : fraction),
    y: glyph.line * metrics.lineHeight, line: glyph.line };
}
