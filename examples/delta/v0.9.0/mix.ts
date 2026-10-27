import { Easings } from '@celesta/react';
import type { AnimatedNumber } from '@celesta/react';
import { duckEnvelope } from '@celesta/media-utils';
import { FPS } from '../components/theme';
import { DURATION } from './timeline';
import { voices } from './voices';

// The film's mix in one place: the bed fades in over the first bar and out
// over the last bar and a half (<Audio fadeIn fadeOut>, #444), and ducks
// 12 dB under the three voice lines (<Audio duck>, #465). film-0.9.0.tsx
// plays it with these props; FADES and DUCKING draw the same curve, built
// with duckEnvelope(), the function the preview and the export follow.
export const BED = {
  fadeIn: 2.4,
  fadeOut: 3.6,
  duck: { under: 'voice', to: { db: -12 } },
} as const;

let envelope: AnimatedNumber | undefined;

/** The ducking curve, keyed in seconds on the composition's clock (the bed starts at 0). */
export function duckCurve(): AnimatedNumber {
  if (!envelope) {
    const { under: _, ...shape } = BED.duck;
    const intervals = Object.values(voices()).map((v) => [v.from / FPS, v.from / FPS + v.line.durationInSeconds] as const);
    envelope = duckEnvelope(intervals, shape);
  }
  return envelope;
}

/** An AnimatedNumber at `seconds`: the destination key's easing, held before the first and after the last. */
function valueAt(value: AnimatedNumber, seconds: number): number {
  if (typeof value === 'number') return value;
  const keys = value.keyframes;
  const time = (i: number) => keys[i].time.value / keys[i].time.timescale;
  if (seconds <= time(0)) return keys[0].value;
  for (let i = 1; i < keys.length; i++) {
    if (seconds <= time(i)) {
      const span = time(i) - time(i - 1);
      const p = span <= 0 ? 1 : (seconds - time(i - 1)) / span;
      const eased = keys[i].easing === 'ease-in-out' ? Easings.easeInOut(p) : p;
      return keys[i - 1].value + (keys[i].value - keys[i - 1].value) * eased;
    }
  }
  return keys[keys.length - 1].value;
}

/** The ducking gain alone at a composition frame. */
export function duckAt(frame: number) {
  return valueAt(duckCurve(), frame / FPS);
}

/** The fades alone: linear in amplitude at the clip's edges, as <Audio> plays them. */
export function fadeAt(frame: number) {
  const t = frame / FPS;
  const end = DURATION / FPS;
  return Math.min(1, Math.max(0, t / BED.fadeIn), Math.max(0, (end - t) / BED.fadeOut));
}

/** What the bed plays at, all told: the fades times the ducking. */
export function bedGain(frame: number) {
  return fadeAt(frame) * duckAt(frame);
}

export const toDb = (gain: number) => (gain <= 0 ? -Infinity : 20 * Math.log10(gain));
