// 2D affine matrices for Lottie transforms, and the path commands of Lottie's
// Bézier paths and parametric shapes, built as lottie-web builds them so
// trim paths start and run the same way.

import type { PathCommand } from '@celesta.studio/react';

import { positionAt, scalarAt, vectorAt } from './property';
import type { LottieBezier, LottieShape, LottieTransform, LottieValue } from './types';

/** `[a, b, c, d, e, f]`: `x' = a·x + c·y + e`, `y' = b·x + d·y + f`. */
export type Matrix = readonly [number, number, number, number, number, number];

export const identity: Matrix = [1, 0, 0, 1, 0, 0];

/** `m · n`: `n` applied first. */
export function multiply(m: Matrix, n: Matrix): Matrix {
  return [
    m[0] * n[0] + m[2] * n[1],
    m[1] * n[0] + m[3] * n[1],
    m[0] * n[2] + m[2] * n[3],
    m[1] * n[2] + m[3] * n[3],
    m[0] * n[4] + m[2] * n[5] + m[4],
    m[1] * n[4] + m[3] * n[5] + m[5],
  ];
}

const radians = Math.PI / 180;

function rotation(degrees: number): Matrix {
  const c = Math.cos(degrees * radians);
  const s = Math.sin(degrees * radians);
  return [c, s, -s, c, 0, 0];
}

/**
 * A Lottie transform at `frame`, without its opacity: position · rotation ·
 * skew · scale · −anchor, the skew sheared along the axis `sa` degrees from
 * horizontal, as lottie-web composes it.
 */
export function transformMatrix(transform: LottieTransform | undefined, frame: number): Matrix {
  if (transform === undefined) {
    return identity;
  }
  const [ax = 0, ay = 0] = vectorAt(transform.a, frame, [0, 0]);
  const [px, py] = positionAt(transform.p, frame);
  const [sx = 100, sy = 100] = vectorAt(transform.s, frame, [100, 100]);
  const r = scalarAt(transform.r ?? transform.rz, frame, 0);
  const sk = scalarAt(transform.sk, frame, 0);
  const sa = scalarAt(transform.sa, frame, 0);
  let m: Matrix = [sx / 100, 0, 0, sy / 100, -ax * sx / 100, -ay * sy / 100];
  if (sk !== 0) {
    const shear: Matrix = [1, 0, -Math.tan(sk * radians), 1, 0, 0];
    m = multiply(multiply(rotation(-sa), multiply(shear, rotation(sa))), m);
  }
  m = multiply(rotation(r), m);
  return [m[0], m[1], m[2], m[3], m[4] + px, m[5] + py];
}

/** Opacity of a transform at `frame`, 0–1. */
export function transformOpacity(transform: { o?: LottieValue } | undefined, frame: number): number {
  return Math.min(1, Math.max(0, scalarAt(transform?.o, frame, 100) / 100));
}

/** The props of a `<Group>` that applies `m`, or `null` when it flattens everything. */
export function groupTransform(m: Matrix): {
  x: number;
  y: number;
  rotation: number;
  skewX: number;
  scaleX: number;
  scaleY: number;
} | null {
  const [a, b, c, d, e, f] = m;
  const det = a * d - b * c;
  if (!Number.isFinite(det) || Math.abs(det) < 1e-12) {
    return null;
  }
  // m = R(θ) · K(skewX) · S(scaleX, scaleY), as a layer's linear transform composes.
  const scaleX = Math.hypot(a, b);
  const theta = Math.atan2(b, a);
  const cos = Math.cos(theta);
  const sin = Math.sin(theta);
  const shear = cos * c + sin * d;
  const scaleY = -sin * c + cos * d;
  // Rounding leaves a trace of shear on unskewed transforms (a mirror turns
  // 180°, say); keep them flat, as skew takes a slower path to draw.
  const skewX = Math.atan(shear / scaleY) / radians;
  return {
    x: e,
    y: f,
    rotation: theta / radians,
    skewX: Math.abs(skewX) < 1e-9 ? 0 : skewX,
    scaleX,
    scaleY,
  };
}

