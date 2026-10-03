// 1 · レイヤーを重ねる
// 二人が登場し、台詞に合わせて背景が育つ（背景は backdrop.tsx）。ここでは
// 足したレイヤーのコードと、その名前のラベルを画面に固定して出す。

import { useFilmFrame } from '../timing.ts';
import { COLOR } from '../theme.ts';
import { Callout } from '../ui/Callout.tsx';
import { CodePanel } from '../ui/CodePanel.tsx';
import { BACKDROP_CUE } from './backdrop.tsx';
import type { SceneDefinition } from './types.ts';
import { COMPOSITION_CODE } from './void.tsx';

function Overlay() {
  const frame = useFilmFrame('layers');
  return (
    <>
      <CodePanel frame={frame} steps={[
        { at: 0, lines: COMPOSITION_CODE },
        { at: BACKDROP_CUE.rect, lines: ['  <Rect width={1920} height={1080}', `    fill="${COLOR.leaf}" />`] },
        { at: BACKDROP_CUE.gradient, lines: ['  <Rect width={1920} height={1080}', "    fill={{ type: 'linear', stops }} />"] },
        { at: BACKDROP_CUE.photo, lines: ['  <Image src="./field.jpg"', '    width={1920} height={1080}', '    fit="cover" />'] },
      ]} />
      <Callout frame={frame} at={BACKDROP_CUE.rect + 4} label="<Rect />" />
      <Callout frame={frame} at={BACKDROP_CUE.gradient + 4} label="type: 'linear'" />
      <Callout frame={frame} at={BACKDROP_CUE.photo + 8} label='fit="cover"' />
    </>
  );
}

export const layersScene: SceneDefinition = { id: 'layers', title: 'レイヤーを重ねる', Overlay };
