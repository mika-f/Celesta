import type { ReactNode } from 'react';
import { computeSeries } from '@celesta/react';
import { Balance } from './scenes/Balance';
import { Files } from './scenes/Files';
import { Intro } from './scenes/Intro';
import { Loop } from './scenes/Loop';
import { Nebula } from './scenes/Nebula';
import { Outro } from './scenes/Outro';
import { Same } from './scenes/Same';
import { Speed } from './scenes/Speed';
import { SyntaxEffects, SyntaxParticle } from './scenes/Syntax';

// Scenes back to back, lengths in frames at 30 fps.
export const SCENES: { name: string; durationInFrames: number; Scene: () => ReactNode }[] = [
  { name: 'intro', durationInFrames: 165, Scene: Intro },
  { name: 'the scene', durationInFrames: 225, Scene: Nebula },
  { name: 'same picture', durationInFrames: 180, Scene: Same },
  { name: 'syntax', durationInFrames: 300, Scene: SyntaxParticle },
  { name: 'syntax', durationInFrames: 300, Scene: SyntaxEffects },
  { name: 'files', durationInFrames: 210, Scene: Files },
  { name: 'ease', durationInFrames: 300, Scene: Loop },
  { name: 'speed', durationInFrames: 360, Scene: Speed },
  { name: 'balance', durationInFrames: 240, Scene: Balance },
  { name: 'celesta', durationInFrames: 180, Scene: Outro },
];

export const SERIES = computeSeries(SCENES);
export const CUTS = SERIES.sequences.slice(1).map((s) => s.from);
export const SECTIONS = SERIES.sequences.map((s, i) => ({ name: SCENES[i].name, from: s.from }));
