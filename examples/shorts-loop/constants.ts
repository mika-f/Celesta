// SHORTS LOOP — canvas, loop length, tempo, palette, and safe area.
// make-score.py reads FPS, BPM, BEATS_PER_BAR, and BARS from this file.

export const W = 1080;
export const H = 1920;
export const FPS = 30;
export const BPM = 75;
export const BEATS_PER_BAR = 4;
export const BARS = 4;
/** Frames per beat: 60 / 75 × 30 = 24. */
export const BEAT = (60 / BPM) * FPS;
/** Frames per bar: 96. */
export const BAR = BEAT * BEATS_PER_BAR;
/** 384 frames, 12.8 s. Frame DURATION would be frame 0 again. */
export const DURATION = BAR * BARS;

// The kaleidoscope's centre and outer radius.
export const CX = W / 2;
export const CY = 940;
export const RADIUS = 430;

// Shorts and TikTok draw their own UI over the bottom fifth and the right
// edge, and tabs over the top. Words stay inside these lines; the picture
// does not, and nothing draws them.
export const SAFE = { top: 180, bottom: H * 0.8, left: 96, right: W - 150 };

export const NIGHT = '#07051A';
export const INDIGO = '#1B1150';
export const VIOLET = '#7B5CFF';
export const CYAN = '#4DF3FF';
export const MAGENTA = '#FF4FD8';
export const AMBER = '#FFC56B';
export const WHITE = '#F4F1FF';
