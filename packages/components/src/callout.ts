// A note pointing at something: a box with a title and text, sized from
// the measured text, and a line with an arrowhead from the box to a point.

import * as React from 'react';
import type { ReactNode } from 'react';

import type { Insets } from '@celesta/layout';
import { Group, Text, useTextMetrics, useTheme } from '@celesta/react';
import type { GroupProps, TextStyle } from '@celesta/react';
import { Arrow, Polyline } from '@celesta/shapes';
import type { LineCap, PolylinePoint } from '@celesta/shapes';

import { hexColor } from './color';
import { Panel } from './panel';
import type { PanelProps } from './panel';
import { validators } from './validate';

/** The box edge the line leaves from: the one facing the target (`'auto'`), or a named one. */
export type CalloutSide = 'auto' | 'top' | 'right' | 'bottom' | 'left';

/** `'straight'` runs one segment to the target; `'elbow'` goes out of the edge, then turns once. */
export type CalloutRoute = 'straight' | 'elbow';

export interface CalloutProps
  extends Pick<GroupProps, 'id' | 'opacity' | 'lang' | 'fontFallback'>,
    Pick<PanelProps, 'fill' | 'stroke' | 'strokeWidth' | 'shadow'> {
  /** The point the line ends at, in the same coordinates as `x`/`y`: the Callout's parent's. */
  target: { x: number; y: number };
  /** The point of the box at `anchorX`/`anchorY` (its top-left corner by default), in the parent's coordinates. */
  x: number;
  y: number;
  /**
   * Which point of the box sits at `x`/`y`, as fractions of its width and
   * measured height: 0 the left or top edge (the default), 0.5 the middle.
   * It only places the box; the line still ends at `target`.
   */
  anchorX?: number;
  anchorY?: number;
  /** Width of the box, padding included; its height comes from the measured text. */
  width: number;
  /** Bold first line. Omit for none. */
  title?: ReactNode;
  /** Body text under the title. Omit for none. */
  text?: ReactNode;
  /** Defaults to `{ fontSize: 28, fontWeight: 700 }`. */
  titleStyle?: TextStyle;
  /** Defaults to `{ fontSize: 22 }`. */
  textStyle?: TextStyle;
  /** A hex color or a `<Theme>` color name for the title and the text, unless their styles set a fill. */
  color?: string;
  /** Space between the title and the text. Defaults to 8. */
  gap?: number;
  /** As `<Panel>` takes it. Defaults to 20. */
  padding?: number | Insets;
  /** Defaults to 12. */
  cornerRadius?: number;
  /** Defaults to `'auto'`. The line leaves from the middle of the edge. */
  side?: CalloutSide;
  /** Defaults to `'straight'`. */
  route?: CalloutRoute;
  /** A hex color or a `<Theme>` color name for the line and the arrowhead. Defaults to `#FFFFFF`. */
  lineColor?: string;
  /** Defaults to 2. */
  lineWidth?: number;
  /** Dashes of the line, as `<Line dash>` takes them. The arrowhead stays solid. */
  dash?: readonly number[];
  /** How the line's ends are drawn without an arrowhead. Defaults to `'round'`. */
  cap?: LineCap;
  /** `'arrow'` (the default) points at the target; `'none'` ends the line there. */
  head?: 'arrow' | 'none';
  /** As `<Arrow>` takes them. Default to 4 × `lineWidth`. */
  headLength?: number;
  headWidth?: number;
  /**
   * How much of the line is drawn, 0–1 of its length from the target, so
   * animating it from 0 to 1 grows the line from the target to the box.
   * Defaults to 1.
   */
  lineProgress?: number;
}

const DEFAULT_TITLE_STYLE: TextStyle = { fontSize: 28, fontWeight: 700 };
const DEFAULT_TEXT_STYLE: TextStyle = { fontSize: 22 };

const { fail, finite, nonNegative, insets } = validators('Callout');

interface Box { left: number; top: number; right: number; bottom: number }

/**
 * The edge facing `target`: of the distances `target` lies outside the box
 * across and down, the larger one's. `undefined` when it is inside.
 */
