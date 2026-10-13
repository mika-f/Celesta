import type { ReactNode } from 'react';
import { placeholder } from './components/Placeholder';
import { DURATION } from './constants';
import { ColdOpen } from './scenes/ColdOpen';
import { Components } from './scenes/Components';
import { Idea } from './scenes/Idea';
import { LiveEdit } from './scenes/LiveEdit';
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
  export: placeholder('Export'),
  outro: placeholder('Outro'),
};

// Each scene runs from its start to the next scene's start.
export const SCENES: { from: number; to: number; Scene: () => ReactNode }[] = SCENE_ORDER.map((name, i) => ({
  from: S[name],
  to: i + 1 < SCENE_ORDER.length ? S[SCENE_ORDER[i + 1]] : DURATION,
  Scene: SCENE[name],
}));
