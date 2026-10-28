import * as React from 'react';

import { clamp01, random } from '@celesta/math';
import { Mask, Rect } from '@celesta/react';
import type { Paint, PathCommand } from '@celesta/react';
import { Path } from '@celesta/shapes';

import { defineTransition } from './presentation';
import type { TransitionDefinition, TransitionSceneStyle, TransitionSeriesEdge } from './presentation';

/** A point as fractions of the series' width and height: `{ x: 0.5, y: 0.5 }` is its centre. */
export interface TransitionCenter {
  x: number;
  y: number;
}

function number(value: unknown, label: string, min = -Infinity, max = Infinity): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max) {
    const range = max === Infinity ? (min === -Infinity ? '' : ` of at least ${min}`) : ` between ${min} and ${max}`;
    throw new Error(`${label} must be a finite number${range}`);
  }
  return value;
}

function centerOf(value: TransitionCenter | undefined, label: string): TransitionCenter {
  if (value === undefined) return { x: 0.5, y: 0.5 };
  if (value === null || typeof value !== 'object') {
    throw new Error(`${label} must be an object with x and y`);
  }
  return { x: number(value.x, `${label}.x`), y: number(value.y, `${label}.y`) };
}

function oneOf<T extends string>(value: T | undefined, fallback: T, allowed: readonly T[], label: string): T {
  if (value === undefined) return fallback;
  if (!allowed.includes(value)) {
    throw new Error(`${label} must be one of ${allowed.map((each) => JSON.stringify(each)).join(', ')}`);
  }
  return value;
}

function isHorizontal(from: TransitionSeriesEdge): boolean {
  return from === 'left' || from === 'right';
}

/** -1 when the entering scene comes from the left or the top, 1 from the right or the bottom. */
function signOf(from: TransitionSeriesEdge): -1 | 1 {
  return from === 'left' || from === 'top' ? -1 : 1;
}

/** The distance from `(x, y)` to the farthest corner of a `width`×`height` box at the origin. */
function farthestCorner(x: number, y: number, width: number, height: number): number {
  return Math.hypot(Math.max(Math.abs(x), Math.abs(width - x)), Math.max(Math.abs(y), Math.abs(height - y)));
}

const h = React.createElement;

const CROSSFADE = defineTransition({
  name: 'crossfade',
  present: ({ progress }) => ({ enter: { opacity: clamp01(progress) } }),
});

/** The next scene fades in over this one. */
export function crossfade(): TransitionDefinition {
  return CROSSFADE;
}

const SLIDE = defineTransition({
  name: 'slide',
  present: ({ progress, from, width, height }) => {
    // At progress 0 the entering scene would sit one width (or height) off
    // the edge it comes from; the leaving one moves the same distance the
    // other way.
    const sign = signOf(from);
    const along = (offset: number): TransitionSceneStyle =>
      isHorizontal(from) ? { x: offset * width } : { y: offset * height };
    return { enter: along(sign * (1 - progress)), exit: along(-sign * progress) };
  },
});

/** The next scene pushes this one out, from the `from` edge. */
export function slide(): TransitionDefinition {
  return SLIDE;
}

const WIPE = defineTransition({
  name: 'wipe',
  present: ({ progress, from, width, height }) => {
    // The strip from the far edge ends exactly on it: size - size * shown.
    const shown = clamp01(progress);
    const sign = signOf(from);
    if (isHorizontal(from)) {
      const strip = width * shown;
      return { enter: { clip: { x: sign < 0 ? 0 : width - strip, y: 0, width: strip, height } } };
    }
    const strip = height * shown;
    return { enter: { clip: { x: 0, y: sign < 0 ? 0 : height - strip, width, height: strip } } };
  },
});

/** The next scene is revealed behind a straight edge moving away from `from`. */
export function wipe(): TransitionDefinition {
  return WIPE;
}

