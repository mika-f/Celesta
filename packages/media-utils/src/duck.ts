// Ducking curves in TypeScript: a port of `duck_curve`, `check_duck_group`,
// and the defaults and rules of `AudioDuck::resolve` in
// `crates/composition/src/duck.rs`, which the fixture
// `crates/composition/tests/fixtures/duck-curves.json` pins the curves to.
// Only types are imported, so browser export can load this file alone.

import type { AnimatedNumber, AudioDuck } from '@celesta.studio/react';

/** A clip's ducking with the defaults applied, in seconds; `to` is a multiplier. */
export interface ResolvedDuck {
  to: number;
  attack: number;
  release: number;
  holdGap: number;
}

/** `<Audio duck>` without `under`: what `duckEnvelope` and `resolveDuck` take. */
export type DuckShape = Omit<AudioDuck, 'under'>;

const GROUP_RULE = 'must be a non-empty name';
const TO_RULE = 'must be a multiplier from 0 to 1, or { db } at or below 0';
const TIME_RULE = 'must be finite and not negative';

/** Checks a clip's `duckGroup`. */
export function checkDuckGroup(group: string): void {
  if (group === '') throw new Error(`duckGroup ${GROUP_RULE}`);
}

/** The groups `duck.under` names, as a list of at least one non-empty name. */
export function duckGroups(under: AudioDuck['under']): string[] {
  const groups = typeof under === 'string' ? [under] : under;
  if (groups.length === 0 || groups.some((group) => group === '')) throw new Error(`duck.under ${GROUP_RULE}`);
  return groups;
}

function seconds(value: number | undefined, fallback: number, field: string): number {
  if (value === undefined) return fallback;
  if (!Number.isFinite(value) || value < 0) throw new Error(`duck.${field} ${TIME_RULE}`);
  return value;
}

/** Applies the defaults (0.3, 0.15 s, 0.4 s, 0.6 s), converts decibels, and checks every rule. */
export function resolveDuck(duck: DuckShape): ResolvedDuck {
  let to = 0.3;
  if (typeof duck.to === 'number') {
    if (!Number.isFinite(duck.to) || duck.to < 0 || duck.to > 1) throw new Error(`duck.to ${TO_RULE}`);
    to = duck.to;
  } else if (duck.to !== undefined) {
    const { db } = duck.to;
    if (!Number.isFinite(db) || db > 0) throw new Error(`duck.to ${TO_RULE}`);
    to = 10 ** (db / 20);
  }
  return {
    to,
    attack: seconds(duck.attack, 0.15, 'attack'),
    release: seconds(duck.release, 0.4, 'release'),
    holdGap: seconds(duck.holdGap, 0.6, 'holdGap'),
  };
}

/** Seconds rounded to microseconds as Rust's `f64::round` does, halves away from zero. */
function microseconds(value: number) {
  return { value: Math.sign(value) * Math.round(Math.abs(value) * 1_000_000), timescale: 1_000_000 };
}

/**
 * The volume of a clip ducked under voices playing over `intervals`
 * (seconds on one clock, such as the composition's): `volume` (default 1)
 * with nobody speaking, easing down to `volume × to` over `attack` before
 * each run of speech and back up over `release` after it. Runs join across
 * silences shorter than `holdGap`, or too short to fit a release and the
 * next attack. Keys are at `time − origin` seconds, so pass the composition
 * time the `<Audio>`'s sequence starts on. Without any interval, the
 * volume itself.
 */
export function duckEnvelope(
  intervals: ReadonlyArray<readonly [number, number]>,
  duck: DuckShape & { volume?: number },
  options: { origin?: number } = {},
): AnimatedNumber {
  const volume = duck.volume ?? 1;
  if (!Number.isFinite(volume) || volume < 0) throw new Error(`volume ${TIME_RULE}`);
  const { to, attack, release, holdGap } = resolveDuck(duck);
  const origin = options.origin ?? 0;
  const join = Math.max(holdGap, attack + release);
  const sorted = intervals
    .filter(([start, end]) => Number.isFinite(start) && Number.isFinite(end) && end > start)
    .map(([start, end]): [number, number] => [start, end])
    .sort((a, b) => a[0] - b[0]);
  const runs: [number, number][] = [];
  for (const [start, end] of sorted) {
    const last = runs[runs.length - 1];
    if (last && start - last[1] < join) last[1] = Math.max(last[1], end);
    else runs.push([start, end]);
  }
  if (runs.length === 0) return volume;
  const key = (time: number, value: number, eased: boolean) => ({
    time: microseconds(time - origin),
    value: value * volume,
    ...(eased ? { easing: 'ease-in-out' as const } : {}),
  });
  return {
    type: 'keyframes',
    keyframes: runs.flatMap(([start, end]) => [
      key(start - attack, 1, false),
      key(start, to, true),
      key(end, to, false),
      key(end + release, 1, true),
    ]),
  };
}
