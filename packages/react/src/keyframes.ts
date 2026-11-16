// Frame-based authoring for the project format's keyframed numbers. `<Audio>`
// and `<Dialogue>` take `volume`/`playbackRate` as an `Animatable<number>`
// whose keyframe times are seconds on the enclosing sequence's clock; this
// builds one from frame numbers so fades line up with the frames scenes are
// laid out in.

import { EASING_FACTORY_NAMES, Easings } from './animation';
import type { Easing, KeyframeAnimation } from './scene';
import { secondsToTime } from './time';

/** One key of `keyframes()`: a number, or an object of numbers that move together. */
export interface ValueKeyframe<T extends number | Record<string, number>> {
  /** Frame the key sits on. May be fractional. */
  frame: number;
  value: T;
  /**
   * Curve of the segment that ends at this key: a function, or the name of
   * an `Easings` curve as JSON keyframes spell it (`'ease-out-back'`).
   * Defaults to linear.
   */
  easing?: Easing | ((t: number) => number);
}

/**
 * The value at `frame` of a list of keys, each segment eased by the key it
 * ends on. For several numbers that move together (a camera's `x`, `y`, and
 * `zoom`), give every key an object with the same fields.
 *
 * Before the first key the value is the first key's, and after the last key
 * the last key's. Two keys with the same value hold it between them. Two
 * keys on the same frame cut: frames before it end on the first key's
 * value, and that frame and later ones use the second, as a
 * `<Sequence from>` starting there would. A third key on one frame would
 * never be seen, so it throws. Keys must be in order of `frame`.
 *
 * ```tsx
 * const cam = keyframes(useCurrentFrame(), [
 *   { frame: 0,   value: { x: 960, y: 540, zoom: 1 } },
 *   { frame: 100, value: { x: 880, y: 520, zoom: 1.12 }, easing: Easings.easeInOutCubic },
 *   { frame: 120, value: { x: 880, y: 520, zoom: 1.12 } },  // a hold
 *   { frame: 120, value: { x: 300, y: 400, zoom: 1 } },     // a cut
 * ]);
 * <Camera {...cam}>…</Camera>
 * ```
 *
 * For `<Audio volume>` and other props that take keyframes themselves, use
 * `frameKeyframes()`.
 */
export function keyframes<T extends number | Record<string, number>>(
  frame: number,
  keys: readonly ValueKeyframe<T>[],
): T {
  if (!Number.isFinite(frame)) {
    throw new Error('keyframes() requires a finite frame');
  }
  if (keys.length === 0) {
    throw new Error('keyframes() requires at least one key');
  }
  const shape = valueShape(keys[0].value);
  keys.forEach((key, index) => {
    if (!Number.isFinite(key.frame)) {
      throw new Error(`keyframes() requires a finite frame (key ${index})`);
    }
    if (valueShape(key.value) !== shape) {
      throw new Error(`keyframes() requires every value to have the same shape (key ${index})`);
    }
    const numbers = typeof key.value === 'number' ? [key.value] : Object.values(key.value);
    if (!numbers.every((n) => Number.isFinite(n))) {
      throw new Error(`keyframes() requires a finite value (key ${index})`);
    }
    if (index > 0 && key.frame < keys[index - 1].frame) {
      throw new Error(`keyframes() requires keys in order of frame (key ${index})`);
    }
    if (index > 1 && key.frame === keys[index - 2].frame) {
      throw new Error(`keyframes() allows at most two keys on one frame (key ${index})`);
    }
    if (typeof key.easing === 'string') namedEasing(key.easing, index);
  });

  if (frame < keys[0].frame) return copyValue(keys[0].value);
  // The last key at or before `frame` starts the segment, so of two keys on
  // one frame the second does.
  let start = 0;
  while (start + 1 < keys.length && keys[start + 1].frame <= frame) start += 1;
  const from = keys[start];
  const to = keys[start + 1];
  if (!to) return copyValue(from.value);
  const easing = typeof to.easing === 'string' ? namedEasing(to.easing, start + 1) : to.easing;
  const linear = (frame - from.frame) / (to.frame - from.frame);
  const eased = easing ? easing(linear) : linear;
  const mix = (a: number, b: number): number => (eased === 0 ? a : a + (b - a) * eased);
  if (typeof from.value === 'number') return mix(from.value, to.value as number) as T;
  const fromFields = from.value as Record<string, number>;
  const toFields = to.value as Record<string, number>;
  return Object.fromEntries(Object.keys(fromFields).map((field) => [field, mix(fromFields[field], toFields[field])])) as T;
}

