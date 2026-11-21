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
  /**
   * How fast the value creeps while it holds after this key, per second:
   * a rate for a number, or a rate per field for an object (fields left out
   * stay still). A field may drift only while it holds: up to the next key
   * if that key has the same value for it, or after the last key. Needs
   * `keyframes()`'s `fps` option.
   */
  drift?: KeyframeDrift<T>;
}

/**
 * A drift rate per second: a number is added (`-4` moves 4 px to the left
 * each second), and `{ log }` multiplies by `e^log` each second, for a
 * zoom that creeps at an even pace (`{ log: 0.006 }` is about 0.6% a
 * second).
 */
export type DriftRate = number | { log: number };

/** The `drift` of a key whose value is `T`: one rate, or a rate per field. */
export type KeyframeDrift<T> = T extends number ? DriftRate : { [Field in keyof T]?: DriftRate };

export interface KeyframesOptions {
  /** Frames per second, to turn `drift`'s rates into frames. Required when a key drifts. */
  fps?: number;
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
 * A key's `drift` creeps a held value at a rate per second while it holds
 * after that key, so a hold still breathes; pass `{ fps }`. The move after
 * the hold starts from where the drift reached, and a later hold of the
 * same value keeps it, so nothing jumps; a cut starts from its own key.
 *
 * ```tsx
 * const { fps } = useVideoConfig();
 * keyframes(frame, [
 *   { frame: 100, value: { x: 880, y: 520, zoom: 1.12 }, drift: { x: -4, zoom: { log: 0.006 } } },
 *   { frame: 160, value: { x: 880, y: 520, zoom: 1.12 } },  // the drift runs to here
 *   { frame: 180, value: { x: 300, y: 400, zoom: 1 }, easing: Easings.whip },
 * ], { fps });
 * ```
 *
 * For `<Audio volume>` and other props that take keyframes themselves, use
 * `frameKeyframes()`.
 */
export function keyframes<K extends ValueKeyframe<number | Record<string, number>>>(
  frame: number,
  keys: readonly K[],
  options: KeyframesOptions = {},
): KeyframeValue<K['value']> {
  // The type comes from every key, so keys declared `as const` (each with
  // its own literal values) are accepted; the shapes are checked here.
  return valueAt(frame, keys, options) as KeyframeValue<K['value']>;
}

/**
 * What `keyframes()` returns for keys whose values are `T`: a number, or an
 * object of the same fields as numbers, never the keys' literal values,
 * since the result lies between them.
 */
export type KeyframeValue<T> = T extends number ? number : { -readonly [Field in keyof T]: number };

type Fields = Record<string, number>;
type Rates = Record<string, DriftRate>;

function valueAt<T extends number | Record<string, number>>(
  frame: number,
  keys: readonly ValueKeyframe<T>[],
  options: KeyframesOptions,
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
    if (valueShape(key.value) === 'invalid') {
      throw new Error(`keyframes() requires each value to be a number or an object of numbers (key ${index})`);
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
  // Every key's value as fields, a number being the one field `''`.
  const values = keys.map((key) => asFields(key.value));
  const rates = keys.map((key, index) => driftRates(key, index, keys[index + 1], options.fps));
  const fps = options.fps as number;
  const result = (fields: Fields): T => (typeof keys[0].value === 'number' ? fields[''] : fields) as T;

  if (frame < keys[0].frame) return result({ ...values[0] });
  // The last key at or before `frame` starts the segment, so of two keys on
  // one frame the second does.
  let start = 0;
  while (start + 1 < keys.length && keys[start + 1].frame <= frame) start += 1;
  // Where each segment starts: its key's value, or, when an earlier hold
  // drifted, where that drift reached. A cut starts from its own key.
  let from = values[0];
  for (let index = 0; index < start; index += 1) {
    const next = index + 1;
    if (keys[next].frame === keys[index].frame) {
      from = values[next];
      continue;
    }
    const seconds = (keys[next].frame - keys[index].frame) / fps;
    from = mapFields(from, (field, value) =>
      values[index][field] === values[next][field] ? drifted(value, rates[index][field], seconds) : values[next][field]);
  }
  const to = keys[start + 1];
  const seconds = (frame - keys[start].frame) / fps;
  if (!to) return result(mapFields(from, (field, value) => drifted(value, rates[start][field], seconds)));
  const easing = typeof to.easing === 'string' ? namedEasing(to.easing, start + 1) : to.easing;
  const linear = (frame - keys[start].frame) / (to.frame - keys[start].frame);
  const eased = easing ? easing(linear) : linear;
  const target = values[start + 1];
  return result(mapFields(from, (field, value) => {
    if (values[start][field] === target[field]) return drifted(value, rates[start][field], seconds);
    return eased === 0 ? value : value + (target[field] - value) * eased;
  }));
}

function asFields(value: number | Record<string, number>): Fields {
  return typeof value === 'number' ? { '': value } : value;
}

function mapFields(fields: Fields, map: (field: string, value: number) => number): Fields {
  return Object.fromEntries(Object.keys(fields).map((field) => [field, map(field, fields[field])]));
}

/** `value` after drifting at `rate` for `seconds`; `value` itself without a rate. */
function drifted(value: number, rate: DriftRate | undefined, seconds: number): number {
  if (rate === undefined) return value;
  return typeof rate === 'number' ? value + rate * seconds : value * Math.exp(rate.log * seconds);
}

/** A key's drift as a rate per field (`''` for a number), checked against the key after it. */
function driftRates<T extends number | Record<string, number>>(
  key: ValueKeyframe<T>,
  index: number,
  next: ValueKeyframe<T> | undefined,
  fps: number | undefined,
): Rates {
  const { drift } = key;
  if (drift === undefined) return {};
  if (typeof fps !== 'number' || !Number.isFinite(fps) || fps <= 0) {
    throw new Error(`keyframes() requires a positive fps option to drift (key ${index})`);
  }
  const invalid = () =>
    new Error(`keyframes() requires drift to be a rate per second or { log }, one for each field of the value (key ${index})`);
  let rates: Rates;
  if (typeof key.value === 'number') {
    if (!isDriftRate(drift)) throw invalid();
    rates = { '': drift };
  } else {
    // `{ log: r }` is one rate, not a rate per field, unless the value has a
    // field named `log`.
    const oneRate = isDriftRate(drift) && !Object.prototype.hasOwnProperty.call(key.value, 'log');
    if (drift === null || typeof drift !== 'object' || Array.isArray(drift) || oneRate) throw invalid();
    rates = drift as Rates;
    for (const field of Object.keys(rates)) {
      if (!Object.prototype.hasOwnProperty.call(key.value, field)) {
        throw new Error(`keyframes() got drift for "${field}", which the value does not have (key ${index})`);
      }
      if (!isDriftRate(rates[field])) throw invalid();
    }
  }
  if (next && next.frame === key.frame) {
    throw new Error(`keyframes() can drift a value only while it holds after its key, and a cut follows (key ${index})`);
  }
  if (next) {
    const here = asFields(key.value);
    const there = asFields(next.value);
    for (const field of Object.keys(rates)) {
      if (here[field] !== there[field]) {
        const what = field === '' ? 'the value moves' : `"${field}" moves`;
        throw new Error(`keyframes() can drift a value only while it holds after its key, and ${what} (key ${index})`);
      }
    }
  }
  return rates;
}

function isDriftRate(rate: unknown): rate is DriftRate {
  if (typeof rate === 'number') return Number.isFinite(rate);
  if (rate === null || typeof rate !== 'object' || Array.isArray(rate)) return false;
  const keys = Object.keys(rate);
  return keys.length === 1 && keys[0] === 'log' && Number.isFinite((rate as { log: unknown }).log);
}

/** `'number'`, or an object's sorted field names, to compare keys' values by. */
function valueShape(value: unknown): string {
  if (typeof value === 'number') return 'number';
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return 'invalid';
  return `{${Object.keys(value).sort().join(',')}}`;
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