export interface IrisOptions {
  /** The circle's centre. Defaults to the series' centre, `{ x: 0.5, y: 0.5 }`. */
  center?: TransitionCenter;
  /**
   * `'open'` (default): the next scene appears in a circle that grows until
   * it covers the series. `'close'`: this scene shrinks away into a circle
   * over the next.
   */
  direction?: 'open' | 'close';
  /** Width of the circle's soft edge, in pixels. Defaults to 0, a hard edge. */
  feather?: number;
}

/** The next scene opens out of a circle, or this one closes into one. */
export function iris(options: IrisOptions = {}): TransitionDefinition {
  const center = centerOf(options.center, 'iris center');
  const direction = oneOf(options.direction, 'open', ['open', 'close'], 'iris direction');
  const feather = number(options.feather ?? 0, 'iris feather', 0);
  return defineTransition({
    name: 'iris',
    present: ({ progress, width, height }) => {
      const x = center.x * width;
      const y = center.y * height;
      // Far enough that the circle and its soft edge clear every corner.
      const reach = farthestCorner(x, y, width, height) + feather;
      const shown = clamp01(progress);
      const radius = reach * (direction === 'open' ? shown : 1 - shown);
      const fill: string | Paint =
        feather === 0
          ? '#FFFFFF'
          : {
              type: 'radial',
              center: { x: radius, y: radius },
              radius,
              stops: [
                { offset: 0, color: '#FFFFFF' },
                { offset: Math.max(0, 1 - feather / radius), color: '#FFFFFF' },
                { offset: 1, color: '#FFFFFF00' },
              ],
            };
      const disc = radius > 0
        ? h(Rect, { x: x - radius, y: y - radius, width: radius * 2, height: radius * 2, cornerRadius: radius, fill })
        : null;
      // Closing shows the next scene everywhere but inside the circle.
      return { enter: { mask: h(Mask, { invert: direction === 'close' }, disc) } };
    },
  });
}

export interface ClockWipeOptions {
  /** The point the hand turns about. Defaults to the series' centre, `{ x: 0.5, y: 0.5 }`. */
  center?: TransitionCenter;
  /** Where the hand starts, in degrees clockwise from 12 o'clock. Defaults to 0. */
  startAngle?: number;
  /** The way the hand turns. Defaults to `'clockwise'`. */
  direction?: 'clockwise' | 'counterclockwise';
}

/** The next scene is revealed behind a hand sweeping once around a point. */
export function clockWipe(options: ClockWipeOptions = {}): TransitionDefinition {
  const center = centerOf(options.center, 'clockWipe center');
  const startAngle = number(options.startAngle ?? 0, 'clockWipe startAngle');
  const turn = oneOf(options.direction, 'clockwise', ['clockwise', 'counterclockwise'], 'clockWipe direction') === 'clockwise' ? 1 : -1;
  return defineTransition({
    name: 'clock-wipe',
    present: ({ progress, width, height }) => {
      const x = center.x * width;
      const y = center.y * height;
      const sweep = clamp01(progress) * 360;
      // The sector is a fan of chords of at most 30°. Each stays at least
      // cos(15°) of the radius from the centre, so this radius keeps them
      // all past the farthest corner.
      const reach = farthestCorner(x, y, width, height) / Math.cos(Math.PI / 12) + 1;
      const steps = Math.ceil(sweep / 30);
      const commands: PathCommand[] = [{ type: 'moveTo', x, y }];
      for (let step = 0; step <= steps; step++) {
        const angle = ((startAngle + turn * sweep * (step / steps)) * Math.PI) / 180;
        commands.push({ type: 'lineTo', x: x + reach * Math.sin(angle), y: y - reach * Math.cos(angle) });
      }
      commands.push({ type: 'close' });
      const sector = steps > 0 ? h(Path, { commands, fill: '#FFFFFF' }) : null;
      return { enter: { mask: h(Mask, null, sector) } };
    },
  });
}

