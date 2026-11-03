// Morphing between two paths, whatever their commands.

import type { PathCommand } from '@celesta.studio/react';

import { MeasuredPath, measureSegment, parameterAt, splitSegment, subpathsOf } from './path-geometry';
import type { MeasuredSegment, Segment } from './path-geometry';

type Vec = readonly [number, number];
type Cubic = readonly [Vec, Vec, Vec, Vec];

/** A subpath as cubic curves only. */
interface Contour {
  start: Vec;
  cubics: Cubic[];
  closed: boolean;
}

const mix = (a: Vec, b: Vec, t: number): Vec => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];

/** A line or a quadratic as the cubic that draws exactly the same curve. */
function toCubic({ kind, points }: Segment): Cubic {
  if (kind === 'cubic') {
    return points as unknown as Cubic;
  }
  if (kind === 'line') {
    const [p0, p1] = points;
    return [p0, mix(p0, p1, 1 / 3), mix(p0, p1, 2 / 3), p1];
  }
  const [p0, c, p2] = points;
  return [p0, mix(p0, c, 2 / 3), mix(p2, c, 2 / 3), p2];
}

function contoursOf(commands: readonly PathCommand[]): Contour[] {
  return subpathsOf(commands).map(({ start, segments, closed }) => ({
    start,
    // A `close` that has no length adds nothing to morph.
    cubics: segments
      .filter((s, i) => !(s.closing && i > 0 && s.points[0][0] === start[0] && s.points[0][1] === start[1]))
      .map(toCubic),
    closed,
  }));
}

/** Fractions closer than this are the same place on a contour. */
const SAME = 1e-9;

const segmentOf = (cubic: Cubic): Segment => ({ kind: 'cubic', points: [...cubic] });

/** A contour's curves, at least one: a contour that draws nothing is a curve with no length. */
function curvesOf(contour: Contour): Cubic[] {
  const { start } = contour;
  return contour.cubics.length > 0 ? contour.cubics : [[start, start, start, start]];
}

/**
 * Where each curve of `cubics` ends, as a fraction (0–1) of their total
 * length; by count instead when they have no length at all.
 */
function endsOf(cubics: readonly Cubic[]): { ends: number[]; measured: MeasuredSegment[]; total: number } {
  const measured = cubics.map((c) => measureSegment(segmentOf(c)));
  const total = measured.reduce((sum, m) => sum + m.length, 0);
  let run = 0;
  const ends = measured.map((m, i) => {
    run += m.length;
    return i === measured.length - 1 ? 1 : total > 0 ? run / total : (i + 1) / measured.length;
  });
  return { ends, measured, total };
}

/** `cubics` split at each fraction of their length in `at` (ascending) that falls inside a curve. */
function splitAt(cubics: readonly Cubic[], at: readonly number[]): Cubic[] {
  const { ends, measured, total } = endsOf(cubics);
  const pieces: Cubic[] = [];
  let next = 0;
  let begin = 0;
  measured.forEach((m, i) => {
    const end = ends[i];
    let rest = m.segment;
    let done = 0;
    for (; next < at.length && at[next] < end - SAME; next += 1) {
      if (at[next] <= begin + SAME) {
        continue;
      }
      const t = total > 0 ? parameterAt(m, (at[next] - begin) * total) : (at[next] - begin) / (end - begin);
      // `rest` runs from `done` to 1 of the original curve; split it where `t` falls.
      const [before, after] = splitSegment(rest, (t - done) / (1 - done));
      pieces.push(before.points as unknown as Cubic);
      rest = after;
      done = t;
    }
    pieces.push(rest.points as unknown as Cubic);
    begin = end;
  });
  return pieces;
}

/**
 * `a` and `b` split into the same number of curves, each pair covering the
 * same fraction of its contour's length, so every corner of one meets the
 * point the same way around the other.
 */
function matchCurves(a: readonly Cubic[], b: readonly Cubic[]): [Cubic[], Cubic[]] {
  const union = [...endsOf(a).ends, ...endsOf(b).ends].sort((x, y) => x - y)
    .filter((f, i, all) => f < 1 - SAME && (i === 0 || f - all[i - 1] > SAME));
  return [splitAt(a, union), splitAt(b, union)];
}

/** The point `fraction` (0–1) of the way around `cubics` by length. */
function sampler(cubics: readonly Cubic[]): (fraction: number) => Vec {
  const commands: PathCommand[] = [{ type: 'moveTo', x: cubics[0][0][0], y: cubics[0][0][1] }];
  for (const c of cubics) {
    commands.push({ type: 'cubicTo', x1: c[1][0], y1: c[1][1], x2: c[2][0], y2: c[2][1], x: c[3][0], y: c[3][1] });
  }
  const path = new MeasuredPath(commands);
  return (fraction) => {
    const { x, y } = path.getPointAtLength(path.length * fraction);
    return [x, y];
  };
}

/** A contour shrunk to the middle of `contour`, to grow from or vanish into. */
function pointAtMiddle(contour: Contour): Contour {
  const ends = contour.cubics.length > 0 ? contour.cubics.map((c) => c[3]) : [contour.start];
  const x = ends.reduce((sum, p) => sum + p[0], 0) / ends.length;
  const y = ends.reduce((sum, p) => sum + p[1], 0) / ends.length;
  return { start: [x, y], cubics: [], closed: contour.closed };
}

