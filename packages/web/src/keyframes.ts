import type { AnimatedNumber, Keyframe, KeyframeEasing } from './types';

// A port of `crates/composition/src/animation.rs`, formula for formula, so a
// clip's keyframes sound the same in browser export as in the native mixer.
// `tests/fixtures/keyframe-evaluation.json` in that crate pins both.

const seconds = (key: Keyframe): number => key.time.value / key.time.timescale;

function keyframes(value: Exclude<AnimatedNumber, number>): Keyframe[] {
  if (value.keyframes.length === 0) throw new Error('animation has no keyframes');
  return value.keyframes;
}

/**
 * The value at clip-local `time` seconds. Easing belongs to the destination
 * keyframe of a segment; before the first and after the last keyframe the
 * value holds.
 */
export function evaluateAnimated(value: AnimatedNumber, time: number): number {
  if (typeof value === 'number') return value;
  const keys = keyframes(value);
  if (time <= seconds(keys[0])) return keys[0].value;
  for (let index = 1; index < keys.length; index++) {
    const from = keys[index - 1], to = keys[index];
    if (time <= seconds(to)) {
      const duration = seconds(to) - seconds(from);
      if (duration <= 0) return to.value;
      const progress = Math.min(1, Math.max(0, (time - seconds(from)) / duration));
      return from.value + (to.value - from.value) * applyEasing(progress, to.easing ?? 'linear');
    }
  }
  return keys[keys.length - 1].value;
}

/**
 * The integral from clip-local 0 through `time` seconds, which turns an
 * animated playback rate into a source position. A segment that starts
 * before 0 counts only from 0 on, from the value `evaluateAnimated` gives
 * there.
 */
export function integrateAnimated(value: AnimatedNumber, time: number): number {
  const end = Math.max(0, time);
  if (typeof value === 'number') return value * end;
  const keys = keyframes(value);
  const firstTime = Math.max(0, seconds(keys[0]));
  let integral = keys[0].value * Math.min(end, firstTime);
  if (end <= firstTime) return integral;
  for (let index = 1; index < keys.length; index++) {
    const from = keys[index - 1], to = keys[index];
    const start = seconds(from), finish = seconds(to);
    const duration = finish - start;
    if (duration <= 0) continue;
    const antiderivative = (progress: number) => from.value * progress + (to.value - from.value) * easingIntegral(progress, to.easing ?? 'linear');
    const fromZero = Math.min(1, Math.max(0, -start / duration));
    const progress = Math.min(1, Math.max(0, (end - start) / duration));
    integral += duration * (antiderivative(progress) - antiderivative(fromZero));
    if (end <= finish) return integral;
  }
  const last = keys[keys.length - 1];
  return integral + last.value * Math.max(0, end - Math.max(0, seconds(last)));
}

/** One AudioParam automation call: `setValueAtTime` or `linearRampToValueAtTime`. */
export type AutomationEvent = { kind: 'set' | 'ramp'; value: number; time: number };

/** Spacing of the points an eased segment is followed with. */
export const AUTOMATION_STEP_SECONDS = 0.001;
/** At most this many points per segment, however long it is. */
export const MAX_POINTS_PER_SEGMENT = 1024;

/**
 * Automation that makes an AudioParam follow `value`, whose clip-local 0 is
 * context time `start`, up to context time `end`. Linear segments are one
 * ramp each, a cut is a set, and an eased segment is followed with ramps
 * through points `AUTOMATION_STEP_SECONDS` apart, so the param never moves
 * in audible steps. Automation cannot start before context time 0, so a clip
 * starting earlier begins from where its curve has got to by then.
 */
