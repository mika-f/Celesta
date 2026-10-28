import { parseHexColor } from '@celesta/math';
import type { Paint } from './types';

type Gradient = Exclude<Paint, { type: 'solid' }>;
/**
 * A 2D affine map in `DOMMatrix` order: `x' = a·x + c·y + e`,
 * `y' = b·x + d·y + f`.
 */
export type Affine = readonly [a: number, b: number, c: number, d: number, e: number, f: number];

/** Values per stop in a stop table: its offset, then its premultiplied `r, g, b` and its alpha, in 0–255. */
const STOP = 5;

/** `Color::from_hex`: `#RRGGBB` or `#RRGGBBAA`. */
function hexBytes(color: string): number[] {
  const channels = parseHexColor(color);
  if (!channels) throw new Error(`Invalid color: ${color}`);
  return channels;
}

/**
 * `resolve_stops`: a stop table (see `STOP`), in offset order. The sort is
 * stable, so stops sharing an offset keep their order (a hard edge).
 */
function resolveStops(stops: Gradient['stops']): Float64Array {
  const sorted = stops.map(stop => ({ offset: stop.offset, color: hexBytes(stop.color) })).sort((x, y) => x.offset - y.offset);
  const table = new Float64Array(sorted.length * STOP);
  sorted.forEach(({ offset, color: [r, g, b, a] }, i) => {
    const alpha = a / 255;
    table.set([offset, r * alpha, g * alpha, b * alpha, a], i * STOP);
  });
  return table;
}

/**
 * `sample_stops`: the straight color at `t`, written to `out` at `at`.
 * Stops interpolate premultiplied, like CSS (Canvas 2D gradients
 * interpolate straight colors instead), so fading to `#00000000` does not
 * darken the color on the way.
 */
function sampleStops(stops: Float64Array, t: number, out: Uint8ClampedArray, at: number) {
  const last = stops.length - STOP;
  // The end stop `t` is at or past, if any.
  const end = last < 0 ? -1 : !(t > stops[0]) ? 0 : t >= stops[last] ? last : -1;
  let r, g, b, alpha;
  if (last < 0) r = g = b = alpha = 0;
  else if (end >= 0) {
    r = stops[end + 1]; g = stops[end + 2]; b = stops[end + 3]; alpha = stops[end + 4];
  } else {
    // The first stop past `t`; the one before it is at or below `t`.
    let to = STOP;
    while (stops[to] <= t) to += STOP;
    const from = to - STOP;
    const span = stops[to] - stops[from];
    const f = span > 0 ? (t - stops[from]) / span : 1;
    r = stops[from + 1] + (stops[to + 1] - stops[from + 1]) * f;
    g = stops[from + 2] + (stops[to + 2] - stops[from + 2]) * f;
    b = stops[from + 3] + (stops[to + 3] - stops[from + 3]) * f;
    alpha = stops[from + 4] + (stops[to + 4] - stops[from + 4]) * f;
  }
  if (alpha <= 0) {
    out[at] = out[at + 1] = out[at + 2] = out[at + 3] = 0;
    return;
  }
  // Rounded explicitly: a `Uint8ClampedArray` rounds halves to even.
  out[at] = Math.min(255, Math.round(r * 255 / alpha));
  out[at + 1] = Math.min(255, Math.round(g * 255 / alpha));
  out[at + 2] = Math.min(255, Math.round(b * 255 / alpha));
  out[at + 3] = Math.min(255, Math.round(alpha));
}

/**
 * Whether Canvas 2D's own gradient draws `paint` as the native renderers
 * do, give or take rounding. Its straight-color interpolation agrees with
 * theirs when every stop has one alpha. But it rejects offsets outside
 * 0–1, which they clamp to, and draws nothing for a gradient with no
 * length or radius, which they fill with its first stop.
 */
export function canvasDrawsAlike(paint: Gradient): boolean {
  const stops = resolveStops(paint.stops);
  for (let at = 0; at < stops.length; at += STOP) {
    if (!(stops[at] >= 0 && stops[at] <= 1) || stops[at + 4] !== stops[4]) return false;
  }
  return paint.type === 'linear'
    ? paint.start.x !== paint.end.x || paint.start.y !== paint.end.y
    : paint.radius > 0;
}

/** The straight RGBA color of `paint` at a point in its local pixels, as the native renderer samples it. */
export function sampleGradient(paint: Gradient, x: number, y: number): number[] {
  // A one-pixel box that every point maps to `x, y`.
  return Array.from(gradientPixels(paint, [0, 0, 0, 0, x, y], [0, 0, 1, 1]));
}

/**
 * The straight RGBA pixels (`ImageData.data`) of `paint` over the device
 * pixel box `[left, top, right, bottom]`, each sampled at its centre mapped
 * into the paint's local pixels by `toLocal`, as the native renderers sample
 * a rect, path or text's gradient.
 */
export function gradientPixels(paint: Gradient, toLocal: Affine, box: readonly [number, number, number, number]): Uint8ClampedArray<ArrayBuffer> {
  const [left, top, right, bottom] = box;
  const width = right - left, height = bottom - top;
  const pixels = new Uint8ClampedArray(width * height * 4);
  // The same pixels, a whole one at a time.
  const whole = new Uint32Array(pixels.buffer);
  const stops = resolveStops(paint.stops);
  // `ResolvedPaint::color_at`'s position along the paint: a point's
  // projection onto a linear one's axis, or its distance from a radial
  // one's centre over its radius; 0 for one with no length or radius.
  const linear = paint.type === 'linear';
  const [originX, originY] = linear ? [paint.start.x, paint.start.y] : [paint.center.x, paint.center.y];
  const dx = linear ? paint.end.x - originX : 0, dy = linear ? paint.end.y - originY : 0;
  const scale = linear ? dx * dx + dy * dy : paint.radius;
  const [a, b, c, d, e, f] = toLocal;
  for (let row = 0, at = 0; row < height; row++) {
    const y = top + row + 0.5;
    let previous = NaN;
    for (let column = 0; column < width; column++, at++) {
      const x = left + column + 0.5;
      const fromX = a * x + c * y + e - originX, fromY = b * x + d * y + f - originY;
      const t = !(scale > 0) ? 0 : linear ? (fromX * dx + fromY * dy) / scale : Math.sqrt(fromX * fromX + fromY * fromY) / scale;
      // Along a gradient's bands (a row of a vertical one), the last color.
      if (t === previous) whole[at] = whole[at - 1];
      else sampleStops(stops, t, pixels, at * 4);
      previous = t;
    }
  }
  return pixels;
}
