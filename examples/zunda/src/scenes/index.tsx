// シーンの一覧と、それを並べて描くコンポーネント。
// シーンを足すときは、script.ts に台詞と SceneId を、timing.ts の LEAD に前置きを
// 足し、シーンのファイルをここに台本の順で登録する。

import { Sequence } from '@celesta/react';
import type { ChapterInput } from '@celesta/react';
import type { ReactNode } from 'react';

import type { SceneId } from '../../script.ts';
import { dialoguePlan, sceneEnd, sceneStart } from '../timing.ts';
import { ChapterTag } from '../ui/ChapterTag.tsx';
import { agentScene } from './agent.tsx';
import { GrowingBackground, backdropCues } from './backdrop.tsx';
import { effectsScene } from './effects.tsx';
import { layersScene } from './layers.tsx';
import { outroScene } from './outro.tsx';
import { pathScene } from './path.tsx';
import { rewindScene } from './rewind.tsx';
import { timelineScene } from './timeline.tsx';
import type { SceneDefinition } from './types.ts';
import { voiceScene } from './voice.tsx';
import { voidScene } from './void.tsx';

/** 台本の順に並べたシーン。 */
export const SCENES: readonly SceneDefinition[] = [
  voidScene, layersScene, effectsScene, pathScene, timelineScene, voiceScene, rewindScene, agentScene, outroScene,
];

/** シーンの間だけ子を描く。子の中の `useCurrentFrame()` はシーンの先頭から数える。 */
function SceneSequence({ id, children }: { id: SceneId; children: ReactNode }) {
  const from = sceneStart(id);
  return <Sequence from={from} durationInFrames={sceneEnd(id) - from}>{children}</Sequence>;
}

/** カメラの内側：背景と各シーンの Stage。 */
export function SceneStage() {
  return (
    <>
      <Sequence durationInFrames={backdropCues().end}>
        <GrowingBackground />
      </Sequence>
      {SCENES.map(({ id, Stage }) => Stage && <SceneSequence key={id} id={id}><Stage /></SceneSequence>)}
    </>
  );
}

/** カメラの外側：各シーンの Overlay と、右上のチャプター表示。 */
export function SceneOverlay() {
  return (
    <>
      {SCENES.map(({ id, title, Overlay }, number) => (
        <SceneSequence key={id} id={id}>
          {Overlay && <Overlay />}
          {title && <ChapterTag number={number} title={title} />}
        </SceneSequence>
      ))}
    </>
  );
}

/** エディタのタイムラインに出すチャプターの目印。タイトルのあるシーンだけ。 */
export function chapters(): ChapterInput[] {
  return SCENES.flatMap(({ id, title }) => {
    const from = sceneStart(id);
    return title ? [{ id, name: title, from, durationInFrames: sceneEnd(id) - from }] : [];
  });
}

/** 帯のワイプで切り替えるフレーム。 */
export function wipeCuts(): number[] {
  return SCENES.filter((scene) => scene.wipeIn).map((scene) => sceneStart(scene.id));
}

/** 各シーンの prepare() をまとめて実行する。台詞の計画のあとに呼ぶ。 */
export async function prepareScenes(): Promise<void> {
  // 登録の順が台本のシーンの順と食い違っていたら、ここで気づけるようにする。
  const planned = dialoguePlan().scenes.map((scene) => scene.id).join(', ');
  const registered = SCENES.map((scene) => scene.id).join(', ');
  if (planned !== registered) {
    throw new Error(`scenes/index.tsx lists [${registered}] but script.ts has [${planned}]`);
  }
  for (const scene of SCENES) await scene.prepare?.();
}

/** 全シーンが描く和文。 */
export const SCENE_STRINGS: readonly string[] = SCENES.flatMap((scene) => scene.strings ?? []);
