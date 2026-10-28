// Horizontal bars that grow while their values count up, one row per item,
// and the same chart with ranks.

import * as React from 'react';
import type { ReactNode } from 'react';

import { Easings, Group, Rect, Text, useCurrentFrame, useTextMetrics, useTheme } from '@celesta/react';
import type { GroupProps, TextStyle } from '@celesta/react';

import { hexColor } from './color';
import { countProgress, formatNumber } from './count';
import type { CountTiming, NumberFormat } from './count';
import { validators } from './validate';

export interface BarChartItem {
  /** Identifies the row: unique in the chart, and the row's React key, so a row keeps its identity when the order changes. */
  id: string;
  /** Text in the label column. */
  label?: ReactNode;
  /** A finite number of 0 or more. */
  value: number;
  /** A hex color or a `<Theme>` color name for this bar, instead of `barColor`. */
  color?: string;
  /** A hex color or a `<Theme>` color name for this row's label and value, over the fill of `labelStyle` and `valueStyle`. */
  textColor?: string;
  /** Smaller text under the label. */
  note?: ReactNode;
}

/** The order rows are drawn in, top to bottom: as given, or by value with equal values kept in their given order. */
export type BarChartOrder = 'input' | 'descending' | 'ascending';

export interface BarChartProps extends Omit<GroupProps, 'children'>, NumberFormat, CountTiming {
  data: readonly BarChartItem[];
  /** Width of the whole chart: label column, bars, and values. */
  width: number;
  /** The value of a full-length bar. Defaults to the largest value; larger values draw full length. */
  max?: number;
  /** Defaults to `'input'`. */
  order?: BarChartOrder;
  /** Height of a row. Defaults to the taller of `barHeight` and the label with its note. */
  rowHeight?: number;
  /** Space between rows. Defaults to 16. */
  rowGap?: number;
  /** Defaults to 28. */
  barHeight?: number;
  /** Rounds the bars, and the track; never more than half a bar. Defaults to 4. */
  cornerRadius?: number;
  /** A hex color or a `<Theme>` color name. Defaults to `#4F8CFF`. */
  barColor?: string;
  /** A hex color or a `<Theme>` color name for the full-length track behind each bar. Omit for none. */
  trackColor?: string;
  /** Width of the label column. Defaults to the widest label or note; a label wider than a given width is cut off. */
  labelWidth?: number;
  /** Space between the label column and the bars. Defaults to 24. */
  labelGap?: number;
  /** Room kept for the value right of a full-length bar. Defaults to the widest value. */
  valueWidth?: number;
  /** Space between a bar's end and its value. Defaults to 16. */
  valueGap?: number;
  /**
   * `'end'` (the default) puts each value just past its bar's end, moving
   * with it; `'right'` lines the values up at the chart's right edge, their
   * last digits in place as they count.
   */
  valuePosition?: 'end' | 'right';
  /** Defaults to `{ fontSize: 28, fontWeight: 700 }`. */
  labelStyle?: TextStyle;
  /** Defaults to `{ fontSize: 18 }`. */
  noteStyle?: TextStyle;
  /** Defaults to `{ fontSize: 28, fontWeight: 700 }`. */
  valueStyle?: TextStyle;
  /** Frames between one row's start and the next's, top to bottom. Defaults to 0. */
  stagger?: number;
  /** Frames a row fades in over from its start, through `easing`; 0 shows it at its start. Defaults to 8. */
  fadeInFrames?: number;
}

export interface RankingProps extends Omit<BarChartProps, 'order'> {
  /**
   * Required: `'descending'` ranks the largest value first, `'ascending'`
   * the smallest, and `'input'` keeps the rows in the order given (already
   * ranked). A rank is the row's position from the top, 1 first; equal
   * values keep their given order and get consecutive ranks.
   */
  order: BarChartOrder;
  /** Prints a rank. Defaults to `String(rank)`. */
  formatRank?: (rank: number) => string;
  /** Width of the rank column; ranks line up at its right edge. Defaults to the widest rank. */
  rankWidth?: number;
  /** Space between the rank column and the labels. Defaults to 24. */
  rankGap?: number;
  /** Defaults to `labelStyle`. */
  rankStyle?: TextStyle;
}

const DEFAULT_LABEL_STYLE: TextStyle = { fontSize: 28, fontWeight: 700 };
const DEFAULT_NOTE_STYLE: TextStyle = { fontSize: 18 };
const DEFAULT_VALUE_STYLE: TextStyle = { fontSize: 28, fontWeight: 700 };
/** Space between a label and its note. */
const NOTE_GAP = 6;

