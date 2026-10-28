// What the text components that bring their pieces in one after another
// (TitleCard, LowerThird) share: their solid fills, and how each piece is
// wrapped in a <Motion> starting `stagger` frames after the one before.

import * as React from 'react';
import type { ReactNode } from 'react';

import { Group } from '@celesta/react';
import type { GroupProps, TextStyle } from '@celesta/react';

import { Motion } from './motion';
import type { MotionEffect } from './motion';

/** The entrance, exit, and stagger props of a component with staggered pieces. */
export interface StaggerTiming {
  enter: MotionEffect | null;
  exit: MotionEffect | null;
  /** Frames between one piece's entrance and the next. */
  stagger: number;
}

/** A solid text fill in `color`. */
export function solid(color: string): TextStyle['fill'] {
  return { type: 'solid', color };
}

/** The default stagger: a fifth of a second, or 0 when the pieces cut in. */
export function defaultStagger(enter: MotionEffect | null, fps: number): number {
  return enter === null ? 0 : Math.round(fps / 5);
}

/**
 * Piece `index` of a staggered block at `at`: a `<Motion>` that enters
 * `index * stagger` frames into the enclosing window and leaves at its end,
 * or a plain `<Group>` when nothing moves.
 */
export function staggeredPiece(
  index: number,
  at: GroupProps & { key: string },
  { enter, exit, stagger }: StaggerTiming,
  children: ReactNode,
): ReturnType<typeof React.createElement> {
  if (enter === null && exit === null && stagger === 0) return React.createElement(Group, at, children);
  return React.createElement(Motion, {
    ...at,
    from: stagger > 0 && index > 0 ? index * stagger : undefined,
    enter: enter ?? undefined,
    exit: exit ?? undefined,
  }, children);
}
