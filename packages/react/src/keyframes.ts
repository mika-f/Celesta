// Frame-based authoring for the project format's keyframed numbers. `<Audio>`
// and `<Dialogue>` take `volume`/`playbackRate` as an `Animatable<number>`
// whose keyframe times are seconds on the enclosing sequence's clock; this
// builds one from frame numbers so fades line up with the frames scenes are
// laid out in.

import type { Animatable, Easing } from './scene';
import { secondsToTime } from './time';

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
): Animatable<number> {
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
