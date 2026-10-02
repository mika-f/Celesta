import { Easings } from '@celesta/react';
import { FPS, MONO_ADVANCE } from './constants';

export const clamp = (n: number, lo = 0, hi = 1) => Math.min(hi, Math.max(lo, n));
export const progress = (f: number, start: number, frames: number, easing = Easings.easeOutCubic) =>
  easing(clamp((f - start) / frames));
export const hash = (i: number, seed = 0) => {
  const s = Math.sin(i * 12.9898 + seed * 78.233) * 43758.5453;
  return s - Math.floor(s);
};
export const pad = (n: number, width = 2) => String(Math.max(0, Math.floor(n))).padStart(width, '0');
export const timecode = (f: number) =>
  `${pad(f / (FPS * 60))}:${pad((f / FPS) % 60)}:${pad(f % FPS)}`;
export const monoWidth = (text: string, size: number) => text.length * size * MONO_ADVANCE;
