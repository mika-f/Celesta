// Rich text: <Span> styles part of a text, and its content flattens into one
// string with code-point color and font runs, so the whole text is shaped as
// one paragraph.

import * as React from 'react';
import type { ReactNode } from 'react';
import { isHexColor } from '@celesta/math';

import { checkFontFeatures } from './font-features';
import type { FontFamily, FontFeatures, Paint, TextColorRun, TextFontRun, TextStyle } from './scene';
import { type ResolvedTheme, themeColor, themeFont, themePaint } from './theme';

export interface SpanStyle {
  /** A hex color (`"#RRGGBB"` or `"#RRGGBBAA"`) or a solid `Paint` with one; other formats throw. */
  fill?: string | Paint;
  /** 1–1000; a family without that weight uses its nearest face. */
  fontWeight?: number;
  /** A family name, or a font stack: names tried in order for each character. */
  fontFamily?: FontFamily;
  /** OpenType features, merged tag by tag over the text's (and outer spans') `fontFeatures`. */
  fontFeatures?: FontFeatures;
}

export interface SpanProps {
  style?: SpanStyle;
  /** `style.fontFamily`, or a `<Theme>` font name; `style.fontFamily` takes priority. */
  fontFamily?: FontFamily;
  /** `style.fontWeight`; `style.fontWeight` takes priority. */
  fontWeight?: number;
  /** A hex color or a `<Theme>` color name, as `style.fill`; `style.fill` takes priority. */
  color?: string;
  children?: ReactNode;
}

/**
 * Changes the fill, weight, family, or OpenType features of part of a text, which is still
 * laid out as one paragraph. Valid only inside the content of `<Text>`,
 * `<TextBox>`, `<TextReveal>`, `<Dialogue>`, and text measurements.
 */
export function Span(_props: SpanProps): never {
  throw new Error(
    '<Span> is only valid inside the content of <Text>, <TextBox>, <TextReveal>, <Dialogue>, or a text measurement',
  );
}

/** Rich text content as one string and its code-point runs. */
export interface FlatText {
  text: string;
  colorRuns: TextColorRun[];
  fontRuns: TextFontRun[];
}

interface Inherited {
  color?: string;
  fontWeight?: number;
  fontFamily?: FontFamily;
  fontFeatures?: FontFeatures;
}

function sameFamily(a: FontFamily | null | undefined, b: FontFamily | null | undefined): boolean {
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((name, i) => name === b[i]);
  return a === b;
}

function sameFeatures(a: FontFeatures | null | undefined, b: FontFeatures | null | undefined): boolean {
  if (a == null || b == null) return a == b;
  const tags = Object.keys(a);
  return tags.length === Object.keys(b).length && tags.every((tag) => b[tag] !== undefined && Number(a[tag]) === Number(b[tag]));
}

/** The color formats the renderer reads; checked here so a bad one fails at the span, not at export. */

/** A span's `style` with its shorthand props filling what the style leaves unset. */
function spanStyle(props: SpanProps): SpanStyle | undefined {
  const { style, fontFamily, fontWeight, color } = props;
  if (fontFamily === undefined && fontWeight === undefined && color === undefined) return style;
  const merged: SpanStyle = { ...style };
  if (merged.fontFamily === undefined && fontFamily !== undefined) merged.fontFamily = fontFamily;
  if (merged.fontWeight === undefined && fontWeight !== undefined) merged.fontWeight = fontWeight;
  if (merged.fill === undefined && color !== undefined) merged.fill = color;
  return merged;
}

/** A span fill with a `<Theme>` color name looked up; `fill` itself when it names none. */
function themeSpanFill(fill: SpanStyle['fill'], theme: ResolvedTheme): SpanStyle['fill'] {
  if (typeof fill === 'string') return themeColor(fill, theme);
  return fill && typeof fill === 'object' ? themePaint(fill, theme) : fill;
}

function resolveSpan(style: SpanStyle | undefined, inherited: Inherited): Inherited {
  const next = { ...inherited };
  if (style?.fill !== undefined) {
    const fill = style.fill;
    const color = typeof fill === 'string' ? fill : fill && fill.type === 'solid' ? fill.color : undefined;
    if (color === undefined) throw new Error('<Span> fill must be a color or a solid paint');
    if (!isHexColor(color)) {
      throw new Error(`<Span> fill must be a #RRGGBB or #RRGGBBAA color, got ${JSON.stringify(color)}`);
    }
    next.color = color;
  }
  if (style?.fontWeight !== undefined) {
    const weight = style.fontWeight;
    if (!Number.isInteger(weight) || weight < 1 || weight > 1000) {
      throw new Error('<Span> fontWeight must be a whole number from 1 to 1000');
    }
    next.fontWeight = weight;
  }
  if (style?.fontFamily !== undefined) {
    const family: unknown = style.fontFamily;
    const isName = (name: unknown) => typeof name === 'string' && name.trim() !== '';
    if (Array.isArray(family) ? family.length === 0 || !family.every(isName) : !isName(family)) {
      throw new Error('<Span> fontFamily must be a non-empty string or a non-empty array of them');
    }
    next.fontFamily = Array.isArray(family) ? [...family] : style.fontFamily;
  }
  if (style?.fontFeatures !== undefined) {
    checkFontFeatures(style.fontFeatures, '<Span>');
    next.fontFeatures = { ...inherited.fontFeatures, ...style.fontFeatures };
  }
  return next;
}

