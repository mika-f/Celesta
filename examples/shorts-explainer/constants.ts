// SHORTS EXPLAINER — canvas, palette, fonts, and the vertical layout.

export const W = 1080;
export const H = 1920;
export const FPS = 30;

export const C = {
  ink: '#1B1E2B',
  paper: '#FFFDF7',
  cream: '#FFF4DC',
  sky: '#DDF1FF',
  line: '#1B1E2B1A',
  soft: '#6B7083',
  code: '#22263A',
  codeLine: '#FFFFFF14',
  codeText: '#E8EAF2',
  codeDim: '#8A90A8',
  pink: '#FF5D8F',
  yellow: '#FFC93C',
  mint: '#2EC4A6',
  blue: '#3D7BFF',
  violet: '#8B6CFF',
} as const;

export const FONT = {
  ja: 'Noto Sans JP',
  display: 'Mochiy Pop One',
  mono: 'JetBrains Mono',
} as const;

// Shorts / TikTok draw their own buttons over the right edge and their
// caption, progress bar and tabs over the bottom ~20%. Pictures use the whole
// canvas; only text the viewer must read (subtitles, headings, labels) is
// kept inside SAFE. Nothing marks SAFE on screen.
export const SAFE = {
  left: 64,
  right: W - 150,
  top: 130,
  bottom: Math.round(H * 0.8),
} as const;
export const SAFE_W = SAFE.right - SAFE.left;
export const SAFE_CX = (SAFE.left + SAFE.right) / 2;

// Top half: the explainer panel, edge to edge. Its contents are laid out in
// PANEL's coordinates; x = SAFE.left … SAFE.right is where text may go.
export const PANEL = { x: 0, y: 0, w: W, h: 900 } as const;
// Middle: the subtitle, hanging from SUB.y (up to three lines), with the
// speaker's name tag just above it.
export const SUB = { x: SAFE_CX, y: 1000, maxWidth: SAFE_W - 24, size: 74 } as const;
export const TAG_Y = SUB.y - 44;
// Bottom half: the portraits, heads just under the subtitle.
export const STAGE_Y = 1190;
