// SHORTS TYPE (EN) — vertical canvas, timebase (120 BPM, one beat = 15
// frames), safe area, palette and fonts.

export const W = 1080;
export const H = 1920;
export const FPS = 30;
export const BPM = 120;
export const BEAT = 15;
export const BAR = BEAT * 4;

// Shorts and TikTok draw their caption, buttons and progress bar over the
// bottom ~20% and the right edge, and a header over the top. Every scene lays
// its copy out inside this inset (local coordinates start at its top-left).
export const SAFE = { top: 192, right: 200, bottom: 384, left: 72 } as const;
export const AREA = { width: W - SAFE.left - SAFE.right, height: H - SAFE.top - SAFE.bottom } as const;
// The text column, kept narrower than the area so a camera push-in of a few
// percent still stays inside it.
export const COL = { x: 24, width: 760 } as const;
// The top of the safe area holds the HUD; copy starts below it.
export const TOP = 72;

export const C = {
  ink: '#0E0E11',
  paper: '#F2EFE8',
  hot: '#FF4D1F',
  sun: '#FFD23F',
  soft: '#BDB9B0',
  grey: '#7C7C86',
  dim: '#2C2C33',
} as const;
// A palette colour with alpha, as #RRGGBBAA.
export const tint = (color: string, alpha: number) =>
  color + Math.round(alpha * 255).toString(16).padStart(2, '0').toUpperCase();

export const FONT = {
  // Archivo Black has a single weight; it sets every fitted headline.
  display: 'Archivo Black',
  body: 'Inter',
  mono: 'JetBrains Mono',
} as const;
// Every weight drawn is listed, so no face falls back to its neighbour.
export const FONT_SRC =
  'https://fonts.googleapis.com/css2?family=Archivo+Black&family=Inter:wght@400;700&family=JetBrains+Mono:wght@500;700';
