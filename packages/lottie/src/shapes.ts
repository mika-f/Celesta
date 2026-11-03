// A shape layer's contents as `<Path>`s and `<Group>`s. Lottie lists a
// group's items top first; each fill or stroke paints every path listed
// before it in its group (nested groups' paths included, through their
// transforms), and a trim path cuts every path before it. Paints listed
// later draw underneath, as in After Effects.

import * as React from 'react';

import { Group } from '@celesta.studio/react';
import type { GradientStop, LineCap, LineJoin, Paint, PathCommand } from '@celesta.studio/react';
import { Path, trimPath } from '@celesta.studio/shapes';

import {
  bezierCommands,
  ellipseCommands,
  groupTransform,
  rectCommands,
  starCommands,
  transformCommands,
  transformMatrix,
  transformOpacity,
} from './geometry';
import { bezierAtFrame, scalarAt, vectorAt } from './property';
import type { LottieShape, LottieShapeProperty, LottieTransform, LottieValue } from './types';

const h = React.createElement;

interface Built {
  /** Drawings, topmost first. */
  elements: React.ReactNode[];
  /** The paths so far, each one a shape, in the group's own coordinates. */
  paths: PathCommand[][];
}

function channel(value: number): string {
  return Math.round(Math.min(1, Math.max(0, value)) * 255).toString(16).padStart(2, '0');
}

/** A Lottie color (`[r, g, b]` or `[r, g, b, a]`, 0–1) as `#RRGGBBAA`. */
export function hexColor(color: readonly number[]): string {
  // Bodymovin before 4.1 wrote 0–255.
  const scale = color.slice(0, 3).some(value => value > 1) ? 255 : 1;
  const [r = 0, g = 0, b = 0, a = 1] = color;
  return `#${channel(r / scale)}${channel(g / scale)}${channel(b / scale)}${channel(a)}`.toUpperCase();
}

/** The stops of a gradient (`g`), its color and opacity stops merged. */
function gradientStops(shape: LottieShape, frame: number): GradientStop[] {
  const g = shape.g as { p: number; k: LottieValue } | undefined;
  if (g === undefined) {
    return [];
  }
  const values = vectorAt(g.k, frame, []);
  const colors: [number, number, number, number][] = [];
  for (let n = 0; n < g.p && n * 4 + 3 < values.length; n += 1) {
    colors.push([values[n * 4], values[n * 4 + 1], values[n * 4 + 2], values[n * 4 + 3]]);
  }
  const alphas: [number, number][] = [];
  for (let n = g.p * 4; n + 1 < values.length; n += 2) {
    alphas.push([values[n], values[n + 1]]);
  }
  if (colors.length === 0) {
    return [];
  }
  const at = <T extends number[]>(stops: T[], offset: number, pick: (stop: T) => number[]): number[] => {
    if (offset <= stops[0][0]) {
      return pick(stops[0]);
    }
    for (let n = 1; n < stops.length; n += 1) {
      if (offset <= stops[n][0]) {
        const [o0] = stops[n - 1];
        const [o1] = stops[n];
        const t = o1 > o0 ? (offset - o0) / (o1 - o0) : 1;
        const a = pick(stops[n - 1]);
        const b = pick(stops[n]);
        return a.map((value, d) => value + (b[d] - value) * t);
      }
    }
    return pick(stops[stops.length - 1]);
  };
  const offsets = [...new Set([...colors.map(([o]) => o), ...alphas.map(([o]) => o)])].sort((a, b) => a - b);
  return offsets.map(offset => {
    const [r, g, b] = at(colors, offset, ([, ...rgb]) => rgb);
    const [a] = alphas.length > 0 ? at(alphas, offset, ([, alpha]) => [alpha]) : [1];
    return { offset: Math.min(1, Math.max(0, offset)), color: hexColor([r, g, b, a]) };
  });
}

/** The `Paint` of a gradient fill or stroke, in its group's coordinates. */
function gradientPaint(shape: LottieShape, frame: number): Paint | undefined {
  const stops = gradientStops(shape, frame);
  if (stops.length === 0) {
    return undefined;
  }
  const [sx = 0, sy = 0] = vectorAt(shape.s as LottieValue, frame, [0, 0]);
  const [ex = 0, ey = 0] = vectorAt(shape.e as LottieValue, frame, [0, 0]);
  if (shape.t === 2) {
    return { type: 'radial', center: { x: sx, y: sy }, radius: Math.hypot(ex - sx, ey - sy), stops };
  }
  return { type: 'linear', start: { x: sx, y: sy }, end: { x: ex, y: ey }, stops };
}

const caps: Record<number, LineCap> = { 1: 'butt', 2: 'round', 3: 'square' };
const joins: Record<number, LineJoin> = { 1: 'miter', 2: 'round', 3: 'bevel' };

