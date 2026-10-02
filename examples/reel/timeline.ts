import type { ReactNode } from 'react';
import { BAR, BEAT, DURATION } from './constants';
import { Code } from './scenes/Code';
import { Export } from './scenes/Export';
import { Field } from './scenes/Field';
import { Intro } from './scenes/Intro';
import { Logo } from './scenes/Logo';
import { Silence } from './scenes/Silence';
import { Timeline } from './scenes/Timeline';
import { Words } from './scenes/Words';

// Scene boundaries, in frames.
export const S = {
  intro: 0,
  words: BAR,
  field: BAR * 3,
  code: BAR * 6,
  timeline: BAR * 10,
  exportAt: BAR * 12,
  silence: BAR * 13 + BEAT * 3,
  logo: BAR * 14,
} as const;

// Each scene runs from one boundary to the next.
export const SCENES: { from: number; to: number; Scene: () => ReactNode }[] = [
  { from: S.intro, to: S.words, Scene: Intro },
  { from: S.words, to: S.field, Scene: Words },
  { from: S.field, to: S.code, Scene: Field },
  { from: S.code, to: S.timeline, Scene: Code },
  { from: S.timeline, to: S.exportAt, Scene: Timeline },
  { from: S.exportAt, to: S.silence, Scene: Export },
  { from: S.silence, to: S.logo, Scene: Silence },
  { from: S.logo, to: DURATION, Scene: Logo },
];
