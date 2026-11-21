import { defineScenes } from '@celesta.studio/react';

import { BAR } from './constants';
import { Beat } from './scenes/Beat';
import { Outro } from './scenes/Outro';
import { Type } from './scenes/Type';

export const SCENES = defineScenes([
  { id: 'type', name: 'Type', durationInFrames: 2 * BAR, Scene: Type },
  { id: 'beat', name: 'Beat', durationInFrames: 2 * BAR, Scene: Beat },
  { id: 'outro', name: 'Outro', durationInFrames: 2 * BAR, Scene: Outro },
]);

/** The first frame of a scene, in the composition's frames. */
export const sceneStart = (id: string): number => SCENES.scenes.find(scene => scene.id === id)!.from;
