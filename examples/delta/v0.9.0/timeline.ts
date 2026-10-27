import { defineScenes } from '@celesta/react';

// Where things land, in composition frames. 0.9.0 is cut to a 100 BPM
// newsroom theme: one beat = 18 frames, one bar = 72 frames.
export const MUSIC_BEAT = 18;
export const MUSIC_BAR = 4 * MUSIC_BEAT;

// The demos count on a grid of half that: BEAT is an eighth note (9
// frames) and BAR two beats (36 frames), so a chapter's 144 frames are four
// of their "bars".
export const BEAT = 9;
export const BAR = 4 * BEAT;

/** Four bars on the multiviewer, before the camera dives into the program monitor. */
export const OPEN_LENGTH = 4 * MUSIC_BAR;
/** Twelve chapters of two bars, then MORE: twelve cards of two beats. */
export const CHAPTER_LENGTH = 2 * MUSIC_BAR;
export const FEATURES = 12;
export const MORE_CARDS = 12;
export const CARD_LENGTH = 2 * MUSIC_BEAT;
export const CHAPTER_LENGTHS: readonly number[] = [...Array<number>(FEATURES).fill(CHAPTER_LENGTH), MORE_CARDS * CARD_LENGTH];
export const CHAPTER_COUNT = CHAPTER_LENGTHS.length;
export const OUTRO_LENGTH = 4 * MUSIC_BAR;

/** The frame chapter `i` goes on air (its downbeat). */
export const chapterAt = (i: number) => OPEN_LENGTH + CHAPTER_LENGTHS.slice(0, i).reduce((sum, n) => sum + n, 0);
export const MORE_AT = chapterAt(FEATURES);
export const OUTRO_AT = chapterAt(CHAPTER_COUNT);
export const DURATION = OUTRO_AT + OUTRO_LENGTH;

/** The chapters' titles, in order; reel.tsx checks them against chapters.tsx. */
export const TITLES = ['LOWER THIRD', 'CALLOUT', 'CAPTIONS', 'DIALOGUE', 'FADES', 'DUCKING', 'LOUDNESS',
  'PROPERTIES', 'CANVAS', 'EDIT SCOPE', 'TIMELINE', 'ADD', 'MORE'] as const;

/** The chapters a voice speaks in, and how far into the chapter it starts. */
export const VOICE_CHAPTERS = { captions: 2, dialogue: 3, ducking: 5 } as const;
export const VOICE_DELAY = 14;

/** The film's scenes by name: the frame chrome, the preview's chapter list and MORE's SCENES card read them. */
export const SCENE_LIST = defineScenes([
  { id: 'open', name: 'ON AIR', durationInFrames: OPEN_LENGTH },
  ...CHAPTER_LENGTHS.map((length, i) => ({ id: `chapter-${i}`, name: TITLES[i], durationInFrames: length })),
  { id: 'outro', name: 'CELESTA 0.9.0', durationInFrames: OUTRO_LENGTH },
]);
