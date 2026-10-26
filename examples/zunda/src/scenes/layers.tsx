// 1 · レイヤーを重ねる
// 二人が登場し、台詞に合わせて背景が育つ（背景は backdrop.tsx）。ここでは
// 足したレイヤーのコードと、その名前のラベルを画面に固定して出す。

import { sceneStart, useFilmFrame } from '../timing.ts';
import { COLOR } from '../theme.ts';
import { Callout } from '../ui/Callout.tsx';
import { CodePanel } from '../ui/CodePanel.tsx';
import { backdropCues } from './backdrop.tsx';
import type { SceneDefinition } from './types.ts';
import { compositionCode } from './void.tsx';

function Overlay() {
  const frame = useFilmFrame('layers');
  const cue = backdropCues();
  /** 動画全体のフレームを、このシーンの先頭からのフレームにする（Callout は <Motion from> で出す）。 */
  const local = (filmFrame: number) => filmFrame - sceneStart('layers');
  return (
    <>
      <CodePanel frame={frame} steps={[
        { at: 0, lines: compositionCode() },
        { at: cue.rect, lines: ['  <Rect width={1920} height={1080}', `    fill="${COLOR.leaf}" />`] },
        { at: cue.gradient, lines: ['  <Rect width={1920} height={1080}', "    fill={{ type: 'linear', stops }} />"] },
        { at: cue.photo, lines: ['  <Image src="./field.jpg"', '    width={1920} height={1080}', '    fit="cover" />'] },
      ]} />
      <Callout from={local(cue.rect + 4)} label="<Rect />" />
      <Callout from={local(cue.gradient + 4)} label="type: 'linear'" />
      <Callout from={local(cue.photo + 8)} label='fit="cover"' />
    </>
  );
}

export const layersScene: SceneDefinition = { id: 'layers', title: 'レイヤーを重ねる', Overlay };