/** Appends `run`, extending the last run instead when it ends where `run` starts with the same attributes. */
function push<T extends { start: number; end: number }>(runs: T[], run: T, same: (a: T, b: T) => boolean): void {
  const last = runs[runs.length - 1];
  if (last && last.end === run.start && same(last, run)) last.end = run.end;
  else runs.push(run);
}

/**
 * Flattens rich text content: strings, numbers, arrays, fragments, and
 * `<Span>`; `null`, `undefined`, and booleans add nothing. `what` names the
 * content in errors, such as `'<Text> children'`.
 */
export function flattenTextContent(content: ReactNode, what: string): FlatText {
  let text = '';
  let length = 0;
  const colorRuns: TextColorRun[] = [];
  const fontRuns: TextFontRun[] = [];
  const visit = (node: ReactNode, inherited: Inherited): void => {
    if (node === null || node === undefined || typeof node === 'boolean') return;
    if (typeof node === 'string' || typeof node === 'number') {
      const value = String(node);
      const count = Array.from(value).length;
      if (count === 0) return;
      const start = length;
      text += value;
      length += count;
      if (inherited.color !== undefined) {
        push(colorRuns, { start, end: length, color: inherited.color }, (a, b) => a.color === b.color);
      }
      if (inherited.fontWeight !== undefined || inherited.fontFamily !== undefined || inherited.fontFeatures !== undefined) {
        push(fontRuns, {
          start,
          end: length,
          ...(inherited.fontWeight !== undefined ? { fontWeight: inherited.fontWeight } : {}),
          ...(inherited.fontFamily !== undefined ? { fontFamily: inherited.fontFamily } : {}),
          ...(inherited.fontFeatures !== undefined ? { fontFeatures: inherited.fontFeatures } : {}),
        }, (a, b) => a.fontWeight === b.fontWeight && sameFamily(a.fontFamily, b.fontFamily)
          && sameFeatures(a.fontFeatures, b.fontFeatures));
      }
      return;
    }
    if (Array.isArray(node)) {
      for (const child of node) visit(child, inherited);
      return;
    }
    if (React.isValidElement(node)) {
      if (node.type === React.Fragment) {
        visit((node.props as { children?: ReactNode }).children, inherited);
        return;
      }
      if (node.type === Span) {
        const props = node.props as SpanProps;
        visit(props.children, resolveSpan(spanStyle(props), inherited));
        return;
      }
    }
    throw new Error(`${what} must be a string, a number, or an array of those; style part of the text with <Span>`);
  };
  visit(content, {});
  return { text, colorRuns, fontRuns };
}

/**
 * @internal Rich text content with every `<Span>`'s `<Theme>` font and color
 * names looked up, so it flattens without the theme. `content` itself when
 * no span names one.
 */
export function themeTextContent(content: ReactNode, theme: ResolvedTheme): ReactNode {
  if (Array.isArray(content)) {
    const children = content.map((child) => themeTextContent(child, theme));
    return children.some((child, i) => child !== content[i]) ? children : content;
  }
  if (!React.isValidElement(content) || (content.type !== Span && content.type !== React.Fragment)) return content;
  const props = content.props as SpanProps;
  const children = themeTextContent(props.children, theme);
  if (content.type === React.Fragment) {
    return children === props.children ? content : React.createElement(React.Fragment, null, children);
  }
  const style = spanStyle(props);
  const fontFamily = style?.fontFamily === undefined ? undefined : themeFont(style.fontFamily, theme);
  const fill = themeSpanFill(style?.fill, theme);
  if (children === props.children && fontFamily === style?.fontFamily && fill === style?.fill) return content;
  return React.createElement(Span, {
    style: {
      ...style,
      ...(fontFamily !== undefined ? { fontFamily } : {}),
      ...(fill !== undefined ? { fill } : {}),
    },
  }, children);
}

/**
 * `style` with `flat`'s runs, or `style` itself when there are none. Runs
 * already in `style` and runs from spans are never combined.
 */
export function withTextRuns(style: TextStyle, flat: FlatText, owner: string): TextStyle {
  if (flat.colorRuns.length === 0 && flat.fontRuns.length === 0) return style;
  if (flat.colorRuns.length > 0 && (style.colorRuns?.length ?? 0) > 0) {
    throw new Error(`${owner} cannot combine style.colorRuns with <Span> fill`);
  }
  if (flat.fontRuns.length > 0 && (style.fontRuns?.length ?? 0) > 0) {
    throw new Error(`${owner} cannot combine style.fontRuns with <Span> fontWeight, fontFamily, or fontFeatures`);
  }
  return {
    ...style,
    ...(flat.colorRuns.length > 0 ? { colorRuns: flat.colorRuns } : {}),
    ...(flat.fontRuns.length > 0 ? { fontRuns: flat.fontRuns } : {}),
  };
}

function sliceRuns<T extends { start: number; end: number }>(runs: T[], start: number, end: number): T[] {
  return runs
    .filter((run) => run.end > start && run.start < end)
    .map((run) => ({ ...run, start: Math.max(run.start, start) - start, end: Math.min(run.end, end) - start }));
}

/** Code points `[start, end)` of `flat`, with its runs clipped and rebased. */
export function sliceTextRuns(flat: FlatText, start: number, end: number): FlatText {
  return {
    text: Array.from(flat.text).slice(start, end).join(''),
    colorRuns: sliceRuns(flat.colorRuns, start, end),
    fontRuns: sliceRuns(flat.fontRuns, start, end),
  };
}
