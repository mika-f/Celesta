// Emphasis on part of a plain text: ranges of grapheme clusters, each with a
// small style override, drawn as the `<Span>`s `<Text>` lays out as one
// paragraph. Captions and dialogue subtitles share it, so both count the
// same clusters and refuse the same mistakes.

import * as React from 'react';
import type { ReactNode } from 'react';

import { isHexColor } from '@celesta/math';
import { Span } from '@celesta/react';
import type { ResolvedTheme } from '@celesta/react';

/** What an emphasis may change about its text. */
export interface EmphasisStyle {
  /** A `#RRGGBB` or `#RRGGBBAA` color, or a `<Theme>` color name. */
  color?: string;
  /** A whole number from 1 to 1000; a family without that weight uses its nearest face. */
  fontWeight?: number;
}

/** Part of a text to emphasize, in grapheme clusters (see `graphemes()`). */
export interface TextEmphasis {
  /** The first emphasized cluster, counted from 0. */
  start: number;
  /** The cluster after the last emphasized one; greater than `start`. */
  end: number;
  /** A name from the `styles` given with the emphasis, or the style itself. */
  style: string | EmphasisStyle;
}

/** A checked `TextEmphasis`, as `resolveEmphasis()` returns it. */
export interface ResolvedEmphasis {
  start: number;
  end: number;
  /** The emphasized text. */
  text: string;
  /** The style's name, when the emphasis named one from `styles`. */
  name?: string;
  /** The style, its `color` a hex color. */
  style: EmphasisStyle;
}

export interface EmphasisOptions {
  /** Styles by name, for emphasis whose `style` is a string. */
  styles?: Readonly<Record<string, EmphasisStyle>>;
  /** The `<Theme>` colors a style's `color` may name; `useTheme()` gives them. */
  theme?: ResolvedTheme;
  /** Names the text in errors, such as `'cue "intro"'`. Defaults to `'text'`. */
  what?: string;
}

const STYLE_KEYS = new Set(['color', 'fontWeight']);

let segmenter: Intl.Segmenter | undefined;

/**
 * `text` split into grapheme clusters: what a reader sees as one character,
 * so an emoji, a flag, or a letter with combining marks is one entry and a
 * surrogate pair is never cut. Emphasis and page ranges count these.
 */
export function graphemes(text: string): string[] {
  segmenter ??= new Intl.Segmenter(undefined, { granularity: 'grapheme' });
  return Array.from(segmenter.segment(text), (part) => part.segment);
}

function checkStyle(style: unknown, where: string, theme: ResolvedTheme | undefined): EmphasisStyle {
  if (style === null || typeof style !== 'object' || Array.isArray(style)) {
    throw new Error(`${where} must be a style name or an object with color and fontWeight`);
  }
  for (const key of Object.keys(style)) {
    if (!STYLE_KEYS.has(key)) {
      throw new Error(`${where} has unknown property "${key}"; an emphasis style sets only color and fontWeight`);
    }
  }
  const { color, fontWeight } = style as EmphasisStyle;
  const checked: EmphasisStyle = {};
  if (color !== undefined) {
    const resolved = typeof color === 'string' && theme && Object.prototype.hasOwnProperty.call(theme.colors, color)
      ? theme.colors[color]!
      : color;
    if (!isHexColor(resolved)) {
      throw new Error(
        `${where} color ${JSON.stringify(color)} is neither a #RRGGBB or #RRGGBBAA color nor a <Theme> color name`,
      );
    }
    checked.color = resolved;
  }
  if (fontWeight !== undefined) {
    if (!Number.isInteger(fontWeight) || fontWeight < 1 || fontWeight > 1000) {
      throw new Error(`${where} fontWeight must be a whole number from 1 to 1000, got ${String(fontWeight)}`);
    }
    checked.fontWeight = fontWeight;
  }
  return checked;
}

/**
 * Checks `emphasis` against `text` and returns it in order, each with its
 * style looked up. Throws, naming `options.what` and the emphasis, for a
 * range that is not whole clusters inside the text, is empty, or overlaps
 * another; a style name not in `styles`; or a style with anything but a
 * valid `color` and `fontWeight`.
 */
