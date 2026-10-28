// An element's entrance, hold, and exit in one wrapper: a preset effect
// eases the element in at the start of its window, leaves it exactly as
// written while it holds, and eases it out at the end. The effects are
// `<Transition>`'s (through `applyTransition`); only the timing is Motion's.

import * as React from 'react';
import type { ReactNode } from 'react';

import { clamp01 } from '@celesta/math';
import { Easings, Group, Sequence, useCurrentFrame, useVideoConfig } from '@celesta/react';
import type { GroupProps } from '@celesta/react';
import { applyTransition } from '@celesta/transitions';
import type { SlideFrom, TransitionEffect } from '@celesta/transitions';

/**
 * A `<Motion>` effect. `fade-up` and the other directions name the way the
 * element moves while it fades: an entrance `fade-up` rises into place, an
 * exit `fade-up` rises away. `scale` fades while growing from (or shrinking
 * to) `scaleFrom`.
 */
export type MotionPreset = 'fade' | 'fade-up' | 'fade-down' | 'fade-left' | 'fade-right' | 'scale';

export interface MotionOptions {
  type: MotionPreset;
  /**
   * Frames from hidden to at rest (or back), a whole number. 0 or 1 is a cut.
   * Defaults to half a second.
   */
  durationInFrames?: number;
  /** How far the directional presets move, in pixels. Defaults to 64. */
  distance?: number;
  /** Scale of `scale` while hidden. Defaults to 0.8. */
  scaleFrom?: number;
  /**
   * Maps linear time to how far the element has arrived, 0 to 1. An exit
   * plays it backwards, like `<Transition direction="out">`, so the default
   * `Easings.easeOutCubic` settles an entrance and speeds up an exit.
   */
  easing?: (progress: number) => number;
}

/** A preset name, or a preset with its timing and distances. */
export type MotionEffect = MotionPreset | MotionOptions;

export interface MotionProps extends GroupProps {
  /** How the element arrives at the start of its window. Omitted, it cuts in. */
  enter?: MotionEffect;
  /** How the element leaves at the end of its window. Omitted, it cuts out. */
  exit?: MotionEffect;
  /**
   * Frame the element appears on, like `<Sequence from>`: the children's
   * clock and media start there too. Omitted (with `durationInFrames`), the
   * element uses the enclosing sequence's window and clock as they are.
   */
  from?: number;
  /**
   * Frames the element is shown, entrance and exit included, like
   * `<Sequence durationInFrames>`. Defaults to the rest of the enclosing window.
   */
  durationInFrames?: number;
  children?: ReactNode;
}

type Direction = 'up' | 'down' | 'left' | 'right';

const OPPOSITE: Record<Direction, SlideFrom> = { up: 'bottom', down: 'top', left: 'right', right: 'left' };
const TOWARD: Record<Direction, SlideFrom> = { up: 'top', down: 'bottom', left: 'left', right: 'right' };

const PRESETS: Record<MotionPreset, { type: TransitionEffect['type']; direction?: Direction }> = {
  fade: { type: 'fade' },
  'fade-up': { type: ['fade', 'slide'], direction: 'up' },
  'fade-down': { type: ['fade', 'slide'], direction: 'down' },
  'fade-left': { type: ['fade', 'slide'], direction: 'left' },
  'fade-right': { type: ['fade', 'slide'], direction: 'right' },
  scale: { type: ['fade', 'scale'] },
};

interface Envelope {
  effect: TransitionEffect;
  /** Frames before the element is at rest (entrance) or after it leaves rest (exit). */
  ramp: number;
  easing: (progress: number) => number;
}

