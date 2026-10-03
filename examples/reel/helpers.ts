import { Easings } from '@celesta/react';
import { FPS } from './constants';

export const clamp = (n: number, lo = 0, hi = 1) => Math.min(hi, Math.max(lo, n));
export const progress = (f: number, start: number, frames: number, easing = Easings.easeOutCubic) =>
  easing(clamp((f - start) / frames));
export const hash = (i: number, seed = 0) => {
  const s = Math.sin(i * 12.9898 + seed * 78.233) * 43758.5453;
  return s - Math.floor(s);
};
export const pad2 = (n: number) => String(Math.floor(n)).padStart(2, '0');
export const timecode = (f: number) =>
  `${pad2(f / (FPS * 3600))}:${pad2((f / (FPS * 60)) % 60)}:${pad2((f / FPS) % 60)}:${pad2(f % FPS)}`;
