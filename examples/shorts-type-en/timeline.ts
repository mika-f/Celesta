import { computeSeries } from '@celesta/react';
import { BAR } from './constants';
import { Beat } from './scenes/Beat';
import { Emphasis } from './scenes/Emphasis';
import { Fit } from './scenes/Fit';
import { Frame } from './scenes/Frame';
import { Hook } from './scenes/Hook';
import { Measure } from './scenes/Measure';
import { Outro } from './scenes/Outro';

// Scene order and length. Every scene is two bars and starts on a downbeat;
// make-score.py places its hits on the same grid.
export const SCENES = [
  { name: 'HOOK', durationInFrames: BAR * 2, Scene: Hook },
  { name: 'FRAME', durationInFrames: BAR * 2, Scene: Frame },
  { name: 'MEASURE', durationInFrames: BAR * 2, Scene: Measure },
  { name: 'FIT', durationInFrames: BAR * 2, Scene: Fit },
  { name: 'SPAN', durationInFrames: BAR * 2, Scene: Emphasis },
  { name: 'BEAT', durationInFrames: BAR * 2, Scene: Beat },
  { name: 'OUTRO', durationInFrames: BAR * 2, Scene: Outro },
];
export const TIMING = computeSeries(SCENES);
export const SCENE_CUES = SCENES.map((s, i) => ({ at: TIMING.sequences[i].from, index: i, name: s.name }));
