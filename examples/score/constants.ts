import { typewriterFrames } from '@celesta.studio/text';

export const WIDTH = 1280;
export const HEIGHT = 720;
export const FPS = 30;
/** 120 BPM: one beat is 15 frames, one bar 60; every scene is two bars. */
export const GRID = { bpm: 120 } as const;
export const BEAT = 15;
export const BAR = 60;

/** What Type types, and when: the scene draws it and the score ticks on every character. */
export const COMMAND = 'npm run build';
export const TYPING = { from: 12, framesPerChar: 3, pauses: { ' ': 4 } } as const;
export const TYPED = typewriterFrames(COMMAND, TYPING);
/** The frame the command finishes typing on. */
export const TYPED_END = TYPED[TYPED.length - 1];

/** The words of Outro, one a beat from its second beat. */
export const WORDS = ['Sound', 'as', 'data'];

export const INK = '#14161C';
export const PAPER = '#F2EEE6';
export const MUTED = '#8C93A3';
export const ACCENT = '#E4572E';
