// Text typed out one character at a time with a caret at the end. The whole
// text is laid out once and revealed with `visibleCharacters`, so nothing
// moves while it types, and the caret goes where the measured layout puts
// the next character, in any font.

import * as React from 'react';
import type { ReactNode } from 'react';

import { Group, Rect, Text, textCaret, useTextMetrics, useTheme } from '@celesta.studio/react';
import type { CommonProps, ResolvedTheme, TextMetrics, TextShorthand, TextStyle } from '@celesta.studio/react';
import { useTypewriter } from '@celesta.studio/text';

import { hexColor } from './color';

/** How `<TypewriterText>` draws its caret. */
export type TypewriterCaretShape = 'line' | 'block';

export interface TypewriterTextProps extends Omit<CommonProps, 'anchorX' | 'anchorY'>, TextShorthand {
  /**
   * The text; may hold `<Span>`. `\n` starts a new line. It types one Unicode
   * code point at a time, as `useTypewriter()` counts.
   */
  children: ReactNode;
  /** As for `<Text>`; `visibleCharacters` is set for you. */
  style?: TextStyle;
  /** Text language, as for `<Text lang>`; an explicit `style.lang` takes priority. */
  lang?: string;
  /** Wraps lines at this width, as `<Text maxWidth>`; the caret follows the wrapped lines. */
  maxWidth?: number;
  /**
   * Horizontal pivot on the complete text's box, as `measureText()` reports
   * it: 0 left, 0.5 center, 1 right. Defaults to 0. The box is the finished
   * text's, so the text does not shift while it types.
   */
  anchorX?: number;
  /**
   * Vertical pivot on the complete text's line boxes: 0 the top of the first
   * line, 1 the bottom of the last. `'baseline'` pivots on the first line's
   * baseline, as `<Text anchorY="baseline">`. Defaults to 0.
   */
  anchorY?: number | 'baseline';
  /** Frame typing starts, as for `useTypewriter()`. Defaults to 0. */
  from?: number;
  /** Frames per character; below 1 types several characters a frame. Defaults to 1. */
  framesPerChar?: number;
  /** Length of one caret blink cycle (on, then off) while idle, in frames. Defaults to one second. */
  blinkFrames?: number;
  /** Draws the caret. Defaults to `true`; compute it to hide the caret from some frame on. */
  caret?: boolean;
  /** `'line'` (the default), a thin bar before the next character, or `'block'`, one `0` wide. */
  caretShape?: TypewriterCaretShape;
  /** A hex color or a `<Theme>` color name. Defaults to the text's solid fill, else white. */
  caretColor?: string;
  /** Defaults to 6% of the font's line height (at least 2) for a line, and the advance of `0` for a block. */
  caretWidth?: number;
  /** Defaults to the font's own line height, without `style.lineHeight`'s extra leading. */
  caretHeight?: number;
}

/** `style` with the shorthand and `lang` filling what it leaves unset. */
function textStyleOf(
  style: TextStyle | undefined,
  { fontFamily, fontSize, fontWeight, color }: TextShorthand,
  lang: string | undefined,
  theme: ResolvedTheme,
): TextStyle {
  return {
    ...style,
    ...(fontFamily !== undefined && style?.fontFamily == null ? { fontFamily } : {}),
    ...(fontSize !== undefined && style?.fontSize == null ? { fontSize } : {}),
    ...(fontWeight !== undefined && style?.fontWeight == null ? { fontWeight } : {}),
    ...(color !== undefined && style?.fill == null
      ? { fill: { type: 'solid' as const, color: hexColor(color, theme, 'TypewriterText', 'color') } }
      : {}),
    ...(lang !== undefined && style?.lang == null ? { lang } : {}),
  };
}

/** The text's solid fill color, its `<Theme>` name looked up; undefined for a gradient. */
function solidFill(style: TextStyle, theme: ResolvedTheme): string | undefined {
  if (style.fill?.type !== 'solid' || typeof style.fill.color !== 'string') return undefined;
  return hexColor(style.fill.color, theme, 'TypewriterText', 'style.fill');
}

/** The shaped clusters, or null where the measurer has none (the browser). */
function glyphsOf(metrics: TextMetrics): TextMetrics['glyphs'] | null {
  try {
    return metrics.glyphs;
  } catch {
    return null;
  }
}

/** The characters of `content` as `<Text>` draws them, without the `<Span>`s around them. */
function plainText(content: ReactNode): string {
  if (content == null || typeof content === 'boolean') return '';
  if (typeof content === 'string' || typeof content === 'number') return String(content);
  if (Array.isArray(content)) return content.map(plainText).join('');
  if (React.isValidElement(content)) return plainText((content.props as { children?: ReactNode }).children);
  return '';
}

function positive(value: number | undefined, prop: string): void {
  if (value !== undefined && (!Number.isFinite(value) || value <= 0)) {
    throw new Error(`<TypewriterText> requires a positive \`${prop}\``);
  }
}

