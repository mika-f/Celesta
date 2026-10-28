// A repeating background — dots, a grid, stripes, or marks of your own in
// every cell — filling a rectangle and clipped to it. Every mark of a shape
// is one particle of a single <Instances> layer, so a pattern costs a few
// layers however many marks it draws.

import * as React from 'react';

import { parseHexColor, random } from '@celesta/math';
import type { Seed } from '@celesta/math';
import { Group, Instances, Rect, useCurrentFrame, useTheme, useVideoConfig } from '@celesta/react';
import type { BlendMode } from '@celesta/react';

import { alphaOf, hexColor } from './color';
import { validators } from './validate';

/** The patterns `<PatternBackground>` draws by name. */
export type PatternName = 'dots' | 'grid' | 'stripes';

/** One cell of the pattern, as a custom pattern's `marks` function sees it. */
export interface PatternCell {
  /**
   * Column and row of the cell, counted from the cell at the area's top-left
   * corner when the pattern is not moved. A cell keeps its column and row as
   * the pattern moves, so marks computed from them move with it.
   */
  column: number;
  row: number;
  /** Top-left corner of the cell, in the area's coordinates (0, 0 is the area's top-left). */
  x: number;
  y: number;
}

/** One mark of a custom pattern, placed inside its cell. */
export interface PatternMark {
  /** Defaults to `'rect'`. */
  shape?: 'rect' | 'circle';
  /** Centre of the mark from the cell's top-left corner. Defaults to the cell's centre. */
  x?: number;
  y?: number;
  width: number;
  /** Defaults to `width`. */
  height?: number;
  /** Degrees, clockwise, about the mark's centre. */
  rotation?: number;
  /** A hex color or a `<Theme>` color name. Defaults to the pattern's `color`. */
  color?: string;
  /** Multiplies the mark's alpha, 0–1. Defaults to 1. */
  opacity?: number;
}

/**
 * A pattern of your own: the marks every cell of `spacing` draws. A function
 * gets each cell and returns its marks, to vary them from cell to cell.
 * Marks may reach into the neighboring cells, up to one cell beyond their own.
 */
export interface PatternTile {
  marks: readonly PatternMark[] | ((cell: PatternCell) => readonly PatternMark[]);
}

/** Random variation of each mark, the same for the same `seed` on every frame. */
export interface PatternVariation {
  /** How much a mark's alpha may drop, 0–1: a mark keeps 1 − `opacity` to all of it. */
  opacity?: number;
  /** How much a mark may shrink, 0–1: a mark keeps 1 − `scale` to all of its size. */
  scale?: number;
  /** Defaults to 0; another seed varies the marks differently. */
  seed?: Seed;
}

export interface PatternBackgroundProps {
  /** `'dots'`, `'grid'`, `'stripes'`, or a `PatternTile` of your own marks. Defaults to `'dots'`. */
  pattern?: PatternName | PatternTile;
  /** Left edge of the area to fill. Defaults to 0. */
  x?: number;
  /** Top edge of the area to fill. Defaults to 0. */
  y?: number;
  /** Defaults to the canvas width. */
  width?: number;
  /** Defaults to the canvas height. */
  height?: number;
  /** Rounds the area's corners. Defaults to 0. */
  cornerRadius?: number;
  /**
   * Distance from one mark to the next, in pixels: a number for both
   * directions, or `[across, down]`. The stripes use the first. Defaults to 40.
   */
  spacing?: number | readonly [number, number];
  /**
   * Diameter of a dot, width of a grid line, or thickness of a stripe.
   * Defaults to 4 for dots, 1 for a grid, and half of `spacing` for stripes.
   */
  size?: number;
  /** `'square'` draws the dots as squares. Defaults to `'circle'`. */
  dotShape?: 'circle' | 'square';
  /** Degrees, clockwise from horizontal, of the stripes. Defaults to 45. */
  angle?: number;
  /** A hex color or a `<Theme>` color name for the area beneath the pattern. Omit for none. */
  background?: string;
  /** A hex color or a `<Theme>` color name for the marks. Defaults to `#ffffff`. */
  color?: string;
  /** Opacity of the marks, as one layer, so crossing grid lines do not darken. Defaults to 1. */
  opacity?: number;
  /** Blends the marks over the background and what lies beneath. */
  blendMode?: BlendMode;
  /** Moves the pattern right, in pixels. */
  offsetX?: number;
  /** Moves the pattern down, in pixels. */
  offsetY?: number;
  /** Moves the pattern right this many pixels per second, from the caller's frame 0. */
  speedX?: number;
  /** Moves the pattern down this many pixels per second, from the caller's frame 0. */
  speedY?: number;
  /** Varies each mark's opacity and size at random, the same on every frame. */
  variation?: PatternVariation;
  /** Throws when the area would need more marks than this. Defaults to 100,000. */
  maxMarks?: number;
}

