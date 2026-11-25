import { evaluateAnimated } from '@celesta.studio/react/internal/animated-number';
import type { AnimatedNumber, Keyframe } from './types';

const seconds = (key: Keyframe): number => key.time.value / key.time.timescale;

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
  const keys = value.keyframes;
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
