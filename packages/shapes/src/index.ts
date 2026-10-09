import * as React from 'react';

import { Rect } from '@celesta/react';
import type { CommonProps, LineCap, LineJoin, Paint, PathCommand } from '@celesta/react';

export type { LineCap, LineJoin, PathCommand };

/** A point as `[x, y]`, in the path's own coordinates. */
export type PolylinePoint = readonly [number, number];

/**
 * Transform and compositing props of a path. There is no `anchorX`/`anchorY`:
 * a path's coordinates already have their own origin, which `x`/`y` place
 * and `scale`/`rotation` turn about.
 */
type PathLayerProps = Omit<CommonProps, 'anchorX' | 'anchorY'>;

interface StrokeProps extends PathLayerProps {
  /** Hex color (`"#RRGGBB"` or `"#RRGGBBAA"`) or a gradient `Paint` of the stroke. */
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
   * Fills the inside of the path (non-zero rule), as a hex color or a
   * `Paint` in the path's own coordinates. Omit for no fill.
   */
  fill?: string | Paint;
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
  ...props
}: PathProps): ReturnType<typeof React.createElement> {
  if (commands === undefined && points === undefined) {
    throw new Error('<Path> requires `commands` or `points`');
  }
  return React.createElement('path', {
    ...props,
    commands: commands ?? commandsThrough(points ?? [], closed),
    stroke,
    ...(stroke === undefined ? {} : { strokeWidth: strokeWidthOf(strokeWidth, 'Path') }),
  });
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

/** The first `progress` (0–1) of a polyline, by length. */
function polylinePrefix(points: readonly PolylinePoint[], progress: number): PolylinePoint[] {
  const lengths = segmentLengths(points);
  let remaining = lengths.reduce((sum, n) => sum + n, 0) * Math.min(1, Math.max(0, progress));
  const prefix: PolylinePoint[] = points.slice(0, 1);
  for (let i = 0; i < lengths.length && remaining > 0; i += 1) {
    const u = Math.min(1, remaining / lengths[i]);
    const [x1, y1] = points[i];
    const [x2, y2] = points[i + 1];
    prefix.push([x1 + (x2 - x1) * u, y1 + (y2 - y1) * u]);
    remaining -= lengths[i];
  }
  return prefix.length > 1 ? prefix : [];
}

/**
 * Connected line segments through `points`, such as a line chart or a route,
 * drawn as one `<Path>`. With round caps (the default) the corners are
 * rounded too, unless `join` says otherwise.
 */
export function Polyline({
  points,
  closed = false,
  progress = 1,
  stroke = '#FFFFFF',
  strokeWidth,
  cap = 'round',
  join,
  ...props
}: PolylineProps): ReturnType<typeof React.createElement> {
  const width = strokeWidthOf(strokeWidth, 'Polyline');
  const whole = progress >= 1;
  const drawn = whole
    ? points
    : polylinePrefix(closed && points.length > 0 ? [...points, points[0]] : points, progress);
  return React.createElement(Path, {
    ...props,
    points: drawn,
    closed: closed && whole,
    stroke,
    strokeWidth: width,
    cap,
    join: join ?? (cap === 'round' ? 'round' : 'miter'),
  });
}

export interface EllipseProps extends CommonProps {
  /** Width of the ellipse's bounding box, in pixels. 0 draws nothing. */
  width: number;
  /** Height of the ellipse's bounding box, in pixels. 0 draws nothing. */
  height: number;
  /**
   * Hex color or a gradient `Paint` in the shape's local pixels (origin at
   * its bounding box's top-left, as for `<Rect>`). Omit for no fill.
   */
  fill?: string | Paint;
  /** Hex color or `Paint` of a stroke drawn inside the outline, as for `<Rect>`. */
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
  /** Hex color or a gradient `Paint` (in the arrow's coordinates) of the whole arrow. Defaults to white. */
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
 * by default), and the stroke stays inside the box.
 */
export function Ellipse({
  width,
  height,
  fill,
  stroke,
  strokeWidth,
  anchorX = 0,
  anchorY = 0,
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
  return React.createElement(Path, {
    ...props,
    commands: ellipseCommands(left + width / 2, top + height / 2, width / 2 - inset, height / 2 - inset),
    fill: offsetPaint(fill, left, top),
    stroke: offsetPaint(stroke, left, top),
    ...(stroke === undefined ? {} : { strokeWidth: strokeWidthDrawn }),
  });
}

/**
 * A circle of `radius`, placed like an `<Ellipse>` `2 * radius` square but
 * drawn as a `<Rect>` with `cornerRadius` `radius`: shaded directly on the
 * GPU with no outline to flatten, so many animated circles are much cheaper
 * than paths.
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
  return React.createElement(Rect, {
    ...props,
    width: radius * 2,
    height: radius * 2,
    cornerRadius: radius,
    stroke,
    ...(stroke === undefined ? {} : { strokeWidth: strokeWidthDrawn }),
  });
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
