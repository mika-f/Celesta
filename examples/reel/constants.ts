// REEL — canvas, timebase (120 BPM, one beat = 15 frames), palette and fonts.

export const W = 1920;
export const H = 1080;
export const FPS = 30;
export const BEAT = 15;
export const BAR = BEAT * 4;
export const DURATION = BAR * 16;

export const C = {
  ink: '#0B0B0D',
  panel: '#151518',
  clip: '#1C1C21',
  paper: '#EDEBE6',
  accent: '#FF4D1F',
  tint: '#FFB29C',
  soft: '#C9C5BC',
  grey: '#8B8B90',
  dim: '#3A3A40',
} as const;

export const FONT = {
  display: 'Space Grotesk',
  mono: 'JetBrains Mono',
} as const;
export const FONT_SRC =
  'https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@500;700&family=JetBrains+Mono:wght@400;700';
