import { DURATION } from '../constants.ts';

// The Export scene's terminal: the command, typed by local frame TYPED_AT,
// renders this video's frames until DONE_AT (make-score.py rings a bell there).

export const EXPORT_COMMAND = 'celesta-exporter --react film.tsx intro.mp4';
export const TYPED_AT = 30;
export const DONE_AT = 120;

export function framesDone(local: number): number {
  const t = Math.min(1, Math.max(0, (local - TYPED_AT) / (DONE_AT - TYPED_AT)));
  return Math.round(DURATION * t * t * (3 - 2 * t));
}

export const AGENT_LOG = [
  'Read skills/celesta/SKILL.md',
  'Wrote examples/intro/film.tsx',
  'Rendered a contact sheet: 61 frames, 0 warnings',
  'Exported intro.mp4: 1800 frames',
];