/** The `<Path>` a fill or stroke draws over `paths`, or null when it draws nothing. */
function paintElement(shape: LottieShape, paths: PathCommand[][], frame: number, key: string): React.ReactNode {
  const commands = paths.flat();
  if (commands.length === 0) {
    return null;
  }
  const opacity = Math.min(1, Math.max(0, scalarAt(shape.o as LottieValue, frame, 100) / 100));
  if (opacity <= 0) {
    return null;
  }
  const gradient = shape.ty === 'gf' || shape.ty === 'gs';
  const paint = gradient ? gradientPaint(shape, frame) : hexColor(vectorAt(shape.c as LottieValue, frame, [0, 0, 0]));
  if (paint === undefined) {
    return null;
  }
  if (shape.ty === 'fl' || shape.ty === 'gf') {
    return h(Path, { key, commands, fill: paint, opacity });
  }
  const strokeWidth = scalarAt(shape.w as LottieValue, frame, 0);
  if (!(strokeWidth > 0)) {
    return null;
  }
  let dash: number[] | undefined;
  let dashOffset: number | undefined;
  const dashes = shape.d as { n: string; v: LottieValue }[] | undefined;
  if (dashes !== undefined && dashes.length > 0) {
    dash = dashes.filter(d => d.n !== 'o').map(d => Math.max(0, scalarAt(d.v, frame, 0)));
    dashOffset = scalarAt(dashes.find(d => d.n === 'o')?.v, frame, 0);
    if (dash.length === 0 || dash.every(length => length === 0)) {
      dash = undefined;
      dashOffset = undefined;
    }
  }
  return h(Path, {
    key,
    commands,
    stroke: paint,
    strokeWidth,
    cap: caps[shape.lc as number] ?? 'butt',
    join: joins[shape.lj as number] ?? 'miter',
    miterLimit: typeof shape.ml === 'number' ? shape.ml : scalarAt(shape.ml2 as LottieValue, frame, 4),
    dash,
    dashOffset,
    opacity,
  });
}

/**
 * The ranges, 0–1 of the length, a trim path keeps: its start and end
 * turned by its offset, wrapping past the end as two ranges.
 */
export function trimRanges(start: number, end: number, offsetDegrees: number): [number, number][] {
  let s = Math.min(start, end) / 100;
  let e = Math.max(start, end) / 100;
  s = Math.min(1, Math.max(0, s));
  e = Math.min(1, Math.max(0, e));
  if (e - s >= 1) {
    return [[0, 1]];
  }
  if (e <= s) {
    return [];
  }
  const offset = ((offsetDegrees / 360) % 1 + 1) % 1;
  s += offset;
  e += offset;
  if (s >= 1) {
    s -= 1;
    e -= 1;
  }
  return e > 1 ? [[s, 1], [0, e - 1]] : [[s, e]];
}

function trimmed(commands: PathCommand[], ranges: [number, number][]): PathCommand[] {
  if (ranges.length === 1 && ranges[0][0] <= 0 && ranges[0][1] >= 1) {
    return commands;
  }
  return ranges.flatMap(([s, e]) => [...trimPath(commands, s, e)]);
}

function trimPaths(shape: LottieShape, paths: PathCommand[][], frame: number): PathCommand[][] {
  const ranges = trimRanges(
    scalarAt(shape.s as LottieValue, frame, 0),
    scalarAt(shape.e as LottieValue, frame, 100),
    scalarAt(shape.o as LottieValue, frame, 0),
  );
  if (shape.m === 2) {
    // Individually: every path in turn, as one length.
    return [trimmed(paths.flat(), ranges)];
  }
  // Simultaneously: each path over its own length.
  return paths.map(commands => trimmed(commands, ranges));
}

/** The paths and drawings of a list of shape items at `frame`. */
export function buildShapes(items: readonly LottieShape[] | undefined, frame: number, key = 's'): Built {
  const elements: React.ReactNode[] = [];
  let paths: PathCommand[][] = [];
  (items ?? []).forEach((shape, index) => {
    if (shape.hd === true) {
      return;
    }
    const itemKey = `${key}.${index}`;
    switch (shape.ty) {
      case 'gr': {
        const children = (shape.it as LottieShape[] | undefined) ?? [];
        const tr = children.find(child => child.ty === 'tr') as (LottieShape & LottieTransform) | undefined;
        const built = buildShapes(children.filter(child => child.ty !== 'tr'), frame, itemKey);
        const matrix = transformMatrix(tr, frame);
        const transform = groupTransform(matrix);
        if (transform === null) {
          return;
        }
        const opacity = transformOpacity(tr, frame);
        if (built.elements.length > 0 && opacity > 0) {
          elements.push(h(Group, { key: itemKey, ...transform, opacity }, ...[...built.elements].reverse()));
        }
        paths.push(...built.paths.map(commands => transformCommands(commands, matrix)));
        return;
      }
      case 'sh':
        paths.push(bezierCommands(bezierAtFrame(shape.ks as LottieShapeProperty, frame)));
        return;
      case 'rc':
        paths.push(rectCommands(shape, frame));
        return;
      case 'el':
        paths.push(ellipseCommands(shape, frame));
        return;
      case 'sr':
        paths.push(starCommands(shape, frame));
        return;
      case 'fl':
      case 'st':
      case 'gf':
      case 'gs': {
        const element = paintElement(shape, paths, frame, itemKey);
        if (element !== null) {
          elements.push(element);
        }
        return;
      }
      case 'tm':
        paths = trimPaths(shape, paths, frame);
        return;
      default:
        // Unsupported items are reported by `inspectLottie`.
        return;
    }
  });
  return { elements, paths };
}