/** Points compared around two closed contours to choose where the second starts. */
const SAMPLES = 48;

/**
 * Closed `b` started from whichever of its corners brings it, all the way
 * around, nearest to `a`, so a closed shape does not twist into the other.
 */
function align(a: readonly Cubic[], b: readonly Cubic[]): Cubic[] {
  if (b.length < 2) {
    return [...b];
  }
  const onA = sampler(a);
  const onB = sampler(b);
  const { ends } = endsOf(b);
  const fixed = Array.from({ length: SAMPLES }, (_, j) => onA(j / SAMPLES));
  let best = 0;
  let bestCost = Infinity;
  for (let shift = 0; shift < b.length; shift += 1) {
    const offset = shift === 0 ? 0 : ends[shift - 1];
    let cost = 0;
    for (let j = 0; j < SAMPLES; j += 1) {
      const p = fixed[j];
      const q = onB((j / SAMPLES + offset) % 1);
      cost += (p[0] - q[0]) ** 2 + (p[1] - q[1]) ** 2;
    }
    if (cost < bestCost - SAME) {
      bestCost = cost;
      best = shift;
    }
  }
  return [...b.slice(best), ...b.slice(0, best)];
}

/** Pairs of contours with the same number of curves, ready to blend point by point. */
type Prepared = Array<[Contour, Contour]> | 'same-commands';

function sameCommands(a: readonly PathCommand[], b: readonly PathCommand[]): boolean {
  return a.length === b.length && a.every((command, i) => command.type === b[i].type);
}

function prepare(a: readonly PathCommand[], b: readonly PathCommand[]): Prepared {
  if (sameCommands(a, b)) {
    return 'same-commands';
  }
  const from = contoursOf(a);
  const to = contoursOf(b);
  // A subpath with no partner grows from (or shrinks to) its own middle.
  while (from.length < to.length) {
    from.push(pointAtMiddle(to[from.length]));
  }
  while (to.length < from.length) {
    to.push(pointAtMiddle(from[to.length]));
  }
  return from.map((contour, i) => {
    const target = to[i];
    const source = curvesOf(contour);
    const aligned = contour.closed && target.closed ? align(source, curvesOf(target)) : curvesOf(target);
    const [a1, b1] = matchCurves(source, aligned);
    return [
      { start: a1[0][0], cubics: a1, closed: contour.closed },
      { start: b1[0][0], cubics: b1, closed: target.closed },
    ];
  });
}

const prepared = new WeakMap<readonly PathCommand[], WeakMap<readonly PathCommand[], Prepared>>();

function preparedFor(a: readonly PathCommand[], b: readonly PathCommand[]): Prepared {
  let byTarget = prepared.get(a);
  if (byTarget === undefined) {
    byTarget = new WeakMap();
    prepared.set(a, byTarget);
  }
  let pairs = byTarget.get(b);
  if (pairs === undefined) {
    pairs = prepare(a, b);
    byTarget.set(b, pairs);
  }
  return pairs;
}

/** Blends two commands of the same type field by field. */
function blendCommand(a: PathCommand, b: PathCommand, t: number): PathCommand {
  const blended: Record<string, unknown> = { type: a.type };
  for (const [key, value] of Object.entries(a)) {
    if (key !== 'type') {
      const target = (b as unknown as Record<string, number>)[key];
      blended[key] = (value as number) + (target - (value as number)) * t;
    }
  }
  return blended as unknown as PathCommand;
}

/**
 * The shape `t` (0–1) of the way from path `a` to path `b`, as commands for
 * `<Path>`. Paths with the same commands in the same order blend point by
 * point. Otherwise every segment becomes a cubic curve, and each subpath is
 * split wherever the other has a corner, at the same fraction of its length,
 * so each point travels to the place the same way around the other shape. A
 * closed shape starts from whichever of its corners brings it nearest the
 * other all the way around, and a subpath with no partner grows from or
 * shrinks to its middle. `t` is not clamped, so an overshooting easing
 * overshoots the shape too.
 *
 * The matching is worked out once per pair of command arrays and reused, so
 * keep `a` and `b` the same arrays from frame to frame.
 */
export function interpolatePath(a: readonly PathCommand[], b: readonly PathCommand[], t: number): PathCommand[] {
  if (Number.isNaN(t)) {
    throw new Error('interpolatePath() requires a number for `t`, got NaN');
  }
  if (a.length === 0 || b.length === 0) {
    throw new Error('interpolatePath() requires two paths with at least one command each');
  }
  const pairs = preparedFor(a, b);
  if (pairs === 'same-commands') {
    return a.map((command, i) => blendCommand(command, b[i], t));
  }
  if (t === 0) {
    return [...a];
  }
  if (t === 1) {
    return [...b];
  }
  const commands: PathCommand[] = [];
  for (const [from, to] of pairs) {
    const [x, y] = mix(from.start, to.start, t);
    commands.push({ type: 'moveTo', x, y });
    from.cubics.forEach((c, i) => {
      const [, p1, p2, p3] = c.map((p, j) => mix(p, to.cubics[i][j], t));
      commands.push({ type: 'cubicTo', x1: p1[0], y1: p1[1], x2: p2[0], y2: p2[1], x: p3[0], y: p3[1] });
    });
    if (t < 0.5 ? from.closed : to.closed) {
      commands.push({ type: 'close' });
    }
  }
  return commands;
}