export function automationEvents(value: AnimatedNumber, start: number, end: number, range?: { min: number; max: number }): AutomationEvent[] {
  const clamp = (v: number) => (range ? Math.min(range.max, Math.max(range.min, v)) : v);
  const first = Math.max(0, start);
  if (typeof value === 'number') return [{ kind: 'set', value: clamp(value), time: first }];
  const keys = keyframes(value);
  const from0 = first - start, until = end - start;
  const events: AutomationEvent[] = [{ kind: 'set', value: clamp(evaluateAnimated(value, from0)), time: first }];
  for (let index = 1; index < keys.length; index++) {
    const from = keys[index - 1], to = keys[index];
    const begin = seconds(from), finish = seconds(to);
    // A segment that is over by the first event is skipped, but a cut right
    // there still applies: the first event holds the value before it.
    if (finish < from0 || (finish === from0 && begin < finish)) continue;
    if (begin >= until) break;
    if (finish - begin <= 0) {
      events.push({ kind: 'set', value: clamp(to.value), time: start + finish });
      continue;
    }
    // Only what is heard is scheduled, so a segment across the end stops there.
    const stop = Math.min(finish, until);
    const valueAt = (local: number) => (local === finish ? to.value : evaluateAnimated(value, local));
    if ((to.easing ?? 'linear') === 'linear') {
      events.push({ kind: 'ramp', value: clamp(valueAt(stop)), time: start + stop });
    } else {
      const a = Math.max(begin, from0);
      const points = Math.max(1, Math.min(MAX_POINTS_PER_SEGMENT, Math.ceil((stop - a) / AUTOMATION_STEP_SECONDS - 1e-9)));
      for (let point = 1; point <= points; point++) {
        const local = point === points ? stop : a + ((stop - a) * point) / points;
        events.push({ kind: 'ramp', value: clamp(valueAt(local)), time: start + local });
      }
    }
  }
  return events;
}

function easeOutBounce(t: number): number {
  const n1 = 7.5625, d1 = 2.75;
  if (t < 1 / d1) return n1 * t * t;
  if (t < 2 / d1) return n1 * (t - 1.5 / d1) ** 2 + 0.75;
  if (t < 2.5 / d1) return n1 * (t - 2.25 / d1) ** 2 + 0.9375;
  return n1 * (t - 2.625 / d1) ** 2 + 0.984375;
}

const C1 = 1.70158, C2 = C1 * 1.525, C3 = C1 + 1, C4 = (2 * Math.PI) / 3, C5 = (2 * Math.PI) / 4.5;
/** `Easing::Whip`'s speed in the middle, as a multiple of the average. */
const WHIP_SHARPNESS = 7;