function facingSide(box: Box, target: { x: number; y: number }): Exclude<CalloutSide, 'auto'> | undefined {
  const across = target.x < box.left ? box.left - target.x : target.x > box.right ? target.x - box.right : 0;
  const down = target.y < box.top ? box.top - target.y : target.y > box.bottom ? target.y - box.bottom : 0;
  if (across === 0 && down === 0) return undefined;
  if (across >= down) return target.x < box.left ? 'left' : 'right';
  return target.y < box.top ? 'top' : 'bottom';
}

/** The line's points from the middle of `side` to `target`, without zero-length segments. */
function route(box: Box, side: Exclude<CalloutSide, 'auto'>, target: { x: number; y: number }, shape: CalloutRoute): PolylinePoint[] {
  const middleX = (box.left + box.right) / 2;
  const middleY = (box.top + box.bottom) / 2;
  const start: PolylinePoint = side === 'left' ? [box.left, middleY]
    : side === 'right' ? [box.right, middleY]
      : side === 'top' ? [middleX, box.top] : [middleX, box.bottom];
  const end: PolylinePoint = [target.x, target.y];
  const points: PolylinePoint[] = [start];
  if (shape === 'elbow') {
    // Out of the edge, then one turn towards the target.
    points.push(side === 'left' || side === 'right' ? [target.x, start[1]] : [start[0], target.y]);
  }
  points.push(end);
  return points.filter((point, i) => i === 0 || point[0] !== points[i - 1]![0] || point[1] !== points[i - 1]![1]);
}

/** The part of `points` within `amount` (0–1) of its length from the last point, still ending there. */
function fromEnd(points: PolylinePoint[], amount: number): PolylinePoint[] {
  if (amount >= 1) return points;
  const lengths = points.slice(1).map((point, i) => Math.hypot(point[0] - points[i]![0], point[1] - points[i]![1]));
  let left = lengths.reduce((sum, length) => sum + length, 0) * amount;
  const kept: PolylinePoint[] = [points[points.length - 1]!];
  for (let i = points.length - 1; i > 0 && left > 0; i -= 1) {
    const length = lengths[i - 1]!;
    const [x0, y0] = points[i - 1]!;
    const [x1, y1] = points[i]!;
    if (length <= left) {
      kept.unshift([x0, y0]);
    } else {
      const t = left / length;
      kept.unshift([x1 + (x0 - x1) * t, y1 + (y0 - y1) * t]);
    }
    left -= length;
  }
  return kept;
}

/**
 * A note that points at something: a `<Panel>` at `x`/`y`, `width` wide,
 * holding a `title` and a `text` wrapped inside its padding and as tall as
 * they measure, and a line from the box to `target` ending in an arrowhead.
 *
 * `target`, `x`, and `y` are all in the coordinates of the Callout's parent,
 * so a Callout inside the same `<Group>` as what it points at stays on it
 * whatever that group's transform. The Callout has no transform of its own
 * for the same reason: scaling or rotating it would move the line's end off
 * the target. Animate `target`, `x`, and `y` instead, and the line follows.
 *
 * The line leaves from the middle of `side`, by default the edge facing the
 * target, and runs `'straight'` to it or as an `'elbow'`: out of the edge,
 * then one turn. When the target is inside the box, no line is drawn. The
 * line is drawn under the box, so the box's border covers its end.
 *
 * ```tsx
 * <Callout target={{ x: 960, y: 540 }} x={1200} y={240} width={480}
 *   title="ここに注目" text="対象の特徴を説明します"
 *   fill="#182030" color="#F4F1EA" lineColor="#FF4D1F" />
 * ```
 */
