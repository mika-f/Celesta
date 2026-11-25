// Cuts an audio clip report around the spans a failed scene did not play, for
// the preview's audio sweep (`collectAudio({ preview })` in render.ts). Each
// piece must sound the way the uncut clip does there, so its source position
// is the playback rate integrated up to the piece and its curves move by
// exactly the cut, as `crates/composition/src/animation.rs` reads them.

import { integrateAnimated } from './animated-number';
import type { AudioClipDescriptor } from './render';
import type { Animatable } from './scene';
import { secondsFromTime, secondsToTime } from './time';

type AnimatedNumber = Animatable<number>;

/**
 * `value` as seen from `offset` seconds into the clip, in microseconds, so a
 * cut that is no whole number of the keyframes' own timescale moves them by
 * exactly that much.
 */
export function moveEarlier(value: AnimatedNumber, offset: number): AnimatedNumber {
  if (typeof value === 'number' || offset === 0) return value;
  return {
    ...value,
    keyframes: value.keyframes.map((key) => ({
      ...key,
      time: secondsToTime(secondsFromTime(key.time) - offset),
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
