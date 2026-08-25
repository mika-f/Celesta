import * as React from 'react';
import type { ReactNode } from 'react';

import type { TextStyle } from './scene';

// Unlike the pre-reconciler tree walker, these are real function components:
// each returns a host element (a lowercase intrinsic type the reconciler in
// reconciler.ts understands), so ordinary React composition — conditionals,
// `.map()`, context, hooks — works through them exactly as it would for any
// other component.

export interface CommonProps {
  id?: string;
  x?: number;
  y?: number;
  scale?: number;
  scaleX?: number;
  scaleY?: number;
  rotation?: number;
  anchorX?: number;
  anchorY?: number;
  opacity?: number;
}

export interface CompositionProps {
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
  children?: ReactNode;
}

export interface GroupProps extends CommonProps {
  children?: ReactNode;
}

export interface ImageProps extends CommonProps {
  src: string;
}

export interface TextProps extends CommonProps {
  children: ReactNode;
  style?: TextStyle;
  maxWidth?: number;
}

export function Composition(props: CompositionProps): ReturnType<typeof React.createElement> {
  return React.createElement('composition', props);
}

export function Group(props: GroupProps): ReturnType<typeof React.createElement> {
  return React.createElement('group', props);
}

export function Image(props: ImageProps): ReturnType<typeof React.createElement> {
  return React.createElement('image', props);
}

export function Text(props: TextProps): ReturnType<typeof React.createElement> {
  return React.createElement('text', props);
}