export interface ZoomOptions {
  /**
   * `'in'` (default): this scene grows to `scale` times its size as the
   * next one grows from `1 / scale` to its own size and fades in, as if
   * moving forward. `'out'`: this scene shrinks to `1 / scale` as the next
   * one shrinks from `scale` to its own size.
   */
  direction?: 'in' | 'out';
  /** How far each scene zooms. Defaults to 1.5. */
  scale?: number;
  /** The point both scenes zoom about. Defaults to the series' centre, `{ x: 0.5, y: 0.5 }`. */
  center?: TransitionCenter;
}

/** Both scenes zoom the same way about a point while the next one fades in. */
export function zoom(options: ZoomOptions = {}): TransitionDefinition {
  const direction = oneOf(options.direction, 'in', ['in', 'out'], 'zoom direction');
  const scale = number(options.scale ?? 1.5, 'zoom scale', Number.MIN_VALUE);
  const center = centerOf(options.center, 'zoom center');
  const [exitTo, enterFrom] = direction === 'in' ? [scale, 1 / scale] : [1 / scale, scale];
  return defineTransition({
    name: 'zoom',
    present: ({ progress, width, height }) => {
      const x = center.x * width;
      const y = center.y * height;
      // A group scales about its origin; moving it keeps (x, y) in place.
      const about = (factor: number): TransitionSceneStyle => ({ x: x * (1 - factor), y: y * (1 - factor), scale: factor });
      return {
        enter: { ...about(enterFrom + (1 - enterFrom) * progress), opacity: clamp01(progress) },
        exit: about(1 + (exitTo - 1) * progress),
      };
    },
  });
}

export interface DissolveOptions {
  /** The side of each square the scenes change in, in pixels. Defaults to 16. */
  cellSize?: number;
  /** Picks the order the squares change in. Defaults to 0. */
  seed?: number;
}

/** The next scene shows through square by square, in an order from `seed`. */
export function dissolve(options: DissolveOptions = {}): TransitionDefinition {
  const cellSize = number(options.cellSize ?? 16, 'dissolve cellSize', Number.MIN_VALUE);
  const seed = number(options.seed ?? 0, 'dissolve seed');
  // When each square shows, for the last grid size asked for.
  let thresholds: { columns: number; rows: number; values: Float64Array } | null = null;
  const thresholdsFor = (columns: number, rows: number): Float64Array => {
    if (!thresholds || thresholds.columns !== columns || thresholds.rows !== rows) {
      const values = new Float64Array(columns * rows);
      for (let index = 0; index < values.length; index++) values[index] = random(`dissolve:${seed}:${index}`);
      thresholds = { columns, rows, values };
    }
    return thresholds.values;
  };
  return defineTransition({
    name: 'dissolve',
    present: ({ progress, width, height }) => {
      const columns = Math.max(1, Math.ceil(width / cellSize));
      const rows = Math.max(1, Math.ceil(height / cellSize));
      const values = thresholdsFor(columns, rows);
      const shown = clamp01(progress);
      // One rectangle per run of shown squares in a row, all in one path.
      const commands: PathCommand[] = [];
      for (let row = 0; row < rows; row++) {
        const top = row * cellSize;
        const bottom = top + cellSize;
        let start = -1;
        for (let column = 0; column <= columns; column++) {
          const on = column < columns && values[row * columns + column] < shown;
          if (on && start < 0) start = column;
          if (!on && start >= 0) {
            const left = start * cellSize;
            const right = column * cellSize;
            commands.push(
              { type: 'moveTo', x: left, y: top },
              { type: 'lineTo', x: right, y: top },
              { type: 'lineTo', x: right, y: bottom },
              { type: 'lineTo', x: left, y: bottom },
              { type: 'close' },
            );
            start = -1;
          }
        }
      }
      const squares = commands.length > 0 ? h(Path, { commands, fill: '#FFFFFF' }) : null;
      return { enter: { mask: h(Mask, null, squares) } };
    },
  });
}

export interface BlurOptions {
  /** The blur radius each scene reaches at the far end of the transition, in pixels, 0–64. Defaults to 32. */
  amount?: number;
}

