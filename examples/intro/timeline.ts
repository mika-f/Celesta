import type { ReactNode } from 'react';
import { DURATION } from './constants';
import { ColdOpen } from './scenes/ColdOpen';
import { Components } from './scenes/Components';
import { Export } from './scenes/Export';
import { Idea } from './scenes/Idea';
import { LiveEdit } from './scenes/LiveEdit';
import { Outro } from './scenes/Outro';
import { Problem } from './scenes/Problem';
import { Reveal } from './scenes/Reveal';
import { S, SCENE_ORDER, type SceneName } from './shared/cues';

const SCENE: Record<SceneName, () => ReactNode> = {
  coldOpen: ColdOpen,
  reveal: Reveal,
  problem: Problem,
  idea: Idea,
  components: Components,
  liveEdit: LiveEdit,
  export: Export,
  outro: Outro,
};

// Each scene runs from its start to the next scene's start.
export const SCENES: { from: number; to: number; Scene: () => ReactNode }[] = SCENE_ORDER.map((name, i) => ({
  from: S[name],
  to: i + 1 < SCENE_ORDER.length ? S[SCENE_ORDER[i + 1]] : DURATION,
  Scene: SCENE[name],
}));
