import { Easings, progress } from '@celesta.studio/react';

// The DELTA look, shared by every release film in this folder: ink black,
// one orange, paper white; Bebas Neue headlines, Space Grotesk copy and
// JetBrains Mono code. 1920×1080 at 30 fps, cut to a 120 BPM score
// (one beat = 15 frames, one bar = 60 frames).
export const W = 1920;
export const H = 1080;
export const FPS = 30;
export const BEAT = 15;
export const BAR = 60;

export const C = {
  ink: '#0C0D0F',
  panel: '#121317',
  raised: '#17191E',
  line: '#26282E',
  dim: '#5C606A',
  grey: '#8E929B',
  paper: '#EFECE4',
  orange: '#F26A2E',
  amber: '#FFB547',
  green: '#3DDC84',
  red: '#FF5F56',
  blue: '#5AA9FF',
  violet: '#9B87F5',
} as const;

// Font stacks: Latin from the first family, Japanese from Noto Sans JP, so
// the Japanese PR titles and captions never fall back to a system font.
export const FONT = {
  display: ['Bebas Neue', 'Noto Sans JP'],
  sans: ['Space Grotesk', 'Noto Sans JP'],
  mono: ['JetBrains Mono', 'Noto Sans JP'],
} as const;

export const LATIN_FONTS = 'https://fonts.googleapis.com/css2?family=Bebas+Neue'
  + '&family=Space+Grotesk:wght@400;500;700&family=JetBrains+Mono:wght@400;700';

// The characters Noto Sans JP loads with <Font google text>: Latin comes
// from the first family of each stack, so only the Japanese glyphs a film
// draws are kept and the face loads as a few small files.
export const japaneseText = (strings: readonly string[]) => strings.join('').replace(/[\0-\u2000]/g, '');

export const solid = (color: string) => ({ type: 'solid' as const, color });

export const ease = (frame: number, start: number, duration: number) =>
  progress(frame, start, duration, Easings.easeOutExpo);
