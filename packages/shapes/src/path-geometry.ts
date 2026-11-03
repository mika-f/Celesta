// Lengths, points, and tangents along path commands, and the part of a path
// between two lengths. Curves are measured with adaptive Gauss–Legendre
// quadrature, accurate to well under a thousandth of a pixel.

import type { PathCommand } from '@celesta.studio/react';

/** A point, as `{ x, y }` like a path command's coordinates. */
export interface PathPoint {
  x: number;
  y: number;
}

/**
 * The direction of a path at a point: a unit vector `x`/`y`, and `angle`, the
 * same direction in degrees clockwise from +x, ready for a layer's `rotation`.
 */
export interface PathTangent {
  x: number;
  y: number;
  angle: number;
}

/** A path measured once, to ask for many lengths along it cheaply. */
export interface PathMeasure {
  /** Total length of every drawn segment; the jumps of `moveTo` do not count. */
  readonly length: number;
  /** The point `length` along the path, clamped to the ends. */
  getPointAtLength(length: number): PathPoint;
  /** The direction of the path `length` along it, clamped to the ends. */
  getTangentAtLength(length: number): PathTangent;
}

type Vec = readonly [number, number];

/** One drawn segment: a line, a quadratic, or a cubic Bézier, by its control points. */
export interface Segment {
  kind: 'line' | 'quad' | 'cubic';
  points: Vec[];
  /** The straight segment a `close` draws back to the subpath's start. */
  closing?: boolean;
}

/** Segments drawn from one `moveTo` on, and whether a `close` ends them. */
export interface Subpath {
  start: Vec;
  segments: Segment[];
  closed: boolean;
}

/**
 * The subpaths `commands` draw, as the renderer reads them: a drawing
 * command with no `moveTo` before it starts at the origin, one after a
 * `close` starts where that subpath started, and `close` adds the straight
 * segment back to the start (even when it has no length).
 */
export function subpathsOf(commands: readonly PathCommand[]): Subpath[] {
  const subpaths: Subpath[] = [];
  let current: Subpath | null = null;
  let point: Vec = [0, 0];
  let start: Vec = [0, 0];
  const open = (): Subpath => {
    if (current === null) {
      current = { start, segments: [], closed: false };
      subpaths.push(current);
    }
    return current;
  };
  for (const command of commands) {
    switch (command.type) {
      case 'moveTo':
        point = start = [command.x, command.y];
        current = { start, segments: [], closed: false };
        subpaths.push(current);
        break;
      case 'lineTo':
        open().segments.push({ kind: 'line', points: [point, (point = [command.x, command.y])] });
        break;
      case 'quadTo':
        open().segments.push({
          kind: 'quad',
          points: [point, [command.x1, command.y1], (point = [command.x, command.y])],
        });
        break;
      case 'cubicTo':
        open().segments.push({
          kind: 'cubic',
          points: [point, [command.x1, command.y1], [command.x2, command.y2], (point = [command.x, command.y])],
        });
        break;
      case 'close': {
        const subpath = open();
        subpath.segments.push({ kind: 'line', points: [point, subpath.start], closing: true });
        subpath.closed = true;
        point = subpath.start;
        current = null;
        break;
      }
    }
  }
  return subpaths;
}

/** The point at `t` (0–1) of a segment, by de Casteljau. */
export function segmentPoint({ points }: Segment, t: number): Vec {
  let level = points;
  while (level.length > 1) {
    level = level.slice(1).map((p, i) => [level[i][0] + (p[0] - level[i][0]) * t, level[i][1] + (p[1] - level[i][1]) * t]);
  }
  return level[0];
}

/** The derivative of a segment at `t`. */
function segmentVelocity({ points }: Segment, t: number): Vec {
  const n = points.length - 1;
  const hull: Vec[] = points.slice(1).map((p, i) => [n * (p[0] - points[i][0]), n * (p[1] - points[i][1])]);
  return segmentPoint({ kind: 'line', points: hull }, t);
}

/** `segment` split at `t` into the part before and the part after. */
export function splitSegment(segment: Segment, t: number): [Segment, Segment] {
  const before: Vec[] = [];
  const after: Vec[] = [];
  let level = segment.points;
  while (level.length > 0) {
    before.push(level[0]);
    after.unshift(level[level.length - 1]);
    level = level.slice(1).map((p, i) => [level[i][0] + (p[0] - level[i][0]) * t, level[i][1] + (p[1] - level[i][1]) * t]);
  }
  return [
    { kind: segment.kind, points: before },
    { kind: segment.kind, points: after },
  ];
}