/** This scene blurs away as the next one comes into focus and fades in. */
export function blur(options: BlurOptions = {}): TransitionDefinition {
  const amount = number(options.amount ?? 32, 'blur amount', 0, 64);
  return defineTransition({
    name: 'blur',
    present: ({ progress }) => {
      const shown = clamp01(progress);
      return {
        enter: { blur: amount * (1 - shown), opacity: shown },
        exit: { blur: amount * shown },
      };
    },
  });
}

export interface FlipOptions {
  /** How dark a scene gets as it turns edge-on, 0–1. Defaults to 0.4. */
  shade?: number;
  /**
   * Turns the scenes in 3D about the series' centre, seen from this
   * distance in pixels (see `perspective` on `Group`), so the near edge
   * grows and the far one shrinks. Leave it out (the default) to narrow the
   * scenes flat, as seen straight on.
   */
  perspective?: number;
}

/** A transition's `perspective` option: positive and finite, or absent. */
function perspectiveOf(value: number | undefined, label: string): number | undefined {
  return value === undefined ? undefined : number(value, label, Number.MIN_VALUE);
}

/**
 * The series turns over like a card: this scene narrows to its edge, then
 * the next one widens from it. `from` picks the axis: `'left'` and `'right'`
 * turn about the vertical axis, `'top'` and `'bottom'` about the horizontal
 * one, the edge it names coming toward the viewer. Without `perspective`,
 * the two directions on an axis look the same.
 */
export function flip(options: FlipOptions = {}): TransitionDefinition {
  const shade = number(options.shade ?? 0.4, 'flip shade', 0, 1);
  const perspective = perspectiveOf(options.perspective, 'flip perspective');
  return defineTransition({
    name: 'flip',
    present: ({ progress, from, width, height }) => {
      const angle = Math.PI * clamp01(progress);
      const brightness = 1 - shade * Math.sin(angle);
      // How far the face showing has turned from facing the viewer, in
      // degrees: the leaving one up to 90, the entering one back from -90.
      const turned = ((angle < Math.PI / 2 ? angle : angle - Math.PI) * 180) / Math.PI;
      const face: TransitionSceneStyle =
        perspective === undefined
          ? (() => {
              // The face's width as it turns, seen straight on, about the centre.
              const side = Math.abs(Math.cos(angle));
              return isHorizontal(from)
                ? { x: (width / 2) * (1 - side), scaleX: side, brightness }
                : { y: (height / 2) * (1 - side), scaleY: side, brightness };
            })()
          : {
              originX: width / 2,
              originY: height / 2,
              perspective,
              // A positive rotationY turns the right edge away, a positive
              // rotationX the top: either way the `from` edge comes forward.
              ...(isHorizontal(from)
                ? { rotationY: -signOf(from) * turned }
                : { rotationX: signOf(from) * turned }),
              brightness,
            };
      return angle < Math.PI / 2 ? { enter: { opacity: 0 }, exit: face } : { enter: face, exit: { opacity: 0 } };
    },
  });
}

export interface CubeOptions {
  /** How dark a face gets as it turns away, 0–1. Defaults to 0.4. */
  shade?: number;
  /**
   * Turns the faces in 3D, seen from this distance in pixels (see
   * `perspective` on `Group`) from in front of the edge they share, which
   * comes toward the viewer and crosses the series. Leave it out (the
   * default) to see the cube straight on, scaled to span the series.
   */
  perspective?: number;
}

/**
 * The scenes are two faces of a cube turning a quarter turn, the next
 * scene's face coming round from `from`. Seen straight on, the pair is
 * scaled down by the turn so it always spans the series' width (or height).
 */
