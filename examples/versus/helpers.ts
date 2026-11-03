import { Easings, progress } from '@celesta.studio/react';

// 1 while a scene shows, easing in at its start and out at its end.
export const presence = (f: number, total: number, inFrames = 14, outFrames = 12) =>
  progress(f, 0, inFrames, Easings.easeOutExpo) * (1 - progress(f, total - outFrames, outFrames, Easings.easeInCubic));
