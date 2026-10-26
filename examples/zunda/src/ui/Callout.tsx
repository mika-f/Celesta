// いま足したもの（`<Rect />` など）の名前が、画面の真ん中にぽんと出て消える。

import { Text } from '@celesta/react';
import { Motion } from '@celesta/components';

import { COLOR, FONT, solid } from '../theme.ts';

/** 出ている長さ（フレーム）。出入りを含む。 */
const SHOW_FRAMES = 70;

/** `from`（シーンの先頭からのフレーム）から、`label` を拡大しながら出して、フェードで消す。 */
export function Callout({ from, label }: { from: number; label: string }) {
  return (
    <Motion from={from} durationInFrames={SHOW_FRAMES} enter="scale" exit="fade" x={960} y={430}>
      <Text anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.mono, fontSize: 64, fontWeight: 700, fill: solid(COLOR.white),
          stroke: { paint: solid('#00000088'), width: 4 } }}
        shadow={{ color: '#00000066', blur: 18, offsetX: 0, offsetY: 8 }}>
        {label}
      </Text>
    </Motion>
  );
}
