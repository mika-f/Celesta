// What a line shows and what it says, kept apart: the subtitle shows `text`
// ("Voice Folder"), and the mouth follows `spokenText`, the kana the
// recording actually says. loadDialogueLipSync() builds the waveform lip
// sync from the local WAV and the spoken text in prepare(); no speech engine
// runs. The subtitle file gets `text`, never `spokenText`.
//
//   cargo run -p celesta-exporter -- --react packages/cli/examples/with-spoken-text.tsx out.mp4 --subtitles out.srt
//
// Author-editable inputs: `text` and `spokenText` below. The WAV is
// examples/assets/voice-folder/shizuku/voice-1.wav (see its README.md).
import * as React from 'react';

import { Assets, Composition, Font, Rect, Text } from '@celesta.studio/react';
import { Character, CharacterView, DialogueSeries, loadDialogueLipSync, planDialogue } from '@celesta.studio/character';
import type { AssetReference } from '@celesta.studio/react';
import type { CharacterViewReference, DialogueLine, DialoguePlan } from '@celesta.studio/character';

const WIDTH = 1280;
const HEIGHT = 720;
const FPS = 30;
const PORTRAITS = '../../../examples/assets/dialogue-demo/portraits';

const script: DialogueLine[] = [{
  id: 'hello',
  speaker: 'shizuku',
  text: 'こんにちは。Voice Folder を読み込んでみます。',
  spokenText: 'こんにちは。ぼいすふぉるだーをよみこんでみます。',
  audio: '../../../examples/assets/voice-folder/shizuku/voice-1.wav',
}];

const shizuku = React.createRef<AssetReference>();
const view = React.createRef<CharacterViewReference>();

let plan: DialoguePlan;

export async function prepare(): Promise<void> {
  plan = await planDialogue(await loadDialogueLipSync(script), { fps: FPS, gap: 0.5 });
}

export default function Root() {
  const mouth = (shape: string) => `${PORTRAITS}/mouth-${shape}.png`;
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={plan.durationInFrames} lang="ja-JP">
      <Assets>
        <Font src="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;700" />
        <Character ref={shizuku} name="しずく"
          portrait={{
            defaultExpression: 'normal',
            expressions: { normal: `${PORTRAITS}/shizuku-normal.png` },
            lipSync: { a: mouth('a'), i: mouth('i'), u: mouth('u'), e: mouth('e'), o: mouth('o'), closed: mouth('closed') },
          }}
          subtitle={{
            x: WIDTH / 2, y: 610, anchorX: 0.5, anchorY: 0.5, maxWidth: 1100,
            style: { fontFamily: 'Noto Sans JP', fontSize: 40, fontWeight: 700, align: 'center', lineBreak: 'phrase', fill: { type: 'solid', color: '#ffffff' } },
          }} />
      </Assets>
      <Rect width={WIDTH} height={HEIGHT} fill="#3a4d63" />
      <Text x={40} y={40} style={{ fontFamily: 'Noto Sans JP', fontSize: 22, fill: { type: 'solid', color: '#c9d3df' } }}>
        {`spokenText: ${script[0].spokenText}`}
      </Text>
      <CharacterView ref={view} character={shizuku} x={460} y={60} mouth="closed" />
      <DialogueSeries plan={plan} views={{ shizuku: view }} />
    </Composition>
  );
}
