// 右上の「01 レイヤーを重ねる」のようなチャプター表示。

import { Easings, Group, Rect, Text, progress, useCurrentFrame } from '@celesta/react';

import { COLOR, FONT, solid } from '../theme.ts';

/** `from` から `to` の手前までが 1 つのチャプター。 */
export type Chapter = { number: number; title: string; from: number; to: number };

/** いまのチャプターを出す。チャプターが変わるたびに右から滑り込み、チャプターの外では出さない。 */
export function ChapterTag({ chapters }: { chapters: readonly Chapter[] }) {
  const frame = useCurrentFrame();
  const current = chapters.find((chapter) => frame >= chapter.from && frame < chapter.to);
  if (!current) return null;
  const enter = progress(frame, current.from + 6, 16, Easings.easeOutExpo);
  return (
    <Group x={1880 + 40 * (1 - enter)} y={40} opacity={enter}>
      <Rect x={-430} width={430} height={56} cornerRadius={28} fill="#0B0F0CCC" />
      <Rect x={-420} y={8} width={64} height={40} cornerRadius={20} fill={COLOR.zunda} />
      <Text x={-388} y={28} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.mono, fontSize: 22, fontWeight: 700, fill: solid('#0B2A06') }}>
        {String(current.number).padStart(2, '0')}
      </Text>
      <Text x={-338} y={38} anchorY="baseline"
        style={{ fontFamily: FONT.ja, fontSize: 26, fontWeight: 800, fill: solid(COLOR.white) }}>
        {current.title}
      </Text>
    </Group>
  );
}