const EASINGS: Record<KeyframeEasing, (t: number) => number> = {
  'linear': (t) => t,
  'ease-in': (t) => t * t,
  'ease-out': (t) => 1 - (1 - t) * (1 - t),
  'ease-in-out': (t) => (t < 0.5 ? 2 * t * t : -1 + (4 - 2 * t) * t),
  'ease-in-sine': (t) => 1 - Math.cos(t * (Math.PI / 2)),
  'ease-out-sine': (t) => Math.sin(t * (Math.PI / 2)),
  'ease-in-out-sine': (t) => -(Math.cos(Math.PI * t) - 1) / 2,
  'ease-in-quad': (t) => t * t,
  'ease-out-quad': (t) => t * (2 - t),
  'ease-in-out-quad': (t) => (t < 0.5 ? 2 * t * t : -1 + (4 - 2 * t) * t),
  'ease-in-cubic': (t) => t * t * t,
  'ease-out-cubic': (t) => (t - 1) ** 3 + 1,
  'ease-in-out-cubic': (t) => (t < 0.5 ? 4 * t * t * t : (t - 1) * (2 * t - 2) ** 2 + 1),
  'ease-in-quart': (t) => t ** 4,
  'ease-out-quart': (t) => 1 - (t - 1) ** 4,
  'ease-in-out-quart': (t) => (t < 0.5 ? 8 * t ** 4 : 1 - 8 * (t - 1) ** 4),
  'ease-in-quint': (t) => t ** 5,
  'ease-out-quint': (t) => 1 + (t - 1) ** 5,
  'ease-in-out-quint': (t) => (t < 0.5 ? 16 * t ** 5 : 1 + 16 * (t - 1) ** 5),
  'ease-in-expo': (t) => (t === 0 ? 0 : 2 ** (10 * t - 10)),
  'ease-out-expo': (t) => (t === 1 ? 1 : 1 - 2 ** (-10 * t)),
  'ease-in-out-expo': (t) => (t === 0 ? 0 : t === 1 ? 1 : t < 0.5 ? 2 ** (20 * t - 10) / 2 : (2 - 2 ** (-20 * t + 10)) / 2),
  'ease-in-circ': (t) => 1 - Math.sqrt(1 - t * t),
  'ease-out-circ': (t) => Math.sqrt(1 - (t - 1) ** 2),
  'ease-in-out-circ': (t) => (t < 0.5 ? (1 - Math.sqrt(1 - 4 * t * t)) / 2 : (Math.sqrt(1 - 4 * (t - 1) ** 2) + 1) / 2),
  'ease-in-back': (t) => C3 * t * t * t - C1 * t * t,
  'ease-out-back': (t) => 1 + C3 * (t - 1) ** 3 + C1 * (t - 1) ** 2,
  'ease-in-out-back': (t) => (t < 0.5 ? ((2 * t) ** 2 * ((C2 + 1) * 2 * t - C2)) / 2 : ((2 * t - 2) ** 2 * ((C2 + 1) * (t * 2 - 2) + C2) + 2) / 2),
  'ease-in-elastic': (t) => (t === 0 ? 0 : t === 1 ? 1 : -(2 ** (10 * t - 10)) * Math.sin((t * 10 - 10.75) * C4)),
  'ease-out-elastic': (t) => (t === 0 ? 0 : t === 1 ? 1 : 2 ** (-10 * t) * Math.sin((t * 10 - 0.75) * C4) + 1),
  'ease-in-out-elastic': (t) =>
    t === 0 ? 0
    : t === 1 ? 1
    : t < 0.5 ? -(2 ** (20 * t - 10) * Math.sin((20 * t - 11.125) * C5)) / 2
    : (2 ** (-20 * t + 10) * Math.sin((20 * t - 11.125) * C5)) / 2 + 1,
  'ease-in-bounce': (t) => 1 - easeOutBounce(1 - t),
  'ease-out-bounce': easeOutBounce,
  'ease-in-out-bounce': (t) => (t < 0.5 ? (1 - easeOutBounce(1 - 2 * t)) / 2 : (1 + easeOutBounce(2 * t - 1)) / 2),
  'whip': (t) => (t === 0 ? 0 : t === 1 ? 1 : (1 + Math.tanh(WHIP_SHARPNESS * (2 * t - 1)) / Math.tanh(WHIP_SHARPNESS)) / 2),
};

function easing(name: KeyframeEasing): (t: number) => number {
  const curve = Object.hasOwn(EASINGS, name) ? EASINGS[name] : undefined;
  if (!curve) throw new Error(`unknown easing: ${name}`);
  return curve;
}

function applyEasing(progress: number, name: KeyframeEasing): number {
  return easing(name)(progress);
}

function easingIntegral(progress: number, name: KeyframeEasing): number {
  switch (name) {
    case 'linear': return (progress * progress) / 2;
    case 'ease-in': return (progress * progress * progress) / 3;
    case 'ease-out': return progress * progress - (progress * progress * progress) / 3;
    case 'ease-in-out':
    case 'ease-in-out-quad':
      return progress < 0.5
        ? (2 * progress * progress * progress) / 3
        : 2 * progress * progress - progress - (2 * progress * progress * progress) / 3 + 1 / 6;
    default: return integrateNumerically(progress, easing(name));
  }
}

/** Composite Simpson's rule for ∫₀^progress f, with the native mixer's step count. */
function integrateNumerically(progress: number, f: (x: number) => number): number {
  if (progress <= 0) return 0;
  const steps = 128;
  const h = progress / steps;
  let sum = f(0) + f(progress);
  for (let index = 1; index < steps; index++) sum += f(index * h) * (index % 2 === 0 ? 2 : 4);
  return (sum * h) / 3;
}
