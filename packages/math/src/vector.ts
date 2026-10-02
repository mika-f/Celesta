// 2D point helpers. Angles are radians; with Celesta's y-down coordinates a
// positive angle turns clockwise on screen, like a layer's `rotation`.

/** A 2D point or vector. Structurally the same as `@celesta/react`'s `Point`. */
export interface Vec2 {
  x: number;
  y: number;
}

const ORIGIN: Vec2 = { x: 0, y: 0 };

/** The straight-line distance between `a` and `b`. */
export function distance(a: Vec2, b: Vec2): number {
  return Math.hypot(b.x - a.x, b.y - a.y);
}

/** The direction from `from` to `to`, in radians (`0` points along +x). */
export function angleBetween(from: Vec2, to: Vec2): number {
  return Math.atan2(to.y - from.y, to.x - from.x);
}

/** The point `t` of the way from `a` to `b` (`t` is not clamped). */
export function lerpPoint(a: Vec2, b: Vec2, t: number): Vec2 {
  return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
}

/** The point halfway between `a` and `b`. */
export function midpoint(a: Vec2, b: Vec2): Vec2 {
  return lerpPoint(a, b, 0.5);
}

/** `point` turned by `angle` radians about `origin` (the coordinate origin by default). */
export function rotatePoint(point: Vec2, angle: number, origin: Vec2 = ORIGIN): Vec2 {
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  const dx = point.x - origin.x;
  const dy = point.y - origin.y;
  return { x: origin.x + dx * cos - dy * sin, y: origin.y + dx * sin + dy * cos };
}

/** The point `radius` away from `center` in the direction `angle` (radians). */
export function polarToCartesian(angle: number, radius: number, center: Vec2 = ORIGIN): Vec2 {
  return { x: center.x + Math.cos(angle) * radius, y: center.y + Math.sin(angle) * radius };
}

/** `point`'s `angle` (radians) and `radius` as seen from `center`. */
export function cartesianToPolar(point: Vec2, center: Vec2 = ORIGIN): { angle: number; radius: number } {
  return { angle: angleBetween(center, point), radius: distance(center, point) };
}

/** The point at `t` (0–1) along a quadratic Bézier curve, as drawn by a path's `quadTo`. */
export function quadraticBezierPoint(p0: Vec2, p1: Vec2, p2: Vec2, t: number): Vec2 {
  const u = 1 - t;
  return {
    x: u * u * p0.x + 2 * u * t * p1.x + t * t * p2.x,
    y: u * u * p0.y + 2 * u * t * p1.y + t * t * p2.y,
  };
}

/** The point at `t` (0–1) along a cubic Bézier curve, as drawn by a path's `cubicTo`. */
export function cubicBezierPoint(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: number): Vec2 {
  const u = 1 - t;
  const a = u * u * u;
  const b = 3 * u * u * t;
  const c = 3 * u * t * t;
  const d = t * t * t;
  return {
    x: a * p0.x + b * p1.x + c * p2.x + d * p3.x,
    y: a * p0.y + b * p1.y + c * p2.y + d * p3.y,
  };
}
