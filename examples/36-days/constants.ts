// 36 DAYS — canvas, timebase (120 BPM, one beat = 15 frames), palette and fonts.

export const W = 1920;
export const H = 1080;
export const FPS = 30;
export const BPM = 120;
export const BEAT = 15;
export const BAR = BEAT * 4;

export const C = {
  ink: '#0A0F0D',
  line: '#FFFFFF1F',
  paper: '#ECEFE8',
  soft: '#B4BDB6',
  grey: '#7D867F',
  dim: '#26302B',
  mint: '#7CF29C',
  coral: '#FF6B4A',
} as const;

export const FONT = {
  display: 'Archivo Black',
  mono: 'JetBrains Mono',
  ja: 'Noto Sans JP',
} as const;
// JetBrains Mono advances every glyph by 0.6 em, so mono text can be measured.
export const MONO_ADVANCE = 0.6;
