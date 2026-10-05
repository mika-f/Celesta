// Frame-driven animation helpers, meant to be combined with useCurrentFrame()
// (hooks.ts). No function here reads React state itself — each is plain math
// over whatever frame/time value the caller passes in, so they work the same
// whether called from a component body or anywhere else.

export type Extrapolate = 'extend' | 'clamp' | 'identity';

export interface InterpolateOptions {
  easing?: (input: number) => number;
  extrapolateLeft?: Extrapolate;
  extrapolateRight?: Extrapolate;
}

/**
 * Maps `input` from `inputRange` into `outputRange`, linearly interpolating
 * within each segment (and applying `options.easing` to the 0-1 position
 * inside that segment, if given). `inputRange` must be strictly increasing
 * and the same length as `outputRange` (at least 2 entries each).
 *
 * Outside `inputRange`, `extrapolateLeft`/`extrapolateRight` control the
 * result: `'extend'` (default) continues the boundary segment's linear
 * slope, `'clamp'` holds the boundary output value, and `'identity'`
 * returns `input` itself unchanged.
 */
export function interpolate(
  input: number,
  inputRange: readonly number[],
  outputRange: readonly number[],
  options: InterpolateOptions = {},
): number {
  if (inputRange.length < 2 || inputRange.length !== outputRange.length) {
    throw new Error('interpolate() requires inputRange and outputRange of the same length, at least 2');
  }
  for (let i = 1; i < inputRange.length; i += 1) {
    if (inputRange[i] <= inputRange[i - 1]) {
      throw new Error('interpolate() requires inputRange to be strictly increasing');
    }
  }

  const { easing = (t: number) => t, extrapolateLeft = 'extend', extrapolateRight = 'extend' } = options;

  if (input < inputRange[0]) {
    if (extrapolateLeft === 'identity') {
      return input;
    }
    if (extrapolateLeft === 'clamp') {
      return outputRange[0];
    }
  }
  const lastIndex = inputRange.length - 1;
  if (input > inputRange[lastIndex]) {
    if (extrapolateRight === 'identity') {
      return input;
    }
    if (extrapolateRight === 'clamp') {
      return outputRange[lastIndex];
    }
  }

  const { segment, progress } = locateSegment(input, inputRange, easing);
  return outputRange[segment] + progress * (outputRange[segment + 1] - outputRange[segment]);
}

/**
 * The `inputRange` segment `input` falls in (the first or last one outside
 * the range) and the eased position inside it: 0 at its start, 1 at its end,
 * and beyond 0–1 when extrapolating or when `easing` overshoots.
 */
function locateSegment(
  input: number,
  inputRange: readonly number[],
  easing: (input: number) => number,
): { segment: number; progress: number } {
  let segment = 0;
  while (segment < inputRange.length - 2 && input >= inputRange[segment + 1]) {
    segment += 1;
  }
  const segmentStart = inputRange[segment];
  const segmentEnd = inputRange[segment + 1];
  return { segment, progress: easing((input - segmentStart) / (segmentEnd - segmentStart)) };
}

/** How `interpolateColor()` treats inputs outside `inputRange`. */
export type ColorExtrapolate = 'extend' | 'clamp';

export interface InterpolateColorOptions {
  easing?: (input: number) => number;
  extrapolateLeft?: ColorExtrapolate;
  extrapolateRight?: ColorExtrapolate;
}

const HEX_COLOR = /^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/;

/**
 * Maps `input` from `inputRange` onto `colors` (`#RRGGBB` or `#RRGGBBAA`)
 * the way `interpolate()` maps it onto numbers, returning `#RRGGBBAA` with
 * uppercase digits. `inputRange` must be strictly increasing, finite, and
 * the same length as `colors` (at least 2 entries each).
 *
 * Colors blend in gamma-encoded sRGB with premultiplied alpha, like the
 * renderer's gradient stops and CSS: a fade to `#00000000` doesn't darken the
 * color on the way, and a fully transparent color's RGB has no effect. Each
 * channel is rounded to the nearest 8-bit value. Where the eased position in
 * a segment is exactly 0 or 1 (at every `inputRange` entry, with `Easings`)
 * the result is that end's color, even a fully transparent one.
 *
 * Outside `inputRange`, `extrapolateLeft`/`extrapolateRight` choose
 * `'clamp'` (default), which holds the boundary color, or `'extend'`, which
 * continues the boundary segment's slope. Channels that `'extend'` or an
 * overshooting `easing` (such as `Easings.easeOutBack`) push past 0–255 are
 * clamped to it.
 */
