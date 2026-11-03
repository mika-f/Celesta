import { defineScenes, type SceneEntry } from '@celesta.studio/react';
import { CHAPTERS } from './chapters';
import { ChapterShell } from './components/ChapterShell';
import { BAR } from './constants';
import { pad } from './helpers';
import { Index } from './scenes/Index';
import { Open } from './scenes/Open';
import { Outro } from './scenes/Outro';

// The film's scenes, back to back: the open, the index, the nine chapters
// and the outro. `chapter` numbers the feature chapters for the HUD.
export const SCENES = defineScenes<SceneEntry & { chapter?: number }>([
  { id: 'open', durationInFrames: BAR * 2, Scene: Open },
  { id: 'index', name: 'INDEX', durationInFrames: BAR * 2, Scene: Index },
  ...CHAPTERS.map((chapter, i) => ({
    id: chapter.key,
    name: `${pad(i + 1)} ${chapter.key}`,
    durationInFrames: chapter.len,
    chapter: i,
    Scene: () => <ChapterShell index={i}><chapter.Demo /></ChapterShell>,
  })),
  { id: 'outro', durationInFrames: BAR * 3, Scene: Outro },
]);

const sceneFrom = (id: string) => SCENES.scenes.find((scene) => scene.id === id)!.from;

// Chapter start frames, for the cuts and the score.
export const CHAPTER_AT = CHAPTERS.map((chapter) => sceneFrom(chapter.key));
