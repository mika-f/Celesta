import { defineScenes } from '@celesta.studio/react';
import { BAR, BEAT } from '../components/theme';

// Where things land, in composition frames. 0.10.0 is cut to a 120 BPM
// modular techno track, the series' own grid (components/theme.ts): one
// beat = 15 frames, one bar = 60 frames.
export { BAR, BEAT };
/** A sixteenth note, in frames (not a whole number: round where it lands). */
export const STEP = BEAT / 4;
export const step = (n: number) => Math.round(n * STEP);

/** Four bars on the rack, before the camera goes into the first module. */
export const OPEN_LENGTH = 4 * BAR;
/** Fourteen modules of two bars, in four blocks; then MORE, twelve cards of two beats. */
export const CHAPTER_LENGTH = 2 * BAR;
export const FEATURES = 14;
export const MORE_CARDS = 12;
export const CARD_LENGTH = 2 * BEAT;
export const CHAPTER_LENGTHS: readonly number[] = [...Array<number>(FEATURES).fill(CHAPTER_LENGTH), MORE_CARDS * CARD_LENGTH];
export const CHAPTER_COUNT = CHAPTER_LENGTHS.length;
export const OUTRO_LENGTH = 4 * BAR;

/** The frame chapter `i` starts (its downbeat). */
export const chapterAt = (i: number) => OPEN_LENGTH + CHAPTER_LENGTHS.slice(0, i).reduce((sum, n) => sum + n, 0);
export const MORE_AT = chapterAt(FEATURES);
export const OUTRO_AT = chapterAt(CHAPTER_COUNT);
export const DURATION = OUTRO_AT + OUTRO_LENGTH;

/** The chapters' titles, in order; reel.tsx checks them against chapters.tsx. */
export const TITLES = [
  'CONTENT PLUGINS', '3D SCENES', 'VRM', 'APPROVALS',
  'MEDIA PLUGINS', 'NATIVE CODECS', 'PIPELINE',
  'REGISTRY', 'CELESTA PLUGIN', 'LOCK', 'FIND PLUGINS',
  'INSPECTOR', 'TIMELINE', 'EXPORT PAGE',
  'MORE',
] as const;

/** The four blocks of modules: what plugs in, and the color of its cables. */
export const BLOCKS = [
  { name: 'CONTENT', from: 0, count: 4, cable: 'orange' },
  { name: 'MEDIA', from: 4, count: 3, cable: 'blue' },
  { name: 'REGISTRY', from: 7, count: 4, cable: 'green' },
  { name: 'EDITOR', from: 11, count: 3, cable: 'violet' },
] as const;

/** The block module `i` belongs to. */
export const blockOf = (i: number) => BLOCKS.find((b) => i >= b.from && i < b.from + b.count) ?? BLOCKS[0];

/** The film's scenes by name: the frame chrome, the preview's chapter list and the rack map read them. */
export const SCENE_LIST = defineScenes([
  { id: 'open', name: 'PATCH', durationInFrames: OPEN_LENGTH },
  ...CHAPTER_LENGTHS.map((length, i) => ({ id: `chapter-${i}`, name: TITLES[i], durationInFrames: length })),
  { id: 'outro', name: 'CELESTA 0.10.0', durationInFrames: OUTRO_LENGTH },
]);
