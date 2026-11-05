// The beep-voice plugin's demo: two portraits talking in beeps. The voices
// come from `prepareBeepNarration()` in prepare(), in place of a speech
// engine; planDialogue(), <DialogueSeries>, and the portraits' lip sync are
// Celesta's own. Build the plugin and the codec plugin first, then export
// with both directories:
//
//   node scripts/plugins/build.mjs examples/plugins/beep-voice
//   node scripts/plugins/build.mjs plugins/ffmpeg
//   cargo run -p celesta-exporter -- --react examples/plugins/beep-voice/demo.tsx out.mp4 --plugin-dir examples/plugins/dist --plugin-dir plugins/dist
import * as React from 'react';

import { Assets, Composition, Font, Rect, Text } from '@celesta.studio/react';
import { Character, CharacterView, DialogueSeries, planDialogue } from '@celesta.studio/character';
import type { AssetReference } from '@celesta.studio/react';
import type { CharacterViewReference, DialoguePlan } from '@celesta.studio/character';
import { prepareBeepNarration } from '@celesta-plugin/beep-voice';
import type { BeepLine } from '@celesta-plugin/beep-voice';

const WIDTH = 1280;
const HEIGHT = 720;
const FPS = 30;
const PORTRAITS = '../../assets/dialogue-demo/portraits';

const script: BeepLine[] = [
  { id: 'hello', speaker: 'shizuku', text: 'こんにちは！今日はピコピコ声でお話しします。' },
  { id: 'reply', speaker: 'komugi', text: 'ボイスはプラグインが作っているの？' },
  { id: 'answer', speaker: 'shizuku', text: 'そう。エンジンを差し替えれば、ほかの声にもできるよ。' },
];

const shizuku = React.createRef<AssetReference>();
const komugi = React.createRef<AssetReference>();
const views = {
  shizuku: React.createRef<CharacterViewReference>(),
  komugi: React.createRef<CharacterViewReference>(),
};

let plan: DialoguePlan;

export async function prepare(): Promise<void> {
  const lines = await prepareBeepNarration(script, {
    speakers: {
      shizuku: { pitch: 330, speed: 13 },
      komugi: { pitch: 220, speed: 11, waveform: 'triangle', volume: 0.4 },
    },
  });
  plan = await planDialogue(lines, { fps: FPS, gap: 0.4 });
}

const mouths = Object.fromEntries(
  (['a', 'i', 'u', 'e', 'o', 'closed'] as const).map((shape) => [shape, `${PORTRAITS}/mouth-${shape}.png`]),
);
const subtitle = {
  x: WIDTH / 2, y: 640, anchorX: 0.5, anchorY: 0.5, maxWidth: 1100,
  style: { fontFamily: 'Noto Sans JP', fontSize: 36, fontWeight: 700, align: 'center', lineBreak: 'phrase', fill: { type: 'solid', color: '#ffffff' } },
} as const;

export default function Root() {
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={plan.durationInFrames} lang="ja-JP">
      <Assets>
        <Font src="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;700" />
        <Character ref={shizuku} name="しずく" subtitle={subtitle}
          portrait={{ defaultExpression: 'normal', expressions: { normal: `${PORTRAITS}/shizuku-normal.png` }, lipSync: mouths }} />
        <Character ref={komugi} name="こむぎ" subtitle={subtitle}
          portrait={{ defaultExpression: 'normal', expressions: { normal: `${PORTRAITS}/komugi-normal.png` }, lipSync: mouths }} />
      </Assets>
      <Rect width={WIDTH} height={HEIGHT} fill="#3a4d63" />
      <Text x={40} y={36} style={{ fontFamily: 'Noto Sans JP', fontSize: 28, fontWeight: 700, fill: { type: 'solid', color: '#c9d3df' } }}>
        @celesta-plugin/beep-voice
      </Text>
      <CharacterView ref={views.shizuku} character={shizuku} x={180} y={120} mouth="closed" />
      <CharacterView ref={views.komugi} character={komugi} x={740} y={120} mouth="closed" />
      <DialogueSeries plan={plan} views={views} />
    </Composition>
  );
}
