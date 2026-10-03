// Angle helpers. Functions that take or return angles use radians; convert a
// layer's `rotation` (degrees) with `degToRad`.
import { lerp, mod } from './scalar';

/** A full turn in radians (2π). */
export const TAU = Math.PI * 2;

/** Degrees to radians. */
export function degToRad(degrees: number): number {
  return (degrees * Math.PI) / 180;
}

/** Radians to degrees. */
export function radToDeg(radians: number): number {
  return (radians * 180) / Math.PI;
}

/** `angle` wrapped into `[-π, π)`. */
export function normalizeAngle(angle: number): number {
  return mod(angle + Math.PI, TAU) - Math.PI;
}

/** The shortest signed turn from `from` to `to`, in `[-π, π)`. */
export function angleDifference(from: number, to: number): number {
  return normalizeAngle(to - from);
}

/** The angle `t` of the way from `a` to `b`, turning the short way round. */
export function lerpAngle(a: number, b: number, t: number): number {
  return lerp(a, a + angleDifference(a, b), t);
}
