import { noise } from '@celesta/math';
import * as React from 'react';
import type { ReactNode } from 'react';

import { Group, useCurrentFrame, useVideoConfig } from '@celesta/react';
import type { BlendMode } from '@celesta/react';

import { useLayoutBounds } from './layout';

export interface CameraProps {
  /** The point of the world shown at the center of the frame. Defaults to the center of the current area. */
  x?: number;
  y?: number;
  /** Magnification about that point. Defaults to 1. */
  zoom?: number;
  /** Degrees; positive turns the world clockwise. Defaults to 0. */
  rotation?: number;
  /** Handheld shake: the largest drift from the focus point, in world pixels. Defaults to 0. */
  shake?: number;
  /** How many times a second the shake changes direction. Defaults to 2. */
  shakeFrequency?: number;
  /** Seed for the shake, so two cameras do not shake in unison. */
  seed?: number | string;
  opacity?: number;
  blendMode?: BlendMode;
  id?: string;
  children?: ReactNode;
}

/**
 * A virtual camera over its children: lay the world out in its own
 * coordinates, then move `x`/`y` to look at a point, `zoom` in, tilt with
 * `rotation`, or add a little `shake`. Inside a `<SafeArea>` or `<Fit>` the
 * focus point lands in the middle of that area.
 *
 * ```tsx
 * <Camera x={interpolate(frame, [0, 60], [0, 3000])} y={540} zoom={1.2}>
 *   <World />
 * </Camera>
 * ```
 */
export function Camera({
  x,
  y,
  zoom = 1,
  rotation = 0,
  shake = 0,
  shakeFrequency = 2,
  seed = 0,
  opacity,
  blendMode,
  id,
  children,
}: CameraProps): ReturnType<typeof React.createElement> {
  if (!Number.isFinite(zoom) || zoom <= 0) {
    throw new Error('<Camera> requires a positive finite `zoom` prop');
  }
  const { width, height } = useLayoutBounds();
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const t = (frame / fps) * shakeFrequency;
  const focusX = (x ?? width / 2) + (shake === 0 ? 0 : shake * noise(`${seed}:x`, t));
  const focusY = (y ?? height / 2) + (shake === 0 ? 0 : shake * noise(`${seed}:y`, t));
  return React.createElement(
    Group,
    { id, opacity, blendMode, x: width / 2, y: height / 2, scale: zoom, rotation },
    React.createElement(Group, { x: -focusX, y: -focusY }, children),
  );
}
