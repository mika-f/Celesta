// SHORTS TIPS — canvas, safe area, timebase (120 BPM, one beat = 15 frames), palette and fonts.

export const W = 1080;
export const H = 1920;
export const FPS = 30;
export const BPM = 120;
export const BEAT = 15;
export const BAR = BEAT * 4;
// Twelve bars, 24 s. Every variant has the same length so one score fits all.
export const DURATION = BAR * 12;

// Where Shorts / TikTok / Reels draw nothing over the picture. The bottom
// 20 % holds the caption and channel name, the right edge the like and share
// buttons. Text the viewer must read stays inside this box; backgrounds,
// panels and the demos' pictures use the whole frame.
export const SAFE = { left: 64, top: 120, right: W - 144, bottom: H * 0.8 } as const;
export const SAFE_W = SAFE.right - SAFE.left;

// Three full-width bands, top to bottom: the header, the code (together the
// upper half) and the demo (the lower half).
export const HEADER = { y: SAFE.top, titleY: 200, titleSize: 64, titleLine: 82 } as const;
export const CODE = { y: 384, height: H / 2 - 384, bar: 56, textX: SAFE.left + 8, padY: 24 } as const;
// A demo draws in its own coordinates: (0, 0) is the band's top-left, the
// band is W × DEMO.height, and its text stays within DEMO.safe.
export const DEMO = {
  y: H / 2,
  height: H / 2,
  safe: { left: SAFE.left, right: SAFE.right, bottom: SAFE.bottom - H / 2 },
} as const;

// Code is set in JetBrains Mono, which advances every glyph by 0.6 em, so
// columns convert to pixels without shaping (CJK falls back at ~1 em). The
// size follows the snippet's longest line, between these bounds.
export const MONO_ADVANCE = 0.6;
export const CODE_SIZE = { max: 46, min: 34 } as const;
export const CODE_MAX_LINES = 9;

// Scene boundaries (TransitionSeries), in frames. Each transition is
// centred on a downbeat: the wipe on bar 1, the cross-fade on bar 8.
export const HOOK_LEN = BAR + 6;
export const WIPE = 12;
export const BUILD_LEN = BAR * 7 + 12;
export const FADE = 12;
export const OUTRO_LEN = DURATION - HOOK_LEN - BUILD_LEN + WIPE + FADE;
export const BUILD_AT = HOOK_LEN - WIPE;
export const OUTRO_AT = BUILD_AT + BUILD_LEN - FADE;

// Code lines are revealed on beats from REVEAL_AT, spread over REVEAL_BEATS
// beats; each line types in over TYPE_FRAMES.
export const REVEAL_AT = BAR + BEAT * 2;
export const REVEAL_BEATS = 18;
export const TYPE_FRAMES = 10;

export const C = {
  ink: '#0A0C12',
  panel: '#141824',
  stage: '#10131D',
  bar: '#1B2030',
  edge: '#FFFFFF14',
  paper: '#F3F1EC',
  soft: '#B8BDCB',
  grey: '#7D8396',
  dim: '#2A3042',
  bad: '#FF5C6C',
} as const;

export const FONT = {
  display: 'Unbounded',
  ja: 'Noto Sans JP',
  mono: 'JetBrains Mono',
} as const;

// Titles, hooks and closing lines come from the variant files, so the
// Japanese face is loaded whole rather than subset to the glyphs used.
export const FONT_URLS = [
  'https://fonts.googleapis.com/css2?family=Unbounded:wght@800&family=JetBrains+Mono:wght@400;700',
  'https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@500;700;900',
] as const;
