// シーンの一覧と、それを並べて描くコンポーネント。
// シーンを足すときは、script.ts に台詞と SceneId を、timing.ts の SCENE_ORDER と
// LEAD に順番と前置きを足し、シーンのファイルをここに登録する。

import { Sequence } from '@celesta/react';
import type { ReactNode } from 'react';

import type { SceneId } from '../../script.ts';
import { SCENE_AT, SCENE_ORDER, sceneEnd } from '../timing.ts';
import { ChapterTag } from '../ui/ChapterTag.tsx';
import type { Chapter } from '../ui/ChapterTag.tsx';
import { agentScene } from './agent.tsx';
import { BACKDROP_CUE, GrowingBackground } from './backdrop.tsx';
import { effectsScene } from './effects.tsx';
import { layersScene } from './layers.tsx';
import { outroScene } from './outro.tsx';
import { pathScene } from './path.tsx';
import { rewindScene } from './rewind.tsx';
import { timelineScene } from './timeline.tsx';
import type { SceneDefinition } from './types.ts';
import { voiceScene } from './voice.tsx';
import { voidScene } from './void.tsx';

const ALL: readonly SceneDefinition[] = [
  voidScene, layersScene, effectsScene, pathScene, timelineScene, voiceScene, rewindScene, agentScene, outroScene,
];

/** timing.ts の SCENE_ORDER の順に並べたシーン。 */
export const SCENES: readonly SceneDefinition[] = SCENE_ORDER.map((id) => {
  const scene = ALL.find((candidate) => candidate.id === id);
  if (!scene) throw new Error(`scene "${id}" is not registered in scenes/index.tsx`);
  return scene;
});

/** シーンの間だけ子を描く。子の中の `useCurrentFrame()` はシーンの先頭から数える。 */
function SceneSequence({ id, children }: { id: SceneId; children: ReactNode }) {
  return <Sequence from={SCENE_AT[id]} durationInFrames={sceneEnd(id) - SCENE_AT[id]}>{children}</Sequence>;
}

/** カメラの内側：背景と各シーンの Stage。 */
export function SceneStage() {
  return (
    <>
      <Sequence durationInFrames={BACKDROP_CUE.end}>
        <GrowingBackground />
      </Sequence>
      {SCENES.map(({ id, Stage }) => Stage && <SceneSequence key={id} id={id}><Stage /></SceneSequence>)}
    </>
  );
}

const CHAPTERS: readonly Chapter[] = SCENES.flatMap((scene, number) =>
  scene.title ? [{ number, title: scene.title, from: SCENE_AT[scene.id], to: sceneEnd(scene.id) }] : []);

/** カメラの外側：各シーンの Overlay と、右上のチャプター表示。 */
export function SceneOverlay() {
  return (
    <>
      {SCENES.map(({ id, Overlay }) => Overlay && <SceneSequence key={id} id={id}><Overlay /></SceneSequence>)}
      <ChapterTag chapters={CHAPTERS} />
    </>
  );
}

/** 帯のワイプで切り替えるフレーム。 */
export const WIPE_CUTS: readonly number[] = SCENES.filter((scene) => scene.wipeIn).map((scene) => SCENE_AT[scene.id]);

/** 各シーンの prepare() をまとめて実行する。 */
export async function prepareScenes(): Promise<void> {
  for (const scene of SCENES) await scene.prepare?.();
}

/** 全シーンが描く和文。 */
export const SCENE_STRINGS: readonly string[] = SCENES.flatMap((scene) => scene.strings ?? []);