/**
 * Text that types itself out with a caret after it, timed by
 * `useTypewriter()`: one character every `framesPerChar` frames from
 * `from`, the caret steady while typing and blinking every `blinkFrames`
 * while idle. The complete text is laid out up front and revealed with
 * `style.visibleCharacters`, so wrapped lines, alignment, and `anchorX`/
 * `anchorY` stay where the finished text puts them. The caret is placed
 * with `textCaret()` on that layout, so it follows proportional fonts,
 * Japanese, `\n`, and `maxWidth` wrapping.
 *
 * ```tsx
 * <TypewriterText x={160} y={300} from={12} framesPerChar={2}
 *   caretShape="block" fontFamily="mono" fontSize={40} color="#f4f1ea">
 *   pnpm run build
 * </TypewriterText>
 * ```
 *
 * Before `from` and once the text is typed, the caret blinks where the
 * next character would go; empty text shows only the blinking caret.
 */
export function TypewriterText(props: TypewriterTextProps): ReturnType<typeof React.createElement> {
  const {
    children, style, lang, maxWidth, anchorX = 0, anchorY = 0,
    fontFamily, fontSize, fontWeight, color,
    from = 0, framesPerChar = 1, blinkFrames,
    caret = true, caretShape = 'line', caretColor, caretWidth, caretHeight,
    ...groupProps
  } = props;
  positive(maxWidth, 'maxWidth');
  positive(caretWidth, 'caretWidth');
  positive(caretHeight, 'caretHeight');
  if (caretShape !== 'line' && caretShape !== 'block') {
    throw new Error(`<TypewriterText> caretShape must be 'line' or 'block', got ${JSON.stringify(caretShape)}`);
  }
  const theme = useTheme();
  const typed = useTypewriter(children, { from, framesPerChar, blinkFrames });
  const textStyle = textStyleOf(style, { fontFamily, fontSize, fontWeight, color }, lang, theme);
  const metrics = useTextMetrics(children, textStyle, maxWidth === undefined ? {} : { maxWidth });
  // The font's own box: what a caret spans, whatever leading the text adds.
  const { lineHeight: _lineHeight, ...cellStyle } = textStyle;
  const cell = useTextMetrics('0', cellStyle);

  // Where the box's pivot puts the first line's baseline.
  const left = -anchorX * metrics.width;
  const baseline = anchorY === 'baseline' ? 0 : metrics.ascent - anchorY * metrics.height;
  const empty = typed.done && typed.length === 0;
  const text = empty ? null : React.createElement(Text, {
    x: left,
    y: baseline,
    anchorY: 'baseline',
    style: { ...textStyle, visibleCharacters: typed.length },
    ...(maxWidth === undefined ? {} : { maxWidth }),
    children,
  });

  let caretLayer = null;
  if (caret && typed.caretVisible) {
    const height = caretHeight ?? cell.lineHeight;
    const shape: CaretShape = {
      width: caretWidth ?? (caretShape === 'block' ? cell.width : Math.max(2, cell.lineHeight * 0.06)),
      height,
      top: (cell.lineHeight - height) / 2 - cell.ascent,
      color: caretColor === undefined
        ? solidFill(textStyle, theme) ?? '#ffffff'
        : hexColor(caretColor, theme, 'TypewriterText', 'caretColor'),
    };
    const glyphs = glyphsOf(metrics);
    caretLayer = glyphs
      ? caretRect(shape, left, baseline, textCaret(metrics, typed.length))
      : React.createElement(MeasuredCaret, {
        shape, left, baseline, metrics, maxWidth, style: textStyle, text: plainText(children), offset: typed.length,
      });
  }
  return React.createElement(Group, groupProps, text, caretLayer);
}

interface CaretShape {
  width: number;
  height: number;
  /** From the baseline of the caret's line to the caret's top. */
  top: number;
  color: string;
}

/** The caret at `position` (from `textCaret()`), the text's first baseline at `baseline`. */
function caretRect(shape: CaretShape, left: number, baseline: number, position: { x: number; y: number }) {
  return React.createElement(Rect, {
    x: left + position.x,
    y: baseline + position.y + shape.top,
    width: shape.width,
    height: shape.height,
    fill: shape.color,
  });
}

interface MeasuredCaretProps {
  shape: CaretShape;
  left: number;
  baseline: number;
  metrics: TextMetrics;
  maxWidth: number | undefined;
  style: TextStyle;
  text: string;
  offset: number;
}

/**
 * The caret where the measurer has no glyph positions (the browser):
 * after the typed part of its line, measured as text of its own, and
 * shifted as `style.align` shifts the whole line within `maxWidth`, else
 * within the widest line, as the browser draws it. `<Span>` fonts are not
 * applied to these measurements.
 */
function MeasuredCaret({ shape, left, baseline, metrics, maxWidth, style, text, offset }: MeasuredCaretProps) {
  const characters = Array.from(text);
  const starts = metrics.lineStarts ?? [0];
  const line = starts.reduce((line, start, index) => start <= offset ? index : line, 0);
  const start = starts[line] ?? 0;
  // A line ends before its `\n`, and a wrapped line before its trailing spaces.
  let end = starts[line + 1] ?? characters.length;
  if (characters[end - 1] === '\n') end -= 1;
  else if (line + 1 < starts.length) while (end > start && /\s/u.test(characters[end - 1]!)) end -= 1;
  const { align, ...lineStyle } = style;
  const typed = useTextMetrics(characters.slice(start, offset).join(''), lineStyle).width;
  const whole = useTextMetrics(characters.slice(start, end).join(''), lineStyle).width;
  const shift = align === 'center' ? 0.5 : align === 'right' ? 1 : 0;
  const box = maxWidth ?? metrics.width;
  return caretRect(shape, left, baseline, { x: shift * (box - whole) + typed, y: line * metrics.lineHeight });
}