export function interpolateColor(
  input: number,
  inputRange: readonly number[],
  colors: readonly string[],
  options: InterpolateColorOptions = {},
): string {
  if (typeof input !== 'number' || !Number.isFinite(input)) {
    throw new Error('interpolateColor() requires a finite input');
  }
  if (inputRange.length < 2 || inputRange.length !== colors.length) {
    throw new Error('interpolateColor() requires inputRange and colors of the same length, at least 2');
  }
  // Indexed loops rather than every()/map(), which skip sparse-array holes.
  for (let i = 0; i < inputRange.length; i += 1) {
    const value = inputRange[i];
    if (typeof value !== 'number' || !Number.isFinite(value)) {
      throw new Error('interpolateColor() requires finite inputRange values');
    }
    if (i > 0 && value <= inputRange[i - 1]) {
      throw new Error('interpolateColor() requires inputRange to be strictly increasing');
    }
  }
  const rgba = Array.from(colors, (color, index) => {
    if (typeof color !== 'string' || !HEX_COLOR.test(color)) {
      throw new Error(
        `interpolateColor() requires #RRGGBB or #RRGGBBAA colors, got ${String(JSON.stringify(color))} at index ${index}`,
      );
    }
    return parseHexColor(color);
  });
  const { easing = (t: number) => t, extrapolateLeft = 'clamp', extrapolateRight = 'clamp' } = options;
  for (const [name, value] of [['extrapolateLeft', extrapolateLeft], ['extrapolateRight', extrapolateRight]]) {
    if (value !== 'extend' && value !== 'clamp') {
      throw new Error(`interpolateColor() ${name} must be 'extend' or 'clamp'`);
    }
  }

  const lastIndex = inputRange.length - 1;
  if (input <= inputRange[0] && extrapolateLeft === 'clamp') {
    return formatHexColor(rgba[0]);
  }
  if (input >= inputRange[lastIndex] && extrapolateRight === 'clamp') {
    return formatHexColor(rgba[lastIndex]);
  }

  const { segment, progress } = locateSegment(input, inputRange, easing);
  if (progress === 0) {
    return formatHexColor(rgba[segment]);
  }
  if (progress === 1) {
    return formatHexColor(rgba[segment + 1]);
  }
  const from = premultiply(rgba[segment]);
  const to = premultiply(rgba[segment + 1]);
  const mixed = from.map((channel, i) => channel + progress * (to[i] - channel));
  const alpha = Math.min(255, mixed[3]);
  if (!(alpha > 0)) {
    return '#00000000';
  }
  const unpremultiply = (channel: number) => Math.min(255, Math.max(0, (channel * 255) / alpha));
  return formatHexColor([unpremultiply(mixed[0]), unpremultiply(mixed[1]), unpremultiply(mixed[2]), alpha]);
}

/** `[r, g, b, a]`, each 0–255; `#RRGGBB` is opaque. */
function parseHexColor(color: string): [number, number, number, number] {
  const channel = (offset: number) => parseInt(color.slice(offset, offset + 2), 16);
  return [channel(1), channel(3), channel(5), color.length === 9 ? channel(7) : 255];
}

function premultiply([r, g, b, a]: readonly number[]): number[] {
  return [(r * a) / 255, (g * a) / 255, (b * a) / 255, a];
}

/** Rounds each 0–255 channel to `#RRGGBBAA` with uppercase digits. */
function formatHexColor(channels: readonly number[]): string {
  return `#${channels.map((channel) => Math.round(channel).toString(16).padStart(2, '0')).join('').toUpperCase()}`;
}

/**
 * How far `frame` is through the span that starts at `start` and lasts
 * `durationInFrames`, as an eased value: 0 up to `start`, `easing` applied to
 * the linear 0–1 position inside the span, and 1 from its end on. The eased
 * value may leave 0–1 inside the span for curves such as `easeOutBack`.
 *
 * The workhorse behind most entrances: `opacity={progress(frame, 10, 20)}`.
 */
export function progress(
  frame: number,
  start: number,
  durationInFrames: number,
  easing: (input: number) => number = (t) => t,
): number {
  if (!Number.isFinite(durationInFrames) || durationInFrames <= 0) {
    throw new Error('progress() requires a positive durationInFrames');
  }
  return easing(Math.min(1, Math.max(0, (frame - start) / durationInFrames)));
}

