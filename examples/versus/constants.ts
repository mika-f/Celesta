// VERSUS: canvas, palette and fonts.

export const W = 1920;
export const H = 1080;
export const FPS = 30;

export const C = {
  bg: '#05060C',
  panel: '#0C0F1C',
  panel2: '#131830',
  line: '#FFFFFF16',
  ink: '#F2F4FA',
  soft: '#AAB3CC',
  grey: '#66708C',
  dim: '#252C44',
} as const;

// One color per tool, used everywhere it appears.
export const TOOL = {
  remotion: { name: 'Remotion', color: '#4C9BFF', stack: 'React · Chromium', lang: 'TSX' },
  fframes: { name: 'fframes', color: '#FF8A3D', stack: 'Rust · Skia on Vulkan', lang: 'Rust' },
  celesta: { name: 'Celesta', color: '#B39CFF', stack: 'React · wgpu', lang: 'TSX' },
} as const;
export type ToolId = keyof typeof TOOL;
export const ORDER: ToolId[] = ['remotion', 'fframes', 'celesta'];

export const FONT = {
  sans: 'Space Grotesk',
  mono: 'JetBrains Mono',
} as const;
export const FONT_SRC =
  'https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;700&family=JetBrains+Mono:wght@400;700';
