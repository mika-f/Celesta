// INTRO — canvas, timebase (120 BPM, one beat = 15 frames), palette and fonts.
// Imported by shared/ and by node --test, so it imports nothing.

export const W = 1920;
export const H = 1080;
export const FPS = 30;
export const BEAT = 15;
export const BAR = BEAT * 4;
export const DURATION = BAR * 30;

// The website's Noir theme, dark mode (packages/website/theme/noir.css).
export const C = {
  bg: '#000000',
  fg: '#F5F5F7',
  accent: '#3D7BFF',
  muted: '#6B6B70',
  line: '#1C1C1E',
  panel: '#0D0D0F',
} as const;

export const FONT = {
  display: 'Space Grotesk',
  mono: 'JetBrains Mono',
} as const;
export const FONT_SRC =
  'https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@700&family=JetBrains+Mono:wght@400;700';
