// Frame-driven animation helpers, meant to be combined with useCurrentFrame()
// (hooks.ts). Neither function here reads React state itself — both are
// plain math over whatever frame/time value the caller passes in, so they
// work the same whether called from a component body or anywhere else.

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

  let segment = 0;
  while (segment < inputRange.length - 2 && input >= inputRange[segment + 1]) {
    segment += 1;
  }
  const segmentStart = inputRange[segment];
  const segmentEnd = inputRange[segment + 1];
  const progress = easing((input - segmentStart) / (segmentEnd - segmentStart));
  return outputRange[segment] + progress * (outputRange[segment + 1] - outputRange[segment]);
}

/**
 * Common easing curves for `interpolate()`'s `easing` option. Named `Easings`
 * (not `Easing`) to avoid colliding with the generated `Easing` union type
 * used by the project's own `Animatable<T>`/`Keyframe<T>` model, which is an
 * unrelated, project.json-facing concept.
 */
export const Easings = {
  linear: (t: number): number => t,
  easeIn: (t: number): number => t * t,
  easeOut: (t: number): number => t * (2 - t),
  easeInOut: (t: number): number => (t < 0.5 ? 2 * t * t : -1 + (4 - 2 * t) * t),
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