/** `commands` with every point mapped through `m`. */
export function transformCommands(commands: readonly PathCommand[], m: Matrix): PathCommand[] {
  const x = (px: number, py: number) => m[0] * px + m[2] * py + m[4];
  const y = (px: number, py: number) => m[1] * px + m[3] * py + m[5];
  return commands.map((command): PathCommand => {
    switch (command.type) {
      case 'moveTo':
      case 'lineTo':
        return { type: command.type, x: x(command.x, command.y), y: y(command.x, command.y) };
      case 'quadTo':
        return {
          type: 'quadTo',
          x1: x(command.x1, command.y1),
          y1: y(command.x1, command.y1),
          x: x(command.x, command.y),
          y: y(command.x, command.y),
        };
      case 'cubicTo':
        return {
          type: 'cubicTo',
          x1: x(command.x1, command.y1),
          y1: y(command.x1, command.y1),
          x2: x(command.x2, command.y2),
          y2: y(command.x2, command.y2),
          x: x(command.x, command.y),
          y: y(command.x, command.y),
        };
      default:
        return command;
    }
  });
}

/** The commands of a Lottie Bézier path; segments without handles are straight. */
export function bezierCommands(path: LottieBezier): PathCommand[] {
  const { v, i, o } = path;
  if (v.length === 0) {
    return [];
  }
  const commands: PathCommand[] = [{ type: 'moveTo', x: v[0][0], y: v[0][1] }];
  const segment = (from: number, to: number) => {
    const out = o[from] ?? [0, 0];
    const into = i[to] ?? [0, 0];
    if (out[0] === 0 && out[1] === 0 && into[0] === 0 && into[1] === 0) {
      // `close` draws the straight segment back to the start itself.
      if (to !== 0) {
        commands.push({ type: 'lineTo', x: v[to][0], y: v[to][1] });
      }
      return;
    }
    commands.push({
      type: 'cubicTo',
      x1: v[from][0] + out[0],
      y1: v[from][1] + out[1],
      x2: v[to][0] + into[0],
      y2: v[to][1] + into[1],
      x: v[to][0],
      y: v[to][1],
    });
  };
  for (let n = 1; n < v.length; n += 1) {
    segment(n - 1, n);
  }
  if (path.c) {
    if (v.length > 1) {
      segment(v.length - 1, 0);
    }
    commands.push({ type: 'close' });
  }
  return commands;
}

/** How far a cubic's handle reaches to approximate a quarter circle. */
const kappa = 0.5519150244935105;

interface Vertex {
  v: [number, number];
  i: [number, number];
  o: [number, number];
}

function verticesPath(vertices: Vertex[], closed: boolean): PathCommand[] {
  return bezierCommands({
    c: closed,
    v: vertices.map(vertex => vertex.v),
    i: vertices.map(vertex => vertex.i),
    o: vertices.map(vertex => vertex.o),
  });
}

/** Reverses the drawing direction of `vertices`, keeping the first vertex first. */
function reversed(vertices: Vertex[]): Vertex[] {
  if (vertices.length === 0) {
    return vertices;
  }
  const [first, ...rest] = vertices;
  return [first, ...rest.reverse()].map(vertex => ({ v: vertex.v, i: vertex.o, o: vertex.i }));
}

