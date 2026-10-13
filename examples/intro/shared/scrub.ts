// Where the Idea scene's scrub bar points, in composition-relative frames of
// the ball, at each of the scene's local frames (0–239). It sweeps back and
// forth so frame numbers repeat; make-score.py plucks at each turn.

export const SCRUB_TURNS = [60, 110, 170];
const STOPS = [0, ...SCRUB_TURNS, 239];
const TARGETS = [0, 89, 30, 120, 60];

const easeInOutCubic = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2);

export function scrubFrame(local: number): number {
  const l = Math.min(Math.max(local, 0), 239);
  const i = Math.max(0, STOPS.findIndex((stop, k) => l >= stop && l <= STOPS[k + 1]));
  const t = (l - STOPS[i]) / (STOPS[i + 1] - STOPS[i]);
  return Math.round(TARGETS[i] + (TARGETS[i + 1] - TARGETS[i]) * easeInOutCubic(t));
}