export function cube(options: CubeOptions = {}): TransitionDefinition {
  const shade = number(options.shade ?? 0.4, 'cube shade', 0, 1);
  const perspective = perspectiveOf(options.perspective, 'cube perspective');
  return defineTransition({
    name: 'cube',
    present: ({ progress, from, width, height }) => {
      const angle = (Math.PI / 2) * clamp01(progress);
      const cos = Math.cos(angle);
      const sin = Math.sin(angle);
      const horizontal = isHorizontal(from);
      const size = horizontal ? width : height;
      const across = horizontal ? height : width;
      const flipped = signOf(from) > 0;
      if (perspective !== undefined) {
        return cubeIn3d({ angle, cos, sin, horizontal, flipped, size, across, perspective, shade });
      }
      // Seen straight on, the faces meet at `edge` from the cube's centre:
      // the leaving face spans `edge` to `edge + size·cos`, the entering one
      // `edge - size·sin` to `edge`, for a face coming from the left or top.
      const edge = (size / 2) * (sin - cos);
      const exitStart = flipped ? -(edge + size * cos) : edge;
      const enterStart = flipped ? -edge : edge - size * sin;
      // Together they span size·(cos + sin); scaling by its inverse fits them.
      const fit = 1 / (cos + sin);
      const face = (start: number, extent: number, turned: number): TransitionSceneStyle => {
        const along = size / 2 + fit * start;
        const offset = across / 2 - (fit * across) / 2;
        return {
          ...(horizontal
            ? { x: along, y: offset, scaleX: fit * extent, scaleY: fit }
            : { x: offset, y: along, scaleX: fit, scaleY: fit * extent }),
          brightness: 1 - shade * turned,
        };
      };
      return { enter: face(enterStart, sin, cos), exit: face(exitStart, cos, sin) };
    },
  });
}

/**
 * `cube` in 3D. Each face turns into the screen about the edge the two
 * share, which is also the vanishing point, so both are seen from one
 * place: the leaving face by `angle`, the entering one by the rest of the
 * quarter turn. The edge crosses the series so that what is left uncovered
 * either side of the pair is in proportion to the faces' drawn sizes,
 * starting on the `from` edge and ending on the other.
 */
function cubeIn3d({
  angle,
  cos,
  sin,
  horizontal,
  flipped,
  size,
  across,
  perspective,
  shade,
}: {
  angle: number;
  cos: number;
  sin: number;
  horizontal: boolean;
  flipped: boolean;
  size: number;
  across: number;
  perspective: number;
  shade: number;
}): { enter: TransitionSceneStyle; exit: TransitionSceneStyle } {
  // How wide each face is drawn: its far edge, `size` away from the shared
  // one, recedes by `size` times the sine of its turn.
  const drawn = (turnCos: number, turnSin: number) => (size * turnCos * perspective) / (perspective + size * turnSin);
  const leaving = drawn(cos, sin);
  const entering = drawn(sin, cos);
  const total = leaving + entering;
  // How far the shared edge has come from the `from` side of the series.
  const travelled = total > 0 ? (size * entering) / total : 0;
  const edge = flipped ? size - travelled : travelled;
  const degrees = (angle * 180) / Math.PI;
  // A face `after` the edge (to its right, or below it) has its own near
  // side (0) on it and turns its far side away with a positive rotationY
  // or a negative rotationX; a face before the edge has its far side
  // (`size`) on it and turns the other way.
  const face = (after: boolean, by: number, turned: number): TransitionSceneStyle => {
    const pivot = after ? 0 : size;
    const sign = after ? 1 : -1;
    return {
      ...(horizontal
        ? { x: edge - pivot, originX: pivot, originY: across / 2, rotationY: sign * by }
        : { y: edge - pivot, originX: across / 2, originY: pivot, rotationX: -sign * by }),
      perspective,
      brightness: 1 - shade * turned,
    };
  };
  // The entering face comes from `from`, before the edge from the left or top.
  return { enter: face(flipped, 90 - degrees, cos), exit: face(!flipped, degrees, sin) };
}

/** The built-in transitions by the names `<TransitionSeries.Transition type>` takes, with their defaults. */
export const BUILT_IN_TRANSITIONS = {
  crossfade: CROSSFADE,
  slide: SLIDE,
  wipe: WIPE,
  iris: iris(),
  'clock-wipe': clockWipe(),
  zoom: zoom(),
  dissolve: dissolve(),
  blur: blur(),
  flip: flip(),
  cube: cube(),
} as const satisfies Record<string, TransitionDefinition>;
