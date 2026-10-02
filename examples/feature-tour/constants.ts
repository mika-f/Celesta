// FEATURE TOUR — canvas, timebase (120 BPM, one beat = 15 frames), palette, fonts and scene boundaries.

export const W = 1920;
export const H = 1080;
export const FPS = 30;
export const BEAT = 15;
export const BAR = BEAT * 4;
// Number of feature chapters. Keep equal to CHAPTERS.length (chapters/index.ts);
// it lives here so chapter demos need not import the registry that imports them.
export const CHAPTER_COUNT = 9;

export const C = {
  ink: '#07080C',
  panel: '#10121A',
  clip: '#171A24',
  line: '#FFFFFF1F',
  paper: '#F1F0EC',
  soft: '#B9BCC6',
  grey: '#80859A',
  dim: '#2A2E3A',
  blue: '#3D5BFF',
  sky: '#9DB0FF',
  pink: '#FF4D8D',
} as const;

export const FONT = {
  display: 'Unbounded',
  serif: 'Instrument Serif',
  mono: 'JetBrains Mono',
  ja: 'Noto Sans JP',
  jaDisplay: 'Dela Gothic One',
  dot: 'DotGothic16',
} as const;
// JetBrains Mono advances every glyph by 0.6 em, so mono text can be measured.
export const MONO_ADVANCE = 0.6;

// Scene boundaries, in frames. Every chapter starts on a downbeat.
export const S = {
  open: 0,
  index: BAR * 2,
  outro: BAR * 23,
} as const;
export const DURATION = BAR * 26;

// The demo area on the right of every chapter.
export const DX = 1000;
export const DW = 800;