/** The marks of one shape, as the columns of an `<Instances>`. */
interface Batch {
  x: number[];
  y: number[];
  width: number[];
  height: number[];
  rotation: number[];
  color: number[];
}

const emptyBatch = (): Batch => ({ x: [], y: [], width: [], height: [], rotation: [], color: [] });

const { fail, finite, positive } = validators('PatternBackground');

function rgb(color: string): [number, number, number] {
  const [r, g, b] = parseHexColor(color)!;
  return [r, g, b];
}

/** Whole numbers from `first` to `last`, both included. */
function range(first: number, last: number): number[] {
  const out: number[] = [];
  for (let i = first; i <= last; i += 1) out.push(i);
  return out;
}

/**
 * A pattern repeated over a rectangle, the canvas by default, and clipped to
 * it: dots in the middle of each `spacing` cell, grid lines along the cell
 * edges, or stripes at `angle`. Pass `{ marks }` to draw marks of your own in
 * every cell. `offsetX`/`offsetY` and `speedX`/`speedY` move it, wrapping
 * without gaps at the edges; `variation` varies each mark at random.
 *
 * ```tsx
 * <PatternBackground pattern="dots" spacing={80} size={3}
 *   background="#101820" color="#FFFFFF" opacity={0.12} />
 * ```
 */
