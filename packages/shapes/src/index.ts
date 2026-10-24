import * as React from 'react';

import { Group, Rect } from '@celesta/react';
import {
  ThemeContext,
  color,
  markPrimitive,
  num,
  propertyTable,
  registerPrimitiveProperties,
  required,
  themedShapePaint,
} from '@celesta/react/internal';
import type { CommonProps, LineCap, LineJoin, Paint, PathCommand } from '@celesta/react';

import { trimCommands, trimStartIntoSubpath } from './path-geometry';

export type { LineCap, LineJoin, PathCommand };
export {
  getLength,
  getPointAtLength,
  getTangentAtLength,
  measurePath,
  pointOnPath,
  tangentOnPath,
  trimCommands as trimPath,
} from './path-geometry';
export type { PathMeasure, PathPoint, PathTangent } from './path-geometry';
export { interpolatePath } from './path-morph';
export { parseSvgPath, toSvgPath } from './path-svg';

/** A point as `[x, y]`, in the path's own coordinates. */
export type PolylinePoint = readonly [number, number];

/**
 * Transform and compositing props of a path. There is no `anchorX`/`anchorY`:
 * a path's coordinates already have their own origin, which `x`/`y` place
 * and `scale`/`rotation` turn about.
 */
type PathLayerProps = Omit<CommonProps, 'anchorX' | 'anchorY'>;

interface StrokeProps extends PathLayerProps {
  /**
   * Hex color (`"#RRGGBB"` or `"#RRGGBBAA"`), `<Theme>` color name, or a
   * gradient `Paint` of the stroke, whose stop colors may be names too.
   */
  stroke?: string | Paint;
  /** Thickness in pixels. Defaults to 2. */
  strokeWidth?: number;
  /** How open ends finish: `'butt'` ends exactly at the end point, `'round'` adds a half disc, `'square'` half a square. */
  cap?: LineCap;
  /** How corners are drawn: `'miter'` (sharp), `'round'`, or `'bevel'` (cut off). */
  join?: LineJoin;
  /**
   * The longest a miter join may be, as a multiple of `strokeWidth`, before
   * it is drawn as a bevel instead. Defaults to 4, like SVG.
   */
  miterLimit?: number;
  /**
   * Breaks the stroke into dashes: lengths of the dashes and the gaps
   * between them, in turn, in pixels, starting over at each `moveTo`, like
   * SVG's `stroke-dasharray`. An odd count repeats once more. With `cap`
   * `'round'`, `[0, gap]` draws dots. The pattern is anchored at the start
   * of the path, so it stays put while `progress` or `progressStart` trims
   * the line.
   */
  dash?: readonly number[];
  /** How far into `dash` the stroke starts; animate it to make the dashes flow. */
  dashOffset?: number;
}

/** The dash props of a closed shape, whose dashes run clockwise from a point it names. */
interface ShapeDashProps {
  /** As for `<Path>`, from the point the shape's docs name, clockwise. */
  dash?: readonly number[];
  /** How far into `dash` the stroke starts; animate it to make the dashes flow. */
  dashOffset?: number;
  /** How each dash ends: `'butt'` (the default), `'round'`, or `'square'`. Only matters with `dash`. */
  cap?: LineCap;
}

function isDashed(dash: readonly number[] | undefined): dash is readonly number[] {
  return dash !== undefined && dash.length > 0;
}

export interface PathProps extends StrokeProps {
  /**
   * Absolute path commands, like SVG's `M`/`L`/`Q`/`C`/`Z`:
   * `{ type: 'moveTo', x, y }`, `{ type: 'lineTo', x, y }`,
   * `{ type: 'quadTo', x1, y1, x, y }`,
   * `{ type: 'cubicTo', x1, y1, x2, y2, x, y }`, and `{ type: 'close' }`.
   * Use `points` instead for straight segments.
   */
  commands?: readonly PathCommand[];
  /** Straight segments through these points; ignored when `commands` is set. */
  points?: readonly PolylinePoint[];
  /** Joins the last of `points` back to the first, without a seam. */
  closed?: boolean;
  /**
   * Fills the inside of the path (non-zero rule), as a hex color, a
   * `<Theme>` color name, or a `Paint` in the path's own coordinates. Omit
   * for no fill.
   */
  fill?: string | Paint;
  /**
   * Where the stroke ends, 0–1 of the path's length (every subpath in turn,
   * curves measured along their length). Animate it for a path that draws
   * itself on; `pointOnPath(commands, progress)` is its tip. Defaults to 1.
   */
  progress?: number;
  /**
   * Where the stroke starts, 0–1 of the path's length. Defaults to 0. A
   * closed subpath drawn only in part ends open, with caps. A `fill` is
   * always the whole shape: with `progress < 1` or `progressStart > 0` it is
   * drawn as a separate layer under the stroke, in a `<Group>` that takes
   * the transform, `opacity`, and effects. So is a dashed stroke whose start
   * is partway along a subpath with more subpaths after it: that first part
   * is a layer of its own, to keep its dashes in place.
   */
  progressStart?: number;
}

