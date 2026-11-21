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

/**
 * The value at clip-local `time` seconds: held before the first and after the
 * last keyframe, eased by the destination keyframe's easing between them.
 */
function evaluate(value: AnimatedNumber, time: number): number {
  if (typeof value === 'number') return value;
  const keys = value.keyframes;
  if (keys.length === 0) return 0;
  if (time <= secondsOf(keys[0])) return keys[0].value;
  for (let index = 1; index < keys.length; index++) {
    const from = keys[index - 1];
    const to = keys[index];
    if (time <= secondsOf(to)) {
      const duration = secondsOf(to) - secondsOf(from);
      if (duration <= 0) return to.value;
      const progress = Math.min(1, Math.max(0, (time - secondsOf(from)) / duration));
      return from.value + (to.value - from.value) * easingCurve(to.easing)(progress);
    }
  }
  return keys[keys.length - 1].value;
}

/** Composite Simpson's rule for ∫ₐᵇ f, as the native mixer integrates eased segments. */
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
 * has played after `time` at an animated rate. Integrated piece by piece
 * between keyframes, where the curve is smooth.
 */
export function integrateAnimated(value: AnimatedNumber, time: number): number {
  if (time <= 0) return 0;
  if (typeof value === 'number') return value * time;
  const breaks = [0, ...value.keyframes.map(secondsOf).filter((at) => at > 0 && at < time), time];
  let integral = 0;
  for (let index = 1; index < breaks.length; index++) {
    integral += simpson((at) => evaluate(value, at), breaks[index - 1], breaks[index]);
  }
  return integral;
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