/** The part of `segment` between `t0` and `t1`. */
function subSegment(segment: Segment, t0: number, t1: number): Segment {
  const tail = t0 <= 0 ? segment : splitSegment(segment, t0)[1];
  return t1 >= 1 ? tail : splitSegment(tail, t0 >= 1 ? 0 : (t1 - t0) / (1 - t0))[0];
}

// Ten-point Gauss–Legendre abscissae and weights on [-1, 1], half of them.
const GL_X = [0.1488743389816312, 0.4333953941292472, 0.6794095682990244, 0.8650633666889845, 0.9739065285171717];
const GL_W = [0.2955242247147529, 0.2692667193099963, 0.2190863625159820, 0.1494513491505806, 0.0666713443086881];

function speed(segment: Segment, t: number): number {
  const [dx, dy] = segmentVelocity(segment, t);
  return Math.hypot(dx, dy);
}

function quadrature(segment: Segment, a: number, b: number): number {
  const half = (b - a) / 2;
  const mid = (a + b) / 2;
  let sum = 0;
  for (let i = 0; i < GL_X.length; i += 1) {
    sum += GL_W[i] * (speed(segment, mid - half * GL_X[i]) + speed(segment, mid + half * GL_X[i]));
  }
  return sum * half;
}

/** The arc length of `segment` from `a` to `b`, halving until both halves agree with the whole. */
function arcLength(segment: Segment, a: number, b: number, whole = quadrature(segment, a, b), depth = 0): number {
  const mid = (a + b) / 2;
  const left = quadrature(segment, a, mid);
  const right = quadrature(segment, mid, b);
  if (depth >= 12 || Math.abs(left + right - whole) <= 1e-9 * Math.max(1, whole)) {
    return left + right;
  }
  return arcLength(segment, a, mid, left, depth + 1) + arcLength(segment, mid, b, right, depth + 1);
}

/** Sample count per curve: the arc length is tabulated at these many equal steps of `t`. */
const STEPS = 16;

/** A segment with its lengths tabulated, so a length along it maps back to `t`. */
export interface MeasuredSegment {
  segment: Segment;
  /** Arc length from `t = 0` to `t = i / STEPS` (lines: just `[0, length]`). */
  table: number[];
  length: number;
  /** Where along the whole path the segment begins. */
  offset: number;
  /** Index of its subpath, so a slice knows where a new `moveTo` is needed. */
  subpath: number;
}

export function measureSegment(segment: Segment, offset = 0, subpath = 0): MeasuredSegment {
  if (segment.kind === 'line') {
    const [[x0, y0], [x1, y1]] = segment.points;
    const length = Math.hypot(x1 - x0, y1 - y0);
    return { segment, table: [0, length], length, offset, subpath };
  }
  const table = [0];
  for (let i = 1; i <= STEPS; i += 1) {
    table.push(table[i - 1] + arcLength(segment, (i - 1) / STEPS, i / STEPS));
  }
  return { segment, table, length: table[STEPS], offset, subpath };
}

/** The `t` at which `measured` has covered `distance` of its own length. */
export function parameterAt({ segment, table, length }: MeasuredSegment, distance: number): number {
  if (length === 0) {
    return 0;
  }
  if (segment.kind === 'line') {
    return Math.min(1, Math.max(0, distance / length));
  }
  if (distance <= 0) {
    return 0;
  }
  if (distance >= length) {
    return 1;
  }
  let i = 1;
  while (i < STEPS && table[i] < distance) {
    i += 1;
  }
  // Newton's method on the arc length inside step i, kept within the step.
  const lo = (i - 1) / STEPS;
  const hi = i / STEPS;
  let t = lo + ((distance - table[i - 1]) / (table[i] - table[i - 1] || 1)) / STEPS;
  for (let iteration = 0; iteration < 8; iteration += 1) {
    const error = table[i - 1] + arcLength(segment, lo, t) - distance;
    const v = speed(segment, t);
    if (Math.abs(error) < 1e-9 || v === 0) {
      break;
    }
    t = Math.min(hi, Math.max(lo, t - error / v));
  }
  return t;
}

