// 右上の「01 レイヤーを重ねる」のようなチャプター表示。シーンの <Sequence> の
// 中に置くと、シーンが始まって少ししてから右から滑り込み、シーンの終わりで消える。

import { Rect, Text } from '@celesta/react';
import { Badge, Motion } from '@celesta/components';

import { COLOR, FONT, solid } from '../theme.ts';

/** チャプターの番号札（緑の丸い札に 2 桁の番号）。 */
function ChapterNumber({ number }: { number: number }) {
  return (
    <>
      <Rect y={-4} width={64} height={40} cornerRadius={20} fill={COLOR.zunda} />
      <Text x={32} y={16} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.mono, fontSize: 22, fontWeight: 700, fill: solid('#0B2A06') }}>
        {String(number).padStart(2, '0')}
      </Text>
    </>
  );
}

export function ChapterTag({ number, title }: { number: number; title: string }) {
  return (
    <Motion from={6} enter="fade-left">
      <Badge x={1880} y={40} anchorX={1} fill="#0B0F0CCC" cornerRadius="pill"
        padding={{ top: 12, right: 28, bottom: 12, left: 10 }} gap={14}
        leading={<ChapterNumber number={number} />} leadingWidth={64}
        fontFamily={FONT.ja} fontSize={26} fontWeight={800} color={COLOR.white}>
        {title}
      </Badge>
    </Motion>
  );
}
