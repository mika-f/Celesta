// Scalar helpers for shaping animation values.

/** `value` limited to `[min, max]`. */
export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** `value` limited to `[0, 1]`. */
export function clamp01(value: number): number {
  return clamp(value, 0, 1);
}

/** The value `t` of the way from `a` to `b` (`t` is not clamped). */
export function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

/** How far `value` is from `a` to `b`: the `t` for which `lerp(a, b, t)` is `value`. `0` when `a === b`. */
export function inverseLerp(a: number, b: number, value: number): number {
  return a === b ? 0 : (value - a) / (b - a);
}

/**
 * `value` moved from the range `[inMin, inMax]` to `[outMin, outMax]`,
 * without clamping. For eased or clamped mappings over frames, use
 * `@celesta/react`'s `interpolate`.
 */
export function remap(value: number, inMin: number, inMax: number, outMin: number, outMax: number): number {
  return lerp(outMin, outMax, inverseLerp(inMin, inMax, value));
}

/** Like `remap`, but the result stays within `[outMin, outMax]`. */
export function remapClamped(value: number, inMin: number, inMax: number, outMin: number, outMax: number): number {
  return lerp(outMin, outMax, clamp01(inverseLerp(inMin, inMax, value)));
}

/** `0` below `edge`, `1` from `edge` on (GLSL's `step`). */
export function step(edge: number, x: number): number {
  return x < edge ? 0 : 1;
}

/** A smooth 0–1 ramp as `x` goes from `edge0` to `edge1` (GLSL's `smoothstep`). */
export function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = clamp01(inverseLerp(edge0, edge1, x));
  return t * t * (3 - 2 * t);
}

/** Like `smoothstep`, but also flat in its second derivative at both ends (Perlin's quintic). */
export function smootherstep(edge0: number, edge1: number, x: number): number {
  const t = clamp01(inverseLerp(edge0, edge1, x));
  return t * t * t * (t * (t * 6 - 15) + 10);
}

/** The fractional part of `x`, always in `[0, 1)`, also for negative `x`. */
export function fract(x: number): number {
  return x - Math.floor(x);
}

/** `value` modulo `divisor`, with the sign of `divisor` (`mod(-1, 4)` is `3`, not `-1`). */
export function mod(value: number, divisor: number): number {
  return ((value % divisor) + divisor) % divisor;
}

/** `value` wrapped into `[min, max)`, as for looping positions or hues. */
export function wrap(value: number, min: number, max: number): number {
  return min + mod(value - min, max - min);
}

/** `value` bounced back and forth between `0` and `length`. */
export function pingPong(value: number, length: number): number {
  return length - Math.abs(mod(value, 2 * length) - length);
}

/** `value` rounded to the nearest multiple of `increment`, for stepped or grid-locked motion. */
export function snap(value: number, increment: number): number {
  return increment === 0 ? value : Math.round(value / increment) * increment;
}

/** `value` rounded to `decimals` digits after the point. */
export function roundTo(value: number, decimals = 0): number {
  const factor = 10 ** decimals;
  return Math.round(value * factor) / factor;
}

/** Whether `a` and `b` differ by at most `epsilon`. */
export function approxEqual(a: number, b: number, epsilon = 1e-6): boolean {
  return Math.abs(a - b) <= epsilon;
}
