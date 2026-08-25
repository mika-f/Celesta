import type { ReactNode } from 'react';

import type { TextStyle } from './scene';

// These are never invoked as functions. render.ts walks the JSX element tree
// and matches host elements by reference against this module's exports, so
// they only need to exist as stable, unique identities. The prop types below
// exist purely for authoring: render.ts still reads props dynamically.

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

export function Composition(_props: CompositionProps): null {
  return null;
}

export function Group(_props: GroupProps): null {
  return null;
}

export function Image(_props: ImageProps): null {
  return null;
}

export function Text(_props: TextProps): null {
  return null;
}
