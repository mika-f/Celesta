import { BAR } from '../constants.ts';

// Scene starts, in composition frames. Every scene starts on a bar line.
export const S = {
  coldOpen: 0,
  reveal: BAR * 3,
  problem: BAR * 5,
  idea: BAR * 9,
  components: BAR * 13,
  liveEdit: BAR * 17,
  export: BAR * 22,
  outro: BAR * 26,
} as const;

export type SceneName = keyof typeof S;

export const SCENE_ORDER: SceneName[] = [
  'coldOpen', 'reveal', 'problem', 'idea', 'components', 'liveEdit', 'export', 'outro',
];

// From 55 s nothing moves, leaving YouTube's end-screen its 5 seconds.
export const HOLD = 1650;

export function sceneAt(frame: number): SceneName {
  return [...SCENE_ORDER].reverse().find((name) => frame >= S[name]) ?? 'coldOpen';
}
