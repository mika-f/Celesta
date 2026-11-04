import { defineScenes } from '@celesta.studio/react';

// Where things land, in composition frames. 0.8.0 is cut to a 90 BPM score:
// one beat = 20 frames, one bar = 80 frames.
export const MUSIC_BEAT = 20;
export const MUSIC_BAR = 4 * MUSIC_BEAT;

// The demos count on a grid of half that: BEAT is an eighth note (10
// frames) and BAR two beats (40 frames), so a chapter's 160 frames are four
// of their "bars".
export const BEAT = 10;
export const BAR = 4 * BEAT;

/** Four bars on the sheet, before the camera lands on the first tile. */
export const OPEN_LENGTH = 4 * MUSIC_BAR;
/** Twelve chapters of two bars, then MORE: eight cards of two beats. */
export const CHAPTER_LENGTH = 2 * MUSIC_BAR;
export const FEATURES = 12;
export const MORE_CARDS = 8;
export const CARD_LENGTH = 2 * MUSIC_BEAT;
export const CHAPTER_LENGTHS: readonly number[] = [...Array<number>(FEATURES).fill(CHAPTER_LENGTH), MORE_CARDS * CARD_LENGTH];
export const CHAPTER_COUNT = CHAPTER_LENGTHS.length;
export const OUTRO_LENGTH = 4 * MUSIC_BAR;

/** The frame chapter `i`'s tile fills the screen (its downbeat). */
export const chapterAt = (i: number) => OPEN_LENGTH + CHAPTER_LENGTHS.slice(0, i).reduce((sum, n) => sum + n, 0);
export const MORE_AT = chapterAt(FEATURES);
export const OUTRO_AT = chapterAt(CHAPTER_COUNT);
export const DURATION = OUTRO_AT + OUTRO_LENGTH;

/** The chapters' titles, in order; reel.tsx checks them against chapters.tsx. */
export const TITLES = ['3D', 'LOTTIE', 'COMPONENTS', 'TITLES', 'THEME', 'STROKES', 'METRICS', 'PITCH',
  'FALLBACK', 'EDGES', 'EXPORTER', 'VOICES', 'MORE'] as const;

/** The film's scenes by name, for useScene() / sceneAt() (#419): the frame chrome and MORE's SCENES card read them. */
export const SCENE_LIST = defineScenes([
  { id: 'open', name: 'PROOF', durationInFrames: OPEN_LENGTH },
  ...CHAPTER_LENGTHS.map((length, i) => ({ id: `chapter-${i}`, name: TITLES[i], durationInFrames: length })),
  { id: 'outro', name: 'CELESTA 0.8.0', durationInFrames: OUTRO_LENGTH },
]);
