import { createElement } from 'react';
import type { ReactNode } from 'react';

import { clamp01 } from '@celesta/math';
import { Group, useCurrentFrame, useVideoConfig } from '@celesta/react';
import type { CommonProps } from '@celesta/react';

export type TransitionType = 'fade' | 'slide' | 'scale';
export type TransitionDirection = 'in' | 'out';
export type SlideFrom = 'left' | 'right' | 'top' | 'bottom';

/** What a transition does to a group, apart from its timing. */
export interface TransitionEffect {
  /** One effect, or several applied together (`['fade', 'slide']`). */
  type: TransitionType | readonly TransitionType[];
  /** Side a slide comes from at progress 0. Defaults to `'left'`. */
  slideFrom?: SlideFrom;
  /** Slide distance in pixels at progress 0. Defaults to 64. */
  distance?: number;
  /** Scale at progress 0. Defaults to 0.8. */
  scaleFrom?: number;
}

export interface TransitionProps extends CommonProps, TransitionEffect {
  durationInFrames: number;
  direction?: TransitionDirection;
  easing?: (progress: number) => number;
  children?: ReactNode;
}

/**
 * The group props `<Transition>` draws at `progress` (0 hidden, 1 at rest),
 * on top of the group's own: opacity and scale multiply, the slide offset
 * adds to `x`/`y`. Applying two effects in turn composes them the same way.
 */
export function applyTransition<P extends CommonProps>(
  props: P,
  { type, slideFrom = 'left', distance = 64, scaleFrom = 0.8 }: TransitionEffect,
  progress: number,
): P {
  const types: readonly TransitionType[] = typeof type === 'string' ? [type] : type;
  for (const each of types) {
    if (each !== 'fade' && each !== 'slide' && each !== 'scale') {
      throw new Error(`unknown <Transition> type ${JSON.stringify(each)}; expected fade, slide, or scale`);
    }
  }

  const opacity = (props.opacity ?? 1) * (types.includes('fade') ? progress : 1);
  let x = props.x ?? 0;
  let y = props.y ?? 0;
  if (types.includes('slide')) {
    const offset = distance * (1 - progress);
    if (slideFrom === 'left') x -= offset;
    if (slideFrom === 'right') x += offset;
    if (slideFrom === 'top') y -= offset;
    if (slideFrom === 'bottom') y += offset;
  }

  const scale = types.includes('scale') ? scaleFrom + (1 - scaleFrom) * progress : 1;
  return {
    ...props,
    x,
    y,
    opacity,
    scale: (props.scale ?? 1) * scale,
    scaleX: props.scaleX === undefined ? undefined : props.scaleX * scale,
    scaleY: props.scaleY === undefined ? undefined : props.scaleY * scale,
  };
}

/** Applies a small frame-based entrance or exit without adding renderer primitives. */
export function Transition({
  type,
  durationInFrames,
  direction = 'in',
  slideFrom,
  distance,
  scaleFrom,
  easing = (progress) => progress,
  children,
  ...groupProps
}: TransitionProps): ReturnType<typeof Group> {
  if (!Number.isInteger(durationInFrames) || durationInFrames <= 0) {
    throw new Error('<Transition> requires a positive integer `durationInFrames` prop');
  }

  const frame = useCurrentFrame();
  const { durationInFrames: windowDuration } = useVideoConfig();
  const duration = Math.min(durationInFrames, windowDuration);
  const start = direction === 'out' ? windowDuration - duration : 0;
  const linear = duration === 1 ? (frame >= start ? 1 : 0) : clamp01((frame - start) / (duration - 1));
  const progress = clamp01(easing(direction === 'out' ? 1 - linear : linear));

  return createElement(Group, applyTransition(groupProps, { type, slideFrom, distance, scaleFrom }, progress), children);
}
