// Rich text: <Span> styles part of a text, and its content flattens into one
// string with code-point color and font runs, so the whole text is shaped as
// one paragraph.

import * as React from 'react';
import type { ReactNode } from 'react';

import type { Paint, TextColorRun, TextFontRun, TextStyle } from './scene';

export interface SpanStyle {
  /** A hex color (`"#RRGGBB"` or `"#RRGGBBAA"`) or a solid `Paint` with one; other formats throw. */
  fill?: string | Paint;
  /** 1–1000; a family without that weight uses its nearest face. */
  fontWeight?: number;
  fontFamily?: string;
}

export interface SpanProps {
  style?: SpanStyle;
  children?: ReactNode;
}

/**
 * Changes the fill, weight, or family of part of a text, which is still
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
  fontFamily?: string;
}

/** The color formats the renderer reads; checked here so a bad one fails at the span, not at export. */
const HEX_COLOR = /^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/;

function resolveSpan(style: SpanStyle | undefined, inherited: Inherited): Inherited {
  const next = { ...inherited };
  if (style?.fill !== undefined) {
    const fill = style.fill;
    const color = typeof fill === 'string' ? fill : fill && fill.type === 'solid' ? fill.color : undefined;
    if (color === undefined) throw new Error('<Span> fill must be a color or a solid paint');
    if (typeof color !== 'string' || !HEX_COLOR.test(color)) {
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
    if (typeof style.fontFamily !== 'string' || style.fontFamily.trim() === '') {
      throw new Error('<Span> fontFamily must be a non-empty string');
    }
    next.fontFamily = style.fontFamily;
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
      if (inherited.fontWeight !== undefined || inherited.fontFamily !== undefined) {
        push(fontRuns, {
          start,
          end: length,
          ...(inherited.fontWeight !== undefined ? { fontWeight: inherited.fontWeight } : {}),
          ...(inherited.fontFamily !== undefined ? { fontFamily: inherited.fontFamily } : {}),
        }, (a, b) => a.fontWeight === b.fontWeight && a.fontFamily === b.fontFamily);
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
        visit(props.children, resolveSpan(props.style, inherited));
        return;
      }
    }
    throw new Error(`${what} must be a string, a number, or an array of those; style part of the text with <Span>`);
  };
  visit(content, {});
  return { text, colorRuns, fontRuns };
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
    throw new Error(`${owner} cannot combine style.fontRuns with <Span> fontWeight or fontFamily`);
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