/** The direction of `segment` at `t`, looking past a zero-length derivative at its ends. */
function segmentDirection(segment: Segment, t: number): Vec | null {
  const [dx, dy] = segmentVelocity(segment, t);
  if (Math.hypot(dx, dy) > 1e-12) {
    return [dx, dy];
  }
  // A control point on an end point: the curve leaves toward the next distinct point.
  const { points } = segment;
  const ordered = t < 0.5 ? points : [...points].reverse();
  const from = ordered[0];
  for (const p of ordered.slice(1)) {
    const ex = p[0] - from[0];
    const ey = p[1] - from[1];
    if (Math.hypot(ex, ey) > 1e-12) {
      return t < 0.5 ? [ex, ey] : [-ex, -ey];
    }
  }
  return null;
}

function finite(value: number, name: string, fn: string): number {
  if (Number.isNaN(value)) {
    throw new Error(`${fn}() requires a number for \`${name}\`, got NaN`);
  }
  return value;
}

/** Every segment of `commands`, with its lengths, in drawing order. */
export class MeasuredPath implements PathMeasure {
  readonly length: number;
  readonly segments: readonly MeasuredSegment[];
  private readonly origin: Vec;

  constructor(commands: readonly PathCommand[]) {
    const subpaths = subpathsOf(commands);
    const segments: MeasuredSegment[] = [];
    let offset = 0;
    subpaths.forEach((subpath, index) => {
      for (const segment of subpath.segments) {
        const measured = measureSegment(segment, offset, index);
        segments.push(measured);
        offset += measured.length;
      }
    });
    this.length = offset;
    this.segments = segments;
    this.origin = subpaths[0]?.start ?? [0, 0];
  }

  /** The segment `distance` lies on: at a join, the one that starts there. */
  private locate(distance: number): MeasuredSegment | undefined {
    const drawn = this.segments.filter((s) => s.length > 0);
    return drawn.find((s) => distance < s.offset + s.length) ?? drawn[drawn.length - 1];
  }

  /** How far `distance` is past the start of the subpath it lies on (0 at a subpath's start). */
  distanceIntoSubpath(distance: number): number {
    const at = this.locate(distance);
    if (at === undefined) {
      return 0;
    }
    const begin = this.segments.find((s) => s.subpath === at.subpath)!.offset;
    return Math.max(0, distance - begin);
  }

  getPointAtLength(length: number): PathPoint {
    const distance = Math.min(this.length, Math.max(0, finite(length, 'length', 'getPointAtLength')));
    const measured = this.locate(distance);
    if (measured === undefined) {
      const [x, y] = this.segments[0]?.segment.points[0] ?? this.origin;
      return { x, y };
    }
    const [x, y] = segmentPoint(measured.segment, parameterAt(measured, distance - measured.offset));
    return { x, y };
  }

  getTangentAtLength(length: number): PathTangent {
    const distance = Math.min(this.length, Math.max(0, finite(length, 'length', 'getTangentAtLength')));
    const measured = this.locate(distance);
    const direction = measured && segmentDirection(measured.segment, parameterAt(measured, distance - measured.offset));
    if (!direction) {
      return { x: 1, y: 0, angle: 0 };
    }
    const norm = Math.hypot(direction[0], direction[1]);
    return {
      x: direction[0] / norm,
      y: direction[1] / norm,
      angle: (Math.atan2(direction[1], direction[0]) * 180) / Math.PI,
    };
  }

  /**
   * The commands that draw this path from `from` to `to` (lengths along it).
   * A closed subpath drawn whole keeps its `close`; any other part of one
   * ends open, like a `<Polyline>` that has not finished drawing.
   */
  slice(from: number, to: number): PathCommand[] {
    const commands: PathCommand[] = [];
    if (!(to > from)) {
      return commands;
    }
    // A subpath is whole when the range covers it from its first segment to its last.
    const bounds = new Map<number, [number, number]>();
    for (const { subpath, offset, length } of this.segments) {
      const [begin] = bounds.get(subpath) ?? [offset];
      bounds.set(subpath, [begin, offset + length]);
    }
    let drawing = -1;
    for (const measured of this.segments) {
      const { segment, offset: s0, length, subpath } = measured;
      const s1 = s0 + length;
      const inside = length > 0 ? s1 > from && s0 < to : s0 >= from && s0 <= to;
      if (!inside) {
        continue;
      }
      const part = length > 0
        ? subSegment(segment, parameterAt(measured, from - s0), parameterAt(measured, to - s0))
        : segment;
      if (drawing !== subpath) {
        const [x, y] = part.points[0];
        commands.push({ type: 'moveTo', x, y });
        drawing = subpath;
      }
      const [begin, end] = bounds.get(subpath)!;
      commands.push(segment.closing && from <= begin && to >= end ? { type: 'close' } : commandOf(part));
    }
    return commands;
  }
}

