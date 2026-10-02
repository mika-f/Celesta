import { computeSeries } from '@celesta/react';
import { BAR } from './constants';
import { Activity } from './scenes/Activity';
import { Crates } from './scenes/Crates';
import { Milestones } from './scenes/Milestones';
import { Numbers } from './scenes/Numbers';
import { Open } from './scenes/Open';
import { Outro } from './scenes/Outro';
import { Title } from './scenes/Title';

// Scene order and length (in bars). Each scene starts on a downbeat.
export const SCENES = [
  { name: 'COLD OPEN', durationInFrames: BAR * 2, Scene: Open },
  { name: 'TITLE', durationInFrames: BAR * 2, Scene: Title },
  { name: 'NUMBERS', durationInFrames: BAR * 3, Scene: Numbers },
  { name: 'ACTIVITY', durationInFrames: BAR * 4, Scene: Activity },
  { name: 'MILESTONES', durationInFrames: BAR * 5, Scene: Milestones },
  { name: 'CRATES', durationInFrames: BAR * 3, Scene: Crates },
  { name: 'NEXT', durationInFrames: BAR * 3, Scene: Outro },
];
export const TIMING = computeSeries(SCENES);
export const SCENE_CUES = SCENES.map((s, i) => ({ at: TIMING.sequences[i].from, index: i, name: s.name }));
