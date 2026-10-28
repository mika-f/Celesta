// A label whose background is sized from the measured text: a tag, a chip,
// or a pill, with an optional decoration (a dot, an icon) before the text.

import * as React from 'react';
import type { ReactNode } from 'react';

import { LayoutBounds } from '@celesta/layout';
import type { Insets } from '@celesta/layout';
import { Text, useTextMetrics } from '@celesta/react';
import type { TextShorthand, TextStyle } from '@celesta/react';

import { Panel } from './panel';
import type { PanelProps } from './panel';
import { validators } from './validate';

export interface BadgeProps extends Omit<PanelProps, 'width' | 'height' | 'cornerRadius' | 'children'>, TextShorthand {
  /** The label: text, which may hold `<Span>`. */
  children: ReactNode;
  /** The label's `TextStyle`; the shorthand props fill what it leaves unset. */
  style?: TextStyle;
  /**
   * Space between the edges and the content, as `<Panel>` takes it.
   * Defaults to 8 above and below and 16 left and right.
   */
  padding?: number | Insets;
  /**
   * Rounds the corners: a radius, or `'pill'` for half the badge's height,
   * so the ends are semicircles whatever the font size. Defaults to 8.
   */
  cornerRadius?: number | 'pill';
  /**
   * Drawn before the text, such as a colored dot. It lays out in a box
   * `leadingWidth` wide and one line tall at the start of the content, so
   * `<Center>` inside it centers on the first line. Needs `leadingWidth`.
   */
  leading?: ReactNode;
  /** Width of the `leading` box. */
  leadingWidth?: number;
  /** Space between `leading` and the text. Defaults to 8. */
  gap?: number;
  /**
   * The widest the badge may be, padding included. Longer text wraps onto
   * more lines and the badge grows taller; a word wider than the room is cut
   * off at the edge, as `<Text maxWidth>` does. Omit for one line.
   */
  maxWidth?: number;
}

const DEFAULT_PADDING: Required<Insets> = { top: 8, right: 16, bottom: 8, left: 16 };

const { fail, nonNegative, insets } = validators('Badge');

/**
 * A label drawn on a `<Panel>` sized from its measured text, so the
 * background fits Japanese, Latin, and proportional fonts alike without
 * guessing widths from the number of characters. It measures with
 * `useTextMetrics()` and draws with `<Text>`, passing both the same text and
 * style, and is as tall as the text's line boxes plus the padding.
 *
 * The background, border, shadow, and `anchorX`/`anchorY` are `<Panel>`'s,
 * and every transform applies to the whole badge, so the background and the
 * text move together. `cornerRadius="pill"` makes a chip with round ends.
 *
 * ```tsx
 * <Badge x={120} y={840} fill="#204060" cornerRadius="pill"
 *   fontFamily="mono" fontSize={22} color="#ffffff"
 *   leading={<Center><Circle radius={4} fill="#5BC0FF" /></Center>} leadingWidth={8}>
 *   第1章
 * </Badge>
 * ```
 */
export function Badge(props: BadgeProps): ReturnType<typeof React.createElement> {
  const {
    children, style, fontFamily, fontSize, fontWeight, color, padding = DEFAULT_PADDING, cornerRadius = 8,
    leading, leadingWidth, gap = 8, maxWidth, ...panel
  } = props;
  const { lang, fontFallback } = panel;
  const { top, right, bottom, left } = insets(padding);
  if (cornerRadius !== 'pill') nonNegative('cornerRadius', cornerRadius);
  nonNegative('gap', gap);
  if (leading != null && leadingWidth === undefined) fail('leading needs a leadingWidth, the width it lays out in');
  const leadWidth = leading == null ? 0 : nonNegative('leadingWidth', leadingWidth!);
  const lead = leading == null ? 0 : leadWidth + gap;
  const textMaxWidth = maxWidth === undefined ? undefined : Math.max(0, nonNegative('maxWidth', maxWidth) - left - right - lead);

  // One style for the measurement and the drawing. The measurement does not
  // see the badge's own group, so its language and fallback go in too.
  const textStyle: TextStyle = {
    ...style,
    ...(fontFamily !== undefined && style?.fontFamily == null ? { fontFamily } : {}),
    ...(fontSize !== undefined && style?.fontSize == null ? { fontSize } : {}),
    ...(fontWeight !== undefined && style?.fontWeight == null ? { fontWeight } : {}),
    ...(lang !== undefined && style?.lang == null ? { lang } : {}),
    ...(fontFallback !== undefined && style?.fontFallback == null ? { fontFallback } : {}),
  };
  const metrics = useTextMetrics(children, textStyle, textMaxWidth === undefined ? {} : { maxWidth: textMaxWidth });
  // Empty text still keeps one line's height.
  const textHeight = Math.max(metrics.height, metrics.lineHeight);
  const width = left + lead + metrics.width + right;
  const height = top + textHeight + bottom;

  return React.createElement(Panel, {
    ...panel,
    width,
    height,
    padding: { top, right, bottom, left },
    cornerRadius: cornerRadius === 'pill' ? height / 2 : cornerRadius,
  },
  leading == null ? null : React.createElement(LayoutBounds, { key: 'leading', width: leadWidth, height: metrics.lineHeight }, leading),
  React.createElement(Text, {
    key: 'text',
    x: lead,
    y: metrics.ascent,
    anchorY: 'baseline',
    style: textStyle,
    color,
    maxWidth: textMaxWidth,
    children,
  }));
}