/**
 * Common easing curves for `interpolate()`'s `easing` option. Every curve
 * returns exactly 0 at 0 and exactly 1 at 1, so `progress()` is exactly 0
 * before its span and exactly 1 after it. Named `Easings`
 * (not `Easing`) to avoid colliding with the generated `Easing` union type
 * used by the project's own `Animatable<T>`/`Keyframe<T>` model, which is an
 * unrelated, project.json-facing concept.
 */
export const Easings = {
  linear: (t: number): number => t,
  easeIn: (t: number): number => t * t,
  easeOut: (t: number): number => t * (2 - t),
  easeInOut: (t: number): number => (t < 0.5 ? 2 * t * t : -1 + (4 - 2 * t) * t),
  easeInSine: (t: number): number => (t === 1 ? 1 : 1 - Math.cos((t * Math.PI) / 2)),
  easeOutSine: (t: number): number => Math.sin((t * Math.PI) / 2),
  easeInOutSine: (t: number): number => -(Math.cos(Math.PI * t) - 1) / 2,
  easeInQuad: (t: number): number => t * t,
  easeOutQuad: (t: number): number => t * (2 - t),
  easeInOutQuad: (t: number): number => (t < 0.5 ? 2 * t * t : -1 + (4 - 2 * t) * t),
  easeInCubic: (t: number): number => t * t * t,
  easeOutCubic: (t: number): number => --t * t * t + 1,
  easeInOutCubic: (t: number): number => (t < 0.5 ? 4 * t * t * t : (t - 1) * (2 * t - 2) * (2 * t - 2) + 1),
  easeInQuart: (t: number): number => t * t * t * t,
  easeOutQuart: (t: number): number => 1 - --t * t * t * t,
  easeInOutQuart: (t: number): number => (t < 0.5 ? 8 * t * t * t * t : 1 - 8 * --t * t * t * t),
  easeInQuint: (t: number): number => t * t * t * t * t,
  easeOutQuint: (t: number): number => 1 + --t * t * t * t * t,
  easeInOutQuint: (t: number): number => (t < 0.5 ? 16 * t * t * t * t * t : 1 + 16 * --t * t * t * t * t),
  easeInExpo: (t: number): number => (t === 0 ? 0 : Math.pow(1024, t - 1)),
  easeOutExpo: (t: number): number => (t === 1 ? 1 : 1 - Math.pow(2, -10 * t)),
  easeInOutExpo: (t: number): number =>
    t === 0 ? 0 : t === 1 ? 1 : t < 0.5 ? Math.pow(1024, t * 2 - 1) / 2 : (2 - Math.pow(2, -20 * t + 10)) / 2,
  easeInCirc: (t: number): number => 1 - Math.sqrt(1 - t * t),
  easeOutCirc: (t: number): number => Math.sqrt(1 - --t * t),
  easeInOutCirc: (t: number): number =>
    t < 0.5 ? (1 - Math.sqrt(1 - 4 * t * t)) / 2 : (Math.sqrt(1 - --t * 2 * t * 2) + 1) / 2,
  easeInBack: (t: number): number => {
    const c1 = 1.70158;
    const c3 = c1 + 1;
    return t === 1 ? 1 : c3 * t * t * t - c1 * t * t;
  },
  easeOutBack: (t: number): number => {
    const c1 = 1.70158;
    const c3 = c1 + 1;
    return t === 0 ? 0 : 1 + c3 * --t * t * t + c1 * t * t;
  },
  easeInOutBack: (t: number): number => {
    const c1 = 1.70158;
    const c2 = c1 * 1.525;
    return t < 0.5
      ? (Math.pow(2 * t, 2) * ((c2 + 1) * 2 * t - c2)) / 2
      : (Math.pow(2 * t - 2, 2) * ((c2 + 1) * (t * 2 - 2) + c2) + 2) / 2;
  },
  easeInElastic: (t: number): number => {
    const c4 = (2 * Math.PI) / 3;
    return t === 0
      ? 0
      : t === 1
        ? 1
        : -Math.pow(2, 10 * t - 10) * Math.sin((t * 10 - 10.75) * c4);
  },
  easeOutElastic: (t: number): number => {
    const c4 = (2 * Math.PI) / 3;
    return t === 0
      ? 0
      : t === 1
        ? 1
        : Math.pow(2, -10 * t) * Math.sin((t * 10 - 0.75) * c4) + 1;
  },
  easeInOutElastic: (t: number): number => {
    const c5 = (2 * Math.PI) / 4.5;
    return t === 0
      ? 0
      : t === 1
        ? 1
        : t < 0.5
          ? -(Math.pow(2, 20 * t - 10) * Math.sin((20 * t - 11.125) * c5)) / 2
          : (Math.pow(2, -20 * t + 10) * Math.sin((20 * t - 11.125) * c5)) / 2 + 1;
  },
  easeInBounce: (t: number): number => 1 - Easings.easeOutBounce(1 - t),
  easeOutBounce: (t: number): number => {
    const n1 = 7.5625;
    const d1 = 2.75;
    if (t < 1 / d1) {
      return n1 * t * t;
    } else if (t < 2 / d1) {
      return n1 * (t -= 1.5 / d1) * t + 0.75;
    } else if (t < 2.5 / d1) {
      return n1 * (t -= 2.25 / d1) * t + 0.9375;
    } else {
      return n1 * (t -= 2.625 / d1) * t + 0.984375;
    }
  },
  easeInOutBounce: (t: number): number =>
    t < 0.5 ? (1 - Easings.easeOutBounce(1 - 2 * t)) / 2 : (1 + Easings.easeOutBounce(2 * t - 1)) / 2,
};

