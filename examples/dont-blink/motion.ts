import { Easings, progress } from '@celesta/react';

export const WIDTH = 1080;
export const HEIGHT = 1920;
export const FPS = 30;
export const FRAMES = 540;
export const INK = '#141418';
export const PAPER = '#F8F5EF';
export const RED = '#FF3B30';
export const LEFT = 84;
export const WORD_WIDTH = 800;
export const WORD_Y = 738;
export const TYPE_SIZE = 530;
// Includes Bebas Neue's rounded-letter overshoot at 530 px, not only flat caps.
export const CAP_HEIGHT = 382;
export const BANDS = 5;
export const BAND_HEIGHT = CAP_HEIGHT / BANDS;

/** Deliberate holds surround these clamped frame-based moves. */
export const move = (frame: number, start: number, end: number) =>
  progress(frame, start, end - start, Easings.easeInOutQuint);
export const hit = (frame: number, start: number, end: number) =>
  progress(frame, start, end - start, Easings.easeOutExpo);
export const mix = (a: number, b: number, t: number) => a + (b - a) * t;
export const direction = (index: number) => index % 2 ? -1 : 1;

export const CHAPTERS = [
  { at: 0, label: '01 / KEEP YOUR EYES OPEN', note: 'A CHALLENGE IN EIGHTEEN SECONDS.' },
  { at: 60, label: '02 / NOTHING STAYS STILL', note: 'THE SAME PIECES. A DIFFERENT WORD.' },
  { at: 180, label: '03 / LOOK CLOSER', note: 'DID SOMETHING JUST LOOK BACK?' },
  { at: 300, label: '04 / ONCE IS NEVER ENOUGH', note: 'FASTER THAN A SECOND THOUGHT.' },
  { at: 420, label: '05 / WAIT FOR IT', note: 'STILL HERE?' },
  { at: 510, label: '06 / CAUGHT YOU', note: 'GO ON. WATCH IT AGAIN.' },
] as const;