export interface LineProps extends StrokeProps {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

export interface PolylineProps extends StrokeProps {
  points: readonly PolylinePoint[];
  /** Joins the last point back to the first, without a seam. */
  closed?: boolean;
  /**
   * How much of the path to draw, 0–1 of its length from the first point.
   * Animate it for a line that draws itself on. Defaults to 1.
   */
  progress?: number;
  /** Where the drawn part starts, 0–1 of the length. Defaults to 0. */
  progressStart?: number;
}

function strokeWidthOf(value: number | undefined, component: string): number {
  const width = value ?? 2;
  if (!Number.isFinite(width) || width <= 0) {
    throw new Error(`<${component}> requires a positive \`strokeWidth\``);
  }
  return width;
}

/** `moveTo` the first point and `lineTo` each of the others. */
function commandsThrough(points: readonly PolylinePoint[], closed: boolean): PathCommand[] {
  const commands: PathCommand[] = points.map(([x, y], i) =>
    i === 0 ? { type: 'moveTo', x, y } : { type: 'lineTo', x, y },
  );
  if (closed && commands.length > 0) {
    commands.push({ type: 'close' });
  }
  return commands;
}

/**
 * A vector shape: lines and Bézier curves, stroked and/or filled as one layer
 * however many segments it has. Overlapping parts of a translucent stroke
 * are painted once, and the shape stays sharp at any `scale`. Coordinates
 * are in the path's own pixels; `x`/`y` move their origin.
 */
export function Path({
  commands,
  points,
  closed = false,
  stroke,
  strokeWidth,
  progress = 1,
  progressStart = 0,
  ...props
}: PathProps): ReturnType<typeof React.createElement> {
  if (commands === undefined && points === undefined) {
    throw new Error('<Path> requires `commands` or `points`');
  }
  for (const [name, value] of [['progress', progress], ['progressStart', progressStart]] as const) {
    if (Number.isNaN(value)) {
      throw new Error(`<Path> requires a number for \`${name}\`, got NaN`);
    }
  }
  const theme = React.useContext(ThemeContext);
  const themedFill = themedShapePaint(props.fill, theme, '<Path>', 'fill');
  if (themedFill !== props.fill) props = { ...props, fill: themedFill };
  stroke = themedShapePaint(stroke, theme, '<Path>', 'stroke');
  const whole = commands ?? commandsThrough(points ?? [], closed);
  const strokeStyle = {
    stroke,
    ...(stroke === undefined ? {} : { strokeWidth: strokeWidthOf(strokeWidth, 'Path') }),
  };
  // `progress` trims only the stroke; a fill alone is always whole.
  const trimmed = stroke === undefined ? whole : trimCommands(whole, progressStart, progress);
  if (trimmed === whole) {
    return React.createElement('path', { ...props, commands: whole, ...strokeStyle });
  }
  const { fill, cap, join, miterLimit, dash, dashOffset, ...layer } = props;
  const strokes = dashedFromStart(whole, trimmed, progressStart, dash, dashOffset);
  const lineStyle = { cap, join, miterLimit, dash, ...strokeStyle };
  if (fill === undefined && strokes.length === 1) {
    return React.createElement('path', { ...layer, ...lineStyle, ...strokes[0] });
  }
  // The fill keeps the whole shape, so it and the trimmed stroke are separate layers.
  return React.createElement(
    Group,
    layer,
    ...(fill === undefined ? [] : [React.createElement('path', { key: 'fill', commands: whole, fill })]),
    ...strokes.map((part, i) => React.createElement('path', { key: i, ...lineStyle, ...part })),
  );
}

/**
 * The trimmed stroke's commands and dash offsets, so each dash falls where
 * it does on the whole path. A dash pattern starts over at each `moveTo`, so
 * only a first subpath that starts partway along needs its offset moved on;
 * with later subpaths after it, it is drawn as a layer of its own.
 */
function dashedFromStart(
  whole: readonly PathCommand[],
  trimmed: readonly PathCommand[],
  start: number,
  dash: readonly number[] | undefined,
  dashOffset: number | undefined,
): Array<{ commands: readonly PathCommand[]; dashOffset?: number }> {
  const into = isDashed(dash) && start > 0 ? trimStartIntoSubpath(whole, start) : 0;
  if (into < 1e-9) {
    return [{ commands: trimmed, dashOffset }];
  }
  const moved = (dashOffset ?? 0) + into;
  const next = trimmed.findIndex((command, i) => i > 0 && command.type === 'moveTo');
  if (next < 0) {
    return [{ commands: trimmed, dashOffset: moved }];
  }
  return [
    { commands: trimmed.slice(0, next), dashOffset: moved },
    { commands: trimmed.slice(next), dashOffset },
  ];
}

/** A straight line segment between two points. */
export function Line({
  x1,
  y1,
  x2,
  y2,
  stroke = '#FFFFFF',
  strokeWidth,
  cap = 'round',
  ...props
}: LineProps): ReturnType<typeof React.createElement> | null {
  const width = strokeWidthOf(strokeWidth, 'Line');
  if (cap === 'butt' && x1 === x2 && y1 === y2) {
    return null;
  }
  return React.createElement(Path, {
    ...props,
    points: [
      [x1, y1],
      [x2, y2],
    ],
    stroke,
    strokeWidth: width,
    cap,
  });
}

function segmentLengths(points: readonly PolylinePoint[]): number[] {
  return points.slice(1).map(([x, y], i) => Math.hypot(x - points[i][0], y - points[i][1]));
}

/**
 * The point `t` (0–1) of the way along a polyline, measured by length. Use it
 * to put a marker or a label on the tip of a `<Polyline>` that is drawing on.
 */
export function pointOnPolyline(points: readonly PolylinePoint[], t: number): [number, number] {
  if (points.length === 0) {
    throw new Error('pointOnPolyline() requires at least one point');
  }
  const lengths = segmentLengths(points);
  let remaining = lengths.reduce((sum, n) => sum + n, 0) * Math.min(1, Math.max(0, t));
  for (let i = 0; i < lengths.length; i += 1) {
    if (remaining <= lengths[i]) {
      const u = lengths[i] === 0 ? 0 : remaining / lengths[i];
      const [x1, y1] = points[i];
      const [x2, y2] = points[i + 1];
      return [x1 + (x2 - x1) * u, y1 + (y2 - y1) * u];
    }
    remaining -= lengths[i];
  }
  const [x, y] = points[points.length - 1];
  return [x, y];
}

/**
 * Connected line segments through `points`, such as a line chart or a route,
 * drawn as one `<Path>`. With round caps (the default) the corners are
 * rounded too, unless `join` says otherwise.
 */
export function Polyline({
  points,
  closed = false,
  stroke = '#FFFFFF',
  strokeWidth,
  cap = 'round',
  join,
  ...props
}: PolylineProps): ReturnType<typeof React.createElement> {
  const width = strokeWidthOf(strokeWidth, 'Polyline');
  return React.createElement(Path, {
    ...props,
    points,
    closed,
    stroke,
    strokeWidth: width,
    cap,
    join: join ?? (cap === 'round' ? 'round' : 'miter'),
  });
}

export interface EllipseProps extends CommonProps, ShapeDashProps {
  /** Width of the ellipse's bounding box, in pixels. 0 draws nothing. */
  width: number;
  /** Height of the ellipse's bounding box, in pixels. 0 draws nothing. */
  height: number;
  /**
   * Hex color, `<Theme>` color name, or a gradient `Paint` in the shape's
   * local pixels (origin at its bounding box's top-left, as for `<Rect>`).
   * Omit for no fill.
   */
  fill?: string | Paint;
  /** Hex color, `<Theme>` color name, or `Paint` of a stroke drawn inside the outline, as for `<Rect>`. */
  stroke?: string | Paint;
  /** Thickness in pixels, at most half the shorter side. Defaults to 2. */
  strokeWidth?: number;
}

export interface CircleProps extends Omit<EllipseProps, 'width' | 'height'> {
  /** Radius in pixels; the bounding box is `2 * radius` square. 0 draws nothing. */
  radius: number;
}

export interface ArrowProps extends PathLayerProps {
  /** Tail point. */
  x1: number;
  y1: number;
  /** Tip point, where the head points. */
  x2: number;
  y2: number;
  /**
   * Hex color, `<Theme>` color name, or a gradient `Paint` (in the arrow's
   * coordinates) of the whole arrow. Defaults to white.
   */
  stroke?: string | Paint;
  /** Thickness of the shaft in pixels. Defaults to 2. */
  strokeWidth?: number;
  /** Length of each head along the arrow, in pixels. Defaults to 4 × `strokeWidth`. */
  headLength?: number;
  /**
   * Width of each head across the arrow, in pixels, at least `strokeWidth`.
   * Defaults to 4 × `strokeWidth`.
   */
  headWidth?: number;
  /** Which ends get a head: `'end'` (the default, at `x2`/`y2`), `'start'`, or `'both'`. */
  heads?: 'end' | 'start' | 'both';
  /**
   * Breaks the shaft into flat-ended dashes, measured from the tail
   * (`x1`/`y1`) as for a `<Line>` between the same points; the heads stay
   * solid. An odd count repeats once more.
   */
  dash?: readonly number[];
  /** How far into `dash` the shaft starts; animate it to make the dashes flow. */
  dashOffset?: number;
}

function dimensionOf(value: number, name: string, component: string): number {
  if (!Number.isFinite(value) || value < 0) {
    throw new Error(`<${component}> requires a finite \`${name}\` of at least 0, got ${String(value)}`);
  }
  return value;
}

/** Eight cubic arcs, each within 0.0004% of the radius of a true ellipse. */
function ellipseCommands(cx: number, cy: number, rx: number, ry: number): PathCommand[] {
  const k = (4 / 3) * Math.tan(Math.PI / 16);
  const at = (i: number) => [Math.cos((i * Math.PI) / 4), Math.sin((i * Math.PI) / 4)];
  const commands: PathCommand[] = [{ type: 'moveTo', x: cx + rx, y: cy }];
  for (let i = 0; i < 8; i += 1) {
    const [c0, s0] = at(i);
    const [c1, s1] = at(i + 1);
    commands.push({
      type: 'cubicTo',
      x1: cx + rx * (c0 - k * s0),
      y1: cy + ry * (s0 + k * c0),
      x2: cx + rx * (c1 + k * s1),
      y2: cy + ry * (s1 - k * c1),
      x: cx + rx * c1,
      y: cy + ry * s1,
    });
  }
  commands.push({ type: 'close' });
  return commands;
}

/** `paint` with its gradient points moved by `dx`/`dy`. */
function offsetPaint(paint: string | Paint | undefined, dx: number, dy: number): string | Paint | undefined {
  if (paint === undefined || typeof paint === 'string' || paint.type === 'solid') {
    return paint;
  }
  const move = ({ x, y }: { x: number; y: number }) => ({ x: x + dx, y: y + dy });
  return paint.type === 'linear'
    ? { ...paint, start: move(paint.start), end: move(paint.end) }
    : { ...paint, center: move(paint.center) };
}

/**
 * An ellipse filling a `width` × `height` box, drawn as one `<Path>`. Like
 * `<Rect>`, `x`/`y` place the box's `anchorX`/`anchorY` point (its top-left
 * by default), and the stroke stays inside the box. Its `dash` runs
 * clockwise from its rightmost point, as in SVG; a dashed ellipse with a
 * fill is drawn as a fill and a stroke path in a `<Group>`, so the fill
 * reaches the box's edge between the dashes and shows through a
 * translucent dash.
 */
export function Ellipse({
  width,
  height,
  fill,
  stroke,
  strokeWidth,
  anchorX = 0,
  anchorY = 0,
  dash,
  dashOffset,
  cap,
  ...props
}: EllipseProps): ReturnType<typeof React.createElement> | null {
  dimensionOf(width, 'width', 'Ellipse');
  dimensionOf(height, 'height', 'Ellipse');
  const strokeWidthDrawn = stroke === undefined
    ? 0
    : Math.min(strokeWidthOf(strokeWidth, 'Ellipse'), Math.min(width, height) / 2);
  if (width === 0 || height === 0) {
    return null;
  }
  // The path has no anchor, so its origin is the anchor point.
  const left = -anchorX * width;
  const top = -anchorY * height;
  // The stroke is centred on the outline, so inset it by half its width.
  const inset = strokeWidthDrawn / 2;
  const outline = (by: number) =>
    ellipseCommands(left + width / 2, top + height / 2, width / 2 - by, height / 2 - by);
  const dashes = { cap, dash, dashOffset };
  if (fill === undefined || stroke === undefined || !isDashed(dash)) {
    return React.createElement(Path, {
      ...props,
      ...dashes,
      commands: outline(inset),
      fill: offsetPaint(fill, left, top),
      stroke: offsetPaint(stroke, left, top),
      ...(stroke === undefined ? {} : { strokeWidth: strokeWidthDrawn }),
    });
  }
  return React.createElement(
    Group,
    props,
    React.createElement(Path, { commands: outline(0), fill: offsetPaint(fill, left, top) }),
    React.createElement(Path, {
      ...dashes,
      commands: outline(inset),
      stroke: offsetPaint(stroke, left, top),
      strokeWidth: strokeWidthDrawn,
    }),
  );
}

/**
 * A circle of `radius`, placed like an `<Ellipse>` `2 * radius` square but
 * drawn as a `<Rect>` with `cornerRadius` `radius`: shaded directly on the
 * GPU with no outline to flatten, so many animated circles are much cheaper
 * than paths. A dashed stroke makes it an `<Ellipse>`, whose dashes run
 * clockwise from its rightmost point.
 */
export function Circle({
  radius,
  stroke,
  strokeWidth,
  ...props
}: CircleProps): ReturnType<typeof React.createElement> | null {
  dimensionOf(radius, 'radius', 'Circle');
  // A rect with a non-finite size draws nothing, so reject one up front.
  if (!Number.isFinite(radius * 2)) {
    throw new Error(`<Circle> requires a \`radius\` whose diameter is finite, got ${String(radius)}`);
  }
  const strokeWidthDrawn = stroke === undefined ? 0 : Math.min(strokeWidthOf(strokeWidth, 'Circle'), radius);
  if (radius === 0) {
    return null;
  }
  if (stroke !== undefined && isDashed(props.dash)) {
    return React.createElement(Ellipse, { ...props, width: radius * 2, height: radius * 2, stroke, strokeWidth });
  }
  return React.createElement(Rect, {
    ...props,
    width: radius * 2,
    height: radius * 2,
    cornerRadius: radius,
    stroke,
    ...(stroke === undefined ? {} : { strokeWidth: strokeWidthDrawn }),
  });
}

/** Above this many dashes, a dashed `<Arrow>` is refused, as the renderers refuse a stroke. */
const MAX_DASHES = 1_000_000;

/**
 * The stretches `[start, end]` of `from`–`to` along a line that `dash` and
 * `offset` leave drawn, with flat ends, measured from the line's start as a
 * stroke's dashes are; `null` when the pattern draws a solid line.
 */
function dashedStretches(
  dash: readonly number[],
  offset: number,
  from: number,
  to: number,
): [number, number][] | null {
  if (!dash.every((length) => Number.isFinite(length) && length >= 0)) {
    throw new Error('<Arrow> dash must be an array of finite numbers of at least 0');
  }
  if (!Number.isFinite(offset)) {
    throw new Error('<Arrow> dashOffset must be a finite number');
  }
  const lengths = dash.length % 2 === 1 ? [...dash, ...dash] : dash;
  const period = lengths.reduce((sum, length) => sum + length, 0);
  if (!(period > 0)) {
    return null;
  }
  if (((to - from) / period) * (lengths.length / 2) > MAX_DASHES) {
    throw new Error(`<Arrow> dash would draw more than ${MAX_DASHES} dashes`);
  }
  // The pattern starts `offset` before the tail; begin at its last start
  // before `from`.
  let distance = -(((offset % period) + period) % period);
  distance += Math.floor((from - distance) / period) * period;
  const stretches: [number, number][] = [];
  for (let i = 0; distance < to; i = (i + 1) % lengths.length) {
    const end = distance + lengths[i];
    if (i % 2 === 0 && Math.min(end, to) > Math.max(distance, from)) {
      stretches.push([Math.max(distance, from), Math.min(end, to)]);
    }
    distance = end;
  }
  return stretches;
}

/**
 * A straight arrow from `x1`/`y1` to `x2`/`y2`, filled as one `<Path>`
 * outline, so a translucent or gradient arrow has no seam between shaft and
 * head. The shaft ends flat, and each head's tip is exactly at its end
 * point. An arrow shorter than its heads scales them down to fit, keeping
 * their shape, and one with no length draws nothing.
 */
export function Arrow({
  x1,
  y1,
  x2,
  y2,
  stroke = '#FFFFFF',
  strokeWidth,
  headLength,
  headWidth,
  heads = 'end',
  dash,
  dashOffset,
  ...props
}: ArrowProps): ReturnType<typeof React.createElement> | null {
  for (const [name, value] of [['x1', x1], ['y1', y1], ['x2', x2], ['y2', y2]] as const) {
    if (!Number.isFinite(value)) {
      throw new Error(`<Arrow> requires a finite \`${name}\`, got ${String(value)}`);
    }
  }
  const width = strokeWidthOf(strokeWidth, 'Arrow');
  const length = headLength ?? width * 4;
  const spread = headWidth ?? width * 4;
  for (const [name, value] of [['headLength', length], ['headWidth', spread]] as const) {
    if (!Number.isFinite(value) || value <= 0) {
      throw new Error(`<Arrow> requires a positive \`${name}\`, got ${String(value)}`);
    }
  }
  if (heads !== 'end' && heads !== 'start' && heads !== 'both') {
    throw new Error(`<Arrow> heads must be 'end', 'start', or 'both', not ${JSON.stringify(heads)}`);
  }
  const total = Math.hypot(x2 - x1, y2 - y1);
  if (total === 0) {
    return null;
  }
  const atStart = heads !== 'end';
  const atEnd = heads !== 'start';
  const fit = Math.min(1, total / (length * (atStart && atEnd ? 2 : 1)));
  const head = length * fit;
  const shaft = width / 2;
  const wing = Math.max(spread * fit, width) / 2;
  // `s` runs along the arrow from the tail, `t` across it.
  const ux = (x2 - x1) / total;
  const uy = (y2 - y1) / total;
  const point = (s: number, t: number): PolylinePoint => [x1 + ux * s - uy * t, y1 + uy * s + ux * t];
  const back = atStart ? head : 0;
  const front = atEnd ? total - head : total;
  const stretches = dash === undefined ? null : dashedStretches(dash, dashOffset ?? 0, back, front);
  if (stretches !== null) {
    // The heads and each dash of the shaft as outlines of their own, all
    // turning the same way, filled as one.
    const outlines: PolylinePoint[][] = [
      ...(atStart ? [[point(0, 0), point(back, -wing), point(back, wing)]] : []),
      ...stretches.map(([from, to]) => [point(from, -shaft), point(to, -shaft), point(to, shaft), point(from, shaft)]),
      ...(atEnd ? [[point(front, -wing), point(total, 0), point(front, wing)]] : []),
    ];
    return React.createElement(Path, {
      ...props,
      commands: outlines.flatMap((points) => commandsThrough(points, true)),
      fill: stroke,
    });
  }
  const outline: PolylinePoint[] = [
    ...(atStart ? [point(0, 0), point(back, -wing)] : []),
    point(back, -shaft),
    point(front, -shaft),
    ...(atEnd ? [point(front, -wing), point(total, 0), point(front, wing)] : []),
    point(front, shaft),
    point(back, shaft),
    ...(atStart ? [point(back, wing)] : []),
  ];
  return React.createElement(Path, { ...props, points: outline, closed: true, fill: stroke });
}

export interface TrianglePathOptions {
  /** Width of the triangle's box, in pixels. 0 draws nothing. */
  width: number;
  /** Height of the triangle's box, in pixels. 0 draws nothing. */
  height: number;
  /**
   * Where the top corner is along the box's top edge, 0–1 of its width: 0.5
   * (the default) is isosceles, 0 a right angle on the left.
   */
  apex?: number;
  /** Rounds each corner to an arc of this radius, in pixels. Defaults to 0. */
  cornerRadius?: number;
}

export interface PolygonPathOptions {
  /** Radius of the circle the corners lie on; the box is `2 * radius` square. 0 draws nothing. */
  radius: number;
  /** Number of sides, a whole number of at least 3. */
  sides: number;
  /** Rounds each corner to an arc of this radius, in pixels. Defaults to 0. */
  cornerRadius?: number;
}

export interface StarPathOptions {
  /** Radius of the circle the tips lie on; the box is `2 * radius` square. 0 draws nothing. */
  radius: number;
  /** Number of tips, a whole number of at least 3. */
  points: number;
  /** Radius of the inner corners as a fraction (0–1) of `radius`. Defaults to 0.5. */
  innerRatio?: number;
  /** Rounds each tip and inner corner to an arc of this radius, in pixels. Defaults to 0. */
  cornerRadius?: number;
}

export interface PiePathOptions {
  /** Radius of the circle; the box is `2 * radius` square. 0 draws nothing. */
  radius: number;
  /** Angle the slice starts at, in degrees clockwise from 12 o'clock. Defaults to 0. */
  startAngle?: number;
  /**
   * Angle the slice ends at, in degrees clockwise from 12 o'clock. Below
   * `startAngle` it runs counter-clockwise; the same angle draws nothing, and
   * 360° or more apart a whole circle.
   */
  endAngle: number;
  /** Rounds the slice's three corners to arcs of this radius, in pixels. Defaults to 0. */
  cornerRadius?: number;
}

export interface HeartPathOptions {
  /** Width of the heart's box, in pixels. 0 draws nothing. */
  width: number;
  /** Height of the heart's box, in pixels. 0 draws nothing. */
  height: number;
}

type Vec = readonly [number, number];

function finiteOf(value: number, name: string, who: string): number {
  if (!Number.isFinite(value)) {
    throw new Error(`${who} requires a finite \`${name}\`, got ${String(value)}`);
  }
  return value;
}

function sizeOf(value: number, name: string, who: string): number {
  if (!Number.isFinite(value) || value < 0) {
    throw new Error(`${who} requires a finite \`${name}\` of at least 0, got ${String(value)}`);
  }
  return value;
}

function fractionOf(value: number, name: string, who: string): number {
  if (!Number.isFinite(value) || value < 0 || value > 1) {
    throw new Error(`${who} requires \`${name}\` to be from 0 to 1, got ${String(value)}`);
  }
  return value;
}

function countOf(value: number, name: string, who: string): number {
  if (!Number.isInteger(value) || value < 3) {
    throw new Error(`${who} requires a whole \`${name}\` of at least 3, got ${String(value)}`);
  }
  return value;
}

/**
 * Appends cubic arcs of the ellipse centred on `cx`/`cy`, from angle `from`
 * (radians from +x, clockwise on screen) turning by `sweep`, each at most a
 * quarter turn. The current point must already be the arc's start.
 */
function arcTo(
  commands: PathCommand[],
  cx: number,
  cy: number,
  rx: number,
  ry: number,
  from: number,
  sweep: number,
): void {
  const count = Math.max(1, Math.ceil(Math.abs(sweep) / (Math.PI / 2) - 1e-9));
  const step = sweep / count;
  const k = (4 / 3) * Math.tan(step / 4);
  for (let i = 0; i < count; i += 1) {
    const c0 = Math.cos(from + step * i);
    const s0 = Math.sin(from + step * i);
    const c1 = Math.cos(from + step * (i + 1));
    const s1 = Math.sin(from + step * (i + 1));
    commands.push({
      type: 'cubicTo',
      x1: cx + rx * (c0 - k * s0),
      y1: cy + ry * (s0 + k * c0),
      x2: cx + rx * (c1 + k * s1),
      y2: cy + ry * (s1 - k * c1),
      x: cx + rx * c1,
      y: cy + ry * s1,
    });
  }
}

/** Appends the shorter arc of the circle about `center` from `start` to `end`, both on it. */
function arcBetween(commands: PathCommand[], center: Vec, radius: number, start: Vec, end: Vec): void {
  const from = Math.atan2(start[1] - center[1], start[0] - center[0]);
  const to = Math.atan2(end[1] - center[1], end[0] - center[0]);
  const sweep = to - from - Math.PI * 2 * Math.round((to - from) / (Math.PI * 2));
  arcTo(commands, center[0], center[1], radius, radius, from, sweep);
}

interface Corner {
  /** Where the arc leaves the side coming in. */
  start: Vec;
  /** Where the arc joins the side going out. */
  end: Vec;
  center: Vec;
  radius: number;
}

/**
 * The arc of `radius` tangent to both sides of the corner at `vertex`
 * between `previous` and `next`, touching each side at most `reach` from
 * the vertex (a smaller radius where it would reach further). `undefined`
 * where there is nothing to round.
 */
function cornerAt(vertex: Vec, previous: Vec, next: Vec, radius: number, reach: number): Corner | undefined {
  const ax = previous[0] - vertex[0];
  const ay = previous[1] - vertex[1];
  const bx = next[0] - vertex[0];
  const by = next[1] - vertex[1];
  const la = Math.hypot(ax, ay);
  const lb = Math.hypot(bx, by);
  if (radius <= 0 || la === 0 || lb === 0) {
    return undefined;
  }
  // Half the angle between the two sides.
  const half = Math.acos(Math.min(1, Math.max(-1, (ax * bx + ay * by) / (la * lb)))) / 2;
  const tangent = Math.tan(half);
  if (!(tangent > 0 && tangent < 1e9)) {
    return undefined;
  }
  const distance = Math.min(radius / tangent, reach);
  const fitted = distance * tangent;
  if (fitted <= 0) {
    return undefined;
  }
  const ux = ax / la + bx / lb;
  const uy = ay / la + by / lb;
  const toCenter = fitted / Math.sin(half) / Math.hypot(ux, uy);
  return {
    start: [vertex[0] + (ax / la) * distance, vertex[1] + (ay / la) * distance],
    end: [vertex[0] + (bx / lb) * distance, vertex[1] + (by / lb) * distance],
    center: [vertex[0] + ux * toCenter, vertex[1] + uy * toCenter],
    radius: fitted,
  };
}

/**
 * A closed outline through `vertices`, starting at the first, with each
 * corner rounded to an arc of `cornerRadius` tangent to both its sides. An
 * arc reaches at most halfway along either side, so a short side gets a
 * smaller one.
 */
function roundedOutline(vertices: readonly Vec[], cornerRadius: number): PathCommand[] {
  const commands: PathCommand[] = [];
  const n = vertices.length;
  vertices.forEach((vertex, i) => {
    const previous = vertices[(i + n - 1) % n];
    const next = vertices[(i + 1) % n];
    const reach = Math.min(
      Math.hypot(previous[0] - vertex[0], previous[1] - vertex[1]),
      Math.hypot(next[0] - vertex[0], next[1] - vertex[1]),
    ) / 2;
    const corner = cornerAt(vertex, previous, next, cornerRadius, reach);
    const [x, y] = corner?.start ?? vertex;
    commands.push(i === 0 ? { type: 'moveTo', x, y } : { type: 'lineTo', x, y });
    if (corner !== undefined) {
      arcBetween(commands, corner.center, corner.radius, corner.start, corner.end);
    }
  });
  commands.push({ type: 'close' });
  return commands;
}

/** The point `distance` from `center` at `degrees` clockwise from 12 o'clock. */
function polar(center: Vec, distance: number, degrees: number): Vec {
  const radians = (degrees * Math.PI) / 180;
  return [center[0] + distance * Math.sin(radians), center[1] - distance * Math.cos(radians)];
}

function triangleCommands(options: TrianglePathOptions, who: string): PathCommand[] {
  const width = sizeOf(options.width, 'width', who);
  const height = sizeOf(options.height, 'height', who);
  const apex = fractionOf(options.apex ?? 0.5, 'apex', who);
  const cornerRadius = sizeOf(options.cornerRadius ?? 0, 'cornerRadius', who);
  if (width === 0 || height === 0) {
    return [];
  }
  return roundedOutline([[apex * width, 0], [width, height], [0, height]], cornerRadius);
}

function polygonCommands(options: PolygonPathOptions, who: string): PathCommand[] {
  const radius = sizeOf(options.radius, 'radius', who);
  const sides = countOf(options.sides, 'sides', who);
  const cornerRadius = sizeOf(options.cornerRadius ?? 0, 'cornerRadius', who);
  if (radius === 0) {
    return [];
  }
  const center: Vec = [radius, radius];
  const vertices = Array.from({ length: sides }, (_, i) => polar(center, radius, (360 * i) / sides));
  return roundedOutline(vertices, cornerRadius);
}

function starCommands(options: StarPathOptions, who: string): PathCommand[] {
  const radius = sizeOf(options.radius, 'radius', who);
  const points = countOf(options.points, 'points', who);
  const innerRatio = fractionOf(options.innerRatio ?? 0.5, 'innerRatio', who);
  const cornerRadius = sizeOf(options.cornerRadius ?? 0, 'cornerRadius', who);
  if (radius === 0) {
    return [];
  }
  const center: Vec = [radius, radius];
  const vertices = Array.from({ length: points * 2 }, (_, i) =>
    polar(center, i % 2 === 0 ? radius : radius * innerRatio, (180 * i) / points));
  return roundedOutline(vertices, cornerRadius);
}

function pieCommands(options: PiePathOptions, who: string): PathCommand[] {
  const radius = sizeOf(options.radius, 'radius', who);
  const startAngle = finiteOf(options.startAngle ?? 0, 'startAngle', who);
  const endAngle = finiteOf(options.endAngle, 'endAngle', who);
  const cornerRadius = sizeOf(options.cornerRadius ?? 0, 'cornerRadius', who);
  const degrees = endAngle - startAngle;
  if (radius === 0 || degrees === 0) {
    return [];
  }
  const center: Vec = [radius, radius];
  // `arcTo` measures from +x; these angles are from 12 o'clock.
  const from = ((startAngle - 90) * Math.PI) / 180;
  if (Math.abs(degrees) >= 360) {
    const [x, y] = polar(center, radius, startAngle);
    const commands: PathCommand[] = [{ type: 'moveTo', x, y }];
    arcTo(commands, radius, radius, radius, radius, from, Math.sign(degrees) * Math.PI * 2);
    commands.push({ type: 'close' });
    return commands;
  }
  const sweep = (degrees * Math.PI) / 180;
  const turn = Math.sign(degrees);
  // The corners where the sides meet the rim: the largest arc that touches
  // each side no nearer the centre than halfway and the rim no further than
  // halfway round it.
  const halfway = Math.sin(Math.min(Math.abs(sweep) / 2, Math.PI / 2));
  const rim = Math.min(cornerRadius, (radius * 3) / 8, (radius * halfway) / (1 + halfway));
  // How far round the rim each of those arcs reaches, in degrees.
  const inset = rim > 0 ? (Math.asin(rim / (radius - rim)) * 180) / Math.PI : 0;
  const along = Math.sqrt(radius * radius - 2 * radius * rim);
  const first: Vec = polar(center, along, startAngle);
  const last: Vec = polar(center, along, endAngle);
  const centerCorner = cornerAt(center, last, first, cornerRadius, radius / 2);

  const [x0, y0] = centerCorner?.end ?? center;
  const commands: PathCommand[] = [{ type: 'moveTo', x: x0, y: y0 }];
  commands.push({ type: 'lineTo', x: first[0], y: first[1] });
  if (rim > 0) {
    arcBetween(
      commands,
      polar(center, radius - rim, startAngle + turn * inset),
      rim,
      first,
      polar(center, radius, startAngle + turn * inset),
    );
  }
  arcTo(commands, radius, radius, radius, radius, from + (turn * inset * Math.PI) / 180, sweep - (turn * 2 * inset * Math.PI) / 180);
  if (rim > 0) {
    arcBetween(
      commands,
      polar(center, radius - rim, endAngle - turn * inset),
      rim,
      polar(center, radius, endAngle - turn * inset),
      last,
    );
  }
  if (centerCorner !== undefined) {
    commands.push({ type: 'lineTo', x: centerCorner.start[0], y: centerCorner.start[1] });
    arcBetween(commands, centerCorner.center, centerCorner.radius, centerCorner.start, centerCorner.end);
  }
  commands.push({ type: 'close' });
  return commands;
}

function heartCommands(options: HeartPathOptions, who: string): PathCommand[] {
  const width = sizeOf(options.width, 'width', who);
  const height = sizeOf(options.height, 'height', who);
  if (width === 0 || height === 0) {
    return [];
  }
  // One lobe in a 1 × 1 box, from the bottom tip round to the cleft. Every
  // control point is inside the box, and the curve touches its left, top,
  // and bottom edges, so the heart fills its box exactly.
  const lobe = [
    [0.4, 0.86, 0, 0.62, 0, 0.32],
    [0, 0.12, 0.13, 0, 0.27, 0],
    [0.4, 0, 0.47, 0.07, 0.5, 0.2],
  ];
  const at = (u: number, v: number): Vec => [u * width, v * height];
  const commands: PathCommand[] = [{ type: 'moveTo', x: width / 2, y: height }];
  for (const [u1, v1, u2, v2, u, v] of lobe) {
    const [x1, y1] = at(u1, v1);
    const [x2, y2] = at(u2, v2);
    const [x, y] = at(u, v);
    commands.push({ type: 'cubicTo', x1, y1, x2, y2, x, y });
  }
  // The right lobe mirrors the left, from the cleft back to the tip.
  for (let i = lobe.length - 1; i >= 0; i -= 1) {
    const [u1, v1, u2, v2] = lobe[i];
    const [u, v] = i === 0 ? [0.5, 1] : lobe[i - 1].slice(4);
    const [x1, y1] = at(1 - u2, v2);
    const [x2, y2] = at(1 - u1, v1);
    const [x, y] = at(1 - u, v);
    commands.push({ type: 'cubicTo', x1, y1, x2, y2, x, y });
  }
  commands.push({ type: 'close' });
  return commands;
}

/**
 * Path commands for a triangle filling a `width` × `height` box with its
 * top-left at the origin: the top corner at `apex` along the top edge and
 * the base along the bottom, clockwise from the top. Pass them to `<Path>`,
 * or move or reshape them first.
 */
export function trianglePath(options: TrianglePathOptions): PathCommand[] {
  return triangleCommands(options, 'trianglePath()');
}

/**
 * Path commands for a regular polygon in a `2 * radius` square box with its
 * top-left at the origin: `sides` corners on the circle of `radius`, the
 * first at 12 o'clock, clockwise.
 */
export function polygonPath(options: PolygonPathOptions): PathCommand[] {
  return polygonCommands(options, 'polygonPath()');
}

/**
 * Path commands for a star in a `2 * radius` square box with its top-left
 * at the origin: `points` tips on the circle of `radius`, the first at 12
 * o'clock, clockwise, with inner corners at `innerRatio` of the radius.
 */
export function starPath(options: StarPathOptions): PathCommand[] {
  return starCommands(options, 'starPath()');
}

/**
 * Path commands for a pie slice in a `2 * radius` square box with its
 * top-left at the origin, from `startAngle` to `endAngle` (degrees clockwise
 * from 12 o'clock), starting from the centre. Returns no commands when the
 * angles are equal, and a whole circle when they are 360° or more apart.
 */
export function piePath(options: PiePathOptions): PathCommand[] {
  return pieCommands(options, 'piePath()');
}

/**
 * Path commands for a heart filling a `width` × `height` box with its
 * top-left at the origin, clockwise from the bottom tip.
 */
export function heartPath(options: HeartPathOptions): PathCommand[] {
  return heartCommands(options, 'heartPath()');
}

/**
 * Props shared by `<Triangle>`, `<Polygon>`, `<Star>`, `<Pie>`, and
 * `<Heart>`. Like `<Rect>`, `x`/`y` place the box's `anchorX`/`anchorY`
 * point (its top-left by default), which `scale`/`rotation` turn about.
 */
interface ClosedShapeProps extends StrokeProps {
  anchorX?: number;
  anchorY?: number;
  /**
   * How the ends of dashes finish, as for `<Path>`. A closed outline has no
   * other ends, so it changes nothing on a solid stroke.
   */
  cap?: LineCap;
  /**
   * Hex color, `<Theme>` color name, or a gradient `Paint` in the shape's
   * local pixels (origin at its box's top-left, as for `<Rect>`). Omit for
   * no fill.
   */
  fill?: string | Paint;
}

export interface TriangleProps extends ClosedShapeProps, TrianglePathOptions {}
export interface PolygonProps extends ClosedShapeProps, PolygonPathOptions {}
export interface StarProps extends ClosedShapeProps, StarPathOptions {}
export interface PieProps extends ClosedShapeProps, PiePathOptions {}
export interface HeartProps extends ClosedShapeProps, HeartPathOptions {}

/** `commands` moved by `dx`/`dy`. */
function offsetCommands(commands: readonly PathCommand[], dx: number, dy: number): PathCommand[] {
  return commands.map((command) => {
    switch (command.type) {
      case 'close':
        return command;
      case 'moveTo':
      case 'lineTo':
        return { ...command, x: command.x + dx, y: command.y + dy };
      case 'quadTo':
        return { ...command, x1: command.x1 + dx, y1: command.y1 + dy, x: command.x + dx, y: command.y + dy };
      case 'cubicTo':
        return {
          ...command,
          x1: command.x1 + dx,
          y1: command.y1 + dy,
          x2: command.x2 + dx,
          y2: command.y2 + dy,
          x: command.x + dx,
          y: command.y + dy,
        };
    }
  });
}

/**
 * `commands` (in a `width` × `height` box from its top-left) as one
 * `<Path>`, placed by the box's anchor like `<Ellipse>`, with the stroke
 * centred on the outline as for `<Path>`.
 */
function closedShape(
  component: string,
  commands: readonly PathCommand[],
  width: number,
  height: number,
  { anchorX = 0, anchorY = 0, fill, stroke, strokeWidth, ...props }: ClosedShapeProps,
): ReturnType<typeof React.createElement> | null {
  const strokeWidthDrawn = stroke === undefined ? undefined : strokeWidthOf(strokeWidth, component);
  if (commands.length === 0) {
    return null;
  }
  // The path has no anchor, so its origin is the anchor point.
  const left = -anchorX * width;
  const top = -anchorY * height;
  return React.createElement(Path, {
    ...props,
    commands: left === 0 && top === 0 ? commands : offsetCommands(commands, left, top),
    fill: offsetPaint(fill, left, top),
    stroke: offsetPaint(stroke, left, top),
    ...(strokeWidthDrawn === undefined ? {} : { strokeWidth: strokeWidthDrawn }),
  });
}

/**
 * A triangle filling a `width` × `height` box, drawn as one `<Path>`: its
 * top corner at `apex` along the top edge (the middle by default) and its
 * base along the bottom. Turn it with `rotation` to point elsewhere.
 */
export function Triangle({
  width,
  height,
  apex,
  cornerRadius,
  ...props
}: TriangleProps): ReturnType<typeof React.createElement> | null {
  const commands = triangleCommands({ width, height, apex, cornerRadius }, '<Triangle>');
  return closedShape('Triangle', commands, width, height, props);
}

/**
 * A regular polygon of `sides` corners on a circle of `radius`, the first at
 * 12 o'clock, drawn as one `<Path>` and placed like a `<Circle>` of that
 * radius.
 */
export function Polygon({
  radius,
  sides,
  cornerRadius,
  ...props
}: PolygonProps): ReturnType<typeof React.createElement> | null {
  const commands = polygonCommands({ radius, sides, cornerRadius }, '<Polygon>');
  return closedShape('Polygon', commands, radius * 2, radius * 2, props);
}

/**
 * A star of `points` tips on a circle of `radius`, the first at 12 o'clock,
 * with inner corners at `innerRatio` of the radius, drawn as one `<Path>`
 * and placed like a `<Circle>` of that radius.
 */
export function Star({
  radius,
  points,
  innerRatio,
  cornerRadius,
  ...props
}: StarProps): ReturnType<typeof React.createElement> | null {
  const commands = starCommands({ radius, points, innerRatio, cornerRadius }, '<Star>');
  return closedShape('Star', commands, radius * 2, radius * 2, props);
}

/**
 * A slice of a circle of `radius` from `startAngle` to `endAngle` (degrees
 * clockwise from 12 o'clock), drawn as one `<Path>` and placed like a
 * `<Circle>` of that radius. Animate `endAngle` to sweep it open: equal
 * angles draw nothing, and 360° or more apart a whole circle.
 */
export function Pie({
  radius,
  startAngle,
  endAngle,
  cornerRadius,
  ...props
}: PieProps): ReturnType<typeof React.createElement> | null {
  const commands = pieCommands({ radius, startAngle, endAngle, cornerRadius }, '<Pie>');
  return closedShape('Pie', commands, radius * 2, radius * 2, props);
}

/** A heart filling a `width` × `height` box, drawn as one `<Path>`. */
export function Heart({
  width,
  height,
  ...props
}: HeartProps): ReturnType<typeof React.createElement> | null {
  const commands = heartCommands({ width, height }, '<Heart>');
  return closedShape('Heart', commands, width, height, props);
}

// Layer primitives, so a source chain can tell `<Star>` from a component the
// author named `Star`.
for (const shape of [Path, Line, Polyline, Ellipse, Circle, Arrow, Triangle, Polygon, Star, Pie, Heart]) {
  markPrimitive(shape);
}

// What the editor's Properties tab edits on each, by section. The bounds are
// the shapes' own: sizes and radii of at least 0 (`sizeOf`, `dimensionOf`),
// a whole number of at least 3 sides or points (`countOf`), and a stroke
// wider than nothing (`strokeWidthOf`; the field's bounds are inclusive, so
// 0.1).
const size = (defaultValue?: number) => num(defaultValue, { min: 0 });
const count = () => num(undefined, { min: 3, integer: true });
const strokeWidth = () => num(2, { min: 0.1 });
registerPrimitiveProperties('Ellipse', propertyTable<EllipseProps>({
  size: { width: 'width', height: 'height' },
  groups: {
    Transform: {
      x: num(0), y: num(0), rotation: num(0), scale: num(1, { step: 0.01 }),
      anchorX: num(0, { step: 0.01 }), anchorY: num(0, { step: 0.01 }), opacity: num(1, { min: 0, max: 1 }),
    },
    Size: { width: required(size()), height: required(size()) },
    Appearance: { fill: color(), stroke: color(), strokeWidth: strokeWidth() },
  },
}));
registerPrimitiveProperties('Circle', propertyTable<CircleProps>({
  size: { radius: 'radius' },
  groups: {
    Transform: {
      x: num(0), y: num(0), rotation: num(0), scale: num(1, { step: 0.01 }),
      anchorX: num(0, { step: 0.01 }), anchorY: num(0, { step: 0.01 }), opacity: num(1, { min: 0, max: 1 }),
    },
    Size: { radius: required(size()) },
    Appearance: { fill: color(), stroke: color(), strokeWidth: strokeWidth() },
  },
}));
registerPrimitiveProperties('Triangle', propertyTable<TriangleProps>({
  size: { width: 'width', height: 'height' },
  groups: {
    Transform: {
      x: num(0), y: num(0), rotation: num(0), scale: num(1, { step: 0.01 }),
      anchorX: num(0, { step: 0.01 }), anchorY: num(0, { step: 0.01 }), opacity: num(1, { min: 0, max: 1 }),
    },
    Size: { width: required(size()), height: required(size()), apex: num(0.5, { min: 0, max: 1 }), cornerRadius: size(0) },
    Appearance: { fill: color(), stroke: color(), strokeWidth: strokeWidth() },
  },
}));
registerPrimitiveProperties('Polygon', propertyTable<PolygonProps>({
  size: { radius: 'radius' },
  groups: {
    Transform: {
      x: num(0), y: num(0), rotation: num(0), scale: num(1, { step: 0.01 }),
      anchorX: num(0, { step: 0.01 }), anchorY: num(0, { step: 0.01 }), opacity: num(1, { min: 0, max: 1 }),
    },
    Size: { radius: required(size()), sides: required(count()), cornerRadius: size(0) },
    Appearance: { fill: color(), stroke: color(), strokeWidth: strokeWidth() },
  },
}));
registerPrimitiveProperties('Star', propertyTable<StarProps>({
  size: { radius: 'radius' },
  groups: {
    Transform: {
      x: num(0), y: num(0), rotation: num(0), scale: num(1, { step: 0.01 }),
      anchorX: num(0, { step: 0.01 }), anchorY: num(0, { step: 0.01 }), opacity: num(1, { min: 0, max: 1 }),
    },
    Size: { radius: required(size()), points: required(count()), innerRatio: num(0.5, { min: 0, max: 1 }), cornerRadius: size(0) },
    Appearance: { fill: color(), stroke: color(), strokeWidth: strokeWidth() },
  },
}));
registerPrimitiveProperties('Pie', propertyTable<PieProps>({
  size: { radius: 'radius' },
  groups: {
    Transform: {
      x: num(0), y: num(0), rotation: num(0), scale: num(1, { step: 0.01 }),
      anchorX: num(0, { step: 0.01 }), anchorY: num(0, { step: 0.01 }), opacity: num(1, { min: 0, max: 1 }),
    },
    Size: { radius: required(size()), startAngle: num(0), endAngle: required(num()), cornerRadius: size(0) },
    Appearance: { fill: color(), stroke: color(), strokeWidth: strokeWidth() },
  },
}));
registerPrimitiveProperties('Heart', propertyTable<HeartProps>({
  size: { width: 'width', height: 'height' },
  groups: {
    Transform: {
      x: num(0), y: num(0), rotation: num(0), scale: num(1, { step: 0.01 }),
      anchorX: num(0, { step: 0.01 }), anchorY: num(0, { step: 0.01 }), opacity: num(1, { min: 0, max: 1 }),
    },
    Size: { width: required(size()), height: required(size()) },
    Appearance: { fill: color(), stroke: color(), strokeWidth: strokeWidth() },
  },
}));
