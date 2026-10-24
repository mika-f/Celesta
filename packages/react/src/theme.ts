// Named fonts and colors: a `<Theme>` gives font stacks and colors names,
// and `<Text>`, `<Span>`, text measurements, `<Rect>`, and `<Path>` look
// those names up before anything reaches the renderer, which only ever sees
// family names and hex colors.

import * as React from 'react';
import type { ReactNode } from 'react';

import { checkFontFeatures } from './font-features';
import type { FontFamily, Paint, TextStyle } from './scene';
import { markTransparent } from './source-chain';

/** Font stacks and colors by name, as `<Theme>` takes them and `useTheme()` returns them. */
export interface ThemeValues {
  /** A name for a family name or a font stack, such as `{ mono: 'JetBrains Mono' }`. */
  fonts?: Readonly<Record<string, FontFamily>>;
  /** A name for a `#RRGGBB` or `#RRGGBBAA` color, such as `{ paper: '#f4f1ea' }`. */
  colors?: Readonly<Record<string, string>>;
}

/** The fonts and colors in effect, every enclosing `<Theme>` merged. */
export interface ResolvedTheme {
  fonts: Readonly<Record<string, FontFamily>>;
  colors: Readonly<Record<string, string>>;
}

export interface ThemeProps extends ThemeValues {
  children?: ReactNode;
}

const EMPTY_THEME: ResolvedTheme = { fonts: {}, colors: {} };

/** @internal */
export const ThemeContext = React.createContext<ResolvedTheme>(EMPTY_THEME);

const HEX_COLOR = /^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/;

function checkEntries(theme: ThemeValues): void {
  for (const [name, family] of Object.entries(theme.fonts ?? {})) {
    const isName = (value: unknown) => typeof value === 'string' && value.trim() !== '';
    if (Array.isArray(family) ? family.length === 0 || !family.every(isName) : !isName(family)) {
      throw new Error(`<Theme> font "${name}" must be a non-empty string or a non-empty array of them`);
    }
  }
  for (const [name, color] of Object.entries(theme.colors ?? {})) {
    if (typeof color !== 'string' || !HEX_COLOR.test(color)) {
      throw new Error(`<Theme> color "${name}" must be a #RRGGBB or #RRGGBBAA color, got ${JSON.stringify(color)}`);
    }
  }
}

/** `inner`'s names over `outer`'s. */
export function mergeTheme(outer: ResolvedTheme, inner: ThemeValues): ResolvedTheme {
  checkEntries(inner);
  return {
    fonts: inner.fonts ? { ...outer.fonts, ...inner.fonts } : outer.fonts,
    colors: inner.colors ? { ...outer.colors, ...inner.colors } : outer.colors,
  };
}

/**
 * Names fonts and colors for what is inside it: `<Text fontFamily="mono"
 * color="paper">`, the same names in `style` and in `<Span>`, and colors in
 * `<Rect>` and `<Path>` `fill` and `stroke`, gradient stops included. A
 * nested `<Theme>` adds to the names around it, its own winning where both
 * name the same thing. A name no theme defines is used as it is: a font
 * name as a family name, a color as a color, so the `color` shorthand and a
 * shape's colors throw unless they are hex colors.
 */
export function Theme({ fonts, colors, children }: ThemeProps): ReturnType<typeof React.createElement> {
  const outer = React.useContext(ThemeContext);
  const value = React.useMemo(() => mergeTheme(outer, { fonts, colors }), [outer, fonts, colors]);
  return React.createElement(ThemeContext.Provider, { value }, children);
}

// Hands its caller's children a context and draws nothing of its own.
markTransparent(Theme);

/** The fonts and colors the nearest `<Theme>`s define; empty outside any. */
export function useTheme(): ResolvedTheme {
  return React.useContext(ThemeContext);
}

/** @internal `theme` as a `ResolvedTheme`, for a `prepare()`-time measurement. */
export function resolvedTheme(theme: ThemeValues | undefined): ResolvedTheme {
  return theme ? mergeTheme(EMPTY_THEME, theme) : EMPTY_THEME;
}

/** A font name from `theme`, or `family` itself when it is not one. */
export function themeFont(family: FontFamily, theme: ResolvedTheme): FontFamily {
  return typeof family === 'string' && Object.prototype.hasOwnProperty.call(theme.fonts, family)
    ? theme.fonts[family]!
    : family;
}

/** A color name from `theme`, or `color` itself when it is not one. */
export function themeColor(color: string, theme: ResolvedTheme): string {
  return Object.prototype.hasOwnProperty.call(theme.colors, color) ? theme.colors[color]! : color;
}

/** A color name from `theme`, or `color` itself, which must then be a hex color; `what` names it in the error. */
function hexOrThemeColor(color: unknown, theme: ResolvedTheme, what: string): string {
  if (typeof color !== 'string') throw new Error(`${what} must be a string`);
  const resolved = themeColor(color, theme);
  if (!HEX_COLOR.test(resolved)) {
    const names = Object.keys(theme.colors);
    throw new Error(
      `${what} ${JSON.stringify(color)} is neither a #RRGGBB or #RRGGBBAA color nor a <Theme> color name`
        + (names.length > 0 ? ` (${names.join(', ')})` : ''),
    );
  }
  return resolved;
}