export interface SpringConfig {
  damping?: number;
  mass?: number;
  stiffness?: number;
  /** Hold the settled value once it's reached instead of overshooting past it. */
  overshootClamping?: boolean;
}

export interface SpringOptions {
  frame: number;
  fps: number;
  config?: SpringConfig;
  from?: number;
  to?: number;
  delay?: number;
  /** Frames beyond this are clamped to the settled value. */
  durationInFrames?: number;
}

const DEFAULT_SPRING_CONFIG: Required<SpringConfig> = {
  damping: 10,
  mass: 1,
  stiffness: 100,
  overshootClamping: false,
};

/**
 * A damped harmonic oscillator's step response (0 at t=0 settling toward 1),
 * mapped into `[from, to]`. `frame`/`fps`/`delay` convert to elapsed seconds;
 * frames before `delay` return `from`.
 */
export function spring(options: SpringOptions): number {
  const { frame, fps, from = 0, to = 1, delay = 0 } = options;
  if (fps <= 0) {
    throw new Error('spring() requires a positive fps');
  }
  const config = { ...DEFAULT_SPRING_CONFIG, ...options.config };

  let effectiveFrame = frame - delay;
  if (options.durationInFrames !== undefined) {
    effectiveFrame = Math.min(effectiveFrame, options.durationInFrames - delay);
  }
  if (effectiveFrame <= 0) {
    return from;
  }

  const t = effectiveFrame / fps;
  const settled = dampedOscillatorStepResponse(t, config);
  const value = from + settled * (to - from);

  if (config.overshootClamping) {
    return to >= from ? Math.min(value, to) : Math.max(value, to);
  }
  return value;
}

/** Step response x(t) of m*x'' + c*x' + k*x = k, x(0) = 0, x'(0) = 0. */
function dampedOscillatorStepResponse(t: number, config: Required<SpringConfig>): number {
  const { mass, stiffness, damping } = config;
  const naturalFrequency = Math.sqrt(stiffness / mass);
  const dampingRatio = damping / (2 * Math.sqrt(stiffness * mass));

  if (dampingRatio < 1) {
    const dampedFrequency = naturalFrequency * Math.sqrt(1 - dampingRatio * dampingRatio);
    const envelope = Math.exp(-dampingRatio * naturalFrequency * t);
    return (
      1 -
      envelope *
      (Math.cos(dampedFrequency * t) +
        ((dampingRatio * naturalFrequency) / dampedFrequency) * Math.sin(dampedFrequency * t))
    );
  }
  if (dampingRatio === 1) {
    return 1 - Math.exp(-naturalFrequency * t) * (1 + naturalFrequency * t);
  }
  const root = naturalFrequency * Math.sqrt(dampingRatio * dampingRatio - 1);
  const r1 = -naturalFrequency * dampingRatio + root;
  const r2 = -naturalFrequency * dampingRatio - root;
  return 1 - (r2 * Math.exp(r1 * t) - r1 * Math.exp(r2 * t)) / (r2 - r1);
}
