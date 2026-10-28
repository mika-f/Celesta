// A card with a counting number, a label above it, and a note below.

import * as React from 'react';
import type { ReactNode } from 'react';

import type { Insets } from '@celesta/layout';
import { Easings, Text, useCurrentFrame, useTextMetrics } from '@celesta/react';
import type { TextMetrics, TextStyle } from '@celesta/react';

import { countProgress, finiteValue, formatNumber } from './count';
import type { CountTiming, NumberFormat } from './count';
import { Panel } from './panel';
import type { PanelProps } from './panel';
import { validators } from './validate';

export interface StatCardProps extends Omit<PanelProps, 'width' | 'height' | 'children'>, NumberFormat, CountTiming {
  /** The number the card counts to. */
  value: number;
  /** The number it starts from. Defaults to 0. */
  from?: number;
  /** Text above the number, such as `'総ダウンロード'`. Omit for none. */
  label?: ReactNode;
  /** Text below the number, such as a period or a source. Omit for none. */
  note?: ReactNode;
  /** Defaults to `{ fontSize: 72, fontWeight: 700 }`. */
  valueStyle?: TextStyle;
  /** Defaults to `{ fontSize: 24 }`. */
  labelStyle?: TextStyle;
  /** Defaults to `{ fontSize: 20 }`. */
  noteStyle?: TextStyle;
  /** A hex color or a `<Theme>` color name for the number, the label, and the note, unless their styles set a fill. */
  color?: string;
  /** Space between the label, the number, and the note. Defaults to 8. */
  gap?: number;
  /**
   * Where the lines sit across the card, and the edge the number grows
   * from: with `'right'` its last digit and suffix stay in place. Defaults
   * to `'left'`.
   */
  align?: 'left' | 'center' | 'right';
  /** Width of the card, padding included. Defaults to the widest line plus the padding. */
  width?: number;
  /** Height of the card, padding included; the lines are centered in it. Defaults to the lines plus the padding. */
  height?: number;
  /** As `<Panel>` takes it. Defaults to 32. */
  padding?: number | Insets;
  /** Defaults to 16. */
  cornerRadius?: number;
}

const DEFAULT_VALUE_STYLE: TextStyle = { fontSize: 72, fontWeight: 700 };
const DEFAULT_LABEL_STYLE: TextStyle = { fontSize: 24 };
const DEFAULT_NOTE_STYLE: TextStyle = { fontSize: 20 };

const { nonNegative, insets } = validators('StatCard');

/**
 * A `<Panel>` holding a label, a number that counts from `from` to `value`
 * as `<Counter>` does, and a note, top to bottom; the label and the note are
 * optional. The number is printed with `formatNumber()`, so `locale`
 * (`'en-US'` unless given), `decimals`, `prefix`, `suffix`, and `formatter`
 * work as they do on `<Counter>`.
 *
 * The card is sized once, from the widest of the label, the note, and the
 * number at `from` and at `value`, so it does not grow while the number
 * counts. Pass `width` and `height` to size it yourself, such as for cards
 * of the same size in a row.
 *
 * ```tsx
 * <StatCard x={120} y={200} label="総ダウンロード" value={1280} suffix=" 件"
 *   fill="#182030" color="#F4F1EA" align="right" />
 * ```
 */
export function StatCard(props: StatCardProps): ReturnType<typeof React.createElement> {
  const {
    value, from = 0, delay = 0, durationInFrames = 30, easing = Easings.easeOutExpo,
    locale, decimals, prefix, suffix, formatter, label, note,
    valueStyle = DEFAULT_VALUE_STYLE, labelStyle = DEFAULT_LABEL_STYLE, noteStyle = DEFAULT_NOTE_STYLE,
    color, gap = 8, align = 'left', width, height, padding = 32, cornerRadius = 16, ...panel
  } = props;
  const frame = useCurrentFrame();
  const to = finiteValue(value, 'value', '<StatCard>');
  const start = finiteValue(from, 'from', '<StatCard>');
  const format = { locale, decimals, prefix, suffix, formatter };
  nonNegative('gap', gap);
  const { top, right, bottom, left } = insets(padding);

  // The number's room: the wider of its first and last text.
  const first = useTextMetrics(formatNumber(start, format, '<StatCard>'), valueStyle);
  const last = useTextMetrics(formatNumber(to, format, '<StatCard>'), valueStyle);
  const labelMetrics = useTextMetrics(label ?? '', labelStyle);
  const noteMetrics = useTextMetrics(note ?? '', noteStyle);
  const number = first.width > last.width ? first : last;

  const lines: { metrics: TextMetrics; style: TextStyle; content: ReactNode; key: string }[] = [];
  const p = countProgress(frame, delay, durationInFrames, easing, '<StatCard>');
  if (label != null) lines.push({ metrics: labelMetrics, style: labelStyle, content: label, key: 'label' });
  lines.push({ metrics: number, style: valueStyle, content: formatNumber(start + (to - start) * p, format, '<StatCard>'), key: 'value' });
  if (note != null) lines.push({ metrics: noteMetrics, style: noteStyle, content: note, key: 'note' });

  const contentWidth = Math.max(...lines.map((line) => line.metrics.width));
  const blockHeight = lines.reduce((sum, line) => sum + line.metrics.height, 0) + gap * (lines.length - 1);
  const outerWidth = width ?? left + contentWidth + right;
  const outerHeight = height ?? top + blockHeight + bottom;
  const innerWidth = Math.max(0, outerWidth - left - right);
  const innerHeight = Math.max(0, outerHeight - top - bottom);
  const anchorX = align === 'left' ? 0 : align === 'center' ? 0.5 : 1;

  let y = (innerHeight - blockHeight) / 2;
  const texts = lines.map((line) => {
    const element = React.createElement(Text, {
      key: line.key,
      x: innerWidth * anchorX,
      y: y + line.metrics.ascent,
      anchorX,
      anchorY: 'baseline',
      style: line.style,
      color,
      children: line.content,
    });
    y += line.metrics.height + gap;
    return element;
  });

  return React.createElement(Panel, {
    ...panel,
    width: outerWidth,
    height: outerHeight,
    padding: { top, right, bottom, left },
    cornerRadius,
  }, texts);
}