export function resolveEmphasis(
  text: string,
  emphasis: readonly TextEmphasis[] | undefined,
  options: EmphasisOptions = {},
): ResolvedEmphasis[] {
  const what = options.what ?? 'text';
  if (emphasis === undefined) return [];
  if (!Array.isArray(emphasis)) throw new Error(`${what}: emphasis must be a list of { start, end, style }`);
  if (typeof text !== 'string') throw new Error(`${what}: emphasis needs the text as a string`);
  const clusters = graphemes(text);
  const resolved = emphasis.map((entry, index): ResolvedEmphasis => {
    const where = `${what}: emphasis ${index}`;
    if (entry === null || typeof entry !== 'object') throw new Error(`${where} must be an object with start, end, and style`);
    const { start, end, style } = entry;
    if (!Number.isInteger(start) || !Number.isInteger(end)) {
      throw new Error(`${where} needs whole-number start and end (grapheme clusters), got ${start}–${end}`);
    }
    if (start < 0 || end > clusters.length || end <= start) {
      throw new Error(
        `${where} (${start}–${end}) must cover at least one of the text's ${clusters.length} grapheme clusters`,
      );
    }
    const highlighted = clusters.slice(start, end).join('');
    if (typeof style === 'string') {
      const styles = options.styles ?? {};
      if (!Object.prototype.hasOwnProperty.call(styles, style)) {
        const known = Object.keys(styles).map((name) => `"${name}"`).join(', ') || 'none';
        throw new Error(`${where} names the unknown style "${style}" (known: ${known})`);
      }
      return {
        start, end, text: highlighted, name: style,
        style: checkStyle(styles[style], `${what}: style "${style}"`, options.theme),
      };
    }
    return { start, end, text: highlighted, style: checkStyle(style, `${where} style`, options.theme) };
  });
  const sorted = resolved
    .map((entry, index) => ({ entry, index }))
    .sort((a, b) => a.entry.start - b.entry.start || a.index - b.index);
  for (let i = 1; i < sorted.length; i += 1) {
    const before = sorted[i - 1];
    const after = sorted[i];
    if (after.entry.start < before.entry.end) {
      throw new Error(
        `${what}: emphasis ${after.index} (${after.entry.start}–${after.entry.end}) overlaps emphasis ${before.index} `
          + `(${before.entry.start}–${before.entry.end})`,
      );
    }
  }
  return sorted.map(({ entry }) => entry);
}

/**
 * `text` with its emphasis as `<Span>`s, for `<Text>`, `<TextBox>`,
 * `useTextMetrics()`, or anything else that takes text content. `emphasis`
 * is checked as `resolveEmphasis()` checks it, or may already be resolved.
 *
 * ```tsx
 * <Text>{emphasisContent('HPが1です', [{ start: 3, end: 4, style: { color: '#ff6060' } }])}</Text>
 * ```
 */
export function emphasisContent(
  text: string,
  emphasis: readonly (TextEmphasis | ResolvedEmphasis)[] | undefined,
  options: EmphasisOptions = {},
): ReactNode {
  const resolved = resolveEmphasis(text, emphasis, options);
  if (resolved.length === 0) return text;
  const clusters = graphemes(text);
  const parts: ReactNode[] = [];
  let at = 0;
  resolved.forEach((entry, index) => {
    if (entry.start > at) parts.push(clusters.slice(at, entry.start).join(''));
    parts.push(React.createElement(Span, { key: index, style: spanStyle(entry.style) }, entry.text));
    at = entry.end;
  });
  if (at < clusters.length) parts.push(clusters.slice(at).join(''));
  return parts;
}

function spanStyle(style: EmphasisStyle): { fill?: string; fontWeight?: number } {
  return {
    ...(style.color !== undefined ? { fill: style.color } : {}),
    ...(style.fontWeight !== undefined ? { fontWeight: style.fontWeight } : {}),
  };
}

/**
 * The emphasis inside clusters `[start, end)` of its text, cut to them and
 * counted from `start`: the emphasis of one page or line of a longer text.
 */
export function sliceEmphasis(emphasis: readonly ResolvedEmphasis[], start: number, end: number): ResolvedEmphasis[] {
  return emphasis
    .filter((entry) => entry.end > start && entry.start < end)
    .map((entry) => {
      const from = Math.max(entry.start, start);
      const to = Math.min(entry.end, end);
      const cut = from === entry.start && to === entry.end
        ? entry.text
        : graphemes(entry.text).slice(from - entry.start, to - entry.start).join('');
      return { ...entry, start: from - start, end: to - start, text: cut };
    });
}