export function Callout(props: CalloutProps): ReturnType<typeof React.createElement> {
  const {
    target, x, y, anchorX = 0, anchorY = 0, width, title, text, titleStyle = DEFAULT_TITLE_STYLE, textStyle = DEFAULT_TEXT_STYLE, color, gap = 8,
    padding = 20, cornerRadius = 12, side = 'auto', route: shape = 'straight', lineColor = '#FFFFFF', lineWidth = 2,
    dash, cap = 'round', head = 'arrow', headLength, headWidth, lineProgress = 1,
    fill, stroke, strokeWidth, shadow, ...group
  } = props;
  const theme = useTheme();
  if (typeof target !== 'object' || target === null) fail('target must be a point { x, y }');
  finite('target.x', target.x);
  finite('target.y', target.y);
  finite('x', x);
  finite('y', y);
  finite('anchorX', anchorX);
  finite('anchorY', anchorY);
  nonNegative('width', width);
  nonNegative('gap', gap);
  if (finite('lineWidth', lineWidth) <= 0) fail(`lineWidth must be more than 0, not ${lineWidth}`);
  if (finite('lineProgress', lineProgress) < 0 || lineProgress > 1) fail(`lineProgress must be from 0 to 1, not ${lineProgress}`);
  if (!['auto', 'top', 'right', 'bottom', 'left'].includes(side)) {
    fail(`side must be 'auto', 'top', 'right', 'bottom', or 'left', not ${JSON.stringify(side)}`);
  }
  if (shape !== 'straight' && shape !== 'elbow') fail(`route must be 'straight' or 'elbow', not ${JSON.stringify(shape)}`);
  if (head !== 'arrow' && head !== 'none') fail(`head must be 'arrow' or 'none', not ${JSON.stringify(head)}`);
  const { top, right, bottom, left } = insets(padding);
  const line = hexColor(lineColor, theme, 'Callout', 'lineColor');

  // The title and the text measured and drawn with the same style, wrapped
  // to the room inside the padding.
  const room = Math.max(0, width - left - right);
  const withGroup = (style: TextStyle): TextStyle => ({
    ...style,
    ...(group.lang !== undefined && style.lang == null ? { lang: group.lang } : {}),
    ...(group.fontFallback !== undefined && style.fontFallback == null ? { fontFallback: group.fontFallback } : {}),
  });
  const titled = withGroup(titleStyle);
  const texted = withGroup(textStyle);
  const titleMetrics = useTextMetrics(title ?? '', titled, { maxWidth: room });
  const textMetrics = useTextMetrics(text ?? '', texted, { maxWidth: room });
  const lines: React.ReactElement[] = [];
  let cursor = 0;
  if (title != null) {
    lines.push(React.createElement(Text, {
      key: 'title', y: titleMetrics.ascent, anchorY: 'baseline', maxWidth: room, style: titled, color, children: title,
    }));
    cursor += titleMetrics.height;
  }
  if (text != null) {
    if (title != null) cursor += gap;
    lines.push(React.createElement(Text, {
      key: 'text', y: cursor + textMetrics.ascent, anchorY: 'baseline', maxWidth: room, style: texted, color, children: text,
    }));
    cursor += textMetrics.height;
  }
  const height = top + cursor + bottom;

  const boxLeft = x - anchorX * width;
  const boxTop = y - anchorY * height;
  const box = { left: boxLeft, top: boxTop, right: boxLeft + width, bottom: boxTop + height };
  const edge = side === 'auto' ? facingSide(box, target) : facingSide(box, target) === undefined ? undefined : side;
  const connector: React.ReactElement[] = [];
  if (edge !== undefined && lineProgress > 0) {
    const points = fromEnd(route(box, edge, target, shape), lineProgress);
    const stroked = { stroke: line, strokeWidth: lineWidth, dash };
    if (head === 'arrow' && points.length >= 2) {
      const [x1, y1] = points[points.length - 2]!;
      if (points.length > 2) {
        connector.push(React.createElement(Polyline, {
          key: 'line', points: points.slice(0, -1), ...stroked, cap, join: 'round',
        }));
      }
      connector.push(React.createElement(Arrow, {
        key: 'arrow', x1, y1, x2: target.x, y2: target.y, ...stroked, headLength, headWidth,
      }));
    } else if (points.length >= 2) {
      connector.push(React.createElement(Polyline, { key: 'line', points, ...stroked, cap, join: 'round' }));
    }
  }

  return React.createElement(Group, group,
    ...connector,
    React.createElement(Panel, {
      key: 'box', x: boxLeft, y: boxTop, width, height, padding: { top, right, bottom, left }, cornerRadius, fill, stroke, strokeWidth, shadow,
    }, lines));
}