export function PatternBackground(props: PatternBackgroundProps): ReturnType<typeof React.createElement> {
  const frame = useCurrentFrame();
  const { width: canvasWidth, height: canvasHeight, fps } = useVideoConfig();
  const theme = useTheme();
  const {
    pattern = 'dots', x = 0, y = 0, width = canvasWidth, height = canvasHeight, cornerRadius = 0,
    spacing = 40, dotShape = 'circle', angle = 45,
    background, color = '#ffffff', opacity = 1, blendMode,
    offsetX = 0, offsetY = 0, speedX = 0, speedY = 0, variation, maxMarks = 100_000,
  } = props;

  const [across, down] = typeof spacing === 'number' ? [spacing, spacing] : spacing;
  positive('spacing', across);
  positive('spacing', down);
  for (const [name, value] of Object.entries({ x, y, width, height, offsetX, offsetY, speedX, speedY })) {
    finite(name, value);
  }
  if (width < 0 || height < 0) fail(`width and height must not be negative, not ${width}×${height}`);

  const ink = hexColor(color, theme, 'PatternBackground', 'color');
  const fill = background === undefined ? undefined : hexColor(background, theme, 'PatternBackground', 'background');
  // How far the pattern has moved. Cells keep their column and row as it
  // moves, so `variation` and custom marks move with them.
  const shiftX = offsetX + (speedX * frame) / fps;
  const shiftY = offsetY + (speedY * frame) / fps;

  const seed = variation?.seed ?? 0;
  const opacityJitter = variation?.opacity ?? 0;
  const scaleJitter = variation?.scale ?? 0;
  const vary = opacityJitter > 0 || scaleJitter > 0;
  /** The alpha and size factors of the mark with `key`. */
  const jitter = (key: string): [number, number] => vary
    ? [1 - opacityJitter * random(`${String(seed)}:o:${key}`), 1 - scaleJitter * random(`${String(seed)}:s:${key}`)]
    : [1, 1];

  const batches = { rect: emptyBatch(), circle: emptyBatch() };
  let count = 0;
  const checkCount = (more: number) => {
    count += more;
    if (count > maxMarks) {
      fail(`would draw more than ${maxMarks} marks over ${width}×${height}; raise spacing or maxMarks`);
    }
  };
  const push = (shape: 'rect' | 'circle', mx: number, my: number, w: number, h: number, rotation: number, rgba: readonly number[]) => {
    const batch = batches[shape];
    batch.x.push(mx);
    batch.y.push(my);
    batch.width.push(w);
    batch.height.push(h);
    batch.rotation.push(rotation);
    batch.color.push(...rgba);
  };
  const inkRgb = rgb(ink);
  const solid = (alpha: number): number[] => [...inkRgb, Math.round(Math.min(1, Math.max(0, alpha)) * 255)];

  // Cells that reach the area, and one ring more for marks that spill over.
  const columns = (phase: number) => range(Math.floor(-phase / across) - 1, Math.floor((width - phase) / across) + 1);
  const rows = (phase: number) => range(Math.floor(-phase / down) - 1, Math.floor((height - phase) / down) + 1);

  if (pattern === 'dots') {
    const size = positive('size', props.size ?? 4);
    const cols = columns(shiftX);
    const rws = rows(shiftY);
    checkCount(cols.length * rws.length);
    for (const row of rws) {
      for (const column of cols) {
        const [alpha, scale] = jitter(`${column},${row}`);
        push(dotShape === 'square' ? 'rect' : 'circle',
          shiftX + (column + 0.5) * across, shiftY + (row + 0.5) * down, size * scale, size * scale, 0, solid(alpha));
      }
    }
  } else if (pattern === 'grid') {
    const size = positive('size', props.size ?? 1);
    const cols = columns(shiftX);
    const rws = rows(shiftY);
    checkCount(cols.length + rws.length);
    for (const column of cols) {
      const [alpha, scale] = jitter(`c${column}`);
      push('rect', shiftX + column * across, height / 2, size * scale, height + size * 2, 0, solid(alpha));
    }
    for (const row of rws) {
      const [alpha, scale] = jitter(`r${row}`);
      push('rect', width / 2, shiftY + row * down, width + size * 2, size * scale, 0, solid(alpha));
    }
  } else if (pattern === 'stripes') {
    const size = positive('size', props.size ?? across / 2);
    finite('angle', angle);
    const theta = (angle * Math.PI) / 180;
    // Stripe k is the band centred on the line of points p with p·n = k·spacing + shift·n.
    const nx = -Math.sin(theta);
    const ny = Math.cos(theta);
    const corners = [0, width * nx, height * ny, width * nx + height * ny];
    const phase = shiftX * nx + shiftY * ny;
    const first = Math.floor((Math.min(...corners) - size - phase) / across);
    const last = Math.ceil((Math.max(...corners) + size - phase) / across);
    const length = Math.abs(width * Math.cos(theta)) + Math.abs(height * Math.sin(theta)) + size * 2;
    const cx = width / 2;
    const cy = height / 2;
    const centre = cx * nx + cy * ny;
    checkCount(last - first + 1);
    for (const k of range(first, last)) {
      const [alpha, scale] = jitter(`s${k}`);
      const t = k * across + phase - centre;
      push('rect', cx + t * nx, cy + t * ny, length, size * scale, angle, solid(alpha));
    }
  } else if (typeof pattern === 'object' && pattern !== null && 'marks' in pattern) {
    const cols = columns(shiftX);
    const rws = rows(shiftY);
    const colorCache = new Map<string, string>();
    const markColor = (value: string | undefined): string => {
      if (value === undefined) return ink;
      let resolved = colorCache.get(value);
      if (resolved === undefined) {
        resolved = hexColor(value, theme, 'PatternBackground', 'mark color');
        colorCache.set(value, resolved);
      }
      return resolved;
    };
    for (const row of rws) {
      for (const column of cols) {
        const cell: PatternCell = { column, row, x: shiftX + column * across, y: shiftY + row * down };
        const marks = typeof pattern.marks === 'function' ? pattern.marks(cell) : pattern.marks;
        checkCount(marks.length);
        marks.forEach((mark, index) => {
          const [alpha, scale] = jitter(`${column},${row},${index}`);
          const markInk = markColor(mark.color);
          const w = finite('mark width', mark.width);
          const h = finite('mark height', mark.height ?? w);
          push(mark.shape ?? 'rect',
            cell.x + (mark.x ?? across / 2), cell.y + (mark.y ?? down / 2),
            Math.max(0, w * scale), Math.max(0, h * scale), mark.rotation ?? 0,
            [...rgb(markInk), Math.round(Math.min(1, Math.max(0, alphaOf(markInk) * (mark.opacity ?? 1) * alpha)) * 255)]);
        });
      }
    }
  } else {
    fail(`pattern must be "dots", "grid", "stripes", or { marks }, not ${JSON.stringify(pattern)}`);
  }

  const layers = (['rect', 'circle'] as const)
    .filter((shape) => batches[shape].x.length > 0)
    .map((shape) => {
      const batch = batches[shape];
      return React.createElement(Instances, {
        key: shape,
        shape,
        count: batch.x.length,
        x: Float32Array.from(batch.x),
        y: Float32Array.from(batch.y),
        width: Float32Array.from(batch.width),
        height: Float32Array.from(batch.height),
        rotation: Float32Array.from(batch.rotation),
        color: Uint8Array.from(batch.color),
      });
    });

  // Built-in marks are drawn opaque (but for `variation`) and faded by the
  // group, in the color's alpha and `opacity`, so crossing grid lines do not
  // darken; custom marks carry their own alpha.
  const marksOpacity = typeof pattern === 'object' ? opacity : opacity * alphaOf(ink);
  return React.createElement(Group, { x, y, clip: { width, height, cornerRadius } },
    fill === undefined ? null : React.createElement(Rect, { key: 'background', width, height, fill }),
    layers.length > 0 && marksOpacity > 0
      ? React.createElement(Group, { key: 'marks', opacity: marksOpacity, blendMode }, layers)
      : null,
  );
}
