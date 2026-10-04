import * as React from 'react';

import { entryRelativePath, isRemoteUrl } from './entry-dir';
import { CompositionRuntimeContext } from './hooks';
import type { ResolvedAsset, TextStyle } from './scene';

/** One shaped glyph cluster (a ligature or combining sequence is one entry). */
export interface GlyphMetrics {
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
  if (!measure) {
    throw new Error('measureText() requires a Celesta editor or exporter runtime');
  }
  const fonts = (options.fonts ?? []).map((src): ResolvedAsset => {
    const path = isRemoteUrl(src) ? src : entryRelativePath(src);
    return { id: src, location: isRemoteUrl(src) ? { type: 'url', url: src } : { type: 'file', path } };
  });
  return measure({
    text,
    style,
    ...(options.maxWidth !== undefined ? { maxWidth: options.maxWidth } : {}),
    fonts,
  });
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
  const declaredFonts = React.useContext(TextMetricsFontsContext);
  const fonts = [
    ...declaredFonts.map((font) => font.location.type === 'file'
      ? { ...font, location: { ...font.location, path: entryRelativePath(font.location.path) } }
      : font),
    ...(options.fonts ?? []).map((src): ResolvedAsset => ({
      id: src,
      location: isRemoteUrl(src) ? { type: 'url', url: src } : { type: 'file', path: entryRelativePath(src) },
    })),
  ];
  const key = JSON.stringify({ text, style: resolvedStyle, maxWidth: options.maxWidth, fonts });
  return React.useMemo(() => {
    if (!measureSync) {
      throw new Error('useTextMetrics() requires a Celesta editor or exporter runtime');
    }
    return measureSync(JSON.parse(key) as MeasureTextRequest);
  }, [key]);
}