function envelope(spec: MotionEffect | undefined, side: 'enter' | 'exit', fps: number): Envelope | null {
  if (spec === undefined) return null;
  const options = typeof spec === 'string' ? { type: spec } : spec;
  if (!Object.prototype.hasOwnProperty.call(PRESETS, options.type)) {
    throw new Error(`unknown <Motion> ${side} ${JSON.stringify(options.type)}; expected ${Object.keys(PRESETS).join(', ')}`);
  }
  const duration = options.durationInFrames ?? Math.max(1, Math.round(fps / 2));
  if (!Number.isInteger(duration) || duration < 0) {
    throw new Error(`<Motion> ${side} durationInFrames must be a non-negative integer`);
  }
  if (options.distance !== undefined && (!Number.isFinite(options.distance) || options.distance < 0)) {
    throw new Error(`<Motion> ${side} distance must be a finite non-negative number`);
  }
  if (options.scaleFrom !== undefined && (!Number.isFinite(options.scaleFrom) || options.scaleFrom < 0)) {
    throw new Error(`<Motion> ${side} scaleFrom must be a finite non-negative number`);
  }
  const { type, direction } = PRESETS[options.type];
  return {
    effect: {
      type,
      slideFrom: direction === undefined ? undefined : (side === 'enter' ? OPPOSITE : TOWARD)[direction],
      distance: options.distance,
      scaleFrom: options.scaleFrom,
    },
    // Like <Transition>: an n-frame entrance is hidden on its first frame and
    // at rest on its last, so it moves over n - 1 frames.
    ramp: Math.max(0, duration - 1),
    easing: options.easing ?? Easings.easeOutCubic,
  };
}

/**
 * Entrance and exit ramps that fit a window of `frames`. When they would
 * overlap, both shrink in proportion until they meet on one frame, so the
 * element is always at rest on at least one frame.
 */
function fitRamps(enter: number, exit: number, frames: number): [number, number] {
  const room = Math.max(0, frames - 1);
  if (enter + exit <= room) return [enter, exit];
  const fitted = Math.round((enter * room) / (enter + exit));
  return [fitted, room - fitted];
}

function MotionGroup({ enter, exit, children, ...groupProps }: Omit<MotionProps, 'from' | 'durationInFrames'>) {
  const frame = useCurrentFrame();
  const { fps, durationInFrames } = useVideoConfig();
  const entrance = envelope(enter, 'enter', fps);
  const leaving = envelope(exit, 'exit', fps);
  const [enterRamp, exitRamp] = fitRamps(entrance?.ramp ?? 0, leaving?.ramp ?? 0, durationInFrames);

  let props: GroupProps = groupProps;
  if (entrance) {
    const linear = enterRamp === 0 ? 1 : clamp01(frame / enterRamp);
    props = applyTransition(props, entrance.effect, clamp01(entrance.easing(linear)));
  }
  if (leaving) {
    const start = durationInFrames - 1 - exitRamp;
    const linear = exitRamp === 0 ? 0 : clamp01((frame - start) / exitRamp);
    props = applyTransition(props, leaving.effect, clamp01(leaving.easing(1 - linear)));
  }
  return React.createElement(Group, props, children);
}

/**
 * Shows its children for a window of the enclosing sequence, easing them in
 * with `enter` and out with `exit`. In between they are drawn as written:
 * the effects multiply the group's own `opacity` and `scale` and offset its
 * `x`/`y`, and leave the children's props, clock, and media untouched.
 *
 * ```tsx
 * <Motion enter="fade-up" exit={{ type: 'fade-down', durationInFrames: 10 }}>
 *   <Text x={640} y={360} anchorX={0.5}>Hello</Text>
 * </Motion>
 * ```
 *
 * `scale` turns about the group's origin: place the Motion with `x`/`y` and
 * its children at 0, or set `originX`/`originY`. For a cut, cross-fade, or
 * wipe between whole scenes, use `<TransitionSeries>` instead.
 */
export function Motion({ from, durationInFrames, ...props }: MotionProps): ReturnType<typeof React.createElement> {
  const group = React.createElement(MotionGroup, props);
  if (from === undefined && durationInFrames === undefined) return group;
  return React.createElement(Sequence, { from, durationInFrames }, group);
}
