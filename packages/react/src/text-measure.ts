import * as React from 'react';
import type { ReactNode } from 'react';

import { entryRelativePath, isRemoteUrl } from './entry-dir';
import { flattenTextContent, themeTextContent, withTextRuns } from './rich-text';
import { CompositionRuntimeContext } from './hooks';
import { type ResolvedTheme, type ThemeValues, ThemeContext, mergeTheme, resolvedTheme, themedTextStyle } from './theme';
import type { FontFallbackMode, ResolvedAsset, TextStyle } from './scene';

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
  /**
   * Advance width, kerning included: a pair's tightening is taken out of the
   * first glyph's advance, so `next.x - (x + width)` is 0. Measure a glyph
   * alone to get its un-kerned advance.
   */
  width: number;
  /** Zero-based line the cluster sits on. */
  line: number;
  /** Highest point of the drawn outline above the baseline; 0 for a blank cluster. */
  inkAscent: number;
  /** Lowest point of the drawn outline below the baseline; 0 for a blank cluster. */
  inkDescent: number;
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
  /** Height of a capital letter above the baseline in the first glyph's font; 0 if the font does not say. */
  capHeight: number;
  /** Height of a lowercase `x` above the baseline in the first glyph's font; 0 if the font does not say. */
  xHeight: number;
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
  /**
   * Font and color names, as a `<Theme>` gives them. `measureText()` runs
   * before the tree, outside any `<Theme>`, so it knows only these;
   * `useTextMetrics()` adds them to the `<Theme>`s around it.
   */
  theme?: ThemeValues;
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

/** What text inherits from the `<Composition>`, `<Group>`, or `<Sequence>` around it. */
export interface InheritedText {
  lang?: string;
  fontFallback?: FontFallbackMode;
}

/**
 * @internal Resolves the same language and font fallback for text layers and
 * their measurements: each inherited value the style leaves unset.
 */
export function withInheritedText(style: TextStyle, inherited: InheritedText): TextStyle {
  const { lang, fontFallback } = inherited;
  const fillLang = lang !== undefined && style.lang == null;
  const fillFallback = fontFallback !== undefined && style.fontFallback == null;
  if (!fillLang && !fillFallback) return style;
  return { ...style, ...(fillLang ? { lang } : {}), ...(fillFallback ? { fontFallback } : {}) };
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
 * Measures `content` laid out with `style` — the same shaping `<Text>`
 * renders with — during an entry's async `prepare()`. `content` is text,
 * which may hold `<Span>`. Stash the result in module state for the
 * synchronous render to read.
 */
export async function measureText(
  content: ReactNode,
  style: TextStyle = {},
  options: MeasureTextOptions = {},
): Promise<TextMetrics> {
  const theme = resolvedTheme(options.theme);
  const flat = flattenTextContent(themeTextContent(content, theme), 'measureText() text');
  return asynchronousMeasurer('measureText()')({
    text: flat.text,
    style: withTextRuns(themedStyle(style, theme), flat, 'measureText()'),
    ...(options.maxWidth !== undefined ? { maxWidth: options.maxWidth } : {}),
    fonts: prepareFonts(options.fonts),
  });
}

/** `style` with `theme`'s names looked up; color shorthand does not change a measurement. */
function themedStyle(style: TextStyle, theme: ResolvedTheme): TextStyle {
  return themedTextStyle(style, {}, theme, 'text measurement') ?? style;
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
    ...prepareFonts(extra),
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
 * and shaping as `<Text>`; `content` may hold `<Span>`, measured with the
 * spans' weights and families. Unchanged inputs reuse this hook's last
 * result.
 */
export function useTextMetrics(
  content: ReactNode,
  style: TextStyle = {},
  options: MeasureTextOptions = {},
): TextMetrics {
  const runtime = React.useContext(CompositionRuntimeContext);
  const outer = React.useContext(ThemeContext);
  const theme = options.theme ? mergeTheme(outer, options.theme) : outer;
  const flat = flattenTextContent(themeTextContent(content, theme), 'useTextMetrics() text');
  const resolvedStyle = withTextRuns(
    withInheritedText(themedStyle(style, theme), runtime ?? {}),
    flat,
    'useTextMetrics()',
  );
  const fonts = useMeasurementFonts(options.fonts);
  const { colorRuns, visibleCharacters, fill, stroke, ...layoutStyle } = resolvedStyle;
  const key = JSON.stringify({ text: flat.text, style: layoutStyle, maxWidth: options.maxWidth, fonts });
  return React.useMemo(
    () => synchronousMeasurer('useTextMetrics()')(JSON.parse(key) as MeasureTextRequest),
    [key],
  );
}

/** A caret from complete-text shaping; positions inside a cluster interpolate its advance. */
export function textCaret(metrics: TextMetrics, offset: number): { x: number; y: number; line: number } {
  if (!Number.isSafeInteger(offset) || offset < 0) throw new Error('Text caret offset must be a non-negative integer');
  const line = metrics.lineStarts?.reduce((line, start, index) => start <= offset ? index : line, 0);
  let glyph: GlyphMetrics | undefined;
  let next: GlyphMetrics | undefined;
  for (const candidate of metrics.glyphs) {
    if (line !== undefined && candidate.line !== line) continue;
    if (candidate.start <= offset && offset < candidate.end) { glyph = candidate; break; }
    if (candidate.end <= offset && (!glyph || candidate.end >= glyph.end)) glyph = candidate;
    if (candidate.start >= offset && (!next || candidate.start < next.start)) next = candidate;
  }
  glyph ??= next;
  if (!glyph) return { x: 0, y: Math.max(0, line ?? 0) * metrics.lineHeight, line: Math.max(0, line ?? 0) };
  const fraction = Math.max(0, Math.min(1, (offset - glyph.start) / Math.max(1, glyph.end - glyph.start)));
  return { x: glyph.x + glyph.width * (glyph.rtl ? 1 - fraction : fraction),
    y: glyph.line * metrics.lineHeight, line: glyph.line };
}