/** `data` checked and put in `order`. */
function orderedRows(data: readonly BarChartItem[], order: BarChartOrder, component: string): BarChartItem[] {
  const { fail } = validators(component);
  if (!Array.isArray(data)) fail('data must be an array');
  const ids = new Set<string>();
  for (const item of data) {
    if (typeof item.id !== 'string' || item.id === '') fail('every item needs a non-empty string id');
    if (ids.has(item.id)) fail(`id ${JSON.stringify(item.id)} is used by more than one item`);
    ids.add(item.id);
    if (typeof item.value !== 'number' || !Number.isFinite(item.value)) {
      fail(`item ${JSON.stringify(item.id)} value must be a finite number, not ${item.value}`);
    }
    if (item.value < 0) {
      fail(`item ${JSON.stringify(item.id)} value must be 0 or more, not ${item.value}; bars grow from 0 to the right`);
    }
  }
  if (order === 'input') return [...data];
  if (order !== 'descending' && order !== 'ascending') fail(`order must be 'input', 'descending', or 'ascending', not ${JSON.stringify(order)}`);
  // Array sort is stable, so equal values keep their given order.
  return [...data].sort((a, b) => order === 'descending' ? b.value - a.value : a.value - b.value);
}

/** Lines of text joined with line breaks, so one measurement gives the widest. */
function lines(contents: readonly ReactNode[]): ReactNode[] {
  return contents.flatMap((content, i) => (i === 0 ? [content] : ['\n', content]));
}

function Chart(props: BarChartProps & { ranks?: Omit<RankingProps, keyof BarChartProps> & { order: BarChartOrder } }, component: string) {
  const {
    data, width, max, order = 'input', rowHeight, rowGap = 16, barHeight = 28, cornerRadius = 4,
    barColor = '#4F8CFF', trackColor, labelWidth, labelGap = 24, valueWidth, valueGap = 16, valuePosition = 'end',
    labelStyle = DEFAULT_LABEL_STYLE, noteStyle = DEFAULT_NOTE_STYLE, valueStyle = DEFAULT_VALUE_STYLE,
    stagger = 0, fadeInFrames = 8, delay = 0, durationInFrames = 30, easing = Easings.easeOutExpo,
    locale, decimals, prefix, suffix, formatter, ranks, ...group
  } = props;
  const theme = useTheme();
  const frame = useCurrentFrame();
  const { fail, nonNegative } = validators(component);
  const rows = orderedRows(data, ranks?.order ?? order, component);
  for (const [name, value] of Object.entries({ width, rowGap, barHeight, cornerRadius, labelGap, valueGap, stagger, fadeInFrames })) {
    nonNegative(name, value);
  }
  for (const [name, value] of Object.entries({ max, rowHeight, labelWidth, valueWidth })) {
    if (value !== undefined) nonNegative(name, value);
  }
  if (valuePosition !== 'end' && valuePosition !== 'right') fail(`valuePosition must be 'end' or 'right', not ${JSON.stringify(valuePosition)}`);
  const format = { locale, decimals, prefix, suffix, formatter };
  const formatRank = ranks?.formatRank ?? String;
  const rankStyle = ranks?.rankStyle ?? labelStyle;

  // Every label, note, final value, and rank measured at once: each
  // measurement's width is its widest line.
  const labelMetrics = useTextMetrics(lines(rows.map((row) => row.label ?? '')), labelStyle);
  const noteMetrics = useTextMetrics(lines(rows.map((row) => row.note ?? '')), noteStyle);
  const valueMetrics = useTextMetrics(lines(rows.map((row) => formatNumber(row.value, format, `<${component}>`))), valueStyle);
  const rankMetrics = useTextMetrics(ranks ? lines(rows.map((_, i) => formatRank(i + 1))) : '', rankStyle);

  const hasNote = rows.some((row) => row.note != null);
  const labelBlock = labelMetrics.lineHeight + (hasNote ? NOTE_GAP + noteMetrics.lineHeight : 0);
  const height = rowHeight ?? Math.max(barHeight, labelBlock);
  const rankWidth = ranks ? nonNegative('rankWidth', ranks.rankWidth ?? rankMetrics.width) : 0;
  const rankGap = ranks ? nonNegative('rankGap', ranks.rankGap ?? 24) : 0;
  const rankColumn = ranks ? rankWidth + rankGap : 0;
  const labelColumn = labelWidth ?? Math.max(labelMetrics.width, hasNote ? noteMetrics.width : 0);
  const barX = rankColumn + labelColumn + labelGap;
  const track = Math.max(0, width - barX - (valueWidth ?? valueMetrics.width) - valueGap);
  const scale = max ?? Math.max(0, ...rows.map((row) => row.value));
  const labelTop = (height - labelBlock) / 2;
  const barTop = (height - barHeight) / 2;
  // Colors are checked up front, not only once a bar has length.
  const trackFill = trackColor === undefined ? undefined : hexColor(trackColor, theme, component, 'trackColor');
  const defaultFill = hexColor(barColor, theme, component, 'barColor');
  const fills = rows.map((row) => row.color === undefined ? defaultFill : hexColor(row.color, theme, component, `color of ${JSON.stringify(row.id)}`));
  // A row's text color wins over the styles' fill.
  const textStyles = rows.map((row) => {
    if (row.textColor === undefined) return { label: labelStyle, value: valueStyle };
    const fill = { type: 'solid' as const, color: hexColor(row.textColor, theme, component, `textColor of ${JSON.stringify(row.id)}`) };
    return { label: { ...labelStyle, fill }, value: { ...valueStyle, fill } };
  });

  return React.createElement(Group, group, rows.map((row, index) => {
    const start = delay + index * stagger;
    const p = countProgress(frame, start, durationInFrames, easing, `<${component}>`);
    const opacity = countProgress(frame, start, fadeInFrames, easing, `<${component}>`);
    // The bar and the printed value come from the same progress.
    const length = track * (scale > 0 ? Math.min(1, row.value / scale) : 0) * p;
    const radius = Math.min(cornerRadius, barHeight / 2);
    const text = textStyles[index]!;
    const children: ReactNode[] = [];
    if (ranks) {
      children.push(React.createElement(Text, {
        key: 'rank', x: rankWidth, y: height / 2, anchorX: 1, anchorY: 0.5, style: rankStyle,
        children: formatRank(index + 1),
      }));
    }
    children.push(React.createElement(Group, {
      key: 'label', x: rankColumn,
      clip: labelWidth === undefined ? undefined : { x: 0, y: 0, width: labelWidth, height },
    },
    React.createElement(Text, { key: 'text', y: labelTop + labelMetrics.ascent, anchorY: 'baseline', style: text.label, children: row.label ?? '' }),
    row.note == null ? null : React.createElement(Text, {
      key: 'note', y: labelTop + labelMetrics.lineHeight + NOTE_GAP + noteMetrics.ascent, anchorY: 'baseline', style: noteStyle, children: row.note,
    })));
    if (trackFill !== undefined && track > 0) {
      children.push(React.createElement(Rect, {
        key: 'track', x: barX, y: barTop, width: track, height: barHeight, cornerRadius: Math.min(radius, track / 2), fill: trackFill,
      }));
    }
    if (length > 0) {
      children.push(React.createElement(Rect, {
        key: 'bar', x: barX, y: barTop, width: length, height: barHeight, cornerRadius: Math.min(radius, length / 2),
        fill: fills[index],
      }));
    }
    children.push(React.createElement(Text, {
      key: 'value',
      x: valuePosition === 'end' ? barX + length + valueGap : width,
      y: height / 2,
      anchorX: valuePosition === 'end' ? 0 : 1,
      anchorY: 0.5,
      style: text.value,
      children: formatNumber(row.value * p, format, `<${component}>`),
    }));
    return React.createElement(Group, { key: row.id, y: index * (height + rowGap), opacity }, children);
  }));
}