function shorthandColor(color: unknown, theme: ResolvedTheme, owner: string): string {
  return hexOrThemeColor(color, theme, `${owner} color`);
}

/**
 * @internal A shape's `fill` or `stroke` with `theme`'s color names looked
 * up: the color itself, a solid paint's color, and each gradient stop's.
 * Throws, naming `owner` and `prop`, on a color that is neither a hex color
 * nor a name. `paint` itself when nothing changes.
 */
export function themedShapePaint<T extends string | Paint | undefined>(
  paint: T,
  theme: ResolvedTheme,
  owner: string,
  prop: string,
): T {
  if (paint === undefined) return paint;
  if (typeof paint === 'string') return hexOrThemeColor(paint, theme, `${owner} ${prop}`) as T;
  const value = paint as Paint;
  if (value.type === 'solid') {
    const color = hexOrThemeColor(value.color, theme, `${owner} ${prop} color`);
    return (color === value.color ? paint : { ...value, color }) as T;
  }
  if (!Array.isArray(value.stops)) return paint;
  const stops = value.stops.map((stop, i) => {
    const color = hexOrThemeColor(stop?.color, theme, `${owner} ${prop} stop ${i} color`);
    return color === stop.color ? stop : { ...stop, color };
  });
  return (stops.every((stop, i) => stop === value.stops[i]) ? paint : { ...value, stops }) as T;
}

/** A paint with `theme`'s color names looked up in its color or gradient stops; `paint` itself when it names none. */
export function themePaint(paint: Paint, theme: ResolvedTheme): Paint {
  if (paint.type === 'solid') {
    if (typeof paint.color !== 'string') return paint;
    const color = themeColor(paint.color, theme);
    return color === paint.color ? paint : { ...paint, color };
  }
  if (!Array.isArray(paint.stops)) return paint;
  const stops = paint.stops.map((stop) => {
    if (typeof stop?.color !== 'string') return stop;
    const color = themeColor(stop.color, theme);
    return color === stop.color ? stop : { ...stop, color };
  });
  return stops.every((stop, i) => stop === paint.stops[i]) ? paint : { ...paint, stops };
}

/** Text's shorthand for the most used parts of its style. */
export interface TextShorthand {
  /** `style.fontFamily`, or a `<Theme>` font name. */
  fontFamily?: FontFamily;
  /** `style.fontSize`. */
  fontSize?: number;
  /** `style.fontWeight`. */
  fontWeight?: number;
  /** A solid `style.fill`: a hex color or a `<Theme>` color name. */
  color?: string;
}

/**
 * @internal `style` with `shorthand` filling what it leaves unset, and
 * `theme`'s names looked up. `style` itself when nothing changes, so text
 * without shorthand or theme names keeps its style object. Throws on
 * malformed `fontFeatures`.
 */
export function themedTextStyle(
  style: TextStyle | undefined,
  shorthand: TextShorthand,
  theme: ResolvedTheme,
  owner: string,
): TextStyle | undefined {
  let next = style;
  const set = <K extends keyof TextStyle>(key: K, value: TextStyle[K]) => {
    if (next?.[key] === value) return;
    next = { ...next, [key]: value };
  };
  if (shorthand.fontFamily !== undefined && style?.fontFamily == null) set('fontFamily', shorthand.fontFamily);
  if (shorthand.fontSize !== undefined && style?.fontSize == null) set('fontSize', shorthand.fontSize);
  if (shorthand.fontWeight !== undefined && style?.fontWeight == null) set('fontWeight', shorthand.fontWeight);
  if (shorthand.color !== undefined && style?.fill == null) {
    set('fill', { type: 'solid', color: shorthandColor(shorthand.color, theme, owner) });
  }
  if (next?.fontFamily != null) set('fontFamily', themeFont(next.fontFamily, theme));
  // Every text style passes here on its way to the renderer.
  if (next?.fontFeatures != null) checkFontFeatures(next.fontFeatures, owner);
  if (next?.fill != null) set('fill', themePaint(next.fill, theme));
  if (next?.stroke != null) {
    const strokes = Array.isArray(next.stroke) ? next.stroke : [next.stroke];
    const themed = strokes.map((stroke) => {
      const paint = themePaint(stroke.paint, theme);
      return paint === stroke.paint ? stroke : { ...stroke, paint };
    });
    if (themed.some((stroke, i) => stroke !== strokes[i])) {
      set('stroke', Array.isArray(next.stroke) ? themed : themed[0]!);
    }
  }
  if (next?.colorRuns?.length) {
    const runs = next.colorRuns.map((run) => {
      const color = themeColor(run.color, theme);
      return color === run.color ? run : { ...run, color };
    });
    if (runs.some((run, i) => run !== next!.colorRuns![i])) set('colorRuns', runs);
  }
  return next;
}
