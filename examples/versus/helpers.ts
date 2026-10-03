import { Easings } from '@celesta/react';

export const clamp = (n: number, lo = 0, hi = 1) => Math.min(hi, Math.max(lo, n));
export const progress = (f: number, start: number, frames: number, easing = Easings.easeOutCubic) =>
  easing(clamp((f - start) / frames));
// 1 while a scene shows, easing in at its start and out at its end.
export const presence = (f: number, total: number, inFrames = 14, outFrames = 12) =>
  progress(f, 0, inFrames, Easings.easeOutExpo) * (1 - progress(f, total - outFrames, outFrames, Easings.easeInCubic));
