import { createElement } from 'react';
import type { ReactNode } from 'react';

import { Group } from './components';
import type { CommonProps } from './components';
import { useCurrentFrame, useVideoConfig } from './hooks';

export type TransitionType = 'fade' | 'slide' | 'scale';
export type TransitionDirection = 'in' | 'out';
export type SlideFrom = 'left' | 'right' | 'top' | 'bottom';

export interface TransitionProps extends CommonProps {
  type: TransitionType;
  durationInFrames: number;
  direction?: TransitionDirection;
  slideFrom?: SlideFrom;
  distance?: number;
  scaleFrom?: number;
  easing?: (progress: number) => number;
  children?: ReactNode;
}

function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value));
}

/** Applies a small frame-based entrance or exit without adding renderer primitives. */
export function Transition({
  type,
  durationInFrames,
  direction = 'in',
  slideFrom = 'left',
  distance = 64,
  scaleFrom = 0.8,
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

  const opacity = (groupProps.opacity ?? 1) * (type === 'fade' ? progress : 1);
  let x = groupProps.x ?? 0;
  let y = groupProps.y ?? 0;
  if (type === 'slide') {
    const offset = distance * (1 - progress);
    if (slideFrom === 'left') x -= offset;
    if (slideFrom === 'right') x += offset;
    if (slideFrom === 'top') y -= offset;
    if (slideFrom === 'bottom') y += offset;
  }

  const scale = type === 'scale' ? scaleFrom + (1 - scaleFrom) * progress : 1;
  return createElement(
    Group,
    {
      ...groupProps,
      x,
      y,
      opacity,
      scale: (groupProps.scale ?? 1) * scale,
      scaleX: groupProps.scaleX === undefined ? undefined : groupProps.scaleX * scale,
      scaleY: groupProps.scaleY === undefined ? undefined : groupProps.scaleY * scale,
    },
    children,
  );
}
