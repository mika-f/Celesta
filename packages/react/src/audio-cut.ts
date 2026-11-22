// Cuts an audio clip report around the spans a failed scene did not play, for
// the preview's audio sweep (`collectAudio({ preview })` in render.ts). Each
// piece must sound the way the uncut clip does there, so its source position
// is the playback rate integrated up to the piece and its curves move by
// exactly the cut, as `crates/composition/src/animation.rs` reads them.

import { Easings } from './animation';
import type { Easing } from './generated/Easing';
import type { KeyframeAnimation } from './generated/KeyframeAnimation';
import type { AudioClipDescriptor } from './render';
import { SECONDS_TIMESCALE } from './time';

type AnimatedNumber = number | KeyframeAnimation<number>;
type Key = KeyframeAnimation<number>['keyframes'][number];

const secondsOf = (key: Key): number => key.time.value / key.time.timescale;

/** `ease-in-out-back` as the `Easings` key `easeInOutBack`. */
function easingCurve(easing: Easing | null | undefined): (t: number) => number {
  const name = (easing ?? 'linear').replace(/-([a-z])/g, (_, letter: string) => letter.toUpperCase());
  // Only the keyframe easings' names reach here, never a factory's.
  const curve = (Easings as unknown as Record<string, ((t: number) => number) | undefined>)[name];
  if (!curve) throw new Error(`unknown easing: ${easing}`);
  return curve;
}

/** Composite Simpson's rule for ∫ₐᵇ f, with the native mixer's step count. */
function simpson(f: (t: number) => number, a: number, b: number): number {
  if (b <= a) return 0;
  const steps = 128;
  const h = (b - a) / steps;
  let sum = f(a) + f(b);
  for (let index = 1; index < steps; index++) sum += f(a + index * h) * (index % 2 === 0 ? 2 : 4);
  return (sum * h) / 3;
}

/**
 * ∫₀ᵗ of `value` over clip-local seconds: how far into its source a clip
 * has played after `time` at an animated rate. As `integrate_f64` in
 * `crates/composition/src/animation.rs` (and `@celesta.studio/web`'s port)
 * does it: the first value holds before the first keyframe and the last
 * after the last, each segment is integrated over its own eased progress,
 * and a segment of length 0 (a cut between two keys at one time) adds
 * nothing, so neither side of a cut leaks into the other.
 */
export function integrateAnimated(value: AnimatedNumber, time: number): number {
  const end = Math.max(0, time);
  if (typeof value === 'number') return value * end;
  const keys = value.keyframes;
  if (keys.length === 0) return 0;
  const firstTime = Math.max(0, secondsOf(keys[0]));
  let integral = keys[0].value * Math.min(end, firstTime);
  if (end <= firstTime) return integral;
  for (let index = 1; index < keys.length; index++) {
    const from = keys[index - 1];
    const to = keys[index];
    const start = secondsOf(from);
    const finish = secondsOf(to);
    const duration = finish - start;
    if (duration <= 0) continue;
    // Only the part of the segment from clip-local 0 to `end`.
    const a = Math.min(1, Math.max(0, -start / duration));
    const b = Math.min(1, Math.max(0, (end - start) / duration));
    integral += duration * ((b - a) * from.value + (to.value - from.value) * simpson(easingCurve(to.easing), a, b));
    if (end <= finish) return integral;
  }
  const last = keys[keys.length - 1];
  return integral + last.value * Math.max(0, end - Math.max(0, secondsOf(last)));
}

/**
 * `value` as seen from `offset` seconds into the clip, in microseconds, so a
 * cut that is no whole number of the keyframes' own timescale moves them by
 * exactly that much.
 */
function moveEarlier(value: AnimatedNumber, offset: number): AnimatedNumber {
  if (typeof value === 'number' || offset === 0) return value;
  return {
    ...value,
    keyframes: value.keyframes.map((key) => ({
      ...key,
      time: { value: Math.round((secondsOf(key) - offset) * SECONDS_TIMESCALE), timescale: SECONDS_TIMESCALE },
    })),
  };
}

/**
 * The parts of `clip` outside `cuts` (composition seconds, sorted and not
 * overlapping). Each starts its source where the clip had got to and its
 * curves (timed from the clip's start) earlier by as much; a fade stays at
 * the edge of the clip it was on. A looping clip's source position is left
 * unwrapped, as past its end the mixer wraps it.
 */
export function cutAudioClip(clip: AudioClipDescriptor, cuts: readonly [number, number][]): AudioClipDescriptor[] {
  const end = clip.start + clip.duration;
  const pieces: AudioClipDescriptor[] = [];
  let at = clip.start;
  const keep = (from: number, to: number) => {
    if (to <= from) return;
    const offset = from - clip.start;
    const { fadeIn, fadeOut, ...rest } = clip;
    pieces.push({
      ...rest,
      start: from,
      duration: to - from,
      sourceStart: clip.sourceStart + integrateAnimated(clip.playbackRate, offset),
      playbackRate: moveEarlier(clip.playbackRate, offset),
      volume: moveEarlier(clip.volume, offset),
      ...(clip.pitch !== undefined ? { pitch: moveEarlier(clip.pitch, offset) } : {}),
      ...(fadeIn !== undefined && from === clip.start ? { fadeIn } : {}),
      ...(fadeOut !== undefined && to === end ? { fadeOut } : {}),
    });
  };
  for (const [from, to] of cuts) {
    if (to <= at || from >= end) continue;
    keep(at, Math.min(from, end));
    at = Math.max(at, to);
  }
  keep(at, end);
  return pieces;
}
