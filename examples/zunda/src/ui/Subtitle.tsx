// 解説動画の字幕：画面下の帯と名前札と、字幕の文字のスタイル。
//
// 帯と名前札は `subtitle.render` で、字幕の文字と一緒に描く。<DialogueSeries
// holdSubtitle> が台詞の合間も帯を出し続け、シーンの前置きの間だけ引っ込める。
// 改行は `lineBreak: 'phrase'`（文節の途中で折り返さない）に任せる。

import { Group, Rect, Text, interpolate, useTextMetrics } from '@celesta/react';
import type { TextStyle } from '@celesta/react';
import type { CharacterSubtitle, SubtitleRenderProps } from '@celesta/character';

import type { Speaker } from '../../script.ts';
import { SPEAKERS } from '../cast/speakers.ts';
import { CLAMP, COLOR, FONT, solid } from '../theme.ts';

/** 帯の大きさ。文字が 3 行になったときだけ縦に伸びる。 */
const BAND = { width: 1120, height: 196, padding: 48 } as const;
const PLATE: TextStyle = { fontFamily: FONT.ja, fontSize: 24, fontWeight: 800, fill: solid('#0B0F0C') };

/** `<Character subtitle>` に渡す、話者ごとの字幕。帯の中央に文字を置く。 */
export function subtitleFor(speaker: Speaker): CharacterSubtitle {
  const { deep } = SPEAKERS[speaker];
  return {
    x: 960,
    y: 964,
    maxWidth: 1060,
    style: {
      fontFamily: FONT.ja,
      fontSize: 44,
      fontWeight: 800,
      lineHeight: 60,
      align: 'center',
      lineBreak: 'phrase',
      fill: solid(COLOR.white),
      stroke: { paint: solid(deep), width: 9 },
    },
    render: (props) => <SubtitleBand speaker={speaker} {...props} />,
  };
}

/** 半透明の帯、話者の色の名前札、字幕の文字。続けて話す間は帯を出したまま、文字だけ入れ替わる。 */
function SubtitleBand({
  speaker, content, character, metrics, style, maxWidth, held, frame, durationInFrames,
}: SubtitleRenderProps & { speaker: Speaker }) {
  const { color } = SPEAKERS[speaker];
  const plate = useTextMetrics(character.displayName, PLATE);
  // 出入りは帯ごとに 1 回だけ。続く台詞の間はフェードしない。
  const fade = Math.min(10, durationInFrames / 3);
  const opacity = interpolate(frame + 0.5, [0, fade, durationInFrames - fade, durationInFrames], [0, 1, 1, 0], CLAMP);
  const height = Math.max(BAND.height, metrics.height + BAND.padding);
  // 帯が高くなるときは下端をそろえたまま上に伸ばす（画面の下からはみ出さない）
  return (
    <Group y={-(height - BAND.height) / 2} opacity={opacity}>
      <Rect anchorX={0.5} anchorY={0.5} width={BAND.width} height={height} cornerRadius={30}
        fill="#0B0F0CC8" stroke={color} strokeWidth={4} />
      <Rect x={-BAND.width / 2 + 30} y={-height / 2} anchorY={0.5} width={plate.width + 64} height={46}
        cornerRadius={23} fill={color} />
      <Text x={-BAND.width / 2 + 62} y={-height / 2} anchorY={0.5} style={PLATE}>{character.displayName}</Text>
      {!held && <Text anchorX={0.5} anchorY={0.5} maxWidth={maxWidth} style={style}>{content}</Text>}
    </Group>
  );
}