/** A rectangle (`rc`): from its top-right corner, clockwise unless `d` is 3. */
export function rectCommands(shape: LottieShape, frame: number): PathCommand[] {
  const [px, py] = vectorAt(shape.p as LottieValue, frame, [0, 0]);
  const [w = 0, h = 0] = vectorAt(shape.s as LottieValue, frame, [0, 0]);
  const hw = w / 2;
  const hh = h / 2;
  const r = Math.max(0, Math.min(scalarAt(shape.r as LottieValue, frame, 0), hw, hh));
  const point = (x: number, y: number, ix = 0, iy = 0, ox = 0, oy = 0): Vertex =>
    ({ v: [px + x, py + y], i: [ix, iy], o: [ox, oy] });
  let vertices: Vertex[];
  if (r === 0) {
    vertices = [point(hw, -hh), point(hw, hh), point(-hw, hh), point(-hw, -hh)];
  } else {
    const k = r * kappa;
    vertices = [
      point(hw, -hh + r, 0, -k, 0, 0),
      point(hw, hh - r, 0, 0, 0, k),
      point(hw - r, hh, k, 0, 0, 0),
      point(-hw + r, hh, 0, 0, -k, 0),
      point(-hw, hh - r, 0, k, 0, 0),
      point(-hw, -hh + r, 0, 0, 0, -k),
      point(-hw + r, -hh, -k, 0, 0, 0),
      point(hw - r, -hh, 0, 0, k, 0),
    ];
  }
  return verticesPath(shape.d === 3 ? reversed(vertices) : vertices, true);
}

/** An ellipse (`el`): from its top, clockwise unless `d` is 3. */
export function ellipseCommands(shape: LottieShape, frame: number): PathCommand[] {
  const [px, py] = vectorAt(shape.p as LottieValue, frame, [0, 0]);
  const [w = 0, h = 0] = vectorAt(shape.s as LottieValue, frame, [0, 0]);
  const rx = w / 2;
  const ry = h / 2;
  const kx = rx * kappa;
  const ky = ry * kappa;
  const vertices: Vertex[] = [
    { v: [px, py - ry], i: [-kx, 0], o: [kx, 0] },
    { v: [px + rx, py], i: [0, -ky], o: [0, ky] },
    { v: [px, py + ry], i: [kx, 0], o: [-kx, 0] },
    { v: [px - rx, py], i: [0, ky], o: [0, -ky] },
  ];
  return verticesPath(shape.d === 3 ? reversed(vertices) : vertices, true);
}

/**
 * A star (`sy` 1) or polygon (`sy` 2), as lottie-web draws it: the first
 * point straight up from the centre, turned by `r`, with each corner's
 * handles set by its roundness along the circle through it.
 */
export function starCommands(shape: LottieShape, frame: number): PathCommand[] {
  const [px, py] = vectorAt(shape.p as LottieValue, frame, [0, 0]);
  const points = Math.floor(scalarAt(shape.pt as LottieValue, frame, 5));
  if (points < 1) {
    return [];
  }
  const star = shape.sy !== 2;
  const outer = scalarAt(shape.or as LottieValue, frame, 0);
  const inner = star ? scalarAt(shape.ir as LottieValue, frame, 0) : outer;
  const outerRound = scalarAt(shape.os as LottieValue, frame, 0) / 100;
  const innerRound = star ? scalarAt(shape.is as LottieValue, frame, 0) / 100 : outerRound;
  const count = star ? points * 2 : points;
  const step = (2 * Math.PI) / count;
  const dir = shape.d === 3 ? -1 : 1;
  let angle = -Math.PI / 2 + scalarAt(shape.r as LottieValue, frame, 0) * radians;
  const vertices: Vertex[] = [];
  for (let n = 0; n < count; n += 1) {
    const isOuter = !star || n % 2 === 0;
    const radius = isOuter ? outer : inner;
    const roundness = isOuter ? outerRound : innerRound;
    const perimeter = (2 * Math.PI * radius) / (count * 2);
    const x = radius * Math.cos(angle);
    const y = radius * Math.sin(angle);
    const length = Math.hypot(x, y);
    const tx = length === 0 ? 0 : y / length;
    const ty = length === 0 ? 0 : -x / length;
    const reach = perimeter * roundness * dir;
    vertices.push({ v: [px + x, py + y], i: [tx * reach, ty * reach], o: [-tx * reach, -ty * reach] });
    angle += step * dir;
  }
  return verticesPath(vertices, true);
}
