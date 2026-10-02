// Smooth, deterministic noise. Feed it time (`frame / 20`) for drift, wobble,
// or camera shake, or a position for organic textures; different seeds give
// unrelated fields.
import { mix32, requireFinite, seedToInt, streamValue } from './hash';
import type { Seed } from './hash';

/** Perlin's quintic fade: zero first and second derivatives at 0 and 1. */
function fade(t: number): number {
  return t * t * t * (t * (t * 6 - 15) + 10);
}

/**
 * Smooth 1D value noise in `[-1, 1]`: random values at whole numbers of `t`,
 * blended with smoothstep in between. Feed it time (`frame / 20`) for drift,
 * wobble, or camera shake; different seeds give unrelated curves.
 */
export function noise(seed: Seed, t: number): number {
  requireFinite('noise', { t });
  const base = seedToInt(seed, 'noise');
  const i = Math.floor(t);
  const u = t - i;
  const s = u * u * (3 - 2 * u);
  const a = streamValue(base, i);
  const b = streamValue(base, i + 1);
  return (a + (b - a) * s) * 2 - 1;
}

/** The random unit gradient at lattice point (`ix`, `iy`), dotted with the offset to it. */
function gradient2(base: number, ix: number, iy: number, dx: number, dy: number): number {
  const angle = streamValue(base, ix ^ mix32(iy)) * 2 * Math.PI;
  return Math.cos(angle) * dx + Math.sin(angle) * dy;
}

/**
 * Smooth 2D gradient (Perlin) noise in `[-1, 1]`. Sample it at a position
 * scaled down (`x / 200`) for textures and terrain, or add time as a third
 * coordinate with `noise3D` to animate it.
 */
export function noise2D(seed: Seed, x: number, y: number): number {
  requireFinite('noise2D', { x, y });
  const base = seedToInt(seed, 'noise2D');
  const ix = Math.floor(x);
  const iy = Math.floor(y);
  const fx = x - ix;
  const fy = y - iy;
  const u = fade(fx);
  const v = fade(fy);
  const n00 = gradient2(base, ix, iy, fx, fy);
  const n10 = gradient2(base, ix + 1, iy, fx - 1, fy);
  const n01 = gradient2(base, ix, iy + 1, fx, fy - 1);
  const n11 = gradient2(base, ix + 1, iy + 1, fx - 1, fy - 1);
  const top = n00 + (n10 - n00) * u;
  const bottom = n01 + (n11 - n01) * u;
  // Unit gradients peak at ±√2/2 in 2D; scale that to ±1.
  return clampUnit((top + (bottom - top) * v) * Math.SQRT2);
}

/** The random unit gradient at lattice point (`ix`, `iy`, `iz`), dotted with the offset to it. */
function gradient3(base: number, ix: number, iy: number, iz: number, dx: number, dy: number, dz: number): number {
  const cell = ix ^ mix32(iy ^ mix32(iz));
  // A uniform direction on the sphere: z uniform in [-1, 1], longitude uniform.
  const z = streamValue(base, cell) * 2 - 1;
  const longitude = streamValue(base ^ 0x68e31da4, cell) * 2 * Math.PI;
  const r = Math.sqrt(1 - z * z);
  return r * Math.cos(longitude) * dx + r * Math.sin(longitude) * dy + z * dz;
}

/**
 * Smooth 3D gradient (Perlin) noise in `[-1, 1]`. Use two coordinates for
 * position and the third for time (`noise3D(seed, x / 200, y / 200, frame /
 * 60)`) to get a 2D field that evolves smoothly.
 */
export function noise3D(seed: Seed, x: number, y: number, z: number): number {
  requireFinite('noise3D', { x, y, z });
  const base = seedToInt(seed, 'noise3D');
  const ix = Math.floor(x);
  const iy = Math.floor(y);
  const iz = Math.floor(z);
  const fx = x - ix;
  const fy = y - iy;
  const fz = z - iz;
  const u = fade(fx);
  const v = fade(fy);
  const w = fade(fz);
  const corner = (cx: number, cy: number, cz: number) =>
    gradient3(base, ix + cx, iy + cy, iz + cz, fx - cx, fy - cy, fz - cz);
  const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
  const near = lerp(lerp(corner(0, 0, 0), corner(1, 0, 0), u), lerp(corner(0, 1, 0), corner(1, 1, 0), u), v);
  const far = lerp(lerp(corner(0, 0, 1), corner(1, 0, 1), u), lerp(corner(0, 1, 1), corner(1, 1, 1), u), v);
  // Unit gradients peak at ±√3/2 in 3D; scale that to ±1.
  return clampUnit(lerp(near, far, w) * (2 / Math.sqrt(3)));
}

function clampUnit(value: number): number {
  return Math.min(1, Math.max(-1, value));
}

export interface FbmOptions {
  /** How many layers of noise to add. Defaults to 4. */
  octaves?: number;
  /** How much finer each layer is than the one before. Defaults to 2. */
  lacunarity?: number;
  /** How much weaker each layer is than the one before. Defaults to 0.5. */
  gain?: number;
}

function fractal(caller: string, options: FbmOptions, sample: (octave: number, frequency: number) => number): number {
  const { octaves = 4, lacunarity = 2, gain = 0.5 } = options;
  if (!Number.isInteger(octaves) || octaves < 1) {
    throw new Error(`${caller}() requires a whole number of octaves of at least 1`);
  }
  // A negative gain can cancel the summed amplitudes to zero.
  if (!Number.isFinite(gain) || gain < 0) {
    throw new Error(`${caller}() requires a finite, non-negative gain`);
  }
  requireFinite(caller, { lacunarity });
  let sum = 0;
  let total = 0;
  let amplitude = 1;
  let frequency = 1;
  for (let octave = 0; octave < octaves; octave += 1) {
    sum += amplitude * sample(octave, frequency);
    total += amplitude;
    amplitude *= gain;
    frequency *= lacunarity;
  }
  // Dividing by the summed amplitudes keeps the result in [-1, 1].
  return sum / total;
}

/** Each octave gets its own seed so layers do not line up at the origin. */
function octaveSeed(seed: Seed, octave: number): Seed {
  return octave === 0 ? seed : `${seed}:octave${octave}`;
}

/**
 * Fractal 1D noise in `[-1, 1]`: several layers of `noise`, each finer and
 * weaker than the last, for motion that wanders on more than one scale
 * (fractional Brownian motion).
 */
export function fbm(seed: Seed, t: number, options: FbmOptions = {}): number {
  return fractal('fbm', options, (octave, frequency) => noise(octaveSeed(seed, octave), t * frequency));
}

/** Fractal 2D noise in `[-1, 1]`: layered `noise2D` for clouds, smoke, and terrain. */
export function fbm2D(seed: Seed, x: number, y: number, options: FbmOptions = {}): number {
  return fractal('fbm2D', options, (octave, frequency) =>
    noise2D(octaveSeed(seed, octave), x * frequency, y * frequency));
}

/** Fractal 3D noise in `[-1, 1]`: layered `noise3D`, typically a 2D field evolving over time. */
export function fbm3D(seed: Seed, x: number, y: number, z: number, options: FbmOptions = {}): number {
  return fractal('fbm3D', options, (octave, frequency) =>
    noise3D(octaveSeed(seed, octave), x * frequency, y * frequency, z * frequency));
}
