// One long voice, its subtitle a page at a time: paginateSubtitles()
// measures each line with the subtitle's own style and width in prepare()
// and splits it every measured line; the voice and the mouth play on as one
// utterance. Emphasized words keep their color and weight on whichever page
// they land. The second line's pages are written by hand, with a gap between
// them where the band stays up empty.
//
//   cargo run -p celesta-exporter -- --react packages/cli/examples/with-subtitle-pages.tsx out.mp4 --subtitles out.srt
//
// Author-editable inputs: `script` (text, spokenText, emphasis, the manual
// pages), SUBTITLE, and the paginateSubtitles() options. The WAVs are local
// (examples/assets/subtitle-pages and examples/assets/voice-folder); no
// speech engine runs.
import * as React from 'react';

import { Assets, Composition, Font, Group, Rect, Text, interpolate } from '@celesta.studio/react';
import {
  Character, CharacterView, DialogueSeries, loadDialogueLipSync, paginateSubtitles, planDialogue,
} from '@celesta.studio/character';
import type { AssetReference, TextStyle } from '@celesta.studio/react';
import type { CharacterViewReference, DialogueLine, DialoguePlan, SubtitleRenderProps } from '@celesta.studio/character';
import type { EmphasisStyle } from '@celesta.studio/text';

const WIDTH = 1280;
const HEIGHT = 720;
const FPS = 30;
const PORTRAITS = '../../../examples/assets/dialogue-demo/portraits';

const SUBTITLE: TextStyle = {
  fontFamily: 'Noto Sans JP', fontSize: 44, fontWeight: 500, lineHeight: 60, align: 'center', lineBreak: 'phrase',
  fill: { type: 'solid', color: '#ffffff' },
};
const MAX_WIDTH = 620;
const EMPHASIS: Record<string, EmphasisStyle> = {
  danger: { color: '#ff6b6b', fontWeight: 900 },
  key: { color: '#ffd166', fontWeight: 900 },
};

const script: DialogueLine[] = [
  {
    id: 'danger', speaker: 'shizuku',
    text: 'HPが1です。急いで回復しましょう。薬草は、ポーチの中にあります。',
    spokenText: 'えいちぴーがいちです。いそいでかいふくしましょう。やくそうは、ぽーちのなかにあります。',
    audio: '../../../examples/assets/subtitle-pages/danger.wav',
    // Grapheme clusters of `text`: "1", "回復", "薬草".
    emphasis: [{ start: 3, end: 4, style: 'danger' }, { start: 10, end: 12, style: 'key' }, { start: 18, end: 20, style: 'key' }],
  },
  {
    id: 'save', speaker: 'shizuku',
    text: '音声と文字を、同じ名前で保存してください。',
    spokenText: 'おんせいともじを、おなじなまえでほぞんしてください。',
    audio: '../../../examples/assets/voice-folder/shizuku/voice-2.wav',
    // By hand: frames from the line's start; 40–45 shows the band without text.
    subtitlePages: [{ from: 0, durationInFrames: 40, text: '音声と文字を、' }, { from: 46, text: '同じ名前で保存してください。' }],
    emphasis: [{ start: 0, end: 2, style: 'key' }],
  },
];

const shizuku = React.createRef<AssetReference>();
const view = React.createRef<CharacterViewReference>();

let plan: DialoguePlan;

export async function prepare(): Promise<void> {
  const voiced = await loadDialogueLipSync(script);
  const paged = await paginateSubtitles(voiced, { fps: FPS, style: SUBTITLE, maxWidth: MAX_WIDTH, maxLines: 1, emphasisStyles: EMPHASIS });
  plan = await planDialogue(paged, { fps: FPS, gap: 0.6 });
}

/** A band sized to the page shown, with the page number and how its start was timed. */
function Band({ content, metrics, style, maxWidth, held, page, frame, durationInFrames }: SubtitleRenderProps) {
  const width = metrics.width + 64;
  const height = metrics.height + 28;
  // The band runs on the line's clock, so turning a page never restarts its fade.
  const opacity = interpolate(frame, [0, 6, durationInFrames - 6, durationInFrames], [0, 1, 1, 0], {
    extrapolateLeft: 'clamp', extrapolateRight: 'clamp',
  });
  return (
    <Group opacity={opacity}>
      <Rect x={(WIDTH - width) / 2} y={560} width={width} height={height} cornerRadius={14} fill="#101820d9" />
      {held ? null : (
        <Text x={(WIDTH - (maxWidth ?? width)) / 2} y={574 + metrics.ascent} anchorY="baseline" maxWidth={maxWidth} style={style}>
          {content}
        </Text>
      )}
      {page ? (
        <Text x={(WIDTH + width) / 2 - 12} y={556} anchorX={1} anchorY="baseline"
          style={{ fontFamily: 'Noto Sans JP', fontSize: 18, fill: { type: 'solid', color: '#c9d3df' } }}>
          {`${page.index + 1}/${page.count} · ${page.timing}`}
        </Text>
      ) : null}
    </Group>
  );
}

export default function Root() {
  const mouth = (shape: string) => `${PORTRAITS}/mouth-${shape}.png`;
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={plan.durationInFrames} lang="ja-JP">
      <Assets>
        <Font src="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;500;900" />
        <Character ref={shizuku} name="しずく"
          portrait={{
            defaultExpression: 'normal',
            expressions: { normal: `${PORTRAITS}/shizuku-normal.png` },
            lipSync: { a: mouth('a'), i: mouth('i'), u: mouth('u'), e: mouth('e'), o: mouth('o'), closed: mouth('closed') },
          }}
          subtitle={{ style: SUBTITLE, maxWidth: MAX_WIDTH, emphasisStyles: EMPHASIS, render: Band }} />
      </Assets>
      <Rect width={WIDTH} height={HEIGHT} fill="#3a4d63" />
      <CharacterView ref={view} character={shizuku} x={460} y={20} mouth="closed" />
      <DialogueSeries plan={plan} views={{ shizuku: view }} holdSubtitle />
    </Composition>
  );
}