/** `'number'`, or an object's sorted field names, to compare keys' values by. */
function valueShape(value: unknown): string {
  if (typeof value === 'number') return 'number';
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return 'invalid';
  return `{${Object.keys(value).sort().join(',')}}`;
}

function copyValue<T extends number | Record<string, number>>(value: T): T {
  return typeof value === 'number' ? value : ({ ...(value as Record<string, number>) } as T);
}

/** The `Easings` curve a kebab-case `Easing` name stands for. */
function namedEasing(name: string, index: number): (t: number) => number {
  const key = name.replace(/-([a-z])/g, (_, letter: string) => letter.toUpperCase());
  const curve = (EASING_FACTORY_NAMES as readonly string[]).includes(key) || !Object.prototype.hasOwnProperty.call(Easings, key)
    ? undefined
    : (Easings as unknown as Record<string, (t: number) => number>)[key];
  if (!curve || name !== name.toLowerCase()) {
    throw new Error(`keyframes() got an unknown easing "${name}" (key ${index})`);
  }
  return curve;
}

export interface FrameKeyframe {
  /** Frame the key sits on, on the same clock as `origin`. May be fractional. */
  frame: number;
  value: number;
  /** Curve of the segment that ends at this key. Defaults to `'linear'`. */
  easing?: Easing;
}

export interface FrameKeyframesOptions {
  fps: number;
  /**
   * The frame, on the keys' clock, that the `<Audio>`'s enclosing sequence
   * starts on. Defaults to 0, which means the keys already count from that
   * sequence's start. Pass the sequence's start when the keys count from the
   * composition's start (or from any outer sequence's), for example the sum
   * of the `from`s of nested sequences.
   */
  origin?: number;
}

/**
 * Builds keyframes for `<Audio volume>` (or any `Animatable<number>` prop)
 * from frame numbers. Each key's time is `(frame - origin) / fps` seconds,
 * rounded to the nearest microsecond — the timescale `<Sequence>` offsets
 * use, and finer than one audio sample, so a key lands on its frame at any
 * fps, including fractional ones such as 29.97.
 *
 * Keys must be in order of `frame`. Two keys on the same frame make a cut:
 * the first value holds up to that frame and the second applies after it.
 * A third key on one frame would never be heard, so it throws.
 * Keys before `origin` are allowed; before the first key the value holds at
 * the first key's value, and after the last key at the last key's value.
 *
 * ```tsx
 * const { fps } = useVideoConfig();
 * <Audio src="./bgm.wav" volume={frameKeyframes([
 *   { frame: 0, value: 0 },
 *   { frame: 15, value: 0.8, easing: 'ease-out' },
 * ], { fps })} />
 * ```
 */
export function frameKeyframes(
  keys: readonly FrameKeyframe[],
  options: FrameKeyframesOptions,
): KeyframeAnimation<number> {
  const { fps, origin = 0 } = options;
  if (!Number.isFinite(fps) || fps <= 0) {
    throw new Error('frameKeyframes() requires a positive fps');
  }
  if (!Number.isFinite(origin)) {
    throw new Error('frameKeyframes() requires a finite origin');
  }
  if (keys.length === 0) {
    throw new Error('frameKeyframes() requires at least one key');
  }
  return {
    type: 'keyframes',
    keyframes: keys.map((key, index) => {
      if (!Number.isFinite(key.frame)) {
        throw new Error(`frameKeyframes() requires a finite frame (key ${index})`);
      }
      if (!Number.isFinite(key.value)) {
        throw new Error(`frameKeyframes() requires a finite value (key ${index})`);
      }
      if (index > 0 && key.frame < keys[index - 1].frame) {
        throw new Error(`frameKeyframes() requires keys in order of frame (key ${index})`);
      }
      if (index > 1 && key.frame === keys[index - 2].frame) {
        throw new Error(`frameKeyframes() allows at most two keys on one frame (key ${index})`);
      }
      const time = secondsToTime((key.frame - origin) / fps);
      return key.easing === undefined ? { time, value: key.value } : { time, value: key.value, easing: key.easing };
    }),
  };
}