/**
 * A horizontal bar chart: one row per item of `data`, with its label (and a
 * note under it) in a column on the left, a bar from 0 whose length is
 * `value / max` of the room, and the value printed with `formatNumber()`
 * (`locale` `'en-US'` unless given, `decimals`, `prefix`, `suffix`,
 * `formatter`) past the bar's end or at the right edge.
 *
 * Each row starts `stagger` frames after the one above it, from `delay`:
 * it fades in over `fadeInFrames`, and over `durationInFrames` its bar grows
 * and its value counts up from 0 with one progress, so the printed number
 * always matches the bar.
 *
 * Values must be finite and 0 or more; ids must be unique. Empty `data`
 * draws nothing. When every value is 0 (or `max` is 0), the bars have no
 * length and the values print 0.
 *
 * ```tsx
 * <BarChart x={120} y={240} width={1000} stagger={8} suffix=" 件" data={[
 *   { id: 'a', label: 'A', value: 120 },
 *   { id: 'b', label: 'B', value: 80 },
 * ]} />
 * ```
 */
export function BarChart(props: BarChartProps): ReturnType<typeof React.createElement> {
  return Chart(props, 'BarChart');
}

/**
 * A `<BarChart>` with a rank column before the labels. `order` is required,
 * so whether the rows are sorted is always written down: `'descending'` and
 * `'ascending'` sort by value, keeping equal values in their given order,
 * and `'input'` keeps the given order. The rank is the row's position from
 * the top, printed by `formatRank`.
 *
 * ```tsx
 * <Ranking width={1600} order="descending" formatRank={(r) => String(r).padStart(2, '0')}
 *   data={rows.map((row) => ({ id: row.name, label: row.name, value: row.score }))} />
 * ```
 */
export function Ranking(props: RankingProps): ReturnType<typeof React.createElement> {
  const { order, formatRank, rankWidth, rankGap, rankStyle, ...chart } = props;
  if (order === undefined) {
    throw new Error("<Ranking> order is required: 'descending', 'ascending', or 'input' to keep the given order");
  }
  return Chart({ ...chart, ranks: { order, formatRank, rankWidth, rankGap, rankStyle } }, 'Ranking');
}
