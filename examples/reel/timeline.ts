import { defineScenes } from '@celesta.studio/react';
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

// Each scene runs from one boundary to the next; the HUD shows the names.
export const SCENES = defineScenes([
  { id: 'intro', name: 'COLD OPEN', durationInFrames: S.words - S.intro, Scene: Intro },
  { id: 'words', name: 'MANIFESTO', durationInFrames: S.field - S.words, Scene: Words },
  { id: 'field', name: 'DETERMINISTIC', durationInFrames: S.code - S.field, Scene: Field },
  { id: 'code', name: 'CODE', durationInFrames: S.timeline - S.code, Scene: Code },
  { id: 'timeline', name: 'TIMELINE', durationInFrames: S.exportAt - S.timeline, Scene: Timeline },
  { id: 'export', name: 'EXPORT', durationInFrames: S.silence - S.exportAt, Scene: Export },
  { id: 'silence', durationInFrames: S.logo - S.silence, Scene: Silence },
  { id: 'logo', durationInFrames: DURATION - S.logo, Scene: Logo },
]);