// The same commands array is usually measured every frame; measure it once.
const measured = new WeakMap<readonly PathCommand[], MeasuredPath>();

/**
 * Measures `commands` once, for many lengths along them: `length`, and
 * `getPointAtLength`/`getTangentAtLength` without re-measuring. Every drawn
 * segment counts, a `close` included; the jumps of `moveTo` do not.
 */
export function measurePath(commands: readonly PathCommand[]): PathMeasure {
  let path = measured.get(commands);
  if (path === undefined) {
    path = new MeasuredPath(commands);
    measured.set(commands, path);
  }
  return path;
}

/** The total length of `commands`, curves included. See {@link measurePath}. */
export function getLength(commands: readonly PathCommand[]): number {
  return measurePath(commands).length;
}

/**
 * The point `length` along `commands` as `{ x, y }`, in the path's own
 * coordinates; `length` is clamped to 0 and the total length.
 */
export function getPointAtLength(commands: readonly PathCommand[], length: number): PathPoint {
  return measurePath(commands).getPointAtLength(length);
}

/**
 * The direction of `commands` `length` along them: a unit vector `{ x, y }`
 * and its `angle` in degrees clockwise, for a layer's `rotation`. At a
 * corner it is the direction of the segment that starts there.
 */
export function getTangentAtLength(commands: readonly PathCommand[], length: number): PathTangent {
  return measurePath(commands).getTangentAtLength(length);
}

/**
 * The point `t` (0–1) of the way along `commands`, measured by length, as
 * `{ x, y }`. It is the tip of a `<Path>` whose `progress` is `t`; put a
 * marker or a label there. (`pointOnPolyline` returns `[x, y]` instead, the
 * same shape as its `points`.)
 */
export function pointOnPath(commands: readonly PathCommand[], t: number): PathPoint {
  const path = measurePath(commands);
  return path.getPointAtLength(path.length * Math.min(1, Math.max(0, finite(t, 't', 'pointOnPath'))));
}

/** The direction of `commands` `t` (0–1) of the way along them. See {@link getTangentAtLength}. */
export function tangentOnPath(commands: readonly PathCommand[], t: number): PathTangent {
  const path = measurePath(commands);
  return path.getTangentAtLength(path.length * Math.min(1, Math.max(0, finite(t, 't', 'tangentOnPath'))));
}

/**
 * The part of `commands` from `start` to `end` (0–1 of its length). Whole,
 * the commands come back unchanged.
 */
export function trimCommands(commands: readonly PathCommand[], start: number, end: number): readonly PathCommand[] {
  if (start <= 0 && end >= 1) {
    return commands;
  }
  const path = measurePath(commands) as MeasuredPath;
  return path.slice(path.length * Math.max(0, start), path.length * Math.min(1, end));
}

/**
 * How far into its own subpath the part of `commands` from `start` (0–1 of
 * the length) begins, in pixels: what a dash pattern, which starts over at
 * each `moveTo`, has already covered there on the whole path.
 */
export function trimStartIntoSubpath(commands: readonly PathCommand[], start: number): number {
  const path = measurePath(commands) as MeasuredPath;
  return path.distanceIntoSubpath(path.length * Math.min(1, Math.max(0, start)));
}

/** The command that draws `segment` from its first point. */
export function commandOf({ kind, points }: Segment): PathCommand {
  const [x, y] = points[points.length - 1];
  if (kind === 'line') {
    return { type: 'lineTo', x, y };
  }
  if (kind === 'quad') {
    return { type: 'quadTo', x1: points[1][0], y1: points[1][1], x, y };
  }
  return { type: 'cubicTo', x1: points[1][0], y1: points[1][1], x2: points[2][0], y2: points[2][1], x, y };
}
