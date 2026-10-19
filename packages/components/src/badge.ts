// A short label on a background sized to its text: a chapter tag, a
// status chip, an API name. The text is measured with the same shaping
// `<Text>` draws with, so the box fits any font and script without
// estimating widths from the character count.

import * as React from 'react';

import { Group, Rect, Text, useTextMetrics, useTheme } from '@celesta/react';
import type { CommonProps, Paint, TextShorthand, TextStyle } from '@celesta/react';

/**
 * Space between a badge's edge and its contents: one number for every
 * side, or sides by name. `top`/`right`/`bottom`/`left` override
 * `vertical`/`horizontal`; missing sides are 0.
 */
export type BadgePadding = number | {
  horizontal?: number;
  vertical?: number;
  top?: number;
  right?: number;
  bottom?: number;
  left?: number;
};

export interface BadgeProps extends Omit<CommonProps, 'anchorX' | 'anchorY'>, TextShorthand {
  /** Strings, numbers, and `<Span>` elements, as for `<Text>`. */
  children?: React.ReactNode;
  /** Style of the text; its fields win over `fontFamily`, `fontSize`, `fontWeight`, and `color`. */
  style?: TextStyle;
  /** Defaults to `{ horizontal: 16, vertical: 8 }`. */
  padding?: BadgePadding;
  /** A hex color, a `<Theme>` color name, or a `Paint` for the background. Defaults to `#1f1f1f`. */
  fill?: string | Paint;
  /** A hex color, a `<Theme>` color name, or a `Paint` for the outline; needs `strokeWidth`. */
  stroke?: string | Paint;
  strokeWidth?: number;
  /** `'pill'` rounds the ends fully, at half the height. Defaults to 0. */
  cornerRadius?: number | 'pill';
  /**
   * Drawn before the text, such as a colored dot, with its origin at the
   * left padding edge and the middle of the text's lines: draw it from
   * `y = -height / 2`. Takes `leadingWidth` of the row.
   */
  leading?: React.ReactNode;
  /** Width `leading` takes before the gap. Defaults to 0. */
  leadingWidth?: number;
  /** Space between `leading` and the text. Defaults to 8. */
  gap?: number;
  /**
   * Widest the badge may be, padding included. Longer text wraps onto more
   * lines, as `<Text maxWidth>` does, and the badge grows taller; a word
   * wider than the room is cut off. Defaults to no limit.
   */
  maxWidth?: number;
  /** Horizontal point of the badge `x` places and transforms turn about: 0 left, 1 right. Defaults to 0. */
  anchorX?: number;
  /** Vertical point of the badge `y` places and transforms turn about: 0 top, 1 bottom. Defaults to 0. */
  anchorY?: number;
}

const DEFAULT_PADDING = { horizontal: 16, vertical: 8 };

function sides(padding: BadgePadding): { top: number; right: number; bottom: number; left: number } {
  if (typeof padding === 'number') return { top: padding, right: padding, bottom: padding, left: padding };
  const { horizontal = 0, vertical = 0 } = padding;
  return {
    top: padding.top ?? vertical,
    right: padding.right ?? horizontal,
    bottom: padding.bottom ?? vertical,
    left: padding.left ?? horizontal,
  };
}

/**
 * A label whose background is sized to its measured text plus `padding`,
 * drawn and placed as one: `x`/`y` place the badge's `anchorX`/`anchorY`
 * point, and `scale`, `rotation`, and the other transforms turn the
 * background and the text together about it. Empty text keeps the height
 * of one line, so a badge does not collapse while its text changes.
 *
 * ```tsx
 * <Badge x={120} y={80} padding={{ horizontal: 20, vertical: 10 }}
 *   fill="#204060" fontFamily="sans" fontSize={28} color="#ffffff">第1章</Badge>
 * ```
 */
export function Badge(props: BadgeProps): ReturnType<typeof React.createElement> {
  const {
    children, style, fontFamily, fontSize = 24, fontWeight, color = '#ffffff',
    padding = DEFAULT_PADDING, fill = '#1f1f1f', stroke, strokeWidth, cornerRadius = 0,
    leading, leadingWidth = 0, gap = 8, maxWidth,
    anchorX = 0, anchorY = 0,
    ...transform
  } = props;
  const theme = useTheme();
  const pad = sides(padding);

  // What `<Text>` lays out with: `style` wins over the shorthand.
  const textStyle: TextStyle = { fontSize };
  if (fontFamily !== undefined) textStyle.fontFamily = fontFamily;
  if (fontWeight !== undefined) textStyle.fontWeight = fontWeight;
  for (const [key, value] of Object.entries(style ?? {})) {
    if (value != null) (textStyle as Record<string, unknown>)[key] = value;
  }

  const hasLeading = leading !== undefined && leading !== null && leading !== false;
  const natural = useTextMetrics(children, textStyle);
  // No gap after the leading when there is no text to keep apart from it.
  const before = hasLeading ? leadingWidth + (natural.width > 0 ? gap : 0) : 0;
  const room = maxWidth === undefined ? undefined : Math.max(0, maxWidth - pad.left - pad.right - before);
  // Measured again at the room only when the text needs it, so short text
  // keeps its own width and `<Text>` its own alignment.
  const wraps = room !== undefined && natural.width > room;
  const wrapped = useTextMetrics(children, textStyle, wraps ? { maxWidth: room } : {});
  const metrics = wraps ? wrapped : natural;
  // An empty text has no lines; keep the height of one so the badge holds still.
  const line = useTextMetrics('​', textStyle);
  const textHeight = metrics.lines > 0 ? metrics.height : line.height;
  const ascent = metrics.lines > 0 ? metrics.ascent : line.ascent;
  // Wrapped lines center or right-align across the whole room, so only
  // left-aligned text lets the badge hug its widest line.
  const aligned = textStyle.align === 'center' || textStyle.align === 'right';
  const textWidth = wraps ? (aligned ? room! : Math.min(room!, metrics.width)) : metrics.width;

  const width = pad.left + before + textWidth + pad.right;
  const height = pad.top + textHeight + pad.bottom;
  const paint = (value: string | Paint | undefined): string | Paint | undefined =>
    typeof value === 'string' && Object.prototype.hasOwnProperty.call(theme.colors, value) ? theme.colors[value] : value;

  return React.createElement(Group, transform,
    React.createElement(Group, { x: -anchorX * width, y: -anchorY * height },
      React.createElement(Rect, {
        width, height,
        fill: paint(fill), stroke: paint(stroke), strokeWidth,
        cornerRadius: cornerRadius === 'pill' ? height / 2 : cornerRadius,
      }),
      hasLeading ? React.createElement(Group, { x: pad.left, y: pad.top + textHeight / 2 }, leading) : null,
      React.createElement(Text, {
        x: pad.left + before,
        y: pad.top + ascent,
        anchorY: 'baseline',
        ...(wraps ? { maxWidth: room } : {}),
        style: textStyle,
        color,
        children,
      }),
    ),
  );
}
